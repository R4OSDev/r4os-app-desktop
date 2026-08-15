const std = @import("std");

pub fn TextField(comptime capacity: usize) type {
    return struct {
        const Self = @This();

        buffer: [capacity + 1]u8 = .{0} ** (capacity + 1),
        len: usize = 0,

        pub fn set(self: *Self, value: []const u8) void {
            @memset(self.buffer[0..], 0);
            self.len = @min(value.len, capacity);
            if (self.len > 0) @memcpy(self.buffer[0..self.len], value[0..self.len]);
            self.buffer[self.len] = 0;
        }

        pub fn append(self: *Self, ch: u8) bool {
            if (self.len >= capacity or !isTextChar(ch)) return false;
            self.buffer[self.len] = ch;
            self.len += 1;
            self.buffer[self.len] = 0;
            return true;
        }

        pub fn backspace(self: *Self) bool {
            if (self.len == 0) return false;
            self.len -= 1;
            self.buffer[self.len] = 0;
            return true;
        }

        pub fn clear(self: *Self) void {
            self.set("");
        }

        pub fn text(self: *const Self) []const u8 {
            return self.buffer[0..self.len];
        }

        pub fn z(self: *const Self) [*:0]const u8 {
            return @ptrCast(&self.buffer);
        }
    };
}

fn isTextChar(ch: u8) bool {
    return ch >= 0x20 and ch < 0x7F;
}

test "text field appends backspaces and keeps zero termination" {
    var field = TextField(5){};

    field.set("ABC");
    try std.testing.expectEqualStrings("ABC", field.text());
    try std.testing.expect(field.append('D'));
    try std.testing.expect(field.append('E'));
    try std.testing.expect(!field.append('F'));
    try std.testing.expectEqualStrings("ABCDE", field.text());
    try std.testing.expectEqual(@as(u8, 0), field.buffer[field.len]);
    try std.testing.expect(field.backspace());
    try std.testing.expectEqualStrings("ABCD", field.text());
}

test "text field ignores control characters" {
    var field = TextField(8){};

    try std.testing.expect(!field.append('\n'));
    try std.testing.expect(!field.append(0x1B));
    try std.testing.expect(field.append('X'));
    try std.testing.expectEqualStrings("X", field.text());
}
