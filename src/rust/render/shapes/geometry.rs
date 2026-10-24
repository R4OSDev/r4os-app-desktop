//! Original fixed curve subdivision and four-sample shape predicates.
use super::format::*;
use crate::render::{
    float::sqrt,
    primitive_assets::{Error, Result},
};
use alloc::vec::Vec;
use r4os_contract as a;
struct Subpath {
    start: usize,
    count: usize,
    closed: bool,
}
#[derive(Default)]
pub(super) struct Geometry {
    points: Vec<Point>,
    subpaths: Vec<Subpath>,
    active_start: usize,
    active: bool,
}
impl Geometry {
    fn append(&mut self, p: Point) -> Result<()> {
        self.points.try_reserve(1).map_err(|_| Error::OutOfMemory)?;
        self.points.push(p);
        Ok(())
    }
    fn line(&mut self, p: Point) -> Result<()> {
        if !self.active {
            return Err(Error::Invalid);
        }
        self.append(p)
    }
    fn finish(&mut self, closed: bool) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        let count = self.points.len() - self.active_start;
        if count != 0 {
            self.subpaths
                .try_reserve(1)
                .map_err(|_| Error::OutOfMemory)?;
            self.subpaths.push(Subpath {
                start: self.active_start,
                count,
                closed,
            });
        }
        self.active = false;
        Ok(())
    }
    pub fn build(parsed: Parsed<'_>) -> Result<Self> {
        let mut g = Self::default();
        for index in 0..parsed.segment_count as usize {
            let offset = index * core::mem::size_of::<a::GuiPathSegment>();
            let bytes = parsed
                .segments
                .get(offset..offset + core::mem::size_of::<a::GuiPathSegment>())
                .ok_or(Error::Invalid)?;
            // Integer-only wire record, read from initialized unaligned bytes.
            let s =
                unsafe { core::ptr::read_unaligned(bytes.as_ptr().cast::<a::GuiPathSegment>()) };
            if s.flags != 0 {
                return Err(Error::Invalid);
            }
            let bits = [
                s.x1_bits, s.y1_bits, s.x2_bits, s.y2_bits, s.x3_bits, s.y3_bits,
            ];
            let point = |first: usize| -> Result<Point> {
                Ok(Point {
                    x: coordinate(bits[first]).ok_or(Error::Invalid)?,
                    y: coordinate(bits[first + 1]).ok_or(Error::Invalid)?,
                })
            };
            match s.kind {
                a::gui_path_segment_kind_move => {
                    if bits[2..].iter().any(|b| *b != 0) {
                        return Err(Error::Invalid);
                    }
                    g.finish(false)?;
                    g.active = true;
                    g.active_start = g.points.len();
                    g.append(point(0)?)?;
                }
                a::gui_path_segment_kind_line => {
                    if bits[2..].iter().any(|b| *b != 0) {
                        return Err(Error::Invalid);
                    }
                    g.line(point(0)?)?;
                }
                a::gui_path_segment_kind_quadratic => {
                    if !g.active || bits[4..].iter().any(|b| *b != 0) {
                        return Err(Error::Invalid);
                    }
                    let start = *g.points.last().ok_or(Error::Invalid)?;
                    let control = point(0)?;
                    let end = point(2)?;
                    for step in 1..=8 {
                        let t = step as f32 / 8.0;
                        let inv = 1.0 - t;
                        g.line(Point {
                            x: inv * inv * start.x + 2.0 * inv * t * control.x + t * t * end.x,
                            y: inv * inv * start.y + 2.0 * inv * t * control.y + t * t * end.y,
                        })?;
                    }
                }
                a::gui_path_segment_kind_cubic => {
                    if !g.active {
                        return Err(Error::Invalid);
                    }
                    let start = *g.points.last().ok_or(Error::Invalid)?;
                    let first = point(0)?;
                    let second = point(2)?;
                    let end = point(4)?;
                    for step in 1..=16 {
                        let t = step as f32 / 16.0;
                        let inv = 1.0 - t;
                        let inv2 = inv * inv;
                        let t2 = t * t;
                        g.line(Point {
                            x: inv2 * inv * start.x
                                + 3.0 * inv2 * t * first.x
                                + 3.0 * inv * t2 * second.x
                                + t2 * t * end.x,
                            y: inv2 * inv * start.y
                                + 3.0 * inv2 * t * first.y
                                + 3.0 * inv * t2 * second.y
                                + t2 * t * end.y,
                        })?;
                    }
                }
                a::gui_path_segment_kind_close => {
                    if !g.active || bits.iter().any(|b| *b != 0) {
                        return Err(Error::Invalid);
                    }
                    g.finish(true)?;
                }
                _ => return Err(Error::Invalid),
            }
        }
        g.finish(false)?;
        Ok(g)
    }
    fn points(&self, s: &Subpath) -> Option<&[Point]> {
        self.points.get(s.start..s.start.checked_add(s.count)?)
    }
    pub fn inside(&self, p: Point, rule: FillRule) -> bool {
        let (mut winding, mut crossings) = (0i32, 0u32);
        for path in &self.subpaths {
            let Some(points) = self.points(path) else {
                continue;
            };
            if points.len() < 2 {
                continue;
            }
            for index in 0..points.len() {
                let first = points[index];
                let second = points[(index + 1) % points.len()];
                if (first.y <= p.y && second.y > p.y) || (first.y > p.y && second.y <= p.y) {
                    let x = first.x + (p.y - first.y) * (second.x - first.x) / (second.y - first.y);
                    if x > p.x {
                        crossings += 1;
                        winding += if second.y > first.y { 1 } else { -1 };
                    }
                }
            }
        }
        match rule {
            FillRule::Evenodd => crossings & 1 != 0,
            FillRule::Nonzero => winding != 0,
        }
    }
    fn distance(&self, p: Point) -> f32 {
        let mut best = f32::INFINITY;
        for path in &self.subpaths {
            let Some(points) = self.points(path) else {
                continue;
            };
            if points.len() < 2 {
                continue;
            }
            let count = if path.closed {
                points.len()
            } else {
                points.len() - 1
            };
            for index in 0..count {
                best = best.min(distance_segment(
                    p,
                    points[index],
                    points[(index + 1) % points.len()],
                ));
            }
        }
        best
    }
    pub fn stroke(&self, p: Point, width: f32, join: LineJoin, cap: LineCap, miter: f32) -> bool {
        let radius = width * 0.5;
        if radius <= 0.0 {
            return false;
        }
        for path in &self.subpaths {
            let Some(points) = self.points(path) else {
                continue;
            };
            let count = points.len();
            if count == 0 {
                continue;
            }
            if count == 1 {
                if cap == LineCap::Round && distance(p, points[0]) <= radius {
                    return true;
                }
                continue;
            }
            let edge_count = if path.closed { count } else { count - 1 };
            for i in 0..edge_count {
                if segment_strip(p, points[i], points[(i + 1) % count], radius) {
                    return true;
                }
            }
            let start = usize::from(!path.closed);
            let end = if path.closed { count } else { count - 1 };
            for i in start..end {
                if join_contains(
                    p,
                    points[(i + count - 1) % count],
                    points[i],
                    points[(i + 1) % count],
                    radius,
                    join,
                    miter,
                ) {
                    return true;
                }
            }
            if !path.closed {
                let (first, second, before, last) =
                    (points[0], points[1], points[count - 2], points[count - 1]);
                if cap == LineCap::Round
                    && (distance(p, first) <= radius || distance(p, last) <= radius)
                {
                    return true;
                }
                if cap == LineCap::Square {
                    if let (Some(fd), Some(ld)) =
                        (normalized(first, second), normalized(before, last))
                    {
                        if square_cap(p, first, fd, radius, true)
                            || square_cap(p, last, ld, radius, false)
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}
pub(super) fn normalize(mut r: Rounded) -> Rounded {
    if r.w <= 0.0 || r.h <= 0.0 {
        return r;
    }
    let sums = [
        (r.radii.tl.x + r.radii.tr.x, r.w),
        (r.radii.bl.x + r.radii.br.x, r.w),
        (r.radii.tl.y + r.radii.bl.y, r.h),
        (r.radii.tr.y + r.radii.br.y, r.h),
    ];
    let mut factor = 1.0f32;
    for (sum, edge) in sums {
        if sum > 0.0 {
            factor = factor.min(edge / sum);
        }
    }
    factor = factor.clamp(0.0, 1.0);
    for point in [
        &mut r.radii.tl,
        &mut r.radii.tr,
        &mut r.radii.br,
        &mut r.radii.bl,
    ] {
        point.x *= factor;
        point.y *= factor;
    }
    r
}
pub(super) fn inner(p: Parsed<'_>) -> Option<Rounded> {
    let r = normalize(p.rounded);
    let w = r.w - p.border_left - p.border_right;
    let h = r.h - p.border_top - p.border_bottom;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let radius = |p: Point, x: f32, y: f32| Point {
        x: (p.x - x).max(0.0),
        y: (p.y - y).max(0.0),
    };
    Some(normalize(Rounded {
        x: r.x + p.border_left,
        y: r.y + p.border_top,
        w,
        h,
        radii: Radii {
            tl: radius(r.radii.tl, p.border_left, p.border_top),
            tr: radius(r.radii.tr, p.border_right, p.border_top),
            br: radius(r.radii.br, p.border_right, p.border_bottom),
            bl: radius(r.radii.bl, p.border_left, p.border_bottom),
        },
    }))
}
fn ellipse(p: Point, cx: f32, cy: f32, r: Point) -> bool {
    if r.x <= 0.0 || r.y <= 0.0 {
        return true;
    }
    let dx = (p.x - cx) / r.x;
    let dy = (p.y - cy) / r.y;
    dx * dx + dy * dy <= 1.0
}
pub(super) fn rounded(raw: Rounded, p: Point) -> bool {
    let r = normalize(raw);
    if r.w <= 0.0 || r.h <= 0.0 || p.x < r.x || p.y < r.y || p.x >= r.x + r.w || p.y >= r.y + r.h {
        return false;
    }
    if p.x < r.x + r.radii.tl.x && p.y < r.y + r.radii.tl.y {
        return ellipse(p, r.x + r.radii.tl.x, r.y + r.radii.tl.y, r.radii.tl);
    }
    if p.x >= r.x + r.w - r.radii.tr.x && p.y < r.y + r.radii.tr.y {
        return ellipse(p, r.x + r.w - r.radii.tr.x, r.y + r.radii.tr.y, r.radii.tr);
    }
    if p.x >= r.x + r.w - r.radii.br.x && p.y >= r.y + r.h - r.radii.br.y {
        return ellipse(
            p,
            r.x + r.w - r.radii.br.x,
            r.y + r.h - r.radii.br.y,
            r.radii.br,
        );
    }
    if p.x < r.x + r.radii.bl.x && p.y >= r.y + r.h - r.radii.bl.y {
        return ellipse(p, r.x + r.radii.bl.x, r.y + r.h - r.radii.bl.y, r.radii.bl);
    }
    true
}
fn distance(p: Point, q: Point) -> f32 {
    let dx = p.x - q.x;
    let dy = p.y - q.y;
    sqrt(dx * dx + dy * dy)
}
fn distance_segment(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx * dx + dy * dy;
    if length <= 0.000001 {
        return distance(p, a);
    }
    let f = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length).clamp(0.0, 1.0);
    distance(
        p,
        Point {
            x: a.x + f * dx,
            y: a.y + f * dy,
        },
    )
}
fn cross(a: Point, b: Point) -> f32 {
    a.x * b.y - a.y * b.x
}
fn triangle(p: Point, a: Point, b: Point, c: Point) -> bool {
    let d1 = cross(
        Point {
            x: p.x - b.x,
            y: p.y - b.y,
        },
        Point {
            x: a.x - b.x,
            y: a.y - b.y,
        },
    );
    let d2 = cross(
        Point {
            x: p.x - c.x,
            y: p.y - c.y,
        },
        Point {
            x: b.x - c.x,
            y: b.y - c.y,
        },
    );
    let d3 = cross(
        Point {
            x: p.x - a.x,
            y: p.y - a.y,
        },
        Point {
            x: c.x - a.x,
            y: c.y - a.y,
        },
    );
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}
fn normalized(a: Point, b: Point) -> Option<Point> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = sqrt(dx * dx + dy * dy);
    if len <= 0.000001 {
        None
    } else {
        Some(Point {
            x: dx / len,
            y: dy / len,
        })
    }
}
fn segment_strip(p: Point, a: Point, b: Point, r: f32) -> bool {
    let Some(d) = normalized(a, b) else {
        return distance(p, a) <= r;
    };
    let v = Point {
        x: p.x - a.x,
        y: p.y - a.y,
    };
    let along = v.x * d.x + v.y * d.y;
    let len = distance(a, b);
    along >= 0.0 && along <= len && (v.x * -d.y + v.y * d.x).abs() <= r
}
fn square_cap(p: Point, end: Point, d: Point, r: f32, start: bool) -> bool {
    let v = Point {
        x: p.x - end.x,
        y: p.y - end.y,
    };
    let along = v.x * d.x + v.y * d.y;
    let across = (v.x * -d.y + v.y * d.x).abs();
    across <= r
        && if start {
            along >= -r && along <= 0.0
        } else {
            along >= 0.0 && along <= r
        }
}
fn join_contains(
    p: Point,
    previous: Point,
    vertex: Point,
    next: Point,
    r: f32,
    join: LineJoin,
    miter_limit: f32,
) -> bool {
    if join == LineJoin::Round {
        return distance(p, vertex) <= r;
    }
    let Some(incoming) = normalized(previous, vertex) else {
        return false;
    };
    let Some(outgoing) = normalized(vertex, next) else {
        return false;
    };
    let turn = cross(incoming, outgoing);
    if turn.abs() <= 0.000001 {
        return false;
    }
    let side = if turn > 0.0 { -1.0 } else { 1.0 };
    let first_normal = Point {
        x: -incoming.y * side,
        y: incoming.x * side,
    };
    let second_normal = Point {
        x: -outgoing.y * side,
        y: outgoing.x * side,
    };
    let first = Point {
        x: vertex.x + first_normal.x * r,
        y: vertex.y + first_normal.y * r,
    };
    let second = Point {
        x: vertex.x + second_normal.x * r,
        y: vertex.y + second_normal.y * r,
    };
    if join == LineJoin::Bevel {
        return triangle(p, vertex, first, second);
    }
    let determinant = cross(incoming, outgoing);
    if determinant.abs() <= 0.000001 {
        return false;
    }
    let delta = Point {
        x: second.x - first.x,
        y: second.y - first.y,
    };
    let f = cross(delta, outgoing) / determinant;
    let miter = Point {
        x: first.x + incoming.x * f,
        y: first.y + incoming.y * f,
    };
    if distance(vertex, miter) > r * miter_limit.max(1.0) {
        return triangle(p, vertex, first, second);
    }
    triangle(p, vertex, first, miter) || triangle(p, vertex, miter, second)
}
pub(super) fn contains(p: Parsed<'_>, g: Option<&Geometry>, point: Point) -> bool {
    if p.geometry_kind == a::gui_shape_geometry_kind_path {
        g.expect("admitted path geometry")
            .inside(point, p.fill_rule)
    } else {
        rounded(p.rounded, point)
    }
}
pub(super) fn shadow(p: Parsed<'_>, g: Option<&Geometry>, point: Point) -> bool {
    if p.geometry_kind == a::gui_shape_geometry_kind_rounded_rect {
        let r = p.rounded;
        let grow = |q: Point| Point {
            x: (q.x + p.shadow_spread).max(0.0),
            y: (q.y + p.shadow_spread).max(0.0),
        };
        return rounded(
            normalize(Rounded {
                x: r.x - p.shadow_spread + p.shadow_offset_x,
                y: r.y - p.shadow_spread + p.shadow_offset_y,
                w: r.w + 2.0 * p.shadow_spread,
                h: r.h + 2.0 * p.shadow_spread,
                radii: Radii {
                    tl: grow(r.radii.tl),
                    tr: grow(r.radii.tr),
                    br: grow(r.radii.br),
                    bl: grow(r.radii.bl),
                },
            }),
            point,
        );
    }
    let local = Point {
        x: point.x - p.shadow_offset_x,
        y: point.y - p.shadow_offset_y,
    };
    let g = g.expect("admitted path geometry");
    let inside = g.inside(local, p.fill_rule);
    if p.shadow_spread == 0.0 {
        return inside;
    }
    let edge = g.distance(local);
    if p.shadow_spread > 0.0 {
        inside || edge <= p.shadow_spread
    } else {
        inside && edge >= -p.shadow_spread
    }
}
