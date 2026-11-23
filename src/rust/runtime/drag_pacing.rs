//! Event-driven admission for window movement and resize on the primary software
//! output. Managed outputs already own independent clocks and damage queues.
use crate::render::geometry::{Dirty, Rect};
const FALLBACK_NS: u64 = 16_666_667;

/// Identifies the view present when a mouse resize was dispatched. A new
/// generation is a wake-up hint; its contents still follow normal repaint
/// and receipt handling even when the producer responds out of order.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ClientFrame {
    pub instance: u32,
    pub process_generation: u64,
    pub frame_generation: u64,
    pub geometry: Rect,
}

struct ClientWait {
    source: ClientFrame,
    until: u64,
}

#[derive(Default)]
pub(super) struct Pacing {
    last: Option<Rect>,
    desired: Option<Rect>,
    extras: Dirty,
    next: u64,
    force: bool,
    client: Option<ClientWait>,
}
impl Pacing {
    pub fn expect_client(&mut self, source: Option<ClientFrame>, now: u64, interval: Option<u64>) {
        // Yield a small part of the current refresh period to a responsive
        // producer. Replacing motion must never extend a stalled frame.
        let grace = (interval.unwrap_or(FALLBACK_NS) / 8).min(2_000_000);
        let until = now.saturating_add(grace);
        let until = self.client.as_ref().map_or(until, |ack| ack.until.min(until));
        self.client = source.map(|source| ClientWait { source, until });
    }
    pub fn expecting_client(&self) -> bool {
        self.client.is_some()
    }
    pub fn awaiting_client(&self, now: u64, current: Option<ClientFrame>) -> bool {
        !self.force && self.client.as_ref().is_some_and(|wait| {
            now < wait.until && current == Some(wait.source)
        })
    }
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
        self.client = None;
        self.force |= self.desired.is_some();
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn due(&self, now: u64, tick_ns: u64) -> bool {
        self.force || now.saturating_add(tick_ns) >= self.next
    }
    pub fn stage(&self, dirty: &mut Dirty, now: u64, tick_ns: u64) -> bool {
        let Some(new) = self.desired else {
            return false;
        };
        if !dirty.active() && !self.due(now, tick_ns) {
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
        self.client = None;
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
    pub fn wait(&self, now: u64, hz: u32, delay: u64, resize_queued: bool) -> u64 {
        if self.desired.is_none() && !resize_queued {
            return delay;
        }
        if self.force {
            return 1;
        }
        let deadline = self.client.as_ref().map_or(self.next, |wait| wait.until);
        delay.min(
            deadline
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
    fn resize_wait_is_bounded_and_only_applies_to_the_requested_client() {
        let mut pacing = Pacing::default();
        let source = ClientFrame {
            instance: 7,
            process_generation: 20,
            frame_generation: 42,
            geometry: Rect::new(10, 20, 200, 100),
        };
        pacing.motion(Rect::new(10, 20, 199, 100), source.geometry, Dirty::default());
        pacing.expect_client(Some(source), 1_000_000, Some(FALLBACK_NS));
        assert!(pacing.awaiting_client(1_001_000, Some(source)));
        assert!(!pacing.awaiting_client(1_001_000, None));
        assert!(!pacing.awaiting_client(1_001_000, Some(ClientFrame { frame_generation: 43, ..source })));
        assert!(!pacing.awaiting_client(1_001_000, Some(ClientFrame { process_generation: 21, ..source })));
        let next = ClientFrame { geometry: Rect::new(10, 20, 201, 100), ..source };
        assert!(!pacing.awaiting_client(1_001_000, Some(next)));
        pacing.expect_client(Some(next), 2_900_000, Some(FALLBACK_NS));
        assert!(!pacing.awaiting_client(3_000_000, Some(next)));
        pacing.flush();
        assert!(!pacing.awaiting_client(2_900_001, Some(next)));
        pacing.receipt(false, 3_000_000, None);
        assert!(pacing.pending() && pacing.due(3_000_000, 1));
        pacing.receipt(true, 3_100_000, None);
        assert_eq!(pacing.wait(4_000_000, 1_000, 500, false), 500);
    }
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
        assert!(p.wait(8_000_000, 1000, 500, false) <= 9);
        p.flush();
        assert!(p.stage(&mut out, 9_000_000, 1));
        assert_eq!(out.take(), Some(r(1).merged(r(4))));
        p.receipt(false, 9_000_000, None);
        assert!(p.stage(&mut out, 9_100_000, 1));
        p.receipt(true, 9_100_000, None);
        assert_eq!(p.wait(10_000_000, 1000, 500, false), 500);
        assert!(!p.due(10_000_000, 1_000_000));
        assert!(p.wait(10_000_000, 1000, 500, true) <= 7);
        assert!(p.due(16_000_000, 1_000_000));
    }
}
