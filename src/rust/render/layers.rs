//! Ordinary Desktop layer capture. Pixel/primitive storage remains here;
//! presentation, external BO leases and job/fence retirement are separate.
use super::{
    geometry::{MAX_DAMAGE_REGIONS, Rect, Regions},
    output_geometry as geometry,
    painter::Painter,
    primitive_assets::{Error, Result},
    primitive_frame::{Command as Primitive, Frame},
    scene::{CpuScene, intersect},
};
use alloc::vec::Vec;
pub const CAPACITY: usize = 64;
pub const COMMAND_CAPACITY: usize = CAPACITY * MAX_DAMAGE_REGIONS;
#[derive(Default)]
pub struct Entry {
    pub key: u32,
    pixels: Vec<u32>,
    pub bounds: Rect,
    pub capture_bounds: Rect,
    pub generation: u64,
    pub frame: u64,
    pub dirty: Option<Rect>,
    pub initialized: bool,
    pub(crate) external: bool,
    pub(crate) opaque: bool,
    solid_background: Option<u32>,
    pub(crate) background_applied: bool,
}
impl Entry {
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }
}
#[derive(Clone, Copy)]
pub struct Command {
    pub entry: u8,
    pub scissor: Rect,
}
#[derive(Clone, Copy)]
pub enum Mode {
    Desktop(Rect),
    CpuOutput(Rect),
    NativeOutput(geometry::topology::Viewport),
}
pub struct Replay<'a> {
    pub commands: &'a [Primitive],
    pub reusable: &'a [bool; CAPACITY],
    pub retained: &'a [bool; CAPACITY],
    pub changed: Option<Regions>,
}

// Disjoint pieces of an existing local command outside the replacement.
fn outside(rect: Rect, cut: Rect) -> [Rect; 4] {
    let Some(hit) = intersect(rect, cut) else {
        return [rect, Rect::ZERO, Rect::ZERO, Rect::ZERO];
    };
    [
        Rect::new(rect.x, rect.y, rect.w, hit.y - rect.y),
        Rect::new(rect.x, hit.bottom(), rect.w, rect.bottom() - hit.bottom()),
        Rect::new(rect.x, hit.y, hit.x - rect.x, hit.h),
        Rect::new(hit.right(), hit.y, rect.right() - hit.right(), hit.h),
    ]
}
pub struct Cache {
    entries: Vec<Entry>,
    commands: Vec<Command>,
    scratch: Vec<u32>,
    budget: usize,
    reserved: usize,
    screen: Rect,
    logical: Rect,
    view: Option<geometry::topology::Viewport>,
    frame: u64,
    collecting: bool,
    failure: Option<Error>,
    pub complete_scene: bool,
    pub changed_bytes: u64,
    pub unchanged_layers: u64,
    pub reused_captures: u64,
}
impl Cache {
    pub fn new(budget: usize) -> Result<Self> {
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(CAPACITY)
            .map_err(|_| Error::OutOfMemory)?;
        entries.resize_with(CAPACITY, Entry::default);
        let mut commands = Vec::new();
        commands
            .try_reserve_exact(COMMAND_CAPACITY)
            .map_err(|_| Error::OutOfMemory)?;
        Ok(Self {
            entries,
            commands,
            scratch: Vec::new(),
            budget,
            reserved: 0,
            screen: Rect::ZERO,
            logical: Rect::ZERO,
            view: None,
            frame: 0,
            collecting: false,
            failure: None,
            complete_scene: false,
            changed_bytes: 0,
            unchanged_layers: 0,
            reused_captures: 0,
        })
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    /// Only native recordings can be forked without duplicating raster
    /// buffers. External image metadata grants no read loan by itself.
    pub fn fork_recording(&self) -> Result<Self> {
        if self.collecting
            || self.failure.is_some()
            || !self.scratch.is_empty()
            || self.entries.iter().any(|e| !e.pixels.is_empty())
        {
            return Err(Error::State);
        }
        let mut out = Self::new(self.budget)?;
        for (to, from) in out.entries.iter_mut().zip(&self.entries) {
            *to = Entry {
                key: from.key,
                pixels: Vec::new(),
                bounds: from.bounds,
                capture_bounds: from.capture_bounds,
                generation: from.generation,
                frame: from.frame,
                dirty: from.dirty,
                initialized: from.initialized,
                external: from.external,
                opaque: from.opaque,
                solid_background: from.solid_background,
                background_applied: from.background_applied,
            };
        }
        out.commands.extend_from_slice(&self.commands);
        out.screen = self.screen;
        out.logical = self.logical;
        out.view = self.view;
        out.frame = self.frame;
        out.complete_scene = self.complete_scene;
        out.changed_bytes = self.changed_bytes;
        out.unchanged_layers = self.unchanged_layers;
        out.reused_captures = self.reused_captures;
        Ok(out)
    }
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }
    pub fn screen(&self) -> Rect {
        self.screen
    }
    pub fn frame(&self) -> u64 {
        self.frame
    }
    pub fn reserved(&self) -> usize {
        self.reserved
    }
    pub fn view(&self) -> Option<geometry::topology::Viewport> {
        self.view
    }
    pub fn captured(&self) -> bool {
        !self.collecting && self.failure.is_none()
    }
    pub fn collecting(&self) -> bool {
        self.collecting && self.failure.is_none()
    }
    /// Metadata half of a native external layer. Its actual read loan lives
    /// with the GPU transaction and matching window owner, never this cache.
    pub fn external_native(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
    ) -> Result<Option<usize>> {
        if !self.collecting || self.failure.is_some() {
            return Err(Error::State);
        }
        if key == 0 || self.commands.len() == COMMAND_CAPACITY {
            return Err(Error::Capacity);
        }
        let Some(logical) = intersect(self.logical, bounds).and_then(|r| intersect(r, damage))
        else {
            return Ok(None);
        };
        let raster = if let Some(view) = self.view {
            geometry::raster_rect(view, bounds)?
        } else {
            bounds
        };
        let Some(scissor) = (if let Some(view) = self.view {
            intersect(geometry::raster_rect(view, logical)?, self.screen)
        } else {
            Some(logical)
        }) else {
            return Ok(None);
        };
        let index = self
            .entries
            .iter()
            .position(|e| e.key == key)
            .or_else(|| self.entries.iter().position(|e| e.key == 0))
            .ok_or(Error::Capacity)?;
        let entry = &mut self.entries[index];
        if entry.frame == self.frame
            && (!entry.external || entry.bounds != raster || entry.capture_bounds != bounds)
        {
            return Err(Error::State);
        }
        if !entry.external {
            self.reserved -= entry.pixels.len() * 4;
            *entry = Entry::default();
        }
        entry.key = key;
        entry.bounds = raster;
        entry.capture_bounds = bounds;
        entry.generation = self.frame;
        entry.frame = self.frame;
        entry.initialized = true;
        entry.external = true;
        self.commands.push(Command {
            entry: index as u8,
            scissor,
        });
        Ok(Some(index))
    }
    pub(crate) fn ready(&self) -> bool {
        !self.collecting && self.failure.is_none() && self.view.is_none()
    }
    pub(crate) fn reserve_workspace(&mut self, bytes: usize) -> Result<()> {
        if bytes > self.budget || self.reserved > self.budget - bytes {
            return Err(Error::OutOfMemory);
        }
        self.reserved += bytes;
        Ok(())
    }
    pub(crate) fn release_workspace(&mut self, bytes: usize) {
        self.reserved -= bytes;
    }
    pub(crate) fn external_cpu(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
    ) -> Result<Option<usize>> {
        if !self.collecting || self.failure.is_some() || self.view.is_some() {
            return Err(Error::State);
        }
        if key == 0 || self.commands.len() == COMMAND_CAPACITY {
            return Err(Error::Capacity);
        }
        let Some(scissor) = intersect(self.screen, bounds).and_then(|clip| intersect(clip, damage))
        else {
            return Ok(None);
        };
        let index = self
            .entries
            .iter()
            .position(|e| e.key == key)
            .or_else(|| self.entries.iter().position(|e| e.key == 0))
            .ok_or(Error::Capacity)?;
        let entry = &mut self.entries[index];
        if entry.frame == self.frame && (!entry.external || entry.bounds != bounds) {
            return Err(Error::State);
        }
        if !entry.external {
            self.reserved -= entry.pixels.len() * 4;
            *entry = Entry::default();
        }
        entry.key = key;
        entry.bounds = bounds;
        entry.capture_bounds = bounds;
        entry.generation = self.frame;
        entry.frame = self.frame;
        entry.initialized = true;
        entry.external = true;
        self.commands.push(Command {
            entry: index as u8,
            scissor,
        });
        Ok(Some(index))
    }
    pub(crate) fn release_external(&mut self) {
        for entry in &mut self.entries {
            if entry.external {
                *entry = Entry::default();
            }
        }
    }
    pub fn start(&mut self, mode: Mode, mut recording: Option<&mut Frame>) -> Result<()> {
        if self.collecting {
            return Err(Error::State);
        }
        let (screen, logical, view) = match mode {
            Mode::Desktop(bounds) => {
                if bounds.x != 0 || bounds.y != 0 {
                    return Err(Error::Invalid);
                }
                (bounds, bounds, None)
            }
            Mode::CpuOutput(bounds) => {
                if recording.is_some() {
                    return Err(Error::State);
                }
                (bounds, bounds, None)
            }
            Mode::NativeOutput(view) => {
                if recording.as_ref().is_none_or(|r| r.mirror()) {
                    return Err(Error::State);
                }
                let logical = geometry::logical(view)?;
                (geometry::native(view), logical, Some(view))
            }
        };
        if screen.is_empty() {
            return Err(Error::Invalid);
        }
        if view.is_some() && self.view != view {
            for entry in &mut self.entries {
                entry.initialized = false;
            }
        }
        let frame = self.frame.checked_add(1).ok_or(Error::Overflow)?;
        if let Some(recording) = recording.as_deref_mut() {
            recording.set_view(view)?;
            recording.start(frame)?;
        }
        self.view = view;
        self.screen = screen;
        self.logical = logical;
        self.frame = frame;
        self.commands.clear();
        self.failure = None;
        self.collecting = true;
        self.complete_scene = false;
        Ok(())
    }
    pub fn finish(&mut self, recording: Option<&Frame>) -> Result<&[Command]> {
        if !self.collecting {
            return Err(Error::State);
        }
        self.collecting = false;
        if let Some(error) = self.failure.or_else(|| recording.and_then(Frame::failure)) {
            return Err(error);
        }
        Ok(&self.commands)
    }
    pub fn cancel(&mut self) {
        self.collecting = false;
        self.failure = None;
        self.commands.clear();
        for entry in &mut self.entries {
            entry.initialized = false;
        }
    }
    pub fn uploaded(&mut self, index: usize, generation: u64) {
        if let Some(entry) = self.entries.get_mut(index) {
            if entry.generation == generation {
                entry.dirty = None;
            }
        }
    }
    /// Allocator admission counts the old buffer while allocating its larger
    /// replacement. Retained lengths, rather than only requested dimensions,
    /// are included in the budget, matching the existing layer owner.
    fn grow(
        buffer: &mut Vec<u32>,
        count: usize,
        budget: usize,
        reserved: &mut usize,
    ) -> Result<()> {
        if buffer.len() >= count {
            return Ok(());
        }
        let bytes = count.checked_mul(4).ok_or(Error::Overflow)?;
        if bytes > budget || *reserved > budget - bytes {
            return Err(Error::OutOfMemory);
        }
        let mut next = Vec::new();
        next.try_reserve_exact(count)
            .map_err(|_| Error::OutOfMemory)?;
        next.resize(count, 0);
        *reserved += bytes;
        *reserved -= buffer.len() * 4;
        *buffer = next;
        Ok(())
    }
    /// Reposition an entire resident 1:1 CPU layer. The caller proves that
    /// its source is unchanged. The command still clips to current damage.
    pub(crate) fn place(&mut self, key: u32, bounds: Rect, damage: Rect) -> Result<bool> {
        if !self.collecting || self.failure.is_some() || self.view.is_some() {
            return Err(Error::State);
        }
        if self.commands.len() == COMMAND_CAPACITY {
            return Err(Error::Capacity);
        }
        if intersect(self.screen, bounds) != Some(bounds) {
            return Ok(false);
        }
        let Some(scissor) = intersect(bounds, damage) else { return Ok(false) };
        let Some(index) = self.entries.iter().position(|e| e.key == key) else { return Ok(false) };
        let e = &mut self.entries[index];
        if !e.initialized || e.external || e.bounds != e.capture_bounds ||
            e.bounds.w != bounds.w || e.bounds.h != bounds.h ||
            (e.frame == self.frame && e.bounds != bounds) {
            return Ok(false);
        }
        e.bounds = bounds;
        e.capture_bounds = bounds;
        e.frame = self.frame;
        self.commands.push(Command { entry: index as u8, scissor });
        self.reused_captures = self.reused_captures.saturating_add(1);
        Ok(true)
    }
    pub fn paint(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
        recording: Option<&mut Frame>,
        replay: Option<Replay<'_>>,
        paint: impl FnOnce(&mut Painter<'_, '_>) -> Result<()>,
    ) -> Result<bool> {
        self.paint_over_solid(key, bounds, damage, recording, replay, None, paint)
    }
    pub(crate) fn paint_over_solid(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
        mut recording: Option<&mut Frame>,
        replay: Option<Replay<'_>>,
        background: Option<u32>,
        paint: impl FnOnce(&mut Painter<'_, '_>) -> Result<()>,
    ) -> Result<bool> {
        let result = self.paint_inner(
            key,
            bounds,
            damage,
            recording.as_deref_mut(),
            replay,
            background,
            None,
            paint,
        );
        if let Err(error) = result {
            self.failure = Some(error);
            if let Some(recording) = recording {
                recording.reject(error);
            }
        }
        result
    }
    /// CPU-only explicit opaque base fill. It initializes every scratch pixel
    /// before the content closure, so the transparent clear is redundant.
    pub(crate) fn paint_filled(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
        rgb: u32,
        paint: impl FnOnce(&mut Painter<'_, '_>) -> Result<()>,
    ) -> Result<bool> {
        let result = self.paint_inner(key, bounds, damage, None, None, None, Some(rgb), paint);
        if let Err(error) = result {
            self.failure = Some(error);
        }
        result
    }
    fn paint_inner(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
        mut recording: Option<&mut Frame>,
        replay: Option<Replay<'_>>,
        background: Option<u32>,
        initial_fill: Option<u32>,
        paint: impl FnOnce(&mut Painter<'_, '_>) -> Result<()>,
    ) -> Result<bool> {
        if !self.collecting || self.failure.is_some() {
            return Err(Error::State);
        }
        if key == 0 || self.commands.len() == COMMAND_CAPACITY {
            return Err(Error::Capacity);
        }
        let Some(clipped) = intersect(self.logical, bounds) else {
            return Ok(false);
        };
        let Some(logical_scissor) = intersect(clipped, damage) else {
            return Ok(false);
        };
        let (raster, scissor) = if let Some(view) = self.view {
            let Some(raster) = intersect(geometry::raster_rect(view, clipped)?, self.screen) else {
                return Ok(false);
            };
            let Some(scissor) = intersect(geometry::raster_rect(view, logical_scissor)?, raster)
            else {
                return Ok(false);
            };
            (raster, scissor)
        } else {
            (clipped, logical_scissor)
        };
        let index = self
            .entries
            .iter()
            .position(|e| e.key == key)
            .or_else(|| self.entries.iter().position(|e| e.key == 0))
            .ok_or(Error::Capacity)?;
        let mirror = recording.as_ref().is_none_or(|r| r.mirror());
        let entry = &mut self.entries[index];
        if entry.external {
            return Err(Error::State);
        }
        let mut partial = None;
        let mut partial_cut = Rect::ZERO;
        let mut retained_start = 0;
        if !mirror
            && entry.solid_background == background
            && replay.is_some()
            && entry.initialized
            && entry.frame == self.frame - 1
            && entry.bounds == raster
            && entry.capture_bounds == clipped
            && logical_scissor == clipped
        {
            let previous = replay.as_ref().unwrap();
            let count = previous
                .commands
                .iter()
                .filter(|c| c.layer as usize == index)
                .count();
            if count != 0 && previous.reusable[index] {
                let recording = recording.as_deref_mut().ok_or(Error::State)?;
                if count > super::primitive_frame::CAPACITY - recording.commands().len() {
                    return Err(Error::Capacity);
                }
                for command in previous.commands {
                    if command.layer as usize == index {
                        recording.append(*command)?;
                    }
                }
                entry.frame = self.frame;
                self.commands.push(Command {
                    entry: index as u8,
                    scissor,
                });
                self.reused_captures = self.reused_captures.saturating_add(1);
                return Ok(false);
            }
            // Retain a complete layer while recording only a local update.
            // Exact unit-scale edges need no new filtering/fringe proof.
            // Rebuild periodically before clipping fragments fill the budget.
            if previous.retained[index]
                && !entry.background_applied
                // A short solid fill is already cheap and can compare equal
                // to its prior GPU recording; fragmenting it adds work.
                && count > 1
                && previous.commands.iter().any(|c| c.layer as usize == index && c.texture.is_some())
                && self.view.is_some_and(|v| v.scale == 120)
                && previous.commands.len() <= super::primitive_frame::CAPACITY / 2
                && count <= super::primitive_frame::CAPACITY / 4
            {
                let changed = previous.changed.and_then(|regions| {
                    regions
                        .as_slice()
                        .iter()
                        .filter_map(|r| intersect(*r, clipped))
                        .reduce(Rect::merged)
                });
                if let Some(changed) = changed.filter(|r| *r != clipped) {
                    let physical = geometry::raster_rect(self.view.unwrap(), changed)?;
                    let cut = Rect::new(
                        physical.x - raster.x,
                        physical.y - raster.y,
                        physical.w,
                        physical.h,
                    );
                    let recording = recording.as_deref_mut().ok_or(Error::State)?;
                    let pieces = previous
                        .commands
                        .iter()
                        .filter(|c| c.layer as usize == index)
                        .map(|c| {
                            outside(c.scissor, cut)
                                .iter()
                                .filter(|r| !r.is_empty())
                                .count()
                        })
                        .sum::<usize>();
                    let resident = previous
                        .commands
                        .iter()
                        .filter(|c| c.layer as usize == index)
                        .all(|c| {
                            c.texture
                                .is_none_or(|t| recording.assets.contains(c.content, t, c.source))
                        });
                    if pieces <= super::primitive_frame::CAPACITY / 4
                        && recording.commands().len() + pieces + count
                            <= super::primitive_frame::CAPACITY
                        && resident
                    {
                        retained_start = recording.commands().len();
                        for command in previous
                            .commands
                            .iter()
                            .filter(|c| c.layer as usize == index)
                        {
                            let pieces = outside(command.scissor, cut);
                            if pieces.iter().any(|r| !r.is_empty()) {
                                if let Some(texture) = command.texture {
                                    recording.assets.reuse_texture(texture as usize)?;
                                }
                            }
                            for scissor in pieces.into_iter().filter(|r| !r.is_empty()) {
                                recording.append(Primitive {
                                    scissor,
                                    ..*command
                                })?;
                            }
                        }
                        partial = Some(changed);
                        partial_cut = cut;
                    }
                }
            }
        }
        if entry.bounds != raster
            || entry.capture_bounds != clipped
            || entry.solid_background != background
        {
            if entry.frame == self.frame {
                return Err(Error::State);
            }
            if mirror {
                Self::grow(
                    &mut entry.pixels,
                    CpuScene::required_bytes(clipped.w, clipped.h).ok_or(Error::Bounds)? / 4,
                    self.budget,
                    &mut self.reserved,
                )?;
            }
            entry.key = key;
            entry.bounds = raster;
            entry.capture_bounds = clipped;
            entry.initialized = false;
            entry.dirty = None;
        }
        let repaint = partial.unwrap_or(if entry.initialized {
            logical_scissor
        } else {
            clipped
        });
        if mirror {
            Self::grow(
                &mut entry.pixels,
                CpuScene::required_bytes(clipped.w, clipped.h).ok_or(Error::Bounds)? / 4,
                self.budget,
                &mut self.reserved,
            )?;
            Self::grow(
                &mut self.scratch,
                CpuScene::required_bytes(repaint.w, repaint.h).ok_or(Error::Bounds)? / 4,
                self.budget,
                &mut self.reserved,
            )?;
        }
        if let Some(recording) = recording.as_deref_mut() {
            recording.begin(index as u8, clipped)?;
        }
        let patch_start = recording.as_ref().map_or(0, |r| r.commands().len());
        let mut scene = if mirror {
            Some(CpuScene::layer(&mut self.scratch, repaint).ok_or(Error::Bounds)?)
        } else {
            None
        };
        let result = {
            let mut painter = if let Some(recording) = recording.as_deref_mut() {
                Painter::recorded(scene.as_mut(), recording, repaint, repaint)
            } else {
                Painter::cpu(scene.as_mut().ok_or(Error::State)?)
            };
            let initialized = if let Some(rgb) = initial_fill {
                painter.fill(repaint, rgb)
            } else {
                painter.clear(repaint)
            };
            initialized.and_then(|()| paint(&mut painter))
        };
        let mut background_applied = false;
        if let Some(recording) = recording.as_deref_mut() {
            if let Err(error) = result {
                recording.reject(error);
            } else if let Some(rgb) = background {
                background_applied = recording.complete_over_solid(rgb);
            }
            recording.end()?;
        }
        result?;
        if partial.is_some() {
            let previous = replay.as_ref().ok_or(Error::State)?;
            let recording = recording.as_deref_mut().ok_or(Error::State)?;
            let mut old = previous
                .commands
                .iter()
                .filter(|c| c.layer as usize == index)
                .filter_map(|c| c.clipped(partial_cut));
            let mut new = recording.commands()[patch_start..]
                .iter()
                .filter_map(|c| c.clipped(partial_cut));
            let unchanged = loop {
                match (old.next(), new.next()) {
                    (None, None) => break true,
                    (Some(a), Some(b)) if a.same(&b) => {}
                    _ => break false,
                }
            };
            if unchanged {
                recording.truncate_commands(retained_start)?;
                for command in previous
                    .commands
                    .iter()
                    .filter(|c| c.layer as usize == index)
                {
                    if let Some(texture) = command.texture {
                        recording.assets.reuse_texture(texture as usize)?;
                    }
                    recording.append(*command)?;
                }
                entry.frame = self.frame;
                self.commands.push(Command {
                    entry: index as u8,
                    scissor,
                });
                self.reused_captures = self.reused_captures.saturating_add(1);
                return Ok(false);
            }
        }
        entry.solid_background = background;
        entry.background_applied = background_applied;
        entry.frame = self.frame;
        let generation = entry.generation.checked_add(1).ok_or(Error::Overflow)?;
        if !mirror {
            entry.opaque = false;
            entry.generation = generation;
            entry.initialized = true;
            entry.dirty = None;
            self.commands.push(Command {
                entry: index as u8,
                scissor,
            });
            return Ok(true);
        }
        let source = scene.as_ref().ok_or(Error::State)?.pixels();
        let previously_opaque = entry.initialized && entry.opaque;
        let mut opaque = !entry.initialized || repaint == entry.bounds || previously_opaque;
        let mut changed = None::<Rect>;
        for row in 0..repaint.h as usize {
            let offset = ((repaint.y - entry.bounds.y) as usize + row) * entry.bounds.w as usize
                + (repaint.x - entry.bounds.x) as usize;
            let width = repaint.w as usize;
            let source = &source[row * width..row * width + width];
            let destination = &mut entry.pixels[offset..offset + width];
            if entry.initialized && source == destination {
                // Unknown unchanged rows cannot upgrade a partly transparent
                // layer. Known opaque rows retain their existing proof.
                opaque &= previously_opaque;
                continue;
            }
            // Reuse the changed-row traversal, never a per-composition tile
            // scan, to certify a complete opaque layer conservatively.
            opaque =
                opaque && source.iter().fold(u32::MAX, |bits, pixel| bits & *pixel) >> 24 == 255;
            destination.copy_from_slice(source);
            self.changed_bytes = self.changed_bytes.saturating_add((width * 4) as u64);
            let line = Rect::new(repaint.x, repaint.y + row as i32, repaint.w, 1);
            changed = Some(changed.map_or(line, |old| old.merged(line)));
        }
        entry.opaque = opaque;
        if let Some(rect) = changed {
            entry.generation = generation;
            entry.dirty = Some(entry.dirty.map_or(rect, |old| old.merged(rect)));
        } else {
            self.unchanged_layers = self.unchanged_layers.saturating_add(1);
        }
        entry.initialized = true;
        self.commands.push(Command {
            entry: index as u8,
            scissor,
        });
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_capture_retains_rows_generations_and_exact_upload_ack() {
        let full = Rect::new(0, 0, 8, 8);
        let overlay = Rect::new(2, 2, 4, 4);
        let pixel = Rect::new(3, 3, 1, 1);
        let mut cache = Cache::new(1024 * 1024).unwrap();
        let capture = |cache: &mut Cache, damage, color| {
            cache.start(Mode::Desktop(full), None).unwrap();
            cache
                .paint(1, full, damage, None, None, |p| p.fill(full, 0x203040))
                .unwrap();
            cache
                .paint(2, overlay, damage, None, None, |p| p.fill(overlay, color))
                .unwrap();
            cache.finish(None).unwrap();
        };
        capture(&mut cache, full, 0x882244);
        assert_eq!(cache.changed_bytes, 320);
        let generation = cache.entries[1].generation;
        cache.uploaded(1, generation + 1);
        assert_eq!(cache.entries[1].dirty, Some(overlay));
        cache.uploaded(1, generation);
        assert_eq!(cache.entries[1].dirty, None);
        capture(&mut cache, pixel, 0x445566);
        assert_eq!(cache.changed_bytes, 324);
        assert_eq!(cache.entries[1].dirty, Some(pixel));
        assert_eq!(cache.entries[1].pixels()[5], 0xff445566);
        assert_eq!(cache.entries[1].pixels()[0], 0xff882244);
        assert_eq!(cache.entries[1].generation, generation + 1);
        capture(&mut cache, pixel, 0x445566);
        assert_eq!(cache.changed_bytes, 324);
        assert_eq!(cache.entries[1].generation, generation + 1);
        assert!(cache.unchanged_layers >= 3);
    }
    #[test]
    fn failed_capture_requires_cancel_and_replacement_counts_live_old_buffer() {
        let full = Rect::new(0, 0, 4, 4);
        let mut cache = Cache::new(128).unwrap();
        cache.start(Mode::Desktop(full), None).unwrap();
        assert!(
            cache
                .paint(1, full, full, None, None, |p| p.fill(full, 1))
                .unwrap()
        );
        cache.finish(None).unwrap();
        assert_eq!(cache.reserved(), 128);
        let bigger = Rect::new(0, 0, 5, 4);
        cache.start(Mode::Desktop(bigger), None).unwrap();
        assert_eq!(
            cache.paint(1, bigger, bigger, None, None, |_| Ok(())),
            Err(Error::OutOfMemory)
        );
        assert!(cache.finish(None).is_err());
        assert!(!cache.ready());
        assert_eq!(cache.entries[0].pixels().len(), 16);
        cache.cancel();
        cache.start(Mode::Desktop(full), None).unwrap();
        cache
            .paint(1, full, full, None, None, |p| p.fill(full, 2))
            .unwrap();
        cache.finish(None).unwrap();
        assert_eq!(cache.entries[0].pixels()[0], 0xff000002);
        assert_eq!(cache.reserved(), 128);
    }
    #[test]
    fn cached_opacity_never_upgrades_unknown_or_partially_transparent_rows() {
        let full = Rect::new(0, 0, 17, 5);
        let mut cache = Cache::new(8192).unwrap();
        cache.start(Mode::Desktop(full), None).unwrap();
        cache
            .paint(1, full, full, None, None, |p| p.fill(full, 0x173954))
            .unwrap();
        cache.finish(None).unwrap();
        assert!(cache.entries[0].opaque);
        // Unchanged warm rows keep the earlier proof, without rescanning alpha.
        cache.start(Mode::Desktop(full), None).unwrap();
        cache
            .paint(1, full, full, None, None, |p| p.fill(full, 0x173954))
            .unwrap();
        cache.finish(None).unwrap();
        assert!(cache.entries[0].opaque);
        let hole = Rect::new(2, 1, 1, 1);
        cache.start(Mode::Desktop(full), None).unwrap();
        cache.paint(1, full, hole, None, None, |_| Ok(())).unwrap();
        cache.finish(None).unwrap();
        assert!(!cache.entries[0].opaque);
        assert_eq!(cache.entries[0].pixels()[19], 0);
        // A wholly unchanged full capture retains the unknown/transparent state.
        cache.start(Mode::Desktop(full), None).unwrap();
        cache
            .paint(1, full, full, None, None, |p| {
                p.fill(full, 0x173954)?;
                p.clear(hole)
            })
            .unwrap();
        cache.finish(None).unwrap();
        assert!(!cache.entries[0].opaque);
        // An opaque partial region cannot certify the unpainted transparent hole.
        let other = Rect::new(5, 3, 2, 1);
        cache.start(Mode::Desktop(full), None).unwrap();
        cache
            .paint(1, full, other, None, None, |p| p.fill(other, 0x2468ac))
            .unwrap();
        cache.finish(None).unwrap();
        assert!(!cache.entries[0].opaque);
        assert_eq!(cache.entries[0].pixels()[19], 0);
        // A fully changed opaque capture can establish a new whole-layer proof.
        cache.start(Mode::Desktop(full), None).unwrap();
        cache
            .paint(1, full, full, None, None, |p| p.fill(full, 0xabcdef))
            .unwrap();
        cache.finish(None).unwrap();
        assert!(cache.entries[0].opaque);
    }
    #[test]
    fn explicit_filled_capture_initializes_scratch_and_preserves_partial_damage() {
        let bounds = Rect::new(-3, 5, 17, 5);
        let mut cache = Cache::new(8192).unwrap();
        cache.start(Mode::CpuOutput(bounds), None).unwrap();
        cache
            .paint_filled(1, bounds, bounds, 0x173954, |_| Ok(()))
            .unwrap();
        cache.finish(None).unwrap();
        assert!(cache.entries[0].pixels().iter().all(|p| *p == 0xff173954));
        assert!(cache.entries[0].opaque);
        let damage = Rect::new(2, 7, 3, 1);
        cache.start(Mode::CpuOutput(bounds), None).unwrap();
        cache
            .paint_filled(1, bounds, damage, 0x2468ac, |_| Ok(()))
            .unwrap();
        cache.finish(None).unwrap();
        for y in 0..5 {
            for x in 0..17 {
                assert_eq!(
                    cache.entries[0].pixels()[y * 17 + x],
                    if damage.contains(bounds.x + x as i32, bounds.y + y as i32) {
                        0xff2468ac
                    } else {
                        0xff173954
                    }
                );
            }
        }
        // An ordinary empty paint still needs transparent clearing after reuse.
        cache.start(Mode::CpuOutput(bounds), None).unwrap();
        cache
            .paint(1, bounds, damage, None, None, |_| Ok(()))
            .unwrap();
        cache.finish(None).unwrap();
        assert_eq!(cache.entries[0].pixels()[2 * 17 + 5], 0);
        assert!(!cache.entries[0].opaque);
    }
}
