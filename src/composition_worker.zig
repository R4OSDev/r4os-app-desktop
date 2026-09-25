//! Owns the composition device and its immutable frame capture. Only cold
//! resource preparation runs on a thread; normal frames use bounded polling.
const std = @import("std");
const r4os = @import("r4os");
const gfx = @import("r4gfx");
const gpu = @import("composition_gpu.zig");
const layers = @import("composition_layers.zig");
const renderer = @import("gfx_renderer.zig");
const primitives = @import("primitive_frame.zig");
const remote = @import("remote_capture.zig");
const geometry = @import("output_geometry.zig");
pub const Progress = enum { idle, pending, visible, discarded, failed };
pub const Worker = struct {
    graphics: *renderer.Renderer,
    sys: r4os.r4sys.Context,
    cache: layers.Cache,
    engine: gpu.Engine,
    primitives: primitives.Frame,
    capture: remote.Capture,
    capture_demand: bool = false,
    capture_reset: bool = false,
    prepare_capture: bool = false,
    capture_cursor: remote.Cursor = .{},
    capture_damage: ?@import("surface.zig").Rect = null,
    thread: ?r4os.JoinHandle = null,
    done: u32 = 0,
    preparation_error: ?gpu.Error = null,
    deadline: u64 = 0,
    revision: u64 = 0,
    failed_revision: ?u64 = null,
    device_generation: u64 = 0,
    reset_generation: u64 = 0,
    gpu_operations: u32 = 0,
    awaiting_visible: bool = false,
    failure_reported: bool = false,
    prepared_threads: u64 = 0,
    frames_visible: u64 = 0,
    frames_completed: u64 = 0,
    completed_frame: u64 = 0,
    completed_status: ?gfx.R4GfxSwapchainFrameStatus = null,

    pub fn create(allocator: std.mem.Allocator, raw: *const r4os.abi.R4XStartContext, sys: r4os.r4sys.Context) ?*Worker {
        return createForOutput(allocator, raw, sys, 0, null, null, gfx.format_xrgb8888);
    }
    pub fn createForOutput(allocator: std.mem.Allocator, raw: *const r4os.abi.R4XStartContext, sys: r4os.r4sys.Context,
        adapter: u32, head: ?u32, encoding: ?r4os.abi.GfxOutputColorState, format: u32) ?*Worker
    {
        @import("startup_diagnosis.zig").initialize(sys);
        if (encoding) |value| if (value.identity.adapter_id != adapter) return null;
        const graphics = renderer.Renderer.createForAdapter(allocator, raw, adapter) orelse {
            @import("startup_diagnosis.zig").record("worker-create adapter={d} renderer=null", .{adapter}); return null;
        };
        var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
        engine.configureOutput(encoding, format) catch |err| {
            @import("startup_diagnosis.zig").record("worker-create adapter={d} color-error={s}", .{adapter, @errorName(err)});
            graphics.destroy(); return null;
        };
        const self = allocator.create(Worker) catch { graphics.destroy(); return null; };
        const primitive_frame = primitives.Frame.init(allocator) catch { allocator.destroy(self); graphics.destroy(); return null; };
        self.* = .{ .graphics = graphics, .sys = sys, .cache = layers.Cache.init(allocator, 128 * 1024 * 1024),
            .engine = engine, .primitives = primitive_frame,
            .capture = remote.Capture.init(allocator, &graphics.client, &graphics.colors, &graphics.device) };
        self.cache.recording = &self.primitives;
        self.engine.head = head;
        return self;
    }
    pub fn busy(self: *const Worker) bool { return self.thread != null or self.engine.active() or self.engine.pending() or self.awaiting_visible; }
    pub fn needsPolling(self: *const Worker) bool { return self.thread != null or self.engine.needsPolling() or self.awaiting_visible or self.capture.needsPolling() or self.cache.hasBorrowed(); }
    fn captureView(self: *const Worker) geometry.topology.Viewport {
        return self.cache.view orelse .{ .pixel_w = @intCast(self.cache.screen.w), .pixel_h = @intCast(self.cache.screen.h) };
    }
    fn recordCapture(self: *Worker) void {
        const view = self.captureView();
        self.capture.record(self.engine.output_index, self.cache.frame,
            self.capture_damage orelse (geometry.logical(view) catch return), view, self.capture_cursor);
    }
    fn pollCapture(self: *Worker, now: u64) void {
        if (self.capture_reset) { self.capture.invalidate(); self.capture_reset = false; }
        self.capture.setDemand(self.capture_demand);
        self.capture.poll(now);
        self.engine.readback_pin = self.capture.reader.source;
    }
    pub fn blocksCapture(self: *const Worker) bool {
        return !self.failure_reported and (self.thread != null or self.awaiting_visible or
            self.engine.captureBlocked(self.sys.monotonicNanoseconds() orelse 0));
    }
    pub fn available(self: *Worker, revision: u64) bool {
        if (self.thread != null or self.blocksCapture() or self.failed_revision == revision) return false;
        self.revision = revision;
        const info = self.graphics.info() orelse { if (self.engine.head != null) self.fail(error.Graphics); return false; };
        self.gpu_operations = info.gpu_operations;
        const required = self.engine.requiredOperations();
        if (info.gpu_operations & required != required) { if (self.engine.head != null) self.fail(error.Unsupported); return false; }
        if (self.device_generation != info.device_generation or self.reset_generation != info.reset_generation) {
            self.capture.reader.cancel(error.Stale);
            self.capture.reader.valid = false; self.capture.view = null;
            self.pollCapture(self.sys.monotonicNanoseconds() orelse 0);
            self.engine.invalidate();
            self.device_generation = info.device_generation; self.reset_generation = info.reset_generation;
        }
        self.revision = revision;
        self.engine.acquire(self.engine.input_ns) catch |err| {
            if (err != error.Busy) self.fail(err);
            return false;
        };
        return true;
    }
    pub fn start(self: *Worker) bool {
        if (self.thread != null or self.engine.active() or self.awaiting_visible) return false;
        const now = self.sys.monotonicNanoseconds() orelse return false;
        self.deadline = std.math.add(u64, now, 5 * std.time.ns_per_s) catch return false;
        self.failure_reported = false; self.preparation_error = null;
        self.capture.setDemand(self.capture_demand);
        self.prepare_capture = self.capture_demand and now >= self.capture.retry_ns;
        if (self.engine.prepared(&self.cache) and (!self.prepare_capture or
            self.capture.prepared(self.captureView(), self.engine.output_format, self.engine.output_color))) {
            self.engine.begin(&self.cache, self.deadline) catch |err| { self.fail(err); return false; };
            self.recordCapture();
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
            self.capture.poll(self.sys.monotonicNanoseconds() orelse self.deadline);
            self.engine.readback_pin = self.capture.reader.source;
            self.engine.prepare(&self.cache, self.deadline) catch |err| {
                const now = self.sys.monotonicNanoseconds() orelse self.deadline;
                if (err == error.Busy and now < self.deadline) { self.sys.sleepTicks(1); continue; }
                self.preparation_error = err; return -1;
            };
            if (self.prepare_capture) self.capture.prepare(self.captureView(), self.engine.output_format, self.engine.output_color) catch {
                // Capture admission failure cannot fail the local compositor.
                self.capture.redraw = true;
                self.capture.retry_ns = (self.sys.monotonicNanoseconds() orelse self.deadline) +| std.time.ns_per_s;
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
        if (self.failed_revision != self.revision) @import("startup_diagnosis.zig").record(
            "worker-fail reason={s} phase={s} revision={d} preparation={s} thread={} visible={d} frames={d} commands={d} ops={x}",
            .{@errorName(reason), @tagName(self.engine.phase), self.revision,
                if (self.preparation_error) |err| @errorName(err) else "none", self.thread != null,
                self.frames_visible, self.cache.frame, self.cache.command_count, self.gpu_operations});
        self.capture.reader.cancel(error.Stale);
        self.failed_revision = self.revision;
        self.awaiting_visible = false;
        // Once retirement finished, reporting the fault must not start a
        // fresh drain cycle. A live swapchain still needs its close path.
        if (self.engine.active() or self.engine.chain.slot != 0)
            self.engine.cancel(reason)
        else
            self.engine.invalidate();
    }
    pub fn rejectCapture(self: *Worker) void {
        self.fail(error.State); self.failure_reported = true;
        if (self.thread == null) _ = self.engine.discardCapture(&self.cache);
    }
    pub fn poll(self: *Worker, draw: *const r4os.r4draw.Context) Progress {
        if (self.thread != null) {
            if (!self.collectThread()) return .pending;
            if (self.preparation_error) |err| self.fail(err) else {
                self.engine.begin(&self.cache, self.deadline) catch |err| { self.fail(err); return .failed; };
                self.recordCapture();
            }
        }
        const now = self.sys.monotonicNanoseconds() orelse self.deadline;
        self.pollCapture(now);
        if (self.engine.active()) {
            const result = self.engine.advance(&self.cache, now);
            if (self.engine.fault) |err| self.fail(err);
            if (result == .copied and self.engine.chain.slot == 0) self.awaiting_visible = true;
        } else self.engine.pollPresentation() catch |err| self.fail(err);
        if (!self.engine.active()) _ = self.engine.discardCapture(&self.cache);
        if (self.engine.completion()) |done| {
            self.completed_frame = done.frame; self.completed_status = done.status;
            if (done.status.result == 1 or done.status.result == 2) {
                self.capture.complete(done.status.frame.slot - 1, done.status.frame.image, now);
                self.engine.readback_pin = self.capture.reader.source;
            }
            if (done.status.result == 1) { self.frames_visible +|= 1; self.frames_completed +|= 1; return .visible; }
            if (done.status.result == 2) { self.frames_completed +|= 1; return .discarded; }
            if (done.status.result == 3) return .discarded;
            self.fail(error.Graphics);
        }
        if (self.awaiting_visible) {
            const fence = self.engine.present_fence orelse { self.fail(error.State); return .failed; };
            // Statistics carry the last observed Window/BEGUN receipt. A CE
            // fence alone cannot confirm scanout, even after the source frees.
            for (0..8) |head| {
                if (self.engine.head) |selected| if (selected != head) continue;
                var info: r4os.abi.DisplayPresentationStats = .{};
                if (draw.displayPresentationStats(@intCast(head), &info) != r4os.abi.gfx_output_ok or info.backend.adapter_id != fence.adapter_id or
                    info.backend.device_generation != fence.device_generation or info.backend.reset_generation != fence.reset_generation) continue;
                if (info.flags & r4os.abi.display_presentation_flag_lost != 0) { self.fail(error.Graphics); break; }
                if (info.visible_ns != 0 and info.source_timeline == fence.timeline and info.source_point == fence.point) {
                    self.capture.complete(self.engine.output_index, self.engine.outputs[self.engine.output_index].resource, now);
                    self.engine.readback_pin = self.capture.reader.source;
                    self.completed_frame = self.engine.frame; self.completed_status = null;
                    self.awaiting_visible = false; self.frames_visible +|= 1; self.frames_completed +|= 1; return .visible;
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
        self.capture_demand = false; self.capture.setDemand(false);
        if (self.engine.active()) self.engine.cancel(error.State);
        // A quarantined device may retain common BO jobs until physical
        // teardown. Its provider storage must outlive those receipts.
        const now = self.sys.monotonicNanoseconds() orelse 0;
        const end = now +| std.time.ns_per_s;
        while (self.engine.active() or self.capture.reader.pending()) {
            self.pollCapture(self.sys.monotonicNanoseconds() orelse end);
            _ = self.engine.advance(&self.cache, self.sys.monotonicNanoseconds() orelse end);
            if ((self.sys.monotonicNanoseconds() orelse end) >= end) return;
            self.sys.sleepTicks(1);
        }
        while (true) {
            if (!self.engine.discardCapture(&self.cache)) return;
            self.capture.close() catch return;
            self.engine.close() catch |err| {
                if (err != error.Busy or (self.sys.monotonicNanoseconds() orelse end) >= end) return;
                self.sys.sleepTicks(1); continue;
            };
            break;
        }
        const allocator = self.graphics.allocator;
        self.cache.deinit(); self.primitives.deinit(); self.graphics.destroy(); allocator.destroy(self);
    }
    pub fn tryDestroy(self: *Worker) bool {
        if (!self.collectThread()) return false;
        self.capture_demand = false;
        self.pollCapture(self.sys.monotonicNanoseconds() orelse self.deadline);
        self.capture.close() catch return false;
        if (self.engine.active()) {
            if (self.engine.fault == null) self.engine.cancel(error.State);
            _ = self.engine.advance(&self.cache, self.sys.monotonicNanoseconds() orelse self.deadline);
            if (self.engine.active()) return false;
        }
        if (!self.engine.discardCapture(&self.cache)) return false;
        self.engine.close() catch return false;
        const allocator = self.graphics.allocator;
        self.cache.deinit(); self.primitives.deinit(); self.graphics.destroy(); allocator.destroy(self);
        return true;
    }
};
