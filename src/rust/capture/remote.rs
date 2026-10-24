//! Demand-only output capture. The original R4GFX owner retires the GPU copy
//! before orientation or publication lends any CPU shadow to a consumer.
use super::readback::{CONVERSION_PIXELS, MAX_BYTES, Phase, Readback, Stats};
use crate::render::{
    geometry::{Dirty, Rect},
    gpu::{Error, Result, device::Session},
    output_geometry::{
        self as geometry,
        topology::{Projection, Rotation, Viewport},
    },
    scene::intersect,
};
use alloc::vec::Vec;
use r4gfx as g;
use r4os_sdk::App;
#[derive(Clone, Copy, Default)]
pub struct Cursor {
    pub x: i32,
    pub y: i32,
    pub visible: bool,
    pub separate: bool,
}
#[derive(Clone, Copy)]
struct Record {
    key: u64,
    damage: Rect,
    view: Viewport,
    cursor: Cursor,
}
pub struct Image<'a> {
    pub width: i32,
    pub height: i32,
    pub pixels: &'a [u32],
    pub damage: Rect,
    pub epoch: u64,
    pub frame: u64,
    pub cursor: Cursor,
}
pub struct Capture {
    reader: Option<Readback>,
    cpu_profile: bool,
    wanted: bool,
    cancel: bool,
    epoch: u64,
    view: Option<Viewport>,
    records: [Option<Record>; 3],
    current: Option<Record>,
    changes: Dirty,
    last_seen: u64,
    redraw: bool,
    pub retry_ns: u64,
    logical: Vec<u32>,
    damage: Rect,
    offset: usize,
    ready: bool,
    profile_started: u64,
    profile_stats: Stats,
    pub skipped: u64,
    pub oriented_bytes: u64,
}
impl Default for Capture {
    fn default() -> Self {
        Self {
            reader: None,
            cpu_profile: false,
            wanted: false,
            cancel: false,
            epoch: 1,
            view: None,
            records: [None; 3],
            current: None,
            changes: Dirty::default(),
            last_seen: 0,
            redraw: false,
            retry_ns: 0,
            logical: Vec::new(),
            damage: Rect::ZERO,
            offset: 0,
            ready: false,
            profile_started: 0,
            profile_stats: Stats::default(),
            skipped: 0,
            oriented_bytes: 0,
        }
    }
}
impl Capture {
    pub fn wanted(&self) -> bool {
        self.wanted
    }
    pub fn demand(&mut self, wanted: bool) {
        if self.wanted == wanted {
            return;
        }
        self.wanted = wanted;
        self.epoch = self.epoch.wrapping_add(1).max(1);
        if let Some(reader) = &mut self.reader {
            reader.invalidate();
        }
        self.cancel = true;
        self.changes = Dirty::default();
        self.last_seen = 0;
        self.redraw = wanted;
        self.ready = false;
        self.current = None;
        self.records = [None; 3];
    }
    pub fn invalidate(&mut self) {
        let wanted = self.wanted;
        self.demand(false);
        self.demand(wanted);
    }
    pub fn retry(&mut self, now: u64) {
        self.redraw = self.wanted;
        self.retry_ns = now.saturating_add(1_000_000_000);
    }
    pub fn prepared(&self, view: Viewport, format: u32, color: g::R4GfxColorDescription) -> bool {
        !self.cpu_profile
            && self.view == Some(view)
            && self
                .reader
                .as_ref()
                .is_some_and(|reader| reader.matches(view.pixel_w, view.pixel_h, format, color))
    }
    /// # Safety
    /// The exact provider/allocator callbacks remain resident through close,
    /// including handoff of this owning capture to a preparation task.
    pub unsafe fn prepare(
        &mut self,
        app: &App<'_>,
        session: &Session<'_, '_>,
        view: Viewport,
        format: u32,
        color: g::R4GfxColorDescription,
    ) -> Result<()> {
        if self.prepared(view, format, color) {
            return Ok(());
        }
        if self.current.is_some()
            || self.ready
            || self.reader.as_ref().is_some_and(Readback::pending)
        {
            return Err(Error::Busy);
        }
        let bounds = geometry::logical(view).map_err(|_| Error::Limit)?;
        let count = (bounds.w as usize)
            .checked_mul(bounds.h as usize)
            .filter(|v| *v <= MAX_BYTES / 4)
            .ok_or(Error::Limit)?;
        if let Some(reader) = &mut self.reader {
            reader.close(session)?;
        }
        if self.reader.is_none() {
            self.reader = Some(unsafe { Readback::new(app, session) }?);
        }
        self.reader.as_mut().unwrap().prepare(
            session,
            view.pixel_w,
            view.pixel_h,
            format,
            color,
        )?;
        if view.rotation != Rotation::Normal || view.scale != 120 {
            if self.logical.len() != count {
                let mut replacement = Vec::new();
                replacement
                    .try_reserve_exact(count)
                    .map_err(|_| Error::Limit)?;
                replacement.resize(count, 0);
                self.logical = replacement;
            }
        } else {
            self.logical = Vec::new();
        }
        self.cpu_profile = false;
        self.view = Some(view);
        self.reader.as_mut().unwrap().invalidate();
        self.changes = Dirty::default();
        self.last_seen = 0;
        self.cancel = false;
        Ok(())
    }
    pub fn prepare_profile(&mut self, session: &Session<'_, '_>, view: Viewport) -> Result<()> {
        if self.cpu_profile && self.view == Some(view) {
            return Ok(());
        }
        if self.current.is_some() || self.reader.as_ref().is_some_and(Readback::pending) {
            return Err(Error::Busy);
        }
        self.close(session)?;
        let bounds = geometry::logical(view).map_err(|_| Error::Limit)?;
        let count = (bounds.w as usize)
            .checked_mul(bounds.h as usize)
            .filter(|v| *v <= MAX_BYTES / 4)
            .ok_or(Error::Limit)?;
        self.logical
            .try_reserve_exact(count)
            .map_err(|_| Error::Limit)?;
        self.logical.resize(count, 0);
        self.cpu_profile = true;
        self.view = Some(view);
        Ok(())
    }
    pub fn record(&mut self, slot: usize, key: u64, damage: Rect, view: Viewport, cursor: Cursor) {
        if let Some(record) = self.records.get_mut(slot) {
            *record = Some(Record {
                key,
                damage,
                view,
                cursor,
            });
        }
    }
    pub fn stage_profile(&mut self, slot: usize, pixels: &[u32], now: u64) {
        if !self.wanted || !self.cpu_profile {
            return;
        }
        let Some(record) = self.records.get(slot).copied().flatten() else {
            return;
        };
        if self.current.is_some() || self.ready || self.logical.len() != pixels.len() {
            self.redraw = true;
            self.skipped = self.skipped.saturating_add(1);
            return;
        }
        self.logical.copy_from_slice(pixels);
        self.profile_stats.cpu_read_bytes = self
            .profile_stats
            .cpu_read_bytes
            .saturating_add(pixels.len() as u64 * 4);
        self.profile_stats.cpu_write_bytes = self
            .profile_stats
            .cpu_write_bytes
            .saturating_add(pixels.len() as u64 * 4);
        self.current = Some(record);
        self.profile_started = now;
        let Ok(bounds) = geometry::logical(record.view) else {
            self.current = None;
            return;
        };
        self.damage = Rect::new(0, 0, bounds.w, bounds.h);
        self.redraw = false;
    }
    pub fn discarded(&mut self, slot: usize) {
        let Some(record) = self.records.get_mut(slot) else {
            return;
        };
        if self.cpu_profile && record.is_some_and(|r| self.current.is_some_and(|c| c.key == r.key))
        {
            self.current = None;
            self.ready = false;
            self.redraw = self.wanted;
        }
        *record = None;
    }
    pub fn complete(
        &mut self,
        session: &Session<'_, '_>,
        slot: usize,
        source: g::R4GfxResource,
        now: u64,
    ) {
        let Some(record) = self.records.get_mut(slot).and_then(Option::take) else {
            return;
        };
        if !self.wanted {
            return;
        }
        if self.cpu_profile {
            if self
                .current
                .is_some_and(|current| current.key == record.key)
            {
                self.ready = true;
                self.profile_stats.frames = self.profile_stats.frames.saturating_add(1);
                self.profile_stats.last_latency_ns = now.saturating_sub(self.profile_started);
                self.profile_stats.max_latency_ns = self
                    .profile_stats
                    .max_latency_ns
                    .max(self.profile_stats.last_latency_ns);
            }
            self.last_seen = self.last_seen.max(record.key);
            return;
        }
        if record.key <= self.last_seen {
            return;
        }
        let Some(reader) = &mut self.reader else {
            self.redraw = true;
            return;
        };
        if record.key != self.last_seen.saturating_add(1) {
            reader.invalidate();
        }
        self.last_seen = record.key;
        self.changes.invalidate(record.damage);
        if reader.phase() != Ok(Phase::Idle)
            || self.ready
            || self.view != Some(record.view)
            || now < self.retry_ns
        {
            self.skipped = self.skipped.saturating_add(1);
            self.redraw = true;
            return;
        }
        let Ok(bounds) = geometry::logical(record.view) else {
            return;
        };
        let dirty = if reader.valid() {
            self.changes.bounds().unwrap_or(bounds)
        } else {
            bounds
        };
        let Some(clipped) = intersect(dirty, bounds) else {
            return;
        };
        let Ok(Some(native)) = record.view.physical_damage(geometry::topology::Rect {
            x: clipped.x,
            y: clipped.y,
            w: clipped.w as u32,
            h: clipped.h as u32,
        }) else {
            return;
        };
        let region = [g::R4GfxRect {
            x: native.x as u32,
            y: native.y as u32,
            width: native.w,
            height: native.h,
        }];
        if reader
            .begin(
                session,
                source,
                self.epoch,
                record.key,
                reader.acknowledged(),
                &region,
                now,
                now.saturating_add(2_000_000_000),
            )
            .is_err()
        {
            self.redraw = true;
            self.retry_ns = now.saturating_add(1_000_000_000);
            return;
        }
        let full = reader
            .first_region()
            .is_ok_and(|r| r.width == record.view.pixel_w && r.height == record.view.pixel_h);
        self.damage = if full { bounds } else { clipped };
        self.damage.x -= bounds.x;
        self.damage.y -= bounds.y;
        self.current = Some(record);
        self.changes = Dirty::default();
        self.offset = 0;
        self.ready = false;
        self.redraw = false;
    }
    pub fn poll(&mut self, session: &Session<'_, '_>, now: u64) -> Result<()> {
        if self.cancel {
            if let Some(reader) = &mut self.reader {
                reader.cancel(session, false)?;
            }
            self.cancel = false;
        }
        if self.cpu_profile {
            if !self.wanted {
                self.close(session)?;
            }
            return Ok(());
        }
        let Some(reader) = &mut self.reader else {
            return Ok(());
        };
        reader.poll(session, now)?;
        if reader.phase()? == Phase::Failed {
            reader.acknowledge(false)?;
            self.current = None;
            self.ready = false;
            self.redraw = self.wanted;
            self.retry_ns = now.saturating_add(1_000_000_000);
        }
        if !self.wanted {
            match self.close(session) {
                Ok(()) | Err(Error::Busy) => (),
                Err(error) => return Err(error),
            }
            return Ok(());
        }
        if reader.phase()? != Phase::Ready || self.ready {
            return Ok(());
        }
        let Some(record) = self.current else {
            reader.acknowledge(false)?;
            return Ok(());
        };
        if self.logical.is_empty() {
            self.ready = true;
            return Ok(());
        }
        let projection = record.view.projection().map_err(|_| Error::Limit)?;
        let bounds = projection.logical();
        let count = self.damage.w as usize * self.damage.h as usize;
        let end = count.min(self.offset.saturating_add(CONVERSION_PIXELS));
        let pixels = reader.pixels()?;
        for i in self.offset..end {
            let x = self.damage.x as usize + i % self.damage.w as usize;
            let y = self.damage.y as usize + i / self.damage.w as usize;
            let index = native_index(projection, x, y);
            self.logical[y * bounds.w as usize + x] = *pixels.get(index).ok_or(Error::State)?;
        }
        self.oriented_bytes = self
            .oriented_bytes
            .saturating_add((end - self.offset) as u64 * 4);
        self.offset = end;
        self.ready = end == count;
        Ok(())
    }
    pub fn image(&self) -> Option<Image<'_>> {
        if !self.ready {
            return None;
        }
        let record = self.current?;
        let bounds = geometry::logical(record.view).ok()?;
        let pixels = if self.logical.is_empty() {
            self.reader.as_ref()?.pixels().ok()?
        } else {
            &self.logical
        };
        let mut cursor = record.cursor;
        cursor.x -= record.view.origin.x;
        cursor.y -= record.view.origin.y;
        Some(Image {
            width: bounds.w,
            height: bounds.h,
            pixels,
            damage: self.damage,
            epoch: self.epoch,
            frame: record.key,
            cursor,
        })
    }
    pub fn acknowledge(&mut self, published: bool) -> Result<()> {
        if !self.cpu_profile {
            self.reader
                .as_mut()
                .ok_or(Error::State)?
                .acknowledge(published)?;
        }
        self.ready = false;
        self.current = None;
        if !published {
            self.redraw = self.wanted;
        }
        Ok(())
    }
    pub fn needs_polling(&self) -> bool {
        self.cancel
            || self
                .reader
                .as_ref()
                .is_some_and(|r| r.pending() || (r.phase() == Ok(Phase::Ready) && !self.ready))
    }
    pub fn take_redraw(&mut self, now: u64) -> bool {
        if !self.wanted
            || !self.redraw
            || self.current.is_some()
            || self.ready
            || now < self.retry_ns
            || self.reader.as_ref().is_some_and(Readback::pending)
        {
            return false;
        }
        self.redraw = false;
        true
    }
    pub fn source(&self) -> Result<g::R4GfxResource> {
        self.reader
            .as_ref()
            .map_or(Ok(g::R4GfxResource::default()), Readback::source)
    }
    pub fn stats(&self) -> Result<Stats> {
        if self.cpu_profile {
            Ok(self.profile_stats)
        } else {
            self.reader
                .as_ref()
                .map_or(Ok(Stats::default()), Readback::stats)
        }
    }
    pub fn close(&mut self, session: &Session<'_, '_>) -> Result<()> {
        if let Some(reader) = &mut self.reader {
            reader.close(session)?;
        }
        self.logical = Vec::new();
        self.view = None;
        self.current = None;
        self.ready = false;
        self.records = [None; 3];
        self.cpu_profile = false;
        self.cancel = false;
        Ok(())
    }
}
#[inline(always)]
pub fn native_index(projection: Projection, x: usize, y: usize) -> usize {
    let (px, py) = projection.native_pixel(x, y);
    py * projection.pixel_width() + px
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        outputs::topology_wire,
        render::output_geometry::topology::Point,
        runtime::library::{required, scalar},
    };
    use core::mem::MaybeUninit;
    use r4os_library_helpers::raw as c;

    #[test]
    fn local_capture_samples_match_independent_zig_global_point_owner() {
        for rotation in [
            Rotation::Normal,
            Rotation::Clockwise90,
            Rotation::Clockwise180,
            Rotation::Clockwise270,
        ] {
            for scale in [60, 120, 150, 180, 240, 960] {
                let view = Viewport {
                    origin: Point { x: -19, y: 7 },
                    pixel_w: 17,
                    pixel_h: 11,
                    scale,
                    rotation,
                };
                let projection = view.projection().unwrap();
                let bounds = projection.logical();
                let wire = topology_wire::view(view).unwrap();
                // The independent current Zig topology keeps its original
                // arithmetic. Never derive expectations through this Rust core.
                for y in 0..bounds.h as usize {
                    for x in 0..bounds.w as usize {
                        let point = topology_wire::point(Point {
                            x: bounds.x + x as i32,
                            y: bounds.y + y as i32,
                        })
                        .unwrap();
                        let mut original = MaybeUninit::uninit();
                        required(unsafe {
                            c::R4GfxTopologyViewport_physical(
                                0,
                                &wire,
                                &point,
                                original.as_mut_ptr(),
                            )
                        })
                        .unwrap();
                        let ox = scalar(unsafe { c::R4GfxTopologyPoint_get_x(original.as_ptr()) })
                            .unwrap();
                        let oy = scalar(unsafe { c::R4GfxTopologyPoint_get_y(original.as_ptr()) })
                            .unwrap();
                        assert_eq!(
                            native_index(projection, x, y),
                            oy as usize * view.pixel_w as usize + ox as usize
                        );
                    }
                }
            }
        }
    }
}
