//! Temporary 0.80.39 hardware diagnosis. Only the desktop event thread calls
//! these hooks. One remote-input-triggered eight-second memory trace per boot;
//! one TEMP write afterward, never disk I/O inside a measured interval.
const std = @import("std");
const r4os = @import("r4os");
pub const Stage = enum {
    loop_composition, loop_platform, loop_metadata, loop_input,
    loop_windows, loop_redraw, loop_idle,
    output_capture, output_paint, output_finish,
    cpu_color, cpu_capture, cpu_transform, cpu_unmap, cpu_present,
    engine_advance, engine_poll, engine_collect,
    step_idle, step_upload, step_asset_upload, step_primitives,
    step_linear_clear, step_draw, step_encode, step_present, step_drain,
};
const Metric = struct { count: u64 = 0, total_ns: u64 = 0, max_ns: u64 = 0 };
const Frame = struct { elapsed_ns: u64, gpu: bool, completed: u64, primitives: u64, jobs: u64, uploads: u64 };
var clock: ?r4os.r4sys.Context = null;
var started = false;
var active = false;
var start_ns: u64 = 0;
var metrics: [@typeInfo(Stage).@"enum".fields.len]Metric = @splat(.{});
var frames: [512]Frame = undefined;
var frame_count: usize = 0;
var bytes: [98304]u8 = undefined;
pub fn running() bool { return active; }
pub fn arm(sys: r4os.r4sys.Context) void {
    if (started) return;
    const now = sys.monotonicNanoseconds() orelse return;
    started = true; active = true; clock = sys; start_ns = now;
}
pub fn stamp() u64 {
    if (!active) return 0;
    return clock.?.monotonicNanoseconds() orelse 0;
}
pub fn end(stage: Stage, begin: u64) void {
    if (!active or begin == 0 or begin < start_ns) return;
    const now = stamp();
    if (now < begin) return;
    const value = &metrics[@intFromEnum(stage)];
    value.count +|= 1; value.total_ns +|= now - begin; value.max_ns = @max(value.max_ns, now - begin);
}
pub fn frame(gpu: bool, completed: u64, primitives: u64, jobs: u64, uploads: u64) void {
    if (!active or frame_count == frames.len) return;
    const now = stamp();
    if (now < start_ns) return;
    frames[frame_count] = .{ .elapsed_ns = now - start_ns, .gpu = gpu, .completed = completed,
        .primitives = primitives, .jobs = jobs, .uploads = uploads };
    frame_count += 1;
}
pub fn finish() void {
    if (!active) return;
    const now = stamp();
    if (now < start_ns or now - start_ns < 8 * std.time.ns_per_s) return;
    active = false;
    var used: usize = 0;
    append(&bytes, &used, "R4DESK85 phase profile elapsed-ns={d} frames={d}\nNested stage totals overlap; job/draw/upload values are cumulative engine counters.\n", .{now - start_ns, frame_count});
    inline for (@typeInfo(Stage).@"enum".fields, 0..) |field, i| {
        const value = metrics[i];
        append(&bytes, &used, "stage={s} count={d} total-ns={d} max-ns={d}\n", .{field.name, value.count, value.total_ns, value.max_ns});
    }
    for (frames[0..frame_count]) |value| append(&bytes, &used,
        "frame elapsed-ns={d} gpu={} completed={d} primitives={d} jobs={d} uploads={d}\n",
        .{value.elapsed_ns, value.gpu, value.completed, value.primitives, value.jobs, value.uploads});
    append(&bytes, &used, "END R4DESK85 phase profile\n", .{});
    _ = clock.?.fileWrite("C:\\TEMP\\AMD221\\DSK85.LOG", bytes[0..used]);
}
fn append(buffer: []u8, used: *usize, comptime fmt: []const u8, args: anytype) void {
    const text = std.fmt.bufPrint(buffer[used.*..], fmt, args) catch return;
    used.* += text.len;
}
