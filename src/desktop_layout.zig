const std = @import("std");
const r4os = @import("r4os");
const r4std = @import("r4std");
const settings = r4std.settings;

pub const schema = "DESKTOPLAYOUT";
pub const max_entries: usize = 32;
pub const path_max: usize = 159;
pub const max_bytes: usize = 4096;
pub const desktop_path_prefix = "C:\\R4OS\\DESKTOP\\";

pub const Position = struct {
    x: i32 = 0,
    y: i32 = 0,
};

pub const Entry = struct {
    path: [path_max + 1]u8 = .{0} ** (path_max + 1),
    position: Position = .{},

    pub fn pathText(self: *const Entry) []const u8 {
        return spanZ(self.path[0..]);
    }
};

pub const Layout = struct {
    entries: [max_entries]Entry = .{Entry{}} ** max_entries,
    count: usize = 0,
    truncated: bool = false,
    parse_errors: bool = false,
    duplicate_paths: bool = false,
    invalid_schema: bool = false,

    pub fn clear(self: *Layout) void {
        self.* = .{};
    }

    pub fn loadFromBytes(self: *Layout, bytes: []const u8) bool {
        self.clear();
        const doc = settings.Document.init(bytes);
        if (!doc.hasSupportedFormat() or doc.schemaName() == null or !settings.equalsKey(doc.schemaName().?, schema)) {
            self.invalid_schema = true;
            return false;
        }

        var slots: [max_entries]Slot = .{Slot{}} ** max_entries;
        var iter = settings.EntryIterator.init(bytes);
        while (iter.next()) |entry| {
            const parsed = parseItemKey(entry.key) orelse continue;
            if (parsed.index >= max_entries) {
                self.truncated = true;
                continue;
            }
            var slot = &slots[parsed.index];
            switch (parsed.field) {
                .path => {
                    if (!validDesktopPath(entry.value)) {
                        self.parse_errors = true;
                        slot.has_path = false;
                        continue;
                    }
                    copyZ(slot.path[0..], entry.value);
                    slot.has_path = true;
                },
                .x => {
                    if (settings.parseI32(entry.value)) |value| {
                        slot.x = value;
                        slot.has_x = true;
                    } else {
                        self.parse_errors = true;
                        slot.has_x = false;
                    }
                },
                .y => {
                    if (settings.parseI32(entry.value)) |value| {
                        slot.y = value;
                        slot.has_y = true;
                    } else {
                        self.parse_errors = true;
                        slot.has_y = false;
                    }
                },
            }
        }

        var index: usize = 0;
        while (index < slots.len) : (index += 1) {
            const slot = &slots[index];
            if (!slot.has_path and !slot.has_x and !slot.has_y) continue;
            if (!slot.has_path or !slot.has_x or !slot.has_y) {
                self.parse_errors = true;
                continue;
            }
            _ = self.add(slot.pathText(), slot.x, slot.y);
        }
        return true;
    }

    pub fn add(self: *Layout, path: []const u8, x: i32, y: i32) bool {
        if (!validDesktopPath(path)) {
            self.parse_errors = true;
            return false;
        }
        if (self.findIndex(path)) |existing| {
            self.entries[existing].position = .{ .x = x, .y = y };
            self.duplicate_paths = true;
            return true;
        }
        if (self.count >= self.entries.len) {
            self.truncated = true;
            return false;
        }
        copyZ(self.entries[self.count].path[0..], path);
        self.entries[self.count].position = .{ .x = x, .y = y };
        self.count += 1;
        return true;
    }

    pub fn positionForPath(self: *const Layout, path: []const u8) ?Position {
        if (self.findIndex(path)) |index| return self.entries[index].position;
        return null;
    }

    pub fn writeTo(self: *const Layout, out: []u8) []const u8 {
        var writer = settings.Writer.init(out);
        writer.writeHeader(schema);
        writer.writePairU32("ITEM_COUNT", @intCast(self.count));
        var index: usize = 0;
        while (index < self.count) : (index += 1) {
            var key: [24]u8 = .{0} ** 24;
            const entry = &self.entries[index];
            writer.writePair(itemKey(key[0..], index, "PATH"), entry.pathText());
            writer.writePairI32(itemKey(key[0..], index, "X"), entry.position.x);
            writer.writePairI32(itemKey(key[0..], index, "Y"), entry.position.y);
        }
        return writer.bytes();
    }

    fn findIndex(self: *const Layout, path: []const u8) ?usize {
        var index: usize = 0;
        while (index < self.count) : (index += 1) {
            if (equalsIgnoreCase(self.entries[index].pathText(), path)) return index;
        }
        return null;
    }
};

const Field = enum {
    path,
    x,
    y,
};

const ParsedKey = struct {
    index: usize,
    field: Field,
};

const Slot = struct {
    path: [path_max + 1]u8 = .{0} ** (path_max + 1),
    x: i32 = 0,
    y: i32 = 0,
    has_path: bool = false,
    has_x: bool = false,
    has_y: bool = false,

    fn pathText(self: *const Slot) []const u8 {
        return spanZ(self.path[0..]);
    }
};

fn parseItemKey(key: []const u8) ?ParsedKey {
    const text = settings.trim(key);
    const prefix = "ITEM.";
    if (!startsWithIgnoreCase(text, prefix)) return null;
    const rest = text[prefix.len..];
    const split = findByte(rest, '.') orelse return null;
    const index = parseIndex(rest[0..split]) orelse return null;
    const field_text = rest[split + 1 ..];
    if (settings.equalsKey(field_text, "PATH")) return .{ .index = index, .field = .path };
    if (settings.equalsKey(field_text, "X")) return .{ .index = index, .field = .x };
    if (settings.equalsKey(field_text, "Y")) return .{ .index = index, .field = .y };
    return null;
}

pub fn validDesktopPath(path: []const u8) bool {
    const text = settings.trim(path);
    if (text.len <= desktop_path_prefix.len or text.len > path_max) return false;
    if (!startsWithIgnoreCase(text, desktop_path_prefix)) return false;
    var index: usize = 0;
    while (index < text.len) : (index += 1) {
        const ch = text[index];
        if (ch < 0x20 or ch == '/' or ch == '=') return false;
    }
    return !containsParentSegment(text[desktop_path_prefix.len..]);
}

fn containsParentSegment(path: []const u8) bool {
    var start: usize = 0;
    var index: usize = 0;
    while (index <= path.len) : (index += 1) {
        if (index == path.len or path[index] == '\\') {
            const segment = path[start..index];
            if (std.mem.eql(u8, segment, "..")) return true;
            start = index + 1;
        }
    }
    return false;
}

fn itemKey(out: []u8, index: usize, field: []const u8) []const u8 {
    var pos: usize = 0;
    appendSlice(out, &pos, "ITEM.");
    appendUnsigned(out, &pos, index);
    appendByte(out, &pos, '.');
    appendSlice(out, &pos, field);
    return out[0..pos];
}

fn parseIndex(text: []const u8) ?usize {
    if (text.len == 0) return null;
    var result: usize = 0;
    for (text) |ch| {
        if (ch < '0' or ch > '9') return null;
        const digit: usize = ch - '0';
        if (result > (std.math.maxInt(usize) - digit) / 10) return null;
        result = result * 10 + digit;
    }
    return result;
}

fn appendUnsigned(out: []u8, pos: *usize, value: usize) void {
    var tmp: [20]u8 = undefined;
    var tmp_pos: usize = tmp.len;
    var n = value;
    while (true) {
        tmp_pos -= 1;
        tmp[tmp_pos] = '0' + @as(u8, @intCast(n % 10));
        n /= 10;
        if (n == 0) break;
    }
    appendSlice(out, pos, tmp[tmp_pos..]);
}

fn appendSlice(out: []u8, pos: *usize, value: []const u8) void {
    const count = @min(value.len, out.len - pos.*);
    if (count > 0) @memcpy(out[pos.* .. pos.* + count], value[0..count]);
    pos.* += count;
}

fn appendByte(out: []u8, pos: *usize, value: u8) void {
    if (pos.* >= out.len) return;
    out[pos.*] = value;
    pos.* += 1;
}

fn findByte(value: []const u8, needle: u8) ?usize {
    var i: usize = 0;
    while (i < value.len) : (i += 1) {
        if (value[i] == needle) return i;
    }
    return null;
}

fn startsWithIgnoreCase(value: []const u8, prefix: []const u8) bool {
    if (value.len < prefix.len) return false;
    return equalsIgnoreCase(value[0..prefix.len], prefix);
}

fn equalsIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var index: usize = 0;
    while (index < a.len) : (index += 1) {
        if (asciiUpper(a[index]) != asciiUpper(b[index])) return false;
    }
    return true;
}

fn copyZ(out: []u8, value: []const u8) void {
    if (out.len == 0) return;
    @memset(out, 0);
    const text = settings.trim(value);
    const count = @min(text.len, out.len - 1);
    if (count > 0) @memcpy(out[0..count], text[0..count]);
    out[count] = 0;
}

fn spanZ(buffer: []const u8) []const u8 {
    var len: usize = 0;
    while (len < buffer.len and buffer[len] != 0) : (len += 1) {}
    return buffer[0..len];
}

fn asciiUpper(ch: u8) u8 {
    if (ch >= 'a' and ch <= 'z') return ch - ('a' - 'A');
    return ch;
}

test "desktop layout parses canonical R4S positions" {
    try @import("r4std_test").ensure();
    var layout = Layout{};
    try std.testing.expect(layout.loadFromBytes(
        \\R4S_FORMAT=1
        \\SCHEMA=DESKTOPLAYOUT
        \\ITEM_COUNT=2
        \\ITEM.0.PATH=C:\R4OS\DESKTOP\TERMINAL.LNK
        \\ITEM.0.X=24
        \\ITEM.0.Y=76
        \\ITEM.1.PATH=C:\R4OS\DESKTOP\TOOLS
        \\ITEM.1.X=132
        \\ITEM.1.Y=76
    ));

    try std.testing.expectEqual(@as(usize, 2), layout.count);
    const terminal = layout.positionForPath("c:\\r4os\\desktop\\terminal.lnk").?;
    try std.testing.expectEqual(@as(i32, 24), terminal.x);
    try std.testing.expectEqual(@as(i32, 76), terminal.y);
}

test "desktop layout rejects invalid schema without fallback" {
    try @import("r4std_test").ensure();
    var layout = Layout{};
    try std.testing.expect(!layout.loadFromBytes(
        \\R4S_FORMAT=1
        \\SCHEMA=DESKTOP
        \\ITEM.0.PATH=C:\R4OS\DESKTOP\TERMINAL.LNK
        \\ITEM.0.X=24
        \\ITEM.0.Y=76
    ));
    try std.testing.expect(layout.invalid_schema);
    try std.testing.expectEqual(@as(usize, 0), layout.count);
}

test "desktop layout ignores invalid paths and broken numbers" {
    try @import("r4std_test").ensure();
    var layout = Layout{};
    try std.testing.expect(layout.loadFromBytes(
        \\R4S_FORMAT=1
        \\SCHEMA=DESKTOPLAYOUT
        \\ITEM.0.PATH=C:\OTHER\ESCAPE.LNK
        \\ITEM.0.X=24
        \\ITEM.0.Y=76
        \\ITEM.1.PATH=C:\R4OS\DESKTOP\BROKEN.LNK
        \\ITEM.1.X=abc
        \\ITEM.1.Y=76
        \\ITEM.2.PATH=C:\R4OS\DESKTOP\..\BAD.LNK
        \\ITEM.2.X=24
        \\ITEM.2.Y=152
    ));
    try std.testing.expect(layout.parse_errors);
    try std.testing.expectEqual(@as(usize, 0), layout.count);
}

test "desktop layout duplicate item path uses the later canonical entry" {
    try @import("r4std_test").ensure();
    var layout = Layout{};
    try std.testing.expect(layout.loadFromBytes(
        \\R4S_FORMAT=1
        \\SCHEMA=DESKTOPLAYOUT
        \\ITEM.0.PATH=C:\R4OS\DESKTOP\TERMINAL.LNK
        \\ITEM.0.X=24
        \\ITEM.0.Y=76
        \\ITEM.1.PATH=C:\R4OS\DESKTOP\TERMINAL.LNK
        \\ITEM.1.X=132
        \\ITEM.1.Y=152
    ));
    try std.testing.expect(layout.duplicate_paths);
    try std.testing.expectEqual(@as(usize, 1), layout.count);
    const terminal = layout.positionForPath("C:\\R4OS\\DESKTOP\\TERMINAL.LNK").?;
    try std.testing.expectEqual(@as(i32, 132), terminal.x);
    try std.testing.expectEqual(@as(i32, 152), terminal.y);
}

test "desktop layout writes canonical settings document" {
    try @import("r4std_test").ensure();
    var layout = Layout{};
    try std.testing.expect(layout.add("C:\\R4OS\\DESKTOP\\TERMINAL.LNK", 24, 76));
    try std.testing.expect(layout.add("C:\\R4OS\\DESKTOP\\TOOLS", 132, 76));

    var out: [512]u8 = .{0} ** 512;
    const bytes = layout.writeTo(out[0..]);
    try std.testing.expect(std.mem.startsWith(u8, bytes, settings.utf8_bom));
    try std.testing.expect(std.mem.indexOf(u8, bytes, "SCHEMA=DESKTOPLAYOUT") != null);
    try std.testing.expect(std.mem.indexOf(u8, bytes, "ITEM.0.PATH=C:\\R4OS\\DESKTOP\\TERMINAL.LNK") != null);

    var parsed = Layout{};
    try std.testing.expect(parsed.loadFromBytes(bytes));
    try std.testing.expectEqual(@as(usize, 2), parsed.count);
    try std.testing.expect(parsed.positionForPath("C:\\R4OS\\DESKTOP\\TOOLS") != null);
}
