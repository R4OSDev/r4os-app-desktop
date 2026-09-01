const std = @import("std");
const r4os = @import("r4os");

pub fn valid(input: r4os.abi.PhysicalKeyEvent, last_sequence: u64) bool {
    return input.magic == r4os.abi.physical_key_magic and
        input.version == r4os.abi.physical_key_version and
        input.size == @sizeOf(r4os.abi.PhysicalKeyEvent) and
        input.sequence != 0 and
        input.sequence > last_sequence and
        (input.kind == r4os.abi.physical_key_kind_down or
            input.kind == r4os.abi.physical_key_kind_up or
            input.kind == r4os.abi.physical_key_kind_reset);
}

pub fn toGuiEvent(input: r4os.abi.PhysicalKeyEvent, window_id: i32) ?r4os.abi.GuiEvent {
    const kind: r4os.abi.GuiEventKind = if (input.kind == r4os.abi.physical_key_kind_down)
        .physical_key_down
    else if (input.kind == r4os.abi.physical_key_kind_up)
        .physical_key_up
    else if (input.kind == r4os.abi.physical_key_kind_reset)
        .physical_key_reset
    else
        return null;
    return .{
        .kind = @intFromEnum(kind),
        .window_id = window_id,
        .key = input.key,
        .modifiers = input.modifiers,
        .buttons = input.flags,
        .tick = input.tick,
    };
}

test "physical keypad and right control fields reach GUI unchanged" {
    const usages = [_]u32{
        r4os.abi.physical_key_usage_keypad_2,
        r4os.abi.physical_key_usage_keypad_4,
        r4os.abi.physical_key_usage_keypad_6,
        r4os.abi.physical_key_usage_keypad_7,
        r4os.abi.physical_key_usage_keypad_8,
        r4os.abi.physical_key_usage_keypad_9,
        r4os.abi.physical_key_usage_right_control,
    };
    for (usages, 0..) |usage, index| {
        const input = r4os.abi.PhysicalKeyEvent{
            .kind = r4os.abi.physical_key_kind_down,
            .key = usage,
            .modifiers = if (usage == r4os.abi.physical_key_usage_right_control)
                r4os.abi.physical_key_modifier_right_control
            else
                0,
            .flags = r4os.abi.physical_key_flag_repeat,
            .sequence = index + 1,
            .tick = 100 + index,
        };
        try std.testing.expect(valid(input, index));
        const event = toGuiEvent(input, 7).?;
        try std.testing.expectEqual(@intFromEnum(r4os.abi.GuiEventKind.physical_key_down), event.kind);
        try std.testing.expectEqual(usage, event.key);
        try std.testing.expectEqual(input.modifiers, event.modifiers);
        try std.testing.expectEqual(input.flags, event.buttons);
        try std.testing.expectEqual(input.tick, event.tick);
    }
}

test "validation and conversion preserve transitions while rejecting malformed order" {
    var input = r4os.abi.PhysicalKeyEvent{
        .kind = r4os.abi.physical_key_kind_up,
        .key = r4os.abi.physical_key_usage_keypad_8,
        .sequence = 9,
        .tick = 55,
    };
    try std.testing.expect(valid(input, 8));
    try std.testing.expectEqual(
        @intFromEnum(r4os.abi.GuiEventKind.physical_key_up),
        toGuiEvent(input, 2).?.kind,
    );
    try std.testing.expect(!valid(input, 9));
    input.magic = 0;
    try std.testing.expect(!valid(input, 0));
    input.magic = r4os.abi.physical_key_magic;
    input.kind = 99;
    try std.testing.expect(!valid(input, 0));
    try std.testing.expect(toGuiEvent(input, 0) == null);
}

test "keypad remains distinct from numeric row and navigation usages" {
    try std.testing.expect(r4os.abi.physical_key_usage_keypad_8 != 0x25);
    try std.testing.expect(r4os.abi.physical_key_usage_keypad_8 != r4os.abi.physical_key_usage_up);
    try std.testing.expect(r4os.abi.physical_key_usage_right_control != r4os.abi.physical_key_usage_left_control);
}
