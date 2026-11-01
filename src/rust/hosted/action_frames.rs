//! Actual accepted image receipts. These are product view identities, not an
//! all-frame sampler; the coordinator calls only for a pending delegated view.
use super::actions::Actions;
use crate::{
    outputs::{cursor::CleanFrame, selection::Execution},
    render::geometry::Rect,
};
use r4os_sdk::abi as a;
#[derive(Clone, Copy)]
pub struct Native {
    output: usize,
    key: CleanFrame,
    backend: u64,
    reset: u64,
    revision: u64,
    producer: u64,
    invisible: bool,
}
fn covers(rect: Rect, client: Rect) -> bool {
    !client.is_empty()
        && rect.x <= client.x
        && rect.y <= client.y
        && rect.right() >= client.right()
        && rect.bottom() >= client.bottom()
}
impl Actions {
    fn eligible(
        &self,
        index: usize,
        mask: u8,
        bounds: Rect,
        damage: &[Rect],
    ) -> Option<(u64, bool)> {
        let s = &self.slots[index].state;
        if s.view_revision == 0
            || s.composition_serial != 0
            || s.proof_flags & a::desktop_action_proof_unsupported != 0
        {
            return None;
        }
        let client = Rect::new(
            s.info.client_x,
            s.info.client_y,
            s.info.client_w,
            s.info.client_h,
        );
        if !covers(bounds, client) || !damage.iter().any(|r| covers(*r, client)) {
            return None;
        }
        let invisible = s.info.flags & 2 != 0;
        if invisible {
            return (mask & (1 << index) == 0
                && self.slots[index]
                    .receipt
                    .is_some_and(|r| r.result == 0 && r.applied.view_revision == s.view_revision))
            .then_some((0, true));
        }
        let generation = self.scene_generations[index];
        (mask & (1 << index) != 0
            && s.candidate_generation > s.frame_before
            && generation >= s.candidate_generation)
            .then_some((generation, false))
    }
    fn composed(&mut self, index: usize, producer: u64, invisible: bool) {
        let Some(serial) = self.composition.checked_add(1) else {
            self.slots[index].state.proof_flags |= a::desktop_action_proof_unsupported;
            return;
        };
        self.composition = serial;
        let s = &mut self.slots[index].state;
        s.frame_generation = producer;
        s.composition_serial = serial;
        s.proof_flags |= a::desktop_action_proof_composed
            | if invisible {
                a::desktop_action_proof_invisible
            } else {
                a::desktop_action_proof_app_fresh
            };
        self.native[index] = None;
    }
    pub fn software_composed(&mut self, mask: u8, bounds: Rect, damage: &[Rect]) {
        for i in 0..4 {
            if let Some((generation, invisible)) = self.eligible(i, mask, bounds, damage) {
                self.composed(i, generation, invisible);
            }
        }
    }
    pub fn native_captured(
        &mut self,
        output: usize,
        key: CleanFrame,
        execution: Execution,
        mask: u8,
        bounds: Rect,
        damage: &[Rect],
    ) {
        for i in 0..4 {
            if self.native[i].is_some() {
                continue;
            }
            if let Some((producer, invisible)) = self.eligible(i, mask, bounds, damage) {
                self.native[i] = Some(Native {
                    output,
                    key,
                    backend: execution.backend_device_generation,
                    reset: execution.reset_generation,
                    revision: self.slots[i].state.view_revision,
                    producer,
                    invisible,
                });
            }
        }
    }
    pub fn native_completed(
        &mut self,
        output: usize,
        target: a::GfxOutputTarget,
        execution: Execution,
    ) {
        for i in 0..4 {
            let Some(pending) = self.native[i] else {
                continue;
            };
            if pending.output != output {
                continue;
            }
            let actual_target = CleanFrame {
                target,
                ..pending.key
            };
            if actual_target != pending.key
                || pending.revision != self.slots[i].state.view_revision
                || pending.key.device_address != execution.device_address
                || pending.key.device_generation != execution.device_handle_generation
                || pending.backend != execution.backend_device_generation
                || pending.reset != execution.reset_generation
            {
                self.native[i] = None;
                continue;
            }
            if execution.completed_frame == pending.key.frame {
                if execution.result.is_some_and(|r| r == 1 || r == 2) {
                    self.composed(i, pending.producer, pending.invisible);
                } else {
                    self.native[i] = None;
                }
            } else if execution.completed_frame > pending.key.frame {
                // A bounded newest-frame queue may discard a captured frame.
                // Never borrow a later frame's completion for the old image.
                self.native[i] = None;
            }
        }
    }
}
