//! Event-driven admission for a window dragged on the primary software
//! output. Managed outputs already own independent clocks and damage queues.
use crate::render::geometry::{Dirty, Rect};
const FALLBACK_NS: u64 = 16_666_667;

#[derive(Default)]
pub(super) struct Pacing {
    last: Option<Rect>,
    desired: Option<Rect>,
    extras: Dirty,
    next: u64,
    force: bool,
}
impl Pacing {
    pub fn motion(&mut self, old: Rect, new: Rect, damage: Dirty) {
        if self.desired.is_none() {
            self.last = Some(old);
        }
        self.desired = Some(new);
        // Only old/new window and software cursor areas have moved. All
        // previously presented pixels are in 'last'. Preserve outlying
        // hover/cursor damage too, including jumps across application chrome.
        for &r in damage.regions() {
            if !contains(old, r) && !contains(new, r) {
                self.extras.invalidate(r);
            }
        }
    }
    pub fn pending(&self) -> bool {
        self.desired.is_some()
    }
    pub fn edge(&mut self) {
        if self.pending() {
            self.flush();
        } else {
            self.reset();
        }
    }
    pub fn flush(&mut self) {
        self.force |= self.desired.is_some();
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn stage(&self, dirty: &mut Dirty, now: u64, tick_ns: u64) -> bool {
        let Some(new) = self.desired else {
            return false;
        };
        if !dirty.active() && !self.force && now.saturating_add(tick_ns) < self.next {
            return false;
        }
        if let Some(old) = self.last {
            dirty.invalidate(old);
        }
        dirty.invalidate(new);
        for &r in self.extras.regions() {
            dirty.invalidate(r);
        }
        true
    }
    pub fn receipt(&mut self, accepted: bool, now: u64, interval: Option<u64>) {
        if !accepted {
            self.force = true;
            return;
        }
        self.last = self.desired.take();
        self.extras = Dirty::default();
        self.force = false;
        let period = interval
            .filter(|n| (1_000_000..=100_000_000).contains(n))
            .unwrap_or(FALLBACK_NS);
        // Anchor to output phase while input is continuous. A fresh gesture
        // gets its first picture immediately, even after a long idle.
        if self.next == 0 || now.saturating_sub(self.next) >= period {
            self.next = now.saturating_add(period);
        } else if self.next <= now.saturating_add(1_000_000) {
            self.next = self.next.saturating_add(period);
        }
    }
    pub fn wait(&self, now: u64, hz: u32, delay: u64) -> u64 {
        if self.desired.is_none() {
            return delay;
        }
        if self.force {
            return 1;
        }
        delay.min(
            self.next
                .saturating_sub(now)
                .saturating_mul(u64::from(hz))
                .div_ceil(1_000_000_000)
                .max(1),
        )
    }
}
fn contains(outer: Rect, r: Rect) -> bool {
    r.x >= outer.x && r.y >= outer.y && r.right() <= outer.right() && r.bottom() <= outer.bottom()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn high_rate_keeps_accepted_origin_latest_position_and_release_after_rejection() {
        let mut p = Pacing::default();
        let r = |x| Rect::new(x, 10, 40, 40);
        let mut out = Dirty::default();
        p.motion(r(0), r(1), Dirty::default());
        assert!(p.stage(&mut out, 1, 1));
        p.receipt(true, 1, None);
        out.take();
        for x in 2..=4 {
            p.motion(r(x - 1), r(x), Dirty::default());
        }
        assert!(!p.stage(&mut out, 8_000_000, 1));
        assert!(p.wait(8_000_000, 1000, 500) <= 9);
        p.flush();
        assert!(p.stage(&mut out, 9_000_000, 1));
        assert_eq!(out.take(), Some(r(1).merged(r(4))));
        p.receipt(false, 9_000_000, None);
        assert!(p.stage(&mut out, 9_100_000, 1));
        p.receipt(true, 9_100_000, None);
        assert_eq!(p.wait(10_000_000, 1000, 500), 500);
    }
}
