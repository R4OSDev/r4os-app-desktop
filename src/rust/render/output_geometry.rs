//! Desktop's checked projection adapter. Shared topology, point mapping and
//! window rescue remain with R4GFX, with one geometry per admitted output.
use super::{
    geometry::Rect,
    primitive_assets::{Error, Result},
};
pub use r4os_display_geometry as topology;
pub fn logical(view: topology::Viewport) -> Result<Rect> {
    let r = view.logical().map_err(|_| Error::Bounds)?;
    Ok(Rect::new(r.x, r.y, r.w as i32, r.h as i32))
}
pub fn native(view: topology::Viewport) -> Rect {
    Rect::new(0, 0, view.pixel_w as i32, view.pixel_h as i32)
}
pub fn desktop_bounds(layout: &topology::Layout) -> Option<Rect> {
    if layout.outputs().is_empty() {
        return None;
    }
    let (mut left, mut top) = (i64::MAX, i64::MAX);
    let (mut right, mut bottom) = (i64::MIN, i64::MIN);
    for output in layout.outputs() {
        if !output.enabled {
            continue;
        }
        let Ok(r) = output.view.logical() else {
            continue;
        };
        left = left.min(i64::from(r.x));
        top = top.min(i64::from(r.y));
        right = right.max(r.right());
        bottom = bottom.max(r.bottom());
    }
    let limit = topology::CAPACITY as i64 * 65536 * 2;
    if left < -limit || top < -limit || right > limit || bottom > limit {
        return None;
    }
    Some(Rect::new(
        i32::try_from(left).ok()?,
        i32::try_from(top).ok()?,
        i32::try_from(right - left).ok()?,
        i32::try_from(bottom - top).ok()?,
    ))
}
pub fn cursor_rect(bounds: Rect, p: topology::Point) -> Rect {
    let right = i64::from(bounds.x) + i64::from(bounds.w);
    let bottom = i64::from(bounds.y) + i64::from(bounds.h);
    if p.x < bounds.x || p.y < bounds.y || i64::from(p.x) >= right || i64::from(p.y) >= bottom {
        return Rect::new(p.x, p.y, 0, 0);
    }
    Rect::new(
        p.x,
        p.y,
        12.min(right - i64::from(p.x)) as i32,
        18.min(bottom - i64::from(p.y)) as i32,
    )
}
fn translated(value: Rect, shift: topology::Point) -> Option<Rect> {
    let x = i32::try_from(i64::from(value.x) - i64::from(shift.x)).ok()?;
    let y = i32::try_from(i64::from(value.y) - i64::from(shift.y)).ok()?;
    let r = topology::Rect {
        x,
        y,
        w: u32::try_from(value.w).ok()?,
        h: u32::try_from(value.h).ok()?,
    };
    r.valid().then_some(Rect::new(x, y, value.w, value.h))
}
pub fn rebase_window(
    layout: &topology::Layout,
    value: Rect,
    shift: topology::Point,
    fallback: Rect,
    title_height: u32,
) -> Rect {
    let candidate = translated(value, shift).unwrap_or_else(|| value.clamp_inside(fallback));
    if candidate.is_empty() {
        return candidate;
    }
    match layout.rescue(
        topology::Rect {
            x: candidate.x,
            y: candidate.y,
            w: candidate.w as u32,
            h: candidate.h as u32,
        },
        title_height,
    ) {
        Ok(r) => Rect::new(r.x, r.y, r.w as i32, r.h as i32),
        Err(_) => candidate.clamp_inside(fallback),
    }
}
pub fn raster_rect(view: topology::Viewport, value: Rect) -> Result<Rect> {
    view.validate().map_err(|_| Error::Bounds)?;
    let edge = |v: i64, origin: i32| {
        -(-((v - i64::from(origin)) * i64::from(view.scale) - 60))
            .div_euclid(i64::from(topology::SCALE_UNIT))
    };
    let x0 = edge(i64::from(value.x), view.origin.x);
    let y0 = edge(i64::from(value.y), view.origin.y);
    let x1 = edge(i64::from(value.x) + i64::from(value.w), view.origin.x);
    let y1 = edge(i64::from(value.y) + i64::from(value.h), view.origin.y);
    use topology::Rotation::*;
    let r = match view.rotation {
        Normal => [x0, y0, x1 - x0, y1 - y0],
        Clockwise90 => [y0, i64::from(view.pixel_h) - x1, y1 - y0, x1 - x0],
        Clockwise180 => [
            i64::from(view.pixel_w) - x1,
            i64::from(view.pixel_h) - y1,
            x1 - x0,
            y1 - y0,
        ],
        Clockwise270 => [i64::from(view.pixel_w) - y1, x0, y1 - y0, x1 - x0],
    };
    if r[2] < 0
        || r[3] < 0
        || r[0] + r[2] > i64::from(i32::MAX)
        || r[1] + r[3] > i64::from(i32::MAX)
    {
        return Err(Error::Bounds);
    }
    Ok(Rect::new(
        i32::try_from(r[0]).map_err(|_| Error::Bounds)?,
        i32::try_from(r[1]).map_err(|_| Error::Bounds)?,
        i32::try_from(r[2]).map_err(|_| Error::Bounds)?,
        i32::try_from(r[3]).map_err(|_| Error::Bounds)?,
    ))
}
