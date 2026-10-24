//! Damage-bounded coverage and the original separable shadow blur. The halo
//! retains identical edge samples when only a small dirty area is replayed.
use super::format::{Parsed, Point};
use crate::render::{
    geometry::Rect,
    painter::Painter,
    primitive_assets::Result,
    primitive_image::{Format, Picture, View},
};
use r4os_contract::GuiFrameCommand;
#[derive(Clone, Copy)]
pub(super) struct Area {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}
pub(super) fn area(
    clip: Rect,
    bounds: Rect,
    command: &GuiFrameCommand,
    radius: usize,
) -> Option<Area> {
    let ox = i64::from(bounds.x) + i64::from(command.x);
    let oy = i64::from(bounds.y) + i64::from(command.y);
    let left = i64::from(bounds.x).max(i64::from(clip.x)).max(ox);
    let top = i64::from(bounds.y).max(i64::from(clip.y)).max(oy);
    let right = i64::from(bounds.right())
        .min(i64::from(clip.right()))
        .min(ox + i64::from(command.w));
    let bottom = i64::from(bounds.bottom())
        .min(i64::from(clip.bottom()))
        .min(oy + i64::from(command.h));
    if clip.is_empty() || right <= left || bottom <= top {
        return None;
    }
    let halo = radius as i64;
    let x = (left - ox - halo).max(0) as usize;
    let y = (top - oy - halo).max(0) as usize;
    let ex = i64::from(command.w).min(right - ox + halo) as usize;
    let ey = i64::from(command.h).min(bottom - oy + halo) as usize;
    Some(Area {
        x,
        y,
        w: ex - x,
        h: ey - y,
    })
}
pub(super) fn coverage(
    parsed: Parsed<'_>,
    x: usize,
    y: usize,
    contains: impl Fn(Point) -> bool,
) -> u8 {
    let mut covered = 0u32;
    for (dx, dy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
        let point = Point {
            x: x as f32 + dx,
            y: y as f32 + dy,
        };
        if parsed.clip.contains(point) && contains(point) {
            covered += 1;
        }
    }
    ((covered * 255 + 2) / 4) as u8
}
pub(super) fn fill(
    mask: &mut [u8],
    area: Area,
    parsed: Parsed<'_>,
    contains: impl Fn(Point) -> bool,
) {
    for y in 0..area.h {
        for x in 0..area.w {
            mask[y * area.w + x] = coverage(parsed, x + area.x, y + area.y, &contains);
        }
    }
}
pub(super) fn blend(
    painter: &mut Painter<'_, '_>,
    bounds: Rect,
    command: &GuiFrameCommand,
    area: Area,
    mask: &mut [u8],
    argb: u32,
) -> Result<bool> {
    let alpha = argb >> 24;
    if alpha == 0 {
        return Ok(false);
    }
    if alpha != 255 {
        for value in mask.iter_mut() {
            *value = ((u32::from(*value) * alpha + 127) / 255) as u8;
        }
    }
    let dx = i64::from(bounds.x) + i64::from(command.x) + area.x as i64;
    let dy = i64::from(bounds.y) + i64::from(command.y) + area.y as i64;
    let left = dx.max(i64::from(bounds.x));
    let top = dy.max(i64::from(bounds.y));
    let right = (dx + area.w as i64).min(i64::from(bounds.right()));
    let bottom = (dy + area.h as i64).min(i64::from(bounds.bottom()));
    if right <= left
        || bottom <= top
        || left < i64::from(i32::MIN)
        || top < i64::from(i32::MIN)
        || right > i64::from(i32::MAX)
        || bottom > i64::from(i32::MAX)
    {
        return Ok(false);
    }
    let sx = (left - dx) as usize;
    let sy = (top - dy) as usize;
    let offset = sy * area.w + sx;
    let w = (right - left) as u32;
    let h = (bottom - top) as u32;
    let mut view = View::new(Format::Alpha, w, h, area.w, &mask[offset..]);
    view.foreground = argb & 0xffffff;
    let rect = Rect::new(left as i32, top as i32, w as i32, h as i32);
    painter.picture(Picture {
        view,
        viewport: rect,
        clip: rect,
        source_x: 0,
        source_y: 0,
        guest_w: w,
        guest_h: h,
    })?;
    Ok(true)
}
pub(super) fn blur_horizontal(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let divisor = (r * 2 + 1) as u32;
    for y in 0..h {
        let mut sum = 0u32;
        for initial in 0..(r + 1).min(w) {
            sum += u32::from(src[y * w + initial]);
        }
        for x in 0..w {
            dst[y * w + x] = ((sum + divisor / 2) / divisor) as u8;
            if x >= r {
                sum -= u32::from(src[y * w + x - r]);
            }
            let add = x + r + 1;
            if add < w {
                sum += u32::from(src[y * w + add]);
            }
        }
    }
}
pub(super) fn blur_vertical(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let divisor = (r * 2 + 1) as u32;
    for x in 0..w {
        let mut sum = 0u32;
        for initial in 0..(r + 1).min(h) {
            sum += u32::from(src[initial * w + x]);
        }
        for y in 0..h {
            dst[y * w + x] = ((sum + divisor / 2) / divisor) as u8;
            if y >= r {
                sum -= u32::from(src[(y - r) * w + x]);
            }
            let add = y + r + 1;
            if add < h {
                sum += u32::from(src[add * w + x]);
            }
        }
    }
}
