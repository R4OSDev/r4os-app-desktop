const std = @import("std");
const r4os = @import("r4os");
const scene_buffer = @import("scene_buffer.zig");
const surface = @import("surface.zig");

pub const Result = enum {
    drawn,
    empty,
    invalid,
    out_of_memory,
};

const curve_steps_quadratic: usize = 8;
const curve_steps_cubic: usize = 16;

const Point = struct { x: f32, y: f32 };
const Subpath = struct { start: u32, count: u32, closed: bool };
const FillRule = enum { nonzero, evenodd };
const LineJoin = enum { miter, round, bevel };
const LineCap = enum { butt, round, square };

const Clip = struct {
    x: f32,
    y: f32,
    right: f32,
    bottom: f32,

    fn contains(self: Clip, point: Point) bool {
        return point.x >= self.x and point.y >= self.y and point.x < self.right and point.y < self.bottom;
    }
};

const Radii = struct {
    tlx: f32 = 0,
    tly: f32 = 0,
    trx: f32 = 0,
    try_: f32 = 0,
    brx: f32 = 0,
    bry: f32 = 0,
    blx: f32 = 0,
    bly: f32 = 0,
};

const RoundedRect = struct {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radii: Radii,
};

const Parsed = struct {
    geometry_kind: u32,
    flags: u32,
    segment_count: u32,
    fill_rule: FillRule,
    line_join: ?LineJoin,
    line_cap: ?LineCap,
    clip: Clip,
    fill_argb: u32,
    stroke_argb: u32,
    stroke_width: f32,
    miter_limit: f32,
    rounded: RoundedRect,
    border_top: f32,
    border_right: f32,
    border_bottom: f32,
    border_left: f32,
    shadow_argb: u32,
    shadow_offset_x: f32,
    shadow_offset_y: f32,
    shadow_spread: f32,
    shadow_blur: f32,
    segments: []const u8,
};

const Geometry = struct {
    points: std.ArrayList(Point) = .empty,
    subpaths: std.ArrayList(Subpath) = .empty,
    active_start: usize = 0,
    active: bool = false,

    fn deinit(self: *Geometry, allocator: std.mem.Allocator) void {
        self.points.deinit(allocator);
        self.subpaths.deinit(allocator);
    }

    fn moveTo(self: *Geometry, allocator: std.mem.Allocator, point: Point) !void {
        try self.finish(allocator, false);
        self.active = true;
        self.active_start = self.points.items.len;
        try self.points.append(allocator, point);
    }

    fn lineTo(self: *Geometry, allocator: std.mem.Allocator, point: Point) !void {
        if (!self.active) return error.InvalidPath;
        try self.points.append(allocator, point);
    }

    fn finish(self: *Geometry, allocator: std.mem.Allocator, closed: bool) !void {
        if (!self.active) return;
        const count = self.points.items.len - self.active_start;
        if (count != 0) try self.subpaths.append(allocator, .{
            .start = @intCast(self.active_start),
            .count = @intCast(count),
            .closed = closed,
        });
        self.active = false;
    }
};

const sample_offsets = [_]Point{
    .{ .x = 0.25, .y = 0.25 },
    .{ .x = 0.75, .y = 0.25 },
    .{ .x = 0.25, .y = 0.75 },
    .{ .x = 0.75, .y = 0.75 },
};

fn readU32(bytes: []const u8, offset: usize) ?u32 {
    const end = std.math.add(usize, offset, 4) catch return null;
    if (end > bytes.len) return null;
    return @as(u32, bytes[offset]) |
        (@as(u32, bytes[offset + 1]) << 8) |
        (@as(u32, bytes[offset + 2]) << 16) |
        (@as(u32, bytes[offset + 3]) << 24);
}

fn shapeField(bytes: []const u8, comptime name: []const u8) ?u32 {
    return readU32(bytes, @offsetOf(r4os.abi.GuiShapeResource, name));
}

fn floatValue(bits: u32) ?f32 {
    const value: f32 = @bitCast(bits);
    if (!std.math.isFinite(value)) return null;
    return value;
}

fn coordinate(bits: u32) ?f32 {
    const value = floatValue(bits) orelse return null;
    if (@abs(value) > @as(f32, @floatFromInt(r4os.abi.gui_shape_max_coordinate))) return null;
    return value;
}

fn nonNegative(bits: u32, maximum: f32) ?f32 {
    const value = floatValue(bits) orelse return null;
    if (value < 0 or value > maximum) return null;
    return value;
}

fn parse(command: r4os.abi.GuiFrameCommand, resource: []const u8) ?Parsed {
    if (command.w == 0 or command.h == 0 or command.w > r4os.abi.gui_shape_max_dimension or command.h > r4os.abi.gui_shape_max_dimension) return null;
    const pixels = std.math.mul(u64, command.w, command.h) catch return null;
    if (pixels > r4os.abi.gui_shape_max_pixels or command.parameter0 != 0 or command.parameter1 != 0) return null;
    if (command.rgb != 0 or command.fg != 0 or command.bg != 0 or command.font_id != 0 or
        command.text_w != 0 or command.text_h != 0 or command.baseline != 0 or command.line_height != 0) return null;
    if (resource.len < @sizeOf(r4os.abi.GuiShapeResource) or command.resource_bytes != resource.len) return null;
    if ((shapeField(resource, "version") orelse return null) != r4os.abi.gui_shape_resource_version or
        (shapeField(resource, "size") orelse return null) != r4os.abi.gui_shape_resource_size) return null;

    const segment_count = shapeField(resource, "segment_count") orelse return null;
    if (segment_count > r4os.abi.gui_shape_max_segments) return null;
    const segment_bytes = std.math.mul(u64, segment_count, r4os.abi.gui_path_segment_size) catch return null;
    const expected = std.math.add(u64, r4os.abi.gui_shape_resource_size, segment_bytes) catch return null;
    if (expected != resource.len) return null;
    const flags = shapeField(resource, "flags") orelse return null;
    if ((flags & ~r4os.abi.gui_shape_flag_shadow_inset) != 0) return null;
    const clip_w = shapeField(resource, "clip_w") orelse return null;
    const clip_h = shapeField(resource, "clip_h") orelse return null;
    if ((clip_w == 0) != (clip_h == 0) or clip_w > r4os.abi.gui_shape_max_dimension or clip_h > r4os.abi.gui_shape_max_dimension) return null;
    const clip_x_bits = shapeField(resource, "clip_x") orelse return null;
    const clip_y_bits = shapeField(resource, "clip_y") orelse return null;
    const clip_x: i32 = @bitCast(clip_x_bits);
    const clip_y: i32 = @bitCast(clip_y_bits);
    const clip = if (clip_w == 0) Clip{ .x = 0, .y = 0, .right = @floatFromInt(command.w), .bottom = @floatFromInt(command.h) } else Clip{
        .x = @floatFromInt(clip_x),
        .y = @floatFromInt(clip_y),
        .right = @as(f32, @floatFromInt(clip_x)) + @as(f32, @floatFromInt(clip_w)),
        .bottom = @as(f32, @floatFromInt(clip_y)) + @as(f32, @floatFromInt(clip_h)),
    };

    const fill_rule_raw = shapeField(resource, "fill_rule") orelse return null;
    const fill_rule: FillRule = if (fill_rule_raw == r4os.abi.gui_shape_fill_rule_nonzero)
        .nonzero
    else if (fill_rule_raw == r4os.abi.gui_shape_fill_rule_evenodd)
        .evenodd
    else
        return null;
    const join_raw = shapeField(resource, "line_join") orelse return null;
    const cap_raw = shapeField(resource, "line_cap") orelse return null;
    const line_join: ?LineJoin = switch (join_raw) {
        0 => null,
        r4os.abi.gui_shape_line_join_miter => .miter,
        r4os.abi.gui_shape_line_join_round => .round,
        r4os.abi.gui_shape_line_join_bevel => .bevel,
        else => return null,
    };
    const line_cap: ?LineCap = switch (cap_raw) {
        0 => null,
        r4os.abi.gui_shape_line_cap_butt => .butt,
        r4os.abi.gui_shape_line_cap_round => .round,
        r4os.abi.gui_shape_line_cap_square => .square,
        else => return null,
    };
    const coordinate_max: f32 = @floatFromInt(r4os.abi.gui_shape_max_coordinate);
    const stroke_width = nonNegative(shapeField(resource, "stroke_width_bits") orelse return null, coordinate_max) orelse return null;
    const miter_limit = nonNegative(shapeField(resource, "miter_limit_bits") orelse return null, coordinate_max) orelse return null;
    if (line_join == .miter and miter_limit < 1) return null;
    const shadow_blur = nonNegative(shapeField(resource, "shadow_blur_bits") orelse return null, @floatFromInt(r4os.abi.gui_shape_max_blur_radius)) orelse return null;

    const parsed = Parsed{
        .geometry_kind = shapeField(resource, "geometry_kind") orelse return null,
        .flags = flags,
        .segment_count = segment_count,
        .fill_rule = fill_rule,
        .line_join = line_join,
        .line_cap = line_cap,
        .clip = clip,
        .fill_argb = shapeField(resource, "fill_argb") orelse return null,
        .stroke_argb = shapeField(resource, "stroke_argb") orelse return null,
        .stroke_width = stroke_width,
        .miter_limit = miter_limit,
        .rounded = .{
            .x = coordinate(shapeField(resource, "geometry_x_bits") orelse return null) orelse return null,
            .y = coordinate(shapeField(resource, "geometry_y_bits") orelse return null) orelse return null,
            .w = nonNegative(shapeField(resource, "geometry_w_bits") orelse return null, coordinate_max) orelse return null,
            .h = nonNegative(shapeField(resource, "geometry_h_bits") orelse return null, coordinate_max) orelse return null,
            .radii = .{
                .tlx = nonNegative(shapeField(resource, "radius_top_left_x_bits") orelse return null, coordinate_max) orelse return null,
                .tly = nonNegative(shapeField(resource, "radius_top_left_y_bits") orelse return null, coordinate_max) orelse return null,
                .trx = nonNegative(shapeField(resource, "radius_top_right_x_bits") orelse return null, coordinate_max) orelse return null,
                .try_ = nonNegative(shapeField(resource, "radius_top_right_y_bits") orelse return null, coordinate_max) orelse return null,
                .brx = nonNegative(shapeField(resource, "radius_bottom_right_x_bits") orelse return null, coordinate_max) orelse return null,
                .bry = nonNegative(shapeField(resource, "radius_bottom_right_y_bits") orelse return null, coordinate_max) orelse return null,
                .blx = nonNegative(shapeField(resource, "radius_bottom_left_x_bits") orelse return null, coordinate_max) orelse return null,
                .bly = nonNegative(shapeField(resource, "radius_bottom_left_y_bits") orelse return null, coordinate_max) orelse return null,
            },
        },
        .border_top = nonNegative(shapeField(resource, "border_top_bits") orelse return null, coordinate_max) orelse return null,
        .border_right = nonNegative(shapeField(resource, "border_right_bits") orelse return null, coordinate_max) orelse return null,
        .border_bottom = nonNegative(shapeField(resource, "border_bottom_bits") orelse return null, coordinate_max) orelse return null,
        .border_left = nonNegative(shapeField(resource, "border_left_bits") orelse return null, coordinate_max) orelse return null,
        .shadow_argb = shapeField(resource, "shadow_argb") orelse return null,
        .shadow_offset_x = coordinate(shapeField(resource, "shadow_offset_x_bits") orelse return null) orelse return null,
        .shadow_offset_y = coordinate(shapeField(resource, "shadow_offset_y_bits") orelse return null) orelse return null,
        .shadow_spread = coordinate(shapeField(resource, "shadow_spread_bits") orelse return null) orelse return null,
        .shadow_blur = shadow_blur,
        .segments = resource[@sizeOf(r4os.abi.GuiShapeResource)..],
    };
    if (shapeField(resource, "reserved0") != 0 or shapeField(resource, "reserved1") != 0 or shapeField(resource, "reserved2") != 0) return null;
    switch (parsed.geometry_kind) {
        r4os.abi.gui_shape_geometry_kind_path => {
            if (segment_count == 0) return null;
            var offset: usize = @offsetOf(r4os.abi.GuiShapeResource, "geometry_x_bits");
            while (offset <= @offsetOf(r4os.abi.GuiShapeResource, "border_left_bits")) : (offset += 4) if (readU32(resource, offset) != 0) return null;
        },
        r4os.abi.gui_shape_geometry_kind_rounded_rect => {
            if (segment_count != 0 or line_join != null or line_cap != null or stroke_width != 0 or miter_limit != 0 or parsed.rounded.w <= 0 or parsed.rounded.h <= 0) return null;
        },
        else => return null,
    }
    switch (command.kind) {
        r4os.abi.gui_frame_command_kind_path_fill => if (parsed.geometry_kind != r4os.abi.gui_shape_geometry_kind_path) return null,
        r4os.abi.gui_frame_command_kind_path_stroke => if (parsed.geometry_kind != r4os.abi.gui_shape_geometry_kind_path or stroke_width <= 0 or line_join == null or line_cap == null) return null,
        r4os.abi.gui_frame_command_kind_rounded_rect => if (parsed.geometry_kind != r4os.abi.gui_shape_geometry_kind_rounded_rect) return null,
        r4os.abi.gui_frame_command_kind_shadow => {},
        else => return null,
    }
    return parsed;
}

fn segmentField(bytes: []const u8, index: u32, comptime name: []const u8) ?u32 {
    const relative = std.math.mul(usize, @as(usize, index), @sizeOf(r4os.abi.GuiPathSegment)) catch return null;
    const offset = std.math.add(usize, relative, @offsetOf(r4os.abi.GuiPathSegment, name)) catch return null;
    return readU32(bytes, offset);
}

fn pathPoint(bytes: []const u8, index: u32, comptime x_name: []const u8, comptime y_name: []const u8) ?Point {
    return .{
        .x = coordinate(segmentField(bytes, index, x_name) orelse return null) orelse return null,
        .y = coordinate(segmentField(bytes, index, y_name) orelse return null) orelse return null,
    };
}

fn unusedCoordinatesZero(bytes: []const u8, index: u32, comptime first: usize) bool {
    const names = [_][]const u8{ "x1_bits", "y1_bits", "x2_bits", "y2_bits", "x3_bits", "y3_bits" };
    inline for (names[first..]) |name| if ((segmentField(bytes, index, name) orelse return false) != 0) return false;
    return true;
}

fn buildGeometry(allocator: std.mem.Allocator, parsed: Parsed) !Geometry {
    var geometry = Geometry{};
    errdefer geometry.deinit(allocator);
    var index: u32 = 0;
    while (index < parsed.segment_count) : (index += 1) {
        const kind = segmentField(parsed.segments, index, "kind") orelse return error.InvalidPath;
        if ((segmentField(parsed.segments, index, "flags") orelse return error.InvalidPath) != 0) return error.InvalidPath;
        switch (kind) {
            r4os.abi.gui_path_segment_kind_move => {
                if (!unusedCoordinatesZero(parsed.segments, index, 2)) return error.InvalidPath;
                try geometry.moveTo(allocator, pathPoint(parsed.segments, index, "x1_bits", "y1_bits") orelse return error.InvalidPath);
            },
            r4os.abi.gui_path_segment_kind_line => {
                if (!unusedCoordinatesZero(parsed.segments, index, 2)) return error.InvalidPath;
                try geometry.lineTo(allocator, pathPoint(parsed.segments, index, "x1_bits", "y1_bits") orelse return error.InvalidPath);
            },
            r4os.abi.gui_path_segment_kind_quadratic => {
                if (!geometry.active or !unusedCoordinatesZero(parsed.segments, index, 4)) return error.InvalidPath;
                const start = geometry.points.items[geometry.points.items.len - 1];
                const control = pathPoint(parsed.segments, index, "x1_bits", "y1_bits") orelse return error.InvalidPath;
                const finish = pathPoint(parsed.segments, index, "x2_bits", "y2_bits") orelse return error.InvalidPath;
                var step: usize = 1;
                while (step <= curve_steps_quadratic) : (step += 1) {
                    const t = @as(f32, @floatFromInt(step)) / @as(f32, @floatFromInt(curve_steps_quadratic));
                    const inv = 1 - t;
                    try geometry.lineTo(allocator, .{
                        .x = inv * inv * start.x + 2 * inv * t * control.x + t * t * finish.x,
                        .y = inv * inv * start.y + 2 * inv * t * control.y + t * t * finish.y,
                    });
                }
            },
            r4os.abi.gui_path_segment_kind_cubic => {
                if (!geometry.active) return error.InvalidPath;
                const start = geometry.points.items[geometry.points.items.len - 1];
                const first = pathPoint(parsed.segments, index, "x1_bits", "y1_bits") orelse return error.InvalidPath;
                const second = pathPoint(parsed.segments, index, "x2_bits", "y2_bits") orelse return error.InvalidPath;
                const finish = pathPoint(parsed.segments, index, "x3_bits", "y3_bits") orelse return error.InvalidPath;
                var step: usize = 1;
                while (step <= curve_steps_cubic) : (step += 1) {
                    const t = @as(f32, @floatFromInt(step)) / @as(f32, @floatFromInt(curve_steps_cubic));
                    const inv = 1 - t;
                    const inv2 = inv * inv;
                    const t2 = t * t;
                    try geometry.lineTo(allocator, .{
                        .x = inv2 * inv * start.x + 3 * inv2 * t * first.x + 3 * inv * t2 * second.x + t2 * t * finish.x,
                        .y = inv2 * inv * start.y + 3 * inv2 * t * first.y + 3 * inv * t2 * second.y + t2 * t * finish.y,
                    });
                }
            },
            r4os.abi.gui_path_segment_kind_close => {
                if (!geometry.active or !unusedCoordinatesZero(parsed.segments, index, 0)) return error.InvalidPath;
                try geometry.finish(allocator, true);
            },
            else => return error.InvalidPath,
        }
    }
    try geometry.finish(allocator, false);
    return geometry;
}

fn normalizeRoundedRect(input: RoundedRect) RoundedRect {
    var result = input;
    if (result.w <= 0 or result.h <= 0) return result;
    const top = result.radii.tlx + result.radii.trx;
    const bottom = result.radii.blx + result.radii.brx;
    const left = result.radii.tly + result.radii.bly;
    const right = result.radii.try_ + result.radii.bry;
    var factor: f32 = 1;
    if (top > 0) factor = @min(factor, result.w / top);
    if (bottom > 0) factor = @min(factor, result.w / bottom);
    if (left > 0) factor = @min(factor, result.h / left);
    if (right > 0) factor = @min(factor, result.h / right);
    factor = std.math.clamp(factor, 0, 1);
    result.radii.tlx *= factor;
    result.radii.tly *= factor;
    result.radii.trx *= factor;
    result.radii.try_ *= factor;
    result.radii.brx *= factor;
    result.radii.bry *= factor;
    result.radii.blx *= factor;
    result.radii.bly *= factor;
    return result;
}

fn innerRoundedRect(parsed: Parsed) ?RoundedRect {
    const outer = normalizeRoundedRect(parsed.rounded);
    const width = outer.w - parsed.border_left - parsed.border_right;
    const height = outer.h - parsed.border_top - parsed.border_bottom;
    if (width <= 0 or height <= 0) return null;
    return normalizeRoundedRect(.{
        .x = outer.x + parsed.border_left,
        .y = outer.y + parsed.border_top,
        .w = width,
        .h = height,
        .radii = .{
            .tlx = @max(0, outer.radii.tlx - parsed.border_left),
            .tly = @max(0, outer.radii.tly - parsed.border_top),
            .trx = @max(0, outer.radii.trx - parsed.border_right),
            .try_ = @max(0, outer.radii.try_ - parsed.border_top),
            .brx = @max(0, outer.radii.brx - parsed.border_right),
            .bry = @max(0, outer.radii.bry - parsed.border_bottom),
            .blx = @max(0, outer.radii.blx - parsed.border_left),
            .bly = @max(0, outer.radii.bly - parsed.border_bottom),
        },
    });
}

fn insideEllipseCorner(point: Point, cx: f32, cy: f32, rx: f32, ry: f32) bool {
    if (rx <= 0 or ry <= 0) return true;
    const dx = (point.x - cx) / rx;
    const dy = (point.y - cy) / ry;
    return dx * dx + dy * dy <= 1;
}

fn insideRoundedRect(raw: RoundedRect, point: Point) bool {
    const rect = normalizeRoundedRect(raw);
    if (rect.w <= 0 or rect.h <= 0 or point.x < rect.x or point.y < rect.y or point.x >= rect.x + rect.w or point.y >= rect.y + rect.h) return false;
    if (point.x < rect.x + rect.radii.tlx and point.y < rect.y + rect.radii.tly)
        return insideEllipseCorner(point, rect.x + rect.radii.tlx, rect.y + rect.radii.tly, rect.radii.tlx, rect.radii.tly);
    if (point.x >= rect.x + rect.w - rect.radii.trx and point.y < rect.y + rect.radii.try_)
        return insideEllipseCorner(point, rect.x + rect.w - rect.radii.trx, rect.y + rect.radii.try_, rect.radii.trx, rect.radii.try_);
    if (point.x >= rect.x + rect.w - rect.radii.brx and point.y >= rect.y + rect.h - rect.radii.bry)
        return insideEllipseCorner(point, rect.x + rect.w - rect.radii.brx, rect.y + rect.h - rect.radii.bry, rect.radii.brx, rect.radii.bry);
    if (point.x < rect.x + rect.radii.blx and point.y >= rect.y + rect.h - rect.radii.bly)
        return insideEllipseCorner(point, rect.x + rect.radii.blx, rect.y + rect.h - rect.radii.bly, rect.radii.blx, rect.radii.bly);
    return true;
}

fn insidePath(geometry: *const Geometry, point: Point, rule: FillRule) bool {
    var winding: i32 = 0;
    var crossings: u32 = 0;
    for (geometry.subpaths.items) |subpath| {
        const start: usize = subpath.start;
        const count: usize = subpath.count;
        if (count < 2 or start > geometry.points.items.len or count > geometry.points.items.len - start) continue;
        var index: usize = 0;
        while (index < count) : (index += 1) {
            const first = geometry.points.items[start + index];
            const second = geometry.points.items[start + ((index + 1) % count)];
            if ((first.y <= point.y and second.y > point.y) or (first.y > point.y and second.y <= point.y)) {
                const intersection = first.x + (point.y - first.y) * (second.x - first.x) / (second.y - first.y);
                if (intersection > point.x) {
                    crossings += 1;
                    winding += if (second.y > first.y) 1 else -1;
                }
            }
        }
    }
    return if (rule == .evenodd) (crossings & 1) != 0 else winding != 0;
}

fn distanceToSegment(point: Point, first: Point, second: Point) f32 {
    const dx = second.x - first.x;
    const dy = second.y - first.y;
    const length_squared = dx * dx + dy * dy;
    if (length_squared <= 0.000001) return distance(point, first);
    const factor = std.math.clamp(((point.x - first.x) * dx + (point.y - first.y) * dy) / length_squared, 0, 1);
    return distance(point, .{ .x = first.x + factor * dx, .y = first.y + factor * dy });
}

fn distance(point: Point, other: Point) f32 {
    const dx = point.x - other.x;
    const dy = point.y - other.y;
    return @sqrt(dx * dx + dy * dy);
}

fn distanceToPath(geometry: *const Geometry, point: Point) f32 {
    var best = std.math.inf(f32);
    for (geometry.subpaths.items) |subpath| {
        const start: usize = subpath.start;
        const count: usize = subpath.count;
        if (count < 2 or start > geometry.points.items.len or count > geometry.points.items.len - start) continue;
        const edge_count = if (subpath.closed) count else count - 1;
        var index: usize = 0;
        while (index < edge_count) : (index += 1) {
            best = @min(best, distanceToSegment(point, geometry.points.items[start + index], geometry.points.items[start + ((index + 1) % count)]));
        }
    }
    return best;
}

fn pointInTriangle(point: Point, a: Point, b: Point, c: Point) bool {
    const d1 = cross(.{ .x = point.x - b.x, .y = point.y - b.y }, .{ .x = a.x - b.x, .y = a.y - b.y });
    const d2 = cross(.{ .x = point.x - c.x, .y = point.y - c.y }, .{ .x = b.x - c.x, .y = b.y - c.y });
    const d3 = cross(.{ .x = point.x - a.x, .y = point.y - a.y }, .{ .x = c.x - a.x, .y = c.y - a.y });
    const has_negative = d1 < 0 or d2 < 0 or d3 < 0;
    const has_positive = d1 > 0 or d2 > 0 or d3 > 0;
    return !(has_negative and has_positive);
}

fn cross(first: Point, second: Point) f32 {
    return first.x * second.y - first.y * second.x;
}

fn normalized(from: Point, to: Point) ?Point {
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    const length = @sqrt(dx * dx + dy * dy);
    if (length <= 0.000001) return null;
    return .{ .x = dx / length, .y = dy / length };
}

fn insideSegmentStrip(point: Point, first: Point, second: Point, radius: f32) bool {
    const direction = normalized(first, second) orelse return distance(point, first) <= radius;
    const relative = Point{ .x = point.x - first.x, .y = point.y - first.y };
    const along = relative.x * direction.x + relative.y * direction.y;
    const length = distance(first, second);
    if (along < 0 or along > length) return false;
    return @abs(relative.x * -direction.y + relative.y * direction.x) <= radius;
}

fn insideSquareCap(point: Point, endpoint: Point, direction: Point, radius: f32, start: bool) bool {
    const relative = Point{ .x = point.x - endpoint.x, .y = point.y - endpoint.y };
    const along = relative.x * direction.x + relative.y * direction.y;
    const across = @abs(relative.x * -direction.y + relative.y * direction.x);
    return across <= radius and if (start) along >= -radius and along <= 0 else along >= 0 and along <= radius;
}

fn joinContains(point: Point, previous: Point, vertex: Point, next: Point, radius: f32, join: LineJoin, miter_limit: f32) bool {
    if (join == .round) return distance(point, vertex) <= radius;
    const incoming = normalized(previous, vertex) orelse return false;
    const outgoing = normalized(vertex, next) orelse return false;
    const turn = cross(incoming, outgoing);
    if (@abs(turn) <= 0.000001) return false;
    const side: f32 = if (turn > 0) -1 else 1;
    const first_normal = Point{ .x = -incoming.y * side, .y = incoming.x * side };
    const second_normal = Point{ .x = -outgoing.y * side, .y = outgoing.x * side };
    const first = Point{ .x = vertex.x + first_normal.x * radius, .y = vertex.y + first_normal.y * radius };
    const second = Point{ .x = vertex.x + second_normal.x * radius, .y = vertex.y + second_normal.y * radius };
    if (join == .bevel) return pointInTriangle(point, vertex, first, second);

    const determinant = cross(incoming, outgoing);
    if (@abs(determinant) <= 0.000001) return false;
    const delta = Point{ .x = second.x - first.x, .y = second.y - first.y };
    const factor = cross(delta, outgoing) / determinant;
    const miter = Point{ .x = first.x + incoming.x * factor, .y = first.y + incoming.y * factor };
    if (distance(vertex, miter) > radius * @max(1, miter_limit)) return pointInTriangle(point, vertex, first, second);
    return pointInTriangle(point, vertex, first, miter) or pointInTriangle(point, vertex, miter, second);
}

fn insideStroke(geometry: *const Geometry, point: Point, width: f32, join: LineJoin, cap: LineCap, miter_limit: f32) bool {
    const radius = width * 0.5;
    if (radius <= 0) return false;
    for (geometry.subpaths.items) |subpath| {
        const start: usize = subpath.start;
        const count: usize = subpath.count;
        if (count == 0 or start > geometry.points.items.len or count > geometry.points.items.len - start) continue;
        if (count == 1) {
            if (cap == .round and distance(point, geometry.points.items[start]) <= radius) return true;
            continue;
        }
        const edge_count = if (subpath.closed) count else count - 1;
        var index: usize = 0;
        while (index < edge_count) : (index += 1) {
            if (insideSegmentStrip(point, geometry.points.items[start + index], geometry.points.items[start + ((index + 1) % count)], radius)) return true;
        }
        const join_start: usize = if (subpath.closed) 0 else 1;
        const join_end: usize = if (subpath.closed) count else count - 1;
        index = join_start;
        while (index < join_end) : (index += 1) {
            const previous = geometry.points.items[start + ((index + count - 1) % count)];
            const vertex = geometry.points.items[start + index];
            const next = geometry.points.items[start + ((index + 1) % count)];
            if (joinContains(point, previous, vertex, next, radius, join, miter_limit)) return true;
        }
        if (!subpath.closed) {
            const first = geometry.points.items[start];
            const second = geometry.points.items[start + 1];
            const before_last = geometry.points.items[start + count - 2];
            const last = geometry.points.items[start + count - 1];
            if (cap == .round and (distance(point, first) <= radius or distance(point, last) <= radius)) return true;
            if (cap == .square) {
                const first_direction = normalized(first, second) orelse continue;
                const last_direction = normalized(before_last, last) orelse continue;
                if (insideSquareCap(point, first, first_direction, radius, true) or insideSquareCap(point, last, last_direction, radius, false)) return true;
            }
        }
    }
    return false;
}

fn spreadRoundedRect(rect: RoundedRect, spread: f32, offset_x: f32, offset_y: f32) RoundedRect {
    return normalizeRoundedRect(.{
        .x = rect.x - spread + offset_x,
        .y = rect.y - spread + offset_y,
        .w = rect.w + 2 * spread,
        .h = rect.h + 2 * spread,
        .radii = .{
            .tlx = @max(0, rect.radii.tlx + spread),
            .tly = @max(0, rect.radii.tly + spread),
            .trx = @max(0, rect.radii.trx + spread),
            .try_ = @max(0, rect.radii.try_ + spread),
            .brx = @max(0, rect.radii.brx + spread),
            .bry = @max(0, rect.radii.bry + spread),
            .blx = @max(0, rect.radii.blx + spread),
            .bly = @max(0, rect.radii.bly + spread),
        },
    });
}

fn shapeContains(parsed: Parsed, geometry: ?*const Geometry, point: Point) bool {
    return if (parsed.geometry_kind == r4os.abi.gui_shape_geometry_kind_path)
        insidePath(geometry.?, point, parsed.fill_rule)
    else
        insideRoundedRect(parsed.rounded, point);
}

fn shadowShapeContains(parsed: Parsed, geometry: ?*const Geometry, point: Point) bool {
    if (parsed.geometry_kind == r4os.abi.gui_shape_geometry_kind_rounded_rect) {
        return insideRoundedRect(spreadRoundedRect(parsed.rounded, parsed.shadow_spread, parsed.shadow_offset_x, parsed.shadow_offset_y), point);
    }
    const local = Point{ .x = point.x - parsed.shadow_offset_x, .y = point.y - parsed.shadow_offset_y };
    const inside = insidePath(geometry.?, local, parsed.fill_rule);
    if (parsed.shadow_spread == 0) return inside;
    const edge_distance = distanceToPath(geometry.?, local);
    return if (parsed.shadow_spread > 0)
        inside or edge_distance <= parsed.shadow_spread
    else
        inside and edge_distance >= -parsed.shadow_spread;
}

fn coverageFor(parsed: Parsed, x: usize, y: usize, predicate: anytype) u8 {
    var covered: u32 = 0;
    for (sample_offsets) |sample| {
        const point = Point{ .x = @as(f32, @floatFromInt(x)) + sample.x, .y = @as(f32, @floatFromInt(y)) + sample.y };
        if (parsed.clip.contains(point) and predicate.contains(point)) covered += 1;
    }
    return @intCast((covered * 255 + 2) / sample_offsets.len);
}

fn fillMask(mask: []u8, width: usize, height: usize, parsed: Parsed, predicate: anytype) void {
    var y: usize = 0;
    while (y < height) : (y += 1) {
        var x: usize = 0;
        while (x < width) : (x += 1) mask[y * width + x] = coverageFor(parsed, x, y, predicate);
    }
}

fn applyColorAlpha(mask: []u8, argb: u32) bool {
    const alpha: u32 = argb >> 24;
    if (alpha == 0) return false;
    if (alpha != 255) {
        for (mask) |*value| value.* = @intCast((@as(u32, value.*) * alpha + 127) / 255);
    }
    return true;
}

fn blendMask(scene: *scene_buffer.SceneBuffer, bounds: surface.Rect, command: r4os.abi.GuiFrameCommand, mask: []u8, argb: u32) bool {
    if (!applyColorAlpha(mask, argb)) return false;
    const destination_x = @as(i64, bounds.x) + @as(i64, command.x);
    const destination_y = @as(i64, bounds.y) + @as(i64, command.y);
    const left = @max(destination_x, @as(i64, bounds.x));
    const top = @max(destination_y, @as(i64, bounds.y));
    const right = @min(destination_x + command.w, @as(i64, bounds.right()));
    const bottom = @min(destination_y + command.h, @as(i64, bounds.bottom()));
    if (right <= left or bottom <= top) return false;
    if (left < std.math.minInt(i32) or top < std.math.minInt(i32) or right > std.math.maxInt(i32) or bottom > std.math.maxInt(i32)) return false;
    const source_x: usize = @intCast(left - destination_x);
    const source_y: usize = @intCast(top - destination_y);
    const stride: usize = command.w;
    const offset = source_y * stride + source_x;
    return scene.blendAlpha8(
        @intCast(left),
        @intCast(top),
        @intCast(right - left),
        @intCast(bottom - top),
        command.w,
        argb & 0x00FF_FFFF,
        mask[offset..],
    );
}

fn boxBlurHorizontal(source: []const u8, destination: []u8, width: usize, height: usize, radius: usize) void {
    if (radius == 0) {
        @memcpy(destination, source);
        return;
    }
    const divisor: u32 = @intCast(radius * 2 + 1);
    var y: usize = 0;
    while (y < height) : (y += 1) {
        var sum: u32 = 0;
        var initial: usize = 0;
        while (initial <= radius and initial < width) : (initial += 1) sum += source[y * width + initial];
        var x: usize = 0;
        while (x < width) : (x += 1) {
            destination[y * width + x] = @intCast((sum + divisor / 2) / divisor);
            if (x >= radius) sum -= source[y * width + x - radius];
            const add = x + radius + 1;
            if (add < width) sum += source[y * width + add];
        }
    }
}

fn boxBlurVertical(source: []const u8, destination: []u8, width: usize, height: usize, radius: usize) void {
    if (radius == 0) {
        @memcpy(destination, source);
        return;
    }
    const divisor: u32 = @intCast(radius * 2 + 1);
    var x: usize = 0;
    while (x < width) : (x += 1) {
        var sum: u32 = 0;
        var initial: usize = 0;
        while (initial <= radius and initial < height) : (initial += 1) sum += source[initial * width + x];
        var y: usize = 0;
        while (y < height) : (y += 1) {
            destination[y * width + x] = @intCast((sum + divisor / 2) / divisor);
            if (y >= radius) sum -= source[(y - radius) * width + x];
            const add = y + radius + 1;
            if (add < height) sum += source[add * width + x];
        }
    }
}

pub fn replay(
    allocator: std.mem.Allocator,
    scene: *scene_buffer.SceneBuffer,
    bounds: surface.Rect,
    command: r4os.abi.GuiFrameCommand,
    resource: []const u8,
) Result {
    const parsed = parse(command, resource) orelse return .invalid;
    var geometry_storage = Geometry{};
    var geometry: ?*const Geometry = null;
    if (parsed.geometry_kind == r4os.abi.gui_shape_geometry_kind_path) {
        geometry_storage = buildGeometry(allocator, parsed) catch |err| return if (err == error.OutOfMemory) .out_of_memory else .invalid;
        geometry = &geometry_storage;
    }
    defer geometry_storage.deinit(allocator);

    const width: usize = command.w;
    const height: usize = command.h;
    const pixel_count = std.math.mul(usize, width, height) catch return .invalid;
    const mask = allocator.alloc(u8, pixel_count) catch return .out_of_memory;
    defer allocator.free(mask);
    var drew = false;

    switch (command.kind) {
        r4os.abi.gui_frame_command_kind_path_fill => {
            fillMask(mask, width, height, parsed, struct {
                parsed: Parsed,
                geometry: *const Geometry,
                fn contains(self: @This(), point: Point) bool {
                    return insidePath(self.geometry, point, self.parsed.fill_rule);
                }
            }{ .parsed = parsed, .geometry = geometry.? });
            drew = blendMask(scene, bounds, command, mask, parsed.fill_argb);
        },
        r4os.abi.gui_frame_command_kind_path_stroke => {
            fillMask(mask, width, height, parsed, struct {
                geometry: *const Geometry,
                width: f32,
                join: LineJoin,
                cap: LineCap,
                miter: f32,
                fn contains(self: @This(), point: Point) bool {
                    return insideStroke(self.geometry, point, self.width, self.join, self.cap, self.miter);
                }
            }{ .geometry = geometry.?, .width = parsed.stroke_width, .join = parsed.line_join.?, .cap = parsed.line_cap.?, .miter = parsed.miter_limit });
            drew = blendMask(scene, bounds, command, mask, parsed.stroke_argb);
        },
        r4os.abi.gui_frame_command_kind_rounded_rect => {
            const outer = normalizeRoundedRect(parsed.rounded);
            fillMask(mask, width, height, parsed, struct {
                rect: RoundedRect,
                fn contains(self: @This(), point: Point) bool {
                    return insideRoundedRect(self.rect, point);
                }
            }{ .rect = outer });
            drew = blendMask(scene, bounds, command, mask, parsed.fill_argb) or drew;
            const inner = innerRoundedRect(parsed);
            fillMask(mask, width, height, parsed, struct {
                outer: RoundedRect,
                inner: ?RoundedRect,
                fn contains(self: @This(), point: Point) bool {
                    return insideRoundedRect(self.outer, point) and (self.inner == null or !insideRoundedRect(self.inner.?, point));
                }
            }{ .outer = outer, .inner = inner });
            drew = blendMask(scene, bounds, command, mask, parsed.stroke_argb) or drew;
        },
        r4os.abi.gui_frame_command_kind_shadow => {
            fillMask(mask, width, height, parsed, struct {
                parsed: Parsed,
                geometry: ?*const Geometry,
                fn contains(self: @This(), point: Point) bool {
                    return shadowShapeContains(self.parsed, self.geometry, point);
                }
            }{ .parsed = parsed, .geometry = geometry });
            const radius: usize = @intFromFloat(@ceil(parsed.shadow_blur));
            if (radius != 0) {
                const scratch = allocator.alloc(u8, pixel_count) catch return .out_of_memory;
                defer allocator.free(scratch);
                boxBlurHorizontal(mask, scratch, width, height, radius);
                boxBlurVertical(scratch, mask, width, height, radius);
            }
            if ((parsed.flags & r4os.abi.gui_shape_flag_shadow_inset) != 0) {
                var y: usize = 0;
                while (y < height) : (y += 1) {
                    var x: usize = 0;
                    while (x < width) : (x += 1) {
                        const original = coverageFor(parsed, x, y, struct {
                            parsed: Parsed,
                            geometry: ?*const Geometry,
                            fn contains(self: @This(), point: Point) bool {
                                return shapeContains(self.parsed, self.geometry, point);
                            }
                        }{ .parsed = parsed, .geometry = geometry });
                        mask[y * width + x] = @intCast((@as(u32, original) * (255 - @as(u32, mask[y * width + x])) + 127) / 255);
                    }
                }
            }
            drew = blendMask(scene, bounds, command, mask, parsed.shadow_argb);
        },
        else => return .invalid,
    }
    return if (drew) .drawn else .empty;
}

fn pixelHash(pixels: []const u32) u64 {
    var hash: u64 = 1469598103934665603;
    for (pixels) |pixel| {
        var value = pixel;
        var byte: usize = 0;
        while (byte < 4) : (byte += 1) {
            hash = (hash ^ @as(u8, @truncate(value))) *% 1099511628211;
            value >>= 8;
        }
    }
    return hash;
}

fn writeU32(bytes: []u8, offset: usize, value: u32) void {
    bytes[offset] = @truncate(value);
    bytes[offset + 1] = @truncate(value >> 8);
    bytes[offset + 2] = @truncate(value >> 16);
    bytes[offset + 3] = @truncate(value >> 24);
}

test "rounded rectangle golden covers elliptical corners unequal borders clipping and alpha" {
    var pixels: [48 * 36]u32 = .{0x203040} ** (48 * 36);
    var scene = scene_buffer.SceneBuffer{};
    try std.testing.expect(scene.attach(std.mem.sliceAsBytes(pixels[0..]), 48, 36));
    var resource_bytes: [@sizeOf(r4os.abi.GuiShapeResource)]u8 = undefined;
    const resource = try r4os.gui_shapes.roundedRect(resource_bytes[0..], .{
        .x = 4,
        .y = 3,
        .w = 38,
        .h = 28,
        .radii = .{
            .top_left_x = 2,
            .top_left_y = 8,
            .top_right_x = 12,
            .top_right_y = 4,
            .bottom_right_x = 7,
            .bottom_right_y = 11,
            .bottom_left_x = 14,
            .bottom_left_y = 6,
        },
        .borders = .{ .top = 1, .right = 3, .bottom = 5, .left = 2 },
        .fill_argb = 0xC04080C0,
        .border_argb = 0xE0F08020,
        .clip = .{ .x = 2, .y = 1, .w = 44, .h = 32 },
    });
    const command = try r4os.gui_shapes.command(r4os.abi.gui_frame_command_kind_rounded_rect, 0, 0, 48, 36, 0, resource.len);
    try std.testing.expectEqual(Result.drawn, replay(std.testing.allocator, &scene, scene.fullRect(), command, resource));
    try std.testing.expectEqual(@as(u64, 0x42E1A133AE5B1F7E), pixelHash(pixels[0..]));
}

test "path golden covers cubic circle fill and anti-aliased stroke" {
    var pixels: [40 * 40]u32 = .{0xF5F5F5} ** (40 * 40);
    var scene = scene_buffer.SceneBuffer{};
    try std.testing.expect(scene.attach(std.mem.sliceAsBytes(pixels[0..]), 40, 40));
    var bytes: [@sizeOf(r4os.abi.GuiShapeResource) + 6 * @sizeOf(r4os.abi.GuiPathSegment)]u8 = undefined;
    var builder = try r4os.gui_shapes.PathBuilder.init(bytes[0..], .{
        .fill_argb = 0xB02070D0,
        .stroke_argb = 0xF0D04020,
        .stroke_width = 2.5,
        .line_join = .round,
        .line_cap = .round,
    });
    const k: f32 = 0.55228475 * 14;
    try builder.moveTo(.{ .x = 34, .y = 20 });
    try builder.cubicTo(.{ .x = 34, .y = 20 + k }, .{ .x = 20 + k, .y = 34 }, .{ .x = 20, .y = 34 });
    try builder.cubicTo(.{ .x = 20 - k, .y = 34 }, .{ .x = 6, .y = 20 + k }, .{ .x = 6, .y = 20 });
    try builder.cubicTo(.{ .x = 6, .y = 20 - k }, .{ .x = 20 - k, .y = 6 }, .{ .x = 20, .y = 6 });
    try builder.cubicTo(.{ .x = 20 + k, .y = 6 }, .{ .x = 34, .y = 20 - k }, .{ .x = 34, .y = 20 });
    try builder.close();
    const resource = try builder.finish();
    const fill = try r4os.gui_shapes.command(r4os.abi.gui_frame_command_kind_path_fill, 0, 0, 40, 40, 0, resource.len);
    const stroke = try r4os.gui_shapes.command(r4os.abi.gui_frame_command_kind_path_stroke, 0, 0, 40, 40, 0, resource.len);
    try std.testing.expectEqual(Result.drawn, replay(std.testing.allocator, &scene, scene.fullRect(), fill, resource));
    try std.testing.expectEqual(Result.drawn, replay(std.testing.allocator, &scene, scene.fullRect(), stroke, resource));
    try std.testing.expectEqual(@as(u64, 0x60DE7ABF2DCF1CFB), pixelHash(pixels[0..]));
}

test "shadow golden covers separate outer and inset layers" {
    var pixels: [52 * 40]u32 = .{0xD8D8D8} ** (52 * 40);
    var scene = scene_buffer.SceneBuffer{};
    try std.testing.expect(scene.attach(std.mem.sliceAsBytes(pixels[0..]), 52, 40));
    var outer_bytes: [@sizeOf(r4os.abi.GuiShapeResource)]u8 = undefined;
    const outer = try r4os.gui_shapes.roundedRect(outer_bytes[0..], .{
        .x = 10,
        .y = 8,
        .w = 30,
        .h = 21,
        .radii = .{ .top_left_x = 7, .top_left_y = 7, .top_right_x = 7, .top_right_y = 7, .bottom_right_x = 7, .bottom_right_y = 7, .bottom_left_x = 7, .bottom_left_y = 7 },
        .fill_argb = 0xFFF8F8F8,
        .border_argb = 0xFF4070A0,
        .borders = .{ .top = 1, .right = 1, .bottom = 1, .left = 1 },
        .shadow = .{ .argb = 0x80000000, .offset_x = 2, .offset_y = 3, .spread = 2, .blur = 4 },
    });
    const shadow_command = try r4os.gui_shapes.command(r4os.abi.gui_frame_command_kind_shadow, 0, 0, 52, 40, 0, outer.len);
    const rect_command = try r4os.gui_shapes.command(r4os.abi.gui_frame_command_kind_rounded_rect, 0, 0, 52, 40, 0, outer.len);
    try std.testing.expectEqual(Result.drawn, replay(std.testing.allocator, &scene, scene.fullRect(), shadow_command, outer));
    try std.testing.expectEqual(Result.drawn, replay(std.testing.allocator, &scene, scene.fullRect(), rect_command, outer));

    var inset_bytes: [@sizeOf(r4os.abi.GuiShapeResource)]u8 = undefined;
    const inset = try r4os.gui_shapes.roundedRect(inset_bytes[0..], .{
        .x = 10,
        .y = 8,
        .w = 30,
        .h = 21,
        .radii = .{ .top_left_x = 7, .top_left_y = 7, .top_right_x = 7, .top_right_y = 7, .bottom_right_x = 7, .bottom_right_y = 7, .bottom_left_x = 7, .bottom_left_y = 7 },
        .shadow = .{ .argb = 0x70000000, .offset_x = 1, .offset_y = 1, .blur = 3, .inset = true },
    });
    const inset_command = try r4os.gui_shapes.command(r4os.abi.gui_frame_command_kind_shadow, 0, 0, 52, 40, 0, inset.len);
    try std.testing.expectEqual(Result.drawn, replay(std.testing.allocator, &scene, scene.fullRect(), inset_command, inset));
    try std.testing.expectEqual(@as(u64, 0xC80D01C04D523D7D), pixelHash(pixels[0..]));
}

test "one logical command covers tiny and larger rounded shapes" {
    var tiny_pixels: [4 * 4]u32 = .{0} ** 16;
    var tiny_scene = scene_buffer.SceneBuffer{};
    try std.testing.expect(tiny_scene.attach(std.mem.sliceAsBytes(tiny_pixels[0..]), 4, 4));
    var tiny_bytes: [@sizeOf(r4os.abi.GuiShapeResource)]u8 = undefined;
    const tiny = try r4os.gui_shapes.roundedRect(tiny_bytes[0..], .{ .x = 1, .y = 1, .w = 2, .h = 2, .radii = .{ .top_left_x = 4, .top_left_y = 4, .top_right_x = 4, .top_right_y = 4, .bottom_right_x = 4, .bottom_right_y = 4, .bottom_left_x = 4, .bottom_left_y = 4 }, .fill_argb = 0xFFFFFFFF });
    const tiny_command = try r4os.gui_shapes.command(r4os.abi.gui_frame_command_kind_rounded_rect, 0, 0, 4, 4, 0, tiny.len);
    try std.testing.expectEqual(Result.drawn, replay(std.testing.allocator, &tiny_scene, tiny_scene.fullRect(), tiny_command, tiny));

    const large_width = 257;
    const large_height = 129;
    var large_pixels: [large_width * large_height]u32 = .{0x101010} ** (large_width * large_height);
    var large_scene = scene_buffer.SceneBuffer{};
    try std.testing.expect(large_scene.attach(std.mem.sliceAsBytes(large_pixels[0..]), large_width, large_height));
    var large_bytes: [@sizeOf(r4os.abi.GuiShapeResource)]u8 = undefined;
    const large = try r4os.gui_shapes.roundedRect(large_bytes[0..], .{ .x = 2, .y = 2, .w = 253, .h = 125, .radii = .{ .top_left_x = 50, .top_left_y = 30, .top_right_x = 20, .top_right_y = 55, .bottom_right_x = 70, .bottom_right_y = 18, .bottom_left_x = 9, .bottom_left_y = 45 }, .fill_argb = 0xFF336699 });
    const large_command = try r4os.gui_shapes.command(r4os.abi.gui_frame_command_kind_rounded_rect, 0, 0, large_width, large_height, 0, large.len);
    try std.testing.expectEqual(Result.drawn, replay(std.testing.allocator, &large_scene, large_scene.fullRect(), large_command, large));
    try std.testing.expectEqual(@as(u64, 0x67F14B9ED59A9BE3), pixelHash(tiny_pixels[0..]));
    try std.testing.expectEqual(@as(u64, 0xAC46157755C83AC6), pixelHash(large_pixels[0..]));
}

test "malformed shape resources are rejected before replay" {
    var pixels: [16 * 16]u32 = .{0x123456} ** (16 * 16);
    var scene = scene_buffer.SceneBuffer{};
    try std.testing.expect(scene.attach(std.mem.sliceAsBytes(pixels[0..]), 16, 16));
    var bytes: [@sizeOf(r4os.abi.GuiShapeResource) + 2 * @sizeOf(r4os.abi.GuiPathSegment)]u8 = undefined;
    var builder = try r4os.gui_shapes.PathBuilder.init(bytes[0..], .{ .fill_argb = 0xFFFFFFFF });
    try builder.moveTo(.{ .x = 1, .y = 1 });
    try builder.lineTo(.{ .x = 8, .y = 8 });
    const resource = try builder.finish();
    const command = try r4os.gui_shapes.command(r4os.abi.gui_frame_command_kind_path_fill, 0, 0, 16, 16, 0, resource.len);
    try std.testing.expectEqual(Result.invalid, replay(std.testing.allocator, &scene, scene.fullRect(), command, resource[0 .. resource.len - 1]));
    const segment_kind_offset = @sizeOf(r4os.abi.GuiShapeResource) + @offsetOf(r4os.abi.GuiPathSegment, "kind");
    writeU32(bytes[0..], segment_kind_offset, 99);
    try std.testing.expectEqual(Result.invalid, replay(std.testing.allocator, &scene, scene.fullRect(), command, resource));
    writeU32(bytes[0..], segment_kind_offset, r4os.abi.gui_path_segment_kind_move);
    writeU32(bytes[0..], @sizeOf(r4os.abi.GuiShapeResource) + @offsetOf(r4os.abi.GuiPathSegment, "x1_bits"), 0x7F800000);
    try std.testing.expectEqual(Result.invalid, replay(std.testing.allocator, &scene, scene.fullRect(), command, resource));
    var oversized = command;
    oversized.w = r4os.abi.gui_shape_max_dimension + 1;
    try std.testing.expectEqual(Result.invalid, replay(std.testing.allocator, &scene, scene.fullRect(), oversized, resource));
    var foreign = command;
    foreign.kind = r4os.abi.gui_frame_command_kind_rounded_rect;
    try std.testing.expectEqual(Result.invalid, replay(std.testing.allocator, &scene, scene.fullRect(), foreign, resource));
}
