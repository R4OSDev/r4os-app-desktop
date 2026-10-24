//! Rust owner of the current GUI shape renderer. Payload validation, curve
//! expansion, coverage and clipping are separate; no old renderer callback.
mod format;
mod geometry;
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
}
impl Default for Renderer {
    fn default() -> Self {
        Self {
            mask_limit: usize::MAX,
        }
    }
}
fn allocate(count: usize) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    data.try_reserve_exact(count)
        .map_err(|_| Error::OutOfMemory)?;
    data.resize(count, 0);
    Ok(data)
}
impl Renderer {
    pub fn paint(
        &mut self,
        painter: &mut Painter<'_, '_>,
        bounds: Rect,
        command: &a::GuiFrameCommand,
        resource: &[u8],
    ) -> Result<bool> {
        let p = format::parse(command, resource).ok_or(Error::Invalid)?;
        let radius = if command.kind == a::gui_frame_command_kind_shadow {
            ceil(p.shadow_blur) as usize
        } else {
            0
        };
        let Some(area) = mask::area(painter.clip(), bounds, command, radius) else {
            return Ok(false);
        };
        let geometry = if p.geometry_kind == a::gui_shape_geometry_kind_path {
            Some(Geometry::build(p)?)
        } else {
            None
        };
        let g = geometry.as_ref();
        let count = area.w.checked_mul(area.h).ok_or(Error::OutOfMemory)?;
        if count > self.mask_limit {
            return Err(Error::OutOfMemory);
        }
        let mut pixels = allocate(count)?;
        match command.kind {
            a::gui_frame_command_kind_path_fill => {
                mask::fill(&mut pixels, area, p, |point| {
                    g.expect("admitted geometry").inside(point, p.fill_rule)
                });
                mask::blend(painter, bounds, command, area, &mut pixels, p.fill_argb)
            }
            a::gui_frame_command_kind_path_stroke => {
                mask::fill(&mut pixels, area, p, |point| {
                    g.expect("admitted geometry").stroke(
                        point,
                        p.stroke_width,
                        p.line_join.expect("admitted join"),
                        p.line_cap.expect("admitted cap"),
                        p.miter_limit,
                    )
                });
                mask::blend(painter, bounds, command, area, &mut pixels, p.stroke_argb)
            }
            a::gui_frame_command_kind_rounded_rect => {
                let outer = geometry::normalize(p.rounded);
                mask::fill(&mut pixels, area, p, |point| {
                    geometry::rounded(outer, point)
                });
                let drew = mask::blend(painter, bounds, command, area, &mut pixels, p.fill_argb)?;
                let inner = geometry::inner(p);
                mask::fill(&mut pixels, area, p, |point| {
                    geometry::rounded(outer, point)
                        && inner.is_none_or(|inner| !geometry::rounded(inner, point))
                });
                Ok(
                    mask::blend(painter, bounds, command, area, &mut pixels, p.stroke_argb)?
                        || drew,
                )
            }
            a::gui_frame_command_kind_shadow => {
                mask::fill(&mut pixels, area, p, |point| geometry::shadow(p, g, point));
                if radius != 0 {
                    if count > self.mask_limit.saturating_sub(count) {
                        return Err(Error::OutOfMemory);
                    }
                    let mut scratch = allocate(count)?;
                    mask::blur_horizontal(&pixels, &mut scratch, area.w, area.h, radius);
                    mask::blur_vertical(&scratch, &mut pixels, area.w, area.h, radius);
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
                mask::blend(painter, bounds, command, area, &mut pixels, p.shadow_argb)
            }
            _ => Err(Error::Invalid),
        }
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
