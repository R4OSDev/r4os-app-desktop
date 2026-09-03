const r4os = @import("r4os");
const desk_api = @import("api.zig");
const std = @import("std");
const appearance_colors = @import("appearance_colors.zig");
const console_scroll = @import("console_scroll.zig");
const desktop_config = @import("desktop_config.zig");
const desktop_items = @import("desktop_items.zig");
const gui_frame_snapshot = @import("gui_frame_snapshot.zig");
const gui_shape_renderer = @import("gui_shape_renderer.zig");
const icon_source = @import("icon_source.zig");
const message_box = @import("message_box.zig");
const model = @import("model.zig");
const quick_launch = @import("quick_launch.zig");
const theme = @import("theme.zig");
const start_menu = @import("start_menu.zig");
const surface = @import("surface.zig");
const tray = @import("tray.zig");
const volume = @import("volume.zig");
const window = @import("window.zig");

const console_output_render_max: usize = 16 * 1024;
const terminal_line_max: usize = 512;
const terminal_snapshot_line_max: usize = 256;
const terminal_changed_row_words: usize = (terminal_snapshot_line_max + 63) / 64;
const ico_buffer_max: usize = 8192;
// Stufe 3 der Iconkette (0.61.14): das generische Programmicon.
const standard_program_icon_path = "C:\\R4OS\\Media\\Icons\\Application.ico";
const version_file_max: usize = 256;
const inventory_file_max: usize = 2048;
const desktop_info_max: usize = 96;
const version_unknown = "unknown";
const ico_size: u16 = 32;
const ico_alpha_visible: u8 = 128;
const ico_cache_slots: usize = 32;
const ico_cache_path_max: usize = desktop_items.icon_max;
const ico_cache_image_max: u16 = ico_size;
const ico_cache_stride: usize = @as(usize, ico_cache_image_max);
const ico_cache_pixels: usize = ico_cache_stride * ico_cache_stride;
const desktop_grid_color: u32 = 0x20A0A0;
const desktop_grid_highlight: u32 = 0xF0C040;
pub const system_menu_w: i32 = 150;
pub const system_menu_h: i32 = 108;
pub const settings_dialog_w: i32 = 500;
pub const settings_dialog_h: i32 = 294;
pub const run_dialog_w: i32 = 420;
pub const run_dialog_h: i32 = 178;
pub const tasks_dialog_w: i32 = 500;
pub const tasks_dialog_h: i32 = 420;
pub const settings_ok_x: i32 = 162;
pub const settings_cancel_x: i32 = 254;
pub const settings_ok_y: i32 = 262;
pub const message_box_w: i32 = 380;
pub const message_box_h: i32 = 172;
pub const message_box_button_y: i32 = 126;
pub const message_box_button_h: i32 = 26;
pub const message_box_ok_x: i32 = 150;
pub const message_box_ok_w: i32 = 80;
pub const message_box_pair_a_x: i32 = 106;
pub const message_box_pair_b_x: i32 = 198;
pub const message_box_pair_w: i32 = 76;
const system_menu_row_h: i32 = 20;
pub const time_menu_w: i32 = 160;
pub const time_menu_h: i32 = 52;
const time_menu_row_h: i32 = 22;
const glyph_w: i32 = 8;
const glyph_h: i32 = 8;
var desktop_info: [desktop_info_max:0]u8 = .{0} ** desktop_info_max;
var desktop_info_len: usize = 0;
var desktop_info_loaded: bool = false;
var ico_cache: [ico_cache_slots]CachedIco = .{CachedIco{}} ** ico_cache_slots;
var ico_cache_next: usize = 0;
pub const taskbar_clock_w: i32 = 88;
pub const taskbar_clock_right_margin: i32 = 8;
pub const taskbar_layout_w: i32 = 34;
pub const taskbar_layout_gap: i32 = 6;

pub const DamageKind = enum(u8) {
    none = 0,
    cursor = 1,
    mixed = 2,
    full = 3,
};

pub const TerminalSnapshotLine = struct {
    offset: u16 = 0,
    len: u16 = 0,
};

pub const TerminalSnapshot = struct {
    valid: bool = false,
    has_output: bool = false,
    instance_id: u32 = 0,
    revision: u32 = 0,
    width: i32 = 0,
    height: i32 = 0,
    font_size: u8 = 0,
    codepage: u16 = 0,
    scroll_offset: u32 = 0,
    effective_offset: u32 = 0,
    state: r4os.abi.ConsoleState = .{},
    lines: [terminal_snapshot_line_max]TerminalSnapshotLine = .{TerminalSnapshotLine{}} ** terminal_snapshot_line_max,
    line_count: usize = 0,
    text: [console_output_render_max]u8 = .{0} ** console_output_render_max,
    text_len: usize = 0,
    raw: [console_output_render_max]u8 = .{0} ** console_output_render_max,
    raw_len: usize = 0,
    tail_line_len: u16 = 0,

    pub fn matches(self: *const TerminalSnapshot, instance_id: u32, revision: u32, rect: surface.Rect, font_size: u8, codepage: u16, scroll_offset: u32) bool {
        return self.valid and self.instance_id == instance_id and self.revision == revision and
            self.width == rect.w and self.height == rect.h and self.font_size == font_size and
            self.codepage == codepage and self.scroll_offset == scroll_offset;
    }

    pub fn invalidate(self: *TerminalSnapshot) void {
        self.valid = false;
    }
};

pub const TerminalRefreshStats = struct {
    state_reads: u32 = 0,
    output_bytes: u32 = 0,
    parsed_bytes: u32 = 0,
    parse_skipped_bytes: u32 = 0,
    visible_lines: u32 = 0,
    changed_lines: u32 = 0,
    skipped_lines: u32 = 0,
    full_rebuild: bool = false,
    incremental: bool = false,
    changed_rows: [terminal_changed_row_words]u64 = .{0} ** terminal_changed_row_words,

    pub fn rowChanged(self: *const TerminalRefreshStats, row: usize) bool {
        if (row >= terminal_snapshot_line_max) return false;
        return (self.changed_rows[row / 64] & (@as(u64, 1) << @intCast(row % 64))) != 0;
    }
};

pub const RectInfo = struct {
    x: i32 = 0,
    y: i32 = 0,
    w: i32 = 0,
    h: i32 = 0,
};

pub const RenderStats = struct {
    redraws: u32 = 0,
    cursor_moves: u32 = 0,
    cursor_only_presents: u32 = 0,
    console_snapshot_refreshes: u64 = 0,
    console_snapshot_hits: u64 = 0,
    console_state_reads: u64 = 0,
    console_output_bytes: u64 = 0,
    console_parse_bytes: u64 = 0,
    console_parse_skipped_bytes: u64 = 0,
    console_visible_lines: u64 = 0,
    console_changed_lines: u64 = 0,
    console_skipped_lines: u64 = 0,
    console_incremental_refreshes: u64 = 0,
    console_full_refreshes: u64 = 0,
    console_cursor_cache_misses: u64 = 0,
    mixed_damage_presents: u32 = 0,
    full_damage_presents: u32 = 0,
    total_damage_pixels: u64 = 0,
    last_damage_pixels: u32 = 0,
    last_damage_rect: RectInfo = .{},
    last_damage_kind: DamageKind = .none,
    frame_total_ticks: u64 = 0,
    frame_max_ticks: u64 = 0,
    frame_last_ticks: u64 = 0,
    frame_total_ns: u64 = 0,
    frame_max_ns: u64 = 0,
    frame_last_ns: u64 = 0,
    compose_total_ticks: u64 = 0,
    compose_max_ticks: u64 = 0,
    compose_last_ticks: u64 = 0,
    compose_total_ns: u64 = 0,
    compose_max_ns: u64 = 0,
    compose_last_ns: u64 = 0,
    present_total_ticks: u64 = 0,
    present_max_ticks: u64 = 0,
    present_last_ticks: u64 = 0,
    present_total_ns: u64 = 0,
    present_max_ns: u64 = 0,
    present_last_ns: u64 = 0,
    scene_blit_bytes_total: u64 = 0,
    scene_blit_bytes_last: u64 = 0,
    display_blit_bytes_total: u64 = 0,
    display_blit_bytes_last: u64 = 0,
    remote_shadow_bytes_total: u64 = 0,
    remote_shadow_bytes_last: u64 = 0,
    scene_copy_attempts: u64 = 0,
    scene_copy_successes: u64 = 0,
    scene_copy_skips: u64 = 0,
    present_attempts: u64 = 0,
    present_successes: u64 = 0,
    present_failures: u64 = 0,
    layers_visited_total: u64 = 0,
    layers_culled_total: u64 = 0,
    layers_visited_last: u32 = 0,
    layers_culled_last: u32 = 0,
    windows_visited_total: u64 = 0,
    windows_culled_total: u64 = 0,
    windows_visited_last: u32 = 0,
    windows_culled_last: u32 = 0,
    items_visited_total: u64 = 0,
    items_culled_total: u64 = 0,
    items_visited_last: u32 = 0,
    items_culled_last: u32 = 0,
    gui_frame_commands_total: u64 = 0,
    gui_frame_commands_last: u64 = 0,
    gui_resource_bytes_total: u64 = 0,
    gui_resource_bytes_last: u64 = 0,
    display_blit_calls_total: u64 = 0,
    display_blit_calls_last: u32 = 0,
    damage_regions_total: u64 = 0,
    damage_regions_last: u32 = 0,
    present_generation_last: u64 = 0,
    present_fence_last: u64 = 0,
    present_lost_frames: u32 = 0,
    present_backend_fallbacks: u32 = 0,
    input_present_total_ticks: u64 = 0,
    input_present_max_ticks: u64 = 0,
    input_present_last_ticks: u64 = 0,
    display_stride_presents: u32 = 0,
    display_legacy_row_presents: u32 = 0,
    remote_shadow_copies: u32 = 0,
    remote_shadow_skips: u32 = 0,
    remote_shadow_copies_last: u32 = 0,
    cursor_latency_total_ticks: u64 = 0,
    cursor_latency_max_ticks: u64 = 0,
    cursor_latency_last_ticks: u64 = 0,
    ui_blocker_count: u32 = 0,
    ui_blocker_max_ticks: u64 = 0,
    layout_worker_started: u32 = 0,
    layout_worker_completed: u32 = 0,
    layout_worker_errors: u32 = 0,
    layout_worker_total_ticks: u64 = 0,
    layout_worker_max_ticks: u64 = 0,
    layout_worker_last_ticks: u64 = 0,
};

const TerminalMetrics = struct {
    x: i32 = 0,
    y: i32 = 0,
    line_h: i32 = 16,
    cols: u32 = 80,
    rows: u32 = 25,
};

const ButtonGlyph = enum {
    close,
    maximize,
    minimize,
};

const CachedIco = struct {
    used: bool = false,
    target_size: u16 = 0,
    path: [ico_cache_path_max + 1]u8 = .{0} ** (ico_cache_path_max + 1),
    pixels: [ico_cache_pixels]u32 = .{0} ** ico_cache_pixels,
    alpha: [ico_cache_pixels]u8 = .{0} ** ico_cache_pixels,
};

pub fn desktop(ctx: *const desk_api.Context, screen_w: i32, screen_h: i32, bg: u32) void {
    desktopBackground(ctx, screen_w, screen_h, bg);
    desktopInfoLayer(ctx, screen_w, screen_h, bg);
    taskbar(ctx, screen_w, screen_h, null, null, 0, null, null, null, .{}, .{}, .none, .none);
}

pub fn desktopBackground(ctx: *const desk_api.Context, screen_w: i32, screen_h: i32, bg: u32) void {
    fillSurface(ctx, surface.desktop(screen_w, screen_h), bg);
}

pub fn desktopInfoRect(ctx: *const desk_api.Context, screen_w: i32, screen_h: i32) surface.Rect {
    const text = desktopInfoText(ctx);
    _ = text;
    const text_w: i32 = @as(i32, @intCast(desktop_info_len)) * glyph_w;
    const x = @max(8, screen_w - text_w - 10);
    const y = @max(8, screen_h - theme.taskbar_h - glyph_h - 8);
    return .{ .x = x, .y = y, .w = text_w, .h = glyph_h };
}

pub fn desktopInfoLayer(ctx: *const desk_api.Context, screen_w: i32, screen_h: i32, bg: u32) void {
    const text = desktopInfoText(ctx);
    const rect = desktopInfoRect(ctx, screen_w, screen_h);
    const x = rect.x;
    const y = rect.y;
    ctx.paintText(x, y, text, theme.title_text, bg);
}

fn desktopInfoText(ctx: *const desk_api.Context) [*:0]const u8 {
    if (!desktop_info_loaded) loadDesktopInfo(ctx);
    return @ptrCast(&desktop_info);
}

fn loadDesktopInfo(ctx: *const desk_api.Context) void {
    desktop_info_loaded = true;
    desktop_info_len = 0;
    desktop_info[0] = 0;
    appendDesktopInfo("R4OS ");

    var version_data: [version_file_max]u8 = undefined;
    const read = ctx.fileRead(r4os.version_info.release_file_path, version_data[0..]);
    if (read > 0) {
        const len: usize = @intCast(read);
        appendDesktopInfo(r4os.version_info.parseReleaseVersion(version_data[0..len]) orelse version_unknown);
    } else {
        appendDesktopInfo(version_unknown);
    }
    appendDesktopInfo(" | Kernel ");

    const active = ctx.dev.kernelVersion();
    var active_text_buffer: [32]u8 = undefined;
    if (active) |value| {
        appendDesktopInfo(r4os.version_info.formatKernelVersion(value, active_text_buffer[0..]) orelse version_unknown);

        var inventory_data: [inventory_file_max]u8 = undefined;
        const inventory_read = ctx.fileReadAt(r4os.version_info.inventory_file_path, 0, inventory_data[0..]);
        if (inventory_read > 0) {
            const inventory_len: usize = @intCast(inventory_read);
            if (r4os.version_info.parseInstalledKernelVersion(inventory_data[0..inventory_len])) |installed| {
                if (r4os.version_info.restartRequired(value, installed)) appendDesktopInfo(" | Restart required");
            }
        }
    } else {
        appendDesktopInfo(version_unknown);
    }
}

fn appendDesktopInfo(text: []const u8) void {
    if (desktop_info_len >= desktop_info.len) return;
    const count = @min(text.len, desktop_info.len - desktop_info_len);
    if (count == 0) return;
    @memcpy(desktop_info[desktop_info_len .. desktop_info_len + count], text[0..count]);
    desktop_info_len += count;
    desktop_info[desktop_info_len] = 0;
}

fn trim(s: []const u8) []const u8 {
    var start: usize = 0;
    var end: usize = s.len;
    while (start < end and isSpace(s[start])) : (start += 1) {}
    while (end > start and isSpace(s[end - 1])) : (end -= 1) {}
    return s[start..end];
}

fn isSpace(c: u8) bool {
    return c == ' ' or c == '\t' or c == '\r' or c == '\n';
}

fn startsWith(s: []const u8, prefix: []const u8) bool {
    return s.len >= prefix.len and std.mem.eql(u8, s[0..prefix.len], prefix);
}

pub fn desktopItems(ctx: *const desk_api.Context, items: *const desktop_items.Items, selected: usize, hover_target: model.UiTarget, pressed_target: model.UiTarget, bg_color: u32, icon_text_color: u32) void {
    var i: usize = 0;
    while (i < items.count) : (i += 1) {
        desktopItem(ctx, items, i, selected, hover_target, pressed_target, bg_color, icon_text_color);
    }
}

pub fn desktopItemRect(items: *const desktop_items.Items, index: usize) surface.Rect {
    if (index >= items.count) return .{ .x = 0, .y = 0, .w = 0, .h = 0 };
    const entry = &items.entries[index];
    const label_w = @as(i32, @intCast(zStringLen(entry.titleZ()))) * glyph_w;
    return .{
        .x = entry.x,
        .y = entry.y,
        .w = @max(desktop_items.icon_w, label_w + 4),
        .h = desktop_items.icon_h,
    };
}

pub fn desktopItem(ctx: *const desk_api.Context, items: *const desktop_items.Items, index: usize, selected: usize, hover_target: model.UiTarget, pressed_target: model.UiTarget, bg_color: u32, icon_text_color: u32) void {
    if (index >= items.count) return;
    const entry = &items.entries[index];
    const target = items.target(index);
    const active = selected == index or hover_target == target or pressed_target == target;
    const label_colors = appearance_colors.desktopIconLabel(active, bg_color, icon_text_color);
    const label_bg = label_colors.background;
    const label_fg = label_colors.foreground;
    const pressed_offset: i32 = if (pressed_target == target) 1 else 0;
    if (active) {
        ctx.paintRect(entry.x + 2, entry.y + 43, @intCast(desktop_items.icon_w - 4), 15, label_bg);
        if (pressed_target == target) bevel(ctx, entry.x, entry.y, desktop_items.icon_w, desktop_items.icon_h, true);
    }
    if (!desktopItemIco(ctx, entry, entry.x + 20 + pressed_offset, entry.y + 6 + pressed_offset)) {
        desktopIcon(ctx, entry.x + 20 + pressed_offset, entry.y + 6 + pressed_offset, entry.launch_kind);
    }
    if (hover_target == target and pressed_target != target) focusRect(ctx, entry.x + 2, entry.y + 2, desktop_items.icon_w - 4, desktop_items.icon_h - 4);
    ctx.paintText(shortcutLabelX(entry.x, entry.titleZ()), entry.y + 47, entry.titleZ(), label_fg, label_bg);
}

pub fn desktopItemGrid(ctx: *const desk_api.Context, screen_w: i32, screen_h: i32, items: *const desktop_items.Items, drag_index: usize) void {
    if (drag_index >= items.count) return;
    const work_bottom = screen_h - theme.taskbar_h;
    if (screen_w <= 0 or work_bottom <= 0) return;

    const metrics = desktop_items.GridMetrics.init(screen_w, screen_h, theme.taskbar_h);
    const preview = items.snapPreview(drag_index, screen_w, screen_h, theme.taskbar_h);
    var seen: [desktop_items.max_items * 2]desktop_items.GridPosition = undefined;
    var seen_count: usize = 0;
    var linear: usize = 0;
    while (linear < desktop_items.max_items * 2) : (linear += 1) {
        const position = metrics.positionForLinear(linear);
        if (gridPositionSeen(seen[0..seen_count], position)) continue;
        if (seen_count < seen.len) {
            seen[seen_count] = position;
            seen_count += 1;
        }
        const highlighted = if (preview) |target| target.x == position.x and target.y == position.y else false;
        desktopGridSlot(ctx, screen_w, work_bottom, position.x, position.y, highlighted);
    }
}

fn gridPositionSeen(seen: []const desktop_items.GridPosition, position: desktop_items.GridPosition) bool {
    for (seen) |entry| {
        if (entry.x == position.x and entry.y == position.y) return true;
    }
    return false;
}

fn desktopGridSlot(ctx: *const desk_api.Context, screen_w: i32, work_bottom: i32, x: i32, y: i32, highlighted: bool) void {
    const color = if (highlighted) desktop_grid_highlight else desktop_grid_color;
    const right = @min(screen_w, x + desktop_items.cell_w);
    const bottom = @min(work_bottom, y + desktop_items.cell_h);
    const w = right - x;
    const h = bottom - y;
    if (w <= 0 or h <= 0) return;

    dottedH(ctx, x, y, w, screen_w, work_bottom, color);
    dottedH(ctx, x, bottom - 1, w, screen_w, work_bottom, color);
    dottedV(ctx, x, y, h, screen_w, work_bottom, color);
    dottedV(ctx, right - 1, y, h, screen_w, work_bottom, color);
    if (highlighted) solidRectOutline(ctx, x + 1, y + 1, @min(desktop_items.icon_w, w - 2), @min(desktop_items.icon_h, h - 2), screen_w, work_bottom, color);
}

fn dottedH(ctx: *const desk_api.Context, x: i32, y: i32, w: i32, screen_w: i32, work_bottom: i32, color: u32) void {
    if (y < 0 or y >= work_bottom) return;
    const left = @max(0, x);
    const right = @min(screen_w, x + w);
    var px = left;
    while (px < right) : (px += 6) {
        const segment = @min(@as(i32, 3), right - px);
        if (segment > 0) ctx.paintRect(px, y, @intCast(segment), 1, color);
    }
}

fn dottedV(ctx: *const desk_api.Context, x: i32, y: i32, h: i32, screen_w: i32, work_bottom: i32, color: u32) void {
    if (x < 0 or x >= screen_w) return;
    const top = @max(0, y);
    const bottom = @min(work_bottom, y + h);
    var py = top;
    while (py < bottom) : (py += 6) {
        const segment = @min(@as(i32, 3), bottom - py);
        if (segment > 0) ctx.paintRect(x, py, 1, @intCast(segment), color);
    }
}

fn solidRectOutline(ctx: *const desk_api.Context, x: i32, y: i32, w: i32, h: i32, screen_w: i32, work_bottom: i32, color: u32) void {
    if (w <= 1 or h <= 1) return;
    clippedRect(ctx, x, y, w, 1, screen_w, work_bottom, color);
    clippedRect(ctx, x, y + h - 1, w, 1, screen_w, work_bottom, color);
    clippedRect(ctx, x, y, 1, h, screen_w, work_bottom, color);
    clippedRect(ctx, x + w - 1, y, 1, h, screen_w, work_bottom, color);
}

fn clippedRect(ctx: *const desk_api.Context, x: i32, y: i32, w: i32, h: i32, screen_w: i32, work_bottom: i32, color: u32) void {
    const left = @max(0, x);
    const top = @max(0, y);
    const right = @min(screen_w, x + w);
    const bottom = @min(work_bottom, y + h);
    const clipped_w = right - left;
    const clipped_h = bottom - top;
    if (clipped_w <= 0 or clipped_h <= 0) return;
    ctx.paintRect(left, top, @intCast(clipped_w), @intCast(clipped_h), color);
}

fn desktopIco(ctx: *const desk_api.Context, path: [*:0]const u8, x: i32, y: i32) bool {
    return icoImage(ctx, path, x, y, ico_size);
}

/// Zeichnet das Icon eines Desktopitems entlang der Kette aus
/// icon_source.plan - die Reihenfolge lebt DORT, nicht hier. false heisst:
/// alle Stufen fehlgeschlagen, der Aufrufer zeichnet den Code-Fallback.
fn desktopItemIco(ctx: *const desk_api.Context, entry: *const desktop_items.Item, x: i32, y: i32) bool {
    const source = icon_source.classify(entry.iconText());
    const is_program = entry.launch_kind == .program;
    const stages = icon_source.plan(source, is_program);
    for (stages.slice()) |stage| {
        const ok = switch (stage) {
            .external => desktopIco(ctx, entry.iconZ(), x, y),
            .container => containerIco(ctx, entry.launchPathZ(), x, y, ico_size),
            .standard => desktopIco(ctx, standard_program_icon_path, x, y),
        };
        if (ok) return true;
    }
    // Dateityp-Icons (0.61.15): Textdateien bekommen das Standard-Icon aus
    // Media/Icons; die Endungsliste lebt im SDK, nicht hier.
    if (entry.launch_kind == .file and r4os.ico.isTextFileName(entry.pathText())) {
        if (desktopIco(ctx, r4os.ico.textfile_icon_path, x, y)) return true;
    }
    return false;
}

/// Container-Icon 0 eines Moduls: on-demand ueber die R4SYS-Lese-API,
/// gecacht unter einem synthetischen Schluessel, der nie mit einem echten
/// DOS-Pfad kollidiert (Pfade beginnen mit Laufwerksbuchstabe, nie mit
/// "rsrc:").
fn containerIco(ctx: *const desk_api.Context, module_path: [*:0]const u8, x: i32, y: i32, target_size: u16) bool {
    if (target_size == 0 or target_size > ico_cache_image_max) return false;
    const module_len = zStringLen(module_path);
    if (module_len == 0 or module_len + 5 > ico_cache_path_max) return false;
    var key_buf: [ico_cache_path_max + 1]u8 = .{0} ** (ico_cache_path_max + 1);
    @memcpy(key_buf[0..5], "rsrc:");
    @memcpy(key_buf[5 .. 5 + module_len], module_path[0..module_len]);
    const key = key_buf[0 .. 5 + module_len];
    if (findCachedIco(key, target_size)) |icon| {
        return paintCachedIco(ctx, icon, x, y);
    }
    var buffer: [ico_buffer_max]u8 = undefined;
    const read = ctx.sys.moduleResourceRead(module_path, r4os.r4sys.module_resource_type_icon, 0, null, buffer[0..]);
    if (read <= 0) return false;
    const icon = decodeIntoIcoCache(buffer[0..@intCast(read)], key, target_size) orelse return false;
    return paintCachedIco(ctx, icon, x, y);
}

fn quickLaunchIco(ctx: *const desk_api.Context, path: [*:0]const u8, x: i32, y: i32) bool {
    return icoImage(ctx, path, x, y, quick_launch.icon_size);
}

fn icoImage(ctx: *const desk_api.Context, path: [*:0]const u8, x: i32, y: i32, target_size: u16) bool {
    const icon = cachedIco(ctx, path, target_size) orelse return false;
    return paintCachedIco(ctx, icon, x, y);
}

fn paintCachedIco(ctx: *const desk_api.Context, icon: *const CachedIco, x: i32, y: i32) bool {
    const size: usize = @intCast(icon.target_size);
    var py: usize = 0;
    while (py < size) : (py += 1) {
        var px: usize = 0;
        while (px < size) : (px += 1) {
            const idx = py * ico_cache_stride + px;
            if (icon.alpha[idx] < ico_alpha_visible) continue;
            ctx.paintRect(x + @as(i32, @intCast(px)), y + @as(i32, @intCast(py)), 1, 1, icon.pixels[idx]);
        }
    }
    return true;
}

fn cachedIco(ctx: *const desk_api.Context, path: [*:0]const u8, target_size: u16) ?*const CachedIco {
    if (target_size == 0 or target_size > ico_cache_image_max) return null;
    const key = iconCachePath(path) orelse return null;
    if (findCachedIco(key, target_size)) |icon| return icon;
    return loadCachedIco(ctx, path, key, target_size);
}

fn findCachedIco(path: []const u8, target_size: u16) ?*const CachedIco {
    for (&ico_cache) |*icon| {
        if (!icon.used or icon.target_size != target_size) continue;
        if (std.mem.eql(u8, cachedIcoPath(icon), path)) return icon;
    }
    return null;
}

fn loadCachedIco(ctx: *const desk_api.Context, path_z: [*:0]const u8, path: []const u8, target_size: u16) ?*const CachedIco {
    var buffer: [ico_buffer_max]u8 = undefined;
    const read = ctx.fileRead(path_z, buffer[0..]);
    if (read <= 0) return null;
    return decodeIntoIcoCache(buffer[0..@as(usize, @intCast(read))], path, target_size);
}

/// Dekodiert ICO-Bytes in einen Cacheslot. Gemeinsamer Kern fuer Datei-
/// (loadCachedIco) und Containerquelle (containerIco); der Schluessel ist
/// bei Dateien der Pfad, bei Containern "rsrc:" plus Modulpfad.
fn decodeIntoIcoCache(bytes: []const u8, path: []const u8, target_size: u16) ?*const CachedIco {
    if (path.len == 0 or path.len > ico_cache_path_max) return null;
    const entry = r4os.ico.chooseBest(bytes, target_size) catch return null;
    const image = r4os.ico.parseBmpImage(bytes, entry) catch return null;
    if (image.width == 0 or image.height == 0) return null;

    const draw_w = @min(@as(u32, target_size), image.width);
    const draw_h = @min(@as(u32, target_size), image.height);
    const offset_x: u32 = @divTrunc(@as(u32, target_size) - draw_w, 2);
    const offset_y: u32 = @divTrunc(@as(u32, target_size) - draw_h, 2);
    const icon = reserveIcoCacheSlot();
    icon.used = false;
    icon.target_size = target_size;
    @memset(icon.path[0..], 0);
    @memcpy(icon.path[0..path.len], path);
    @memset(icon.pixels[0..], 0);
    @memset(icon.alpha[0..], 0);

    var py: u32 = 0;
    while (py < draw_h) : (py += 1) {
        var px: u32 = 0;
        while (px < draw_w) : (px += 1) {
            const src_x = @divTrunc(px * image.width, draw_w);
            const src_y = @divTrunc(py * image.height, draw_h);
            const pixel = r4os.ico.pixelAt(bytes, image, src_x, src_y) orelse continue;
            const dst_x = offset_x + px;
            const dst_y = offset_y + py;
            const idx = @as(usize, @intCast(dst_y)) * ico_cache_stride + @as(usize, @intCast(dst_x));
            icon.pixels[idx] = pixel.rgb;
            icon.alpha[idx] = pixel.alpha;
        }
    }
    icon.used = true;
    return icon;
}

fn reserveIcoCacheSlot() *CachedIco {
    for (&ico_cache) |*icon| {
        if (!icon.used) return icon;
    }
    const icon = &ico_cache[ico_cache_next];
    ico_cache_next = (ico_cache_next + 1) % ico_cache.len;
    return icon;
}

fn iconCachePath(path: [*:0]const u8) ?[]const u8 {
    const len = zStringLen(path);
    if (len == 0 or len > ico_cache_path_max) return null;
    return path[0..len];
}

fn cachedIcoPath(icon: *const CachedIco) []const u8 {
    var len: usize = 0;
    while (len < icon.path.len and icon.path[len] != 0) : (len += 1) {}
    return icon.path[0..len];
}

fn desktopIcon(ctx: *const desk_api.Context, x: i32, y: i32, kind: desktop_items.LaunchKind) void {
    const accent = switch (kind) {
        .directory => 0xF0C040,
        .program => theme.title_active,
        .file => theme.window_bg,
    };
    ctx.paintRect(x + 5, y, 24, 4, theme.taskbar_light);
    ctx.paintRect(x + 1, y + 4, 32, 26, theme.window_bg);
    ctx.paintRect(x + 3, y + 6, 28, 22, accent);
    ctx.paintRect(x + 6, y + 10, 22, 2, theme.taskbar_light);
    ctx.paintRect(x + 6, y + 15, 18, 2, theme.taskbar_light);
    ctx.paintRect(x + 6, y + 20, 14, 2, theme.taskbar_light);
    ctx.paintRect(x, y + 30, 34, 2, theme.shadow);
}

fn shortcutLabelX(cell_x: i32, label: [*:0]const u8) i32 {
    const text_w = @min(@as(i32, @intCast(zStringLen(label))) * glyph_w, desktop_items.icon_w - 8);
    return cell_x + @divTrunc(desktop_items.icon_w - text_w, 2);
}

fn startGlyph(ctx: *const desk_api.Context, x: i32, y: i32, pressed: bool) void {
    const off: i32 = if (pressed) 1 else 0;
    ctx.paintRect(x + off, y + off, 5, 5, 0xD04040);
    ctx.paintRect(x + 6 + off, y + off, 5, 5, 0x40A050);
    ctx.paintRect(x + off, y + 6 + off, 5, 5, 0x4080D0);
    ctx.paintRect(x + 6 + off, y + 6 + off, 5, 5, 0xF0C040);
    ctx.paintRect(x + 12 + off, y + 1 + off, 1, 11, theme.shadow);
}

fn taskbarStatusGlyph(ctx: *const desk_api.Context, x: i32, y: i32, active: bool, minimized: bool) void {
    const color = if (active) theme.title_active else if (minimized) theme.taskbar_dark else 0x4080D0;
    ctx.paintRect(x, y, 9, 9, theme.taskbar_light);
    ctx.paintRect(x + 1, y + 1, 7, 7, color);
    ctx.paintRect(x + 8, y + 1, 1, 8, theme.shadow);
    ctx.paintRect(x + 1, y + 8, 8, 1, theme.shadow);
}

pub fn taskbarClockRect(screen_w: i32, screen_h: i32) surface.Rect {
    const top = screen_h - theme.taskbar_h;
    return .{
        .x = screen_w - taskbar_clock_right_margin - taskbar_clock_w,
        .y = top + 4,
        .w = taskbar_clock_w,
        .h = theme.start_h,
    };
}

pub fn taskbarVolumeRect(screen_w: i32, screen_h: i32, clock_visible: bool) surface.Rect {
    const top = screen_h - theme.taskbar_h;
    const right = if (clock_visible)
        taskbarClockRect(screen_w, screen_h).x - volume.icon_gap
    else
        screen_w - taskbar_clock_right_margin;
    return volume.iconRect(right, top + 4, theme.start_h);
}

pub fn taskbarKeyboardLayoutRect(screen_w: i32, screen_h: i32, clock_visible: bool, volume_visible: bool) surface.Rect {
    const top = screen_h - theme.taskbar_h;
    const right = if (volume_visible)
        taskbarVolumeRect(screen_w, screen_h, clock_visible).x - taskbar_layout_gap
    else if (clock_visible)
        taskbarClockRect(screen_w, screen_h).x - taskbar_layout_gap
    else
        screen_w - taskbar_clock_right_margin;
    return .{
        .x = right - taskbar_layout_w,
        .y = top + 4,
        .w = taskbar_layout_w,
        .h = theme.start_h,
    };
}

pub fn volumePopupRect(screen_w: i32, screen_h: i32, clock_visible: bool) surface.Rect {
    return volume.popupRect(
        taskbarVolumeRect(screen_w, screen_h, clock_visible),
        surface.workArea(screen_w, screen_h, theme.taskbar_h),
    );
}

pub fn volumeTrackRect(screen_w: i32, screen_h: i32, clock_visible: bool) surface.Rect {
    return volume.trackRect(volumePopupRect(screen_w, screen_h, clock_visible));
}

pub fn volumeMuteRect(screen_w: i32, screen_h: i32, clock_visible: bool) surface.Rect {
    return volume.muteRect(volumePopupRect(screen_w, screen_h, clock_visible));
}

pub fn timeMenuRect(screen_w: i32, screen_h: i32) surface.Rect {
    const work = surface.workArea(screen_w, screen_h, theme.taskbar_h);
    const clock = taskbarClockRect(screen_w, screen_h);
    const desired = surface.Rect{
        .x = clock.right() - time_menu_w,
        .y = screen_h - theme.taskbar_h - time_menu_h - 2,
        .w = time_menu_w,
        .h = time_menu_h,
    };
    return desired.clampInside(work);
}

pub fn taskbar(
    ctx: *const desk_api.Context,
    screen_w: i32,
    screen_h: i32,
    windows: ?[]const window.Window,
    quick_bar: ?*const quick_launch.Bar,
    active_index: usize,
    clock: ?[*:0]const u8,
    keyboard_layout: ?[*:0]const u8,
    volume_view: volume.View,
    tray_registry: ?*const tray.Registry,
    tray_hover: tray.Identity,
    tray_pressed: tray.Identity,
    hover_target: model.UiTarget,
    pressed_target: model.UiTarget,
) void {
    const taskbar_surface = surface.taskbar(screen_w, screen_h, theme.taskbar_h);
    const top = taskbar_surface.rect.y;
    fillSurface(ctx, taskbar_surface, theme.taskbar);
    ctx.paintRect(0, top, @intCast(screen_w), 1, theme.taskbar_light);
    bevel(ctx, 2, top + 4, theme.start_w, theme.start_h, pressed_target == .start_button);
    if (hover_target == .start_button and pressed_target != .start_button) focusRect(ctx, 5, top + 7, theme.start_w - 6, theme.start_h - 6);
    startGlyph(ctx, 10, top + 10, pressed_target == .start_button);
    textLit(ctx, 28, top + 11, "R4OS", theme.text, theme.taskbar);

    const quick_count = if (quick_bar) |bar| bar.count else 0;
    if (quick_bar) |bar| quickLaunchBar(ctx, screen_h, bar, hover_target, pressed_target);

    if (windows) |list| {
        const window_start = quick_launch.taskbarWindowStartX(quick_count);
        const window_right = if (tray_registry) |registry|
            registry.window_right
        else
            taskbarKeyboardLayoutRect(screen_w, screen_h, clock != null, volume_view.installed).x - tray.window_gap;
        const visible_count = visibleWindowCount(list);
        var ordinal: usize = 0;
        for (list, 0..) |win, i| {
            if (!win.visible) continue;
            const rect = tray.windowButtonRect(window_start, window_right, visible_count, ordinal, top + 4, theme.start_h);
            ordinal += 1;
            if (rect.isEmpty()) continue;
            const target = taskbarTarget(win, i);
            const active = i == active_index and !win.minimized;
            const hovered = hover_target == target and pressed_target != target;
            const pressed = active or pressed_target == target;
            const bg = if (pressed) theme.taskbar_pressed else if (hovered) theme.taskbar_hover else theme.taskbar;
            const title = clippedText(std.mem.span(win.title()), @max(0, rect.w - 34));
            bevel(ctx, rect.x, rect.y, rect.w, rect.h, pressed);
            ctx.paintRect(rect.x + 2, rect.y + 2, @intCast(@max(0, rect.w - 4)), @intCast(@max(0, rect.h - 4)), bg);
            if (rect.w >= 20) taskbarStatusGlyph(ctx, rect.x + 8, top + 11, active, win.minimized);
            if (hovered and rect.w >= 8) focusRect(ctx, rect.x + 3, rect.y + 3, rect.w - 6, rect.h - 6);
            if (rect.w >= 30) ctx.paintText(rect.x + 22, top + 11, @ptrCast(&title), theme.text, bg);
        }
    }

    if (tray_registry) |registry| {
        for (&registry.entries) |*entry| {
            if (!entry.used or !entry.layout_visible) continue;
            const identity = entry.identity();
            const pressed = tray_pressed.eql(identity);
            const hovered = tray_hover.eql(identity) and !pressed;
            const bg = if (pressed) theme.taskbar_pressed else if (hovered) theme.taskbar_hover else theme.taskbar;
            if (pressed or hovered) {
                bevel(ctx, entry.rect.x, entry.rect.y, entry.rect.w, entry.rect.h, pressed);
                ctx.paintRect(entry.rect.x + 2, entry.rect.y + 2, @intCast(entry.rect.w - 4), @intCast(entry.rect.h - 4), bg);
            }
            _ = ctx.paintArgb32(entry.rect.x + 4, entry.rect.y + 4, r4os.abi.tray_icon_width, r4os.abi.tray_icon_height, entry.icon[0..]);
            if ((entry.flags & r4os.abi.tray_item_flag_attention) != 0) {
                ctx.paintRect(entry.rect.x + 3, entry.rect.y + entry.rect.h - 3, @intCast(entry.rect.w - 6), 1, 0xE08020);
            }
        }
    }

    if (volume_view.installed) {
        const rect = taskbarVolumeRect(screen_w, screen_h, clock != null);
        const pressed = pressed_target == .taskbar_volume;
        const hovered = hover_target == .taskbar_volume and !pressed;
        const bg = if (pressed) theme.taskbar_pressed else if (hovered) theme.taskbar_hover else theme.taskbar;
        if (pressed or hovered) {
            bevel(ctx, rect.x, rect.y, rect.w, rect.h, pressed);
            ctx.paintRect(rect.x + 2, rect.y + 2, @intCast(rect.w - 4), @intCast(rect.h - 4), bg);
        }
        volumeGlyph(ctx, rect.x + 5, rect.y + 5, volume.iconState(volume_view), bg);
        if (hovered) focusRect(ctx, rect.x + 3, rect.y + 3, rect.w - 6, rect.h - 6);
    }

    if (keyboard_layout) |value| {
        const rect = taskbarKeyboardLayoutRect(screen_w, screen_h, clock != null, volume_view.installed);
        const pressed = pressed_target == .taskbar_keyboard_layout;
        const hovered = hover_target == .taskbar_keyboard_layout and !pressed;
        const bg = if (pressed) theme.taskbar_pressed else if (hovered) theme.taskbar_hover else theme.taskbar;
        bevel(ctx, rect.x, rect.y, rect.w, rect.h, pressed);
        ctx.paintRect(rect.x + 2, rect.y + 2, @intCast(rect.w - 4), @intCast(rect.h - 4), bg);
        if (hovered) focusRect(ctx, rect.x + 3, rect.y + 3, rect.w - 6, rect.h - 6);
        ctx.paintText(rect.x + @divTrunc(rect.w - 16, 2), top + 11, value, theme.text, bg);
    }

    if (clock) |value| {
        const rect = taskbarClockRect(screen_w, screen_h);
        const pressed = pressed_target == .taskbar_clock;
        const hovered = hover_target == .taskbar_clock and !pressed;
        const bg = if (pressed) theme.taskbar_pressed else if (hovered) theme.taskbar_hover else theme.taskbar;
        bevel(ctx, rect.x, rect.y, rect.w, rect.h, true);
        ctx.paintRect(rect.x + 2, rect.y + 2, @intCast(rect.w - 4), @intCast(rect.h - 4), bg);
        if (hovered) focusRect(ctx, rect.x + 3, rect.y + 3, rect.w - 6, rect.h - 6);
        ctx.paintText(rect.x + 10, top + 11, value, theme.text, bg);
    }
}

fn volumeGlyph(ctx: *const desk_api.Context, x: i32, y: i32, state: volume.IconState, bg: u32) void {
    const color = if (state == .unavailable) theme.taskbar_dark else theme.text;
    ctx.paintRect(x, y + 6, 4, 7, color);
    ctx.paintRect(x + 4, y + 4, 2, 11, color);
    ctx.paintRect(x + 6, y + 2, 2, 15, color);
    switch (state) {
        .normal, .loud => {
            ctx.paintRect(x + 10, y + 6, 1, 7, color);
            ctx.paintRect(x + 12, y + 4, 1, 11, color);
            if (state == .loud) {
                ctx.paintRect(x + 14, y + 2, 1, 15, color);
                ctx.paintRect(x + 16, y + 5, 1, 9, color);
            }
        },
        .quiet => ctx.paintRect(x + 10, y + 7, 1, 5, color),
        .muted, .unavailable => {
            const mark = if (state == .unavailable) 0xA00000 else color;
            var offset: i32 = 0;
            while (offset < 7) : (offset += 1) {
                ctx.paintRect(x + 10 + offset, y + 5 + offset, 1, 1, mark);
                ctx.paintRect(x + 16 - offset, y + 5 + offset, 1, 1, mark);
            }
        },
        .hidden => ctx.paintRect(x, y, 1, 1, bg),
    }
}

pub fn volumeTooltipRect(screen_w: i32, screen_h: i32, clock_visible: bool) surface.Rect {
    const icon = taskbarVolumeRect(screen_w, screen_h, clock_visible);
    const work = surface.workArea(screen_w, screen_h, theme.taskbar_h);
    return (surface.Rect{ .x = icon.x - 74, .y = icon.y - 34, .w = 176, .h = 28 }).clampInside(work);
}

pub fn volumeTooltip(ctx: *const desk_api.Context, screen_w: i32, screen_h: i32, clock_visible: bool, view: volume.View) void {
    if (!view.installed) return;
    const rect = volumeTooltipRect(screen_w, screen_h, clock_visible);
    ctx.paintRect(rect.x + 2, rect.y + 2, @intCast(rect.w), @intCast(rect.h), theme.shadow);
    ctx.paintRect(rect.x, rect.y, @intCast(rect.w), @intCast(rect.h), theme.text);
    ctx.paintRect(rect.x + 1, rect.y + 1, @intCast(rect.w - 2), @intCast(rect.h - 2), 0xFFFFE1);
    if (!view.reachable) {
        textLit(ctx, rect.x + 6, rect.y + 10, "Audio service unavailable", theme.text, 0xFFFFE1);
    } else if (view.muted) {
        textLit(ctx, rect.x + 6, rect.y + 10, "System volume: muted", theme.text, 0xFFFFE1);
    } else {
        var number: [4]u8 = .{0} ** 4;
        writeU32Z(number[0..], view.percent);
        textLit(ctx, rect.x + 6, rect.y + 10, "System volume:", theme.text, 0xFFFFE1);
        ctx.paintText(rect.x + 118, rect.y + 10, @ptrCast(&number), theme.text, 0xFFFFE1);
        textLit(ctx, rect.x + 143, rect.y + 10, "%", theme.text, 0xFFFFE1);
    }
}

pub fn volumePopup(ctx: *const desk_api.Context, screen_w: i32, screen_h: i32, clock_visible: bool, view: volume.View, hover_target: model.UiTarget, pressed_target: model.UiTarget) void {
    if (!view.installed or !view.popup_open) return;
    const rect = volumePopupRect(screen_w, screen_h, clock_visible);
    const track = volume.trackRect(rect);
    const mute = volume.muteRect(rect);
    ctx.paintRect(rect.x + 3, rect.y + 3, @intCast(rect.w), @intCast(rect.h), theme.shadow);
    ctx.paintRect(rect.x, rect.y, @intCast(rect.w), @intCast(rect.h), theme.window_bg);
    bevel(ctx, rect.x, rect.y, rect.w, rect.h, false);
    textLit(ctx, rect.x + 14, rect.y + 13, "System volume", theme.text, theme.window_bg);

    var number: [4]u8 = .{0} ** 4;
    writeU32Z(number[0..], view.percent);
    ctx.paintText(rect.right() - 42, rect.y + 13, @ptrCast(&number), theme.text, theme.window_bg);
    textLit(ctx, rect.right() - 16, rect.y + 13, "%", theme.text, theme.window_bg);

    const enabled = view.reachable;
    const track_bg = if (enabled) theme.client_bg else theme.taskbar;
    ctx.paintRect(track.x, track.y + 5, @intCast(track.w), 4, theme.taskbar_dark);
    ctx.paintRect(track.x + 1, track.y + 6, @intCast(@max(0, track.w - 2)), 2, track_bg);
    const fill_w: i32 = if (track.w <= 1) 0 else @intCast(@divTrunc(@as(i64, track.w - 1) * view.percent, 100));
    if (enabled and fill_w > 0) ctx.paintRect(track.x + 1, track.y + 6, @intCast(fill_w), 2, theme.title_active);
    const thumb_x = track.x + fill_w - 4;
    ctx.paintRect(thumb_x, track.y, 9, @intCast(track.h), if (enabled) theme.window_bg else theme.taskbar);
    bevel(ctx, thumb_x, track.y, 9, track.h, pressed_target == .volume_popup_slider);
    if (hover_target == .volume_popup_slider and enabled) focusRect(ctx, track.x - 2, track.y - 2, track.w + 4, track.h + 4);

    const mute_pressed = pressed_target == .volume_popup_mute or view.muted;
    const mute_hot = hover_target == .volume_popup_mute and !mute_pressed;
    const mute_bg = if (!enabled) theme.taskbar else if (mute_pressed) theme.taskbar_pressed else if (mute_hot) theme.taskbar_hover else theme.window_bg;
    bevel(ctx, mute.x, mute.y, mute.w, mute.h, mute_pressed);
    ctx.paintRect(mute.x + 2, mute.y + 2, @intCast(mute.w - 4), @intCast(mute.h - 4), mute_bg);
    ctx.paintRect(mute.x + 10, mute.y + 7, 10, 10, theme.client_bg);
    bevel(ctx, mute.x + 9, mute.y + 6, 12, 12, true);
    if (view.muted) {
        ctx.paintRect(mute.x + 11, mute.y + 11, 3, 2, theme.text);
        ctx.paintRect(mute.x + 13, mute.y + 13, 3, 2, theme.text);
        ctx.paintRect(mute.x + 15, mute.y + 9, 3, 2, theme.text);
    }
    textLit(ctx, mute.x + 29, mute.y + 9, "Mute", if (enabled) theme.text else theme.taskbar_dark, mute_bg);
    if (!enabled) textLit(ctx, rect.x + 119, mute.y + 9, "Unavailable", theme.taskbar_dark, theme.window_bg);
}

pub fn trayTooltip(ctx: *const desk_api.Context, registry: *const tray.Registry, identity: tray.Identity, screen_w: i32, screen_h: i32) void {
    const entry = registry.findByIdentity(identity) orelse return;
    const rect = registry.tooltipRect(identity, screen_w, screen_h, theme.taskbar_h) orelse return;
    ctx.paintRect(rect.x + 2, rect.y + 2, @intCast(rect.w), @intCast(rect.h), theme.shadow);
    ctx.paintRect(rect.x, rect.y, @intCast(rect.w), @intCast(rect.h), theme.text);
    ctx.paintRect(rect.x + 1, rect.y + 1, @intCast(rect.w - 2), @intCast(rect.h - 2), 0xFFFFE1);
    ctx.paintTextFontSlice(r4os.abi.gui_font_builtin_id, rect.x + 6, rect.y + 7, entry.tooltipSlice(), theme.text, 0xFFFFE1, rect.inset(4, 3));
}

fn visibleWindowCount(windows: []const window.Window) usize {
    var count: usize = 0;
    for (windows) |win| if (win.visible) {
        count += 1;
    };
    return count;
}

fn quickLaunchBar(ctx: *const desk_api.Context, screen_h: i32, bar: *const quick_launch.Bar, hover_target: model.UiTarget, pressed_target: model.UiTarget) void {
    if (bar.count == 0) return;
    const rect = quick_launch.barRect(screen_h, bar.count);
    const sep_x = quick_launch.separatorX(bar.count);
    ctx.paintRect(rect.x, rect.y + 3, 1, @intCast(rect.h - 6), theme.taskbar_dark);
    ctx.paintRect(rect.x + 1, rect.y + 3, 1, @intCast(rect.h - 6), theme.taskbar_light);
    ctx.paintRect(sep_x, rect.y + 3, 1, @intCast(rect.h - 6), theme.taskbar_dark);
    ctx.paintRect(sep_x + 1, rect.y + 3, 1, @intCast(rect.h - 6), theme.taskbar_light);

    var i: usize = 0;
    while (i < bar.count) : (i += 1) {
        const item = &bar.items[i];
        const target = bar.target(i);
        const button = quick_launch.buttonRect(screen_h, i);
        const pressed = pressed_target == target;
        const hovered = hover_target == target and !pressed;
        const bg = if (pressed) theme.taskbar_pressed else if (hovered) theme.taskbar_hover else theme.taskbar;
        bevel(ctx, button.x, button.y, button.w, button.h, pressed);
        ctx.paintRect(button.x + 2, button.y + 2, @intCast(button.w - 4), @intCast(button.h - 4), bg);
        if (hovered) focusRect(ctx, button.x + 3, button.y + 3, button.w - 6, button.h - 6);
        quickLaunchGlyph(ctx, button, item, pressed);
    }
}

fn quickLaunchGlyph(ctx: *const desk_api.Context, button: surface.Rect, item: *const quick_launch.Item, pressed: bool) void {
    const icon: i32 = @intCast(quick_launch.icon_size);
    const off: i32 = if (pressed) 1 else 0;
    const x = button.x + @divTrunc(button.w - icon, 2) + off;
    const y = button.y + @divTrunc(button.h - icon, 2) + off;
    if (item.icon[0] != 0 and quickLaunchIco(ctx, item.iconZ(), x, y)) return;
    switch (item.kind) {
        .show_desktop => showDesktopQuickGlyph(ctx, x, y),
        .program => computerQuickGlyph(ctx, x, y),
    }
}

fn showDesktopQuickGlyph(ctx: *const desk_api.Context, x: i32, y: i32) void {
    ctx.paintRect(x + 1, y + 2, 14, 10, theme.taskbar_light);
    ctx.paintRect(x + 2, y + 3, 12, 8, theme.title_active);
    ctx.paintRect(x + 3, y + 4, 10, 1, 0x40A0E0);
    ctx.paintRect(x + 3, y + 8, 7, 1, 0x40A0E0);
    ctx.paintRect(x + 5, y + 12, 6, 1, theme.shadow);
    ctx.paintRect(x + 4, y + 13, 8, 2, theme.taskbar_dark);
}

fn computerQuickGlyph(ctx: *const desk_api.Context, x: i32, y: i32) void {
    ctx.paintRect(x + 1, y + 2, 12, 9, theme.taskbar_light);
    ctx.paintRect(x + 2, y + 3, 10, 7, 0x008080);
    ctx.paintRect(x + 4, y + 11, 6, 1, theme.shadow);
    ctx.paintRect(x + 3, y + 12, 8, 2, theme.window_bg);
    ctx.paintRect(x + 11, y + 6, 4, 7, theme.taskbar_dark);
    ctx.paintRect(x + 12, y + 7, 2, 1, theme.taskbar_light);
    ctx.paintRect(x + 12, y + 10, 2, 1, theme.taskbar_light);
}

pub fn timeMenu(ctx: *const desk_api.Context, screen_w: i32, screen_h: i32, hover_target: model.UiTarget, pressed_target: model.UiTarget) void {
    const rect = timeMenuRect(screen_w, screen_h);
    ctx.paintRect(rect.x, rect.y, @intCast(rect.w), @intCast(rect.h), theme.window_bg);
    bevel(ctx, rect.x, rect.y, rect.w, rect.h, false);
    timeMenuRow(ctx, rect.x, rect.y + 4, "Clock", .time_menu_clock, hover_target, pressed_target);
    timeMenuRow(ctx, rect.x, rect.y + 26, "Settings", .time_menu_settings, hover_target, pressed_target);
}

fn timeMenuRow(ctx: *const desk_api.Context, x: i32, y: i32, comptime label: []const u8, target: model.UiTarget, hover_target: model.UiTarget, pressed_target: model.UiTarget) void {
    const hot = hover_target == target or pressed_target == target;
    const bg = if (hot) theme.select_bg else theme.window_bg;
    const fg = if (hot) theme.title_text else theme.text;
    ctx.paintRect(x + 3, y, @intCast(time_menu_w - 6), @intCast(time_menu_row_h), bg);
    if (target == .time_menu_clock) {
        ctx.paintRect(x + 12, y + 5, 10, 10, fg);
        ctx.paintRect(x + 14, y + 7, 6, 6, bg);
        ctx.paintRect(x + 17, y + 8, 1, 4, fg);
        ctx.paintRect(x + 17, y + 11, 4, 1, fg);
    } else {
        ctx.paintRect(x + 12, y + 6, 10, 2, fg);
        ctx.paintRect(x + 12, y + 10, 10, 2, fg);
        ctx.paintRect(x + 12, y + 14, 10, 2, fg);
    }
    textLit(ctx, x + 30, y + 7, label, fg, bg);
}

pub fn startMenu(
    ctx: *const desk_api.Context,
    screen_w: i32,
    screen_h: i32,
    menu: *const start_menu.Menu,
    selected: usize,
    submenu_open: bool,
    submenu_parent: usize,
    submenu_selected: usize,
    nested_open: bool,
    nested_parent: usize,
    nested_selected: usize,
    hover_target: model.UiTarget,
    pressed_target: model.UiTarget,
) void {
    const top = screen_h - theme.taskbar_h - theme.menu_h;
    ctx.paintRect(0, top, @intCast(theme.menu_w), @intCast(theme.menu_h), theme.window_bg);
    bevel(ctx, 0, top, theme.menu_w, theme.menu_h, false);
    ctx.paintRect(4, top + 4, 28, @intCast(theme.menu_h - 8), theme.title_active);
    textLit(ctx, 40, top + 14, "Desktop", theme.text, theme.window_bg);

    var i: usize = 0;
    while (i < menu.count) : (i += 1) {
        const pressed = menu.target(i) == pressed_target;
        const hovered = menu.target(i) == hover_target;
        const y = menu.itemY(screen_h, i);
        if (menu.startsGroup(i)) {
            const label_y = y - theme.menu_group_h + 5;
            ctx.paintText(42, label_y, menu.groupLabel(i), theme.title_inactive, theme.window_bg);
            ctx.paintRect(104, label_y + 5, @intCast(theme.menu_w - 112), 1, theme.taskbar_dark);
            ctx.paintRect(104, label_y + 6, @intCast(theme.menu_w - 112), 1, theme.taskbar_light);
        }
        if (i == selected or pressed or hovered) {
            ctx.paintRect(34, y - 2, @intCast(theme.menu_w - 40), @intCast(theme.menu_item_h), theme.select_bg);
            if (pressed) bevel(ctx, 34, y - 2, theme.menu_w - 40, theme.menu_item_h, true);
            ctx.paintText(44, y + 4, menu.label(i), theme.title_text, theme.select_bg);
            ctx.paintText(166, y + 4, menu.policy(i), theme.title_text, theme.select_bg);
        } else {
            ctx.paintText(44, y + 4, menu.label(i), theme.text, theme.window_bg);
            ctx.paintText(166, y + 4, menu.policy(i), theme.text, theme.window_bg);
        }
        if (menu.hasSubmenu(i)) {
            const bg = if (i == selected or pressed or hovered) theme.select_bg else theme.window_bg;
            const fg = if (i == selected or pressed or hovered) theme.title_text else theme.text;
            ctx.paintText(theme.menu_w - 18, y + 4, ">", fg, bg);
        }
    }
    if (submenu_open) startMenuSubmenu(ctx, screen_w, screen_h, menu, submenu_parent, submenu_selected, hover_target, pressed_target);
    if (submenu_open and nested_open) startMenuNestedSubmenu(ctx, screen_w, screen_h, menu, submenu_parent, nested_parent, nested_selected, hover_target, pressed_target);
}

fn startMenuSubmenu(
    ctx: *const desk_api.Context,
    screen_w: i32,
    screen_h: i32,
    menu: *const start_menu.Menu,
    parent: usize,
    selected: usize,
    hover_target: model.UiTarget,
    pressed_target: model.UiTarget,
) void {
    const rect = menu.submenuRect(screen_w, screen_h, parent) orelse return;
    const submenu = menu.submenu(parent) orelse return;
    ctx.paintRect(rect.x, rect.y, @intCast(rect.w), @intCast(rect.h), theme.window_bg);
    bevel(ctx, rect.x, rect.y, rect.w, rect.h, false);

    var i: usize = 0;
    while (i < submenu.count) : (i += 1) {
        const target = submenu.target(i);
        const pressed = target == pressed_target;
        const hovered = target == hover_target;
        const y = start_menu.submenuItemY(rect, i);
        if (i == selected or pressed or hovered) {
            ctx.paintRect(rect.x + 3, y - 2, @intCast(rect.w - 6), @intCast(theme.menu_item_h), theme.select_bg);
            if (pressed) bevel(ctx, rect.x + 3, y - 2, rect.w - 6, theme.menu_item_h, true);
            ctx.paintText(rect.x + 12, y + 4, submenu.label(i), theme.title_text, theme.select_bg);
            ctx.paintText(rect.x + rect.w - 70, y + 4, submenu.policy(i), theme.title_text, theme.select_bg);
        } else {
            ctx.paintText(rect.x + 12, y + 4, submenu.label(i), theme.text, theme.window_bg);
            ctx.paintText(rect.x + rect.w - 70, y + 4, submenu.policy(i), theme.text, theme.window_bg);
        }
        if (menu.submenuHasSubmenu(parent, i)) {
            const bg = if (i == selected or pressed or hovered) theme.select_bg else theme.window_bg;
            const fg = if (i == selected or pressed or hovered) theme.title_text else theme.text;
            ctx.paintText(rect.x + rect.w - 18, y + 4, ">", fg, bg);
        }
    }
}

fn startMenuNestedSubmenu(
    ctx: *const desk_api.Context,
    screen_w: i32,
    screen_h: i32,
    menu: *const start_menu.Menu,
    parent: usize,
    child_index: usize,
    selected: usize,
    hover_target: model.UiTarget,
    pressed_target: model.UiTarget,
) void {
    const rect = menu.nestedRect(screen_w, screen_h, parent, child_index) orelse return;
    const nested = menu.nestedSubmenu(parent, child_index) orelse return;
    ctx.paintRect(rect.x, rect.y, @intCast(rect.w), @intCast(rect.h), theme.window_bg);
    bevel(ctx, rect.x, rect.y, rect.w, rect.h, false);

    var i: usize = 0;
    while (i < nested.count) : (i += 1) {
        const target = nested.target(i);
        const pressed = target == pressed_target;
        const hovered = target == hover_target;
        const y = start_menu.submenuItemY(rect, i);
        if (i == selected or pressed or hovered) {
            ctx.paintRect(rect.x + 3, y - 2, @intCast(rect.w - 6), @intCast(theme.menu_item_h), theme.select_bg);
            if (pressed) bevel(ctx, rect.x + 3, y - 2, rect.w - 6, theme.menu_item_h, true);
            ctx.paintText(rect.x + 12, y + 4, nested.label(i), theme.title_text, theme.select_bg);
            ctx.paintText(rect.x + rect.w - 70, y + 4, nested.policy(i), theme.title_text, theme.select_bg);
        } else {
            ctx.paintText(rect.x + 12, y + 4, nested.label(i), theme.text, theme.window_bg);
            ctx.paintText(rect.x + rect.w - 70, y + 4, nested.policy(i), theme.text, theme.window_bg);
        }
    }
}

pub fn appWindow(
    ctx: *const desk_api.Context,
    win: *const window.Window,
    gui_frame: gui_frame_snapshot.View,
    index: usize,
    active: bool,
    console_title: [*:0]const u8,
    console_path: [*:0]const u8,
    console_args: [*:0]const u8,
    console_snapshot: ?*const TerminalSnapshot,
    terminal_font_size: u8,
    terminal_codepage: u16,
    terminal_scroll_offset: u32,
    cursor_blink_on: bool,
    hover_target: model.UiTarget,
    pressed_target: model.UiTarget,
) void {
    _ = terminal_codepage;
    _ = terminal_scroll_offset;
    if (!win.visible or win.minimized) return;
    const frame = win.frameSurface();
    const title_surface = win.titleSurface();
    const client = win.clientSurface();
    fillSurface(ctx, frame, theme.window_bg);
    bevelRect(ctx, frame.rect, false);
    const title = if (active) theme.title_active else theme.title_inactive;
    const window_title = if (win.kind == .terminal and index == 0 and console_title[0] != 0) console_title else win.title();
    fillSurface(ctx, title_surface, title);
    ctx.paintRect(title_surface.rect.x, title_surface.rect.y, @intCast(title_surface.rect.w), 1, if (active) theme.title_light else theme.taskbar_light);
    ctx.paintText(title_surface.rect.x + 5, title_surface.rect.y + 6, window_title, theme.title_text, title);
    windowButton(ctx, win.x + win.w - 22, win.y + 5, .close, pressed_target == closeTarget(win, index));
    windowButton(ctx, win.x + win.w - 40, win.y + 5, .maximize, pressed_target == maxTarget(win, index));
    windowButton(ctx, win.x + win.w - 58, win.y + 5, .minimize, pressed_target == minTarget(win, index));
    drawWindowButtonHover(ctx, win, index, hover_target, pressed_target);
    fillSurface(ctx, client, theme.client_bg);

    switch (win.kind) {
        .terminal => {
            const state = if (console_snapshot) |snapshot|
                if (snapshot.valid) snapshot.state else r4os.abi.ConsoleState{ .fg = theme.text, .bg = theme.client_bg }
            else
                r4os.abi.ConsoleState{ .fg = theme.text, .bg = theme.client_bg };
            fillSurface(ctx, client, state.bg);
            var rendered_snapshot = false;
            if (console_snapshot) |snapshot| {
                if (snapshot.valid and snapshot.has_output) {
                    renderTerminalSnapshot(ctx, client.rect, snapshot);
                    if (snapshot.effective_offset == 0) terminalCursor(ctx, client.rect, state, terminal_font_size, state.fg, cursor_blink_on);
                    rendered_snapshot = true;
                }
            }
            if (!rendered_snapshot) {
                textLit(ctx, client.rect.x + 10, client.rect.y + 6, "Console app host", state.fg, state.bg);
                if (index == 0) {
                    ctx.paintText(client.rect.x + 10, client.rect.y + 22, console_path, state.fg, state.bg);
                    if (console_args[0] != 0) ctx.paintText(client.rect.x + 10, client.rect.y + 38, console_args, state.fg, state.bg);
                }
                textLit(ctx, client.rect.x + 10, client.rect.y + 62, "Waiting for console output.", state.fg, state.bg);
                textLit(ctx, client.rect.x + 10, client.rect.y + 86, "C:\\>", state.fg, state.bg);
            }
        },
        .manager => {
            textLit(ctx, client.rect.x + 10, client.rect.y + 6, "Desktop window manager", theme.text, theme.client_bg);
            textLit(ctx, client.rect.x + 10, client.rect.y + 22, "Focus, move, resize and buttons are active.", theme.text, theme.client_bg);
            textLit(ctx, client.rect.x + 10, client.rect.y + 46, "R4X window binding is prepared.", theme.text, theme.client_bg);
        },
        .app => {
            var text_buf: [512]u8 = .{0} ** 512;
            if (win.close_requested) {
                textLit(ctx, client.rect.x + 10, client.rect.y + 46, "Close requested...", theme.text, theme.client_bg);
            } else if (useCommittedFrameCommands(gui_frame)) {
                hostedFrameCommands(ctx, client.rect, gui_frame);
            } else {
                var id_buf: [12]u8 = .{0} ** 12;
                writeU32Z(id_buf[0..], win.instance_id);
                textLit(ctx, client.rect.x + 10, client.rect.y + 6, "R4X GUI instance", theme.text, theme.client_bg);
                textLit(ctx, client.rect.x + 10, client.rect.y + 22, "Instance:", theme.text, theme.client_bg);
                ctx.paintText(client.rect.x + 88, client.rect.y + 22, @ptrCast(&id_buf), theme.text, theme.client_bg);
                if (ctx.guiText(win.instance_id, text_buf[0..]) > 0) {
                    hostedText(ctx, client.rect, text_buf[0..]);
                } else {
                    textLit(ctx, client.rect.x + 10, client.rect.y + 46, "Hosted text surface waiting.", theme.text, theme.client_bg);
                }
            }
        },
    }
}

fn useCommittedFrameCommands(frame: gui_frame_snapshot.View) bool {
    // An empty committed frame is also how the frozen gui_set_text API clears
    // its former command list before publishing the hosted text surface.
    return frame.valid and frame.commands.len != 0;
}

pub fn messageBoxDialog(ctx: *const desk_api.Context, screen_w: i32, screen_h: i32, box: message_box.View, hover_target: model.UiTarget, pressed_target: model.UiTarget) void {
    const x = @divTrunc(screen_w - message_box_w, 2);
    const y = @divTrunc(screen_h - message_box_h, 2);
    dialogFrame(ctx, x, y, message_box_w, message_box_h, box.title);
    ctx.paintRect(x + 3, y + theme.title_h + 4, @intCast(message_box_w - 6), 1, theme.taskbar_dark);
    ctx.paintRect(x + 3, y + theme.title_h + 5, @intCast(message_box_w - 6), 1, theme.taskbar_light);
    messageBoxIcon(ctx, x + 24, y + 50, box.kind);
    wrappedZ(ctx, x + 76, y + 52, message_box_w - 104, box.text, 3, theme.text, theme.window_bg);
    ctx.paintRect(x + 16, y + 112, @intCast(message_box_w - 32), 1, theme.taskbar_dark);
    ctx.paintRect(x + 16, y + 113, @intCast(message_box_w - 32), 1, theme.taskbar_light);
    messageBoxButtons(ctx, x, y, box.buttons, box.focus, hover_target, pressed_target);
}

pub fn runDialog(ctx: *const desk_api.Context, screen_w: i32, screen_h: i32, path: [*:0]const u8, focus: model.UiTarget, hover_target: model.UiTarget, pressed_target: model.UiTarget) void {
    const x = @divTrunc(screen_w - run_dialog_w, 2);
    const y = @divTrunc(screen_h - run_dialog_h, 2);
    var visible_path: [45]u8 = .{0} ** 45;
    copyTailZ(visible_path[0..], path);
    dialogFrameLit(ctx, x, y, run_dialog_w, run_dialog_h, "Run");
    textLit(ctx, x + 24, y + 48, "Program and arguments:", theme.text, theme.window_bg);
    bevel(ctx, x + 24, y + 68, 372, 24, true);
    ctx.paintText(x + 32, y + 76, @ptrCast(&visible_path), theme.text, theme.client_bg);
    if (focus == .run_input) focusRect(ctx, x + 26, y + 70, 368, 20);
    dialogButton(ctx, x + 24, y + 126, 88, "Browse", .run_browse, focus, hover_target, pressed_target, false, false);
    dialogButton(ctx, x + 202, y + 126, 76, "OK", .run_ok, focus, hover_target, pressed_target, true, false);
    dialogButton(ctx, x + 288, y + 126, 86, "Cancel", .run_cancel, focus, hover_target, pressed_target, false, true);
}

pub fn tasksDialog(
    ctx: *const desk_api.Context,
    screen_w: i32,
    screen_h: i32,
    windows: []const window.Window,
    active: usize,
    status: r4os.abi.ProgramStatus,
    instances: []const r4os.abi.ProgramInstanceSnapshot,
    render: RenderStats,
    focus: model.UiTarget,
    hover_target: model.UiTarget,
    pressed_target: model.UiTarget,
) void {
    const dialog_w: i32 = tasks_dialog_w;
    const dialog_h: i32 = tasks_dialog_h;
    const x = @divTrunc(screen_w - dialog_w, 2);
    const y = @divTrunc(screen_h - dialog_h, 2);
    var exit_buf: [14]u8 = .{0} ** 14;
    var instance_buf: [6]u8 = .{0} ** 6;
    writeI32Z(exit_buf[0..], status.last_exit_code);
    writeU8Z(instance_buf[0..], status.instance_count);
    dialogFrameLit(ctx, x, y, dialog_w, dialog_h, "Tasks");

    panel(ctx, x + 18, y + 38, 464, 106, "Desktop windows");
    var row_y = y + 60;
    row_y += 18;
    textLit(ctx, x + 36, y + 58, "Active", theme.title_inactive, theme.window_bg);
    textLit(ctx, x + 88, y + 58, "Title", theme.title_inactive, theme.window_bg);
    textLit(ctx, x + 286, y + 58, "State", theme.title_inactive, theme.window_bg);
    textLit(ctx, x + 390, y + 58, "Instance", theme.title_inactive, theme.window_bg);
    for (windows, 0..) |win, i| {
        if (!win.visible) continue;
        var bound_buf: [12]u8 = .{0} ** 12;
        taskRowBg(ctx, x + 28, row_y - 3, i == active);
        if (i == active) {
            textLit(ctx, x + 42, row_y, "yes", theme.title_text, theme.select_bg);
        } else {
            textLit(ctx, x + 42, row_y, "-", theme.text, theme.client_bg);
        }
        ctx.paintText(x + 88, row_y, win.title(), if (i == active) theme.title_text else theme.text, if (i == active) theme.select_bg else theme.client_bg);
        const row_fg = if (i == active) theme.title_text else theme.text;
        const row_bg = if (i == active) theme.select_bg else theme.client_bg;
        if (win.close_requested) {
            textLit(ctx, x + 286, row_y, "closing", row_fg, row_bg);
        } else if (win.minimized) {
            textLit(ctx, x + 286, row_y, "minimized", row_fg, row_bg);
        } else {
            textLit(ctx, x + 286, row_y, "visible", row_fg, row_bg);
        }
        if (win.instance_id != 0) {
            writeU32Z(bound_buf[0..], win.instance_id);
            ctx.paintText(x + 390, row_y, @ptrCast(&bound_buf), if (i == active) theme.title_text else theme.text, if (i == active) theme.select_bg else theme.client_bg);
        } else {
            textLit(ctx, x + 390, row_y, "-", row_fg, row_bg);
        }
        row_y += 18;
    }

    panel(ctx, x + 18, y + 156, 464, 154, "R4X runtime");
    row_y = y + 178;
    if (status.shell_running != 0) {
        textLit(ctx, x + 36, row_y, "Shell task: running", theme.text, theme.window_bg);
    } else {
        textLit(ctx, x + 36, row_y, "Shell task: idle", theme.text, theme.window_bg);
    }
    if (status.foreground_running != 0) {
        textLit(ctx, x + 260, row_y, "Foreground R4X: running", theme.text, theme.window_bg);
    } else {
        textLit(ctx, x + 260, row_y, "Foreground R4X: idle", theme.text, theme.window_bg);
    }
    row_y += 18;
    textLit(ctx, x + 36, row_y, "Active instances:", theme.text, theme.window_bg);
    ctx.paintText(x + 172, row_y, @ptrCast(&instance_buf), theme.text, theme.window_bg);
    if (status.display_used != 0) {
        textLit(ctx, x + 260, row_y, "Last display: app framebuffer", theme.text, theme.window_bg);
    } else {
        textLit(ctx, x + 260, row_y, "Last display: console/text", theme.text, theme.window_bg);
    }
    row_y += 18;
    textLit(ctx, x + 36, row_y, "ID", theme.title_inactive, theme.window_bg);
    textLit(ctx, x + 90, row_y, "Role", theme.title_inactive, theme.window_bg);
    textLit(ctx, x + 158, row_y, "Class", theme.title_inactive, theme.window_bg);
    textLit(ctx, x + 234, row_y, "State", theme.title_inactive, theme.window_bg);
    textLit(ctx, x + 328, row_y, "Window", theme.title_inactive, theme.window_bg);
    textLit(ctx, x + 408, row_y, "Exit", theme.title_inactive, theme.window_bg);
    row_y += 18;
    if (instances.len == 0) {
        textLit(ctx, x + 36, row_y, "No instance details", theme.title_inactive, theme.window_bg);
        row_y += 18;
    } else {
        for (instances, 0..) |snapshot, row| {
            const instance = snapshot.info;
            var id_buf: [12]u8 = .{0} ** 12;
            var window_buf: [12]u8 = .{0} ** 12;
            writeU32Z(id_buf[0..], instance.id);
            writeI32Z(window_buf[0..], instance.window_id);
            if ((row & 1) == 1) ctx.paintRect(x + 28, row_y - 3, 444, 16, 0xE8E8E8);
            ctx.paintText(x + 36, row_y, @ptrCast(&id_buf), theme.text, if ((row & 1) == 1) 0xE8E8E8 else theme.window_bg);
            roleText(ctx, x + 90, row_y, instance.role, if ((row & 1) == 1) 0xE8E8E8 else theme.window_bg);
            classText(ctx, x + 158, row_y, instance.app_class, if ((row & 1) == 1) 0xE8E8E8 else theme.window_bg);
            stateText(ctx, x + 234, row_y, instance.state, if ((row & 1) == 1) 0xE8E8E8 else theme.window_bg);
            ctx.paintText(x + 328, row_y, @ptrCast(&window_buf), theme.text, if ((row & 1) == 1) 0xE8E8E8 else theme.window_bg);
            ctx.paintText(x + 408, row_y, @ptrCast(&exit_buf), theme.text, if ((row & 1) == 1) 0xE8E8E8 else theme.window_bg);
            row_y += 18;
        }
    }
    panel(ctx, x + 18, y + 320, 464, 52, "Desktop render");
    var redraw_buf: [12]u8 = .{0} ** 12;
    var cursor_buf: [12]u8 = .{0} ** 12;
    var cursor_present_buf: [12]u8 = .{0} ** 12;
    var full_buf: [12]u8 = .{0} ** 12;
    writeU32Z(redraw_buf[0..], render.redraws);
    writeU32Z(cursor_buf[0..], render.cursor_moves);
    writeU32Z(cursor_present_buf[0..], render.cursor_only_presents);
    writeU32Z(full_buf[0..], render.full_damage_presents);
    textLit(ctx, x + 36, y + 340, "Redraws:", theme.text, theme.window_bg);
    ctx.paintText(x + 112, y + 340, @ptrCast(&redraw_buf), theme.text, theme.window_bg);
    textLit(ctx, x + 178, y + 340, "Cursor moves:", theme.text, theme.window_bg);
    ctx.paintText(x + 292, y + 340, @ptrCast(&cursor_buf), theme.text, theme.window_bg);
    textLit(ctx, x + 36, y + 356, "Cursor presents:", theme.text, theme.window_bg);
    ctx.paintText(x + 174, y + 356, @ptrCast(&cursor_present_buf), theme.text, theme.window_bg);
    textLit(ctx, x + 246, y + 356, "Full:", theme.text, theme.window_bg);
    ctx.paintText(x + 298, y + 356, @ptrCast(&full_buf), theme.text, theme.window_bg);
    textLit(ctx, x + 350, y + 356, "Last:", theme.text, theme.window_bg);
    damageKindText(ctx, x + 400, y + 356, render.last_damage_kind);

    dialogButton(ctx, x + 210, y + 378, 80, "OK", .task_overview_ok, focus, hover_target, pressed_target, true, true);
}

pub fn settingsDialog(ctx: *const desk_api.Context, screen_w: i32, screen_h: i32, config: desktop_config.Config, focus: model.UiTarget, hover_target: model.UiTarget, pressed_target: model.UiTarget) void {
    const x = @divTrunc(screen_w - settings_dialog_w, 2);
    const y = @divTrunc(screen_h - settings_dialog_h, 2);
    var bg_buf: [7]u8 = .{0} ** 7;
    var ui_font_buf: [38]u8 = .{0} ** 38;
    var ui_size_buf: [4]u8 = .{0} ** 4;
    var terminal_font_buf: [38]u8 = .{0} ** 38;
    var terminal_size_buf: [4]u8 = .{0} ** 4;
    writeHex6Z(bg_buf[0..], config.desktop_bg);
    copyTailZ(ui_font_buf[0..], config.uiFontPath());
    writeU8Z(ui_size_buf[0..], config.ui_font_size);
    copyTailZ(terminal_font_buf[0..], config.terminalFontPath());
    writeU8Z(terminal_size_buf[0..], config.terminal_font_size);

    dialogFrameLit(ctx, x, y, settings_dialog_w, settings_dialog_h, "Settings");
    textLit(ctx, x + 24, y + 46, "Desktop background:", theme.text, theme.window_bg);
    ctx.paintRect(x + 198, y + 42, 42, 20, config.desktop_bg);
    focusRect(ctx, x + 198, y + 42, 42, 20);
    ctx.paintText(x + 252, y + 48, @ptrCast(&bg_buf), theme.text, theme.window_bg);
    textLit(ctx, x + 24, y + 74, "Taskbar clock:", theme.text, theme.window_bg);
    if (config.taskbar_clock) {
        textLit(ctx, x + 198, y + 74, "On", theme.text, theme.window_bg);
    } else {
        textLit(ctx, x + 198, y + 74, "Off", theme.text, theme.window_bg);
    }
    textLit(ctx, x + 24, y + 102, "UI font:", theme.text, theme.window_bg);
    ctx.paintText(x + 198, y + 102, @ptrCast(&ui_font_buf), theme.text, theme.window_bg);
    textLit(ctx, x + 24, y + 126, "UI font size:", theme.text, theme.window_bg);
    ctx.paintText(x + 198, y + 126, @ptrCast(&ui_size_buf), theme.text, theme.window_bg);
    textLit(ctx, x + 24, y + 150, "Terminal font:", theme.text, theme.window_bg);
    ctx.paintText(x + 198, y + 150, @ptrCast(&terminal_font_buf), theme.text, theme.window_bg);
    textLit(ctx, x + 24, y + 174, "Terminal font size:", theme.text, theme.window_bg);
    ctx.paintText(x + 198, y + 174, @ptrCast(&terminal_size_buf), theme.text, theme.window_bg);
    textLit(ctx, x + 24, y + 202, "Start menu:", theme.text, theme.window_bg);
    textLit(ctx, x + 136, y + 202, "C:\\R4OS\\SOFTWARE\\DESKTOP\\MENU.R4S", theme.text, theme.window_bg);
    textLit(ctx, x + 24, y + 222, "Desktop folder:", theme.text, theme.window_bg);
    textLit(ctx, x + 136, y + 222, "C:\\R4OS\\DESKTOP", theme.text, theme.window_bg);
    textLit(ctx, x + 24, y + 246, "Config and desktop folder are read at startup with safe defaults.", theme.text, theme.window_bg);
    dialogButton(ctx, x + settings_ok_x, y + settings_ok_y, 80, "OK", .settings_ok, focus, hover_target, pressed_target, true, false);
    dialogButton(ctx, x + settings_cancel_x, y + settings_ok_y, 86, "Cancel", .settings_cancel, focus, hover_target, pressed_target, false, true);
}

fn dialogFrame(ctx: *const desk_api.Context, x: i32, y: i32, w: i32, h: i32, title: [*:0]const u8) void {
    ctx.paintRect(x, y, @intCast(w), @intCast(h), theme.window_bg);
    bevel(ctx, x, y, w, h, false);
    ctx.paintRect(x + 3, y + 3, @intCast(w - 6), @intCast(theme.title_h), theme.title_active);
    clippedZ(ctx, x + 10, y + 9, w - 20, title, theme.title_text, theme.title_active);
}

fn dialogFrameLit(ctx: *const desk_api.Context, x: i32, y: i32, w: i32, h: i32, comptime title: []const u8) void {
    ctx.paintRect(x, y, @intCast(w), @intCast(h), theme.window_bg);
    bevel(ctx, x, y, w, h, false);
    ctx.paintRect(x + 3, y + 3, @intCast(w - 6), @intCast(theme.title_h), theme.title_active);
    textLit(ctx, x + 10, y + 9, title, theme.title_text, theme.title_active);
}

fn dialogButton(ctx: *const desk_api.Context, x: i32, y: i32, w: i32, comptime label: []const u8, target: model.UiTarget, focus: model.UiTarget, hover_target: model.UiTarget, pressed_target: model.UiTarget, is_default: bool, is_cancel: bool) void {
    if (is_default) {
        ctx.paintRect(x - 1, y - 1, @intCast(w + 2), 1, theme.text);
        ctx.paintRect(x - 1, y - 1, 1, 28, theme.text);
        ctx.paintRect(x - 1, y + 26, @intCast(w + 2), 1, theme.text);
        ctx.paintRect(x + w, y - 1, 1, 28, theme.text);
    }
    bevel(ctx, x, y, w, 26, pressed_target == target);
    const bg = theme.taskbar;
    const off: i32 = if (pressed_target == target) 1 else 0;
    const text_x = x + @max(@as(i32, 4), @divTrunc(w - @as(i32, @intCast(label.len)) * glyph_w, 2)) + off;
    textLit(ctx, text_x, y + 9 + off, label, theme.text, bg);
    if ((hover_target == target and pressed_target != target) or focus == target) focusRect(ctx, x + 4, y + 4, w - 8, 18);
    if (is_cancel) ctx.paintRect(x + 8, y + 21, @intCast(@max(@as(i32, 0), w - 16)), 1, theme.taskbar_dark);
}

fn messageBoxButtons(ctx: *const desk_api.Context, x: i32, y: i32, buttons: message_box.Buttons, focus: model.UiTarget, hover_target: model.UiTarget, pressed_target: model.UiTarget) void {
    const by = y + message_box_button_y;
    switch (buttons) {
        .ok => dialogButton(ctx, x + message_box_ok_x, by, message_box_ok_w, "OK", .message_ok, focus, hover_target, pressed_target, true, true),
        .ok_cancel => {
            dialogButton(ctx, x + message_box_pair_a_x, by, message_box_pair_w, "OK", .message_ok, focus, hover_target, pressed_target, true, false);
            dialogButton(ctx, x + message_box_pair_b_x, by, message_box_pair_w, "Cancel", .message_no, focus, hover_target, pressed_target, false, true);
        },
        .yes_no => {
            dialogButton(ctx, x + message_box_pair_a_x, by, message_box_pair_w, "Yes", .message_yes, focus, hover_target, pressed_target, false, false);
            dialogButton(ctx, x + message_box_pair_b_x, by, message_box_pair_w, "No", .message_no, focus, hover_target, pressed_target, true, true);
        },
    }
}

fn messageBoxIcon(ctx: *const desk_api.Context, x: i32, y: i32, kind: message_box.Kind) void {
    switch (kind) {
        .info => {
            iconCircle(ctx, x, y, 0x000080);
            ctx.paintRect(x + 15, y + 7, 3, 3, theme.title_text);
            ctx.paintRect(x + 15, y + 13, 3, 11, theme.title_text);
            ctx.paintRect(x + 12, y + 24, 9, 3, theme.title_text);
        },
        .warning => {
            iconTriangle(ctx, x, y, 0xFFFF00);
            ctx.paintRect(x + 15, y + 11, 3, 11, theme.text);
            ctx.paintRect(x + 15, y + 25, 3, 3, theme.text);
        },
        .@"error" => {
            iconCircle(ctx, x, y, 0xA00000);
            iconX(ctx, x, y, theme.title_text);
        },
        .question => {
            iconCircle(ctx, x, y, 0x000080);
            iconQuestion(ctx, x, y, theme.title_text);
        },
    }
}

const IconRow = struct {
    offset: i32,
    width: i32,
};

fn iconCircle(ctx: *const desk_api.Context, x: i32, y: i32, color: u32) void {
    const rows = [_]IconRow{
        .{ .offset = 11, .width = 10 },
        .{ .offset = 7, .width = 18 },
        .{ .offset = 5, .width = 22 },
        .{ .offset = 3, .width = 26 },
        .{ .offset = 2, .width = 28 },
        .{ .offset = 1, .width = 30 },
        .{ .offset = 0, .width = 32 },
        .{ .offset = 0, .width = 32 },
        .{ .offset = 0, .width = 32 },
        .{ .offset = 0, .width = 32 },
        .{ .offset = 1, .width = 30 },
        .{ .offset = 2, .width = 28 },
        .{ .offset = 3, .width = 26 },
        .{ .offset = 5, .width = 22 },
        .{ .offset = 7, .width = 18 },
        .{ .offset = 11, .width = 10 },
    };
    for (rows, 0..) |row, i| {
        ctx.paintRect(x + row.offset, y + @as(i32, @intCast(i)) * 2, @intCast(row.width), 2, color);
    }
    ctx.paintRect(x + 8, y + 4, 8, 2, 0x70B8FF);
}

fn iconTriangle(ctx: *const desk_api.Context, x: i32, y: i32, color: u32) void {
    var row: i32 = 0;
    while (row < 16) : (row += 1) {
        const width = row * 2 + 2;
        const start = 16 - @divTrunc(width, 2);
        ctx.paintRect(x + start - 1, y + row * 2, @intCast(width + 2), 2, theme.text);
        if (width > 2) ctx.paintRect(x + start, y + row * 2, @intCast(width), 2, color);
    }
}

fn iconX(ctx: *const desk_api.Context, x: i32, y: i32, color: u32) void {
    var i: i32 = 0;
    while (i < 8) : (i += 1) {
        ctx.paintRect(x + 8 + i * 2, y + 8 + i * 2, 4, 4, color);
        ctx.paintRect(x + 20 - i * 2, y + 8 + i * 2, 4, 4, color);
    }
}

fn iconQuestion(ctx: *const desk_api.Context, x: i32, y: i32, color: u32) void {
    ctx.paintRect(x + 11, y + 7, 10, 3, color);
    ctx.paintRect(x + 19, y + 10, 4, 6, color);
    ctx.paintRect(x + 16, y + 16, 5, 3, color);
    ctx.paintRect(x + 14, y + 19, 4, 5, color);
    ctx.paintRect(x + 14, y + 26, 4, 3, color);
}

fn clippedZ(ctx: *const desk_api.Context, x: i32, y: i32, width_px: i32, value: [*:0]const u8, fg: u32, bg: u32) void {
    const clipped = clippedText(std.mem.span(value), width_px);
    if (clipped[0] != 0) ctx.paintText(x, y, @ptrCast(&clipped), fg, bg);
}

fn wrappedZ(ctx: *const desk_api.Context, x: i32, y: i32, width_px: i32, value: [*:0]const u8, max_lines: usize, fg: u32, bg: u32) void {
    const text = std.mem.span(value);
    const max_chars: usize = @intCast(@min(@as(i32, 63), @max(@as(i32, 1), @divTrunc(width_px, glyph_w))));
    var pos: usize = 0;
    var line_index: usize = 0;
    while (line_index < max_lines and pos < text.len) : (line_index += 1) {
        while (pos < text.len and text[pos] == ' ') : (pos += 1) {}
        if (pos >= text.len) break;

        var end = pos;
        var count: usize = 0;
        var last_space: ?usize = null;
        while (end < text.len and text[end] != 0 and text[end] != '\n' and text[end] != '\r' and count < max_chars) {
            if (text[end] == ' ') last_space = end;
            const next = end + utf8SequenceLengthAt(text, end);
            if (next - pos > 63) break;
            end = next;
            count += 1;
        }

        var line_end = end;
        if (end < text.len and text[end] != '\n' and text[end] != '\r' and count >= max_chars) {
            if (last_space) |space| {
                if (space > pos) line_end = space;
            }
        }

        var line: [64]u8 = .{0} ** 64;
        const line_len = @min(line_end - pos, line.len - 1);
        if (line_len != 0) @memcpy(line[0..line_len], text[pos .. pos + line_len]);
        line[line_len] = 0;
        ctx.paintText(x, y + @as(i32, @intCast(line_index)) * 16, @ptrCast(&line), fg, bg);

        pos = line_end;
        while (pos < text.len and text[pos] == ' ') : (pos += 1) {}
        while (pos < text.len and (text[pos] == '\n' or text[pos] == '\r')) : (pos += 1) {}
    }
}

fn panel(ctx: *const desk_api.Context, x: i32, y: i32, w: i32, h: i32, comptime title: []const u8) void {
    bevel(ctx, x, y, w, h, true);
    ctx.paintRect(x + 8, y, 88, 10, theme.window_bg);
    textLit(ctx, x + 12, y - 1, title, theme.text, theme.window_bg);
}

fn taskRowBg(ctx: *const desk_api.Context, x: i32, y: i32, active: bool) void {
    ctx.paintRect(x, y, 444, 16, if (active) theme.select_bg else theme.client_bg);
}

pub fn systemMenu(ctx: *const desk_api.Context, x: i32, y: i32, win: *const window.Window, index: usize, hover_target: model.UiTarget, pressed_target: model.UiTarget) void {
    ctx.paintRect(x, y, system_menu_w, system_menu_h, theme.window_bg);
    bevel(ctx, x, y, system_menu_w, system_menu_h, false);
    systemMenuRow(ctx, x, y + 4, "Restore", windowTarget(index), hover_target, pressed_target);
    systemMenuRow(ctx, x, y + 24, "Minimize", minTarget(win, index), hover_target, pressed_target);
    if (win.maximized) {
        systemMenuRow(ctx, x, y + 44, "Restore Size", maxTarget(win, index), hover_target, pressed_target);
    } else {
        systemMenuRow(ctx, x, y + 44, "Maximize", maxTarget(win, index), hover_target, pressed_target);
    }
    systemMenuRow(ctx, x, y + 64, "Info", infoTarget(index), hover_target, pressed_target);
    ctx.paintRect(x + 4, y + 85, @intCast(system_menu_w - 8), 1, theme.taskbar_dark);
    systemMenuRow(ctx, x, y + 86, "Close", closeTarget(win, index), hover_target, pressed_target);
}

fn systemMenuRow(ctx: *const desk_api.Context, x: i32, y: i32, comptime label: []const u8, target: model.UiTarget, hover_target: model.UiTarget, pressed_target: model.UiTarget) void {
    const hot = hover_target == target or pressed_target == target;
    const bg = if (hot) theme.select_bg else theme.window_bg;
    const fg = if (hot) theme.title_text else theme.text;
    ctx.paintRect(x + 3, y, @intCast(system_menu_w - 6), @intCast(system_menu_row_h), bg);
    textLit(ctx, x + 12, y + 6, label, fg, bg);
}

pub fn cursor(ctx: *const desk_api.Context, x: i32, y: i32, screen_w: i32, screen_h: i32) void {
    const item = surface.cursor(x, y, screen_w, screen_h);
    if (item.rect.isEmpty()) return;

    var row: usize = 0;
    while (row < @as(usize, @intCast(surface.cursor_h))) : (row += 1) {
        cursorRuns(ctx, x, y + @as(i32, @intCast(row)), cursorBlackBits(row), 0x000000);
        cursorRuns(ctx, x, y + @as(i32, @intCast(row)), cursorWhiteBits(row), 0xFFFFFF);
    }
}

fn cursorRuns(ctx: *const desk_api.Context, x: i32, y: i32, bits: u16, color: u32) void {
    var col: usize = 0;
    while (col < @as(usize, @intCast(surface.cursor_w))) {
        if (!cursorBitSet(bits, col)) {
            col += 1;
            continue;
        }
        const start = col;
        while (col < @as(usize, @intCast(surface.cursor_w)) and cursorBitSet(bits, col)) : (col += 1) {}
        ctx.paintRect(x + @as(i32, @intCast(start)), y, @intCast(col - start), 1, color);
    }
}

fn cursorBitSet(bits: u16, col: usize) bool {
    const shift: u4 = @intCast(11 - col);
    return (bits & (@as(u16, 1) << shift)) != 0;
}

fn cursorBlackBits(row: usize) u16 {
    return switch (row) {
        0 => 0b100000000000,
        1 => 0b110000000000,
        2 => 0b101000000000,
        3 => 0b100100000000,
        4 => 0b100010000000,
        5 => 0b100001000000,
        6 => 0b100000100000,
        7 => 0b100000010000,
        8 => 0b100000001000,
        9 => 0b100000000100,
        10 => 0b100001111100,
        11 => 0b100101000000,
        12 => 0b101001000000,
        13 => 0b110010100000,
        14 => 0b100010100000,
        15 => 0b000010010000,
        16 => 0b000010010000,
        17 => 0b000001100000,
        else => 0,
    };
}

fn cursorWhiteBits(row: usize) u16 {
    return switch (row) {
        2 => 0b010000000000,
        3 => 0b011000000000,
        4 => 0b011100000000,
        5 => 0b011110000000,
        6 => 0b011111000000,
        7 => 0b011111100000,
        8 => 0b011111110000,
        9 => 0b011111111000,
        10 => 0b011110000000,
        11 => 0b011010000000,
        12 => 0b010010000000,
        13 => 0b000001000000,
        14 => 0b000001000000,
        15 => 0b000001100000,
        16 => 0b000001100000,
        else => 0,
    };
}

fn windowButton(ctx: *const desk_api.Context, x: i32, y: i32, glyph: ButtonGlyph, pressed: bool) void {
    ctx.paintRect(x + 1, y + 1, @intCast(theme.button - 2), @intCast(theme.button - 2), if (pressed) theme.taskbar_pressed else theme.taskbar);
    bevel(ctx, x, y, theme.button, theme.button, pressed);
    const off: i32 = if (pressed) 1 else 0;
    switch (glyph) {
        .close => drawCloseButtonGlyph(ctx, x + off, y + off),
        .maximize => {
            ctx.paintRect(x + 4 + off, y + 4 + off, 9, 2, theme.text);
            ctx.paintRect(x + 4 + off, y + 6 + off, 1, 7, theme.text);
            ctx.paintRect(x + 12 + off, y + 6 + off, 1, 7, theme.text);
            ctx.paintRect(x + 4 + off, y + 12 + off, 9, 1, theme.text);
        },
        .minimize => {
            ctx.paintRect(x + 4 + off, y + 11 + off, 9, 2, theme.text);
        },
    }
}

fn drawCloseButtonGlyph(ctx: *const desk_api.Context, x: i32, y: i32) void {
    const rows = [_]u7{
        0b1000001,
        0b0100010,
        0b0010100,
        0b0001000,
        0b0010100,
        0b0100010,
        0b1000001,
    };
    for (rows, 0..) |row, index| {
        const glyph_y = y + 5 + @as(i32, @intCast(index));
        for (0..7) |column| {
            const shift: u3 = @intCast(6 - column);
            if ((row & (@as(u7, 1) << shift)) != 0) {
                ctx.paintRect(x + 5 + @as(i32, @intCast(column)), glyph_y, 1, 1, theme.text);
            }
        }
    }
}

fn drawWindowButtonHover(ctx: *const desk_api.Context, win: *const window.Window, index: usize, hover_target: model.UiTarget, pressed_target: model.UiTarget) void {
    if (hover_target == closeTarget(win, index) and pressed_target != closeTarget(win, index)) {
        focusRect(ctx, win.x + win.w - 20, win.y + 7, theme.button - 4, theme.button - 4);
    } else if (hover_target == maxTarget(win, index) and pressed_target != maxTarget(win, index)) {
        focusRect(ctx, win.x + win.w - 38, win.y + 7, theme.button - 4, theme.button - 4);
    } else if (hover_target == minTarget(win, index) and pressed_target != minTarget(win, index)) {
        focusRect(ctx, win.x + win.w - 56, win.y + 7, theme.button - 4, theme.button - 4);
    }
}

fn taskbarTarget(win: window.Window, index: usize) model.UiTarget {
    _ = win;
    if (index == 0) return .terminal_taskbar;
    if (index == 3) return .app3_taskbar;
    return if (index == 2) .app2_taskbar else .wm_taskbar;
}

fn windowTarget(index: usize) model.UiTarget {
    if (index == 0) return .terminal_window;
    if (index == 3) return .app3_window;
    return if (index == 2) .app2_window else .wm_window;
}

fn closeTarget(win: *const window.Window, index: usize) model.UiTarget {
    _ = win;
    if (index == 0) return .terminal_close;
    if (index == 3) return .app3_close;
    return if (index == 2) .app2_close else .wm_close;
}

fn infoTarget(index: usize) model.UiTarget {
    if (index == 0) return .terminal_info;
    if (index == 3) return .app3_info;
    return if (index == 2) .app2_info else .wm_info;
}

fn minTarget(win: *const window.Window, index: usize) model.UiTarget {
    _ = win;
    if (index == 0) return .terminal_min;
    if (index == 3) return .app3_min;
    return if (index == 2) .app2_min else .wm_min;
}

fn maxTarget(win: *const window.Window, index: usize) model.UiTarget {
    if (index == 0) return if (win.maximized) .terminal_max_full else .terminal_max_normal;
    if (index == 3) return if (win.maximized) .app3_max_full else .app3_max_normal;
    if (index == 2) return if (win.maximized) .app2_max_full else .app2_max_normal;
    return if (win.maximized) .wm_max_full else .wm_max_normal;
}

fn hostedFrameCommands(ctx: *const desk_api.Context, bounds: surface.Rect, frame: gui_frame_snapshot.View) void {
    // A cursor-only present replays the desktop into a small dirty rectangle.
    // Reject whole frame commands before they allocate or walk their payload;
    // the SceneBuffer still clips the remaining partial command precisely.
    const paint_bounds = ctx.scenePaintBounds() orelse bounds;
    for (frame.commands) |command| {
        if (command.version != r4os.abi.gui_frame_command_version or command.size != r4os.abi.gui_frame_command_size) continue;
        if (frameCommandHasBoundedOutput(command.kind)) {
            const item = frameCommandPaintRect(bounds, command) orelse continue;
            if (!rectsIntersect(item, paint_bounds)) continue;
        }
        switch (command.kind) {
            r4os.abi.gui_frame_command_kind_clear => {
                if (command.resource_bytes != 0 or command.parameter0 != 0 or command.parameter1 != 0) continue;
                fillRect(ctx, bounds, command.rgb);
            },
            r4os.abi.gui_frame_command_kind_rect => {
                if (command.resource_bytes != 0 or command.parameter0 != 0 or command.parameter1 != 0) continue;
                const item = frameCommandRect(bounds, command) orelse continue;
                fillRect(ctx, item, command.rgb);
            },
            r4os.abi.gui_frame_command_kind_text => {
                if (command.parameter0 != 0 or command.parameter1 != 0) continue;
                const text = frameResource(frame.resources, command) orelse continue;
                const x = std.math.add(i32, bounds.x, command.x) catch continue;
                const y = std.math.add(i32, bounds.y, command.y) catch continue;
                const raw_text_h = if (command.text_h != 0) command.text_h else command.line_height;
                const text_h: i32 = @intCast(@min(raw_text_h, @as(u32, @intCast(std.math.maxInt(i32)))));
                const text_bottom = std.math.add(i32, y, @max(1, text_h)) catch continue;
                if (x >= bounds.right() or y >= bounds.bottom() or text_bottom <= bounds.y) continue;
                const text_clip = rectIntersection(bounds, paint_bounds) orelse continue;
                ctx.paintTextFontSlice(command.font_id, x, y, text, command.fg, command.bg, text_clip);
            },
            r4os.abi.gui_frame_command_kind_raster => hostedFrameRaster(ctx, bounds, command, frame.resources),
            r4os.abi.gui_frame_command_kind_indexed8 => hostedFrameIndexed8(ctx, bounds, command, frame.resources),
            r4os.abi.gui_frame_command_kind_alpha8 => hostedFrameAlpha8(ctx, bounds, command, frame.resources),
            r4os.abi.gui_frame_command_kind_argb32 => hostedFrameArgb32(ctx, bounds, command, frame.resources),
            r4os.abi.gui_frame_command_kind_path_fill,
            r4os.abi.gui_frame_command_kind_path_stroke,
            r4os.abi.gui_frame_command_kind_rounded_rect,
            r4os.abi.gui_frame_command_kind_shadow,
            => hostedFrameShape(ctx, bounds, command, frame.resources),
            else => {},
        }
    }
}

fn frameCommandHasBoundedOutput(kind: u32) bool {
    return switch (kind) {
        r4os.abi.gui_frame_command_kind_rect,
        r4os.abi.gui_frame_command_kind_raster,
        r4os.abi.gui_frame_command_kind_indexed8,
        r4os.abi.gui_frame_command_kind_alpha8,
        r4os.abi.gui_frame_command_kind_argb32,
        r4os.abi.gui_frame_command_kind_path_fill,
        r4os.abi.gui_frame_command_kind_path_stroke,
        r4os.abi.gui_frame_command_kind_rounded_rect,
        r4os.abi.gui_frame_command_kind_shadow,
        => true,
        else => false,
    };
}

fn frameCommandPaintRect(bounds: surface.Rect, command: r4os.abi.GuiFrameCommand) ?surface.Rect {
    if (command.w == 0 or command.h == 0) return null;
    const scale: u64 = switch (command.kind) {
        r4os.abi.gui_frame_command_kind_raster,
        r4os.abi.gui_frame_command_kind_argb32,
        => command.parameter0,
        else => 1,
    };
    if (scale == 0 or scale > 16) return null;
    const width = std.math.mul(i64, @as(i64, command.w), @as(i64, @intCast(scale))) catch return null;
    const height = std.math.mul(i64, @as(i64, command.h), @as(i64, @intCast(scale))) catch return null;
    const x0 = @as(i64, bounds.x) + @as(i64, command.x);
    const y0 = @as(i64, bounds.y) + @as(i64, command.y);
    const x1 = std.math.add(i64, x0, width) catch return null;
    const y1 = std.math.add(i64, y0, height) catch return null;
    const left = @max(@as(i64, bounds.x), x0);
    const top = @max(@as(i64, bounds.y), y0);
    const right = @min(@as(i64, bounds.right()), x1);
    const bottom = @min(@as(i64, bounds.bottom()), y1);
    if (right <= left or bottom <= top) return null;
    return .{ .x = @intCast(left), .y = @intCast(top), .w = @intCast(right - left), .h = @intCast(bottom - top) };
}

fn rectsIntersect(a: surface.Rect, b: surface.Rect) bool {
    return a.x < b.right() and b.x < a.right() and a.y < b.bottom() and b.y < a.bottom();
}

fn rectIntersection(a: surface.Rect, b: surface.Rect) ?surface.Rect {
    const left = @max(@as(i64, a.x), @as(i64, b.x));
    const top = @max(@as(i64, a.y), @as(i64, b.y));
    const right = @min(@as(i64, a.x) + @as(i64, a.w), @as(i64, b.x) + @as(i64, b.w));
    const bottom = @min(@as(i64, a.y) + @as(i64, a.h), @as(i64, b.y) + @as(i64, b.h));
    if (right <= left or bottom <= top) return null;
    return .{ .x = @intCast(left), .y = @intCast(top), .w = @intCast(right - left), .h = @intCast(bottom - top) };
}

fn hostedFrameShape(ctx: *const desk_api.Context, bounds: surface.Rect, command: r4os.abi.GuiFrameCommand, resources: []const u8) void {
    if (frameCommandRect(bounds, command) == null) return;
    const scene = ctx.scene orelse return;
    const resource = frameResource(resources, command) orelse return;
    _ = gui_shape_renderer.replay(ctx.allocator(), scene, bounds, command, resource);
}

fn frameCommandRect(bounds: surface.Rect, command: r4os.abi.GuiFrameCommand) ?surface.Rect {
    if (command.w == 0 or command.h == 0) return null;
    const x0 = @as(i64, bounds.x) + @as(i64, command.x);
    const y0 = @as(i64, bounds.y) + @as(i64, command.y);
    const x1 = x0 + @as(i64, command.w);
    const y1 = y0 + @as(i64, command.h);
    const left = @max(@as(i64, bounds.x), x0);
    const top = @max(@as(i64, bounds.y), y0);
    const right = @min(@as(i64, bounds.right()), x1);
    const bottom = @min(@as(i64, bounds.bottom()), y1);
    if (right <= left or bottom <= top) return null;
    return .{ .x = @intCast(left), .y = @intCast(top), .w = @intCast(right - left), .h = @intCast(bottom - top) };
}

fn frameResource(resources: []const u8, command: r4os.abi.GuiFrameCommand) ?[]const u8 {
    const offset = std.math.cast(usize, command.resource_offset) orelse return null;
    const length = std.math.cast(usize, command.resource_bytes) orelse return null;
    const end = std.math.add(usize, offset, length) catch return null;
    if (end > resources.len) return null;
    return resources[offset..end];
}

fn hostedFrameRaster(ctx: *const desk_api.Context, bounds: surface.Rect, command: r4os.abi.GuiFrameCommand, resources: []const u8) void {
    if (!validFrameRasterGeometry(command)) return;
    const pixels = std.math.mul(u64, command.w, command.h) catch return;
    const needed = std.math.mul(u64, pixels, @sizeOf(u32)) catch return;
    if (needed != command.resource_bytes) return;
    const source = frameResource(resources, command) orelse return;
    if (command.parameter1 != 0) return;
    const scale_value = command.parameter0;
    if (scale_value == 0 or scale_value > 16) return;
    const scale: i64 = @intCast(scale_value);
    const width: usize = @intCast(command.w);
    const height: usize = @intCast(command.h);
    var row: usize = 0;
    while (row < height) : (row += 1) {
        const row_offset = row * width * @sizeOf(u32);
        var run_start: usize = 0;
        while (run_start < width) {
            const color = readXrgb32(source, row_offset + run_start * @sizeOf(u32)) orelse return;
            var run_end = run_start + 1;
            while (run_end < width) : (run_end += 1) {
                const next = readXrgb32(source, row_offset + run_end * @sizeOf(u32)) orelse return;
                if (next != color) break;
            }
            fillScaledFrameRun(ctx, bounds, command, row, run_start, run_end, scale, color);
            run_start = run_end;
        }
    }
}

fn hostedFrameIndexed8(ctx: *const desk_api.Context, bounds: surface.Rect, command: r4os.abi.GuiFrameCommand, resources: []const u8) void {
    if (!validFrameIndexed8Geometry(command)) return;
    const source = frameResource(resources, command) orelse return;
    if (source.len < r4os.abi.gui_indexed8_pixels_offset or source.len != command.resource_bytes) return;
    var header: r4os.abi.GuiIndexed8Resource = .{};
    @memcpy(std.mem.asBytes(&header), source[0..@sizeOf(r4os.abi.GuiIndexed8Resource)]);
    if (header.version != r4os.abi.gui_indexed8_resource_version or header.size != r4os.abi.gui_indexed8_resource_size or
        header.source_w == 0 or header.source_h == 0 or header.source_w > r4os.abi.gui_raster_max_width or header.source_h > r4os.abi.gui_raster_max_height or
        header.guest_w == 0 or header.guest_h == 0 or header.viewport_w == 0 or header.viewport_h == 0 or
        header.palette_entries != r4os.abi.gui_indexed8_palette_entries or header.palette_offset != r4os.abi.gui_indexed8_palette_offset or
        header.pixels_offset != r4os.abi.gui_indexed8_pixels_offset or header.pixel_stride != header.source_w)
    {
        return;
    }
    const pixel_bytes = std.math.mul(u64, header.source_w, header.source_h) catch return;
    const required = std.math.add(u64, header.pixels_offset, pixel_bytes) catch return;
    if (required != source.len or @as(u64, header.source_x) + header.source_w > header.guest_w or @as(u64, header.source_y) + header.source_h > header.guest_h) return;

    const screen_x = std.math.add(i32, bounds.x, command.x) catch return;
    const screen_y = std.math.add(i32, bounds.y, command.y) catch return;
    const item = surface.Rect{ .x = screen_x, .y = screen_y, .w = @intCast(command.w), .h = @intCast(command.h) };
    const clipped_item = rectIntersection(item, bounds) orelse return;
    const viewport_x = std.math.add(i32, bounds.x, header.viewport_x) catch return;
    const viewport_y = std.math.add(i32, bounds.y, header.viewport_y) catch return;
    var palette: [r4os.abi.gui_indexed8_palette_entries]u32 = undefined;
    for (&palette, 0..) |*color, index| {
        const offset = @as(usize, header.palette_offset) + index * @sizeOf(u32);
        color.* = readXrgb32(source, offset) orelse return;
    }
    const pixels_offset: usize = @intCast(header.pixels_offset);
    const scene = ctx.scene orelse return;
    _ = scene.blitIndexed8Nearest(clipped_item, .{
        .indices = source[pixels_offset..],
        .palette = palette[0..],
        .source_x = header.source_x,
        .source_y = header.source_y,
        .source_w = header.source_w,
        .source_h = header.source_h,
        .source_stride = header.pixel_stride,
        .guest_w = header.guest_w,
        .guest_h = header.guest_h,
        .viewport = .{ .x = viewport_x, .y = viewport_y, .w = @intCast(header.viewport_w), .h = @intCast(header.viewport_h) },
    });
}

fn readXrgb32(source: []const u8, offset: usize) ?u32 {
    const end = std.math.add(usize, offset, @sizeOf(u32)) catch return null;
    if (end > source.len) return null;
    return (@as(u32, source[offset])) |
        (@as(u32, source[offset + 1]) << 8) |
        (@as(u32, source[offset + 2]) << 16) |
        (@as(u32, source[offset + 3]) << 24);
}

fn fillScaledFrameRun(ctx: *const desk_api.Context, bounds: surface.Rect, command: r4os.abi.GuiFrameCommand, row: usize, run_start: usize, run_end: usize, scale: i64, color: u32) void {
    const x0 = @as(i64, bounds.x) + @as(i64, command.x) + @as(i64, @intCast(run_start)) * scale;
    const y0 = @as(i64, bounds.y) + @as(i64, command.y) + @as(i64, @intCast(row)) * scale;
    const x1 = x0 + @as(i64, @intCast(run_end - run_start)) * scale;
    const y1 = y0 + scale;
    const left = @max(@as(i64, bounds.x), x0);
    const top = @max(@as(i64, bounds.y), y0);
    const right = @min(@as(i64, bounds.right()), x1);
    const bottom = @min(@as(i64, bounds.bottom()), y1);
    if (right <= left or bottom <= top) return;
    fillRect(ctx, .{ .x = @intCast(left), .y = @intCast(top), .w = @intCast(right - left), .h = @intCast(bottom - top) }, color & 0x00FF_FFFF);
}

fn hostedFrameAlpha8(ctx: *const desk_api.Context, bounds: surface.Rect, command: r4os.abi.GuiFrameCommand, resources: []const u8) void {
    if (!validFrameAlpha8Geometry(command)) return;
    const pixel_count_u64 = std.math.mul(u64, command.w, command.h) catch return;
    if (pixel_count_u64 != command.resource_bytes or command.parameter0 != 0 or command.parameter1 != 0) return;
    const pixel_count = std.math.cast(usize, pixel_count_u64) orelse return;
    const source = frameResource(resources, command) orelse return;
    if (source.len < pixel_count) return;
    const width: usize = @intCast(command.w);
    var row: usize = 0;
    while (row < @as(usize, @intCast(command.h))) : (row += 1) {
        const dest_x = @as(i64, bounds.x) + @as(i64, command.x);
        const dest_y = @as(i64, bounds.y) + @as(i64, command.y) + @as(i64, @intCast(row));
        if (dest_x < std.math.minInt(i32) or dest_x > std.math.maxInt(i32) or dest_y < std.math.minInt(i32) or dest_y > std.math.maxInt(i32)) continue;
        const offset = row * width;
        hostedAlpha8Row(ctx, bounds, @intCast(dest_x), @intCast(dest_y), command.rgb, source[offset .. offset + width]);
    }
}

fn hostedFrameArgb32(ctx: *const desk_api.Context, bounds: surface.Rect, command: r4os.abi.GuiFrameCommand, resources: []const u8) void {
    if (!validFrameArgb32Geometry(command) or command.parameter0 < 1 or command.parameter0 > 16 or command.parameter1 != 0) return;
    const pixel_count = std.math.mul(u64, command.w, command.h) catch return;
    const required = std.math.mul(u64, pixel_count, @sizeOf(u32)) catch return;
    if (required != command.resource_bytes) return;
    const source = frameResource(resources, command) orelse return;
    const destination_x = @as(i64, bounds.x) + @as(i64, command.x);
    const destination_y = @as(i64, bounds.y) + @as(i64, command.y);
    if (destination_x < std.math.minInt(i32) or destination_x > std.math.maxInt(i32) or
        destination_y < std.math.minInt(i32) or destination_y > std.math.maxInt(i32)) return;
    const scene = ctx.scene orelse return;
    _ = scene.blendArgb32(bounds, @intCast(destination_x), @intCast(destination_y), command.w, command.h, @intCast(command.parameter0), source);
}

fn validFrameRasterGeometry(command: r4os.abi.GuiFrameCommand) bool {
    if (command.w == 0 or command.h == 0 or command.w > r4os.abi.gui_raster_max_width or command.h > r4os.abi.gui_raster_max_height) return false;
    const pixels = std.math.mul(u64, command.w, command.h) catch return false;
    return pixels <= r4os.abi.gui_raster_max_pixels;
}

fn validFrameIndexed8Geometry(command: r4os.abi.GuiFrameCommand) bool {
    if (command.w == 0 or command.h == 0 or command.w > r4os.abi.gui_raster_max_width or command.h > r4os.abi.gui_raster_max_height or
        command.rgb != 0 or command.fg != 0 or command.bg != 0 or command.font_id != 0 or command.text_w != 0 or command.text_h != 0 or
        command.baseline != 0 or command.line_height != 0 or command.parameter0 != 0 or command.parameter1 != 0)
    {
        return false;
    }
    return true;
}

fn validFrameAlpha8Geometry(command: r4os.abi.GuiFrameCommand) bool {
    if (command.w == 0 or command.h == 0 or command.w > r4os.abi.gui_alpha8_max_width or command.h > r4os.abi.gui_alpha8_max_height) return false;
    const pixels = std.math.mul(u64, command.w, command.h) catch return false;
    return pixels <= r4os.abi.gui_alpha8_max_pixels;
}

fn validFrameArgb32Geometry(command: r4os.abi.GuiFrameCommand) bool {
    if (command.w == 0 or command.h == 0 or command.w > r4os.abi.gui_argb32_max_width or command.h > r4os.abi.gui_argb32_max_height) return false;
    const pixels = std.math.mul(u64, command.w, command.h) catch return false;
    return pixels <= r4os.abi.gui_argb32_max_pixels;
}

fn hostedAlpha8Row(ctx: *const desk_api.Context, bounds: surface.Rect, dest_x: i32, dest_y: i32, rgb: u32, alpha: []const u8) void {
    if (alpha.len == 0 or dest_y < bounds.y or dest_y >= bounds.bottom()) return;
    const source_right = @as(i64, dest_x) + @as(i64, @intCast(alpha.len));
    const left = @max(@as(i64, bounds.x), @as(i64, dest_x));
    const right = @min(@as(i64, bounds.right()), source_right);
    if (right <= left) return;
    const source_x: usize = @intCast(left - @as(i64, dest_x));
    const visible_width: usize = @intCast(right - left);
    _ = ctx.paintAlpha8(
        @intCast(left),
        dest_y,
        @intCast(visible_width),
        1,
        @intCast(visible_width),
        rgb,
        alpha[source_x .. source_x + visible_width],
    );
}

fn clippedText(value: []const u8, width_px: i32) [64]u8 {
    var out: [64]u8 = .{0} ** 64;
    if (width_px < 8) return out;
    const max_chars: usize = @intCast(@min(@as(i32, 63), @divTrunc(width_px, 8)));
    var source: usize = 0;
    var output: usize = 0;
    var char_count: usize = 0;
    while (source < value.len and value[source] != 0 and char_count < max_chars) : (char_count += 1) {
        const sequence_len = utf8SequenceLengthAt(value, source);
        if (output + sequence_len >= out.len) break;
        @memcpy(out[output .. output + sequence_len], value[source .. source + sequence_len]);
        source += sequence_len;
        output += sequence_len;
    }
    return out;
}

fn hostedText(ctx: *const desk_api.Context, rect: surface.Rect, text_buf: []const u8) void {
    const x = rect.x + 10;
    var y = rect.y + 46;
    const bottom = rect.y + rect.h - 8;
    const max_chars: usize = @intCast(@max(8, @divTrunc(rect.w - 20, 8)));
    var line: [96]u8 = .{0} ** 96;
    var line_len: usize = 0;
    var line_chars: usize = 0;
    var i: usize = 0;
    while (i < text_buf.len and text_buf[i] != 0 and y + 12 <= bottom) {
        const ch = text_buf[i];
        if (ch == '\r') {
            i += 1;
            continue;
        }
        const sequence_len = utf8SequenceLengthAt(text_buf, i);
        if (ch == '\n' or line_chars >= max_chars or line_len + sequence_len >= line.len) {
            line[line_len] = 0;
            ctx.paintText(x, y, @ptrCast(&line), theme.text, theme.client_bg);
            @memset(line[0..], 0);
            line_len = 0;
            line_chars = 0;
            y += 16;
            if (ch == '\n') {
                i += 1;
                continue;
            }
            continue;
        }
        @memcpy(line[line_len .. line_len + sequence_len], text_buf[i .. i + sequence_len]);
        line_len += sequence_len;
        line_chars += 1;
        i += sequence_len;
    }
    if (line_len > 0 and y + 12 <= bottom) {
        line[line_len] = 0;
        ctx.paintText(x, y, @ptrCast(&line), theme.text, theme.client_bg);
    }
}

pub fn fullscreenConsole(ctx: *const desk_api.Context, screen_w: i32, screen_h: i32, snapshot: ?*const TerminalSnapshot, font_size: u8, cursor_blink_on: bool) void {
    const rect: surface.Rect = .{ .x = 0, .y = 0, .w = screen_w, .h = screen_h };
    const state = if (snapshot) |cached| if (cached.valid) cached.state else r4os.abi.ConsoleState{} else r4os.abi.ConsoleState{};
    ctx.paintRect(0, 0, @intCast(@max(0, screen_w)), @intCast(@max(0, screen_h)), state.bg);
    if (snapshot) |cached| if (cached.valid and cached.has_output) {
        renderTerminalSnapshot(ctx, rect, cached);
        if (cached.effective_offset == 0) terminalCursor(ctx, rect, state, font_size, state.fg, cursor_blink_on);
    };
}

pub fn refreshTerminalSnapshot(
    ctx: *const desk_api.Context,
    snapshot: *TerminalSnapshot,
    instance_id: u32,
    revision: u32,
    rect: surface.Rect,
    font_size: u8,
    codepage: u16,
    scroll_offset: u32,
) TerminalRefreshStats {
    var stats = TerminalRefreshStats{};
    if (instance_id == 0 or rect.isEmpty()) {
        snapshot.* = .{};
        return stats;
    }

    const metrics = terminalMetrics(rect, font_size);
    _ = ctx.consoleSetMetrics(instance_id, metrics.cols, metrics.rows);
    var state = r4os.abi.ConsoleState{};
    stats.state_reads = 1;
    if (ctx.consoleState(instance_id, &state) < 0) state = .{ .fg = theme.text, .bg = theme.client_bg };

    var raw: [console_output_render_max]u8 = undefined;
    const output_len = ctx.consoleOutput(instance_id, raw[0..]);
    const raw_len: usize = if (output_len > 0)
        @intCast(@min(@as(i32, @intCast(raw.len)), output_len))
    else
        0;
    stats.output_bytes = @intCast(raw_len);
    const refresh = refreshTerminalSnapshotBytes(snapshot, instance_id, revision, rect, font_size, codepage, scroll_offset, state, raw[0..raw_len]);
    stats.parsed_bytes = refresh.parsed_bytes;
    stats.parse_skipped_bytes = refresh.parse_skipped_bytes;
    stats.visible_lines = refresh.visible_lines;
    stats.changed_lines = refresh.changed_lines;
    stats.skipped_lines = refresh.skipped_lines;
    stats.full_rebuild = refresh.full_rebuild;
    stats.incremental = refresh.incremental;
    stats.changed_rows = refresh.changed_rows;
    return stats;
}

pub fn refreshTerminalSnapshotBytes(
    snapshot: *TerminalSnapshot,
    instance_id: u32,
    revision: u32,
    rect: surface.Rect,
    font_size: u8,
    codepage: u16,
    scroll_offset: u32,
    state: r4os.abi.ConsoleState,
    raw_input: []const u8,
) TerminalRefreshStats {
    var stats = TerminalRefreshStats{};
    const input = raw_input[0..@min(raw_input.len, snapshot.raw.len)];
    var old_hashes: [terminal_snapshot_line_max]u64 = .{0} ** terminal_snapshot_line_max;
    var old_lengths: [terminal_snapshot_line_max]u16 = .{0} ** terminal_snapshot_line_max;
    const old_line_count = @min(snapshot.line_count, snapshot.lines.len);
    var old_index: usize = 0;
    while (old_index < old_line_count) : (old_index += 1) {
        old_lengths[old_index] = snapshot.lines[old_index].len;
        old_hashes[old_index] = terminalSnapshotLineHash(snapshot, old_index);
    }

    if (instance_id == 0 or rect.isEmpty()) {
        snapshot.* = .{};
        stats.full_rebuild = true;
        markTerminalLineChanges(&stats, old_hashes[0..], old_lengths[0..], old_line_count, snapshot);
        return stats;
    }

    const metrics = terminalMetrics(rect, font_size);
    const max_chars: usize = @intCast(@min(metrics.cols, @as(u32, terminal_line_max - 1)));
    const visible_limit: usize = @intCast(@min(@max(@as(u32, 1), metrics.rows), @as(u32, terminal_snapshot_line_max)));
    const same_configuration = snapshot.valid and snapshot.instance_id == instance_id and
        snapshot.width == rect.w and snapshot.height == rect.h and snapshot.font_size == font_size and
        snapshot.codepage == codepage and snapshot.scroll_offset == scroll_offset;
    const same_colors = snapshot.state.fg == state.fg and snapshot.state.bg == state.bg;
    const prefix_preserved = input.len >= snapshot.raw_len and
        std.mem.eql(u8, snapshot.raw[0..snapshot.raw_len], input[0..snapshot.raw_len]);
    const append_only = same_configuration and same_colors and scroll_offset == 0 and
        snapshot.state.clear_count == state.clear_count and
        snapshot.state.output_dropped_bytes == state.output_dropped_bytes and prefix_preserved;

    if (append_only and appendTerminalSnapshotSuffix(snapshot, input[snapshot.raw_len..], max_chars, visible_limit, codepage)) {
        stats.incremental = true;
        stats.parsed_bytes = @intCast(input.len - snapshot.raw_len);
        stats.parse_skipped_bytes = @intCast(snapshot.raw_len);
        snapshot.valid = true;
        snapshot.has_output = input.len != 0;
        snapshot.instance_id = instance_id;
        snapshot.revision = revision;
        snapshot.width = rect.w;
        snapshot.height = rect.h;
        snapshot.font_size = font_size;
        snapshot.codepage = codepage;
        snapshot.scroll_offset = scroll_offset;
        snapshot.effective_offset = 0;
        snapshot.state = state;
        if (input.len != 0) @memcpy(snapshot.raw[0..input.len], input);
        snapshot.raw_len = input.len;
    } else {
        snapshot.* = .{
            .valid = true,
            .has_output = input.len != 0,
            .instance_id = instance_id,
            .revision = revision,
            .width = rect.w,
            .height = rect.h,
            .font_size = font_size,
            .codepage = codepage,
            .scroll_offset = scroll_offset,
            .state = state,
        };
        stats.full_rebuild = true;
        stats.parsed_bytes = @intCast(input.len);
        if (input.len != 0) {
            @memcpy(snapshot.raw[0..input.len], input);
            snapshot.raw_len = input.len;
            const range = console_scroll.visibleRange(input, metrics.cols, @intCast(visible_limit), scroll_offset, codepage);
            snapshot.effective_offset = range.effective_offset;
            compactVisibleTerminalLines(snapshot, input, max_chars, range, codepage);
        }
    }

    stats.visible_lines = @intCast(snapshot.line_count);
    markTerminalLineChanges(&stats, old_hashes[0..], old_lengths[0..], old_line_count, snapshot);
    return stats;
}

fn compactVisibleTerminalLines(snapshot: *TerminalSnapshot, raw: []const u8, max_chars: usize, range: console_scroll.Range, codepage: u16) void {
    snapshot.line_count = 0;
    snapshot.text_len = 0;
    snapshot.tail_line_len = 0;
    var line: [terminal_line_max]u8 = .{0} ** terminal_line_max;
    var line_len: usize = 0;
    var line_index: u32 = 0;
    var write_offset: usize = 0;
    var index: usize = 0;
    while (index < raw.len and raw[index] != 0) : (index += 1) {
        const ch = raw[index];
        if (ch == '\r') continue;
        if (ch == '\n' or line_len >= max_chars or line_len + 1 >= line.len) {
            storeTerminalSnapshotLine(snapshot, line[0..line_len], line_index, range, &write_offset);
            line_index += 1;
            line_len = 0;
            if (ch == '\n') continue;
        }
        if (console_scroll.printable(ch, codepage)) {
            line[line_len] = ch;
            line_len += 1;
        }
    }
    if (line_len > 0 or line_index == 0) storeTerminalSnapshotLine(snapshot, line[0..line_len], line_index, range, &write_offset);
    snapshot.text_len = write_offset;
    snapshot.tail_line_len = @intCast(line_len);
}

fn appendTerminalSnapshotSuffix(snapshot: *TerminalSnapshot, suffix: []const u8, max_chars: usize, visible_limit: usize, codepage: u16) bool {
    var line_len: usize = snapshot.tail_line_len;
    var index: usize = 0;
    while (index < suffix.len and suffix[index] != 0) : (index += 1) {
        const ch = suffix[index];
        if (ch == '\r') continue;
        if (ch == '\n' or line_len >= max_chars) {
            if (line_len == 0 and ch == '\n' and !appendTerminalVisibleLine(snapshot, visible_limit)) return false;
            line_len = 0;
            if (ch == '\n') continue;
        }
        if (!console_scroll.printable(ch, codepage)) continue;
        if (line_len == 0 and !appendTerminalVisibleLine(snapshot, visible_limit)) return false;
        if (snapshot.line_count == 0 or snapshot.text_len >= snapshot.text.len) return false;
        const line = &snapshot.lines[snapshot.line_count - 1];
        if (@as(usize, line.offset) + @as(usize, line.len) != snapshot.text_len or line.len == std.math.maxInt(u16)) return false;
        snapshot.text[snapshot.text_len] = ch;
        snapshot.text_len += 1;
        line.len += 1;
        line_len += 1;
    }
    snapshot.tail_line_len = @intCast(line_len);
    return true;
}

fn appendTerminalVisibleLine(snapshot: *TerminalSnapshot, visible_limit_raw: usize) bool {
    const visible_limit = @max(@as(usize, 1), @min(visible_limit_raw, snapshot.lines.len));
    if (snapshot.line_count >= visible_limit) dropFirstTerminalVisibleLine(snapshot);
    if (snapshot.line_count >= snapshot.lines.len or snapshot.text_len > std.math.maxInt(u16)) return false;
    snapshot.lines[snapshot.line_count] = .{ .offset = @intCast(snapshot.text_len), .len = 0 };
    snapshot.line_count += 1;
    return true;
}

fn dropFirstTerminalVisibleLine(snapshot: *TerminalSnapshot) void {
    if (snapshot.line_count == 0) return;
    const removed: usize = snapshot.lines[0].len;
    if (removed != 0 and removed <= snapshot.text_len) {
        std.mem.copyForwards(u8, snapshot.text[0 .. snapshot.text_len - removed], snapshot.text[removed..snapshot.text_len]);
        snapshot.text_len -= removed;
    }
    var index: usize = 1;
    while (index < snapshot.line_count) : (index += 1) {
        snapshot.lines[index - 1] = snapshot.lines[index];
        snapshot.lines[index - 1].offset -|= @intCast(removed);
    }
    snapshot.line_count -= 1;
    snapshot.lines[snapshot.line_count] = .{};
}

fn terminalSnapshotLineHash(snapshot: *const TerminalSnapshot, index: usize) u64 {
    if (index >= snapshot.line_count) return 0;
    const line = snapshot.lines[index];
    const start: usize = line.offset;
    const len: usize = line.len;
    if (start > snapshot.text_len or len > snapshot.text_len - start) return 0;
    var hash: u64 = 0xcbf29ce484222325;
    for (snapshot.text[start .. start + len]) |byte| {
        hash ^= byte;
        hash *%= 0x100000001b3;
    }
    return hash;
}

fn markTerminalLineChanges(stats: *TerminalRefreshStats, old_hashes: []const u64, old_lengths: []const u16, old_line_count: usize, snapshot: *const TerminalSnapshot) void {
    const rows = @min(@max(old_line_count, snapshot.line_count), terminal_snapshot_line_max);
    var row: usize = 0;
    while (row < rows) : (row += 1) {
        const unchanged = row < old_line_count and row < snapshot.line_count and
            old_lengths[row] == snapshot.lines[row].len and old_hashes[row] == terminalSnapshotLineHash(snapshot, row);
        if (unchanged) {
            stats.skipped_lines += 1;
            continue;
        }
        stats.changed_rows[row / 64] |= @as(u64, 1) << @intCast(row % 64);
        stats.changed_lines += 1;
    }
}

fn storeTerminalSnapshotLine(snapshot: *TerminalSnapshot, line: []const u8, line_index: u32, range: console_scroll.Range, write_offset: *usize) void {
    if (line_index < range.start_line or line_index >= range.end_line or snapshot.line_count >= snapshot.lines.len) return;
    const available = snapshot.text.len - write_offset.*;
    const len = @min(line.len, @min(available, std.math.maxInt(u16)));
    if (len != 0) @memcpy(snapshot.text[write_offset.* .. write_offset.* + len], line[0..len]);
    snapshot.lines[snapshot.line_count] = .{ .offset = @intCast(write_offset.*), .len = @intCast(len) };
    snapshot.line_count += 1;
    write_offset.* += len;
}

fn renderTerminalSnapshot(ctx: *const desk_api.Context, rect: surface.Rect, snapshot: *const TerminalSnapshot) void {
    const metrics = terminalMetrics(rect, snapshot.font_size);
    const paint_bounds = ctx.scenePaintBounds() orelse rect;
    var line_buffer: [terminal_line_max]u8 = .{0} ** terminal_line_max;
    var index: usize = 0;
    while (index < snapshot.line_count) : (index += 1) {
        const item = snapshot.lines[index];
        const start: usize = item.offset;
        const len: usize = item.len;
        if (start + len > snapshot.text_len or len >= line_buffer.len) return;
        const line_rect = surface.Rect{
            .x = metrics.x,
            .y = metrics.y + @as(i32, @intCast(index)) * metrics.line_h,
            .w = @as(i32, @intCast(len)) * glyph_w,
            .h = metrics.line_h,
        };
        if (!line_rect.intersects(paint_bounds)) continue;
        @memset(line_buffer[0..], 0);
        if (len != 0) @memcpy(line_buffer[0..len], snapshot.text[start .. start + len]);
        ctx.paintText(line_rect.x, line_rect.y, @ptrCast(&line_buffer), snapshot.state.fg, snapshot.state.bg);
    }
}

fn terminalText(ctx: *const desk_api.Context, rect: surface.Rect, text_buf: []const u8, font_size: u8, codepage: u16, scroll_offset: u32, fg: u32, bg: u32) u32 {
    const metrics = terminalMetrics(rect, font_size);
    const max_chars: usize = @intCast(@min(metrics.cols, @as(u32, terminal_line_max - 1)));
    const range = console_scroll.visibleRange(text_buf, metrics.cols, metrics.rows, scroll_offset, codepage);
    var line: [terminal_line_max]u8 = .{0} ** terminal_line_max;
    var line_len: usize = 0;
    var line_index: u32 = 0;
    var y = metrics.y;
    var i: usize = 0;
    while (i < text_buf.len and text_buf[i] != 0) : (i += 1) {
        const ch = text_buf[i];
        if (ch == '\r') continue;
        if (ch == '\n' or line_len >= max_chars or line_len + 1 >= line.len) {
            if (line_index >= range.start_line and line_index < range.end_line) {
                drawTerminalLine(ctx, metrics.x, y, &line, line_len, fg, bg);
                y += metrics.line_h;
            }
            line_index += 1;
            clearTerminalLine(line[0..], &line_len);
            if (ch == '\n') continue;
        }
        if (console_scroll.printable(ch, codepage)) {
            line[line_len] = ch;
            line_len += 1;
        }
    }
    if (line_len > 0 or line_index == 0) {
        if (line_index >= range.start_line and line_index < range.end_line) {
            drawTerminalLine(ctx, metrics.x, y, &line, line_len, fg, bg);
        }
    }
    return range.effective_offset;
}

fn syncConsoleMetrics(ctx: *const desk_api.Context, instance_id: u32, rect: surface.Rect, font_size: u8) void {
    const metrics = terminalMetrics(rect, font_size);
    _ = ctx.consoleSetMetrics(instance_id, metrics.cols, metrics.rows);
}

fn terminalMetrics(rect: surface.Rect, font_size: u8) TerminalMetrics {
    const line_h: i32 = if (font_size >= 16) 18 else 16;
    const usable_w = @max(@as(i32, glyph_w), rect.w - 20);
    const usable_h = @max(@as(i32, 1), rect.h - 14);
    return .{
        .x = rect.x + 10,
        .y = rect.y + 6,
        .line_h = line_h,
        .cols = @intCast(@max(@as(i32, 8), @divTrunc(usable_w, glyph_w))),
        .rows = @intCast(@max(@as(i32, 1), @divTrunc(usable_h, line_h))),
    };
}

pub fn terminalRowRect(rect: surface.Rect, font_size: u8, row: usize) surface.Rect {
    const metrics = terminalMetrics(rect, font_size);
    const y64 = @as(i64, metrics.y) + @as(i64, @intCast(row)) * metrics.line_h;
    if (y64 < rect.y or y64 >= rect.bottom()) return .{ .x = 0, .y = 0, .w = 0, .h = 0 };
    const y: i32 = @intCast(y64);
    return .{
        .x = metrics.x,
        .y = y,
        .w = @max(@as(i32, 0), rect.right() - metrics.x),
        .h = @min(metrics.line_h, @max(@as(i32, 0), rect.bottom() - y)),
    };
}

pub fn terminalCursorRect(rect: surface.Rect, state: r4os.abi.ConsoleState, font_size: u8) surface.Rect {
    const metrics = terminalMetrics(rect, font_size);
    const max_col: i32 = @intCast(if (metrics.cols > 0) metrics.cols - 1 else 0);
    const max_row: i32 = @intCast(if (metrics.rows > 0) metrics.rows - 1 else 0);
    const col = @min(max_col, @max(@as(i32, 0), state.cursor_x));
    const row = @min(max_row, @max(@as(i32, 0), state.cursor_y));
    const x = metrics.x + col * glyph_w;
    const y = metrics.y + row * metrics.line_h + @max(@as(i32, 0), metrics.line_h - 9);
    const w = @min(glyph_w, @max(@as(i32, 0), rect.right() - x));
    const h = @min(@as(i32, 2), @max(@as(i32, 0), rect.bottom() - y));
    return .{ .x = x, .y = y, .w = w, .h = h };
}

fn terminalCursor(ctx: *const desk_api.Context, rect: surface.Rect, state: r4os.abi.ConsoleState, font_size: u8, fg: u32, cursor_blink_on: bool) void {
    if (state.cursor_visible == 0 or !cursor_blink_on) return;
    const cursor_rect = terminalCursorRect(rect, state, font_size);
    if (cursor_rect.isEmpty() or cursor_rect.x < rect.x or cursor_rect.y < rect.y or cursor_rect.x >= rect.right() or cursor_rect.y >= rect.bottom()) return;
    ctx.paintRect(cursor_rect.x, cursor_rect.y, @intCast(cursor_rect.w), @intCast(cursor_rect.h), fg);
}

fn drawTerminalLine(ctx: *const desk_api.Context, x: i32, y: i32, line: *[terminal_line_max]u8, line_len: usize, fg: u32, bg: u32) void {
    line[@min(line_len, terminal_line_max - 1)] = 0;
    if (line_len > 0) ctx.paintText(x, y, @ptrCast(line), fg, bg);
}

fn clearTerminalLine(line: []u8, len: *usize) void {
    @memset(line, 0);
    len.* = 0;
}

fn clipRect(rect: surface.Rect, bounds: surface.Rect) surface.Rect {
    const left = @max(rect.x, bounds.x);
    const top = @max(rect.y, bounds.y);
    const right = @min(rect.right(), bounds.right());
    const bottom = @min(rect.bottom(), bounds.bottom());
    return .{
        .x = left,
        .y = top,
        .w = @max(0, right - left),
        .h = @max(0, bottom - top),
    };
}

fn focusRect(ctx: *const desk_api.Context, x: i32, y: i32, w: i32, h: i32) void {
    ctx.paintRect(x, y, @intCast(w), 1, theme.text);
    ctx.paintRect(x, y + h - 1, @intCast(w), 1, theme.text);
    ctx.paintRect(x, y, 1, @intCast(h), theme.text);
    ctx.paintRect(x + w - 1, y, 1, @intCast(h), theme.text);
}

fn fillSurface(ctx: *const desk_api.Context, item: surface.Surface, rgb: u32) void {
    fillRect(ctx, item.rect, rgb);
}

fn textLit(ctx: *const desk_api.Context, x: i32, y: i32, comptime value: []const u8, fg: u32, bg: u32) void {
    var buffer: [value.len + 1]u8 = undefined;
    inline for (value, 0..) |ch, i| {
        buffer[i] = ch;
    }
    buffer[value.len] = 0;
    ctx.paintText(x, y, @ptrCast(&buffer), fg, bg);
}

fn copyTailZ(out: []u8, value: [*:0]const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    var len: usize = 0;
    while (len < 255 and value[len] != 0) : (len += 1) {}
    const capacity = out.len - 1;
    if (len <= capacity) {
        var i: usize = 0;
        while (i < len) : (i += 1) out[i] = value[i];
        return;
    }
    if (capacity < 3) return;
    out[0] = '.';
    out[1] = '.';
    const tail_len = capacity - 2;
    const start = len - tail_len;
    var i: usize = 0;
    while (i < tail_len) : (i += 1) out[i + 2] = value[start + i];
}

fn writeI32Z(out: []u8, value: i32) void {
    @memset(out, 0);
    if (out.len == 0) return;
    var pos: usize = 0;
    var n: u32 = undefined;
    if (value < 0) {
        out[pos] = '-';
        pos += 1;
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
    while (count > 0 and pos + 1 < out.len) {
        count -= 1;
        out[pos] = digits[count];
        pos += 1;
    }
    out[pos] = 0;
}

fn writeU8Z(out: []u8, value: u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    var digits: [3]u8 = undefined;
    var n = value;
    var count: usize = 0;
    if (n == 0) {
        digits[count] = '0';
        count += 1;
    } else {
        while (n > 0 and count < digits.len) : (count += 1) {
            digits[count] = '0' + (n % 10);
            n /= 10;
        }
    }
    var pos: usize = 0;
    while (count > 0 and pos + 1 < out.len) {
        count -= 1;
        out[pos] = digits[count];
        pos += 1;
    }
    out[pos] = 0;
}

fn writeU32Z(out: []u8, value: u32) void {
    @memset(out, 0);
    if (out.len == 0) return;
    var digits: [10]u8 = undefined;
    var n = value;
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
    var pos: usize = 0;
    while (count > 0 and pos + 1 < out.len) {
        count -= 1;
        out[pos] = digits[count];
        pos += 1;
    }
    out[pos] = 0;
}

fn writeHex6Z(out: []u8, value: u32) void {
    @memset(out, 0);
    if (out.len == 0) return;
    var pos: usize = 0;
    var shift: u5 = 20;
    while (pos + 1 < out.len) {
        const nibble: u8 = @intCast((value >> shift) & 0xF);
        out[pos] = if (nibble < 10) '0' + nibble else 'A' + (nibble - 10);
        pos += 1;
        if (shift == 0) break;
        shift -= 4;
    }
    out[pos] = 0;
}

fn roleText(ctx: *const desk_api.Context, x: i32, y: i32, value: u8, bg: u32) void {
    switch (value) {
        @intFromEnum(r4os.abi.ProgramInstanceRole.foreground) => textLit(ctx, x, y, "fg", theme.text, bg),
        @intFromEnum(r4os.abi.ProgramInstanceRole.shell) => textLit(ctx, x, y, "shell", theme.text, bg),
        @intFromEnum(r4os.abi.ProgramInstanceRole.background) => textLit(ctx, x, y, "bg", theme.text, bg),
        else => textLit(ctx, x, y, "?", theme.text, bg),
    }
}

fn classText(ctx: *const desk_api.Context, x: i32, y: i32, value: u8, bg: u32) void {
    switch (value) {
        @intFromEnum(r4os.abi.ProgramInstanceClass.console) => textLit(ctx, x, y, "console", theme.text, bg),
        @intFromEnum(r4os.abi.ProgramInstanceClass.gui) => textLit(ctx, x, y, "gui", theme.text, bg),
        @intFromEnum(r4os.abi.ProgramInstanceClass.service) => textLit(ctx, x, y, "service", theme.text, bg),
        else => textLit(ctx, x, y, "?", theme.text, bg),
    }
}

fn stateText(ctx: *const desk_api.Context, x: i32, y: i32, value: u8, bg: u32) void {
    switch (value) {
        @intFromEnum(r4os.abi.ProgramInstanceState.running) => textLit(ctx, x, y, "running", theme.text, bg),
        @intFromEnum(r4os.abi.ProgramInstanceState.close_requested) => textLit(ctx, x, y, "closing", theme.text, bg),
        @intFromEnum(r4os.abi.ProgramInstanceState.done) => textLit(ctx, x, y, "done", theme.text, bg),
        else => textLit(ctx, x, y, "?", theme.text, bg),
    }
}

fn damageKindText(ctx: *const desk_api.Context, x: i32, y: i32, kind: DamageKind) void {
    switch (kind) {
        .none => textLit(ctx, x, y, "none", theme.text, theme.window_bg),
        .cursor => textLit(ctx, x, y, "cursor", theme.text, theme.window_bg),
        .mixed => textLit(ctx, x, y, "mixed", theme.text, theme.window_bg),
        .full => textLit(ctx, x, y, "full", theme.text, theme.window_bg),
    }
}

fn zStringLen(value: [*:0]const u8) usize {
    var len: usize = 0;
    while (value[len] != 0) : (len += 1) {}
    return len;
}

fn utf8SequenceLengthAt(value: []const u8, start: usize) usize {
    if (start >= value.len) return 0;
    const first = value[start];
    const expected: usize = if (first < 0x80)
        1
    else if (first >= 0xC2 and first <= 0xDF)
        2
    else if (first >= 0xE0 and first <= 0xEF)
        3
    else if (first >= 0xF0 and first <= 0xF4)
        4
    else
        return 1;
    if (start + expected > value.len) return 1;
    var offset: usize = 1;
    while (offset < expected) : (offset += 1) if ((value[start + offset] & 0xC0) != 0x80) return 1;
    if (first == 0xE0 and value[start + 1] < 0xA0) return 1;
    if (first == 0xED and value[start + 1] >= 0xA0) return 1;
    if (first == 0xF0 and value[start + 1] < 0x90) return 1;
    if (first == 0xF4 and value[start + 1] >= 0x90) return 1;
    return expected;
}

test "hosted command text clipping keeps visible prefix" {
    const clipped = clippedText("ABCDEFGHIJ", 32);
    try std.testing.expectEqualStrings("ABCD", std.mem.span(@as([*:0]const u8, @ptrCast(&clipped))));

    const hidden = clippedText("A", 4);
    try std.testing.expectEqualStrings("", std.mem.span(@as([*:0]const u8, @ptrCast(&hidden))));

    const utf8 = clippedText("A\xc3\xa4BC", 24);
    try std.testing.expectEqualStrings("A\xc3\xa4B", std.mem.span(@as([*:0]const u8, @ptrCast(&utf8))));
}

test "frame resource bounds reject overflow and allow unaligned XRGB" {
    const bytes = [_]u8{ 0xAA, 0x33, 0x22, 0x11, 0x00 };
    const command = r4os.abi.GuiFrameCommand{
        .kind = r4os.abi.gui_frame_command_kind_raster,
        .w = 1,
        .h = 1,
        .resource_offset = 1,
        .resource_bytes = 4,
    };
    const resource = frameResource(bytes[0..], command) orelse return error.ExpectedResource;
    try std.testing.expectEqual(@as(u32, 0x00112233), readXrgb32(resource, 0).?);

    var overflow = command;
    overflow.resource_offset = std.math.maxInt(u64);
    overflow.resource_bytes = 2;
    try std.testing.expect(frameResource(bytes[0..], overflow) == null);
}

test "frame raster commands retain per-command geometry limits" {
    try std.testing.expect(validFrameRasterGeometry(.{ .w = r4os.abi.gui_raster_max_width, .h = 1 }));
    try std.testing.expect(!validFrameRasterGeometry(.{ .w = r4os.abi.gui_raster_max_width + 1, .h = 1 }));
    try std.testing.expect(validFrameAlpha8Geometry(.{ .w = r4os.abi.gui_alpha8_max_width, .h = 1 }));
    try std.testing.expect(!validFrameAlpha8Geometry(.{ .w = r4os.abi.gui_alpha8_max_width + 1, .h = 1 }));
    try std.testing.expect(validFrameArgb32Geometry(.{ .w = r4os.abi.gui_argb32_max_width, .h = 1 }));
    try std.testing.expect(!validFrameArgb32Geometry(.{ .w = r4os.abi.gui_argb32_max_width + 1, .h = 1 }));
}

test "terminal snapshot compacts only visible parsed lines" {
    var snapshot = TerminalSnapshot{};
    const input = "L1\nL2\nL3\nL4\nL5";
    const range = console_scroll.visibleRange(input, 80, 3, 0, 437);
    compactVisibleTerminalLines(&snapshot, input, 80, range, 437);
    try std.testing.expectEqual(@as(usize, 3), snapshot.line_count);
    try std.testing.expectEqualStrings("L3", snapshot.text[snapshot.lines[0].offset .. snapshot.lines[0].offset + snapshot.lines[0].len]);
    try std.testing.expectEqualStrings("L4", snapshot.text[snapshot.lines[1].offset .. snapshot.lines[1].offset + snapshot.lines[1].len]);
    try std.testing.expectEqualStrings("L5", snapshot.text[snapshot.lines[2].offset .. snapshot.lines[2].offset + snapshot.lines[2].len]);
    snapshot.valid = true;
    snapshot.instance_id = 9;
    snapshot.revision = 7;
    snapshot.width = 640;
    snapshot.height = 400;
    snapshot.font_size = 14;
    snapshot.codepage = 437;
    try std.testing.expect(snapshot.matches(9, 7, .{ .x = 100, .y = 200, .w = 640, .h = 400 }, 14, 437, 0));
    try std.testing.expect(!snapshot.matches(9, 8, .{ .x = 100, .y = 200, .w = 640, .h = 400 }, 14, 437, 0));
}

test "terminal append parses only suffix and retains full-rebuild pixels" {
    const rect = surface.Rect{ .x = 20, .y = 30, .w = 660, .h = 62 };
    const state = r4os.abi.ConsoleState{ .fg = 0x00FF_FFFF, .bg = 0x0000_0000, .cursor_x = 3, .cursor_y = 2, .cursor_visible = 1 };
    var incremental = TerminalSnapshot{};
    const first = "L1\nL2\nL3";
    const initial = refreshTerminalSnapshotBytes(&incremental, 9, 1, rect, 14, 437, 0, state, first);
    try std.testing.expect(initial.full_rebuild);
    try std.testing.expectEqual(@as(u32, first.len), initial.parsed_bytes);

    const appended = "L1\nL2\nL3X";
    const update = refreshTerminalSnapshotBytes(&incremental, 9, 2, rect, 14, 437, 0, state, appended);
    try std.testing.expect(update.incremental);
    try std.testing.expectEqual(@as(u32, 1), update.parsed_bytes);
    try std.testing.expectEqual(@as(u32, first.len), update.parse_skipped_bytes);
    try std.testing.expectEqual(@as(u32, 1), update.changed_lines);
    try std.testing.expectEqual(@as(u32, 2), update.skipped_lines);
    try std.testing.expect(!update.rowChanged(0));
    try std.testing.expect(!update.rowChanged(1));
    try std.testing.expect(update.rowChanged(2));

    var rebuilt = TerminalSnapshot{};
    const reference = refreshTerminalSnapshotBytes(&rebuilt, 9, 2, rect, 14, 437, 0, state, appended);
    try std.testing.expect(reference.full_rebuild);
    try std.testing.expectEqual(rebuilt.line_count, incremental.line_count);
    var line_index: usize = 0;
    while (line_index < rebuilt.line_count) : (line_index += 1) {
        const expected = rebuilt.lines[line_index];
        const actual = incremental.lines[line_index];
        try std.testing.expectEqualStrings(
            rebuilt.text[expected.offset .. expected.offset + expected.len],
            incremental.text[actual.offset .. actual.offset + actual.len],
        );
    }
    try std.testing.expectEqualSlices(u8, rebuilt.raw[0..rebuilt.raw_len], incremental.raw[0..incremental.raw_len]);
}

test "terminal incremental refresh falls back on ring scroll geometry font codepage and color changes" {
    const rect = surface.Rect{ .x = 0, .y = 0, .w = 640, .h = 80 };
    const state = r4os.abi.ConsoleState{ .fg = 7, .bg = 1, .clear_count = 3 };
    var snapshot = TerminalSnapshot{};
    _ = refreshTerminalSnapshotBytes(&snapshot, 4, 1, rect, 14, 437, 0, state, "ABC");

    var overflowed = state;
    overflowed.output_dropped_bytes = 1;
    var changed_prefix = refreshTerminalSnapshotBytes(&snapshot, 4, 2, rect, 14, 437, 0, overflowed, "ABC");
    try std.testing.expect(changed_prefix.full_rebuild);

    changed_prefix = refreshTerminalSnapshotBytes(&snapshot, 4, 3, rect, 14, 437, 0, overflowed, "XABC");
    try std.testing.expect(changed_prefix.full_rebuild);
    try std.testing.expectEqual(@as(u32, 4), changed_prefix.parsed_bytes);

    changed_prefix = refreshTerminalSnapshotBytes(&snapshot, 4, 4, .{ .x = 0, .y = 0, .w = 632, .h = 80 }, 14, 437, 0, overflowed, "XABC");
    try std.testing.expect(changed_prefix.full_rebuild);
    changed_prefix = refreshTerminalSnapshotBytes(&snapshot, 4, 5, .{ .x = 0, .y = 0, .w = 632, .h = 80 }, 16, 437, 0, overflowed, "XABC");
    try std.testing.expect(changed_prefix.full_rebuild);
    changed_prefix = refreshTerminalSnapshotBytes(&snapshot, 4, 6, .{ .x = 0, .y = 0, .w = 632, .h = 80 }, 16, 850, 0, overflowed, "XABC");
    try std.testing.expect(changed_prefix.full_rebuild);
    changed_prefix = refreshTerminalSnapshotBytes(&snapshot, 4, 7, .{ .x = 0, .y = 0, .w = 632, .h = 80 }, 16, 850, 1, overflowed, "XABC");
    try std.testing.expect(changed_prefix.full_rebuild);

    var recolored = overflowed;
    recolored.bg = 2;
    changed_prefix = refreshTerminalSnapshotBytes(&snapshot, 4, 8, .{ .x = 0, .y = 0, .w = 632, .h = 80 }, 16, 850, 1, recolored, "XABC");
    try std.testing.expect(changed_prefix.full_rebuild);
    try std.testing.expect(!changed_prefix.incremental);
}

test "terminal row damage is bounded to one visible line" {
    const rect = surface.Rect{ .x = 100, .y = 50, .w = 320, .h = 80 };
    try std.testing.expectEqual(surface.Rect{ .x = 110, .y = 56, .w = 310, .h = 16 }, terminalRowRect(rect, 14, 0));
    try std.testing.expectEqual(surface.Rect{ .x = 110, .y = 104, .w = 310, .h = 16 }, terminalRowRect(rect, 14, 3));
    try std.testing.expect(terminalRowRect(rect, 14, 5).isEmpty());
}

test "empty committed frame preserves hosted text fallback" {
    try std.testing.expect(!useCommittedFrameCommands(.{ .supported = true, .valid = true }));
    try std.testing.expect(!useCommittedFrameCommands(.{ .supported = true, .valid = false }));
    const commands = [_]r4os.abi.GuiFrameCommand{.{ .kind = r4os.abi.gui_frame_command_kind_clear }};
    try std.testing.expect(useCommittedFrameCommands(.{ .supported = true, .valid = true, .commands = commands[0..] }));
}

fn fillRect(ctx: *const desk_api.Context, rect: surface.Rect, rgb: u32) void {
    if (rect.isEmpty()) return;
    ctx.paintRect(rect.x, rect.y, @intCast(rect.w), @intCast(rect.h), rgb);
}

fn bevelRect(ctx: *const desk_api.Context, rect: surface.Rect, pressed: bool) void {
    bevel(ctx, rect.x, rect.y, rect.w, rect.h, pressed);
}

pub fn bevel(ctx: *const desk_api.Context, x: i32, y: i32, w: i32, h: i32, pressed: bool) void {
    const hi = if (pressed) theme.taskbar_dark else theme.taskbar_light;
    const lo = if (pressed) theme.taskbar_light else theme.taskbar_dark;
    ctx.paintRect(x, y, @intCast(w), 1, hi);
    ctx.paintRect(x, y, 1, @intCast(h), hi);
    ctx.paintRect(x, y + h - 1, @intCast(w), 1, lo);
    ctx.paintRect(x + w - 1, y, 1, @intCast(h), lo);
}
