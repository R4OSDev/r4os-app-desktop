//! Bounded offscreen analysis, called only by the explicit SNES benchmark.
//! Uses productive owners and public synthetic input; never writes a display.
const std = @import("std");
const gfx = @import("r4gfx");
const r4os = @import("r4os");
const api = @import("api.zig");
const scene = @import("scene_buffer.zig");
const software = @import("composition_software.zig");
const fixture = @import("software_output_fixture.zig");
const compositor = @import("compositor.zig");

const Record = struct {
    omit_covered_background: bool,
    coverage_culled_background: bool,
    client: @import("surface.zig").Rect,
    sample: usize,
    warmup: bool,
    phase_wall_ns: [9]u64,
    changed_bytes: u64,
    direct_pixels: u64,
    reserved_bytes: usize,
    canvas_sha256: [64]u8,
};

fn now(ctx: *const api.Context) !u64 {
    return ctx.sys.monotonicNanoseconds() orelse error.ClockMissing;
}

pub fn run(ctx: *api.Context, colors: *const gfx.ColorV1Client, path: [*:0]const u8,
    caller: r4os.abi.ProgramProcessHandle, deadline: u64) !void
{
    const allocator = ctx.allocator();
    const source = try allocator.alloc(u32, fixture.source_width * fixture.source_height);
    defer allocator.free(source);
    for (source, 0..) |*pixel, i| pixel.* = fixture.sourcePixel(i);
    const pixels = try allocator.alloc(u32, fixture.width * fixture.height);
    defer allocator.free(pixels);
    var clock: r4os.abi.MonotonicClockInfo = .{};
    if (ctx.sys.monotonicClock(&clock) <= 0) return error.ClockMissing;
    var pair_min_ns: u64 = std.math.maxInt(u64);
    for (0..32) |_| {
        const start = try now(ctx);
        const end = try now(ctx);
        if (end < start) return error.ClockRegressed;
        pair_min_ns = @min(pair_min_ns, end - start);
    }
    var records: [20]Record = undefined;
    var index: usize = 0;
    for ([_]bool{ false, true }) |omit_background| {
        @memset(pixels, fixture.canvas_rgb);
        var canvas: scene.SceneBuffer = .{};
        if (!canvas.attach(std.mem.sliceAsBytes(pixels), fixture.width, fixture.height)) return error.Bounds;
        var owner: software.Owner = .{};
        defer owner.deinit();
        for (fixture.clients, fixture.hashes) |client, expected| {
            const view = try r4os.subsystem_host.calculateViewport(client.w, client.h, fixture.source_width, fixture.source_height);
            const frame = fixture.frame(client);
            const windows = [_]@import("window.zig").Window{.{ .kind = .app, .instance_id = 1,
                .x = frame.x, .y = frame.y, .w = frame.w, .h = frame.h,
                .normal_x = frame.x, .normal_y = frame.y, .normal_w = frame.w, .normal_h = frame.h }};
            const frames = [_]@import("gui_frame_snapshot.zig").View{.{ .supported = true, .valid = true }};
            var context = ctx.*;
            context.scene = &canvas; context.output_bounds = null; context.gpu_windows = @splat(null);
            var reserved: usize = 0;
            for (0..5) |sample| {
                // Control checks and the full-canvas oracle stay outside all
                // nine original measured phases. No new frame clock/poller.
                var state: r4os.abi.ProgramInstanceInfo = .{};
                if (ctx.sys.ticks() >= deadline or ctx.sys.programHandleStatus(&caller, &state) != 0 or
                    state.state != @intFromEnum(r4os.abi.ProgramInstanceState.running)) return error.Cancelled;
                var marks: [10]u64 = undefined;
                marks[0] = try now(ctx);
                try owner.begin(allocator, &canvas);
                const cache = owner.cache.?;
                const changed_before = cache.changed_bytes;
                const copied_before = owner.stats.direct_pixels;
                canvas.setPaintClip(client);
                const cover = compositor.CpuCover.init(&context, &windows, &frames, 0, client);
                var cull: compositor.CullStats = .{};
                const coverage_culled = !cover.visible(&cull, client, canvas.fullRect());
                marks[1] = try now(ctx);
                if (!omit_background and !coverage_culled) {
                    const background = (try cache.begin(1, canvas.fullRect(), client)) orelse return error.Layer;
                    marks[2] = try now(ctx);
                    background.fillRect(background.fullRect(), fixture.background_rgb);
                    marks[3] = try now(ctx);
                    try cache.end(1);
                } else {
                    marks[2] = marks[1]; marks[3] = marks[1];
                }
                marks[4] = try now(ctx);
                const painter = (try cache.begin(64, fixture.frame(client), client)) orelse return error.Layer;
                marks[5] = try now(ctx);
                painter.fillRect(painter.fullRect(), fixture.window_rgb);
                marks[6] = try now(ctx);
                if (!painter.blitXrgb32Nearest(painter.fullRect(), .{
                    .pixels = source, .source_x = 0, .source_y = 0,
                    .source_w = fixture.source_width, .source_h = fixture.source_height,
                    .source_stride = fixture.source_width, .guest_w = fixture.source_width, .guest_h = fixture.source_height,
                    .viewport = .{ .x = client.x + view.x, .y = client.y + view.y, .w = @intCast(view.w), .h = @intCast(view.h) },
                })) return error.Raster;
                marks[7] = try now(ctx);
                try cache.end(64);
                marks[8] = try now(ctx);
                try owner.finish(colors, &canvas);
                marks[9] = try now(ctx);
                var spans: [9]u64 = undefined;
                for (&spans, 0..) |*span, p| {
                    if (marks[p+1] < marks[p]) return error.ClockRegressed;
                    span.* = marks[p+1] - marks[p];
                }
                if (sample == 0) reserved = cache.reserved else if (cache.reserved != reserved) return error.WarmAllocation;
                // Full-canvas oracle outside every timed phase, including warmup.
                var digest: [32]u8 = undefined;
                std.crypto.hash.sha2.Sha256.hash(std.mem.sliceAsBytes(pixels), &digest, .{});
                const hex = std.fmt.bytesToHex(digest, .lower);
                if (!std.mem.eql(u8, &hex, expected)) return error.PixelMismatch;
                records[index] = .{ .omit_covered_background = omit_background, .coverage_culled_background = coverage_culled, .client = client,
                    .sample = sample, .warmup = sample == 0, .phase_wall_ns = spans,
                    .changed_bytes = cache.changed_bytes - changed_before,
                    .direct_pixels = owner.stats.direct_pixels - copied_before,
                    .reserved_bytes = cache.reserved, .canvas_sha256 = hex };
                index += 1;
            }
        }
    }
    var clock_end: r4os.abi.MonotonicClockInfo = .{};
    if (ctx.sys.monotonicClock(&clock_end) <= 0 or clock.generation != clock_end.generation or clock.source != clock_end.source)
        return error.ClockChanged;
    const report = try std.json.Stringify.valueAlloc(allocator, .{
        .contract = "desktop-software-output-v1", .clock = clock, .clock_pair_min_ns = pair_min_ns,
        .canvas_width = fixture.width, .canvas_height = fixture.height,
        .phase_order = "owner-begin,background-begin-clear,background-fill,background-compare-copy,window-begin-clear,window-fill,nearest-scale,window-compare-copy,composition",
        .note = "Static public SDR layers in RAM. Begin includes lookup/reuse/clear; warm allocations forbidden. Wall spans include descheduling. No display or ROM. coverage_culled_background records the productive classic CPU client cover; omit_covered_background remains the explicit diagnostic override.",
        .records = records,
    }, .{});
    defer allocator.free(report);
    if (ctx.fileWrite(path, report) != @as(i32, @intCast(report.len))) return error.ReportWrite;
}
