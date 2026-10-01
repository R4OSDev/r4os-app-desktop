//! Explicit composition diagnostic only. Uses the productive snapshot and
//! Print Screen job; its private source has no display/Present receipt.
const std = @import("std");
const r4os = @import("r4os");
const api = @import("api.zig");
const scene = @import("scene_buffer.zig");
const remote = @import("remote_capture.zig");
const screenshot = @import("screenshot.zig");
pub const state_path = "C:\\TEMP\\CAPVERIFY.TXT";
pub const next_path = "C:\\TEMP\\CAPVERIFY.NEXT";
pub const Session = struct {
    ctx: *api.Context,
    client: bool,
    demand: bool = false,
    held: r4os.abi.RemoteFrameLease = .{},
    held_copy: []u32 = &.{},
    baseline: r4os.abi.RemoteFrameCaptureStats = .{},

    pub fn init(ctx: *api.Context, client: bool) !Session {
        var stats: r4os.abi.RemoteFrameCaptureStats = .{};
        if (ctx.desk.remoteFrameCaptureStats(&stats) != 0 or stats.consumers != 0 or stats.leases != 0) return error.ExistingReaders;
        if (client and (ctx.sys.exists(state_path) or ctx.sys.exists(next_path))) return error.ExistingHandshake;
        if (ctx.desk.remoteFrameSourceReset() != 0) return error.SourceReset;
        return .{ .ctx = ctx, .client = client, .baseline = stats };
    }
    pub fn acquire(self: *Session) !void {
        if (self.ctx.remoteFrameAcquire() != 1) return error.Demand;
        self.demand = true;
    }
    pub fn close(self: *Session) void {
        if (self.held.id != 0) { _ = self.ctx.desk.remoteFrameSnapshotRelease(&self.held); self.held = .{}; }
        self.ctx.allocator().free(self.held_copy); self.held_copy = &.{};
        if (self.demand) { _ = self.ctx.remoteFrameRelease(); self.demand = false; }
        // The kernel releases this program's publisher at execution retirement.
        // No other program's demand or publication is forcibly reset here.
    }
    pub fn publish(self: *Session, image: *scene.SceneBuffer, cursor: remote.Cursor, phase: usize) !void {
        var overlay: @import("cursor_capture.zig").Overlay = .{};
        if (cursor.separate and cursor.visible) overlay.apply(image, cursor.x, cursor.y);
        defer overlay.restore(image);
        if (self.ctx.remoteFramePublishSceneRegionsCursor(image, &.{image.fullRect()}, cursor.x, cursor.y, cursor.visible) < 0)
            return error.Publication;
        if (self.held.id != 0) {
            const previous: [*]const u32 = @ptrFromInt(self.held.pixels_addr);
            if (!std.mem.eql(u32, self.held_copy, previous[0..self.held_copy.len])) return error.MixedSnapshot;
            if (self.ctx.desk.remoteFrameSnapshotRelease(&self.held) != 0) return error.Release;
            self.held = .{};
        }
        var info: r4os.abi.RemoteFrameInfo = .{};
        if (self.ctx.desk.remoteFrameSnapshotAcquire(0, &info, &self.held) != 0) return error.Snapshot;
        const pixels: [*]const u32 = @ptrFromInt(self.held.pixels_addr);
        if (info.width != image.width or info.height != image.height or info.cursor_x != cursor.x or info.cursor_y != cursor.y or
            !std.mem.eql(u32, image.pixels.?, pixels[0..info.frame_pixels])) return error.PublishedPixels;
        self.ctx.allocator().free(self.held_copy); self.held_copy = &.{};
        self.held_copy = try self.ctx.allocator().dupe(u32, pixels[0..info.frame_pixels]);
        if (phase == 0) try self.checkLeaseLimit(info.revision);

        var job: ?*screenshot.Job = screenshot.Job.start(self.ctx.allocator(), self.ctx.sys, self.ctx.desk) orelse return error.Screenshot;
        defer if (job) |active| active.close();
        const deadline = self.now() + 15 * std.time.ns_per_s;
        var saved: screenshot.Result = undefined;
        while (true) {
            if (job.?.collect()) |result| { saved = result; job = null; break; }
            if (self.now() >= deadline) return error.ScreenshotTimeout;
            self.ctx.sleepTicks(1);
        }
        const expected_header = try screenshot.bitmapHeader(info.width, info.height);
        var header: [54]u8 = undefined;
        if (saved.code != 0 or saved.bytes != 54 + image.pixels.?.len * 4 or
            self.ctx.sys.fileReadAt(&saved.path, 0, &header) != header.len or
            !std.mem.eql(u8, &expected_header, &header)) return error.BitmapHeader;
        var chunk: [4096]u8 = undefined;
        const expected_bytes = std.mem.sliceAsBytes(image.pixels.?);
        var offset: usize = 0;
        while (offset < expected_bytes.len) {
            const len = @min(chunk.len, expected_bytes.len - offset);
            if (self.ctx.sys.fileReadAt(&saved.path, @intCast(54 + offset), chunk[0..len]) != len or
                !std.mem.eql(u8, chunk[0..len], expected_bytes[offset..][0..len])) return error.BitmapPixels;
            offset += len;
        }
        var text: [512]u8 = undefined;
        const line = try std.fmt.bufPrint(&text, "phase={d} width={d} height={d} revision={d} epoch={d} pixels={d} bmp={s}\n", .{
            phase, info.width, info.height, info.revision, self.held.epoch, info.frame_pixels, std.mem.sliceTo(&saved.path, 0) });
        self.ctx.write("DESKTOP capture-file: "); self.ctx.write(line);
        if (self.client and self.ctx.sys.fileWrite(state_path, line) != line.len) return error.Handshake;
    }
    fn checkLeaseLimit(self: *Session, revision: u32) !void {
        var leases: [7]r4os.abi.RemoteFrameLease = @splat(.{});
        defer for (&leases) |*lease| if (lease.id != 0) { _ = self.ctx.desk.remoteFrameSnapshotRelease(lease); };
        var info: r4os.abi.RemoteFrameInfo = .{};
        for (&leases) |*lease| {
            if (self.ctx.desk.remoteFrameSnapshotAcquire(revision, &info, lease) != 0 or lease.pixels_addr != self.held.pixels_addr)
                return error.LeaseSharing;
        }
        var extra: r4os.abi.RemoteFrameLease = .{};
        if (self.ctx.desk.remoteFrameSnapshotAcquire(revision, &info, &extra) != r4os.abi.remote_frame_error_unavailable or extra.id != 0)
            return error.LeaseLimit;
        self.ctx.println("DESKTOP capture leases: shared=8 ninth=unavailable");
    }
    pub fn waitClient(self: *Session, phase: usize) !void {
        if (!self.client) return;
        const deadline = self.now() + 60 * std.time.ns_per_s;
        while (self.now() < deadline) {
            var text: [32]u8 = undefined;
            const len = self.ctx.sys.fileReadAt(next_path, 0, &text);
            if (len > 0 and len <= text.len) {
                const next = std.fmt.parseInt(usize, std.mem.trim(u8, text[0..@intCast(len)], " \r\n\t"), 10) catch 0;
                if (next == phase + 1) return;
            }
            self.ctx.sleepTicks(1);
        }
        return error.ClientTimeout;
    }
    pub fn finish(self: *Session) !void {
        self.close();
        const deadline = self.now() + 5 * std.time.ns_per_s;
        var stats: r4os.abi.RemoteFrameCaptureStats = .{};
        while (true) {
            if (self.ctx.desk.remoteFrameCaptureStats(&stats) != 0) return error.CaptureStats;
            if (stats.consumers == 0 and stats.leases == 0 and stats.live_bytes == 0 and stats.snapshot_bytes == 0) break;
            if (self.now() >= deadline) return error.ReadersRetained;
            self.ctx.sleepTicks(1);
        }
        var text: [384]u8 = undefined;
        self.ctx.println(try std.fmt.bufPrint(&text, "DESKTOP capture publication: CPU-published={d} CPU-snapshot-copy={d} remaining-snapshots={d} max-reader-ns={d} remaining=0", .{
            stats.published_bytes - self.baseline.published_bytes, stats.snapshot_copy_bytes - self.baseline.snapshot_copy_bytes,
            stats.snapshots, stats.max_reader_ns }));
        if (self.client and self.ctx.sys.fileWrite(state_path, "complete\n") != 9) return error.Handshake;
    }
    fn now(self: *const Session) u64 { return self.ctx.sys.monotonicNanoseconds() orelse 0; }
};
