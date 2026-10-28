//! Explicit Desktop work, separate from the passive WINSVC snapshot.
//! Fixed little-endian bytes; no language object/pointer crosses this boundary.
const std = @import("std");
pub const service: [:0]const u8 = "WINSVC";
pub const operation: u16 = 4501;
pub const tray_operation: u16 = 4502;
pub const complete_operation: u16 = 4503;
pub const request_size = 80;
pub const response_size = 144;
const magic: u32 = 0x31434452;
pub const Op = enum(u32) { begin = 1, bind, snapshot, drag, resize, focus, close, tray, volume, end, restart, audio, capture, screenshot, screenshot_status, screenshot_cancel, begin_benchmark, software_profile, snes_prepare, snes_finish, maximize, launch_status, capture_stats };
/// One explicit read-only owner snapshot. No frame is forced by this request.
pub fn captureReportPath(buffer: []u8, owner: Identity, sequence: u64) ![:0]u8 {
    if (!owner.valid() or sequence > 2) return error.InvalidOwner;
    return std.fmt.bufPrintZ(buffer, "C:\\TEMP\\CR{X:0>8}-{X:0>16}-{d}.JSON", .{ owner.instance, owner.generation, sequence });
}
pub const SnesMode = enum(u2) { normal, detail, paired };
pub const SnesOptions = struct {
    pal: bool = false,
    mode: SnesMode = .normal,
    maximized: bool = false,
    check_only: bool = false,
    pub fn bits(self: SnesOptions) u64 {
        return @as(u64, @intFromBool(self.pal)) | (@as(u64, @intFromEnum(self.mode)) << 1) |
            (@as(u64, @intFromBool(self.maximized)) << 3) | (@as(u64, @intFromBool(self.check_only)) << 4);
    }
    pub fn decode(value: u64) !SnesOptions {
        if (value & ~@as(u64, 31) != 0 or (value >> 1) & 3 == 3) return error.InvalidBenchmarkOptions;
        return .{ .pal = value & 1 != 0, .mode = @enumFromInt((value >> 1) & 3), .maximized = value & 8 != 0, .check_only = value & 16 != 0 };
    }
};
// Existing subsystem files are reserved only for one explicit isolated run.
// The native Core host runner and its comparison context remain independent.
pub const snes_data_path = "C:\\TEMP\\SNES-PERF.JSONL";
pub const snes_error_path = "C:\\TEMP\\SNES-PERF.ERROR";
pub const snes_window_path = "C:\\TEMP\\SNES-WINDOW.JSON";
pub const snes_paths = [_][*:0]const u8{ snes_data_path, snes_error_path, snes_window_path };
pub fn softwareReportPath(buffer: []u8, owner: Identity) ![:0]u8 {
    if (!owner.valid()) return error.InvalidOwner;
    return std.fmt.bufPrintZ(buffer, "C:\\TEMP\\DP{X:0>8}-{X:0>16}.JSON", .{ owner.instance, owner.generation });
}
pub const Identity = struct {
    instance: u32 = 0,
    generation: u64 = 0,
    pub fn valid(self: Identity) bool {
        return self.instance != 0 and self.generation != 0;
    }
    pub fn same(self: Identity, other: Identity) bool {
        return self.valid() and self.instance == other.instance and self.generation == other.generation;
    }
};
pub const Request = struct {
    op: Op,
    caller: Identity,
    target: Identity = .{},
    dx: i32 = 0,
    dy: i32 = 0,
    item: u64 = 0,
    deadline_tick: u64,
};
pub const Response = struct {
    op: Op,
    result: i32 = 0,
    desktop: Identity,
    target: Identity = .{},
    x: i32 = 0,
    y: i32 = 0,
    w: i32 = 0,
    h: i32 = 0,
    gui_revision: u32 = 0,
    flags: u32 = 0,
    idle_timeouts: u64 = 0,
    redraws: u64 = 0,
    input_generation: u64 = 0,
    service_handle: u32 = 0,
    screenshot_state: u32 = 0,
    screenshot_bytes: u64 = 0,
    restart_state: u32 = 0,
    restart_old_handle: u32 = 0,
    restart_result: i32 = 0,
    present_failures: u64 = 0,
    present_lost_frames: u64 = 0,
};
pub const geometry_matches: u32 = 1;
pub const focused: u32 = 2;
pub const tray_hit: u32 = 4;
pub const volume_targets: u32 = 8;
pub const isolated: u32 = 16;
pub const service_available: u32 = 32;
/// The actual Desktop has a committed, generation-bound producer frame.
pub const frame_valid: u32 = 64;
/// snes_finish only: exact completed child still owned by graphics retirement.
/// No completion/report receipt; caller may repeat this idempotent operation.
pub const snes_finish_pending: i32 = 1;
pub fn screenshotPath(buffer: []u8, owner: Identity) ![:0]u8 {
    if (!owner.valid()) return error.InvalidOwner;
    return std.fmt.bufPrintZ(buffer, "C:\\TEMP\\SS{X:0>8}-{X:0>16}.BMP", .{ owner.instance, owner.generation });
}
fn get(comptime T: type, bytes: []const u8, at: usize) T {
    return std.mem.readInt(T, bytes[at..][0..@sizeOf(T)], .little);
}
fn put(comptime T: type, bytes: []u8, at: usize, value: T) void {
    std.mem.writeInt(T, bytes[at..][0..@sizeOf(T)], value, .little);
}
fn identity(bytes: []const u8, at: usize) Identity {
    return .{ .instance = get(u32, bytes, at), .generation = get(u64, bytes, at + 8) };
}
fn writeIdentity(bytes: []u8, at: usize, value: Identity) void {
    put(u32, bytes, at, value.instance);
    put(u64, bytes, at + 8, value.generation);
}
fn header(bytes: []u8, size: u16) void {
    @memset(bytes, 0);
    put(u32, bytes, 0, magic);
    put(u16, bytes, 4, 2);
    put(u16, bytes, 6, size);
}
fn valid(bytes: []const u8, size: usize) !Op {
    if (bytes.len != size or get(u32, bytes, 0) != magic or get(u16, bytes, 4) != 2 or get(u16, bytes, 6) != size) return error.InvalidContract;
    return std.enums.fromInt(Op, get(u32, bytes, 8)) orelse error.InvalidOperation;
}
pub fn encodeRequest(bytes: *[request_size]u8, value: Request) void {
    header(bytes, request_size);
    put(u32, bytes, 8, @intFromEnum(value.op));
    writeIdentity(bytes, 16, value.caller);
    writeIdentity(bytes, 32, value.target);
    put(i32, bytes, 48, value.dx);
    put(i32, bytes, 52, value.dy);
    put(u64, bytes, 56, value.item);
    put(u64, bytes, 64, value.deadline_tick);
}
pub fn decodeRequest(bytes: []const u8) !Request {
    const op = try valid(bytes, request_size);
    if (get(u32, bytes, 12) != 0 or get(u32, bytes, 20) != 0 or get(u32, bytes, 36) != 0 or get(u64, bytes, 72) != 0) return error.Reserved;
    const result: Request = .{ .op = op, .caller = identity(bytes, 16), .target = identity(bytes, 32), .dx = get(i32, bytes, 48), .dy = get(i32, bytes, 52), .item = get(u64, bytes, 56), .deadline_tick = get(u64, bytes, 64) };
    if (!result.caller.valid()) return error.InvalidOwner;
    return result;
}
pub fn encodeResponse(bytes: *[response_size]u8, value: Response) void {
    header(bytes, response_size);
    put(u32, bytes, 8, @intFromEnum(value.op));
    put(i32, bytes, 12, value.result);
    writeIdentity(bytes, 16, value.desktop);
    writeIdentity(bytes, 32, value.target);
    put(i32, bytes, 48, value.x);
    put(i32, bytes, 52, value.y);
    put(i32, bytes, 56, value.w);
    put(i32, bytes, 60, value.h);
    put(u32, bytes, 64, value.gui_revision);
    put(u32, bytes, 68, value.flags);
    put(u64, bytes, 72, value.idle_timeouts);
    put(u64, bytes, 80, value.redraws);
    put(u64, bytes, 88, value.input_generation);
    put(u32, bytes, 96, value.service_handle);
    put(u32, bytes, 100, value.screenshot_state);
    put(u64, bytes, 104, value.screenshot_bytes);
    put(u32, bytes, 112, value.restart_state);
    put(u32, bytes, 116, value.restart_old_handle);
    put(i32, bytes, 120, value.restart_result);
    put(u64, bytes, 128, value.present_failures);
    put(u64, bytes, 136, value.present_lost_frames);
}
pub fn decodeResponse(bytes: []const u8) !Response {
    const op = try valid(bytes, response_size);
    if (get(u32, bytes, 20) != 0 or get(u32, bytes, 36) != 0 or get(u32, bytes, 68) & ~@as(u32, 127) != 0 or
        get(u32, bytes, 100) > 2 or get(u32, bytes, 112) > 3 or get(u32, bytes, 124) != 0) return error.Reserved;
    const result: Response = .{ .op = op, .result = get(i32, bytes, 12), .desktop = identity(bytes, 16), .target = identity(bytes, 32), .x = get(i32, bytes, 48), .y = get(i32, bytes, 52), .w = get(i32, bytes, 56), .h = get(i32, bytes, 60), .gui_revision = get(u32, bytes, 64), .flags = get(u32, bytes, 68), .idle_timeouts = get(u64, bytes, 72), .redraws = get(u64, bytes, 80), .input_generation = get(u64, bytes, 88), .service_handle = get(u32, bytes, 96), .screenshot_state = get(u32, bytes, 100), .screenshot_bytes = get(u64, bytes, 104), .restart_state = get(u32, bytes, 112), .restart_old_handle = get(u32, bytes, 116), .restart_result = get(i32, bytes, 120), .present_failures = get(u64, bytes, 128), .present_lost_frames = get(u64, bytes, 136) };
    if (!result.desktop.valid()) return error.InvalidOwner;
    return result;
}

/// The job owns only identities, never client memory. Expiry/death can drop
/// this reservation without closing a single window or input stream.
pub const Session = struct {
    caller: Identity = .{},
    targets: [2]Identity = .{Identity{}} ** 2,
    deadline: u64 = 0,
    input_generation: u64 = 0,
    benchmark: bool = false,
    pub fn owns(self: *const Session, target: Identity) bool {
        for (self.targets) |value| if (value.same(target)) return true;
        return false;
    }
    pub fn bind(self: *Session, target: Identity) bool {
        if (!target.valid() or self.caller.same(target)) return false;
        if (self.owns(target)) return true;
        for (&self.targets) |*value| if (!value.valid()) {
            value.* = target;
            return true;
        };
        return false;
    }
};

pub const work_size = 120;
pub const completion_size = 24 + response_size;
pub const Work = struct { ticket: u64, deadline_tick: u64, broker: Identity, request: Request };
pub fn encodeWork(bytes: *[work_size]u8, work: ?Work) void {
    header(bytes, work_size);
    if (work) |value| {
        put(u64, bytes, 8, value.ticket);
        put(u64, bytes, 16, value.deadline_tick);
        writeIdentity(bytes, 24, value.broker);
        var request: [request_size]u8 = undefined;
        encodeRequest(&request, value.request);
        @memcpy(bytes[40..], &request);
    }
}
pub fn decodeWork(bytes: []const u8) !?Work {
    if (bytes.len != work_size or get(u32, bytes, 0) != magic or get(u16, bytes, 4) != 2 or get(u16, bytes, 6) != work_size) return error.InvalidContract;
    const ticket = get(u64, bytes, 8);
    if (ticket == 0) {
        if (!std.mem.allEqual(u8, bytes[16..], 0)) return error.Reserved;
        return null;
    }
    const broker = identity(bytes, 24);
    if (!broker.valid() or get(u32, bytes, 28) != 0) return error.InvalidOwner;
    return .{ .ticket = ticket, .deadline_tick = get(u64, bytes, 16), .broker = broker, .request = try decodeRequest(bytes[40..]) };
}
pub fn encodeCompletion(bytes: *[completion_size]u8, ticket: u64, broker: Identity, response: Response) void {
    @memset(bytes, 0);
    put(u64, bytes, 0, ticket);
    writeIdentity(bytes, 8, broker);
    var body: [response_size]u8 = undefined;
    encodeResponse(&body, response);
    @memcpy(bytes[24..], &body);
}
pub fn completionTicket(bytes: []const u8) !u64 {
    if (bytes.len != completion_size or get(u64, bytes, 0) == 0 or get(u32, bytes, 12) != 0 or !identity(bytes, 8).valid()) return error.InvalidContract;
    return get(u64, bytes, 0);
}
pub fn completionBroker(bytes: []const u8) !Identity {
    _ = try completionTicket(bytes);
    return identity(bytes, 8);
}

test "active wire is current exact and has no pointers; reservation binds exact generations" {
    try std.testing.expect(!(@as(Identity, .{})).same(.{}));
    const options: SnesOptions = .{ .pal = true, .mode = .paired, .maximized = true, .check_only = true };
    try std.testing.expectEqualDeep(options, try SnesOptions.decode(options.bits()));
    try std.testing.expectError(error.InvalidBenchmarkOptions, SnesOptions.decode(6));
    try std.testing.expectError(error.InvalidBenchmarkOptions, SnesOptions.decode(32));
    const request: Request = .{ .op = .resize, .caller = .{ .instance = 3, .generation = 0x100000004 }, .target = .{ .instance = 9, .generation = 77 }, .dx = -32, .dy = 24, .deadline_tick = 500 };
    var bytes: [request_size]u8 = undefined;
    encodeRequest(&bytes, request);
    try std.testing.expectEqualDeep(request, try decodeRequest(&bytes));
    var work: [work_size]u8 = undefined;
    encodeWork(&work, .{ .ticket = 90, .deadline_tick = 100, .broker = request.caller, .request = request });
    try std.testing.expectEqualDeep(request, (try decodeWork(&work)).?.request);
    encodeWork(&work, null);
    try std.testing.expectEqual(@as(?Work, null), try decodeWork(&work));
    try std.testing.expectError(error.InvalidContract, decodeRequest(bytes[0..79]));
    bytes[4] = 0;
    try std.testing.expectError(error.InvalidContract, decodeRequest(&bytes));
    encodeRequest(&bytes, request);
    bytes[72] = 1;
    try std.testing.expectError(error.Reserved, decodeRequest(&bytes));
    var session: Session = .{ .caller = request.caller };
    try std.testing.expect(session.bind(request.target));
    try std.testing.expect(!session.owns(.{ .instance = 9, .generation = 78 }));
    try std.testing.expect(!session.bind(request.caller));
    var response: [response_size]u8 = undefined;
    const expected: Response = .{ .op = .snapshot, .desktop = .{ .instance = 4, .generation = 51 }, .x = -3, .flags = focused };
    encodeResponse(&response, expected);
    try std.testing.expectEqualDeep(expected, try decodeResponse(&response));
    response[127] = 1;
    try std.testing.expectError(error.Reserved, decodeResponse(&response));
}

// launch_status retains the existing response byte layout: x=pending loads,
// y=workers inside productive spawn, redraws/input_generation are the ordinary
// Desktop owner counters. No process identity is fabricated from a launch slot.
