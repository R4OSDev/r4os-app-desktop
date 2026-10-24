const std = @import("std");
const model = @import("model.zig");
const start_menu = @import("start_menu.zig");
const surface = @import("surface.zig");
const theme = @import("theme.zig");

pub const max_items: usize = 2;
pub const no_selection: usize = max_items;
pub const button_w: i32 = 24;
pub const button_h: i32 = 22;
pub const button_gap: i32 = 2;
pub const bar_left_gap: i32 = 4;
pub const bar_right_gap: i32 = 8;
pub const separator_w: i32 = 4;
pub const icon_size: u16 = 16;

pub const title_max: usize = 31;
pub const path_max: usize = 63;
pub const args_max: usize = 127;
pub const icon_max: usize = 63;

pub const registry_root_key = "SYSTEM\\Shell\\Taskbar\\QuickLaunch";
pub const registry_item0_key = "SYSTEM\\Shell\\Taskbar\\QuickLaunch\\Item0";
pub const registry_item1_key = "SYSTEM\\Shell\\Taskbar\\QuickLaunch\\Item1";

pub const ItemKind = enum(u8) {
    show_desktop,
    program,
};

pub const Launch = struct {
    target: model.UiTarget,
    kind: start_menu.EntryKind,
    policy: start_menu.LaunchPolicy,
    title: [*:0]const u8,
    path: [*:0]const u8,
    args: [*:0]const u8,
};

pub const Item = struct {
    kind: ItemKind = .program,
    launch_policy: start_menu.LaunchPolicy = .auto,
    title: [title_max + 1]u8 = .{0} ** (title_max + 1),
    path: [path_max + 1]u8 = .{0} ** (path_max + 1),
    args: [args_max + 1]u8 = .{0} ** (args_max + 1),
    icon: [icon_max + 1]u8 = .{0} ** (icon_max + 1),

    pub fn titleZ(self: *const Item) [*:0]const u8 {
        return @ptrCast(&self.title);
    }

    pub fn pathZ(self: *const Item) [*:0]const u8 {
        return @ptrCast(&self.path);
    }

    pub fn argsZ(self: *const Item) [*:0]const u8 {
        return @ptrCast(&self.args);
    }

    pub fn iconZ(self: *const Item) [*:0]const u8 {
        return @ptrCast(&self.icon);
    }

    pub fn isValid(self: *const Item) bool {
        const title = spanZ(self.title[0..]);
        if (title.len == 0) return false;
        return switch (self.kind) {
            .show_desktop => self.launch_policy == .action,
            .program => spanZ(self.path[0..]).len != 0 and self.launch_policy != .action,
        };
    }

    pub fn launch(self: *const Item, target: model.UiTarget) Launch {
        return .{
            .target = target,
            .kind = if (self.kind == .show_desktop) .action else .item,
            .policy = if (self.kind == .show_desktop) .action else self.launch_policy,
            .title = self.titleZ(),
            .path = self.pathZ(),
            .args = self.argsZ(),
        };
    }

    pub fn setKindText(self: *Item, value: []const u8) bool {
        if (equalsIgnoreCase(value, "show_desktop") or equalsIgnoreCase(value, "desktop")) {
            self.kind = .show_desktop;
            self.launch_policy = .action;
            return true;
        }
        if (equalsIgnoreCase(value, "program") or equalsIgnoreCase(value, "app")) {
            self.kind = .program;
            if (self.launch_policy == .action) self.launch_policy = .auto;
            return true;
        }
        return false;
    }

    pub fn setPolicyText(self: *Item, value: []const u8) bool {
        if (equalsIgnoreCase(value, "console")) {
            self.launch_policy = .console;
            return true;
        }
        if (equalsIgnoreCase(value, "gui")) {
            self.launch_policy = .gui;
            return true;
        }
        if (equalsIgnoreCase(value, "auto")) {
            self.launch_policy = .auto;
            return true;
        }
        if (equalsIgnoreCase(value, "action")) {
            self.launch_policy = .action;
            return true;
        }
        return false;
    }

    pub fn setTitle(self: *Item, value: []const u8) void {
        copyZ(self.title[0..], value);
    }

    pub fn setPath(self: *Item, value: []const u8) void {
        copyZ(self.path[0..], value);
    }

    pub fn setArgs(self: *Item, value: []const u8) void {
        copyZ(self.args[0..], value);
    }

    pub fn setIcon(self: *Item, value: []const u8) void {
        copyZ(self.icon[0..], value);
    }
};

pub const Bar = struct {
    items: [max_items]Item = .{Item{}} ** max_items,
    count: usize = 0,

    pub fn initDefault() Bar {
        var bar = Bar{};
        bar.count = max_items;
        bar.items[0].kind = .show_desktop;
        bar.items[0].launch_policy = .action;
        bar.items[0].setTitle("Desktop anzeigen");
        bar.items[1].kind = .program;
        bar.items[1].launch_policy = .gui;
        bar.items[1].setTitle("Computer");
        bar.items[1].setPath("/R4OS/SOFTWARE/DESKTOP/EXPLORER.R4X");
        bar.items[1].setIcon("/R4OS/Media/Icons/Folder.ico");
        return bar;
    }

    pub fn setCount(self: *Bar, value: u32) void {
        const next: usize = @intCast(@min(value, @as(u32, @intCast(max_items))));
        self.count = next;
    }

    pub fn target(self: *const Bar, index: usize) model.UiTarget {
        _ = self;
        return targetForIndex(index);
    }

    pub fn launch(self: *const Bar, index: usize) ?Launch {
        if (index >= self.count) return null;
        return self.items[index].launch(targetForIndex(index));
    }

    pub fn hit(self: *const Bar, screen_h: i32, x: i32, y: i32) ?usize {
        var i: usize = 0;
        while (i < self.count) : (i += 1) {
            if (buttonRect(screen_h, i).contains(x, y)) return i;
        }
        return null;
    }
};

pub fn indexForTarget(target: model.UiTarget) ?usize {
    return switch (target) {
        .quick_show_desktop => 0,
        .quick_computer => 1,
        else => null,
    };
}

pub fn targetForIndex(index: usize) model.UiTarget {
    return switch (index) {
        0 => .quick_show_desktop,
        1 => .quick_computer,
        else => .none,
    };
}

pub fn itemRegistryKey(index: usize) [*:0]const u8 {
    return switch (index) {
        0 => registry_item0_key,
        1 => registry_item1_key,
        else => "",
    };
}

pub fn barX() i32 {
    return theme.start_w + 6;
}

pub fn barWidth(count: usize) i32 {
    if (count == 0) return 0;
    const visible: i32 = @intCast(@min(count, max_items));
    return bar_left_gap + visible * button_w + (visible - 1) * button_gap + bar_right_gap + separator_w;
}

pub fn barRect(screen_h: i32, count: usize) surface.Rect {
    const top = screen_h - theme.taskbar_h;
    return .{
        .x = barX(),
        .y = top + 4,
        .w = barWidth(count),
        .h = theme.start_h,
    };
}

pub fn buttonRect(screen_h: i32, index: usize) surface.Rect {
    const top = screen_h - theme.taskbar_h;
    return .{
        .x = barX() + bar_left_gap + @as(i32, @intCast(index)) * (button_w + button_gap),
        .y = top + 5,
        .w = button_w,
        .h = button_h,
    };
}

pub fn separatorX(count: usize) i32 {
    return barX() + barWidth(count) - separator_w + 1;
}

pub fn taskbarWindowStartX(count: usize) i32 {
    return theme.start_w + 10 + barWidth(count);
}

fn copyZ(out: []u8, value: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const count = @min(value.len, out.len - 1);
    if (count > 0) @memcpy(out[0..count], value[0..count]);
    out[count] = 0;
}

fn spanZ(buffer: []const u8) []const u8 {
    var len: usize = 0;
    while (len < buffer.len and buffer[len] != 0) : (len += 1) {}
    return buffer[0..len];
}

fn equalsIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (asciiLower(a[i]) != asciiLower(b[i])) return false;
    }
    return true;
}

fn asciiLower(ch: u8) u8 {
    if (ch >= 'A' and ch <= 'Z') return ch + ('a' - 'A');
    return ch;
}

test "default quick launch exposes desktop and computer entries" {
    const bar = Bar.initDefault();
    try std.testing.expectEqual(@as(usize, 2), bar.count);
    try std.testing.expectEqual(ItemKind.show_desktop, bar.items[0].kind);
    try std.testing.expectEqual(ItemKind.program, bar.items[1].kind);
    try std.testing.expectEqual(model.UiTarget.quick_show_desktop, bar.target(0));
    try std.testing.expectEqual(model.UiTarget.quick_computer, bar.target(1));
    try std.testing.expectEqualStrings("Desktop anzeigen", spanZ(bar.items[0].title[0..]));
    try std.testing.expectEqualStrings("/R4OS/SOFTWARE/DESKTOP/EXPLORER.R4X", spanZ(bar.items[1].path[0..]));
    try std.testing.expectEqualStrings("/R4OS/Media/Icons/Folder.ico", spanZ(bar.items[1].icon[0..]));
}

test "quick launch hit testing follows taskbar geometry" {
    const bar = Bar.initDefault();
    const first = buttonRect(720, 0);
    const second = buttonRect(720, 1);
    try std.testing.expectEqual(@as(?usize, 0), bar.hit(720, first.x + 2, first.y + 2));
    try std.testing.expectEqual(@as(?usize, 1), bar.hit(720, second.x + 2, second.y + 2));
    try std.testing.expectEqual(@as(?usize, null), bar.hit(720, first.x - 1, first.y + 2));
    try std.testing.expect(taskbarWindowStartX(bar.count) > theme.start_w + 10);
}

test "registry text can override item kind and policy" {
    var item = Item{};
    try std.testing.expect(item.setKindText("show_desktop"));
    try std.testing.expectEqual(ItemKind.show_desktop, item.kind);
    try std.testing.expectEqual(start_menu.LaunchPolicy.action, item.launch_policy);
    try std.testing.expect(item.setKindText("program"));
    try std.testing.expect(item.setPolicyText("gui"));
    try std.testing.expectEqual(ItemKind.program, item.kind);
    try std.testing.expectEqual(start_menu.LaunchPolicy.gui, item.launch_policy);
}

test "quick launch validation rejects broken registry entries" {
    var item = Item{};
    item.setTitle("Computer");
    item.setPath("/R4OS/SOFTWARE/DESKTOP/EXPLORER.R4X");
    item.launch_policy = .gui;
    try std.testing.expect(item.isValid());

    item.setPath("");
    try std.testing.expect(!item.isValid());

    item.setPath("/R4OS/SOFTWARE/DESKTOP/EXPLORER.R4X");
    item.launch_policy = .action;
    try std.testing.expect(!item.isValid());

    item.kind = .show_desktop;
    item.launch_policy = .action;
    try std.testing.expect(item.isValid());
}
