//! Native fixture only: the same original R4STD provider used by the existing
//! Desktop configuration cases. Rust calls this initializer once, serialized.
const provider = @import("provider");
const r = @import("r4os");
const colors = @import("color_provider");
const gpu_model = @import("gpu_owner_model");
export fn r4desk_wallpaper_check(callback: gpu_model.NativeWallpaperCallback) callconv(.c) i32 {
    return gpu_model.nativeWallpaperCheck(callback);
}
export fn r4desk_catalog_context() callconv(.c) *const r.abi.R4XStartContext {
    return gpu_model.nativeCatalogContext();
}
export fn r4desk_catalog_control(op: u32, value: u64) callconv(.c) i32 {
    return gpu_model.nativeCatalogControl(op, value);
}
export fn r4desk_cpu_transform(parameters: *const [11]i64, source: [*]const u32, count: usize, output: [*]u32, length: usize) callconv(.c) void {
    gpu_model.nativeCpuTransform(parameters, source[0..count], output[0..length]);
}
export fn r4desk_gpu_context() callconv(.c) ?*const r.abi.R4XStartContext {
    return gpu_model.nativeContext();
}
export fn r4desk_gpu_control(op: u32, value: u64, device: *const colors.c.R4GfxDevice) callconv(.c) i32 {
    return gpu_model.nativeControl(op, value, device);
}
export fn r4desk_gpu_visible(out: *[64]u32) callconv(.c) void {
    gpu_model.nativeVisible(out);
}
export fn r4desk_gpu_stats(out: *[16]u64) callconv(.c) void {
    gpu_model.nativeStats(out);
}
export fn r4desk_gpu_window_memory(op: u32, input: usize, bytes: u64, output: usize) callconv(.c) i32 {
    return gpu_model.nativeWindowMemory(op, input, bytes, output);
}
export fn r4desk_gpu_window_transport(op: u32, input: usize, output: usize) callconv(.c) i32 {
    return gpu_model.nativeWindowTransport(op, input, output);
}
export fn r4desk_color_transform(source: *const colors.c.R4GfxColorImage, target: *const colors.c.R4GfxColorImage, request: *const colors.c.R4GfxColorTransform, stats: *colors.c.R4GfxCpuStats) callconv(.c) i32 {
    return colors.color_api.imageTransform(source, target, request, stats);
}
export fn r4desk_fixture_sha256(bytes: [*]const u8, count: usize, out: *[32]u8) callconv(.c) void {
    @import("std").crypto.hash.sha2.Sha256.hash(bytes[0..count], out, .{});
}
export fn r4desk_wire_check(kind: u32, bytes: [*]const u8, count: usize) callconv(.c) u32 {
    const std = @import("std");
    const owner: r.abi.ProgramProcessHandle = .{ .instance_id = 86, .generation = 0x123456789abcdef0 };
    const check = @import("desktop_check");
    const request: check.Request = .{ .op = .resize, .caller = .{ .instance = 86, .generation = owner.generation }, .target = .{ .instance = 9, .generation = 77 }, .dx = -32, .dy = 24, .item = 0x1122334455667788, .deadline_tick = 500 };
    if (kind == 5) {
        const actual = check.decodeRequest(bytes[0..count]) catch return 0;
        return @intFromBool(std.meta.eql(request, actual));
    }
    if (kind == 6) {
        const actual = check.decodeWork(bytes[0..count]) catch return 0;
        const value = actual orelse return 0;
        return @intFromBool(value.ticket == 90 and value.deadline_tick == 100 and value.broker.same(request.caller) and std.meta.eql(value.request, request));
    }
    if (kind == 7) {
        const actual = check.decodeResponse(bytes[0..count]) catch return 0;
        const expected: check.Response = .{ .op = .snapshot, .desktop = request.caller, .target = request.target, .x = -3, .flags = check.focused | check.frame_valid, .screenshot_state = 2, .screenshot_bytes = 3686454, .restart_state = 3, .restart_result = -17, .idle_timeouts = 0xfedcba9876543210, .present_failures = 0xfedcba9876543210, .present_lost_frames = 0x8877665544332211 };
        return @intFromBool(std.meta.eql(actual, expected));
    }
    if (kind == 8) {
        return @intFromBool((check.completionTicket(bytes[0..count]) catch return 0) == 90 and
            (check.completionBroker(bytes[0..count]) catch return 0).same(request.caller));
    }
    if (kind == 1 and count == @sizeOf(r.abi.TrayDesktopExchange)) {
        var expected: r.abi.TrayDesktopExchange = .{ .desktop_owner = owner, .known_revision = 0xfedcba9876543210 };
        expected.event.sequence = 42;
        expected.item.icon[255] = 0xabcdef01;
        expected.item.tooltip[63] = 'Z';
        const actual = std.mem.bytesToValue(r.abi.TrayDesktopExchange, bytes[0..@sizeOf(r.abi.TrayDesktopExchange)]);
        return @intFromBool(std.meta.eql(expected, actual));
    }
    if (kind == 2 and count == @sizeOf(r.abi.DisplayColorExchange)) {
        var expected: r.abi.DisplayColorExchange = .{ .base = .{ .desktop_owner = owner, .status = .{ .desktop_epoch = owner.generation, .revision = 1 } } };
        expected.color.output.connection_generation = 0x1122334455667788;
        expected.color.signal.metadata.primaries[5] = 12345;
        const actual = std.mem.bytesToValue(r.abi.DisplayColorExchange, bytes[0..@sizeOf(r.abi.DisplayColorExchange)]);
        return @intFromBool(std.meta.eql(expected, actual));
    }
    if (kind == 3 and count == @sizeOf(r.abi.WindowGraphicsConsumer)) {
        var expected: r.abi.WindowGraphicsConsumer = .{ .desktop = owner, .request_serial = 0xfedcba9876543210, .chain = 55, .acquire_token = 66, .image_slot = 2, .action = r.abi.window_graphics_return };
        expected.surface.owner = owner;
        expected.surface.serial = 42;
        expected.fence = .{ .slot = 17, .adapter_id = 1, .timeline = 55, .point = 88, .device_generation = 99, .reset_generation = 44 };
        return @intFromBool(std.meta.eql(expected, std.mem.bytesToValue(r.abi.WindowGraphicsConsumer, bytes[0..@sizeOf(r.abi.WindowGraphicsConsumer)])));
    }
    if (kind == 4 and count == @sizeOf(r.abi.WindowGraphicsReply)) {
        var expected: r.abi.WindowGraphicsReply = .{ .result = r.abi.window_graphics_ok, .revision = 0x1122334455667788 };
        expected.frame.surface.owner = owner;
        expected.frame.acquire_token = 66;
        expected.frame.source.buffer = .{ .id = 3, .generation = 11 };
        expected.frame.source.reference = .{ .id = 4, .generation = 12 };
        expected.frame.descriptor.plane_pitches[3] = 0x123456789abcdef0;
        expected.frame.format.color[47] = 19;
        expected.config.backend.profile.data[63] = 197;
        expected.config.formats[7].color[47] = 211;
        return @intFromBool(std.meta.eql(expected, std.mem.bytesToValue(r.abi.WindowGraphicsReply, bytes[0..@sizeOf(r.abi.WindowGraphicsReply)])));
    }
    return 0;
}
var system: r.abi.R4XStartR4Sys = .{};
var imports: [6]r.abi.R4XStartImport = undefined;
var context: r.abi.R4XStartContext = undefined;
export fn r4desk_standard_context() callconv(.c) *const r.abi.R4XStartContext {
    imports[0] = .{ .group_id = @intFromEnum(r.abi.R4LGroup.r4sys), .min_version = 1, .resolved_version = 1, .flags = r.abi.r4xstart_import_flag_group_interface, .module_name = @intFromPtr(@as([:0]const u8, "R4SYS").ptr), .symbol_name = @intFromPtr(@as([:0]const u8, "Query").ptr), .table = @intFromPtr(&system) };
    const names = [_][:0]const u8{ "TEXT_V1", "SETTINGS_V1", "DATE_V1", "TIME_V1", "CONFIG_V1" };
    const tables = [_]usize{ @intFromPtr(&provider.r4std_text_v1), @intFromPtr(&provider.r4std_settings_v1), @intFromPtr(&provider.r4std_date_v1), @intFromPtr(&provider.r4std_time_v1), @intFromPtr(&provider.r4std_config_v1) };
    for (names, tables, 0..) |name, table, i| imports[i + 1] = .{ .group_id = 0, .min_version = 1, .resolved_version = 1, .module_name = @intFromPtr(@as([:0]const u8, "R4STD").ptr), .symbol_name = @intFromPtr(name.ptr), .table = table };
    context = .{ .app_class = @intFromEnum(r.abi.R4XStartAppClass.console), .instance_id = 86, .flags = r.abi.r4xstart_flag_imports_valid, .imports = @intFromPtr(&imports), .import_count = imports.len };
    return &context;
}

// Independent host JSON parser for copied owner-report boundaries only.
export fn r4desk_json_valid(bytes: [*]const u8, count: usize) callconv(.c) i32 {
    const std = @import("std");
    return @intFromBool(std.json.validate(std.heap.page_allocator, bytes[0..count]) catch return -1);
}
