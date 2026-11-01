//! Explicit delegated WM work on the main task. A transport retry carries the
//! immutable original receipt; only a new admitted operation can change WM.
use super::{
    model,
    process::{self, Programs},
    runtime::State as Hosted,
};
use crate::{
    outputs::manager::Manager,
    render::geometry::{Dirty, Rect},
    runtime::wire::Wire,
    windows::{layout, window::Kind},
};
use r4os_sdk::{App, abi as a, gui::Session};
#[derive(Clone, Copy)]
pub(super) struct Receipt {
    pub request: a::DesktopActionRequest,
    pub applied: a::DesktopActionState,
    pub tick: u64,
    pub result: i32,
    pub acknowledged: bool,
    pub paint_sent: bool,
}
#[derive(Default)]
pub(super) struct Slot {
    pub state: a::DesktopActionState,
    pub receipt: Option<Receipt>,
}
#[derive(Default)]
pub struct Actions {
    pub(super) slots: [Slot; 4],
    pub(super) revision: u64,
    pub(super) composition: u64,
    pub(super) scene_generations: [u64; 4],
    route: Option<(u64, u8)>,
    pub(super) native: [Option<super::action_frames::Native>; 4],
}
fn generation(hosted: &Hosted<'_>, index: usize) -> u64 {
    let client = &hosted.model.clients.slots()[index];
    let frame = client.frames().view();
    if frame.valid && process::same(frame.info.owner, client.handle()) {
        frame.info.committed_generation
    } else {
        0
    }
}
impl Actions {
    pub(super) fn sync(
        &mut self,
        index: usize,
        identity: a::WindowModeIdentity,
        info: a::GuiWindowInfo,
        frame: u64,
        force: bool,
    ) {
        if index >= 4 || !process::valid(identity.owner) {
            return;
        }
        let same_window = self.slots[index].state.window.same(&identity);
        if !same_window {
            self.slots[index].receipt = None;
        }
        if !force && same_window && self.slots[index].state.info.same(&info) {
            return;
        }
        let Some(next) = self.revision.checked_add(1) else {
            // Publish explicit unavailable current proof. An invalid record
            // would leave WINSVC retaining the old composed geometry instead.
            self.slots[index].state = a::DesktopActionState {
                window: identity,
                view_revision: self.revision,
                input_revision: self.revision,
                frame_before: frame,
                info,
                proof_flags: a::desktop_action_proof_view | a::desktop_action_proof_unsupported,
                ..Default::default()
            };
            self.native[index] = None;
            return;
        };
        self.revision = next;
        self.slots[index].state = a::DesktopActionState {
            window: identity,
            view_revision: next,
            frame_before: frame,
            input_revision: next,
            info,
            proof_flags: a::desktop_action_proof_view,
            ..Default::default()
        };
        self.native[index] = None;
    }
    pub fn clear(&mut self, index: usize) {
        if index < 4 {
            self.slots[index] = Slot::default();
            self.native[index] = None;
        }
    }
    pub fn envelope(
        &mut self,
        index: usize,
        mode: a::WindowModeExchange,
        hosted: &Hosted<'_>,
    ) -> a::DesktopWindowExchange {
        if mode.flags != 1 {
            self.sync(
                index,
                mode.identity,
                hosted.model.windows.windows()[index].gui_info(index),
                generation(hosted, index),
                false,
            );
        }
        let mut out = a::DesktopWindowExchange {
            mode,
            state: self.slots[index].state,
            ..Default::default()
        };
        if let Some(receipt) = &self.slots[index].receipt {
            if !receipt.acknowledged {
                out.ack = receipt.request;
                out.applied = receipt.applied;
                out.applied_tick = receipt.tick;
                out.ack_result = receipt.result;
                out.flags = 1;
            }
        }
        out
    }
    fn reject(&mut self, index: usize, request: a::DesktopActionRequest, tick: u64, result: i32) {
        let mut state = self.slots[index].state;
        if !state.window.same(&request.token.window) {
            state = a::DesktopActionState {
                window: request.token.window,
                ..Default::default()
            };
        }
        self.slots[index].receipt = Some(Receipt {
            request,
            applied: state,
            tick,
            result,
            acknowledged: false,
            paint_sent: false,
        });
    }
    pub fn receive(
        &mut self,
        index: usize,
        identity: a::WindowModeIdentity,
        reply: a::DesktopActionReply,
        hosted: &mut Hosted<'_>,
        manager: &Manager<'_>,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        programs: &Programs<'_>,
        dirty: &mut Dirty,
        width: i32,
        height: i32,
        fallback_tick: u64,
    ) {
        if index >= 4
            || !process::same(identity.owner, hosted.model.clients.slots()[index].handle())
        {
            return;
        }
        self.sync(
            index,
            identity,
            hosted.model.windows.windows()[index].gui_info(index),
            generation(hosted, index),
            false,
        );
        if let Some(mut receipt) = self.slots[index].receipt {
            if receipt.request.same(&reply.request) {
                if (reply.phase == a::desktop_action_phase_applied
                    || reply.phase == a::desktop_action_phase_rejected)
                    && reply.result == receipt.result
                    && reply.applied_tick == receipt.tick
                    && reply.applied.same(&receipt.applied)
                {
                    receipt.acknowledged = true;
                    if !receipt.paint_sent
                        && receipt.result == 0
                        && [
                            a::desktop_action_kind_resize,
                            a::desktop_action_kind_restore,
                        ]
                        .contains(&receipt.request.kind)
                        && self.slots[index].state.view_revision == receipt.applied.view_revision
                        && self.slots[index].state.window.same(&receipt.applied.window)
                    {
                        // WM state is already canonical at WINSVC before the
                        // producer receives the tagged paint-view request.
                        model::request_action_paint(
                            &mut hosted.model,
                            index,
                            session,
                            fallback_tick,
                        );
                        receipt.paint_sent = true;
                    }
                } else if reply.phase == a::desktop_action_phase_pending
                    || reply.phase == a::desktop_action_phase_unknown
                {
                    receipt.acknowledged = false;
                }
                self.slots[index].receipt = Some(receipt);
            }
        }
        if reply.current.window.same(&self.slots[index].state.window)
            && reply.current.view_revision == self.slots[index].state.view_revision
            && reply.current.candidate_generation > self.slots[index].state.frame_before
            && reply.current.candidate_generation != self.slots[index].state.candidate_generation
        {
            self.slots[index].state.candidate_generation = reply.current.candidate_generation;
            self.slots[index].state.proof_flags |= a::desktop_action_proof_app_ack;
            dirty.invalidate(hosted.model.windows.windows()[index].client_surface().rect);
        }
        if reply.phase != a::desktop_action_phase_pending
            && reply.phase != a::desktop_action_phase_unknown
        {
            return;
        }
        let request = reply.request;
        if let Some(receipt) = self.slots[index].receipt {
            if receipt.request.same(&request) {
                return;
            }
            if !receipt.acknowledged {
                return;
            }
        }
        let now = r4os_sdk::time::Clock::new(app)
            .ticks()
            .unwrap_or(fallback_tick);
        if !request.token.window.same(&identity)
            || request.token.grant == 0
            || request.operation == 0
            || request.token.reserved != 0
            || request.token.window.reserved != 0
            || request.flags != 0
            || request.frame_generation != 0
            || !process::same(
                request.token.window.owner,
                hosted.model.clients.slots()[index].handle(),
            )
        {
            self.reject(index, request, now, a::desktop_action_result_stale);
            return;
        }
        if reply.phase == a::desktop_action_phase_unknown || now >= request.deadline {
            // This exact Desktop has no retained apply for that unresolved
            // operation. It rejects after deadline; it never applies a retry.
            self.reject(index, request, now, a::desktop_action_result_expired);
            return;
        }
        match programs.status(request.token.controller) {
            Ok(Some(info)) if info.state != 2 => (),
            Ok(_) => {
                self.reject(index, request, now, a::desktop_action_result_not_owner);
                return;
            }
            Err(_) => {
                self.reject(index, request, now, a::desktop_action_result_unavailable);
                return;
            }
        }
        let now = match r4os_sdk::time::Clock::new(app).ticks() {
            Ok(v) => v,
            Err(_) => {
                self.reject(index, request, now, a::desktop_action_result_unavailable);
                return;
            }
        };
        if now >= request.deadline {
            self.reject(index, request, now, a::desktop_action_result_expired);
            return;
        }
        let window = &hosted.model.windows.windows()[index];
        if window.kind() != Kind::App || window.close_requested().is_some() {
            self.reject(index, request, now, a::desktop_action_result_target_ended);
            return;
        }
        let right = match request.kind {
            1 => 1,
            2 => 2,
            3 => 4,
            _ => 0,
        };
        if right == 0 || request.token.rights & right == 0 {
            self.reject(index, request, now, a::desktop_action_result_not_owner);
            return;
        }
        if self.revision == u64::MAX {
            self.reject(index, request, now, a::desktop_action_result_quota);
            return;
        }
        let before = generation(hosted, index);
        let (_, work) = layout::output(manager, window.geometry(), width, height);
        let change = match request.kind {
            a::desktop_action_kind_resize => hosted.model.windows.resize_client(
                index,
                request.x,
                request.y,
                request.width,
                request.height,
                work,
            ),
            a::desktop_action_kind_minimize => Some(hosted.model.windows.minimize(index)),
            a::desktop_action_kind_restore => Some(hosted.model.windows.delegated_restore(index)),
            _ => None,
        };
        let Some(mut change) = change else {
            self.reject(index, request, now, a::desktop_action_result_unsupported);
            return;
        };
        // Publish actual geometry/focus/capture consequences now. Tagged
        // resize is emitted only after the server ACKs this original receipt.
        change.resized = 0;
        hosted.changes(&change, session, app, now, dirty);
        self.sync(
            index,
            identity,
            hosted.model.windows.windows()[index].gui_info(index),
            before,
            true,
        );
        dirty.invalidate(hosted.model.windows.windows()[index].frame_surface().rect);
        let applied = self.slots[index].state;
        self.slots[index].receipt = Some(Receipt {
            request,
            applied,
            tick: now,
            result: 0,
            acknowledged: false,
            paint_sent: false,
        });
    }
    /// Refresh only while a delegated view needs actual frame proof. Scene
    /// generations are scalar copies from the same stable owner snapshots.
    pub fn route(&mut self, display_revision: u64, route: u8) {
        let key = (display_revision, route);
        let changed = self.route.is_some_and(|old| old != key);
        self.route = Some(key);
        if changed {
            for i in 0..4 {
                let state = self.slots[i].state;
                if process::valid(state.window.owner) {
                    self.sync(i, state.window, state.info, state.frame_before, true);
                }
            }
        }
    }
    pub fn prepare_scene(&mut self, hosted: &Hosted<'_>) {
        for i in 0..4 {
            let identity = self.slots[i].state.window;
            if !process::valid(identity.owner) {
                continue;
            }
            self.sync(
                i,
                identity,
                hosted.model.windows.windows()[i].gui_info(i),
                generation(hosted, i),
                false,
            );
            self.scene_generations[i] = generation(hosted, i);
        }
    }
    pub fn needs_frame(&self) -> bool {
        self.slots.iter().any(|s| {
            s.state.view_revision != 0
                && s.state.proof_flags & a::desktop_action_proof_unsupported == 0
                && s.state.composition_serial == 0
                && (s.state.candidate_generation > s.state.frame_before
                    || (s.state.info.flags & 2 != 0
                        && s.receipt.is_some_and(|r| {
                            r.result == 0 && r.applied.view_revision == s.state.view_revision
                        })))
        })
    }
    pub fn state(&self, index: usize) -> a::DesktopActionState {
        self.slots[index].state
    }
}
