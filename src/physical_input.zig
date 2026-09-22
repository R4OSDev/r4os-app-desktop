const std = @import("std");
const r4os = @import("r4os");

/// Physical samples remain independent of the cursor moved by RDP. A poll
/// of unchanged hardware must neither undo a remote click nor wake the panel.
pub const MouseActivity = struct {
    last: ?r4os.abi.Mouse = null,
    motion: ?[2]u32 = null,
    pub fn changed(self: *MouseActivity, mouse: r4os.abi.Mouse, totals: ?[2]u32) bool {
        const prior = self.last;
        const previous_motion = self.motion;
        self.last = mouse; self.motion = totals;
        const old = prior orelse return mouse.buttons != 0 or mouse.wheel != 0;
        const fresh = mouse.packets != old.packets;
        const moved = if (totals != null and previous_motion != null)
            !std.meta.eql(totals.?, previous_motion.?) else
            fresh and (mouse.x != old.x or mouse.y != old.y or mouse.dx != 0 or mouse.dy != 0);
        return moved or mouse.buttons != old.buttons or (fresh and mouse.wheel != 0);
    }
};

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

test "unchanged physical mouse cannot undo remote input or renew activity" {
    var activity: MouseActivity = .{};
    var mouse = std.mem.zeroes(r4os.abi.Mouse);
    mouse.x = 640; mouse.y = 360; mouse.present = 1; mouse.packets = 99;
    try std.testing.expect(!activity.changed(mouse, .{ 10, 20 }));
    // Remote input does not mutate this physical-source baseline.
    for (0..4) |_| try std.testing.expect(!activity.changed(mouse, .{ 10, 20 }));
    mouse.packets += 1; // Zero-motion hardware report is still idle.
    try std.testing.expect(!activity.changed(mouse, .{ 10, 20 }));
    mouse.buttons = 1; try std.testing.expect(activity.changed(mouse, .{ 10, 20 }));
    try std.testing.expect(!activity.changed(mouse, .{ 10, 20 }));
    mouse.buttons = 0; try std.testing.expect(activity.changed(mouse, .{ 10, 20 }));
    mouse.packets = 0; mouse.wheel = 1; try std.testing.expect(activity.changed(mouse, .{ 10, 20 }));
    mouse.wheel = 0; try std.testing.expect(!activity.changed(mouse, .{ 10, 20 }));
    // Cumulative motion survives another consumer draining relative deltas.
    try std.testing.expect(activity.changed(mouse, .{ 11, 20 }));
    mouse.packets = 1; mouse.dx = 2; try std.testing.expect(activity.changed(mouse, null));
    mouse.dx = 0; try std.testing.expect(!activity.changed(mouse, null));
}
