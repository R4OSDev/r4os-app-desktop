//! A complete bounded primitive stream for one composition capture. Source
//! pointers are consumed immediately into resident, content-keyed texture pages.
const std = @import("std");
const image = @import("primitive_image.zig");
const assets = @import("primitive_assets.zig");
const scene = @import("scene_buffer.zig");
const surface = @import("surface.zig");
pub const capacity = 8192;
const Error = error{ OutOfMemory, State, Invalid, Capacity, Bounds, Overflow };
pub const Command = struct {
    layer: u8,
    texture: ?u8 = null,
    source: surface.Rect = .{ .x = 0, .y = 0, .w = 0, .h = 0 },
    target: surface.Rect,
    scissor: surface.Rect,
    color: u32 = 0,
    over: bool = false,
};
pub const Frame = struct {
    allocator: std.mem.Allocator,
    assets: assets.Cache,
    commands: []Command,
    count: usize = 0,
    mirror: bool = true,
    failure: ?anyerror = null,
    active: ?struct { layer: u8, bounds: surface.Rect } = null,
    fractional_pixels: u64 = 0,
    merged_fills: u64 = 0,

    pub fn init(allocator: std.mem.Allocator) !Frame {
        var cache = try assets.Cache.init(allocator, 64 * 1024 * 1024);
        errdefer cache.deinit();
        return .{ .allocator = allocator, .assets = cache, .commands = try allocator.alloc(Command, capacity) };
    }
    pub fn deinit(self: *Frame) void { self.assets.deinit(); self.allocator.free(self.commands); self.commands = &.{}; }
    pub fn start(self: *Frame, generation: u64) !void {
        if (self.active != null) return error.State;
        try self.assets.start(generation);
        self.count = 0; self.failure = null;
    }
    pub fn begin(self: *Frame, layer: u8, bounds: surface.Rect) !void {
        if (self.active != null) return error.State;
        self.active = .{ .layer = layer, .bounds = bounds };
    }
    pub fn end(self: *Frame) !void {
        if (self.active == null) return error.State;
        self.active = null;
        if (self.failure) |failure| return failure;
    }
    pub fn hook(self: *Frame) scene.SceneBuffer.PrimitiveHook { return .{ .context = @intFromPtr(self), .paint = paintHook, .fail = failHook }; }
    fn failHook(raw: usize, reason: anyerror) void {
        const self: *Frame = @ptrFromInt(raw);
        if (self.failure == null) self.failure = reason;
    }
    fn paintHook(raw: usize, painter: *scene.SceneBuffer, operation: image.Paint) void {
        const self: *Frame = @ptrFromInt(raw);
        if (self.failure != null) return;
        self.record(operation) catch |err| { self.failure = err; return; };
        if (self.mirror) self.software(painter, operation);
    }
    fn append(self: *Frame, value: Command) !void {
        if (self.count != 0 and value.texture == null and !value.over) {
            const prior = &self.commands[self.count - 1];
            if (prior.texture == null and !prior.over and prior.layer == value.layer and
                std.meta.eql(prior.scissor, prior.target) and std.meta.eql(value.scissor, value.target)) {
                const a = prior.target; const b = value.target;
                if (std.meta.eql(a, b)) { prior.* = value; self.merged_fills +|= 1; return; }
                if (prior.color == value.color and ((a.y == b.y and a.h == b.h and a.right() == b.x) or
                    (a.x == b.x and a.w == b.w and a.bottom() == b.y))) {
                    prior.target = a.merged(b); prior.scissor = prior.target; self.merged_fills +|= 1; return;
                }
            }
        }
        if (self.count == self.commands.len) return error.Capacity;
        self.commands[self.count] = value; self.count += 1;
    }
    fn local(self: *const Frame, rect: surface.Rect) surface.Rect {
        const bounds = self.active.?.bounds;
        return .{ .x = rect.x - bounds.x, .y = rect.y - bounds.y, .w = rect.w, .h = rect.h };
    }
    fn record(self: *Frame, operation: image.Paint) Error!void {
        const active = self.active orelse return error.State;
        switch (operation) {
            .clear => |rect| try self.append(.{ .layer = active.layer, .target = self.local(rect), .scissor = self.local(rect) }),
            .fill => |fill| try self.append(.{ .layer = active.layer, .target = self.local(fill.rect), .scissor = self.local(fill.rect), .color = 0xff000000 | fill.rgb }),
            .picture => |picture| {
                if (!picture.view.valid() or picture.guest_w == 0 or picture.guest_h == 0 or picture.viewport.isEmpty()) return error.Invalid;
                if (@as(u64, picture.source_x) + picture.view.width > picture.guest_w or
                    @as(u64, picture.source_y) + picture.view.height > picture.guest_h) return error.Bounds;
                const clipped = intersect(picture.clip, picture.viewport) orelse return;
                const whole = picture.source_x == 0 and picture.source_y == 0 and picture.guest_w == picture.view.width and picture.guest_h == picture.view.height;
                const integer = @rem(picture.viewport.w, @as(i64, picture.guest_w)) == 0 and @rem(picture.viewport.h, @as(i64, picture.guest_h)) == 0;
                if (!whole and !integer) return self.fractional(picture, clipped);
                const entry = try self.assets.intern(picture.view);
                const target: surface.Rect = if (whole) picture.viewport else blk: {
                    const sx = @divTrunc(picture.viewport.w, @as(i64, picture.guest_w));
                    const sy = @divTrunc(picture.viewport.h, @as(i64, picture.guest_h));
                    break :blk .{ .x = @intCast(@as(i64, picture.viewport.x) + sx * picture.source_x),
                        .y = @intCast(@as(i64, picture.viewport.y) + sy * picture.source_y),
                        .w = @intCast(sx * picture.view.width), .h = @intCast(sy * picture.view.height) };
                };
                const clip = intersect(clipped, target) orelse return;
                try self.append(.{ .layer = active.layer, .texture = entry.texture, .source = entry.rect,
                    .target = self.local(target), .scissor = self.local(clip),
                    .over = picture.view.format == .argb or picture.view.format == .alpha });
            },
        }
    }
    fn fractional(self: *Frame, picture: image.Picture, clip: surface.Rect) Error!void {
        if (picture.view.format != .xrgb and picture.view.format != .indexed) return error.Invalid;
        // Transported subimages use integer guest-edge sampling. A fractional
        // chunk edge cannot be represented by today's signed integer quad.
        // Keep exactly that operation on bounded 128x128 software tiles.
        var pixels: [128 * 128]u32 = undefined;
        var y = clip.y;
        while (y < clip.bottom()) {
            const height = @min(128, clip.bottom() - y);
            var x = clip.x;
            while (x < clip.right()) {
                const width = @min(128, clip.right() - x);
                for (0..@intCast(height)) |row| for (0..@intCast(width)) |column| {
                    const gx: u64 = @intCast(@divTrunc((@as(i64, x) + @as(i64, @intCast(column)) - picture.viewport.x) * picture.guest_w, picture.viewport.w));
                    const gy: u64 = @intCast(@divTrunc((@as(i64, y) + @as(i64, @intCast(row)) - picture.viewport.y) * picture.guest_h, picture.viewport.h));
                    if (gx < picture.source_x or gx >= @as(u64, picture.source_x) + picture.view.width or
                        gy < picture.source_y or gy >= @as(u64, picture.source_y) + picture.view.height) return error.Bounds;
                    pixels[row * @as(usize, @intCast(width)) + column] = picture.view.pixel(@intCast(gx - picture.source_x), @intCast(gy - picture.source_y));
                };
                self.fractional_pixels +|= @as(u64, @intCast(width)) * @as(u64, @intCast(height));
                const rect: surface.Rect = .{ .x = x, .y = y, .w = width, .h = height };
                try self.record(.{ .picture = .{ .view = .{ .format = .xrgb, .width = @intCast(width), .height = @intCast(height),
                    .stride = @as(usize, @intCast(width)) * 4, .bytes = std.mem.sliceAsBytes(pixels[0..@intCast(width * height)]) },
                    .viewport = rect, .clip = rect, .guest_w = @intCast(width), .guest_h = @intCast(height) } });
                x += width;
            }
            y += height;
        }
    }
    fn software(_: *Frame, painter: *scene.SceneBuffer, operation: image.Paint) void {
        const hook_value = painter.primitive_hook; painter.primitive_hook = null;
        defer painter.primitive_hook = hook_value;
        switch (operation) {
            .clear => |rect| painter.clearLayer(rect),
            .fill => |fill| painter.fillRect(fill.rect, fill.rgb),
            .picture => |picture| {
                const view = picture.view;
                switch (view.format) {
                    .xrgb => _ = painter.blitXrgb32Nearest(picture.clip, .{ .pixels = std.mem.bytesAsSlice(u32, view.bytes),
                        .source_x = picture.source_x, .source_y = picture.source_y, .source_w = view.width, .source_h = view.height,
                        .source_stride = @intCast(view.stride / 4), .guest_w = picture.guest_w, .guest_h = picture.guest_h, .viewport = picture.viewport }),
                    .indexed => _ = painter.blitIndexed8Nearest(picture.clip, .{ .indices = view.bytes, .palette = view.palette,
                        .source_x = picture.source_x, .source_y = picture.source_y, .source_w = view.width, .source_h = view.height,
                        .source_stride = @intCast(view.stride), .guest_w = picture.guest_w, .guest_h = picture.guest_h, .viewport = picture.viewport }),
                    .alpha => _ = painter.blendAlpha8(picture.viewport.x, picture.viewport.y, view.width, view.height, @intCast(view.stride), view.foreground, view.bytes),
                    .argb => _ = painter.blendArgb32(picture.clip, picture.viewport.x, picture.viewport.y, view.width, view.height,
                        @intCast(@divTrunc(picture.viewport.w, @as(i64, view.width))), view.bytes),
                    .glyph => {
                        var pixels: [64 * 64]u32 = undefined;
                        for (0..view.height) |row| for (0..view.width) |column| { pixels[row * view.width + column] = view.pixel(column, row); };
                        const previous = painter.paint_clip; painter.setPaintClip(picture.clip); defer painter.paint_clip = previous;
                        painter.blitXrgb32(picture.viewport.x, picture.viewport.y, view.width, view.height, pixels[0..@as(usize, view.width) * view.height]);
                    },
                }
            },
        }
    }
};
fn intersect(a: surface.Rect, b: surface.Rect) ?surface.Rect {
    const x = @max(a.x, b.x); const y = @max(a.y, b.y);
    const right = @min(a.right(), b.right()); const bottom = @min(a.bottom(), b.bottom());
    if (a.isEmpty() or b.isEmpty() or right <= x or bottom <= y) return null;
    return .{ .x = x, .y = y, .w = right - x, .h = bottom - y };
}
