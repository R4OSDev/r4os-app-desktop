const std = @import("std");
const r4os = @import("r4os");
const r4img = @import("r4img");
const r4std = @import("r4std");
const appearance_signal = @import("appearance_signal.zig");
const desk_api = @import("api.zig");
const compositor = @import("compositor.zig");
const desktop_config = @import("desktop_config.zig");
const desktop_folder = @import("desktop_folder.zig");
const desktop_items = @import("desktop_items.zig");
const desktop_layout = @import("desktop_layout.zig");
const draw = @import("draw.zig");
const gui_frame_snapshot = @import("gui_frame_snapshot.zig");
const message_box = @import("message_box.zig");
const model = @import("model.zig");
const paint = @import("paint.zig");
const physical_input = @import("physical_input.zig");
const quick_launch = @import("quick_launch.zig");
const run_command = @import("run_command.zig");
const scene_buffer = @import("scene_buffer.zig");
const start_menu = @import("start_menu.zig");
const surface = @import("surface.zig");
const text_field = @import("text_field.zig");
const theme = @import("theme.zig");
const tray = @import("tray.zig");
const volume = @import("volume.zig");
const wallpaper = @import("wallpaper.zig");
const window = @import("window.zig");
const window_service_gate = @import("window_service_gate.zig");

const Dialog = enum(u8) {
    none,
    run,
    message_notepad,
    message_synth,
    message_settings,
    message_run_invalid,
    message_run_no_programs,
    message_run_no_slots,
    message_run_console_busy,
    message_terminal_mode_busy,
    message_run_not_found,
    message_run_failed,
    message_app_error,
    message_window_info,
    confirm_restart,
    confirm_poweroff,
    confirm_halt,
    tasks,
};

const RunButton = enum {
    browse,
    ok,
    cancel,
};

const EventKind = enum(u8) {
    none,
    keyboard,
    mouse,
    timer,
};

const DragState = struct {
    active: bool = false,
    window_index: usize = 0,
    grab_x: i32 = 0,
    grab_y: i32 = 0,
    last_x: i32 = -1,
    last_y: i32 = -1,
};

const DesktopItemDragState = struct {
    pending: bool = false,
    active: bool = false,
    index: usize = desktop_items.no_selection,
    grab_x: i32 = 0,
    grab_y: i32 = 0,
    start_item_x: i32 = 0,
    start_item_y: i32 = 0,
    start_mouse_x: i32 = 0,
    start_mouse_y: i32 = 0,
    last_x: i32 = -1,
    last_y: i32 = -1,
};

const ResizeState = struct {
    active: bool = false,
    window_index: usize = 0,
    handle: window.ResizeHandle = .bottom_right,
    start: window.Geometry = .{ .x = 0, .y = 0, .w = 0, .h = 0 },
    start_mouse_x: i32 = 0,
    start_mouse_y: i32 = 0,
    last_x: i32 = -1,
    last_y: i32 = -1,
};

const ConsoleScrollState = struct {
    instance_id: u32 = 0,
    offset: u32 = 0,
    follow_tail: bool = true,
    clear_count: u32 = 0,
};

const CursorDamage = struct {
    active: bool = false,
    old_rect: surface.Rect = .{ .x = 0, .y = 0, .w = 0, .h = 0 },
    new_rect: surface.Rect = .{ .x = 0, .y = 0, .w = 0, .h = 0 },

    fn reset(self: *CursorDamage) void {
        self.* = .{};
    }
};

const RenderSnapshot = struct {
    redraws: u32 = 0,
    cursor: u32 = 0,
    mixed: u32 = 0,
    full: u32 = 0,
    pixels: u64 = 0,
    copy_bytes: u64 = 0,
    layout_worker_started: u32 = 0,
    layout_worker_completed: u32 = 0,
};

const HeadlessSubsystemLaunch = struct {
    index: usize,
    handle: r4os.abi.ProgramProcessHandle,
};

const menu_config_path = "C:\\R4OS\\SOFTWARE\\DESKTOP\\MENU.R4S";
const desktop_config_path = r4std.settings.paths.desktop;
const desktop_layout_path = r4std.settings.paths.desktop_layout;
const time_config_path = r4std.settings.paths.time;
const terminal_path = "/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X";
const terminal_desktop_args = "/NOAUTOEXEC";
const tray_provider_test_path = "/R4OS/SOFTWARE/TERMINAL/DIAG/APPZCON.R4X";
const tray_provider_test_args = "/TRAYSMOKE";
const tray_provider_test_item_id: u64 = 0x7103_0001;
const audio_service_module_path = "C:\\R4OS\\SERVICES\\AUDSVC.R4X";
const subsystem_host_test_path = "/R4OS/SUBSYSTEMS/test.basic/SUBSYSOK.R4X";
const subsystem_guest_a_path = "C:\\TEMP\\SUBSYSTEM-A.BAS";
const subsystem_guest_b_path = "C:\\TEMP\\SUBSYSTEM-B.BAS";
const subsystem_host_test_marker_path = "C:\\TEMP\\SUBSYS.OK";
const r4basic_host_path = "/R4OS/SUBSYSTEMS/r4os.basic/R4BASIC.R4X";
const r4basic_baseline_guest_path = "C:\\TEMP\\GORILLA.BAS";
const r4basic_baseline_marker_path = "C:\\TEMP\\R4BASIC.BASELINE";
const r4gb_host_path = "/R4OS/SUBSYSTEMS/r4os.gb/R4GB.R4X";
const r4gb_host_test_marker_path = "C:\\TEMP\\R4GB.HOST";
const r4gb_fixture_a_path = "C:\\TEMP\\R4GB-E2E-A.GB";
const r4gb_fixture_b_path = "C:\\TEMP\\R4GB-E2E-B.GBC";
const r4gb_fixture_cgb_path = "C:\\TEMP\\R4GB-CGB-ONLY.GBC";
const r4gb_trace_a = "00000000000000A1";
const r4gb_trace_b = "00000000000000B2";
const r4gb_trace_cgb = "00000000000000C3";
const r4gb_trace_end_key = "end";
const r4gb_report_a_path = "C:\\TEMP\\R4GB-00000000000000A1.REPORT";
const r4gb_report_b_path = "C:\\TEMP\\R4GB-00000000000000B2.REPORT";
const r4gb_report_cgb_path = "C:\\TEMP\\R4GB-00000000000000C3.REPORT";
const r4gb_cgb_rejection_exit: i32 = 71;
const subsystem_audio_service = "AUDSVC";
const subsystem_host_test_marker =
    "SUBSYSTEM host selftest: OK modes=640x350+320x200+256x224 formats=indexed8+xrgb32 damage=sparse indexed8=abi tiles=bounded input=sequenced+policy-filtered idle=no-frame fps>=20\r\n" ++
    "SUBSYSTEM runtime selftest: OK instances=2 slices=bounded time=monotonic audio=s16le-buffered lifecycle=pause+resume+reset+complete+close errors=isolated resources=closed\r\n" ++
    "R4BASIC source-probe: info_calls=1 read_calls=0 read_bytes=0 limit_bytes=262144\r\n" ++
    "DESKTOP terminal-close selftest: OK target=chrome request=cooperative duplicate=idempotent exit=0 reap=generation\r\n" ++
    "DESKTOP tray selftest: OK ipc=service owner=generation registry=16 icon=argb32 events=sequenced wait=bounded cleanup=owner\r\n" ++
    "DESKTOP volume selftest: OK tray=system states=5 popup=anchored input=mouse+keyboard gain=perceptual service=AUDSVC persistence=owned\r\n" ++
    "DESKTOP present selftest: OK regions=2 cursorblink=regional fence=sync backend=DISPBLIT fallback=armed remote=on-demand\r\n" ++
    "R4GB explorer E2E: OK catalog=MODULES.JSON assoc=ids-only formats=.gb+.gbc probe=bounded instances=2 input=physical video=frames audio=app-audio save=sram+rtc rejection=cgb-only close=witness+separate\r\n" ++
    "R4GB long-run E2E: OK duration=60s instances=2 focus=alternating lifecycle=pause+resume+reset+mute endings=witness+close desktop=responsive\r\n";
const console_title_max: usize = 31;
const console_path_max: usize = 63;
const console_args_max: usize = 127;
const window_launch_path_max: usize = 63;
const run_path_max: usize = run_command.max_input;
const default_run_path = "C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X";
const run_browse_dir = "C:\\R4OS\\SOFTWARE\\DESKTOP";
const run_browse_first_index: u32 = 2;
const run_browse_scan_limit: u32 = 96;
const desktop_folder_first_index: u32 = 2;
const desktop_folder_scan_limit: u32 = 512;
const assoc_config_max_bytes: usize = 4096;
const task_inventory_page_capacity: usize = @intCast(r4os.abi.program_inventory_page_max);
const task_inventory_restart_limit: u32 = 8;
const task_inventory_visible_rows: usize = 5;
const desktop_layout_save_stack: u64 = 128 * 1024;
const desktop_ui_blocker_warn_ticks: u64 = 2;
const app_window_first: usize = 1;
const default_monotonic_hz: u32 = 100;
const loop_sleep_ms: u32 = 10;
const double_click_ms: u32 = 250;
const desktop_drag_threshold_px: i32 = 4;
const blink_half_ms: u32 = 500;
const close_kill_timeout_ms: u32 = 2000;
const time_config_check_ms: u32 = 1000;
const window_service_retry_ms: u32 = 2000;
const tray_service_retry_ms: u32 = 2000;
const tray_sync_ms: u32 = 50;
const volume_poll_ms: u32 = 500;
const volume_send_ms: u32 = 50;
const tray_sync_page_burst: u32 = 16;
const remote_input_burst: u32 = 16;
const physical_input_burst: u32 = 32;
const console_scroll_wheel_lines: u32 = 3;
const console_scroll_max_fallback: u32 = 512;
const clipboard_buffer_size: usize = @as(usize, r4os.clipboard.max_text_bytes) + 1;
const system_menu_w: i32 = 150;
const system_menu_h: i32 = 108;
const system_menu_row_h: i32 = 20;

const DesktopLayoutAsyncSave = struct {
    sys: r4os.r4sys.Context = undefined,
    in_flight: bool = false,
    thread_handle: r4os.abi.ProgramJoinHandle = .{},
    generation: u32 = 0,
    started_tick: u64 = 0,
    len: usize = 0,
    bytes: [desktop_layout.max_bytes]u8 = .{0} ** desktop_layout.max_bytes,
};

pub const App = struct {
    ctx: *desk_api.Context,
    images: *const r4img.Context,
    screen_w: i32 = 1280,
    screen_h: i32 = 720,
    start_open: bool = false,
    terminal_mode: bool = false,
    menu_selected: usize = 0,
    menu_submenu_open: bool = false,
    menu_submenu_parent: usize = 0,
    menu_submenu_selected: usize = 0,
    menu_submenu_focus: bool = false,
    menu_nested_open: bool = false,
    menu_nested_parent: usize = 0,
    menu_nested_selected: usize = 0,
    menu_nested_focus: bool = false,
    menu: start_menu.Menu = start_menu.Menu.initDefault(),
    desktop_item_selected: usize = desktop_items.no_selection,
    desktop_items: desktop_items.Items = .{},
    quick_launch: quick_launch.Bar = quick_launch.Bar.initDefault(),
    assoc: r4std.app_assoc.Config = .{},
    assoc_loaded_from_file: bool = false,
    config: desktop_config.Config = .{},
    wallpaper_state: wallpaper.State = .{},
    time_config: r4std.time.Config = .{},
    dialog: Dialog = .none,
    system_menu_open: bool = false,
    system_menu_window: usize = 0,
    window_info_index: usize = 0,
    system_menu_x: i32 = 0,
    system_menu_y: i32 = 0,
    time_menu_open: bool = false,
    active_window: usize = 0,
    prev_buttons: u8 = 0,
    drag: DragState = .{},
    desktop_drag: DesktopItemDragState = .{},
    resize: ResizeState = .{},
    damage: surface.Dirty = .{},
    taskbar_damage: bool = false,
    cursor_damage: CursorDamage = .{},
    cursor_damage_queued_tick: u64 = 0,
    render_stats: compositor.RenderStats = .{},
    present_source_generation: u64 = 0,
    scene: scene_buffer.SceneBuffer = .{},
    cursor_x: i32 = 0,
    cursor_y: i32 = 0,
    event_kind: EventKind = .none,
    event: model.Event = .{},
    event_tick: u64 = 0,
    event_key: u32 = 0,
    event_mouse: r4os.abi.Mouse = .{ .x = 0, .y = 0, .dx = 0, .dy = 0, .wheel = 0, .buttons = 0, .present = 0, .reserved = 0, .packets = 0 },
    event_remote_input: bool = false,
    remote_prev_buttons: u8 = 0,
    remote_input_events: u32 = 0,
    remote_input_keys: u32 = 0,
    remote_input_mouse: u32 = 0,
    last_remote_input_sequence: u32 = 0,
    physical_input_events: u64 = 0,
    physical_input_forwarded: u64 = 0,
    physical_input_invalid: u64 = 0,
    last_physical_input_sequence: u64 = 0,
    remote_frame_consumers: u32 = 0,
    next_event_id: u32 = 0,
    last_mouse_down_tick: u64 = 0,
    last_mouse_down_target: model.UiTarget = .none,
    last_key_tick: u64 = 0,
    key_repeat_armed: bool = false,
    double_click_pending: bool = false,
    blink_phase: u8 = 0,
    monotonic_hz: u32 = default_monotonic_hz,
    loop_sleep_ticks: u64 = 3,
    // 0.56.28: event-getriebener Idle-Wait statt festem Sleep-Poll.
    activity_seq: u64 = 0,
    activity_wait_supported: bool = true,
    activity_wait_wakes: u64 = 0,
    activity_wait_timeouts: u64 = 0,
    double_click_ticks: u64 = 25,
    blink_half_ticks: u64 = 50,
    close_kill_timeout_ticks: u64 = 200,
    time_config_check_ticks: u64 = 100,
    desktop_layout_writeback: r4std.settings.Writeback = .{},
    desktop_layout_async: DesktopLayoutAsyncSave = .{},
    desktop_layout_generation: u32 = 0,
    desktop_layout_invalid_reported: bool = false,
    desktop_layout_recovery_reported: bool = false,
    desktop_layout_recovery_failed_reported: bool = false,
    next_time_config_check_tick: u64 = 0,
    keyboard_focus: model.UiTarget = .none,
    dialog_focus: model.UiTarget = .none,
    hover_target: model.UiTarget = .none,
    mouse_down_target: model.UiTarget = .none,
    console_title: [console_title_max + 1]u8 = .{0} ** (console_title_max + 1),
    console_path: [console_path_max + 1]u8 = .{0} ** (console_path_max + 1),
    console_args: [console_args_max + 1]u8 = .{0} ** (console_args_max + 1),
    console_scrolls: [4]ConsoleScrollState = .{ConsoleScrollState{}} ** 4,
    console_snapshots: [4]draw.TerminalSnapshot = .{draw.TerminalSnapshot{}} ** 4,
    dialog_title: [message_box.title_buffer_len]u8 = .{0} ** message_box.title_buffer_len,
    dialog_text: [message_box.text_buffer_len]u8 = .{0} ** message_box.text_buffer_len,
    app_error_title: [message_box.title_buffer_len]u8 = .{0} ** message_box.title_buffer_len,
    app_error_text: [message_box.text_buffer_len]u8 = .{0} ** message_box.text_buffer_len,
    message_box_kind: message_box.Kind = .info,
    message_box_buttons: message_box.Buttons = .ok,
    message_box_result: message_box.Result = .none,
    run_path: text_field.TextField(run_path_max) = .{},
    run_browse_index: u32 = run_browse_first_index,
    program_status: r4os.abi.ProgramStatus = .{},
    // The task overview uses a reusable double buffer. A failed refresh never
    // destroys the last complete generation, while both buffers retain their
    // dynamically grown capacity across timer refreshes.
    program_instances: std.ArrayList(r4os.abi.ProgramInstanceSnapshot) = .empty,
    program_inventory_staging: std.ArrayList(r4os.abi.ProgramInstanceSnapshot) = .empty,
    task_scroll: usize = 0,
    inventory_page: u32 = 0,
    inventory_restarts: u32 = 0,
    inventory_restart_pending: bool = false,
    inventory_out_of_memory: bool = false,
    win_service_status: r4os.abi.WindowServiceStatus = .{},
    win_service_snapshot: r4os.abi.WindowServiceSnapshot = .{},
    win_service_gate: window_service_gate.Gate = .{},
    window_service_retry_ticks: u64 = 200,
    tray_service_retry_ticks: u64 = 200,
    tray_sync_ticks: u64 = 5,
    tray_registry: tray.Registry = .{},
    tray_staging: tray.Registry = .{},
    tray_broker_revision: u64 = 0,
    tray_sync_revision: u64 = 0,
    tray_sync_cursor: u16 = r4os.abi.tray_desktop_cursor_poll,
    tray_next_sync_tick: u64 = 0,
    tray_visibility_dirty: bool = false,
    tray_hover_identity: tray.Identity = .{},
    tray_pressed_identity: tray.Identity = .{},
    tray_right_pressed_identity: tray.Identity = .{},
    last_mouse_down_tray_identity: tray.Identity = .{},
    volume_ui: volume.View = .{},
    volume_master: r4os.abi.AudioServiceMasterState = .{},
    volume_next_poll_tick: u64 = 0,
    volume_next_send_tick: u64 = 0,
    volume_poll_ticks: u64 = 50,
    volume_send_ticks: u64 = 5,
    volume_pending_flags: u32 = 0,
    volume_pending_percent: u8 = 100,
    volume_pending_muted: bool = false,
    volume_dragging: bool = false,
    window_service_mirrored: [4]bool = .{false} ** 4,
    window_process_handles: [4]r4os.abi.ProgramProcessHandle = .{r4os.abi.ProgramProcessHandle{}} ** 4,
    window_completion_handles: [4]r4os.abi.ProgramProcessHandle = .{r4os.abi.ProgramProcessHandle{}} ** 4,
    window_completion_exit_codes: [4]i32 = .{0} ** 4,
    headless_acceptance_terminal: bool = false,
    gui_frame_caches: [4]gui_frame_snapshot.Cache = .{gui_frame_snapshot.Cache{}} ** 4,
    window_launch_paths: [4][window_launch_path_max + 1]u8 = .{.{0} ** (window_launch_path_max + 1)} ** 4,
    last_display_revision: u32 = 0,
    clock: [9]u8 = .{ '0', '0', ':', '0', '0', 0, 0, 0, 0 },
    keyboard_layout: r4os.abi.KeyboardLayoutInfo = .{
        .name = namedZ(16, "en_en"),
        .display = namedZ(8, "EN"),
        .index = 0,
        .count = 2,
    },
    windows: [4]window.Window = .{
        .{
            .kind = .terminal,
            .x = 78,
            .y = 72,
            .w = 470,
            .h = 248,
            .normal_x = 78,
            .normal_y = 72,
            .normal_w = 470,
            .normal_h = 248,
            .visible = false,
            .minimized = false,
            .maximized = false,
        },
        .{
            .kind = .manager,
            .x = 620,
            .y = 110,
            .w = 450,
            .h = 250,
            .normal_x = 620,
            .normal_y = 110,
            .normal_w = 450,
            .normal_h = 250,
            .visible = false,
            .minimized = false,
            .maximized = false,
        },
        .{
            .kind = .app,
            .x = 680,
            .y = 160,
            .w = 430,
            .h = 240,
            .normal_x = 680,
            .normal_y = 160,
            .normal_w = 430,
            .normal_h = 240,
            .visible = false,
            .minimized = false,
            .maximized = false,
        },
        .{
            .kind = .app,
            .x = 520,
            .y = 250,
            .w = 430,
            .h = 240,
            .normal_x = 520,
            .normal_y = 250,
            .normal_w = 430,
            .normal_h = 240,
            .visible = false,
            .minimized = false,
            .maximized = false,
        },
    },

    pub fn run(self: *App) i32 {
        self.screen_w = fallbackDimension(self.ctx.screenWidth(), 1280);
        self.screen_h = fallbackDimension(self.ctx.screenHeight(), 720);
        self.initTiming();
        self.initWindowTitles();
        self.fitWindowsToWorkArea();
        self.setConsoleLaunch("Terminal", terminal_path, terminal_desktop_args);
        self.run_path.set(default_run_path);
        self.loadDesktopConfig();
        self.reloadWallpaper();
        self.loadTimeConfig();
        self.loadMenuConfig();
        self.repairDesktopFolder();
        self.loadAssociationConfig();
        self.loadDesktopItemsFolder();
        self.loadQuickLaunchRegistry();
        _ = self.updateKeyboardLayout();
        _ = self.updateClock();
        self.readInitialCursor();
        self.ctx.mouseHide();
        self.resetWindowServiceState();
        self.tray_registry = tray.Registry.init(self.ctx.self_handle.generation);
        self.tray_staging = tray.Registry.init(self.ctx.self_handle.generation);
        _ = self.syncVolume();
        _ = self.syncTrayBroker();
        _ = self.updateTrayLayout();
        self.invalidateFull();
        self.redraw();
        _ = self.ctx.bootReady();
        if (hasHeadlessSubsystemArg(self.ctx.argsRaw()) and !self.startHeadlessSubsystemAcceptance()) {
            if (!self.ctx.exists(subsystem_host_test_marker_path)) _ = self.headlessSubsystemFailure("unknown");
            self.forceCloseWindowsByLaunchPath(subsystem_host_test_path);
            self.forceCloseWindowsByLaunchPath(r4basic_host_path);
            self.forceCloseWindowsByLaunchPath(r4gb_host_path);
            _ = self.syncProgramWindows();
            self.enterTerminalModeWithArgs("");
            if (!self.terminal_mode or self.windows[0].instance_id == 0) self.ctx.systemPoweroff();
            self.headless_acceptance_terminal = true;
        }
        if (hasKlickifaxSmokeArg(self.ctx.argsRaw())) self.runKlickifaxSmokeAndPoweroff();
        if (hasR4XSmokeArg(self.ctx.argsRaw())) self.runR4XSmokeAndPoweroff();
        if (hasSmokeArg(self.ctx.argsRaw())) self.runSmokeAndPoweroff();

        while (true) {
            if (self.headless_acceptance_terminal) {
                self.ctx.sleepTicks(self.loop_sleep_ticks);
                continue;
            }
            var needs_redraw = false;
            if (self.syncTrayBroker()) needs_redraw = true;
            if (self.pollRemoteFrameDemand()) needs_redraw = true;
            var remote_events: u32 = 0;
            while (remote_events < remote_input_burst and self.pollRemoteInputEvent()) : (remote_events += 1) {
                if (self.dispatchEvent()) needs_redraw = true;
            }
            var physical_events: u32 = 0;
            while (physical_events < physical_input_burst and self.pollPhysicalKeyEvent()) : (physical_events += 1) {}
            if (self.pollKeyboardEvent() and self.dispatchEvent()) needs_redraw = true;
            if (self.pollMouseEvent() and self.dispatchEvent()) needs_redraw = true;
            if (self.pollTimerEvent() and self.dispatchEvent()) needs_redraw = true;
            if (needs_redraw) self.redraw();
            self.idleWait(needs_redraw or remote_events != 0 or physical_events != 0);
        }
    }

    // 0.56.28: Im aktiven Fall kurz schlafen (Responsiveness fuer
    // Folge-Events); im Leerlauf event-getrieben auf Desktop-Aktivitaet
    // warten (Input, RDP-Input, GUI-/Console-Revisionen wecken sofort;
    // Blink/Uhr/Restsyncs laufen im blink_half_ticks-Raster weiter).
    // Fallback auf den alten Sleep, wenn der Kernel das API nicht hat.
    fn idleWait(self: *App, active: bool) void {
        if (active) {
            // A freshly launched GUI task must get a turn immediately. A
            // timer wait here can miss the child's first revision wake while
            // the launch dispatch is still active; a cooperative boundary
            // hands off once and lets the next loop consume that revision.
            self.ctx.sleepTicks(0);
            return;
        }
        _ = self.retryWindowServiceIfDue();
        // WINSVC registry changes do not participate in Desktop activity.
        // A bounded sleep keeps the visual mirror current without busy-looping.
        if (self.tray_broker_revision != 0 or self.tray_next_sync_tick <= self.ctx.ticks()) {
            self.ctx.sleepTicks(self.loop_sleep_ticks);
            return;
        }
        if (self.activity_wait_supported) {
            const rc = self.ctx.desktopActivityWait(self.activity_seq, self.blink_half_ticks, &self.activity_seq);
            if (rc >= 0) {
                if (rc > 0) {
                    self.activity_wait_wakes +%= 1;
                } else {
                    self.activity_wait_timeouts +%= 1;
                }
                return;
            }
            self.activity_wait_supported = false;
        }
        self.ctx.sleepTicks(self.loop_sleep_ticks);
    }

    fn syncTrayBroker(self: *App) bool {
        const now = self.ctx.ticks();
        if (self.tray_sync_cursor == r4os.abi.tray_desktop_cursor_poll and now < self.tray_next_sync_tick) return false;

        var changed = false;
        var pages: u32 = 0;
        while (pages < tray_sync_page_burst) : (pages += 1) {
            var request: r4os.abi.TrayDesktopExchange = .{
                .desktop_owner = self.ctx.self_handle,
                .known_revision = if (self.tray_sync_cursor == r4os.abi.tray_desktop_cursor_poll)
                    self.tray_broker_revision
                else
                    self.tray_sync_revision,
                .cursor = self.tray_sync_cursor,
            };
            var response: r4os.abi.TrayDesktopExchange = .{};
            const rc = self.ctx.trayDesktopExchange(r4os.abi.tray_service_op_desktop_sync, &request, &response);
            if (rc != r4os.abi.service_api_result_ok or !self.validTrayDesktopResponse(&response) or response.result != r4os.abi.tray_result_ok) {
                return self.loseTrayBroker() or changed;
            }

            const restarting = (response.flags & r4os.abi.tray_desktop_flag_restart) != 0;
            if (restarting) {
                self.tray_staging = tray.Registry.init(self.ctx.self_handle.generation);
                self.tray_sync_revision = response.registry_revision;
            } else if (self.tray_sync_cursor != r4os.abi.tray_desktop_cursor_poll and response.registry_revision != self.tray_sync_revision) {
                return self.loseTrayBroker() or changed;
            }

            if ((response.flags & r4os.abi.tray_desktop_flag_item) != 0) {
                const mutation = self.tray_staging.upsert(&response.item);
                if (mutation.result != r4os.abi.tray_result_ok) return self.loseTrayBroker() or changed;
            }

            if ((response.flags & r4os.abi.tray_desktop_flag_complete) != 0) {
                self.tray_sync_cursor = r4os.abi.tray_desktop_cursor_poll;
                self.tray_next_sync_tick = now +| self.tray_sync_ticks;
                if (response.registered_count != self.tray_staging.registered_count and response.registry_revision != self.tray_broker_revision) {
                    return self.loseTrayBroker() or changed;
                }
                if (response.registry_revision != self.tray_broker_revision) {
                    if (self.publishTraySnapshot(response.registry_revision)) changed = true;
                } else if (self.tray_visibility_dirty) {
                    _ = self.publishTrayVisibility();
                }
                return changed;
            }
            if (response.next_cursor == r4os.abi.tray_desktop_cursor_poll) return self.loseTrayBroker() or changed;
            self.tray_sync_cursor = response.next_cursor;
        }
        return changed;
    }

    fn validTrayDesktopResponse(self: *const App, response: *const r4os.abi.TrayDesktopExchange) bool {
        const valid_flags = r4os.abi.tray_desktop_flag_item |
            r4os.abi.tray_desktop_flag_complete |
            r4os.abi.tray_desktop_flag_restart |
            r4os.abi.tray_desktop_flag_layout_visible;
        return response.magic == r4os.abi.tray_desktop_exchange_magic and
            response.version == r4os.abi.tray_desktop_exchange_version and
            response.size == @sizeOf(r4os.abi.TrayDesktopExchange) and
            tray.sameOwner(response.desktop_owner, self.ctx.self_handle) and
            response.desktop_epoch == self.ctx.self_handle.generation and
            response.registry_revision != 0 and
            response.capacity == tray.max_items and
            response.registered_count <= tray.max_items and
            (response.flags & ~valid_flags) == 0;
    }

    fn publishTraySnapshot(self: *App, revision: u64) bool {
        const old_tooltip = self.currentTrayTooltipRect();
        self.tray_registry = self.tray_staging;
        self.tray_broker_revision = revision;
        self.tray_hover_identity = .{};
        self.tray_pressed_identity = .{};
        self.tray_right_pressed_identity = .{};
        self.last_mouse_down_tray_identity = .{};
        self.tray_visibility_dirty = true;
        _ = self.updateTrayLayout();
        self.afterTrayRegistryChange(old_tooltip);
        return true;
    }

    fn loseTrayBroker(self: *App) bool {
        const changed = self.tray_registry.registered_count != 0;
        const old_tooltip = self.currentTrayTooltipRect();
        self.tray_registry = tray.Registry.init(self.ctx.self_handle.generation);
        self.tray_staging = tray.Registry.init(self.ctx.self_handle.generation);
        self.tray_broker_revision = 0;
        self.tray_sync_revision = 0;
        self.tray_sync_cursor = r4os.abi.tray_desktop_cursor_poll;
        self.tray_visibility_dirty = false;
        self.tray_next_sync_tick = self.ctx.ticks() +| self.tray_service_retry_ticks;
        self.tray_hover_identity = .{};
        self.tray_pressed_identity = .{};
        self.tray_right_pressed_identity = .{};
        self.last_mouse_down_tray_identity = .{};
        if (old_tooltip) |rect| self.damage.invalidate(rect);
        if (changed) self.invalidateTaskbar();
        _ = self.updateTrayLayout();
        return changed;
    }

    fn publishTrayVisibility(self: *App) bool {
        if (self.tray_broker_revision == 0) return false;
        var index: usize = 0;
        while (index < tray.max_items) : (index += 1) {
            const entry = self.tray_registry.entryAt(index) orelse continue;
            var request: r4os.abi.TrayDesktopExchange = .{
                .desktop_owner = self.ctx.self_handle,
                .known_revision = self.tray_broker_revision,
                .flags = if (entry.layout_visible) r4os.abi.tray_desktop_flag_layout_visible else 0,
            };
            request.event.owner = entry.owner;
            request.event.item_id = entry.item_id;
            request.event.item_revision = entry.revision;
            var response: r4os.abi.TrayDesktopExchange = .{};
            const rc = self.ctx.trayDesktopExchange(r4os.abi.tray_service_op_desktop_visibility, &request, &response);
            if (rc != r4os.abi.service_api_result_ok or !self.validTrayDesktopResponse(&response) or
                response.result != r4os.abi.tray_result_ok or response.registry_revision != self.tray_broker_revision)
            {
                self.tray_visibility_dirty = true;
                return false;
            }
        }
        self.tray_visibility_dirty = false;
        return true;
    }

    fn submitTrayActivation(self: *App, identity: tray.Identity, kind: u16, wheel_delta: i32, x: i32, y: i32, tick: u64) bool {
        const entry = self.tray_registry.findByIdentity(identity) orelse return false;
        if (!entry.enabled() or !entry.layout_visible or self.tray_broker_revision == 0) return false;
        var request: r4os.abi.TrayDesktopExchange = .{
            .desktop_owner = self.ctx.self_handle,
            .known_revision = self.tray_broker_revision,
        };
        request.event.owner = entry.owner;
        request.event.item_id = entry.item_id;
        request.event.item_revision = entry.revision;
        request.event.kind = kind;
        request.event.wheel_delta = wheel_delta;
        request.event.x = x;
        request.event.y = y;
        request.event.tick = tick;
        var response: r4os.abi.TrayDesktopExchange = .{};
        const rc = self.ctx.trayDesktopExchange(r4os.abi.tray_service_op_desktop_activate, &request, &response);
        return rc == r4os.abi.service_api_result_ok and self.validTrayDesktopResponse(&response) and
            response.result == r4os.abi.tray_result_ok and response.registry_revision == self.tray_broker_revision;
    }

    fn updateTrayLayout(self: *App) bool {
        const old_tooltip = self.currentTrayTooltipRect();
        const changed = self.tray_registry.layout(.{
            .screen_w = self.screen_w,
            .screen_h = self.screen_h,
            .taskbar_h = theme.taskbar_h,
            .item_y = 4,
            .system_left = draw.taskbarKeyboardLayoutRect(self.screen_w, self.screen_h, self.config.taskbar_clock, self.volume_ui.installed).x,
            .window_start = quick_launch.taskbarWindowStartX(self.quick_launch.count),
            .visible_windows = self.visibleWindowCount(),
        });
        if (!changed) return false;
        self.tray_visibility_dirty = true;
        _ = self.publishTrayVisibility();
        if (old_tooltip) |rect| self.damage.invalidate(rect);
        self.invalidateTaskbar();
        return true;
    }

    fn afterTrayRegistryChange(self: *App, old_tooltip: ?surface.Rect) void {
        if (old_tooltip) |rect| self.damage.invalidate(rect);
        self.invalidateTaskbar();
        self.updateHover(self.cursor_x, self.cursor_y);
        if (self.currentTrayTooltipRect()) |rect| self.damage.invalidate(rect);
    }

    fn currentTrayTooltipRect(self: *const App) ?surface.Rect {
        return self.tray_registry.tooltipRect(self.tray_hover_identity, self.screen_w, self.screen_h, theme.taskbar_h);
    }

    fn syncVolume(self: *App) bool {
        const now = self.ctx.ticks();
        var changed = false;
        if (self.volume_pending_flags != 0 and now >= self.volume_next_send_tick) {
            if (self.flushVolumeUpdate(false)) changed = true;
        }
        if (now < self.volume_next_poll_tick) return changed;
        self.volume_next_poll_tick = now +| self.volume_poll_ticks;

        const old = self.volume_ui;
        const installed = self.ctx.exists(audio_service_module_path);
        self.volume_ui.installed = installed;
        if (!installed) {
            self.volume_ui.reachable = false;
            self.volume_ui.popup_open = false;
            self.volume_dragging = false;
            self.volume_pending_flags = 0;
        } else {
            var state: r4os.abi.AudioServiceMasterState = .{};
            const result = self.ctx.audioMasterState(&state);
            if (result == r4os.abi.service_api_result_ok) {
                self.acceptVolumeState(state);
            } else {
                self.volume_ui.reachable = false;
            }
        }
        if (!volumeViewEqual(old, self.volume_ui)) {
            if (old.installed != self.volume_ui.installed) _ = self.updateTrayLayout();
            self.invalidateVolumeFeedback();
            changed = true;
        }
        return changed;
    }

    fn acceptVolumeState(self: *App, state: r4os.abi.AudioServiceMasterState) void {
        self.volume_master = state;
        self.volume_ui.installed = true;
        self.volume_ui.reachable = true;
        self.volume_ui.muted = (state.flags & r4os.abi.audio_master_state_flag_muted) != 0;
        self.volume_ui.percent = volume.percentForGain(state.selected_volume_fixed);
    }

    fn setVolumePercent(self: *App, percent: u8, immediate: bool) bool {
        if (!self.volume_ui.installed or !self.volume_ui.reachable) return false;
        const next = @min(percent, 100);
        const old = self.volume_ui;
        self.volume_ui.percent = next;
        if (next != 0) self.volume_ui.muted = false;
        if (self.volume_pending_flags == 0) self.volume_next_send_tick = self.ctx.ticks();
        self.volume_pending_flags |= r4os.abi.audio_master_request_flag_set_volume;
        self.volume_pending_percent = next;
        if (!volumeViewEqual(old, self.volume_ui)) self.invalidateVolumeFeedback();
        if (immediate or self.ctx.ticks() >= self.volume_next_send_tick) _ = self.flushVolumeUpdate(immediate);
        return true;
    }

    fn toggleVolumeMute(self: *App) bool {
        if (!self.volume_ui.installed or !self.volume_ui.reachable) return false;
        const old = self.volume_ui;
        self.volume_ui.muted = !self.volume_ui.muted;
        if (self.volume_pending_flags == 0) self.volume_next_send_tick = self.ctx.ticks();
        self.volume_pending_flags |= r4os.abi.audio_master_request_flag_set_muted;
        self.volume_pending_muted = self.volume_ui.muted;
        if (!volumeViewEqual(old, self.volume_ui)) self.invalidateVolumeFeedback();
        _ = self.flushVolumeUpdate(true);
        return true;
    }

    fn adjustVolume(self: *App, direction: i32) bool {
        return self.setVolumePercent(volume.stepped(self.volume_ui.percent, direction), true);
    }

    fn flushVolumeUpdate(self: *App, force: bool) bool {
        if (self.volume_pending_flags == 0 or !self.volume_ui.installed) return false;
        const now = self.ctx.ticks();
        if (!force and now < self.volume_next_send_tick) return false;
        const old = self.volume_ui;
        var request: r4os.abi.AudioServiceMasterRequest = .{
            .flags = self.volume_pending_flags,
            .fixed_volume = volume.gainForPercent(self.volume_pending_percent),
        };
        if ((request.flags & r4os.abi.audio_master_request_flag_set_muted) != 0 and self.volume_pending_muted) {
            request.flags |= r4os.abi.audio_master_request_flag_muted;
        }
        self.volume_pending_flags = 0;
        self.volume_next_send_tick = now +| self.volume_send_ticks;
        var state: r4os.abi.AudioServiceMasterState = .{};
        if (self.ctx.audioSetMasterState(&request, &state) == r4os.abi.service_api_result_ok) {
            self.acceptVolumeState(state);
            self.volume_next_poll_tick = now +| self.volume_poll_ticks;
        } else {
            self.volume_ui.reachable = false;
            self.volume_next_poll_tick = now +| self.volume_poll_ticks;
        }
        if (!volumeViewEqual(old, self.volume_ui)) self.invalidateVolumeFeedback();
        return !volumeViewEqual(old, self.volume_ui);
    }

    fn updateVolumeFromPointer(self: *App, x: i32, force: bool) bool {
        const track = draw.volumeTrackRect(self.screen_w, self.screen_h, self.config.taskbar_clock);
        return self.setVolumePercent(volume.percentAtX(track, x), force);
    }

    fn invalidateVolumeFeedback(self: *App) void {
        self.invalidateTaskbar();
        if (self.volume_ui.popup_open) self.invalidateVolumePopup();
        if (self.currentVolumeTooltipRect()) |rect| self.damage.invalidate(rect);
    }

    fn currentVolumeTooltipRect(self: *const App) ?surface.Rect {
        if (self.hover_target != .taskbar_volume or self.volume_ui.popup_open or !self.volume_ui.installed) return null;
        return draw.volumeTooltipRect(self.screen_w, self.screen_h, self.config.taskbar_clock);
    }

    fn pollRemoteFrameDemand(self: *App) bool {
        if (!self.ctx.supportsRemoteFrameDemand()) return false;
        const consumers = self.ctx.remoteFrameConsumers();
        if (consumers == self.remote_frame_consumers) return false;
        const became_active = self.remote_frame_consumers == 0 and consumers != 0;
        self.remote_frame_consumers = consumers;
        if (became_active) self.invalidateFull();
        return became_active;
    }

    fn runR4XSmokeAndPoweroff(self: *App) noreturn {
        var ok = true;
        self.ctx.println("R4DESK R4X smoke");
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\EXAMPLE.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\PAINT.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\CALC.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\CLOCK.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\TIMESET.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\EXPLORER.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\FONTS.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\APPDEF.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\CONFIG\\ASSOC.R4S") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\MEMVIEW.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\DEVMGR.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\SERVICES.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\LOGCENTER.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\NETCFG.R4X") and ok;
        ok = self.smokeDesktopFolderDefaults() and ok;
        ok = self.smokeDesktopFolderItems() and ok;
        ok = self.smokeDesktopLayoutDrag() and ok;
        ok = self.smokeInitialDesktopState() and ok;
        var render_snap = RenderSnapshot{};
        render_snap = self.printSmokeRenderStats("initial", render_snap);
        ok = self.smokeRemoteFrameContract() and ok;
        ok = self.smokeLaunchMultipleTerminalWindows() and ok;
        render_snap = self.printSmokeRenderStats("terminal-multi-window", render_snap);

        ok = self.smokeLaunchDefaultAppsFromMenu() and ok;
        render_snap = self.printSmokeRenderStats("appdef-menu-launch", render_snap);
        if (self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\APPDEF.R4X")) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\EXPLORER.R4X", "", "Explorer", .gui);
        self.smokePumpFrames(4);
        if (self.hasDamage()) self.redraw();
        const explorer_window = self.findWindowByLaunchPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\EXPLORER.R4X");
        self.ctx.print("Explorer window: ");
        if (explorer_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
            ok = self.smokeWindowServiceMirror("Explorer", index) and ok;
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and explorer_window != null;
        render_snap = self.printSmokeRenderStats("explorer-open", render_snap);
        if (explorer_window) |index| {
            self.closeWindow(index);
            self.smokePumpFrames(2);
        }

        ok = self.smokeLaunchComputerDesktopItem() and ok;
        render_snap = self.printSmokeRenderStats("computer-shortcut", render_snap);
        self.closeExplorerWindows();
        self.smokePumpFrames(2);

        ok = self.smokeQuickLaunch() and ok;
        render_snap = self.printSmokeRenderStats("quick-launch", render_snap);
        self.closeExplorerWindows();
        self.smokePumpFrames(2);

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\APPDEF.R4X", "", "Default Apps", .gui);
        self.smokePumpFrames(6);
        if (self.hasDamage()) self.redraw();
        const appdef_window = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\APPDEF.R4X");
        self.ctx.print("Default Apps window: ");
        if (appdef_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
            self.smokePushGuiKey(index, 'C');
            self.smokePushGuiKey(index, r4os.gui.Key.down);
            self.smokePushGuiKey(index, r4os.gui.Key.enter);
            self.smokePushGuiKey(index, r4os.gui.Key.escape);
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and appdef_window != null;
        _ = self.printSmokeRenderStats("appdef-open", render_snap);
        if (appdef_window) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X", "", "Notepad", .gui);
        self.smokePumpFrames(6);
        if (self.hasDamage()) self.redraw();
        const notepad_window = self.findWindowByLaunchPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X");
        self.ctx.print("Notepad window: ");
        if (notepad_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and notepad_window != null;
        _ = self.printSmokeRenderStats("notepad-open", render_snap);
        if (notepad_window) |index| {
            const input_stress = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz1234567890";
            var input_index: usize = 0;
            while (input_index < input_stress.len) : (input_index += 1) self.smokePushGuiKey(index, input_stress[input_index]);
            self.ctx.print("Desktop Computer after Notepad input: ");
            const explorer_ok = self.smokeLaunchComputerDesktopItem();
            ok = explorer_ok and ok;
            render_snap = self.printSmokeRenderStats("computer-after-notepad-input", render_snap);
            self.closeExplorerWindows();
            self.smokePumpFrames(2);
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\PAINT.R4X", "", "Paint", .gui);
        self.smokePumpFrames(6);
        if (self.hasDamage()) self.redraw();
        const paint_window = self.findWindowByLaunchPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\PAINT.R4X");
        self.ctx.print("Paint window: ");
        if (paint_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and paint_window != null;
        _ = self.printSmokeRenderStats("paint-open", render_snap);
        if (paint_window) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\MEMVIEW.R4X", "", "MemView", .gui);
        self.smokePumpFrames(6);
        if (self.hasDamage()) self.redraw();
        const memview_window = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\MEMVIEW.R4X");
        self.ctx.print("MemView window: ");
        if (memview_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and memview_window != null;
        _ = self.printSmokeRenderStats("memview-open", render_snap);
        if (memview_window) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\DEVMGR.R4X", "", "Device Manager", .gui);
        self.smokePumpFrames(6);
        if (self.hasDamage()) self.redraw();
        const devmgr_window = self.findWindowByLaunchPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\DEVMGR.R4X");
        self.ctx.print("Device Manager window: ");
        if (devmgr_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and devmgr_window != null;
        _ = self.printSmokeRenderStats("devmgr-open", render_snap);
        if (devmgr_window) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\SERVICES.R4X", "", "Services", .gui);
        self.smokePumpFrames(6);
        if (self.hasDamage()) self.redraw();
        const services_window = self.findWindowByLaunchPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\SERVICES.R4X");
        self.ctx.print("Services window: ");
        if (services_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and services_window != null;
        _ = self.printSmokeRenderStats("services-open", render_snap);
        if (services_window) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\LOGCENTER.R4X", "", "Log Center", .gui);
        self.smokePumpFrames(6);
        if (self.hasDamage()) self.redraw();
        const logcenter_window = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\LOGCENTER.R4X");
        self.ctx.print("Log Center window: ");
        if (logcenter_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
            self.smokePushGuiKey(index, 'P');
            self.smokePushGuiKey(index, 'E');
            self.smokePushGuiKey(index, r4os.gui.Key.page_down);
            self.smokePushGuiKey(index, r4os.gui.Key.page_up);
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and logcenter_window != null;
        _ = self.printSmokeRenderStats("logcenter-open", render_snap);
        if (logcenter_window) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\NETCFG.R4X", "", "Network", .gui);
        self.smokePumpFrames(6);
        if (self.hasDamage()) self.redraw();
        const netcfg_window = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\NETCFG.R4X");
        self.ctx.print("NetConfig window: ");
        if (netcfg_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and netcfg_window != null;
        _ = self.printSmokeRenderStats("netcfg-open", render_snap);
        if (netcfg_window) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\EXAMPLE.R4X", "", "SDK GUI Example", .gui);
        self.smokePumpFrames(6);
        if (self.hasDamage()) self.redraw();
        const example_window = self.findWindowByLaunchPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\EXAMPLE.R4X");
        self.ctx.print("Example window: ");
        if (example_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
            self.smokePushGuiKey(index, 'A');
            self.smokePushGuiKey(index, '1');
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and example_window != null;
        _ = self.printSmokeRenderStats("example-open", render_snap);
        if (example_window) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\CALC.R4X", "", "Calculator", .gui);
        self.smokePumpFrames(6);
        if (self.hasDamage()) self.redraw();
        const calc_window = self.findWindowByLaunchPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\CALC.R4X");
        self.ctx.print("Calculator window: ");
        if (calc_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
            self.smokePushGuiKey(index, '1');
            self.smokePushGuiKey(index, '+');
            self.smokePushGuiKey(index, '2');
            self.smokePushGuiKey(index, r4os.gui.Key.enter);
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and calc_window != null;
        _ = self.printSmokeRenderStats("calc-open", render_snap);
        if (calc_window) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\CLOCK.R4X", "", "Clock", .gui);
        self.smokePumpFrames(6);
        if (self.hasDamage()) self.redraw();
        const clock_window = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\CLOCK.R4X");
        self.ctx.print("Clock window: ");
        if (clock_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and clock_window != null;
        _ = self.printSmokeRenderStats("clock-open", render_snap);
        if (clock_window) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\TIMESET.R4X", "", "Time Settings", .gui);
        self.smokePumpFrames(6);
        if (self.hasDamage()) self.redraw();
        const timeset_window = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\TIMESET.R4X");
        self.ctx.print("Time Settings window: ");
        if (timeset_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and timeset_window != null;
        _ = self.printSmokeRenderStats("timeset-open", render_snap);
        if (timeset_window) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(2);
        }

        ok = self.smokeDesktopResponsiveness(false) and ok;
        self.ctx.print("R4DESK R4X smoke result: ");
        self.ctx.println(if (ok) "OK" else "FAILED");
        self.flushDesktopLayoutBeforeSystemAction();
        self.ctx.systemPoweroff();
    }

    fn runKlickifaxSmokeAndPoweroff(self: *App) noreturn {
        var ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\INTERNET\\KLICKIFAX.R4X");
        self.ctx.println("Klickifax Desktop smoke");
        // A real Desktop launch can only arrive through the event loop. Give
        // the focused smoke the same settled frame boundary before spawning.
        self.smokePumpFrames(4);
        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\INTERNET\\KLICKIFAX.R4X", "", "Klickifax", .gui);
        var browser_started = false;
        if (self.findWindowByLaunchPath("C:\\R4OS\\SOFTWARE\\INTERNET\\KLICKIFAX.R4X")) |index| {
            const initial_revision = self.windows[index].gui_revision;
            browser_started = self.smokeWaitForGuiRevision(self.windows[index].instance_id, initial_revision, 200);
        }
        self.ctx.print("Klickifax first frame: ");
        self.ctx.println(if (browser_started) "OK" else "FAILED");
        ok = browser_started and ok;
        self.smokePumpCooperativeFrames(4);
        if (self.hasDamage()) self.redraw();
        const gui_window = self.findWindowByLaunchPath("C:\\R4OS\\SOFTWARE\\INTERNET\\KLICKIFAX.R4X");
        self.ctx.print("Klickifax desktop launch: ");
        if (gui_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
            self.pushGuiKeyEvent(index, 'A');
            self.smokePumpCooperativeFrames(4);
        } else {
            self.ctx.println("FAILED");
        }
        const alive = self.findWindowByLaunchPath("C:\\R4OS\\SOFTWARE\\INTERNET\\KLICKIFAX.R4X");
        ok = ok and gui_window != null and alive != null;
        self.ctx.print("Klickifax Desktop smoke result: ");
        self.ctx.println(if (ok) "OK" else "FAILED");
        self.ctx.systemPoweroff();
    }

    fn smokeWaitForGuiRevision(self: *App, instance_id: u32, initial_revision: u32, attempts: u32) bool {
        var attempt: u32 = 0;
        while (attempt < attempts) : (attempt += 1) {
            self.ctx.sleepTicks(0);
            if (self.ctx.guiRevision(instance_id) != initial_revision) return true;
            if (self.pollTimerEvent() and self.dispatchEvent()) self.redraw();
        }
        return false;
    }

    fn runSmokeAndPoweroff(self: *App) noreturn {
        var ok = true;
        self.ctx.println("DESKTOP smoke");
        ok = smokeDrive(self.ctx, 'C', 2) and ok;
        ok = smokeDrive(self.ctx, 'D', 2) and ok;
        ok = smokeExists(self.ctx, menu_config_path) and ok;
        ok = smokeExists(self.ctx, desktop_config_path) and ok;
        ok = smokeExists(self.ctx, time_config_path) and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\PAINT.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\CALC.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\CLOCK.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\TIMESET.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\EXPLORER.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\APPDEF.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\CONFIG\\ASSOC.R4S") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\SERVICES.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\LOGCENTER.R4X") and ok;
        ok = smokeExists(self.ctx, "C:\\R4OS\\SOFTWARE\\DESKTOP\\NETCFG.R4X") and ok;
        ok = self.smokeDesktopFolderDefaults() and ok;
        ok = self.smokeDesktopFolderItems() and ok;
        ok = self.smokeDesktopLayoutDrag() and ok;
        ok = self.smokeInitialDesktopState() and ok;
        var render_snap = RenderSnapshot{};
        render_snap = self.printSmokeRenderStats("initial", render_snap);
        ok = self.smokeRemoteFrameContract() and ok;
        ok = self.smokeLaunchMultipleTerminalWindows() and ok;
        render_snap = self.printSmokeRenderStats("terminal-multi-window", render_snap);
        ok = self.smokeKeyboardLayoutSwitch() and ok;
        if (self.hasDamage()) self.redraw();
        render_snap = self.printSmokeRenderStats("keyboard-layout-switch", render_snap);
        ok = self.smokeStartMenuKey() and ok;
        if (self.hasDamage()) self.redraw();
        render_snap = self.printSmokeRenderStats("start-menu-key", render_snap);
        ok = self.smokeClipboardContract() and ok;
        if (self.hasDamage()) self.redraw();
        render_snap = self.printSmokeRenderStats("clipboard", render_snap);
        self.openDialog(.run);
        if (self.hasDamage()) self.redraw();
        render_snap = self.printSmokeRenderStats("run-open", render_snap);
        self.closeTop();
        if (self.hasDamage()) self.redraw();
        render_snap = self.printSmokeRenderStats("run-close", render_snap);
        ok = self.smokeLaunchNetworkSettingsFromMenu() and ok;
        render_snap = self.printSmokeRenderStats("netcfg-menu-launch", render_snap);
        if (self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\NETCFG.R4X")) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(4);
            _ = self.syncProgramWindows();
        }
        ok = self.smokeLaunchDefaultAppsFromMenu() and ok;
        render_snap = self.printSmokeRenderStats("appdef-menu-launch", render_snap);
        if (self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\APPDEF.R4X")) |index| {
            _ = self.requestWindowProcessClose(index);
            self.smokePumpFrames(4);
            _ = self.syncProgramWindows();
        }
        ok = self.smokeTimeMenu() and ok;
        render_snap = self.printSmokeRenderStats("time-menu", render_snap);
        ok = self.smokeLaunchTimeSettingsFromMenu() and ok;
        render_snap = self.printSmokeRenderStats("time-settings-menu-launch", render_snap);
        if (self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\CLOCK.R4X")) |index| _ = self.requestWindowProcessClose(index);
        if (self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\TIMESET.R4X")) |index| _ = self.requestWindowProcessClose(index);
        self.smokePumpFrames(4);
        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\EXPLORER.R4X", "", "Explorer", .gui);
        self.smokePumpFrames(4);
        if (self.hasDamage()) self.redraw();
        const explorer_window = self.findWindowByLaunchPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\EXPLORER.R4X");
        self.ctx.print("Explorer window: ");
        if (explorer_window) |index| {
            self.ctx.printI32(@intCast(index));
            self.ctx.println(" ok");
        } else {
            self.ctx.println("FAILED");
        }
        ok = ok and explorer_window != null;
        render_snap = self.printSmokeRenderStats("explorer-open", render_snap);
        _ = self.updateCursor(self.cursor_x + 2, self.cursor_y + 2);
        if (self.hasDamage()) self.redraw();
        render_snap = self.printSmokeRenderStats("cursor-step", render_snap);
        _ = self.updateCursor(8, 1);
        if (self.hasDamage()) self.redraw();
        render_snap = self.printSmokeRenderStats("cursor-top-enter", render_snap);
        _ = self.updateCursor(8, 0);
        if (self.hasDamage()) self.redraw();
        render_snap = self.printSmokeRenderStats("cursor-top-edge", render_snap);
        const cursor_top_ok = self.render_stats.last_damage_kind == .cursor and
            self.render_stats.last_damage_rect.y == 0 and
            self.render_stats.last_damage_rect.h >= surface.cursor_h;
        ok = cursor_top_ok and ok;
        if (explorer_window) |index| {
            const win = &self.windows[index];
            self.beginDrag(index, win.x + 32, win.y + 10);
            _ = self.updateDrag(win.x + 64, win.y + 34);
            self.drag.active = false;
            if (self.hasDamage()) self.redraw();
            render_snap = self.printSmokeRenderStats("explorer-move", render_snap);
            self.smokePushGuiKey(index, 0x81);
            self.smokePushGuiKey(index, 0x81);
            self.smokePushGuiKey(index, 0x81);
            self.smokePushGuiKey(index, '\r');
            render_snap = self.printSmokeRenderStats("explorer-bin-open", render_snap);
            var scroll_step: u32 = 0;
            while (scroll_step < 12) : (scroll_step += 1) self.smokePushGuiKey(index, 0x81);
            _ = self.printSmokeRenderStats("explorer-scroll", render_snap);
        }
        self.ctx.sleepTicks(self.loop_sleep_ticks * 3);
        if (explorer_window) |index| self.closeWindow(index);
        if (self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\NETCFG.R4X")) |index| _ = self.requestWindowProcessClose(index);
        if (self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\CLOCK.R4X")) |index| _ = self.requestWindowProcessClose(index);
        if (self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\TIMESET.R4X")) |index| _ = self.requestWindowProcessClose(index);
        ok = self.smokeDesktopResponsiveness(true) and ok;
        self.ctx.print("R4DESK smoke result: ");
        self.ctx.println(if (ok) "OK" else "FAILED");
        self.flushDesktopLayoutBeforeSystemAction();
        self.ctx.systemPoweroff();
    }

    fn smokeLaunchNetworkSettingsFromMenu(self: *App) bool {
        self.ctx.print("Settings/Network menu launch: ");
        const settings_hit = self.menuIndexForTarget(.menu_settings) orelse {
            self.ctx.println("FAILED-settings");
            return false;
        };
        const network_hit = self.submenuIndexForTarget(.menu_settings_network) orelse {
            self.ctx.println("FAILED-network");
            return false;
        };
        if (network_hit.parent != settings_hit) {
            self.ctx.println("FAILED-parent");
            return false;
        }

        self.toggleStart();
        self.menu_selected = settings_hit;
        self.menuRight();
        self.menu_submenu_selected = network_hit.index;
        const command = self.defaultCommand();
        if (command != .menu_settings_network) {
            self.ctx.println("FAILED-command");
            self.closeStartMenu();
            return false;
        }
        self.dispatchCommand(command, .keyboard);
        self.smokePumpFrames(4);
        if (self.hasDamage()) self.redraw();
        const first = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\NETCFG.R4X") orelse {
            self.ctx.println("FAILED-launch");
            return false;
        };

        self.toggleStart();
        self.menu_selected = settings_hit;
        self.menuRight();
        self.menu_submenu_selected = network_hit.index;
        self.dispatchCommand(self.defaultCommand(), .keyboard);
        self.smokePumpFrames(2);
        if (self.hasDamage()) self.redraw();
        const second = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\NETCFG.R4X") orelse {
            self.ctx.println("FAILED-refocus");
            return false;
        };
        if (first != second) {
            self.ctx.println("FAILED-duplicate");
            return false;
        }
        self.ctx.println("ok");
        return true;
    }

    fn smokeLaunchDefaultAppsFromMenu(self: *App) bool {
        self.ctx.print("Settings/Default Apps menu launch: ");
        const settings_hit = self.menuIndexForTarget(.menu_settings) orelse {
            self.ctx.println("FAILED-settings");
            return false;
        };
        const appdef_hit = self.submenuIndexForTarget(.menu_settings_default_apps) orelse {
            self.ctx.println("FAILED-appdef");
            return false;
        };
        if (appdef_hit.parent != settings_hit) {
            self.ctx.println("FAILED-parent");
            return false;
        }

        self.toggleStart();
        self.menu_selected = settings_hit;
        self.menuRight();
        self.menu_submenu_selected = appdef_hit.index;
        const command = self.defaultCommand();
        if (command != .menu_settings_default_apps) {
            self.ctx.println("FAILED-command");
            self.closeStartMenu();
            return false;
        }
        self.dispatchCommand(command, .keyboard);
        self.smokePumpFrames(4);
        if (self.hasDamage()) self.redraw();
        const first = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\APPDEF.R4X") orelse {
            self.ctx.println("FAILED-launch");
            return false;
        };

        self.toggleStart();
        self.menu_selected = settings_hit;
        self.menuRight();
        self.menu_submenu_selected = appdef_hit.index;
        self.dispatchCommand(self.defaultCommand(), .keyboard);
        self.smokePumpFrames(2);
        if (self.hasDamage()) self.redraw();
        const second = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\APPDEF.R4X") orelse {
            self.ctx.println("FAILED-refocus");
            return false;
        };
        if (first != second) {
            self.ctx.println("FAILED-duplicate");
            return false;
        }
        self.ctx.println("ok");
        return true;
    }

    fn smokeLaunchComputerDesktopItem(self: *App) bool {
        self.ctx.print("Desktop Computer item launch: ");
        var item_index: ?usize = null;
        var i: usize = 0;
        while (i < self.desktop_items.count) : (i += 1) {
            const entry = &self.desktop_items.entries[i];
            if (entry.kind == .shortcut and equalsIgnoreCase(entry.titleText(), "Computer") and isExplorerPath(entry.launchPathText())) {
                item_index = i;
                break;
            }
        }
        const index = item_index orelse {
            self.ctx.println("FAILED-missing");
            return false;
        };

        const before = self.countExplorerWindows();
        self.activateDesktopItem(index);
        self.smokePumpFrames(4);
        if (self.hasDamage()) self.redraw();
        const after_first = self.countExplorerWindows();
        const first = self.active_window;
        if (first >= self.windows.len or !self.windowLaunchIsExplorer(first)) {
            self.ctx.println("FAILED-launch");
            return false;
        }
        if (after_first != before + 1) {
            self.ctx.println("FAILED-first-count");
            return false;
        }

        self.activateDesktopItem(index);
        self.smokePumpFrames(2);
        if (self.hasDamage()) self.redraw();
        const after_second = self.countExplorerWindows();
        if (after_second != before + 2) {
            self.ctx.println("FAILED-second-count");
            return false;
        }
        if (self.active_window == first or self.active_window >= self.windows.len or !self.windowLaunchIsExplorer(self.active_window)) {
            self.ctx.println("FAILED-new-focus");
            return false;
        }
        self.ctx.println("ok");
        return true;
    }

    fn smokeQuickLaunch(self: *App) bool {
        self.ctx.print("Quick Launch defaults: ");
        if (self.quick_launch.count < 2) {
            self.ctx.println("FAILED-count");
            return false;
        }
        if (self.quick_launch.items[0].kind != .show_desktop or self.quick_launch.items[1].kind != .program) {
            self.ctx.println("FAILED-kind");
            return false;
        }
        if (!isExplorerPath(spanZ(self.quick_launch.items[1].path[0..]))) {
            self.ctx.println("FAILED-path");
            return false;
        }

        const before = self.countExplorerWindows();
        self.activateQuickLaunch(1);
        self.smokePumpFrames(4);
        if (self.hasDamage()) self.redraw();
        const after = self.countExplorerWindows();
        if (after != before + 1) {
            self.ctx.println("FAILED-launch");
            return false;
        }
        const explorer_window = self.findExplorerWindow() orelse {
            self.ctx.println("FAILED-window");
            return false;
        };

        self.activateQuickLaunch(0);
        self.smokePumpFrames(2);
        if (self.hasDamage()) self.redraw();
        if (!self.windows[explorer_window].minimized) {
            self.ctx.println("FAILED-show-desktop");
            return false;
        }
        self.ctx.println("ok");
        return true;
    }

    fn smokeLaunchMultipleTerminalWindows(self: *App) bool {
        self.ctx.print("Terminal multi-window launch: ");
        const perf_start = self.ctx.performanceInput() orelse {
            self.ctx.println("FAILED-metrics");
            return false;
        };
        const before = self.countWindowsByLaunchPath(terminal_path);
        self.launchConsolePath(terminal_path, "", "Terminal");
        self.smokePumpFrames(4);
        _ = self.syncProgramWindows();
        if (self.hasDamage()) self.redraw();
        const after_first = self.countWindowsByLaunchPath(terminal_path);
        const first = self.active_window;
        if (after_first != before + 1 or first >= self.windows.len or self.windows[first].kind != .terminal) {
            self.ctx.println("FAILED-first");
            self.forceCloseWindowsByLaunchPath(terminal_path);
            return false;
        }

        self.launchConsolePath(terminal_path, "", "Terminal");
        self.smokePumpFrames(4);
        _ = self.syncProgramWindows();
        if (self.hasDamage()) self.redraw();
        const after_second = self.countWindowsByLaunchPath(terminal_path);
        const second = self.active_window;
        const windows_ok = after_second == before + 2 and
            second < self.windows.len and
            second != first and
            self.windows[second].kind == .terminal and
            !self.terminal_mode;
        if (!windows_ok) {
            self.ctx.println("FAILED-second");
            self.forceCloseWindowsByLaunchPath(terminal_path);
            return false;
        }

        const first_id = self.windows[first].instance_id;
        const second_id = self.windows[second].instance_id;
        const first_handle = self.window_process_handles[first];
        const second_handle = self.window_process_handles[second];
        var settled_perf: ?r4os.abi.ProgramInputPerformanceInfo = null;
        var settle_attempt: u32 = 0;
        while (settle_attempt < 40) : (settle_attempt += 1) {
            self.smokePumpFrames(1);
            const current = self.ctx.performanceInput() orelse continue;
            if (current.console_wait_blocks - perf_start.console_wait_blocks >= 2) {
                settled_perf = current;
                break;
            }
        }
        const idle_before = settled_perf orelse {
            self.ctx.println("FAILED-input-wait");
            self.forceCloseWindowsByLaunchPath(terminal_path);
            return false;
        };
        const runtime_before = self.smokeTaskRuntimeTicks(first_id, second_id) orelse {
            self.ctx.println("FAILED-runtime-before");
            self.forceCloseWindowsByLaunchPath(terminal_path);
            return false;
        };
        self.smokePumpFrames(12);
        const runtime_after = self.smokeTaskRuntimeTicks(first_id, second_id) orelse {
            self.ctx.println("FAILED-runtime-after");
            self.forceCloseWindowsByLaunchPath(terminal_path);
            return false;
        };
        const idle_after = self.ctx.performanceInput() orelse {
            self.ctx.println("FAILED-idle-metrics");
            self.forceCloseWindowsByLaunchPath(terminal_path);
            return false;
        };
        if (runtime_after != runtime_before or
            idle_after.console_read_calls != idle_before.console_read_calls or
            idle_after.console_wait_calls != idle_before.console_wait_calls or
            idle_after.console_wait_blocks != idle_before.console_wait_blocks or
            idle_after.console_output_revision_batches != idle_before.console_output_revision_batches or
            idle_after.console_output_desktop_signals != idle_before.console_output_desktop_signals)
        {
            self.ctx.println("FAILED-idle-work");
            self.forceCloseWindowsByLaunchPath(terminal_path);
            return false;
        }

        if (!self.pushConsoleInputBounded(first_id, "PAUSE\n")) {
            self.ctx.println("FAILED-pause-input");
            self.forceCloseWindowsByLaunchPath(terminal_path);
            return false;
        }
        var pause_ready = false;
        var pause_attempt: u32 = 0;
        while (pause_attempt < 40) : (pause_attempt += 1) {
            self.smokePumpFrames(1);
            var output: [4096]u8 = undefined;
            const got = self.ctx.consoleOutput(first_id, output[0..]);
            const current = self.ctx.performanceInput();
            if (got > 0 and containsBytes(output[0..@intCast(got)], "Press any key") and
                current != null and current.?.console_wait_blocks > idle_after.console_wait_blocks)
            {
                pause_ready = true;
                break;
            }
        }
        if (!pause_ready or !self.pushConsoleInputBounded(first_id, "XEXIT\n")) {
            self.ctx.println("FAILED-pause");
            self.forceCloseWindowsByLaunchPath(terminal_path);
            return false;
        }
        const close_window = &self.windows[second];
        const close_x = close_window.x + close_window.w - 4 - (theme.button + 2) + @divTrunc(theme.button, 2);
        const close_y = close_window.y + 2 + @divTrunc(theme.button, 2);
        const close_target = self.targetAt(close_x, close_y);
        if (close_target != self.closeTargetForIndex(second)) {
            self.ctx.println("FAILED-close-target");
            self.forceCloseWindowsByLaunchPath(terminal_path);
            return false;
        }
        self.dispatchMouseCommand(close_target);
        self.dispatchMouseCommand(close_target);
        if (!self.windows[second].close_requested or
            !sameProcessHandle(self.window_process_handles[second], second_handle) or
            self.windows[first].close_requested)
        {
            self.ctx.println("FAILED-close-request");
            self.forceCloseWindowsByLaunchPath(terminal_path);
            return false;
        }

        var closed = false;
        var close_attempt: u32 = 0;
        while (close_attempt < 80) : (close_attempt += 1) {
            self.smokePumpFrames(1);
            _ = self.syncProgramWindows();
            if (self.windows[first].instance_id == 0 and self.windows[second].instance_id == 0) {
                closed = true;
                break;
            }
        }
        if (!closed or
            !sameProcessHandle(self.window_completion_handles[first], first_handle) or
            !sameProcessHandle(self.window_completion_handles[second], second_handle) or
            self.window_completion_exit_codes[first] != 0 or
            self.window_completion_exit_codes[second] != 0)
        {
            self.ctx.println("FAILED-close");
            self.forceCloseWindowsByLaunchPath(terminal_path);
            return false;
        }

        const active_after = self.ctx.performanceInput() orelse {
            self.ctx.println("FAILED-active-metrics");
            return false;
        };
        const active_write_calls = active_after.console_output_write_calls - idle_after.console_output_write_calls;
        const active_revisions = active_after.console_output_revision_batches - idle_after.console_output_revision_batches;
        const active_signals = active_after.console_output_desktop_signals - idle_after.console_output_desktop_signals;
        if (active_after.console_read_bytes - idle_after.console_read_bytes < 12 or
            active_after.console_wait_wakes - idle_after.console_wait_wakes < 3 or
            active_write_calls == 0 or active_revisions > active_write_calls * 2 or active_signals > active_write_calls)
        {
            self.ctx.println("FAILED-active-work");
            return false;
        }
        if (self.hasDamage()) self.redraw();
        self.ctx.print("ok idle-cpu=");
        self.ctx.printU64(runtime_after - runtime_before);
        self.ctx.print(" waits=");
        self.ctx.printU64(active_after.console_wait_blocks - perf_start.console_wait_blocks);
        self.ctx.print(" wakes=");
        self.ctx.printU64(active_after.console_wait_wakes - perf_start.console_wait_wakes);
        self.ctx.print(" revisions=");
        self.ctx.printU64(active_revisions);
        self.ctx.print(" signals=");
        self.ctx.printU64(active_signals);
        self.ctx.println("");
        return true;
    }

    fn smokeTaskRuntimeTicks(self: *const App, first_id: u32, second_id: u32) ?u64 {
        var attempt: u32 = 0;
        retry: while (attempt < task_inventory_restart_limit) : (attempt += 1) {
            var cursor: r4os.abi.ProgramInventoryCursor = .{};
            var summary: r4os.abi.ProgramInventorySummary = .{};
            if (self.ctx.programInventoryBegin(&cursor, &summary) != r4os.abi.program_handle_ok) continue;
            var total: u64 = 0;
            var first_found = false;
            var second_found = false;
            while (true) {
                var entries: [task_inventory_page_capacity]r4os.abi.ProgramTaskSnapshot = undefined;
                var page: r4os.abi.ProgramInventoryPageInfo = .{};
                if (self.ctx.programInventoryTasks(&cursor, entries[0..], &page) != r4os.abi.program_handle_ok) continue :retry;
                if (page.status == r4os.abi.program_inventory_status_restart or
                    page.snapshot_generation != cursor.snapshot_generation or page.returned > entries.len)
                {
                    continue :retry;
                }
                for (entries[0..@intCast(page.returned)]) |entry| {
                    if (entry.owner_instance_id == first_id) {
                        total +|= entry.runtime_ticks;
                        first_found = true;
                    } else if (entry.owner_instance_id == second_id) {
                        total +|= entry.runtime_ticks;
                        second_found = true;
                    }
                }
                if (page.status == r4os.abi.program_inventory_status_complete)
                    return if (first_found and second_found) total else null;
                if (page.status != r4os.abi.program_inventory_status_more or page.returned == 0) continue :retry;
            }
        }
        return null;
    }

    fn smokeInitialDesktopState(self: *const App) bool {
        self.ctx.print("Initial desktop state: ");
        for (self.windows) |win| {
            if (win.visible) {
                self.ctx.println("FAILED-visible-window");
                return false;
            }
        }
        if (self.keyboard_focus != .none) {
            self.ctx.println("FAILED-focus");
            return false;
        }
        self.ctx.println("ok");
        return true;
    }

    fn smokeDesktopFolderDefaults(self: *const App) bool {
        self.ctx.print("Desktop folder defaults: ");
        var ok = self.ctx.exists(desktop_folder.default_dir);
        var i: usize = 0;
        while (i < desktop_folder.default_links.len) : (i += 1) {
            ok = self.smokeDesktopFolderLink(desktop_folder.default_links[i]) and ok;
        }
        self.ctx.println(if (ok) "ok" else "FAILED");
        return ok;
    }

    fn smokeDesktopFolderLink(self: *const App, spec: desktop_folder.DefaultLink) bool {
        var path: [desktop_folder.max_link_path + 1]u8 = .{0} ** (desktop_folder.max_link_path + 1);
        _ = desktop_folder.defaultLinkPath(path[0..], desktop_folder.fileName(spec)) orelse return false;
        var bytes: [desktop_folder.max_link_bytes]u8 = .{0} ** desktop_folder.max_link_bytes;
        const len = self.ctx.fileRead(zptr(path[0..]), bytes[0..]);
        if (len <= 0) return false;
        const data = bytes[0..@as(usize, @intCast(len))];
        const link = r4std.shortcut.parse(data) catch return false;
        return desktop_folder.linkMatchesSpec(&link, spec);
    }

    fn smokeDesktopFolderItems(self: *const App) bool {
        self.ctx.print("Desktop folder items: ");
        if (desktop_items.max_items <= 8 or desktop_items.indexForTarget(.desktop_item_32) == null) {
            self.ctx.println("FAILED-target-range");
            return false;
        }
        if (self.desktop_items.count < desktop_folder.default_links.len) {
            self.ctx.println("FAILED-count");
            return false;
        }
        if (self.desktop_items.truncated) {
            self.ctx.println("FAILED-truncated");
            return false;
        }
        var found_computer = false;
        var found_terminal = false;
        var found_test_file = false;
        var found_test_directory = false;
        var index: usize = 0;
        while (index < self.desktop_items.count) : (index += 1) {
            const item = &self.desktop_items.entries[index];
            if (item.target == .none or item.titleText().len == 0 or item.pathText().len == 0 or item.launchPathText().len == 0) {
                self.ctx.println("FAILED-item");
                return false;
            }
            if (item.kind == .shortcut and equalsIgnoreCase(item.titleText(), "Computer") and isExplorerPath(item.launchPathText()) and item.launch_policy == .gui) {
                found_computer = true;
            }
            if (item.kind == .shortcut and equalsIgnoreCase(item.titleText(), "Terminal") and endsWithIgnoreCase(item.launchPathText(), "TERMINAL.R4X") and item.launch_policy == .console) {
                found_terminal = true;
            }
            if (item.kind == .file and item.launch_kind == .file and equalsIgnoreCase(item.titleText(), "README.TXT")) {
                found_test_file = true;
            }
            if (item.kind == .directory and item.launch_kind == .directory and equalsIgnoreCase(item.titleText(), "TOOLS")) {
                found_test_directory = true;
            }
        }
        if (!found_computer or !found_terminal) {
            self.ctx.println("FAILED-default-link");
            return false;
        }
        if ((self.ctx.exists("C:\\R4OS\\DESKTOP\\README.TXT") and !found_test_file) or
            (self.ctx.exists("C:\\R4OS\\DESKTOP\\TOOLS\\README.TXT") and !found_test_directory))
        {
            self.ctx.println("FAILED-file-dir");
            return false;
        }
        self.ctx.println("ok");
        return true;
    }

    fn smokeDesktopLayoutDrag(self: *App) bool {
        self.ctx.print("Desktop layout drag: ");
        if (self.desktop_items.count < 2) {
            self.ctx.println("FAILED-count");
            return false;
        }

        const index: usize = 0;
        var path: [desktop_items.path_max + 1]u8 = .{0} ** (desktop_items.path_max + 1);
        copySliceZ(path[0..], self.desktop_items.entries[index].pathText());
        const path_text = spanZ(path[0..]);
        if (!desktop_layout.validDesktopPath(path_text)) {
            self.ctx.println("FAILED-path");
            return false;
        }
        if (!self.ctx.exists(zptr(path[0..]))) {
            self.ctx.println("FAILED-source");
            return false;
        }

        const visible_before = self.countVisibleWindows();
        const old_x = self.desktop_items.entries[index].x;
        const old_y = self.desktop_items.entries[index].y;
        self.beginDesktopItemDrag(index, old_x + 8, old_y + 8);
        _ = self.updateDesktopItemDrag(old_x + 9, old_y + 9);
        if (self.desktop_drag.active or self.desktop_items.entries[index].x != old_x or self.desktop_items.entries[index].y != old_y) {
            self.ctx.println("FAILED-threshold");
            self.desktop_drag = .{};
            return false;
        }
        if (self.desktopGridDragIndex() != desktop_items.no_selection) {
            self.ctx.println("FAILED-grid-threshold");
            self.desktop_drag = .{};
            return false;
        }

        _ = self.updateDesktopItemDrag(old_x + desktop_items.cell_w + 32, old_y + 8);
        if (!self.desktop_drag.active) {
            self.ctx.println("FAILED-start");
            self.desktop_drag = .{};
            return false;
        }
        if (self.desktopGridDragIndex() != index or self.desktop_items.snapPreview(index, self.screen_w, self.screen_h, theme.taskbar_h) == null) {
            self.ctx.println("FAILED-grid");
            self.desktop_drag = .{};
            return false;
        }
        const finished_drag = self.finishDesktopItemDrag();
        self.desktop_drag = .{};
        if (self.countVisibleWindows() != visible_before) {
            self.ctx.println("FAILED-launch");
            return false;
        }

        const moved_x = self.desktop_items.entries[index].x;
        const moved_y = self.desktop_items.entries[index].y;
        if (moved_x == old_x and moved_y == old_y) {
            self.ctx.println("FAILED-move");
            return false;
        }
        if (!finished_drag or !self.desktop_layout_writeback.isDirty()) {
            self.ctx.println("FAILED-dirty");
            return false;
        }
        if (self.flushDesktopLayoutIfDue()) {
            self.ctx.println("FAILED-defer");
            return false;
        }
        self.smokePumpFrames((r4std.settings.WritebackPolicy.defaultSaveDelayMs(.lazy) / loop_sleep_ms) + 4);
        if (self.desktop_layout_writeback.isDirty()) {
            self.ctx.println("FAILED-writeback");
            return false;
        }
        if (self.render_stats.layout_worker_started == 0 or self.render_stats.layout_worker_completed == 0) {
            self.ctx.println("FAILED-worker");
            return false;
        }
        if (self.render_stats.layout_worker_errors != 0) {
            self.ctx.println("FAILED-worker-error");
            return false;
        }
        if (!self.ctx.exists(desktop_layout_path)) {
            self.ctx.println("FAILED-write");
            return false;
        }

        const layout_state = self.loadDesktopLayout();
        if (layout_state.count == 0) {
            self.ctx.println("FAILED-load");
            return false;
        }
        const saved = layout_state.positionForPath(path_text) orelse {
            self.ctx.println("FAILED-save");
            return false;
        };
        if (saved.x != moved_x or saved.y != moved_y) {
            self.ctx.println("FAILED-position");
            return false;
        }

        self.loadDesktopItemsFolder();
        if (self.findDesktopItemByPath(path_text)) |reloaded| {
            const item = &self.desktop_items.entries[reloaded];
            if (item.x == moved_x and item.y == moved_y and self.ctx.exists(zptr(path[0..]))) {
                self.ctx.println("ok");
                return true;
            }
            self.ctx.println("FAILED-reload");
            return false;
        }
        self.ctx.println("FAILED-missing");
        return false;
    }

    fn smokeTimeMenu(self: *App) bool {
        self.ctx.print("Clock taskbar menu launch: ");
        self.openTimeMenu();
        if (!self.time_menu_open) {
            self.ctx.println("FAILED-open");
            return false;
        }
        self.dispatchCommand(.time_menu_clock, .keyboard);
        self.smokePumpFrames(4);
        if (self.hasDamage()) self.redraw();
        const clock_window = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\CLOCK.R4X") orelse {
            self.ctx.println("FAILED-clock");
            return false;
        };
        _ = clock_window;
        if (self.time_menu_open) {
            self.ctx.println("FAILED-still-open");
            return false;
        }
        self.ctx.println("ok");
        return true;
    }

    fn smokeLaunchTimeSettingsFromMenu(self: *App) bool {
        self.ctx.print("Settings/Time menu launch: ");
        const settings_hit = self.menuIndexForTarget(.menu_settings) orelse {
            self.ctx.println("FAILED-settings");
            return false;
        };
        const time_hit = self.submenuIndexForTarget(.menu_settings_time) orelse {
            self.ctx.println("FAILED-time");
            return false;
        };
        if (time_hit.parent != settings_hit) {
            self.ctx.println("FAILED-parent");
            return false;
        }

        self.toggleStart();
        self.menu_selected = settings_hit;
        self.menuRight();
        self.menu_submenu_selected = time_hit.index;
        const command = self.defaultCommand();
        if (command != .menu_settings_time) {
            self.ctx.println("FAILED-command");
            self.closeStartMenu();
            return false;
        }
        self.dispatchCommand(command, .keyboard);
        self.smokePumpFrames(4);
        if (self.hasDamage()) self.redraw();
        const first = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\TIMESET.R4X") orelse {
            self.ctx.println("FAILED-launch");
            return false;
        };

        self.toggleStart();
        self.menu_selected = settings_hit;
        self.menuRight();
        self.menu_submenu_selected = time_hit.index;
        self.dispatchCommand(self.defaultCommand(), .keyboard);
        self.smokePumpFrames(2);
        if (self.hasDamage()) self.redraw();
        const second = self.findSingleInstanceGuiWindow("C:\\R4OS\\SOFTWARE\\DESKTOP\\TIMESET.R4X") orelse {
            self.ctx.println("FAILED-refocus");
            return false;
        };
        if (first != second) {
            self.ctx.println("FAILED-duplicate");
            return false;
        }
        self.ctx.println("ok");
        return true;
    }

    fn smokeKeyboardLayoutSwitch(self: *App) bool {
        self.ctx.print("Keyboard layout taskbar switch: ");
        var before: r4os.abi.KeyboardLayoutInfo = .{};
        if (self.ctx.keyboardLayoutCurrent(&before) <= 0) {
            self.ctx.println("FAILED-current");
            return false;
        }
        if (before.count < 2) {
            self.ctx.println("FAILED-count");
            return false;
        }
        self.cycleKeyboardLayout();
        var after: r4os.abi.KeyboardLayoutInfo = .{};
        if (self.ctx.keyboardLayoutCurrent(&after) <= 0) {
            self.ctx.println("FAILED-after");
            return false;
        }
        const changed = !fixedBytesEqual(before.name[0..], after.name[0..]);
        _ = self.ctx.keyboardLayoutSet(zptr(before.name[0..]));
        _ = self.updateKeyboardLayout();
        self.ctx.println(if (changed) "ok" else "FAILED-unchanged");
        return changed;
    }

    fn smokeStartMenuKey(self: *App) bool {
        self.ctx.print("Windows key start menu: ");
        const was_open = self.start_open;
        if (was_open) self.closeStartMenu();
        _ = self.handleKeyboardEvent(r4os.gui.Key.start_menu);
        const opened = self.start_open;
        if (opened) self.closeStartMenu();
        if (was_open and !self.start_open) self.toggleStart();
        self.ctx.println(if (opened) "ok" else "FAILED");
        return opened;
    }

    fn smokeClipboardContract(self: *App) bool {
        self.ctx.print("Clipboard contract and Run paste: ");
        if (!self.ctx.supportsClipboardContract()) {
            self.ctx.println("FAILED-unsupported");
            return false;
        }
        if (self.ctx.clipboardClear() != 0) {
            self.ctx.println("FAILED-clear");
            return false;
        }
        const sample = "C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X";
        if (self.ctx.clipboardWrite(sample) != @as(i32, @intCast(sample.len))) {
            self.ctx.println("FAILED-write");
            return false;
        }
        var info: r4os.abi.ClipboardInfo = .{};
        if (self.ctx.clipboardInfo(&info) != 0 or info.length != @as(u32, @intCast(sample.len)) or (info.flags & r4os.abi.clipboard_flag_has_text) == 0) {
            self.ctx.println("FAILED-info");
            return false;
        }
        var out: [clipboard_buffer_size]u8 = .{0} ** clipboard_buffer_size;
        const read_len = self.ctx.clipboardRead(out[0..]);
        if (read_len != @as(i32, @intCast(sample.len)) or !bytesEqual(out[0..@as(usize, @intCast(read_len))], sample)) {
            self.ctx.println("FAILED-read");
            return false;
        }

        self.openDialog(.run);
        self.run_path.clear();
        self.setDialogFocus(.run_input);
        if (!self.handleRunKey(r4os.gui.Key.ctrl_v) or !bytesEqual(self.run_path.text(), sample)) {
            self.closeTop();
            self.run_path.set(default_run_path);
            _ = self.ctx.clipboardClear();
            self.ctx.println("FAILED-paste");
            return false;
        }
        self.closeTop();
        self.run_path.set(default_run_path);
        _ = self.ctx.clipboardClear();
        self.ctx.println("ok");
        return true;
    }

    fn smokeRemoteFrameContract(self: *App) bool {
        const demand_supported = self.ctx.supportsRemoteFrameDemand();
        var acquired = false;
        if (demand_supported) {
            const acquire_rc = self.ctx.remoteFrameAcquire();
            if (acquire_rc <= 0) {
                self.ctx.println("Remote frame snapshot: FAILED-acquire");
                return false;
            }
            acquired = true;
            self.remote_frame_consumers = @intCast(acquire_rc);
            self.invalidateFull();
            self.redraw();
        }
        defer if (acquired) {
            const remaining = self.ctx.remoteFrameRelease();
            self.remote_frame_consumers = if (remaining > 0) @intCast(remaining) else 0;
        };

        var info: r4os.abi.RemoteFrameInfo = .{};
        const info_rc = self.ctx.remoteFrameInfo(&info);
        if (info_rc != 0 or info.magic != r4os.abi.remote_frame_magic or info.version != r4os.abi.remote_frame_version) {
            self.ctx.println("Remote frame snapshot: FAILED-info");
            return false;
        }
        if ((info.flags & r4os.abi.remote_frame_flag_ready) == 0 or
            info.format != r4os.abi.remote_frame_format_xrgb32 or
            info.width != @as(u32, @intCast(self.screen_w)) or
            info.height != @as(u32, @intCast(self.screen_h)) or
            info.revision == 0 or
            info.frame_pixels == 0 or
            info.dirty_w == 0 or
            info.dirty_h == 0)
        {
            self.ctx.println("Remote frame snapshot: FAILED-shape");
            return false;
        }

        var pixels: [16]u32 = .{0} ** 16;
        var read_info: r4os.abi.RemoteFrameInfo = .{};
        const read_rc = self.ctx.remoteFrameRead(0, pixels[0..], &read_info);
        if (read_rc <= 0 or read_info.revision != info.revision) {
            self.ctx.println("Remote frame snapshot: FAILED-read");
            return false;
        }
        var wait_info: r4os.abi.RemoteFrameInfo = .{};
        const wait_rc = self.ctx.remoteFrameWait(info.revision, 1, &wait_info);
        if (wait_rc < 0) {
            self.ctx.println("Remote frame snapshot: FAILED-wait");
            return false;
        }

        if (!self.runRemoteFrameDiag()) return false;

        self.ctx.write("Remote frame snapshot: ok mode=");
        self.ctx.printU64(@as(u64, info.width));
        self.ctx.write("x");
        self.ctx.printU64(@as(u64, info.height));
        self.ctx.write(" rev=");
        self.ctx.printU64(@as(u64, info.revision));
        self.ctx.write(" read=");
        self.ctx.printI32(read_rc);
        self.ctx.write(" wait=");
        self.ctx.printI32(wait_rc);
        self.ctx.println("");
        return true;
    }

    fn runRemoteFrameDiag(self: *App) bool {
        var handle: r4os.abi.ProgramProcessHandle = .{};
        const spawn_rc = self.ctx.programSpawnHandle("C:\\R4OS\\SOFTWARE\\TERMINAL\\DIAG\\RFDIAG.R4X", "/DESKTOP", .console, &handle);
        if (spawn_rc != r4os.abi.program_handle_ok or !processHandleValid(handle)) {
            self.ctx.print("FAILED-diag-spawn=");
            self.ctx.printI32(spawn_rc);
            self.ctx.println("");
            return false;
        }

        const instance_id = handle.instance_id;
        var output: [2048]u8 = .{0} ** 2048;
        var output_len: usize = 0;
        var exit_code: i32 = -1;
        var done = false;
        var tries: u32 = 0;
        while (tries < 80) : (tries += 1) {
            self.ctx.sleepTicks(1);
            var info: r4os.abi.ProgramInstanceInfo = .{};
            const status = self.ctx.programHandleStatus(&handle, &info);
            const got = self.ctx.consoleOutput(instance_id, output[0..]);
            if (got > 0) output_len = @intCast(got);
            if (status == r4os.abi.program_handle_ok and info.state == @intFromEnum(r4os.abi.ProgramInstanceState.done)) {
                done = true;
                exit_code = info.exit_code;
                break;
            }
            if (status != r4os.abi.program_handle_ok and status != r4os.abi.program_handle_error_would_block) break;
        }

        const got = self.ctx.consoleOutput(instance_id, output[0..]);
        if (got > 0) output_len = @intCast(got);
        const text = output[0..output_len];
        if (text.len > 0) self.ctx.write(text);

        if (!done) {
            var kill_attempt: u32 = 0;
            while (kill_attempt < task_inventory_restart_limit) : (kill_attempt += 1) {
                const kill_status = self.ctx.programHandleKill(&handle);
                if (kill_status == r4os.abi.program_handle_ok or processHandleDefinitelyGone(kill_status)) break;
                if (kill_status != r4os.abi.program_handle_error_would_block and kill_status != r4os.abi.program_handle_error_not_running) break;
                self.ctx.sleepTicks(1);
            }
        }

        var reaped = false;
        var reap_attempt: u32 = 0;
        while (reap_attempt < 80) : (reap_attempt += 1) {
            var completion: r4os.abi.ProgramProcessCompletion = .{};
            const reap_status = self.ctx.programHandleReap(&handle, &completion);
            if (reap_status == r4os.abi.program_handle_ok or processHandleDefinitelyGone(reap_status)) {
                reaped = true;
                break;
            }
            if (reap_status != r4os.abi.program_handle_error_would_block and reap_status != r4os.abi.program_handle_error_not_running) break;
            self.ctx.sleepTicks(1);
        }
        if (!reaped) {
            self.ctx.println("FAILED-diag-reap");
            return false;
        }

        if (!done) {
            self.ctx.println("FAILED-diag-timeout");
            return false;
        }
        if (exit_code != 0) {
            self.ctx.print("FAILED-diag-exit=");
            self.ctx.printI32(exit_code);
            self.ctx.println("");
            return false;
        }
        if (!containsBytes(text, "RFDIAG snapshot: OK") or !containsBytes(text, "RFDIAG result: OK")) {
            self.ctx.println("FAILED-diag-output");
            return false;
        }
        return true;
    }

    fn programInstanceByIdLive(self: *const App, instance_id: u32, out: *r4os.abi.ProgramInstanceInfo) bool {
        var attempt: u32 = 0;
        restart: while (attempt < task_inventory_restart_limit) : (attempt += 1) {
            var cursor: r4os.abi.ProgramInventoryCursor = .{};
            var summary: r4os.abi.ProgramInventorySummary = .{};
            if (self.ctx.programInventoryBegin(&cursor, &summary) != r4os.abi.program_handle_ok) break;

            while (true) {
                var entries: [task_inventory_page_capacity]r4os.abi.ProgramInstanceSnapshot = undefined;
                var page: r4os.abi.ProgramInventoryPageInfo = .{};
                if (self.ctx.programInventoryPrograms(&cursor, entries[0..], &page) != r4os.abi.program_handle_ok) break :restart;
                if (page.status == r4os.abi.program_inventory_status_restart) continue :restart;
                if (page.returned > entries.len or page.snapshot_generation != cursor.snapshot_generation) break :restart;
                for (entries[0..@intCast(page.returned)]) |entry| {
                    if (entry.info.id == instance_id) {
                        out.* = entry.info;
                        return true;
                    }
                }
                if (page.status == r4os.abi.program_inventory_status_complete) break;
                if (page.status != r4os.abi.program_inventory_status_more or page.returned == 0) break :restart;
            }
            out.* = .{};
            return false;
        }
        out.* = .{};
        return false;
    }

    fn smokeTerminalCtrlC(self: *App, index: usize, instance_id: u32) bool {
        self.ctx.print("Terminal Ctrl+C lifecycle close: ");
        if (index >= self.windows.len or self.windows[index].instance_id != instance_id) {
            self.ctx.println("FAILED-window");
            return false;
        }
        self.focusOrRestore(index);
        if (!self.handleKeyboardEvent(r4os.gui.Key.ctrl_c)) {
            self.ctx.println("FAILED-dispatch");
            return false;
        }
        var tries: u32 = 0;
        while (tries < 12) : (tries += 1) {
            self.smokePumpFrames(1);
            _ = self.syncProgramWindows();
            if (index >= self.windows.len or self.windows[index].instance_id != instance_id or !self.windows[index].visible) {
                self.ctx.println("ok");
                return true;
            }
        }
        self.ctx.println("FAILED-timeout");
        return false;
    }

    fn smokeWindowServiceMirror(self: *App, comptime label: []const u8, index: usize) bool {
        self.ctx.print("WINSVC mirror ");
        self.ctx.write(label);
        self.ctx.print(": ");
        if (index >= self.windows.len or self.windows[index].instance_id == 0) {
            self.ctx.println("FAILED-window");
            return false;
        }
        if (!self.refreshWindowServiceSnapshot()) {
            self.ctx.println("FAILED-service");
            return false;
        }
        const expected_instance = self.windows[index].instance_id;
        var record_count: usize = @intCast(self.win_service_snapshot.status.window_count);
        if (record_count > self.win_service_snapshot.records.len) record_count = self.win_service_snapshot.records.len;
        var i: usize = 0;
        while (i < record_count) : (i += 1) {
            const record = self.win_service_snapshot.records[i];
            if (record.window_id != @as(u32, @intCast(index))) continue;
            if (record.instance_id != expected_instance) {
                self.ctx.println("FAILED-instance");
                return false;
            }
            if ((record.flags & r4os.abi.window_service_flag_visible) == 0) {
                self.ctx.println("FAILED-visible");
                return false;
            }
            self.ctx.print("ok windows=");
            self.ctx.printU64(self.win_service_snapshot.status.window_count);
            self.ctx.print(" focus=");
            self.ctx.printU64(self.win_service_snapshot.status.focused_window);
            self.ctx.println("");
            return true;
        }
        self.ctx.println("FAILED-missing");
        return false;
    }

    fn printSmokeRenderStats(self: *const App, comptime label: []const u8, before: RenderSnapshot) RenderSnapshot {
        const after = self.renderSnapshot();
        self.ctx.print("DESKTOP render ");
        self.ctx.write(label);
        self.ctx.print(": redraws=");
        self.ctx.printU64(after.redraws);
        self.ctx.print(" delta_redraws=");
        self.ctx.printU64(@as(u32, after.redraws -% before.redraws));
        self.ctx.print(" full=");
        self.ctx.printU64(after.full);
        self.ctx.print(" delta_full=");
        self.ctx.printU64(@as(u32, after.full -% before.full));
        self.ctx.print(" mixed=");
        self.ctx.printU64(after.mixed);
        self.ctx.print(" delta_mixed=");
        self.ctx.printU64(@as(u32, after.mixed -% before.mixed));
        self.ctx.print(" cursor=");
        self.ctx.printU64(after.cursor);
        self.ctx.print(" delta_cursor=");
        self.ctx.printU64(@as(u32, after.cursor -% before.cursor));
        self.ctx.print(" total_pixels=");
        self.ctx.printU64(after.pixels);
        self.ctx.print(" delta_pixels=");
        self.ctx.printU64(after.pixels -% before.pixels);
        self.ctx.print(" copy_bytes=");
        self.ctx.printU64(after.copy_bytes);
        self.ctx.print(" delta_copy=");
        self.ctx.printU64(after.copy_bytes -% before.copy_bytes);
        self.ctx.print(" frame_ticks=");
        self.ctx.printU64(self.render_stats.frame_last_ticks);
        self.ctx.print("/");
        self.ctx.printU64(self.render_stats.frame_max_ticks);
        self.ctx.print(" compose_ticks=");
        self.ctx.printU64(self.render_stats.compose_last_ticks);
        self.ctx.print("/");
        self.ctx.printU64(self.render_stats.compose_max_ticks);
        self.ctx.print(" present_ticks=");
        self.ctx.printU64(self.render_stats.present_last_ticks);
        self.ctx.print("/");
        self.ctx.printU64(self.render_stats.present_max_ticks);
        self.ctx.print(" frame_ns=");
        self.ctx.printU64(self.render_stats.frame_last_ns);
        self.ctx.print("/");
        self.ctx.printU64(self.render_stats.frame_max_ns);
        self.ctx.print(" copies=");
        self.ctx.printU64(self.render_stats.scene_copy_successes);
        self.ctx.print("/");
        self.ctx.printU64(self.render_stats.scene_copy_attempts);
        self.ctx.print(" gui_work=");
        self.ctx.printU64(self.render_stats.gui_frame_commands_last);
        self.ctx.print("/");
        self.ctx.printU64(self.render_stats.gui_resource_bytes_last);
        self.ctx.print(" cursor_latency=");
        self.ctx.printU64(self.render_stats.cursor_latency_last_ticks);
        self.ctx.print("/");
        self.ctx.printU64(self.render_stats.cursor_latency_max_ticks);
        self.ctx.print(" ui_blockers=");
        self.ctx.printU64(self.render_stats.ui_blocker_count);
        self.ctx.print("/");
        self.ctx.printU64(self.render_stats.ui_blocker_max_ticks);
        self.ctx.print(" layout_worker=");
        self.ctx.printU64(after.layout_worker_completed);
        self.ctx.print("/");
        self.ctx.printU64(after.layout_worker_started);
        self.ctx.print(" layout_ticks=");
        self.ctx.printU64(self.render_stats.layout_worker_last_ticks);
        self.ctx.print("/");
        self.ctx.printU64(self.render_stats.layout_worker_max_ticks);
        self.ctx.print(" last=");
        self.ctx.write(damageKindName(self.render_stats.last_damage_kind));
        self.ctx.print(" rect=");
        self.ctx.printI32(self.render_stats.last_damage_rect.x);
        self.ctx.print(",");
        self.ctx.printI32(self.render_stats.last_damage_rect.y);
        self.ctx.print(",");
        self.ctx.printI32(self.render_stats.last_damage_rect.w);
        self.ctx.print(",");
        self.ctx.printI32(self.render_stats.last_damage_rect.h);
        self.ctx.print(" pixels=");
        self.ctx.printU64(self.render_stats.last_damage_pixels);
        self.ctx.println("");
        return after;
    }

    fn smokeDesktopResponsiveness(self: *App, expect_cursor: bool) bool {
        const s = self.render_stats;
        const copy_ok = s.redraws > 0 and s.scene_blit_bytes_total > 0 and s.scene_blit_bytes_last > 0;
        const frame_ok = s.frame_max_ticks >= s.frame_last_ticks and
            s.compose_max_ticks >= s.compose_last_ticks and
            s.present_max_ticks >= s.present_last_ticks and
            s.frame_max_ns >= s.frame_last_ns and
            s.compose_max_ns >= s.compose_last_ns and
            s.present_max_ns >= s.present_last_ns;
        const accounting_ok = s.scene_blit_bytes_total == s.display_blit_bytes_total + s.remote_shadow_bytes_total and
            s.scene_copy_attempts == s.scene_copy_successes + s.scene_copy_skips and
            s.present_attempts == s.present_successes + s.present_failures;
        const cursor_ok = !expect_cursor or (s.cursor_moves > 0 and s.cursor_only_presents > 0 and s.cursor_latency_max_ticks >= s.cursor_latency_last_ticks);
        const worker_ok = s.layout_worker_started > 0 and s.layout_worker_completed > 0 and s.layout_worker_errors == 0;
        const ok = copy_ok and frame_ok and accounting_ok and cursor_ok and worker_ok;

        self.ctx.print("DESKTOP responsiveness: ");
        self.ctx.write(if (ok) "OK" else "FAILED");
        self.ctx.print(" copy=");
        self.ctx.printU64(s.scene_blit_bytes_last);
        self.ctx.print("/");
        self.ctx.printU64(s.scene_blit_bytes_total);
        self.ctx.print(" frame=");
        self.ctx.printU64(s.frame_last_ticks);
        self.ctx.print("/");
        self.ctx.printU64(s.frame_max_ticks);
        self.ctx.print(" compose=");
        self.ctx.printU64(s.compose_last_ticks);
        self.ctx.print("/");
        self.ctx.printU64(s.compose_max_ticks);
        self.ctx.print(" frame_ns=");
        self.ctx.printU64(s.frame_last_ns);
        self.ctx.print("/");
        self.ctx.printU64(s.frame_max_ns);
        self.ctx.print(" copy_targets=");
        self.ctx.printU64(s.scene_copy_successes);
        self.ctx.print("/");
        self.ctx.printU64(s.scene_copy_attempts);
        self.ctx.print(" present=");
        self.ctx.printU64(s.present_last_ticks);
        self.ctx.print("/");
        self.ctx.printU64(s.present_max_ticks);
        self.ctx.print(" cursor=");
        self.ctx.printU64(s.cursor_only_presents);
        self.ctx.print(" latency=");
        self.ctx.printU64(s.cursor_latency_last_ticks);
        self.ctx.print("/");
        self.ctx.printU64(s.cursor_latency_max_ticks);
        self.ctx.print(" worker=");
        self.ctx.printU64(s.layout_worker_completed);
        self.ctx.print("/");
        self.ctx.printU64(s.layout_worker_started);
        self.ctx.print(" workerTicks=");
        self.ctx.printU64(s.layout_worker_last_ticks);
        self.ctx.print("/");
        self.ctx.printU64(s.layout_worker_max_ticks);
        self.ctx.print(" errors=");
        self.ctx.printU64(s.layout_worker_errors);
        self.ctx.print(" blockers=");
        self.ctx.printU64(s.ui_blocker_count);
        self.ctx.print("/");
        self.ctx.printU64(s.ui_blocker_max_ticks);
        self.ctx.println("");
        return ok;
    }

    fn renderSnapshot(self: *const App) RenderSnapshot {
        return .{
            .redraws = self.render_stats.redraws,
            .cursor = self.render_stats.cursor_only_presents,
            .mixed = self.render_stats.mixed_damage_presents,
            .full = self.render_stats.full_damage_presents,
            .pixels = self.render_stats.total_damage_pixels,
            .copy_bytes = self.render_stats.scene_blit_bytes_total,
            .layout_worker_started = self.render_stats.layout_worker_started,
            .layout_worker_completed = self.render_stats.layout_worker_completed,
        };
    }

    fn smokePushGuiKey(self: *App, index: usize, key: u8) void {
        self.pushGuiKeyEvent(index, key);
        self.smokePumpFrames(1);
    }

    fn smokePumpFrames(self: *App, frames: u32) void {
        var i: u32 = 0;
        while (i < frames) : (i += 1) {
            self.ctx.sleepTicks(self.loop_sleep_ticks);
            if (self.pollTimerEvent() and self.dispatchEvent()) self.redraw();
        }
    }

    fn smokePumpCooperativeFrames(self: *App, frames: u32) void {
        var i: u32 = 0;
        while (i < frames) : (i += 1) {
            self.ctx.sleepTicks(0);
            if (self.pollTimerEvent() and self.dispatchEvent()) self.redraw();
        }
    }

    fn smokeTrayContract(self: *App) bool {
        var handle: r4os.abi.ProgramProcessHandle = .{};
        const spawn_rc = self.ctx.programSpawnHandle(tray_provider_test_path, tray_provider_test_args, .console, &handle);
        if (spawn_rc != r4os.abi.program_handle_ok or !processHandleValid(handle)) return self.headlessSubsystemFailure("tray-provider-spawn");

        const started = self.ctx.ticks();
        const timeout_ticks = @as(u64, self.monotonic_hz) * 15;
        while (self.tray_registry.statusResult(handle, tray_provider_test_item_id) != r4os.abi.tray_result_ok) {
            _ = self.syncTrayBroker();
            self.ctx.sleepTicks(1);
            if (self.ctx.ticks() -| started >= timeout_ticks) {
                _ = self.ctx.programHandleKill(&handle);
                return self.headlessSubsystemFailure("tray-provider-register");
            }
        }

        _ = self.updateTrayLayout();
        const hit = self.smokeFindTrayIdentity(handle, tray_provider_test_item_id) orelse {
            _ = self.ctx.programHandleKill(&handle);
            return self.headlessSubsystemFailure("tray-provider-layout");
        };
        self.last_mouse_down_target = .none;
        self.last_mouse_down_tray_identity = .{};
        self.last_mouse_down_tick = 0;
        self.double_click_pending = false;
        self.prev_buttons = 0;
        self.smokeTrayMouse(hit.x, hit.y, model.MouseButton.left);
        self.smokeTrayMouse(hit.x, hit.y, 0);

        var exit_code: i32 = -1;
        var completed = false;
        while (self.ctx.ticks() -| started < timeout_ticks) {
            _ = self.syncTrayBroker();
            var info: r4os.abi.ProgramInstanceInfo = .{};
            const status = self.ctx.programHandleStatus(&handle, &info);
            if (status == r4os.abi.program_handle_ok and info.state == @intFromEnum(r4os.abi.ProgramInstanceState.done)) {
                completed = true;
                exit_code = info.exit_code;
                break;
            }
            if (status != r4os.abi.program_handle_ok and status != r4os.abi.program_handle_error_would_block) break;
            self.ctx.sleepTicks(1);
        }
        if (!completed or exit_code != 0) {
            _ = self.ctx.programHandleKill(&handle);
            return self.headlessSubsystemFailure("tray-provider-event");
        }

        // A done process no longer owns UI state even before its completion
        // record is reaped. Exercise that normal bounded owner sweep first.
        var cleanup_wait: u32 = 0;
        while (cleanup_wait < self.monotonic_hz and self.tray_registry.registered_count != 0) : (cleanup_wait += 1) {
            self.tray_next_sync_tick = 0;
            _ = self.syncTrayBroker();
            self.ctx.sleepTicks(1);
        }
        if (self.tray_registry.registered_count != 0 or self.tray_registry.ownerAt(0) != null) {
            return self.headlessSubsystemFailure("tray-provider-cleanup");
        }

        var completion: r4os.abi.ProgramProcessCompletion = .{};
        var reaped = false;
        var reap_attempt: u32 = 0;
        while (reap_attempt < 80) : (reap_attempt += 1) {
            const reap_status = self.ctx.programHandleReap(&handle, &completion);
            if (reap_status == r4os.abi.program_handle_ok or processHandleDefinitelyGone(reap_status)) {
                reaped = true;
                break;
            }
            if (reap_status != r4os.abi.program_handle_error_would_block and reap_status != r4os.abi.program_handle_error_not_running) break;
            self.ctx.sleepTicks(1);
        }
        if (!reaped or (completion.sequence != 0 and completion.exit_code != 0)) {
            return self.headlessSubsystemFailure("tray-provider-reap");
        }

        return true;
    }

    fn restoreSmokeVolumeState(self: *App, original: r4os.abi.AudioServiceMasterState) bool {
        var restored: r4os.abi.AudioServiceMasterState = .{};
        var request: r4os.abi.AudioServiceMasterRequest = .{
            .flags = r4os.abi.audio_master_request_flag_set_volume,
            .fixed_volume = original.last_audible_volume_fixed,
        };
        if (self.ctx.audioSetMasterState(&request, &restored) != r4os.abi.service_api_result_ok) return false;

        request = .{
            .flags = r4os.abi.audio_master_request_flag_set_volume |
                r4os.abi.audio_master_request_flag_set_muted |
                (if ((original.flags & r4os.abi.audio_master_state_flag_muted) != 0)
                    r4os.abi.audio_master_request_flag_muted
                else
                    0),
            .fixed_volume = original.selected_volume_fixed,
        };
        if (self.ctx.audioSetMasterState(&request, &restored) != r4os.abi.service_api_result_ok) return false;
        return restored.selected_volume_fixed == original.selected_volume_fixed and
            restored.last_audible_volume_fixed == original.last_audible_volume_fixed and
            (restored.flags & r4os.abi.audio_master_state_flag_muted) ==
                (original.flags & r4os.abi.audio_master_state_flag_muted);
    }

    fn smokeVolumeContract(self: *App) bool {
        if (!self.config.taskbar_clock or !self.ctx.exists(audio_service_module_path)) {
            return self.headlessSubsystemFailure("volume-profile");
        }

        var original: r4os.abi.AudioServiceMasterState = .{};
        if (self.ctx.audioMasterState(&original) != r4os.abi.service_api_result_ok) {
            return self.headlessSubsystemFailure("volume-service-status");
        }
        self.acceptVolumeState(original);
        _ = self.updateTrayLayout();

        const clock = draw.taskbarClockRect(self.screen_w, self.screen_h);
        const icon = draw.taskbarVolumeRect(self.screen_w, self.screen_h, true);
        const keyboard = draw.taskbarKeyboardLayoutRect(self.screen_w, self.screen_h, true, true);
        if (icon.right() + volume.icon_gap != clock.x or keyboard.right() + draw.taskbar_layout_gap != icon.x) {
            return self.headlessSubsystemFailure("volume-layout");
        }

        const popup_was_open = self.volume_ui.popup_open;
        var restore_needed = false;
        defer {
            self.volume_ui.popup_open = popup_was_open;
            if (restore_needed) _ = self.restoreSmokeVolumeState(original);
        }

        var changed: r4os.abi.AudioServiceMasterState = .{};
        var request: r4os.abi.AudioServiceMasterRequest = .{
            .flags = r4os.abi.audio_master_request_flag_set_volume |
                r4os.abi.audio_master_request_flag_set_muted |
                r4os.abi.audio_master_request_flag_muted,
            .fixed_volume = volume.gainForPercent(35),
        };
        if (self.ctx.audioSetMasterState(&request, &changed) != r4os.abi.service_api_result_ok) {
            return self.headlessSubsystemFailure("volume-set-muted");
        }
        restore_needed = true;
        if ((changed.flags & r4os.abi.audio_master_state_flag_muted) == 0 or
            changed.selected_volume_fixed != request.fixed_volume or changed.effective_volume_fixed != 0)
        {
            return self.headlessSubsystemFailure("volume-muted-state");
        }

        request = .{
            .flags = r4os.abi.audio_master_request_flag_set_volume,
            .fixed_volume = volume.gainForPercent(40),
        };
        if (self.ctx.audioSetMasterState(&request, &changed) != r4os.abi.service_api_result_ok or
            (changed.flags & r4os.abi.audio_master_state_flag_muted) != 0 or
            changed.selected_volume_fixed != request.fixed_volume or
            changed.effective_volume_fixed != request.fixed_volume)
        {
            return self.headlessSubsystemFailure("volume-positive-unmute");
        }

        self.volume_ui.popup_open = true;
        const track = draw.volumeTrackRect(self.screen_w, self.screen_h, true);
        const mute = draw.volumeMuteRect(self.screen_w, self.screen_h, true);
        if (self.volumePopupTargetAt(track.x + @divTrunc(track.w, 2), track.y + @divTrunc(track.h, 2)) != .volume_popup_slider or
            self.volumePopupTargetAt(mute.x + @divTrunc(mute.w, 2), mute.y + @divTrunc(mute.h, 2)) != .volume_popup_mute or
            self.volumePopupTargetAt(icon.x + @divTrunc(icon.w, 2), icon.y + @divTrunc(icon.h, 2)) != .taskbar_volume or
            self.volumePopupTargetAt(0, 0) != .volume_popup_backdrop)
        {
            return self.headlessSubsystemFailure("volume-popup-targets");
        }

        if (!self.restoreSmokeVolumeState(original)) return self.headlessSubsystemFailure("volume-restore");
        restore_needed = false;
        return true;
    }

    fn smokeFindTrayIdentity(self: *const App, owner: r4os.abi.ProgramProcessHandle, item_id: u64) ?struct { x: i32, y: i32 } {
        const y = self.screen_h - theme.taskbar_h + 12;
        var x: i32 = 0;
        while (x < self.screen_w) : (x += 1) {
            const identity = self.tray_registry.hit(x, y) orelse continue;
            if (tray.sameOwner(identity.owner, owner) and identity.item_id == item_id) return .{ .x = x, .y = y };
        }
        return null;
    }

    fn smokeTrayMouse(self: *App, x: i32, y: i32, buttons: u8) void {
        self.beginEvent(.mouse);
        self.event_mouse = .{
            .x = x,
            .y = y,
            .dx = 0,
            .dy = 0,
            .wheel = 0,
            .buttons = buttons,
            .present = 1,
            .reserved = 0,
            .packets = 0,
        };
        if (self.prepareMouseEvent(.mouse)) _ = self.dispatchEvent();
    }

    fn startHeadlessSubsystemAcceptance(self: *App) bool {
        _ = self.ctx.fileDelete(subsystem_host_test_marker_path);
        _ = self.ctx.fileDelete(r4basic_baseline_marker_path);
        _ = self.ctx.fileDelete(r4gb_host_test_marker_path);
        _ = self.ctx.fileDelete(r4gb_report_a_path);
        _ = self.ctx.fileDelete(r4gb_report_b_path);
        _ = self.ctx.fileDelete(r4gb_report_cgb_path);
        if (!self.waitForHeadlessSubsystemAudio()) return self.headlessSubsystemFailure("audio-service");
        if (!self.smokeLaunchMultipleTerminalWindows()) return self.headlessSubsystemFailure("terminal-window-close");
        if (!self.smokeTrayContract()) return false;
        if (!self.smokeVolumeContract()) return false;
        self.window_completion_handles = .{r4os.abi.ProgramProcessHandle{}} ** self.window_completion_handles.len;
        self.window_completion_exit_codes = .{0} ** self.window_completion_exit_codes.len;
        if (r4std.subsystem_runtime.load(&self.ctx.sys) != .loaded) return self.headlessSubsystemFailure("catalog-load");

        const first = self.launchHeadlessSubsystemGuest(subsystem_guest_a_path) orelse return false;
        const second = self.launchHeadlessSubsystemGuest(subsystem_guest_b_path) orelse return false;
        if (first.index == second.index or sameProcessHandle(first.handle, second.handle)) return self.headlessSubsystemFailure("instance-identity");
        if (!self.smokePresentPathContract()) return false;

        self.toggleMaximizeWindow(first.index);
        self.smokePumpCooperativeFrames(1);
        if (sameProcessHandle(self.window_process_handles[first.index], first.handle)) self.toggleMaximizeWindow(first.index);
        self.activateWindow(second.index, false);
        self.pushGuiKeyEvent(first.index, 'P');
        self.pushGuiKeyEvent(second.index, 'P');

        const started = self.ctx.ticks();
        const timeout_ticks = @as(u64, self.monotonic_hz) * 45;
        while (sameProcessHandle(self.window_process_handles[first.index], first.handle) or
            sameProcessHandle(self.window_process_handles[second.index], second.handle))
        {
            self.smokePumpCooperativeFrames(1);
            if (self.ctx.ticks() -| started >= timeout_ticks) return self.headlessSubsystemFailure("runtime-timeout");
        }

        if (!sameProcessHandle(self.window_completion_handles[first.index], first.handle)) return self.headlessSubsystemFailure("completion-a-handle");
        if (self.window_completion_exit_codes[first.index] != 0) return self.headlessSubsystemFailure("completion-a-exit");
        if (!sameProcessHandle(self.window_completion_handles[second.index], second.handle)) return self.headlessSubsystemFailure("completion-b-handle");
        if (self.window_completion_exit_codes[second.index] != 0) return self.headlessSubsystemFailure("completion-b-exit");

        const baseline = self.launchHeadlessR4BasicBaseline() orelse return false;
        const baseline_started = self.ctx.ticks();
        const baseline_timeout_ticks = @as(u64, self.monotonic_hz) * 60;
        while (sameProcessHandle(self.window_process_handles[baseline.index], baseline.handle)) {
            self.smokePumpCooperativeFrames(1);
            if (self.ctx.ticks() -| baseline_started >= baseline_timeout_ticks) return self.headlessSubsystemFailure("r4basic-timeout");
        }
        if (!sameProcessHandle(self.window_completion_handles[baseline.index], baseline.handle)) return self.headlessSubsystemFailure("r4basic-completion-handle");
        if (self.window_completion_exit_codes[baseline.index] != 0) return self.headlessSubsystemFailure("r4basic-completion-exit");
        if (!self.ctx.exists(r4basic_baseline_marker_path)) return self.headlessSubsystemFailure("r4basic-marker");

        const r4gb = self.launchHeadlessR4GbHostSelfTest() orelse return false;
        const r4gb_started = self.ctx.ticks();
        const r4gb_timeout_ticks = @as(u64, self.monotonic_hz) * 30;
        while (sameProcessHandle(self.window_process_handles[r4gb.index], r4gb.handle)) {
            self.smokePumpCooperativeFrames(1);
            if (self.ctx.ticks() -| r4gb_started >= r4gb_timeout_ticks) return self.headlessSubsystemFailure("r4gb-timeout");
        }
        if (!sameProcessHandle(self.window_completion_handles[r4gb.index], r4gb.handle)) return self.headlessSubsystemFailure("r4gb-completion-handle");
        if (self.window_completion_exit_codes[r4gb.index] != 0) return self.headlessSubsystemFailure("r4gb-completion-exit");
        if (!self.ctx.exists(r4gb_host_test_marker_path)) return self.headlessSubsystemFailure("r4gb-marker");
        if (!self.smokeR4GbProductPath()) return false;
        if (self.ctx.fileWrite(subsystem_host_test_marker_path, subsystem_host_test_marker) != @as(i32, @intCast(subsystem_host_test_marker.len))) return false;

        self.enterTerminalModeWithArgs("");
        if (!self.terminal_mode or self.windows[0].instance_id == 0) return self.headlessSubsystemFailure("terminal-mode");
        self.headless_acceptance_terminal = true;
        return true;
    }

    fn smokePresentPathContract(self: *App) bool {
        if (!self.ctx.supportsDisplayBlitStride()) return self.headlessSubsystemFailure("present-stride-api");
        if (!self.ctx.supportsDisplayPresentRegions()) return self.headlessSubsystemFailure("present-regions-api");
        if (!self.ctx.supportsRemoteFrameRegions()) return self.headlessSubsystemFailure("present-remote-regions-api");
        if (!self.ctx.supportsRemoteFrameDemand()) return self.headlessSubsystemFailure("present-demand-api");
        if (self.ctx.remoteFrameConsumers() != 0) return self.headlessSubsystemFailure("present-demand-initial");
        if (!self.smokeTextDamageContract()) return false;

        var capabilities: r4os.abi.DisplayPresentCapabilities = .{};
        if (self.ctx.displayPresentCapabilities(&capabilities) != 0 or
            capabilities.max_regions < 2 or
            capabilities.backend_kind != r4os.abi.display_present_backend_external_blit or
            (capabilities.flags & (r4os.abi.display_present_cap_cpu_fallback |
                r4os.abi.display_present_cap_exact_regions |
                r4os.abi.display_present_cap_sync_fence |
                r4os.abi.display_present_cap_accelerated_blit |
                r4os.abi.display_present_cap_external_backend)) !=
                (r4os.abi.display_present_cap_cpu_fallback |
                    r4os.abi.display_present_cap_exact_regions |
                    r4os.abi.display_present_cap_sync_fence |
                    r4os.abi.display_present_cap_accelerated_blit |
                    r4os.abi.display_present_cap_external_backend) or
            !fixedNameEquals(capabilities.backend_name[0..], "DISPBLIT"))
        {
            return self.headlessSubsystemFailure("present-backend-capabilities");
        }

        var absent_info: r4os.abi.RemoteFrameInfo = .{};
        if (self.ctx.remoteFrameInfo(&absent_info) == 0) return self.headlessSubsystemFailure("present-shadow-without-consumer");

        const narrow = surface.Rect{ .x = @max(0, self.screen_w - 1), .y = @max(0, self.screen_h - 1), .w = 1, .h = 1 };
        self.presentDamageRect(narrow, .mixed);
        if (self.render_stats.display_blit_calls_last != 1 or
            self.render_stats.layers_culled_last == 0 or
            self.render_stats.windows_culled_last == 0 or
            self.render_stats.items_culled_last == 0 or
            self.render_stats.remote_shadow_copies_last != 0)
        {
            return self.headlessSubsystemFailure("present-narrow-cull");
        }

        const separated = [_]surface.Rect{
            .{ .x = 2, .y = 2, .w = 2, .h = 2 },
            .{ .x = @max(4, self.screen_w - 4), .y = @max(4, self.screen_h - 4), .w = 2, .h = 2 },
        };
        const lost_before = self.render_stats.present_lost_frames;
        const fallback_before = self.render_stats.present_backend_fallbacks;
        const probe_input_tick = self.ctx.ticks();
        self.ctx.sleepTicks(1);
        self.presentDamageRegionsTimed(separated[0..], .mixed, probe_input_tick);
        if (self.render_stats.damage_regions_last != 2 or
            self.render_stats.last_damage_pixels != 8 or
            self.render_stats.display_blit_calls_last != 1 or
            self.render_stats.present_generation_last == 0 or
            self.render_stats.present_fence_last == 0 or
            self.render_stats.present_lost_frames != lost_before or
            self.render_stats.present_backend_fallbacks != fallback_before or
            self.render_stats.input_present_last_ticks == 0)
        {
            return self.headlessSubsystemFailure("present-separated-damage");
        }
        var completion: r4os.abi.DisplayPresentCompletion = .{};
        if (self.ctx.displayPresentCompletion(self.render_stats.present_fence_last, &completion) != 0 or
            (completion.flags & r4os.abi.display_present_completion_complete) == 0 or
            completion.completed_fence < self.render_stats.present_fence_last)
        {
            return self.headlessSubsystemFailure("present-fence-completion");
        }

        const cursor_blink = draw.terminalCursorRect(
            .{ .x = 0, .y = 0, .w = self.screen_w, .h = self.screen_h },
            .{ .cursor_x = 3, .cursor_y = 2, .cursor_visible = 1 },
            self.config.terminal_font_size,
        );
        if (cursor_blink.isEmpty()) return self.headlessSubsystemFailure("present-cursorblink-geometry");
        const cursor_lost_before = self.render_stats.present_lost_frames;
        const cursor_fallback_before = self.render_stats.present_backend_fallbacks;
        self.presentDamageRegionsTimed((&cursor_blink)[0..1], .cursor, self.ctx.ticks());
        if (self.render_stats.last_damage_kind != .cursor or
            self.render_stats.damage_regions_last != 1 or
            self.render_stats.last_damage_rect.x != cursor_blink.x or
            self.render_stats.last_damage_rect.y != cursor_blink.y or
            self.render_stats.last_damage_rect.w != cursor_blink.w or
            self.render_stats.last_damage_rect.h != cursor_blink.h or
            self.render_stats.last_damage_pixels != @as(u32, @intCast(rectArea(cursor_blink))) or
            self.render_stats.display_blit_calls_last != 1 or
            self.render_stats.present_generation_last == 0 or
            self.render_stats.present_fence_last == 0 or
            self.render_stats.present_lost_frames != cursor_lost_before or
            self.render_stats.present_backend_fallbacks != cursor_fallback_before)
        {
            return self.headlessSubsystemFailure("present-cursorblink-regional");
        }
        completion = .{};
        if (self.ctx.displayPresentCompletion(self.render_stats.present_fence_last, &completion) != 0 or
            (completion.flags & r4os.abi.display_present_completion_complete) == 0 or
            completion.completed_fence < self.render_stats.present_fence_last)
        {
            return self.headlessSubsystemFailure("present-cursorblink-fence");
        }

        const acquire_rc = self.ctx.remoteFrameAcquire();
        if (acquire_rc <= 0) return self.headlessSubsystemFailure("present-demand-acquire");
        self.remote_frame_consumers = @intCast(acquire_rc);
        self.invalidateFull();
        self.redraw();

        var ready_info: r4os.abi.RemoteFrameInfo = .{};
        if (self.render_stats.remote_shadow_copies_last == 0 or
            self.ctx.remoteFrameInfo(&ready_info) != 0 or
            ready_info.revision == 0 or
            ready_info.width != @as(u32, @intCast(self.screen_w)) or
            ready_info.height != @as(u32, @intCast(self.screen_h)))
        {
            _ = self.ctx.remoteFrameRelease();
            self.remote_frame_consumers = 0;
            return self.headlessSubsystemFailure("present-demand-frame");
        }

        const release_rc = self.ctx.remoteFrameRelease();
        self.remote_frame_consumers = if (release_rc > 0) @intCast(release_rc) else 0;
        if (release_rc != 0) return self.headlessSubsystemFailure("present-demand-release");
        self.presentDamageRect(narrow, .mixed);
        var released_info: r4os.abi.RemoteFrameInfo = .{};
        if (self.render_stats.remote_shadow_copies_last != 0 or self.ctx.remoteFrameInfo(&released_info) == 0) {
            return self.headlessSubsystemFailure("present-shadow-release");
        }
        return true;
    }

    fn smokeTextDamageContract(self: *App) bool {
        var font_id: u32 = r4os.abi.gui_font_builtin_id;
        var font_info: r4os.abi.GuiFontInfo = .{};
        var index: u32 = 1;
        while (index < self.ctx.draw.fontCount()) : (index += 1) {
            var candidate: r4os.abi.GuiFontInfo = .{};
            if (self.ctx.draw.fontInfo(index, &candidate) <= 0 or
                (candidate.flags & r4os.abi.gui_font_flag_renderable) == 0)
            {
                continue;
            }
            font_id = candidate.id;
            font_info = candidate;
            break;
        }
        if (font_id == r4os.abi.gui_font_builtin_id) return self.headlessSubsystemFailure("present-text-installed-font");

        const canvas_w: i32 = 64;
        const canvas_h: i32 = 64;
        const pixel_count: usize = @intCast(canvas_w * canvas_h);
        const allocator = self.ctx.allocator();
        const reference = allocator.alloc(u32, pixel_count) catch return self.headlessSubsystemFailure("present-text-reference-memory");
        defer allocator.free(reference);
        const actual = allocator.alloc(u32, pixel_count) catch return self.headlessSubsystemFailure("present-text-actual-memory");
        defer allocator.free(actual);

        const untouched: u32 = 0x0012_3456;
        @memset(reference, untouched);
        var reference_scene = scene_buffer.SceneBuffer{};
        if (!reference_scene.attach(std.mem.sliceAsBytes(reference), canvas_w, canvas_h)) return self.headlessSubsystemFailure("present-text-reference-scene");
        const bounds = surface.Rect{ .x = 0, .y = 0, .w = canvas_w, .h = canvas_h };
        paint.textFontSliceScene(&reference_scene, &self.ctx.draw, font_id, 8, 8, "A", 0x00FF_FFFF, 0, bounds);

        const cell_w: i32 = @intCast(@min(@max(@as(u32, 1), font_info.max_advance), @as(u32, 40)));
        const cell_h: i32 = @intCast(@min(@max(@max(@as(u32, 1), font_info.height), font_info.line_height), @as(u32, 40)));
        const clips = [_]surface.Rect{
            .{ .x = 8, .y = 8 + @divTrunc(cell_h, 2), .w = 1, .h = 1 },
            .{ .x = 8 + cell_w - 1, .y = 8 + @divTrunc(cell_h, 2), .w = 1, .h = 1 },
            .{ .x = 8 + @divTrunc(cell_w, 2), .y = 8, .w = 1, .h = 1 },
            .{ .x = 8 + @divTrunc(cell_w, 2), .y = 8 + cell_h - 1, .w = 1, .h = 1 },
        };
        for (clips) |clip| {
            @memset(actual, untouched);
            var scene = scene_buffer.SceneBuffer{};
            if (!scene.attach(std.mem.sliceAsBytes(actual), canvas_w, canvas_h)) return self.headlessSubsystemFailure("present-text-damage-scene");
            scene.setPaintClip(clip);
            paint.textFontSliceScene(&scene, &self.ctx.draw, font_id, 8, 8, "A", 0x00FF_FFFF, 0, clip);

            var y: i32 = 0;
            while (y < canvas_h) : (y += 1) {
                var x: i32 = 0;
                while (x < canvas_w) : (x += 1) {
                    const pixel_index: usize = @intCast(y * canvas_w + x);
                    const expected = if (clip.contains(x, y)) reference[pixel_index] else untouched;
                    if (actual[pixel_index] != expected) return self.headlessSubsystemFailure("present-text-glyph-edge");
                }
            }
        }
        return true;
    }

    fn launchHeadlessSubsystemGuest(self: *App, guest_path: []const u8) ?HeadlessSubsystemLaunch {
        const input = r4std.subsystem_runtime.probe(&self.ctx.sys, guest_path) catch return self.headlessSubsystemLaunchFailure("guest-probe");
        var args_storage: [r4os.subsystem_launch.max_args_bytes]u8 = undefined;
        var resolution: r4std.file_handler.Resolution = .{};
        r4std.file_handler.resolve(&self.assoc, r4std.subsystem_runtime.catalog(), input, args_storage[0..], &resolution) catch return self.headlessSubsystemLaunchFailure("handler-resolve");
        const target = resolution.target orelse return self.headlessSubsystemLaunchFailure("handler-missing");
        if (target.kind != .subsystem) return self.headlessSubsystemLaunchFailure("handler-kind");
        if (!equalsIgnoreCase(target.handler_id, "test.basic")) return self.headlessSubsystemLaunchFailure("handler-id");
        if (!equalsIgnoreCase(target.format_id, "basic.qbasic-source")) return self.headlessSubsystemLaunchFailure("guest-format");
        if (!equalsIgnoreCase(target.app_path, subsystem_host_test_path)) return self.headlessSubsystemLaunchFailure("host-path");
        if (!r4std.subsystem_runtime.hostPresent(&self.ctx.sys, target.app_path)) return self.headlessSubsystemLaunchFailure("host-missing");
        if (target.app_path.len > window_launch_path_max or target.args.len > r4os.subsystem_launch.max_args_bytes or target.title.len > console_title_max) return self.headlessSubsystemLaunchFailure("launch-limits");

        const request = r4os.subsystem_launch.parse(target.args) catch return self.headlessSubsystemLaunchFailure("launch-request");
        if (!equalsIgnoreCase(request.guest_path, guest_path)) return self.headlessSubsystemLaunchFailure("guest-identity");
        const index = self.findFreeAppWindow() orelse return self.headlessSubsystemLaunchFailure("window-slot");
        var path_z: [window_launch_path_max + 1]u8 = .{0} ** (window_launch_path_max + 1);
        var args_z: [r4os.subsystem_launch.max_args_bytes + 1]u8 = .{0} ** (r4os.subsystem_launch.max_args_bytes + 1);
        var title_z: [console_title_max + 1]u8 = .{0} ** (console_title_max + 1);
        copySliceZ(path_z[0..], target.app_path);
        copySliceZ(args_z[0..], target.args);
        copySliceZ(title_z[0..], target.title);
        self.launchGuiPath(zptr(path_z[0..]), zptr(args_z[0..]), zptr(title_z[0..]), target.policy);
        const handle = self.window_process_handles[index];
        if (!processHandleValid(handle)) return self.headlessSubsystemLaunchFailure("program-spawn");
        return .{ .index = index, .handle = handle };
    }

    fn launchHeadlessR4BasicBaseline(self: *App) ?HeadlessSubsystemLaunch {
        const start_ns = self.ctx.sys.monotonicNanoseconds() orelse return self.headlessSubsystemLaunchFailure("r4basic-clock");
        const inspection = r4std.subsystem_runtime.inspect(&self.ctx.sys, r4basic_baseline_guest_path) catch return self.headlessSubsystemLaunchFailure("r4basic-probe");
        const probe_end_ns = self.ctx.sys.monotonicNanoseconds() orelse return self.headlessSubsystemLaunchFailure("r4basic-probe-clock");
        const associations = [_]r4os.subsystem_catalog.Association{.{
            .extension = ".BAS",
            .subsystem_id = "r4os.basic",
            .format_id = "basic.qbasic-source",
        }};
        var resolution: r4os.subsystem_catalog.Resolution = .{};
        r4os.subsystem_catalog.resolve(r4std.subsystem_runtime.catalog(), inspection.input, .{ .user_associations = &associations }, &resolution) catch return self.headlessSubsystemLaunchFailure("r4basic-resolve");
        const target = resolution.selected() orelse return self.headlessSubsystemLaunchFailure("r4basic-handler");
        const resolve_end_ns = self.ctx.sys.monotonicNanoseconds() orelse return self.headlessSubsystemLaunchFailure("r4basic-resolve-clock");
        if (!equalsIgnoreCase(target.subsystem_id, "r4os.basic") or
            !equalsIgnoreCase(target.format_id, "basic.qbasic-source") or
            !equalsIgnoreCase(target.host_path, r4basic_host_path) or
            !r4std.subsystem_runtime.hostPresent(&self.ctx.sys, target.host_path))
        {
            return self.headlessSubsystemLaunchFailure("r4basic-target");
        }
        if (inspection.access.info_calls != 1 or inspection.access.read_calls != 0 or inspection.access.read_bytes != 0 or
            r4os.subsystem_catalog.max_probe_bytes != 256 * 1024)
        {
            return self.headlessSubsystemLaunchFailure("r4basic-source-probe");
        }

        var trace_id: [16]u8 = undefined;
        writeR4BasicTraceId(&trace_id, start_ns);
        var phases_storage: [64]u8 = undefined;
        const desktop_ns = self.ctx.sys.monotonicNanoseconds() orelse return self.headlessSubsystemLaunchFailure("r4basic-host-clock");
        const phases = std.fmt.bufPrint(phases_storage[0..], "{d},{d},{d}", .{
            probe_end_ns -| start_ns,
            resolve_end_ns -| start_ns,
            desktop_ns -| start_ns,
        }) catch return self.headlessSubsystemLaunchFailure("r4basic-timeline");
        const options = [_]r4os.subsystem_launch.Option{
            .{ .key = r4os.subsystem_launch.trace_key, .value = trace_id[0..] },
            .{ .key = r4os.subsystem_launch.trace_mode_key, .value = r4os.subsystem_launch.trace_mode_headless },
            .{ .key = r4os.subsystem_launch.trace_phases_key, .value = phases },
        };
        var args_storage: [r4os.subsystem_launch.max_args_bytes + 1]u8 = .{0} ** (r4os.subsystem_launch.max_args_bytes + 1);
        const args = r4os.subsystem_launch.encode(r4basic_baseline_guest_path, &options, args_storage[0..r4os.subsystem_launch.max_args_bytes]) catch return self.headlessSubsystemLaunchFailure("r4basic-args");
        const index = self.findFreeAppWindow() orelse return self.headlessSubsystemLaunchFailure("r4basic-window-slot");
        var path_z: [window_launch_path_max + 1]u8 = .{0} ** (window_launch_path_max + 1);
        var title_z: [console_title_max + 1]u8 = .{0} ** (console_title_max + 1);
        copySliceZ(path_z[0..], target.host_path);
        copySliceZ(title_z[0..], target.display_name);
        self.launchGuiPath(zptr(path_z[0..]), zptr(args_storage[0 .. args.len + 1]), zptr(title_z[0..]), .gui);
        const handle = self.window_process_handles[index];
        if (!processHandleValid(handle)) return self.headlessSubsystemLaunchFailure("r4basic-program-spawn");
        return .{ .index = index, .handle = handle };
    }

    fn launchHeadlessR4GbHostSelfTest(self: *App) ?HeadlessSubsystemLaunch {
        if (!r4std.subsystem_runtime.hostPresent(&self.ctx.sys, r4gb_host_path)) return self.headlessSubsystemLaunchFailure("r4gb-host-missing");
        const index = self.findFreeAppWindow() orelse return self.headlessSubsystemLaunchFailure("r4gb-window-slot");
        self.launchGuiPath(r4gb_host_path, "/HOSTTEST", "R4GB host selftest", .gui);
        const handle = self.window_process_handles[index];
        if (!processHandleValid(handle)) return self.headlessSubsystemLaunchFailure("r4gb-program-spawn");
        return .{ .index = index, .handle = handle };
    }

    fn smokeR4GbProductPath(self: *App) bool {
        const first = self.launchHeadlessR4GbGuest(r4gb_fixture_a_path, r4gb_trace_a, "witness") orelse return false;
        const second = self.launchHeadlessR4GbGuest(r4gb_fixture_b_path, r4gb_trace_b, "close") orelse return false;
        if (first.index == second.index or sameProcessHandle(first.handle, second.handle)) return self.headlessSubsystemFailure("r4gb-instance-identity");

        self.toggleMaximizeWindow(first.index);
        self.smokePumpCooperativeFrames(1);
        if (sameProcessHandle(self.window_process_handles[first.index], first.handle)) self.toggleMaximizeWindow(first.index);
        self.activateWindow(first.index, false);
        if (!self.pushGuiPhysicalKeyEvent(first.index, r4os.abi.physical_key_usage_right, true) or
            !self.pushGuiPhysicalKeyEvent(first.index, r4os.abi.physical_key_usage_right, false) or
            !self.pushGuiPhysicalKeyEvent(first.index, 0x3E, true))
        {
            return self.headlessSubsystemFailure("r4gb-input-a");
        }
        self.ctx.sleepTicks(ticksFromMs(self.monotonic_hz, 20));
        if (!self.pushGuiPhysicalKeyEvent(first.index, 0x3F, true) or
            !self.pushGuiPhysicalKeyEvent(first.index, 0x41, true)) return self.headlessSubsystemFailure("r4gb-controls-a");

        self.activateWindow(second.index, false);
        if (!self.pushGuiPhysicalKeyEvent(second.index, r4os.abi.physical_key_usage_space, true) or
            !self.pushGuiPhysicalKeyEvent(second.index, r4os.abi.physical_key_usage_space, false) or
            !self.pushGuiPhysicalKeyEvent(second.index, 0x42, true))
        {
            return self.headlessSubsystemFailure("r4gb-input-b");
        }
        self.ctx.sleepTicks(ticksFromMs(self.monotonic_hz, 20));
        if (!self.pushGuiPhysicalKeyEvent(second.index, 0x43, true)) return self.headlessSubsystemFailure("r4gb-controls-b");

        const exercise_started = self.ctx.ticks();
        const exercise_ticks = @as(u64, @max(self.monotonic_hz, 1)) * 60;
        const focus_interval = @as(u64, @max(self.monotonic_hz, 1)) * 10;
        var next_focus = focus_interval;
        var focus_round: u32 = 0;
        const redraws_before = self.render_stats.redraws;
        const lost_frames_before = self.render_stats.present_lost_frames;
        const present_failures_before = self.render_stats.present_failures;
        while (self.ctx.ticks() -| exercise_started < exercise_ticks) {
            if (!sameProcessHandle(self.window_process_handles[first.index], first.handle) or
                !sameProcessHandle(self.window_process_handles[second.index], second.handle))
            {
                return self.headlessSubsystemFailure("r4gb-early-exit");
            }
            const elapsed = self.ctx.ticks() -| exercise_started;
            if (elapsed >= next_focus) {
                const target = if ((focus_round & 1) == 0) first.index else second.index;
                const usage = if ((focus_round & 1) == 0) r4os.abi.physical_key_usage_up else r4os.abi.physical_key_usage_space;
                self.activateWindow(target, false);
                if (!self.pushGuiPhysicalKeyEvent(target, usage, true) or !self.pushGuiPhysicalKeyEvent(target, usage, false)) {
                    return self.headlessSubsystemFailure("r4gb-longrun-input");
                }
                focus_round +%= 1;
                next_focus +|= focus_interval;
            }
            self.smokePumpCooperativeFrames(1);
            self.ctx.sleepTicks(1);
        }
        if (focus_round < 5 or self.render_stats.redraws <= redraws_before or
            self.render_stats.present_lost_frames != lost_frames_before or
            self.render_stats.present_failures != present_failures_before)
        {
            return self.headlessSubsystemFailure("r4gb-longrun-desktop");
        }

        // The first original fixture ends from a guest-authored Start+Select
        // witness. This exercises the natural completion path independently
        // from the explicit close used for the second instance.
        self.activateWindow(first.index, false);
        if (!self.pushGuiPhysicalKeyEvent(first.index, r4os.abi.physical_key_usage_enter, true) or
            !self.pushGuiPhysicalKeyEvent(first.index, r4os.abi.physical_key_usage_right_control, true))
        {
            return self.headlessSubsystemFailure("r4gb-witness-input");
        }
        const first_started = self.ctx.ticks();
        const close_timeout_ticks = @as(u64, @max(self.monotonic_hz, 1)) * 30;
        while (sameProcessHandle(self.window_process_handles[first.index], first.handle)) {
            self.smokePumpCooperativeFrames(1);
            if (self.ctx.ticks() -| first_started >= close_timeout_ticks) return self.headlessSubsystemFailure("r4gb-witness-a-timeout");
        }
        if (!sameProcessHandle(self.window_process_handles[second.index], second.handle)) return self.headlessSubsystemFailure("r4gb-close-isolation");
        if (!sameProcessHandle(self.window_completion_handles[first.index], first.handle)) return self.headlessSubsystemFailure("r4gb-completion-a-handle");
        if (self.window_completion_exit_codes[first.index] != 0) return self.headlessSubsystemFailure("r4gb-completion-a-exit");
        if (!self.ctx.exists(r4gb_report_a_path)) return self.headlessSubsystemFailure("r4gb-completion-a-report");

        self.pushGuiEvent(second.index, .close);
        const second_started = self.ctx.ticks();
        while (sameProcessHandle(self.window_process_handles[second.index], second.handle)) {
            self.smokePumpCooperativeFrames(1);
            if (self.ctx.ticks() -| second_started >= close_timeout_ticks) return self.headlessSubsystemFailure("r4gb-close-b-timeout");
        }
        if (!sameProcessHandle(self.window_completion_handles[second.index], second.handle)) return self.headlessSubsystemFailure("r4gb-completion-b-handle");
        if (self.window_completion_exit_codes[second.index] != 0) return self.headlessSubsystemFailure("r4gb-completion-b-exit");
        if (!self.ctx.exists(r4gb_report_b_path)) return self.headlessSubsystemFailure("r4gb-completion-b-report");

        const rejected = self.launchHeadlessR4GbGuest(r4gb_fixture_cgb_path, r4gb_trace_cgb, "reject") orelse return false;
        const rejection_ready_started = self.ctx.ticks();
        const rejection_ready_timeout = @as(u64, @max(self.monotonic_hz, 1)) * 10;
        while (!std.mem.startsWith(u8, spanZPtr(self.windows[rejected.index].title()), "R4GB - Cartridgefehler") or
            !self.gui_frame_caches[rejected.index].view().valid)
        {
            if (!sameProcessHandle(self.window_process_handles[rejected.index], rejected.handle)) return self.headlessSubsystemFailure("r4gb-rejection-early-exit");
            self.smokePumpCooperativeFrames(1);
            if (self.ctx.ticks() -| rejection_ready_started >= rejection_ready_timeout) return self.headlessSubsystemFailure("r4gb-rejection-not-visible");
            self.ctx.sleepTicks(1);
        }
        const rejection_visible = self.ctx.ticks();
        const visibility_ticks = ticksFromMs(self.monotonic_hz, 100);
        while (self.ctx.ticks() -| rejection_visible < visibility_ticks) {
            if (!sameProcessHandle(self.window_process_handles[rejected.index], rejected.handle)) return self.headlessSubsystemFailure("r4gb-rejection-early-exit");
            self.smokePumpCooperativeFrames(1);
            self.ctx.sleepTicks(1);
        }
        if (self.requestWindowProcessClose(rejected.index) != r4os.abi.program_handle_ok) return self.headlessSubsystemFailure("r4gb-rejection-close-request");
        // Mark the close as user-requested without arming Desktop's 200 ms
        // forced-kill timer; the test below owns its longer bounded timeout.
        self.windows[rejected.index].requestClose(0);
        self.pushGuiEvent(rejected.index, .close);
        const rejection_started = self.ctx.ticks();
        while (sameProcessHandle(self.window_process_handles[rejected.index], rejected.handle)) {
            self.smokePumpCooperativeFrames(1);
            if (self.ctx.ticks() -| rejection_started >= close_timeout_ticks) return self.headlessSubsystemFailure("r4gb-rejection-timeout");
        }
        if (!sameProcessHandle(self.window_completion_handles[rejected.index], rejected.handle) or
            self.window_completion_exit_codes[rejected.index] != r4gb_cgb_rejection_exit or
            !self.ctx.exists(r4gb_report_cgb_path))
        {
            return self.headlessSubsystemFailure("r4gb-rejection-result");
        }
        return true;
    }

    fn launchHeadlessR4GbGuest(self: *App, guest_path: []const u8, trace_id: []const u8, expected_end: []const u8) ?HeadlessSubsystemLaunch {
        var inspection = r4std.subsystem_runtime.inspect(&self.ctx.sys, guest_path) catch return self.headlessSubsystemLaunchFailure("r4gb-inspect");
        const metadata_input = inspection.input;
        const probed_input = r4std.subsystem_runtime.completeProbe(&self.ctx.sys, metadata_input, &inspection.access) catch return self.headlessSubsystemLaunchFailure("r4gb-probe");
        if (inspection.access.info_calls != 1 or inspection.access.read_calls == 0 or
            inspection.access.read_bytes == 0 or inspection.access.read_bytes > r4os.subsystem_catalog.max_probe_bytes)
        {
            return self.headlessSubsystemLaunchFailure("r4gb-probe-bounds");
        }
        var probed: r4os.subsystem_catalog.Resolution = .{};
        r4os.subsystem_catalog.resolve(r4std.subsystem_runtime.catalog(), probed_input, .{}, &probed) catch return self.headlessSubsystemLaunchFailure("r4gb-probe-resolve");
        const probe_target = probed.selected() orelse return self.headlessSubsystemLaunchFailure("r4gb-probe-target");
        if (!equalsIgnoreCase(probe_target.subsystem_id, "r4os.gb") or
            !equalsIgnoreCase(probe_target.format_id, "gameboy.dmg-cartridge") or
            probe_target.evidence != .confirmed_probe)
        {
            return self.headlessSubsystemLaunchFailure("r4gb-probe-identity");
        }

        var args_storage: [r4os.subsystem_launch.max_args_bytes]u8 = undefined;
        var resolution: r4std.file_handler.Resolution = .{};
        r4std.file_handler.resolve(&self.assoc, r4std.subsystem_runtime.catalog(), metadata_input, args_storage[0..], &resolution) catch return self.headlessSubsystemLaunchFailure("r4gb-assoc-resolve");
        const target = resolution.target orelse return self.headlessSubsystemLaunchFailure("r4gb-assoc-target");
        if (target.kind != .subsystem or !equalsIgnoreCase(target.handler_id, "r4os.gb") or
            !equalsIgnoreCase(target.format_id, "gameboy.dmg-cartridge") or
            !equalsIgnoreCase(target.app_path, r4gb_host_path))
        {
            return self.headlessSubsystemLaunchFailure("r4gb-assoc-identity");
        }
        const request = r4os.subsystem_launch.parse(target.args) catch return self.headlessSubsystemLaunchFailure("r4gb-request");
        if (!equalsIgnoreCase(request.guest_path, guest_path)) return self.headlessSubsystemLaunchFailure("r4gb-guest-identity");

        var choices: r4std.file_handler.ChoiceList = .{};
        var choice_access: r4std.subsystem_runtime.AccessStats = .{};
        r4std.file_handler.collectPathChoices(&self.ctx.sys, &self.assoc, r4std.subsystem_runtime.catalog(), guest_path, &choices, &choice_access) catch return self.headlessSubsystemLaunchFailure("r4gb-open-with");
        if (choice_access.info_calls != 1 or choice_access.read_calls != 0 or choice_access.read_bytes != 0) return self.headlessSubsystemLaunchFailure("r4gb-open-with-probe");
        var found = false;
        for (choices.slice()) |choice| if (choice.kind == .subsystem and
            equalsIgnoreCase(choice.handler_id, "r4os.gb") and equalsIgnoreCase(choice.format_id, "gameboy.dmg-cartridge"))
        {
            found = true;
        };
        if (!found) return self.headlessSubsystemLaunchFailure("r4gb-open-with-choice");

        const options = [_]r4os.subsystem_launch.Option{
            .{ .key = r4os.subsystem_launch.trace_key, .value = trace_id },
            .{ .key = r4os.subsystem_launch.trace_mode_key, .value = r4os.subsystem_launch.trace_mode_headless },
            .{ .key = r4gb_trace_end_key, .value = expected_end },
        };
        var traced_storage: [r4os.subsystem_launch.max_args_bytes + 1]u8 = .{0} ** (r4os.subsystem_launch.max_args_bytes + 1);
        const traced = r4os.subsystem_launch.encode(guest_path, &options, traced_storage[0..r4os.subsystem_launch.max_args_bytes]) catch return self.headlessSubsystemLaunchFailure("r4gb-trace-args");
        const index = self.findFreeAppWindow() orelse return self.headlessSubsystemLaunchFailure("r4gb-window-slot");
        var path_z: [window_launch_path_max + 1]u8 = .{0} ** (window_launch_path_max + 1);
        var title_z: [console_title_max + 1]u8 = .{0} ** (console_title_max + 1);
        copySliceZ(path_z[0..], target.app_path);
        copySliceZ(title_z[0..], target.title);
        self.launchGuiPath(zptr(path_z[0..]), zptr(traced_storage[0 .. traced.len + 1]), zptr(title_z[0..]), .gui);
        const handle = self.window_process_handles[index];
        if (!processHandleValid(handle)) return self.headlessSubsystemLaunchFailure("r4gb-product-spawn");
        return .{ .index = index, .handle = handle };
    }

    fn headlessSubsystemFailure(self: *App, comptime reason: []const u8) bool {
        if (self.ctx.exists(subsystem_host_test_marker_path)) return false;
        const marker = "SUBSYSTEM runtime bootstrap FAILED: " ++ reason ++ "\r\n";
        _ = self.ctx.fileWrite(subsystem_host_test_marker_path, marker);
        return false;
    }

    fn headlessSubsystemLaunchFailure(self: *App, comptime reason: []const u8) ?HeadlessSubsystemLaunch {
        _ = self.headlessSubsystemFailure(reason);
        return null;
    }

    fn waitForHeadlessSubsystemAudio(self: *App) bool {
        const started = self.ctx.ticks();
        const timeout_ticks = @as(u64, @max(self.monotonic_hz, 1)) * 5;
        while (self.ctx.ticks() -| started < timeout_ticks) {
            var info: r4os.abi.ServiceInfo = .{};
            if (self.ctx.sys.serviceOpen(subsystem_audio_service, &info) == r4os.abi.service_api_result_ok and info.handle != 0) {
                _ = self.ctx.sys.serviceClose(info.handle);
                return true;
            }
            self.ctx.sleepTicks(1);
        }
        return false;
    }

    fn initTiming(self: *App) void {
        const state = self.ctx.timeState();
        self.monotonic_hz = if (state.monotonic_hz == 0) default_monotonic_hz else state.monotonic_hz;
        self.loop_sleep_ticks = ticksFromMs(self.monotonic_hz, loop_sleep_ms);
        self.double_click_ticks = ticksFromMs(self.monotonic_hz, double_click_ms);
        self.blink_half_ticks = ticksFromMs(self.monotonic_hz, blink_half_ms);
        self.close_kill_timeout_ticks = ticksFromMs(self.monotonic_hz, close_kill_timeout_ms);
        self.time_config_check_ticks = ticksFromMs(self.monotonic_hz, time_config_check_ms);
        self.window_service_retry_ticks = ticksFromMs(self.monotonic_hz, window_service_retry_ms);
        self.tray_service_retry_ticks = ticksFromMs(self.monotonic_hz, tray_service_retry_ms);
        self.tray_sync_ticks = ticksFromMs(self.monotonic_hz, tray_sync_ms);
        self.volume_poll_ticks = ticksFromMs(self.monotonic_hz, volume_poll_ms);
        self.volume_send_ticks = ticksFromMs(self.monotonic_hz, volume_send_ms);
        self.desktop_layout_writeback.configure(r4std.settings.WritebackPolicy.forHz(.lazy, self.monotonic_hz), self.ctx.ticks());
    }

    fn initWindowTitles(self: *App) void {
        self.windows[0].setTitleLit("Terminal");
        self.windows[1].setTitleLit("Desktop Manager");
        self.windows[2].setTitleLit("R4X App");
        self.windows[3].setTitleLit("R4X App");
    }

    fn beginEvent(self: *App, kind: EventKind) void {
        self.event_kind = kind;
        self.event.reset();
        self.event_tick = self.ctx.ticks();
        self.event_key = 0;
        self.event_remote_input = false;
    }

    fn startStructuredEvent(self: *App, kind: model.EventKind, source: model.EventSource) void {
        self.next_event_id +%= 1;
        if (self.next_event_id == 0) self.next_event_id = 1;
        self.event.id = self.next_event_id;
        self.event.kind = kind;
        self.event.tick = self.event_tick;
        self.event.source = source;
        if (self.captureOwner()) |owner| self.event.setCapture(owner);
    }

    fn pollKeyboardEvent(self: *App) bool {
        const key = self.ctx.readKeyCodepoint();
        if (key == 0) return false;
        self.beginEvent(.keyboard);
        self.event_key = key;
        self.startStructuredEvent(.keyboard, keyboardSource(key));
        self.event.key = key;
        self.event.modifiers = modifiersForKey(key);
        self.last_key_tick = self.event_tick;
        self.key_repeat_armed = true;
        return true;
    }

    fn pollPhysicalKeyEvent(self: *App) bool {
        var input: r4os.abi.PhysicalKeyEvent = .{};
        const result = self.ctx.physicalKeyPoll(&input);
        if (result <= 0) return false;
        self.physical_input_events +%= 1;
        if (!physical_input.valid(input, self.last_physical_input_sequence)) {
            self.physical_input_invalid +%= 1;
            return true;
        }
        self.last_physical_input_sequence = input.sequence;
        if (self.forwardPhysicalKeyToActiveApp(input)) self.physical_input_forwarded +%= 1;
        return true;
    }

    fn pollMouseEvent(self: *App) bool {
        self.beginEvent(.mouse);
        self.ctx.mouseState(&self.event_mouse);
        return self.prepareMouseEvent(.mouse);
    }

    fn pollRemoteInputEvent(self: *App) bool {
        var input: r4os.abi.RemoteInputEvent = .{};
        const rc = self.ctx.remoteInputPoll(&input);
        if (rc <= 0) return false;
        if (input.magic != r4os.abi.remote_input_magic or input.version != r4os.abi.remote_input_version) {
            self.beginEvent(.none);
            return true;
        }

        self.remote_input_events +%= 1;
        self.last_remote_input_sequence = input.sequence;

        if (input.kind == r4os.abi.remote_input_kind_key_down) {
            self.remote_input_keys +%= 1;
            const key = remoteInputKey(input.key) orelse {
                self.beginEvent(.none);
                return true;
            };
            self.beginEvent(.keyboard);
            self.event_key = key;
            self.startStructuredEvent(.keyboard, remoteKeyboardSource(input, key));
            self.event.key = key;
            self.event.modifiers = remoteModifiers(input.modifiers) | modifiersForKey(key);
            self.last_key_tick = self.event_tick;
            self.key_repeat_armed = true;
            return true;
        }

        if (input.kind == r4os.abi.remote_input_kind_key_up) {
            self.remote_input_keys +%= 1;
            self.beginEvent(.none);
            return true;
        }

        if (input.kind == r4os.abi.remote_input_kind_mouse_move or
            input.kind == r4os.abi.remote_input_kind_mouse_buttons or
            input.kind == r4os.abi.remote_input_kind_mouse_wheel)
        {
            self.remote_input_mouse +%= 1;
            self.beginEvent(.mouse);
            self.event_remote_input = true;
            self.event_mouse = .{
                .x = input.x,
                .y = input.y,
                .dx = input.x - self.cursor_x,
                .dy = input.y - self.cursor_y,
                .wheel = input.wheel,
                .buttons = @intCast(input.buttons & 0xff),
                .present = 1,
                .reserved = 0,
                .packets = input.sequence,
            };
            return self.prepareMouseEvent(.mouse);
        }

        self.beginEvent(.none);
        return true;
    }

    fn prepareMouseEvent(self: *App, source: model.EventSource) bool {
        const previous_buttons = self.inputPreviousButtons();
        const right_down = (self.event_mouse.buttons & model.MouseButton.right) != 0;
        const right_was_down = (previous_buttons & model.MouseButton.right) != 0;
        if (right_down and !right_was_down) {
            self.startStructuredEvent(.mouse_down, source);
            self.event.mouse_x = self.event_mouse.x;
            self.event.mouse_y = self.event_mouse.y;
            self.event.button = model.MouseButton.right;
            const target = self.targetAt(self.event_mouse.x, self.event_mouse.y);
            if (target == .taskbar_tray_external) {
                self.tray_right_pressed_identity = self.tray_registry.hit(self.event_mouse.x, self.event_mouse.y) orelse .{};
                self.tray_pressed_identity = self.tray_right_pressed_identity;
            } else {
                self.tray_right_pressed_identity = .{};
            }
            if (target != .none) self.event.setTarget(target);
            return true;
        } else if (!right_down and right_was_down) {
            self.startStructuredEvent(.mouse_up, source);
            self.event.mouse_x = self.event_mouse.x;
            self.event.mouse_y = self.event_mouse.y;
            self.event.button = model.MouseButton.right;
            const target = self.targetAt(self.event_mouse.x, self.event_mouse.y);
            if (target != .none) self.event.setTarget(target);
            return true;
        }

        const down = (self.event_mouse.buttons & model.MouseButton.left) != 0;
        const was_down = (previous_buttons & model.MouseButton.left) != 0;
        if (down and !was_down) {
            self.startStructuredEvent(.mouse_down, source);
            self.event.mouse_x = self.event_mouse.x;
            self.event.mouse_y = self.event_mouse.y;
            self.event.button = model.MouseButton.left;
            const target = self.targetAt(self.event_mouse.x, self.event_mouse.y);
            const tray_identity = if (target == .taskbar_tray_external)
                self.tray_registry.hit(self.event_mouse.x, self.event_mouse.y) orelse tray.Identity{}
            else
                tray.Identity{};
            self.double_click_pending = self.last_mouse_down_tick != 0 and
                self.event_tick >= self.last_mouse_down_tick and
                self.event_tick - self.last_mouse_down_tick <= self.double_click_ticks and
                target != .none and
                target == self.last_mouse_down_target and
                (target != .taskbar_tray_external or tray_identity.eql(self.last_mouse_down_tray_identity));
            self.event.click_count = if (self.double_click_pending) 2 else 1;
            self.last_mouse_down_tick = self.event_tick;
            self.last_mouse_down_target = target;
            self.last_mouse_down_tray_identity = tray_identity;
            self.tray_pressed_identity = tray_identity;
            if (target != .none) self.event.setTarget(target);
        } else if (!down and was_down) {
            self.startStructuredEvent(.mouse_up, source);
            self.event.mouse_x = self.event_mouse.x;
            self.event.mouse_y = self.event_mouse.y;
            self.event.button = model.MouseButton.left;
            const target = self.targetAt(self.event_mouse.x, self.event_mouse.y);
            if (target != .none) self.event.setTarget(target);
        }
        return true;
    }

    fn pollTimerEvent(self: *App) bool {
        self.beginEvent(.timer);
        const program_changed = self.syncProgramWindows();
        const host_launch_changed = self.syncHostLaunchRequests();
        const gui_changed = self.syncGuiRevisions();
        const console_changed = self.syncConsoleRevision();
        self.refreshConsoleSnapshots();
        const display_changed = self.syncDisplayRevision();
        const keyboard_layout_changed = self.updateKeyboardLayout();
        const volume_changed = self.syncVolume();
        const time_config_changed = self.syncTimeConfig();
        const desktop_layout_writeback = self.flushDesktopLayoutIfDue();
        const clock_changed = self.updateClock();
        const next_blink: u8 = @intCast((self.event_tick / self.blink_half_ticks) & 1);
        const blink_changed = next_blink != self.blink_phase;
        self.blink_phase = next_blink;
        const cursor_blink_changed = blink_changed and self.invalidateConsoleCursors();
        if (!program_changed and !host_launch_changed and !gui_changed and !console_changed and !display_changed and !keyboard_layout_changed and !volume_changed and !time_config_changed and !desktop_layout_writeback and !clock_changed and !cursor_blink_changed) return false;
        self.startStructuredEvent(.timer, .timer);
        return true;
    }

    fn dispatchEvent(self: *App) bool {
        const handled = switch (self.event_kind) {
            .none => false,
            .keyboard => self.handleKeyboardEvent(self.event_key),
            .mouse => self.handleMouseEvent(self.event_mouse),
            .timer => self.hasDamage(),
        };
        if (handled and self.event.id != 0) self.event.consume();
        return handled;
    }

    fn handleKeyboardEvent(self: *App, key: u32) bool {
        const legacy_key = legacyKey(key);
        if (legacy_key == r4os.gui.Key.ctrl_c and self.terminalCloseHotkeyApplies()) {
            return self.requestTerminalProgramsClose();
        }
        if (self.terminal_mode) {
            if (legacy_key != 0 and self.handleTerminalScrollKey(legacy_key)) return true;
            _ = self.deliverTerminalKeyToActiveConsole(key);
            return self.hasDamage();
        }
        if (self.volume_ui.popup_open and legacy_key != 0 and self.handleVolumeKey(legacy_key)) return true;
        if (legacy_key != 0 and self.dialog == .run and self.handleRunKey(legacy_key)) return true;
        if (legacy_key != 0 and self.dialog == .tasks and self.handleTaskInventoryKey(legacy_key)) return true;
        if (legacy_key != 0 and self.dialog == .none and !self.start_open and self.handleTerminalScrollKey(legacy_key)) return true;
        if (self.dialog == .none and !self.start_open and self.deliverTerminalKeyToActiveConsole(key)) return true;
        if (self.dialog == .none and !self.start_open and self.deliverGuiKeyToActiveApp(key)) return true;

        switch (legacy_key) {
            0x1B => self.dispatchCommand(self.closeCommand(), .keyboard),
            0x80 => self.menuUp(),
            0x81 => self.menuDown(),
            0x88 => self.menuLeft(),
            0x89 => self.menuRight(),
            0x83, r4os.gui.Key.start_menu => self.dispatchCommand(.start_button, .hotkey),
            0x84 => self.previousDialogFocus(),
            '\t' => self.nextDialogFocus(),
            0x85 => self.nextWindow(),
            0x86 => self.altF4(),
            0x87 => self.previousWindow(),
            '\r', '\n' => self.dispatchCommand(self.defaultCommand(), .enter),
            ' ' => if (self.dialog != .none) self.dispatchCommand(self.defaultCommand(), .keyboard) else self.dispatchCommand(.start_button, .keyboard),
            'r', 'R' => self.dispatchCommand(.menu_run, .hotkey),
            't', 'T' => self.dispatchCommand(.menu_tasks, .hotkey),
            'd', 'D' => self.dispatchCommand(.menu_terminal, .hotkey),
            else => return false,
        }
        if (!self.hasDamage()) self.invalidateFull();
        return self.hasDamage();
    }

    fn handleMouseEvent(self: *App, mouse: r4os.abi.Mouse) bool {
        if (mouse.wheel != 0) {
            self.setInputPreviousButtons(mouse.buttons);
            if (self.handleMouseWheel(mouse.x, mouse.y, mouse.wheel)) return true;
            return self.hasDamage();
        }
        if (self.terminal_mode) {
            self.setInputPreviousButtons(mouse.buttons);
            return self.hasDamage();
        }
        const cursor_moved = self.updateCursor(mouse.x, mouse.y);
        self.updateHover(mouse.x, mouse.y);
        if (self.event.button == model.MouseButton.right) {
            self.setInputPreviousButtons(mouse.buttons);
            if (self.event.target == .taskbar_tray_external or self.tray_right_pressed_identity.valid()) {
                if (self.event.kind == .mouse_up) {
                    const released = self.tray_registry.hit(mouse.x, mouse.y) orelse tray.Identity{};
                    if (released.eql(self.tray_right_pressed_identity)) {
                        _ = self.submitTrayActivation(released, r4os.abi.tray_event_kind_context, 0, mouse.x, mouse.y, self.event_tick);
                    }
                    self.tray_right_pressed_identity = .{};
                    self.tray_pressed_identity = .{};
                }
                self.invalidateTaskbar();
                return self.hasDamage();
            }
            if (self.event.kind == .mouse_down) {
                if (self.time_menu_open) {
                    self.closeTop();
                    return true;
                }
                if (self.event.target == .taskbar_clock or self.event.target == .time_menu_clock or self.event.target == .time_menu_settings) {
                    self.openTimeMenu();
                    return true;
                }
                if (self.openSystemMenuForTarget(self.event.target, mouse.x, mouse.y)) return true;
            }
            return self.hasDamage();
        }
        const down = (mouse.buttons & 1) != 0;
        const previous_buttons = self.inputPreviousButtons();
        if (cursor_moved and self.dialog == .none and !self.start_open and !self.drag.active and !self.desktop_drag.pending and !self.desktop_drag.active and !self.resize.active) {
            self.deliverGuiMouseMove(mouse.x, mouse.y, mouse.buttons);
        }
        if (!down) {
            const was_down = (previous_buttons & 1) != 0;
            const pressed_target = self.mouse_down_target;
            const was_drag = self.drag.active;
            const drag_index = self.drag.window_index;
            const was_desktop_drag = self.desktop_drag.active;
            const desktop_drag_index = self.desktop_drag.index;
            const was_resize = self.resize.active;
            const resize_index = self.resize.window_index;
            if (was_down and pressed_target == .volume_popup_slider and self.volume_dragging) {
                _ = self.updateVolumeFromPointer(mouse.x, true);
                self.volume_dragging = false;
            }
            if (was_desktop_drag) _ = self.finishDesktopItemDrag();
            if (was_down and pressed_target != .none and !was_desktop_drag) {
                if (pressed_target == .taskbar_tray_external) {
                    const released = self.tray_registry.hit(mouse.x, mouse.y) orelse tray.Identity{};
                    if (released.eql(self.tray_pressed_identity)) {
                        const kind: u16 = if (self.double_click_pending) r4os.abi.tray_event_kind_double else r4os.abi.tray_event_kind_primary;
                        _ = self.submitTrayActivation(released, kind, 0, mouse.x, mouse.y, self.event_tick);
                    }
                } else {
                    self.deliverGuiMouseButton(.mouse_up, pressed_target, mouse.x, mouse.y, mouse.buttons);
                    if (pressed_target == self.event.target) self.dispatchMouseCommand(pressed_target);
                }
            }
            self.setInputPreviousButtons(mouse.buttons);
            self.drag.active = false;
            self.desktop_drag = .{};
            self.resize.active = false;
            self.mouse_down_target = .none;
            self.tray_pressed_identity = .{};
            if (was_down) self.invalidatePointerRelease(pressed_target, was_drag, drag_index, was_desktop_drag, desktop_drag_index, was_resize, resize_index);
            return self.hasDamage();
        }
        if ((previous_buttons & 1) != 0) {
            if (self.volume_dragging and self.mouse_down_target == .volume_popup_slider) {
                _ = self.updateVolumeFromPointer(mouse.x, false);
            } else if (self.resize.active) {
                _ = self.updateResize(mouse.x, mouse.y);
            } else if (self.drag.active) {
                _ = self.updateDrag(mouse.x, mouse.y);
            } else if (self.desktop_drag.pending or self.desktop_drag.active) {
                _ = self.updateDesktopItemDrag(mouse.x, mouse.y);
            } else {
                self.deliverGuiMouseMoveToTarget(self.mouse_down_target, mouse.x, mouse.y, mouse.buttons);
            }
            return self.hasDamage();
        }
        self.setInputPreviousButtons(mouse.buttons);
        self.mouse_down_target = self.event.target;
        self.dispatchMouseDown(self.event.target, mouse.x, mouse.y);
        return self.hasDamage();
    }

    fn updateHover(self: *App, x: i32, y: i32) void {
        const target = self.targetAt(x, y);
        const next_tray_identity = if (target == .taskbar_tray_external)
            self.tray_registry.hit(x, y) orelse tray.Identity{}
        else
            tray.Identity{};
        if (target == self.hover_target and next_tray_identity.eql(self.tray_hover_identity)) return;
        const old_target = self.hover_target;
        const old_tooltip = self.currentTrayTooltipRect();
        const old_volume_tooltip = self.currentVolumeTooltipRect();
        self.hover_target = target;
        self.tray_hover_identity = next_tray_identity;
        if (self.start_open) {
            if (self.nestedIndexForTarget(target)) |hit| {
                self.menu_submenu_open = true;
                self.menu_submenu_parent = hit.parent;
                self.menu_submenu_selected = hit.child;
                self.menu_submenu_focus = false;
                self.menu_nested_open = true;
                self.menu_nested_parent = hit.child;
                self.menu_nested_selected = hit.index;
                self.menu_nested_focus = true;
            } else if (self.submenuIndexForTarget(target)) |hit| {
                self.menu_submenu_open = true;
                self.menu_submenu_parent = hit.parent;
                self.menu_submenu_selected = hit.index;
                self.menu_submenu_focus = true;
                self.menu_nested_focus = false;
                if (self.menu.submenuHasSubmenu(hit.parent, hit.index)) {
                    self.menu_nested_open = true;
                    self.menu_nested_parent = hit.index;
                    self.menu_nested_selected = 0;
                } else {
                    self.menu_nested_open = false;
                }
            } else if (self.menuIndexForTarget(target)) |index| {
                self.menu_selected = index;
                self.menu_submenu_focus = false;
                self.menu_nested_focus = false;
                self.menu_nested_open = false;
                if (self.menu.hasSubmenu(index)) {
                    self.menu_submenu_open = true;
                    self.menu_submenu_parent = index;
                    self.menu_submenu_selected = 0;
                } else {
                    self.menu_submenu_open = false;
                }
            }
        }
        self.invalidateHoverChange(old_target, target);
        if (old_tooltip) |rect| self.damage.invalidate(rect);
        if (self.currentTrayTooltipRect()) |rect| self.damage.invalidate(rect);
        if (old_volume_tooltip) |rect| self.damage.invalidate(rect);
        if (self.currentVolumeTooltipRect()) |rect| self.damage.invalidate(rect);
    }

    fn dispatchMouseDown(self: *App, target: model.UiTarget, x: i32, y: i32) void {
        if (target == .none) return;
        self.activateEventCommand(target, .mouse);
        if (target == .volume_popup_slider) {
            self.volume_dragging = self.volume_ui.reachable;
            _ = self.updateVolumeFromPointer(x, false);
            self.invalidateVolumePopup();
            return;
        }
        if (self.time_menu_open) {
            self.invalidateTimeMenu();
            return;
        }
        if (self.system_menu_open) {
            self.invalidateSystemMenu();
            return;
        }
        if (desktop_items.indexForTarget(target)) |index| {
            self.desktop_item_selected = index;
            if (self.event.click_count >= 2) {
                self.desktop_drag = .{};
                self.activateDesktopItem(index);
            } else {
                self.beginDesktopItemDrag(index, x, y);
            }
        } else {
            switch (target) {
                .run_input, .run_browse, .run_ok, .run_cancel => self.setDialogFocus(target),
                .message_ok, .message_yes, .message_no, .task_overview_ok, .settings_ok, .settings_cancel => self.setDialogFocus(target),
                .menu_update, .menu_programs, .menu_terminal_mode, .menu_run, .menu_settings, .menu_tasks, .menu_restart, .menu_poweroff, .menu_halt => {
                    if (self.menuIndexForTarget(target)) |index| {
                        self.menu_selected = index;
                        self.menu_submenu_focus = false;
                        self.menu_nested_focus = false;
                        self.menu_nested_open = false;
                        if (self.menu.hasSubmenu(index)) {
                            self.menu_submenu_open = true;
                            self.menu_submenu_parent = index;
                            self.menu_submenu_selected = 0;
                        } else {
                            self.menu_submenu_open = false;
                        }
                        self.invalidateStartMenu();
                    }
                },
                .menu_terminal, .menu_notepad, .menu_paint, .menu_calc, .menu_synth, .menu_devmgr, .menu_r4code, .menu_programs_internet, .menu_settings_appearance, .menu_settings_default_apps, .menu_settings_registry, .menu_settings_network, .menu_settings_services, .menu_settings_log_center, .menu_settings_time => {
                    if (self.submenuIndexForTarget(target)) |hit| {
                        self.menu_submenu_open = true;
                        self.menu_submenu_parent = hit.parent;
                        self.menu_submenu_selected = hit.index;
                        self.menu_submenu_focus = true;
                        self.menu_nested_focus = false;
                        if (self.menu.submenuHasSubmenu(hit.parent, hit.index)) {
                            self.menu_nested_open = true;
                            self.menu_nested_parent = hit.index;
                            self.menu_nested_selected = 0;
                        } else {
                            self.menu_nested_open = false;
                        }
                        self.invalidateStartMenu();
                    }
                },
                .menu_klickifax => {
                    if (self.nestedIndexForTarget(target)) |hit| {
                        self.menu_submenu_open = true;
                        self.menu_submenu_parent = hit.parent;
                        self.menu_submenu_selected = hit.child;
                        self.menu_nested_open = true;
                        self.menu_nested_parent = hit.child;
                        self.menu_nested_selected = hit.index;
                        self.menu_nested_focus = true;
                        self.invalidateStartMenu();
                    }
                },
                .terminal_window, .wm_window, .app2_window, .app3_window, .terminal_close, .wm_close, .app2_close, .app3_close, .terminal_min, .wm_min, .app2_min, .app3_min, .terminal_max_normal, .wm_max_normal, .app2_max_normal, .app3_max_normal, .terminal_max_full, .wm_max_full, .app2_max_full, .app3_max_full => {
                    if (self.windowIndexForTarget(target)) |index| self.prepareWindowMouseDown(index, x, y);
                },
                else => {},
            }
        }
        if (!self.hasDamage()) self.invalidateTargetVisual(target);
    }

    fn dispatchMouseCommand(self: *App, target: model.UiTarget) void {
        self.dispatchCommand(target, .mouse);
    }

    fn openSystemMenuForTarget(self: *App, target: model.UiTarget, x: i32, y: i32) bool {
        if (self.dialog != .none) return false;
        const index = self.windowIndexForTarget(target) orelse return false;
        if (!self.windows[index].visible or self.windows[index].minimized) return false;
        const was_start_open = self.start_open;
        const was_time_menu_open = self.time_menu_open;
        const old_active = self.active_window;
        self.system_menu_open = true;
        self.system_menu_window = index;
        self.system_menu_x = clamp(x, 0, @max(0, self.screen_w - system_menu_w));
        self.system_menu_y = clamp(y, 0, @max(0, self.screen_h - theme.taskbar_h - system_menu_h));
        if (was_start_open) self.invalidateStartMenu();
        if (was_time_menu_open) self.invalidateTimeMenu();
        self.start_open = false;
        self.time_menu_open = false;
        self.menu_submenu_open = false;
        self.menu_submenu_focus = false;
        self.activateWindow(index, false);
        self.keyboard_focus = self.windowTargetForIndex(index);
        self.invalidateWindow(old_active);
        self.invalidateWindow(index);
        self.invalidateSystemMenu();
        if (was_start_open) {
            self.invalidateTaskbar();
        }
        return true;
    }

    fn dispatchCommand(self: *App, target: model.UiTarget, source: model.EventSource) void {
        if (target == .none) return;
        self.activateEventCommand(target, source);
        const was_system_menu_command = self.system_menu_open and self.systemMenuTargetForCommand(target);
        const was_time_menu_command = self.time_menu_open and self.timeMenuTargetForCommand(target);
        switch (target) {
            .run_browse => self.browseRunProgram(),
            .run_ok => self.submitRunDialog(),
            .run_cancel, .run_backdrop, .task_overview_ok, .settings_ok, .settings_cancel, .time_menu_backdrop, .volume_popup_backdrop => self.closeTop(),
            .message_ok, .message_no, .message_backdrop => self.closeMessageBoxWithTarget(target),
            .message_yes => {
                self.message_box_result = message_box.targetResult(self.message_box_buttons, target);
                self.confirmDialogAction();
            },
            .menu_update, .menu_programs, .menu_terminal_mode, .menu_run, .menu_settings, .menu_tasks, .menu_restart, .menu_poweroff, .menu_halt => {
                if (self.menuIndexForTarget(target)) |index| self.activateMenu(index);
            },
            .menu_terminal, .menu_notepad, .menu_paint, .menu_calc, .menu_synth, .menu_devmgr, .menu_r4code, .menu_programs_internet, .menu_settings_appearance, .menu_settings_default_apps, .menu_settings_registry, .menu_settings_network, .menu_settings_services, .menu_settings_log_center, .menu_settings_time => {
                if (self.submenuIndexForTarget(target)) |hit| self.activateSubmenu(hit.parent, hit.index);
            },
            .menu_klickifax => if (self.nestedIndexForTarget(target)) |hit| self.activateNestedSubmenu(hit.parent, hit.child, hit.index),
            .start_menu_backdrop => self.closeTop(),
            .start_button => self.toggleStart(),
            .quick_show_desktop, .quick_computer => {
                if (quick_launch.indexForTarget(target)) |index| self.activateQuickLaunch(index);
            },
            .terminal_taskbar => self.focusOrRestore(0),
            .wm_taskbar => self.focusOrRestore(1),
            .app2_taskbar => self.focusOrRestore(2),
            .app3_taskbar => self.focusOrRestore(3),
            .taskbar_keyboard_layout => self.cycleKeyboardLayout(),
            .taskbar_volume => self.toggleVolumePopup(),
            .volume_popup_mute => _ = self.toggleVolumeMute(),
            .taskbar_clock => self.launchClockFromTaskbar(),
            .time_menu_clock => self.launchClockFromTimeMenu(),
            .time_menu_settings => self.launchTimeSettingsFromTimeMenu(),
            .terminal_info, .wm_info, .app2_info, .app3_info => if (self.windowIndexForTarget(target)) |index| self.openWindowInfoDialog(index),
            .terminal_close, .wm_close, .app2_close, .app3_close => if (self.windowIndexForTarget(target)) |index| self.closeWindow(index),
            .terminal_min, .wm_min, .app2_min, .app3_min => if (self.windowIndexForTarget(target)) |index| self.minimizeWindow(index),
            .terminal_max_normal, .wm_max_normal, .app2_max_normal, .app3_max_normal, .terminal_max_full, .wm_max_full, .app2_max_full, .app3_max_full => if (self.windowIndexForTarget(target)) |index| self.toggleMaximizeWindow(index),
            .terminal_window, .wm_window, .app2_window, .app3_window => if (self.windowIndexForTarget(target)) |index| self.focusOrRestore(index),
            else => {
                if (desktop_items.indexForTarget(target)) |index| self.desktop_item_selected = index;
            },
        }
        if (was_system_menu_command) {
            self.invalidateSystemMenu();
            self.system_menu_open = false;
        }
        if (was_time_menu_command) {
            self.invalidateTimeMenu();
            self.time_menu_open = false;
        }
    }

    fn activateEventCommand(self: *App, target: model.UiTarget, source: model.EventSource) void {
        self.event.activateTarget(target, source);
    }

    fn targetAt(self: *const App, x: i32, y: i32) model.UiTarget {
        if (self.terminal_mode) return .none;
        if (self.system_menu_open) {
            if (self.systemMenuTargetAt(x, y)) |target| return target;
            return .start_menu_backdrop;
        }
        if (self.time_menu_open) {
            if (self.timeMenuTargetAt(x, y)) |target| return target;
            return .time_menu_backdrop;
        }
        if (self.volume_ui.popup_open) return self.volumePopupTargetAt(x, y);

        if (self.dialog != .none) {
            if (self.dialog == .run) {
                if (self.runButtonHit(x, y)) |button| {
                    return switch (button) {
                        .browse => .run_browse,
                        .ok => .run_ok,
                        .cancel => .run_cancel,
                    };
                }
                if (self.runInputHit(x, y)) return .run_input;
                return .run_backdrop;
            }
            if (self.dialog == .message_settings) {
                if (self.settingsButtonHit(x, y)) |target| return target;
                return .message_backdrop;
            }
            if (self.dialog == .tasks) {
                if (self.dialogButtonHit(x, y)) return .task_overview_ok;
                return .message_backdrop;
            }
            if (self.messageBoxButtonHit(x, y)) |target| return target;
            return .message_backdrop;
        }

        if (self.start_open) {
            if (self.menu_submenu_open) {
                if (self.menu_nested_open) {
                    if (self.menu.nestedHit(self.screen_w, self.screen_h, self.menu_submenu_parent, self.menu_nested_parent, x, y)) |index| {
                        return self.menu.nestedTarget(self.menu_submenu_parent, self.menu_nested_parent, index);
                    }
                }
                if (self.menu.submenuHit(self.screen_w, self.screen_h, self.menu_submenu_parent, x, y)) |index| {
                    return self.menu.submenuTarget(self.menu_submenu_parent, index);
                }
            }
            if (self.menu.hit(self.screen_h, x, y)) |index| return self.menu.target(index);
            return if (self.startButtonHit(x, y)) .start_button else .start_menu_backdrop;
        }

        if (self.startButtonHit(x, y)) return .start_button;
        if (self.quick_launch.hit(self.screen_h, x, y)) |index| return self.quick_launch.target(index);
        if (self.tray_registry.hit(x, y) != null) return .taskbar_tray_external;
        if (self.volumeHit(x, y)) return .taskbar_volume;
        if (self.keyboardLayoutHit(x, y)) return .taskbar_keyboard_layout;
        if (self.clockHit(x, y)) return .taskbar_clock;
        if (self.taskbarHit(x, y)) |index| {
            return self.taskbarTargetForIndex(index);
        }

        if (window.topmostAt(self.windows[0..], self.active_window, x, y)) |index| {
            return self.windowTarget(index, x, y);
        }
        if (self.desktop_items.hit(x, y)) |index| return self.desktop_items.target(index);
        return .none;
    }

    fn windowTarget(self: *const App, index: usize, x: i32, y: i32) model.UiTarget {
        const win = &self.windows[index];
        if (win.closeHit(x, y)) return self.closeTargetForIndex(index);
        if (win.minHit(x, y)) return self.minTargetForIndex(index);
        if (win.maxHit(x, y)) return self.maxTargetForIndex(index);
        return self.windowTargetForIndex(index);
    }

    fn systemMenuTargetAt(self: *const App, x: i32, y: i32) ?model.UiTarget {
        if (!self.system_menu_open or self.system_menu_window >= self.windows.len) return null;
        if (x < self.system_menu_x or x >= self.system_menu_x + system_menu_w) return null;
        if (y < self.system_menu_y or y >= self.system_menu_y + system_menu_h) return null;
        const rel_y = y - self.system_menu_y;
        if (rel_y >= 4 and rel_y < 4 + system_menu_row_h) return self.windowTargetForIndex(self.system_menu_window);
        if (rel_y >= 24 and rel_y < 24 + system_menu_row_h) return self.minTargetForIndex(self.system_menu_window);
        if (rel_y >= 44 and rel_y < 44 + system_menu_row_h) return self.maxTargetForIndex(self.system_menu_window);
        if (rel_y >= 64 and rel_y < 64 + system_menu_row_h) return self.infoTargetForIndex(self.system_menu_window);
        if (rel_y >= 86 and rel_y < 86 + system_menu_row_h) return self.closeTargetForIndex(self.system_menu_window);
        return null;
    }

    fn systemMenuTargetForCommand(self: *const App, target: model.UiTarget) bool {
        if (!self.system_menu_open or self.system_menu_window >= self.windows.len) return false;
        return target == self.windowTargetForIndex(self.system_menu_window) or
            target == self.minTargetForIndex(self.system_menu_window) or
            target == self.maxTargetForIndex(self.system_menu_window) or
            target == self.infoTargetForIndex(self.system_menu_window) or
            target == self.closeTargetForIndex(self.system_menu_window);
    }

    fn timeMenuTargetAt(self: *const App, x: i32, y: i32) ?model.UiTarget {
        if (!self.time_menu_open) return null;
        const rect = draw.timeMenuRect(self.screen_w, self.screen_h);
        if (!rect.contains(x, y)) return null;
        const rel_y = y - rect.y;
        if (rel_y >= 4 and rel_y < 4 + draw.time_menu_h / 2 - 4) return .time_menu_clock;
        if (rel_y >= 26 and rel_y < 26 + draw.time_menu_h / 2 - 4) return .time_menu_settings;
        return null;
    }

    fn timeMenuTargetForCommand(self: *const App, target: model.UiTarget) bool {
        if (!self.time_menu_open) return false;
        return target == .time_menu_clock or target == .time_menu_settings;
    }

    fn volumePopupTargetAt(self: *const App, x: i32, y: i32) model.UiTarget {
        if (self.volumeHit(x, y)) return .taskbar_volume;
        if (draw.volumeTrackRect(self.screen_w, self.screen_h, self.config.taskbar_clock).contains(x, y)) return .volume_popup_slider;
        if (draw.volumeMuteRect(self.screen_w, self.screen_h, self.config.taskbar_clock).contains(x, y)) return .volume_popup_mute;
        return .volume_popup_backdrop;
    }

    fn captureOwner(self: *const App) ?model.UiOwner {
        if (self.system_menu_open) return .window;
        if (self.time_menu_open) return .taskbar;
        if (self.volume_ui.popup_open) return .taskbar;
        if (self.dialog != .none) return .dialog;
        if (self.start_open) return .start_menu;
        return null;
    }

    fn menuIndexForTarget(self: *const App, target: model.UiTarget) ?usize {
        var i: usize = 0;
        while (i < self.menu.count) : (i += 1) {
            if (self.menu.target(i) == target) return i;
        }
        return null;
    }

    const SubmenuHit = struct {
        parent: usize,
        index: usize,
    };

    const NestedHit = struct {
        parent: usize,
        child: usize,
        index: usize,
    };

    fn submenuIndexForTarget(self: *const App, target: model.UiTarget) ?SubmenuHit {
        var parent: usize = 0;
        while (parent < self.menu.count) : (parent += 1) {
            const submenu = self.menu.submenu(parent) orelse continue;
            var index: usize = 0;
            while (index < submenu.count) : (index += 1) {
                if (submenu.target(index) == target) return .{ .parent = parent, .index = index };
            }
        }
        return null;
    }

    fn nestedIndexForTarget(self: *const App, target: model.UiTarget) ?NestedHit {
        var parent: usize = 0;
        while (parent < self.menu.count) : (parent += 1) {
            const submenu = self.menu.submenu(parent) orelse continue;
            var child: usize = 0;
            while (child < submenu.count) : (child += 1) {
                const nested = self.menu.nestedSubmenu(parent, child) orelse continue;
                var index: usize = 0;
                while (index < nested.count) : (index += 1) {
                    if (nested.target(index) == target) return .{ .parent = parent, .child = child, .index = index };
                }
            }
        }
        return null;
    }

    fn windowIndexForTarget(self: *const App, target: model.UiTarget) ?usize {
        _ = self;
        return switch (target) {
            .terminal_window, .terminal_close, .terminal_min, .terminal_max_normal, .terminal_max_full, .terminal_info, .terminal_taskbar => 0,
            .wm_window, .wm_close, .wm_min, .wm_max_normal, .wm_max_full, .wm_info, .wm_taskbar => 1,
            .app2_window, .app2_close, .app2_min, .app2_max_normal, .app2_max_full, .app2_info, .app2_taskbar => 2,
            .app3_window, .app3_close, .app3_min, .app3_max_normal, .app3_max_full, .app3_info, .app3_taskbar => 3,
            else => null,
        };
    }

    fn windowTargetForIndex(self: *const App, index: usize) model.UiTarget {
        if (index >= self.windows.len) return .none;
        if (index == 0) return .terminal_window;
        if (index == 3) return .app3_window;
        return if (index == 2) .app2_window else .wm_window;
    }

    fn activeWindowTargetOrNone(self: *const App) model.UiTarget {
        if (self.active_window >= self.windows.len) return .none;
        const win = &self.windows[self.active_window];
        if (!win.visible or win.minimized) return .none;
        return self.windowTarget(self.active_window, self.cursor_x, self.cursor_y);
    }

    fn taskbarTargetForIndex(self: *const App, index: usize) model.UiTarget {
        if (index >= self.windows.len) return .none;
        if (index == 0) return .terminal_taskbar;
        if (index == 3) return .app3_taskbar;
        return if (index == 2) .app2_taskbar else .wm_taskbar;
    }

    fn closeTargetForIndex(self: *const App, index: usize) model.UiTarget {
        if (index >= self.windows.len) return .none;
        if (index == 0) return .terminal_close;
        if (index == 3) return .app3_close;
        return if (index == 2) .app2_close else .wm_close;
    }

    fn infoTargetForIndex(self: *const App, index: usize) model.UiTarget {
        if (index >= self.windows.len) return .none;
        if (index == 0) return .terminal_info;
        if (index == 3) return .app3_info;
        return if (index == 2) .app2_info else .wm_info;
    }

    fn minTargetForIndex(self: *const App, index: usize) model.UiTarget {
        if (index >= self.windows.len) return .none;
        if (index == 0) return .terminal_min;
        if (index == 3) return .app3_min;
        return if (index == 2) .app2_min else .wm_min;
    }

    fn maxTargetForIndex(self: *const App, index: usize) model.UiTarget {
        if (index >= self.windows.len) return .none;
        if (index == 0) return if (self.windows[index].maximized) .terminal_max_full else .terminal_max_normal;
        if (index == 3) return if (self.windows[index].maximized) .app3_max_full else .app3_max_normal;
        if (index == 2) return if (self.windows[index].maximized) .app2_max_full else .app2_max_normal;
        return if (self.windows[index].maximized) .wm_max_full else .wm_max_normal;
    }

    fn prepareWindowMouseDown(self: *App, index: usize, x: i32, y: i32) void {
        if (index >= self.windows.len) return;
        const old_active = self.active_window;
        self.activateWindow(index, false);
        self.keyboard_focus = self.windowTarget(index, x, y);
        self.mirrorWindowFocus(index);
        self.invalidateWindow(old_active);
        self.invalidateWindow(index);
        self.invalidateTaskbar();

        const win = &self.windows[index];
        if (win.resizeHit(x, y)) |handle| {
            self.beginResize(index, handle, x, y);
        } else if (win.titleHit(x, y)) {
            self.beginDrag(index, x, y);
        } else if (self.isHostedAppWindow(index)) {
            self.pushGuiPointerEvent(index, .mouse_down, x, y, model.MouseButton.left);
        }
    }

    fn closeWindow(self: *App, index: usize) void {
        if (index >= self.windows.len) return;
        if (self.windows[index].instance_id != 0) {
            if (self.windows[index].close_requested) return;
            const result = self.requestWindowProcessClose(index);
            if (result == r4os.abi.program_handle_ok) {
                self.windows[index].requestClose(self.ctx.ticks());
                self.mirrorWindowClose(index);
                if (self.windows[index].kind == .app) {
                    self.updateGuiWindowInfo(index);
                    self.pushGuiEvent(index, .close);
                }
                self.invalidateWindow(index);
                self.invalidateTaskbar();
                return;
            }
            if (!processHandleDefinitelyGone(result)) return;
            self.clearWindowInstanceBinding(index);
        }
        self.invalidateWindow(index);
        self.windows[index].close();
        if (self.active_window == index) self.focusFirstVisibleWindow();
        self.invalidateTaskbar();
    }

    fn forceCloseWindow(self: *App, index: usize) void {
        if (index >= self.windows.len) return;
        if (index == 0) self.terminal_mode = false;
        if (self.windows[index].instance_id != 0) {
            const handle = self.window_process_handles[index];
            if (!processHandleValid(handle)) {
                self.clearWindowInstanceBinding(index);
            } else {
                const result = self.ctx.programHandleKill(&handle);
                if (result == r4os.abi.program_handle_ok) {
                    self.windows[index].requestClose(self.ctx.ticks());
                    self.invalidateWindow(index);
                    self.invalidateTaskbar();
                    return;
                }
                if (!processHandleDefinitelyGone(result)) return;
                self.clearWindowInstanceBinding(index);
            }
        }
        self.invalidateWindow(index);
        self.windows[index].close();
        if (self.active_window == index) self.focusFirstVisibleWindow();
        self.invalidateTaskbar();
    }

    fn minimizeWindow(self: *App, index: usize) void {
        if (index >= self.windows.len) return;
        self.invalidateWindow(index);
        self.windows[index].minimize();
        self.mirrorWindowMinimize(index);
        self.updateGuiWindowInfo(index);
        if (self.active_window == index) self.focusFirstVisibleWindow();
        self.invalidateTaskbar();
    }

    fn toggleMaximizeWindow(self: *App, index: usize) void {
        if (index >= self.windows.len) return;
        self.invalidateWindow(index);
        self.windows[index].toggleMaximize(self.screen_w, self.screen_h);
        if (self.windows[index].maximized) self.mirrorWindowMaximize(index) else self.mirrorWindowRestore(index);
        self.updateGuiWindowInfo(index);
        self.pushGuiEvent(index, .resize);
        self.activateWindow(index, false);
        self.keyboard_focus = self.windowTargetForIndex(index);
        self.mirrorWindowFocus(index);
        self.invalidateWindow(index);
        self.invalidateTaskbar();
    }

    fn fitWindowsToWorkArea(self: *App) void {
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            self.windows[i].fitToWorkArea(self.screen_w, self.screen_h);
        }
    }

    fn focusFirstVisibleWindow(self: *App) void {
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].visible and !self.windows[i].minimized) {
                self.activateWindow(i, false);
                self.keyboard_focus = self.windowTargetForIndex(i);
                self.mirrorWindowFocus(i);
                self.invalidateWindow(i);
                return;
            }
        }
        if (self.isBoundGuiAppWindow(self.active_window)) self.pushGuiEvent(self.active_window, .focus_lost);
        self.keyboard_focus = .none;
    }

    fn beginDrag(self: *App, index: usize, x: i32, y: i32) void {
        const win = &self.windows[index];
        if (win.maximized) return;
        self.drag = .{
            .active = true,
            .window_index = index,
            .grab_x = x - win.x,
            .grab_y = y - win.y,
            .last_x = x,
            .last_y = y,
        };
    }

    fn beginResize(self: *App, index: usize, handle: window.ResizeHandle, x: i32, y: i32) void {
        const win = &self.windows[index];
        if (win.maximized) return;
        self.resize = .{
            .active = true,
            .window_index = index,
            .handle = handle,
            .start = win.geometry(),
            .start_mouse_x = x,
            .start_mouse_y = y,
            .last_x = x,
            .last_y = y,
        };
    }

    fn updateDrag(self: *App, x: i32, y: i32) bool {
        if (!self.drag.active) return false;
        if (self.drag.last_x == x and self.drag.last_y == y) return false;
        self.drag.last_x = x;
        self.drag.last_y = y;
        const index = self.drag.window_index;
        const old_frame = self.windows[index].frameSurface().rect;
        self.windows[index].moveTo(x - self.drag.grab_x, y - self.drag.grab_y, self.screen_w, self.screen_h);
        const new_frame = self.windows[index].frameSurface().rect;
        if (rectEqual(old_frame, new_frame)) return false;
        self.damage.invalidate(old_frame);
        self.invalidateWindow(index);
        self.activateWindow(index, false);
        self.updateGuiWindowInfo(index);
        self.mirrorWindowUpdate(index);
        return true;
    }

    fn beginDesktopItemDrag(self: *App, index: usize, x: i32, y: i32) void {
        if (index >= self.desktop_items.count) return;
        const item = &self.desktop_items.entries[index];
        self.desktop_drag = .{
            .pending = true,
            .active = false,
            .index = index,
            .grab_x = x - item.x,
            .grab_y = y - item.y,
            .start_item_x = item.x,
            .start_item_y = item.y,
            .start_mouse_x = x,
            .start_mouse_y = y,
            .last_x = x,
            .last_y = y,
        };
    }

    fn updateDesktopItemDrag(self: *App, x: i32, y: i32) bool {
        if (!self.desktop_drag.pending and !self.desktop_drag.active) return false;
        if (self.desktop_drag.index >= self.desktop_items.count) {
            const was_active = self.desktop_drag.active;
            self.desktop_drag = .{};
            if (was_active) {
                self.invalidateDesktopGrid();
                return true;
            }
            return false;
        }
        if (!self.desktop_drag.active) {
            const dx = absI32(x - self.desktop_drag.start_mouse_x);
            const dy = absI32(y - self.desktop_drag.start_mouse_y);
            if (dx < desktop_drag_threshold_px and dy < desktop_drag_threshold_px) return false;
            self.desktop_drag.active = true;
            self.invalidateDesktopGrid();
        }
        if (self.desktop_drag.last_x == x and self.desktop_drag.last_y == y) return false;
        self.desktop_drag.last_x = x;
        self.desktop_drag.last_y = y;
        const index = self.desktop_drag.index;
        const old_rect = self.desktopItemRect(index) orelse return false;
        const old_preview = self.desktop_items.snapPreview(index, self.screen_w, self.screen_h, theme.taskbar_h);
        const item = &self.desktop_items.entries[index];
        item.x = clamp(x - self.desktop_drag.grab_x, 0, @max(0, self.screen_w - desktop_items.icon_w));
        item.y = clamp(y - self.desktop_drag.grab_y, 0, @max(0, self.screen_h - theme.taskbar_h - desktop_items.icon_h));
        const new_rect = self.desktopItemRect(index) orelse return false;
        const new_preview = self.desktop_items.snapPreview(index, self.screen_w, self.screen_h, theme.taskbar_h);
        if (rectEqual(old_rect, new_rect)) return false;
        self.damage.invalidate(old_rect);
        self.damage.invalidate(new_rect);
        if (old_preview) |position| self.invalidateDesktopGridSlot(position);
        if (new_preview) |position| self.invalidateDesktopGridSlot(position);
        return true;
    }

    fn finishDesktopItemDrag(self: *App) bool {
        if (!self.desktop_drag.active or self.desktop_drag.index >= self.desktop_items.count) return false;
        const index = self.desktop_drag.index;
        const start_item_x = self.desktop_drag.start_item_x;
        const start_item_y = self.desktop_drag.start_item_y;
        const old_rect = self.desktopItemRect(index) orelse return false;
        self.desktop_items.snapItemToGrid(index, self.screen_w, self.screen_h, theme.taskbar_h);
        const new_rect = self.desktopItemRect(index) orelse return false;
        self.damage.invalidate(old_rect);
        self.damage.invalidate(new_rect);
        self.invalidateDesktopGrid();
        const item = &self.desktop_items.entries[index];
        if (item.x != start_item_x or item.y != start_item_y) self.markDesktopLayoutDirty();
        self.last_mouse_down_tick = 0;
        self.last_mouse_down_target = .none;
        self.double_click_pending = false;
        return true;
    }

    fn updateResize(self: *App, x: i32, y: i32) bool {
        if (!self.resize.active) return false;
        if (self.resize.last_x == x and self.resize.last_y == y) return false;
        self.resize.last_x = x;
        self.resize.last_y = y;
        const index = self.resize.window_index;
        const old_frame = self.windows[index].frameSurface().rect;
        self.windows[index].resizeFrom(
            self.resize.start,
            self.resize.handle,
            x - self.resize.start_mouse_x,
            y - self.resize.start_mouse_y,
            self.screen_w,
            self.screen_h,
        );
        const new_frame = self.windows[index].frameSurface().rect;
        if (rectEqual(old_frame, new_frame)) return false;
        self.damage.invalidate(old_frame);
        self.invalidateWindow(index);
        self.activateWindow(index, false);
        self.updateGuiWindowInfo(index);
        self.mirrorWindowUpdate(index);
        self.pushGuiEvent(index, .resize);
        return true;
    }

    fn redraw(self: *App) void {
        _ = self.updateTrayLayout();
        var regions: [surface.max_damage_regions]surface.Rect = undefined;
        var region_count = self.damage.takeRegions(&regions);
        const cursor_only = region_count == 0 and !self.taskbar_damage and self.cursor_damage.active;
        var cursor_queued_tick: u64 = 0;

        if (self.taskbar_damage) {
            appendDamageRegion(&regions, &region_count, self.taskbarRect());
            self.taskbar_damage = false;
        }
        if (self.cursor_damage.active) {
            cursor_queued_tick = self.cursor_damage_queued_tick;
            self.cursor_damage_queued_tick = 0;
            appendDamageRegion(&regions, &region_count, self.cursor_damage.old_rect);
            if (!rectEqual(self.cursor_damage.old_rect, self.cursor_damage.new_rect)) {
                appendDamageRegion(&regions, &region_count, self.cursor_damage.new_rect);
            }
            self.cursor_damage.reset();
        }
        if (region_count == 0) {
            regions[0] = surface.desktop(self.screen_w, self.screen_h).rect;
            region_count = 1;
        }

        const kind: compositor.DamageKind = if (region_count == 1 and isFullRect(regions[0], self.screen_w, self.screen_h))
            .full
        else if (cursor_only)
            .cursor
        else
            .mixed;
        self.presentDamageRegionsTimed(regions[0..region_count], kind, cursor_queued_tick);
    }

    fn presentDamageRect(self: *App, damage_rect: surface.Rect, kind: compositor.DamageKind) void {
        self.presentDamageRegionsTimed((&damage_rect)[0..1], kind, 0);
    }

    fn presentDamageRegionsTimed(self: *App, damage_regions: []const surface.Rect, kind: compositor.DamageKind, cursor_queued_tick: u64) void {
        if (damage_regions.len == 0) return;
        var clipped_storage: [surface.max_damage_regions]surface.Rect = undefined;
        var clipped_count: usize = 0;
        for (damage_regions) |region| {
            if (clipDamageRect(region, self.screen_w, self.screen_h)) |clipped| {
                appendDamageRegion(&clipped_storage, &clipped_count, clipped);
            }
        }
        if (clipped_count == 0) return;
        const clipped_regions = clipped_storage[0..clipped_count];
        var damage_bounds = clipped_regions[0];
        var damage_pixels: u64 = 0;
        for (clipped_regions, 0..) |region, index| {
            damage_pixels += rectArea(region);
            if (index != 0) damage_bounds = damage_bounds.merged(region);
        }
        const frame_start = self.ctx.ticks();
        const frame_start_ns = self.ctx.sys.monotonicNanoseconds() orelse 0;
        _ = self.ctx.displayBeginFrameRect(
            damage_bounds.x,
            damage_bounds.y,
            @intCast(@max(0, damage_bounds.w)),
            @intCast(@max(0, damage_bounds.h)),
        );
        const scene_ready = self.ensureSceneBuffer();
        const console_scroll_offsets = self.consoleScrollOffsets();
        self.refreshConsoleSnapshots();
        const gui_frame_views = self.guiFrameViews();
        const compose_start = self.ctx.ticks();
        const compose_start_ns = self.ctx.sys.monotonicNanoseconds() orelse 0;
        var cull_stats = compositor.CullStats{};
        for (clipped_regions) |damage_rect| {
            if (scene_ready) self.ctx.beginSceneClipped(&self.scene, damage_rect);
            const region_cull = self.composeDamageRect(damage_rect, console_scroll_offsets[0..], gui_frame_views[0..]);
            accumulateCullStats(&cull_stats, region_cull);
            if (scene_ready) {
                self.ctx.endScene();
                self.scene.clearPaintClip();
            }
        }
        const compose_ticks = elapsedTicks(compose_start, self.ctx.ticks());
        const compose_end_ns = self.ctx.sys.monotonicNanoseconds() orelse 0;
        const present_start = self.ctx.ticks();
        const present_start_ns = compose_end_ns;
        var remote_publish_rc: i32 = r4os.abi.remote_frame_error_unavailable;
        var display_blit_calls: u32 = 0;
        var present_result: r4os.abi.DisplayPresentResult = .{};
        var canonical_present = false;
        var display_copy_succeeded = false;
        var display_present_succeeded = false;
        if (scene_ready) {
            remote_publish_rc = self.ctx.remoteFramePublishSceneRegions(&self.scene, clipped_regions, self.cursor_x, self.cursor_y);
            self.present_source_generation +%= 1;
            if (self.present_source_generation == 0) self.present_source_generation = 1;
            const present_rc = self.ctx.displayPresentSceneRegions(
                &self.scene,
                clipped_regions,
                self.present_source_generation,
                cursor_queued_tick,
                &present_result,
            );
            canonical_present = present_rc == 0;
            if (canonical_present) {
                display_blit_calls = 1;
                display_copy_succeeded = true;
                display_present_succeeded = true;
            } else {
                const blit_rc = self.ctx.displayBlitSceneRect(&self.scene, damage_bounds);
                const fallback_present_rc = self.ctx.displayPresent();
                display_blit_calls = 1;
                display_copy_succeeded = blit_rc >= 0;
                display_present_succeeded = fallback_present_rc >= 0;
            }
        }
        self.last_display_revision = self.ctx.displayRevision();
        const now = self.ctx.ticks();
        const now_ns = self.ctx.sys.monotonicNanoseconds() orelse 0;
        const frame_ticks = elapsedTicks(frame_start, now);
        const present_ticks = elapsedTicks(present_start, now);
        const frame_ns = elapsedNanoseconds(frame_start_ns, now_ns);
        const compose_ns = elapsedNanoseconds(compose_start_ns, compose_end_ns);
        const present_ns = elapsedNanoseconds(present_start_ns, now_ns);
        const cursor_latency_ticks = if (canonical_present and present_result.elapsed_ticks != 0)
            present_result.elapsed_ticks
        else if (cursor_queued_tick == 0)
            0
        else
            elapsedTicks(cursor_queued_tick, now);
        self.recordPresentStats(
            damage_bounds,
            @intCast(@min(damage_pixels, ~@as(u32, 0))),
            @intCast(clipped_regions.len),
            kind,
            frame_ticks,
            compose_ticks,
            present_ticks,
            frame_ns,
            compose_ns,
            present_ns,
            cursor_latency_ticks,
            cull_stats,
            display_blit_calls,
            display_copy_succeeded,
            display_present_succeeded,
            canonical_present or self.ctx.supportsDisplayBlitStride(),
            remote_publish_rc,
            present_result,
        );
    }

    fn composeDamageRect(self: *App, damage_rect: surface.Rect, console_scroll_offsets: []const u32, gui_frame_views: []const gui_frame_snapshot.View) compositor.CullStats {
        return compositor.compose(
            self.ctx,
            self.screen_w,
            self.screen_h,
            self.windows[0..],
            gui_frame_views,
            self.active_window,
            zptr(self.clock[0..]),
            zptr(self.keyboard_layout.display[0..]),
            self.volume_ui,
            &self.tray_registry,
            self.tray_hover_identity,
            self.tray_pressed_identity,
            &self.desktop_items,
            &self.quick_launch,
            self.desktop_item_selected,
            self.desktopGridDragIndex(),
            self.start_open,
            &self.menu,
            self.menu_selected,
            self.menu_submenu_open,
            self.menu_submenu_parent,
            self.menu_submenu_selected,
            self.menu_nested_open,
            self.menu_nested_parent,
            self.menu_nested_selected,
            self.overlay(),
            self.systemMenuOverlay(),
            self.time_menu_open,
            zptr(self.console_title[0..]),
            zptr(self.console_path[0..]),
            zptr(self.console_args[0..]),
            console_scroll_offsets,
            self.console_snapshots[0..],
            self.wallpaper_state.view(),
            self.config,
            self.config.terminal_font_size,
            self.config.terminal_codepage,
            self.terminal_mode,
            self.blink_phase == 0,
            self.cursor_x,
            self.cursor_y,
            self.hover_target,
            self.mouse_down_target,
            damage_rect,
        );
    }

    fn ensureSceneBuffer(self: *App) bool {
        if (self.scene.matches(self.screen_w, self.screen_h)) return true;
        const allocator = self.ctx.allocator();
        if (self.scene.memory) |memory| {
            allocator.free(memory);
            self.scene.reset();
        }
        const bytes = scene_buffer.SceneBuffer.requiredBytes(self.screen_w, self.screen_h) orelse return false;
        const memory = allocator.alignedAlloc(u8, .fromByteUnits(@alignOf(u32)), bytes) catch return false;
        if (self.scene.attach(memory, self.screen_w, self.screen_h)) return true;
        allocator.free(memory);
        self.scene.reset();
        return false;
    }

    fn consoleScrollOffsets(self: *const App) [4]u32 {
        var offsets: [4]u32 = .{0} ** 4;
        var i: usize = 0;
        while (i < offsets.len and i < self.console_scrolls.len) : (i += 1) {
            offsets[i] = self.console_scrolls[i].offset;
        }
        return offsets;
    }

    fn guiFrameViews(self: *const App) [4]gui_frame_snapshot.View {
        var views: [4]gui_frame_snapshot.View = .{gui_frame_snapshot.View{}} ** 4;
        const supported = self.ctx.supportsGuiFrameContract();
        var i: usize = 0;
        while (i < views.len and i < self.gui_frame_caches.len) : (i += 1) {
            views[i] = self.gui_frame_caches[i].view();
            views[i].supported = supported;
        }
        return views;
    }

    fn overlay(self: *const App) compositor.Overlay {
        return switch (self.dialog) {
            .none => .none,
            .run => .{ .run = .{ .path = self.run_path.z(), .focus = self.dialog_focus } },
            .tasks => .{ .tasks = .{ .status = self.program_status, .instances = self.visibleProgramInstances(), .render = self.render_stats, .focus = self.dialog_focus } },
            .message_notepad, .message_synth, .message_run_invalid, .message_run_no_programs, .message_run_no_slots, .message_run_console_busy, .message_terminal_mode_busy, .message_run_not_found, .message_run_failed, .message_window_info => .{ .message_box = self.messageBoxOverlay(zptr(self.dialog_title[0..]), zptr(self.dialog_text[0..])) },
            .message_settings => .{ .settings = .{ .config = self.config, .focus = self.dialog_focus } },
            .message_app_error => .{ .message_box = self.messageBoxOverlay(zptr(self.app_error_title[0..]), zptr(self.app_error_text[0..])) },
            .confirm_restart, .confirm_poweroff, .confirm_halt => .{ .message_box = self.messageBoxOverlay(zptr(self.dialog_title[0..]), zptr(self.dialog_text[0..])) },
        };
    }

    fn messageBoxOverlay(self: *const App, title: [*:0]const u8, text: [*:0]const u8) compositor.MessageBox {
        return .{
            .kind = self.message_box_kind,
            .buttons = self.message_box_buttons,
            .title = title,
            .text = text,
            .focus = self.dialog_focus,
        };
    }

    fn systemMenuOverlay(self: *const App) compositor.SystemMenu {
        return .{
            .open = self.system_menu_open,
            .x = self.system_menu_x,
            .y = self.system_menu_y,
            .window_index = self.system_menu_window,
        };
    }

    fn activateMenu(self: *App, index: usize) void {
        const launch = self.menu.launch(index) orelse return;
        if (launch.kind == .submenu and self.menu.hasSubmenu(index)) {
            self.openStartSubmenu(index, true);
            return;
        }
        self.closeStartMenu();
        self.runMenuLaunch(launch);
    }

    fn activateSubmenu(self: *App, parent: usize, index: usize) void {
        const launch = self.menu.submenuLaunch(parent, index) orelse return;
        if (launch.kind == .submenu and self.menu.submenuHasSubmenu(parent, index)) {
            self.menu_submenu_open = true;
            self.menu_submenu_parent = parent;
            self.menu_submenu_selected = index;
            self.menu_submenu_focus = false;
            self.menu_nested_open = true;
            self.menu_nested_parent = index;
            self.menu_nested_selected = 0;
            self.menu_nested_focus = true;
            self.invalidateStartMenu();
            return;
        }
        self.closeStartMenu();
        self.runMenuLaunch(launch);
    }

    fn activateNestedSubmenu(self: *App, parent: usize, child: usize, index: usize) void {
        const launch = self.menu.nestedLaunch(parent, child, index) orelse return;
        self.closeStartMenu();
        self.runMenuLaunch(launch);
    }

    fn runMenuLaunch(self: *App, launch: start_menu.Launch) void {
        switch (launch.target) {
            model.UiTarget.menu_terminal => self.openConsoleApp(launch),
            model.UiTarget.menu_update => self.launchProgram(launch),
            model.UiTarget.menu_terminal_mode => self.enterTerminalMode(),
            model.UiTarget.menu_run => self.openDialog(.run),
            model.UiTarget.menu_tasks => self.openDialog(.tasks),
            model.UiTarget.menu_notepad => self.launchProgram(launch),
            model.UiTarget.menu_paint => self.launchProgram(launch),
            model.UiTarget.menu_calc => self.launchProgram(launch),
            model.UiTarget.menu_synth => self.openConsoleApp(launch),
            model.UiTarget.menu_devmgr => self.launchProgram(launch),
            model.UiTarget.menu_r4code => self.launchProgram(launch),
            model.UiTarget.menu_klickifax => self.launchProgram(launch),
            model.UiTarget.menu_programs, model.UiTarget.menu_programs_internet => {},
            model.UiTarget.menu_settings => self.openDialog(.message_settings),
            model.UiTarget.menu_settings_appearance, model.UiTarget.menu_settings_default_apps, model.UiTarget.menu_settings_registry, model.UiTarget.menu_settings_network, model.UiTarget.menu_settings_services, model.UiTarget.menu_settings_log_center, model.UiTarget.menu_settings_time => self.launchProgram(launch),
            model.UiTarget.menu_restart => self.openDialog(.confirm_restart),
            model.UiTarget.menu_poweroff => self.openDialog(.confirm_poweroff),
            model.UiTarget.menu_halt => self.openDialog(.confirm_halt),
            else => {},
        }
    }

    fn closeStartMenu(self: *App) void {
        const was_start_open = self.start_open;
        if (was_start_open) {
            self.invalidateStartMenu();
            self.invalidateTaskbar();
        }
        self.start_open = false;
        self.menu_submenu_open = false;
        self.menu_submenu_focus = false;
        self.menu_nested_open = false;
        self.menu_nested_focus = false;
    }

    fn confirmDialogAction(self: *App) void {
        switch (self.dialog) {
            .confirm_restart => {
                self.flushDesktopLayoutBeforeSystemAction();
                self.ctx.systemReboot();
            },
            .confirm_poweroff => {
                self.flushDesktopLayoutBeforeSystemAction();
                self.ctx.systemPoweroff();
            },
            .confirm_halt => {
                self.flushDesktopLayoutBeforeSystemAction();
                self.ctx.systemHalt();
            },
            else => self.closeTop(),
        }
    }

    fn openConsoleApp(self: *App, launch: start_menu.Launch) void {
        if (launch.kind != .item or launch.policy != .console) {
            self.openDialog(.message_synth);
            return;
        }
        self.launchConsolePath(launch.path, launch.args, launch.title);
    }

    fn launchProgram(self: *App, launch: start_menu.Launch) void {
        if (launch.kind != .item) return;
        if (launch.policy == .console) {
            self.launchConsolePath(launch.path, launch.args, launch.title);
            return;
        }
        if (launch.policy == .gui) {
            self.launchGuiProgram(launch);
            return;
        }
        switch (self.ctx.programLaunch(launch.path, launch.args, launchPolicyForApi(launch.policy))) {
            0 => self.invalidateFull(),
            -1 => self.openDialog(.message_run_not_found),
            else => self.openDialog(.message_run_failed),
        }
    }

    fn activateDesktopItem(self: *App, index: usize) void {
        if (index >= self.desktop_items.count) return;
        const item = &self.desktop_items.entries[index];
        self.closeTaskbarTransientUi();
        switch (item.launch_kind) {
            .directory => self.launchDesktopDirectory(item.launchPathText(), item.titleText()),
            .program => self.launchDesktopPath(item.launchPathText(), item.argsText(), item.titleText(), item.launch_policy),
            .file => self.launchDesktopFile(item.launchPathText()),
        }
    }

    fn launchDesktopDirectory(self: *App, path: []const u8, title: []const u8) void {
        const window_title = if (title.len == 0) "Explorer" else title;
        self.launchDesktopPath(desktop_items.explorer_path, path, window_title, .gui);
    }

    fn launchDesktopFile(self: *App, path: []const u8) void {
        var args_buf: [desktop_items.args_max + 1]u8 = .{0} ** (desktop_items.args_max + 1);
        const target = self.assoc.resolvePath(path, args_buf[0..]) orelse {
            self.openDialog(.message_run_failed);
            return;
        };
        self.launchDesktopPath(target.app_path, target.args, target.title, target.policy);
    }

    fn launchDesktopPath(self: *App, path: []const u8, args: []const u8, title: []const u8, policy: r4os.abi.LaunchPolicy) void {
        var path_buf: [desktop_items.path_max + 1]u8 = .{0} ** (desktop_items.path_max + 1);
        var args_buf: [desktop_items.args_max + 1]u8 = .{0} ** (desktop_items.args_max + 1);
        var title_buf: [desktop_items.title_max + 1]u8 = .{0} ** (desktop_items.title_max + 1);
        if (path.len == 0 or path.len + 1 > path_buf.len or args.len + 1 > args_buf.len or title.len + 1 > title_buf.len) {
            self.openDialog(.message_run_failed);
            return;
        }
        copySliceZ(path_buf[0..], path);
        copySliceZ(args_buf[0..], args);
        copySliceZ(title_buf[0..], title);
        switch (policy) {
            .console => self.launchConsolePath(zptr(path_buf[0..]), zptr(args_buf[0..]), zptr(title_buf[0..])),
            .gui => self.launchGuiPath(zptr(path_buf[0..]), zptr(args_buf[0..]), zptr(title_buf[0..]), .gui),
            .auto => self.launchAutoPath(zptr(path_buf[0..]), zptr(args_buf[0..]), zptr(title_buf[0..])),
        }
    }

    fn activateQuickLaunch(self: *App, index: usize) void {
        const launch = self.quick_launch.launch(index) orelse return;
        self.closeTaskbarTransientUi();
        const item = &self.quick_launch.items[index];
        switch (item.kind) {
            .show_desktop => self.showDesktop(),
            .program => switch (launch.policy) {
                .console => self.launchConsolePath(launch.path, launch.args, launch.title),
                .gui => self.launchGuiPath(launch.path, launch.args, launch.title, .gui),
                .auto => self.launchAutoPath(launch.path, launch.args, launch.title),
                .action => {},
            },
        }
    }

    fn showDesktop(self: *App) void {
        self.closeTaskbarTransientUi();
        var changed = false;
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            if (!self.windows[i].visible or self.windows[i].minimized) continue;
            self.invalidateWindow(i);
            self.windows[i].minimize();
            self.mirrorWindowMinimize(i);
            self.updateGuiWindowInfo(i);
            changed = true;
        }
        if (!changed) return;
        self.focusFirstVisibleWindow();
        self.invalidateTaskbar();
    }

    fn closeTaskbarTransientUi(self: *App) void {
        const was_start_open = self.start_open;
        const was_time_menu_open = self.time_menu_open;
        const was_system_menu_open = self.system_menu_open;
        if (was_start_open) self.invalidateStartMenu();
        if (was_time_menu_open) self.invalidateTimeMenu();
        if (was_system_menu_open) self.invalidateSystemMenu();
        self.start_open = false;
        self.time_menu_open = false;
        self.system_menu_open = false;
        self.menu_submenu_open = false;
        self.menu_submenu_focus = false;
        if (was_start_open or was_time_menu_open) self.invalidateTaskbar();
        self.keyboard_focus = self.activeWindowTargetOrNone();
    }

    fn launchAutoPath(self: *App, path: [*:0]const u8, args: [*:0]const u8, title: [*:0]const u8) void {
        switch (self.ctx.programClass(path, .auto)) {
            1 => self.launchConsolePath(path, args, title),
            2 => self.launchGuiPath(path, args, title, .auto),
            -1 => self.openDialog(.message_run_not_found),
            else => self.openDialog(.message_run_failed),
        }
    }

    fn launchGuiProgram(self: *App, launch: start_menu.Launch) void {
        self.launchGuiPath(launch.path, launch.args, launch.title, .gui);
    }

    fn launchConsolePath(self: *App, path: [*:0]const u8, args: [*:0]const u8, title: [*:0]const u8) void {
        _ = self.syncProgramWindows();
        const window_index = self.findFreeConsoleWindow(equalsIgnoreCase(spanZPtr(path), terminal_path)) orelse {
            self.focusFirstConsoleWindow();
            self.openDialog(.message_run_console_busy);
            return;
        };

        const spawn_args = self.consoleSpawnArgs(path, args);
        var handle: r4os.abi.ProgramProcessHandle = .{};
        const result = self.ctx.programSpawnWithConsoleHostHandle(path, spawn_args, .console, .terminal_window, &handle);
        if (result != r4os.abi.program_handle_ok or !processHandleValid(handle)) {
            if (result == r4os.abi.program_handle_error_not_found) {
                self.openDialog(.message_run_not_found);
            } else {
                self.openDialog(.message_run_failed);
            }
            return;
        }

        const instance_id = handle.instance_id;
        self.dialog = .none;
        self.dialog_focus = .none;
        self.window_process_handles[window_index] = handle;
        self.windows[window_index].bindConsole(instance_id, title);
        self.windows[window_index].gui_revision = self.ctx.consoleRevision(instance_id);
        self.resetConsoleScroll(window_index, instance_id);
        _ = self.ctx.programSetWindowHandle(&handle, @intCast(window_index));
        self.recordWindowLaunch(window_index, path);
        self.mirrorWindowRegister(window_index);
        if (window_index == 0) self.setConsoleLaunch(title, path, spawn_args);
        self.focusConsoleWindow(window_index);
    }

    fn consoleSpawnArgs(self: *const App, path: [*:0]const u8, args: [*:0]const u8) [*:0]const u8 {
        _ = self;
        if (equalsIgnoreCase(spanZPtr(path), terminal_path) and spanZPtr(args).len == 0) return terminal_desktop_args;
        return args;
    }

    fn enterTerminalMode(self: *App) void {
        self.enterTerminalModeWithArgs(terminal_desktop_args);
    }

    fn enterTerminalModeWithArgs(self: *App, terminal_args: [*:0]const u8) void {
        _ = self.syncProgramWindows();
        if (self.hasRunningDesktopApps()) {
            self.openDialog(.message_terminal_mode_busy);
            return;
        }
        if (self.windows[0].instance_id == 0) {
            var handle: r4os.abi.ProgramProcessHandle = .{};
            const result = self.ctx.programSpawnWithConsoleHostHandle(terminal_path, terminal_args, .console, .terminal_mode, &handle);
            if (result != r4os.abi.program_handle_ok or !processHandleValid(handle)) {
                if (result == r4os.abi.program_handle_error_not_found) {
                    self.openDialog(.message_run_not_found);
                } else {
                    self.openDialog(.message_run_failed);
                }
                return;
            }
            const instance_id = handle.instance_id;
            self.window_process_handles[0] = handle;
            self.windows[0].bindConsole(instance_id, "Terminal Mode");
            self.windows[0].gui_revision = self.ctx.consoleRevision(instance_id);
            self.resetConsoleScroll(0, instance_id);
            _ = self.ctx.programSetWindowHandle(&handle, 0);
            self.recordWindowLaunch(0, terminal_path);
            self.mirrorWindowRegister(0);
        } else if (!equalsIgnoreCase(spanZ(self.console_path[0..]), terminal_path)) {
            self.openTerminal();
            self.openDialog(.message_run_console_busy);
            return;
        } else {
            _ = self.ctx.programSetConsoleHost(self.windows[0].instance_id, .terminal_mode);
        }

        self.setConsoleLaunch("Terminal Mode", terminal_path, terminal_args);
        self.followConsoleTail(0);
        self.windows[0].setTitleLit("Terminal Mode");
        self.windows[0].restore();
        self.activateWindow(0, false);
        self.keyboard_focus = .terminal_window;
        self.mirrorWindowRegister(0);
        self.mirrorWindowFocus(0);
        self.start_open = false;
        self.menu_submenu_open = false;
        self.menu_submenu_focus = false;
        self.dialog = .none;
        self.dialog_focus = .none;
        self.system_menu_open = false;
        self.terminal_mode = true;
        self.invalidateFull();
    }

    fn hasRunningDesktopApps(self: *const App) bool {
        var i: usize = app_window_first;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].instance_id != 0) return true;
        }
        return false;
    }

    fn findFreeConsoleWindow(self: *const App, prefer_primary: bool) ?usize {
        if (prefer_primary and self.windows[0].instance_id == 0 and !self.windows[0].close_requested) return 0;
        var i: usize = app_window_first;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].instance_id == 0 and !self.windows[i].close_requested) return i;
        }
        if (!prefer_primary and self.windows[0].instance_id == 0 and !self.windows[0].close_requested) return 0;
        return null;
    }

    fn launchGuiPath(self: *App, path: [*:0]const u8, args: [*:0]const u8, title: [*:0]const u8, policy: r4os.abi.LaunchPolicy) void {
        const trace_klickifax = hasKlickifaxSmokeArg(self.ctx.argsRaw()) and
            equalsIgnoreCase(spanZPtr(path), "C:\\R4OS\\SOFTWARE\\INTERNET\\KLICKIFAX.R4X");
        _ = self.syncProgramWindows();
        if (trace_klickifax) self.ctx.println("Klickifax launch: windows synchronized");
        if (self.findSingleInstanceGuiWindow(path)) |existing_index| {
            self.dialog = .none;
            self.dialog_focus = .none;
            self.start_open = false;
            self.menu_submenu_open = false;
            self.menu_submenu_focus = false;
            self.menu_nested_open = false;
            self.menu_nested_focus = false;
            self.focusOrRestore(existing_index);
            return;
        }
        const window_index = self.findFreeAppWindow() orelse {
            self.focusFirstAppWindow();
            self.openDialog(.message_run_no_slots);
            return;
        };
        var handle: r4os.abi.ProgramProcessHandle = .{};
        const result = self.ctx.programSpawnHandle(path, args, policy, &handle);
        if (trace_klickifax) self.ctx.println("Klickifax launch: process spawned");
        if (result != r4os.abi.program_handle_ok or !processHandleValid(handle)) {
            if (result == r4os.abi.program_handle_error_not_found) {
                self.openDialog(.message_run_not_found);
            } else {
                self.openDialog(.message_run_failed);
            }
            return;
        }

        const instance_id = handle.instance_id;
        _ = self.ctx.programSetWindowHandle(&handle, @intCast(window_index));
        if (trace_klickifax) self.ctx.println("Klickifax launch: window handle assigned");
        const old_active = self.active_window;
        self.dialog = .none;
        self.dialog_focus = .none;
        self.window_process_handles[window_index] = handle;
        self.gui_frame_caches[window_index].bind(self.ctx.allocator(), handle);
        self.windows[window_index].bindApp(instance_id, title);
        self.recordWindowLaunch(window_index, path);
        _ = self.syncGuiTitle(window_index);
        _ = self.syncGuiMinSize(window_index);
        if (trace_klickifax) self.ctx.println("Klickifax launch: GUI metadata synchronized");
        self.windows[window_index].gui_revision = self.ctx.guiRevision(instance_id);
        self.updateGuiWindowInfo(window_index);
        self.pushGuiEvent(window_index, .resize);
        if (trace_klickifax) self.ctx.println("Klickifax launch: initial GUI event queued");
        self.activateWindow(window_index, true);
        self.keyboard_focus = self.windowTargetForIndex(window_index);
        // R4DESK remains the authoritative host. The optional synchronous
        // WINSVC mirror is not part of browser correctness and may block while
        // the larger GUI child is runnable, so Klickifax is kept local.
        if (!equalsIgnoreCase(spanZPtr(path), "C:\\R4OS\\SOFTWARE\\INTERNET\\KLICKIFAX.R4X")) {
            self.mirrorWindowRegister(window_index);
            self.mirrorWindowFocus(window_index);
        }
        self.invalidateWindow(old_active);
        self.invalidateWindow(window_index);
        self.invalidateTaskbar();
    }

    fn findFreeAppWindow(self: *const App) ?usize {
        var i: usize = app_window_first;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].instance_id == 0 and !self.windows[i].close_requested) return i;
        }
        return null;
    }

    fn findSingleInstanceGuiWindow(self: *const App, path: [*:0]const u8) ?usize {
        if (!isSingleInstanceGuiPath(spanZPtr(path))) return null;
        var i: usize = app_window_first;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].instance_id == 0 or self.windows[i].close_requested) continue;
            if (sameSingleInstanceGuiPath(spanZPtr(path), spanZ(self.window_launch_paths[i][0..]))) return i;
        }
        return null;
    }

    fn findTerminalWindowByTitle(self: *const App, comptime title: []const u8) ?usize {
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].kind != .terminal) continue;
            if (self.windows[i].instance_id == 0 or self.windows[i].close_requested) continue;
            if (bytesEqual(spanZPtr(self.windows[i].title()), title)) return i;
        }
        return null;
    }

    fn findWindowByLaunchPath(self: *const App, path: []const u8) ?usize {
        var i: usize = app_window_first;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].instance_id == 0 or self.windows[i].close_requested) continue;
            if (equalsIgnoreCase(spanZ(self.window_launch_paths[i][0..]), path)) return i;
        }
        return null;
    }

    fn findDesktopItemByPath(self: *const App, path: []const u8) ?usize {
        var i: usize = 0;
        while (i < self.desktop_items.count) : (i += 1) {
            if (equalsIgnoreCase(self.desktop_items.entries[i].pathText(), path)) return i;
        }
        return null;
    }

    fn countWindowsByLaunchPath(self: *const App, path: []const u8) usize {
        var count: usize = 0;
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].instance_id == 0 or self.windows[i].close_requested) continue;
            if (equalsIgnoreCase(spanZ(self.window_launch_paths[i][0..]), path)) count += 1;
        }
        return count;
    }

    fn countVisibleWindows(self: *const App) usize {
        var count: usize = 0;
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].visible and !self.windows[i].close_requested) count += 1;
        }
        return count;
    }

    fn findExplorerWindow(self: *const App) ?usize {
        var i: usize = app_window_first;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].instance_id == 0 or self.windows[i].close_requested) continue;
            if (self.windowLaunchIsExplorer(i)) return i;
        }
        return null;
    }

    fn countExplorerWindows(self: *const App) usize {
        var count: usize = 0;
        var i: usize = app_window_first;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].instance_id == 0 or self.windows[i].close_requested) continue;
            if (self.windowLaunchIsExplorer(i)) count += 1;
        }
        return count;
    }

    fn closeWindowsByLaunchPath(self: *App, path: []const u8) void {
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].instance_id == 0 or self.windows[i].close_requested) continue;
            if (equalsIgnoreCase(spanZ(self.window_launch_paths[i][0..]), path)) {
                _ = self.requestWindowProcessClose(i);
            }
        }
    }

    fn closeExplorerWindows(self: *App) void {
        var i: usize = app_window_first;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].instance_id == 0 or self.windows[i].close_requested) continue;
            if (self.windowLaunchIsExplorer(i)) {
                self.closeWindow(i);
            }
        }
    }

    fn windowLaunchIsExplorer(self: *const App, index: usize) bool {
        if (index >= self.window_launch_paths.len) return false;
        return isExplorerPath(spanZ(self.window_launch_paths[index][0..]));
    }

    fn forceCloseWindowsByLaunchPath(self: *App, path: []const u8) void {
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].instance_id == 0) continue;
            if (equalsIgnoreCase(spanZ(self.window_launch_paths[i][0..]), path)) {
                self.closeConsoleHostFromDesk(i);
            }
        }
    }

    fn recordWindowLaunch(self: *App, index: usize, path: [*:0]const u8) void {
        if (index >= self.window_launch_paths.len) return;
        copyZPtr(self.window_launch_paths[index][0..], path);
    }

    fn clearWindowLaunch(self: *App, index: usize) void {
        if (index >= self.window_launch_paths.len) return;
        clearZ(self.window_launch_paths[index][0..]);
    }

    fn focusFirstAppWindow(self: *App) void {
        var i: usize = app_window_first;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].instance_id != 0) {
                self.focusOrRestore(i);
                return;
            }
        }
    }

    fn focusConsoleWindow(self: *App, index: usize) void {
        if (index >= self.windows.len) return;
        const old_active = self.active_window;
        self.terminal_mode = false;
        if (self.windows[index].instance_id != 0) _ = self.ctx.programSetConsoleHost(self.windows[index].instance_id, .terminal_window);
        self.windows[index].restore();
        self.activateWindow(index, false);
        self.keyboard_focus = self.windowTargetForIndex(index);
        self.mirrorWindowFocus(index);
        self.invalidateWindow(old_active);
        self.invalidateWindow(index);
        self.invalidateTaskbar();
    }

    fn openTerminal(self: *App) void {
        self.focusConsoleWindow(0);
    }

    fn focusFirstConsoleWindow(self: *App) void {
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].kind == .terminal and self.windows[i].instance_id != 0) {
                self.focusConsoleWindow(i);
                return;
            }
        }
    }

    fn openDialog(self: *App, dialog: Dialog) void {
        if (dialog == .tasks) {
            self.task_scroll = 0;
            _ = self.refreshProgramOverview();
            _ = self.refreshWindowServiceSnapshot();
        }
        const was_system_menu_open = self.system_menu_open;
        const was_time_menu_open = self.time_menu_open;
        const was_volume_popup_open = self.volume_ui.popup_open;
        const was_start_open = self.start_open;
        const old_dialog = self.dialog;
        self.system_menu_open = false;
        self.time_menu_open = false;
        self.volume_ui.popup_open = false;
        self.volume_dragging = false;
        self.dialog = dialog;
        self.prepareDialogText(dialog);
        if (was_start_open) self.invalidateStartMenu();
        self.start_open = false;
        self.menu_submenu_open = false;
        self.menu_submenu_focus = false;
        self.menu_nested_open = false;
        self.menu_nested_focus = false;
        self.dialog_focus = defaultDialogFocus(dialog);
        self.keyboard_focus = self.dialog_focus;
        if (dialog == .run and self.run_path.len == 0) self.run_path.set(default_run_path);
        if (was_system_menu_open) self.invalidateSystemMenu();
        if (was_time_menu_open) self.invalidateTimeMenu();
        if (was_volume_popup_open) self.invalidateVolumePopup();
        if (was_start_open) {
            self.invalidateTaskbar();
        }
        self.invalidateDialogFor(old_dialog);
        self.invalidateDialog();
    }

    fn openWindowInfoDialog(self: *App, index: usize) void {
        if (index >= self.windows.len) return;
        if (!self.windows[index].visible or self.windows[index].minimized) return;
        self.window_info_index = index;
        self.openDialog(.message_window_info);
    }

    fn prepareDialogText(self: *App, dialog: Dialog) void {
        clearZ(self.dialog_title[0..]);
        clearZ(self.dialog_text[0..]);
        self.message_box_kind = .info;
        self.message_box_buttons = .ok;
        self.message_box_result = .none;
        switch (dialog) {
            .message_notepad => self.setDialogText("Notepad", "External app launch follows after launcher work."),
            .message_synth => self.setDialogText("R4Synth", "Use Terminal: SYNTH C:\\TEMP\\TADA.WAV"),
            .message_run_invalid => self.setDialogText("Run", "Enter a program path, optionally followed by arguments."),
            .message_run_no_programs => self.setMessageBox(.warning, .ok, "Run", "No R4X program found in C:\\R4OS\\SOFTWARE\\DESKTOP."),
            .message_run_no_slots => self.setMessageBox(.warning, .ok, "Run", "No free GUI window slot is available."),
            .message_run_console_busy => self.setMessageBox(.warning, .ok, "Run", "No free console window slot is available."),
            .message_terminal_mode_busy => self.setMessageBox(.warning, .ok, "Terminal Mode", "Close desktop apps before switching to Terminal Mode."),
            .message_run_not_found => self.setMessageBox(.@"error", .ok, "Run", "Program was not found."),
            .message_run_failed => self.setMessageBox(.@"error", .ok, "Run", "Program loader returned an error."),
            .message_app_error => {
                self.message_box_kind = .@"error";
                self.message_box_buttons = .ok;
            },
            .message_window_info => self.prepareWindowInfoText(),
            .confirm_restart => self.setMessageBox(.question, .yes_no, "Restart", "Restart R4OS now?"),
            .confirm_poweroff => self.setMessageBox(.question, .yes_no, "Poweroff", "Power off the machine now?"),
            .confirm_halt => self.setMessageBox(.question, .yes_no, "Halt", "Halt R4OS now?"),
            else => {},
        }
    }

    fn setDialogText(self: *App, comptime title: []const u8, comptime text: []const u8) void {
        self.setMessageBox(.info, .ok, title, text);
    }

    fn setMessageBox(self: *App, kind: message_box.Kind, buttons: message_box.Buttons, comptime title: []const u8, comptime text: []const u8) void {
        self.message_box_kind = kind;
        self.message_box_buttons = buttons;
        copyLit(self.dialog_title[0..], title);
        copyLit(self.dialog_text[0..], text);
    }

    fn closeMessageBoxWithTarget(self: *App, target: model.UiTarget) void {
        self.message_box_result = message_box.targetResult(self.message_box_buttons, target);
        self.closeTop();
    }

    fn prepareWindowInfoText(self: *App) void {
        self.message_box_kind = .info;
        self.message_box_buttons = .ok;
        copyLit(self.dialog_title[0..], "Window Info");
        if (self.window_info_index >= self.windows.len) {
            copyLit(self.dialog_text[0..], "Window is no longer available.");
            return;
        }
        formatWindowInfo(self.dialog_text[0..], &self.windows[self.window_info_index]);
    }

    fn closeTop(self: *App) void {
        if (self.system_menu_open) {
            self.invalidateSystemMenu();
            self.system_menu_open = false;
            self.mouse_down_target = .none;
            self.keyboard_focus = self.activeWindowTargetOrNone();
        } else if (self.time_menu_open) {
            self.invalidateTimeMenu();
            self.time_menu_open = false;
            self.mouse_down_target = .none;
            self.keyboard_focus = self.activeWindowTargetOrNone();
        } else if (self.volume_ui.popup_open) {
            self.invalidateVolumePopup();
            self.volume_ui.popup_open = false;
            self.volume_dragging = false;
            self.mouse_down_target = .none;
            self.keyboard_focus = self.activeWindowTargetOrNone();
            self.invalidateTaskbar();
        } else if (self.dialog != .none) {
            const old_dialog = self.dialog;
            self.dialog = .none;
            self.dialog_focus = .none;
            self.keyboard_focus = self.activeWindowTargetOrNone();
            self.invalidateDialogFor(old_dialog);
        } else if (self.start_open) {
            self.invalidateStartMenu();
            self.start_open = false;
            self.menu_submenu_open = false;
            self.menu_submenu_focus = false;
            self.keyboard_focus = self.activeWindowTargetOrNone();
            self.invalidateTaskbar();
        }
    }

    fn resetWindowServiceState(self: *App) void {
        self.window_service_mirrored = .{false} ** 4;
        var record = r4os.abi.WindowServiceRecord{};
        var result: r4os.abi.WindowServiceResult = .{};
        const rc = self.ctx.windowServiceRecord(r4os.abi.window_service_op_restart_cleanup, &record, &result);
        if (rc != r4os.abi.window_service_result_ok or !validWindowServiceResult(&result) or
            result.result != r4os.abi.window_service_result_ok)
        {
            self.markWindowServiceUnavailable();
            return;
        }
        self.win_service_gate.markAvailable();
        self.win_service_status.window_count = result.window_count;
        self.win_service_status.focused_window = result.focused_window;
        self.win_service_status.focused_instance = result.focused_instance;
    }

    fn refreshWindowServiceSnapshot(self: *App) bool {
        var snapshot: r4os.abi.WindowServiceSnapshot = .{};
        const rc = self.ctx.windowServiceSnapshot(&snapshot);
        if (rc == 0) {
            self.win_service_snapshot = snapshot;
            self.win_service_status = snapshot.status;
            self.win_service_gate.markAvailable();
            return true;
        }
        self.markWindowServiceUnavailable();
        return false;
    }

    fn mirrorWindowRegister(self: *App, index: usize) void {
        self.sendWindowServiceOp(index, r4os.abi.window_service_op_register);
    }

    fn mirrorWindowUpdate(self: *App, index: usize) void {
        self.sendWindowServiceOp(index, r4os.abi.window_service_op_update);
    }

    fn mirrorWindowFocus(self: *App, index: usize) void {
        self.sendWindowServiceOp(index, r4os.abi.window_service_op_focus);
    }

    fn mirrorWindowMinimize(self: *App, index: usize) void {
        self.sendWindowServiceOp(index, r4os.abi.window_service_op_minimize);
    }

    fn mirrorWindowRestore(self: *App, index: usize) void {
        self.sendWindowServiceOp(index, r4os.abi.window_service_op_restore);
    }

    fn mirrorWindowMaximize(self: *App, index: usize) void {
        self.sendWindowServiceOp(index, r4os.abi.window_service_op_maximize);
    }

    fn mirrorWindowClose(self: *App, index: usize) void {
        self.sendWindowServiceOp(index, r4os.abi.window_service_op_close);
    }

    fn mirrorWindowRemove(self: *App, index: usize) void {
        self.sendWindowServiceOp(index, r4os.abi.window_service_op_remove);
    }

    fn sendWindowServiceOp(self: *App, index: usize, op: u16) void {
        if (!self.win_service_gate.available) return;
        if (index >= self.windows.len) return;
        if (op != r4os.abi.window_service_op_remove and self.windows[index].instance_id == 0) return;
        if (op != r4os.abi.window_service_op_register and !self.window_service_mirrored[index]) return;
        var record = self.windowServiceRecordForIndex(index);
        if (op == r4os.abi.window_service_op_focus) record.flags |= r4os.abi.window_service_flag_focused;
        if (op == r4os.abi.window_service_op_minimize) record.flags |= r4os.abi.window_service_flag_minimized;
        if (op == r4os.abi.window_service_op_close) record.flags |= r4os.abi.window_service_flag_closing;
        var result: r4os.abi.WindowServiceResult = .{};
        const rc = self.ctx.windowServiceRecord(op, &record, &result);
        if (rc != r4os.abi.window_service_result_ok or !validWindowServiceResult(&result) or
            result.result != r4os.abi.window_service_result_ok)
        {
            self.markWindowServiceUnavailable();
            return;
        }
        if (op == r4os.abi.window_service_op_register) {
            self.window_service_mirrored[index] = true;
        } else if (op == r4os.abi.window_service_op_remove) {
            self.window_service_mirrored[index] = false;
        }
        self.win_service_gate.markAvailable();
        self.win_service_status.window_count = result.window_count;
        self.win_service_status.focused_window = result.focused_window;
        self.win_service_status.focused_instance = result.focused_instance;
    }

    fn markWindowServiceUnavailable(self: *App) void {
        self.window_service_mirrored = .{false} ** 4;
        self.win_service_gate.markUnavailable(self.ctx.ticks(), self.window_service_retry_ticks);
    }

    fn retryWindowServiceIfDue(self: *App) bool {
        const now = self.ctx.ticks();
        if (!self.win_service_gate.retryDue(now, true)) return false;
        self.resetWindowServiceState();
        if (!self.win_service_gate.available) return false;

        var index: usize = 0;
        while (index < self.windows.len) : (index += 1) {
            if (self.windows[index].instance_id == 0) continue;
            self.mirrorWindowRegister(index);
            if (!self.win_service_gate.available) return false;
        }
        return true;
    }

    fn windowServiceRecordForIndex(self: *const App, index: usize) r4os.abi.WindowServiceRecord {
        const win = &self.windows[index];
        var flags: u32 = 0;
        if (win.visible) flags |= r4os.abi.window_service_flag_visible;
        if (win.minimized) flags |= r4os.abi.window_service_flag_minimized;
        if (win.maximized) flags |= r4os.abi.window_service_flag_maximized;
        if (self.active_window == index and win.visible and !win.minimized) flags |= r4os.abi.window_service_flag_focused;
        if (win.close_requested) flags |= r4os.abi.window_service_flag_closing;
        if (self.terminal_mode and index == 0) flags |= r4os.abi.window_service_flag_fullscreen;

        const kind: u16 = switch (win.kind) {
            .terminal => blk: {
                flags |= r4os.abi.window_service_flag_terminal;
                break :blk r4os.abi.window_service_kind_terminal;
            },
            .manager => blk: {
                flags |= r4os.abi.window_service_flag_gui;
                break :blk r4os.abi.window_service_kind_manager;
            },
            .app => blk: {
                flags |= r4os.abi.window_service_flag_gui;
                break :blk r4os.abi.window_service_kind_gui;
            },
        };

        var out = r4os.abi.WindowServiceRecord{
            .kind = kind,
            .window_id = @intCast(index),
            .instance_id = win.instance_id,
            .flags = flags,
            .x = win.x,
            .y = win.y,
            .w = win.w,
            .h = win.h,
            .normal_x = win.normal_x,
            .normal_y = win.normal_y,
            .normal_w = win.normal_w,
            .normal_h = win.normal_h,
        };
        copySliceZ(out.title[0..], spanZPtr(win.title()));
        const launch_path = spanZ(self.window_launch_paths[index][0..]);
        if (launch_path.len != 0) {
            copySliceZ(out.path[0..], launch_path);
        } else if (win.kind == .terminal) {
            copySliceZ(out.path[0..], spanZ(self.console_path[0..]));
        }
        return out;
    }

    fn refreshProgramOverview(self: *App) bool {
        self.ctx.programStatus(&self.program_status);
        const allocator = self.ctx.allocator();
        var attempt: u32 = 0;
        refresh: while (attempt < task_inventory_restart_limit) : (attempt += 1) {
            self.program_inventory_staging.clearRetainingCapacity();
            var cursor: r4os.abi.ProgramInventoryCursor = .{};
            var summary: r4os.abi.ProgramInventorySummary = .{};
            if (self.ctx.programInventoryBegin(&cursor, &summary) != r4os.abi.program_handle_ok) {
                self.inventory_restart_pending = true;
                return false;
            }
            self.program_inventory_staging.ensureTotalCapacity(allocator, @intCast(summary.program_total)) catch {
                // Preserve the last complete generation and retry on a later
                // timer tick once memory pressure has changed.
                self.inventory_out_of_memory = true;
                return false;
            };

            self.inventory_page = 0;
            while (true) {
                var entries: [task_inventory_page_capacity]r4os.abi.ProgramInstanceSnapshot = undefined;
                var page: r4os.abi.ProgramInventoryPageInfo = .{};
                const rc = self.ctx.programInventoryPrograms(&cursor, entries[0..], &page);
                if (rc != r4os.abi.program_handle_ok) {
                    self.inventory_restart_pending = true;
                    return false;
                }
                if (page.status == r4os.abi.program_inventory_status_restart) {
                    self.inventory_restarts +|= 1;
                    self.inventory_restart_pending = true;
                    continue :refresh;
                }
                if (page.snapshot_generation != cursor.snapshot_generation or
                    page.returned > entries.len or
                    (page.status != r4os.abi.program_inventory_status_complete and
                        page.status != r4os.abi.program_inventory_status_more))
                {
                    self.inventory_restart_pending = true;
                    return false;
                }
                for (entries[0..@intCast(page.returned)]) |entry| {
                    self.program_inventory_staging.append(allocator, entry) catch {
                        self.inventory_out_of_memory = true;
                        return false;
                    };
                }
                self.inventory_page +|= 1;
                if (page.status == r4os.abi.program_inventory_status_complete) break;
                if (page.returned == 0) {
                    self.inventory_restart_pending = true;
                    return false;
                }
            }

            const previous = self.program_instances;
            self.program_instances = self.program_inventory_staging;
            self.program_inventory_staging = previous;
            self.inventory_restart_pending = false;
            self.inventory_out_of_memory = false;
            self.clampTaskScroll();
            return true;
        }
        // Continuous mutation exhausted the bounded retry budget. Keep the
        // previous snapshot visible instead of presenting a mixed generation.
        self.inventory_restart_pending = true;
        return false;
    }

    fn syncProgramWindows(self: *App) bool {
        const inventory_authoritative = self.refreshProgramOverview();
        // The last complete generation remains useful for display, but it is
        // never evidence that a currently bound instance disappeared. A busy
        // snapshot, restart or allocation failure therefore defers every
        // lifecycle mutation until a complete generation is available.
        if (!inventory_authoritative) return false;
        var changed = false;
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            const instance_id = self.windows[i].instance_id;
            if (instance_id == 0) continue;
            if (self.instanceSnapshotForWindow(i)) |snapshot| {
                const info = snapshot.info;
                if (info.state == @intFromEnum(r4os.abi.ProgramInstanceState.done)) {
                    var completion: r4os.abi.ProgramProcessCompletion = .{};
                    const reap_status = self.ctx.programHandleReap(&snapshot.handle, &completion);
                    if (reap_status != r4os.abi.program_handle_ok and !processHandleDefinitelyGone(reap_status)) continue;
                    const exit_code = if (reap_status == r4os.abi.program_handle_ok) completion.exit_code else info.exit_code;
                    self.window_completion_handles[i] = snapshot.handle;
                    self.window_completion_exit_codes[i] = exit_code;
                    self.invalidateWindow(i);
                    self.reportFinishedWindow(i, exit_code);
                    self.clearWindowInstanceBinding(i);
                    self.windows[i].close();
                    if (self.active_window == i) self.focusFirstVisibleWindow();
                    changed = true;
                    continue;
                }
                if (self.windows[i].kind == .terminal and (info.flags & r4os.abi.ProgramInstanceFlag.desktop_requested) != 0) {
                    self.closeConsoleHostFromDesk(i);
                    changed = true;
                    continue;
                }
                if (info.window_id != @as(i32, @intCast(i))) {
                    _ = self.ctx.programSetWindowHandle(&snapshot.handle, @intCast(i));
                }
                if (self.shouldKillClosingWindow(i)) {
                    self.forceCloseWindow(i);
                    changed = true;
                    continue;
                }
                self.updateGuiWindowInfo(i);
                continue;
            }
            self.invalidateWindow(i);
            self.reportFinishedWindow(i, self.program_status.last_exit_code);
            // The exact generation is absent from the authoritative snapshot.
            // Never let an ID-reused successor inherit the stale window action.
            self.clearWindowInstanceBinding(i);
            self.windows[i].close();
            if (self.active_window == i) self.focusFirstVisibleWindow();
            changed = true;
        }
        if (changed) self.invalidateTaskbar();
        return changed;
    }

    fn reportFinishedWindow(self: *App, index: usize, exit_code: i32) void {
        if (exit_code == 0) return;
        if (self.windows[index].close_requested) return;
        copyZPtr(self.app_error_title[0..], self.windows[index].title());
        formatAppError(self.app_error_text[0..], exit_code);
        if (self.dialog == .none) self.openDialog(.message_app_error);
    }

    fn shouldKillClosingWindow(self: *const App, index: usize) bool {
        if (index >= self.windows.len) return false;
        const win = &self.windows[index];
        if (!win.close_requested or win.close_requested_tick == 0) return false;
        const now = self.ctx.ticks();
        return now >= win.close_requested_tick and now - win.close_requested_tick >= self.close_kill_timeout_ticks;
    }

    fn syncGuiRevisions(self: *App) bool {
        var changed = false;
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            const instance_id = self.windows[i].instance_id;
            if (instance_id == 0 or !self.windows[i].visible or self.windows[i].minimized) continue;
            const revision = self.ctx.guiRevision(instance_id);
            const revision_changed = revision != 0 and revision != self.windows[i].gui_revision;
            if (revision_changed) {
                self.windows[i].gui_revision = revision;
                self.gui_frame_caches[i].markPending();
                const appearance_changed = self.syncAppearanceSignal(i);
                const title_changed = self.syncGuiTitle(i);
                const size_changed = self.syncGuiMinSize(i);
                if (title_changed and !size_changed and !appearance_changed) self.invalidateWindow(i);
                changed = true;
            }
            if (self.syncGuiFrameSnapshot(i)) changed = true;
        }
        return changed;
    }

    fn syncGuiFrameSnapshot(self: *App, index: usize) bool {
        if (index >= self.windows.len or !self.ctx.supportsGuiFrameContract()) return false;
        if (self.windows[index].kind != .app) return false;
        const handle = self.window_process_handles[index];
        if (!processHandleValid(handle) or self.windows[index].instance_id != handle.instance_id) return false;
        const cache = &self.gui_frame_caches[index];
        if (!sameProcessHandle(cache.owner, handle)) cache.bind(self.ctx.allocator(), handle);
        if (!cache.pending) return false;
        return switch (cache.refresh(self.ctx.allocator(), self.ctx)) {
            .updated => blk: {
                self.invalidateGuiFrameDamage(index, cache.view());
                break :blk true;
            },
            .unchanged, .no_frame, .retry => false,
        };
    }

    fn invalidateGuiFrameDamage(self: *App, index: usize, frame: gui_frame_snapshot.View) void {
        if (index >= self.windows.len or frame.full_damage or frame.damage_regions.len == 0) {
            self.invalidateWindowClient(index);
            return;
        }
        const client = self.windows[index].clientSurface().rect;
        for (frame.damage_regions) |region| {
            const left = @max(@as(i64, 0), region.x);
            const top = @max(@as(i64, 0), region.y);
            const right = @min(@as(i64, client.w), @as(i64, region.x) + region.w);
            const bottom = @min(@as(i64, client.h), @as(i64, region.y) + region.h);
            if (right <= left or bottom <= top) continue;
            self.damage.invalidate(.{
                .x = client.x + @as(i32, @intCast(left)),
                .y = client.y + @as(i32, @intCast(top)),
                .w = @intCast(right - left),
                .h = @intCast(bottom - top),
            });
        }
    }

    fn syncAppearanceSignal(self: *App, index: usize) bool {
        if (index >= self.windows.len) return false;
        const instance_id = self.windows[index].instance_id;
        if (instance_id == 0) return false;
        var text: [64]u8 = .{0} ** 64;
        const len = self.ctx.guiText(instance_id, text[0..]);
        if (len <= 0) return false;
        const signal = appearance_signal.parse(text[0..@intCast(len)]) orelse return false;

        var next = self.config;
        var config_bytes: [r4std.config.max_file_bytes]u8 = undefined;
        const config_len = self.ctx.fileRead(desktop_config_path, config_bytes[0..]);
        if (config_len <= 0 or !next.loadFromBytes(config_bytes[0..@intCast(config_len)])) return false;
        switch (signal) {
            .background => |requested| {
                if (next.desktop_bg != requested) return false;
                if (requested == self.config.desktop_bg) return false;
            },
            .reload => {},
        }
        self.config = next;
        self.reloadWallpaper();
        self.invalidateFull();
        return true;
    }

    fn syncConsoleRevision(self: *App) bool {
        var changed = false;
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].kind != .terminal) continue;
            const instance_id = self.windows[i].instance_id;
            if (instance_id == 0) continue;
            if (!self.terminal_mode and (!self.windows[i].visible or self.windows[i].minimized)) continue;
            const revision = self.ctx.consoleRevision(instance_id);
            if (revision == self.windows[i].gui_revision) continue;
            self.windows[i].gui_revision = revision;
            var state: r4os.abi.ConsoleState = .{};
            if (self.ctx.consoleState(instance_id, &state) >= 0) self.syncConsoleScrollState(i, instance_id, state);
            if (self.terminal_mode and i == 0) {
                self.invalidateFull();
            } else {
                self.invalidateWindow(i);
            }
            changed = true;
        }
        return changed;
    }

    fn syncHostLaunchRequests(self: *App) bool {
        var i: usize = app_window_first;
        while (i < self.windows.len) : (i += 1) {
            const instance_id = self.windows[i].instance_id;
            if (instance_id == 0) continue;
            var request: r4os.abi.ProgramHostLaunchRequest = .{};
            const result = self.ctx.programTakeHostLaunch(instance_id, &request);
            if (result <= 0) continue;
            self.launchHostRequest(request);
            return true;
        }
        return false;
    }

    fn launchHostRequest(self: *App, request: r4os.abi.ProgramHostLaunchRequest) void {
        const path = zptr(request.path[0..]);
        var traced_args: [r4os.subsystem_launch.max_args_bytes + 1]u8 = .{0} ** (r4os.subsystem_launch.max_args_bytes + 1);
        const args = if (self.addDesktopR4BasicTrace(spanZ(request.args[0..]), traced_args[0..r4os.subsystem_launch.max_args_bytes])) |encoded|
            zptr(traced_args[0 .. encoded.len + 1])
        else
            zptr(request.args[0..]);
        var title_buf: [console_title_max + 1]u8 = .{0} ** (console_title_max + 1);
        titleFromPath(title_buf[0..], path);
        const title: [*:0]const u8 = @ptrCast(&title_buf);
        switch (parseAbiLaunchPolicy(request.policy)) {
            .console => self.launchConsolePath(path, args, title),
            .gui => self.launchGuiPath(path, args, title, .gui),
            .auto => self.launchAutoPath(path, args, title),
        }
    }

    fn addDesktopR4BasicTrace(self: *const App, encoded: []const u8, out: []u8) ?[]const u8 {
        const request = r4os.subsystem_launch.parse(encoded) catch return null;
        if (request.option_count != 3) return null;
        const trace_id = (request.option(r4os.subsystem_launch.trace_key) catch null) orelse return null;
        const mode = (request.option(r4os.subsystem_launch.trace_mode_key) catch null) orelse return null;
        const prior_phases = (request.option(r4os.subsystem_launch.trace_phases_key) catch null) orelse return null;
        if (trace_id.len != 16 or !equalsIgnoreCase(mode, r4os.subsystem_launch.trace_mode_gui)) return null;
        const start_ns = std.fmt.parseInt(u64, trace_id, 16) catch return null;
        const now_ns = self.ctx.sys.monotonicNanoseconds() orelse return null;
        if (now_ns < start_ns) return null;
        var fields = std.mem.splitScalar(u8, prior_phases, ',');
        const probe_elapsed = fields.next() orelse return null;
        const resolve_elapsed = fields.next() orelse return null;
        _ = fields.next() orelse return null;
        if (fields.next() != null) return null;
        _ = std.fmt.parseInt(u64, probe_elapsed, 10) catch return null;
        _ = std.fmt.parseInt(u64, resolve_elapsed, 10) catch return null;
        var phases_storage: [64]u8 = undefined;
        const phases = std.fmt.bufPrint(phases_storage[0..], "{s},{s},{d}", .{
            probe_elapsed,
            resolve_elapsed,
            now_ns - start_ns,
        }) catch return null;
        const options = [_]r4os.subsystem_launch.Option{
            .{ .key = r4os.subsystem_launch.trace_key, .value = trace_id },
            .{ .key = r4os.subsystem_launch.trace_mode_key, .value = mode },
            .{ .key = r4os.subsystem_launch.trace_phases_key, .value = phases },
        };
        return r4os.subsystem_launch.encode(request.guest_path, &options, out) catch null;
    }

    fn syncDisplayRevision(self: *App) bool {
        const revision = self.ctx.displayRevision();
        if (revision == self.last_display_revision) return false;
        self.last_display_revision = revision;
        const next_w = fallbackDimension(self.ctx.screenWidth(), self.screen_w);
        const next_h = fallbackDimension(self.ctx.screenHeight(), self.screen_h);
        if (next_w != self.screen_w or next_h != self.screen_h) {
            self.screen_w = next_w;
            self.screen_h = next_h;
            self.fitWindowsToWorkArea();
            if (self.scene.memory) |memory| {
                const allocator = self.ctx.allocator();
                allocator.free(memory);
                self.scene.reset();
            }
        }
        self.invalidateFull();
        return true;
    }

    fn syncGuiTitle(self: *App, index: usize) bool {
        if (index >= self.windows.len) return false;
        const instance_id = self.windows[index].instance_id;
        if (instance_id == 0) return false;
        var title_buf: [41]u8 = .{0} ** 41;
        if (self.ctx.guiTitle(instance_id, title_buf[0..]) <= 0) return false;
        if (bytesEqual(spanZPtr(self.windows[index].title()), spanZ(title_buf[0..]))) return false;
        self.windows[index].setTitle(@ptrCast(&title_buf));
        self.invalidateTaskbar();
        self.mirrorWindowUpdate(index);
        return true;
    }

    fn syncGuiMinSize(self: *App, index: usize) bool {
        if (index >= self.windows.len) return false;
        const instance_id = self.windows[index].instance_id;
        if (instance_id == 0) return false;
        var size: r4os.abi.GuiSize = .{};
        if (self.ctx.guiMinSize(instance_id, &size) < 0) return false;
        if (size.w <= 0 and size.h <= 0) return false;
        const changed = self.windows[index].setMinClientSize(size.w, size.h, @intCast(self.ctx.screenWidth()), @intCast(self.ctx.screenHeight()));
        if (changed) {
            self.updateGuiWindowInfo(index);
            self.pushGuiEvent(index, .resize);
            self.invalidateWindow(index);
            self.mirrorWindowUpdate(index);
        }
        return changed;
    }

    fn instanceSnapshotForWindow(self: *const App, index: usize) ?r4os.abi.ProgramInstanceSnapshot {
        if (index >= self.windows.len) return null;
        const handle = self.window_process_handles[index];
        if (!processHandleValid(handle) or self.windows[index].instance_id != handle.instance_id) return null;
        for (self.program_instances.items) |snapshot| {
            if (sameProcessHandle(snapshot.handle, handle) and snapshot.info.id == handle.instance_id) return snapshot;
        }
        return null;
    }

    fn visibleProgramInstances(self: *const App) []const r4os.abi.ProgramInstanceSnapshot {
        const start = @min(self.task_scroll, self.program_instances.items.len);
        const end = @min(self.program_instances.items.len, start + task_inventory_visible_rows);
        return self.program_instances.items[start..end];
    }

    fn clampTaskScroll(self: *App) void {
        const max_scroll = self.program_instances.items.len -| task_inventory_visible_rows;
        self.task_scroll = @min(self.task_scroll, max_scroll);
    }

    fn scrollTaskInventory(self: *App, delta: i32) bool {
        const max_scroll = self.program_instances.items.len -| task_inventory_visible_rows;
        const next = if (delta > 0)
            @min(max_scroll, self.task_scroll +| @as(usize, @intCast(delta)))
        else if (delta < 0)
            self.task_scroll -| @as(usize, @intCast(-delta))
        else
            self.task_scroll;
        if (next == self.task_scroll) return true;
        self.task_scroll = next;
        self.invalidateDialog();
        return true;
    }

    fn requestWindowProcessClose(self: *App, index: usize) i32 {
        if (index >= self.windows.len) return r4os.abi.program_handle_error_invalid;
        const handle = self.window_process_handles[index];
        if (!processHandleValid(handle) or self.windows[index].instance_id != handle.instance_id)
            return r4os.abi.program_handle_error_stale;
        return self.ctx.programHandleRequestClose(&handle);
    }

    fn clearWindowInstanceBinding(self: *App, index: usize) void {
        self.mirrorWindowRemove(index);
        self.window_service_mirrored[index] = false;
        self.gui_frame_caches[index].deinit(self.ctx.allocator());
        self.window_process_handles[index] = .{};
        self.windows[index].unbindInstance();
        self.resetConsoleScroll(index, 0);
        self.clearWindowLaunch(index);
    }

    fn closeConsoleHostFromDesk(self: *App, index: usize) void {
        if (index >= self.windows.len) return;
        const handle = self.window_process_handles[index];
        if (!processHandleValid(handle)) {
            self.clearWindowInstanceBinding(index);
        } else {
            const result = self.ctx.programHandleKill(&handle);
            if (result == r4os.abi.program_handle_ok) {
                self.windows[index].requestClose(self.ctx.ticks());
                if (index == 0) self.terminal_mode = false;
                self.invalidateWindow(index);
                self.invalidateTaskbar();
                self.invalidateFull();
                return;
            }
            if (!processHandleDefinitelyGone(result)) return;
            self.clearWindowInstanceBinding(index);
        }
        self.windows[index].close();
        if (index == 0) {
            self.windows[0].setTitleLit("Terminal");
            self.setConsoleLaunch("Terminal", terminal_path, terminal_desktop_args);
            self.terminal_mode = false;
        }
        self.dialog = .none;
        self.dialog_focus = .none;
        if (self.active_window == index) self.focusFirstVisibleWindow();
        self.keyboard_focus = self.activeWindowTargetOrNone();
        self.invalidateFull();
    }

    fn updateGuiWindowInfo(self: *App, index: usize) void {
        if (index >= self.windows.len) return;
        const instance_id = self.windows[index].instance_id;
        if (instance_id == 0) return;
        const info = self.guiWindowInfoForIndex(index);
        _ = self.ctx.guiSetWindowInfo(instance_id, &info);
    }

    fn pushGuiEvent(self: *App, index: usize, kind: r4os.abi.GuiEventKind) void {
        if (index >= self.windows.len) return;
        const instance_id = self.windows[index].instance_id;
        if (instance_id == 0) return;
        const client = self.windows[index].clientSurface().rect;
        const event = r4os.abi.GuiEvent{
            .kind = @intFromEnum(kind),
            .window_id = @intCast(index),
            .x = client.x,
            .y = client.y,
            .tick = self.ctx.ticks(),
        };
        _ = self.pushGuiEventBounded(instance_id, &event, true);
    }

    fn activateWindow(self: *App, index: usize, announce_same: bool) void {
        if (index >= self.windows.len) return;
        const previous = self.active_window;
        if (previous != index and self.isBoundGuiAppWindow(previous)) self.pushGuiEvent(previous, .focus_lost);
        self.active_window = index;
        if ((previous != index or announce_same) and self.isHostedAppWindow(index)) self.pushGuiEvent(index, .focus_gained);
    }

    fn isBoundGuiAppWindow(self: *const App, index: usize) bool {
        return index < self.windows.len and self.windows[index].kind == .app and self.windows[index].instance_id != 0;
    }

    fn pushGuiPointerEvent(self: *App, index: usize, kind: r4os.abi.GuiEventKind, x: i32, y: i32, buttons: u32) void {
        if (index >= self.windows.len) return;
        const instance_id = self.windows[index].instance_id;
        if (instance_id == 0) return;
        const client = self.windows[index].clientSurface().rect;
        if (!client.contains(x, y)) return;
        const event = r4os.abi.GuiEvent{
            .kind = @intFromEnum(kind),
            .window_id = @intCast(index),
            .x = x - client.x,
            .y = y - client.y,
            .buttons = buttons,
            .tick = self.ctx.ticks(),
        };
        _ = self.pushGuiEventBounded(instance_id, &event, kind != .mouse_move);
    }

    fn pushGuiKeyEvent(self: *App, index: usize, key: u32) void {
        if (index >= self.windows.len) return;
        const instance_id = self.windows[index].instance_id;
        if (instance_id == 0) return;
        const event = r4os.abi.GuiEvent{
            .kind = @intFromEnum(r4os.abi.GuiEventKind.key_down),
            .window_id = @intCast(index),
            .key = key,
            .modifiers = self.event.modifiers,
            .tick = self.ctx.ticks(),
        };
        _ = self.pushGuiEventBounded(instance_id, &event, true);
    }

    fn pushGuiPhysicalKeyEvent(self: *App, index: usize, usage: u32, down: bool) bool {
        if (index >= self.windows.len) return false;
        const instance_id = self.windows[index].instance_id;
        if (instance_id == 0) return false;
        const event = r4os.abi.GuiEvent{
            .kind = @intFromEnum(if (down) r4os.abi.GuiEventKind.physical_key_down else r4os.abi.GuiEventKind.physical_key_up),
            .window_id = @intCast(index),
            .key = usage,
            .tick = self.ctx.ticks(),
        };
        return self.pushGuiEventBounded(instance_id, &event, true);
    }

    fn pushGuiEventBounded(self: *App, instance_id: u32, event: *const r4os.abi.GuiEvent, ordering_sensitive: bool) bool {
        const attempts: usize = if (ordering_sensitive) 4 else 1;
        var attempt: usize = 0;
        while (attempt < attempts) : (attempt += 1) {
            const result = self.ctx.guiPushEvent(instance_id, event);
            if (result == 0) return true;
            if (result != -3 or attempt + 1 == attempts) return false;
            self.ctx.taskYield();
        }
        return false;
    }

    fn deliverGuiMouseMove(self: *App, x: i32, y: i32, buttons: u8) void {
        self.deliverGuiMouseMoveToTarget(self.targetAt(x, y), x, y, buttons);
    }

    fn deliverGuiMouseMoveToTarget(self: *App, target: model.UiTarget, x: i32, y: i32, buttons: u8) void {
        self.deliverGuiMouseButton(.mouse_move, target, x, y, buttons);
    }

    fn deliverGuiMouseButton(self: *App, kind: r4os.abi.GuiEventKind, target: model.UiTarget, x: i32, y: i32, buttons: u8) void {
        if (self.windowIndexForTarget(target)) |index| {
            if (self.isHostedAppWindow(index)) self.pushGuiPointerEvent(index, kind, x, y, buttons);
        }
    }

    fn deliverGuiKeyToActiveApp(self: *App, key: u32) bool {
        if (!self.isHostedAppWindow(self.active_window)) return false;
        if (!isAppKey(key)) return false;
        self.pushGuiKeyEvent(self.active_window, key);
        return true;
    }

    fn forwardPhysicalKeyToActiveApp(self: *App, input: r4os.abi.PhysicalKeyEvent) bool {
        if (!self.isHostedAppWindow(self.active_window)) return false;
        if (self.keyboard_focus != self.windowTargetForIndex(self.active_window)) return false;
        const instance_id = self.windows[self.active_window].instance_id;
        if (instance_id == 0) return false;
        const event = physical_input.toGuiEvent(input, @intCast(self.active_window)) orelse return false;
        return self.pushGuiEventBounded(instance_id, &event, true);
    }

    fn deliverTerminalKeyToActiveConsole(self: *App, key: u32) bool {
        if (self.active_window >= self.windows.len) return false;
        if (self.windows[self.active_window].kind != .terminal) return false;
        if (self.keyboard_focus != self.windowTargetForIndex(self.active_window)) return false;
        const instance_id = self.windows[self.active_window].instance_id;
        if (instance_id == 0 or !isConsoleKey(key)) return false;
        self.followConsoleTail(self.active_window);
        if (key == r4os.gui.Key.ctrl_v) return self.pasteClipboardToConsole(instance_id);
        if (isTextKey(key) and key >= 0x80) {
            var encoded: [4]u8 = undefined;
            const len = encodeUtf8Codepoint(key, &encoded);
            if (len == 0) return false;
            return self.pushConsoleInputBounded(instance_id, encoded[0..len]);
        }
        return self.ctx.consolePushKey(instance_id, @intCast(key)) == 0;
    }

    fn handleTerminalScrollKey(self: *App, key: u8) bool {
        const index = self.activeConsoleWindowIndex() orelse return false;
        switch (key) {
            r4os.gui.Key.page_up => return self.scrollConsoleByPage(index, 1),
            r4os.gui.Key.page_down => return self.scrollConsoleByPage(index, -1),
            r4os.gui.Key.end => {
                if (self.console_scrolls[index].offset == 0) return false;
                self.followConsoleTail(index);
                self.invalidateConsoleView(index);
                return true;
            },
            r4os.gui.Key.home => {
                if (self.console_scrolls[index].offset == 0) return false;
                return self.scrollConsoleBy(index, @intCast(console_scroll_max_fallback));
            },
            else => return false,
        }
    }

    fn handleMouseWheel(self: *App, x: i32, y: i32, wheel: i32) bool {
        if (self.terminal_mode) return self.scrollConsoleBy(0, signedScrollLines(wheel, console_scroll_wheel_lines));
        if (self.volume_ui.installed and (self.volumeHit(x, y) or
            (self.volume_ui.popup_open and draw.volumePopupRect(self.screen_w, self.screen_h, self.config.taskbar_clock).contains(x, y))))
        {
            return self.adjustVolume(if (wheel > 0) 1 else -1);
        }
        if (self.dialog == .tasks) {
            return self.scrollTaskInventory(if (wheel > 0) -3 else 3);
        }
        if (self.tray_registry.hit(x, y)) |identity| {
            if (self.submitTrayActivation(identity, r4os.abi.tray_event_kind_wheel, wheel, x, y, self.ctx.ticks())) {
                self.invalidateTaskbar();
                return true;
            }
        }
        const target = self.targetAt(x, y);
        const index = self.windowIndexForTarget(target) orelse return false;
        if (self.windows[index].kind != .terminal) return false;
        return self.scrollConsoleBy(index, signedScrollLines(wheel, console_scroll_wheel_lines));
    }

    fn handleVolumeKey(self: *App, key: u8) bool {
        switch (key) {
            r4os.gui.Key.escape => self.closeTop(),
            r4os.gui.Key.left, r4os.gui.Key.down => _ = self.adjustVolume(-1),
            r4os.gui.Key.right, r4os.gui.Key.up => _ = self.adjustVolume(1),
            r4os.gui.Key.home => _ = self.setVolumePercent(0, true),
            r4os.gui.Key.end => _ = self.setVolumePercent(100, true),
            ' ' => _ = self.toggleVolumeMute(),
            else => return false,
        }
        return true;
    }

    fn handleTaskInventoryKey(self: *App, key: u8) bool {
        switch (key) {
            0x80 => return self.scrollTaskInventory(-1),
            0x81 => return self.scrollTaskInventory(1),
            r4os.gui.Key.page_up => return self.scrollTaskInventory(-@as(i32, @intCast(task_inventory_visible_rows))),
            r4os.gui.Key.page_down => return self.scrollTaskInventory(@intCast(task_inventory_visible_rows)),
            r4os.gui.Key.home => {
                if (self.task_scroll == 0) return true;
                self.task_scroll = 0;
                self.invalidateDialog();
                return true;
            },
            r4os.gui.Key.end => {
                const max_scroll = self.program_instances.items.len -| task_inventory_visible_rows;
                if (self.task_scroll == max_scroll) return true;
                self.task_scroll = max_scroll;
                self.invalidateDialog();
                return true;
            },
            else => return false,
        }
    }

    fn activeConsoleWindowIndex(self: *const App) ?usize {
        if (self.terminal_mode) {
            if (self.windows[0].kind == .terminal and self.windows[0].instance_id != 0) return 0;
            return null;
        }
        if (self.active_window >= self.windows.len) return null;
        if (self.windows[self.active_window].kind != .terminal) return null;
        if (self.windows[self.active_window].instance_id == 0) return null;
        if (self.keyboard_focus != self.windowTargetForIndex(self.active_window)) return null;
        return self.active_window;
    }

    fn scrollConsoleByPage(self: *App, index: usize, direction: i32) bool {
        const page = @max(@as(u32, 1), self.consoleVisibleRows(index) -| 1);
        return self.scrollConsoleBy(index, direction * @as(i32, @intCast(page)));
    }

    fn scrollConsoleBy(self: *App, index: usize, delta_lines: i32) bool {
        if (index >= self.windows.len or index >= self.console_scrolls.len) return false;
        if (self.windows[index].kind != .terminal or self.windows[index].instance_id == 0) return false;
        const max_offset = self.consoleMaxScrollOffset(index);
        const current = self.console_scrolls[index].offset;
        const next = if (delta_lines > 0)
            @min(max_offset, current +| @as(u32, @intCast(delta_lines)))
        else if (delta_lines < 0)
            current -| @as(u32, @intCast(-delta_lines))
        else
            current;
        if (next == current and (next != 0 or self.console_scrolls[index].follow_tail)) return true;
        self.console_scrolls[index].offset = next;
        self.console_scrolls[index].follow_tail = next == 0;
        self.invalidateConsoleView(index);
        return true;
    }

    fn followConsoleTail(self: *App, index: usize) void {
        if (index >= self.console_scrolls.len) return;
        if (self.console_scrolls[index].offset == 0 and self.console_scrolls[index].follow_tail) return;
        self.console_scrolls[index].offset = 0;
        self.console_scrolls[index].follow_tail = true;
        self.invalidateConsoleView(index);
    }

    fn resetConsoleScroll(self: *App, index: usize, instance_id: u32) void {
        if (index >= self.console_scrolls.len) return;
        self.console_scrolls[index] = .{ .instance_id = instance_id };
    }

    fn syncConsoleScrollState(self: *App, index: usize, instance_id: u32, state: r4os.abi.ConsoleState) void {
        if (index >= self.console_scrolls.len) return;
        const scroll = &self.console_scrolls[index];
        if (scroll.instance_id != instance_id or scroll.clear_count != state.clear_count) {
            scroll.* = .{ .instance_id = instance_id, .clear_count = state.clear_count };
            return;
        }
        if (scroll.follow_tail) {
            scroll.offset = 0;
        } else {
            scroll.offset = self.clampConsoleOffset(index, scroll.offset, state);
            scroll.follow_tail = scroll.offset == 0;
        }
    }

    fn consoleMaxScrollOffset(self: *App, index: usize) u32 {
        var state: r4os.abi.ConsoleState = .{};
        if (index >= self.windows.len or self.windows[index].instance_id == 0) return 0;
        if (self.ctx.consoleState(self.windows[index].instance_id, &state) < 0) return console_scroll_max_fallback;
        return maxScrollOffsetFromState(state);
    }

    fn consoleVisibleRows(self: *App, index: usize) u32 {
        var state: r4os.abi.ConsoleState = .{};
        if (index >= self.windows.len or self.windows[index].instance_id == 0) return 10;
        if (self.ctx.consoleState(self.windows[index].instance_id, &state) < 0) return 10;
        return @max(@as(u32, 1), state.rows);
    }

    fn clampConsoleOffset(self: *App, index: usize, offset: u32, state: r4os.abi.ConsoleState) u32 {
        _ = self;
        _ = index;
        return @min(offset, maxScrollOffsetFromState(state));
    }

    fn invalidateConsoleView(self: *App, index: usize) void {
        if (self.terminal_mode and index == 0) {
            self.invalidateFull();
        } else {
            self.invalidateWindow(index);
        }
    }

    fn terminalCloseHotkeyApplies(self: *const App) bool {
        if (self.terminal_mode) return self.windows[0].kind == .terminal and self.windows[0].instance_id != 0;
        if (self.dialog != .none or self.start_open) return false;
        if (self.active_window >= self.windows.len) return false;
        if (self.windows[self.active_window].kind != .terminal) return false;
        if (self.windows[self.active_window].instance_id == 0) return false;
        return self.keyboard_focus == self.windowTargetForIndex(self.active_window);
    }

    fn requestTerminalProgramsClose(self: *App) bool {
        const now = self.ctx.ticks();
        var requested = false;
        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].kind != .terminal) continue;
            if (self.windows[i].instance_id == 0) continue;
            if (self.requestWindowProcessClose(i) != r4os.abi.program_handle_ok) continue;
            self.windows[i].requestClose(now);
            self.invalidateWindow(i);
            requested = true;
        }
        if (requested) self.invalidateTaskbar();
        return requested;
    }

    fn pasteClipboardToConsole(self: *App, instance_id: u32) bool {
        var data: [clipboard_buffer_size]u8 = .{0} ** clipboard_buffer_size;
        const len = self.ctx.clipboardRead(data[0..]);
        if (len <= 0) return true;
        var previous_cr = false;
        var write_index: usize = 0;
        var i: usize = 0;
        while (i < @as(usize, @intCast(len))) : (i += 1) {
            const ch = data[i];
            if (ch == 0) break;
            if (ch == '\r') {
                data[write_index] = r4os.gui.Key.enter;
                write_index += 1;
                previous_cr = true;
                continue;
            }
            if (ch == '\n') {
                if (!previous_cr) {
                    data[write_index] = r4os.gui.Key.enter;
                    write_index += 1;
                }
                previous_cr = false;
                continue;
            }
            previous_cr = false;
            if (isTextKey(ch)) {
                data[write_index] = ch;
                write_index += 1;
            } else if (ch == r4os.gui.Key.tab) {
                // Terminal input has no tab-editing state; preserve the text
                // boundary explicitly as one ordinary space.
                data[write_index] = ' ';
                write_index += 1;
            }
        }
        return self.pushConsoleInputBounded(instance_id, data[0..write_index]);
    }

    fn pushConsoleInputBounded(self: *App, instance_id: u32, data: []const u8) bool {
        if (data.len == 0) return true;
        var offset: usize = 0;
        var calls: usize = 0;
        while (offset < data.len and calls < 64) : (calls += 1) {
            const result = self.ctx.consolePushInput(instance_id, data[offset..]);
            if (result == r4os.abi.err_no_fn) {
                for (data[offset..]) |ch| {
                    if (self.ctx.consolePushKey(instance_id, ch) != 0) return false;
                }
                return true;
            }
            if (result < 0) return false;
            const accepted: usize = @intCast(result);
            if (accepted > data.len - offset) return false;
            if (accepted == 0) {
                self.ctx.taskYield();
                continue;
            }
            offset += accepted;
            if (offset < data.len) self.ctx.taskYield();
        }
        return offset == data.len;
    }

    fn pasteClipboardToRunPath(self: *App) bool {
        var data: [clipboard_buffer_size]u8 = .{0} ** clipboard_buffer_size;
        const len = self.ctx.clipboardRead(data[0..]);
        if (len <= 0) return false;
        var changed = false;
        var i: usize = 0;
        while (i < @as(usize, @intCast(len))) : (i += 1) {
            const ch = data[i];
            if (ch == 0) break;
            if (!isTextKey(ch)) continue;
            changed = self.run_path.append(ch) or changed;
        }
        return changed;
    }

    fn isHostedAppWindow(self: *const App, index: usize) bool {
        return index < self.windows.len and
            self.windows[index].kind == .app and
            self.windows[index].instance_id != 0 and
            self.windows[index].visible and
            !self.windows[index].minimized;
    }

    fn guiWindowInfoForIndex(self: *const App, index: usize) r4os.abi.GuiWindowInfo {
        const win = &self.windows[index];
        const frame = win.geometry();
        const client = win.clientSurface().rect;
        var flags: u32 = 0;
        if (win.visible) flags |= r4os.abi.GuiWindowFlag.visible;
        if (win.minimized) flags |= r4os.abi.GuiWindowFlag.minimized;
        if (win.maximized) flags |= r4os.abi.GuiWindowFlag.maximized;
        if (win.close_requested) flags |= r4os.abi.GuiWindowFlag.close_requested;
        return .{
            .window_id = @intCast(index),
            .frame_x = frame.x,
            .frame_y = frame.y,
            .frame_w = frame.w,
            .frame_h = frame.h,
            .client_x = client.x,
            .client_y = client.y,
            .client_w = client.w,
            .client_h = client.h,
            .flags = flags,
        };
    }

    fn altF4(self: *App) void {
        if (self.dialog != .none or self.start_open or self.time_menu_open) {
            self.closeTop();
        } else if (self.active_window < self.windows.len and self.windows[self.active_window].visible) {
            self.closeWindow(self.active_window);
        }
    }

    fn menuUp(self: *App) void {
        if (!self.start_open) return;
        if (self.menu_nested_focus and self.menu_nested_open) {
            const nested = self.menu.nestedSubmenu(self.menu_submenu_parent, self.menu_nested_parent) orelse return;
            if (nested.count == 0) return;
            if (self.menu_nested_selected == 0) {
                self.menu_nested_selected = nested.count - 1;
            } else {
                self.menu_nested_selected -= 1;
            }
            self.invalidateStartMenu();
            return;
        }
        if (self.menu_submenu_focus and self.menu_submenu_open) {
            const submenu = self.menu.submenu(self.menu_submenu_parent) orelse return;
            if (submenu.count == 0) return;
            if (self.menu_submenu_selected == 0) {
                self.menu_submenu_selected = submenu.count - 1;
            } else {
                self.menu_submenu_selected -= 1;
            }
            self.syncNestedForSelected(false);
            self.invalidateStartMenu();
            return;
        }
        if (self.menu.count == 0) return;
        if (self.menu_selected == 0) {
            self.menu_selected = self.menu.count - 1;
        } else {
            self.menu_selected -= 1;
        }
        self.menu_submenu_focus = false;
        self.menu_nested_focus = false;
        self.menu_nested_open = false;
        if (self.menu.hasSubmenu(self.menu_selected)) {
            self.openStartSubmenu(self.menu_selected, false);
        } else {
            self.menu_submenu_open = false;
        }
        self.invalidateStartMenu();
    }

    fn menuDown(self: *App) void {
        if (!self.start_open) return;
        if (self.menu_nested_focus and self.menu_nested_open) {
            const nested = self.menu.nestedSubmenu(self.menu_submenu_parent, self.menu_nested_parent) orelse return;
            if (nested.count == 0) return;
            self.menu_nested_selected = (self.menu_nested_selected + 1) % nested.count;
            self.invalidateStartMenu();
            return;
        }
        if (self.menu_submenu_focus and self.menu_submenu_open) {
            const submenu = self.menu.submenu(self.menu_submenu_parent) orelse return;
            if (submenu.count == 0) return;
            self.menu_submenu_selected = (self.menu_submenu_selected + 1) % submenu.count;
            self.syncNestedForSelected(false);
            self.invalidateStartMenu();
            return;
        }
        if (self.menu.count == 0) return;
        self.menu_selected = (self.menu_selected + 1) % self.menu.count;
        self.menu_submenu_focus = false;
        self.menu_nested_focus = false;
        self.menu_nested_open = false;
        if (self.menu.hasSubmenu(self.menu_selected)) {
            self.openStartSubmenu(self.menu_selected, false);
        } else {
            self.menu_submenu_open = false;
        }
        self.invalidateStartMenu();
    }

    fn menuLeft(self: *App) void {
        if (!self.start_open) return;
        if (self.menu_nested_focus) {
            self.menu_nested_focus = false;
            self.menu_submenu_focus = true;
            self.menu_submenu_selected = self.menu_nested_parent;
            self.invalidateStartMenu();
            return;
        }
        if (self.menu_submenu_focus) {
            self.menu_submenu_focus = false;
            self.menu_nested_open = false;
            self.menu_selected = self.menu_submenu_parent;
            self.invalidateStartMenu();
        }
    }

    fn menuRight(self: *App) void {
        if (!self.start_open) return;
        if (self.menu_nested_focus) return;
        if (self.menu_submenu_focus) {
            if (self.menu.submenuHasSubmenu(self.menu_submenu_parent, self.menu_submenu_selected)) {
                self.menu_nested_open = true;
                self.menu_nested_parent = self.menu_submenu_selected;
                self.menu_nested_selected = 0;
                self.menu_nested_focus = true;
                self.menu_submenu_focus = false;
                self.invalidateStartMenu();
            }
            return;
        }
        if (self.menu.hasSubmenu(self.menu_selected)) self.openStartSubmenu(self.menu_selected, true);
    }

    fn openStartSubmenu(self: *App, parent: usize, focus_child: bool) void {
        if (!self.menu.hasSubmenu(parent)) return;
        self.menu_submenu_open = true;
        self.menu_submenu_parent = parent;
        self.menu_submenu_selected = 0;
        self.menu_submenu_focus = focus_child;
        self.menu_nested_open = false;
        self.menu_nested_focus = false;
        self.invalidateStartMenu();
    }

    fn syncNestedForSelected(self: *App, focus_nested: bool) void {
        if (self.menu.submenuHasSubmenu(self.menu_submenu_parent, self.menu_submenu_selected)) {
            self.menu_nested_open = true;
            self.menu_nested_parent = self.menu_submenu_selected;
            self.menu_nested_selected = 0;
            self.menu_nested_focus = focus_nested;
        } else {
            self.menu_nested_open = false;
            self.menu_nested_focus = false;
        }
    }

    fn nextWindow(self: *App) void {
        const old_active = self.active_window;
        var candidate = self.active_window;
        var step: usize = 0;
        while (step < self.windows.len) : (step += 1) {
            candidate = (candidate + 1) % self.windows.len;
            if (self.windows[candidate].visible) {
                self.windows[candidate].restore();
                self.activateWindow(candidate, false);
                self.keyboard_focus = self.windowTargetForIndex(candidate);
                self.invalidateWindow(old_active);
                self.invalidateWindow(candidate);
                self.invalidateTaskbar();
                return;
            }
        }
    }

    fn previousWindow(self: *App) void {
        const old_active = self.active_window;
        var candidate = self.active_window;
        var step: usize = 0;
        while (step < self.windows.len) : (step += 1) {
            candidate = if (candidate == 0) self.windows.len - 1 else candidate - 1;
            if (self.windows[candidate].visible) {
                self.windows[candidate].restore();
                self.activateWindow(candidate, false);
                self.keyboard_focus = self.windowTargetForIndex(candidate);
                self.invalidateWindow(old_active);
                self.invalidateWindow(candidate);
                self.invalidateTaskbar();
                return;
            }
        }
    }

    fn toggleStart(self: *App) void {
        const was_start_open = self.start_open;
        const was_system_menu_open = self.system_menu_open;
        const was_time_menu_open = self.time_menu_open;
        const was_volume_popup_open = self.volume_ui.popup_open;
        const old_dialog = self.dialog;
        self.start_open = !self.start_open;
        self.system_menu_open = false;
        self.time_menu_open = false;
        self.volume_ui.popup_open = false;
        self.volume_dragging = false;
        self.dialog = .none;
        self.dialog_focus = .none;
        self.menu_submenu_open = false;
        self.menu_submenu_focus = false;
        self.menu_submenu_parent = self.menu_selected;
        self.menu_submenu_selected = 0;
        self.keyboard_focus = if (self.start_open) .start_button else self.activeWindowTargetOrNone();
        if (was_system_menu_open) self.invalidateSystemMenu();
        if (was_time_menu_open) self.invalidateTimeMenu();
        if (was_volume_popup_open) self.invalidateVolumePopup();
        self.invalidateDialogFor(old_dialog);
        if (was_start_open or self.start_open) self.invalidateStartMenu();
        self.invalidateTaskbar();
    }

    fn focusOrRestore(self: *App, index: usize) void {
        const old_active = self.active_window;
        self.windows[index].restore();
        self.activateWindow(index, false);
        self.keyboard_focus = self.windowTargetForIndex(index);
        self.mirrorWindowFocus(index);
        self.invalidateWindow(old_active);
        self.invalidateWindow(index);
        self.invalidateTaskbar();
    }

    fn invalidateWindow(self: *App, index: usize) void {
        if (index >= self.windows.len) return;
        if (!self.windows[index].visible) return;
        self.damage.invalidateSurface(self.windows[index].frameSurface());
    }

    fn invalidateWindowClient(self: *App, index: usize) void {
        if (index >= self.windows.len) return;
        if (!self.windows[index].visible) return;
        self.damage.invalidateSurface(self.windows[index].clientSurface());
    }

    fn invalidateConsoleCursors(self: *App) bool {
        var changed = false;
        if (self.terminal_mode and self.windows[0].instance_id != 0) {
            if (self.consoleCursorRectForWindow(0, true)) |rect| {
                self.damage.invalidate(rect);
                changed = true;
            }
            return changed;
        }

        var i: usize = 0;
        while (i < self.windows.len) : (i += 1) {
            if (self.windows[i].kind != .terminal) continue;
            if (self.windows[i].instance_id == 0 or !self.windows[i].visible or self.windows[i].minimized) continue;
            if (self.consoleCursorRectForWindow(i, false)) |rect| {
                self.damage.invalidate(rect);
                changed = true;
            }
        }
        return changed;
    }

    fn consoleCursorRectForWindow(self: *App, index: usize, fullscreen: bool) ?surface.Rect {
        if (index >= self.windows.len) return null;
        const instance_id = self.windows[index].instance_id;
        if (instance_id == 0) return null;
        const snapshot = &self.console_snapshots[index];
        if (!snapshot.valid or snapshot.instance_id != instance_id) {
            self.render_stats.console_cursor_cache_misses +%= 1;
            return null;
        }
        if (snapshot.effective_offset != 0 or snapshot.state.cursor_visible == 0) return null;
        const bounds: surface.Rect = if (fullscreen)
            .{ .x = 0, .y = 0, .w = self.screen_w, .h = self.screen_h }
        else
            self.windows[index].clientSurface().rect;
        const rect = draw.terminalCursorRect(bounds, snapshot.state, self.config.terminal_font_size);
        if (rect.isEmpty()) return null;
        return rect;
    }

    fn refreshConsoleSnapshots(self: *App) void {
        var index: usize = 0;
        while (index < self.windows.len and index < self.console_snapshots.len) : (index += 1) {
            const win = &self.windows[index];
            const visible = if (self.terminal_mode)
                index == 0 and win.instance_id != 0
            else
                win.kind == .terminal and win.instance_id != 0 and win.visible and !win.minimized;
            if (!visible) {
                self.console_snapshots[index].invalidate();
                continue;
            }
            const rect: surface.Rect = if (self.terminal_mode and index == 0)
                .{ .x = 0, .y = 0, .w = self.screen_w, .h = self.screen_h }
            else
                win.clientSurface().rect;
            const scroll_offset = self.console_scrolls[index].offset;
            const snapshot = &self.console_snapshots[index];
            if (snapshot.matches(win.instance_id, win.gui_revision, rect, self.config.terminal_font_size, self.config.terminal_codepage, scroll_offset)) {
                self.render_stats.console_snapshot_hits +%= 1;
                continue;
            }
            const stats = draw.refreshTerminalSnapshot(
                self.ctx,
                snapshot,
                win.instance_id,
                win.gui_revision,
                rect,
                self.config.terminal_font_size,
                self.config.terminal_codepage,
                scroll_offset,
            );
            self.render_stats.console_snapshot_refreshes +%= 1;
            self.render_stats.console_state_reads +%= stats.state_reads;
            self.render_stats.console_output_bytes +%= stats.output_bytes;
            self.render_stats.console_parse_bytes +%= stats.parsed_bytes;
            self.render_stats.console_visible_lines +%= stats.visible_lines;
        }
    }

    fn invalidateTaskbar(self: *App) void {
        self.taskbar_damage = true;
    }

    fn taskbarRect(self: *const App) surface.Rect {
        return surface.taskbar(self.screen_w, self.screen_h, theme.taskbar_h).rect;
    }

    fn invalidateStartMenu(self: *App) void {
        var rect = surface.Rect{
            .x = 0,
            .y = self.screen_h - theme.taskbar_h - theme.menu_h,
            .w = theme.menu_w,
            .h = theme.menu_h,
        };
        const possible_flyout = surface.Rect{
            .x = theme.menu_w - 4,
            .y = self.screen_h - theme.taskbar_h - theme.menu_h,
            .w = @min(start_menu.submenu_w, @max(0, self.screen_w - (theme.menu_w - 4))),
            .h = theme.menu_h,
        };
        rect = rect.merged(possible_flyout);
        if (self.menu_submenu_open) {
            if (self.menu.submenuRect(self.screen_w, self.screen_h, self.menu_submenu_parent)) |submenu_rect| {
                rect = rect.merged(submenu_rect);
            }
            if (self.menu_nested_open) {
                if (self.menu.nestedRect(self.screen_w, self.screen_h, self.menu_submenu_parent, self.menu_nested_parent)) |nested_rect| {
                    rect = rect.merged(nested_rect);
                }
            }
        }
        self.damage.invalidate(rect);
    }

    fn invalidateSystemMenu(self: *App) void {
        self.damage.invalidate(.{
            .x = self.system_menu_x,
            .y = self.system_menu_y,
            .w = system_menu_w,
            .h = system_menu_h,
        });
    }

    fn invalidateTimeMenu(self: *App) void {
        self.damage.invalidate(draw.timeMenuRect(self.screen_w, self.screen_h));
    }

    fn invalidateVolumePopup(self: *App) void {
        self.damage.invalidate(draw.volumePopupRect(self.screen_w, self.screen_h, self.config.taskbar_clock));
    }

    fn invalidateDialog(self: *App) void {
        const rect = self.dialogRect() orelse return;
        self.damage.invalidate(rect);
    }

    fn invalidateDialogFor(self: *App, dialog: Dialog) void {
        const rect = self.dialogRectFor(dialog) orelse return;
        self.damage.invalidate(rect);
    }

    fn desktopGridDragIndex(self: *const App) usize {
        if (self.desktop_drag.active and self.desktop_drag.index < self.desktop_items.count) return self.desktop_drag.index;
        return desktop_items.no_selection;
    }

    fn invalidateDesktopGrid(self: *App) void {
        self.damage.invalidate(surface.workArea(self.screen_w, self.screen_h, theme.taskbar_h));
    }

    fn invalidateDesktopGridSlot(self: *App, position: desktop_items.GridPosition) void {
        const work_bottom = self.screen_h - theme.taskbar_h;
        if (self.screen_w <= 0 or work_bottom <= 0) return;
        const x = clamp(position.x, 0, self.screen_w);
        const y = clamp(position.y, 0, work_bottom);
        self.damage.invalidate(.{
            .x = x,
            .y = y,
            .w = @max(0, @min(desktop_items.cell_w, self.screen_w - x)),
            .h = @max(0, @min(desktop_items.cell_h, work_bottom - y)),
        });
    }

    fn invalidateDesktopItem(self: *App, index: usize) void {
        const rect = self.desktopItemRect(index) orelse return;
        self.damage.invalidate(rect);
    }

    fn desktopItemRect(self: *const App, index: usize) ?surface.Rect {
        if (index >= self.desktop_items.count) return null;
        const entry = &self.desktop_items.entries[index];
        return .{
            .x = entry.x,
            .y = entry.y,
            .w = desktop_items.icon_w,
            .h = desktop_items.icon_h,
        };
    }

    fn invalidateHoverChange(self: *App, old_target: model.UiTarget, new_target: model.UiTarget) void {
        if (self.system_menu_open) {
            self.invalidateSystemMenu();
            return;
        }
        if (self.time_menu_open) {
            self.invalidateTimeMenu();
            return;
        }
        if (self.volume_ui.popup_open) {
            self.invalidateVolumePopup();
            if (old_target == .taskbar_volume or new_target == .taskbar_volume) self.invalidateTaskbar();
            return;
        }
        if (self.dialog != .none) {
            self.invalidateDialog();
            return;
        }
        if (self.start_open) {
            self.invalidateStartMenu();
            if (old_target == .start_button or new_target == .start_button) self.invalidateTaskbar();
            return;
        }

        self.invalidateTargetVisual(old_target);
        self.invalidateTargetVisual(new_target);
    }

    fn invalidatePointerRelease(self: *App, pressed_target: model.UiTarget, was_drag: bool, drag_index: usize, was_desktop_drag: bool, desktop_drag_index: usize, was_resize: bool, resize_index: usize) void {
        if (was_drag) {
            self.invalidateWindow(drag_index);
            return;
        }
        if (was_desktop_drag) {
            self.invalidateDesktopItem(desktop_drag_index);
            return;
        }
        if (was_resize) {
            self.invalidateWindow(resize_index);
            return;
        }
        self.invalidateTargetVisual(pressed_target);
    }

    fn invalidateTargetVisual(self: *App, target: model.UiTarget) void {
        if (target == .volume_popup_backdrop or target == .volume_popup_slider or target == .volume_popup_mute) {
            self.invalidateVolumePopup();
            return;
        }
        switch (model.ownerForTarget(target)) {
            .none => {},
            .start_menu => self.invalidateStartMenu(),
            .dialog => self.invalidateDialog(),
            .window => {
                if (self.windowIndexForTarget(target)) |index| self.invalidateWindow(index);
            },
            .taskbar => self.invalidateTaskbar(),
            .desktop => {
                if (desktop_items.indexForTarget(target)) |index| self.invalidateDesktopItem(index);
            },
        }
    }

    fn invalidateCursor(self: *App) void {
        self.damage.invalidateSurface(surface.cursor(self.cursor_x, self.cursor_y, self.screen_w, self.screen_h));
    }

    fn queueCursorDamage(self: *App, old_rect: surface.Rect, new_rect: surface.Rect) void {
        if (self.cursor_damage.active) {
            self.cursor_damage.old_rect = self.cursor_damage.old_rect.merged(old_rect);
            self.cursor_damage.new_rect = new_rect;
        } else {
            self.cursor_damage = .{ .active = true, .old_rect = old_rect, .new_rect = new_rect };
            self.cursor_damage_queued_tick = self.ctx.ticks();
        }
    }

    fn invalidateFull(self: *App) void {
        self.damage.invalidateFull(self.screen_w, self.screen_h);
    }

    fn loadMenuConfig(self: *App) void {
        var buffer: [1024]u8 = undefined;
        const len = self.ctx.fileRead(menu_config_path, buffer[0..]);
        if (len <= 0) return;
        if (self.menu.loadFromBytes(buffer[0..@intCast(len)])) {
            self.menu_selected = 0;
            self.menu_submenu_open = false;
            self.menu_submenu_focus = false;
            self.menu_submenu_parent = 0;
            self.menu_submenu_selected = 0;
        }
    }

    fn loadAssociationConfig(self: *App) void {
        self.assoc = r4std.app_assoc.Config.initDefault();
        self.assoc_loaded_from_file = false;
        var buffer: [assoc_config_max_bytes]u8 = undefined;
        const len = self.ctx.fileRead(r4std.settings.paths.assoc, buffer[0..]);
        if (len > 0) {
            self.assoc_loaded_from_file = self.assoc.loadFromBytes(buffer[0..@intCast(len)]);
        }
    }

    fn loadDesktopItemsFolder(self: *App) void {
        var next = desktop_items.Items{};
        var path_buf: [desktop_items.path_max + 1]u8 = .{0} ** (desktop_items.path_max + 1);
        var index: u32 = desktop_folder_first_index;
        var scanned: u32 = 0;
        while (scanned < desktop_folder_scan_limit) : (scanned += 1) {
            @memset(path_buf[0..], 0);
            const kind = self.ctx.dirEntry(desktop_folder.default_dir, index, path_buf[0 .. path_buf.len - 1]);
            if (kind < 0) break;
            const path = spanZ(path_buf[0..]);
            if (path.len != 0) {
                const fs_kind: desktop_items.FsKind = if (kind == 1) .directory else .file;
                var item = desktop_items.Item.init(path, fs_kind);
                if (fs_kind == .file and endsWithIgnoreCase(path, ".LNK")) self.applyDesktopLinkFile(&item);
                _ = next.add(item);
            }
            index += 1;
        }
        next.sortByTitle();
        const layout_state = self.loadDesktopLayout();
        next.layoutWith(&layout_state, self.screen_w, self.screen_h, theme.taskbar_h);
        self.desktop_items = next;
        self.desktop_item_selected = desktop_items.no_selection;
    }

    fn loadDesktopLayout(self: *App) desktop_layout.Layout {
        self.recoverDesktopLayoutSave();
        var layout_state = desktop_layout.Layout{};
        var buffer: [desktop_layout.max_bytes]u8 = undefined;
        const len = self.ctx.fileRead(desktop_layout_path, buffer[0..]);
        if (len <= 0) return layout_state;
        if (!layout_state.loadFromBytes(buffer[0..@intCast(len)])) {
            self.reportInvalidDesktopLayout();
            self.markDesktopLayoutDirty();
            return desktop_layout.Layout{};
        }
        if (layout_state.parse_errors or layout_state.truncated or layout_state.duplicate_paths) {
            self.reportInvalidDesktopLayout();
            self.markDesktopLayoutDirty();
        }
        return layout_state;
    }

    fn recoverDesktopLayoutSave(self: *App) void {
        const recovery = r4std.config.recoverDocumentSave(self.ctx, desktop_layout_path);
        if (recovery == r4std.config.result_recovered and !self.desktop_layout_recovery_reported) {
            self.ctx.println("Desktop layout recovery: recovered atomic save");
            self.desktop_layout_recovery_reported = true;
        } else if (recovery < 0 and !self.desktop_layout_recovery_failed_reported) {
            self.ctx.print("Desktop layout recovery: FAILED rc=");
            self.ctx.printI32(recovery);
            self.ctx.println("");
            self.desktop_layout_recovery_failed_reported = true;
        }
    }

    fn reportInvalidDesktopLayout(self: *App) void {
        if (self.desktop_layout_invalid_reported) return;
        self.ctx.println("Desktop layout load: invalid, canonical rewrite scheduled");
        self.desktop_layout_invalid_reported = true;
    }

    fn markDesktopLayoutDirty(self: *App) void {
        self.desktop_layout_generation +%= 1;
        self.desktop_layout_writeback.markDirty(self.ctx.ticks());
    }

    fn flushDesktopLayoutIfDue(self: *App) bool {
        const changed = self.pollDesktopLayoutAsyncSave();
        const now = self.ctx.ticks();
        if (!self.desktop_layout_writeback.isDirty() or !self.desktop_layout_writeback.isDue(now)) return changed;
        if (self.desktop_layout_async.in_flight) return changed;
        return self.startDesktopLayoutAsyncSave(now) or changed;
    }

    fn flushDesktopLayoutBeforeSystemAction(self: *App) void {
        _ = self.flushDesktopLayoutNow();
    }

    fn flushDesktopLayoutNow(self: *App) bool {
        var changed = self.pollDesktopLayoutAsyncSave();
        if (self.desktop_layout_async.in_flight) changed = self.waitDesktopLayoutAsyncSave() or changed;
        if (!self.desktop_layout_writeback.isDirty()) return changed;
        if (self.startDesktopLayoutAsyncSave(self.ctx.ticks()) and self.desktop_layout_async.in_flight) {
            changed = self.waitDesktopLayoutAsyncSave() or changed;
        }
        return changed;
    }

    fn handleDesktopLayoutWritebackResult(self: *App, result: r4std.settings.WritebackFlush) bool {
        if (result.action == .saved and result.result_code == r4std.config.result_recovered and !self.desktop_layout_recovery_reported) {
            self.ctx.println("Desktop layout save: recovered atomic leftovers");
            self.desktop_layout_recovery_reported = true;
        } else if (result.action == .saved and result.recovered_after_failure) {
            self.ctx.println("Desktop layout save: OK");
        }
        if (result.action == .failed and result.first_failure) {
            self.ctx.print("Desktop layout save: FAILED rc=");
            self.ctx.printI32(result.result_code);
            self.ctx.println("");
        }
        return result.attempted();
    }

    fn startDesktopLayoutAsyncSave(self: *App, now: u64) bool {
        if (self.desktop_layout_async.in_flight) return false;
        if (!self.ctx.sys.hasFn("thread_create_handle") or !self.ctx.sys.hasFn("thread_handle_join")) {
            self.render_stats.layout_worker_errors +%= 1;
            const result = self.desktop_layout_writeback.complete(now, r4std.config.error_write_failed);
            return self.handleDesktopLayoutWritebackResult(result);
        }

        const bytes = self.composeDesktopLayout(self.desktop_layout_async.bytes[0..]);
        if (bytes.len == 0) {
            self.render_stats.layout_worker_errors +%= 1;
            const result = self.desktop_layout_writeback.complete(now, r4std.config.error_buffer_too_small);
            return self.handleDesktopLayoutWritebackResult(result);
        }

        self.desktop_layout_async.sys = self.ctx.sys;
        self.desktop_layout_async.len = bytes.len;
        self.desktop_layout_async.generation = self.desktop_layout_generation;
        self.desktop_layout_async.thread_handle = .{};
        self.desktop_layout_async.started_tick = now;
        var thread_handle: r4os.abi.ProgramJoinHandle = .{};
        const rc = self.ctx.sys.threadCreateHandle(desktopLayoutSaveWorker, @intFromPtr(&self.desktop_layout_async), desktop_layout_save_stack, 0, &thread_handle);
        if (rc != r4os.abi.thread_ok) {
            self.render_stats.layout_worker_errors +%= 1;
            self.desktop_layout_async.len = 0;
            self.desktop_layout_async.started_tick = 0;
            const result = self.desktop_layout_writeback.complete(now, r4std.config.error_write_failed);
            return self.handleDesktopLayoutWritebackResult(result);
        }
        self.desktop_layout_async.thread_handle = thread_handle;
        self.desktop_layout_async.in_flight = true;
        self.render_stats.layout_worker_started +%= 1;
        return false;
    }

    fn pollDesktopLayoutAsyncSave(self: *App) bool {
        return if (self.joinDesktopLayoutAsyncSave(0)) |result| self.handleDesktopLayoutWritebackResult(result) else false;
    }

    fn waitDesktopLayoutAsyncSave(self: *App) bool {
        return if (self.joinDesktopLayoutAsyncSave(r4os.abi.thread_wait_forever)) |result| self.handleDesktopLayoutWritebackResult(result) else false;
    }

    fn joinDesktopLayoutAsyncSave(self: *App, timeout_ticks: u64) ?r4std.settings.WritebackFlush {
        if (!self.desktop_layout_async.in_flight) return null;
        var exit_code: i32 = 0;
        const rc = self.ctx.sys.threadHandleJoin(&self.desktop_layout_async.thread_handle, timeout_ticks, &exit_code);
        if (rc == r4os.abi.thread_error_timeout) return null;

        const saved_generation = self.desktop_layout_async.generation;
        const elapsed = elapsedTicks(self.desktop_layout_async.started_tick, self.ctx.ticks());
        self.desktop_layout_async.in_flight = false;
        self.desktop_layout_async.thread_handle = .{};
        self.desktop_layout_async.len = 0;
        self.desktop_layout_async.started_tick = 0;

        const result_code = if (rc == r4os.abi.thread_ok) exit_code else r4std.config.error_write_failed;
        self.render_stats.layout_worker_completed +%= 1;
        recordTickStat(&self.render_stats.layout_worker_total_ticks, &self.render_stats.layout_worker_max_ticks, &self.render_stats.layout_worker_last_ticks, elapsed);
        if (result_code < 0) self.render_stats.layout_worker_errors +%= 1;
        const result = self.desktop_layout_writeback.complete(self.ctx.ticks(), result_code);
        if (result_code >= 0 and saved_generation != self.desktop_layout_generation) {
            self.desktop_layout_writeback.markDirty(self.ctx.ticks());
        }
        return result;
    }

    fn composeDesktopLayout(self: *const App, buffer: []u8) []const u8 {
        var layout_state = desktop_layout.Layout{};
        var index: usize = 0;
        while (index < self.desktop_items.count) : (index += 1) {
            const item = &self.desktop_items.entries[index];
            _ = layout_state.add(item.pathText(), item.x, item.y);
        }
        return layout_state.writeTo(buffer);
    }

    fn applyDesktopLinkFile(self: *const App, item: *desktop_items.Item) void {
        var bytes: [desktop_folder.max_link_bytes]u8 = .{0} ** desktop_folder.max_link_bytes;
        const len = self.ctx.fileRead(item.pathZ(), bytes[0..]);
        if (len <= 0) return;
        const link = r4std.shortcut.parse(bytes[0..@intCast(len)]) catch return;
        item.applyShortcut(&link);
    }

    fn repairDesktopFolder(self: *const App) void {
        _ = self.ctx.dirCreate(desktop_folder.default_dir);
        var i: usize = 0;
        while (i < desktop_folder.default_links.len) : (i += 1) {
            self.repairDesktopFolderLink(desktop_folder.default_links[i]);
        }
    }

    fn repairDesktopFolderLink(self: *const App, spec: desktop_folder.DefaultLink) void {
        var path: [desktop_folder.max_link_path + 1]u8 = .{0} ** (desktop_folder.max_link_path + 1);
        _ = desktop_folder.defaultLinkPath(path[0..], desktop_folder.fileName(spec)) orelse return;
        const path_z = zptr(path[0..]);
        if (self.ctx.exists(path_z)) return;
        var bytes: [desktop_folder.max_link_bytes]u8 = .{0} ** desktop_folder.max_link_bytes;
        const data = desktop_folder.writeDefaultLink(spec, bytes[0..]) catch return;
        _ = self.ctx.fileWrite(path_z, data);
    }

    fn loadQuickLaunchRegistry(self: *App) void {
        var next = quick_launch.Bar.initDefault();
        if (!self.ctx.sys.hasFn("registry_get_value")) {
            self.quick_launch = next;
            return;
        }

        if (self.registryReadBool(quick_launch.registry_root_key, "Enabled")) |enabled| {
            if (!enabled) {
                next.count = 0;
                self.quick_launch = next;
                return;
            }
        }
        if (self.registryReadU32(quick_launch.registry_root_key, "Count")) |count| next.setCount(count);

        var i: usize = 0;
        while (i < quick_launch.max_items) : (i += 1) {
            self.loadQuickLaunchItem(quick_launch.itemRegistryKey(i), &next.items[i]);
        }
        self.quick_launch = next;
    }

    fn loadQuickLaunchItem(self: *App, key: [*:0]const u8, item: *quick_launch.Item) void {
        var candidate = item.*;
        var value: [160]u8 = .{0} ** 160;
        if (self.registryReadString(key, "Kind", value[0..])) _ = candidate.setKindText(spanZ(value[0..]));
        if (self.registryReadString(key, "Policy", value[0..])) _ = candidate.setPolicyText(spanZ(value[0..]));
        if (self.registryReadString(key, "Title", value[0..])) candidate.setTitle(spanZ(value[0..]));
        if (self.registryReadString(key, "Path", value[0..])) candidate.setPath(spanZ(value[0..]));
        if (self.registryReadString(key, "Args", value[0..])) candidate.setArgs(spanZ(value[0..]));
        if (self.registryReadString(key, "Icon", value[0..])) candidate.setIcon(spanZ(value[0..]));
        if (candidate.kind == .show_desktop) candidate.launch_policy = .action;
        if (candidate.isValid()) item.* = candidate;
    }

    fn registryReadString(self: *const App, key: [*:0]const u8, name: [*:0]const u8, out: []u8) bool {
        var info: r4os.abi.RegistryValueInfo = .{};
        var data: [160]u8 = .{0} ** 160;
        const result = self.ctx.sys.registryGetValue(key, name, &info, data[0..]);
        if (result < 0) return false;
        if (info.value_type != r4os.abi.registry_value_type_string) return false;
        const got: usize = @intCast(result);
        const available = @min(@min(got, @as(usize, @intCast(info.data_len))), data.len);
        copySliceZ(out, data[0..available]);
        return true;
    }

    fn registryReadU32(self: *const App, key: [*:0]const u8, name: [*:0]const u8) ?u32 {
        var info: r4os.abi.RegistryValueInfo = .{};
        var data: [4]u8 = .{0} ** 4;
        const result = self.ctx.sys.registryGetValue(key, name, &info, data[0..]);
        if (result < 0) return null;
        if (info.value_type != r4os.abi.registry_value_type_u32 or info.data_len != 4 or result != 4) return null;
        return @as(u32, data[0]) |
            (@as(u32, data[1]) << 8) |
            (@as(u32, data[2]) << 16) |
            (@as(u32, data[3]) << 24);
    }

    fn registryReadBool(self: *const App, key: [*:0]const u8, name: [*:0]const u8) ?bool {
        var info: r4os.abi.RegistryValueInfo = .{};
        var data: [1]u8 = .{0};
        const result = self.ctx.sys.registryGetValue(key, name, &info, data[0..]);
        if (result < 0) return null;
        if (info.value_type != r4os.abi.registry_value_type_bool or info.data_len != 1 or result != 1) return null;
        return data[0] != 0;
    }

    fn loadDesktopConfig(self: *App) void {
        if (self.loadDesktopConfigPath(desktop_config_path)) return;
        self.writeDesktopConfig();
    }

    fn loadTimeConfig(self: *App) void {
        if (self.loadTimeConfigPath(time_config_path)) return;
        self.writeTimeConfig();
    }

    fn loadTimeConfigPath(self: *App, path: [*:0]const u8) bool {
        var buffer: [768]u8 = undefined;
        const len = self.ctx.fileRead(path, buffer[0..]);
        if (len <= 0) return false;
        return self.time_config.loadFromBytes(buffer[0..@intCast(len)]);
    }

    fn syncTimeConfig(self: *App) bool {
        if (self.event_tick < self.next_time_config_check_tick) return false;
        self.next_time_config_check_tick = self.event_tick + self.time_config_check_ticks;
        var next = self.time_config;
        var buffer: [768]u8 = undefined;
        const len = self.ctx.fileRead(time_config_path, buffer[0..]);
        if (len <= 0 or !next.loadFromBytes(buffer[0..@intCast(len)])) return false;
        if (next.selectedIndex() == self.time_config.selectedIndex() and next.selectedClockFormat() == self.time_config.selectedClockFormat()) return false;
        self.time_config = next;
        self.invalidateTaskbar();
        return true;
    }

    fn loadDesktopConfigPath(self: *App, path: [*:0]const u8) bool {
        var buffer: [r4std.config.max_file_bytes]u8 = undefined;
        const len = self.ctx.fileRead(path, buffer[0..]);
        if (len <= 0) return false;
        return self.config.loadFromBytes(buffer[0..@intCast(len)]);
    }

    fn writeDesktopConfig(self: *App) void {
        r4std.settings.ensureDesktopDirs(self.ctx);
        var buffer: [r4std.config.max_output_bytes]u8 = .{0} ** r4std.config.max_output_bytes;
        const bytes = self.config.writeTo(buffer[0..]);
        if (bytes.len == 0) return;
        _ = self.ctx.fileWrite(desktop_config_path, bytes);
    }

    fn reloadWallpaper(self: *App) void {
        const allocator = self.ctx.allocator();
        if (self.config.wallpaper_path[0] == 0) {
            self.wallpaper_state.clear(allocator);
            return;
        }
        if (!self.wallpaper_state.load(&self.ctx.sys, self.images, allocator, self.config.wallpaperPath())) {
            self.wallpaper_state.clear(allocator);
        }
    }

    fn writeTimeConfig(self: *App) void {
        r4std.settings.ensureSystemDirs(self.ctx);
        var buffer: [384]u8 = .{0} ** 384;
        const bytes = self.time_config.writeToForState(buffer[0..], self.ctx.timeState());
        if (bytes.len == 0) return;
        _ = self.ctx.fileWrite(time_config_path, bytes);
    }

    fn setConsoleLaunch(self: *App, title: [*:0]const u8, path: [*:0]const u8, args: [*:0]const u8) void {
        copyZPtr(self.console_title[0..], title);
        copyZPtr(self.console_path[0..], path);
        copyZPtr(self.console_args[0..], args);
    }

    fn handleRunKey(self: *App, key: u8) bool {
        switch (key) {
            0x1B => self.dispatchCommand(self.closeCommand(), .keyboard),
            0x84 => self.previousDialogFocus(),
            '\t' => self.nextDialogFocus(),
            '\r', '\n' => self.dispatchCommand(self.defaultCommand(), .enter),
            r4os.gui.Key.ctrl_c => {
                if (self.dialog_focus == .run_input) _ = self.ctx.clipboardWrite(self.run_path.text());
            },
            r4os.gui.Key.ctrl_x => {
                if (self.dialog_focus == .run_input and self.ctx.clipboardWrite(self.run_path.text()) >= 0) {
                    self.run_path.clear();
                    self.invalidateDialog();
                }
            },
            r4os.gui.Key.ctrl_v => {
                if (self.dialog_focus == .run_input and self.pasteClipboardToRunPath()) self.invalidateDialog();
            },
            0x08 => {
                if (self.dialog_focus == .run_input and self.run_path.backspace()) self.invalidateDialog();
            },
            else => {
                if (isTextKey(key)) {
                    self.setDialogFocus(.run_input);
                    if (self.run_path.append(key)) self.invalidateDialog();
                }
            },
        }
        return self.hasDamage();
    }

    fn browseRunProgram(self: *App) void {
        var path_buf: [run_path_max + 1]u8 = .{0} ** (run_path_max + 1);
        var index = self.run_browse_index;
        var scanned: u32 = 0;
        var wrapped = false;
        while (scanned < run_browse_scan_limit) : (scanned += 1) {
            @memset(path_buf[0..], 0);
            const kind = self.ctx.dirEntry(run_browse_dir, index, path_buf[0 .. path_buf.len - 1]);
            if (kind < 0) {
                if (wrapped) break;
                index = run_browse_first_index;
                wrapped = true;
                continue;
            }
            if (kind == 0 and endsWithIgnoreCase(spanZ(path_buf[0..]), ".R4X")) {
                self.run_path.set(spanZ(path_buf[0..]));
                self.run_browse_index = index + 1;
                self.invalidateDialog();
                return;
            }
            index += 1;
        }
        self.openDialog(.message_run_no_programs);
    }

    fn submitRunDialog(self: *App) void {
        if (self.run_path.len == 0) {
            self.invalidateDialog();
            return;
        }
        const command = run_command.parse(self.run_path.text()) orelse {
            self.openDialog(.message_run_invalid);
            return;
        };

        switch (self.ctx.programClass(command.pathZ(), .auto)) {
            1 => {
                var title_buf: [41]u8 = .{0} ** 41;
                titleFromPath(title_buf[0..], command.pathZ());
                self.launchConsolePath(command.pathZ(), command.argsZ(), @ptrCast(&title_buf));
            },
            2 => {
                var title_buf: [41]u8 = .{0} ** 41;
                titleFromPath(title_buf[0..], command.pathZ());
                self.launchGuiPath(command.pathZ(), command.argsZ(), @ptrCast(&title_buf), .auto);
            },
            -1 => self.openDialog(.message_run_not_found),
            else => self.openDialog(.message_run_failed),
        }
    }

    fn readInitialCursor(self: *App) void {
        var mouse: r4os.abi.Mouse = undefined;
        self.ctx.mouseState(&mouse);
        self.cursor_x = clamp(mouse.x, 0, @max(0, self.screen_w - 1));
        self.cursor_y = clamp(mouse.y, 0, @max(0, self.screen_h - 1));
    }

    fn updateCursor(self: *App, x: i32, y: i32) bool {
        const next_x = clamp(x, 0, @max(0, self.screen_w - 1));
        const next_y = clamp(y, 0, @max(0, self.screen_h - 1));
        if (self.cursor_x == next_x and self.cursor_y == next_y) return false;
        const old_rect = surface.cursor(self.cursor_x, self.cursor_y, self.screen_w, self.screen_h).rect;
        self.cursor_x = next_x;
        self.cursor_y = next_y;
        const new_rect = surface.cursor(self.cursor_x, self.cursor_y, self.screen_w, self.screen_h).rect;
        self.queueCursorDamage(old_rect, new_rect);
        self.render_stats.cursor_moves +%= 1;
        return true;
    }

    fn hasDamage(self: *const App) bool {
        return self.damage.active or self.taskbar_damage or self.cursor_damage.active;
    }

    fn inputPreviousButtons(self: *const App) u8 {
        return if (self.event_remote_input) self.remote_prev_buttons else self.prev_buttons;
    }

    fn setInputPreviousButtons(self: *App, buttons: u8) void {
        if (self.event_remote_input) {
            self.remote_prev_buttons = buttons;
        } else {
            self.prev_buttons = buttons;
        }
    }

    fn recordPresentStats(self: *App, rect: surface.Rect, pixels: u32, region_count: u32, kind: compositor.DamageKind, frame_ticks: u64, compose_ticks: u64, present_ticks: u64, frame_ns: u64, compose_ns: u64, present_ns: u64, cursor_latency_ticks: u64, cull: compositor.CullStats, display_blit_calls: u32, display_copy_succeeded: bool, display_present_succeeded: bool, stride_blit: bool, remote_publish_rc: i32, present_result: r4os.abi.DisplayPresentResult) void {
        const copy_bytes = @as(u64, pixels) * 4;
        const display_bytes = if (display_copy_succeeded) copy_bytes else 0;
        const remote_bytes = if (remote_publish_rc > 0) copy_bytes else 0;
        const copy_attempts: u64 = if (display_blit_calls == 0) 0 else 2;
        const copy_successes = @as(u64, @intFromBool(display_copy_succeeded)) + @intFromBool(remote_publish_rc > 0);
        self.render_stats.redraws +%= 1;
        self.render_stats.total_damage_pixels +%= pixels;
        self.render_stats.last_damage_pixels = pixels;
        self.render_stats.last_damage_rect = .{ .x = rect.x, .y = rect.y, .w = rect.w, .h = rect.h };
        self.render_stats.last_damage_kind = kind;
        recordTickStat(&self.render_stats.frame_total_ticks, &self.render_stats.frame_max_ticks, &self.render_stats.frame_last_ticks, frame_ticks);
        recordTickStat(&self.render_stats.compose_total_ticks, &self.render_stats.compose_max_ticks, &self.render_stats.compose_last_ticks, compose_ticks);
        recordTickStat(&self.render_stats.present_total_ticks, &self.render_stats.present_max_ticks, &self.render_stats.present_last_ticks, present_ticks);
        recordTickStat(&self.render_stats.frame_total_ns, &self.render_stats.frame_max_ns, &self.render_stats.frame_last_ns, frame_ns);
        recordTickStat(&self.render_stats.compose_total_ns, &self.render_stats.compose_max_ns, &self.render_stats.compose_last_ns, compose_ns);
        recordTickStat(&self.render_stats.present_total_ns, &self.render_stats.present_max_ns, &self.render_stats.present_last_ns, present_ns);
        self.render_stats.scene_blit_bytes_total +%= display_bytes + remote_bytes;
        self.render_stats.scene_blit_bytes_last = display_bytes + remote_bytes;
        self.render_stats.display_blit_bytes_total +%= display_bytes;
        self.render_stats.display_blit_bytes_last = display_bytes;
        self.render_stats.remote_shadow_bytes_total +%= remote_bytes;
        self.render_stats.remote_shadow_bytes_last = remote_bytes;
        self.render_stats.scene_copy_attempts +%= copy_attempts;
        self.render_stats.scene_copy_successes +%= copy_successes;
        self.render_stats.scene_copy_skips +%= copy_attempts -| copy_successes;
        if (display_blit_calls != 0) {
            self.render_stats.present_attempts +%= 1;
            if (display_present_succeeded) self.render_stats.present_successes +%= 1 else self.render_stats.present_failures +%= 1;
        }
        self.render_stats.layers_visited_total +%= cull.layers_visited;
        self.render_stats.layers_culled_total +%= cull.layers_culled;
        self.render_stats.layers_visited_last = cull.layers_visited;
        self.render_stats.layers_culled_last = cull.layers_culled;
        self.render_stats.windows_visited_total +%= cull.windows_visited;
        self.render_stats.windows_culled_total +%= cull.windows_culled;
        self.render_stats.windows_visited_last = cull.windows_visited;
        self.render_stats.windows_culled_last = cull.windows_culled;
        self.render_stats.items_visited_total +%= cull.items_visited;
        self.render_stats.items_culled_total +%= cull.items_culled;
        self.render_stats.items_visited_last = cull.items_visited;
        self.render_stats.items_culled_last = cull.items_culled;
        self.render_stats.gui_frame_commands_total +%= cull.gui_frame_commands;
        self.render_stats.gui_frame_commands_last = cull.gui_frame_commands;
        self.render_stats.gui_resource_bytes_total +%= cull.gui_resource_bytes;
        self.render_stats.gui_resource_bytes_last = cull.gui_resource_bytes;
        self.render_stats.display_blit_calls_total +%= display_blit_calls;
        self.render_stats.display_blit_calls_last = display_blit_calls;
        self.render_stats.damage_regions_total +%= region_count;
        self.render_stats.damage_regions_last = region_count;
        if (present_result.present_generation != 0) {
            if (self.render_stats.present_generation_last != 0 and
                present_result.present_generation != self.render_stats.present_generation_last +% 1)
            {
                self.render_stats.present_lost_frames +%= 1;
            }
            self.render_stats.present_generation_last = present_result.present_generation;
            self.render_stats.present_fence_last = present_result.fence;
            if ((present_result.flags & r4os.abi.display_present_result_fallback) != 0) {
                self.render_stats.present_backend_fallbacks +%= 1;
            }
            if (present_result.elapsed_ticks != 0) {
                recordTickStat(
                    &self.render_stats.input_present_total_ticks,
                    &self.render_stats.input_present_max_ticks,
                    &self.render_stats.input_present_last_ticks,
                    present_result.elapsed_ticks,
                );
            }
        }
        if (stride_blit) {
            self.render_stats.display_stride_presents +%= 1;
        } else if (display_blit_calls > 1) {
            self.render_stats.display_legacy_row_presents +%= 1;
        }
        self.render_stats.remote_shadow_copies_last = if (remote_publish_rc > 0) 1 else 0;
        if (remote_publish_rc > 0) {
            self.render_stats.remote_shadow_copies +%= 1;
        } else if (display_blit_calls != 0) {
            self.render_stats.remote_shadow_skips +%= 1;
        }
        if (cursor_latency_ticks != 0 or kind == .cursor) {
            recordTickStat(&self.render_stats.cursor_latency_total_ticks, &self.render_stats.cursor_latency_max_ticks, &self.render_stats.cursor_latency_last_ticks, cursor_latency_ticks);
        }
        if (frame_ticks >= desktop_ui_blocker_warn_ticks) {
            self.render_stats.ui_blocker_count +%= 1;
            if (frame_ticks > self.render_stats.ui_blocker_max_ticks) self.render_stats.ui_blocker_max_ticks = frame_ticks;
        }
        if (display_present_succeeded) switch (kind) {
            .cursor => self.render_stats.cursor_only_presents +%= 1,
            .mixed => self.render_stats.mixed_damage_presents +%= 1,
            .full => self.render_stats.full_damage_presents +%= 1,
            else => {},
        };
    }

    fn startButtonHit(self: *const App, x: i32, y: i32) bool {
        const top = self.screen_h - theme.taskbar_h;
        return x >= 2 and x < 2 + theme.start_w and y >= top + 4 and y < top + 4 + theme.start_h;
    }

    fn keyboardLayoutHit(self: *const App, x: i32, y: i32) bool {
        return draw.taskbarKeyboardLayoutRect(self.screen_w, self.screen_h, self.config.taskbar_clock, self.volume_ui.installed).contains(x, y);
    }

    fn volumeHit(self: *const App, x: i32, y: i32) bool {
        return self.volume_ui.installed and draw.taskbarVolumeRect(self.screen_w, self.screen_h, self.config.taskbar_clock).contains(x, y);
    }

    fn clockHit(self: *const App, x: i32, y: i32) bool {
        return self.config.taskbar_clock and draw.taskbarClockRect(self.screen_w, self.screen_h).contains(x, y);
    }

    fn taskbarHit(self: *const App, x: i32, y: i32) ?usize {
        const top = self.screen_h - theme.taskbar_h;
        if (y < top + 4 or y >= top + 4 + theme.start_h) return null;
        const window_start = quick_launch.taskbarWindowStartX(self.quick_launch.count);
        const visible_count = self.visibleWindowCount();
        var ordinal: usize = 0;
        for (self.windows, 0..) |win, i| {
            if (!win.visible) continue;
            const rect = tray.windowButtonRect(window_start, self.tray_registry.window_right, visible_count, ordinal, top + 4, theme.start_h);
            ordinal += 1;
            if (rect.contains(x, y)) return i;
        }
        return null;
    }

    fn visibleWindowCount(self: *const App) usize {
        var count: usize = 0;
        for (self.windows) |win| if (win.visible) {
            count += 1;
        };
        return count;
    }

    fn dialogButtonHit(self: *const App, x: i32, y: i32) bool {
        switch (self.dialog) {
            .none => return false,
            .run => return self.runButtonHit(x, y) != null,
            .tasks => {
                const dx = @divTrunc(self.screen_w - 500, 2);
                const dy = @divTrunc(self.screen_h - 420, 2);
                return x >= dx + 210 and x < dx + 290 and y >= dy + 378 and y < dy + 404;
            },
            .message_settings => return self.settingsButtonHit(x, y) != null,
            else => return self.messageBoxButtonHit(x, y) != null,
        }
    }

    fn messageBoxButtonHit(self: *const App, x: i32, y: i32) ?model.UiTarget {
        if (!isMessageBoxDialog(self.dialog)) return null;
        const rect = self.messageBoxRect();
        if (y < rect.y + draw.message_box_button_y or y >= rect.y + draw.message_box_button_y + draw.message_box_button_h) return null;
        return switch (self.message_box_buttons) {
            .ok => if (x >= rect.x + draw.message_box_ok_x and x < rect.x + draw.message_box_ok_x + draw.message_box_ok_w) .message_ok else null,
            .ok_cancel => if (x >= rect.x + draw.message_box_pair_a_x and x < rect.x + draw.message_box_pair_a_x + draw.message_box_pair_w)
                .message_ok
            else if (x >= rect.x + draw.message_box_pair_b_x and x < rect.x + draw.message_box_pair_b_x + draw.message_box_pair_w)
                .message_no
            else
                null,
            .yes_no => if (x >= rect.x + draw.message_box_pair_a_x and x < rect.x + draw.message_box_pair_a_x + draw.message_box_pair_w)
                .message_yes
            else if (x >= rect.x + draw.message_box_pair_b_x and x < rect.x + draw.message_box_pair_b_x + draw.message_box_pair_w)
                .message_no
            else
                null,
        };
    }

    fn messageBoxNextFocus(self: *const App) model.UiTarget {
        return switch (self.message_box_buttons) {
            .ok => .message_ok,
            .ok_cancel => if (self.dialog_focus == .message_ok) .message_no else .message_ok,
            .yes_no => if (self.dialog_focus == .message_yes) .message_no else .message_yes,
        };
    }

    fn messageBoxDefaultCommand(self: *const App) model.UiTarget {
        return switch (self.message_box_buttons) {
            .ok => .message_ok,
            .ok_cancel, .yes_no => self.dialog_focus,
        };
    }

    fn messageBoxCloseCommand(self: *const App) model.UiTarget {
        if (!isMessageBoxDialog(self.dialog)) return .none;
        return message_box.cancelTarget(self.message_box_buttons);
    }

    fn messageBoxRect(self: *const App) surface.Rect {
        return .{
            .x = @divTrunc(self.screen_w - draw.message_box_w, 2),
            .y = @divTrunc(self.screen_h - draw.message_box_h, 2),
            .w = draw.message_box_w,
            .h = draw.message_box_h,
        };
    }

    fn runButtonHit(self: *const App, x: i32, y: i32) ?RunButton {
        const dx = @divTrunc(self.screen_w - 420, 2);
        const dy = @divTrunc(self.screen_h - 178, 2);
        if (y < dy + 126 or y >= dy + 152) return null;
        if (x >= dx + 24 and x < dx + 112) return .browse;
        if (x >= dx + 202 and x < dx + 278) return .ok;
        if (x >= dx + 288 and x < dx + 374) return .cancel;
        return null;
    }

    fn settingsButtonHit(self: *const App, x: i32, y: i32) ?model.UiTarget {
        const dx = @divTrunc(self.screen_w - draw.settings_dialog_w, 2);
        const dy = @divTrunc(self.screen_h - draw.settings_dialog_h, 2);
        if (y < dy + draw.settings_ok_y or y >= dy + draw.settings_ok_y + 26) return null;
        if (x >= dx + draw.settings_ok_x and x < dx + draw.settings_ok_x + 80) return .settings_ok;
        if (x >= dx + draw.settings_cancel_x and x < dx + draw.settings_cancel_x + 86) return .settings_cancel;
        return null;
    }

    fn runInputHit(self: *const App, x: i32, y: i32) bool {
        const dx = @divTrunc(self.screen_w - 420, 2);
        const dy = @divTrunc(self.screen_h - 178, 2);
        return x >= dx + 24 and x < dx + 396 and y >= dy + 68 and y < dy + 92;
    }

    fn dialogRect(self: *const App) ?surface.Rect {
        return self.dialogRectFor(self.dialog);
    }

    fn dialogRectFor(self: *const App, dialog: Dialog) ?surface.Rect {
        var w: i32 = 0;
        var h: i32 = 0;
        switch (dialog) {
            .none => return null,
            .run => {
                w = 420;
                h = 178;
            },
            .tasks => {
                w = 500;
                h = 420;
            },
            .message_settings => {
                w = draw.settings_dialog_w;
                h = draw.settings_dialog_h;
            },
            else => {
                w = draw.message_box_w;
                h = draw.message_box_h;
            },
        }
        return .{
            .x = @divTrunc(self.screen_w - w, 2),
            .y = @divTrunc(self.screen_h - h, 2),
            .w = w,
            .h = h,
        };
    }

    fn setDialogFocus(self: *App, target: model.UiTarget) void {
        if (self.dialog_focus == target) return;
        self.dialog_focus = target;
        self.keyboard_focus = target;
        self.invalidateDialog();
    }

    fn nextDialogFocus(self: *App) void {
        if (isMessageBoxDialog(self.dialog)) {
            self.setDialogFocus(self.messageBoxNextFocus());
            return;
        }
        if (self.dialog == .message_settings) {
            self.setDialogFocus(if (self.dialog_focus == .settings_ok) .settings_cancel else .settings_ok);
            return;
        }
        if (self.dialog == .tasks) {
            self.setDialogFocus(.task_overview_ok);
            return;
        }
        if (self.dialog != .run) return;
        self.setDialogFocus(switch (self.dialog_focus) {
            .run_input => .run_browse,
            .run_browse => .run_ok,
            .run_ok => .run_cancel,
            else => .run_input,
        });
    }

    fn previousDialogFocus(self: *App) void {
        if (isMessageBoxDialog(self.dialog)) {
            self.setDialogFocus(self.messageBoxNextFocus());
            return;
        }
        if (self.dialog == .message_settings) {
            self.setDialogFocus(if (self.dialog_focus == .settings_ok) .settings_cancel else .settings_ok);
            return;
        }
        if (self.dialog == .tasks) {
            self.setDialogFocus(.task_overview_ok);
            return;
        }
        if (self.dialog != .run) return;
        self.setDialogFocus(switch (self.dialog_focus) {
            .run_input => .run_cancel,
            .run_browse => .run_input,
            .run_ok => .run_browse,
            .run_cancel => .run_ok,
            else => .run_input,
        });
    }

    fn defaultCommand(self: *const App) model.UiTarget {
        if (self.dialog != .none) {
            if (self.dialog == .run) return switch (self.dialog_focus) {
                .run_browse => .run_browse,
                .run_cancel => .run_cancel,
                else => .run_ok,
            };
            if (isMessageBoxDialog(self.dialog)) return self.messageBoxDefaultCommand();
            if (self.dialog == .message_settings) return self.dialog_focus;
            return if (self.dialog == .tasks) .task_overview_ok else .message_ok;
        }
        if (self.start_open) {
            if (self.menu_nested_focus and self.menu_nested_open) return self.menu.nestedTarget(self.menu_submenu_parent, self.menu_nested_parent, self.menu_nested_selected);
            if (self.menu_submenu_focus and self.menu_submenu_open) return self.menu.submenuTarget(self.menu_submenu_parent, self.menu_submenu_selected);
            return self.menu.target(self.menu_selected);
        }
        if (self.time_menu_open) return .time_menu_backdrop;
        return .start_button;
    }

    fn closeCommand(self: *const App) model.UiTarget {
        if (self.dialog == .run) return .run_cancel;
        if (isMessageBoxDialog(self.dialog)) return self.messageBoxCloseCommand();
        if (self.dialog == .message_settings) return .settings_cancel;
        if (self.dialog == .tasks) return .task_overview_ok;
        if (self.dialog != .none) return .message_ok;
        if (self.start_open) return .start_menu_backdrop;
        if (self.time_menu_open) return .time_menu_backdrop;
        return .none;
    }

    fn updateClock(self: *App) bool {
        const state = self.ctx.timeState();
        // The timer path runs every 10 ms.  TIMESVC calls are backed by a
        // short-lived async-I/O task, so querying the service here used to
        // create and retire up to 100 kernel tasks per second while the
        // desktop was otherwise idle.  syncTimeConfig() already refreshes
        // the authoritative timezone/format file once per second; combine
        // that cached profile with the live kernel clock instead.
        const seconds = r4std.time.secondsInZone(state.seconds_since_midnight, self.time_config.offsetMinutesForState(state));
        const clock_time = r4std.time.splitTime(seconds);
        const clock_format = self.time_config.selectedClockFormat();
        var next: [9]u8 = .{0} ** 9;
        const written = r4std.time.formatHmDisplay(next[0..], clock_time, clock_format);
        if (written.len == 0) return false;
        const current = spanZ(self.clock[0..]);
        if (current.len == written.len and bytesEqual(current, written)) {
            return false;
        }
        @memset(self.clock[0..], 0);
        @memcpy(self.clock[0..written.len], written);
        self.invalidateTaskbar();
        return true;
    }

    fn toggleTimeMenu(self: *App) void {
        if (self.time_menu_open) {
            self.closeTop();
        } else {
            self.openTimeMenu();
        }
    }

    fn openTimeMenu(self: *App) void {
        if (!self.config.taskbar_clock) return;
        const was_start_open = self.start_open;
        const was_system_menu_open = self.system_menu_open;
        const was_volume_popup_open = self.volume_ui.popup_open;
        const old_dialog = self.dialog;
        if (was_start_open) self.invalidateStartMenu();
        if (was_system_menu_open) self.invalidateSystemMenu();
        self.invalidateDialogFor(old_dialog);
        self.start_open = false;
        self.system_menu_open = false;
        self.volume_ui.popup_open = false;
        self.volume_dragging = false;
        self.dialog = .none;
        self.dialog_focus = .none;
        self.menu_submenu_open = false;
        self.menu_submenu_focus = false;
        if (!self.time_menu_open) {
            self.time_menu_open = true;
            self.invalidateTimeMenu();
        }
        if (was_volume_popup_open) self.invalidateVolumePopup();
        self.keyboard_focus = .taskbar_clock;
        self.invalidateTaskbar();
    }

    fn toggleVolumePopup(self: *App) void {
        if (!self.volume_ui.installed) return;
        if (self.volume_ui.popup_open) {
            self.closeTop();
            return;
        }
        const was_start_open = self.start_open;
        const was_system_menu_open = self.system_menu_open;
        const was_time_menu_open = self.time_menu_open;
        const old_dialog = self.dialog;
        if (was_start_open) self.invalidateStartMenu();
        if (was_system_menu_open) self.invalidateSystemMenu();
        if (was_time_menu_open) self.invalidateTimeMenu();
        self.invalidateDialogFor(old_dialog);
        self.start_open = false;
        self.system_menu_open = false;
        self.time_menu_open = false;
        self.dialog = .none;
        self.dialog_focus = .none;
        self.menu_submenu_open = false;
        self.menu_submenu_focus = false;
        self.volume_ui.popup_open = true;
        self.volume_dragging = false;
        self.keyboard_focus = .volume_popup_slider;
        self.volume_next_poll_tick = 0;
        _ = self.syncVolume();
        if (self.volume_ui.installed) {
            self.volume_ui.popup_open = true;
            self.invalidateVolumePopup();
            self.invalidateTaskbar();
        }
    }

    fn launchClockFromTimeMenu(self: *App) void {
        self.time_menu_open = false;
        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\CLOCK.R4X", "", "Clock", .gui);
    }

    fn launchClockFromTaskbar(self: *App) void {
        if (self.time_menu_open) {
            self.invalidateTimeMenu();
            self.time_menu_open = false;
        }
        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\CLOCK.R4X", "", "Clock", .gui);
    }

    fn launchTimeSettingsFromTimeMenu(self: *App) void {
        self.time_menu_open = false;
        self.launchGuiPath("C:\\R4OS\\SOFTWARE\\DESKTOP\\TIMESET.R4X", "", "Time Settings", .gui);
    }

    fn updateKeyboardLayout(self: *App) bool {
        var info: r4os.abi.KeyboardLayoutInfo = .{};
        if (self.ctx.keyboardLayoutCurrent(&info) <= 0) return false;
        if (keyboardLayoutInfoEqual(self.keyboard_layout, info)) return false;
        self.keyboard_layout = info;
        self.invalidateTaskbar();
        return true;
    }

    fn cycleKeyboardLayout(self: *App) void {
        var current: r4os.abi.KeyboardLayoutInfo = .{};
        if (self.ctx.keyboardLayoutCurrent(&current) <= 0) return;
        if (current.count <= 1) return;
        var next_index = current.index + 1;
        if (next_index >= current.count) next_index = 0;
        var next: r4os.abi.KeyboardLayoutInfo = .{};
        if (self.ctx.keyboardLayoutAt(next_index, &next) <= 0) return;
        if (self.ctx.keyboardLayoutSet(zptr(next.name[0..])) < 0) return;
        self.keyboard_layout = next;
        self.closeStartMenu();
        self.invalidateTaskbar();
    }
};

fn desktopLayoutSaveWorker(arg: u64) callconv(.c) i32 {
    const job: *DesktopLayoutAsyncSave = @ptrFromInt(arg);
    r4std.settings.ensureDesktopDirs(job.sys);
    return r4std.config.saveDocument(job.sys, desktop_layout_path, job.bytes[0..job.len]);
}

fn fallbackDimension(value: u32, fallback: i32) i32 {
    if (value == 0) return fallback;
    return @intCast(value);
}

fn ticksFromMs(hz: u32, ms: u32) u64 {
    const effective_hz = if (hz == 0) default_monotonic_hz else hz;
    const ticks = (@as(u64, effective_hz) * ms + 999) / 1000;
    return if (ticks == 0) 1 else ticks;
}

fn elapsedTicks(start: u64, end: u64) u64 {
    return if (end >= start) end - start else 0;
}

fn elapsedNanoseconds(start: u64, end: u64) u64 {
    if (start == 0 or end < start) return 0;
    return end - start;
}

fn recordTickStat(total: *u64, max: *u64, last: *u64, ticks: u64) void {
    total.* +%= ticks;
    last.* = ticks;
    if (ticks > max.*) max.* = ticks;
}

fn digit(value: u32) u8 {
    return '0' + @as(u8, @intCast(value % 10));
}

fn clamp(value: i32, min: i32, max: i32) i32 {
    if (value < min) return min;
    if (value > max) return max;
    return value;
}

fn absI32(value: i32) i32 {
    return if (value < 0) -value else value;
}

fn rectArea(rect: surface.Rect) u32 {
    return @intCast(@max(0, rect.w) * @max(0, rect.h));
}

fn rectEqual(a: surface.Rect, b: surface.Rect) bool {
    return a.x == b.x and a.y == b.y and a.w == b.w and a.h == b.h;
}

fn rectsOverlap(a: surface.Rect, b: surface.Rect) bool {
    return !a.isEmpty() and !b.isEmpty() and
        a.x < b.right() and a.right() > b.x and
        a.y < b.bottom() and a.bottom() > b.y;
}

fn appendDamageRegion(regions: *[surface.max_damage_regions]surface.Rect, count: *usize, rect: surface.Rect) void {
    if (rect.isEmpty()) return;
    var merged = rect;
    var index: usize = 0;
    while (index < count.*) {
        if (!rectsOverlap(regions[index], merged)) {
            index += 1;
            continue;
        }
        merged = merged.merged(regions[index]);
        count.* -= 1;
        regions[index] = regions[count.*];
        index = 0;
    }
    if (count.* < regions.len) {
        regions[count.*] = merged;
        count.* += 1;
        return;
    }
    var bounds = merged;
    for (regions[0..count.*]) |existing| bounds = bounds.merged(existing);
    regions[0] = bounds;
    count.* = 1;
}

fn clipDamageRect(rect: surface.Rect, screen_w: i32, screen_h: i32) ?surface.Rect {
    const left = @max(0, rect.x);
    const top = @max(0, rect.y);
    const right = @min(screen_w, rect.right());
    const bottom = @min(screen_h, rect.bottom());
    if (right <= left or bottom <= top) return null;
    return .{ .x = left, .y = top, .w = right - left, .h = bottom - top };
}

fn accumulateCullStats(total: *compositor.CullStats, value: compositor.CullStats) void {
    total.layers_visited +%= value.layers_visited;
    total.layers_culled +%= value.layers_culled;
    total.windows_visited +%= value.windows_visited;
    total.windows_culled +%= value.windows_culled;
    total.items_visited +%= value.items_visited;
    total.items_culled +%= value.items_culled;
    total.gui_frame_commands +%= value.gui_frame_commands;
    total.gui_resource_bytes +%= value.gui_resource_bytes;
}

fn writeR4BasicTraceId(out: *[16]u8, value: u64) void {
    const hex = "0123456789ABCDEF";
    for (out[0..], 0..) |*byte, index| {
        const shift: u6 = @intCast((15 - index) * 4);
        byte.* = hex[@as(u4, @truncate(value >> shift))];
    }
}

fn isFullRect(rect: surface.Rect, screen_w: i32, screen_h: i32) bool {
    return rect.x <= 0 and rect.y <= 0 and rect.w >= screen_w and rect.h >= screen_h;
}

fn damageKindName(kind: compositor.DamageKind) []const u8 {
    return switch (kind) {
        .none => "none",
        .cursor => "cursor",
        .mixed => "mixed",
        .full => "full",
    };
}

fn fixedNameEquals(value: []const u8, expected: []const u8) bool {
    var len: usize = 0;
    while (len < value.len and value[len] != 0) : (len += 1) {}
    if (len != expected.len) return false;
    var index: usize = 0;
    while (index < len) : (index += 1) if (value[index] != expected[index]) return false;
    return true;
}

fn copyZPtr(out: []u8, value: [*:0]const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    var i: usize = 0;
    while (i + 1 < out.len and value[i] != 0) : (i += 1) {
        out[i] = value[i];
    }
    out[i] = 0;
}

fn copySliceZ(out: []u8, value: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const count = @min(value.len, out.len - 1);
    if (count != 0) @memcpy(out[0..count], value[0..count]);
    out[count] = 0;
}

fn copyLit(out: []u8, comptime value: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const count = @min(value.len, out.len - 1);
    inline for (value, 0..) |ch, i| {
        if (i < count) out[i] = ch;
    }
    out[count] = 0;
}

fn clearZ(out: []u8) void {
    @memset(out, 0);
}

fn spanZ(buffer: []const u8) []const u8 {
    var len: usize = 0;
    while (len < buffer.len and buffer[len] != 0) : (len += 1) {}
    return buffer[0..len];
}

fn spanZPtr(value: [*:0]const u8) []const u8 {
    var len: usize = 0;
    while (len < 255 and value[len] != 0) : (len += 1) {}
    return value[0..len];
}

fn formatAppError(out: []u8, exit_code: i32) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const prefix = "App ended with exit code ";
    var pos: usize = 0;
    while (pos < prefix.len and pos + 1 < out.len) : (pos += 1) out[pos] = prefix[pos];

    var n: u32 = undefined;
    if (exit_code < 0 and pos + 1 < out.len) {
        out[pos] = '-';
        pos += 1;
        n = @intCast(-@as(i64, exit_code));
    } else {
        n = @intCast(exit_code);
    }

    var digits: [10]u8 = undefined;
    var count: usize = 0;
    if (n == 0) {
        digits[count] = '0';
        count += 1;
    } else {
        while (n > 0 and count < digits.len) : (count += 1) {
            digits[count] = '0' + @as(u8, @intCast(n % 10));
            n /= 10;
        }
    }
    while (count > 0 and pos + 1 < out.len) {
        count -= 1;
        out[pos] = digits[count];
        pos += 1;
    }
    out[pos] = 0;
}

fn formatWindowInfo(out: []u8, win: *const window.Window) void {
    @memset(out, 0);
    if (out.len == 0) return;
    var pos: usize = 0;
    pos = appendLit(out, pos, "Window size: ");
    pos = appendI32(out, pos, win.w);
    pos = appendLit(out, pos, " x ");
    pos = appendI32(out, pos, win.h);
    pos = appendLit(out, pos, "\nPosition: ");
    pos = appendI32(out, pos, win.x);
    pos = appendLit(out, pos, ", ");
    pos = appendI32(out, pos, win.y);
    if (pos < out.len) out[pos] = 0;
}

fn appendLit(out: []u8, pos: usize, comptime value: []const u8) usize {
    var next = pos;
    inline for (value) |ch| {
        if (next + 1 >= out.len) return next;
        out[next] = ch;
        next += 1;
    }
    out[next] = 0;
    return next;
}

fn appendI32(out: []u8, pos: usize, value: i32) usize {
    var next = pos;
    var n: u32 = undefined;
    if (value < 0) {
        if (next + 1 >= out.len) return next;
        out[next] = '-';
        next += 1;
        n = @intCast(-@as(i64, value));
    } else {
        n = @intCast(value);
    }

    var digits: [10]u8 = undefined;
    var count: usize = 0;
    if (n == 0) {
        digits[count] = '0';
        count += 1;
    } else {
        while (n > 0 and count < digits.len) : (count += 1) {
            digits[count] = '0' + @as(u8, @intCast(n % 10));
            n /= 10;
        }
    }
    while (count > 0 and next + 1 < out.len) {
        count -= 1;
        out[next] = digits[count];
        next += 1;
    }
    if (next < out.len) out[next] = 0;
    return next;
}

fn titleFromPath(out: []u8, path: [*:0]const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    var len: usize = 0;
    var start: usize = 0;
    while (len < 255 and path[len] != 0) : (len += 1) {
        if (path[len] == '\\' or path[len] == '/') start = len + 1;
    }
    var end = len;
    if (end >= start + 4 and
        asciiLower(path[end - 4]) == '.' and
        asciiLower(path[end - 3]) == 'r' and
        asciiLower(path[end - 2]) == '4' and
        asciiLower(path[end - 1]) == 'x')
    {
        end -= 4;
    }
    if (end <= start) {
        copyZPtr(out, "R4X App");
        return;
    }
    const count = @min(end - start, out.len - 1);
    var i: usize = 0;
    while (i < count) : (i += 1) {
        out[i] = path[start + i];
    }
    out[count] = 0;
}

fn launchPolicyForApi(policy: start_menu.LaunchPolicy) r4os.abi.LaunchPolicy {
    return switch (policy) {
        .console => .console,
        .gui => .gui,
        .auto => .auto,
        .action => .auto,
    };
}

fn parseAbiLaunchPolicy(value: u32) r4os.abi.LaunchPolicy {
    return switch (value) {
        @intFromEnum(r4os.abi.LaunchPolicy.console) => .console,
        @intFromEnum(r4os.abi.LaunchPolicy.gui) => .gui,
        else => .auto,
    };
}

fn endsWithIgnoreCase(value: []const u8, suffix: []const u8) bool {
    if (value.len < suffix.len) return false;
    const start = value.len - suffix.len;
    var i: usize = 0;
    while (i < suffix.len) : (i += 1) {
        if (asciiLower(value[start + i]) != asciiLower(suffix[i])) return false;
    }
    return true;
}

fn isExplorerPath(path: []const u8) bool {
    return endsWithIgnoreCase(path, "explorer.r4x");
}

fn isNetCfgPath(path: []const u8) bool {
    return endsWithIgnoreCase(path, "netcfg.r4x");
}

fn isClockPath(path: []const u8) bool {
    return endsWithIgnoreCase(path, "clock.r4x");
}

fn isTimeSettingsPath(path: []const u8) bool {
    return endsWithIgnoreCase(path, "timeset.r4x");
}

fn isRegEditPath(path: []const u8) bool {
    return endsWithIgnoreCase(path, "regedit.r4x");
}

fn isCalcPath(path: []const u8) bool {
    return endsWithIgnoreCase(path, "calc.r4x");
}

fn isMemViewPath(path: []const u8) bool {
    return endsWithIgnoreCase(path, "memview.r4x");
}

fn isAppDefPath(path: []const u8) bool {
    return endsWithIgnoreCase(path, "appdef.r4x");
}

fn isLogCenterPath(path: []const u8) bool {
    return endsWithIgnoreCase(path, "logcenter.r4x");
}

fn isSingleInstanceGuiPath(path: []const u8) bool {
    return isNetCfgPath(path) or isClockPath(path) or isTimeSettingsPath(path) or isRegEditPath(path) or isCalcPath(path) or isMemViewPath(path) or isAppDefPath(path) or isLogCenterPath(path);
}

fn sameSingleInstanceGuiPath(a: []const u8, b: []const u8) bool {
    return (isNetCfgPath(a) and isNetCfgPath(b)) or
        (isClockPath(a) and isClockPath(b)) or
        (isTimeSettingsPath(a) and isTimeSettingsPath(b)) or
        (isRegEditPath(a) and isRegEditPath(b)) or
        (isCalcPath(a) and isCalcPath(b)) or
        (isMemViewPath(a) and isMemViewPath(b)) or
        (isAppDefPath(a) and isAppDefPath(b)) or
        (isLogCenterPath(a) and isLogCenterPath(b));
}

fn hasSmokeArg(args: [*:0]const u8) bool {
    return argsContain(args, "--smoke-poweroff") or argsContain(args, "SMOKE") or argsContain(args, "/SMOKE");
}

fn hasR4XSmokeArg(args: [*:0]const u8) bool {
    return argsContain(args, "--smoke-r4x") or argsContain(args, "SMOKE-R4X") or argsContain(args, "/SMOKE-R4X");
}

fn hasKlickifaxSmokeArg(args: [*:0]const u8) bool {
    return argsContain(args, "--smoke-klickifax") or argsContain(args, "SMOKE-KLICKIFAX") or argsContain(args, "/SMOKE-KLICKIFAX");
}

fn hasHeadlessSubsystemArg(args: [*:0]const u8) bool {
    return argsContain(args, "/HEADLESS-SUBSYSTEM");
}

fn argsContain(args: [*:0]const u8, wanted: []const u8) bool {
    var offset: usize = 0;
    while (args[offset] != 0) {
        while (args[offset] == ' ' or args[offset] == '\t') : (offset += 1) {}
        if (args[offset] == 0) break;
        const start = offset;
        while (args[offset] != 0 and args[offset] != ' ' and args[offset] != '\t') : (offset += 1) {}
        if (equalsIgnoreCase(ptrSlice(args, start, offset), wanted)) return true;
    }
    return false;
}

fn ptrSlice(ptr: [*:0]const u8, start: usize, end: usize) []const u8 {
    return ptr[start..end];
}

fn smokeDrive(ctx: *const desk_api.Context, letter: u8, kind: u32) bool {
    ctx.print("drive ");
    ctx.putc(letter);
    ctx.print(": ");
    if (ctx.driveInfo(letter - 'A')) |info| {
        const ok = info.mounted != 0 and info.kind == kind;
        ctx.print(if (ok) "mounted" else "FAILED");
        ctx.print(" bytes=");
        ctx.printU64(info.bytes);
        ctx.println("");
        return ok;
    }
    ctx.println("missing");
    return false;
}

fn smokeExists(ctx: *const desk_api.Context, path: [*:0]const u8) bool {
    ctx.print("exists ");
    ctx.print(path);
    ctx.print(": ");
    const ok = ctx.exists(path);
    ctx.println(if (ok) "yes" else "FAILED");
    return ok;
}

fn equalsIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (asciiLower(a[i]) != asciiLower(b[i])) return false;
    }
    return true;
}

fn bytesEqual(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

fn containsBytes(haystack: []const u8, needle: []const u8) bool {
    if (needle.len == 0) return true;
    if (needle.len > haystack.len) return false;
    var i: usize = 0;
    while (i + needle.len <= haystack.len) : (i += 1) {
        if (bytesEqual(haystack[i .. i + needle.len], needle)) return true;
    }
    return false;
}

fn asciiLower(ch: u8) u8 {
    if (ch >= 'A' and ch <= 'Z') return ch + ('a' - 'A');
    return ch;
}

fn processHandleValid(handle: r4os.abi.ProgramProcessHandle) bool {
    return handle.instance_id != 0 and handle.generation != 0 and handle.reserved == 0;
}

fn sameProcessHandle(a: r4os.abi.ProgramProcessHandle, b: r4os.abi.ProgramProcessHandle) bool {
    return a.instance_id == b.instance_id and a.generation == b.generation and a.reserved == b.reserved;
}

fn processHandleDefinitelyGone(status: i32) bool {
    return status == r4os.abi.program_handle_error_stale or status == r4os.abi.program_handle_error_not_found;
}

fn defaultDialogFocus(dialog: Dialog) model.UiTarget {
    return switch (dialog) {
        .none => .none,
        .run => .run_input,
        .tasks => .task_overview_ok,
        .message_settings => .settings_ok,
        .confirm_restart, .confirm_poweroff, .confirm_halt => message_box.defaultFocus(.yes_no),
        else => .message_ok,
    };
}

fn isMessageBoxDialog(dialog: Dialog) bool {
    return switch (dialog) {
        .message_notepad,
        .message_synth,
        .message_run_invalid,
        .message_run_no_programs,
        .message_run_no_slots,
        .message_run_console_busy,
        .message_terminal_mode_busy,
        .message_run_not_found,
        .message_run_failed,
        .message_app_error,
        .message_window_info,
        .confirm_restart,
        .confirm_poweroff,
        .confirm_halt,
        => true,
        else => false,
    };
}

fn runButtonTarget(button: RunButton) model.UiTarget {
    return switch (button) {
        .browse => .run_browse,
        .ok => .run_ok,
        .cancel => .run_cancel,
    };
}

fn legacyKey(key: u32) u8 {
    return if (key <= 0xff) @intCast(key) else 0;
}

fn isTextKey(key: u32) bool {
    if (key < 0x20 or key > 0x10ffff) return false;
    if (key >= 0x7f and key <= 0x9f) return false;
    return key < 0xd800 or key > 0xdfff;
}

fn isAppKey(key: u32) bool {
    return isTextKey(key) or
        key == r4os.gui.Key.ctrl_a or
        key == r4os.gui.Key.ctrl_c or
        key == r4os.gui.Key.ctrl_v or
        key == r4os.gui.Key.ctrl_x or
        key == r4os.gui.Key.backspace or
        key == r4os.gui.Key.enter or
        key == '\n' or
        key == r4os.gui.Key.escape or
        key == r4os.gui.Key.delete or
        key == r4os.gui.Key.up or
        key == r4os.gui.Key.down or
        key == r4os.gui.Key.f3 or
        key == r4os.gui.Key.shift_tab or
        key == r4os.gui.Key.left or
        key == r4os.gui.Key.right or
        key == r4os.gui.Key.home or
        key == r4os.gui.Key.end or
        key == r4os.gui.Key.page_up or
        key == r4os.gui.Key.page_down or
        key == r4os.gui.Key.f10;
}

fn isConsoleKey(key: u32) bool {
    return isTextKey(key) or
        key == r4os.gui.Key.backspace or
        key == r4os.gui.Key.tab or
        key == r4os.gui.Key.enter or
        key == '\n' or
        key == r4os.gui.Key.ctrl_v or
        key == r4os.gui.Key.up or
        key == r4os.gui.Key.down or
        key == r4os.gui.Key.f3 or
        key == r4os.gui.Key.left or
        key == r4os.gui.Key.right or
        key == r4os.gui.Key.home or
        key == r4os.gui.Key.end or
        key == r4os.gui.Key.delete;
}

fn keyboardSource(key: u32) model.EventSource {
    if (key == '\r' or key == '\n') return .enter;
    if (key == r4os.gui.Key.start_menu) return .hotkey;
    return if (modifiersForKey(key) != 0) .hotkey else .keyboard;
}

fn remoteKeyboardSource(input: r4os.abi.RemoteInputEvent, key: u32) model.EventSource {
    if ((input.modifiers & (r4os.abi.remote_input_modifier_ctrl | r4os.abi.remote_input_modifier_alt)) != 0) return .hotkey;
    return keyboardSource(key);
}

fn remoteInputKey(key: u32) ?u32 {
    if (key == 0) return null;
    if (isAppKey(key)) return key;
    return null;
}

fn remoteModifiers(modifiers: u32) u8 {
    var out: u8 = 0;
    if ((modifiers & r4os.abi.remote_input_modifier_ctrl) != 0) out |= model.Modifier.ctrl;
    if ((modifiers & r4os.abi.remote_input_modifier_shift) != 0) out |= model.Modifier.shift;
    if ((modifiers & r4os.abi.remote_input_modifier_alt) != 0) out |= model.Modifier.alt;
    return out;
}

fn validWindowServiceResult(result: *const r4os.abi.WindowServiceResult) bool {
    return result.magic == r4os.abi.window_service_result_magic and
        result.version == r4os.abi.window_service_result_version;
}

fn modifiersForKey(key: u32) u8 {
    return switch (key) {
        0x83 => model.Modifier.ctrl,
        0x84 => model.Modifier.shift,
        0x85, 0x86 => model.Modifier.alt,
        0x87 => model.Modifier.alt | model.Modifier.shift,
        else => 0,
    };
}

fn encodeUtf8Codepoint(codepoint: u32, out: *[4]u8) usize {
    if (codepoint <= 0x7f) {
        out[0] = @intCast(codepoint);
        return 1;
    }
    if (codepoint <= 0x7ff) {
        out[0] = @intCast(0xc0 | (codepoint >> 6));
        out[1] = @intCast(0x80 | (codepoint & 0x3f));
        return 2;
    }
    if (codepoint >= 0xd800 and codepoint <= 0xdfff) return 0;
    if (codepoint <= 0xffff) {
        out[0] = @intCast(0xe0 | (codepoint >> 12));
        out[1] = @intCast(0x80 | ((codepoint >> 6) & 0x3f));
        out[2] = @intCast(0x80 | (codepoint & 0x3f));
        return 3;
    }
    if (codepoint <= 0x10ffff) {
        out[0] = @intCast(0xf0 | (codepoint >> 18));
        out[1] = @intCast(0x80 | ((codepoint >> 12) & 0x3f));
        out[2] = @intCast(0x80 | ((codepoint >> 6) & 0x3f));
        out[3] = @intCast(0x80 | (codepoint & 0x3f));
        return 4;
    }
    return 0;
}

fn signedScrollLines(wheel: i32, lines_per_step: u32) i32 {
    const lines: i32 = @intCast(@min(lines_per_step, @as(u32, 32)));
    if (wheel > 0) return lines;
    if (wheel < 0) return -lines;
    return 0;
}

fn maxScrollOffsetFromState(state: r4os.abi.ConsoleState) u32 {
    const visible = @max(@as(u32, 1), state.rows);
    return if (state.scrollback_lines > visible) state.scrollback_lines - visible else 0;
}

fn keyboardLayoutInfoEqual(a: r4os.abi.KeyboardLayoutInfo, b: r4os.abi.KeyboardLayoutInfo) bool {
    return a.index == b.index and
        a.count == b.count and
        fixedBytesEqual(a.name[0..], b.name[0..]) and
        fixedBytesEqual(a.display[0..], b.display[0..]);
}

fn volumeViewEqual(a: volume.View, b: volume.View) bool {
    return a.installed == b.installed and
        a.reachable == b.reachable and
        a.muted == b.muted and
        a.percent == b.percent and
        a.popup_open == b.popup_open;
}

fn fixedBytesEqual(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

fn namedZ(comptime len: usize, comptime value: []const u8) [len]u8 {
    var out: [len]u8 = .{0} ** len;
    const count = @min(value.len, len - 1);
    inline for (value, 0..) |ch, i| {
        if (i < count) out[i] = ch;
    }
    return out;
}

fn zptr(buffer: []const u8) [*:0]const u8 {
    return @ptrCast(buffer.ptr);
}
