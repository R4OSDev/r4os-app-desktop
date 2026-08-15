const std = @import("std");
const surface = @import("surface.zig");
const theme = @import("theme.zig");

pub const WindowKind = enum(u8) {
    terminal,
    manager,
    app,
};

pub const ResizeHandle = enum(u8) {
    left,
    right,
    top,
    bottom,
    top_left,
    top_right,
    bottom_left,
    bottom_right,

    fn affectsLeft(self: ResizeHandle) bool {
        return self == .left or self == .top_left or self == .bottom_left;
    }

    fn affectsRight(self: ResizeHandle) bool {
        return self == .right or self == .top_right or self == .bottom_right;
    }

    fn affectsTop(self: ResizeHandle) bool {
        return self == .top or self == .top_left or self == .top_right;
    }

    fn affectsBottom(self: ResizeHandle) bool {
        return self == .bottom or self == .bottom_left or self == .bottom_right;
    }
};

pub const Geometry = surface.Rect;

const resize_margin: i32 = 6;
pub const default_min_w: i32 = 180;
pub const default_min_h: i32 = 96;
const title_max: usize = 40;

pub const Window = struct {
    kind: WindowKind,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    normal_x: i32,
    normal_y: i32,
    normal_w: i32,
    normal_h: i32,
    visible: bool = true,
    minimized: bool = false,
    maximized: bool = false,
    instance_id: u32 = 0,
    close_requested: bool = false,
    close_requested_tick: u64 = 0,
    gui_revision: u32 = 0,
    min_frame_w: i32 = default_min_w,
    min_frame_h: i32 = default_min_h,
    title_buf: [title_max + 1]u8 = .{0} ** (title_max + 1),

    pub fn title(self: *const Window) [*:0]const u8 {
        return @ptrCast(&self.title_buf);
    }

    pub fn setNormal(self: *Window, x: i32, y: i32, w: i32, h: i32) void {
        self.normal_x = x;
        self.normal_y = y;
        self.normal_w = w;
        self.normal_h = h;
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
        self.maximized = false;
    }

    pub fn restore(self: *Window) void {
        self.visible = true;
        self.minimized = false;
    }

    pub fn minimize(self: *Window) void {
        self.minimized = true;
    }

    pub fn close(self: *Window) void {
        self.visible = false;
        self.minimized = false;
        self.close_requested = false;
        self.close_requested_tick = 0;
    }

    pub fn bindInstance(self: *Window, instance_id: u32) void {
        self.instance_id = instance_id;
    }

    pub fn bindApp(self: *Window, instance_id: u32, title_text: [*:0]const u8) void {
        self.kind = .app;
        self.bindInstance(instance_id);
        self.setTitle(title_text);
        self.resetMinSize();
        self.visible = true;
        self.minimized = false;
        self.close_requested = false;
        self.close_requested_tick = 0;
    }

    pub fn bindConsole(self: *Window, instance_id: u32, title_text: [*:0]const u8) void {
        self.kind = .terminal;
        self.bindInstance(instance_id);
        self.setTitle(title_text);
        self.resetMinSize();
        self.visible = true;
        self.minimized = false;
        self.close_requested = false;
        self.close_requested_tick = 0;
    }

    pub fn unbindInstance(self: *Window) void {
        self.instance_id = 0;
        self.close_requested = false;
        self.close_requested_tick = 0;
        self.gui_revision = 0;
    }

    pub fn requestClose(self: *Window, tick: u64) void {
        self.close_requested = true;
        self.close_requested_tick = tick;
    }

    pub fn setTitle(self: *Window, title_text: [*:0]const u8) void {
        @memset(self.title_buf[0..], 0);
        var source: usize = 0;
        var output: usize = 0;
        while (output < title_max and title_text[source] != 0) {
            const sequence_len = utf8SequenceLengthZ(title_text, source);
            if (output + sequence_len > title_max) break;
            var offset: usize = 0;
            while (offset < sequence_len) : (offset += 1) self.title_buf[output + offset] = title_text[source + offset];
            source += sequence_len;
            output += sequence_len;
        }
    }

    pub fn setTitleLit(self: *Window, comptime title_text: []const u8) void {
        @memset(self.title_buf[0..], 0);
        const count = utf8PrefixBytes(title_text, title_max);
        @memcpy(self.title_buf[0..count], title_text[0..count]);
    }

    pub fn resetMinSize(self: *Window) void {
        self.min_frame_w = default_min_w;
        self.min_frame_h = default_min_h;
    }

    pub fn setMinClientSize(self: *Window, client_w: i32, client_h: i32, screen_w: i32, screen_h: i32) bool {
        const work_h = @max(default_min_h, screen_h - theme.taskbar_h);
        const next_w = clamp(@max(default_min_w, client_w + 16), default_min_w, @max(default_min_w, screen_w));
        const next_h = clamp(@max(default_min_h, client_h + theme.title_h + 18), default_min_h, work_h);
        if (next_w == self.min_frame_w and next_h == self.min_frame_h) return false;

        self.min_frame_w = next_w;
        self.min_frame_h = next_h;
        if (!self.maximized) {
            const next = (surface.Rect{
                .x = self.x,
                .y = self.y,
                .w = @max(self.w, self.min_frame_w),
                .h = @max(self.h, self.min_frame_h),
            }).clampInside(surface.workArea(screen_w, screen_h, theme.taskbar_h));
            self.setNormal(next.x, next.y, @max(next.w, self.min_frame_w), @max(next.h, self.min_frame_h));
        }
        return true;
    }

    pub fn toggleMaximize(self: *Window, screen_w: i32, screen_h: i32) void {
        const work_area = surface.workArea(screen_w, screen_h, theme.taskbar_h);
        if (self.maximized) {
            const restored = self.normalRect(screen_w, screen_h);
            self.x = restored.x;
            self.y = restored.y;
            self.w = restored.w;
            self.h = restored.h;
            self.maximized = false;
        } else {
            const normal = self.geometry().clampInside(work_area);
            self.normal_x = normal.x;
            self.normal_y = normal.y;
            self.normal_w = normal.w;
            self.normal_h = normal.h;
            self.x = work_area.x;
            self.y = work_area.y;
            self.w = work_area.w;
            self.h = work_area.h;
            self.maximized = true;
        }
        self.visible = true;
        self.minimized = false;
    }

    pub fn fitToWorkArea(self: *Window, screen_w: i32, screen_h: i32) void {
        const next = if (self.maximized)
            surface.workArea(screen_w, screen_h, theme.taskbar_h)
        else
            self.normalRect(screen_w, screen_h);

        self.x = next.x;
        self.y = next.y;
        self.w = next.w;
        self.h = next.h;
        if (!self.maximized) {
            self.normal_x = next.x;
            self.normal_y = next.y;
            self.normal_w = next.w;
            self.normal_h = next.h;
        }
    }

    pub fn moveTo(self: *Window, x: i32, y: i32, screen_w: i32, screen_h: i32) void {
        if (!self.visible or self.minimized or self.maximized) return;

        const work_area = surface.workArea(screen_w, screen_h, theme.taskbar_h);
        const moved = (surface.Rect{ .x = x, .y = y, .w = self.w, .h = self.h }).clampInside(work_area);
        self.x = moved.x;
        self.y = moved.y;
        self.normal_x = self.x;
        self.normal_y = self.y;
        self.normal_w = self.w;
        self.normal_h = self.h;
    }

    pub fn geometry(self: *const Window) Geometry {
        return .{ .x = self.x, .y = self.y, .w = self.w, .h = self.h };
    }

    pub fn frameSurface(self: *const Window) surface.Surface {
        return surface.make(.window_frame, self.geometry());
    }

    pub fn titleSurface(self: *const Window) surface.Surface {
        return surface.make(.window_title, .{
            .x = self.x + 3,
            .y = self.y + 3,
            .w = self.w - 6,
            .h = theme.title_h,
        });
    }

    pub fn titleDragRect(self: *const Window) surface.Rect {
        return .{
            .x = self.x + 3,
            .y = self.y + 3,
            .w = self.w - 65,
            .h = theme.title_h,
        };
    }

    pub fn clientSurface(self: *const Window) surface.Surface {
        return surface.make(.window_client, .{
            .x = self.x + 8,
            .y = self.y + theme.title_h + 10,
            .w = self.w - 16,
            .h = self.h - theme.title_h - 18,
        });
    }

    pub fn resizeFrom(self: *Window, start: Geometry, handle: ResizeHandle, mouse_dx: i32, mouse_dy: i32, screen_w: i32, screen_h: i32) void {
        if (!self.visible or self.minimized or self.maximized) return;

        const work_w = screen_w;
        const work_h = screen_h - theme.taskbar_h;
        var next = start;

        if (handle.affectsLeft()) {
            const right = start.x + start.w;
            const max_x = @max(0, right - self.min_frame_w);
            next.x = clamp(start.x + mouse_dx, 0, max_x);
            next.w = right - next.x;
        } else if (handle.affectsRight()) {
            const max_w = @max(self.min_frame_w, work_w - start.x);
            next.w = clamp(start.w + mouse_dx, self.min_frame_w, max_w);
        }

        if (handle.affectsTop()) {
            const bottom = start.y + start.h;
            const max_y = @max(0, bottom - self.min_frame_h);
            next.y = clamp(start.y + mouse_dy, 0, max_y);
            next.h = bottom - next.y;
        } else if (handle.affectsBottom()) {
            const max_h = @max(self.min_frame_h, work_h - start.y);
            next.h = clamp(start.h + mouse_dy, self.min_frame_h, max_h);
        }

        self.setNormal(next.x, next.y, next.w, next.h);
    }

    fn normalRect(self: *const Window, screen_w: i32, screen_h: i32) surface.Rect {
        const work_area = surface.workArea(screen_w, screen_h, theme.taskbar_h);
        const w = clamp(self.normal_w, self.min_frame_w, @max(self.min_frame_w, work_area.w));
        const h = clamp(self.normal_h, self.min_frame_h, @max(self.min_frame_h, work_area.h));
        return (surface.Rect{
            .x = self.normal_x,
            .y = self.normal_y,
            .w = w,
            .h = h,
        }).clampInside(work_area);
    }

    pub fn contains(self: *const Window, x: i32, y: i32) bool {
        return self.visible and !self.minimized and
            x >= self.x and x < self.x + self.w and y >= self.y and y < self.y + self.h;
    }

    pub fn resizeHit(self: *const Window, x: i32, y: i32) ?ResizeHandle {
        if (!self.contains(x, y) or self.maximized) return null;

        const near_left = x < self.x + resize_margin;
        const near_right = x >= self.x + self.w - resize_margin;
        const near_top = y < self.y + resize_margin;
        const near_bottom = y >= self.y + self.h - resize_margin;

        if (near_left and near_top) return .top_left;
        if (near_right and near_top) return .top_right;
        if (near_left and near_bottom) return .bottom_left;
        if (near_right and near_bottom) return .bottom_right;
        if (near_left) return .left;
        if (near_right) return .right;
        if (near_top) return .top;
        if (near_bottom) return .bottom;
        return null;
    }

    pub fn titleHit(self: *const Window, x: i32, y: i32) bool {
        return self.visible and !self.minimized and !self.maximized and
            self.titleDragRect().contains(x, y);
    }

    pub fn closeHit(self: *const Window, x: i32, y: i32) bool {
        return self.buttonHit(x, y, 1);
    }

    pub fn maxHit(self: *const Window, x: i32, y: i32) bool {
        return self.buttonHit(x, y, 2);
    }

    pub fn minHit(self: *const Window, x: i32, y: i32) bool {
        return self.buttonHit(x, y, 3);
    }

    fn buttonHit(self: *const Window, x: i32, y: i32, index_from_right: i32) bool {
        if (!self.visible or self.minimized) return false;
        const bx = self.x + self.w - 4 - index_from_right * (theme.button + 2);
        return x >= bx and x < bx + theme.button and y >= self.y + 2 and y < self.y + 2 + theme.button;
    }
};

fn utf8SequenceLengthZ(value: [*:0]const u8, start: usize) usize {
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
    var offset: usize = 1;
    while (offset < expected) : (offset += 1) {
        if (value[start + offset] == 0 or (value[start + offset] & 0xC0) != 0x80) return 1;
    }
    if (first == 0xE0 and value[start + 1] < 0xA0) return 1;
    if (first == 0xED and value[start + 1] >= 0xA0) return 1;
    if (first == 0xF0 and value[start + 1] < 0x90) return 1;
    if (first == 0xF4 and value[start + 1] >= 0x90) return 1;
    return expected;
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

fn utf8PrefixBytes(value: []const u8, max_bytes: usize) usize {
    const limit = @min(value.len, max_bytes);
    var index: usize = 0;
    while (index < limit) {
        const next = index + utf8SequenceLengthAt(value, index);
        if (next > limit) break;
        index = next;
    }
    return index;
}

/// Mirrors the compositor order: inactive windows are painted in slot order,
/// then the active window is painted last. Hit testing walks that order back
/// to front so an active window also acts as an input barrier over its full
/// frame.
pub fn topmostAt(windows: []const Window, active_window: usize, x: i32, y: i32) ?usize {
    if (active_window < windows.len and windows[active_window].contains(x, y)) {
        return active_window;
    }

    var i = windows.len;
    while (i > 0) {
        i -= 1;
        if (i == active_window) continue;
        if (windows[i].contains(x, y)) return i;
    }
    return null;
}

fn clamp(value: i32, min: i32, max: i32) i32 {
    if (value < min) return min;
    if (value > max) return max;
    return value;
}

test "window title hit ignores buttons and maximized windows" {
    var win = Window{
        .kind = .manager,
        .x = 100,
        .y = 80,
        .w = 300,
        .h = 160,
        .normal_x = 100,
        .normal_y = 80,
        .normal_w = 300,
        .normal_h = 160,
    };

    try std.testing.expect(win.titleHit(120, 90));
    try std.testing.expect(!win.titleHit(384, 90));
    win.maximized = true;
    try std.testing.expect(!win.titleHit(120, 90));
}

test "window move clamps to desktop work area" {
    var win = Window{
        .kind = .manager,
        .x = 100,
        .y = 80,
        .w = 300,
        .h = 160,
        .normal_x = 100,
        .normal_y = 80,
        .normal_w = 300,
        .normal_h = 160,
    };

    win.moveTo(-20, -10, 640, 480);
    try std.testing.expectEqual(@as(i32, 0), win.x);
    try std.testing.expectEqual(@as(i32, 0), win.y);

    win.moveTo(999, 999, 640, 480);
    try std.testing.expectEqual(@as(i32, 340), win.x);
    try std.testing.expectEqual(@as(i32, 288), win.y);
}

test "window resize hit finds edges and ignores maximized windows" {
    var win = Window{
        .kind = .manager,
        .x = 100,
        .y = 80,
        .w = 300,
        .h = 160,
        .normal_x = 100,
        .normal_y = 80,
        .normal_w = 300,
        .normal_h = 160,
    };

    try std.testing.expectEqual(ResizeHandle.top_left, win.resizeHit(102, 82).?);
    try std.testing.expectEqual(ResizeHandle.right, win.resizeHit(397, 120).?);
    try std.testing.expectEqual(ResizeHandle.bottom, win.resizeHit(180, 238).?);
    try std.testing.expectEqual(@as(?ResizeHandle, null), win.resizeHit(160, 120));
    win.maximized = true;
    try std.testing.expectEqual(@as(?ResizeHandle, null), win.resizeHit(102, 82));
}

test "window resize clamps to minimum size and work area" {
    var win = Window{
        .kind = .manager,
        .x = 100,
        .y = 80,
        .w = 300,
        .h = 160,
        .normal_x = 100,
        .normal_y = 80,
        .normal_w = 300,
        .normal_h = 160,
    };

    win.resizeFrom(win.geometry(), .right, -400, 0, 640, 480);
    try std.testing.expectEqual(@as(i32, 180), win.w);
    try std.testing.expectEqual(@as(i32, 100), win.x);

    win.setNormal(100, 80, 300, 160);
    win.resizeFrom(win.geometry(), .bottom_right, 999, 999, 640, 480);
    try std.testing.expectEqual(@as(i32, 540), win.w);
    try std.testing.expectEqual(@as(i32, 368), win.h);

    win.setNormal(100, 80, 300, 160);
    win.resizeFrom(win.geometry(), .top_left, 260, 120, 640, 480);
    try std.testing.expectEqual(@as(i32, 220), win.x);
    try std.testing.expectEqual(@as(i32, 144), win.y);
    try std.testing.expectEqual(@as(i32, 180), win.w);
    try std.testing.expectEqual(@as(i32, 96), win.h);
}

test "window maximize and restore stay inside work area" {
    var win = Window{
        .kind = .manager,
        .x = 520,
        .y = 400,
        .w = 300,
        .h = 160,
        .normal_x = 520,
        .normal_y = 400,
        .normal_w = 300,
        .normal_h = 160,
    };

    win.toggleMaximize(640, 480);
    try std.testing.expect(win.maximized);
    try std.testing.expectEqual(@as(i32, 0), win.x);
    try std.testing.expectEqual(@as(i32, 0), win.y);
    try std.testing.expectEqual(@as(i32, 640), win.w);
    try std.testing.expectEqual(@as(i32, 448), win.h);

    win.toggleMaximize(640, 480);
    try std.testing.expect(!win.maximized);
    try std.testing.expectEqual(@as(i32, 340), win.x);
    try std.testing.expectEqual(@as(i32, 288), win.y);
    try std.testing.expectEqual(@as(i32, 300), win.w);
    try std.testing.expectEqual(@as(i32, 160), win.h);
}

test "window fit normalizes startup geometry" {
    var win = Window{
        .kind = .manager,
        .x = -40,
        .y = 460,
        .w = 80,
        .h = 40,
        .normal_x = -40,
        .normal_y = 460,
        .normal_w = 80,
        .normal_h = 40,
    };

    win.fitToWorkArea(640, 480);
    try std.testing.expectEqual(@as(i32, 0), win.x);
    try std.testing.expectEqual(@as(i32, 352), win.y);
    try std.testing.expectEqual(@as(i32, 180), win.w);
    try std.testing.expectEqual(@as(i32, 96), win.h);
    try std.testing.expectEqual(win.x, win.normal_x);
    try std.testing.expectEqual(win.y, win.normal_y);
    try std.testing.expectEqual(win.w, win.normal_w);
    try std.testing.expectEqual(win.h, win.normal_h);
}

test "window app minimum client size raises resize floor" {
    var win = Window{
        .kind = .app,
        .x = 100,
        .y = 80,
        .w = 300,
        .h = 160,
        .normal_x = 100,
        .normal_y = 80,
        .normal_w = 300,
        .normal_h = 160,
    };

    try std.testing.expect(win.setMinClientSize(240, 170, 640, 480));
    try std.testing.expectEqual(@as(i32, 256), win.min_frame_w);
    try std.testing.expectEqual(@as(i32, 208), win.min_frame_h);

    win.resizeFrom(win.geometry(), .bottom_right, -400, -400, 640, 480);
    try std.testing.expectEqual(@as(i32, 256), win.w);
    try std.testing.expectEqual(@as(i32, 208), win.h);
}

test "active window blocks input to overlapping higher slot" {
    const windows = [_]Window{
        .{
            .kind = .manager,
            .x = 20,
            .y = 20,
            .w = 300,
            .h = 220,
            .normal_x = 20,
            .normal_y = 20,
            .normal_w = 300,
            .normal_h = 220,
        },
        .{
            .kind = .app,
            .x = 80,
            .y = 60,
            .w = 300,
            .h = 220,
            .normal_x = 80,
            .normal_y = 60,
            .normal_w = 300,
            .normal_h = 220,
        },
    };

    try std.testing.expectEqual(@as(?usize, 0), topmostAt(windows[0..], 0, 100, 100));
}

test "inactive windows keep compositor slot order for hit testing" {
    var windows = [_]Window{
        .{
            .kind = .manager,
            .x = 20,
            .y = 20,
            .w = 300,
            .h = 220,
            .normal_x = 20,
            .normal_y = 20,
            .normal_w = 300,
            .normal_h = 220,
        },
        .{
            .kind = .app,
            .x = 80,
            .y = 60,
            .w = 300,
            .h = 220,
            .normal_x = 80,
            .normal_y = 60,
            .normal_w = 300,
            .normal_h = 220,
        },
    };

    try std.testing.expectEqual(@as(?usize, 1), topmostAt(windows[0..], windows.len, 100, 100));
    windows[1].minimize();
    try std.testing.expectEqual(@as(?usize, 0), topmostAt(windows[0..], 1, 100, 100));
    try std.testing.expectEqual(@as(?usize, null), topmostAt(windows[0..], 0, 500, 400));
}

test "window keeps optional program instance binding" {
    var win = Window{
        .kind = .manager,
        .x = 100,
        .y = 80,
        .w = 300,
        .h = 160,
        .normal_x = 100,
        .normal_y = 80,
        .normal_w = 300,
        .normal_h = 160,
    };

    try std.testing.expectEqual(@as(u32, 0), win.instance_id);
    win.bindInstance(42);
    try std.testing.expectEqual(@as(u32, 42), win.instance_id);
    win.requestClose(1234);
    try std.testing.expect(win.close_requested);
    try std.testing.expectEqual(@as(u64, 1234), win.close_requested_tick);
    win.unbindInstance();
    try std.testing.expectEqual(@as(u32, 0), win.instance_id);
    try std.testing.expect(!win.close_requested);
    try std.testing.expectEqual(@as(u64, 0), win.close_requested_tick);
}

test "app window uses bound title" {
    var win = Window{
        .kind = .manager,
        .x = 100,
        .y = 80,
        .w = 300,
        .h = 160,
        .normal_x = 100,
        .normal_y = 80,
        .normal_w = 300,
        .normal_h = 160,
    };

    win.bindApp(7, "Notepad");
    try std.testing.expectEqual(WindowKind.app, win.kind);
    try std.testing.expectEqual(@as(u32, 7), win.instance_id);
    try std.testing.expectEqualStrings("Notepad", std.mem.span(win.title()));
}

test "window title uses runtime buffer" {
    var win = Window{
        .kind = .manager,
        .x = 100,
        .y = 80,
        .w = 300,
        .h = 160,
        .normal_x = 100,
        .normal_y = 80,
        .normal_w = 300,
        .normal_h = 160,
    };

    try std.testing.expectEqualStrings("", std.mem.span(win.title()));
    win.setTitleLit("Desktop Manager");
    try std.testing.expectEqualStrings("Desktop Manager", std.mem.span(win.title()));

    win.setTitle("123456789012345678901234567890123456789\xc3\xa4");
    try std.testing.expectEqualStrings("123456789012345678901234567890123456789", std.mem.span(win.title()));
    win.setTitle("12345678901234567890123456789012345678\xc3\xa4");
    try std.testing.expectEqualStrings("12345678901234567890123456789012345678\xc3\xa4", std.mem.span(win.title()));
}
