//! A complete bounded primitive stream for one composition capture. Source
//! pointers are consumed immediately into resident, content-keyed texture pages.
const std = @import("std");
const image = @import("primitive_image.zig");
const assets = @import("primitive_assets.zig");
const scene = @import("scene_buffer.zig");
const surface = @import("surface.zig");
const geometry = @import("output_geometry.zig");
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
    grid: @import("r4os").abi.GfxSampleGrid = .{},
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
    view: ?geometry.topology.Viewport = null,

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
        const raster = if (self.view) |view| geometry.intersect(try geometry.rasterRect(view, bounds), geometry.native(view)) orelse return error.Bounds else bounds;
        self.active = .{ .layer = layer, .bounds = raster };
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
        if (value.target.isEmpty() or value.scissor.isEmpty()) return;
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
    fn local(self: *const Frame, rect: surface.Rect) Error!surface.Rect {
        const bounds = self.active.?.bounds;
        const raster = if (self.view) |view| geometry.rasterRect(view, rect) catch return error.Bounds else rect;
        return .{ .x = raster.x - bounds.x, .y = raster.y - bounds.y, .w = raster.w, .h = raster.h };
    }
    fn record(self: *Frame, operation: image.Paint) Error!void {
        const active = self.active orelse return error.State;
        switch (operation) {
            .clear => |rect| try self.append(.{ .layer = active.layer, .target = try self.local(rect), .scissor = try self.local(rect) }),
            .fill => |fill| try self.append(.{ .layer = active.layer, .target = try self.local(fill.rect), .scissor = try self.local(fill.rect), .color = 0xff000000 | fill.rgb }),
            .picture => |picture| {
                if (!picture.view.valid() or picture.guest_w == 0 or picture.guest_h == 0 or picture.viewport.isEmpty()) return error.Invalid;
                if (@as(u64, picture.source_x) + picture.view.width > picture.guest_w or
                    @as(u64, picture.source_y) + picture.view.height > picture.guest_h) return error.Bounds;
                const clipped = intersect(picture.clip, picture.viewport) orelse return;
                if (self.view != null) return self.gridPicture(picture, clipped);
                const whole = picture.source_x == 0 and picture.source_y == 0 and picture.guest_w == picture.view.width and picture.guest_h == picture.view.height;
                const integer = @rem(picture.viewport.w, @as(i64, picture.guest_w)) == 0 and @rem(picture.viewport.h, @as(i64, picture.guest_h)) == 0;
                if (!whole and !integer) return self.fractional(picture, clipped);
                var asset_view = picture.view;
                if (self.view) |view| {
                    asset_view.rotation = @intFromEnum(view.rotation);
                    asset_view.identity.dpi_x = (96 * view.scale + 60) / 120;
                    asset_view.identity.dpi_y = asset_view.identity.dpi_x;
                }
                const entry = try self.assets.intern(asset_view);
                const target: surface.Rect = if (whole) picture.viewport else blk: {
                    const sx = @divTrunc(picture.viewport.w, @as(i64, picture.guest_w));
                    const sy = @divTrunc(picture.viewport.h, @as(i64, picture.guest_h));
                    break :blk .{ .x = @intCast(@as(i64, picture.viewport.x) + sx * picture.source_x),
                        .y = @intCast(@as(i64, picture.viewport.y) + sy * picture.source_y),
                        .w = @intCast(sx * picture.view.width), .h = @intCast(sy * picture.view.height) };
                };
                const clip = intersect(clipped, target) orelse return;
                try self.append(.{ .layer = active.layer, .texture = entry.texture, .source = entry.rect,
                    .target = try self.local(target), .scissor = try self.local(clip),
                    .over = picture.view.format == .argb or picture.view.format == .alpha });
            },
        }
    }
    fn gridPicture(self: *Frame, picture: image.Picture, clipped: surface.Rect) Error!void {
        const view = self.view.?; const active = self.active.?;
        var asset = picture.view;
        asset.rotation = 0;
        asset.identity.dpi_x = (96 * view.scale + 60) / 120; asset.identity.dpi_y = asset.identity.dpi_x;
        const entry = try self.assets.intern(asset);
        // A transported subimage covers only the logical cells whose guest-
        // edge samples fall into its source range. Adjacent chunks share the
        // same integer boundary, without CPU scaling or repeated edge texels.
        const x0 = (@as(u64, picture.source_x) * @as(u32, @intCast(picture.viewport.w)) + picture.guest_w - 1) / picture.guest_w;
        const y0 = (@as(u64, picture.source_y) * @as(u32, @intCast(picture.viewport.h)) + picture.guest_h - 1) / picture.guest_h;
        const x1 = ((@as(u64, picture.source_x) + picture.view.width) * @as(u32, @intCast(picture.viewport.w)) + picture.guest_w - 1) / picture.guest_w;
        const y1 = ((@as(u64, picture.source_y) + picture.view.height) * @as(u32, @intCast(picture.viewport.h)) + picture.guest_h - 1) / picture.guest_h;
        const coverage: surface.Rect = .{ .x = std.math.cast(i32, @as(i64, picture.viewport.x) + @as(i64, @intCast(x0))) orelse return error.Bounds,
            .y = std.math.cast(i32, @as(i64, picture.viewport.y) + @as(i64, @intCast(y0))) orelse return error.Bounds,
            .w = @intCast(x1 - x0), .h = @intCast(y1 - y0) };
        const clip = intersect(clipped, coverage) orelse return;
        try self.append(.{ .layer = active.layer, .texture = entry.texture, .source = entry.rect,
            .target = try self.local(coverage), .scissor = try self.local(clip),
            .over = asset.format == .argb or asset.format == .alpha,
            .grid = .{ .enabled = 1, .rotation = @intFromEnum(view.rotation), .scale = view.scale,
                .pixel_width = view.pixel_w, .pixel_height = view.pixel_h,
                .target_x = active.bounds.x, .target_y = active.bounds.y,
                .viewport_x = std.math.cast(i32, @as(i64, picture.viewport.x) - view.origin.x) orelse return error.Bounds,
                .viewport_y = std.math.cast(i32, @as(i64, picture.viewport.y) - view.origin.y) orelse return error.Bounds,
                .viewport_width = @intCast(picture.viewport.w), .viewport_height = @intCast(picture.viewport.h),
                .guest_width = picture.guest_w, .guest_height = picture.guest_h, .source_x = picture.source_x, .source_y = picture.source_y } });
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
