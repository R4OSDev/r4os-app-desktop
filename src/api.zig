const r4os = @import("r4os");
const std = @import("std");
const paint = @import("paint.zig");
const scene_buffer = @import("scene_buffer.zig");
const surface = @import("surface.zig");

pub const Context = struct {
    sys: r4os.r4sys.Context,
    dev: r4os.r4dev.Context,
    desk: r4os.r4desk.Context,
    draw: r4os.r4draw.Context,
    self_handle: r4os.abi.ProgramProcessHandle,
    scene: ?*scene_buffer.SceneBuffer = null,

    pub fn init(app: *r4os.App) ?Context {
        const sys = app.system();
        const instance_id = app.startContext().instance_id;
        if (instance_id == 0 or instance_id > std.math.maxInt(u32)) return null;
        var self_handle: r4os.abi.ProgramProcessHandle = .{};
        if (sys.programOpenHandle(@intCast(instance_id), &self_handle) != r4os.abi.program_handle_ok) return null;
        return .{
            .sys = sys,
            .dev = app.devicesLowLevel() orelse return null,
            .desk = app.desktop() orelse return null,
            .draw = app.drawing() orelse return null,
            .self_handle = self_handle,
            .scene = null,
        };
    }

    pub fn argsRaw(self: *const Context) [*:0]const u8 {
        return self.sys.argsRaw();
    }

    pub fn print(self: *const Context, value: [*:0]const u8) void {
        self.sys.print(value);
    }

    pub fn putc(self: *const Context, ch: u8) void {
        self.sys.putc(ch);
    }

    pub fn write(self: *const Context, value: []const u8) void {
        self.sys.write(value);
    }

    pub fn println(self: *const Context, value: []const u8) void {
        self.sys.println(value);
    }

    pub fn printU64(self: *const Context, value: u64) void {
        self.sys.printU64(value);
    }

    pub fn printI32(self: *const Context, value: i32) void {
        self.sys.printI32(value);
    }

    pub fn ticks(self: *const Context) u64 {
        return self.sys.ticks();
    }

    pub fn sleepTicks(self: *const Context, duration: u64) void {
        self.sys.sleepTicks(duration);
    }

    pub fn taskYield(self: *const Context) void {
        self.sys.taskYield();
    }

    pub fn timeState(self: *const Context) r4os.abi.TimeState {
        return self.sys.timeState();
    }

    pub fn bootReady(self: *const Context) i32 {
        return self.sys.bootReady();
    }

    pub fn timeServiceStatus(self: *const Context, out: *r4os.abi.TimeServiceStatus) i32 {
        return self.sys.timeServiceStatus(out);
    }

    pub fn systemHalt(self: *const Context) noreturn {
        self.sys.systemHalt();
    }

    pub fn systemReboot(self: *const Context) noreturn {
        self.sys.systemReboot();
    }

    pub fn systemPoweroff(self: *const Context) noreturn {
        self.sys.systemPoweroff();
    }

    pub fn programLaunch(self: *const Context, path: [*:0]const u8, args: [*:0]const u8, policy: r4os.abi.LaunchPolicy) i32 {
        return self.sys.programLaunch(path, args, policy);
    }

    pub fn programSpawnHandle(self: *const Context, path: [*:0]const u8, args: [*:0]const u8, policy: r4os.abi.LaunchPolicy, out_handle: *r4os.abi.ProgramProcessHandle) i32 {
        return self.sys.programSpawnHandle(path, args, policy, out_handle);
    }

    pub fn programSpawnWithConsoleHostHandle(self: *const Context, path: [*:0]const u8, args: [*:0]const u8, policy: r4os.abi.LaunchPolicy, host: r4os.abi.ConsoleHostKind, out_handle: *r4os.abi.ProgramProcessHandle) i32 {
        return self.desk.programSpawnWithConsoleHostHandle(path, args, policy, host, out_handle);
    }

    pub fn programHandleStatus(self: *const Context, handle: *const r4os.abi.ProgramProcessHandle, out: *r4os.abi.ProgramInstanceInfo) i32 {
        return self.sys.programHandleStatus(handle, out);
    }

    pub fn programHandleRequestClose(self: *const Context, handle: *const r4os.abi.ProgramProcessHandle) i32 {
        return self.sys.programHandleRequestClose(handle);
    }

    pub fn programHandleKill(self: *const Context, handle: *const r4os.abi.ProgramProcessHandle) i32 {
        return self.sys.programHandleKill(handle);
    }

    pub fn programHandleReap(self: *const Context, handle: *const r4os.abi.ProgramProcessHandle, out: *r4os.abi.ProgramProcessCompletion) i32 {
        return self.sys.programHandleReap(handle, out);
    }

    pub fn programInventoryBegin(self: *const Context, cursor: *r4os.abi.ProgramInventoryCursor, summary: *r4os.abi.ProgramInventorySummary) i32 {
        return self.sys.programInventoryBegin(cursor, summary);
    }

    pub fn programInventoryPrograms(self: *const Context, cursor: *r4os.abi.ProgramInventoryCursor, out: []r4os.abi.ProgramInstanceSnapshot, page: *r4os.abi.ProgramInventoryPageInfo) i32 {
        return self.sys.programInventoryPrograms(cursor, out, page);
    }

    pub fn programInventoryTasks(self: *const Context, cursor: *r4os.abi.ProgramInventoryCursor, out: []r4os.abi.ProgramTaskSnapshot, page: *r4os.abi.ProgramInventoryPageInfo) i32 {
        return self.sys.programInventoryTasks(cursor, out, page);
    }

    pub fn programInventoryThreads(self: *const Context, cursor: *r4os.abi.ProgramInventoryCursor, out: []r4os.abi.ProgramThreadSnapshot, page: *r4os.abi.ProgramInventoryPageInfo) i32 {
        return self.sys.programInventoryThreads(cursor, out, page);
    }

    pub fn programStatus(self: *const Context, out: *r4os.abi.ProgramStatus) void {
        self.sys.programStatus(out);
    }

    pub fn performanceInput(self: *const Context) ?r4os.abi.ProgramInputPerformanceInfo {
        return self.dev.performanceInput();
    }

    pub fn programClass(self: *const Context, path: [*:0]const u8, policy: r4os.abi.LaunchPolicy) i32 {
        return self.sys.programClass(path, policy);
    }

    pub fn fileRead(self: *const Context, path: [*:0]const u8, out: []u8) i32 {
        return self.sys.fileRead(path, out);
    }

    pub fn fileReadAt(self: *const Context, path: [*:0]const u8, offset: u32, out: []u8) i32 {
        return self.sys.fileReadAt(path, offset, out);
    }

    pub fn fileWrite(self: *const Context, path: [*:0]const u8, data: []const u8) i32 {
        return self.sys.fileWrite(path, data);
    }

    pub fn fileDelete(self: *const Context, path: [*:0]const u8) i32 {
        return self.sys.fileDelete(path);
    }

    pub fn fileRename(self: *const Context, old_path: [*:0]const u8, new_path: [*:0]const u8) i32 {
        return self.sys.fileRename(old_path, new_path);
    }

    pub fn dirEntry(self: *const Context, path: [*:0]const u8, index: u32, out: []u8) i32 {
        return self.sys.dirEntry(path, index, out);
    }

    pub fn dirCreate(self: *const Context, path: [*:0]const u8) i32 {
        return self.sys.dirCreate(path);
    }

    pub fn driveInfo(self: *const Context, index: u32) ?r4os.abi.DriveInfo {
        return self.sys.driveInfo(index);
    }

    pub fn exists(self: *const Context, path: [*:0]const u8) bool {
        return self.sys.exists(path);
    }

    pub fn readKey(self: *const Context) u8 {
        return self.desk.readKey();
    }

    pub fn readKeyCodepoint(self: *const Context) u32 {
        return self.desk.readKeyCodepoint();
    }

    pub fn mouseState(self: *const Context, out: *r4os.abi.Mouse) void {
        self.desk.mouseState(out);
    }

    pub fn mouseHide(self: *const Context) void {
        self.desk.mouseHide();
    }

    pub fn physicalKeyPoll(self: *const Context, out: *r4os.abi.PhysicalKeyEvent) i32 {
        return self.desk.physicalKeyPoll(out);
    }

    pub fn keyboardLayoutCurrent(self: *const Context, out: *r4os.abi.KeyboardLayoutInfo) i32 {
        return self.desk.keyboardLayoutCurrent(out);
    }

    pub fn keyboardLayoutAt(self: *const Context, index: u32, out: *r4os.abi.KeyboardLayoutInfo) i32 {
        return self.desk.keyboardLayoutAt(index, out);
    }

    pub fn keyboardLayoutSet(self: *const Context, name_value: [*:0]const u8) i32 {
        return self.desk.keyboardLayoutSet(name_value);
    }

    pub fn programSetWindowHandle(self: *const Context, handle: *const r4os.abi.ProgramProcessHandle, window_id: i32) i32 {
        return self.desk.programSetWindowHandle(handle, window_id);
    }

    pub fn programSetConsoleHost(self: *const Context, instance_id: u32, host: r4os.abi.ConsoleHostKind) i32 {
        return self.desk.programSetConsoleHost(instance_id, host);
    }

    pub fn programTakeHostLaunch(self: *const Context, instance_id: u32, out: *r4os.abi.ProgramHostLaunchRequest) i32 {
        return self.desk.programTakeHostLaunch(instance_id, out);
    }

    pub fn guiSetWindowInfo(self: *const Context, instance_id: u32, info: *const r4os.abi.GuiWindowInfo) i32 {
        return self.desk.guiSetWindowInfo(instance_id, info);
    }

    pub fn guiPushEvent(self: *const Context, instance_id: u32, event: *const r4os.abi.GuiEvent) i32 {
        return self.desk.guiPushEvent(instance_id, event);
    }

    pub fn guiText(self: *const Context, instance_id: u32, out: []u8) i32 {
        return self.desk.guiText(instance_id, out);
    }

    pub fn guiRevision(self: *const Context, instance_id: u32) u32 {
        return self.desk.guiRevision(instance_id);
    }

    pub fn guiCommand(self: *const Context, instance_id: u32, index: u32, out: *r4os.abi.GuiCommand) i32 {
        return self.desk.guiCommand(instance_id, index, out);
    }

    pub fn supportsGuiFrameContract(self: *const Context) bool {
        return self.draw.supportsGuiFrameContract();
    }

    pub fn guiFrameInfo(self: *const Context, handle: ?*const r4os.abi.ProgramProcessHandle, out: *r4os.abi.GuiFrameInfo) i32 {
        return self.draw.guiFrameInfo(handle, out);
    }

    pub fn guiFrameRead(
        self: *const Context,
        handle: *const r4os.abi.ProgramProcessHandle,
        expected_generation: u64,
        commands: []r4os.abi.GuiFrameCommand,
        resources: []u8,
        out: *r4os.abi.GuiFrameInfo,
    ) i32 {
        return self.draw.guiFrameRead(handle, expected_generation, commands, resources, out);
    }

    pub fn guiFrameGenerationInfo(
        self: *const Context,
        handle: *const r4os.abi.ProgramProcessHandle,
        generation: u64,
        out: *r4os.abi.GuiFrameGenerationInfo,
    ) i32 {
        return self.draw.guiFrameGenerationInfo(handle, generation, out);
    }

    pub fn guiFrameGenerationRead(
        self: *const Context,
        handle: *const r4os.abi.ProgramProcessHandle,
        generation: u64,
        commands: []r4os.abi.GuiFrameCommand,
        resources: []u8,
        regions: []r4os.abi.DisplayDamageRect,
        out: *r4os.abi.GuiFrameGenerationInfo,
    ) i32 {
        return self.draw.guiFrameGenerationRead(handle, generation, commands, resources, regions, out);
    }

    pub fn guiTitle(self: *const Context, instance_id: u32, out: []u8) i32 {
        return self.desk.guiTitle(instance_id, out);
    }

    pub fn guiMinSize(self: *const Context, instance_id: u32, out: *r4os.abi.GuiSize) i32 {
        return self.desk.guiMinSize(instance_id, out);
    }

    pub fn consoleOutput(self: *const Context, instance_id: u32, out: []u8) i32 {
        return self.desk.consoleOutput(instance_id, out);
    }

    pub fn consoleRevision(self: *const Context, instance_id: u32) u32 {
        return self.desk.consoleRevision(instance_id);
    }

    pub fn consoleState(self: *const Context, instance_id: u32, out: *r4os.abi.ConsoleState) i32 {
        return self.desk.consoleState(instance_id, out);
    }

    pub fn consoleSetMetrics(self: *const Context, instance_id: u32, cols: u32, rows: u32) i32 {
        return self.desk.consoleSetMetrics(instance_id, cols, rows);
    }

    pub fn consolePushKey(self: *const Context, instance_id: u32, key: u8) i32 {
        return self.desk.consolePushKey(instance_id, key);
    }

    pub fn consolePushInput(self: *const Context, instance_id: u32, data: []const u8) i32 {
        return self.desk.consolePushInput(instance_id, data);
    }

    pub fn trayDesktopExchange(
        self: *const Context,
        op: u16,
        request: *const r4os.abi.TrayDesktopExchange,
        out: *r4os.abi.TrayDesktopExchange,
    ) i32 {
        var info: r4os.abi.ServiceInfo = .{};
        const opened = self.sys.serviceOpen(r4os.abi.window_service_name, &info);
        if (opened != r4os.abi.service_api_result_ok or info.handle == 0) return opened;
        defer _ = self.sys.serviceClose(info.handle);

        var header: r4os.abi.ServiceMessageHeader = .{};
        const got = self.sys.serviceCall(
            info.handle,
            op,
            std.mem.asBytes(request),
            &header,
            std.mem.asBytes(out),
            self.sys.ticksFromMilliseconds(250),
        );
        if (got != @as(i32, @intCast(@sizeOf(r4os.abi.TrayDesktopExchange)))) return if (got < 0) got else r4os.abi.service_api_result_buffer_too_small;
        if (header.status != r4os.abi.service_api_result_ok) return header.status;
        return r4os.abi.service_api_result_ok;
    }

    pub fn audioMasterState(self: *const Context, out: *r4os.abi.AudioServiceMasterState) i32 {
        return self.audioMasterCall(r4os.abi.audio_service_op_master_status, "", out);
    }

    pub fn audioSetMasterState(self: *const Context, request: *const r4os.abi.AudioServiceMasterRequest, out: *r4os.abi.AudioServiceMasterState) i32 {
        return self.audioMasterCall(r4os.abi.audio_service_op_set_master_state, std.mem.asBytes(request), out);
    }

    fn audioMasterCall(self: *const Context, op: u16, payload: []const u8, out: *r4os.abi.AudioServiceMasterState) i32 {
        var info: r4os.abi.ServiceInfo = .{};
        const opened = self.sys.serviceOpen("AUDSVC", &info);
        if (opened != r4os.abi.service_api_result_ok or info.handle == 0) return opened;
        defer _ = self.sys.serviceClose(info.handle);

        var header: r4os.abi.ServiceMessageHeader = .{};
        const got = self.sys.serviceCall(
            info.handle,
            op,
            payload,
            &header,
            std.mem.asBytes(out),
            self.sys.ticksFromMilliseconds(100),
        );
        if (got != @as(i32, @intCast(@sizeOf(r4os.abi.AudioServiceMasterState)))) return if (got < 0) got else r4os.abi.service_api_result_buffer_too_small;
        if (header.status != r4os.abi.service_api_result_ok) return header.status;
        if (out.magic != r4os.abi.audio_master_state_magic or out.version != r4os.abi.audio_master_state_version or
            out.size != @sizeOf(r4os.abi.AudioServiceMasterState)) return r4os.abi.service_api_result_invalid;
        return r4os.abi.service_api_result_ok;
    }

    pub fn windowServiceStatus(self: *const Context, out: *r4os.abi.WindowServiceStatus) i32 {
        var info: r4os.abi.ServiceInfo = .{};
        const rc = self.sys.serviceOpen(r4os.abi.window_service_name, &info);
        if (rc != r4os.abi.service_api_result_ok or info.handle == 0) return rc;
        defer _ = self.sys.serviceClose(info.handle);

        var header: r4os.abi.ServiceMessageHeader = .{};
        var response: [@sizeOf(r4os.abi.WindowServiceStatus)]u8 = .{0} ** @sizeOf(r4os.abi.WindowServiceStatus);
        const got = self.sys.serviceCall(info.handle, r4os.abi.window_service_op_status, "", &header, response[0..], self.sys.ticksFromMilliseconds(250));
        if (got < @as(i32, @intCast(@sizeOf(r4os.abi.WindowServiceStatus))) or header.status != r4os.abi.service_api_result_ok) return -1;
        const out_bytes: [*]u8 = @ptrCast(out);
        @memcpy(out_bytes[0..@sizeOf(r4os.abi.WindowServiceStatus)], response[0..@sizeOf(r4os.abi.WindowServiceStatus)]);
        if (out.magic != r4os.abi.window_service_status_magic or out.version != r4os.abi.window_service_status_version) return -1;
        return 0;
    }

    pub fn windowServiceSnapshot(self: *const Context, out: *r4os.abi.WindowServiceSnapshot) i32 {
        var info: r4os.abi.ServiceInfo = .{};
        const rc = self.sys.serviceOpen(r4os.abi.window_service_name, &info);
        if (rc != r4os.abi.service_api_result_ok or info.handle == 0) return rc;
        defer _ = self.sys.serviceClose(info.handle);

        var header: r4os.abi.ServiceMessageHeader = .{};
        var response: [@sizeOf(r4os.abi.WindowServiceSnapshot)]u8 = .{0} ** @sizeOf(r4os.abi.WindowServiceSnapshot);
        const got = self.sys.serviceCall(info.handle, r4os.abi.window_service_op_snapshot, "", &header, response[0..], self.sys.ticksFromMilliseconds(250));
        if (got < @as(i32, @intCast(@sizeOf(r4os.abi.WindowServiceSnapshot))) or header.status != r4os.abi.service_api_result_ok) return -1;
        const out_bytes: [*]u8 = @ptrCast(out);
        @memcpy(out_bytes[0..@sizeOf(r4os.abi.WindowServiceSnapshot)], response[0..@sizeOf(r4os.abi.WindowServiceSnapshot)]);
        if (out.magic != r4os.abi.window_service_snapshot_magic or out.version != r4os.abi.window_service_snapshot_version) return -1;
        return 0;
    }

    pub fn windowServiceRecord(self: *const Context, op: u16, record: *const r4os.abi.WindowServiceRecord, out: *r4os.abi.WindowServiceResult) i32 {
        var info: r4os.abi.ServiceInfo = .{};
        const rc = self.sys.serviceOpen(r4os.abi.window_service_name, &info);
        if (rc != r4os.abi.service_api_result_ok or info.handle == 0) return rc;
        defer _ = self.sys.serviceClose(info.handle);

        var header: r4os.abi.ServiceMessageHeader = .{};
        var response: [@sizeOf(r4os.abi.WindowServiceResult)]u8 = .{0} ** @sizeOf(r4os.abi.WindowServiceResult);
        const request: [*]const u8 = @ptrCast(record);
        const got = self.sys.serviceCall(info.handle, op, request[0..@sizeOf(r4os.abi.WindowServiceRecord)], &header, response[0..], self.sys.ticksFromMilliseconds(250));
        if (got < @as(i32, @intCast(@sizeOf(r4os.abi.WindowServiceResult))) or header.status != r4os.abi.service_api_result_ok) return -1;
        const out_bytes: [*]u8 = @ptrCast(out);
        @memcpy(out_bytes[0..@sizeOf(r4os.abi.WindowServiceResult)], response[0..@sizeOf(r4os.abi.WindowServiceResult)]);
        if (out.magic != r4os.abi.window_service_result_magic or out.version != r4os.abi.window_service_result_version) return -1;
        return out.result;
    }

    pub fn supportsClipboardContract(self: *const Context) bool {
        return self.desk.hasFn("clipboard_info");
    }

    pub fn clipboardWrite(self: *const Context, data: []const u8) i32 {
        return self.desk.clipboardWrite(data);
    }

    pub fn clipboardRead(self: *const Context, out: []u8) i32 {
        return self.desk.clipboardRead(out);
    }

    pub fn clipboardInfo(self: *const Context, out: *r4os.abi.ClipboardInfo) i32 {
        return self.desk.clipboardInfo(out);
    }

    pub fn clipboardClear(self: *const Context) i32 {
        return self.desk.clipboardClear();
    }

    pub fn remoteFrameInfo(self: *const Context, out: *r4os.abi.RemoteFrameInfo) i32 {
        return self.desk.remoteFrameInfo(out);
    }

    pub fn remoteFrameRead(self: *const Context, offset_pixels: u32, out: []u32, out_info: *r4os.abi.RemoteFrameInfo) i32 {
        return self.desk.remoteFrameRead(offset_pixels, out, out_info);
    }

    pub fn remoteFrameWait(self: *const Context, last_revision: u32, timeout_ticks: u64, out: *r4os.abi.RemoteFrameInfo) i32 {
        return self.desk.remoteFrameWait(last_revision, timeout_ticks, out);
    }

    pub fn supportsRemoteFrameDemand(self: *const Context) bool {
        return self.desk.supportsRemoteFrameDemand();
    }

    pub fn supportsRemoteFrameRegions(self: *const Context) bool {
        return self.desk.supportsRemoteFrameRegions();
    }

    pub fn remoteFrameAcquire(self: *const Context) i32 {
        return self.desk.remoteFrameAcquire();
    }

    pub fn remoteFrameRelease(self: *const Context) i32 {
        return self.desk.remoteFrameRelease();
    }

    pub fn remoteFrameConsumers(self: *const Context) u32 {
        return self.desk.remoteFrameConsumers();
    }

    pub fn desktopActivityWait(self: *const Context, last_seq: u64, timeout_ticks: u64, out_seq: *u64) i32 {
        return self.desk.desktopActivityWait(last_seq, timeout_ticks, out_seq);
    }

    pub fn remoteFramePublishSceneRect(self: *const Context, scene: *const scene_buffer.SceneBuffer, rect: surface.Rect, cursor_x: i32, cursor_y: i32) i32 {
        if (scene.width <= 0 or scene.height <= 0) return r4os.abi.remote_frame_error_invalid;
        const pixels = scene.pixels orelse return r4os.abi.remote_frame_error_unavailable;
        const clipped = scene.clipRect(rect) orelse scene.fullRect();
        var info = r4os.abi.RemoteFrameInfo{
            .flags = r4os.abi.remote_frame_flag_ready |
                r4os.abi.remote_frame_flag_dirty_valid |
                r4os.abi.remote_frame_flag_cursor_valid,
            .width = @intCast(scene.width),
            .height = @intCast(scene.height),
            .stride_pixels = @intCast(scene.width),
            .bytes_per_pixel = 4,
            .dirty_x = clipped.x,
            .dirty_y = clipped.y,
            .dirty_w = @intCast(clipped.w),
            .dirty_h = @intCast(clipped.h),
            .cursor_x = cursor_x,
            .cursor_y = cursor_y,
            .cursor_flags = r4os.abi.remote_frame_cursor_flag_visible,
        };
        const frame_pixels = @as(u64, info.width) * @as(u64, info.height);
        const max_frame_pixels: u64 = 0xffff_ffff / @sizeOf(u32);
        if (frame_pixels == 0 or frame_pixels > max_frame_pixels) return r4os.abi.remote_frame_error_invalid;
        info.frame_pixels = @intCast(frame_pixels);
        info.frame_bytes = @intCast(frame_pixels * @sizeOf(u32));
        return self.desk.remoteFramePublish(&info, pixels);
    }

    pub fn remoteFramePublishSceneRegions(self: *const Context, scene: *const scene_buffer.SceneBuffer, rects: []const surface.Rect, cursor_x: i32, cursor_y: i32) i32 {
        if (rects.len == 0 or rects.len > r4os.abi.display_damage_max_regions) return r4os.abi.remote_frame_error_invalid;
        if (scene.width <= 0 or scene.height <= 0) return r4os.abi.remote_frame_error_invalid;
        const pixels = scene.pixels orelse return r4os.abi.remote_frame_error_unavailable;
        var regions: [r4os.abi.display_damage_max_regions]r4os.abi.DisplayDamageRect =
            .{r4os.abi.DisplayDamageRect{}} ** r4os.abi.display_damage_max_regions;
        var bounds: surface.Rect = undefined;
        for (rects, 0..) |rect, index| {
            const clipped = scene.clipRect(rect) orelse return r4os.abi.remote_frame_error_out_of_range;
            regions[index] = .{ .x = clipped.x, .y = clipped.y, .w = @intCast(clipped.w), .h = @intCast(clipped.h) };
            bounds = if (index == 0) clipped else bounds.merged(clipped);
        }
        var info = r4os.abi.RemoteFrameInfo{
            .flags = r4os.abi.remote_frame_flag_ready | r4os.abi.remote_frame_flag_dirty_valid | r4os.abi.remote_frame_flag_cursor_valid,
            .width = @intCast(scene.width),
            .height = @intCast(scene.height),
            .stride_pixels = @intCast(scene.width),
            .bytes_per_pixel = 4,
            .dirty_x = bounds.x,
            .dirty_y = bounds.y,
            .dirty_w = @intCast(bounds.w),
            .dirty_h = @intCast(bounds.h),
            .cursor_x = cursor_x,
            .cursor_y = cursor_y,
            .cursor_flags = r4os.abi.remote_frame_cursor_flag_visible,
        };
        const frame_pixels = @as(u64, info.width) * info.height;
        const max_frame_pixels: u64 = 0xffff_ffff / @sizeOf(u32);
        if (frame_pixels == 0 or frame_pixels > max_frame_pixels) return r4os.abi.remote_frame_error_invalid;
        info.frame_pixels = @intCast(frame_pixels);
        info.frame_bytes = @intCast(frame_pixels * @sizeOf(u32));
        if (self.desk.supportsRemoteFrameRegions()) {
            return self.desk.remoteFramePublishRegions(&info, pixels, regions[0..rects.len]);
        }
        return self.desk.remoteFramePublish(&info, pixels);
    }

    pub fn remoteInputPoll(self: *const Context, out: *r4os.abi.RemoteInputEvent) i32 {
        return self.desk.remoteInputPoll(out);
    }

    pub fn remoteInputStatus(self: *const Context, out: *r4os.abi.RemoteInputStatus) i32 {
        return self.desk.remoteInputStatus(out);
    }

    pub fn screenWidth(self: *const Context) u32 {
        return self.draw.screenWidth();
    }

    pub fn screenHeight(self: *const Context) u32 {
        return self.draw.screenHeight();
    }

    pub fn allocator(self: *const Context) std.mem.Allocator {
        return self.sys.allocator();
    }

    pub fn beginScene(self: *Context, scene: *scene_buffer.SceneBuffer) void {
        self.scene = scene;
    }

    pub fn beginSceneClipped(self: *Context, scene: *scene_buffer.SceneBuffer, clip: surface.Rect) void {
        scene.setPaintClip(clip);
        self.scene = scene;
    }

    pub fn endScene(self: *Context) void {
        self.scene = null;
    }

    pub fn scenePaintBounds(self: *const Context) ?surface.Rect {
        const scene = self.scene orelse return null;
        return scene.paintBounds();
    }

    pub fn paintRect(self: *const Context, x: i32, y: i32, w: u32, h: u32, rgb: u32) void {
        if (self.scene) |scene| {
            paint.rectScene(scene, x, y, w, h, rgb);
            return;
        }
        paint.rect(&self.draw, x, y, w, h, rgb);
    }

    pub fn paintXrgb32(self: *const Context, x: i32, y: i32, w: u32, h: u32, pixels: []const u32) void {
        if (self.scene) |scene| {
            paint.xrgb32Scene(scene, x, y, w, h, pixels);
            return;
        }
        paint.xrgb32(&self.draw, x, y, w, h, pixels);
    }

    pub fn paintArgb32(self: *const Context, x: i32, y: i32, w: u32, h: u32, pixels: []const u32) bool {
        const needed = std.math.mul(usize, @as(usize, w), @as(usize, h)) catch return false;
        if (w == 0 or h == 0 or pixels.len != needed) return false;
        if (self.scene) |scene| {
            return scene.blendArgb32(scene.fullRect(), x, y, w, h, 1, std.mem.sliceAsBytes(pixels));
        }

        // The Desktop normally renders through the XRGB scene. Keep the
        // direct-display fallback bounded and transparent-pixel preserving;
        // partial alpha is flattened only on this emergency path.
        var row: usize = 0;
        while (row < h) : (row += 1) {
            var column: usize = 0;
            while (column < w) {
                while (column < w and (pixels[row * @as(usize, w) + column] >> 24) == 0) : (column += 1) {}
                const start = column;
                while (column < w and (pixels[row * @as(usize, w) + column] >> 24) != 0) : (column += 1) {}
                if (column == start) continue;
                var flattened: [r4os.abi.tray_icon_width]u32 = .{0} ** r4os.abi.tray_icon_width;
                const count = @min(column - start, flattened.len);
                var index: usize = 0;
                while (index < count) : (index += 1) flattened[index] = pixels[row * @as(usize, w) + start + index] & 0x00ff_ffff;
                paint.xrgb32(&self.draw, x + @as(i32, @intCast(start)), y + @as(i32, @intCast(row)), @intCast(count), 1, flattened[0..count]);
            }
        }
        return true;
    }

    pub fn paintAlpha8(self: *const Context, x: i32, y: i32, w: u32, h: u32, stride: u32, rgb: u32, alpha: []const u8) bool {
        const scene = self.scene orelse return false;
        return scene.blendAlpha8(x, y, w, h, stride, rgb, alpha);
    }

    pub fn paintText(self: *const Context, x: i32, y: i32, value: [*:0]const u8, fg: u32, bg: u32) void {
        if (self.scene) |scene| {
            paint.textScene(scene, &self.draw, x, y, value, fg, bg);
            return;
        }
        paint.text(&self.draw, x, y, value, fg, bg);
    }

    pub fn displayRevision(self: *const Context) u32 {
        return self.draw.displayRevision();
    }

    pub fn displayBeginFrameRect(self: *const Context, x: i32, y: i32, w: u32, h: u32) i32 {
        return self.draw.displayBeginFrameRect(x, y, w, h);
    }

    pub fn displayPresent(self: *const Context) i32 {
        return self.draw.displayPresent();
    }

    pub fn supportsDisplayBlitStride(self: *const Context) bool {
        return self.draw.hasFn("display_blit_xrgb32_stride");
    }

    pub fn displayBlitSceneRect(self: *const Context, scene: *const scene_buffer.SceneBuffer, rect: surface.Rect) i32 {
        const clipped = scene.clipRect(rect) orelse return 0;
        const pixels = scene.pixels orelse return -1;
        const x: u32 = @intCast(clipped.x);
        const y: u32 = @intCast(clipped.y);
        const w: u32 = @intCast(clipped.w);
        const h: u32 = @intCast(clipped.h);
        const scene_width: usize = @intCast(scene.width);
        const start_y: usize = @intCast(clipped.y);
        const start_x: usize = @intCast(clipped.x);

        if (self.supportsDisplayBlitStride()) {
            const offset = start_y * scene_width + start_x;
            const needed = (@as(usize, @intCast(clipped.h)) - 1) * scene_width + @as(usize, @intCast(clipped.w));
            return self.draw.displayBlitXrgb32Stride(
                @intCast(x),
                @intCast(y),
                w,
                h,
                @intCast(scene_width),
                pixels[offset .. offset + needed],
            );
        }

        if (clipped.x == 0 and clipped.w == scene.width) {
            const offset = start_y * scene_width;
            const count = @as(usize, @intCast(clipped.w)) * @as(usize, @intCast(clipped.h));
            return self.draw.displayBlitXrgb32(@intCast(x), @intCast(y), w, h, pixels[offset .. offset + count]);
        }

        var row: i32 = 0;
        while (row < clipped.h) : (row += 1) {
            const row_y: usize = @intCast(clipped.y + row);
            const offset = row_y * scene_width + start_x;
            const row_pixels = pixels[offset .. offset + @as(usize, @intCast(clipped.w))];
            const rc = self.draw.displayBlitXrgb32(@intCast(x), @intCast(clipped.y + row), w, 1, row_pixels);
            if (rc < 0) return rc;
        }
        return 1;
    }

    pub fn supportsDisplayPresentRegions(self: *const Context) bool {
        return self.draw.supportsDisplayPresentRegions();
    }

    pub fn displayPresentSceneRegions(
        self: *const Context,
        scene: *const scene_buffer.SceneBuffer,
        rects: []const surface.Rect,
        source_generation: u64,
        input_tick: u64,
        out: *r4os.abi.DisplayPresentResult,
    ) i32 {
        if (!self.supportsDisplayPresentRegions() or rects.len == 0 or rects.len > r4os.abi.display_damage_max_regions) return r4os.abi.display_present_error_unavailable;
        const pixels = scene.pixels orelse return r4os.abi.display_present_error_unavailable;
        if (scene.width <= 0 or scene.height <= 0) return r4os.abi.display_present_error_invalid;
        var regions: [r4os.abi.display_damage_max_regions]r4os.abi.DisplayDamageRect =
            .{r4os.abi.DisplayDamageRect{}} ** r4os.abi.display_damage_max_regions;
        for (rects, 0..) |rect, index| {
            const clipped = scene.clipRect(rect) orelse return r4os.abi.display_present_error_out_of_range;
            regions[index] = .{ .x = clipped.x, .y = clipped.y, .w = @intCast(clipped.w), .h = @intCast(clipped.h) };
        }
        const request = r4os.abi.DisplayPresentRequest{
            .flags = if (input_tick != 0) r4os.abi.display_present_request_flag_input_tick_valid else 0,
            .source_width = @intCast(scene.width),
            .source_height = @intCast(scene.height),
            .source_stride_pixels = @intCast(scene.width),
            .source_generation = source_generation,
            .input_tick = input_tick,
        };
        return self.draw.displayPresentRegions(&request, pixels, regions[0..rects.len], out);
    }

    pub fn displayPresentCapabilities(self: *const Context, out: *r4os.abi.DisplayPresentCapabilities) i32 {
        return self.draw.displayPresentCapabilities(out);
    }

    pub fn displayPresentCompletion(self: *const Context, fence: u64, out: *r4os.abi.DisplayPresentCompletion) i32 {
        return self.draw.displayPresentCompletion(fence, out);
    }

    pub fn guiRasterRead(self: *const Context, instance_id: u32, offset: u32, out: []u32) i32 {
        return self.draw.guiRasterRead(instance_id, offset, out);
    }

    pub fn fontMeasure(self: *const Context, font_id: u32, value: [*:0]const u8, out: *r4os.abi.GuiTextMetrics) i32 {
        return self.draw.fontMeasure(font_id, value, out);
    }

    pub fn paintTextFont(self: *const Context, font_id: u32, x: i32, y: i32, value: [*:0]const u8, fg: u32, bg: u32) void {
        if (self.scene) |scene| {
            paint.textFontScene(scene, &self.draw, font_id, x, y, value, fg, bg);
            return;
        }
        paint.textFont(&self.draw, font_id, x, y, value, fg, bg);
    }

    pub fn paintTextFontSlice(self: *const Context, font_id: u32, x: i32, y: i32, value: []const u8, fg: u32, bg: u32, bounds: surface.Rect) void {
        if (self.scene) |scene| {
            paint.textFontSliceScene(scene, &self.draw, font_id, x, y, value, fg, bg, bounds);
            return;
        }
        paint.textFontSlice(&self.draw, font_id, x, y, value, fg, bg, bounds);
    }
};
