const std = @import("std");
const r4os = @import("r4os");
const r4std = @import("r4std");
const settings = r4std.settings;

pub const max_font_path: usize = 63;
pub const max_wallpaper_path: usize = r4os.path.file_path_max;
pub const default_desktop_bg: u32 = 0x008080;
pub const default_desktop_icon_text: u32 = 0xFFFFFF;

pub const Config = struct {
    ui_font_path: [max_font_path + 1]u8 = withDefaultFontPath(),
    ui_font_size: u8 = 8,
    terminal_font_path: [max_font_path + 1]u8 = withDefaultPath(),
    terminal_font_size: u8 = 8,
    terminal_codepage: u16 = 437,
    desktop_bg: u32 = default_desktop_bg,
    desktop_icon_text: u32 = default_desktop_icon_text,
    wallpaper_path: [max_wallpaper_path + 1]u8 = .{0} ** (max_wallpaper_path + 1),
    taskbar_clock: bool = true,

    pub fn uiFontPath(self: *const Config) [*:0]const u8 {
        return @ptrCast(&self.ui_font_path);
    }

    pub fn terminalFontPath(self: *const Config) [*:0]const u8 {
        return @ptrCast(&self.terminal_font_path);
    }

    pub fn wallpaperPath(self: *const Config) [*:0]const u8 {
        return @ptrCast(&self.wallpaper_path);
    }

    pub fn loadFromBytes(self: *Config, bytes: []const u8) bool {
        var loaded = false;
        var iter = settings.EntryIterator.init(bytes);
        while (iter.next()) |entry| {
            if (parseEntry(self, entry)) loaded = true;
        }
        return loaded;
    }

    pub fn writeTo(self: *const Config, out: []u8) []const u8 {
        var writer = settings.Writer.init(out);
        writer.writeHeader("DESKTOP");
        writer.writePairRgb24("DESKTOP_BG", self.desktop_bg);
        writer.writePairRgb24("DESKTOP_ICON_TEXT", self.desktop_icon_text);
        writer.writePair("WALLPAPER", if (self.wallpaper_path[0] == 0) "NONE" else std.mem.span(self.wallpaperPath()));
        writer.writePair("WALLPAPER_MODE", "CENTER");
        writer.writePairBool("TASKBAR_CLOCK", self.taskbar_clock);
        writer.writePair("UI_FONT", std.mem.span(self.uiFontPath()));
        writer.writePairU32("UI_FONT_SIZE", if (self.ui_font_size == 16) 16 else 8);
        writer.writePair("TERMINAL_FONT", std.mem.span(self.terminalFontPath()));
        writer.writePairU32("TERMINAL_FONT_SIZE", if (self.terminal_font_size == 16) 16 else 8);
        writer.writePairU32("TERMINAL_CODEPAGE", if (self.terminal_codepage == 437) 437 else 437);
        return writer.bytes();
    }
};

fn parseEntry(config: *Config, entry: settings.Entry) bool {
    const key = entry.key;
    const value = entry.value;
    if (value.len == 0) return false;

    if (settings.equalsKey(key, "UI_FONT") or settings.equalsKey(key, "DESKTOP_FONT")) {
        copyZ(config.ui_font_path[0..], value);
        return true;
    }
    if (settings.equalsKey(key, "UI_FONT_SIZE") or settings.equalsKey(key, "DESKTOP_FONT_SIZE")) {
        if (settings.equalsKey(value, "8") or settings.equalsKey(value, "8X8")) {
            config.ui_font_size = 8;
            return true;
        }
        if (settings.equalsKey(value, "16") or settings.equalsKey(value, "8X16")) {
            config.ui_font_size = 16;
            return true;
        }
        return false;
    }
    if (settings.equalsKey(key, "TERMINAL_FONT")) {
        copyZ(config.terminal_font_path[0..], value);
        return true;
    }
    if (settings.equalsKey(key, "TERMINAL_FONT_SIZE")) {
        if (settings.equalsKey(value, "8") or settings.equalsKey(value, "8X8")) {
            config.terminal_font_size = 8;
            return true;
        }
        if (settings.equalsKey(value, "16") or settings.equalsKey(value, "8X16")) {
            config.terminal_font_size = 16;
            return true;
        }
        return false;
    }
    if (settings.equalsKey(key, "TERMINAL_CODEPAGE")) {
        if (settings.equalsKey(value, "437") or settings.equalsKey(value, "CP437")) {
            config.terminal_codepage = 437;
            return true;
        }
        return false;
    }
    if (settings.equalsKey(key, "DESKTOP_BG") or settings.equalsKey(key, "BACKGROUND") or settings.equalsKey(key, "BACKGROUND_COLOR")) {
        if (settings.parseRgb24(value)) |color| {
            config.desktop_bg = color;
            return true;
        }
        return false;
    }
    if (settings.equalsKey(key, "DESKTOP_ICON_TEXT")) {
        if (settings.parseRgb24(value)) |color| {
            config.desktop_icon_text = color;
            return true;
        }
        return false;
    }
    if (settings.equalsKey(key, "WALLPAPER")) {
        if (settings.equalsKey(value, "NONE")) {
            @memset(config.wallpaper_path[0..], 0);
            return true;
        }
        if (value.len > max_wallpaper_path) return false;
        const parsed = r4os.path.AbsoluteFilePath.parse(value) catch return false;
        copyZ(config.wallpaper_path[0..], parsed.bytes());
        return true;
    }
    if (settings.equalsKey(key, "WALLPAPER_MODE")) {
        return settings.equalsKey(value, "CENTER");
    }
    if (settings.equalsKey(key, "TASKBAR_CLOCK") or settings.equalsKey(key, "CLOCK")) {
        if (settings.parseBool(value)) |enabled| {
            config.taskbar_clock = enabled;
            return true;
        }
        return false;
    }
    return false;
}

fn withDefaultFontPath() [max_font_path + 1]u8 {
    var out: [max_font_path + 1]u8 = .{0} ** (max_font_path + 1);
    copyZ(out[0..], "C:\\R4OS\\FONTS\\TERMINAL8.R4F");
    return out;
}

fn withDefaultPath() [max_font_path + 1]u8 {
    return withDefaultFontPath();
}

fn copyZ(out: []u8, value: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const count = @min(value.len, out.len - 1);
    if (count > 0) @memcpy(out[0..count], value[0..count]);
    out[count] = 0;
}

test "desktop config parses terminal keys" {
    try @import("r4std_test").ensure();
    var config = Config{};
    const ok = config.loadFromBytes(
        \\# Desktop
        \\TERMINAL_FONT=C:\R4OS\FONTS\TERMINAL16.R4F
        \\TERMINAL_FONT_SIZE=16
        \\TERMINAL_CODEPAGE=CP437
    );

    try std.testing.expect(ok);
    try std.testing.expectEqualStrings("C:\\R4OS\\FONTS\\TERMINAL16.R4F", std.mem.span(config.terminalFontPath()));
    try std.testing.expectEqual(@as(u8, 16), config.terminal_font_size);
    try std.testing.expectEqual(@as(u16, 437), config.terminal_codepage);
}

test "desktop config parses appearance keys" {
    try @import("r4std_test").ensure();
    var config = Config{};
    const ok = config.loadFromBytes(
        \\DESKTOP_BG=#004080
        \\DESKTOP_ICON_TEXT=FFD700
        \\WALLPAPER=C:\WALLPAPER.BMP
        \\WALLPAPER_MODE=CENTER
        \\TASKBAR_CLOCK=off
    );

    try std.testing.expect(ok);
    try std.testing.expectEqual(@as(u32, 0x004080), config.desktop_bg);
    try std.testing.expectEqual(@as(u32, 0xFFD700), config.desktop_icon_text);
    try std.testing.expectEqualStrings("C:\\WALLPAPER.BMP", std.mem.span(config.wallpaperPath()));
    try std.testing.expect(!config.taskbar_clock);
}

test "desktop config serializes safe defaults" {
    try @import("r4std_test").ensure();
    var config = Config{};
    var out: [384]u8 = .{0} ** 384;
    const bytes = config.writeTo(out[0..]);
    try std.testing.expect(std.mem.indexOf(u8, bytes, "DESKTOP_BG=008080") != null);
    try std.testing.expect(std.mem.indexOf(u8, bytes, "DESKTOP_ICON_TEXT=FFFFFF") != null);
    try std.testing.expect(std.mem.indexOf(u8, bytes, "WALLPAPER=NONE") != null);
    try std.testing.expect(std.mem.indexOf(u8, bytes, "WALLPAPER_MODE=CENTER") != null);
    try std.testing.expect(std.mem.indexOf(u8, bytes, "TASKBAR_CLOCK=ON") != null);
    try std.testing.expect(std.mem.indexOf(u8, bytes, "UI_FONT=C:\\R4OS\\FONTS\\TERMINAL8.R4F") != null);
    try std.testing.expect(std.mem.indexOf(u8, bytes, "UI_FONT_SIZE=8") != null);
    try std.testing.expect(std.mem.indexOf(u8, bytes, "TERMINAL_FONT=C:\\R4OS\\FONTS\\TERMINAL8.R4F") != null);
}

test "desktop config keeps white icon text for missing and invalid values" {
    try @import("r4std_test").ensure();
    var missing = Config{};
    try std.testing.expect(missing.loadFromBytes("DESKTOP_BG=000080\r\n"));
    try std.testing.expectEqual(default_desktop_icon_text, missing.desktop_icon_text);

    var invalid = Config{};
    try std.testing.expect(!invalid.loadFromBytes("DESKTOP_ICON_TEXT=GGGGGG\r\n"));
    try std.testing.expectEqual(default_desktop_icon_text, invalid.desktop_icon_text);
}

test "desktop config rejects unsupported wallpaper values and mode" {
    try @import("r4std_test").ensure();
    var config = Config{};
    try std.testing.expect(!config.loadFromBytes("WALLPAPER=RELATIVE.BMP\r\n"));
    try std.testing.expectEqualStrings("", std.mem.span(config.wallpaperPath()));
    try std.testing.expect(!config.loadFromBytes("WALLPAPER_MODE=STRETCH\r\n"));
}

test "desktop config parses ui font keys" {
    try @import("r4std_test").ensure();
    var config = Config{};
    const ok = config.loadFromBytes(
        \\UI_FONT=C:\R4OS\FONTS\TERMINAL16.R4F
        \\UI_FONT_SIZE=16
    );

    try std.testing.expect(ok);
    try std.testing.expectEqualStrings("C:\\R4OS\\FONTS\\TERMINAL16.R4F", std.mem.span(config.uiFontPath()));
    try std.testing.expectEqual(@as(u8, 16), config.ui_font_size);
}

test "desktop config rejects legacy terminal key aliases" {
    try @import("r4std_test").ensure();
    var config = Config{};
    const ok = config.loadFromBytes(
        \\FONT=C:\R4OS\FONTS\TERMINAL8.R4F
        \\FONT_SIZE=8
        \\CODEPAGE=437
    );

    try std.testing.expect(!ok);
    try std.testing.expectEqualStrings("C:\\R4OS\\FONTS\\TERMINAL8.R4F", std.mem.span(config.terminalFontPath()));
    try std.testing.expectEqual(@as(u8, 8), config.terminal_font_size);
    try std.testing.expectEqual(@as(u16, 437), config.terminal_codepage);
}
