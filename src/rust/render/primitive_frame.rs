//! One bounded native primitive capture, with optional software mirroring.
//! Each source is consumed immediately into content-keyed texture storage.
//! Recording and CPU drawing use disjoint short loans, without callbacks
//! retaining an exclusive Scene reference across a provider call.
use super::{
    geometry::Rect,
    output_geometry as geometry,
    pixels::PixelBytes,
    primitive_assets::{Cache, Error, Result},
    primitive_image::{Format, Paint, Picture, View},
    scene::{CpuScene, Indexed8Nearest, Mapping, Xrgb32Nearest, intersect},
};
use alloc::vec::Vec;
use r4os_contract::GfxSampleGrid;
pub const CAPACITY: usize = 8192;
#[derive(Clone, Copy)]
pub struct Command {
    pub layer: u8,
    pub texture: Option<u8>,
    pub source: Rect,
    pub target: Rect,
    pub scissor: Rect,
    pub color: u32,
    pub over: bool,
    pub grid: GfxSampleGrid,
}
impl Command {
    fn fill(layer: u8, rect: Rect, color: u32) -> Self {
        Self {
            layer,
            texture: None,
            source: Rect::ZERO,
            target: rect,
            scissor: rect,
            color,
            over: false,
            grid: GfxSampleGrid::default(),
        }
    }
}
#[derive(Clone, Copy)]
struct Active {
    layer: u8,
    bounds: Rect,
}
pub struct Frame {
    pub assets: Cache,
    commands: Vec<Command>,
    mirror: bool,
    failure: Option<Error>,
    active: Option<Active>,
    pub fractional_pixels: u64,
    pub merged_fills: u64,
    view: Option<geometry::topology::Viewport>,
}
impl Frame {
    pub fn new() -> Result<Self> {
        let assets = Cache::new(64 * 1024 * 1024)?;
        let mut commands = Vec::new();
        commands
            .try_reserve_exact(CAPACITY)
            .map_err(|_| Error::OutOfMemory)?;
        Ok(Self {
            assets,
            commands,
            mirror: true,
            failure: None,
            active: None,
            fractional_pixels: 0,
            merged_fills: 0,
            view: None,
        })
    }
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }
    pub fn view(&self) -> Option<geometry::topology::Viewport> {
        self.view
    }
    pub fn mirror(&self) -> bool {
        self.mirror
    }
    pub fn set_mirror(&mut self, mirror: bool) -> Result<()> {
        if self.active.is_some() {
            return Err(Error::State);
        }
        self.mirror = mirror;
        Ok(())
    }
    pub fn set_view(&mut self, view: Option<geometry::topology::Viewport>) -> Result<()> {
        if self.active.is_some() {
            return Err(Error::State);
        }
        if let Some(view) = view {
            view.validate().map_err(|_| Error::Bounds)?;
        }
        self.view = view;
        Ok(())
    }
    pub fn failure(&self) -> Option<Error> {
        self.failure
    }
    pub fn reject(&mut self, error: Error) {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
    }
    pub fn start(&mut self, generation: u64) -> Result<()> {
        if self.active.is_some() {
            return Err(Error::State);
        }
        self.assets.start(generation)?;
        self.commands.clear();
        self.failure = None;
        Ok(())
    }
    pub fn begin(&mut self, layer: u8, bounds: Rect) -> Result<()> {
        if self.active.is_some() {
            return Err(Error::State);
        }
        let raster = if let Some(view) = self.view {
            intersect(geometry::raster_rect(view, bounds)?, geometry::native(view))
                .ok_or(Error::Bounds)?
        } else {
            bounds
        };
        self.active = Some(Active {
            layer,
            bounds: raster,
        });
        Ok(())
    }
    pub fn end(&mut self) -> Result<()> {
        if self.active.take().is_none() {
            return Err(Error::State);
        }
        if let Some(error) = self.failure {
            return Err(error);
        }
        Ok(())
    }
    pub fn paint(
        &mut self,
        painter: Option<&mut CpuScene<'_>>,
        operation: Paint<'_>,
    ) -> Result<()> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if let Err(error) = self.record(operation) {
            self.reject(error);
            return Err(error);
        }
        if self.mirror {
            if let Some(painter) = painter {
                Self::software(painter, operation);
            }
        }
        Ok(())
    }
    pub fn append(&mut self, value: Command) -> Result<()> {
        if value.target.is_empty() || value.scissor.is_empty() {
            return Ok(());
        }
        if value.texture.is_none() && !value.over {
            if let Some(prior) = self.commands.last_mut() {
                if prior.texture.is_none()
                    && !prior.over
                    && prior.layer == value.layer
                    && prior.scissor == prior.target
                    && value.scissor == value.target
                {
                    let (a, b) = (prior.target, value.target);
                    if a == b {
                        *prior = value;
                        self.merged_fills = self.merged_fills.saturating_add(1);
                        return Ok(());
                    }
                    if prior.color == value.color
                        && ((a.y == b.y && a.h == b.h && a.right() == b.x)
                            || (a.x == b.x && a.w == b.w && a.bottom() == b.y))
                    {
                        prior.target = a.merged(b);
                        prior.scissor = prior.target;
                        self.merged_fills = self.merged_fills.saturating_add(1);
                        return Ok(());
                    }
                }
            }
        }
        if self.commands.len() == CAPACITY {
            return Err(Error::Capacity);
        }
        // Join a nearby compatible run only across disjoint destination
        // footprints. Alpha operations that touch the same pixels keep their
        // original order; source rectangles and sampling grids stay intact.
        if let Some(footprint) = intersect(value.target, value.scissor) {
            let start = self.commands.len().saturating_sub(16);
            for index in (start..self.commands.len()).rev() {
                let prior = self.commands[index];
                if prior.layer != value.layer {
                    break;
                }
                if prior.texture == value.texture && prior.over == value.over {
                    self.commands.insert(index + 1, value);
                    return Ok(());
                }
                if intersect(prior.target, prior.scissor)
                    .is_some_and(|area| intersect(area, footprint).is_some())
                {
                    break;
                }
            }
        }
        self.commands.push(value);
        Ok(())
    }
    fn local(&self, rect: Rect) -> Result<Rect> {
        let bounds = self.active.ok_or(Error::State)?.bounds;
        let raster = if let Some(view) = self.view {
            geometry::raster_rect(view, rect)?
        } else {
            rect
        };
        Ok(Rect::new(
            raster.x.checked_sub(bounds.x).ok_or(Error::Bounds)?,
            raster.y.checked_sub(bounds.y).ok_or(Error::Bounds)?,
            raster.w,
            raster.h,
        ))
    }
    fn record(&mut self, operation: Paint<'_>) -> Result<()> {
        let active = self.active.ok_or(Error::State)?;
        match operation {
            Paint::Clear(rect) => {
                let local = self.local(rect)?;
                self.append(Command::fill(active.layer, local, 0))
            }
            Paint::Fill { rect, rgb } => {
                let local = self.local(rect)?;
                self.append(Command::fill(active.layer, local, 0xff000000 | rgb))
            }
            Paint::Picture(picture) => {
                if !picture.view.valid()
                    || picture.guest_w == 0
                    || picture.guest_h == 0
                    || picture.viewport.is_empty()
                {
                    return Err(Error::Invalid);
                }
                if u64::from(picture.source_x) + u64::from(picture.view.width)
                    > u64::from(picture.guest_w)
                    || u64::from(picture.source_y) + u64::from(picture.view.height)
                        > u64::from(picture.guest_h)
                {
                    return Err(Error::Bounds);
                }
                let Some(clipped) = intersect(picture.clip, picture.viewport) else {
                    return Ok(());
                };
                if self.view.is_some() {
                    return self.grid_picture(picture, clipped);
                }
                let whole = picture.source_x == 0
                    && picture.source_y == 0
                    && picture.guest_w == picture.view.width
                    && picture.guest_h == picture.view.height;
                let integer = i64::from(picture.viewport.w) % i64::from(picture.guest_w) == 0
                    && i64::from(picture.viewport.h) % i64::from(picture.guest_h) == 0;
                if !whole && !integer {
                    return self.fractional(picture, clipped);
                }
                let entry = self.assets.intern(picture.view)?;
                let target = if whole {
                    picture.viewport
                } else {
                    let sx = i64::from(picture.viewport.w) / i64::from(picture.guest_w);
                    let sy = i64::from(picture.viewport.h) / i64::from(picture.guest_h);
                    Rect::new(
                        i32::try_from(
                            i64::from(picture.viewport.x) + sx * i64::from(picture.source_x),
                        )
                        .map_err(|_| Error::Bounds)?,
                        i32::try_from(
                            i64::from(picture.viewport.y) + sy * i64::from(picture.source_y),
                        )
                        .map_err(|_| Error::Bounds)?,
                        i32::try_from(sx * i64::from(picture.view.width))
                            .map_err(|_| Error::Bounds)?,
                        i32::try_from(sy * i64::from(picture.view.height))
                            .map_err(|_| Error::Bounds)?,
                    )
                };
                let Some(clip) = intersect(clipped, target) else {
                    return Ok(());
                };
                self.append(Command {
                    layer: active.layer,
                    texture: Some(entry.texture),
                    source: entry.rect,
                    target: self.local(target)?,
                    scissor: self.local(clip)?,
                    color: 0,
                    over: matches!(picture.view.format, Format::Argb | Format::Alpha),
                    grid: GfxSampleGrid::default(),
                })
            }
        }
    }
    fn grid_picture(&mut self, picture: Picture<'_>, clipped: Rect) -> Result<()> {
        let view = self.view.ok_or(Error::State)?;
        let active = self.active.ok_or(Error::State)?;
        let mut asset = picture.view;
        asset.rotation = 0;
        asset.identity.dpi_x = (96 * view.scale + 60) / 120;
        asset.identity.dpi_y = asset.identity.dpi_x;
        let entry = self.assets.intern(asset)?;
        let edge = |source: u64, viewport: i32, guest: u32| {
            (source * viewport as u64).div_ceil(u64::from(guest))
        };
        let x0 = edge(
            u64::from(picture.source_x),
            picture.viewport.w,
            picture.guest_w,
        );
        let y0 = edge(
            u64::from(picture.source_y),
            picture.viewport.h,
            picture.guest_h,
        );
        let x1 = edge(
            u64::from(picture.source_x) + u64::from(picture.view.width),
            picture.viewport.w,
            picture.guest_w,
        );
        let y1 = edge(
            u64::from(picture.source_y) + u64::from(picture.view.height),
            picture.viewport.h,
            picture.guest_h,
        );
        let coverage = Rect::new(
            i32::try_from(i64::from(picture.viewport.x) + x0 as i64).map_err(|_| Error::Bounds)?,
            i32::try_from(i64::from(picture.viewport.y) + y0 as i64).map_err(|_| Error::Bounds)?,
            i32::try_from(x1 - x0).map_err(|_| Error::Bounds)?,
            i32::try_from(y1 - y0).map_err(|_| Error::Bounds)?,
        );
        let Some(clip) = intersect(clipped, coverage) else {
            return Ok(());
        };
        self.append(Command {
            layer: active.layer,
            texture: Some(entry.texture),
            source: entry.rect,
            target: self.local(coverage)?,
            scissor: self.local(clip)?,
            color: 0,
            over: matches!(asset.format, Format::Argb | Format::Alpha),
            grid: GfxSampleGrid {
                enabled: 1,
                rotation: view.rotation as u32,
                scale: view.scale,
                pixel_width: view.pixel_w,
                pixel_height: view.pixel_h,
                target_x: active.bounds.x,
                target_y: active.bounds.y,
                viewport_x: picture
                    .viewport
                    .x
                    .checked_sub(view.origin.x)
                    .ok_or(Error::Bounds)?,
                viewport_y: picture
                    .viewport
                    .y
                    .checked_sub(view.origin.y)
                    .ok_or(Error::Bounds)?,
                viewport_width: picture.viewport.w as u32,
                viewport_height: picture.viewport.h as u32,
                guest_width: picture.guest_w,
                guest_height: picture.guest_h,
                source_x: picture.source_x,
                source_y: picture.source_y,
                ..GfxSampleGrid::default()
            },
        })
    }
    fn fractional(&mut self, picture: Picture<'_>, clip: Rect) -> Result<()> {
        if !matches!(picture.view.format, Format::Xrgb | Format::Indexed) {
            return Err(Error::Invalid);
        }
        let mut pixels = [0; 128 * 128];
        let mut y = clip.y;
        while y < clip.bottom() {
            let height = 128.min(clip.bottom() - y);
            let mut x = clip.x;
            while x < clip.right() {
                let width = 128.min(clip.right() - x);
                for row in 0..height {
                    for column in 0..width {
                        let gx =
                            ((i64::from(x) + i64::from(column) - i64::from(picture.viewport.x))
                                * i64::from(picture.guest_w)
                                / i64::from(picture.viewport.w)) as u64;
                        let gy = ((i64::from(y) + i64::from(row) - i64::from(picture.viewport.y))
                            * i64::from(picture.guest_h)
                            / i64::from(picture.viewport.h))
                            as u64;
                        if gx < u64::from(picture.source_x)
                            || gx >= u64::from(picture.source_x) + u64::from(picture.view.width)
                            || gy < u64::from(picture.source_y)
                            || gy >= u64::from(picture.source_y) + u64::from(picture.view.height)
                        {
                            return Err(Error::Bounds);
                        }
                        pixels[(row * width + column) as usize] = picture.view.pixel(
                            (gx - u64::from(picture.source_x)) as usize,
                            (gy - u64::from(picture.source_y)) as usize,
                        );
                    }
                }
                self.fractional_pixels = self
                    .fractional_pixels
                    .saturating_add(width as u64 * height as u64);
                let rect = Rect::new(x, y, width, height);
                self.record(Paint::Picture(Picture {
                    view: View::new(
                        Format::Xrgb,
                        width as u32,
                        height as u32,
                        width as usize * 4,
                        PixelBytes::from_words(&pixels[..(width * height) as usize]).bytes(),
                    ),
                    viewport: rect,
                    clip: rect,
                    source_x: 0,
                    source_y: 0,
                    guest_w: width as u32,
                    guest_h: height as u32,
                }))?;
                x += width;
            }
            y += height;
        }
        Ok(())
    }
    pub(super) fn software(painter: &mut CpuScene<'_>, operation: Paint<'_>) {
        match operation {
            Paint::Clear(rect) => painter.clear_layer(rect),
            Paint::Fill { rect, rgb } => painter.fill_rect(rect, rgb),
            Paint::Picture(picture) => {
                let view = picture.view;
                let mapping = Mapping {
                    source_x: picture.source_x,
                    source_y: picture.source_y,
                    source_w: view.width,
                    source_h: view.height,
                    source_stride: 0,
                    guest_w: picture.guest_w,
                    guest_h: picture.guest_h,
                    viewport: picture.viewport,
                };
                match view.format {
                    Format::Xrgb => {
                        if let Some(pixels) = PixelBytes::new(view.bytes) {
                            let _ = painter.blit_xrgb32_nearest(
                                picture.clip,
                                Xrgb32Nearest {
                                    pixels,
                                    mapping: Mapping {
                                        source_stride: (view.stride / 4) as u32,
                                        ..mapping
                                    },
                                },
                            );
                        }
                    }
                    Format::Indexed => {
                        let _ = painter.blit_indexed8_nearest(
                            picture.clip,
                            Indexed8Nearest {
                                indices: view.bytes,
                                palette: view.palette,
                                mapping: Mapping {
                                    source_stride: view.stride as u32,
                                    ..mapping
                                },
                            },
                        );
                    }
                    Format::Alpha => {
                        let _ = painter.blend_alpha8(
                            picture.viewport.x,
                            picture.viewport.y,
                            view.width,
                            view.height,
                            view.stride as u32,
                            view.foreground,
                            view.bytes,
                        );
                    }
                    Format::Argb => {
                        let _ = painter.blend_argb32(
                            picture.clip,
                            picture.viewport.x,
                            picture.viewport.y,
                            view.width,
                            view.height,
                            (i64::from(picture.viewport.w) / i64::from(view.width)) as u32,
                            view.bytes,
                        );
                    }
                    Format::Glyph => {
                        let mut pixels = [0; 64 * 64];
                        for row in 0..view.height as usize {
                            for column in 0..view.width as usize {
                                pixels[row * view.width as usize + column] =
                                    view.pixel(column, row);
                            }
                        }
                        let previous = painter.paint_clip;
                        painter.set_paint_clip(picture.clip);
                        painter.blit_xrgb32(
                            picture.viewport.x,
                            picture.viewport.y,
                            view.width,
                            view.height,
                            PixelBytes::from_words(
                                &pixels[..view.width as usize * view.height as usize],
                            ),
                        );
                        painter.paint_clip = previous;
                    }
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::super::content_hash::Blake3;
    use super::*;
    fn digest(frame: &Frame) -> [u8; 32] {
        let mut hash = Blake3::new();
        for c in frame.commands() {
            for v in [
                u32::from(c.layer),
                u32::from(c.texture.unwrap_or(255)),
                c.source.x as u32,
                c.source.y as u32,
                c.source.w as u32,
                c.source.h as u32,
                c.target.x as u32,
                c.target.y as u32,
                c.target.w as u32,
                c.target.h as u32,
                c.scissor.x as u32,
                c.scissor.y as u32,
                c.scissor.w as u32,
                c.scissor.h as u32,
                c.color,
                u32::from(c.over),
            ] {
                hash.update(&v.to_le_bytes());
            }
            let g = c.grid;
            for v in [
                g.enabled,
                g.rotation,
                g.scale,
                g.pixel_width,
                g.pixel_height,
                g.target_x as u32,
                g.target_y as u32,
                g.reserved,
                g.viewport_x as u32,
                g.viewport_y as u32,
                g.viewport_width,
                g.viewport_height,
                g.guest_width,
                g.guest_height,
                g.source_x,
                g.source_y,
            ] {
                hash.update(&v.to_le_bytes());
            }
        }
        hash.finish()
    }
    fn check(frame: &Frame, count: usize, expected: &str) {
        assert_eq!(frame.commands().len(), count);
        let mut bytes = [0; 32];
        for (i, v) in bytes.iter_mut().enumerate() {
            *v = u8::from_str_radix(&expected[i * 2..i * 2 + 2], 16).unwrap();
        }
        assert_eq!(digest(frame), bytes);
    }
    fn pictures(frame: &mut Frame) {
        let mut palette = [0u32; 256];
        palette[0] = 0x773311;
        palette[1] = 0x229955;
        let mut view = View::new(Format::Indexed, 2, 2, 2, &[0, 1, 1, 0]);
        view.palette = PixelBytes::from_words(&palette);
        frame
            .paint(
                None,
                Paint::Picture(Picture {
                    view,
                    viewport: Rect::new(0, 4, 4, 4),
                    clip: Rect::new(0, 4, 4, 4),
                    source_x: 0,
                    source_y: 0,
                    guest_w: 2,
                    guest_h: 2,
                }),
            )
            .unwrap();
        let mut view = View::new(Format::Alpha, 2, 2, 2, &[0, 128, 255, 64]);
        view.foreground = 0x669933;
        frame
            .paint(
                None,
                Paint::Picture(Picture {
                    view,
                    viewport: Rect::new(4, 4, 2, 2),
                    clip: Rect::new(4, 4, 2, 2),
                    source_x: 0,
                    source_y: 0,
                    guest_w: 2,
                    guest_h: 2,
                }),
            )
            .unwrap();
        let argb = [0x80ff0000, 0xff445566];
        let view = View::new(Format::Argb, 2, 1, 8, PixelBytes::from_words(&argb).bytes());
        frame
            .paint(
                None,
                Paint::Picture(Picture {
                    view,
                    viewport: Rect::new(5, 6, 2, 1),
                    clip: Rect::new(5, 6, 2, 1),
                    source_x: 0,
                    source_y: 0,
                    guest_w: 2,
                    guest_h: 1,
                }),
            )
            .unwrap();
    }
    #[test]
    fn exact_original_zig_commands_current_images_four_rotations_and_three_scales() {
        // Four image/fill commands and twelve original8x8 output transforms.
        // Digests come from the unchanged current Zig Frame, not this port.
        let mut frame = Frame::new().unwrap();
        frame.set_mirror(false).unwrap();
        let full = Rect::new(0, 0, 8, 8);
        frame.start(1).unwrap();
        frame.begin(1, full).unwrap();
        frame.paint(None, Paint::Clear(full)).unwrap();
        frame
            .paint(
                None,
                Paint::Fill {
                    rect: full,
                    rgb: 0x203040,
                },
            )
            .unwrap();
        pictures(&mut frame);
        frame.end().unwrap();
        check(
            &frame,
            4,
            "f5ffac30abb8fdd07c75c925928cee7e892df093aa5e5bd180b798546c42b85b",
        );
        assert_eq!(frame.assets.misses, 3);
        assert_eq!(frame.merged_fills, 1);
        let conversions = frame.assets.converted_pixels;
        frame.start(2).unwrap();
        frame.begin(1, full).unwrap();
        frame.paint(None, Paint::Clear(full)).unwrap();
        frame
            .paint(
                None,
                Paint::Fill {
                    rect: full,
                    rgb: 0x203040,
                },
            )
            .unwrap();
        pictures(&mut frame);
        frame.end().unwrap();
        check(
            &frame,
            4,
            "f5ffac30abb8fdd07c75c925928cee7e892df093aa5e5bd180b798546c42b85b",
        );
        assert_eq!(conversions, frame.assets.converted_pixels);
        let expected = [
            "a016f2fc89a6d7d60f890064fe3ff17b063826999fbd134d3f46e02a17ce303f",
            "0571bd05597bb43b709402860d339d01ccb040ef1a847399b31183732343ff56",
            "b71159374cfc5518094e1ebf2391f50534619abc49ba9eb9cb40e91e926eabe6",
            "51388c8e8d8ee3c23ae33c9e9d3b2b126bfa69a1f732b82578c9924d6c0daddf",
            "5a8195a3a051bfaeb71a6ef41fa1c557d00462ec2a3451d658e970692b025bef",
            "d21ec3a14b8098d097ed8a243810cc3f11408b97b82e1fc7f9f3d2ab55652600",
            "a2022c1dda11a6358b7389ff60a5f12f8cf253b6f96920e32df02983b567f205",
            "e099efc689029be807d361dd1400d7d287dc74db9af39d991dde17b12b4ccd52",
            "8189e92b815ec982e3b7470807d8f2c458b26d3f1c644468cb210889de0e2a60",
            "03087cdda1b8910ba45975aaa53fee17cecbe6a5e80fd7e400214619415b059b",
            "23ca06b6d0fb2c975ced1a97584910ab1f5167cac32dccd7e00bb6628de3603f",
            "abf4a8a2376b8d4708815bc5eeb984fd9b3004d6691fff1a5c0f88e081963225",
        ];
        use geometry::topology::{Point, Rotation, Viewport};
        let rotations = [
            Rotation::Normal,
            Rotation::Clockwise90,
            Rotation::Clockwise180,
            Rotation::Clockwise270,
        ];
        let mut index = 0;
        for scale in [120, 240, 180] {
            for rotation in rotations {
                let view = Viewport {
                    pixel_w: 8,
                    pixel_h: 8,
                    scale,
                    origin: Point { x: -31, y: 12 },
                    rotation,
                };
                let bounds = geometry::logical(view).unwrap();
                let mut pixels = [0u32; 64];
                for y in 0..bounds.h as usize {
                    for x in 0..bounds.w as usize {
                        pixels[y * bounds.w as usize + x] =
                            0x102030 + y as u32 * 0x010700 + x as u32 * 0x0b01;
                    }
                }
                frame.set_view(Some(view)).unwrap();
                frame.start(3 + index as u64).unwrap();
                frame.begin(1, bounds).unwrap();
                frame
                    .paint(
                        None,
                        Paint::Picture(Picture {
                            view: View::new(
                                Format::Xrgb,
                                bounds.w as u32,
                                bounds.h as u32,
                                bounds.w as usize * 4,
                                PixelBytes::from_words(&pixels[..(bounds.w * bounds.h) as usize])
                                    .bytes(),
                            ),
                            viewport: bounds,
                            clip: bounds,
                            source_x: 0,
                            source_y: 0,
                            guest_w: bounds.w as u32,
                            guest_h: bounds.h as u32,
                        }),
                    )
                    .unwrap();
                frame.end().unwrap();
                check(&frame, 1, expected[index]);
                index += 1;
            }
        }
        frame.set_view(None).unwrap();
        frame.start(15).unwrap();
        frame.begin(1, full).unwrap();
        let chunk = [0x123456, 0x654321, 0x204060, 0x6080a0];
        frame
            .paint(
                None,
                Paint::Picture(Picture {
                    view: View::new(
                        Format::Xrgb,
                        2,
                        2,
                        8,
                        PixelBytes::from_words(&chunk).bytes(),
                    ),
                    viewport: Rect::new(0, 0, 7, 7),
                    clip: Rect::new(2, 2, 4, 4),
                    source_x: 1,
                    source_y: 1,
                    guest_w: 4,
                    guest_h: 4,
                }),
            )
            .unwrap();
        frame.end().unwrap();
        check(
            &frame,
            1,
            "bdfff5334f0c5bfad4dad1ae9fc59db58be631a4a09447eaaf9c2d0a38cbe0a4",
        );
        assert_eq!(frame.fractional_pixels, 16);
    }
}
