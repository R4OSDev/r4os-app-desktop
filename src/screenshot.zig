//! Print Screen saves an immutable CPU snapshot on one separate thread.
//! Neither file I/O nor its snapshot lease owns a GPU/display buffer.
const std = @import("std");
const r4os = @import("r4os");
const capture_file = @import("capture_file.zig");
pub const directory = "C:\\SCREENSHOTS";
pub const usage = 0x46; // Existing physical-key contract: HID Print Screen.
pub const Result = struct { code: i32, path: [128:0]u8, bytes: u64, retained: bool = false };
pub const Job = struct {
    allocator: std.mem.Allocator,
    sys: r4os.r4sys.Context,
    desk: r4os.r4desk.Context,
    handle: r4os.JoinHandle = undefined,
    done: u32 = 0,
    cancel: u32 = 0,
    result: i32 = -1,
    path: [128:0]u8 = @splat(0),
    bytes: u64 = 0,
    file: ?capture_file.Output = null,
    reported: bool = false,
    pub fn start(allocator: std.mem.Allocator, sys: r4os.r4sys.Context, desk: r4os.r4desk.Context) ?*Job {
        return startAt(allocator, sys, desk, "");
    }
    /// Diagnostic caller supplies its unique CREATE-only path. The same
    /// worker, immutable lease, BMP writer and join receipts are used.
    pub fn startAt(allocator: std.mem.Allocator, sys: r4os.r4sys.Context, desk: r4os.r4desk.Context, path: []const u8) ?*Job {
        if (path.len >= 128) return null;
        if (!desk.hasFn("remote_frame_snapshot_acquire") or !desk.hasFn("remote_frame_snapshot_release")) return null;
        const self = allocator.create(Job) catch return null;
        self.* = .{ .allocator = allocator, .sys = sys, .desk = desk };
        @memcpy(self.path[0..path.len], path);
        const resources: r4os.Resources = .{ .sys = sys };
        switch (resources.createThread(run, @intFromPtr(self), 128 * 1024)) {
            .handle => |handle| self.handle = handle,
            .failure => { allocator.destroy(self); return null; },
        }
        return self;
    }
    fn cancelled(self: *const Job) bool { return @atomicLoad(u32, &self.cancel, .acquire) != 0; }
    fn run(raw: u64) callconv(.c) i32 {
        const self: *Job = @ptrFromInt(raw);
        defer @atomicStore(u32, &self.done, 1, .release);
        defer if (self.file) |*file| if (!file.abort()) { self.result = -1; file.reportRetained(self.sys); };
        self.result = self.save() catch |err| {
            self.sys.write("Screenshot failed: "); self.sys.println(@errorName(err)); return -1;
        };
        self.sys.write("Screenshot saved: "); self.sys.println(std.mem.sliceTo(&self.path, 0));
        return self.result;
    }
    fn save(self: *Job) !i32 {
        if (self.desk.remoteFrameAcquire() <= 0) return error.CaptureUnavailable;
        var demand = true;
        defer if (demand) { _ = self.desk.remoteFrameRelease(); };
        var info: r4os.abi.RemoteFrameInfo = .{};
        var lease: r4os.abi.RemoteFrameLease = .{};
        const deadline = (self.sys.monotonicNanoseconds() orelse return error.Clock) +| 5 * std.time.ns_per_s;
        while (self.desk.remoteFrameSnapshotAcquire(0, &info, &lease) != 0) {
            if (self.cancelled()) return error.Cancelled;
            if ((self.sys.monotonicNanoseconds() orelse deadline) >= deadline) return error.CaptureUnavailable;
            self.sys.sleepTicks(1);
        }
        defer _ = self.desk.remoteFrameSnapshotRelease(&lease);
        _ = self.desk.remoteFrameRelease(); demand = false;
        if (lease.id == 0 or lease.pixels_addr == 0 or lease.capacity_pixels < info.frame_pixels or
            info.format != r4os.abi.remote_frame_format_xrgb32 or info.stride_pixels != info.width or
            info.frame_pixels != @as(u64, info.width) * info.height) return error.InvalidSnapshot;
        const header = try bitmapHeader(info.width, info.height);
        if (self.path[0] == 0) {
            if (!self.sys.exists(directory) and self.sys.dirCreate(directory) < 0) return error.Directory;
            const now = self.sys.monotonicNanoseconds() orelse return error.Clock;
            _ = try std.fmt.bufPrintZ(&self.path, "{s}\\SHOT-{x}.BMP", .{ directory, now });
        }
        const path = self.path[0..std.mem.indexOfScalar(u8, &self.path, 0).? :0];
        self.file = try capture_file.Output.open(self.sys, path);
        const file = &self.file.?;
        if (!file.write(&header)) return error.Write;
        const pixels: [*]const u32 = @ptrFromInt(lease.pixels_addr);
        const bytes = std.mem.sliceAsBytes(pixels[0..info.frame_pixels]);
        var offset: usize = 0;
        while (offset < bytes.len) {
            if (self.cancelled()) return error.Cancelled;
            const end = @min(bytes.len, offset + 64 * 1024);
            if (!file.write(bytes[offset..end])) return error.Write;
            offset = end;
        }
        try file.finish();
        self.bytes = file.bytes();
        return 0;
    }
    pub fn collect(self: *Job) ?Result {
        if (@atomicLoad(u32, &self.done, .acquire) == 0 or self.reported) return null;
        switch (self.handle.join(r4os.time_contract.timeoutPoll())) {
            .timed_out => return null,
            .failure => if (self.handle.valid()) return null,
            .exited => {},
        }
        const result: Result = .{ .code = self.result, .path = self.path, .bytes = self.bytes, .retained = self.retainedFile() };
        if (result.retained) self.reported = true else self.allocator.destroy(self);
        return result;
    }
    pub fn retainedFile(self: *const Job) bool { return if (self.file) |*file| file.retained() else false; }
    pub fn close(self: *Job) void {
        self.requestCancel();
        const end = (self.sys.monotonicNanoseconds() orelse 0) +| std.time.ns_per_s;
        while (true) {
            if (self.collect() != null) return;
            if ((self.sys.monotonicNanoseconds() orelse end) >= end) return;
            self.sys.sleepTicks(1);
        }
        // An unretired worker keeps its heap owner until program teardown.
    }
    pub fn requestCancel(self: *Job) void { @atomicStore(u32, &self.cancel, 1, .release); }
};
/// Standard 32-bit BI_RGB, top-down, tightly packed XRGB; capture is sRGB SDR.
pub fn bitmapHeader(width: u32, height: u32) ![54]u8 {
    if (width == 0 or height == 0 or width > std.math.maxInt(i32) or height > std.math.maxInt(i32)) return error.ImageSize;
    const bytes = @as(u64, width) * height * 4;
    if (bytes > 64 * 1024 * 1024) return error.ImageSize;
    var header: [54]u8 = @splat(0); header[0] = 'B'; header[1] = 'M';
    std.mem.writeInt(u32, header[2..6], @intCast(bytes + header.len), .little);
    std.mem.writeInt(u32, header[10..14], header.len, .little);
    std.mem.writeInt(u32, header[14..18], 40, .little);
    std.mem.writeInt(u32, header[18..22], width, .little);
    std.mem.writeInt(i32, header[22..26], -@as(i32, @intCast(height)), .little);
    std.mem.writeInt(u16, header[26..28], 1, .little);
    std.mem.writeInt(u16, header[28..30], 32, .little);
    std.mem.writeInt(u32, header[34..38], @intCast(bytes), .little);
    return header;
}
