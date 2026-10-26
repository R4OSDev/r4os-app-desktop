//! Bounded ownership of current and retiring connector generations. Complete
//! discovery precedes withdrawal; failed/mixed catalogs keep old outputs.
use super::{
    catalog::{Entry, Snapshot, same_target},
    cpu,
    intents::Intents,
    native,
    preferences::{self, Preferences},
    topology_wire,
};
use crate::{
    hosted::{window_image::Memory, window_set::Set},
    render::{
        geometry::{Dirty, Rect},
        gpu::{device::Device, receipt::Consumer},
        output_geometry::{
            self as geometry,
            topology::{self as t, Layout, Output, Point, Viewport},
        },
    },
    runtime::library::Context,
    shell::persistence::Store,
};
use alloc::vec::Vec;
use r4os_library_helpers::Error as LibraryError;
use r4os_sdk::{App, abi as a, graphics::Graphics, storage::Error as ApiError};
const SECOND: u64 = 1_000_000_000;
#[derive(Debug)]
pub enum Error {
    Memory,
    Geometry,
    Library(LibraryError),
    Preferences(preferences::Error),
}
impl From<LibraryError> for Error {
    fn from(e: LibraryError) -> Self {
        Self::Library(e)
    }
}
impl From<preferences::Error> for Error {
    fn from(e: preferences::Error) -> Self {
        Self::Preferences(e)
    }
}
type Result<T> = core::result::Result<T, Error>;
/// One renderer owns a slot. A native owner awaiting retirement remains
/// Native; only its real close acknowledgement permits CPU replacement.
pub enum Resource<'a> {
    Empty,
    Native(native::Output<'a>),
    Cpu(cpu::Output),
}
impl<'a> Resource<'a> {
    pub fn gpu(&self) -> Option<&native::Output<'a>> {
        match self {
            Self::Native(owner) => Some(owner),
            _ => None,
        }
    }
    pub fn gpu_mut(&mut self) -> Option<&mut native::Output<'a>> {
        match self {
            Self::Native(owner) => Some(owner),
            _ => None,
        }
    }
    pub fn software(&self) -> Option<&cpu::Output> {
        match self {
            Self::Cpu(owner) => Some(owner),
            _ => None,
        }
    }
    pub fn software_mut(&mut self) -> Option<&mut cpu::Output> {
        match self {
            Self::Cpu(owner) => Some(owner),
            _ => None,
        }
    }
}
pub struct Slot<'a> {
    pub target: a::GfxOutputTarget,
    color_revision: u64,
    pub view: Viewport,
    pub logical_index: Option<usize>,
    pub disabled: bool,
    pub paused: bool,
    pub sleeping: bool,
    pub resource: Resource<'a>,
    pub selection: super::selection::Decision,
    retained: Option<Device>,
    pub damage: Dirty,
    pub failed: bool,
    pub reconfiguring: bool,
    retry_after_ns: u64,
    pub reported: u64,
    discarded_reported: u64,
    pub intents: Intents,
}
impl<'a> Slot<'a> {
    fn new() -> Result<Self> {
        Ok(Self {
            target: a::GfxOutputTarget::default(),
            color_revision: 0,
            view: Viewport::default(),
            logical_index: None,
            disabled: false,
            paused: false,
            sleeping: false,
            resource: Resource::Empty,
            selection: super::selection::Decision::default(),
            retained: None,
            damage: Dirty::default(),
            failed: false,
            reconfiguring: false,
            retry_after_ns: 0,
            reported: 0,
            discarded_reported: 0,
            intents: Intents::new()?,
        })
    }
    pub fn occupied(&self) -> bool {
        self.target.connector_id != 0
    }
    pub fn bounds(&self) -> Option<Rect> {
        geometry::logical(self.view).ok()
    }
    pub fn dirty(&self) -> bool {
        self.logical_index.is_some()
            && !self.failed
            && !self.paused
            && !self.sleeping
            && self.damage.active()
    }
    pub fn invalidate(&mut self) {
        if let Some(r) = self.bounds() {
            self.damage.invalidate(r);
        }
    }
    fn drained(&self) -> bool {
        matches!(self.resource, Resource::Empty) && self.retained.is_none()
    }
    fn close<M: Memory>(
        &mut self,
        app: &App<'_>,
        windows: &mut Set<M, Consumer>,
        graphics: &Graphics<'_, '_>,
        now: u64,
    ) {
        let closed = match &mut self.resource {
            Resource::Empty => false,
            Resource::Native(owner) => owner.close(app, windows, graphics, now),
            Resource::Cpu(owner) => owner.close(app, graphics, now),
        };
        if closed {
            self.resource = Resource::Empty;
        }
        if self
            .retained
            .as_mut()
            .is_some_and(|d| d.close(app).unwrap_or(false))
        {
            self.retained = None;
        }
    }
    pub fn cursor_compatible(&self, info: a::DisplayCursorInfo, width: u32, height: u32) -> bool {
        self.logical_index.is_some()
            && !self.disabled
            && !self.paused
            && !self.sleeping
            && !self.failed
            && !self.reconfiguring
            && self.view.origin == Point::default()
            && self.view.rotation == t::Rotation::Normal
            && self.view.scale == t::SCALE_UNIT
            && self.view.pixel_w == width
            && self.view.pixel_h == height
            && width != 0
            && height != 0
            && info.version == 1
            && info.flags & 15 == 15
            && info.display_generation != 0
            && self.target.connector_id != 0
            && self.target.device_generation != 0
            && info.backend.reset_generation != 0
            && self.target.adapter_id == info.backend.adapter_id
            && self.target.device_generation == info.backend.device_generation
            && self.target.display_generation == info.display_generation
            && self.target.head_id == info.head_id
    }
}
pub struct Manager<'a> {
    pub slots: Vec<Slot<'a>>,
    pub layout: Layout,
    pub desktop_bounds: Rect,
    pub snapshot: Option<Snapshot>,
    pub preferences: Preferences,
    desired: Option<Layout>,
    pub translation: Point,
    pub revision: u64,
    retry_ns: u64,
    pub reconcile: bool,
    software_targets: [a::GfxOutputTarget; t::CAPACITY],
    motion: Option<[u32; 2]>,
    pub cursor: Point,
    closing: bool,
}
impl<'a> Manager<'a> {
    pub fn new() -> Result<Self> {
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(t::CAPACITY * 2)
            .map_err(|_| Error::Memory)?;
        for _ in 0..t::CAPACITY * 2 {
            slots.push(Slot::new()?);
        }
        Ok(Self {
            slots,
            layout: Layout::default(),
            desktop_bounds: Rect::ZERO,
            snapshot: None,
            preferences: Preferences::new()?,
            desired: None,
            translation: Point::default(),
            revision: 0,
            retry_ns: 0,
            reconcile: true,
            software_targets: [a::GfxOutputTarget::default(); t::CAPACITY],
            motion: None,
            cursor: Point::default(),
            closing: false,
        })
    }
    pub fn configure(&mut self, layout: Option<Layout>) {
        self.desired = layout;
        self.reconcile = true;
    }
    pub fn active(&self) -> bool {
        !self.layout.outputs().is_empty()
    }
    /// Managed ownership includes a draining/temporarily empty layout. Do not
    /// open a private native primary on that same known head while it retires.
    pub fn owns_output_route(&self) -> bool {
        self.active()
            || self.slots.iter().any(Slot::occupied)
            || self.snapshot.as_ref().is_some_and(|s| {
                s.entries().iter().any(|e| {
                    e.active() && e.presentation.flags & a::display_presentation_info_native != 0
                })
            })
    }
    pub fn composition_choices(
        &self,
        region: Rect,
    ) -> impl Iterator<Item = super::selection::Choice> + '_ {
        self.slots
            .iter()
            .filter(move |s| {
                s.logical_index.is_some()
                    && s.bounds().is_some_and(|bounds| bounds.intersects(region))
            })
            .map(|s| {
                let active = !s.failed
                    && !s.paused
                    && !s.sleeping
                    && !s.disabled
                    && !s.reconfiguring
                    && !matches!(s.resource, Resource::Empty);
                let owner = match &s.resource {
                    Resource::Native(_) if active => "native-gr",
                    Resource::Cpu(_) if active => "managed-cpu",
                    Resource::Native(_) => "retained-native",
                    Resource::Cpu(_) => "inactive-cpu",
                    Resource::Empty => "unavailable",
                };
                super::selection::Choice {
                    target: Some(s.target.into()),
                    owner,
                    active,
                    decision: s.selection,
                    execution: s.resource.gpu().map(native::Output::execution_observation),
                }
            })
    }
    pub fn software_only(&self, target: a::GfxOutputTarget) -> bool {
        self.software_targets.iter().any(|old| {
            old.connector_id != 0
                && old.adapter_id == target.adapter_id
                && old.connector_id == target.connector_id
                && old.head_id == target.head_id
                && old.device_generation == target.device_generation
                && old.connection_generation == target.connection_generation
        })
    }
    pub fn color_ready(&mut self, app: &App<'_>, entry: Entry) -> bool {
        if self.software_only(entry.target)
            || entry.presentation.flags & a::display_presentation_info_native == 0
        {
            return false;
        }
        if self
            .preferences
            .color(entry.key)
            .ok()
            .flatten()
            .as_ref()
            .is_some_and(|v| preferences::profile(v).ok().flatten().is_some())
        {
            return false;
        }
        self.slots
            .iter_mut()
            .find(|s| !s.failed && !s.reconfiguring && same_target(s.target, entry.target))
            .and_then(|s| s.resource.gpu_mut())
            .is_some_and(|o| o.admission(app, r4gfx::device_gpu_color).admitted())
    }
    pub fn reload_colors(&mut self, store: &Store<'_, '_>) -> Result<()> {
        let next = Preferences::read_colors(store)?;
        if self.preferences.same_colors(&next)? {
            return Ok(());
        }
        for slot in &mut self.slots {
            let Some(entry) = self.snapshot.as_ref().and_then(|v| {
                v.entries()
                    .iter()
                    .find(|e| same_target(e.target, slot.target))
            }) else {
                continue;
            };
            let old = self.preferences.color(entry.key)?;
            let new = Preferences::color_in(&next, entry.key)?;
            if Preferences::same_choice(old.as_ref(), new.as_ref())? {
                continue;
            }
            slot.reconfiguring = true;
            slot.logical_index = None;
        }
        self.preferences.replace_colors(next);
        self.reconcile = true;
        Ok(())
    }
    pub fn content_frame(&mut self, rect: Rect, now: u64) {
        for s in &mut self.slots {
            if s.logical_index.is_some()
                && !s.disabled
                && s.bounds().is_some_and(|b| b.intersects(rect))
            {
                let _ = s.intents.frame(now);
            }
        }
    }
    pub fn sync_policy(
        &mut self,
        context: &Context<'_, '_>,
        now: u64,
        fullscreen: Option<Rect>,
        capture_consumers: bool,
        settings_busy: bool,
    ) -> Result<()> {
        for slot in &mut self.slots {
            if !slot.occupied() || slot.logical_index.is_none() {
                continue;
            }
            let Some(entry) = self.snapshot.as_ref().and_then(|v| {
                v.entries()
                    .iter()
                    .find(|e| same_target(e.target, slot.target))
            }) else {
                continue;
            };
            let refresh = self.preferences.refresh_choice(entry.key)?;
            let brightness = self.preferences.brightness(entry.key)?;
            let full = fullscreen.zip(slot.bounds()).is_some_and(|(r, b)| {
                r.x <= b.x
                    && r.y <= b.y
                    && i64::from(r.x) + i64::from(r.w) >= i64::from(b.x) + i64::from(b.w)
                    && i64::from(r.y) + i64::from(r.h) >= i64::from(b.y) + i64::from(b.h)
            });
            let scene = if full {
                a::gfx_refresh_scene_fullscreen
            } else {
                0
            } | if slot.intents.animated(now)? {
                a::gfx_refresh_scene_animated
            } else {
                0
            } | if capture_consumers {
                a::gfx_refresh_scene_capture
            } else {
                0
            };
            let ready = !slot.disabled
                && !slot.paused
                && !slot.failed
                && !slot.reconfiguring
                && !self.reconcile
                && !settings_busy;
            slot.intents.step(
                context,
                slot.target,
                entry.info.identity,
                &refresh,
                brightness.as_ref(),
                now,
                scene,
                ready,
                slot.sleeping,
            )?;
        }
        Ok(())
    }
    pub fn cursor_target(
        &self,
        info: a::DisplayCursorInfo,
        width: u32,
        height: u32,
    ) -> Option<a::GfxOutputTarget> {
        if self.layout.outputs().len() != 1
            || self.layout.primary() != Some(0)
            || !self.layout.outputs()[0].enabled
        {
            return None;
        }
        self.slots
            .iter()
            .find(|s| {
                s.logical_index == Some(0)
                    && s.resource.gpu().is_some()
                    && s.cursor_compatible(info, width, height)
                    && self.snapshot.as_ref().is_some_and(|v| {
                        v.entries().iter().any(|e| {
                            e.active()
                                && e.info.flags & a::gfx_output_flag_active != 0
                                && same_target(e.target, s.target)
                        })
                    })
            })
            .map(|s| s.target)
    }
    fn same_encoding(&self, slot: &Slot<'_>, entry: &Entry) -> bool {
        self.snapshot
            .as_ref()
            .and_then(|s| {
                s.entries().iter().find(|old| {
                    same_target(old.target, slot.target)
                        && slot.color_revision == old.color.map_or(0, |v| v.revision)
                })
            })
            .is_some_and(|old| old.same_encoding(entry))
    }
    pub(crate) fn fail(&mut self, index: usize, now: u64) {
        let slot = &mut self.slots[index];
        slot.failed = true;
        if slot.resource.gpu().is_some() {
            let target = slot.target;
            let at = self
                .software_targets
                .iter()
                .position(|t| t.adapter_id == target.adapter_id && t.head_id == target.head_id)
                .or_else(|| {
                    self.software_targets
                        .iter()
                        .position(|t| t.connector_id == 0)
                })
                .unwrap_or(0);
            self.software_targets[at] = target;
        }
        if slot.retry_after_ns == 0 {
            slot.retry_after_ns = now.saturating_add(SECOND);
            self.reconcile = true;
        }
    }
    /// # Safety
    /// All provider/loader callback code remains resident until close() really
    /// succeeds, including the native cold preparation tasks owned by slots.
    unsafe fn acquire<M: Memory>(
        &mut self,
        entry: Entry,
        view: Viewport,
        app: &App<'_>,
        graphics: &Graphics<'_, '_>,
        context: &Context<'_, '_>,
        windows: &mut Set<M, Consumer>,
        now: u64,
    ) -> Result<Option<usize>> {
        let index = self
            .slots
            .iter()
            .position(|s| {
                s.logical_index.is_some()
                    && !s.failed
                    && !s.reconfiguring
                    && same_target(s.target, entry.target)
                    && self.same_encoding(s, &entry)
                    && s.view.pixel_w == view.pixel_w
                    && s.view.pixel_h == view.pixel_h
            })
            .or_else(|| self.slots.iter().position(|s| !s.occupied()));
        let Some(index) = index else {
            return Ok(None);
        };
        if self.slots[index].occupied() {
            let slot = &mut self.slots[index];
            if slot.sleeping {
                if !slot.drained() {
                    slot.reconfiguring = true;
                    return Ok(None);
                }
                *slot = Slot::new()?;
            } else {
                if slot.resource.software().is_some_and(|o| o.acquired()) {
                    self.fail(index, now);
                    return Ok(None);
                }
                slot.color_revision = entry.color.map_or(0, |v| v.revision);
                return Ok(Some(index));
            }
        }
        let choice = self.preferences.color(entry.key)?;
        let profile = choice
            .as_ref()
            .map(preferences::profile)
            .transpose()?
            .flatten();
        let profile_enabled = profile.is_some() && entry.profile_supported();
        let software_only = self.software_only(entry.target);
        let slot = &mut self.slots[index];
        slot.target = entry.target;
        slot.view = view;
        slot.color_revision = entry.color.map_or(0, |v| v.revision);
        slot.selection = super::selection::Decision::new(if software_only {
            super::selection::Reason::PriorNativeFailure
        } else if profile_enabled {
            super::selection::Reason::ColorProfile
        } else {
            super::selection::Reason::NotPrepared
        });
        if !software_only && !profile_enabled {
            match unsafe {
                native::Output::open(
                    app,
                    entry.target.adapter_id,
                    Some(entry.target.head_id),
                    entry.color,
                    entry.presentation.format,
                )
            } {
                Ok(owner) => slot.resource = Resource::Native(owner),
                Err(e) => {
                    slot.selection = super::selection::Decision::open(e.error);
                    slot.retained = e.retained;
                }
            }
        }
        let accelerated = slot.resource.gpu_mut().is_some_and(|o| {
            slot.selection = o.admission(app, 0);
            slot.selection.admitted()
        });
        if !accelerated {
            slot.close(app, windows, graphics, now);
            let cpu_admitted = matches!(slot.resource, Resource::Empty)
                && slot.retained.is_none()
                && entry.presentation.format == a::gfx_buffer_format_xrgb8888
                && (entry.color.is_none() || entry.profile_supported())
                && entry.presentation.flags & a::display_presentation_info_system_source != 0;
            if cpu_admitted {
                match unsafe { cpu::Output::open(app, graphics, view, entry.target) } {
                    Ok(owner) => slot.resource = Resource::Cpu(owner),
                    Err(e) => slot.retained = e.retained,
                }
            }
            if slot.resource.software().is_none_or(|o| o.lost()) {
                self.fail(index, now);
                return Ok(None);
            }
            let owner = slot.resource.software_mut().unwrap();
            owner.limited(entry.color.is_some_and(|v| v.range == 2));
            if profile_enabled {
                let (path, intent, flags) = profile.unwrap();
                let end = path.iter().position(|v| *v == 0).unwrap_or(path.len());
                // An unavailable ICC is reported by the caller through its
                // ordinary settings path; keep the original admitted SDR.
                let _ = unsafe { owner.set_profile(app, context, &path[..end], intent, flags) };
            }
        }
        Ok(Some(index))
    }
    /// # Safety
    /// Same admission/lifetime requirement as acquire(). Discovery itself is
    /// transactional; errors leave the previously published catalog/layout.
    pub unsafe fn refresh<M: Memory>(
        &mut self,
        revision: u64,
        app: &App<'_>,
        graphics: &Graphics<'_, '_>,
        context: &Context<'_, '_>,
        windows: &mut Set<M, Consumer>,
        now: u64,
    ) -> Result<bool> {
        self.poll(app, graphics, context, windows, now);
        if self.closing || (revision == self.revision && !self.reconcile && now < self.retry_ns) {
            return Ok(false);
        }
        self.retry_ns = now.saturating_add(SECOND);
        let next = Snapshot::read(context)?;
        let changed = match &self.snapshot {
            Some(old) => !next.same_outputs(old)?,
            None => !next.entries().is_empty() || next.revision() != 0,
        };
        if !changed && !self.reconcile {
            return Ok(false);
        }
        let mut values = [Output::default(); t::CAPACITY];
        let mut owners = [0usize; t::CAPACITY];
        let (mut count, mut x, mut primary) = (0, 0i32, None);
        for entry in next.entries().iter().copied() {
            if entry.info.flags & a::gfx_output_flag_sleeping != 0 {
                if let Some((index, slot)) = self.slots.iter_mut().enumerate().find(|(_, s)| {
                    s.logical_index
                        .is_some_and(|i| i < self.layout.outputs().len())
                        && s.target.adapter_id == entry.info.identity.adapter_id
                        && s.target.connector_id == entry.info.identity.connector_id
                        && s.target.device_generation == entry.info.identity.device_generation
                }) {
                    let old = self.layout.outputs()[slot.logical_index.unwrap()];
                    let Ok(bounds) = old.view.logical() else {
                        continue;
                    };
                    slot.sleeping = true;
                    values[count] = old;
                    owners[count] = index;
                    if old.primary && old.enabled {
                        primary = Some(count);
                    }
                    x = i64::from(x).max(bounds.right()).min(i64::from(i32::MAX)) as i32;
                    count += 1;
                }
                continue;
            }
            if !entry.active()
                || entry.presentation.flags & a::display_presentation_info_native == 0
            {
                continue;
            }
            if self
                .slots
                .iter()
                .any(|s| (s.failed || s.reconfiguring) && same_target(s.target, entry.target))
            {
                continue;
            }
            let mut value = Output {
                key: entry.key,
                view: Viewport {
                    pixel_w: entry.presentation.width,
                    pixel_h: entry.presentation.height,
                    origin: Point { x, y: 0 },
                    ..Default::default()
                },
                refresh_millihz: if entry.presentation.interval_ns == 0 {
                    0
                } else {
                    (1_000_000_000_000 / entry.presentation.interval_ns).min(1_000_000) as u32
                },
                ..Default::default()
            };
            let desired = self
                .desired
                .as_ref()
                .and_then(|l| l.outputs().iter().find(|v| v.key == entry.key))
                .copied();
            let previous = self
                .layout
                .outputs()
                .iter()
                .find(|v| v.key == entry.key)
                .copied();
            let saved = if desired.is_none() && previous.is_none() {
                self.preferences.layout(entry.key)?
            } else {
                None
            };
            let choice = desired.or(previous).or(saved);
            if let Some(c) = choice {
                value.view.origin = c.view.origin;
                value.view.rotation = c.view.rotation;
                value.view.scale = c.view.scale;
                value.clone_group = c.clone_group;
                value.enabled = c.enabled;
            }
            let Ok(bounds) = value.view.logical() else {
                continue;
            };
            let Some(index) =
                (unsafe { self.acquire(entry, value.view, app, graphics, context, windows, now) })?
            else {
                continue;
            };
            if choice.is_some_and(|c| c.primary) && value.enabled {
                primary = Some(count);
            }
            x = i64::from(x).max(bounds.right()).min(i64::from(i32::MAX)) as i32;
            values[count] = value;
            owners[count] = index;
            count += 1;
        }
        let (layout, translation) = if count == 0 {
            (Layout::default(), Point::default())
        } else {
            let selected = primary
                .or_else(|| values[..count].iter().position(|v| v.enabled))
                .unwrap_or_else(|| {
                    values[0].enabled = true;
                    0
                });
            let normalized =
                match topology_wire::normalize(&mut values[..count], selected, next.revision()) {
                    Ok(v) => v,
                    Err(_) => {
                        let mut right = 0i32;
                        for value in &mut values[..count] {
                            value.clone_group = 0;
                            value.view.origin = Point { x: right, y: 0 };
                            right = i32::try_from(
                                value.view.logical().map_err(|_| Error::Geometry)?.right(),
                            )
                            .map_err(|_| Error::Geometry)?;
                        }
                        topology_wire::normalize(&mut values[..count], selected, next.revision())
                            .map_err(|e| {
                                self.reconcile = true;
                                Error::Library(e)
                            })?
                    }
                };
            (normalized.layout, normalized.translation)
        };
        let bounds = if count == 0 {
            Rect::ZERO
        } else {
            geometry::desktop_bounds(&layout).ok_or(Error::Geometry)?
        };
        let output_changed = changed
            || self.layout.outputs() != layout.outputs()
            || self.layout.revision() != layout.revision()
            || self.translation != translation;
        self.translation = translation;
        self.layout = layout;
        self.desktop_bounds = bounds;
        self.revision = next.revision();
        self.snapshot = Some(next);
        self.reconcile = false;
        for slot in &mut self.slots {
            slot.logical_index = None;
        }
        for (i, value) in values[..count].iter().enumerate() {
            let slot = &mut self.slots[owners[i]];
            slot.view = value.view;
            if let Some(owner) = slot.resource.software_mut() {
                if !owner.set_view(value.view) {
                    slot.failed = true;
                }
            }
            slot.logical_index = Some(i);
            slot.disabled = !value.enabled;
            slot.damage = Dirty::default();
            slot.invalidate();
        }
        self.poll(app, graphics, context, windows, now);
        Ok(output_changed)
    }
    fn retire_superseded(&mut self, graphics: &Graphics<'_, '_>, context: &Context<'_, '_>) {
        let old_revision = self.snapshot.as_ref().map_or(0, |s| s.revision());
        let changed = graphics
            .outputs()
            .revision()
            .is_ok_and(|r| r.revision != old_revision)
            || self.slots.iter().any(|s| {
                if s.logical_index.is_none() || s.failed || s.reconfiguring || s.sleeping {
                    return false;
                }
                match graphics
                    .outputs()
                    .target(s.target.adapter_id, s.target.head_id)
                {
                    Ok(t) => {
                        !same_target(t, s.target)
                            || graphics.outputs().presentation_info(&t).is_ok_and(|p| {
                                p.width != s.view.pixel_w
                                    || p.height != s.view.pixel_h
                                    || self.snapshot.as_ref().is_some_and(|old| {
                                        old.entries().iter().any(|e| {
                                            same_target(e.target, s.target)
                                                && (p.format != e.presentation.format
                                                    || p.buffer_count
                                                        != e.presentation.buffer_count)
                                        })
                                    })
                            })
                    }
                    Err(ApiError::Raw(rc)) => [
                        a::gfx_output_error_unsupported,
                        a::err_no_fn,
                        a::err_no_group,
                    ]
                    .contains(&rc),
                    _ => false,
                }
            });
        if !changed {
            return;
        }
        let Ok(next) = Snapshot::read(context) else {
            return;
        };
        for index in 0..self.slots.len() {
            let s = &self.slots[index];
            if s.logical_index.is_none() || s.failed || s.reconfiguring {
                continue;
            }
            let current = next.entries().iter().any(|e| {
                (e.info.flags & a::gfx_output_flag_sleeping != 0
                    && e.info.identity.adapter_id == s.target.adapter_id
                    && e.info.identity.connector_id == s.target.connector_id
                    && e.info.identity.device_generation == s.target.device_generation)
                    || (e.active()
                        && same_target(e.target, s.target)
                        && self.same_encoding(s, e)
                        && e.presentation.width == s.view.pixel_w
                        && e.presentation.height == s.view.pixel_h)
            });
            if !current {
                let s = &mut self.slots[index];
                s.reconfiguring = true;
                s.logical_index = None;
                self.reconcile = true;
            }
        }
    }
    pub fn poll<M: Memory>(
        &mut self,
        app: &App<'_>,
        graphics: &Graphics<'_, '_>,
        context: &Context<'_, '_>,
        windows: &mut Set<M, Consumer>,
        now: u64,
    ) {
        if !self.closing {
            self.retire_superseded(graphics, context);
        }
        for index in 0..self.slots.len() {
            let s = &mut self.slots[index];
            if !s.occupied() {
                continue;
            }
            let identity = a::GfxOutputId {
                adapter_id: s.target.adapter_id,
                connector_id: s.target.connector_id,
                device_generation: s.target.device_generation,
                connection_generation: s.target.connection_generation,
            };
            if let Ok(power) = graphics.outputs().power(&identity) {
                s.sleeping = (a::gfx_power_phase_stopping..=a::gfx_power_phase_waking)
                    .contains(&power.phase);
            }
            if (s.sleeping && s.logical_index.is_some())
                || s.logical_index.is_none()
                || s.failed
                || s.reconfiguring
            {
                let _ = s.intents.release(context, s.target);
                s.close(app, windows, graphics, now);
                if s.logical_index.is_none() && s.drained() && now >= s.retry_after_ns {
                    if let Ok(empty) = Slot::new() {
                        *s = empty;
                        self.reconcile = true;
                    }
                }
                continue;
            }
            let mut failed = false;
            if let Some(o) = s.resource.gpu_mut() {
                match o.poll(app, windows, graphics, now) {
                    native::Progress::Failed => failed = true,
                    native::Progress::Discarded => {
                        if o.completed_status.is_none_or(|v| v.result != 2) {
                            s.invalidate();
                        }
                    }
                    _ => {}
                }
            }
            if let Some(o) = s.resource.software_mut() {
                o.poll(app, now);
                failed |= o.lost();
                let discarded = o.stats().discarded;
                if discarded != s.discarded_reported {
                    s.discarded_reported = discarded;
                    s.invalidate();
                }
            }
            if failed {
                self.fail(index, now);
            }
        }
    }
    pub fn invalidate(&mut self, regions: &[Rect]) {
        for s in &mut self.slots {
            if s.logical_index.is_none() || s.disabled {
                continue;
            }
            if let Some(bounds) = s.bounds() {
                for r in regions {
                    if let Some(r) = crate::render::scene::intersect(bounds, *r) {
                        s.damage.invalidate(r);
                    }
                }
            }
        }
    }
    pub fn invalidate_all(&mut self) {
        for s in &mut self.slots {
            if s.logical_index.is_some() {
                s.invalidate();
            }
        }
    }
    pub fn immediate_work(&self) -> bool {
        self.slots
            .iter()
            .any(|s| s.resource.gpu().is_some_and(|o| o.immediate_work()))
    }
    pub fn needs_polling(&self) -> bool {
        self.slots.iter().any(|s| {
            (s.logical_index.is_none() && !s.drained())
                || s.resource.gpu().is_some_and(|o| o.needs_polling())
                || s.resource.software().is_some_and(|o| o.pending())
        })
    }
    pub fn needs_capture(&self, now: u64) -> bool {
        self.slots.iter().any(|s| {
            s.dirty()
                && (s.resource.gpu().is_some_and(|o| !o.blocks_capture(now))
                    || s.resource
                        .software()
                        .is_some_and(|o| o.ready() && !o.lost() && !o.pending()))
        })
    }
    pub fn capture_wait_ticks(&self, now: u64, hz: u32, limit: u64) -> u64 {
        self.slots.iter().filter(|s| s.dirty()).fold(limit, |d, s| {
            s.resource
                .gpu()
                .map_or(d, |o| o.capture_wait_ticks(now, hz, d))
        })
    }
    pub fn pointer(&mut self, sample: a::MouseMotion, old: Point) -> Point {
        let totals = [sample.motion_x, sample.motion_y];
        let previous = self.motion.replace(totals);
        let Some(previous) = previous else {
            self.cursor = old;
            return old;
        };
        let candidate = Point {
            x: old
                .x
                .saturating_add(totals[0].wrapping_sub(previous[0]) as i32),
            y: old
                .y
                .saturating_add(totals[1].wrapping_sub(previous[1]) as i32),
        };
        self.cursor = self.layout.nearest(candidate).map_or(old, |(_, p)| p);
        self.cursor
    }
    pub fn close<M: Memory>(
        &mut self,
        app: &App<'_>,
        graphics: &Graphics<'_, '_>,
        context: &Context<'_, '_>,
        windows: &mut Set<M, Consumer>,
        now: u64,
    ) -> bool {
        self.closing = true;
        for s in &mut self.slots {
            s.logical_index = None;
        }
        self.poll(app, graphics, context, windows, now);
        self.slots.iter().all(|s| s.drained())
    }
}
