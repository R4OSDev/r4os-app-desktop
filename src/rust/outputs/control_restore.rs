//! At most one automatic attempt for the current physical receiver and saved
//! choice. Mode rollback alone cannot restart restoration indefinitely.
use super::*;
impl Owner {
    pub(super) fn restore(
        &mut self,
        app: &App<'_>,
        context: &Context<'_, '_>,
        manager: &mut Manager<'_>,
        owner: a::ProgramProcessHandle,
        now: u64,
    ) -> Result<()> {
        if self.next_transaction == u64::MAX || !self.mode.cleanup(context)? {
            return Ok(());
        }
        let entries = manager.snapshot.as_ref().map(|s| s.entries());
        let mut candidates = [Entry::default(); t::CAPACITY];
        let count = entries.map_or(0, |v| {
            candidates[..v.len()].copy_from_slice(v);
            v.len()
        });
        for entry in &candidates[..count] {
            let Some(saved) = pref(manager.preferences.layout(entry.key))? else {
                continue;
            };
            let Some(index) = find(&manager.layout, entry.key) else {
                continue;
            };
            if !saved.enabled
                || modes::same_timing(manager.layout.outputs()[index], saved)?
                || self.restoration.seen(saved, entry.info.identity)?
                || entry.info.limits.flags & a::gfx_output_limit_modeset == 0
                || entry.info.flags & a::gfx_output_flag_fixed_geometry != 0
                || entry.info.mode_count == 0
            {
                continue;
            }
            if !self.reset_selection(context)? {
                return Ok(());
            }
            if !self.select_mode(context, manager, saved)? {
                if self.mode.error() == 0 && self.mode.count() != 0 {
                    self.restoration.record(saved, entry.info.identity)?;
                }
                continue;
            }
            if !self.preserve_color(app, context, manager)? {
                self.restoration.record(saved, entry.info.identity)?;
                continue;
            }
            let mut values = [Output::default(); t::CAPACITY];
            let count = manager.layout.outputs().len();
            values[..count].copy_from_slice(manager.layout.outputs());
            values[index].view.pixel_w = saved.view.pixel_w;
            values[index].view.pixel_h = saved.view.pixel_h;
            values[index].refresh_millihz = saved.refresh_millihz;
            let desired = match Layout::new(&values[..count], manager.layout.revision()) {
                Ok(v) => v,
                Err(_) => {
                    let mut right = 0i32;
                    for value in &mut values[..count] {
                        value.clone_group = 0;
                        value.view.origin = t::Point { x: right, y: 0 };
                        let Ok(bounds) = value.view.logical() else {
                            return Ok(());
                        };
                        let Ok(next) = i32::try_from(bounds.right()) else {
                            return Ok(());
                        };
                        right = next;
                    }
                    let Some(primary) = manager.layout.primary() else {
                        return Ok(());
                    };
                    match wire::normalize(&mut values[..count], primary, manager.layout.revision())
                    {
                        Ok(v) => v.layout,
                        Err(_) => return Ok(()),
                    }
                }
            };
            self.restoration.record(saved, entry.info.identity)?;
            self.state.owner = owner;
            self.state.request_id = 0;
            self.start(manager, desired, now, true);
            return Ok(());
        }
        Ok(())
    }
    pub(super) fn restore_color(
        &mut self,
        app: &App<'_>,
        context: &Context<'_, '_>,
        manager: &mut Manager<'_>,
        owner: a::ProgramProcessHandle,
        now: u64,
    ) -> Result<bool> {
        if self.next_transaction == u64::MAX || !self.mode.cleanup(context)? {
            return Ok(false);
        }
        let mut candidates = [Entry::default(); t::CAPACITY];
        let count = manager.snapshot.as_ref().map_or(0, |s| {
            candidates[..s.entries().len()].copy_from_slice(s.entries());
            s.entries().len()
        });
        for entry in candidates[..count].iter().copied() {
            let Some(state) = entry.color else {
                continue;
            };
            let saved = pref(manager.preferences.color(entry.key))?;
            let recovery = manager.software_only(entry.target) && !entry.profile_supported();
            let fallback = color::choice(entry.key)?;
            if recovery && self.color_recovery.seen(&fallback, entry.info.identity)? {
                continue;
            }
            if !recovery {
                let Some(choice) = saved.as_ref() else {
                    continue;
                };
                if color::matches(state, color::signal(choice)?)?
                    || self.color_restoration.seen(choice, entry.info.identity)?
                    || manager.software_only(entry.target)
                {
                    continue;
                }
                if !color::signal(choice)?.same(&color::sdr()?) && !manager.color_ready(app, entry)
                {
                    continue;
                }
            }
            let Some(index) = find(&manager.layout, entry.key) else {
                continue;
            };
            if !manager.layout.outputs()[index].enabled {
                continue;
            }
            if !self.reset_selection(context)? {
                return Ok(false);
            }
            if !self.select_mode(context, manager, manager.layout.outputs()[index])? {
                continue;
            }
            let signal = if recovery {
                color::sdr()?
            } else {
                color::signal(saved.as_ref().unwrap())?
            };
            if !self.select_color(app, context, manager, entry, signal)? {
                if let Some(saved) = saved.as_ref() {
                    self.color_restoration.record(saved, entry.info.identity)?;
                }
                continue;
            }
            if let Some(saved) = saved.as_ref() {
                self.color_restoration.record(saved, entry.info.identity)?;
            }
            if recovery {
                self.color_recovery.record(&fallback, entry.info.identity)?;
            }
            self.state.owner = owner;
            self.state.request_id = 0;
            let desired = copy_layout(&manager.layout);
            self.start(manager, desired, now, true);
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn restore_arrangement(&mut self, manager: &mut Manager<'_>) -> Result<()> {
        let mut values = [Output::default(); t::CAPACITY];
        let count = manager.layout.outputs().len();
        values[..count].copy_from_slice(manager.layout.outputs());
        for value in &mut values[..count] {
            let Some(saved) = pref(manager.preferences.layout(value.key))? else {
                return Ok(());
            };
            if !modes::same_timing(*value, saved)? {
                return Ok(());
            }
            value.view.origin = saved.view.origin;
            value.view.scale = saved.view.scale;
            value.view.rotation = saved.view.rotation;
            value.clone_group = saved.clone_group;
            value.enabled = saved.enabled;
            value.primary = saved.primary;
        }
        let Ok(valid) = Layout::new(&values[..count], manager.layout.revision()) else {
            return Ok(());
        };
        let Some(selected) = valid.primary() else {
            return Ok(());
        };
        if let Ok(normalized) = wire::normalize(&mut values[..count], selected, valid.revision()) {
            manager.configure(Some(normalized.layout));
        }
        Ok(())
    }
}
