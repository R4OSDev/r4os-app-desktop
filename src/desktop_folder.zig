const std = @import("std");
const r4os = @import("r4os");
const r4std = @import("r4std");

pub const default_dir = "C:\\R4OS\\DESKTOP";
pub const default_dir_display = "C:\\R4OS\\DESKTOP\\";
pub const max_link_path = 159;
pub const max_link_bytes = 768;

pub const DefaultLink = enum(u8) {
    terminal,
    notepad,
    r4synth,
    devices,
    memview,
    computer,
};

pub const default_links = [_]DefaultLink{
    .terminal,
    .notepad,
    .r4synth,
    .devices,
    .memview,
    .computer,
};

pub fn fileName(spec: DefaultLink) []const u8 {
    return switch (spec) {
        .terminal => "TERMINAL.LNK",
        .notepad => "NOTEPAD.LNK",
        .r4synth => "R4SYNTH.LNK",
        .devices => "DEVICES.LNK",
        .memview => "MEMVIEW.LNK",
        .computer => "COMPUTER.LNK",
    };
}

pub fn title(spec: DefaultLink) []const u8 {
    return switch (spec) {
        .terminal => "Terminal",
        .notepad => "Notepad",
        .r4synth => "R4Synth",
        .devices => "Devices",
        .memview => "MemView",
        .computer => "Computer",
    };
}

pub fn target(spec: DefaultLink) []const u8 {
    return switch (spec) {
        .terminal => "C:\\R4OS\\SOFTWARE\\TERMINAL\\TERMINAL.R4X",
        .notepad => "C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X",
        .r4synth => "C:\\R4OS\\SOFTWARE\\TERMINAL\\SYNTH.R4X",
        .devices => "C:\\R4OS\\SOFTWARE\\DESKTOP\\DEVMGR.R4X",
        .memview => "C:\\R4OS\\SOFTWARE\\DESKTOP\\MEMVIEW.R4X",
        .computer => "C:\\R4OS\\SOFTWARE\\DESKTOP\\EXPLORER.R4X",
    };
}

pub fn args(spec: DefaultLink) []const u8 {
    return switch (spec) {
        .r4synth => "C:\\TEMP\\TADA.WAV",
        else => "",
    };
}

pub fn policy(spec: DefaultLink) r4os.abi.LaunchPolicy {
    return switch (spec) {
        .terminal, .r4synth => .console,
        .notepad, .devices, .memview, .computer => .gui,
    };
}

pub fn icon(spec: DefaultLink) []const u8 {
    return switch (spec) {
        .terminal => "C:\\R4OS\\Media\\Icons\\Terminal.ico",
        .notepad => "C:\\R4OS\\Media\\Icons\\Notepad.ico",
        .r4synth => "C:\\R4OS\\Media\\Icons\\Synth.ico",
        .devices, .memview => "C:\\R4OS\\Media\\Icons\\Config.ico",
        .computer => "C:\\R4OS\\Media\\Icons\\Computer.ico",
    };
}

pub fn defaultLinkPath(out: []u8, file_name: []const u8) ?[]const u8 {
    if (out.len == 0) return null;
    @memset(out, 0);
    var pos: usize = 0;
    if (!appendSlice(out, &pos, default_dir)) return null;
    if (!appendByte(out, &pos, '\\')) return null;
    if (!appendSlice(out, &pos, file_name)) return null;
    out[pos] = 0;
    return out[0..pos];
}

pub fn writeDefaultLink(spec: DefaultLink, out: []u8) r4std.shortcut.Error![]const u8 {
    var link = try r4std.shortcut.Shortcut.init(target(spec));
    try link.setTitle(title(spec));
    try link.setArgs(args(spec));
    link.setPolicy(policy(spec));
    try link.setIcon(icon(spec));
    return link.writeTo(out);
}

pub fn linkMatchesSpec(link: *const r4std.shortcut.Shortcut, spec: DefaultLink) bool {
    return std.mem.eql(u8, link.titleText(), title(spec)) and
        std.mem.eql(u8, link.targetText(), target(spec)) and
        std.mem.eql(u8, link.argsText(), args(spec)) and
        link.policy == policy(spec) and
        std.mem.eql(u8, link.iconText(), icon(spec));
}

fn appendSlice(out: []u8, pos: *usize, value: []const u8) bool {
    if (out.len == 0 or pos.* > out.len - 1 or value.len > out.len - 1 - pos.*) return false;
    if (value.len > 0) @memcpy(out[pos.* .. pos.* + value.len], value);
    pos.* += value.len;
    return true;
}

fn appendByte(out: []u8, pos: *usize, value: u8) bool {
    if (out.len == 0 or pos.* >= out.len - 1) return false;
    out[pos.*] = value;
    pos.* += 1;
    return true;
}

test "default desktop folder links are canonical R4LNK files" {
    try @import("r4std_test").ensure();
    try std.testing.expectEqual(@as(usize, 6), default_links.len);
    inline for (default_links) |spec| {
        var bytes: [max_link_bytes]u8 = .{0} ** max_link_bytes;
        const link_bytes = try writeDefaultLink(spec, bytes[0..]);
        try std.testing.expect(std.mem.indexOf(u8, link_bytes, "SCHEMA=R4LNK") != null);
        try std.testing.expect(std.mem.indexOf(u8, link_bytes, "TARGET=") != null);
        const parsed = try r4std.shortcut.parse(link_bytes);
        try std.testing.expect(linkMatchesSpec(&parsed, spec));
    }
}

test "default desktop folder paths stay under canonical desktop directory" {
    inline for (default_links) |spec| {
        var path: [max_link_path + 1]u8 = .{0} ** (max_link_path + 1);
        const text = defaultLinkPath(path[0..], fileName(spec)).?;
        try std.testing.expect(std.mem.startsWith(u8, text, default_dir ++ "\\"));
        try std.testing.expect(std.mem.endsWith(u8, text, ".LNK"));
    }
}
