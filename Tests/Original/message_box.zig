const std = @import("std");
const model = @import("model.zig");

pub const title_buffer_len: usize = 64;
pub const text_buffer_len: usize = 160;

pub const Kind = enum(u8) {
    info,
    warning,
    @"error",
    question,
};

pub const Buttons = enum(u8) {
    ok,
    ok_cancel,
    yes_no,
};

pub const Result = enum(u8) {
    none,
    ok,
    cancel,
    yes,
    no,
};

pub const View = struct {
    kind: Kind = .info,
    buttons: Buttons = .ok,
    title: [*:0]const u8,
    text: [*:0]const u8,
    focus: model.UiTarget = .message_ok,
};

pub fn defaultFocus(buttons: Buttons) model.UiTarget {
    return switch (buttons) {
        .ok, .ok_cancel => .message_ok,
        .yes_no => .message_no,
    };
}

pub fn cancelResult(buttons: Buttons) Result {
    return switch (buttons) {
        .ok => .ok,
        .ok_cancel => .cancel,
        .yes_no => .no,
    };
}

pub fn cancelTarget(buttons: Buttons) model.UiTarget {
    return switch (buttons) {
        .ok => .message_ok,
        .ok_cancel, .yes_no => .message_no,
    };
}

pub fn targetResult(buttons: Buttons, target: model.UiTarget) Result {
    return switch (buttons) {
        .ok => switch (target) {
            .message_ok, .message_backdrop => .ok,
            else => .none,
        },
        .ok_cancel => switch (target) {
            .message_ok => .ok,
            .message_no, .message_backdrop => .cancel,
            else => .none,
        },
        .yes_no => switch (target) {
            .message_yes => .yes,
            .message_no, .message_backdrop => .no,
            else => .none,
        },
    };
}

test "message box default focus follows button set" {
    try std.testing.expectEqual(model.UiTarget.message_ok, defaultFocus(.ok));
    try std.testing.expectEqual(model.UiTarget.message_ok, defaultFocus(.ok_cancel));
    try std.testing.expectEqual(model.UiTarget.message_no, defaultFocus(.yes_no));
}

test "message box target result maps reusable dialog targets" {
    try std.testing.expectEqual(Result.ok, targetResult(.ok, .message_ok));
    try std.testing.expectEqual(Result.cancel, targetResult(.ok_cancel, .message_no));
    try std.testing.expectEqual(Result.yes, targetResult(.yes_no, .message_yes));
    try std.testing.expectEqual(Result.no, targetResult(.yes_no, .message_backdrop));
    try std.testing.expectEqual(Result.none, targetResult(.yes_no, .message_ok));
}

test "message box cancel target follows button semantics" {
    try std.testing.expectEqual(model.UiTarget.message_ok, cancelTarget(.ok));
    try std.testing.expectEqual(model.UiTarget.message_no, cancelTarget(.ok_cancel));
    try std.testing.expectEqual(model.UiTarget.message_no, cancelTarget(.yes_no));
}

test "message box cancel result follows button semantics" {
    try std.testing.expectEqual(Result.ok, cancelResult(.ok));
    try std.testing.expectEqual(Result.cancel, cancelResult(.ok_cancel));
    try std.testing.expectEqual(Result.no, cancelResult(.yes_no));
}
