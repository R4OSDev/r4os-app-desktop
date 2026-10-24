//! Bounded asynchronous WINSVC mode work. One exact transaction is in flight;
//! received mode intents and retained ACKs remain separate from local geometry.
use super::{process, runtime::State};
use crate::{
    outputs::manager::Manager,
    render::geometry::Dirty,
    runtime::{service::Host, wire::Wire},
    windows::{layout, window::Kind},
};
use r4os_sdk::{App, abi as a, gui::Session};
#[derive(Clone, Copy, Default)]
struct Slot {
    exchange: a::WindowModeExchange,
    ack_pending: bool,
    next_tick: u64,
}
#[derive(Default)]
pub struct Modes {
    slots: [Slot; 4],
    cursor: usize,
    last_tick: Option<u64>,
    pending: bool,
}
struct Intent {
    index: usize,
    request_id: u64,
    mode: u32,
}
impl Modes {
    pub fn busy(&self) -> bool {
        self.pending
    }
    fn request(
        &mut self,
        index: usize,
        owner: a::ProgramProcessHandle,
        live: bool,
        mode: u32,
        desktop: a::ProgramProcessHandle,
        tick: u64,
    ) -> Option<a::WindowModeExchange> {
        let slot = &mut self.slots[index];
        if slot.exchange.identity.serial != 0 && !process::same(slot.exchange.identity.owner, owner)
        {
            let mut request = slot.exchange;
            request.flags = 1;
            return Some(request);
        }
        if !live || !process::valid(owner) {
            return None;
        }
        if !process::same(slot.exchange.identity.owner, owner) {
            *slot = Slot {
                exchange: a::WindowModeExchange {
                    identity: a::WindowModeIdentity {
                        desktop,
                        owner,
                        window_id: index as u32,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            };
        }
        if !slot.ack_pending && slot.exchange.mode == mode && tick < slot.next_tick {
            return None;
        }
        let mut request = slot.exchange;
        // An ACK reports the geometry which was actually applied for that
        // exact nonce. Later local toggles are published only after its ACK.
        if !slot.ack_pending {
            request.mode = mode
        }
        Some(request)
    }
    fn sent(&mut self, index: usize, request: a::WindowModeExchange, tick: u64, interval: u64) {
        if request.flags == 0 {
            self.slots[index].exchange.mode = request.mode
        }
        self.slots[index].next_tick = tick.saturating_add(interval);
        self.cursor = (index + 1) % 4;
    }
    fn finish(
        &mut self,
        request: a::WindowModeExchange,
        reply: a::WindowModeReply,
        current: a::ProgramProcessHandle,
        admit: bool,
    ) -> Result<Option<Intent>, ()> {
        let index = request.identity.window_id as usize;
        let Some(slot) = self.slots.get_mut(index) else {
            return Err(());
        };
        if request.flags == 1 {
            if !slot.exchange.identity.same(&request.identity) {
                return Err(());
            }
            if [0, -3, -4].contains(&reply.result) {
                *slot = Slot::default();
                return Ok(None);
            }
            return Err(());
        }
        if reply.identity.serial == 0
            || !process::same(reply.identity.owner, request.identity.owner)
            || reply.identity.window_id != request.identity.window_id
            || !process::same(reply.identity.desktop, request.identity.desktop)
        {
            return Ok(None);
        }
        if !process::same(slot.exchange.identity.owner, request.identity.owner) {
            return Err(());
        }
        if !slot.exchange.identity.same(&reply.identity) {
            slot.exchange.identity = reply.identity;
            slot.exchange.ack_request_id = 0;
            slot.exchange.ack_result = 0;
            slot.ack_pending = false;
        }
        // Even a publication completed after window replacement is retained
        // above, so its exact server identity is removed before new admission.
        if !process::same(current, request.identity.owner) {
            return Ok(None);
        }
        if reply.phase != 1 {
            if request.ack_request_id == slot.exchange.ack_request_id {
                slot.ack_pending = false
            }
            return Ok(None);
        }
        if reply.request_id == 0 || reply.requested_mode > 1 {
            return Err(());
        }
        if slot.exchange.ack_request_id == reply.request_id {
            slot.ack_pending = true;
            return Ok(None);
        }
        if !admit {
            slot.exchange.ack_request_id = reply.request_id;
            slot.exchange.ack_result = -3;
            slot.exchange.mode = reply.mode;
            slot.ack_pending = true;
            return Ok(None);
        }
        Ok(Some(Intent {
            index,
            request_id: reply.request_id,
            mode: reply.requested_mode,
        }))
    }
    fn applied(&mut self, intent: Intent) {
        let slot = &mut self.slots[intent.index];
        slot.exchange.ack_request_id = intent.request_id;
        slot.exchange.ack_result = 0;
        slot.exchange.mode = intent.mode;
        slot.ack_pending = true;
    }
    pub fn synchronize(
        &mut self,
        hosted: &mut State<'_>,
        manager: &Manager<'_>,
        host: &mut Host<'_>,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        dirty: &mut Dirty,
        width: i32,
        height: i32,
        tick: u64,
        hz: u32,
    ) -> bool {
        let interval = u64::from(hz).div_ceil(20).max(1);
        let timeout = u64::from(hz).div_ceil(4).max(1);
        if self.last_tick.is_some_and(|last| tick < last) {
            for slot in &mut self.slots {
                slot.next_tick = 0
            }
        }
        self.last_tick = Some(tick);
        let mut changed = false;
        // Poll/reclaim a pending native loan even when the current host gate
        // is unavailable; such a reply cannot mutate any window.
        match host.mode_poll() {
            Ok(Some(done))
                if hosted.mirror.gate.available() && !hosted.terminal_mode && host.claimed() =>
            {
                let i = done.request.identity.window_id as usize;
                if i >= 4 {
                    let _ = host.close();
                    self.pending = host.mode_busy();
                    return false;
                }
                let owner = hosted.model.clients.slots()[i].handle();
                let win = &hosted.model.windows.windows()[i];
                match self.finish(
                    done.request,
                    done.reply,
                    owner,
                    win.kind() == Kind::App && win.close_requested().is_none(),
                ) {
                    Ok(Some(intent)) => {
                        let (output, work) = layout::output(
                            manager,
                            hosted.model.windows.windows()[i].geometry(),
                            width,
                            height,
                        );
                        let change =
                            hosted
                                .model
                                .windows
                                .set_fullscreen(i, intent.mode == 1, output, work);
                        changed |= change.dirty != 0;
                        hosted.changes(&change, session, app, tick, dirty);
                        self.applied(intent);
                    }
                    Ok(None) => {}
                    Err(()) => {
                        let _ = host.close();
                    }
                }
            }
            Ok(_) | Err(_) => {}
        }
        self.pending = host.mode_busy();
        if self.pending
            || !host.claimed()
            || !hosted.mirror.gate.available()
            || hosted.terminal_mode
        {
            return changed;
        }
        for offset in 0..4 {
            let i = (self.cursor + offset) % 4;
            let owner = hosted.model.clients.slots()[i].handle();
            let win = &hosted.model.windows.windows()[i];
            // A closing owner may still owe an already-applied exact ACK.
            let live = win.kind() == Kind::App
                && (win.close_requested().is_none() || self.slots[i].ack_pending);
            let Some(request) = self.request(
                i,
                owner,
                live,
                u32::from(win.fullscreen()),
                host.owner(),
                tick,
            ) else {
                continue;
            };
            match host.mode_start(request, timeout) {
                Ok(true) => self.sent(i, request, tick, interval),
                Ok(false) => {}
                Err(_) => {
                    host.invalidate();
                }
            }
            break;
        }
        self.pending = host.mode_busy();
        changed
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn handle(id: u32) -> a::ProgramProcessHandle {
        a::ProgramProcessHandle {
            instance_id: id,
            generation: 100 + id as u64,
            ..Default::default()
        }
    }
    fn reply(request: a::WindowModeExchange, serial: u64) -> a::WindowModeReply {
        a::WindowModeReply {
            identity: a::WindowModeIdentity {
                service: handle(7),
                serial,
                ..request.identity
            },
            mode: request.mode,
            ..Default::default()
        }
    }
    #[test]
    fn d088_mode_rechecks_bound_actual_unchanged_work_and_keep_urgent_publications() {
        let mut modes = Modes::default();
        let mut calls = 0;
        for tick in 0..32 {
            for i in 0..4 {
                if let Some(r) = modes.request(i, handle(i as u32 + 1), true, 0, handle(88), tick) {
                    calls += 1;
                    modes.sent(i, r, tick, 50);
                    assert!(
                        modes
                            .finish(r, reply(r, i as u64 + 1), handle(i as u32 + 1), true)
                            .unwrap()
                            .is_none()
                    );
                }
            }
        }
        assert_eq!(calls, 4);
        assert!(
            modes
                .request(0, handle(1), true, 0, handle(88), 49)
                .is_none()
        );
        assert!(
            modes
                .request(0, handle(1), true, 0, handle(88), 50)
                .is_some()
        );
        let changed = modes
            .request(0, handle(1), true, 1, handle(88), 32)
            .unwrap();
        assert_eq!(changed.mode, 1);
        let removal = modes
            .request(0, handle(9), true, 0, handle(88), 33)
            .unwrap();
        assert_eq!(removal.flags, 1);
        assert!(
            modes
                .finish(removal, reply(removal, 1), handle(9), true)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            modes
                .request(0, handle(9), true, 0, handle(88), 34)
                .unwrap()
                .identity
                .serial,
            0
        );
        // Refused/unavailable first publication is bounded too; serial0 is
        // not an excuse to submit another empty query on every refresh.
        let mut refused = Modes::default();
        let first = refused
            .request(0, handle(1), true, 0, handle(88), 0)
            .unwrap();
        refused.sent(0, first, 0, 50);
        assert!(
            refused
                .finish(
                    first,
                    a::WindowModeReply {
                        result: -3,
                        ..Default::default()
                    },
                    handle(1),
                    true
                )
                .unwrap()
                .is_none()
        );
        assert!(
            refused
                .request(0, handle(1), true, 0, handle(88), 1)
                .is_none()
        );
        assert!(
            refused
                .request(0, handle(1), true, 0, handle(88), 50)
                .is_some()
        );
        std::println!(
            "{{\"schema\":\"r4os.desktop-work\",\"case\":\"mode-schedule-after88\",\"iterations\":32,\"windows\":4,\"eligible_queries\":{}}}",
            calls
        );
    }
    #[test]
    fn d088_mode_ack_keeps_applied_geometry_late_identity_and_nonce() {
        let mut modes = Modes::default();
        let r = modes.request(0, handle(1), true, 0, handle(88), 1).unwrap();
        modes.sent(0, r, 1, 50);
        let mut answer = reply(r, 17);
        answer.phase = 1;
        answer.request_id = 41;
        answer.requested_mode = 1;
        let intent = modes.finish(r, answer, handle(1), true).unwrap().unwrap();
        modes.applied(intent);
        let ack = modes.request(0, handle(1), true, 0, handle(88), 2).unwrap();
        assert_eq!((ack.ack_request_id, ack.mode), (41, 1));
        assert!(
            modes
                .finish(ack, answer, handle(1), true)
                .unwrap()
                .is_none()
        );
        let mut accepted = answer;
        accepted.phase = 2;
        assert!(
            modes
                .finish(ack, accepted, handle(1), true)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            modes
                .request(0, handle(1), true, 0, handle(88), 3)
                .unwrap()
                .mode,
            0
        );
        let mut late = Modes::default();
        let r = late.request(0, handle(1), true, 0, handle(88), 1).unwrap();
        assert!(
            late.finish(r, reply(r, 19), handle(9), true)
                .unwrap()
                .is_none()
        );
        let removal = late.request(0, handle(9), true, 0, handle(88), 2).unwrap();
        assert_eq!((removal.flags, removal.identity.serial), (1, 19));
        let mut closing = Modes::default();
        let r = closing
            .request(0, handle(1), true, 0, handle(88), 1)
            .unwrap();
        assert!(
            closing
                .finish(r, answer, handle(1), false)
                .unwrap()
                .is_none()
        );
        let denied = closing
            .request(0, handle(1), true, 0, handle(88), 2)
            .unwrap();
        assert_eq!((denied.ack_request_id, denied.ack_result), (41, -3));
    }
}
