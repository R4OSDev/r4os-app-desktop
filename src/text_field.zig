const std = @import("std");
const r4os = @import("r4os");

pub fn validText(value: []const u8) bool {
    const view = std.unicode.Utf8View.init(value) catch return false;
    var it = view.iterator();
    while (it.nextCodepoint()) |cp| if (!isTextChar(cp)) return false;
    return true;
}

pub fn TextField(comptime capacity: usize) type {
    return struct {
        const Self = @This();

        buffer: [capacity + 1]u8 = .{0} ** (capacity + 1),
        len: usize = 0,

        pub fn set(self: *Self, value: []const u8) bool {
            if (value.len > capacity or !validText(value)) return false;
            std.mem.copyForwards(u8, self.buffer[0..value.len], value);
            self.len = value.len;
            @memset(self.buffer[self.len..], 0);
            return true;
        }

        pub fn append(self: *Self, ch: u32) bool {
            if (!isTextChar(ch)) return false;
            var bytes: [4]u8 = undefined;
            const count = std.unicode.utf8Encode(@intCast(ch), &bytes) catch return false;
            return self.appendText(bytes[0..count]);
        }

        pub fn appendText(self: *Self, value: []const u8) bool {
            if (value.len > capacity - self.len or !validText(value)) return false;
            std.mem.copyForwards(u8, self.buffer[self.len..][0..value.len], value);
            self.len += value.len;
            self.buffer[self.len] = 0;
            return true;
        }

        pub fn backspace(self: *Self) bool {
            if (self.len == 0) return false;
            const next = r4os.grapheme.previous(self.text(), self.len) catch return false;
            @memset(self.buffer[next..self.len], 0);
            self.len = next;
            self.buffer[self.len] = 0;
            return true;
        }

        pub fn clear(self: *Self) void {
            @memset(&self.buffer, 0);
            self.len = 0;
        }

        pub fn text(self: *const Self) []const u8 {
            return self.buffer[0..self.len];
        }

        pub fn z(self: *const Self) [*:0]const u8 {
            return @ptrCast(&self.buffer);
        }
    };
}

const isTextChar = @import("input_keys.zig").isTextKey;

/// Display-only tail: exact input bytes remain with the editor/launcher.
/// Existing SDK scalar-cell widths and original R4STD grapheme boundaries
/// decide clipping; neither a codepoint nor a cluster is split.
pub fn copyTailLabel(out: []u8, value: []const u8, columns: usize) []const u8 {
    @memset(out, 0);
    if (out.len == 0 or columns == 0 or !r4os.grapheme.valid(value)) return out[0..0];
    const width: i32 = @intCast(@min(columns, @as(usize, std.math.maxInt(i32) / 8)) * 8);
    if (value.len < out.len and r4os.gui.textWidth(value) <= width) {
        @memcpy(out[0..value.len], value);
        return out[0..value.len];
    }
    const dots: usize = @min(2, @min(columns, out.len - 1));
    const tail_width = width - @as(i32, @intCast(dots * 8));
    var start: usize = 0;
    while (start < value.len and (value.len - start > out.len - 1 - dots or r4os.gui.textWidth(value[start..]) > tail_width)) {
        start = r4os.grapheme.next(value, start) catch return out[0..0];
    }
    @memset(out[0..dots], '.');
    @memcpy(out[dots..][0..value.len - start], value[start..]);
    return out[0 .. dots + value.len - start];
}

test "text field appends backspaces and keeps zero termination" {
    var field = TextField(5){};

    try std.testing.expect(field.set("ABC"));
    try std.testing.expectEqualStrings("ABC", field.text());
    try std.testing.expect(field.append('D'));
    try std.testing.expect(field.append('E'));
    try std.testing.expect(!field.append('F'));
    try std.testing.expectEqualStrings("ABCDE", field.text());
    try std.testing.expectEqual(@as(u8, 0), field.buffer[field.len]);
    try std.testing.expect(field.backspace());
    try std.testing.expectEqualStrings("ABCD", field.text());
    try std.testing.expect(!field.appendText("\xc3\xa4"));
    try std.testing.expect(!field.set("abcd\xc3\xa4"));
    try std.testing.expect(!field.set("a\xff"));
    try std.testing.expectEqualStrings("ABCD", field.text());
    try std.testing.expect(field.set("A\u{308}"));
    try std.testing.expect(field.backspace());
    try std.testing.expectEqualStrings("", field.text());
}

test "text field ignores control characters" {
    var field = TextField(8){};

    try std.testing.expect(!field.append('\n'));
    try std.testing.expect(!field.append(0x1B));
    try std.testing.expect(field.append('X'));
    try std.testing.expectEqualStrings("X", field.text());
    try std.testing.expect(field.append(0x1f680));
    try std.testing.expect(field.append(0x4e2d));
    try std.testing.expectEqualStrings("X\u{1f680}\u{4e2d}", field.text());
    try std.testing.expect(!field.append(0xd800));
    try std.testing.expect(!field.append(0x110000));
    try std.testing.expect(!field.appendText("\xc3\xa4"));
    try std.testing.expect(!field.set("a\x00b"));
    try std.testing.expectEqualStrings("X\u{1f680}\u{4e2d}", field.text());
    var label: [8]u8 = undefined;
    const clipped = copyTailLabel(&label, "prefixA\u{308}\u{4e2d}", 4);
    try std.testing.expect(r4os.grapheme.valid(clipped));
    try std.testing.expectEqualStrings("..\u{4e2d}", clipped);
}
