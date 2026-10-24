//! Progress only follows actual native receipts and fresh reported frames.
use super::*;
impl Owner {
    pub(super) fn progress(
        &mut self,
        app: &App<'_>,
        context: &Context<'_, '_>,
        manager: &mut Manager<'_>,
        store: &Store<'_, '_>,
        now: u64,
    ) -> Result<()> {
        if !self.busy() {
            return Ok(());
        }
        if self.mode_started {
            let _ = self.mode.poll(context)?;
        }
        let status = self.mode.status()?;
        if self.state.phase == 3 {
            if self.mode_started && status.phase == a::gfx_mode_phase_lost {
                self.state.phase = 6;
                self.state.result = -3;
                self.state.deadline_ns = 0;
                self.before = None;
                self.desired = None;
                self.restoring = false;
                return Ok(());
            }
            if (!self.mode_started || !self.mode.busy()?)
                && !manager.reconcile
                && manager.active()
                && settled(manager)
            {
                self.state.phase = 5;
                self.state.deadline_ns = 0;
                self.before = None;
                self.desired = None;
                self.restoring = false;
            }
            return Ok(());
        }
        if self.confirming {
            if status.phase == a::gfx_mode_phase_confirmed {
                self.keep(manager, store)?;
            } else if [a::gfx_mode_phase_reverted, a::gfx_mode_phase_lost].contains(&status.phase) {
                self.rollback(context, manager, -3)?;
            }
            return Ok(());
        }
        if self.mode_started
            && [a::gfx_mode_phase_reverted, a::gfx_mode_phase_lost].contains(&status.phase)
        {
            self.rollback(context, manager, self.mode.error())?;
            return Ok(());
        }
        if !self.mode_started {
            if let Some(key) = self.mode_key {
                let mut waiting = false;
                for slot in &mut manager.slots {
                    let Some(index) = slot.logical_index else {
                        continue;
                    };
                    if manager.layout.outputs()[index].key != key {
                        continue;
                    }
                    slot.paused = true;
                    waiting |= slot.resource.gpu().is_some_and(|o| o.busy())
                        || slot
                            .resource
                            .software()
                            .is_some_and(|o| o.pending() || o.acquired());
                }
                if waiting {
                    return Ok(());
                }
                if !self.mode.apply(app, context, self.mode_color)? {
                    if self.mode.error() != a::gfx_output_error_busy {
                        self.rollback(context, manager, self.mode.error())?;
                    }
                    return Ok(());
                }
                self.mode_started = true;
            }
        }
        let status = self.mode.status()?;
        if self.mode_started && status.phase != a::gfx_mode_phase_awaiting_confirmation {
            return Ok(());
        }
        if !self.configured {
            self.configure(manager);
        }
        let Some(desired) = &self.desired else {
            return Ok(());
        };
        if !matches(&manager.layout, desired)? {
            return Ok(());
        }
        for (i, slot) in manager.slots.iter().enumerate() {
            if slot.logical_index.is_none() {
                continue;
            }
            if slot.failed {
                self.rollback(context, manager, -3)?;
                return Ok(());
            }
            let before = if same_target(self.frame_targets[i], slot.target) {
                self.frame_baseline[i]
            } else {
                0
            };
            if slot.reported <= before {
                return Ok(());
            }
        }
        if self.state.phase == 1 {
            if self.restoring {
                if !self.mode.resolve(context, true)? {
                    self.rollback(context, manager, self.mode.error())?;
                    return Ok(());
                }
                self.confirming = true;
                self.state.deadline_ns = now.saturating_add(OPERATION_NS);
                return Ok(());
            }
            self.state.phase = 2;
            self.state.deadline_ns = now.saturating_add(CONFIRMATION_NS);
            if self.mode_started {
                self.state.deadline_ns =
                    self.state.deadline_ns.min(status.confirmation_deadline_ns);
            }
        }
        Ok(())
    }
    pub(super) fn keep(&mut self, manager: &mut Manager<'_>, store: &Store<'_, '_>) -> Result<()> {
        if !self.restoring {
            if let Some(choice) = self.color_choice.as_ref() {
                if manager.preferences.save_color(store, choice).is_err() {
                    self.finish_save_error();
                    self.color_choice = None;
                    return Ok(());
                }
            }
        }
        if let Some(desired) = &self.desired {
            if !matches(&manager.layout, desired)? {
                self.state.result = -4;
                return Ok(());
            }
            if !self.restoring
                && self.color_choice.is_none()
                && persistent(&manager.layout)?
                && manager
                    .preferences
                    .save_layout(store, &manager.layout)
                    .is_err()
            {
                self.finish_save_error();
                return Ok(());
            }
        }
        if self.restoring {
            self.restore_arrangement(manager)?;
        }
        self.state.phase = 4;
        self.state.result = 0;
        self.state.deadline_ns = 0;
        self.before = None;
        self.desired = None;
        self.confirming = false;
        self.restoring = false;
        self.color_choice = None;
        self.mode_color = None;
        Ok(())
    }
    fn finish_save_error(&mut self) {
        self.state.result = -6;
        self.state.phase = 4;
        self.state.deadline_ns = 0;
        self.before = None;
        self.desired = None;
        self.confirming = false;
    }
}
fn settled(manager: &Manager<'_>) -> bool {
    manager
        .slots
        .iter()
        .filter(|s| s.logical_index.is_some())
        .all(|s| {
            !s.failed
                && !s.dirty()
                && s.reported != 0
                && !s
                    .resource
                    .software()
                    .is_some_and(|o| o.pending() || o.acquired())
                && !s.resource.gpu().is_some_and(|o| o.busy())
        })
}
