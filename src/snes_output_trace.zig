//! Bounded diagnostic receipts for one exact benchmark window. No scanout claim.
const std = @import("std");
const Rect = @import("surface.zig").Rect;
pub const Sample = struct {
    generation: u64,
    completed_ns: u64,
    frame_wall_ns: u64,
    compose_wall_ns: u64,
    present_wall_ns: u64,
    damage_bounds: Rect,
    full_client_copy: bool,
    cursor_queued_to_copy_ticks: ?u64,
};
pub const Trace = struct {
    window_index: usize,
    samples: [4096]Sample = undefined,
    count: usize = 0,
    overflow: bool = false,
    frame: ?Rect = null,
    client: ?Rect = null,

    pub fn record(self: *Trace, frame: Rect, client: Rect, sample: Sample) void {
        if (sample.generation == 0) return;
        // A resize makes the supposedly fixed geometry run non-comparable.
        if (self.frame) |old| if (!std.meta.eql(old, frame) or !std.meta.eql(self.client.?, client)) {
            self.overflow = true;
            return;
        };
        self.frame = frame; self.client = client;
        if (self.count != 0 and self.samples[self.count - 1].generation == sample.generation) return;
        if (self.count == self.samples.len or (self.count != 0 and sample.generation < self.samples[self.count - 1].generation)) {
            self.overflow = true;
            return;
        }
        self.samples[self.count] = sample;
        self.count += 1;
    }
};

pub const ReportContext = struct {
    maximized: bool,
    process: @import("r4os").abi.ProgramProcessHandle,
    screen_w: u32,
    screen_h: u32,
    started_ns: u64,
    ended_ns: u64,
    presents: u64,
    cursor_moves: u64,
    tick_hz: u64,
};

/// Original owner report, emitted after the measured child has actually ended.
pub fn writeReport(ctx: *@import("api.zig").Context, trace: *const Trace, context: ReportContext,
    report_path: [*:0]const u8) !void
{
    if (trace.overflow or trace.count == 0 or context.ended_ns <= context.started_ns) return error.IncompleteTrace;
    const report = try std.json.Stringify.valueAlloc(ctx.allocator(), .{
        .contract = "desktop-window-output-v1", .receipt = "software-display-copy-present-success",
        .window_case = if (context.maximized) "maximized" else "small", .process = context.process,
        .screen = .{ .w = context.screen_w, .h = context.screen_h }, .frame = trace.frame, .client = trace.client,
        .session_start_ns = context.started_ns, .session_end_ns = context.ended_ns,
        .global_desktop_presents = context.presents, .cursor_moves = context.cursor_moves,
        .cursor_tick_hz = context.tick_hz, .physical_visibility = @as(?u64, null),
        .records = trace.samples[0..trace.count],
        .note = "Window generations observed in successful software copies; full_client_copy needs one complete covering region. Generation cohorts may complete after the guest timer. Frame/compose/present wall spans reuse existing owner clocks; no input-to-scanout or physical frame receipt.",
    }, .{});
    defer ctx.allocator().free(report);
    if (ctx.fileWrite(report_path, report) != @as(i32, @intCast(report.len))) return error.ReportWrite;
}
