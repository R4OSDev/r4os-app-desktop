//! Desktop consumes its ordered layers in linear light through COLOR_V1.
//! A reusable bounded tile retains FP16 precision until all layers have been
//! blended. Pixels outside damage stay byte-identical.
const std = @import("std");
const gfx = @import("r4gfx");
const layers = @import("composition_layers.zig");
const scene = @import("scene_buffer.zig");
const Rect = @import("surface.zig").Rect;
const side = layers.ColorScratch.side;
pub const Stats = struct { commands: u64 = 0, pixels: u64 = 0 };
/// Retained CPU capture belongs to the output that consumes it. It never
/// borrows the asynchronous GPU worker's layer storage.
pub const Owner = struct {
    cache: ?*layers.Cache = null,
    frames: u64 = 0,
    stats: Stats = .{},
    pub fn begin(self: *Owner, allocator: std.mem.Allocator, canvas: *scene.SceneBuffer) !void {
        if (self.cache == null) {
            const cache = try allocator.create(layers.Cache);
            cache.* = layers.Cache.init(allocator, 128 * 1024 * 1024);
            self.cache = cache;
        }
        const cache = self.cache.?;
        try cache.startCpuOutput(canvas.fullRect());
        canvas.failure = null;
        canvas.layer_hook = cache.hook();
    }
    pub fn finish(self: *Owner, colors: *const gfx.ColorV1Client, canvas: *scene.SceneBuffer) !void {
        canvas.layer_hook = null;
        canvas.clearPaintClip();
        const cache = self.cache orelse return error.State;
        defer cache.releaseUnsubmitted();
        _ = try cache.finish();
        if (canvas.failure != null) return error.Graphics;
        const result = try paint(colors, cache, canvas);
        self.frames +|= 1;
        self.stats.commands +|= result.commands;
        self.stats.pixels +|= result.pixels;
    }
    pub fn cancel(self: *Owner, canvas: *scene.SceneBuffer) void {
        canvas.layer_hook = null;
        if (self.cache) |cache| if (cache.collecting) {
            cache.active = null;
            cache.collecting = false;
            for (&cache.entries) |*entry| entry.initialized = false;
        };
        if (self.cache) |cache| cache.releaseUnsubmitted();
    }
    pub fn deinit(self: *Owner) void {
        if (self.cache) |cache| {
            const allocator = cache.allocator;
            cache.deinit();
            allocator.destroy(cache);
        }
        self.cache = null;
    }
};
pub fn description(linear: bool, opaque_alpha: bool) gfx.R4GfxColorDescription {
    return .{
        .version = 1,
        .size = @sizeOf(gfx.R4GfxColorDescription),
        .primaries = gfx.color_primaries_srgb,
        .transfer = if (linear) gfx.color_transfer_linear else gfx.color_transfer_srgb,
        .range = gfx.color_range_full,
        .alpha = if (opaque_alpha) gfx.color_alpha_opaque else if (linear) gfx.color_alpha_optical else gfx.color_alpha_electrical,
        .precision = if (linear) gfx.color_precision_float16 else gfx.color_precision_unorm8,
        .flags = 0,
        .reference_white = 1000000,
        .peak = 1000000,
        .black = 0,
        .reserved = 0,
    };
}
fn image(pointer: u64, bytes: u64, pitch: u64, width: u32, height: u32, linear: bool, opaque_alpha: bool) gfx.R4GfxColorImage {
    return .{
        .version = 1,
        .size = @sizeOf(gfx.R4GfxColorImage),
        .image = .{ .cpu_address = pointer, .byte_length = bytes, .pitch = pitch, .width = width, .height = height, .format = if (linear) gfx.format_abgr16161616f else if (opaque_alpha) gfx.format_xrgb8888 else gfx.format_argb8888, .reserved = 0 },
        .description = description(linear, opaque_alpha),
        .profile = std.mem.zeroes(gfx.R4GfxColorProfile),
    };
}
fn rect(x: i32, y: i32, width: i32, height: i32) gfx.R4GfxRect {
    return .{ .x = @intCast(x), .y = @intCast(y), .width = @intCast(width), .height = @intCast(height) };
}
fn transform(colors: *const gfx.ColorV1Client, source: *const gfx.R4GfxColorImage, target: *const gfx.R4GfxColorImage, from: gfx.R4GfxRect, to: gfx.R4GfxRect, over: bool, dither: bool, stats: *Stats) !void {
    var result: gfx.R4GfxCpuStats = undefined;
    const rc = colors.color_image_transform(source, target, &.{
        .version = 1,
        .size = @sizeOf(gfx.R4GfxColorTransform),
        .source_rect = from,
        .target_rect = to,
        .sampler = gfx.render_sampler_nearest,
        .operation = if (over) gfx.render_operation_over else gfx.render_operation_blit,
        .opacity = 65535,
        .flags = if (dither) gfx.color_transform_dither else 0,
        .pixel_budget = @as(u64, to.width) * to.height,
    }, &result);
    if (rc != gfx.status_ok) return error.Graphics;
    stats.commands +|= result.commands;
    stats.pixels +|= result.pixels;
}
fn bits(width: u32) u64 {
    return if (width == 64) std.math.maxInt(u64) else (@as(u64, 1) << @as(u6, @intCast(width))) - 1;
}
pub fn paint(colors: *const gfx.ColorV1Client, cache: *layers.Cache, target: *scene.SceneBuffer) !Stats {
    if (cache.collecting or cache.active != null or cache.failure != null or cache.view != null or target.premultiplied or
        target.width != cache.screen.w or target.height != cache.screen.h or target.origin_x != cache.screen.x or target.origin_y != cache.screen.y) return error.State;
    var stats: Stats = .{};
    if (cache.command_count == 0) return stats;
    const destination = target.pixels orelse return error.State;
    for (cache.commands[0..cache.command_count]) |command| {
        if (command.entry >= cache.entries.len) return error.State;
        const entry = &cache.entries[command.entry];
        if (!entry.initialized or entry.generation == 0 or entry.bounds.isEmpty() or command.scissor.isEmpty()) return error.State;
        if (entry.external) |frame| {
            if (!entry.borrowed or frame.cpuImage() == null or frame.message.descriptor.width != entry.bounds.w or
                frame.message.descriptor.height != entry.bounds.h) return error.State;
        } else if (@as(u64, @intCast(entry.bounds.w)) * @as(u32, @intCast(entry.bounds.h)) > entry.pixels.len) return error.State;
        const clip = layers.intersect(entry.bounds, command.scissor) orelse return error.State;
        if (!std.meta.eql(clip, command.scissor) or !std.meta.eql(layers.intersect(cache.screen, clip) orelse return error.State, clip)) return error.State;
    }
    const workspace = try cache.colorStorage();
    target.flushPending();
    const existing = image(@intFromPtr(destination.ptr), destination.len * 4, @as(u64, @intCast(target.width)) * 4, @intCast(target.width), @intCast(target.height), false, true);
    var y: i32 = 0;
    while (y < target.height) : (y += side) {
        var x: i32 = 0;
        while (x < target.width) : (x += side) {
            const tile: Rect = .{ .x = target.origin_x + x, .y = target.origin_y + y, .w = @min(side, target.width - x), .h = @min(side, target.height - y) };
            var active = false;
            for (cache.commands[0..cache.command_count]) |command| if (layers.intersect(tile, command.scissor) != null) {
                active = true;
                break;
            };
            if (!active) continue;
            @memset(&workspace.touched, 0);
            const local = rect(0, 0, tile.w, tile.h);
            const working = image(@intFromPtr(&workspace.linear), @sizeOf(@TypeOf(workspace.linear)), side * 8, @intCast(tile.w), @intCast(tile.h), true, false);
            const output = image(@intFromPtr(&workspace.encoded), @sizeOf(@TypeOf(workspace.encoded)), side * 4, @intCast(tile.w), @intCast(tile.h), false, true);
            try transform(colors, &existing, &working, rect(x, y, tile.w, tile.h), local, false, false, &stats);
            for (cache.commands[0..cache.command_count]) |command| {
                const clip = layers.intersect(tile, command.scissor) orelse continue;
                const entry = &cache.entries[command.entry];
                const source = if (entry.external) |frame| frame.cpuImage().? else
                    image(@intFromPtr(entry.pixels.ptr), entry.pixels.len * 4, @as(u64, @intCast(entry.bounds.w)) * 4, @intCast(entry.bounds.w), @intCast(entry.bounds.h), false, false);
                const dx = clip.x - tile.x;
                const dy = clip.y - tile.y;
                try transform(colors, &source, &working, rect(clip.x - entry.bounds.x, clip.y - entry.bounds.y, clip.w, clip.h), rect(dx, dy, clip.w, clip.h), true, false, &stats);
                const mask = bits(@intCast(clip.w)) << @as(u6, @intCast(dx));
                for (workspace.touched[@intCast(dy)..@intCast(dy + clip.h)]) |*row| row.* |= mask;
            }
            // Ordinary SDR UI assets already have8-bit source precision.
            // Nearest final quantization preserves unchanged opaque UI colors;
            // high-precision image/output conversion chooses dither explicitly.
            try transform(colors, &working, &output, local, local, false, false, &stats);
            for (0..@intCast(tile.h)) |row| {
                const offset = (@as(usize, @intCast(y)) + row) * @as(usize, @intCast(target.width)) + @as(usize, @intCast(x));
                var mask = workspace.touched[row];
                while (mask != 0) {
                    const start: u6 = @intCast(@ctz(mask));
                    const count: u32 = @intCast(@ctz(~(mask >> start)));
                    @memcpy(destination[offset + start ..][0..count], workspace.encoded[row * side + start ..][0..count]);
                    mask &= ~(bits(count) << start);
                }
            }
        }
    }
    for (cache.commands[0..cache.command_count]) |command| if (cache.entries[command.entry].external) |frame| { frame.cpu_consumed = true; };
    return stats;
}
