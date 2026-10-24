//! Ordered CPU composition through the unchanged R4GFX COLOR_V1 provider.
//! SDR opaque copies retain the existing partial-tile fast path. Other
//! pixels keep linear FP16 precision until the final output conversion.
use super::{
    geometry::Rect,
    layers::{CAPACITY, Cache, Command, Entry, Mode},
    painter::Painter,
    pixels::PixelBytes,
    primitive_assets::{Error, Result},
    scene::{CpuScene, intersect},
    tiles::{Index, Mask, SIDE},
};
use crate::hosted::window_image::{Frame as WindowImage, Memory, Receipt};
use alloc::{
    alloc::{Layout, alloc_zeroed},
    boxed::Box,
};
use r4gfx as g;

mod sealed {
    pub trait External {}
    impl<M: super::Memory, R: super::Receipt> External for super::WindowImage<M, R> {}
}
/// Only the admitted BO owner implements this view. The capture borrows it
/// exclusively through all provider calls; no frame pointer is stored in the
/// retained ordinary-layer cache or survives this capture's completion.
pub trait ExternalFrame: sealed::External {
    fn cpu_image(&self) -> Option<g::R4GfxColorImage>;
    fn begin_read(&mut self) -> bool;
    fn end_read(&mut self, consumed: bool);
}
impl<M: Memory, R: Receipt> ExternalFrame for WindowImage<M, R> {
    fn cpu_image(&self) -> Option<g::R4GfxColorImage> {
        WindowImage::cpu_image(self)
    }
    fn begin_read(&mut self) -> bool {
        self.borrow().is_ok()
    }
    fn end_read(&mut self, consumed: bool) {
        if consumed {
            self.mark_cpu_consumed();
        }
        let released = self.finish(None, false).is_ok();
        debug_assert!(released);
    }
}
struct External<'a> {
    index: usize,
    frame: &'a mut dyn ExternalFrame,
}
pub struct Capture<'owner, 'frame> {
    owner: &'owner mut Owner,
    external: [Option<External<'frame>>; CAPACITY],
    count: usize,
    consumed: bool,
    finished: bool,
    failed: bool,
}
impl Capture<'_, '_> {
    pub fn paint(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
        paint: impl FnOnce(&mut Painter<'_, '_>) -> Result<()>,
    ) -> Result<bool> {
        self.owner
            .cache
            .paint(key, bounds, damage, None, None, paint)
    }
}
impl<'o, 'f> Capture<'o, 'f> {
    pub fn external(
        &mut self,
        key: u32,
        bounds: Rect,
        damages: &[Rect],
        frame: &'f mut dyn ExternalFrame,
    ) -> Result<()> {
        let result = self.external_inner(key, bounds, damages, frame);
        self.failed |= result.is_err();
        result
    }
    fn external_inner(
        &mut self,
        key: u32,
        bounds: Rect,
        damages: &[Rect],
        frame: &'f mut dyn ExternalFrame,
    ) -> Result<()> {
        if self.failed {
            return Err(Error::State);
        }
        if self.count == CAPACITY {
            return Err(Error::Capacity);
        }
        let source = frame.cpu_image().ok_or(Error::State)?;
        if source.image.width != bounds.w as u32 || source.image.height != bounds.h as u32 {
            return Err(Error::Invalid);
        }
        let mut index = None;
        for damage in damages {
            let next = self.owner.cache.external_cpu(key, bounds, *damage)?;
            if next.is_some() {
                index = next;
            }
        }
        let Some(index) = index else {
            return Ok(());
        };
        if self.external[..self.count]
            .iter()
            .flatten()
            .any(|old| old.index == index)
        {
            return Err(Error::State);
        }
        if !frame.begin_read() {
            return Err(Error::State);
        }
        self.external[self.count] = Some(External { index, frame });
        self.count += 1;
        Ok(())
    }
    pub fn finish(mut self, colors: &impl Colors, target: &mut CpuScene<'_>) -> Result<Stats> {
        if self.failed {
            return Err(Error::State);
        }
        self.owner.cache.finish(None)?;
        if self.owner.cache.commands().is_empty() {
            self.owner.frames = self.owner.frames.saturating_add(1);
            self.finished = true;
            return Ok(Stats::default());
        }
        self.owner.workspace()?;
        let result = paint(
            colors,
            &self.owner.cache,
            self.owner.scratch.as_deref_mut().unwrap(),
            target,
            &self.external[..self.count],
        )?;
        self.owner.frames = self.owner.frames.saturating_add(1);
        self.owner.stats.add(result);
        self.consumed = true;
        self.finished = true;
        Ok(result)
    }
}
impl Drop for Capture<'_, '_> {
    fn drop(&mut self) {
        if !self.finished {
            self.owner.cache.cancel();
        }
        for external in self.external[..self.count].iter_mut().flatten() {
            external.frame.end_read(self.consumed);
        }
        self.owner.cache.release_external();
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub commands: u64,
    pub pixels: u64,
    pub direct_pixels: u64,
    pub tiles: u64,
    pub candidates: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
}
impl Stats {
    fn add(&mut self, other: Self) {
        self.commands = self.commands.saturating_add(other.commands);
        self.pixels = self.pixels.saturating_add(other.pixels);
        self.direct_pixels = self.direct_pixels.saturating_add(other.direct_pixels);
        self.tiles = self.tiles.saturating_add(other.tiles);
        self.candidates = self.candidates.saturating_add(other.candidates);
        self.read_bytes = self.read_bytes.saturating_add(other.read_bytes);
        self.write_bytes = self.write_bytes.saturating_add(other.write_bytes);
    }
}
/// # Safety
/// Implementations forward to the admitted COLOR_V1 synchronous transform.
/// They must not retain CPU images or mutate anything outside the target.
pub unsafe trait Colors {
    /// # Safety
    /// Images and request describe disjoint, live initialized storage through
    /// return. The caller serializes any referenced profile state.
    unsafe fn transform(
        &self,
        source: &g::R4GfxColorImage,
        target: &g::R4GfxColorImage,
        request: &g::R4GfxColorTransform,
        stats: &mut g::R4GfxCpuStats,
    ) -> i32;
}
unsafe impl Colors for g::ColorV1Client<'_> {
    unsafe fn transform(
        &self,
        source: &g::R4GfxColorImage,
        target: &g::R4GfxColorImage,
        request: &g::R4GfxColorTransform,
        stats: &mut g::R4GfxCpuStats,
    ) -> i32 {
        unsafe { self.color_image_transform(source, target, request, stats) }
    }
}
struct Scratch {
    tiles: Index,
    linear: [u16; SIDE as usize * SIDE as usize * 4],
    encoded: [u32; SIDE as usize * SIDE as usize],
    touched: [u64; SIDE as usize],
}
impl Scratch {
    fn new() -> Result<Box<Self>> {
        // Index contains only integers and Edge's bools; all-zero is its
        // empty state. All other fields are integer arrays. Initialize in
        // place: this bounded workspace is larger than a normal app stack.
        let raw = unsafe { alloc_zeroed(Layout::new::<Self>()) }.cast::<Self>();
        if raw.is_null() {
            return Err(Error::OutOfMemory);
        }
        Ok(unsafe { Box::from_raw(raw) })
    }
}
pub struct Owner {
    pub cache: Cache,
    scratch: Option<Box<Scratch>>,
    pub frames: u64,
    pub stats: Stats,
}
impl Owner {
    pub fn new(budget: usize) -> Result<Self> {
        Ok(Self {
            cache: Cache::new(budget)?,
            scratch: None,
            frames: 0,
            stats: Stats::default(),
        })
    }
    pub fn begin(&mut self, bounds: Rect) -> Result<()> {
        self.cache.start(Mode::CpuOutput(bounds), None)
    }
    pub fn capture<'o, 'f>(&'o mut self, bounds: Rect) -> Result<Capture<'o, 'f>> {
        self.begin(bounds)?;
        Ok(Capture {
            owner: self,
            external: core::array::from_fn(|_| None),
            count: 0,
            consumed: false,
            finished: false,
            failed: false,
        })
    }
    pub fn cancel(&mut self) {
        self.cache.cancel();
    }
    pub fn finish(&mut self, colors: &impl Colors, target: &mut CpuScene<'_>) -> Result<Stats> {
        self.cache.finish(None)?;
        if self.cache.commands().is_empty() {
            self.frames = self.frames.saturating_add(1);
            return Ok(Stats::default());
        }
        self.workspace()?;
        let result = paint(
            colors,
            &self.cache,
            self.scratch.as_deref_mut().unwrap(),
            target,
            &[],
        )?;
        self.frames = self.frames.saturating_add(1);
        self.stats.add(result);
        Ok(result)
    }
    fn workspace(&mut self) -> Result<()> {
        if self.scratch.is_none() {
            let bytes = core::mem::size_of::<Scratch>();
            self.cache.reserve_workspace(bytes)?;
            match Scratch::new() {
                Ok(scratch) => self.scratch = Some(scratch),
                Err(error) => {
                    self.cache.release_workspace(bytes);
                    return Err(error);
                }
            }
        }
        Ok(())
    }
}
pub fn description(linear: bool, opaque: bool) -> g::R4GfxColorDescription {
    g::R4GfxColorDescription {
        version: 1,
        size: core::mem::size_of::<g::R4GfxColorDescription>() as u32,
        primaries: g::color_primaries_srgb,
        transfer: if linear {
            g::color_transfer_linear
        } else {
            g::color_transfer_srgb
        },
        range: g::color_range_full,
        alpha: if opaque {
            g::color_alpha_opaque
        } else if linear {
            g::color_alpha_optical
        } else {
            g::color_alpha_electrical
        },
        precision: if linear {
            g::color_precision_float16
        } else {
            g::color_precision_unorm8
        },
        flags: 0,
        reference_white: 1000000,
        peak: 1000000,
        black: 0,
        reserved: 0,
    }
}
fn image(
    address: u64,
    bytes: usize,
    pitch: usize,
    width: i32,
    height: i32,
    linear: bool,
    opaque: bool,
) -> g::R4GfxColorImage {
    g::R4GfxColorImage {
        version: 1,
        size: core::mem::size_of::<g::R4GfxColorImage>() as u32,
        image: g::R4GfxCpuImage {
            cpu_address: address,
            byte_length: bytes as u64,
            pitch: pitch as u64,
            width: width as u32,
            height: height as u32,
            format: if linear {
                g::format_abgr16161616f
            } else if opaque {
                g::format_xrgb8888
            } else {
                g::format_argb8888
            },
            reserved: 0,
        },
        description: description(linear, opaque),
        profile: g::R4GfxColorProfile::default(),
    }
}
fn rect(rect: Rect) -> g::R4GfxRect {
    g::R4GfxRect {
        x: rect.x as u32,
        y: rect.y as u32,
        width: rect.w as u32,
        height: rect.h as u32,
    }
}
fn transform(
    colors: &impl Colors,
    source: &g::R4GfxColorImage,
    target: &g::R4GfxColorImage,
    from: Rect,
    to: Rect,
    over: bool,
    stats: &mut Stats,
) -> Result<()> {
    let request = g::R4GfxColorTransform {
        version: 1,
        size: core::mem::size_of::<g::R4GfxColorTransform>() as u32,
        source_rect: rect(from),
        target_rect: rect(to),
        sampler: g::render_sampler_nearest,
        operation: if over {
            g::render_operation_over
        } else {
            g::render_operation_blit
        },
        opacity: 65535,
        flags: 0,
        pixel_budget: to.w as u64 * to.h as u64,
    };
    let mut result = g::R4GfxCpuStats::default();
    // All source/target ranges originate in disjoint Vecs, the canvas, or
    // this owner's workspace and remain borrowed until the synchronous ACK.
    if unsafe { colors.transform(source, target, &request, &mut result) } != g::status_ok {
        return Err(Error::Graphics);
    }
    stats.commands = stats.commands.saturating_add(u64::from(result.commands));
    stats.pixels = stats.pixels.saturating_add(result.pixels);
    stats.read_bytes = stats.read_bytes.saturating_add(result.read_bytes);
    stats.write_bytes = stats.write_bytes.saturating_add(result.write_bytes);
    Ok(())
}
fn bits(width: u32) -> u64 {
    if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}
fn offset(entry: &Entry, area: Rect) -> usize {
    (area.y - entry.bounds.y) as usize * entry.bounds.w as usize
        + (area.x - entry.bounds.x) as usize
}
fn opaque(entry: &Entry, area: Rect) -> bool {
    if entry.external {
        return false;
    }
    let start = offset(entry, area);
    (0..area.h as usize).all(|row| {
        entry.pixels()[start + row * entry.bounds.w as usize..][..area.w as usize]
            .iter()
            .all(|pixel| pixel >> 24 == 255)
    })
}
fn source(
    entry: &Entry,
    index: usize,
    external: &[Option<External<'_>>],
) -> Option<g::R4GfxColorImage> {
    if entry.external {
        external
            .iter()
            .flatten()
            .find(|item| item.index == index)?
            .frame
            .cpu_image()
    } else {
        Some(image(
            entry.pixels().as_ptr() as u64,
            entry.pixels().len() * 4,
            entry.bounds.w as usize * 4,
            entry.bounds.w,
            entry.bounds.h,
            false,
            false,
        ))
    }
}
fn same_description(a: g::R4GfxColorDescription, b: g::R4GfxColorDescription) -> bool {
    a.version == b.version
        && a.size == b.size
        && a.primaries == b.primaries
        && a.transfer == b.transfer
        && a.range == b.range
        && a.alpha == b.alpha
        && a.precision == b.precision
        && a.flags == b.flags
        && a.reference_white == b.reference_white
        && a.peak == b.peak
        && a.black == b.black
        && a.reserved == b.reserved
}
fn copy_opaque(
    cache: &Cache,
    area: Rect,
    target: &mut CpuScene<'_>,
    candidates: Mask,
    external: &[Option<External<'_>>],
) -> bool {
    for index in candidates.iter().rev() {
        let command = cache.commands()[index];
        let Some(clip) = intersect(area, command.scissor) else {
            continue;
        };
        if clip != area {
            return false;
        }
        let entry = &cache.entries()[command.entry as usize];
        let Some(source) = source(entry, command.entry as usize, external) else {
            return false;
        };
        let opaque_alpha = source.description.alpha == g::color_alpha_opaque;
        if !matches!(source.image.format, g::format_xrgb8888 | g::format_argb8888)
            || !same_description(source.description, description(false, opaque_alpha))
            || source.profile.address != 0
            || source.profile.generation != 0
            || source.image.pitch % 4 != 0
            || source.image.byte_length > isize::MAX as u64
        {
            return false;
        }
        // Only Frame's admitted immutable CPU lease or this cache's own Vec
        // reaches here. The exclusive capture loan prevents concurrent unmap.
        let bytes = unsafe {
            core::slice::from_raw_parts(
                source.image.cpu_address as *const u8,
                source.image.byte_length as usize,
            )
        };
        let Some(pixels) = PixelBytes::new(bytes) else {
            return false;
        };
        let pitch = source.image.pitch as usize / 4;
        let bounds = target.full_rect();
        let first = (area.y - entry.bounds.y) as usize * pitch + (area.x - entry.bounds.x) as usize;
        if !opaque_alpha && source.image.format != g::format_xrgb8888 {
            for row in 0..area.h as usize {
                if (0..area.w as usize).any(|x| pixels.word(first + row * pitch + x) >> 24 != 255) {
                    return false;
                }
            }
        }
        let output =
            (area.y - bounds.y) as usize * bounds.w as usize + (area.x - bounds.x) as usize;
        for row in 0..area.h as usize {
            let destination =
                &mut target.pixels[output + row * bounds.w as usize..][..area.w as usize];
            for (x, destination) in destination.iter_mut().enumerate() {
                *destination = pixels.word(first + row * pitch + x) & 0xffffff;
            }
        }
        return true;
    }
    false
}
fn paint(
    colors: &impl Colors,
    cache: &Cache,
    scratch: &mut Scratch,
    target: &mut CpuScene<'_>,
    external: &[Option<External<'_>>],
) -> Result<Stats> {
    if !cache.ready() || target.is_layer() || target.full_rect() != cache.screen() {
        return Err(Error::State);
    }
    let bounds = target.full_rect();
    for command in cache.commands() {
        let entry = cache
            .entries()
            .get(command.entry as usize)
            .ok_or(Error::State)?;
        if !entry.initialized
            || entry.generation == 0
            || entry.bounds.is_empty()
            || command.scissor.is_empty()
            || (!entry.external
                && entry.bounds.w as u64 * entry.bounds.h as u64 > entry.pixels().len() as u64)
            || (entry.external && source(entry, command.entry as usize, external).is_none())
            || intersect(entry.bounds, command.scissor) != Some(command.scissor)
            || intersect(bounds, command.scissor) != Some(command.scissor)
        {
            return Err(Error::State);
        }
    }
    if !scratch
        .tiles
        .reset(cache.commands().iter().map(|c| c.scissor), bounds)
    {
        return Err(Error::State);
    }
    let existing = image(
        target.pixels.as_ptr() as u64,
        target.pixels.len() * 4,
        bounds.w as usize * 4,
        bounds.w,
        bounds.h,
        false,
        true,
    );
    let mut stats = Stats::default();
    while let Some(block) = scratch.tiles.next() {
        for y in (block.y * SIDE..block.bottom * SIDE).step_by(SIDE as usize) {
            for x in (block.x * SIDE..block.right * SIDE).step_by(SIDE as usize) {
                let tile = Rect::new(
                    bounds.x + x,
                    bounds.y + y,
                    SIDE.min(bounds.w - x),
                    SIDE.min(bounds.h - y),
                );
                stats.tiles += 1;
                stats.candidates += block.candidates.count() as u64;
                let area = block
                    .candidates
                    .iter()
                    .filter_map(|i| intersect(tile, cache.commands()[i].scissor))
                    .reduce(|old, clip| old.merged(clip));
                let Some(area) = area else {
                    continue;
                };
                if copy_opaque(cache, area, target, block.candidates, external) {
                    let count = area.w as u64 * area.h as u64;
                    stats.commands = stats.commands.saturating_add(1);
                    stats.pixels = stats.pixels.saturating_add(count);
                    stats.direct_pixels = stats.direct_pixels.saturating_add(count);
                    continue;
                }
                scratch.touched.fill(0);
                let local = Rect::new(area.x - tile.x, area.y - tile.y, area.w, area.h);
                let first = block.candidates.iter().next().ok_or(Error::State)?;
                let Command { entry, scissor } = cache.commands()[first];
                let opaque_base = intersect(area, scissor) == Some(area)
                    && opaque(&cache.entries()[entry as usize], area);
                let working = image(
                    scratch.linear.as_ptr() as u64,
                    core::mem::size_of_val(&scratch.linear),
                    SIDE as usize * 8,
                    tile.w,
                    tile.h,
                    true,
                    false,
                );
                let output = image(
                    scratch.encoded.as_ptr() as u64,
                    core::mem::size_of_val(&scratch.encoded),
                    SIDE as usize * 4,
                    tile.w,
                    tile.h,
                    false,
                    true,
                );
                if !opaque_base {
                    transform(
                        colors,
                        &existing,
                        &working,
                        Rect::new(area.x - bounds.x, area.y - bounds.y, area.w, area.h),
                        local,
                        false,
                        &mut stats,
                    )?;
                }
                for index in block.candidates.iter() {
                    let command = cache.commands()[index];
                    let Some(clip) = intersect(tile, command.scissor) else {
                        continue;
                    };
                    let entry = &cache.entries()[command.entry as usize];
                    let source =
                        source(entry, command.entry as usize, external).ok_or(Error::State)?;
                    let dx = clip.x - tile.x;
                    let dy = clip.y - tile.y;
                    transform(
                        colors,
                        &source,
                        &working,
                        Rect::new(
                            clip.x - entry.bounds.x,
                            clip.y - entry.bounds.y,
                            clip.w,
                            clip.h,
                        ),
                        Rect::new(dx, dy, clip.w, clip.h),
                        !(opaque_base && index == first),
                        &mut stats,
                    )?;
                    let mask = bits(clip.w as u32) << dx;
                    for row in &mut scratch.touched[dy as usize..(dy + clip.h) as usize] {
                        *row |= mask;
                    }
                }
                transform(colors, &working, &output, local, local, false, &mut stats)?;
                for row in 0..tile.h as usize {
                    let offset = (y as usize + row) * bounds.w as usize + x as usize;
                    let mut mask = scratch.touched[row];
                    while mask != 0 {
                        let start = mask.trailing_zeros() as usize;
                        let count = (!(mask >> start)).trailing_zeros() as usize;
                        target.pixels[offset + start..offset + start + count].copy_from_slice(
                            &scratch.encoded
                                [row * SIDE as usize + start..row * SIDE as usize + start + count],
                        );
                        mask &= !(bits(count as u32) << start);
                    }
                }
            }
        }
    }
    Ok(stats)
}

#[cfg(test)]
#[path = "../../../Tests/RustNative/software.rs"]
pub(crate) mod tests;
