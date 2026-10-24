//! Desktop policy for confirmed ACPI/USB facts, never raw EC ports or AML.
//! Observe a copied SDK snapshot, then apply its ordered screen intents after
//! releasing this owner. Sys/Draw/Screen need no overlapping mutable aliases.
use crate::outputs::{NativeResult, OutputCatalog, same_id};
use r4os_contract as a;
const SECOND: u64 = 1_000_000_000;
pub fn snapshot(app: &r4os_sdk::App<'_>) -> Option<a::PlatformInputSnapshot> {
    let sys = app
        .system_api()
        .filter(|s| s.platform_input_snapshot != 0)?;
    let call: a::R4SysFns::platform_input_snapshot =
        unsafe { core::mem::transmute(sys.platform_input_snapshot) };
    let mut out = a::PlatformInputSnapshot::default();
    (unsafe { call(&mut out) } == 0).then_some(out)
}
pub trait OutputBrightness: OutputCatalog {
    fn brightness(&mut self, id: &a::GfxOutputId) -> NativeResult<a::GfxOutputBrightness>;
    fn request_brightness(
        &mut self,
        request: &a::GfxBrightnessRequest,
    ) -> NativeResult<a::GfxBrightnessRequest>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenAction {
    Sleep,
    Wake,
}
#[derive(Clone, Copy, Default)]
pub struct Observation {
    pub activity: bool,
    actions: [Option<ScreenAction>; 2],
}
impl Observation {
    pub fn actions(self) -> impl Iterator<Item = ScreenAction> {
        self.actions.into_iter().flatten()
    }
}
#[derive(Default)]
pub struct Owner {
    previous: Option<a::PlatformInputSnapshot>,
    pending: i16,
    target: Option<a::GfxOutputId>,
    accepted: u64,
    deadline: u64,
    next_ns: u64,
}
impl Owner {
    fn clear(&mut self) {
        self.pending = 0;
        self.target = None;
        self.accepted = 0;
        self.next_ns = 0;
    }
    pub fn observe(&mut self, value: Option<a::PlatformInputSnapshot>, now: u64) -> Observation {
        if now == 0 {
            return Observation::default();
        }
        let value = value.filter(|v| {
            v.version == 1
                && v.size as usize == core::mem::size_of::<a::PlatformInputSnapshot>()
                && v.capabilities <= 3
                && v.sources <= 3
                && v.lid_state <= 2
                && v.reserved == 0
        });
        let Some(value) = value else {
            let closed = self.previous.is_some_and(|old| old.lid_state == 2);
            self.previous = None;
            self.clear();
            return Observation {
                activity: closed,
                actions: [closed.then_some(ScreenAction::Wake), None],
            };
        };
        let prior = self.previous;
        self.previous = Some(value);
        let fresh = prior.is_some_and(|old| {
            value.sequence >= old.sequence
                && value.brightness_up >= old.brightness_up
                && value.brightness_down >= old.brightness_down
        });
        let mut observation = Observation::default();
        if prior.is_none()
            || !fresh
            || prior.is_some_and(|old| old.lid_sequence != value.lid_sequence)
        {
            if value.lid_state == 2 {
                observation.activity = true;
                observation.actions[0] = Some(ScreenAction::Sleep);
            } else if prior.is_some_and(|old| old.lid_state == 2) {
                observation.activity = true;
                observation.actions[0] = Some(ScreenAction::Wake);
            }
        }
        if !fresh {
            self.clear();
            return observation;
        } // No historical key replay.
        let old = prior.expect("fresh platform baseline");
        let up = 20.min(value.brightness_up - old.brightness_up);
        let down = 20.min(value.brightness_down - old.brightness_down);
        if up != 0 || down != 0 {
            self.pending = (self.pending + up as i16 - down as i16).clamp(-20, 20);
            self.deadline = now.saturating_add(35 * SECOND);
            self.next_ns = 0;
            observation.actions[1] = Some(ScreenAction::Wake);
            observation.activity = true;
        }
        observation
    }
    pub fn step(&mut self, outputs: &mut impl OutputBrightness, now: u64) {
        if self.pending == 0 && self.accepted == 0 {
            return;
        }
        if now == 0 || now >= self.deadline {
            self.clear();
            return;
        }
        if now < self.next_ns {
            return;
        }
        self.next_ns = now.saturating_add(100_000_000);
        let Ok(before) = outputs.revision() else {
            return;
        };
        if before.present > 32 {
            return;
        }
        let mut panel = None;
        for index in 0..before.present {
            let Ok(info) = outputs.info(index) else {
                return;
            };
            if info.topology_revision != before.revision {
                return;
            }
            if info.connector_kind == a::gfx_output_kind_edp
                && info.flags & a::gfx_output_flag_connected != 0
            {
                if panel.is_some() {
                    self.clear();
                    return;
                }
                panel = Some(info.identity);
            }
        }
        let Ok(after) = outputs.revision() else {
            return;
        };
        if after.revision != before.revision {
            return;
        }
        let Some(identity) = panel else {
            self.clear();
            return;
        };
        if self.target.is_some_and(|old| !same_id(old, identity)) {
            self.clear();
            return;
        }
        self.target = Some(identity);
        if let Ok(power) = outputs.power(&identity) {
            if power.phase != a::gfx_power_phase_on {
                return;
            }
        }
        let state = match outputs.brightness(&identity) {
            Ok(state) => state,
            Err(rc) if [a::gfx_output_error_unsupported, a::err_no_fn].contains(&rc) => {
                self.clear();
                return;
            }
            Err(_) => return,
        };
        if !same_id(state.identity, identity) {
            return;
        }
        if state.phase != a::gfx_brightness_phase_ready
            || state.flags & a::gfx_brightness_flag_current_known == 0
            || state.minimum >= state.maximum
            || state.maximum > 65535
        {
            self.clear();
            return;
        }
        if self.accepted != 0 {
            if state.request_sequence < self.accepted {
                return;
            }
            self.accepted = 0;
        }
        if self.pending == 0 {
            self.clear();
            return;
        }
        let level = (i64::from(state.current) + i64::from(self.pending) * 3277)
            .clamp(i64::from(state.minimum), i64::from(state.maximum)) as u32;
        if level == state.current {
            self.clear();
            return;
        }
        let request = a::GfxBrightnessRequest {
            identity,
            level,
            ..a::GfxBrightnessRequest::default()
        };
        let accepted = match outputs.request_brightness(&request) {
            Ok(accepted) => accepted,
            Err(a::gfx_output_error_busy) => return,
            Err(_) => {
                self.clear();
                return;
            }
        };
        if !same_id(accepted.identity, identity) || accepted.sequence == 0 {
            self.clear();
            return;
        }
        self.accepted = accepted.sequence;
        self.pending = 0;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Model {
        input: a::PlatformInputSnapshot,
        state: a::GfxOutputBrightness,
        power_phase: u32,
        count: u32,
        sleeps: usize,
        wakes: usize,
        calls: usize,
        last: a::GfxBrightnessRequest,
        available: bool,
        busy: bool,
        torn: bool,
    }
    impl Default for Model {
        fn default() -> Self {
            Self {
                input: a::PlatformInputSnapshot {
                    sequence: 2,
                    brightness_up: 7,
                    brightness_down: 3,
                    lid_sequence: 1,
                    lid_state: 1,
                    capabilities: 3,
                    sources: 3,
                    ..a::PlatformInputSnapshot::default()
                },
                state: a::GfxOutputBrightness {
                    identity: a::GfxOutputId {
                        adapter_id: 1,
                        connector_id: 2,
                        device_generation: 3,
                        connection_generation: 4,
                    },
                    phase: a::gfx_brightness_phase_ready,
                    flags: a::gfx_brightness_flag_current_known,
                    minimum: 100,
                    maximum: 65535,
                    current: 30000,
                    ..a::GfxOutputBrightness::default()
                },
                power_phase: a::gfx_power_phase_on,
                count: 1,
                sleeps: 0,
                wakes: 0,
                calls: 0,
                last: a::GfxBrightnessRequest::default(),
                available: true,
                busy: false,
                torn: false,
            }
        }
    }
    impl OutputCatalog for Model {
        fn revision(&mut self) -> NativeResult<a::GfxDisplayRevision> {
            Ok(a::GfxDisplayRevision {
                revision: 1,
                present: self.count,
                ..a::GfxDisplayRevision::default()
            })
        }
        fn info(&mut self, _: u32) -> NativeResult<a::GfxOutputInfo> {
            Ok(a::GfxOutputInfo {
                identity: self.state.identity,
                topology_revision: if self.torn { 2 } else { 1 },
                connector_kind: a::gfx_output_kind_edp,
                flags: a::gfx_output_flag_connected,
                ..a::GfxOutputInfo::default()
            })
        }
        fn power(&mut self, id: &a::GfxOutputId) -> NativeResult<a::GfxOutputPower> {
            Ok(a::GfxOutputPower {
                identity: *id,
                phase: self.power_phase,
                ..a::GfxOutputPower::default()
            })
        }
    }
    impl OutputBrightness for Model {
        fn brightness(&mut self, _: &a::GfxOutputId) -> NativeResult<a::GfxOutputBrightness> {
            Ok(self.state)
        }
        fn request_brightness(
            &mut self,
            request: &a::GfxBrightnessRequest,
        ) -> NativeResult<a::GfxBrightnessRequest> {
            self.calls += 1;
            if self.busy {
                return Err(a::gfx_output_error_busy);
            }
            self.last = *request;
            self.last.sequence = self.calls as u64;
            Ok(self.last)
        }
    }
    impl Model {
        fn poll(&mut self, owner: &mut Owner, now: u64) -> bool {
            let observation = owner.observe(self.available.then_some(self.input), now);
            for action in observation.actions() {
                match action {
                    ScreenAction::Sleep => self.sleeps += 1,
                    ScreenAction::Wake => self.wakes += 1,
                }
            }
            observation.activity
        }
        fn key(&mut self, owner: &mut Owner, now: u64, up: u64, down: u64) {
            self.input.sequence += 1;
            self.input.brightness_up += up;
            self.input.brightness_down += down;
            assert!(self.poll(owner, now));
        }
    }
    #[test]
    fn original_lid_and_brightness_confirmed_receipts_busy_replug_ambiguity_and_deadline() {
        let mut model = Model::default();
        let mut owner = Owner::default();
        assert!(!model.poll(&mut owner, SECOND));
        owner.step(&mut model, SECOND);
        assert!(model.calls == 0 && model.wakes == 0);
        model.input.lid_state = 2;
        model.input.lid_sequence += 1;
        assert!(model.poll(&mut owner, 2 * SECOND));
        assert!(!model.poll(&mut owner, 3 * SECOND));
        assert_eq!(model.sleeps, 1);
        model.input.lid_state = 0;
        model.input.lid_sequence += 1;
        assert!(model.poll(&mut owner, 4 * SECOND));
        assert_eq!(model.wakes, 1);
        model.key(&mut owner, 5 * SECOND, 1, 0);
        model.power_phase = a::gfx_power_phase_waking;
        owner.step(&mut model, 5 * SECOND);
        assert_eq!(model.calls, 0);
        model.power_phase = a::gfx_power_phase_on;
        model.busy = true;
        owner.step(&mut model, 6 * SECOND);
        assert!(model.calls == 1 && owner.pending == 1 && owner.accepted == 0);
        model.busy = false;
        owner.step(&mut model, 7 * SECOND);
        assert!(model.calls == 2 && model.last.level == 33277 && model.state.current == 30000);
        model.key(&mut owner, 8 * SECOND, 2, 0);
        owner.step(&mut model, 8 * SECOND);
        assert_eq!(model.calls, 2);
        model.state.current = model.last.level;
        model.state.request_sequence = model.last.sequence;
        owner.step(&mut model, 9 * SECOND);
        assert!(model.calls == 3 && model.last.level == 39831);
        model.state.current = model.last.level;
        model.state.request_sequence = model.last.sequence;
        owner.step(&mut model, 10 * SECOND);
        assert!(owner.accepted == 0 && owner.pending == 0);
        model.key(&mut owner, 11 * SECOND, 0, 20);
        owner.step(&mut model, 11 * SECOND);
        assert_eq!(model.last.level, model.state.minimum);
        model.state.phase = a::gfx_brightness_phase_failed;
        owner.step(&mut model, 12 * SECOND);
        let calls = model.calls;
        owner.step(&mut model, 13 * SECOND);
        assert!(owner.accepted == 0 && model.calls == calls);
        model.state.phase = a::gfx_brightness_phase_ready;
        model.key(&mut owner, 14 * SECOND, 1, 0);
        model.power_phase = a::gfx_power_phase_off;
        owner.step(&mut model, 14 * SECOND);
        model.state.identity.connection_generation += 1;
        model.power_phase = a::gfx_power_phase_on;
        owner.step(&mut model, 15 * SECOND);
        assert!(owner.pending == 0 && model.calls == calls);
        model.key(&mut owner, 16 * SECOND, 1, 0);
        model.count = 0;
        owner.step(&mut model, 16 * SECOND);
        assert_eq!(owner.pending, 0);
        model.key(&mut owner, 17 * SECOND, 1, 0);
        model.count = 2;
        owner.step(&mut model, 17 * SECOND);
        assert_eq!(owner.pending, 0);
        model.key(&mut owner, 18 * SECOND, 1, 0);
        model.count = 1;
        model.torn = true;
        owner.step(&mut model, 18 * SECOND);
        assert!(model.calls == calls && owner.pending == 1);
        owner.step(&mut model, 54 * SECOND);
        assert_eq!(owner.pending, 0);
        model.input.sequence = 1;
        model.input.brightness_up = 0;
        model.input.brightness_down = 0;
        assert!(!model.poll(&mut owner, 55 * SECOND));
        model.input.lid_state = 2;
        model.input.lid_sequence += 1;
        assert!(model.poll(&mut owner, 56 * SECOND));
        let wakes = model.wakes;
        model.available = false;
        assert!(model.poll(&mut owner, 57 * SECOND));
        assert!(model.wakes == wakes + 1 && owner.previous.is_none());
        assert!(!model.poll(&mut owner, 58 * SECOND));
    }
}
