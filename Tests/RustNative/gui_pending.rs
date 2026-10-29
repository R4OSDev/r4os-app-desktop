//! Transport acknowledgements drive the actual Pending owner, not a second
//! delivery model. Native queue/control semantics are qualified separately.
use super::*;
#[derive(Default)]
struct Transport {
    live: a::GuiPointerWindowIdentity,
    budget: usize,
    barrier: bool,
    accepted: Vec<a::GuiEvent>,
    reset: Vec<(a::GuiPointerWindowIdentity, u32, u64)>,
}
fn owner(generation: u64) -> a::ProgramProcessHandle {
    a::ProgramProcessHandle {
        instance_id: 12,
        generation,
        reserved: 0,
    }
}
fn window(generation: u64, epoch: u64) -> a::GuiPointerWindowIdentity {
    a::GuiPointerWindowIdentity {
        owner: owner(generation),
        epoch,
        window_id: 1,
        reserved: 0,
    }
}
fn event(kind: a::GuiEventKind, tick: u64) -> a::GuiEvent {
    a::GuiEvent {
        kind: kind.0,
        window_id: 1,
        key: 0x61,
        tick,
        ..Default::default()
    }
}
impl Transport {
    fn new() -> Self {
        Self {
            live: window(11, 91),
            ..Default::default()
        }
    }
    fn current(&self, w: a::GuiPointerWindowIdentity) -> bool {
        same(w.owner, self.live.owner)
            && w.window_id == self.live.window_id
            && w.epoch == self.live.epoch
            && w.reserved == 0
    }
    fn received(&self) -> Vec<u64> {
        self.accepted.iter().map(|e| e.tick).collect()
    }
}
impl Sink for Transport {
    fn query(
        &mut self,
        owner: a::ProgramProcessHandle,
        index: i32,
    ) -> Result<a::GuiPointerWindowIdentity, i32> {
        if same(owner, self.live.owner) && index == self.live.window_id {
            Ok(self.live)
        } else {
            Err(a::gui_host_input_error_stale)
        }
    }
    fn post(&mut self, w: a::GuiPointerWindowIdentity, e: a::GuiEvent) -> i32 {
        if !self.current(w) {
            return a::gui_host_input_error_stale;
        }
        if self.budget == 0 || self.barrier {
            return a::gui_host_input_error_busy;
        }
        self.budget -= 1;
        self.accepted.push(e);
        0
    }
    fn reset(&mut self, w: a::GuiPointerWindowIdentity, reason: u32, tick: u64) -> i32 {
        if !self.current(w) {
            return a::gui_host_input_error_stale;
        }
        self.reset.push((w, reason, tick));
        self.barrier = true;
        0
    }
}
#[test]
fn delayed_consumer_preserves_key_physical_reset_and_final_button_release_prefixes() {
    let mut p = Pending::default();
    let mut t = Transport::new();
    for (i, kind) in [
        a::GuiEventKind::key_down,
        a::GuiEventKind::physical_key_up,
        a::GuiEventKind::physical_key_reset,
        a::GuiEventKind::mouse_down,
        a::GuiEventKind::mouse_up,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            p.send(&mut t, owner(11), 1, event(kind, i as u64 + 1)),
            Receipt::Pending
        );
    }
    assert!(t.accepted.is_empty());
    t.budget = 2;
    assert!(p.flush(&mut t));
    assert_eq!(t.received(), [1, 2]);
    assert!(p.busy());
    assert!(!p.flush(&mut t));
    assert_eq!(t.received(), [1, 2]);
    t.budget = 3;
    assert!(p.flush(&mut t));
    assert!(!p.busy());
    assert_eq!(t.received(), [1, 2, 3, 4, 5]);
    assert_eq!(t.accepted[2].kind, a::GuiEventKind::physical_key_reset.0);
    assert_eq!(t.accepted[4].kind, a::GuiEventKind::mouse_up.0);
}
#[test]
fn stale_process_generation_never_receives_a_retained_suffix_or_retargets_old_records() {
    let mut p = Pending::default();
    let mut t = Transport::new();
    assert_eq!(
        p.send(&mut t, owner(11), 1, event(a::GuiEventKind::key_down, 1)),
        Receipt::Pending
    );
    t.live = window(12, 92);
    t.budget = 4;
    assert!(p.flush(&mut t));
    assert!(!p.busy());
    assert!(t.accepted.is_empty());
    assert_eq!(
        p.send(&mut t, owner(12), 1, event(a::GuiEventKind::key_down, 2)),
        Receipt::Accepted
    );
    assert_eq!(t.received(), [2]);
}
#[test]
fn reopened_same_process_window_has_a_new_epoch_and_no_old_input_replay() {
    let mut p = Pending::default();
    let mut t = Transport::new();
    p.send(&mut t, owner(11), 1, event(a::GuiEventKind::mouse_up, 1));
    t.live = window(11, 92);
    t.budget = 3;
    assert!(p.flush(&mut t));
    assert!(!p.busy());
    assert!(t.accepted.is_empty());
    assert_eq!(
        p.send(&mut t, owner(11), 1, event(a::GuiEventKind::mouse_down, 2)),
        Receipt::Accepted
    );
    assert_eq!(t.received(), [2]);
}
#[test]
fn move_coalescing_removes_and_appends_without_displacing_down_or_up() {
    let mut p = Pending::default();
    let mut t = Transport::new();
    for (kind, tick) in [
        (a::GuiEventKind::mouse_move, 1),
        (a::GuiEventKind::mouse_down, 2),
        (a::GuiEventKind::mouse_move, 3),
        (a::GuiEventKind::mouse_up, 4),
        (a::GuiEventKind::mouse_move, 5),
    ] {
        p.send(&mut t, owner(11), 1, event(kind, tick));
    }
    t.budget = 8;
    assert!(p.flush(&mut t));
    assert_eq!(t.received(), [2, 4, 5]);
    assert_eq!(t.accepted[0].kind, a::GuiEventKind::mouse_down.0);
    assert_eq!(t.accepted[1].kind, a::GuiEventKind::mouse_up.0);
}
#[test]
fn finite_overload_has_an_explicit_barrier_and_no_silent_up_loss_or_old_replay() {
    let mut p = Pending::default();
    let mut t = Transport::new();
    for tick in 1..=CAPACITY as u64 {
        p.send(
            &mut t,
            owner(11),
            1,
            event(a::GuiEventKind::physical_key_down, tick),
        );
    }
    assert!(p.busy());
    assert_eq!(
        p.send(&mut t, owner(11), 1, event(a::GuiEventKind::mouse_up, 200)),
        Receipt::Cancelled
    );
    assert!(!p.busy());
    assert_eq!(t.reset.len(), 1);
    assert_eq!(
        (t.reset[0].1, t.reset[0].2),
        (a::gui_input_reset_reason_overflow, 200)
    );
    t.budget = 4;
    assert_eq!(
        p.send(&mut t, owner(11), 1, event(a::GuiEventKind::key_down, 201)),
        Receipt::Pending
    );
    assert!(!p.flush(&mut t));
    assert!(t.accepted.is_empty());
    t.barrier = false;
    assert!(p.flush(&mut t));
    assert_eq!(t.received(), [201]);
}
#[test]
fn focus_cancel_preserves_control_and_never_replays_unaccepted_old_button_up() {
    let mut p = Pending::default();
    let mut t = Transport::new();
    t.budget = 1;
    assert_eq!(
        p.send(&mut t, owner(11), 1, event(a::GuiEventKind::mouse_down, 1)),
        Receipt::Accepted
    );
    assert_eq!(
        p.send(&mut t, owner(11), 1, event(a::GuiEventKind::mouse_up, 2)),
        Receipt::Pending
    );
    assert!(p.cancel_for(
        &mut t,
        owner(11),
        1,
        a::gui_input_reset_reason_focus_lost,
        3
    ));
    assert_eq!(
        p.send(&mut t, owner(11), 1, event(a::GuiEventKind::focus_lost, 4)),
        Receipt::Pending
    );
    t.budget = 4;
    assert!(!p.flush(&mut t));
    t.barrier = false;
    assert!(p.flush(&mut t));
    assert_eq!(t.received(), [1, 4]);
    assert_eq!(t.reset.len(), 1);
}
#[test]
fn metadata_overload_keeps_final_absolute_states_and_terminal_close_after_cancel() {
    let mut p = Pending::default();
    let mut t = Transport::new();
    for tick in 1..=CAPACITY as u64 {
        p.send(&mut t, owner(11), 1, event(a::GuiEventKind::resize, tick));
    }
    assert_eq!(
        p.send(&mut t, owner(11), 1, event(a::GuiEventKind::close, 200)),
        Receipt::Pending
    );
    assert_eq!(t.reset.len(), 1);
    t.barrier = false;
    t.budget = 8;
    assert!(p.flush(&mut t));
    assert_eq!(t.received(), [128, 200]);
    assert_eq!(t.accepted[1].kind, a::GuiEventKind::close.0);
}
#[test]
fn failed_exact_query_does_not_accept_a_different_owner_from_the_provider() {
    let mut p = Pending::default();
    let mut t = Transport::new();
    t.live = window(12, 92);
    t.budget = 4;
    assert_eq!(
        p.send(&mut t, owner(11), 1, event(a::GuiEventKind::key_down, 1)),
        Receipt::Cancelled
    );
    assert!(!p.busy());
    assert!(t.accepted.is_empty());
    assert!(t.reset.is_empty());
}
