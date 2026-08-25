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
const surface = @import("surface.zig");
const theme = @import("theme.zig");
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

pub const CullStats = struct {
    layers_visited: u32 = 0,
    layers_culled: u32 = 0,
    windows_visited: u32 = 0,
    windows_culled: u32 = 0,
    items_visited: u32 = 0,
    items_culled: u32 = 0,
    gui_frame_commands: u64 = 0,
    gui_resource_bytes: u64 = 0,
};

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
    damage: surface.Rect,
) CullStats {
    var stats = CullStats{};
    const desktop_rect = surface.desktop(screen_w, screen_h).rect;

    if (terminal_mode) {
        if (layerVisible(&stats, damage, desktop_rect)) {
            const instance_id = if (windows.len > 0) windows[0].instance_id else 0;
            const scroll_offset = if (console_scroll_offsets.len > 0) console_scroll_offsets[0] else 0;
            draw.fullscreenConsole(ctx, screen_w, screen_h, instance_id, terminal_font_size, terminal_codepage, scroll_offset, cursor_blink_on);
        }
        return stats;
    }

    if (layerVisible(&stats, damage, desktop_rect)) {
        draw.desktopBackground(ctx, screen_w, screen_h, config.desktop_bg);
    }

    const info_rect = draw.desktopInfoRect(ctx, screen_w, screen_h);
    if (layerVisible(&stats, damage, info_rect)) {
        draw.desktopInfoLayer(ctx, screen_w, screen_h, config.desktop_bg);
    }

    if (wallpaper_view.pixels.len > 0) {
        const origin = wallpaper_view.origin(screen_w, screen_h);
        const wallpaper_rect = surface.Rect{
            .x = origin.x,
            .y = origin.y,
            .w = @intCast(wallpaper_view.width),
            .h = @intCast(wallpaper_view.height),
        };
        if (layerVisible(&stats, damage, wallpaper_rect)) {
            ctx.paintXrgb32(origin.x, origin.y, wallpaper_view.width, wallpaper_view.height, wallpaper_view.pixels);
        }
    }

    if (desktop_grid_drag_index != desktop_items.no_selection and
        layerVisible(&stats, damage, surface.workArea(screen_w, screen_h, theme.taskbar_h)))
    {
        draw.desktopItemGrid(ctx, screen_w, screen_h, items, desktop_grid_drag_index);
    }

    if (layerVisible(&stats, damage, surface.workArea(screen_w, screen_h, theme.taskbar_h))) {
        drawDesktopItems(ctx, items, selected_item, hover_target, pressed_target, config.desktop_bg, config.desktop_icon_text, damage, &stats);
    } else {
        stats.items_visited +%= @intCast(items.count);
        stats.items_culled +%= @intCast(items.count);
    }

    if (layerVisible(&stats, damage, surface.workArea(screen_w, screen_h, theme.taskbar_h))) {
        drawWindows(ctx, windows, gui_frames, active_window, console_title, console_path, console_args, console_scroll_offsets, terminal_font_size, terminal_codepage, cursor_blink_on, hover_target, pressed_target, damage, &stats);
    } else {
        countCulledWindows(windows, &stats);
    }

    const taskbar_rect = surface.taskbar(screen_w, screen_h, theme.taskbar_h).rect;
    if (layerVisible(&stats, damage, taskbar_rect)) {
        draw.taskbar(ctx, screen_w, screen_h, windows, quick_bar, active_window, if (config.taskbar_clock) clock else null, keyboard_layout, hover_target, pressed_target);
    }

    if (start_open) {
        const rect = startMenuRect(screen_w, screen_h, menu, menu_submenu_open, menu_submenu_parent, menu_nested_open, menu_nested_parent);
        if (layerVisible(&stats, damage, rect)) {
            draw.startMenu(ctx, screen_w, screen_h, menu, menu_selected, menu_submenu_open, menu_submenu_parent, menu_submenu_selected, menu_nested_open, menu_nested_parent, menu_nested_selected, hover_target, pressed_target);
        }
    }

    if (system_menu.open and system_menu.window_index < windows.len) {
        const rect = surface.Rect{ .x = system_menu.x, .y = system_menu.y, .w = draw.system_menu_w, .h = draw.system_menu_h };
        if (layerVisible(&stats, damage, rect)) {
            draw.systemMenu(ctx, system_menu.x, system_menu.y, &windows[system_menu.window_index], system_menu.window_index, hover_target, pressed_target);
        }
    }

    if (time_menu_open) {
        const rect = draw.timeMenuRect(screen_w, screen_h);
        if (layerVisible(&stats, damage, rect)) {
            draw.timeMenu(ctx, screen_w, screen_h, hover_target, pressed_target);
        }
    }

    if (overlayRect(screen_w, screen_h, overlay)) |rect| {
        if (layerVisible(&stats, damage, rect)) {
            drawOverlay(ctx, screen_w, screen_h, windows, active_window, overlay, hover_target, pressed_target);
        }
    }

    const cursor_rect = surface.cursor(cursor_x, cursor_y, screen_w, screen_h).rect;
    if (layerVisible(&stats, damage, cursor_rect)) {
        draw.cursor(ctx, cursor_x, cursor_y, screen_w, screen_h);
    }
    return stats;
}

fn layerVisible(stats: *CullStats, damage: surface.Rect, bounds: surface.Rect) bool {
    stats.layers_visited +%= 1;
    if (damage.intersects(bounds)) return true;
    stats.layers_culled +%= 1;
    return false;
}

fn drawDesktopItems(
    ctx: *const desk_api.Context,
    items: *const desktop_items.Items,
    selected: usize,
    hover_target: model.UiTarget,
    pressed_target: model.UiTarget,
    bg_color: u32,
    icon_text_color: u32,
    damage: surface.Rect,
    stats: *CullStats,
) void {
    var index: usize = 0;
    while (index < items.count) : (index += 1) {
        stats.items_visited +%= 1;
        if (!damage.intersects(draw.desktopItemRect(items, index))) {
            stats.items_culled +%= 1;
            continue;
        }
        draw.desktopItem(ctx, items, index, selected, hover_target, pressed_target, bg_color, icon_text_color);
    }
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
    damage: surface.Rect,
    stats: *CullStats,
) void {
    for (windows, 0..) |*win, index| {
        if (index == active_window) continue;
        drawWindow(ctx, win, gui_frames, index, false, console_title, console_path, console_args, console_scroll_offsets, terminal_font_size, terminal_codepage, cursor_blink_on, hover_target, pressed_target, damage, stats);
    }
    if (active_window < windows.len) {
        drawWindow(ctx, &windows[active_window], gui_frames, active_window, true, console_title, console_path, console_args, console_scroll_offsets, terminal_font_size, terminal_codepage, cursor_blink_on, hover_target, pressed_target, damage, stats);
    }
}

fn drawWindow(
    ctx: *const desk_api.Context,
    win: *const window.Window,
    gui_frames: []const gui_frame_snapshot.View,
    index: usize,
    active: bool,
    console_title: [*:0]const u8,
    console_path: [*:0]const u8,
    console_args: [*:0]const u8,
    console_scroll_offsets: []const u32,
    terminal_font_size: u8,
    terminal_codepage: u16,
    cursor_blink_on: bool,
    hover_target: model.UiTarget,
    pressed_target: model.UiTarget,
    damage: surface.Rect,
    stats: *CullStats,
) void {
    if (!win.visible or win.minimized) return;
    stats.windows_visited +%= 1;
    if (!damage.intersects(win.frameSurface().rect)) {
        stats.windows_culled +%= 1;
        return;
    }
    const scroll_offset = if (index < console_scroll_offsets.len) console_scroll_offsets[index] else 0;
    const gui_frame = if (index < gui_frames.len) gui_frames[index] else gui_frame_snapshot.View{};
    if (gui_frame.valid) {
        stats.gui_frame_commands +%= @intCast(gui_frame.commands.len);
        stats.gui_resource_bytes +%= @intCast(gui_frame.resources.len);
    }
    draw.appWindow(ctx, win, gui_frame, index, active, console_title, console_path, console_args, terminal_font_size, terminal_codepage, scroll_offset, cursor_blink_on, hover_target, pressed_target);
}

fn countCulledWindows(windows: []const window.Window, stats: *CullStats) void {
    for (windows) |win| {
        if (!win.visible or win.minimized) continue;
        stats.windows_visited +%= 1;
        stats.windows_culled +%= 1;
    }
}

fn startMenuRect(
    screen_w: i32,
    screen_h: i32,
    menu: *const start_menu.Menu,
    submenu_open: bool,
    submenu_parent: usize,
    nested_open: bool,
    nested_parent: usize,
) surface.Rect {
    var rect = surface.Rect{
        .x = 0,
        .y = screen_h - theme.taskbar_h - theme.menu_h,
        .w = theme.menu_w,
        .h = theme.menu_h,
    };
    if (submenu_open) {
        if (menu.submenuRect(screen_w, screen_h, submenu_parent)) |submenu| rect = rect.merged(submenu);
    }
    if (submenu_open and nested_open) {
        if (menu.nestedRect(screen_w, screen_h, submenu_parent, nested_parent)) |nested| rect = rect.merged(nested);
    }
    return rect;
}

fn overlayRect(screen_w: i32, screen_h: i32, overlay: Overlay) ?surface.Rect {
    return switch (overlay) {
        .none => null,
        .run => centeredRect(screen_w, screen_h, draw.run_dialog_w, draw.run_dialog_h),
        .tasks => centeredRect(screen_w, screen_h, draw.tasks_dialog_w, draw.tasks_dialog_h),
        .settings => centeredRect(screen_w, screen_h, draw.settings_dialog_w, draw.settings_dialog_h),
        .message_box => centeredRect(screen_w, screen_h, draw.message_box_w, draw.message_box_h),
    };
}

fn centeredRect(screen_w: i32, screen_h: i32, w: i32, h: i32) surface.Rect {
    return .{
        .x = @divTrunc(screen_w - w, 2),
        .y = @divTrunc(screen_h - h, 2),
        .w = w,
        .h = h,
    };
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
