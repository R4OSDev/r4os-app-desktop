const std = @import("std");

pub const background_prefix = "R4OS_APPEARANCE_BG=";
pub const reload = "R4OS_APPEARANCE_RELOAD=1";

pub const Signal = union(enum) {
    background: u32,
    reload,
};

pub fn parse(text: []const u8) ?Signal {
    if (std.mem.eql(u8, text, reload)) return .reload;
    if (parseBackground(text)) |color| return .{ .background = color };
    return null;
}

pub fn parseBackground(text: []const u8) ?u32 {
    if (text.len != background_prefix.len + 6) return null;
    if (!std.mem.eql(u8, text[0..background_prefix.len], background_prefix)) return null;
    var color: u32 = 0;
    for (text[background_prefix.len..]) |ch| {
        color = (color << 4) | (hexValue(ch) orelse return null);
    }
    return color;
}

fn hexValue(ch: u8) ?u32 {
    if (ch >= '0' and ch <= '9') return ch - '0';
    if (ch >= 'A' and ch <= 'F') return ch - 'A' + 10;
    if (ch >= 'a' and ch <= 'f') return ch - 'a' + 10;
    return null;
}

test "Appearance background signal is exact" {
    try std.testing.expectEqual(@as(?u32, 0x008080), parseBackground("R4OS_APPEARANCE_BG=008080"));
    try std.testing.expectEqual(@as(?u32, 0xA1B2C3), parseBackground("R4OS_APPEARANCE_BG=a1B2c3"));
    try std.testing.expectEqual(@as(?u32, null), parseBackground("R4OS_APPEARANCE_BG=00808"));
    try std.testing.expectEqual(@as(?u32, null), parseBackground("R4OS_APPEARANCE_BG=008080X"));
    try std.testing.expectEqual(@as(?u32, null), parseBackground("R4OS_APPEARANCE_BG=00G080"));
}

test "Appearance reload signal is exact" {
    const signal = parse(reload) orelse return error.TestUnexpectedResult;
    switch (signal) {
        .reload => {},
        else => return error.TestUnexpectedResult,
    }
    try std.testing.expectEqual(@as(?Signal, null), parse("R4OS_APPEARANCE_RELOAD=0"));
}
