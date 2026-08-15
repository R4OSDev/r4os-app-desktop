const std = @import("std");

pub const max_input: usize = 127;
pub const max_path: usize = 127;
pub const max_args: usize = 127;

pub const Command = struct {
    path: [max_path + 1]u8 = .{0} ** (max_path + 1),
    args: [max_args + 1]u8 = .{0} ** (max_args + 1),

    pub fn pathZ(self: *const Command) [*:0]const u8 {
        return @ptrCast(&self.path);
    }

    pub fn argsZ(self: *const Command) [*:0]const u8 {
        return @ptrCast(&self.args);
    }
};

pub fn parse(input: []const u8) ?Command {
    const trimmed = trim(input);
    if (trimmed.len == 0) return null;

    var path: []const u8 = "";
    var rest: []const u8 = "";
    if (trimmed[0] == '"') {
        const end_quote = findByte(trimmed[1..], '"') orelse return null;
        path = trimmed[1 .. 1 + end_quote];
        rest = trimmed[1 + end_quote + 1 ..];
    } else {
        const split = findSpace(trimmed) orelse trimmed.len;
        path = trimmed[0..split];
        rest = trimmed[split..];
    }

    const args = trim(rest);
    if (path.len == 0 or path.len > max_path or args.len > max_args) return null;

    var result = Command{};
    copyZ(result.path[0..], path);
    copyZ(result.args[0..], args);
    return result;
}

fn copyZ(out: []u8, value: []const u8) void {
    @memset(out, 0);
    const count = @min(value.len, out.len - 1);
    if (count > 0) @memcpy(out[0..count], value[0..count]);
    out[count] = 0;
}

fn trim(value: []const u8) []const u8 {
    var start: usize = 0;
    var end = value.len;
    while (start < end and isSpace(value[start])) : (start += 1) {}
    while (end > start and isSpace(value[end - 1])) : (end -= 1) {}
    return value[start..end];
}

fn findSpace(value: []const u8) ?usize {
    var i: usize = 0;
    while (i < value.len) : (i += 1) {
        if (isSpace(value[i])) return i;
    }
    return null;
}

fn findByte(value: []const u8, needle: u8) ?usize {
    var i: usize = 0;
    while (i < value.len) : (i += 1) {
        if (value[i] == needle) return i;
    }
    return null;
}

fn isSpace(ch: u8) bool {
    return ch == ' ' or ch == '\t' or ch == '\r' or ch == '\n';
}

test "parse command path without arguments" {
    const command = parse("C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X") orelse return error.ParseFailed;

    try std.testing.expectEqualStrings("C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X", std.mem.span(command.pathZ()));
    try std.testing.expectEqualStrings("", std.mem.span(command.argsZ()));
}

test "parse command path with arguments" {
    const command = parse("C:\\R4OS\\SOFTWARE\\TERMINAL\\SYNTH.R4X C:\\TEMP\\TADA.WAV") orelse return error.ParseFailed;

    try std.testing.expectEqualStrings("C:\\R4OS\\SOFTWARE\\TERMINAL\\SYNTH.R4X", std.mem.span(command.pathZ()));
    try std.testing.expectEqualStrings("C:\\TEMP\\TADA.WAV", std.mem.span(command.argsZ()));
}

test "parse quoted path with arguments" {
    const command = parse("\"C:\\R4OS\\SOFTWARE\\DESKTOP\\MY APP.R4X\" /v C:\\TEMP\\TADA.WAV") orelse return error.ParseFailed;

    try std.testing.expectEqualStrings("C:\\R4OS\\SOFTWARE\\DESKTOP\\MY APP.R4X", std.mem.span(command.pathZ()));
    try std.testing.expectEqualStrings("/v C:\\TEMP\\TADA.WAV", std.mem.span(command.argsZ()));
}

test "reject empty or unterminated command" {
    try std.testing.expect(parse("   ") == null);
    try std.testing.expect(parse("\"C:\\R4OS\\SOFTWARE\\DESKTOP\\BROKEN.R4X") == null);
}
