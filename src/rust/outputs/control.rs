//! Desktop confirmation and saved-choice lifetime. WINSVC carries copied
//! requests; the common mode owner retains native transaction receipts.
use super::{
    catalog::{Entry, same_target},
    color_policy as color,
    manager::Manager,
    modes::{self, Modes},
    preferences, topology_wire as wire,
};
use crate::{
    render::output_geometry::topology::{self as t, Key, Layout, Output},
    runtime::{
        library::{Context, required, scalar},
        service::Host,
        wire::Wire,
    },
    shell::persistence::Store,
};
use core::mem::MaybeUninit;
use r4os_library_helpers::{Error, raw as c};
use r4os_sdk::{App, abi as a};
const OPERATION_NS: u64 = 30_000_000_000;
const CONFIRMATION_NS: u64 = 15_000_000_000;
type Result<T> = core::result::Result<T, Error>;
#[path = "control_progress.rs"]
mod progress;
#[path = "control_restore.rs"]
mod restore;
pub struct Owner {
    pub state: a::DisplayControlStatus,
    before: Option<Layout>,
    desired: Option<Layout>,
    mode: Modes,
    mode_key: Option<Key>,
    mode_started: bool,
    configured: bool,
    confirming: bool,
    restoring: bool,
    restoration: modes::Restoration,
    color_restoration: color::Restoration,
    color_recovery: color::Restoration,
    mode_color: Option<a::GfxColorSignal>,
    color_choice: Option<color::Choice>,
    next_transaction: u64,
    next_sync_ns: u64,
    frame_baseline: [u64; t::CAPACITY * 2],
    frame_targets: [a::GfxOutputTarget; t::CAPACITY * 2],
}
fn copy_layout(layout: &Layout) -> Layout {
    if layout.outputs().is_empty() {
        Layout::default()
    } else {
        Layout::new(layout.outputs(), layout.revision()).expect("validated immutable layout")
    }
}
fn pref<T>(value: core::result::Result<T, preferences::Error>) -> Result<T> {
    value.map_err(|_| Error::Native("Preferences"))
}
fn encoded(layout: &Layout) -> Result<a::DisplayLayout> {
    let native = wire::layout(layout)?;
    let mut out = a::DisplayLayout::default();
    required(unsafe {
        c::R4GfxControl_encode(0, &native, (&mut out as *mut a::DisplayLayout).cast())
    })?;
    Ok(out)
}
fn decoded(value: &a::DisplayLayout) -> Result<Layout> {
    let mut out = MaybeUninit::uninit();
    required(unsafe {
        c::R4GfxControl_decode(
            0,
            (value as *const a::DisplayLayout).cast(),
            out.as_mut_ptr(),
        )
    })?;
    let native = unsafe { out.assume_init() };
    let mut normalized = MaybeUninit::uninit();
    required(unsafe { c::R4GfxControl_normalized(0, &native, normalized.as_mut_ptr()) })?;
    wire::read_layout(unsafe { normalized.assume_init_ref() })
}
fn matches(actual: &Layout, desired: &Layout) -> Result<bool> {
    let actual = wire::layout(actual)?;
    let desired = wire::layout(desired)?;
    Ok(scalar(unsafe { c::R4GfxControl_matches(0, &actual, &desired) })? != 0)
}
fn persistent(layout: &Layout) -> Result<bool> {
    let layout = wire::layout(layout)?;
    Ok(scalar(unsafe { c::R4GfxControl_persistent(0, &layout) })? != 0)
}
fn find(layout: &Layout, key: Key) -> Option<usize> {
    layout.outputs().iter().position(|v| v.key == key)
}
impl Owner {
    pub fn new() -> Result<Self> {
        Ok(Self {
            state: a::DisplayControlStatus {
                revision: 1,
                ..Default::default()
            },
            before: None,
            desired: None,
            mode: Modes::new()?,
            mode_key: None,
            mode_started: false,
            configured: false,
            confirming: false,
            restoring: false,
            restoration: modes::Restoration::new()?,
            color_restoration: color::Restoration::new()?,
            color_recovery: color::Restoration::new()?,
            mode_color: None,
            color_choice: None,
            next_transaction: 1,
            next_sync_ns: 0,
            frame_baseline: [0; t::CAPACITY * 2],
            frame_targets: [a::GfxOutputTarget::default(); t::CAPACITY * 2],
        })
    }
    pub fn busy(&self) -> bool {
        matches!(self.state.phase, 1..=3)
    }
    pub fn step(
        &mut self,
        app: &App<'_>,
        context: &Context<'_, '_>,
        manager: &mut Manager<'_>,
        host: &mut Host<'_>,
        store: &Store<'_, '_>,
        now: Option<u64>,
    ) -> Result<bool> {
        let Some(now) = now else {
            if self.busy() {
                self.rollback(context, manager, -3)?;
            }
            return Ok(false);
        };
        let was_busy = self.busy();
        if !was_busy && self.mode.busy()? {
            let _ = self.mode.poll(context)?;
        }
        if was_busy {
            if !host.claimed() {
                self.rollback(context, manager, -3)?;
            } else if self.state.deadline_ns != 0 && now >= self.state.deadline_ns {
                self.rollback(context, manager, if self.state.phase == 2 { 0 } else { -3 })?;
            }
            self.progress(app, context, manager, store, now)?;
        }
        let layout = encoded(&manager.layout)?;
        let changed = !layout.same(&self.state.layout);
        if changed {
            self.state.layout = layout;
            self.state.revision = self.state.revision.saturating_add(1);
        }
        self.state.desktop_epoch = host.owner().generation;
        self.state.flags =
            u32::from(manager.active()) | (u32::from(persistent(&manager.layout)?) << 1);
        if !host.claimed() || (!changed && now < self.next_sync_ns) {
            return Ok(changed);
        }
        self.next_sync_ns = now.saturating_add(if self.busy() {
            100_000_000
        } else {
            500_000_000
        });
        let request = a::DisplayColorExchange {
            base: a::DisplayControlExchange {
                desktop_owner: host.owner(),
                status: self.state,
                ..Default::default()
            },
            ..Default::default()
        };
        let reply = match host.color(&request) {
            Ok(reply) => reply,
            Err(_) => {
                if self.busy() {
                    self.rollback(context, manager, -3)?;
                }
                return Ok(changed);
            }
        };
        if reply.base.flags & 1 != 0 && was_busy {
            self.rollback(context, manager, -4)?;
        }
        if reply.base.request.action != 0 {
            self.handle(
                app,
                context,
                manager,
                host.owner(),
                store,
                &reply.base.request,
                reply.color,
                now,
            )?;
        } else if !self.busy() && !self.mode.busy()? && manager.active() && !manager.reconcile {
            if !self.restore_color(app, context, manager, host.owner(), now)? {
                self.restore(app, context, manager, host.owner(), now)?;
            }
        }
        Ok(changed)
    }
    fn reset_selection(&mut self, context: &Context<'_, '_>) -> Result<bool> {
        if !self.mode.reset(context)? {
            return Ok(false);
        }
        self.mode_key = None;
        self.mode_color = None;
        self.color_choice = None;
        self.mode_started = false;
        self.configured = false;
        self.confirming = false;
        Ok(true)
    }
    fn handle(
        &mut self,
        app: &App<'_>,
        context: &Context<'_, '_>,
        manager: &mut Manager<'_>,
        owner: a::ProgramProcessHandle,
        store: &Store<'_, '_>,
        request: &a::DisplayControlRequest,
        selection: a::DisplayColorSelection,
        now: u64,
    ) -> Result<()> {
        if request.request_id == self.state.request_id && request.owner.same(&self.state.owner) {
            return Ok(());
        }
        if self.busy() && !request.owner.same(&self.state.owner) {
            return Ok(());
        }
        self.state.owner = request.owner;
        self.state.request_id = request.request_id;
        if request.desktop_epoch != self.state.desktop_epoch
            || request.base_revision != self.state.revision
        {
            self.state.result = -4;
            return Ok(());
        }
        match request.action {
            1 | 4 => {
                if self.busy() || self.next_transaction == u64::MAX {
                    self.state.result = -2;
                    return Ok(());
                }
                let desired = match decoded(&request.layout) {
                    Ok(v) => v,
                    Err(_) => {
                        self.reject(-1);
                        return Ok(());
                    }
                };
                if desired.revision() != manager.revision
                    || desired.outputs().len() != manager.layout.outputs().len()
                {
                    self.reject(-4);
                    return Ok(());
                }
                if !self.reset_selection(context)? {
                    self.reject(-2);
                    return Ok(());
                }
                if request.action == 4 {
                    if !matches(&manager.layout, &desired)?
                        || selection.version != 1
                        || selection.size as usize
                            != core::mem::size_of::<a::DisplayColorSelection>()
                    {
                        self.reject(-4);
                        return Ok(());
                    }
                    let entry = manager
                        .snapshot
                        .as_ref()
                        .and_then(|s| {
                            s.entries()
                                .iter()
                                .find(|e| e.info.identity.same(&selection.output))
                        })
                        .copied();
                    let Some(entry) = entry else {
                        self.reject(-4);
                        return Ok(());
                    };
                    let Some(index) = find(&manager.layout, entry.key) else {
                        self.reject(-4);
                        return Ok(());
                    };
                    if !self.select_mode(context, manager, manager.layout.outputs()[index])?
                        || !self.select_color(app, context, manager, entry, selection.signal)?
                    {
                        self.reject(-7);
                        return Ok(());
                    }
                    let mut choice = pref(manager.preferences.color(entry.key))?
                        .unwrap_or(color::choice(entry.key)?);
                    if color::set_signal(&mut choice, selection.signal).is_err() {
                        self.reject(-7);
                        return Ok(());
                    }
                    self.color_restoration
                        .record(&choice, entry.info.identity)?;
                    self.color_choice = Some(choice);
                } else if !selection.same(&a::DisplayColorSelection::default()) {
                    self.reject(-1);
                    return Ok(());
                }
                for choice in desired.outputs() {
                    let Some(index) = find(&manager.layout, choice.key) else {
                        self.reject(-4);
                        return Ok(());
                    };
                    if !modes::same_timing(manager.layout.outputs()[index], *choice)?
                        && (self.mode_key.is_some()
                            || !self.select_mode(context, manager, *choice)?)
                    {
                        self.reject(-7);
                        return Ok(());
                    }
                }
                if request.action == 1 && !self.preserve_color(app, context, manager)? {
                    self.reject(-7);
                    return Ok(());
                }
                if let Some(snapshot) = &manager.snapshot {
                    for entry in snapshot.entries() {
                        if let Some(saved) = pref(manager.preferences.layout(entry.key))? {
                            self.restoration.record(saved, entry.info.identity)?;
                        }
                    }
                }
                self.start(manager, desired, now, false);
            }
            2 => {
                if request.transaction_id != self.state.transaction_id
                    || self.state.phase != 2
                    || now >= self.state.deadline_ns
                {
                    self.state.result = -4;
                    return Ok(());
                }
                if self.mode_started {
                    if !self.mode.resolve(context, true)? {
                        self.state.result = self.mode.error();
                        return Ok(());
                    }
                    self.confirming = true;
                    self.state.phase = 1;
                    self.state.deadline_ns = now.saturating_add(OPERATION_NS);
                } else {
                    self.keep(manager, store)?;
                }
            }
            3 => {
                if request.transaction_id != self.state.transaction_id {
                    self.state.result = -4;
                    return Ok(());
                }
                self.rollback(context, manager, 0)?;
            }
            _ => self.state.result = -1,
        }
        let _ = owner;
        Ok(())
    }
    fn start(&mut self, manager: &mut Manager<'_>, desired: Layout, now: u64, restoring: bool) {
        self.before = Some(copy_layout(&manager.layout));
        self.desired = Some(desired);
        self.restoring = restoring;
        self.mode_started = false;
        self.configured = false;
        self.confirming = false;
        self.state.transaction_id = self.next_transaction;
        self.next_transaction += 1;
        self.state.phase = 1;
        self.state.result = 0;
        self.state.deadline_ns = now.saturating_add(OPERATION_NS);
        for (i, slot) in manager.slots.iter().enumerate() {
            self.frame_targets[i] = slot.target;
            self.frame_baseline[i] = slot.reported;
        }
        if self.mode_key.is_none() {
            self.configure(manager);
        }
    }
    fn select_mode(
        &mut self,
        context: &Context<'_, '_>,
        manager: &Manager<'_>,
        choice: Output,
    ) -> Result<bool> {
        let Some(entry) = manager
            .snapshot
            .as_ref()
            .and_then(|s| s.entries().iter().find(|e| e.key == choice.key))
        else {
            return Ok(false);
        };
        let selected = self.mode.select(context, entry.info.identity, choice)?;
        if selected {
            self.mode_key = Some(choice.key);
        }
        Ok(selected)
    }
    fn configure(&mut self, manager: &mut Manager<'_>) {
        manager.configure(self.desired.as_ref().map(copy_layout));
        self.configured = true;
        for slot in &mut manager.slots {
            slot.paused = false;
        }
    }
    fn select_color(
        &mut self,
        app: &App<'_>,
        context: &Context<'_, '_>,
        manager: &mut Manager<'_>,
        entry: Entry,
        signal: a::GfxColorSignal,
    ) -> Result<bool> {
        if color::requested(signal).is_err() {
            return Ok(false);
        }
        if signal.same(&color::sdr()?)
            && (entry.color.is_none() || color::canonical(entry.color.unwrap())?)
        {
            self.mode_color = None;
            return Ok(true);
        }
        let cpu_color = color::profile_signal(signal)?
            && entry.presentation.flags & a::display_presentation_info_system_source != 0;
        if (!cpu_color && !manager.color_ready(app, entry)) || self.mode.count() == 0 {
            return Ok(false);
        }
        if color::validate(context, entry, self.mode.selected()?, signal).is_err()
            || r4gfx::ColorV1Client::init(app).is_err()
        {
            return Ok(false);
        }
        self.mode_color = Some(signal);
        Ok(true)
    }
    fn preserve_color(
        &mut self,
        app: &App<'_>,
        context: &Context<'_, '_>,
        manager: &mut Manager<'_>,
    ) -> Result<bool> {
        let Some(key) = self.mode_key else {
            return Ok(true);
        };
        let Some(entry) = manager
            .snapshot
            .as_ref()
            .and_then(|s| s.entries().iter().find(|e| e.key == key))
            .copied()
        else {
            return Ok(false);
        };
        let Some(state) = entry.color else {
            return Ok(true);
        };
        if color::canonical(state)? {
            return Ok(true);
        }
        let mut signal = match color::published(state) {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        if let Some(choice) = pref(manager.preferences.color(key))? {
            let saved = color::signal(&choice)?;
            if color::matches(state, saved)? {
                signal = saved;
            }
        }
        self.select_color(app, context, manager, entry, signal)
    }
    fn rollback(
        &mut self,
        context: &Context<'_, '_>,
        manager: &mut Manager<'_>,
        result: i32,
    ) -> Result<()> {
        if !self.busy() || self.state.phase == 3 {
            return Ok(());
        }
        if self.mode_started && !self.confirming {
            let _ = self.mode.close(context)?;
        }
        manager.configure(self.before.as_ref().map(copy_layout));
        for slot in &mut manager.slots {
            slot.paused = false;
        }
        self.state.phase = 3;
        self.state.result = result;
        self.state.deadline_ns = 0;
        self.confirming = false;
        Ok(())
    }
    fn reject(&mut self, result: i32) {
        self.state.phase = 6;
        self.state.result = result;
        self.state.deadline_ns = 0;
    }
    pub fn close(&mut self, context: &Context<'_, '_>, manager: &mut Manager<'_>) -> Result<bool> {
        self.rollback(context, manager, -3)?;
        self.mode.close(context)
    }
}
