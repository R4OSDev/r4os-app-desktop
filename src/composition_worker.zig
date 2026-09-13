//! Owns the composition device and its immutable frame capture. Only cold
//! resource preparation runs on a thread; normal frames use bounded polling.
const std = @import("std");
const r4os = @import("r4os");
const gfx = @import("r4gfx");
const gpu = @import("composition_gpu.zig");
const layers = @import("composition_layers.zig");
const renderer = @import("gfx_renderer.zig");
pub const Progress = enum { idle, pending, visible, failed };
pub const Worker = struct {
    graphics: *renderer.Renderer,
    sys: r4os.r4sys.Context,
    cache: layers.Cache,
    engine: gpu.Engine,
    thread: ?r4os.JoinHandle = null,
    done: u32 = 0,
    preparation_error: ?gpu.Error = null,
    deadline: u64 = 0,
    revision: u64 = 0,
    failed_revision: ?u64 = null,
    device_generation: u64 = 0,
    reset_generation: u64 = 0,
    awaiting_visible: bool = false,
    failure_reported: bool = false,
    prepared_threads: u64 = 0,
    frames_visible: u64 = 0,

    pub fn create(allocator: std.mem.Allocator, raw: *const r4os.abi.R4XStartContext, sys: r4os.r4sys.Context) ?*Worker {
        const graphics = renderer.Renderer.create(allocator, raw) orelse return null;
        const self = allocator.create(Worker) catch { graphics.destroy(); return null; };
        self.* = .{ .graphics = graphics, .sys = sys, .cache = layers.Cache.init(allocator, 128 * 1024 * 1024),
            .engine = gpu.Engine.init(&graphics.client, &graphics.device) };
        return self;
    }
    pub fn busy(self: *const Worker) bool { return self.thread != null or self.engine.active() or self.awaiting_visible; }
    pub fn blocksCapture(self: *const Worker) bool { return self.busy() and !self.failure_reported; }
    pub fn available(self: *Worker, revision: u64) bool {
        if (self.busy() or self.failed_revision == revision) return false;
        const info = self.graphics.info() orelse return false;
        const required = gfx.device_gpu_copy_rows | gfx.device_gpu_render | gfx.device_gpu_present;
        if (info.gpu_operations & required != required) return false;
        if (self.device_generation != info.device_generation or self.reset_generation != info.reset_generation) {
            self.engine.output.generation = 0;
            self.device_generation = info.device_generation; self.reset_generation = info.reset_generation;
        }
        self.revision = revision;
        return true;
    }
    pub fn start(self: *Worker) bool {
        if (self.busy()) return false;
        const now = self.sys.monotonicNanoseconds() orelse return false;
        self.deadline = std.math.add(u64, now, 5 * std.time.ns_per_s) catch return false;
        self.failure_reported = false; self.preparation_error = null;
        if (self.engine.prepared(&self.cache)) {
            self.engine.begin(&self.cache, self.deadline) catch |err| { self.fail(err); return false; };
            return true;
        }
        @atomicStore(u32, &self.done, 0, .release);
        const resources: r4os.Resources = .{ .sys = self.sys };
        switch (resources.createThread(prepare, @intFromPtr(self), 256 * 1024)) {
            .handle => |handle| { self.thread = handle; self.prepared_threads +|= 1; },
            .failure => { self.fail(error.Graphics); return false; },
        }
        return true;
    }
    fn prepare(raw: u64) callconv(.c) i32 {
        const self: *Worker = @ptrFromInt(raw);
        defer @atomicStore(u32, &self.done, 1, .release);
        while (true) {
            self.engine.prepare(&self.cache, self.deadline) catch |err| {
                const now = self.sys.monotonicNanoseconds() orelse self.deadline;
                if (err == error.Busy and now < self.deadline) { self.sys.sleepTicks(1); continue; }
                self.preparation_error = err; return -1;
            };
            return 0;
        }
    }
    fn collectThread(self: *Worker) bool {
        const handle = if (self.thread) |*value| value else return true;
        if (@atomicLoad(u32, &self.done, .acquire) == 0) return false;
        switch (handle.join(r4os.time_contract.timeoutPoll())) {
            .exited => {},
            .timed_out => return false,
            .failure => if (handle.valid()) return false,
        }
        self.thread = null;
        return true;
    }
    fn fail(self: *Worker, reason: gpu.Error) void {
        self.failed_revision = self.revision;
        self.awaiting_visible = false;
        if (self.engine.active()) self.engine.cancel(reason) else self.engine.output.generation = 0;
    }
    pub fn rejectCapture(self: *Worker) void {
        self.fail(error.State); self.failure_reported = true;
    }
    pub fn poll(self: *Worker, draw: *const r4os.r4draw.Context) Progress {
        if (self.thread != null) {
            if (!self.collectThread()) return .pending;
            if (self.preparation_error) |err| self.fail(err) else
                self.engine.begin(&self.cache, self.deadline) catch |err| self.fail(err);
        }
        const now = self.sys.monotonicNanoseconds() orelse self.deadline;
        if (self.engine.active()) {
            const result = self.engine.advance(&self.cache, now);
            if (self.engine.fault) |err| self.fail(err);
            if (result == .copied) self.awaiting_visible = true;
        }
        if (self.awaiting_visible) {
            const fence = self.engine.present_fence orelse { self.fail(error.State); return .failed; };
            // Statistics carry the last observed Window/BEGUN receipt. A CE
            // fence alone cannot confirm scanout, even after the source frees.
            for (0..8) |head| {
                var info: r4os.abi.DisplayPresentationStats = .{};
                if (draw.displayPresentationStats(@intCast(head), &info) != 0 or info.backend.adapter_id != fence.adapter_id or
                    info.backend.device_generation != fence.device_generation or info.backend.reset_generation != fence.reset_generation) continue;
                if (info.flags & r4os.abi.display_presentation_flag_lost != 0) { self.fail(error.Graphics); break; }
                if (info.visible_ns != 0 and info.source_timeline == fence.timeline and info.source_point == fence.point) {
                    self.awaiting_visible = false; self.frames_visible +|= 1; return .visible;
                }
            }
            if (self.awaiting_visible and now >= self.deadline) self.fail(error.Deadline);
        }
        if (self.failed_revision == self.revision and !self.failure_reported) {
            self.failure_reported = true;
            return .failed;
        }
        return if (self.busy()) .pending else .idle;
    }
    pub fn destroy(self: *Worker) void {
        while (!self.collectThread()) self.sys.sleepTicks(1);
        if (self.engine.active()) self.engine.cancel(error.State);
        // A quarantined device may retain common BO jobs until physical
        // teardown. Its provider storage must outlive those receipts.
        const now = self.sys.monotonicNanoseconds() orelse 0;
        const end = now +| std.time.ns_per_s;
        while (self.engine.active()) {
            _ = self.engine.advance(&self.cache, self.sys.monotonicNanoseconds() orelse end);
            if ((self.sys.monotonicNanoseconds() orelse end) >= end) return;
            self.sys.sleepTicks(1);
        }
        self.engine.close() catch return;
        const allocator = self.graphics.allocator;
        self.cache.deinit(); self.graphics.destroy(); allocator.destroy(self);
    }
};
