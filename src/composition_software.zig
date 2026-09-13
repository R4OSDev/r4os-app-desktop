//! Software consumption of the same ordered premultiplied layers used by the
//! GPU compositor. Pixel arithmetic remains in the shared R4GFX provider.
const std = @import("std");
const gfx = @import("r4gfx");
const layers = @import("composition_layers.zig");
const scene = @import("scene_buffer.zig");
pub const Stats = struct { commands: u64 = 0, pixels: u64 = 0 };
fn accepted(status: i32) !void { if (status != gfx.status_ok) return error.Graphics; }
fn descriptor(kind: u32) gfx.R4GfxResourceDesc {
    var result = std.mem.zeroes(gfx.R4GfxResourceDesc);
    result.version = 1; result.size = @sizeOf(gfx.R4GfxResourceDesc); result.kind = kind;
    return result;
}
pub fn paint(client: *const gfx.DeviceV1Client, device: *const gfx.R4GfxDevice, cache: *const layers.Cache, target: *scene.SceneBuffer) !Stats {
    if (cache.collecting or cache.active != null or cache.failure != null or
        !target.matches(cache.screen.w,cache.screen.h)) return error.State;
    var stats: Stats = .{};
    if (cache.command_count == 0) return stats;
    target.flushPending();
    var desc = descriptor(gfx.resource_pipeline); desc.operation = gfx.render_operation_over;
    var pipeline: gfx.R4GfxResource = undefined;
    try accepted(client.resource_create(device,&desc,&pipeline));
    defer _ = client.resource_release(device,&pipeline);
    desc = descriptor(gfx.resource_sampler); desc.sampler = gfx.render_sampler_nearest;
    var sampler: gfx.R4GfxResource = undefined;
    try accepted(client.resource_create(device,&desc,&sampler));
    defer _ = client.resource_release(device,&sampler);
    desc = descriptor(gfx.resource_image); desc.flags = gfx.image_target;
    desc.source_kind = gfx.source_borrow_cpu; desc.source_generation = cache.frame;
    desc.image = .{ .cpu_address = @intFromPtr(target.pixels.?.ptr), .byte_length = target.pixels.?.len*4,
        .pitch = @as(u64,@intCast(target.width))*4, .width = @intCast(target.width), .height = @intCast(target.height),
        .format = gfx.format_xrgb8888, .reserved = 0 };
    var output: gfx.R4GfxResource = undefined;
    try accepted(client.resource_create(device,&desc,&output));
    defer _ = client.resource_release(device,&output);
    for (cache.commands[0..cache.command_count]) |command| {
        const entry = &cache.entries[command.entry];
        if (!entry.initialized or entry.generation == 0) return error.State;
        desc = descriptor(gfx.resource_image); desc.source_kind = gfx.source_borrow_cpu; desc.source_generation = entry.generation;
        desc.image = .{ .cpu_address = @intFromPtr(entry.pixels.ptr), .byte_length = entry.pixels.len*4,
            .pitch = @as(u64,@intCast(entry.bounds.w))*4, .width = @intCast(entry.bounds.w), .height = @intCast(entry.bounds.h),
            .format = gfx.format_argb8888, .reserved = 0 };
        var source: gfx.R4GfxResource = undefined;
        try accepted(client.resource_create(device,&desc,&source));
        defer _ = client.resource_release(device,&source);
        var draw = std.mem.zeroes(gfx.R4GfxDraw);
        draw.source = source; draw.target = output; draw.pipeline = pipeline; draw.sampler = sampler; draw.opacity = 255;
        const clip = command.scissor;
        const max_rows = gfx.render_max_pixels / @as(u64,@intCast(clip.w));
        if (max_rows == 0) return error.Bounds;
        var row: u32 = 0;
        while (row < clip.h) {
            const rows: u32 = @intCast(@min(max_rows,@as(u32,@intCast(clip.h))-row));
            draw.source_rect = .{ .x = @intCast(clip.x-entry.bounds.x), .y = @as(u32,@intCast(clip.y-entry.bounds.y))+row,
                .width = @intCast(clip.w), .height = rows };
            draw.target_rect = .{ .x = @intCast(clip.x), .y = @as(u32,@intCast(clip.y))+row, .width = @intCast(clip.w), .height = rows };
            var rendered: gfx.R4GfxRenderStats = undefined;
            try accepted(client.render(device,&.{ .commands = @intFromPtr(&draw), .command_count = 1, .flags = 0,
                .pixel_budget = @as(u64,@intCast(clip.w))*rows },&rendered));
            stats.commands +|= rendered.cpu.commands; stats.pixels +|= rendered.cpu.pixels;
            row += rows;
        }
    }
    return stats;
}
