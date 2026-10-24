//! The compositor works in logical desktop coordinates. Its storage and
//! command lists use the exact native pixels of one output.
const std = @import("std");
pub const topology = @import("r4gfx_desktop_outputs").topology;
const surface = @import("surface.zig");
pub fn logical(view: topology.Viewport) !surface.Rect {
    const r = try view.logical();
    return .{ .x = r.x, .y = r.y, .w = @intCast(r.w), .h = @intCast(r.h) };
}
pub fn native(view: topology.Viewport) surface.Rect {
    return .{ .x = 0, .y = 0, .w = @intCast(view.pixel_w), .h = @intCast(view.pixel_h) };
}
/// Compute once when admitting a layout, before publishing its desktop edges.
pub fn desktopBounds(layout: *const topology.Layout) ?surface.Rect {
    if (layout.count == 0 or layout.count > topology.capacity) return null;
    var left: i64 = std.math.maxInt(i64); var top = left;
    var right: i64 = std.math.minInt(i64); var bottom = right;
    for (layout.outputs[0..layout.count]) |output| {
        if (!output.enabled) continue;
        const bounds = output.view.logical() catch continue;
        left = @min(left, bounds.x); top = @min(top, bounds.y);
        right = @max(right, bounds.right()); bottom = @max(bottom, bounds.bottom());
    }
    // Current outputs admit at most65536 native pixels at scale60/120.
    // Keep the normalized workspace inside the full eight-output extent,
    // including gaps. Frame chrome and pending old/new window damage then
    // retain ample i32 headroom, even for an individually tiny output.
    const coordinate_limit: i64 = topology.capacity * 65536 * 2;
    if (left < -coordinate_limit or top < -coordinate_limit or right > coordinate_limit or bottom > coordinate_limit) return null;
    return .{ .x = std.math.cast(i32, left) orelse return null, .y = std.math.cast(i32, top) orelse return null,
        .w = std.math.cast(i32, right - left) orelse return null, .h = std.math.cast(i32, bottom - top) orelse return null };
}
/// Cursor pixels outside the whole desktop cannot reach an output. Clip there
/// before damage unions, while retaining spill across adjacent output edges.
pub fn cursorRect(bounds: surface.Rect, point: topology.Point) surface.Rect {
    const empty: surface.Rect = .{ .x = point.x, .y = point.y, .w = 0, .h = 0 };
    const right = @as(i64, bounds.x) + bounds.w; const bottom = @as(i64, bounds.y) + bounds.h;
    if (point.x < bounds.x or point.y < bounds.y or point.x >= right or point.y >= bottom) return empty;
    return .{ .x = point.x, .y = point.y, .w = @intCast(@min(surface.cursor_w, right - point.x)),
        .h = @intCast(@min(surface.cursor_h, bottom - point.y)) };
}
fn translated(value: surface.Rect, shift: topology.Point) ?surface.Rect {
    const x = std.math.cast(i32, @as(i64, value.x) - shift.x) orelse return null;
    const y = std.math.cast(i32, @as(i64, value.y) - shift.y) orelse return null;
    const w = std.math.cast(u32, value.w) orelse return null;
    const h = std.math.cast(u32, value.h) orelse return null;
    if (!(topology.Rect{ .x = x, .y = y, .w = w, .h = h }).valid()) return null;
    return .{ .x = x, .y = y, .w = value.w, .h = value.h };
}
/// Keep the prior geometry until a complete translated/rescued rectangle is
/// available. Unrepresentable old coordinates recover inside a known output.
pub fn rebaseWindow(layout: *const topology.Layout, value: surface.Rect, shift: topology.Point, fallback: surface.Rect, title_height: u32) surface.Rect {
    const candidate = translated(value, shift) orelse value.clampInside(fallback);
    if (candidate.isEmpty()) return candidate;
    const result = layout.rescue(.{ .x = candidate.x, .y = candidate.y, .w = @intCast(candidate.w), .h = @intCast(candidate.h) }, title_height) catch
        return candidate.clampInside(fallback);
    return .{ .x = result.x, .y = result.y, .w = @intCast(result.w), .h = @intCast(result.h) };
}
/// One shared rounding of adjacent edges avoids overlapping translucent
/// primitives. Damage alone uses the topology's conservative outer rounding.
/// Do not clip the target quad: that would change source texture sampling.
pub fn rasterRect(view: topology.Viewport, value: surface.Rect) !surface.Rect {
    try view.validate();
    // First native pixel whose center samples this logical cell. Shared
    // edges agree with the inverse grid, including half-pixel exact ties.
    const x0 = -@divFloor(-((@as(i64, value.x) - view.origin.x) * view.scale - 60), topology.scale_unit);
    const y0 = -@divFloor(-((@as(i64, value.y) - view.origin.y) * view.scale - 60), topology.scale_unit);
    const x1 = -@divFloor(-((@as(i64, value.x) + value.w - view.origin.x) * view.scale - 60), topology.scale_unit);
    const y1 = -@divFloor(-((@as(i64, value.y) + value.h - view.origin.y) * view.scale - 60), topology.scale_unit);
    const r: [4]i64 = switch (view.rotation) {
        .normal => .{ x0, y0, x1 - x0, y1 - y0 },
        .clockwise90 => .{ y0, @as(i64, view.pixel_h) - x1, y1 - y0, x1 - x0 },
        .clockwise180 => .{ @as(i64, view.pixel_w) - x1, @as(i64, view.pixel_h) - y1, x1 - x0, y1 - y0 },
        .clockwise270 => .{ @as(i64, view.pixel_w) - y1, x0, y1 - y0, x1 - x0 },
    };
    if (r[2] < 0 or r[3] < 0 or r[0] + r[2] > std.math.maxInt(i32) or r[1] + r[3] > std.math.maxInt(i32)) return error.Bounds;
    return .{ .x = std.math.cast(i32, r[0]) orelse return error.Bounds, .y = std.math.cast(i32, r[1]) orelse return error.Bounds,
        .w = std.math.cast(i32, r[2]) orelse return error.Bounds, .h = std.math.cast(i32, r[3]) orelse return error.Bounds };
}
pub fn intersect(a: surface.Rect, b: surface.Rect) ?surface.Rect {
    const x = @max(a.x, b.x); const y = @max(a.y, b.y);
    const right = @min(a.right(), b.right()); const bottom = @min(a.bottom(), b.bottom());
    if (right <= x or bottom <= y) return null;
    return .{ .x = x, .y = y, .w = right - x, .h = bottom - y };
}
