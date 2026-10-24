const std = @import("std");
const r4os = @import("r4os");
const r4std = @import("r4std");
const desktop_layout = @import("desktop_layout.zig");
const model = @import("model.zig");

pub const max_items: usize = 32;
pub const no_selection: usize = max_items;
pub const icon_w: i32 = 72;
pub const icon_h: i32 = 62;
pub const cell_w: i32 = 108;
pub const cell_h: i32 = 76;
pub const start_x: i32 = 24;
pub const start_y: i32 = 76;
pub const bottom_margin: i32 = 8;
pub const title_max: usize = 31;
pub const path_max: usize = 159;
pub const args_max: usize = 127;
pub const icon_max: usize = 127;
pub const folder_icon_path = "C:\\R4OS\\Media\\Icons\\Folder.ico";
pub const explorer_path = "C:\\R4OS\\SOFTWARE\\DESKTOP\\EXPLORER.R4X";
pub const GridPosition = desktop_layout.Position;

pub const FsKind = enum(u8) {
    file,
    directory,
};

pub const ItemKind = enum(u8) {
    file,
    directory,
    program,
    shortcut,
};

pub const LaunchKind = enum(u8) {
    file,
    directory,
    program,
};

pub const Item = struct {
    target: model.UiTarget = .none,
    kind: ItemKind = .file,
    launch_kind: LaunchKind = .file,
    launch_policy: r4os.abi.LaunchPolicy = .auto,
    x: i32 = 0,
    y: i32 = 0,
    title: [title_max + 1]u8 = .{0} ** (title_max + 1),
    path: [path_max + 1]u8 = .{0} ** (path_max + 1),
    launch_path: [path_max + 1]u8 = .{0} ** (path_max + 1),
    args: [args_max + 1]u8 = .{0} ** (args_max + 1),
    icon: [icon_max + 1]u8 = .{0} ** (icon_max + 1),

    pub fn init(path: []const u8, fs_kind: FsKind) Item {
        var item = Item{};
        item.kind = if (fs_kind == .directory) .directory else if (endsWithIgnoreCase(path, ".R4X")) .program else .file;
        item.launch_kind = switch (item.kind) {
            .directory => .directory,
            .program => .program,
            else => .file,
        };
        item.launch_policy = .auto;
        item.setPath(path);
        item.setLaunchPath(path);
        item.setTitle(titleFromPath(path, item.kind));
        if (item.kind == .directory) item.setIcon(folder_icon_path);
        return item;
    }

    pub fn applyShortcut(self: *Item, link: *const r4std.shortcut.Shortcut) void {
        self.kind = .shortcut;
        const resolved = link.resolve() catch return;
        self.launch_kind = switch (resolved.kind) {
            .program => .program,
            .directory => .directory,
            .file => .file,
        };
        self.launch_policy = resolved.policy;
        self.setLaunchPath(resolved.target);
        self.setArgs(resolved.args);
        if (resolved.icon.len != 0) self.setIcon(resolved.icon);
        if (resolved.title.len != 0) {
            self.setTitle(resolved.title);
        } else {
            self.setTitle(titleFromPath(self.pathText(), .shortcut));
        }
    }

    pub fn titleZ(self: *const Item) [*:0]const u8 {
        return @ptrCast(&self.title);
    }

    pub fn pathZ(self: *const Item) [*:0]const u8 {
        return @ptrCast(&self.path);
    }

    pub fn launchPathZ(self: *const Item) [*:0]const u8 {
        return @ptrCast(&self.launch_path);
    }

    pub fn argsZ(self: *const Item) [*:0]const u8 {
        return @ptrCast(&self.args);
    }

    pub fn iconZ(self: *const Item) [*:0]const u8 {
        return @ptrCast(&self.icon);
    }

    pub fn titleText(self: *const Item) []const u8 {
        return spanZ(self.title[0..]);
    }

    pub fn pathText(self: *const Item) []const u8 {
        return spanZ(self.path[0..]);
    }

    pub fn launchPathText(self: *const Item) []const u8 {
        return spanZ(self.launch_path[0..]);
    }

    pub fn argsText(self: *const Item) []const u8 {
        return spanZ(self.args[0..]);
    }

    pub fn iconText(self: *const Item) []const u8 {
        return spanZ(self.icon[0..]);
    }

    pub fn setTitle(self: *Item, value: []const u8) void {
        copyZ(self.title[0..], value);
    }

    pub fn setPath(self: *Item, value: []const u8) void {
        copyZ(self.path[0..], value);
    }

    pub fn setLaunchPath(self: *Item, value: []const u8) void {
        copyZ(self.launch_path[0..], value);
    }

    pub fn setArgs(self: *Item, value: []const u8) void {
        copyZ(self.args[0..], value);
    }

    pub fn setIcon(self: *Item, value: []const u8) void {
        copyZ(self.icon[0..], value);
    }
};

pub const Items = struct {
    entries: [max_items]Item = .{Item{}} ** max_items,
    count: usize = 0,
    truncated: bool = false,

    pub fn clear(self: *Items) void {
        self.* = .{};
    }

    pub fn add(self: *Items, item: Item) bool {
        if (self.count >= self.entries.len) {
            self.truncated = true;
            return false;
        }
        var next = item;
        next.target = targetForIndex(self.count);
        self.entries[self.count] = next;
        self.count += 1;
        return true;
    }

    pub fn sortByTitle(self: *Items) void {
        var i: usize = 1;
        while (i < self.count) : (i += 1) {
            const value = self.entries[i];
            var j = i;
            while (j > 0 and compareItems(value, self.entries[j - 1]) < 0) : (j -= 1) {
                self.entries[j] = self.entries[j - 1];
            }
            self.entries[j] = value;
        }
        var index: usize = 0;
        while (index < self.count) : (index += 1) self.entries[index].target = targetForIndex(index);
    }

    pub fn layout(self: *Items, screen_w: i32, screen_h: i32, taskbar_h: i32) void {
        const empty = desktop_layout.Layout{};
        self.layoutWith(&empty, screen_w, screen_h, taskbar_h);
    }

    pub fn layoutWith(self: *Items, saved: *const desktop_layout.Layout, screen_w: i32, screen_h: i32, taskbar_h: i32) void {
        const metrics = GridMetrics.init(screen_w, screen_h, taskbar_h);
        var occupied: [max_items]desktop_layout.Position = undefined;
        var occupied_count: usize = 0;
        var index: usize = 0;
        while (index < self.count) : (index += 1) {
            const desired = saved.positionForPath(self.entries[index].pathText()) orelse metrics.autoPosition(index);
            const position = metrics.firstFreePosition(desired, occupied[0..occupied_count]);
            self.entries[index].x = position.x;
            self.entries[index].y = position.y;
            if (occupied_count < occupied.len) {
                occupied[occupied_count] = position;
                occupied_count += 1;
            }
        }
    }

    pub fn snapItemToGrid(self: *Items, index: usize, screen_w: i32, screen_h: i32, taskbar_h: i32) void {
        const position = self.snapPreview(index, screen_w, screen_h, taskbar_h) orelse return;
        self.entries[index].x = position.x;
        self.entries[index].y = position.y;
    }

    pub fn snapPreview(self: *const Items, index: usize, screen_w: i32, screen_h: i32, taskbar_h: i32) ?desktop_layout.Position {
        if (index >= self.count) return null;
        const metrics = GridMetrics.init(screen_w, screen_h, taskbar_h);
        var occupied: [max_items]desktop_layout.Position = undefined;
        var occupied_count: usize = 0;
        var other: usize = 0;
        while (other < self.count) : (other += 1) {
            if (other == index) continue;
            if (occupied_count >= occupied.len) break;
            occupied[occupied_count] = .{ .x = self.entries[other].x, .y = self.entries[other].y };
            occupied_count += 1;
        }
        const desired = desktop_layout.Position{ .x = self.entries[index].x, .y = self.entries[index].y };
        return metrics.firstFreePosition(desired, occupied[0..occupied_count]);
    }

    pub fn hit(self: *const Items, x: i32, y: i32) ?usize {
        var index: usize = 0;
        while (index < self.count) : (index += 1) {
            const entry = &self.entries[index];
            if (x >= entry.x and x < entry.x + icon_w and y >= entry.y and y < entry.y + icon_h) return index;
        }
        return null;
    }

    pub fn target(self: *const Items, index: usize) model.UiTarget {
        if (index >= self.count) return .none;
        return self.entries[index].target;
    }
};

pub fn targetForIndex(index: usize) model.UiTarget {
    return switch (index) {
        0 => .desktop_icon_1,
        1 => .desktop_icon_2,
        2 => .desktop_icon_3,
        3 => .desktop_icon_4,
        4 => .desktop_icon_5,
        5 => .desktop_icon_6,
        6 => .desktop_icon_7,
        7 => .desktop_icon_8,
        8 => .desktop_item_9,
        9 => .desktop_item_10,
        10 => .desktop_item_11,
        11 => .desktop_item_12,
        12 => .desktop_item_13,
        13 => .desktop_item_14,
        14 => .desktop_item_15,
        15 => .desktop_item_16,
        16 => .desktop_item_17,
        17 => .desktop_item_18,
        18 => .desktop_item_19,
        19 => .desktop_item_20,
        20 => .desktop_item_21,
        21 => .desktop_item_22,
        22 => .desktop_item_23,
        23 => .desktop_item_24,
        24 => .desktop_item_25,
        25 => .desktop_item_26,
        26 => .desktop_item_27,
        27 => .desktop_item_28,
        28 => .desktop_item_29,
        29 => .desktop_item_30,
        30 => .desktop_item_31,
        31 => .desktop_item_32,
        else => .none,
    };
}

pub fn indexForTarget(target: model.UiTarget) ?usize {
    return switch (target) {
        .desktop_icon_1 => 0,
        .desktop_icon_2 => 1,
        .desktop_icon_3 => 2,
        .desktop_icon_4 => 3,
        .desktop_icon_5 => 4,
        .desktop_icon_6 => 5,
        .desktop_icon_7 => 6,
        .desktop_icon_8 => 7,
        .desktop_item_9 => 8,
        .desktop_item_10 => 9,
        .desktop_item_11 => 10,
        .desktop_item_12 => 11,
        .desktop_item_13 => 12,
        .desktop_item_14 => 13,
        .desktop_item_15 => 14,
        .desktop_item_16 => 15,
        .desktop_item_17 => 16,
        .desktop_item_18 => 17,
        .desktop_item_19 => 18,
        .desktop_item_20 => 19,
        .desktop_item_21 => 20,
        .desktop_item_22 => 21,
        .desktop_item_23 => 22,
        .desktop_item_24 => 23,
        .desktop_item_25 => 24,
        .desktop_item_26 => 25,
        .desktop_item_27 => 26,
        .desktop_item_28 => 27,
        .desktop_item_29 => 28,
        .desktop_item_30 => 29,
        .desktop_item_31 => 30,
        .desktop_item_32 => 31,
        else => null,
    };
}

fn compareItems(a: Item, b: Item) i32 {
    const ak = kindRank(a.kind);
    const bk = kindRank(b.kind);
    if (ak < bk) return -1;
    if (ak > bk) return 1;
    return compareIgnoreCase(a.titleText(), b.titleText());
}

fn kindRank(kind: ItemKind) u8 {
    return switch (kind) {
        .directory => 0,
        .shortcut => 1,
        .program => 2,
        .file => 3,
    };
}

pub const GridMetrics = struct {
    rows: usize,
    max_x: i32,
    max_y: i32,

    pub fn init(screen_w: i32, screen_h: i32, taskbar_h: i32) GridMetrics {
        const usable_bottom = @max(start_y + cell_h, screen_h - taskbar_h - bottom_margin);
        const rows_i32 = @max(@as(i32, 1), @divTrunc(usable_bottom - start_y, cell_h));
        return .{
            .rows = @intCast(rows_i32),
            .max_x = @max(0, screen_w - icon_w),
            .max_y = @max(0, screen_h - taskbar_h - icon_h),
        };
    }

    pub fn autoPosition(self: GridMetrics, index: usize) desktop_layout.Position {
        const row: i32 = @intCast(index % self.rows);
        const col: i32 = @intCast(index / self.rows);
        return self.positionForCell(.{ .row = row, .col = col });
    }

    pub fn firstFreePosition(self: GridMetrics, desired: desktop_layout.Position, occupied: []const desktop_layout.Position) desktop_layout.Position {
        var cell = self.cellForPosition(desired);
        var linear: usize = @as(usize, @intCast(@max(@as(i32, 0), cell.col))) * self.rows + @as(usize, @intCast(@max(@as(i32, 0), cell.row)));
        var attempt: usize = 0;
        while (attempt < max_items * 2) : (attempt += 1) {
            cell = .{
                .row = @intCast(linear % self.rows),
                .col = @intCast(linear / self.rows),
            };
            const candidate = self.positionForCell(cell);
            if (!isOccupied(candidate, occupied)) return candidate;
            linear += 1;
        }
        return self.positionForCell(self.cellForPosition(desired));
    }

    pub fn cellForPosition(self: GridMetrics, position: desktop_layout.Position) GridCell {
        const snapped = desktop_layout.Position{
            .x = clamp(position.x, 0, self.max_x),
            .y = clamp(position.y, 0, self.max_y),
        };
        const dx = @max(@as(i32, 0), snapped.x - start_x);
        const dy = @max(@as(i32, 0), snapped.y - start_y);
        const row = @min(@as(i32, @intCast(self.rows - 1)), @divTrunc(dy, cell_h));
        const col = @divTrunc(dx, cell_w);
        return .{ .row = row, .col = col };
    }

    pub fn positionForCell(self: GridMetrics, cell: GridCell) desktop_layout.Position {
        return .{
            .x = clamp(start_x + cell.col * cell_w, 0, self.max_x),
            .y = clamp(start_y + cell.row * cell_h, 0, self.max_y),
        };
    }

    pub fn positionForLinear(self: GridMetrics, linear: usize) desktop_layout.Position {
        return self.positionForCell(.{
            .row = @intCast(linear % self.rows),
            .col = @intCast(linear / self.rows),
        });
    }
};

pub const GridCell = struct {
    row: i32,
    col: i32,
};

fn isOccupied(position: desktop_layout.Position, occupied: []const desktop_layout.Position) bool {
    for (occupied) |entry| {
        if (entry.x == position.x and entry.y == position.y) return true;
    }
    return false;
}

fn titleFromPath(path: []const u8, kind: ItemKind) []const u8 {
    const tail = tailName(path);
    if (kind == .shortcut and endsWithIgnoreCase(tail, ".LNK")) return tail[0 .. tail.len - 4];
    if (kind == .program and endsWithIgnoreCase(tail, ".R4X")) return tail[0 .. tail.len - 4];
    return tail;
}

fn tailName(path: []const u8) []const u8 {
    var start: usize = 0;
    var index: usize = 0;
    while (index < path.len) : (index += 1) {
        if (path[index] == '\\' or path[index] == '/') start = index + 1;
    }
    return path[start..];
}

fn endsWithIgnoreCase(value: []const u8, suffix: []const u8) bool {
    if (value.len < suffix.len) return false;
    const start = value.len - suffix.len;
    var index: usize = 0;
    while (index < suffix.len) : (index += 1) {
        if (asciiUpper(value[start + index]) != asciiUpper(suffix[index])) return false;
    }
    return true;
}

fn compareIgnoreCase(a: []const u8, b: []const u8) i32 {
    const len = @min(a.len, b.len);
    var index: usize = 0;
    while (index < len) : (index += 1) {
        const ac = asciiUpper(a[index]);
        const bc = asciiUpper(b[index]);
        if (ac < bc) return -1;
        if (ac > bc) return 1;
    }
    if (a.len < b.len) return -1;
    if (a.len > b.len) return 1;
    return 0;
}

fn copyZ(out: []u8, value: []const u8) void {
    if (out.len == 0) return;
    @memset(out, 0);
    const count = @min(value.len, out.len - 1);
    if (count > 0) @memcpy(out[0..count], value[0..count]);
    out[count] = 0;
}

fn spanZ(buffer: []const u8) []const u8 {
    var len: usize = 0;
    while (len < buffer.len and buffer[len] != 0) : (len += 1) {}
    return buffer[0..len];
}

fn clamp(value: i32, min_value: i32, max_value: i32) i32 {
    if (value < min_value) return min_value;
    if (value > max_value) return max_value;
    return value;
}

fn asciiUpper(ch: u8) u8 {
    if (ch >= 'a' and ch <= 'z') return ch - ('a' - 'A');
    return ch;
}

test "desktop folder model supports more than eight stable targets" {
    try std.testing.expect(max_items > 8);
    try std.testing.expectEqual(model.UiTarget.desktop_icon_1, targetForIndex(0));
    try std.testing.expectEqual(model.UiTarget.desktop_item_9, targetForIndex(8));
    try std.testing.expectEqual(model.UiTarget.desktop_item_32, targetForIndex(31));
    try std.testing.expectEqual(@as(?usize, 31), indexForTarget(.desktop_item_32));
}

test "desktop folder items sort and layout as a directory view" {
    var items = Items{};
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\ZETA.TXT", .file));
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\TOOLS", .directory));
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\APP.R4X", .file));
    items.sortByTitle();
    items.layout(1280, 720, 28);

    try std.testing.expectEqual(ItemKind.directory, items.entries[0].kind);
    try std.testing.expectEqualStrings("TOOLS", items.entries[0].titleText());
    try std.testing.expectEqual(ItemKind.program, items.entries[1].kind);
    try std.testing.expectEqualStrings("APP", items.entries[1].titleText());
    try std.testing.expectEqual(@as(?usize, 0), items.hit(start_x + 4, start_y + 4));
}

test "desktop layout positions are restored and collisions move to next grid cell" {
    var items = Items{};
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\ALPHA.LNK", .file));
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\BETA.LNK", .file));
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\GAMMA.LNK", .file));

    var layout_state = desktop_layout.Layout{};
    try std.testing.expect(layout_state.add("C:\\R4OS\\DESKTOP\\ALPHA.LNK", start_x, start_y));
    try std.testing.expect(layout_state.add("C:\\R4OS\\DESKTOP\\BETA.LNK", start_x, start_y));
    items.layoutWith(&layout_state, 1280, 720, 28);

    try std.testing.expectEqual(@as(i32, start_x), items.entries[0].x);
    try std.testing.expectEqual(@as(i32, start_y), items.entries[0].y);
    try std.testing.expectEqual(@as(i32, start_x), items.entries[1].x);
    try std.testing.expectEqual(@as(i32, start_y + cell_h), items.entries[1].y);
    try std.testing.expect(items.entries[2].x != items.entries[0].x or items.entries[2].y != items.entries[0].y);
}

test "desktop layout clamps saved positions into the visible work area" {
    var items = Items{};
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\ALPHA.LNK", .file));

    var layout_state = desktop_layout.Layout{};
    try std.testing.expect(layout_state.add("C:\\R4OS\\DESKTOP\\ALPHA.LNK", -500, 5000));
    items.layoutWith(&layout_state, 320, 240, 28);

    try std.testing.expect(items.entries[0].x >= 0);
    try std.testing.expect(items.entries[0].y >= 0);
    try std.testing.expect(items.entries[0].x <= 320 - icon_w);
    try std.testing.expect(items.entries[0].y <= 240 - 28 - icon_h);
}

test "desktop layout resize keeps saved items visible and separated" {
    var items = Items{};
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\ALPHA.LNK", .file));
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\BETA.LNK", .file));
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\GAMMA.LNK", .file));
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\DELTA.LNK", .file));

    var layout_state = desktop_layout.Layout{};
    try std.testing.expect(layout_state.add("C:\\R4OS\\DESKTOP\\ALPHA.LNK", 4000, 4000));
    try std.testing.expect(layout_state.add("C:\\R4OS\\DESKTOP\\BETA.LNK", 4000, 4000));
    try std.testing.expect(layout_state.add("C:\\R4OS\\DESKTOP\\GAMMA.LNK", -200, -200));
    items.layoutWith(&layout_state, 400, 340, 28);

    var index: usize = 0;
    while (index < items.count) : (index += 1) {
        try std.testing.expect(items.entries[index].x >= 0);
        try std.testing.expect(items.entries[index].y >= 0);
        try std.testing.expect(items.entries[index].x <= 400 - icon_w);
        try std.testing.expect(items.entries[index].y <= 340 - 28 - icon_h);
        var other = index + 1;
        while (other < items.count) : (other += 1) {
            try std.testing.expect(items.entries[index].x != items.entries[other].x or items.entries[index].y != items.entries[other].y);
        }
    }
}

test "desktop item snap uses next free grid cell without moving other items" {
    var items = Items{};
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\ALPHA.LNK", .file));
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\BETA.LNK", .file));
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\GAMMA.LNK", .file));
    items.layout(1280, 720, 28);

    const beta_x = items.entries[1].x;
    const beta_y = items.entries[1].y;
    items.entries[2].x = items.entries[0].x + 9;
    items.entries[2].y = items.entries[0].y + 7;
    items.snapItemToGrid(2, 1280, 720, 28);

    try std.testing.expectEqual(@as(i32, start_x), items.entries[0].x);
    try std.testing.expectEqual(@as(i32, start_y), items.entries[0].y);
    try std.testing.expectEqual(beta_x, items.entries[1].x);
    try std.testing.expectEqual(beta_y, items.entries[1].y);
    try std.testing.expect(items.entries[2].x != items.entries[0].x or items.entries[2].y != items.entries[0].y);
    try std.testing.expect(items.entries[2].x != items.entries[1].x or items.entries[2].y != items.entries[1].y);
}

test "desktop grid metrics match snap preview positions" {
    var items = Items{};
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\ALPHA.LNK", .file));
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\BETA.LNK", .file));
    _ = items.add(Item.init("C:\\R4OS\\DESKTOP\\GAMMA.LNK", .file));
    items.layout(1280, 720, 28);

    const metrics = GridMetrics.init(1280, 720, 28);
    try std.testing.expectEqual(GridPosition{ .x = start_x, .y = start_y }, metrics.positionForLinear(0));
    try std.testing.expectEqual(GridPosition{ .x = start_x, .y = start_y + cell_h }, metrics.positionForLinear(1));

    items.entries[2].x = items.entries[0].x + 12;
    items.entries[2].y = items.entries[0].y + 10;
    const preview = items.snapPreview(2, 1280, 720, 28).?;
    items.snapItemToGrid(2, 1280, 720, 28);

    try std.testing.expectEqual(preview.x, items.entries[2].x);
    try std.testing.expectEqual(preview.y, items.entries[2].y);
}

test "desktop shortcut items use R4LNK launch metadata" {
    var item = Item.init("C:\\R4OS\\DESKTOP\\NOTEPAD.LNK", .file);
    var link = try r4std.shortcut.Shortcut.init("C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X");
    try link.setTitle("Notepad");
    link.setPolicy(.gui);
    try link.setIcon("C:\\R4OS\\Media\\Icons\\Notepad.ico");

    item.applyShortcut(&link);

    try std.testing.expectEqual(ItemKind.shortcut, item.kind);
    try std.testing.expectEqual(LaunchKind.program, item.launch_kind);
    try std.testing.expectEqual(r4os.abi.LaunchPolicy.gui, item.launch_policy);
    try std.testing.expectEqualStrings("Notepad", item.titleText());
    try std.testing.expectEqualStrings("C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X", item.launchPathText());
    try std.testing.expectEqualStrings("C:\\R4OS\\Media\\Icons\\Notepad.ico", item.iconText());
}
