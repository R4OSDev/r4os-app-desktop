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
