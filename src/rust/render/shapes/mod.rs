//! Rust owner of the current GUI shape renderer. Payload validation, curve
//! expansion, coverage and clipping are separate; no old renderer callback.
mod format;
mod geometry;
mod gradient;
mod mask;
#[cfg(any(test, feature = "render-probe"))]
#[path = "../../../../Tests/RustNative/shapes.rs"]
mod tests;
#[cfg(feature = "render-probe")]
pub(crate) fn conformance_probe() {
    tests::conformance_probe();
}
use crate::{
    hosted::replay::Shapes,
    render::{
        float::ceil,
        geometry::Rect,
        painter::Painter,
        primitive_assets::{Error, Result},
    },
};
use alloc::vec::Vec;
use geometry::Geometry;
use r4os_contract as a;
pub struct Renderer {
    mask_limit: usize,
    pixels: Vec<u8>,
    scratch: Vec<u8>,
}
impl Default for Renderer {
    fn default() -> Self {
        Self {
            mask_limit: usize::MAX,
            pixels: Vec::new(),
            scratch: Vec::new(),
        }
    }
}
fn resize_buffer(data: &mut Vec<u8>, count: usize) -> Result<()> {
    data.clear();
    data.try_reserve_exact(count)
        .map_err(|_| Error::OutOfMemory)?;
    data.resize(count, 0);
    Ok(())
}
impl Renderer {
    pub fn paint(
        &mut self,
        painter: &mut Painter<'_, '_>,
        bounds: Rect,
        command: &a::GuiFrameCommand,
        resource: &[u8],
    ) -> Result<bool> {
        let result = self.paint_inner(painter, bounds, command, resource);
        // Retention is independent of admission: larger legal draws still
        // work, but do not pin their peak scratch allocation for the session.
        let retained = self.mask_limit.min(1024 * 1024);
        for data in [&mut self.pixels, &mut self.scratch] {
            data.clear();
            if data.capacity() > retained {
                *data = Vec::new();
            }
        }
        result
    }
    fn paint_inner(
        &mut self,
        painter: &mut Painter<'_, '_>,
        bounds: Rect,
        command: &a::GuiFrameCommand,
        resource: &[u8],
    ) -> Result<bool> {
        let p = format::parse(command, resource).ok_or(Error::Invalid)?;
        if command.kind == a::gui_frame_command_kind_shadow
            && p.geometry_kind == a::gui_shape_geometry_kind_rounded_rect
            && p.shadow_argb >> 24 == 0
        {
            return Ok(false);
        }
        let radius = if command.kind == a::gui_frame_command_kind_shadow {
            ceil(p.shadow_blur) as usize
        } else {
            0
        };
        let Some(area) = mask::area(painter.clip(), bounds, command, radius) else {
            return Ok(false);
        };
        if let Some(result) = self.rounded_panel(painter, bounds, command, p) {
            return result;
        }
        self.paint_raster(painter, bounds, command, p, area, radius)
    }
    fn paint_raster(
        &mut self,
        painter: &mut Painter<'_, '_>,
        bounds: Rect,
        command: &a::GuiFrameCommand,
        p: format::Parsed<'_>,
        area: mask::Area,
        radius: usize,
    ) -> Result<bool> {
        let geometry = if p.geometry_kind == a::gui_shape_geometry_kind_path {
            Some(Geometry::build(p)?)
        } else {
            None
        };
        let g = geometry.as_ref();
        let count = area.w.checked_mul(area.h).ok_or(Error::OutOfMemory)?;
        let scratch_bytes = count
            .checked_mul(if p.gradient.is_some() { 5 } else { 1 })
            .ok_or(Error::OutOfMemory)?;
        if scratch_bytes > self.mask_limit {
            return Err(Error::OutOfMemory);
        }
        resize_buffer(&mut self.pixels, count)?;
        let pixels = self.pixels.as_mut_slice();
        match command.kind {
            a::gui_frame_command_kind_path_fill => {
                mask::fill(pixels, area, p, |point| {
                    g.expect("admitted geometry").inside(point, p.fill_rule)
                });
                mask::blend(painter, bounds, command, area, pixels, p.fill_argb)
            }
            a::gui_frame_command_kind_path_stroke => {
                mask::fill(pixels, area, p, |point| {
                    g.expect("admitted geometry").stroke(
                        point,
                        p.stroke_width,
                        p.line_join.expect("admitted join"),
                        p.line_cap.expect("admitted cap"),
                        p.miter_limit,
                    )
                });
                mask::blend(painter, bounds, command, area, pixels, p.stroke_argb)
            }
            a::gui_frame_command_kind_rounded_rect
            | a::gui_frame_command_kind_gradient_rounded_rect => {
                // The original path normalized the outer shape, then again
                // inside rounded(). Keep both f32 operations, once per command.
                let outer = geometry::NormalizedRounded::new(geometry::normalize(p.rounded));
                let drew = if p.gradient.is_some() || p.fill_argb >> 24 != 0 {
                    mask::fill(pixels, area, p, |point| outer.contains(point));
                    if let Some(paint) = p.gradient {
                        gradient::blend(
                            painter,
                            bounds,
                            command,
                            area,
                            pixels,
                            paint,
                            &mut self.scratch,
                        )?
                    } else {
                        mask::blend(painter, bounds, command, area, pixels, p.fill_argb)?
                    }
                } else {
                    false
                };
                if p.stroke_argb >> 24 == 0 {
                    return Ok(drew);
                }
                let inner = geometry::inner(p).map(geometry::NormalizedRounded::new);
                mask::fill(pixels, area, p, |point| {
                    outer.contains(point) && inner.is_none_or(|inner| !inner.contains(point))
                });
                Ok(mask::blend(painter, bounds, command, area, pixels, p.stroke_argb)? || drew)
            }
            a::gui_frame_command_kind_shadow => {
                mask::fill(pixels, area, p, |point| geometry::shadow(p, g, point));
                if radius != 0 {
                    if count > self.mask_limit.saturating_sub(count) {
                        return Err(Error::OutOfMemory);
                    }
                    resize_buffer(&mut self.scratch, count)?;
                    mask::blur_horizontal(pixels, &mut self.scratch, area.w, area.h, radius);
                    mask::blur_vertical(&self.scratch, pixels, area.w, area.h, radius);
                }
                if p.flags & a::gui_shape_flag_shadow_inset != 0 {
                    for y in 0..area.h {
                        for x in 0..area.w {
                            let original = mask::coverage(p, x + area.x, y + area.y, |point| {
                                geometry::contains(p, g, point)
                            });
                            let index = y * area.w + x;
                            pixels[index] =
                                ((u32::from(original) * (255 - u32::from(pixels[index])) + 127)
                                    / 255) as u8;
                        }
                    }
                }
                mask::blend(painter, bounds, command, area, pixels, p.shadow_argb)
            }
            _ => Err(Error::Invalid),
        }
    }
    /// Exact opaque integer panels: rasterize four bounded corners, reuse
    /// their atlas tiles, and emit the five fully covered rectangles directly.
    /// Complex/large-radius/translucent/gradient/stroked cases retain full masks.
    fn rounded_panel(
        &mut self,
        painter: &mut Painter<'_, '_>,
        bounds: Rect,
        command: &a::GuiFrameCommand,
        p: format::Parsed<'_>,
    ) -> Option<Result<bool>> {
        if command.kind != a::gui_frame_command_kind_rounded_rect
            || p.gradient.is_some()
            || p.fill_argb >> 24 != 255
            || p.stroke_argb >> 24 != 0
            || p.rounded.x != 0.0
            || p.rounded.y != 0.0
            || p.rounded.w != command.w as f32
            || p.rounded.h != command.h as f32
            || p.clip.x > 0.0
            || p.clip.y > 0.0
            || p.clip.right < command.w as f32
            || p.clip.bottom < command.h as f32
            || !painter.unit_grid()
        {
            return None;
        }
        let normalized = geometry::normalize(geometry::normalize(p.rounded));
        let r = normalized.radii.tl.x;
        let radii = [
            normalized.radii.tl,
            normalized.radii.tr,
            normalized.radii.br,
            normalized.radii.bl,
        ];
        if r < 0.0
            || r > 64.0
            || r != (r as u32) as f32
            || radii.iter().any(|p| p.x != r || p.y != r)
        {
            return None;
        }
        let r = r as usize;
        let area = mask::area(painter.clip(), bounds, command, 0)?;
        if area.w * area.h <= 128 * 128
            || (command.w as usize) < 2 * r
            || (command.h as usize) < 2 * r
        {
            return None;
        }
        let w = command.w as usize;
        let h = command.h as usize;
        let xs = [0, r, w - r, w];
        let ys = [0, r, h - r, h];
        let mut corners = [None; 4];
        let mut required = 0;
        for (i, (col, row)) in [(0, 0), (2, 0), (2, 2), (0, 2)].into_iter().enumerate() {
            let left = area.x.max(xs[col]);
            let top = area.y.max(ys[row]);
            let right = (area.x + area.w).min(xs[col + 1]);
            let bottom = (area.y + area.h).min(ys[row + 1]);
            if right > left && bottom > top {
                let a = mask::Area {
                    x: left,
                    y: top,
                    w: right - left,
                    h: bottom - top,
                };
                required = required.max(a.w * a.h);
                corners[i] = Some(a);
            }
        }
        if required > self.mask_limit {
            return None;
        }
        Some((|| {
            // Reserve before any drawing so allocation refusal cannot expose
            // a partly painted successful shape. Later resizes only reuse it.
            resize_buffer(&mut self.pixels, required)?;
            for (col, row) in [(1, 0), (0, 1), (1, 1), (2, 1), (1, 2)] {
                let x = xs[col];
                let y = ys[row];
                let width = xs[col + 1] - x;
                let height = ys[row + 1] - y;
                if width == 0 || height == 0 {
                    continue;
                }
                let ox = i64::from(bounds.x) + i64::from(command.x) + x as i64;
                let oy = i64::from(bounds.y) + i64::from(command.y) + y as i64;
                // Off-screen partitions can lie outside i32 even though the
                // admitted visible subset is valid. Intersect in wide space.
                let left = ox.max(i64::from(painter.clip().x)).max(i64::from(bounds.x));
                let top = oy.max(i64::from(painter.clip().y)).max(i64::from(bounds.y));
                let right = (ox + width as i64)
                    .min(i64::from(painter.clip().right()))
                    .min(i64::from(bounds.right()));
                let bottom = (oy + height as i64)
                    .min(i64::from(painter.clip().bottom()))
                    .min(i64::from(bounds.bottom()));
                if right > left && bottom > top {
                    painter.fill(
                        Rect::new(
                            left as i32,
                            top as i32,
                            (right - left) as i32,
                            (bottom - top) as i32,
                        ),
                        p.fill_argb & 0xffffff,
                    )?;
                }
            }
            let outer = geometry::NormalizedRounded::new(geometry::normalize(p.rounded));
            for area in corners.into_iter().flatten() {
                resize_buffer(&mut self.pixels, area.w * area.h)?;
                mask::fill(&mut self.pixels, area, p, |point| outer.contains(point));
                mask::blend(
                    painter,
                    bounds,
                    command,
                    area,
                    &mut self.pixels,
                    p.fill_argb,
                )?;
            }
            Ok(true)
        })())
    }
}
impl Shapes for Renderer {
    fn replay(
        &mut self,
        painter: &mut Painter<'_, '_>,
        bounds: Rect,
        command: &a::GuiFrameCommand,
        resource: &[u8],
    ) -> Result<()> {
        self.paint(painter, bounds, command, resource).map(|_| ())
    }
}
