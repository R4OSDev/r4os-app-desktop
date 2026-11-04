//! Bounded paint grammar and damage-local gradient rasterization. Coordinates
//! always refer to the original command, so dirty replay keeps identical colors.
use super::{format::Point, mask::Area};
use crate::render::{
    float::sqrt,
    geometry::Rect,
    painter::Painter,
    primitive_assets::{Error, Result},
    primitive_image::{Format, Picture, View},
};
use r4os_contract as a;

#[derive(Clone, Copy)]
pub(super) struct Paint<'a> {
    kind: u32,
    start: Point,
    end: Point,
    stops: &'a [u8],
}
fn word(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}
pub(super) fn parse(bytes: &[u8]) -> Option<Paint<'_>> {
    if bytes.len() < core::mem::size_of::<a::GuiGradientPaint>() {
        return None;
    }
    let h = unsafe { core::ptr::read_unaligned(bytes.as_ptr().cast::<a::GuiGradientPaint>()) };
    if h.version != a::gui_gradient_resource_version
        || h.size as usize != core::mem::size_of::<a::GuiGradientPaint>()
        || !(2..=a::gui_gradient_max_stops).contains(&h.stop_count)
        || bytes.len()
            != h.size as usize + h.stop_count as usize * core::mem::size_of::<a::GuiGradientStop>()
    {
        return None;
    }
    let start = Point {
        x: super::format::coordinate(h.start_x_bits)?,
        y: super::format::coordinate(h.start_y_bits)?,
    };
    let end = Point {
        x: super::format::coordinate(h.end_x_bits)?,
        y: super::format::coordinate(h.end_y_bits)?,
    };
    let minimum = 1.0 / 256.0;
    match h.kind {
        a::gui_gradient_kind_linear => {
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            if dx * dx + dy * dy < minimum * minimum {
                return None;
            }
        }
        a::gui_gradient_kind_radial => {
            if end.x < minimum || end.y < minimum {
                return None;
            }
        }
        _ => return None,
    }
    let stops = &bytes[h.size as usize..];
    let mut previous = -1.0;
    for i in 0..h.stop_count as usize {
        let t = f32::from_bits(word(stops, i * 8)?);
        if !t.is_finite()
            || !(0.0..=1.0).contains(&t)
            || t <= previous
            || (i == 0 && t != 0.0)
            || (i + 1 == h.stop_count as usize && t != 1.0)
        {
            return None;
        }
        previous = t;
    }
    Some(Paint {
        kind: h.kind,
        start,
        end,
        stops,
    })
}
impl Paint<'_> {
    fn stop(self, i: usize) -> (f32, u32) {
        (
            f32::from_bits(word(self.stops, i * 8).expect("admitted stop")),
            word(self.stops, i * 8 + 4).expect("admitted color"),
        )
    }
    pub(super) fn color(self, x: f32, y: f32) -> u32 {
        let t = if self.kind == a::gui_gradient_kind_linear {
            let dx = self.end.x - self.start.x;
            let dy = self.end.y - self.start.y;
            ((x - self.start.x) * dx + (y - self.start.y) * dy) / (dx * dx + dy * dy)
        } else {
            let dx = (x - self.start.x) / self.end.x;
            let dy = (y - self.start.y) / self.end.y;
            sqrt(dx * dx + dy * dy)
        }
        .clamp(0.0, 1.0);
        let (mut left, mut c1) = self.stop(0);
        for i in 1..self.stops.len() / 8 {
            let (right, c2) = self.stop(i);
            if t <= right {
                let ratio = ((t - left) / (right - left)).clamp(0.0, 1.0);
                let a1 = (c1 >> 24) as f32;
                let a2 = (c2 >> 24) as f32;
                let alpha = a1 + (a2 - a1) * ratio;
                if alpha <= 0.0 {
                    return 0;
                }
                let mut color = ((alpha + 0.5) as u32).min(255) << 24;
                for shift in [0, 8, 16] {
                    let p1 = ((c1 >> shift) & 255) as f32 * a1;
                    let p2 = ((c2 >> shift) & 255) as f32 * a2;
                    color |= (((p1 + (p2 - p1) * ratio) / alpha + 0.5) as u32).min(255) << shift;
                }
                return color;
            }
            left = right;
            c1 = c2;
        }
        c1
    }
}
pub(super) fn blend(
    painter: &mut Painter<'_, '_>,
    bounds: Rect,
    command: &a::GuiFrameCommand,
    area: Area,
    mask: &[u8],
    paint: Paint<'_>,
) -> Result<bool> {
    let mut bytes = super::allocate(mask.len().checked_mul(4).ok_or(Error::OutOfMemory)?)?;
    for y in 0..area.h {
        for x in 0..area.w {
            let i = y * area.w + x;
            let color = paint.color((x + area.x) as f32 + 0.5, (y + area.y) as f32 + 0.5);
            let alpha = ((color >> 24) * u32::from(mask[i]) + 127) / 255;
            bytes[i * 4..i * 4 + 4]
                .copy_from_slice(&((color & 0xffffff) | (alpha << 24)).to_le_bytes());
        }
    }
    let x = i64::from(bounds.x) + i64::from(command.x) + area.x as i64;
    let y = i64::from(bounds.y) + i64::from(command.y) + area.y as i64;
    let rect = Rect::new(
        i32::try_from(x).map_err(|_| Error::Invalid)?,
        i32::try_from(y).map_err(|_| Error::Invalid)?,
        area.w as i32,
        area.h as i32,
    );
    painter.picture(Picture {
        view: View::new(
            Format::Argb,
            area.w as u32,
            area.h as u32,
            area.w * 4,
            &bytes,
        ),
        viewport: rect,
        clip: rect,
        source_x: 0,
        source_y: 0,
        guest_w: area.w as u32,
        guest_h: area.h as u32,
    })?;
    Ok(true)
}
