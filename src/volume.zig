const std = @import("std");
const surface = @import("surface.zig");

pub const icon_w: i32 = 30;
pub const icon_gap: i32 = 4;
pub const popup_w: i32 = 224;
pub const popup_h: i32 = 116;
pub const popup_gap: i32 = 6;
pub const track_inset_x: i32 = 20;
pub const track_y: i32 = 45;
pub const track_h: i32 = 14;
pub const mute_x: i32 = 16;
pub const mute_y: i32 = 78;
pub const mute_w: i32 = 92;
pub const mute_h: i32 = 24;
pub const percent_step: u8 = 5;

pub const View = struct {
    installed: bool = false,
    reachable: bool = false,
    muted: bool = false,
    percent: u8 = 100,
    popup_open: bool = false,
};

pub const IconState = enum {
    hidden,
    unavailable,
    muted,
    quiet,
    normal,
    loud,
};

pub fn iconState(view: View) IconState {
    if (!view.installed) return .hidden;
    if (!view.reachable) return .unavailable;
    if (view.muted or view.percent == 0) return .muted;
    if (view.percent <= 25) return .quiet;
    if (view.percent <= 70) return .normal;
    return .loud;
}

pub fn iconRect(right: i32, top: i32, height: i32) surface.Rect {
    return .{ .x = right - icon_w, .y = top, .w = icon_w, .h = height };
}

pub fn popupRect(icon: surface.Rect, work: surface.Rect) surface.Rect {
    const desired = surface.Rect{
        .x = icon.x + @divTrunc(icon.w - popup_w, 2),
        .y = icon.y - popup_h - popup_gap,
        .w = popup_w,
        .h = popup_h,
    };
    return desired.clampInside(work);
}

pub fn trackRect(popup: surface.Rect) surface.Rect {
    return .{ .x = popup.x + track_inset_x, .y = popup.y + track_y, .w = popup.w - track_inset_x * 2, .h = track_h };
}

pub fn muteRect(popup: surface.Rect) surface.Rect {
    return .{ .x = popup.x + mute_x, .y = popup.y + mute_y, .w = mute_w, .h = mute_h };
}

pub fn percentAtX(track: surface.Rect, x: i32) u8 {
    if (track.w <= 1 or x <= track.x) return 0;
    if (x >= track.right() - 1) return 100;
    const numerator = @as(i64, x - track.x) * 100 + @divTrunc(track.w - 1, 2);
    return @intCast(@divTrunc(numerator, track.w - 1));
}

/// Quadratic amplitude curve: the UI remains useful at quiet settings while
/// AUDSVC still receives a monotonic unsigned 16.16 gain.
pub fn gainForPercent(percent_value: u8) u32 {
    const percent: u64 = @min(percent_value, 100);
    return @intCast((percent * percent * 0x1_0000 + 5000) / 10_000);
}

pub fn percentForGain(gain: u32) u8 {
    var best: u8 = 0;
    var best_distance: u64 = std.math.maxInt(u64);
    var candidate: u16 = 0;
    while (candidate <= 100) : (candidate += 1) {
        const mapped = gainForPercent(@intCast(candidate));
        const distance = if (mapped >= gain) @as(u64, mapped - gain) else @as(u64, gain - mapped);
        if (distance < best_distance) {
            best_distance = distance;
            best = @intCast(candidate);
        }
    }
    return best;
}

pub fn stepped(percent: u8, direction: i32) u8 {
    if (direction > 0) return @min(@as(u8, 100), percent +| percent_step);
    if (direction < 0) return percent -| percent_step;
    return percent;
}

test "perceptual curve is monotonic with stable UI round trips" {
    var previous: u32 = 0;
    var percent: u16 = 0;
    while (percent <= 100) : (percent += 1) {
        const gain = gainForPercent(@intCast(percent));
        try std.testing.expect(gain >= previous);
        const round_trip = percentForGain(gain);
        const difference = if (round_trip >= percent) round_trip - percent else percent - round_trip;
        try std.testing.expect(difference <= 1);
        previous = gain;
    }
    try std.testing.expectEqual(@as(u32, 0), gainForPercent(0));
    try std.testing.expectEqual(@as(u32, 0x1_0000), gainForPercent(100));
}

test "popup remains in the work area and keeps stable controls" {
    const work = surface.Rect{ .x = 0, .y = 0, .w = 640, .h = 440 };
    const icon = iconRect(540, 440, 28);
    const popup = popupRect(icon, work);
    try std.testing.expect(popup.x >= work.x and popup.right() <= work.right());
    try std.testing.expect(popup.y >= work.y and popup.bottom() <= work.bottom());
    try std.testing.expect(trackRect(popup).contains(trackRect(popup).x, trackRect(popup).y));
    try std.testing.expect(muteRect(popup).contains(muteRect(popup).x, muteRect(popup).y));
    try std.testing.expectEqual(@as(u8, 0), percentAtX(trackRect(popup), trackRect(popup).x));
    try std.testing.expectEqual(@as(u8, 100), percentAtX(trackRect(popup), trackRect(popup).right() - 1));
}

test "icon states cover unavailable mute and audible ranges" {
    try std.testing.expectEqual(IconState.hidden, iconState(.{}));
    try std.testing.expectEqual(IconState.unavailable, iconState(.{ .installed = true }));
    try std.testing.expectEqual(IconState.muted, iconState(.{ .installed = true, .reachable = true, .muted = true }));
    try std.testing.expectEqual(IconState.quiet, iconState(.{ .installed = true, .reachable = true, .percent = 20 }));
    try std.testing.expectEqual(IconState.normal, iconState(.{ .installed = true, .reachable = true, .percent = 50 }));
    try std.testing.expectEqual(IconState.loud, iconState(.{ .installed = true, .reachable = true, .percent = 90 }));
}
