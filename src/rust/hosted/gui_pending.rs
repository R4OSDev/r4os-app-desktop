//! Bounded main-task GUI suffix ownership. Native admission never yields;
//! a busy destination cannot turn a GUI key into a shell command. Only the
//! exact window incarnation may acknowledge records or its cancel barrier.
use alloc::collections::VecDeque;
use r4os_contract as a;
const CAPACITY: usize = 128;
const FLUSH_BUDGET: usize = 16;
pub trait Sink {
    fn query(
        &mut self,
        owner: a::ProgramProcessHandle,
        index: i32,
    ) -> Result<a::GuiPointerWindowIdentity, i32>;
    fn post(&mut self, window: a::GuiPointerWindowIdentity, event: a::GuiEvent) -> i32;
    /// The current provider's RESET is allocation-free and has an independent
    /// reliable control slot: a valid exact target cannot return queue Busy.
    fn reset(&mut self, window: a::GuiPointerWindowIdentity, reason: u32, tick: u64) -> i32;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Receipt {
    Accepted,
    Pending,
    Cancelled,
}
#[derive(Default)]
pub struct Pending {
    window: a::GuiPointerWindowIdentity,
    events: VecDeque<a::GuiEvent>,
}
fn same(a: a::ProgramProcessHandle, b: a::ProgramProcessHandle) -> bool {
    a.instance_id != 0
        && a.instance_id == b.instance_id
        && a.generation != 0
        && a.generation == b.generation
        && a.reserved == 0
        && b.reserved == 0
}
fn input(kind: u32) -> bool {
    [
        a::GuiEventKind::key_down.0,
        a::GuiEventKind::mouse_down.0,
        a::GuiEventKind::mouse_up.0,
        a::GuiEventKind::mouse_move.0,
        a::GuiEventKind::physical_key_down.0,
        a::GuiEventKind::physical_key_up.0,
        a::GuiEventKind::physical_key_reset.0,
        a::GuiEventKind::mouse_wheel.0,
    ]
    .contains(&kind)
}
impl Pending {
    pub fn busy(&self) -> bool {
        !self.events.is_empty()
    }
    pub fn forget(&mut self) {
        self.events.clear();
        self.window = a::GuiPointerWindowIdentity::default();
    }
    /// Association/rebind is explicit. No rejected old record is retried into
    /// a new epoch merely because a later lookup returns the same numeric ID.
    pub fn bind(
        &mut self,
        sink: &mut impl Sink,
        owner: a::ProgramProcessHandle,
        index: i32,
    ) -> bool {
        if self.window.epoch != 0 {
            // The former incarnation may still be alive. End its input before
            // taking a new binding; stale exact targets cannot affect reuse.
            let _ = sink.reset(self.window, a::gui_input_reset_reason_window_closed, 0);
        }
        self.forget();
        let Ok(window) = sink.query(owner, index) else {
            return false;
        };
        if !same(window.owner, owner)
            || window.window_id != index
            || window.epoch == 0
            || window.reserved != 0
        {
            return false;
        }
        self.window = window;
        true
    }
    fn ensure(&mut self, sink: &mut impl Sink, owner: a::ProgramProcessHandle, index: i32) -> bool {
        if same(self.window.owner, owner)
            && self.window.window_id == index
            && self.window.epoch != 0
        {
            true
        } else {
            self.bind(sink, owner, index)
        }
    }
    /// RESET explicitly cancels unread input. Keep final absolute metadata in
    /// its original relative order; no accepted prefix is played again.
    pub fn cancel(&mut self, sink: &mut impl Sink, reason: u32, tick: u64) -> bool {
        if self.window.epoch == 0 {
            self.forget();
            return false;
        }
        if sink.reset(self.window, reason, tick) != 0 {
            self.forget();
            return false;
        }
        self.events.retain(|event| !input(event.kind));
        // This is a cancel boundary, with no surviving input between metadata.
        // Repeated absolute resize/font/focus states may collapse to their last
        // occurrence; Close remains a terminal record. This also bounds an
        // all-metadata overload without dropping a held input silently.
        let mut index = 0;
        while index < self.events.len() {
            let kind = self.events[index].kind;
            if self
                .events
                .iter()
                .skip(index + 1)
                .any(|event| event.kind == kind)
            {
                self.events.remove(index);
            } else {
                index += 1;
            }
        }
        true
    }
    pub fn cancel_for(
        &mut self,
        sink: &mut impl Sink,
        owner: a::ProgramProcessHandle,
        index: i32,
        reason: u32,
        tick: u64,
    ) -> bool {
        self.ensure(sink, owner, index) && self.cancel(sink, reason, tick)
    }
    pub fn send(
        &mut self,
        sink: &mut impl Sink,
        owner: a::ProgramProcessHandle,
        index: i32,
        event: a::GuiEvent,
    ) -> Receipt {
        if !self.ensure(sink, owner, index) {
            return Receipt::Cancelled;
        }
        if self.events.is_empty() {
            match sink.post(self.window, event) {
                0 => return Receipt::Accepted,
                a::gui_host_input_error_busy => {}
                _ => {
                    self.forget();
                    return Receipt::Cancelled;
                }
            }
        }
        if event.kind == a::GuiEventKind::mouse_move.0 {
            if let Some(at) = self.events.iter().position(|old| old.kind == event.kind) {
                self.events.remove(at);
            }
        }
        if self.events.len() == CAPACITY
            || (self.events.capacity() < CAPACITY
                && self
                    .events
                    .try_reserve_exact(CAPACITY - self.events.len())
                    .is_err())
        {
            let cancelled = self.cancel(sink, a::gui_input_reset_reason_overflow, event.tick);
            if !cancelled || input(event.kind) {
                return Receipt::Cancelled;
            }
            // At most one final event of each supported metadata kind remains.
            // The bounded buffer was already allocated in a capacity overflow.
            if self.events.try_reserve(1).is_err() {
                self.forget();
                return Receipt::Cancelled;
            }
        }
        self.events.push_back(event);
        Receipt::Pending
    }
    /// At most one bounded prefix per client and main-loop turn. Busy leaves
    /// the exact front in place; the native queue arms a one-shot space wake.
    pub fn flush(&mut self, sink: &mut impl Sink) -> bool {
        let mut changed = false;
        for _ in 0..FLUSH_BUDGET {
            let Some(event) = self.events.front().copied() else {
                break;
            };
            match sink.post(self.window, event) {
                0 => {
                    self.events.pop_front();
                    changed = true;
                }
                a::gui_host_input_error_busy => break,
                _ => {
                    self.forget();
                    return true;
                }
            }
        }
        changed
    }
}

/// Short native host-session loans; no Session or provider pointer enters a
/// stored event or survives the synchronous call.
pub struct Native<'loan, 'app, 'storage>(pub &'loan mut r4os_sdk::gui::Session<'app, 'storage>);
impl Sink for Native<'_, '_, '_> {
    fn query(
        &mut self,
        owner: a::ProgramProcessHandle,
        index: i32,
    ) -> Result<a::GuiPointerWindowIdentity, i32> {
        let request = a::GuiHostInputRequest {
            window: a::GuiPointerWindowIdentity {
                owner,
                window_id: index,
                ..Default::default()
            },
            operation: a::gui_host_input_operation_query,
            ..Default::default()
        };
        let mut out = a::GuiPointerWindowIdentity::default();
        let code = super::gui::Host::new(self.0).input(&request, &mut out);
        if code == 0 { Ok(out) } else { Err(code) }
    }
    fn post(&mut self, window: a::GuiPointerWindowIdentity, event: a::GuiEvent) -> i32 {
        let request = a::GuiHostInputRequest {
            window,
            event,
            operation: a::gui_host_input_operation_post,
            ..Default::default()
        };
        super::gui::Host::new(self.0).input(&request, &mut a::GuiPointerWindowIdentity::default())
    }
    fn reset(&mut self, window: a::GuiPointerWindowIdentity, reason: u32, tick: u64) -> i32 {
        let request = a::GuiHostInputRequest {
            window,
            reason,
            event: a::GuiEvent {
                tick,
                ..Default::default()
            },
            operation: a::gui_host_input_operation_reset,
        };
        super::gui::Host::new(self.0).input(&request, &mut a::GuiPointerWindowIdentity::default())
    }
}

#[cfg(test)]
#[path = "../../../Tests/RustNative/gui_pending.rs"]
mod tests;
