const std = @import("std");
const testing = std.testing;

pub const UiTarget = enum(u16) {
    none = 0,
    menu_terminal = 1,
    menu_notepad = 2,
    menu_synth = 3,
    menu_run = 4,
    menu_settings = 5,
    menu_tasks = 6,
    menu_halt = 7,
    menu_restart = 8,
    menu_poweroff = 9,
    menu_terminal_mode = 10,
    menu_devmgr = 11,
    menu_settings_appearance = 12,
    menu_settings_network = 14,
    menu_settings_time = 15,
    menu_paint = 16,
    menu_calc = 17,
    menu_settings_default_apps = 18,
    menu_settings_services = 19,
    menu_settings_registry = 20,
    menu_settings_log_center = 21,
    menu_r4code = 22,
    menu_programs = 23,
    menu_programs_internet = 24,
    menu_klickifax = 25,
    menu_update = 26,
    start_menu_panel = 50,
    start_menu_backdrop = 51,
    run_close = 60,
    run_ok = 61,
    run_cancel = 62,
    message_close = 63,
    message_ok = 64,
    task_overview_close = 65,
    task_overview_ok = 66,
    run_backdrop = 67,
    message_backdrop = 68,
    run_browse = 69,
    run_input = 70,
    message_yes = 71,
    message_no = 72,
    settings_ok = 73,
    settings_cancel = 74,
    wm_close = 80,
    terminal_close = 81,
    wm_min = 82,
    terminal_min = 83,
    wm_max_normal = 84,
    terminal_max_normal = 85,
    wm_max_full = 86,
    terminal_max_full = 87,
    app2_close = 88,
    app2_min = 89,
    app2_max_normal = 90,
    app2_max_full = 91,
    app3_close = 92,
    app3_min = 93,
    app3_max_normal = 94,
    app3_max_full = 95,
    terminal_info = 96,
    wm_info = 97,
    app2_info = 98,
    app3_info = 99,
    terminal_window = 100,
    wm_window = 101,
    app2_window = 102,
    app3_window = 103,
    terminal_taskbar = 120,
    wm_taskbar = 121,
    app2_taskbar = 122,
    app3_taskbar = 123,
    volume_popup_backdrop = 124,
    volume_popup_slider = 125,
    volume_popup_mute = 126,
    start_button = 140,
    taskbar_keyboard_layout = 141,
    time_menu_clock = 142,
    time_menu_settings = 143,
    taskbar_clock = 144,
    time_menu_backdrop = 145,
    quick_show_desktop = 146,
    quick_computer = 147,
    taskbar_volume = 148,
    taskbar_tray_external = 149,
    desktop_icon_1 = 150,
    desktop_icon_2 = 151,
    desktop_icon_3 = 152,
    desktop_icon_4 = 153,
    desktop_icon_5 = 154,
    desktop_icon_6 = 155,
    desktop_icon_7 = 156,
    desktop_icon_8 = 157,
    desktop_item_9 = 158,
    desktop_item_10 = 159,
    desktop_item_11 = 160,
    desktop_item_12 = 161,
    desktop_item_13 = 162,
    desktop_item_14 = 163,
    desktop_item_15 = 164,
    desktop_item_16 = 165,
    desktop_item_17 = 166,
    desktop_item_18 = 167,
    desktop_item_19 = 168,
    desktop_item_20 = 169,
    desktop_item_21 = 170,
    desktop_item_22 = 171,
    desktop_item_23 = 172,
    desktop_item_24 = 173,
    desktop_item_25 = 174,
    desktop_item_26 = 175,
    desktop_item_27 = 176,
    desktop_item_28 = 177,
    desktop_item_29 = 178,
    desktop_item_30 = 179,
    desktop_item_31 = 180,
    desktop_item_32 = 181,
};

pub const UiOwner = enum(u8) {
    none = 0,
    start_menu = 1,
    dialog = 2,
    window = 3,
    taskbar = 4,
    desktop = 5,
};

pub const UiLayer = enum(u8) {
    none = 0,
    desktop = 10,
    taskbar = 20,
    window = 30,
    dialog = 40,
    popup = 50,
};

pub const EventKind = enum(u8) {
    none = 0,
    keyboard = 1,
    mouse_down = 2,
    mouse_up = 3,
    timer = 4,
};

pub const EventSource = enum(u8) {
    none = 0,
    mouse = 1,
    keyboard = 2,
    enter = 3,
    hotkey = 4,
    timer = 5,
};

pub const EventPhase = enum(u8) {
    none = 0,
    captured = 1,
    targeted = 2,
    activated = 3,
    consumed = 4,
};

pub const MouseButton = struct {
    pub const none: u8 = 0;
    pub const left: u8 = 1;
    pub const right: u8 = 2;
};

pub const EventFlag = struct {
    pub const enabled: u32 = 1;
};

pub const Modifier = struct {
    pub const ctrl: u8 = 1;
    pub const shift: u8 = 2;
    pub const alt: u8 = 4;
};

pub const PsConstant = struct {
    name: []const u8,
    value: u32,
};

pub const OwnerRange = struct {
    min: u16,
    max: u16,
    owner: UiOwner,
    ps_owner_name: []const u8,
};

pub const OwnerLayer = struct {
    owner: UiOwner,
    layer: UiLayer,
    ps_owner_name: []const u8,
    ps_layer_name: []const u8,
};

pub const StartMenuAction = enum {
    terminal,
    terminal_mode,
    message,
    run,
};

pub const StartMenuItem = struct {
    name: []const u8,
    hover_id: u8,
    hit_y: i32,
    text_y: i32,
    target: UiTarget,
    ps_target_name: []const u8,
    label: []const u8,
    title: []const u8,
    policy_x: i32,
    policy_label: []const u8,
    policy_text: []const u8,
    hover_skip: []const u8,
    down_name: []const u8,
    release_name: []const u8,
    draw_label: []const u8,
    dispatch_label: []const u8,
    action: StartMenuAction,
    message_id: u8,
    separator_before: bool,
};

pub const PolicyLabel = struct {
    label: []const u8,
    text: []const u8,
};

pub const owner_ranges = [_]OwnerRange{
    .{ .min = 1, .max = 51, .owner = .start_menu, .ps_owner_name = "UiOwnerStartMenu" },
    .{ .min = 60, .max = 74, .owner = .dialog, .ps_owner_name = "UiOwnerDialog" },
    .{ .min = 80, .max = 103, .owner = .window, .ps_owner_name = "UiOwnerWindow" },
    .{ .min = 120, .max = 149, .owner = .taskbar, .ps_owner_name = "UiOwnerTaskbar" },
    .{ .min = 150, .max = 181, .owner = .desktop, .ps_owner_name = "UiOwnerDesktop" },
};

pub const owner_layers = [_]OwnerLayer{
    .{ .owner = .start_menu, .layer = .popup, .ps_owner_name = "UiOwnerStartMenu", .ps_layer_name = "UiLayerPopup" },
    .{ .owner = .dialog, .layer = .dialog, .ps_owner_name = "UiOwnerDialog", .ps_layer_name = "UiLayerDialog" },
    .{ .owner = .window, .layer = .window, .ps_owner_name = "UiOwnerWindow", .ps_layer_name = "UiLayerWindow" },
    .{ .owner = .taskbar, .layer = .taskbar, .ps_owner_name = "UiOwnerTaskbar", .ps_layer_name = "UiLayerTaskbar" },
    .{ .owner = .desktop, .layer = .desktop, .ps_owner_name = "UiOwnerDesktop", .ps_layer_name = "UiLayerDesktop" },
};

pub const start_menu_items = [_]StartMenuItem{
    .{
        .name = "Terminal",
        .hover_id = 1,
        .hit_y = 492,
        .text_y = 496,
        .target = .menu_terminal,
        .ps_target_name = "UiTargetMenuTerminal",
        .label = "menuTerminal",
        .title = "Terminal",
        .policy_x = 166,
        .policy_label = "policyConsole",
        .policy_text = "console",
        .hover_skip = "skipHoverTerminal",
        .down_name = "MenuDownTerminal",
        .release_name = "MenuReleaseTerminal",
        .draw_label = "drawHoverTerminal",
        .dispatch_label = "dispatchStartMenuTerminal",
        .action = .terminal,
        .message_id = 0,
        .separator_before = false,
    },
    .{
        .name = "TerminalMode",
        .hover_id = 10,
        .hit_y = 516,
        .text_y = 520,
        .target = .menu_terminal_mode,
        .ps_target_name = "UiTargetMenuTerminalMode",
        .label = "menuTerminalMode",
        .title = "Terminal Mode",
        .policy_x = 174,
        .policy_label = "policyAction",
        .policy_text = "action",
        .hover_skip = "skipHoverTerminalMode",
        .down_name = "MenuDownTerminalMode",
        .release_name = "MenuReleaseTerminalMode",
        .draw_label = "drawHoverTerminalMode",
        .dispatch_label = "dispatchStartMenuTerminalMode",
        .action = .terminal_mode,
        .message_id = 0,
        .separator_before = false,
    },
    .{
        .name = "Notepad",
        .hover_id = 2,
        .hit_y = 516,
        .text_y = 520,
        .target = .menu_notepad,
        .ps_target_name = "UiTargetMenuNotepad",
        .label = "menuNotepad",
        .title = "Notepad",
        .policy_x = 190,
        .policy_label = "policyGui",
        .policy_text = "gui",
        .hover_skip = "skipHoverNotepad",
        .down_name = "MenuDownNotepad",
        .release_name = "MenuReleaseNotepad",
        .draw_label = "drawHoverNotepad",
        .dispatch_label = "dispatchStartMenuNotepad",
        .action = .message,
        .message_id = 1,
        .separator_before = false,
    },
    .{
        .name = "Paint",
        .hover_id = 16,
        .hit_y = 540,
        .text_y = 544,
        .target = .menu_paint,
        .ps_target_name = "UiTargetMenuPaint",
        .label = "menuPaint",
        .title = "Paint",
        .policy_x = 190,
        .policy_label = "policyGui",
        .policy_text = "gui",
        .hover_skip = "skipHoverPaint",
        .down_name = "MenuDownPaint",
        .release_name = "MenuReleasePaint",
        .draw_label = "drawHoverPaint",
        .dispatch_label = "dispatchStartMenuPaint",
        .action = .message,
        .message_id = 1,
        .separator_before = false,
    },
    .{
        .name = "Synth",
        .hover_id = 3,
        .hit_y = 540,
        .text_y = 544,
        .target = .menu_synth,
        .ps_target_name = "UiTargetMenuSynth",
        .label = "menuSynth",
        .title = "R4Synth",
        .policy_x = 166,
        .policy_label = "policyConsole",
        .policy_text = "console",
        .hover_skip = "skipHoverSynth",
        .down_name = "MenuDownSynth",
        .release_name = "MenuReleaseSynth",
        .draw_label = "drawHoverSynth",
        .dispatch_label = "dispatchStartMenuSynth",
        .action = .message,
        .message_id = 2,
        .separator_before = false,
    },
    .{
        .name = "DeviceManager",
        .hover_id = 11,
        .hit_y = 564,
        .text_y = 568,
        .target = .menu_devmgr,
        .ps_target_name = "UiTargetMenuDeviceManager",
        .label = "menuDeviceManager",
        .title = "Device Manager",
        .policy_x = 190,
        .policy_label = "policyGui",
        .policy_text = "gui",
        .hover_skip = "skipHoverDeviceManager",
        .down_name = "MenuDownDeviceManager",
        .release_name = "MenuReleaseDeviceManager",
        .draw_label = "drawHoverDeviceManager",
        .dispatch_label = "dispatchStartMenuDeviceManager",
        .action = .message,
        .message_id = 0,
        .separator_before = false,
    },
    .{
        .name = "Run",
        .hover_id = 4,
        .hit_y = 580,
        .text_y = 584,
        .target = .menu_run,
        .ps_target_name = "UiTargetMenuRun",
        .label = "menuRun",
        .title = "Run...",
        .policy_x = 174,
        .policy_label = "policyAction",
        .policy_text = "action",
        .hover_skip = "skipHoverRun",
        .down_name = "MenuDownRun",
        .release_name = "MenuReleaseRun",
        .draw_label = "drawHoverRun",
        .dispatch_label = "dispatchStartMenuRun",
        .action = .run,
        .message_id = 0,
        .separator_before = true,
    },
    .{
        .name = "Settings",
        .hover_id = 5,
        .hit_y = 604,
        .text_y = 608,
        .target = .menu_settings,
        .ps_target_name = "UiTargetMenuSettings",
        .label = "menuSettings",
        .title = "Settings",
        .policy_x = 174,
        .policy_label = "policyAction",
        .policy_text = "action",
        .hover_skip = "skipHoverSettings",
        .down_name = "MenuDownSettings",
        .release_name = "MenuReleaseSettings",
        .draw_label = "drawHoverSettings",
        .dispatch_label = "dispatchStartMenuSettings",
        .action = .message,
        .message_id = 3,
        .separator_before = false,
    },
    .{
        .name = "Tasks",
        .hover_id = 6,
        .hit_y = 628,
        .text_y = 632,
        .target = .menu_tasks,
        .ps_target_name = "UiTargetMenuTasks",
        .label = "menuTasks",
        .title = "Tasks",
        .policy_x = 174,
        .policy_label = "policyAction",
        .policy_text = "action",
        .hover_skip = "skipHoverTasks",
        .down_name = "MenuDownTasks",
        .release_name = "MenuReleaseTasks",
        .draw_label = "drawHoverTasks",
        .dispatch_label = "dispatchStartMenuTasks",
        .action = .message,
        .message_id = 5,
        .separator_before = false,
    },
    .{
        .name = "Restart",
        .hover_id = 7,
        .hit_y = 652,
        .text_y = 656,
        .target = .menu_restart,
        .ps_target_name = "UiTargetMenuRestart",
        .label = "menuRestart",
        .title = "Restart",
        .policy_x = 174,
        .policy_label = "policyAction",
        .policy_text = "action",
        .hover_skip = "skipHoverRestart",
        .down_name = "MenuDownRestart",
        .release_name = "MenuReleaseRestart",
        .draw_label = "drawHoverRestart",
        .dispatch_label = "dispatchStartMenuRestart",
        .action = .message,
        .message_id = 4,
        .separator_before = true,
    },
    .{
        .name = "Poweroff",
        .hover_id = 8,
        .hit_y = 676,
        .text_y = 680,
        .target = .menu_poweroff,
        .ps_target_name = "UiTargetMenuPoweroff",
        .label = "menuPoweroff",
        .title = "Poweroff",
        .policy_x = 174,
        .policy_label = "policyAction",
        .policy_text = "action",
        .hover_skip = "skipHoverPoweroff",
        .down_name = "MenuDownPoweroff",
        .release_name = "MenuReleasePoweroff",
        .draw_label = "drawHoverPoweroff",
        .dispatch_label = "dispatchStartMenuPoweroff",
        .action = .message,
        .message_id = 4,
        .separator_before = false,
    },
    .{
        .name = "Halt",
        .hover_id = 9,
        .hit_y = 700,
        .text_y = 704,
        .target = .menu_halt,
        .ps_target_name = "UiTargetMenuHalt",
        .label = "menuHalt",
        .title = "Halt",
        .policy_x = 174,
        .policy_label = "policyAction",
        .policy_text = "action",
        .hover_skip = "skipHoverHalt",
        .down_name = "MenuDownHalt",
        .release_name = "MenuReleaseHalt",
        .draw_label = "drawHoverHalt",
        .dispatch_label = "dispatchStartMenuHalt",
        .action = .message,
        .message_id = 4,
        .separator_before = false,
    },
};

pub const policy_labels = [_]PolicyLabel{
    .{ .label = "policyConsole", .text = "console" },
    .{ .label = "policyGui", .text = "gui" },
    .{ .label = "policyAction", .text = "action" },
};

pub const start_menu_first_id: u8 = start_menu_items[0].hover_id;
pub const start_menu_last_id: u8 = 19;

pub const ps_constants = [_]PsConstant{
    .{ .name = "UiTargetNone", .value = @intFromEnum(UiTarget.none) },
    .{ .name = "UiTargetMenuTerminal", .value = @intFromEnum(UiTarget.menu_terminal) },
    .{ .name = "UiTargetMenuNotepad", .value = @intFromEnum(UiTarget.menu_notepad) },
    .{ .name = "UiTargetMenuPaint", .value = @intFromEnum(UiTarget.menu_paint) },
    .{ .name = "UiTargetMenuCalculator", .value = @intFromEnum(UiTarget.menu_calc) },
    .{ .name = "UiTargetMenuSynth", .value = @intFromEnum(UiTarget.menu_synth) },
    .{ .name = "UiTargetMenuDeviceManager", .value = @intFromEnum(UiTarget.menu_devmgr) },
    .{ .name = "UiTargetMenuRun", .value = @intFromEnum(UiTarget.menu_run) },
    .{ .name = "UiTargetMenuSettings", .value = @intFromEnum(UiTarget.menu_settings) },
    .{ .name = "UiTargetMenuSettingsAppearance", .value = @intFromEnum(UiTarget.menu_settings_appearance) },
    .{ .name = "UiTargetMenuSettingsDefaultApps", .value = @intFromEnum(UiTarget.menu_settings_default_apps) },
    .{ .name = "UiTargetMenuSettingsNetwork", .value = @intFromEnum(UiTarget.menu_settings_network) },
    .{ .name = "UiTargetMenuSettingsServices", .value = @intFromEnum(UiTarget.menu_settings_services) },
    .{ .name = "UiTargetMenuSettingsRegistry", .value = @intFromEnum(UiTarget.menu_settings_registry) },
    .{ .name = "UiTargetMenuSettingsTime", .value = @intFromEnum(UiTarget.menu_settings_time) },
    .{ .name = "UiTargetMenuSettingsLogCenter", .value = @intFromEnum(UiTarget.menu_settings_log_center) },
    .{ .name = "UiTargetMenuR4Code", .value = @intFromEnum(UiTarget.menu_r4code) },
    .{ .name = "UiTargetMenuPrograms", .value = @intFromEnum(UiTarget.menu_programs) },
    .{ .name = "UiTargetMenuProgramsInternet", .value = @intFromEnum(UiTarget.menu_programs_internet) },
    .{ .name = "UiTargetMenuKlickifax", .value = @intFromEnum(UiTarget.menu_klickifax) },
    .{ .name = "UiTargetMenuUpdate", .value = @intFromEnum(UiTarget.menu_update) },
    .{ .name = "UiTargetMenuTasks", .value = @intFromEnum(UiTarget.menu_tasks) },
    .{ .name = "UiTargetMenuHalt", .value = @intFromEnum(UiTarget.menu_halt) },
    .{ .name = "UiTargetMenuRestart", .value = @intFromEnum(UiTarget.menu_restart) },
    .{ .name = "UiTargetMenuPoweroff", .value = @intFromEnum(UiTarget.menu_poweroff) },
    .{ .name = "UiTargetMenuTerminalMode", .value = @intFromEnum(UiTarget.menu_terminal_mode) },
    .{ .name = "UiTargetStartMenuPanel", .value = @intFromEnum(UiTarget.start_menu_panel) },
    .{ .name = "UiTargetStartMenuBackdrop", .value = @intFromEnum(UiTarget.start_menu_backdrop) },
    .{ .name = "UiTargetRunClose", .value = @intFromEnum(UiTarget.run_close) },
    .{ .name = "UiTargetRunOk", .value = @intFromEnum(UiTarget.run_ok) },
    .{ .name = "UiTargetRunCancel", .value = @intFromEnum(UiTarget.run_cancel) },
    .{ .name = "UiTargetMessageClose", .value = @intFromEnum(UiTarget.message_close) },
    .{ .name = "UiTargetMessageOk", .value = @intFromEnum(UiTarget.message_ok) },
    .{ .name = "UiTargetTaskOverviewClose", .value = @intFromEnum(UiTarget.task_overview_close) },
    .{ .name = "UiTargetTaskOverviewOk", .value = @intFromEnum(UiTarget.task_overview_ok) },
    .{ .name = "UiTargetRunBackdrop", .value = @intFromEnum(UiTarget.run_backdrop) },
    .{ .name = "UiTargetMessageBackdrop", .value = @intFromEnum(UiTarget.message_backdrop) },
    .{ .name = "UiTargetRunBrowse", .value = @intFromEnum(UiTarget.run_browse) },
    .{ .name = "UiTargetRunInput", .value = @intFromEnum(UiTarget.run_input) },
    .{ .name = "UiTargetMessageYes", .value = @intFromEnum(UiTarget.message_yes) },
    .{ .name = "UiTargetMessageNo", .value = @intFromEnum(UiTarget.message_no) },
    .{ .name = "UiTargetSettingsOk", .value = @intFromEnum(UiTarget.settings_ok) },
    .{ .name = "UiTargetSettingsCancel", .value = @intFromEnum(UiTarget.settings_cancel) },
    .{ .name = "UiTargetWmClose", .value = @intFromEnum(UiTarget.wm_close) },
    .{ .name = "UiTargetTerminalClose", .value = @intFromEnum(UiTarget.terminal_close) },
    .{ .name = "UiTargetWmMin", .value = @intFromEnum(UiTarget.wm_min) },
    .{ .name = "UiTargetTerminalMin", .value = @intFromEnum(UiTarget.terminal_min) },
    .{ .name = "UiTargetWmMaxNormal", .value = @intFromEnum(UiTarget.wm_max_normal) },
    .{ .name = "UiTargetTerminalMaxNormal", .value = @intFromEnum(UiTarget.terminal_max_normal) },
    .{ .name = "UiTargetWmMaxFull", .value = @intFromEnum(UiTarget.wm_max_full) },
    .{ .name = "UiTargetTerminalMaxFull", .value = @intFromEnum(UiTarget.terminal_max_full) },
    .{ .name = "UiTargetApp2Close", .value = @intFromEnum(UiTarget.app2_close) },
    .{ .name = "UiTargetApp2Min", .value = @intFromEnum(UiTarget.app2_min) },
    .{ .name = "UiTargetApp2MaxNormal", .value = @intFromEnum(UiTarget.app2_max_normal) },
    .{ .name = "UiTargetApp2MaxFull", .value = @intFromEnum(UiTarget.app2_max_full) },
    .{ .name = "UiTargetApp3Close", .value = @intFromEnum(UiTarget.app3_close) },
    .{ .name = "UiTargetApp3Min", .value = @intFromEnum(UiTarget.app3_min) },
    .{ .name = "UiTargetApp3MaxNormal", .value = @intFromEnum(UiTarget.app3_max_normal) },
    .{ .name = "UiTargetApp3MaxFull", .value = @intFromEnum(UiTarget.app3_max_full) },
    .{ .name = "UiTargetTerminalInfo", .value = @intFromEnum(UiTarget.terminal_info) },
    .{ .name = "UiTargetWmInfo", .value = @intFromEnum(UiTarget.wm_info) },
    .{ .name = "UiTargetApp2Info", .value = @intFromEnum(UiTarget.app2_info) },
    .{ .name = "UiTargetApp3Info", .value = @intFromEnum(UiTarget.app3_info) },
    .{ .name = "UiTargetTerminalWindow", .value = @intFromEnum(UiTarget.terminal_window) },
    .{ .name = "UiTargetWmWindow", .value = @intFromEnum(UiTarget.wm_window) },
    .{ .name = "UiTargetApp2Window", .value = @intFromEnum(UiTarget.app2_window) },
    .{ .name = "UiTargetApp3Window", .value = @intFromEnum(UiTarget.app3_window) },
    .{ .name = "UiTargetTerminalTaskbar", .value = @intFromEnum(UiTarget.terminal_taskbar) },
    .{ .name = "UiTargetWmTaskbar", .value = @intFromEnum(UiTarget.wm_taskbar) },
    .{ .name = "UiTargetApp2Taskbar", .value = @intFromEnum(UiTarget.app2_taskbar) },
    .{ .name = "UiTargetApp3Taskbar", .value = @intFromEnum(UiTarget.app3_taskbar) },
    .{ .name = "UiTargetVolumePopupBackdrop", .value = @intFromEnum(UiTarget.volume_popup_backdrop) },
    .{ .name = "UiTargetVolumePopupSlider", .value = @intFromEnum(UiTarget.volume_popup_slider) },
    .{ .name = "UiTargetVolumePopupMute", .value = @intFromEnum(UiTarget.volume_popup_mute) },
    .{ .name = "UiTargetStartButton", .value = @intFromEnum(UiTarget.start_button) },
    .{ .name = "UiTargetTaskbarKeyboardLayout", .value = @intFromEnum(UiTarget.taskbar_keyboard_layout) },
    .{ .name = "UiTargetTimeMenuClock", .value = @intFromEnum(UiTarget.time_menu_clock) },
    .{ .name = "UiTargetTimeMenuSettings", .value = @intFromEnum(UiTarget.time_menu_settings) },
    .{ .name = "UiTargetTaskbarClock", .value = @intFromEnum(UiTarget.taskbar_clock) },
    .{ .name = "UiTargetTimeMenuBackdrop", .value = @intFromEnum(UiTarget.time_menu_backdrop) },
    .{ .name = "UiTargetQuickShowDesktop", .value = @intFromEnum(UiTarget.quick_show_desktop) },
    .{ .name = "UiTargetQuickComputer", .value = @intFromEnum(UiTarget.quick_computer) },
    .{ .name = "UiTargetTaskbarVolume", .value = @intFromEnum(UiTarget.taskbar_volume) },
    .{ .name = "UiTargetTaskbarTrayExternal", .value = @intFromEnum(UiTarget.taskbar_tray_external) },
    .{ .name = "UiTargetDesktopIcon1", .value = @intFromEnum(UiTarget.desktop_icon_1) },
    .{ .name = "UiTargetDesktopIcon2", .value = @intFromEnum(UiTarget.desktop_icon_2) },
    .{ .name = "UiTargetDesktopIcon3", .value = @intFromEnum(UiTarget.desktop_icon_3) },
    .{ .name = "UiTargetDesktopIcon4", .value = @intFromEnum(UiTarget.desktop_icon_4) },
    .{ .name = "UiTargetDesktopIcon5", .value = @intFromEnum(UiTarget.desktop_icon_5) },
    .{ .name = "UiTargetDesktopIcon6", .value = @intFromEnum(UiTarget.desktop_icon_6) },
    .{ .name = "UiTargetDesktopIcon7", .value = @intFromEnum(UiTarget.desktop_icon_7) },
    .{ .name = "UiTargetDesktopIcon8", .value = @intFromEnum(UiTarget.desktop_icon_8) },
    .{ .name = "UiTargetDesktopItem9", .value = @intFromEnum(UiTarget.desktop_item_9) },
    .{ .name = "UiTargetDesktopItem10", .value = @intFromEnum(UiTarget.desktop_item_10) },
    .{ .name = "UiTargetDesktopItem11", .value = @intFromEnum(UiTarget.desktop_item_11) },
    .{ .name = "UiTargetDesktopItem12", .value = @intFromEnum(UiTarget.desktop_item_12) },
    .{ .name = "UiTargetDesktopItem13", .value = @intFromEnum(UiTarget.desktop_item_13) },
    .{ .name = "UiTargetDesktopItem14", .value = @intFromEnum(UiTarget.desktop_item_14) },
    .{ .name = "UiTargetDesktopItem15", .value = @intFromEnum(UiTarget.desktop_item_15) },
    .{ .name = "UiTargetDesktopItem16", .value = @intFromEnum(UiTarget.desktop_item_16) },
    .{ .name = "UiTargetDesktopItem17", .value = @intFromEnum(UiTarget.desktop_item_17) },
    .{ .name = "UiTargetDesktopItem18", .value = @intFromEnum(UiTarget.desktop_item_18) },
    .{ .name = "UiTargetDesktopItem19", .value = @intFromEnum(UiTarget.desktop_item_19) },
    .{ .name = "UiTargetDesktopItem20", .value = @intFromEnum(UiTarget.desktop_item_20) },
    .{ .name = "UiTargetDesktopItem21", .value = @intFromEnum(UiTarget.desktop_item_21) },
    .{ .name = "UiTargetDesktopItem22", .value = @intFromEnum(UiTarget.desktop_item_22) },
    .{ .name = "UiTargetDesktopItem23", .value = @intFromEnum(UiTarget.desktop_item_23) },
    .{ .name = "UiTargetDesktopItem24", .value = @intFromEnum(UiTarget.desktop_item_24) },
    .{ .name = "UiTargetDesktopItem25", .value = @intFromEnum(UiTarget.desktop_item_25) },
    .{ .name = "UiTargetDesktopItem26", .value = @intFromEnum(UiTarget.desktop_item_26) },
    .{ .name = "UiTargetDesktopItem27", .value = @intFromEnum(UiTarget.desktop_item_27) },
    .{ .name = "UiTargetDesktopItem28", .value = @intFromEnum(UiTarget.desktop_item_28) },
    .{ .name = "UiTargetDesktopItem29", .value = @intFromEnum(UiTarget.desktop_item_29) },
    .{ .name = "UiTargetDesktopItem30", .value = @intFromEnum(UiTarget.desktop_item_30) },
    .{ .name = "UiTargetDesktopItem31", .value = @intFromEnum(UiTarget.desktop_item_31) },
    .{ .name = "UiTargetDesktopItem32", .value = @intFromEnum(UiTarget.desktop_item_32) },

    .{ .name = "UiOwnerNone", .value = @intFromEnum(UiOwner.none) },
    .{ .name = "UiOwnerStartMenu", .value = @intFromEnum(UiOwner.start_menu) },
    .{ .name = "UiOwnerDialog", .value = @intFromEnum(UiOwner.dialog) },
    .{ .name = "UiOwnerWindow", .value = @intFromEnum(UiOwner.window) },
    .{ .name = "UiOwnerTaskbar", .value = @intFromEnum(UiOwner.taskbar) },
    .{ .name = "UiOwnerDesktop", .value = @intFromEnum(UiOwner.desktop) },

    .{ .name = "UiLayerNone", .value = @intFromEnum(UiLayer.none) },
    .{ .name = "UiLayerDesktop", .value = @intFromEnum(UiLayer.desktop) },
    .{ .name = "UiLayerTaskbar", .value = @intFromEnum(UiLayer.taskbar) },
    .{ .name = "UiLayerWindow", .value = @intFromEnum(UiLayer.window) },
    .{ .name = "UiLayerDialog", .value = @intFromEnum(UiLayer.dialog) },
    .{ .name = "UiLayerPopup", .value = @intFromEnum(UiLayer.popup) },

    .{ .name = "UiEventFlagEnabled", .value = EventFlag.enabled },
    .{ .name = "UiMouseButtonLeft", .value = MouseButton.left },
    .{ .name = "UiMouseButtonRight", .value = MouseButton.right },

    .{ .name = "UiEventSourceNone", .value = @intFromEnum(EventSource.none) },
    .{ .name = "UiEventSourceMouse", .value = @intFromEnum(EventSource.mouse) },
    .{ .name = "UiEventSourceKeyboard", .value = @intFromEnum(EventSource.keyboard) },
    .{ .name = "UiEventSourceEnter", .value = @intFromEnum(EventSource.enter) },
    .{ .name = "UiEventSourceHotkey", .value = @intFromEnum(EventSource.hotkey) },
    .{ .name = "UiEventSourceTimer", .value = @intFromEnum(EventSource.timer) },

    .{ .name = "UiEventPhaseNone", .value = @intFromEnum(EventPhase.none) },
    .{ .name = "UiEventPhaseCaptured", .value = @intFromEnum(EventPhase.captured) },
    .{ .name = "UiEventPhaseTargeted", .value = @intFromEnum(EventPhase.targeted) },
    .{ .name = "UiEventPhaseActivated", .value = @intFromEnum(EventPhase.activated) },
    .{ .name = "UiEventPhaseConsumed", .value = @intFromEnum(EventPhase.consumed) },

    .{ .name = "UiModifierCtrl", .value = Modifier.ctrl },
    .{ .name = "UiModifierShift", .value = Modifier.shift },
    .{ .name = "UiModifierAlt", .value = Modifier.alt },
};

pub const Event = struct {
    id: u32 = 0,
    kind: EventKind = .none,
    tick: u64 = 0,
    key: u32 = 0,
    target: UiTarget = .none,
    owner: UiOwner = .none,
    layer: UiLayer = .none,
    flags: u32 = 0,
    mouse_x: i32 = -1,
    mouse_y: i32 = -1,
    button: u8 = MouseButton.none,
    command: UiTarget = .none,
    source: EventSource = .none,
    click_count: u8 = 0,
    phase: EventPhase = .none,
    modifiers: u8 = 0,
    capture_owner: UiOwner = .none,
    capture_layer: UiLayer = .none,
    consumed: bool = false,

    pub fn reset(self: *Event) void {
        self.* = .{};
    }

    pub fn setTarget(self: *Event, target: UiTarget) void {
        self.target = target;
        self.owner = ownerForTarget(target);
        self.layer = layerForOwner(self.owner);
        self.flags |= EventFlag.enabled;
        self.phase = .targeted;
    }

    pub fn setCapture(self: *Event, owner: UiOwner) void {
        self.capture_owner = owner;
        self.capture_layer = layerForOwner(owner);
        self.phase = .captured;
    }

    pub fn activate(self: *Event, command: UiTarget, source: EventSource) void {
        self.command = command;
        self.source = source;
        self.phase = .activated;
    }

    pub fn activateTarget(self: *Event, target: UiTarget, source: EventSource) void {
        if (self.target == .none) self.setTarget(target);
        self.activate(target, source);
    }

    pub fn consume(self: *Event) void {
        self.consumed = true;
        self.phase = .consumed;
    }
};

pub fn ownerForTarget(target: UiTarget) UiOwner {
    const id = @intFromEnum(target);
    for (owner_ranges) |range| {
        if (id >= range.min and id <= range.max) return range.owner;
    }
    return .none;
}

pub fn layerForOwner(owner: UiOwner) UiLayer {
    for (owner_layers) |mapping| {
        if (owner == mapping.owner) return mapping.layer;
    }
    return .none;
}

test "target owner and layer mapping follows Desktop ids" {
    try testing.expectEqual(@as(usize, 5), owner_ranges.len);
    try testing.expectEqual(@as(usize, 5), owner_layers.len);
    try testing.expectEqual(UiOwner.start_menu, ownerForTarget(.menu_terminal));
    try testing.expectEqual(UiOwner.start_menu, ownerForTarget(.menu_paint));
    try testing.expectEqual(UiOwner.start_menu, ownerForTarget(.menu_settings_default_apps));
    try testing.expectEqual(UiOwner.start_menu, ownerForTarget(.menu_settings_network));
    try testing.expectEqual(UiOwner.start_menu, ownerForTarget(.menu_settings_services));
    try testing.expectEqual(UiOwner.start_menu, ownerForTarget(.menu_settings_time));
    try testing.expectEqual(UiOwner.start_menu, ownerForTarget(.menu_settings_log_center));
    try testing.expectEqual(UiOwner.start_menu, ownerForTarget(.menu_r4code));
    try testing.expectEqual(UiOwner.start_menu, ownerForTarget(.menu_programs));
    try testing.expectEqual(UiOwner.start_menu, ownerForTarget(.menu_klickifax));
    try testing.expectEqual(UiOwner.start_menu, ownerForTarget(.menu_update));
    try testing.expectEqual(UiOwner.start_menu, ownerForTarget(.start_menu_backdrop));
    try testing.expectEqual(UiOwner.dialog, ownerForTarget(.run_ok));
    try testing.expectEqual(UiOwner.dialog, ownerForTarget(.run_browse));
    try testing.expectEqual(UiOwner.window, ownerForTarget(.terminal_window));
    try testing.expectEqual(UiOwner.window, ownerForTarget(.terminal_info));
    try testing.expectEqual(UiOwner.window, ownerForTarget(.app2_window));
    try testing.expectEqual(UiOwner.window, ownerForTarget(.app3_window));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.terminal_taskbar));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.wm_taskbar));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.app2_taskbar));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.app3_taskbar));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.start_button));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.taskbar_keyboard_layout));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.time_menu_clock));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.time_menu_settings));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.taskbar_clock));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.quick_show_desktop));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.quick_computer));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.taskbar_volume));
    try testing.expectEqual(UiOwner.taskbar, ownerForTarget(.taskbar_tray_external));
    try testing.expectEqual(UiOwner.desktop, ownerForTarget(.desktop_icon_1));
    try testing.expectEqual(UiOwner.desktop, ownerForTarget(.desktop_icon_8));
    try testing.expectEqual(UiOwner.desktop, ownerForTarget(.desktop_item_32));
    try testing.expectEqual(UiLayer.popup, layerForOwner(.start_menu));
    try testing.expectEqual(UiLayer.dialog, layerForOwner(.dialog));
    try testing.expectEqual(UiLayer.desktop, layerForOwner(.desktop));
}

test "event target metadata is filled together" {
    var event = Event{};

    event.setTarget(.start_button);

    try testing.expectEqual(UiTarget.start_button, event.target);
    try testing.expectEqual(UiOwner.taskbar, event.owner);
    try testing.expectEqual(UiLayer.taskbar, event.layer);
    try testing.expect((event.flags & EventFlag.enabled) != 0);
    try testing.expectEqual(EventPhase.targeted, event.phase);
}

test "capture owner derives modal and popup layers" {
    var dialog_event = Event{};
    var menu_event = Event{};

    dialog_event.setCapture(.dialog);
    menu_event.setCapture(.start_menu);

    try testing.expectEqual(UiLayer.dialog, dialog_event.capture_layer);
    try testing.expectEqual(EventPhase.captured, dialog_event.phase);
    try testing.expectEqual(UiLayer.popup, menu_event.capture_layer);
    try testing.expectEqual(EventPhase.captured, menu_event.phase);
}

test "activation and consumption are explicit phases" {
    var event = Event{};

    event.activate(.menu_run, .keyboard);
    try testing.expectEqual(UiTarget.menu_run, event.command);
    try testing.expectEqual(EventSource.keyboard, event.source);
    try testing.expectEqual(EventPhase.activated, event.phase);

    event.consume();
    try testing.expect(event.consumed);
    try testing.expectEqual(EventPhase.consumed, event.phase);
}

test "target activation keeps owner metadata for keyboard commands" {
    var event = Event{};

    event.activateTarget(.menu_tasks, .hotkey);

    try testing.expectEqual(UiTarget.menu_tasks, event.target);
    try testing.expectEqual(UiOwner.start_menu, event.owner);
    try testing.expectEqual(UiLayer.popup, event.layer);
    try testing.expectEqual(UiTarget.menu_tasks, event.command);
    try testing.expectEqual(EventSource.hotkey, event.source);
    try testing.expect((event.flags & EventFlag.enabled) != 0);
    try testing.expectEqual(EventPhase.activated, event.phase);
}

test "target activation preserves existing hit-test target" {
    var event = Event{};

    event.setTarget(.run_ok);
    event.activateTarget(.run_cancel, .mouse);

    try testing.expectEqual(UiTarget.run_ok, event.target);
    try testing.expectEqual(UiOwner.dialog, event.owner);
    try testing.expectEqual(UiLayer.dialog, event.layer);
    try testing.expectEqual(UiTarget.run_cancel, event.command);
    try testing.expectEqual(EventSource.mouse, event.source);
}

test "PowerShell export keeps current builder constants covered" {
    try testing.expectEqual(@as(usize, 145), ps_constants.len);
    try testing.expectEqual(@as(usize, 12), start_menu_items.len);
    try testing.expectEqual(@as(usize, 3), policy_labels.len);
    try testing.expectEqual(@as(u8, 1), start_menu_first_id);
    try testing.expectEqual(@as(u8, 19), start_menu_last_id);
    try testing.expectEqualStrings("UiTargetNone", ps_constants[0].name);
    try testing.expectEqual(@as(u32, @intFromEnum(UiTarget.none)), ps_constants[0].value);
    try testing.expectEqualStrings("UiModifierAlt", ps_constants[ps_constants.len - 1].name);
    try testing.expectEqual(@as(u32, Modifier.alt), ps_constants[ps_constants.len - 1].value);
}
