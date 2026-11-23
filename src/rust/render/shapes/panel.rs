//! On the CPU, an integer rounded panel has a rectangular interior and four
//! small antialiased corners. The corners use the ordinary coverage/blend path.
use super::{Renderer, format::Parsed, mask::Area, resize_buffer};
use crate::render::{
    geometry::Rect,
    painter::Painter,
    pixels::PixelBytes,
    primitive_assets::{Error, Result},
    scene::intersect,
};
use r4os_contract as a;

fn small_integer(value: f32) -> Option<i32> {
    let integer = value as i32;
    ((0..=64).contains(&integer) && integer as f32 == value).then_some(integer)
}

// Four quarter-pixel samples, matching mask::coverage. Small circular
// corners repeat across controls. Keep their coverage counts in read-only
// storage, independent of width, color and paint generation.
const CORNER_LIMIT: usize = 16;
const CORNER_COUNTS: [[[u8; CORNER_LIMIT]; CORNER_LIMIT]; CORNER_LIMIT + 1] = {
    let mut counts = [[[0; CORNER_LIMIT]; CORNER_LIMIT]; CORNER_LIMIT + 1];
    let mut radius = 1;
    while radius <= CORNER_LIMIT {
        let extent = (radius * 4) as i32;
        let mut y = 0;
        while y < radius {
            let mut x = 0;
            while x < radius {
                let mut sample = 0;
                while sample < 4 {
                    let dx = extent - (x * 4 + 1 + 2 * (sample % 2)) as i32;
                    let dy = extent - (y * 4 + 1 + 2 * (sample / 2)) as i32;
                    counts[radius][y][x] += (dx * dx + dy * dy <= extent * extent) as u8;
                    sample += 1;
                }
                x += 1;
            }
            y += 1;
        }
        radius += 1;
    }
    counts
};

fn samples(radius: i32, x: i32, y: i32) -> u8 {
    if x < 0 || y < 0 {
        0
    } else if x < radius && y < radius {
        CORNER_COUNTS[radius as usize][y as usize][x as usize]
    } else {
        4
    }
}

fn corner_mask(mask: &mut [u8], area: Area, w: i32, h: i32, radius: i32, border: Option<i32>) {
    for (row, target) in mask.chunks_exact_mut(area.w).enumerate() {
        let y = (area.y + row) as i32;
        let y = y.min(h - 1 - y);
        for (column, value) in target.iter_mut().enumerate() {
            let x = (area.x + column) as i32;
            let x = x.min(w - 1 - x);
            let mut count = samples(radius, x, y);
            if let Some(inset) = border {
                count -= samples((radius - inset).max(0), x - inset, y - inset);
            }
            *value = ((u16::from(count) * 255 + 2) / 4) as u8;
        }
    }
}

pub(super) fn paint(
    renderer: &mut Renderer,
    painter: &mut Painter<'_, '_>,
    bounds: Rect,
    cmd: &a::GuiFrameCommand,
    p: Parsed<'_>,
    area: Area,
) -> Option<Result<bool>> {
    if painter.records_primitives()
        || !matches!(
            cmd.kind,
            a::gui_frame_command_kind_rounded_rect
                | a::gui_frame_command_kind_gradient_rounded_rect
        )
        || p.rounded.x != 0.0
        || p.rounded.y != 0.0
        || p.rounded.w != cmd.w as f32
        || p.rounded.h != cmd.h as f32
        || p.clip.x > 0.0
        || p.clip.y > 0.0
        || p.clip.right < p.rounded.w
        || p.clip.bottom < p.rounded.h
        || !matches!(p.fill_argb >> 24, 0 | 255)
        || !matches!(p.stroke_argb >> 24, 0 | 255)
        || p.gradient.is_some_and(|g| !g.horizontal_opaque())
    {
        return None;
    }
    let radius = small_integer(p.rounded.radii.tl.x)?;
    if [
        p.rounded.radii.tl,
        p.rounded.radii.tr,
        p.rounded.radii.br,
        p.rounded.radii.bl,
    ]
    .iter()
    .any(|r| r.x != radius as f32 || r.y != radius as f32)
    {
        return None;
    }
    let border = if p.stroke_argb >> 24 == 0 {
        0
    } else {
        let width = small_integer(p.border_top)?;
        if [p.border_left, p.border_right, p.border_bottom]
            .iter()
            .any(|v| *v != p.border_top)
        {
            return None;
        }
        width
    };
    let edge = radius.max(border);
    let (w, h) = (cmd.w as i32, cmd.h as i32);
    if w < 2 * edge
        || h < 2 * edge
        || area.w * area.h < 256
        || area.w * area.h * (if p.gradient.is_some() { 5 } else { 1 }) > renderer.mask_limit
    {
        return None;
    }
    // Work in command-local coordinates until the visible intersection is known.
    let visible = Rect::new(area.x as i32, area.y as i32, area.w as i32, area.h as i32);
    let origin = (
        i64::from(bounds.x) + i64::from(cmd.x),
        i64::from(bounds.y) + i64::from(cmd.y),
    );
    let screen = |r: Rect| {
        Rect::new(
            (origin.0 + i64::from(r.x)) as i32,
            (origin.1 + i64::from(r.y)) as i32,
            r.w,
            r.h,
        )
    };
    let inner = Rect::new(border, border, w - 2 * border, h - 2 * border);
    Some((|| {
        let corners = [
            Rect::new(0, 0, edge, edge),
            Rect::new(w - edge, 0, edge, edge),
            Rect::new(0, h - edge, edge, edge),
            Rect::new(w - edge, h - edge, edge, edge),
        ];
        let max_corner = corners
            .iter()
            .filter_map(|r| intersect(visible, *r))
            .map(|r| (r.w * r.h) as usize)
            .max()
            .unwrap_or(0);
        resize_buffer(&mut renderer.pixels, max_corner)?;
        if p.gradient.is_some() {
            resize_buffer(&mut renderer.scratch, 4 * area.w.max(max_corner))?;
        }
        for r in corners.iter().filter_map(|r| intersect(visible, *r)) {
            let corner = Area {
                x: r.x as usize,
                y: r.y as usize,
                w: r.w as usize,
                h: r.h as usize,
            };
            // Integer panel geometry and the complete command clip were
            // checked above. Bounds keep quarter samples exactly representable.
            if radius <= CORNER_LIMIT as i32 && w <= 65536 && h <= 65536 {
                let mask = &mut renderer.pixels[..corner.w * corner.h];
                if p.fill_argb >> 24 != 0 || p.gradient.is_some() {
                    corner_mask(mask, corner, w, h, radius, None);
                    if let Some(paint) = p.gradient {
                        super::gradient::blend(painter, bounds, cmd, corner, mask, paint, &mut renderer.scratch)?;
                    } else {
                        super::mask::blend(painter, bounds, cmd, corner, mask, p.fill_argb)?;
                    }
                }
                if border != 0 {
                    corner_mask(mask, corner, w, h, radius, Some(border));
                    super::mask::blend(painter, bounds, cmd, corner, mask, p.stroke_argb)?;
                }
            } else {
                renderer.paint_raster(painter, bounds, cmd, p, corner, 0)?;
            }
        }
        // The horizontal gradient has exactly the same color on each row.
        let row = if let Some(gradient) = p.gradient {
            let bytes = &mut renderer.scratch;
            resize_buffer(bytes, area.w * 4)?;
            for (x, pixel) in bytes.chunks_exact_mut(4).enumerate() {
                let color = gradient.color((area.x + x) as f32 + 0.5, area.y as f32 + 0.5);
                pixel.copy_from_slice(&(color & 0xffffff).to_le_bytes());
            }
            Some(&bytes[..])
        } else {
            None
        };
        let strips = [
            Rect::new(edge, 0, w - 2 * edge, edge),
            Rect::new(0, edge, w, h - 2 * edge),
            Rect::new(edge, h - edge, w - 2 * edge, edge),
        ];
        for strip in strips.iter().filter_map(|r| intersect(visible, *r)) {
            if let Some(body) = intersect(strip, inner) {
                let raster = screen(body);
                if let Some(bytes) = row {
                    let offset = (body.x - visible.x) as usize * 4;
                    let pixels = PixelBytes::new(&bytes[offset..offset + body.w as usize * 4])
                        .ok_or(Error::Invalid)?;
                    for y in raster.y..raster.bottom() {
                        painter.xrgb(raster.x, y, raster.w as u32, 1, pixels)?;
                    }
                } else if p.fill_argb >> 24 == 255 {
                    painter.fill(raster, p.fill_argb)?;
                }
            }
        }
        if border != 0 {
            let borders = [
                Rect::new(edge, 0, w - 2 * edge, border),
                Rect::new(edge, h - border, w - 2 * edge, border),
                Rect::new(0, edge, border, h - 2 * edge),
                Rect::new(w - border, edge, border, h - 2 * edge),
            ];
            for r in borders.iter().filter_map(|r| intersect(visible, *r)) {
                painter.fill(screen(r), p.stroke_argb)?;
            }
        }
        Ok(p.gradient.is_some() || p.fill_argb >> 24 != 0 || p.stroke_argb >> 24 != 0)
    })())
}
