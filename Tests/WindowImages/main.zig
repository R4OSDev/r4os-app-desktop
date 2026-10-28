//! Explicit owner fixture using the real Rust Desktop and WINSVC. Default
//! execution uses CPU queue/BOs; nativeProducer requires actual native GPU
//! receipts. Neither path supplies a physical visibility or latency receipt.
const std = @import("std");
const r = @import("r4os");
const a = r.abi;
const work = @import("desktop_check");
pub const NativeStatus = @import("native_status.zig");
var last_phase: []const u8 = "entry";
var last_rpc: i32 = 0;
var last_window: i32 = 0;
var last_native_phase: u32 = 0;
var last_native_result: i32 = 0;
const report = "C:\\TEMP\\WINIMG.TXT";
const path = "C:\\TEMP\\WINIMAGE.R4X";
const Context = struct {
    sys: r.r4sys.Context,
    draw: r.r4draw.Context,
    desk: r.r4desk.Context,
    owner: a.ProgramProcessHandle,
    endpoint: u32,
    deadline: u64,
    native: bool = false,
    native_report: ?[*:0]const u8 = null,
    native_report_failed: bool = false,
    fn check(self: *const Context) !void {
        if (self.native_report_failed) return error.NativeStatusWrite;
        if (self.sys.programShouldClose() or self.sys.ticks() >= self.deadline) return error.Timeout;
    }
    fn rpc(self: *const Context, op: u16, request: anytype) !a.WindowGraphicsReply {
        try self.check();
        var header: a.ServiceMessageHeader = .{};
        var reply: a.WindowGraphicsReply = .{};
        const got = self.sys.serviceCall(self.endpoint, op, std.mem.asBytes(request), &header, std.mem.asBytes(&reply), self.sys.ticksFromMilliseconds(1000));
        if (got != @sizeOf(a.WindowGraphicsReply) or header.status != 0) return error.GraphicsRPC;
        last_rpc = reply.result;
        return reply;
    }
    fn control(self: *const Context, op: work.Op, target: a.ProgramProcessHandle) !work.Response {
        var bytes: [work.request_size]u8 = undefined;
        var response: [work.response_size]u8 = undefined;
        var header: a.ServiceMessageHeader = .{};
        work.encodeRequest(&bytes, .{ .op = op, .caller = identity(self.owner), .target = identity(target), .deadline_tick = self.deadline });
        const got = self.sys.serviceCall(self.endpoint, work.operation, &bytes, &header, &response, self.sys.ticksFromMilliseconds(3000));
        if (got != response.len or header.status != 0) return error.DesktopRPC;
        const value = try work.decodeResponse(&response);
        if (value.op != op or value.result != 0) return error.DesktopWork;
        return value;
    }
    fn active(self: *const Context) !a.ProgramProcessHandle {
        var bytes: [128]u8 = undefined;
        var header: a.ServiceMessageHeader = .{};
        if (self.sys.serviceCall(self.endpoint, 4500, "", &header, &bytes, self.sys.ticksFromMilliseconds(1000)) != bytes.len or
            header.status != 0 or !std.mem.eql(u8, bytes[0..8], "RDM1\x01\x00\x80\x00")) return error.DesktopStatus;
        if (std.mem.readInt(u32, bytes[8..12], .little) & 1 == 0) return .{};
        return .{ .instance_id = std.mem.readInt(u32, bytes[32..36], .little), .generation = std.mem.readInt(u64, bytes[40..48], .little) };
    }
};
fn trace(c: *Context, value: []const u8) void {
    last_phase = value;
    if (c.native) {
        const path_value = c.native_report orelse { c.native_report_failed = true; return; };
        var bytes: [512]u8 = undefined;
        const text = NativeStatus.encode(&bytes, .{ .instance = c.owner.instance_id, .generation = c.owner.generation }, value) catch { c.native_report_failed = true; return; };
        if (c.sys.fileWrite(path_value, text) != @as(i32, @intCast(text.len))) c.native_report_failed = true;
    } else _ = c.sys.fileWrite(report, value);
}
fn identity(h: a.ProgramProcessHandle) work.Identity {
    return .{ .instance = h.instance_id, .generation = h.generation };
}
var failed_line: u32 = 0;
fn need(value: bool, comptime where: std.builtin.SourceLocation) !void {
    if (!value) {
        failed_line = where.line;
        return error.Receipt;
    }
}

fn retire(c: *const Context, handle: *a.ProgramProcessHandle, kill: bool) !void {
    if (handle.instance_id == 0) return;
    if (kill) _ = c.sys.programHandleKill(handle) else _ = c.sys.programHandleRequestClose(handle);
    var done: a.ProgramProcessCompletion = .{};
    try need(c.sys.programHandleWait(handle, c.sys.ticksFromMilliseconds(10000), &done) == 0 and
        std.meta.eql(done.handle, handle.*) and done.sequence != 0, @src());
    const code = done.exit_code;
    c.sys.write("WINIMAGE completion id=");
    c.sys.printU64(handle.instance_id);
    c.sys.write(" exit=");
    c.sys.printI32(code);
    c.sys.println("");
    var report_bytes: [512]u8 = undefined;
    const report_read = c.sys.fileRead(report, &report_bytes);
    if (report_read > 0 and report_read <= report_bytes.len) {
        c.sys.write(report_bytes[0..@intCast(report_read)]);
        c.sys.println("");
    }

    const sequence = done.sequence;
    var bytes: [2048]u8 = undefined;
    var read: u32 = 0;
    if (c.sys.programCompletionRead(handle, 0, &bytes, &read) == 0 and read <= bytes.len) c.sys.write(bytes[0..read]);
    const end = c.sys.ticks() +| c.sys.ticksFromMilliseconds(3000);
    while (true) {
        const result = c.sys.programHandleReap(handle, &done);
        if (result == 0) break;
        try need(result == a.program_handle_error_would_block and c.sys.ticks() < end, @src());
        c.sys.sleepTicks(1);
    }
    try need(std.meta.eql(done.handle, handle.*) and done.sequence == sequence and done.exit_code == code, @src());
    handle.* = .{};
    if (!kill) try need(code == 0, @src());
}

fn parent(c: *Context) !void {
    try need((try c.active()).instance_id == 0 and !c.sys.exists(report), @src());
    defer {
        if (c.sys.exists(report)) _ = c.sys.fileDelete(report);
    }
    var host: a.ProgramProcessHandle = .{};
    var child: a.ProgramProcessHandle = .{};
    defer retire(c, &host, true) catch c.sys.println("WINIMAGE cleanup host FAILED");
    defer retire(c, &child, true) catch c.sys.println("WINIMAGE cleanup child FAILED");
    try need(c.sys.programSpawnHandle("C:\\R4OS\\SOFTWARE\\DESKTOP\\R4DESK.R4X", "/CHECK-ISOLATED", .gui, &host) == 0, @src());
    while (true) {
        try c.check();
        const active = try c.active();
        if (active.instance_id != 0) {
            try need(std.meta.eql(active, host), @src());
            break;
        }
        c.sys.sleepTicks(1);
    }
    const started = try c.control(.begin, .{});
    try need(started.desktop.same(identity(host)) and started.flags & work.isolated != 0, @src());
    try need(c.sys.programSpawnHandle(path, "/PRODUCER", .gui, &child) == 0, @src());
    _ = try c.control(.bind, child);
    var done: a.ProgramProcessCompletion = .{};
    try need(c.sys.programHandleWait(&child, c.sys.ticksFromMilliseconds(20000), &done) == 0, @src());
    // The parent owns this exact spawn. Desktop binding does not transfer Reap.
    try retire(c, &child, false);
    _ = try c.control(.end, .{});
    try retire(c, &host, false);
    c.sys.println("WINIMAGE real Desktop/WINSVC two publications, revocation, exact child/host reap: OK");
}

fn producer(c: *Context) !void {
    try need(c.draw.guiClear(0x00101010) > 0 and c.draw.guiPresent() >= 0, @src());
    trace(c, "WINIMAGE producer static GUI published");
    var request: a.WindowGraphicsRequest = .{ .owner = c.owner };
    var discovery: a.WindowGraphicsReply = .{};
    while (true) {
        try c.check();
        var info: a.ProgramInstanceInfo = .{};
        try need(c.sys.programHandleStatus(&c.owner, &info) == 0, @src());
        last_window = @intCast(info.window_id);
        if (info.window_id > 0) {
            request.window_id = @intCast(info.window_id);
            discovery = try c.rpc(a.window_graphics_op_client, &request);
            if (discovery.result == a.window_graphics_ok) break;
        }
        c.sys.sleepTicks(1);
    }
    trace(c, "WINIMAGE producer surface discovered");
    const config = discovery.config;
    try need((if (c.native) config.backend.binding.adapter_id != 0 else config.backend.binding.adapter_id == 0) and config.width > 0 and config.height > 0, @src());
    request.surface = discovery.surface;
    request.config_revision = config.revision;
    var format: ?u32 = null;
    for (config.formats[0..config.format_count], 0..) |value, index| if (value.format == a.gfx_buffer_format_xrgb8888) {
        format = @intCast(index);
        break;
    };
    request.format_index = format orelse return error.Format;
    request.action = a.window_graphics_create_chain;
    request.request_serial = 1;
    request.image_count = 2;
    request.present_mode = a.window_graphics_fifo;
    const chain = try c.rpc(a.window_graphics_op_client, &request);
    try need(chain.result == a.window_graphics_ok, @src());
    request.chain = chain.chain;
    const bytes = @as(u64, config.width) * config.height * 4;
    const descriptor: a.GfxBufferDescriptor = .{ .byte_length = bytes, .width = config.width, .height = config.height, .format = a.gfx_buffer_format_xrgb8888, .plane_count = 1, .plane_pitches = .{ config.width * 4, 0, 0, 0 }, .usage = 31 };
    var refs = [_]a.GfxBufferReference{.{}} ** 3;
    defer for (&refs) |*value| {
        if (value.reference.id != 0) _ = c.draw.gfxBufferRelease(&value.reference);
    };
    for (&refs, 0..) |*value, index| {
        if (!c.native or index == 2) {
            try need(c.draw.gfxBufferCreate(&descriptor, value) == a.gfx_buffer_result_ok, @src());
        } else {
            var allocation: a.GfxNativeStatus = .{};
            last_phase = "native allocation start";
            last_rpc = c.draw.gfxNativeStart(&.{ .adapter_id = config.backend.binding.adapter_id, .memory_generation = config.backend.memory_generation, .kind = 1, .width = config.width, .height = config.height, .format = a.gfx_buffer_format_xrgb8888, .usage = 28, .layout = 0, .deadline_ns = (c.sys.monotonicNanoseconds() orelse return error.Clock) + 3_000_000_000 }, &allocation);
            last_native_phase = allocation.phase;
            last_native_result = allocation.result;
            try need(last_rpc == a.gfx_buffer_result_ok, @src());
            const ticket = allocation.request;
            var received = false;
            defer if (!received) {
                _ = c.draw.gfxNativeClose(&ticket);
            };
            last_phase = "native allocation wait";
            last_rpc = c.draw.gfxNativeWait(&ticket, c.sys.ticksFromMilliseconds(3000), &allocation);
            last_native_phase = allocation.phase;
            last_native_result = allocation.result;
            try need(last_rpc == a.gfx_buffer_result_ok and allocation.phase == 2 and allocation.result == a.gfx_buffer_result_ok, @src());
            last_phase = "native allocation receive";
            last_rpc = c.draw.gfxNativeReceive(&ticket, value);
            try need(last_rpc == a.gfx_buffer_result_ok, @src());
            received = true;
        }
    }
    for (0..2) |index| {
        request.action = a.window_graphics_attach;
        request.request_serial += 1;
        request.image_slot = @intCast(index);
        request.source = refs[index].reference;
        try need((try c.rpc(a.window_graphics_op_client, &request)).result == a.window_graphics_ok, @src());
    }
    trace(c, "WINIMAGE producer BOs attached");
    const queue = c.draw.queues();
    var handle: a.GfxQueueHandle = .{};
    const binding = config.backend.binding;
    const queue_config: a.GfxQueueConfig = if (c.native) .{ .adapter_id = binding.adapter_id, .device_generation = binding.device_generation, .reset_generation = binding.reset_generation, .milestone = binding.milestone } else .{};
    try need(queue.open(&queue_config, &handle) == a.gfx_queue_ok, @src());
    var queue_open = true;
    defer if (queue_open) {
        _ = queue.close(&handle);
    };
    var fences = [_]a.GfxFence{.{}} ** 2;
    defer for (fences) |fence| {
        if (fence.slot != 0) _ = queue.release(&fence);
    };
    var first: a.WindowGraphicsReply = .{};
    for (0..2) |round| {
        if (round != 0) {
            // Let the real Desktop observe an empty chain under foreign
            // activity before the next Present. No GUI revision changes.
            for (0..30) |_| {
                try need(c.desk.desktopActivityNotify() == 0, @src());
                c.sys.sleepTicks(1);
            }
        }
        request.action = a.window_graphics_acquire;
        request.request_serial += 1;
        const acquired = try c.rpc(a.window_graphics_op_client, &request);
        try need(acquired.result == a.window_graphics_ok, @src());
        if (round == 0) first = acquired;
        var map: a.GfxBufferMap = .{};
        try need(c.draw.gfxBufferMap(&refs[2].reference, a.gfx_buffer_map_write, 0, bytes, &map) == a.gfx_buffer_result_ok, @src());
        @memset(@as([*]u8, @ptrFromInt(map.cpu_address))[0..@intCast(bytes)], if (round == 0) 0x5a else 0x7b);
        try need(c.draw.gfxBufferUnmap(&map.lease) == a.gfx_buffer_result_ok, @src());
        var status: a.GfxFenceStatus = .{};
        var submission: a.GfxSubmission = .{ .source = refs[2].reference, .target = refs[acquired.image_slot].reference, .byte_length = bytes, .deadline_ns = (c.sys.monotonicNanoseconds() orelse return error.Clock) + 5_000_000_000 };
        if (c.native) {
            var target: a.GfxBufferDescriptor = .{};
            try need(c.draw.gfxBufferDescribe(&refs[acquired.image_slot].reference, &target) == a.gfx_buffer_result_ok and target.location == a.gfx_buffer_location_device_local and target.adapter_id == binding.adapter_id and target.device_generation == config.backend.memory_generation, @src());
            submission.operation = a.gfx_queue_operation_copy_rows;
            submission.byte_length = @as(u64, config.width) * 4;
            submission.row_count = config.height;
            submission.source_pitch = config.width * 4;
            submission.target_pitch = target.plane_pitches[0];
        }
        try need(queue.submit(&handle, &submission, &status) == a.gfx_queue_ok, @src());
        fences[round] = status.fence;
        try need(c.draw.gfxFenceWait(&status.fence, c.sys.ticksFromMilliseconds(3000), a.gfx_queue_wait_resources_released, &status) == a.gfx_queue_ok and status.result == a.gfx_queue_result_complete, @src());
        request.action = a.window_graphics_present;
        request.request_serial += 1;
        request.image_slot = acquired.image_slot;
        request.acquire_token = acquired.acquire_token;
        request.fence = status.fence;
        try need((try c.rpc(a.window_graphics_op_client, &request)).result == a.window_graphics_ok, @src());
        var inspect = request;
        inspect.action = a.window_graphics_chain_status;
        while (true) {
            const current = try c.rpc(a.window_graphics_op_client, &inspect);
            try need(current.result == a.window_graphics_ok, @src());
            if (current.flags == 4) {
                try need(current.frame.acquire_token == acquired.acquire_token, @src());
                break;
            }
            c.sys.sleepTicks(1);
        }
    }
    trace(c, "WINIMAGE producer two leases observed");
    // The second real Take retires the first image; verify its own slot and
    // fresh token only become reusable after the genuine consumer return.
    request.action = a.window_graphics_acquire;
    request.request_serial += 1;
    while (true) {
        const again = try c.rpc(a.window_graphics_op_client, &request);
        if (again.result == a.window_graphics_ok) {
            try need(again.image_slot == first.image_slot and again.acquire_token > first.acquire_token, @src());
            break;
        }
        try need(again.result == a.window_graphics_not_ready, @src());
        c.sys.sleepTicks(1);
    }
    trace(c, "WINIMAGE producer first image reused");
    request.action = a.window_graphics_close_chain;
    request.request_serial += 1;
    try need((try c.rpc(a.window_graphics_op_client, &request)).result == a.window_graphics_ok, @src());
    request.action = a.window_graphics_chain_status;
    while (true) {
        const ended = try c.rpc(a.window_graphics_op_client, &request);
        if (ended.result == a.window_graphics_closed and ended.chain == 0) break;
        c.sys.sleepTicks(1);
    }
    for (&fences) |*fence| {
        try need(queue.release(fence) == a.gfx_queue_ok, @src());
        fence.* = .{};
    }
    try need(queue.close(&handle) == a.gfx_queue_ok, @src());
    queue_open = false;
    for (&refs) |*value| {
        try need(c.draw.gfxBufferRelease(&value.reference) == a.gfx_buffer_result_ok, @src());
        value.* = .{};
    }
    trace(c, if (c.native) "WINIMAGE two real native frames leased, empty-wake progress, exact reuse and closed-chain ACK: OK" else "WINIMAGE two real CPU frames leased, empty-wake progress, exact reuse and closed-chain ACK: OK");
}
pub fn r4_app_main(app: *r.App) i32 {
    const sys = app.system();
    const child_mode = std.mem.eql(u8, app.args(), "/PRODUCER");
    var service: a.ServiceInfo = .{};
    if (sys.serviceOpen("WINSVC", &service) != 0) return 2;
    defer _ = sys.serviceClose(service.handle);
    var owner: a.ProgramProcessHandle = .{};
    if (sys.programOpenHandle(@intCast(app.raw.instance_id), &owner) != 0) return 3;
    var c: Context = .{ .sys = sys, .draw = app.drawing() orelse return 4, .desk = app.desktop() orelse return 5, .owner = owner, .endpoint = service.handle, .deadline = sys.ticks() +| sys.ticksFromMilliseconds(45000) };
    if (child_mode) {
        trace(&c, "WINIMAGE producer entry");
        c.deadline = sys.ticks() +| sys.ticksFromMilliseconds(15000);
        producer(&c) catch |err| {
            var failure: [256]u8 = undefined;
            trace(&c, std.fmt.bufPrint(&failure, "WINIMAGE producer FAILED: {s} line={d} window={d} rpc={d} phase={s}", .{ @errorName(err), failed_line, last_window, last_rpc, last_phase }) catch "WINIMAGE producer FAILED report");
            return 1;
        };
    } else parent(&c) catch |err| {
        sys.write("WINIMAGE parent FAILED: ");
        sys.write(@errorName(err));
        sys.write(" line=");
        sys.printU64(failed_line);
        sys.println("");
        return 1;
    };
    return 0;
}

/// Existing exact two-image/Return/reuse/Close-chain oracle on the actual
/// published native backend. Spectra owns the GUI child and its exact reap.
pub fn nativeProducer(app: *r.App, status_path: [*:0]const u8) i32 {
    const sys = app.system();
    var service: a.ServiceInfo = .{};
    if (sys.serviceOpen("WINSVC", &service) != 0) return 2;
    defer _ = sys.serviceClose(service.handle);
    var owner: a.ProgramProcessHandle = .{};
    if (sys.programOpenHandle(@intCast(app.raw.instance_id), &owner) != 0) return 3;
    var c: Context = .{ .sys = sys, .draw = app.drawing() orelse return 4, .desk = app.desktop() orelse return 5, .owner = owner, .endpoint = service.handle, .native = true, .native_report = status_path, .deadline = sys.ticks() +| sys.ticksFromMilliseconds(15000) };
    trace(&c, "WINIMAGE native entry");
    if (c.native_report_failed) return 8;
    producer(&c) catch |err| {
        var text: [256]u8 = undefined;
        trace(&c, std.fmt.bufPrint(&text, "WINIMAGE native FAILED: {s} line={d} rpc={d} phase={s} native-phase={d} native-result={d}", .{ @errorName(err), failed_line, last_rpc, last_phase, last_native_phase, last_native_result }) catch "WINIMAGE native FAILED");
        return 1;
    };
    trace(&c, NativeStatus.success);
    return if (c.native_report_failed) 8 else 0;
}
