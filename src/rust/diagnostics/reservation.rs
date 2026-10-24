//! A bounded explicit reservation owns identities only. Expiry never grants
//! authority over unrelated clients or extends the caller's original lease.
use super::wire::{Identity, Op, Request};
#[derive(Default)]
pub struct Session {
    pub caller: Identity,
    targets: [Identity; 2],
    pub deadline: u64,
    pub input_generation: u64,
    pub benchmark: bool,
}
impl Session {
    pub fn targets(&self) -> &[Identity; 2] {
        &self.targets
    }
    pub fn owns(&self, value: Identity) -> bool {
        self.targets.iter().any(|v| v.same(value))
    }
    pub fn bind(&mut self, value: Identity) -> bool {
        if !value.valid() || self.caller.same(value) {
            return false;
        }
        if self.owns(value) {
            return true;
        }
        if let Some(slot) = self.targets.iter_mut().find(|v| !v.valid()) {
            *slot = value;
            true
        } else {
            false
        }
    }
    pub fn foreign(&self, windows: impl IntoIterator<Item = Identity>, binding: Identity) -> bool {
        windows
            .into_iter()
            .any(|v| v.valid() && !self.owns(v) && !v.same(self.caller) && !v.same(binding))
    }
    pub fn admit(
        &mut self,
        r: Request,
        now: u64,
        hz: u32,
        input: u64,
        caller_live: bool,
        foreign: bool,
        interaction: bool,
    ) -> bool {
        if !caller_live || r.deadline_tick <= now {
            return false;
        }
        let limit = if r.op == Op::BeginBenchmark || self.benchmark {
            300_000u64
        } else {
            60_000
        };
        if r.deadline_tick - now > limit.saturating_mul(u64::from(hz)).div_ceil(1000) {
            return false;
        }
        if matches!(r.op, Op::Begin | Op::BeginBenchmark) {
            if self.caller.valid() || foreign || interaction {
                return false;
            }
            *self = Self {
                caller: r.caller,
                targets: [Identity::default(); 2],
                deadline: r.deadline_tick,
                input_generation: input,
                benchmark: r.op == Op::BeginBenchmark,
            };
            return true;
        }
        if r.op == Op::End && !self.caller.valid() {
            return true;
        }
        if !self.caller.same(r.caller) {
            return false;
        }
        let cleanup = matches!(r.op, Op::End | Op::ScreenshotCancel | Op::ScreenshotStatus)
            || (r.op == Op::Capture && r.dx == 0);
        cleanup || (now < self.deadline && input == self.input_generation && !foreign)
    }
}
