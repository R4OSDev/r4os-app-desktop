const std = @import("std");
const surface = @import("surface.zig");

pub const SceneBuffer = struct {
    memory: ?[]u8 = null,
    pixels: ?[]u32 = null,
    width: i32 = 0,
    height: i32 = 0,
    // During an incremental desktop present only this rectangle is repainted.
    // Keeping it on the scene avoids replaying a whole hosted application for
    // every cursor movement while the committed framebuffer remains complete.
    paint_clip: ?surface.Rect = null,

    pub fn requiredBytes(width: i32, height: i32) ?usize {
        if (width <= 0 or height <= 0) return null;
        const w: usize = @intCast(width);
        const h: usize = @intCast(height);
        const count = std.math.mul(usize, w, h) catch return null;
        return std.math.mul(usize, count, @sizeOf(u32)) catch return null;
    }

    pub fn attach(self: *SceneBuffer, memory: []u8, width: i32, height: i32) bool {
        const bytes = requiredBytes(width, height) orelse return false;
        if (memory.len < bytes) return false;
        const count = bytes / @sizeOf(u32);
        const aligned: [*]align(@alignOf(u32)) u8 = @alignCast(memory.ptr);
        const raw: [*]u32 = @ptrCast(aligned);
        self.* = .{
            .memory = memory,
            .pixels = raw[0..count],
            .width = width,
            .height = height,
        };
        return true;
    }

    pub fn reset(self: *SceneBuffer) void {
        self.* = .{};
    }

    pub fn setPaintClip(self: *SceneBuffer, rect: surface.Rect) void {
        self.paint_clip = self.clipRect(rect);
    }

    pub fn clearPaintClip(self: *SceneBuffer) void {
        self.paint_clip = null;
    }

    pub fn paintBounds(self: *const SceneBuffer) surface.Rect {
        return self.paint_clip orelse self.fullRect();
    }

    pub fn matches(self: *const SceneBuffer, width: i32, height: i32) bool {
        return self.pixels != null and self.width == width and self.height == height;
    }

    pub fn fullRect(self: *const SceneBuffer) surface.Rect {
        return .{ .x = 0, .y = 0, .w = self.width, .h = self.height };
    }

    pub fn clipRect(self: *const SceneBuffer, rect: surface.Rect) ?surface.Rect {
        if (self.pixels == null or rect.isEmpty() or self.width <= 0 or self.height <= 0) return null;
        const left = @max(0, rect.x);
        const top = @max(0, rect.y);
        const right = @min(self.width, rect.right());
        const bottom = @min(self.height, rect.bottom());
        if (right <= left or bottom <= top) return null;
        return .{ .x = left, .y = top, .w = right - left, .h = bottom - top };
    }

    fn paintClipRect(self: *const SceneBuffer, rect: surface.Rect) ?surface.Rect {
        const surface_clip = self.clipRect(rect) orelse return null;
        const paint_clip = self.paint_clip orelse return surface_clip;
        const left = @max(surface_clip.x, paint_clip.x);
        const top = @max(surface_clip.y, paint_clip.y);
        const right = @min(surface_clip.right(), paint_clip.right());
        const bottom = @min(surface_clip.bottom(), paint_clip.bottom());
        if (right <= left or bottom <= top) return null;
        return .{ .x = left, .y = top, .w = right - left, .h = bottom - top };
    }

    pub fn fillRect(self: *SceneBuffer, rect: surface.Rect, rgb: u32) void {
        const clipped = self.paintClipRect(rect) orelse return;
        const pixels = self.pixels orelse return;
        const color = rgb & 0x00FF_FFFF;
        const width: usize = @intCast(self.width);
        const left: usize = @intCast(clipped.x);
        const row_count: usize = @intCast(clipped.w);
        var y: i32 = clipped.y;
        while (y < clipped.bottom()) : (y += 1) {
            const row: usize = @intCast(y);
            const offset = row * width + left;
            @memset(pixels[offset .. offset + row_count], color);
        }
    }

    pub fn blitXrgb32(self: *SceneBuffer, x: i32, y: i32, w: u32, h: u32, source: []const u32) void {
        if (w == 0 or h == 0) return;
        if (w > @as(u32, @intCast(std.math.maxInt(i32))) or h > @as(u32, @intCast(std.math.maxInt(i32)))) return;
        const src_w: i32 = @intCast(w);
        const src_h: i32 = @intCast(h);
        const needed = @as(usize, @intCast(w)) * @as(usize, @intCast(h));
        if (source.len < needed) return;

        const clipped = self.paintClipRect(.{ .x = x, .y = y, .w = src_w, .h = src_h }) orelse return;
        const pixels = self.pixels orelse return;
        const dst_stride: usize = @intCast(self.width);
        const src_stride: usize = @intCast(w);
        const copy_count: usize = @intCast(clipped.w);
        const dst_x: usize = @intCast(clipped.x);
        const src_x: usize = @intCast(clipped.x - x);
        const src_y0: usize = @intCast(clipped.y - y);

        var row: i32 = 0;
        while (row < clipped.h) : (row += 1) {
            const dst_y: usize = @intCast(clipped.y + row);
            const src_y = src_y0 + @as(usize, @intCast(row));
            const src_offset = src_y * src_stride + src_x;
            const dst_offset = dst_y * dst_stride + dst_x;
            @memcpy(pixels[dst_offset .. dst_offset + copy_count], source[src_offset .. src_offset + copy_count]);
        }
    }

    pub const Indexed8Nearest = struct {
        indices: []const u8,
        palette: []const u32,
        source_x: u32,
        source_y: u32,
        source_w: u32,
        source_h: u32,
        source_stride: u32,
        guest_w: u32,
        guest_h: u32,
        viewport: surface.Rect,
    };

    pub const Xrgb32Nearest = struct {
        pixels: []align(1) const u32,
        source_x: u32,
        source_y: u32,
        source_w: u32,
        source_h: u32,
        source_stride: u32,
        guest_w: u32,
        guest_h: u32,
        viewport: surface.Rect,
    };

    /// Replays one native XRGB32 source block without converting it into
    /// color runs. Exact-size viewports use row copies, integer viewports
    /// expand one row and repeat it, and all other sizes use global nearest
    /// mapping so independently transported blocks meet without seams.
    pub fn blitXrgb32Nearest(self: *SceneBuffer, clip: surface.Rect, image: Xrgb32Nearest) bool {
        if (image.source_w == 0 or image.source_h == 0 or image.source_stride < image.source_w or
            image.guest_w == 0 or image.guest_h == 0 or image.viewport.w <= 0 or image.viewport.h <= 0 or
            @as(u64, image.source_x) + image.source_w > image.guest_w or
            @as(u64, image.source_y) + image.source_h > image.guest_h)
        {
            return false;
        }
        const preceding_rows = std.math.mul(usize, @as(usize, image.source_h - 1), @as(usize, image.source_stride)) catch return false;
        const required = std.math.add(usize, preceding_rows, @as(usize, image.source_w)) catch return false;
        if (image.pixels.len < required) return false;

        const scene_clip = self.paintClipRect(clip) orelse return true;
        const left = @max(scene_clip.x, image.viewport.x);
        const top = @max(scene_clip.y, image.viewport.y);
        const right = @min(scene_clip.right(), image.viewport.right());
        const bottom = @min(scene_clip.bottom(), image.viewport.bottom());
        if (right <= left or bottom <= top) return true;

        if (image.viewport.w == @as(i32, @intCast(image.guest_w)) and image.viewport.h == @as(i32, @intCast(image.guest_h))) {
            return self.blitXrgb32Identity(left, top, right, bottom, image);
        }
        if (@rem(image.viewport.w, @as(i32, @intCast(image.guest_w))) == 0 and
            @rem(image.viewport.h, @as(i32, @intCast(image.guest_h))) == 0)
        {
            const scale_x = @divTrunc(image.viewport.w, @as(i32, @intCast(image.guest_w)));
            const scale_y = @divTrunc(image.viewport.h, @as(i32, @intCast(image.guest_h)));
            if (scale_x > 0 and scale_x == scale_y) return self.blitXrgb32Integer(left, top, right, bottom, image, scale_x);
        }
        return self.blitXrgb32Fractional(left, top, right, bottom, image);
    }

    fn blitXrgb32Identity(self: *SceneBuffer, left: i32, top: i32, right: i32, bottom: i32, image: Xrgb32Nearest) bool {
        const destination = self.pixels orelse return false;
        const destination_stride: usize = @intCast(self.width);
        const copy_count: usize = @intCast(right - left);
        const guest_x: u32 = @intCast(left - image.viewport.x);
        if (guest_x < image.source_x or @as(u64, guest_x) + copy_count > @as(u64, image.source_x) + image.source_w) return false;
        var destination_y = top;
        while (destination_y < bottom) : (destination_y += 1) {
            const guest_y: u32 = @intCast(destination_y - image.viewport.y);
            if (guest_y < image.source_y or guest_y >= image.source_y + image.source_h) return false;
            const source_offset = @as(usize, guest_y - image.source_y) * image.source_stride + (guest_x - image.source_x);
            const destination_offset = @as(usize, @intCast(destination_y)) * destination_stride + @as(usize, @intCast(left));
            @memcpy(destination[destination_offset .. destination_offset + copy_count], image.pixels[source_offset .. source_offset + copy_count]);
        }
        return true;
    }

    fn blitXrgb32Integer(self: *SceneBuffer, left: i32, top: i32, right: i32, bottom: i32, image: Xrgb32Nearest, scale: i32) bool {
        const destination = self.pixels orelse return false;
        const destination_stride: usize = @intCast(self.width);
        const first_guest_y: u32 = @intCast(@divTrunc(top - image.viewport.y, scale));
        const last_guest_y: u32 = @intCast(@divTrunc(bottom - 1 - image.viewport.y, scale));
        var guest_y = first_guest_y;
        while (guest_y <= last_guest_y) : (guest_y += 1) {
            if (guest_y < image.source_y or guest_y >= image.source_y + image.source_h) return false;
            const row_top = @max(top, image.viewport.y + @as(i32, @intCast(guest_y)) * scale);
            const row_bottom = @min(bottom, image.viewport.y + (@as(i32, @intCast(guest_y)) + 1) * scale);
            const destination_row = @as(usize, @intCast(row_top)) * destination_stride;
            var guest_x: u32 = @intCast(@divTrunc(left - image.viewport.x, scale));
            const last_guest_x: u32 = @intCast(@divTrunc(right - 1 - image.viewport.x, scale));
            while (guest_x <= last_guest_x) : (guest_x += 1) {
                if (guest_x < image.source_x or guest_x >= image.source_x + image.source_w) return false;
                const run_left = @max(left, image.viewport.x + @as(i32, @intCast(guest_x)) * scale);
                const run_right = @min(right, image.viewport.x + (@as(i32, @intCast(guest_x)) + 1) * scale);
                const source_index = @as(usize, guest_y - image.source_y) * image.source_stride + (guest_x - image.source_x);
                @memset(destination[destination_row + @as(usize, @intCast(run_left)) .. destination_row + @as(usize, @intCast(run_right))], image.pixels[source_index]);
            }
            var destination_y = row_top + 1;
            while (destination_y < row_bottom) : (destination_y += 1) {
                const repeat_row = @as(usize, @intCast(destination_y)) * destination_stride;
                @memcpy(destination[repeat_row + @as(usize, @intCast(left)) .. repeat_row + @as(usize, @intCast(right))], destination[destination_row + @as(usize, @intCast(left)) .. destination_row + @as(usize, @intCast(right))]);
            }
        }
        return true;
    }

    fn blitXrgb32Fractional(self: *SceneBuffer, left: i32, top: i32, right: i32, bottom: i32, image: Xrgb32Nearest) bool {
        const destination = self.pixels orelse return false;
        const destination_stride: usize = @intCast(self.width);
        const viewport_w: u64 = @intCast(image.viewport.w);
        const viewport_h: u64 = @intCast(image.viewport.h);
        var destination_y = top;
        while (destination_y < bottom) : (destination_y += 1) {
            const viewport_y: u64 = @intCast(destination_y - image.viewport.y);
            const guest_y: u32 = @intCast((viewport_y * image.guest_h) / viewport_h);
            if (guest_y < image.source_y or guest_y >= image.source_y + image.source_h) return false;
            const source_row = @as(usize, guest_y - image.source_y) * image.source_stride;
            const destination_row = @as(usize, @intCast(destination_y)) * destination_stride;
            var destination_x = left;
            while (destination_x < right) : (destination_x += 1) {
                const viewport_x: u64 = @intCast(destination_x - image.viewport.x);
                const guest_x: u32 = @intCast((viewport_x * image.guest_w) / viewport_w);
                if (guest_x < image.source_x or guest_x >= image.source_x + image.source_w) return false;
                destination[destination_row + @as(usize, @intCast(destination_x))] = image.pixels[source_row + @as(usize, guest_x - image.source_x)];
            }
        }
        return true;
    }

    /// Replays one bounded Indexed8 source block directly into the XRGB scene.
    /// The full guest/viewport mapping keeps adjacent blocks pixel-identical;
    /// palette expansion happens once per source block instead of once per
    /// scaled destination pixel.
    pub fn blitIndexed8Nearest(self: *SceneBuffer, clip: surface.Rect, image: Indexed8Nearest) bool {
        if (image.source_w == 0 or image.source_h == 0 or image.source_stride < image.source_w or
            image.guest_w == 0 or image.guest_h == 0 or image.viewport.w <= 0 or image.viewport.h <= 0 or
            image.palette.len < 256 or
            @as(u64, image.source_x) + image.source_w > image.guest_w or
            @as(u64, image.source_y) + image.source_h > image.guest_h)
        {
            return false;
        }
        const preceding_rows = std.math.mul(usize, @as(usize, image.source_h - 1), @as(usize, image.source_stride)) catch return false;
        const required = std.math.add(usize, preceding_rows, @as(usize, image.source_w)) catch return false;
        if (image.indices.len < required) return false;

        const scene_clip = self.paintClipRect(clip) orelse return true;
        const left = @max(scene_clip.x, image.viewport.x);
        const top = @max(scene_clip.y, image.viewport.y);
        const right = @min(scene_clip.right(), image.viewport.right());
        const bottom = @min(scene_clip.bottom(), image.viewport.bottom());
        if (right <= left or bottom <= top) return true;

        const pixels = self.pixels orelse return false;
        const destination_stride: usize = @intCast(self.width);
        const viewport_w: u64 = @intCast(image.viewport.w);
        const viewport_h: u64 = @intCast(image.viewport.h);
        var destination_y = top;
        while (destination_y < bottom) : (destination_y += 1) {
            const viewport_y: u64 = @intCast(destination_y - image.viewport.y);
            const guest_y: u32 = @intCast((viewport_y * image.guest_h) / viewport_h);
            if (guest_y < image.source_y or guest_y >= image.source_y + image.source_h) return false;
            const source_row = @as(usize, guest_y - image.source_y) * image.source_stride;
            const destination_row = @as(usize, @intCast(destination_y)) * destination_stride;
            var destination_x = left;
            while (destination_x < right) : (destination_x += 1) {
                const viewport_x: u64 = @intCast(destination_x - image.viewport.x);
                const guest_x: u32 = @intCast((viewport_x * image.guest_w) / viewport_w);
                if (guest_x < image.source_x or guest_x >= image.source_x + image.source_w) return false;
                const source_index = source_row + @as(usize, guest_x - image.source_x);
                const palette_index = image.indices[source_index];
                pixels[destination_row + @as(usize, @intCast(destination_x))] = image.palette[palette_index] & 0x00FF_FFFF;
            }
        }
        return true;
    }

    /// Blends an Alpha8 coverage mask over the current XRGB scene.  The source
    /// may contain row padding; clipping advances into the original stride.
    pub fn blendAlpha8(self: *SceneBuffer, x: i32, y: i32, w: u32, h: u32, stride: u32, rgb: u32, alpha: []const u8) bool {
        if (w == 0 or h == 0 or stride < w) return false;
        if (w > @as(u32, @intCast(std.math.maxInt(i32))) or h > @as(u32, @intCast(std.math.maxInt(i32)))) return false;
        const preceding_rows = std.math.mul(usize, @as(usize, h - 1), @as(usize, stride)) catch return false;
        const required = std.math.add(usize, preceding_rows, @as(usize, w)) catch return false;
        if (alpha.len < required) return false;

        const clipped = self.paintClipRect(.{ .x = x, .y = y, .w = @intCast(w), .h = @intCast(h) }) orelse return true;
        const pixels = self.pixels orelse return false;
        const destination_stride: usize = @intCast(self.width);
        const source_stride: usize = @intCast(stride);
        const source_x: usize = @intCast(clipped.x - x);
        const source_y: usize = @intCast(clipped.y - y);
        const destination_x: usize = @intCast(clipped.x);
        const copy_width: usize = @intCast(clipped.w);

        var row: i32 = 0;
        while (row < clipped.h) : (row += 1) {
            const destination_y: usize = @intCast(clipped.y + row);
            const source_row = (source_y + @as(usize, @intCast(row))) * source_stride + source_x;
            const destination_row = destination_y * destination_stride + destination_x;
            var column: usize = 0;
            while (column < copy_width) : (column += 1) {
                const coverage = alpha[source_row + column];
                if (coverage == 0) continue;
                pixels[destination_row + column] = blendXrgb(pixels[destination_row + column], rgb, coverage);
            }
        }
        return true;
    }

    /// Blends a byte-exact, straight-alpha ARGB32 source over the current
    /// XRGB scene. The caller supplies an explicit client clip; scaling uses
    /// nearest-neighbour source pixels and never exposes transparent corners.
    pub fn blendArgb32(self: *SceneBuffer, clip: surface.Rect, x: i32, y: i32, w: u32, h: u32, scale: u32, source: []const u8) bool {
        if (w == 0 or h == 0 or scale == 0 or scale > 16) return false;
        const pixel_count = std.math.mul(usize, @as(usize, w), @as(usize, h)) catch return false;
        const required = std.math.mul(usize, pixel_count, @sizeOf(u32)) catch return false;
        if (source.len != required) return false;
        const pixels = self.pixels orelse return false;
        const scene_clip = self.paintClipRect(clip) orelse return true;

        const scaled_width = std.math.mul(i64, @as(i64, w), @as(i64, scale)) catch return false;
        const scaled_height = std.math.mul(i64, @as(i64, h), @as(i64, scale)) catch return false;
        const source_left: i64 = x;
        const source_top: i64 = y;
        const source_right = std.math.add(i64, source_left, scaled_width) catch return false;
        const source_bottom = std.math.add(i64, source_top, scaled_height) catch return false;
        const left = @max(@as(i64, scene_clip.x), source_left);
        const top = @max(@as(i64, scene_clip.y), source_top);
        const right = @min(@as(i64, scene_clip.right()), source_right);
        const bottom = @min(@as(i64, scene_clip.bottom()), source_bottom);
        if (right <= left or bottom <= top) return true;

        const destination_stride: usize = @intCast(self.width);
        var destination_y = top;
        while (destination_y < bottom) : (destination_y += 1) {
            const source_y: usize = @intCast(@divFloor(destination_y - source_top, @as(i64, scale)));
            const destination_row = @as(usize, @intCast(destination_y)) * destination_stride;
            var destination_x = left;
            while (destination_x < right) : (destination_x += 1) {
                const source_x: usize = @intCast(@divFloor(destination_x - source_left, @as(i64, scale)));
                const source_index = (source_y * @as(usize, w) + source_x) * @sizeOf(u32);
                const argb = @as(u32, source[source_index]) |
                    (@as(u32, source[source_index + 1]) << 8) |
                    (@as(u32, source[source_index + 2]) << 16) |
                    (@as(u32, source[source_index + 3]) << 24);
                const alpha: u8 = @truncate(argb >> 24);
                if (alpha == 0) continue;
                const destination_index = destination_row + @as(usize, @intCast(destination_x));
                pixels[destination_index] = blendXrgb(pixels[destination_index], argb, alpha);
            }
        }
        return true;
    }
};

fn blendXrgb(destination: u32, source: u32, alpha: u8) u32 {
    if (alpha == 0) return destination;
    const src = source & 0x00FF_FFFF;
    if (alpha == 255) return src;
    const amount: u32 = alpha;
    const inverse = 255 - amount;
    const red = blendChannel(src >> 16, destination >> 16, amount, inverse);
    const green = blendChannel(src >> 8, destination >> 8, amount, inverse);
    const blue = blendChannel(src, destination, amount, inverse);
    return (red << 16) | (green << 8) | blue;
}

fn blendChannel(source: u32, destination: u32, alpha: u32, inverse: u32) u32 {
    return (((source & 0xFF) * alpha + (destination & 0xFF) * inverse + 127) / 255) & 0xFF;
}

test "scene buffer fills clipped rectangles" {
    var pixels: [16]u32 = .{0} ** 16;
    var buffer = SceneBuffer{};
    try std.testing.expect(buffer.attach(std.mem.sliceAsBytes(pixels[0..]), 4, 4));

    buffer.fillRect(.{ .x = 1, .y = 1, .w = 5, .h = 2 }, 0x123456);

    try std.testing.expectEqual(@as(u32, 0), pixels[0]);
    try std.testing.expectEqual(@as(u32, 0x123456), pixels[5]);
    try std.testing.expectEqual(@as(u32, 0x123456), pixels[7]);
    try std.testing.expectEqual(@as(u32, 0), pixels[8]);
}

test "scene buffer replays native xrgb32 identity integer fractional and tile seams" {
    var memory: [7 * 4 * @sizeOf(u32)]u8 align(@alignOf(u32)) = undefined;
    var buffer = SceneBuffer{};
    try std.testing.expect(buffer.attach(memory[0..], 7, 4));
    @memset(buffer.pixels.?, 0x00EEEEEE);

    const identity = [_]u32{ 1, 2, 3, 4 };
    try std.testing.expect(buffer.blitXrgb32Nearest(.{ .x = 1, .y = 1, .w = 2, .h = 2 }, .{
        .pixels = identity[0..],
        .source_x = 0,
        .source_y = 0,
        .source_w = 2,
        .source_h = 2,
        .source_stride = 2,
        .guest_w = 2,
        .guest_h = 2,
        .viewport = .{ .x = 1, .y = 1, .w = 2, .h = 2 },
    }));
    try std.testing.expectEqualSlices(u32, &.{ 1, 2, 3, 4 }, &.{ buffer.pixels.?[8], buffer.pixels.?[9], buffer.pixels.?[15], buffer.pixels.?[16] });

    var unaligned_bytes: [1 + identity.len * @sizeOf(u32)]u8 = undefined;
    @memcpy(unaligned_bytes[1..], std.mem.sliceAsBytes(identity[0..]));
    @memset(buffer.pixels.?, 0);
    try std.testing.expect(buffer.blitXrgb32Nearest(.{ .x = 1, .y = 1, .w = 2, .h = 2 }, .{
        .pixels = std.mem.bytesAsSlice(u32, unaligned_bytes[1..]),
        .source_x = 0,
        .source_y = 0,
        .source_w = 2,
        .source_h = 2,
        .source_stride = 2,
        .guest_w = 2,
        .guest_h = 2,
        .viewport = .{ .x = 1, .y = 1, .w = 2, .h = 2 },
    }));
    try std.testing.expectEqualSlices(u32, &.{ 1, 2, 3, 4 }, &.{ buffer.pixels.?[8], buffer.pixels.?[9], buffer.pixels.?[15], buffer.pixels.?[16] });

    @memset(buffer.pixels.?, 0);
    const integer = [_]u32{ 0x11, 0x22, 0x33, 0x44 };
    try std.testing.expect(buffer.blitXrgb32Nearest(.{ .x = 0, .y = 0, .w = 4, .h = 4 }, .{
        .pixels = integer[0..],
        .source_x = 0,
        .source_y = 0,
        .source_w = 2,
        .source_h = 2,
        .source_stride = 2,
        .guest_w = 2,
        .guest_h = 2,
        .viewport = .{ .x = 0, .y = 0, .w = 4, .h = 4 },
    }));
    try std.testing.expectEqualSlices(u32, &.{ 0x11, 0x11, 0x22, 0x22 }, buffer.pixels.?[0..4]);
    try std.testing.expectEqualSlices(u32, &.{ 0x33, 0x33, 0x44, 0x44 }, buffer.pixels.?[14..18]);

    @memset(buffer.pixels.?, 0);
    const fractional = [_]u32{ 10, 20, 30, 40 };
    try std.testing.expect(buffer.blitXrgb32Nearest(.{ .x = 0, .y = 0, .w = 5, .h = 3 }, .{
        .pixels = fractional[0..],
        .source_x = 0,
        .source_y = 0,
        .source_w = 2,
        .source_h = 2,
        .source_stride = 2,
        .guest_w = 2,
        .guest_h = 2,
        .viewport = .{ .x = 0, .y = 0, .w = 5, .h = 3 },
    }));
    try std.testing.expectEqualSlices(u32, &.{ 10, 10, 10, 20, 20 }, buffer.pixels.?[0..5]);
    try std.testing.expectEqualSlices(u32, &.{ 30, 30, 30, 40, 40 }, buffer.pixels.?[14..19]);

    @memset(buffer.pixels.?, 0x00EEEEEE);
    buffer.setPaintClip(.{ .x = 1, .y = 0, .w = 5, .h = 1 });
    const left = [_]u32{ 100, 200 };
    const right = [_]u32{ 300, 400 };
    try std.testing.expect(buffer.blitXrgb32Nearest(.{ .x = 0, .y = 0, .w = 4, .h = 1 }, .{
        .pixels = left[0..],
        .source_x = 0,
        .source_y = 0,
        .source_w = 2,
        .source_h = 1,
        .source_stride = 2,
        .guest_w = 4,
        .guest_h = 1,
        .viewport = .{ .x = 0, .y = 0, .w = 7, .h = 1 },
    }));
    try std.testing.expect(buffer.blitXrgb32Nearest(.{ .x = 4, .y = 0, .w = 3, .h = 1 }, .{
        .pixels = right[0..],
        .source_x = 2,
        .source_y = 0,
        .source_w = 2,
        .source_h = 1,
        .source_stride = 2,
        .guest_w = 4,
        .guest_h = 1,
        .viewport = .{ .x = 0, .y = 0, .w = 7, .h = 1 },
    }));
    try std.testing.expectEqualSlices(u32, &.{ 0x00EEEEEE, 100, 200, 200, 300, 300, 0x00EEEEEE }, buffer.pixels.?[0..7]);
}

test "scene buffer expands scaled Indexed8 blocks directly with paint clipping" {
    var pixels: [16]u32 = .{0xA0A0A0} ** 16;
    var memory: [pixels.len * @sizeOf(u32)]u8 align(@alignOf(u32)) = undefined;
    var buffer = SceneBuffer{};
    try std.testing.expect(buffer.attach(memory[0..], 4, 4));
    @memcpy(buffer.pixels.?, pixels[0..]);

    var palette: [256]u32 = .{0} ** 256;
    palette[0] = 0x00112233;
    palette[1] = 0x00445566;
    palette[2] = 0x00778899;
    palette[3] = 0x00AABBCC;
    const indices = [_]u8{ 0, 1, 2, 3 };
    buffer.setPaintClip(.{ .x = 1, .y = 0, .w = 3, .h = 3 });
    try std.testing.expect(buffer.blitIndexed8Nearest(.{ .x = 0, .y = 0, .w = 4, .h = 4 }, .{
        .indices = indices[0..],
        .palette = palette[0..],
        .source_x = 0,
        .source_y = 0,
        .source_w = 2,
        .source_h = 2,
        .source_stride = 2,
        .guest_w = 2,
        .guest_h = 2,
        .viewport = .{ .x = 0, .y = 0, .w = 4, .h = 4 },
    }));

    try std.testing.expectEqualSlices(u32, &.{
        0xA0A0A0, 0x112233, 0x445566, 0x445566,
        0xA0A0A0, 0x112233, 0x445566, 0x445566,
        0xA0A0A0, 0x778899, 0xAABBCC, 0xAABBCC,
        0xA0A0A0, 0xA0A0A0, 0xA0A0A0, 0xA0A0A0,
    }, buffer.pixels.?);
}

test "scene buffer maps adjacent Indexed8 blocks without scaling seams" {
    var memory: [7 * @sizeOf(u32)]u8 align(@alignOf(u32)) = undefined;
    var buffer = SceneBuffer{};
    try std.testing.expect(buffer.attach(memory[0..], 7, 1));
    var palette: [256]u32 = .{0} ** 256;
    palette[0] = 0x10;
    palette[1] = 0x20;
    palette[2] = 0x30;
    palette[3] = 0x40;
    const viewport = surface.Rect{ .x = 0, .y = 0, .w = 7, .h = 1 };

    try std.testing.expect(buffer.blitIndexed8Nearest(.{ .x = 0, .y = 0, .w = 4, .h = 1 }, .{
        .indices = &.{ 0, 1 },
        .palette = palette[0..],
        .source_x = 0,
        .source_y = 0,
        .source_w = 2,
        .source_h = 1,
        .source_stride = 2,
        .guest_w = 4,
        .guest_h = 1,
        .viewport = viewport,
    }));
    try std.testing.expect(buffer.blitIndexed8Nearest(.{ .x = 4, .y = 0, .w = 3, .h = 1 }, .{
        .indices = &.{ 2, 3 },
        .palette = palette[0..],
        .source_x = 2,
        .source_y = 0,
        .source_w = 2,
        .source_h = 1,
        .source_stride = 2,
        .guest_w = 4,
        .guest_h = 1,
        .viewport = viewport,
    }));

    try std.testing.expectEqualSlices(u32, &.{ 0x10, 0x10, 0x20, 0x20, 0x30, 0x30, 0x40 }, buffer.pixels.?);
}

test "scene buffer paint clip preserves pixels outside incremental damage" {
    var pixels: [16]u32 = .{0x112233} ** 16;
    var buffer = SceneBuffer{};
    try std.testing.expect(buffer.attach(std.mem.sliceAsBytes(pixels[0..]), 4, 4));
    buffer.setPaintClip(.{ .x = 1, .y = 1, .w = 2, .h = 2 });

    buffer.fillRect(.{ .x = 0, .y = 0, .w = 4, .h = 4 }, 0xABCDEF);

    try std.testing.expectEqual(@as(u32, 0x112233), pixels[0]);
    try std.testing.expectEqual(@as(u32, 0xABCDEF), pixels[5]);
    try std.testing.expectEqual(@as(u32, 0xABCDEF), pixels[10]);
    try std.testing.expectEqual(@as(u32, 0x112233), pixels[15]);
    buffer.clearPaintClip();
    try std.testing.expectEqual(buffer.fullRect(), buffer.paintBounds());
}

test "scene buffer copies clipped xrgb32 pixels" {
    var pixels: [16]u32 = .{0} ** 16;
    var source = [_]u32{
        1, 2, 3,
        4, 5, 6,
        7, 8, 9,
    };
    var buffer = SceneBuffer{};
    try std.testing.expect(buffer.attach(std.mem.sliceAsBytes(pixels[0..]), 4, 4));

    buffer.blitXrgb32(2, 1, 3, 3, source[0..]);

    try std.testing.expectEqual(@as(u32, 1), pixels[6]);
    try std.testing.expectEqual(@as(u32, 2), pixels[7]);
    try std.testing.expectEqual(@as(u32, 4), pixels[10]);
    try std.testing.expectEqual(@as(u32, 5), pixels[11]);
    try std.testing.expectEqual(@as(u32, 0), pixels[12]);
}

test "scene buffer alpha8 source-over preserves zero and replaces full coverage" {
    var pixels = [_]u32{ 0x112233, 0x204060, 0xABCDEF };
    var buffer = SceneBuffer{};
    try std.testing.expect(buffer.attach(std.mem.sliceAsBytes(pixels[0..]), 3, 1));
    const alpha = [_]u8{ 0, 128, 255 };

    try std.testing.expect(buffer.blendAlpha8(0, 0, 3, 1, 3, 0xE08020, alpha[0..]));
    try std.testing.expectEqual(@as(u32, 0x112233), pixels[0]);
    try std.testing.expectEqual(@as(u32, 0x806040), pixels[1]);
    try std.testing.expectEqual(@as(u32, 0xE08020), pixels[2]);
}

test "scene buffer alpha8 clipping advances through padded source rows" {
    var pixels: [6]u32 = .{0} ** 6;
    var buffer = SceneBuffer{};
    try std.testing.expect(buffer.attach(std.mem.sliceAsBytes(pixels[0..]), 3, 2));
    const alpha = [_]u8{
        0, 255, 128, 0xEE,
        0, 128, 255, 0xDD,
    };

    try std.testing.expect(buffer.blendAlpha8(-1, 0, 3, 2, 4, 0xFFFFFF, alpha[0..]));
    try std.testing.expectEqual(@as(u32, 0xFFFFFF), pixels[0]);
    try std.testing.expectEqual(@as(u32, 0x808080), pixels[1]);
    try std.testing.expectEqual(@as(u32, 0), pixels[2]);
    try std.testing.expectEqual(@as(u32, 0x808080), pixels[3]);
    try std.testing.expectEqual(@as(u32, 0xFFFFFF), pixels[4]);
    try std.testing.expectEqual(@as(u32, 0), pixels[5]);
}

test "scene buffer straight alpha ARGB32 clips scales and preserves transparency" {
    var pixels = [_]u32{0x204060} ** 8;
    var buffer = SceneBuffer{};
    try std.testing.expect(buffer.attach(std.mem.sliceAsBytes(pixels[0..]), 4, 2));
    const source = [_]u8{
        0x00, 0x00, 0xFF, 0x00,
        0x00, 0x00, 0xFF, 0x80,
        0x00, 0xFF, 0x00, 0xFF,
    };

    try std.testing.expect(buffer.blendArgb32(.{ .x = 0, .y = 0, .w = 3, .h = 1 }, 0, 0, 3, 1, 1, source[0..]));
    try std.testing.expectEqual(@as(u32, 0x204060), pixels[0]);
    try std.testing.expectEqual(@as(u32, 0x902030), pixels[1]);
    try std.testing.expectEqual(@as(u32, 0x00FF00), pixels[2]);
    try std.testing.expectEqual(@as(u32, 0x204060), pixels[3]);

    const opaque_white = [_]u8{ 0xFF, 0xFF, 0xFF, 0xFF };
    try std.testing.expect(buffer.blendArgb32(.{ .x = 0, .y = 1, .w = 1, .h = 1 }, -1, 1, 1, 1, 2, opaque_white[0..]));
    try std.testing.expectEqual(@as(u32, 0xFFFFFF), pixels[4]);
    try std.testing.expectEqual(@as(u32, 0x204060), pixels[5]);
}
