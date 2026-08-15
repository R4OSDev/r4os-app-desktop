const r4os = @import("r4os");
const desk_api = @import("api.zig");
const desktop_config = @import("desktop_config.zig");
const desktop_items = @import("desktop_items.zig");
const gui_frame_snapshot = @import("gui_frame_snapshot.zig");
const draw = @import("draw.zig");
const message_box = @import("message_box.zig");
const model = @import("model.zig");
const quick_launch = @import("quick_launch.zig");
const start_menu = @import("start_menu.zig");
const wallpaper = @import("wallpaper.zig");
const window = @import("window.zig");

pub const MessageBox = message_box.View;

pub const RunOverlay = struct {
    path: [*:0]const u8,
    focus: model.UiTarget,
};

pub const TasksOverlay = struct {
    status: r4os.abi.ProgramStatus,
    instances: []const r4os.abi.ProgramInstanceSnapshot,
    render: RenderStats,
    focus: model.UiTarget,
};

pub const RenderStats = draw.RenderStats;
pub const DamageKind = draw.DamageKind;

pub const SettingsOverlay = struct {
    config: desktop_config.Config,
    focus: model.UiTarget,
};

pub const SystemMenu = struct {
    open: bool = false,
    x: i32 = 0,
    y: i32 = 0,
    window_index: usize = 0,
};

pub const Overlay = union(enum) {
    none,
    run: RunOverlay,
    tasks: TasksOverlay,
    settings: SettingsOverlay,
    message_box: MessageBox,
};

pub fn compose(
    ctx: *const desk_api.Context,
    screen_w: i32,
    screen_h: i32,
    windows: []const window.Window,
    gui_frames: []const gui_frame_snapshot.View,
    active_window: usize,
    clock: [*:0]const u8,
    keyboard_layout: [*:0]const u8,
    items: *const desktop_items.Items,
    quick_bar: *const quick_launch.Bar,
    selected_item: usize,
    desktop_grid_drag_index: usize,
    start_open: bool,
    menu: *const start_menu.Menu,
    menu_selected: usize,
    menu_submenu_open: bool,
    menu_submenu_parent: usize,
    menu_submenu_selected: usize,
    menu_nested_open: bool,
    menu_nested_parent: usize,
    menu_nested_selected: usize,
    overlay: Overlay,
    system_menu: SystemMenu,
    time_menu_open: bool,
    console_title: [*:0]const u8,
    console_path: [*:0]const u8,
    console_args: [*:0]const u8,
    console_scroll_offsets: []const u32,
    wallpaper_view: wallpaper.View,
    config: desktop_config.Config,
    terminal_font_size: u8,
    terminal_codepage: u16,
    terminal_mode: bool,
    cursor_blink_on: bool,
    cursor_x: i32,
    cursor_y: i32,
    hover_target: model.UiTarget,
    pressed_target: model.UiTarget,
) void {
    if (terminal_mode) {
        const instance_id = if (windows.len > 0) windows[0].instance_id else 0;
        const scroll_offset = if (console_scroll_offsets.len > 0) console_scroll_offsets[0] else 0;
        draw.fullscreenConsole(ctx, screen_w, screen_h, instance_id, terminal_font_size, terminal_codepage, scroll_offset, cursor_blink_on);
        return;
    }

    draw.desktop(ctx, screen_w, screen_h, config.desktop_bg);
    if (wallpaper_view.pixels.len > 0) {
        const origin = wallpaper_view.origin(screen_w, screen_h);
        ctx.paintXrgb32(origin.x, origin.y, wallpaper_view.width, wallpaper_view.height, wallpaper_view.pixels);
    }
    if (desktop_grid_drag_index != desktop_items.no_selection) draw.desktopItemGrid(ctx, screen_w, screen_h, items, desktop_grid_drag_index);
    draw.desktopItems(ctx, items, selected_item, hover_target, pressed_target, config.desktop_bg, config.desktop_icon_text);
    drawWindows(ctx, windows, gui_frames, active_window, console_title, console_path, console_args, console_scroll_offsets, terminal_font_size, terminal_codepage, cursor_blink_on, hover_target, pressed_target);
    draw.taskbar(ctx, screen_w, screen_h, windows, quick_bar, active_window, if (config.taskbar_clock) clock else null, keyboard_layout, hover_target, pressed_target);
    if (start_open) draw.startMenu(ctx, screen_w, screen_h, menu, menu_selected, menu_submenu_open, menu_submenu_parent, menu_submenu_selected, menu_nested_open, menu_nested_parent, menu_nested_selected, hover_target, pressed_target);
    if (system_menu.open and system_menu.window_index < windows.len) draw.systemMenu(ctx, system_menu.x, system_menu.y, &windows[system_menu.window_index], system_menu.window_index, hover_target, pressed_target);
    if (time_menu_open) draw.timeMenu(ctx, screen_w, screen_h, hover_target, pressed_target);
    drawOverlay(ctx, screen_w, screen_h, windows, active_window, overlay, hover_target, pressed_target);
    draw.cursor(ctx, cursor_x, cursor_y, screen_w, screen_h);
}

fn drawWindows(
    ctx: *const desk_api.Context,
    windows: []const window.Window,
    gui_frames: []const gui_frame_snapshot.View,
    active_window: usize,
    console_title: [*:0]const u8,
    console_path: [*:0]const u8,
    console_args: [*:0]const u8,
    console_scroll_offsets: []const u32,
    terminal_font_size: u8,
    terminal_codepage: u16,
    cursor_blink_on: bool,
    hover_target: model.UiTarget,
    pressed_target: model.UiTarget,
) void {
    for (windows, 0..) |*win, i| {
        const scroll_offset = if (i < console_scroll_offsets.len) console_scroll_offsets[i] else 0;
        const gui_frame = if (i < gui_frames.len) gui_frames[i] else gui_frame_snapshot.View{};
        if (i != active_window) draw.appWindow(ctx, win, gui_frame, i, false, console_title, console_path, console_args, terminal_font_size, terminal_codepage, scroll_offset, cursor_blink_on, hover_target, pressed_target);
    }
    if (active_window < windows.len) {
        const scroll_offset = if (active_window < console_scroll_offsets.len) console_scroll_offsets[active_window] else 0;
        const gui_frame = if (active_window < gui_frames.len) gui_frames[active_window] else gui_frame_snapshot.View{};
        draw.appWindow(ctx, &windows[active_window], gui_frame, active_window, true, console_title, console_path, console_args, terminal_font_size, terminal_codepage, scroll_offset, cursor_blink_on, hover_target, pressed_target);
    }
}

fn drawOverlay(
    ctx: *const desk_api.Context,
    screen_w: i32,
    screen_h: i32,
    windows: []const window.Window,
    active_window: usize,
    overlay: Overlay,
    hover_target: model.UiTarget,
    pressed_target: model.UiTarget,
) void {
    switch (overlay) {
        .none => {},
        .run => |run| draw.runDialog(ctx, screen_w, screen_h, run.path, run.focus, hover_target, pressed_target),
        .tasks => |tasks| draw.tasksDialog(ctx, screen_w, screen_h, windows, active_window, tasks.status, tasks.instances, tasks.render, tasks.focus, hover_target, pressed_target),
        .settings => |settings| draw.settingsDialog(ctx, screen_w, screen_h, settings.config, settings.focus, hover_target, pressed_target),
        .message_box => |box| draw.messageBoxDialog(ctx, screen_w, screen_h, box, hover_target, pressed_target),
    }
}
