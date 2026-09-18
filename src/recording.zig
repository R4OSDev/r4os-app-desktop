// Copyright 2026 R4. SPDX-License-Identifier: Apache-2.0
//! Explicit desktop recorder. Only its worker waits for the codec or disk.
//! One submitted NV12 BO and one delayed compressed packet bound the backlog;
//! busy time drops intermediate capture revisions instead of queuing them.
const std = @import("std");
const r = @import("r4os");
const a = r.abi;
const enc = @import("r4enc");
const pixels = @import("r4enc_recording_pixels");
const mux = @import("r4enc_recording_mux");
pub const directory = "C:\\RECORDINGS";
pub const interval_ns = std.time.ns_per_s / 30;
const work_ns = 2 * std.time.ns_per_s;
const memory_limit = 256 * 1024 * 1024;
const packet_limit = 8 * 1024 * 1024;
pub const Result = struct { ok: bool, parts: u32, frames: u64, bytes: u64, fallback: bool, max_work_ns: u64 };

/// Stable owner survives every recording. ENCODE_V1 Finish is process-final,
/// so it is called only at desktop shutdown, after the last exact worker join.
pub const Manager = struct {
    allocator: std.mem.Allocator,
    raw: *const a.R4XStartContext,
    sys: r.r4sys.Context,
    desk: r.r4desk.Context,
    api: enc.EncodeV1Client,
    runtime: ?enc.R4EncRuntime = null, // worker-owned until join
    job: ?*Job = null,
    pub fn create(allocator: std.mem.Allocator, raw: *const a.R4XStartContext,
        sys: r.r4sys.Context, desk: r.r4desk.Context) ?*Manager
    {
        const api = enc.EncodeV1Client.init(raw) catch return null;
        if (!desk.hasFn("remote_frame_snapshot_acquire") or !desk.hasFn("remote_frame_snapshot_release")) return null;
        const self = allocator.create(Manager) catch return null;
        self.* = .{ .allocator = allocator, .raw = raw, .sys = sys, .desk = desk, .api = api };
        return self;
    }
    pub fn active(self: *const Manager) bool { return self.job != null; }
    pub fn stopping(self: *const Manager) bool {
        const job = self.job orelse return false;
        return @atomicLoad(u64, &job.stop_ns, .acquire) != 0;
    }
    pub fn start(self: *Manager, adapter: u32) bool {
        if (self.job != null) return false;
        const job = self.allocator.create(Job) catch return false;
        job.* = .{ .owner = self, .adapter = adapter };
        const resources: r.Resources = .{ .sys = self.sys };
        switch (resources.createThread(Job.run, @intFromPtr(job), 256 * 1024)) {
            .handle => |handle| job.thread = handle,
            .failure => { self.allocator.destroy(job); return false; },
        }
        self.job = job;
        return true;
    }
    pub fn stop(self: *Manager) void {
        if (self.job) |job| {
            const now = @max(@as(u64, 1), self.sys.monotonicNanoseconds() orelse 1);
            _ = @cmpxchgStrong(u64, &job.stop_ns, 0, now, .release, .monotonic);
        }
    }
    pub fn collect(self: *Manager) ?Result {
        const job = self.job orelse return null;
        if (@atomicLoad(u32, &job.done, .acquire) == 0) return null;
        switch (job.thread.join(r.time_contract.timeoutPoll())) {
            .timed_out => return null,
            .failure => if (job.thread.valid()) return null,
            .exited => {},
        }
        const result = job.result;
        self.allocator.destroy(job); self.job = null;
        return result;
    }
    pub fn destroy(self: *Manager) void {
        self.stop();
        const end = (self.sys.monotonicNanoseconds() orelse 0) +| std.time.ns_per_s;
        while (self.job != null) {
            _ = self.collect();
            if (self.job == null) break;
            if ((self.sys.monotonicNanoseconds() orelse end) >= end) return;
            self.sys.sleepTicks(1);
        }
        if (self.runtime) |*runtime| if (self.api.finish(runtime, std.time.ns_per_s) != enc.ok) return;
        self.allocator.destroy(self);
        // A live worker or unretired codec keeps its stable owner until program
        // teardown; it can never write to a freed stack/application object.
    }
};

const Job = struct {
    owner: *Manager,
    adapter: u32,
    thread: r.JoinHandle = undefined,
    stop_ns: u64 = 0,
    done: u32 = 0,
    result: Result = .{ .ok = false, .parts = 0, .frames = 0, .bytes = 0, .fallback = false, .max_work_ns = 0 },
    demand: bool = false,
    encoder: ?enc.R4EncEncoder = null,
    native: bool = false,
    software_only: bool = false,
    buffer: a.GfxBufferReference = .{},
    mapped: a.GfxBufferMap = .{},
    layout: pixels.Layout = undefined,
    epoch: u64 = 0,
    revision: u32 = 0,
    pending: ?enc.R4EncPacket = null,
    request_id: u64 = 0,
    tag: u64 = 0,
    started_ns: u64 = 0,
    next_capture_ns: u64 = 0,
    last_capture_ns: u64 = 0,
    part: u32 = 0,
    path: [160:0]u8 = @splat(0),
    staged: [168:0]u8 = @splat(0),
    file: ?r.file_stream.WriterState = null,
    writer: mux.Writer = undefined,

    fn now(self: *Job) !u64 { return self.owner.sys.monotonicNanoseconds() orelse error.Clock; }
    fn stopped(self: *Job) bool { return @atomicLoad(u64, &self.stop_ns, .acquire) != 0; }
    fn buffers(self: *Job) r.gfx_buffers.Context { return .{ .base = self.owner.sys.base }; }
    fn pause(self: *Job) void { self.owner.sys.sleepTicks(1); }
    fn releaseDemand(self: *Job) void {
        if (self.demand) { _ = self.owner.desk.remoteFrameRelease(); self.demand = false; }
    }
    fn run(raw: u64) callconv(.c) i32 {
        const self: *Job = @ptrFromInt(raw);
        self.record() catch |err| {
            self.owner.sys.write("Recording failed: "); self.owner.sys.println(@errorName(err));
        };
        self.releaseDemand();
        // Even a failed fence or delayed release retains all owners. Capture
        // demand is gone before retirement waits; no UI thread is involved.
        self.closeEncoder();
        self.abortFile();
        @atomicStore(u32, &self.done, 1, .release);
        return if (self.result.ok) 0 else -1;
    }
    fn openRuntime(self: *Job) !void {
        if (self.owner.runtime != null) return;
        var runtime: enc.R4EncRuntime = undefined;
        const startup: enc.R4EncStartup = .{ .version = 1, .size = @sizeOf(enc.R4EncStartup),
            .application = @intFromPtr(self.owner.raw), .memory_limit = memory_limit, .max_encoders = 1, .thread_limit = 1 };
        if (self.owner.api.open(&startup, &runtime) != enc.ok) return error.EncoderUnavailable;
        self.owner.runtime = runtime;
    }
    fn record(self: *Job) !void {
        try self.openRuntime();
        self.started_ns = try self.now();
        if (!self.owner.sys.exists(directory) and self.owner.sys.dirCreate(directory) < 0) return error.Directory;
        if (self.owner.desk.remoteFrameAcquire() <= 0) return error.CaptureUnavailable;
        self.demand = true;
        while (!self.stopped()) {
            const time = try self.now();
            if (time < self.next_capture_ns) { self.pause(); continue; }
            var info: a.RemoteFrameInfo = .{};
            var snapshot: a.RemoteFrameLease = .{};
            const rc = self.owner.desk.remoteFrameSnapshotAcquire(0, &info, &snapshot);
            if (rc != 0) {
                if (self.encoder == null and time -| self.started_ns > 5 * std.time.ns_per_s) return error.CaptureUnavailable;
                self.pause(); continue;
            }
            // Scope ends before any file/codec wait: only conversion owns the
            // immutable CPU snapshot. It never owns a display/GPU source.
            var captured = false;
            var rotate = false;
            {
                defer _ = self.owner.desk.remoteFrameSnapshotRelease(&snapshot);
                if (snapshot.id == 0 or snapshot.pixels_addr == 0 or snapshot.capacity_pixels < info.frame_pixels or
                    info.format != a.remote_frame_format_xrgb32 or info.stride_pixels < info.width or
                    info.frame_pixels < @as(u64, info.stride_pixels) * info.height) return error.InvalidSnapshot;
                const layout = try pixels.Layout.init(info.width, info.height);
                rotate = self.encoder != null and (snapshot.epoch != self.epoch or !std.meta.eql(layout, self.layout));
                if (!rotate and self.encoder != null and self.tag != 0 and info.revision == self.revision and snapshot.epoch == self.epoch) {
                    self.next_capture_ns = time +| interval_ns;
                    continue;
                }
                // Cold setup happens on the next pass, without a capture lease.
                if (!rotate and self.encoder != null) {
                    try self.convert(snapshot, info);
                    self.revision = info.revision;
                    captured = true;
                }
                if (self.encoder == null) { self.layout = layout; self.epoch = snapshot.epoch; }
            }
            if (rotate) {
                try self.finishPart(time);
                self.closeEncoder();
                continue;
            }
            if (self.encoder == null) { try self.openEncoder(); continue; }
            if (!captured) continue;
            if (self.stopped()) break;
            const pts = try self.now();
            self.last_capture_ns = pts;
            // One outstanding input. Never replay missed timer intervals or
            // capture revisions after slow encoding/output.
            self.next_capture_ns = pts +| interval_ns;
            self.encode(pts) catch |err| {
                if (!self.native or (err != error.Encode and err != error.Timeout and err != error.Send)) return err;
                self.software_only = true; self.result.fallback = true;
                try self.finishPart(try self.now());
                self.closeEncoder(); // physical retirement precedes fallback
                self.owner.sys.println("Recording: switching to software; starting a new file part.");
                continue;
            };
            self.result.max_work_ns = @max(self.result.max_work_ns, (try self.now()) -| pts);
        }
        self.releaseDemand();
        const stopped_at = @atomicLoad(u64, &self.stop_ns, .acquire);
        if (self.encoder != null) {
            try self.drain();
            try self.finishPart(@max(stopped_at, self.last_capture_ns +| 1));
        }
        self.result.ok = self.result.parts != 0;
    }
    fn config(self: *Job, native: bool) enc.R4EncConfig {
        var c = std.mem.zeroes(enc.R4EncConfig);
        c.version = 1; c.size = @sizeOf(enc.R4EncConfig);
        c.query = .{ .version = 1, .size = @sizeOf(enc.R4EncCapsQuery), .backend = if (native) enc.backend_nvidia else enc.backend_software,
            .adapter_id = if (native) self.adapter else 0, .codec = enc.codec_h264, .profile = enc.profile_h264_baseline, .bit_depth = 8, .chroma = enc.chroma_420 };
        c.memory_limit = memory_limit; c.width = self.layout.width; c.height = self.layout.height;
        c.fps_num = 30; c.fps_den = 1; c.gop_frames = 60;
        c.pending_frames = 1; c.packet_leases = 2; c.max_packet_bytes = packet_limit; c.work_timeout_ns = work_ns;
        c.rate = .{ .version = 1, .size = @sizeOf(enc.R4EncRate), .mode = enc.rate_cqp,
            .qp = 26, .min_qp = 0, .max_qp = 51, .target_bps = 0, .peak_bps = 0, .buffer_bits = 0 };
        c.color.version = 1; c.color.size = @sizeOf(enc.R4EncColor);
        c.color.primaries = 1; c.color.transfer = 13; c.color.matrix = 1; c.color.range = 2;
        c.color.chroma_location = enc.chroma_unspecified; c.color.bit_depth = 8; // H.264 default: left.
        return c;
    }
    fn openEncoder(self: *Job) !void {
        self.native = !self.software_only and self.adapter != 0;
        while (true) {
            const c = self.config(self.native);
            var handle: enc.R4EncEncoder = undefined;
            var caps: enc.R4EncCaps = undefined;
            if (self.owner.api.query_caps(&self.owner.runtime.?, &c.query, &caps) == enc.ok and
                self.owner.api.create(&self.owner.runtime.?, &c, &handle) == enc.ok) {
                self.encoder = handle; break;
            }
            if (!self.native) return error.EncoderUnavailable;
            self.native = false; self.software_only = true; self.result.fallback = true;
        }
        self.request_id = 0; self.revision = 0; self.tag = 0;
        const layout = self.layout;
        if (self.buffers().create(&.{ .byte_length = layout.bytes, .alignment = 65536,
            .location = a.gfx_buffer_location_system, .format = a.gfx_buffer_format_nv12,
            .width = layout.width, .height = layout.height, .plane_count = 2,
            .plane_offsets = .{ 0, layout.uv_offset, 0, 0 }, .plane_pitches = .{ layout.pitch, layout.pitch, 0, 0 },
            .usage = a.gfx_buffer_usage_cpu_read | a.gfx_buffer_usage_cpu_write | a.gfx_buffer_usage_transfer_source }, &self.buffer) != a.gfx_buffer_result_ok) return error.Buffer;
        self.writer = .{ .width = layout.width, .height = layout.height, .color = .{ .transfer = 13 } };
        self.part = std.math.add(u32, self.part, 1) catch return error.FileLimit;
        const path = try std.fmt.bufPrintZ(&self.path, "{s}\\REC-{x}-{d}.MKV", .{ directory, self.started_ns, self.part });
        const staged = try std.fmt.bufPrintZ(&self.staged, "{s}.PART", .{path});
        if (self.owner.sys.exists(path) or self.owner.sys.exists(staged)) return error.Exists;
        self.file = .{ .path = staged };
        if (!r.file_stream.begin(&self.owner.sys, &self.file.?, staged, a.file_stream_open_create)) return error.Open;
        self.owner.sys.println(if (self.native) "Recording: NVIDIA H.264" else "Recording: software H.264");
    }
    fn convert(self: *Job, lease: a.RemoteFrameLease, info: a.RemoteFrameInfo) !void {
        if (self.buffers().map(&self.buffer.reference, a.gfx_buffer_map_write, 0, self.layout.bytes, &self.mapped) != a.gfx_buffer_result_ok) return error.Map;
        const source: [*]const u32 = @ptrFromInt(lease.pixels_addr);
        const target: [*]u8 = @ptrFromInt(self.mapped.cpu_address);
        var row: u32 = 0;
        while (row < self.layout.height) : (row += 16) {
            if (self.stopped()) break;
            try pixels.rows(source[0..info.frame_pixels], info.stride_pixels, self.layout, target[0..self.layout.bytes], row, @min(16, self.layout.height - row));
        }
        if (self.buffers().unmap(&self.mapped.lease) != a.gfx_buffer_result_ok) return error.Unmap;
        self.mapped = .{}; // Writable CPU lease ends before ENCODE_V1 imports.
    }
    fn encode(self: *Job, pts: u64) !void {
        var frame = std.mem.zeroes(enc.R4EncFrame);
        frame.version = 1; frame.size = @sizeOf(enc.R4EncFrame); frame.stream_generation = 1;
        self.tag +|= 1; frame.tag = self.tag;
        frame.pts_ns = std.math.cast(i64, pts) orelse return error.Clock;
        frame.duration_ns = interval_ns; frame.format = enc.format_nv12; frame.plane_count = 2;
        frame.plane0 = .{ .buffer = @bitCast(self.buffer.buffer), .reference = @bitCast(self.buffer.reference),
            .offset = 0, .pitch = self.layout.pitch, .row_bytes = self.layout.width, .rows = self.layout.height, .reserved = 0 };
        frame.plane1 = frame.plane0; frame.plane1.offset = self.layout.uv_offset; frame.plane1.rows /= 2;
        const end = pts +| work_ns +| std.time.ns_per_s;
        while (true) {
            const rc = self.owner.api.send(&self.encoder.?, &frame);
            if (rc == enc.ok) break;
            if (rc != enc.error_busy and rc != enc.again) return error.Send;
            if (try self.now() >= end) return error.Timeout;
            self.pause();
        }
        var packet: enc.R4EncPacket = undefined;
        while (true) {
            const rc = self.owner.api.receive(&self.encoder.?, &packet);
            if (rc == enc.ok) break;
            if (rc != enc.again and rc != enc.error_busy) return error.Encode;
            if (try self.now() >= end) return error.Timeout;
            self.pause();
        }
        // Receive only publishes after the input is retired. The reusable BO
        // is now free even if the disk stalls while consuming the old packet.
        if (packet.result != enc.ok or packet.flags & enc.packet_skipped != 0 or packet.tag != frame.tag or
            packet.pts_ns != frame.pts_ns or packet.data_address == 0 or packet.data_bytes == 0 or packet.data_bytes > packet_limit) {
            self.releasePacket(packet); return error.Encode;
        }
        errdefer self.releasePacket(packet);
        try self.writePending(pts);
        self.pending = packet;
    }
    pub fn write(self: *Job, bytes: []const u8) bool {
        return if (self.file) |*file| r.file_stream.write(&self.owner.sys, file, bytes) else false;
    }
    fn writePending(self: *Job, until: u64) !void {
        const packet = self.pending orelse return;
        if (packet.pts_ns < 0) return error.Timestamp;
        const pts: u64 = @intCast(packet.pts_ns);
        const data: [*]const u8 = @ptrFromInt(packet.data_address);
        try self.writer.packet(self, data[0..@intCast(packet.data_bytes)], packet.flags & enc.packet_key != 0, packet.pts_ns, @max(@as(u64, 1), until -| pts));
        self.releasePacket(packet); self.pending = null;
        self.result.frames +|= 1;
    }
    fn releasePacket(self: *Job, packet: enc.R4EncPacket) void {
        while (self.owner.api.release(&packet.lease) == enc.error_busy) self.pause();
    }
    fn control(self: *Job, op: u32, id: u64, state: *enc.R4EncState) i32 {
        return self.owner.api.control(&self.encoder.?, &.{ .version = 1, .size = @sizeOf(enc.R4EncControl),
            .operation = op, .request_id = id, .reserved = 0 }, state);
    }
    fn drain(self: *Job) !void {
        const end = (try self.now()) +| work_ns;
        self.request_id += 1;
        var state: enc.R4EncState = undefined;
        while (true) {
            const rc = self.control(enc.control_drain, self.request_id, &state);
            if (rc == enc.ok) break;
            if (rc != enc.error_busy) return error.Drain;
            if (try self.now() >= end) return error.Timeout;
            self.pause();
        }
        while (state.completed_request != self.request_id or state.phase != enc.phase_drained) {
            const rc = self.control(enc.control_query, 0, &state);
            if (rc != enc.ok and rc != enc.error_busy) return error.Drain;
            if (try self.now() >= end) return error.Timeout;
            self.pause();
        }
    }
    fn finishPart(self: *Job, until: u64) !void {
        try self.writePending(until);
        if (self.file) |*file| {
            if (self.writer.frames == 0) { self.abortFile(); return; }
            if (!r.file_stream.finish(&self.owner.sys, file)) return error.Finish;
            if (self.owner.sys.fileRename(&self.staged, &self.path) <= 0) return error.Publish;
            self.result.parts +|= 1; self.result.bytes +|= file.offset;
            self.file = null;
            self.owner.sys.write("Recording saved: "); self.owner.sys.println(std.mem.sliceTo(&self.path, 0));
        }
    }
    fn abortFile(self: *Job) void {
        if (self.file) |*file| { _ = r.file_stream.abort(&self.owner.sys, file); self.file = null; }
    }
    fn closeEncoder(self: *Job) void {
        if (self.pending) |packet| { self.releasePacket(packet); self.pending = null; }
        if (self.encoder != null) {
            self.request_id +|= 1;
            var state: enc.R4EncState = undefined;
            while (true) {
                const rc = self.control(enc.control_close, self.request_id, &state);
                if (rc == enc.ok) break;
                self.pause();
            }
            while (true) {
                const rc = self.control(enc.control_query, 0, &state);
                if (rc == enc.ok and state.phase == enc.phase_closed and state.completed_request == self.request_id) break;
                self.pause();
            }
            while (self.owner.api.destroy(&self.encoder.?) != enc.ok) self.pause();
            self.encoder = null;
        }
        if (self.mapped.lease.id != 0) {
            while (self.buffers().unmap(&self.mapped.lease) != a.gfx_buffer_result_ok) self.pause();
            self.mapped = .{};
        }
        if (self.buffer.reference.id != 0) {
            while (self.buffers().release(&self.buffer.reference) != a.gfx_buffer_result_ok) self.pause();
            self.buffer = .{};
        }
    }
};
