const std = @import("std");

pub const selected_background: u32 = 0x000080;
pub const selected_text: u32 = 0xFFFFFF;

pub const LabelColors = struct {
    foreground: u32,
    background: u32,
};

pub fn desktopIconLabel(active: bool, desktop_background: u32, configured_text: u32) LabelColors {
    if (active) {
        return .{
            .foreground = selected_text,
            .background = selected_background,
        };
    }
    return .{
        .foreground = configured_text & 0x00FF_FFFF,
        .background = desktop_background & 0x00FF_FFFF,
    };
}

test "normal desktop icon uses configured text and desktop background" {
    const colors = desktopIconLabel(false, 0x123456, 0xABCDEF);
    try std.testing.expectEqual(@as(u32, 0xABCDEF), colors.foreground);
    try std.testing.expectEqual(@as(u32, 0x123456), colors.background);
}

test "active desktop icon always keeps white on blue contrast" {
    const colors = desktopIconLabel(true, 0x123456, 0x000000);
    try std.testing.expectEqual(selected_text, colors.foreground);
    try std.testing.expectEqual(selected_background, colors.background);
}
