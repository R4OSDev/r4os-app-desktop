const std = @import("std");

pub const SurfaceKind = enum(u8) {
    desktop,
    taskbar,
    window_frame,
    window_title,
    window_client,
    popup,
    dialog,
    cursor,
};

pub const Rect = struct {
    x: i32,
    y: i32,
    w: i32,
    h: i32,

    pub fn right(self: Rect) i32 {
        return self.x + self.w;
    }

    pub fn bottom(self: Rect) i32 {
        return self.y + self.h;
    }

    pub fn isEmpty(self: Rect) bool {
        return self.w <= 0 or self.h <= 0;
    }

    pub fn contains(self: Rect, x: i32, y: i32) bool {
        return !self.isEmpty() and x >= self.x and x < self.right() and y >= self.y and y < self.bottom();
    }

    pub fn inset(self: Rect, dx: i32, dy: i32) Rect {
        const next_w = @max(0, self.w - dx * 2);
        const next_h = @max(0, self.h - dy * 2);
        return .{
            .x = self.x + dx,
            .y = self.y + dy,
            .w = next_w,
            .h = next_h,
        };
    }

    pub fn merged(self: Rect, other: Rect) Rect {
        if (self.isEmpty()) return other;
        if (other.isEmpty()) return self;

        const left = @min(self.x, other.x);
        const top = @min(self.y, other.y);
        const right_edge = @max(self.right(), other.right());
        const bottom_edge = @max(self.bottom(), other.bottom());
        return .{
            .x = left,
            .y = top,
            .w = right_edge - left,
            .h = bottom_edge - top,
        };
    }

    pub fn clampInside(self: Rect, bounds: Rect) Rect {
        if (self.w >= bounds.w or self.h >= bounds.h) {
            return .{
                .x = bounds.x,
                .y = bounds.y,
                .w = @min(self.w, bounds.w),
                .h = @min(self.h, bounds.h),
            };
        }

        return .{
            .x = clamp(self.x, bounds.x, bounds.right() - self.w),
            .y = clamp(self.y, bounds.y, bounds.bottom() - self.h),
            .w = self.w,
            .h = self.h,
        };
    }
};

pub const Surface = struct {
    kind: SurfaceKind,
    rect: Rect,

    pub fn contains(self: Surface, x: i32, y: i32) bool {
        return self.rect.contains(x, y);
    }
};

pub const cursor_w: i32 = 12;
pub const cursor_h: i32 = 18;

pub const Dirty = struct {
    active: bool = false,
    bounds: Rect = .{ .x = 0, .y = 0, .w = 0, .h = 0 },

    pub fn invalidate(self: *Dirty, rect: Rect) void {
        if (rect.isEmpty()) return;
        if (self.active) {
            self.bounds = self.bounds.merged(rect);
        } else {
            self.bounds = rect;
            self.active = true;
        }
    }

    pub fn invalidateSurface(self: *Dirty, item: Surface) void {
        self.invalidate(item.rect);
    }

    pub fn invalidateFull(self: *Dirty, screen_w: i32, screen_h: i32) void {
        self.invalidate(desktop(screen_w, screen_h).rect);
    }

    pub fn take(self: *Dirty) ?Rect {
        if (!self.active) return null;
        const rect = self.bounds;
        self.* = .{};
        return rect;
    }
};

pub fn make(kind: SurfaceKind, rect: Rect) Surface {
    return .{ .kind = kind, .rect = rect };
}

pub fn desktop(screen_w: i32, screen_h: i32) Surface {
    return make(.desktop, .{ .x = 0, .y = 0, .w = screen_w, .h = screen_h });
}

pub fn taskbar(screen_w: i32, screen_h: i32, taskbar_h: i32) Surface {
    return make(.taskbar, .{ .x = 0, .y = screen_h - taskbar_h, .w = screen_w, .h = taskbar_h });
}

pub fn cursor(x: i32, y: i32, screen_w: i32, screen_h: i32) Surface {
    const max_w = @max(0, screen_w - x);
    const max_h = @max(0, screen_h - y);
    return make(.cursor, .{
        .x = x,
        .y = y,
        .w = @min(cursor_w, max_w),
        .h = @min(cursor_h, max_h),
    });
}

pub fn workArea(screen_w: i32, screen_h: i32, taskbar_h: i32) Rect {
    return .{ .x = 0, .y = 0, .w = screen_w, .h = screen_h - taskbar_h };
}

fn clamp(value: i32, min: i32, max: i32) i32 {
    if (value < min) return min;
    if (value > max) return max;
    return value;
}

test "rect contains and inset use stable bounds" {
    const rect = Rect{ .x = 10, .y = 20, .w = 100, .h = 50 };
    try std.testing.expect(rect.contains(10, 20));
    try std.testing.expect(rect.contains(109, 69));
    try std.testing.expect(!rect.contains(110, 69));

    const inner = rect.inset(8, 4);
    try std.testing.expectEqual(Rect{ .x = 18, .y = 24, .w = 84, .h = 42 }, inner);
}

test "rect merged covers both inputs" {
    const a = Rect{ .x = 10, .y = 20, .w = 40, .h = 30 };
    const b = Rect{ .x = 35, .y = 10, .w = 50, .h = 70 };

    try std.testing.expectEqual(Rect{ .x = 10, .y = 10, .w = 75, .h = 70 }, a.merged(b));
}

test "rect clamp keeps surface inside bounds" {
    const bounds = Rect{ .x = 0, .y = 0, .w = 640, .h = 448 };
    const rect = Rect{ .x = 600, .y = 440, .w = 100, .h = 80 };

    try std.testing.expectEqual(Rect{ .x = 540, .y = 368, .w = 100, .h = 80 }, rect.clampInside(bounds));
}

test "desktop taskbar and work area are separate surfaces" {
    const desktop_surface = desktop(1280, 720);
    const taskbar_surface = taskbar(1280, 720, 32);

    try std.testing.expectEqual(SurfaceKind.desktop, desktop_surface.kind);
    try std.testing.expectEqual(Rect{ .x = 0, .y = 0, .w = 1280, .h = 720 }, desktop_surface.rect);
    try std.testing.expectEqual(SurfaceKind.taskbar, taskbar_surface.kind);
    try std.testing.expectEqual(Rect{ .x = 0, .y = 688, .w = 1280, .h = 32 }, taskbar_surface.rect);
    try std.testing.expectEqual(Rect{ .x = 0, .y = 0, .w = 1280, .h = 688 }, workArea(1280, 720, 32));
}

test "cursor surface is clipped at screen edge" {
    try std.testing.expectEqual(SurfaceKind.cursor, cursor(100, 80, 1280, 720).kind);
    try std.testing.expectEqual(Rect{ .x = 100, .y = 80, .w = 12, .h = 18 }, cursor(100, 80, 1280, 720).rect);
    try std.testing.expectEqual(Rect{ .x = 1275, .y = 710, .w = 5, .h = 10 }, cursor(1275, 710, 1280, 720).rect);
    try std.testing.expectEqual(Rect{ .x = 8, .y = 0, .w = 12, .h = 18 }, cursor(8, 0, 1280, 720).rect);
}

test "dirty tracker merges invalid rectangles and resets on take" {
    var dirty = Dirty{};
    dirty.invalidate(.{ .x = 10, .y = 10, .w = 20, .h = 20 });
    dirty.invalidate(.{ .x = 25, .y = 4, .w = 20, .h = 10 });

    try std.testing.expect(dirty.active);
    try std.testing.expectEqual(Rect{ .x = 10, .y = 4, .w = 35, .h = 26 }, dirty.bounds);
    try std.testing.expectEqual(Rect{ .x = 10, .y = 4, .w = 35, .h = 26 }, dirty.take().?);
    try std.testing.expect(!dirty.active);
    try std.testing.expectEqual(@as(?Rect, null), dirty.take());
}
