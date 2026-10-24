//! Desktop intent for confirmed R4D power facts. Every connector generation
//! keeps its own off lease and actual request receipt, including reconnects.
use super::{NativeResult, OutputCatalog, same_id};
use r4os_contract as a;
const SECOND: u64 = 1_000_000_000;
/// A short adapter to the existing general SDK output context. These calls
/// do not transfer ownership of the Desktop state to a provider.
pub trait OutputPower: OutputCatalog {
    fn request_power(&mut self, request: &a::GfxPowerRequest) -> NativeResult<a::GfxPowerRequest>;
}
#[derive(Clone, Copy, Default)]
struct Entry {
    state: a::GfxOutputPower,
    request: a::GfxPowerRequest,
    renew_ns: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unavailable,
    Stale,
    Clock,
    Unsupported,
}
#[derive(Default)]
pub struct Owner {
    entries: [Entry; 32],
    count: usize,
    last_activity_ns: u64,
    poll_ns: u64,
    transition_deadline: u64,
    want_off: bool,
    wake_pending: bool,
    faulted: bool,
    observed_current: bool,
}
impl Owner {
    pub fn want_off(&self) -> bool {
        self.want_off
    }
    pub fn wake_pending(&self) -> bool {
        self.wake_pending
    }
    pub fn faulted(&self) -> bool {
        self.faulted
    }
    fn discover(&mut self, outputs: &mut impl OutputPower) -> Result<(), Error> {
        self.observed_current = false;
        let before = outputs.revision().map_err(|_| Error::Unavailable)?;
        if before.present as usize > self.entries.len() {
            return Err(Error::Unavailable);
        }
        let mut found = [Entry::default(); 32];
        let mut count = 0;
        for index in 0..before.present {
            let info = outputs.info(index).map_err(|_| Error::Stale)?;
            if info.topology_revision != before.revision {
                return Err(Error::Stale);
            }
            let state = match outputs.power(&info.identity) {
                Ok(state) => state,
                Err(rc)
                    if [
                        a::err_no_fn,
                        a::err_no_group,
                        a::gfx_output_error_unsupported,
                    ]
                    .contains(&rc) =>
                {
                    continue;
                }
                Err(_) => return Err(Error::Stale),
            };
            if state.version != 1
                || state.size as usize != core::mem::size_of::<a::GfxOutputPower>()
                || !same_id(state.identity, info.identity)
            {
                return Err(Error::Stale);
            }
            if state.capabilities & a::gfx_power_cap_signal == 0 {
                continue;
            }
            let mut entry = Entry {
                state,
                ..Entry::default()
            };
            if let Some(old) = self.entries[..self.count]
                .iter()
                .find(|e| same_id(e.state.identity, state.identity))
            {
                entry.request = old.request;
                entry.renew_ns = old.renew_ns;
            }
            found[count] = entry;
            count += 1;
        }
        let after = outputs.revision().map_err(|_| Error::Stale)?;
        if before.revision != after.revision {
            return Err(Error::Stale);
        }
        self.entries = found;
        self.count = count;
        self.observed_current = true;
        Ok(())
    }
    pub fn sleep(&mut self, outputs: &mut impl OutputPower, now: u64) -> Result<(), Error> {
        if now == 0 {
            return Err(Error::Clock);
        }
        self.discover(outputs)?;
        if self.count == 0 {
            return Err(Error::Unsupported);
        }
        self.want_off = true;
        self.wake_pending = false;
        self.faulted = false;
        self.transition_deadline = now.saturating_add(35 * SECOND);
        self.poll_ns = 0;
        Ok(())
    }
    /// The waking event is consumed, so it cannot activate a hidden control.
    pub fn input(&mut self, outputs: &mut impl OutputPower, now: u64) -> bool {
        self.input_observed(outputs, now, true)
    }
    /// Only for later samples of the same poll_input burst, after input() took
    /// its fresh copied snapshot. No snapshot is reused across loop iterations.
    pub fn input_cached(&mut self, outputs: &mut impl OutputPower, now: u64) -> bool {
        self.input_observed(outputs, now, false)
    }
    fn input_observed(&mut self, outputs: &mut impl OutputPower, now: u64, fresh: bool) -> bool {
        self.last_activity_ns = now;
        if fresh {
            let _ = self.discover(outputs);
        }
        let mut sleeping = self.want_off || self.wake_pending;
        for entry in &self.entries[..self.count] {
            if (entry.state.phase >= a::gfx_power_phase_stopping
                && entry.state.phase <= a::gfx_power_phase_waking)
                || entry.request.off != 0
                || (self.faulted
                    && entry.request.sequence != 0
                    && entry.state.phase == a::gfx_power_phase_unavailable
                    && entry.state.reason != a::gfx_power_reason_link)
            {
                sleeping = true;
                break;
            }
        }
        self.want_off = false;
        self.faulted = false;
        if sleeping {
            let begin_wake = !self.wake_pending;
            if begin_wake {
                self.transition_deadline = now.saturating_add(35 * SECOND);
                for entry in &mut self.entries[..self.count] {
                    entry.request = a::GfxPowerRequest::default();
                }
            }
            self.wake_pending = true;
            if fresh || begin_wake {
                self.poll_ns = 0;
                if now != 0 && self.observed_current {
                    self.poll_ns = now.saturating_add(500_000_000);
                    self.drive(outputs, now, 0);
                }
            }
        }
        sleeping
    }
    pub fn tick(&mut self, outputs: &mut impl OutputPower, now: u64, idle_seconds: u32) {
        if now == 0 || now < self.poll_ns {
            return;
        }
        self.poll_ns = now.saturating_add(500_000_000);
        if self.last_activity_ns == 0 || now < self.last_activity_ns {
            self.last_activity_ns = now;
        }
        if self.discover(outputs).is_err() {
            return;
        }
        self.drive(outputs, now, idle_seconds);
    }
    // Act only on the already copied discovery. A wake request never requires
    // a second catalog pass inside the same input event.
    fn drive(&mut self, outputs: &mut impl OutputPower, now: u64, idle_seconds: u32) {
        if !self.want_off
            && !self.wake_pending
            && !self.faulted
            && idle_seconds != 0
            && now - self.last_activity_ns >= u64::from(idle_seconds) * SECOND
        {
            let _ = self.sleep(outputs, now);
        }
        if !self.want_off && !self.wake_pending {
            return;
        }
        let mut complete = self.count != 0;
        for entry in &self.entries[..self.count] {
            let wanted = if self.want_off {
                a::gfx_power_phase_off
            } else {
                a::gfx_power_phase_on
            };
            if entry.state.phase != wanted {
                complete = false;
            }
            if !self.want_off
                && (entry.request.off != 0
                    || entry.request.sequence == 0
                    || entry.state.request_sequence < entry.request.sequence)
            {
                complete = false;
            }
            if self.want_off && entry.state.phase == a::gfx_power_phase_unavailable {
                self.want_off = false;
                self.wake_pending = true;
                self.faulted = true;
                self.transition_deadline = now.saturating_add(35 * SECOND);
                complete = false;
                break;
            }
        }
        if complete && self.wake_pending {
            self.wake_pending = false;
            return;
        }
        if !complete && now >= self.transition_deadline {
            if self.want_off {
                self.want_off = false;
                self.wake_pending = true;
                self.transition_deadline = now.saturating_add(35 * SECOND);
            } else {
                self.wake_pending = false;
            }
            self.faulted = true;
        }
        for entry in &mut self.entries[..self.count] {
            let off = u32::from(self.want_off);
            if entry.request.sequence != 0
                && entry.request.off == off
                && (off == 0 || now < entry.renew_ns)
            {
                continue;
            }
            if let Ok(result) = outputs.request_power(&a::GfxPowerRequest {
                identity: entry.state.identity,
                off,
                ..a::GfxPowerRequest::default()
            }) {
                entry.request = result;
                entry.renew_ns = now.saturating_add(10 * SECOND);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Model {
        state: a::GfxOutputPower,
        intent: a::GfxPowerRequest,
        calls: usize,
        serial: u64,
        supported: bool,
        stale: bool,
        reads: usize,
    }
    impl Default for Model {
        fn default() -> Self {
            Self {
                state: a::GfxOutputPower {
                    identity: a::GfxOutputId {
                        adapter_id: 1,
                        connector_id: 2,
                        device_generation: 3,
                        connection_generation: 4,
                    },
                    capabilities: 3,
                    phase: a::gfx_power_phase_on,
                    ..a::GfxOutputPower::default()
                },
                intent: a::GfxPowerRequest::default(),
                calls: 0,
                serial: 0,
                supported: true,
                stale: false,
                reads: 0,
            }
        }
    }
    impl OutputCatalog for Model {
        fn revision(&mut self) -> NativeResult<a::GfxDisplayRevision> {
            self.reads += 1;
            Ok(a::GfxDisplayRevision {
                revision: 1,
                present: 1,
                ..a::GfxDisplayRevision::default()
            })
        }
        fn info(&mut self, index: u32) -> NativeResult<a::GfxOutputInfo> {
            self.reads += 1;
            assert_eq!(index, 0);
            Ok(a::GfxOutputInfo {
                identity: self.state.identity,
                topology_revision: if self.stale { 2 } else { 1 },
                ..a::GfxOutputInfo::default()
            })
        }
        fn power(&mut self, id: &a::GfxOutputId) -> NativeResult<a::GfxOutputPower> {
            self.reads += 1;
            assert!(same_id(*id, self.state.identity));
            if self.supported {
                Ok(self.state)
            } else {
                Err(a::err_no_fn)
            }
        }
    }
    impl OutputPower for Model {
        fn request_power(
            &mut self,
            request: &a::GfxPowerRequest,
        ) -> NativeResult<a::GfxPowerRequest> {
            assert!(same_id(request.identity, self.state.identity) && request.sequence == 0);
            self.calls += 1;
            if request.off == 0
                || self.intent.sequence == 0
                || self.intent.off != request.off
                || !same_id(self.intent.identity, request.identity)
            {
                self.serial += 1;
            }
            self.intent = *request;
            self.intent.sequence = self.serial;
            Ok(self.intent)
        }
    }
    #[test]
    fn original_screen_power_receipts_reconnect_renew_failed_wake_idle_and_torn_catalog() {
        let mut draw = Model::default();
        let mut owner = Owner::default();
        draw.supported = false;
        assert_eq!(owner.sleep(&mut draw, SECOND), Err(Error::Unsupported));
        assert!(!owner.input(&mut draw, SECOND) && draw.calls == 0);
        draw.supported = true;
        owner.sleep(&mut draw, 2 * SECOND).unwrap();
        owner.tick(&mut draw, 2 * SECOND, 0);
        assert!(draw.intent.off == 1 && draw.calls == 1);
        assert!(owner.input(&mut draw, 3 * SECOND));
        assert!(draw.intent.off == 0 && draw.serial == 2 && owner.wake_pending());
        draw.state.request_sequence = draw.intent.sequence;
        owner.tick(&mut draw, 4 * SECOND, 0);
        assert!(!owner.wake_pending() && !owner.input(&mut draw, 5 * SECOND));
        owner.sleep(&mut draw, 6 * SECOND).unwrap();
        owner.tick(&mut draw, 6 * SECOND, 0);
        draw.state.phase = a::gfx_power_phase_off;
        draw.state.request_sequence = draw.intent.sequence;
        owner.tick(&mut draw, 15 * SECOND, 0);
        assert_eq!(draw.calls, 3);
        owner.tick(&mut draw, 16 * SECOND, 0);
        assert!(draw.calls == 4 && draw.serial == 3);
        assert!(owner.input(&mut draw, 17 * SECOND));
        draw.state.identity.connection_generation += 1;
        draw.state.phase = a::gfx_power_phase_on;
        draw.state.request_sequence = 0;
        owner.tick(&mut draw, 18 * SECOND, 0);
        assert!(draw.intent.off == 0 && draw.serial == 5 && owner.wake_pending());
        draw.state.request_sequence = draw.intent.sequence;
        owner.tick(&mut draw, 19 * SECOND, 0);
        assert!(!owner.wake_pending());
        owner.sleep(&mut draw, 20 * SECOND).unwrap();
        owner.tick(&mut draw, 20 * SECOND, 0);
        draw.state.phase = a::gfx_power_phase_unavailable;
        draw.state.reason = a::gfx_power_reason_rejected;
        owner.tick(&mut draw, 21 * SECOND, 0);
        assert!(
            !owner.want_off() && owner.wake_pending() && owner.faulted() && draw.intent.off == 0
        );
        owner.tick(&mut draw, 57 * SECOND, 0);
        let calls = draw.calls;
        owner.tick(&mut draw, 100 * SECOND, 30);
        assert!(!owner.wake_pending() && owner.faulted() && draw.calls == calls);
        assert!(owner.input(&mut draw, 101 * SECOND));
        assert!(owner.wake_pending() && draw.calls == calls + 1);
        owner = Owner::default();
        draw.state.reason = a::gfx_power_reason_link;
        assert!(!owner.input(&mut draw, 102 * SECOND));
        draw.state.phase = a::gfx_power_phase_on;
        owner.tick(&mut draw, 131 * SECOND, 30);
        assert!(!owner.want_off());
        owner.tick(&mut draw, 132 * SECOND, 30);
        assert!(owner.want_off() && draw.intent.off == 1);
        draw.stale = true;
        assert_eq!(owner.sleep(&mut draw, 133 * SECOND), Err(Error::Stale));
    }
    #[test]
    fn d088_awake_and_waking_bursts_share_reads_and_keep_fresh_ack() {
        let mut draw = Model::default();
        let mut owner = Owner::default();
        assert!(!owner.input(&mut draw, SECOND));
        for _ in 1..32 {
            assert!(!owner.input_cached(&mut draw, SECOND));
        }
        assert_eq!(draw.reads, 4);
        std::println!(
            "{{\"schema\":\"r4os.desktop-work\",\"case\":\"power-after88\",\"events\":32,\"catalog_power_reads\":{}}}",
            draw.reads
        );
        assert!(!owner.input(&mut draw, SECOND + 1));
        assert_eq!(draw.reads, 8);
        owner.sleep(&mut draw, 2 * SECOND).unwrap();
        owner.tick(&mut draw, 2 * SECOND, 0);
        draw.state.phase = a::gfx_power_phase_off;
        draw.reads = 0;
        assert!(owner.input(&mut draw, 3 * SECOND));
        let wake_requests = draw.calls;
        for _ in 1..32 {
            assert!(owner.input_cached(&mut draw, 3 * SECOND));
        }
        assert_eq!((draw.reads, draw.calls), (4, wake_requests));
        // A later burst sees a real ACK even without a topology revision.
        draw.state.phase = a::gfx_power_phase_on;
        draw.state.request_sequence = draw.intent.sequence;
        assert!(owner.input(&mut draw, 3 * SECOND + 1));
        assert!(!owner.input_cached(&mut draw, 3 * SECOND + 1));
        // A sleep intent inside this burst still consumes the next wake event.
        owner.sleep(&mut draw, 4 * SECOND).unwrap();
        owner.tick(&mut draw, 4 * SECOND, 0);
        assert!(owner.input_cached(&mut draw, 4 * SECOND));
        assert_eq!(draw.intent.off, 0);
        draw.stale = true;
        let requests = draw.calls;
        assert!(owner.input(&mut draw, 4 * SECOND + 1));
        for _ in 0..32 {
            assert!(owner.input_cached(&mut draw, 4 * SECOND + 1));
        }
        assert_eq!(draw.calls, requests); // Failed discovery grants no new request.
        draw.stale = false;
        owner.tick(&mut draw, 4 * SECOND + 2, 0);
        assert!(owner.wake_pending());
    }
}
