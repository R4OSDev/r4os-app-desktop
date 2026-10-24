//! Three independently aged CPU images. Damage survives failed preparation;
//! only accepted presentation makes that image a complete current scene.
const std = @import("std");
const Rect = @import("surface.zig").Rect;
pub const History = struct {
    valid: [3]bool = @splat(false),
    missing: [3]?Rect = @splat(null),
    pub fn invalidate(self: *History, damage: Rect) void {
        if (damage.isEmpty()) return;
        for (&self.missing) |*pending| pending.* = if (pending.*) |old| old.merged(damage) else damage;
    }
    pub fn required(self: *const History, index: usize, bounds: Rect) Rect {
        if (!self.valid[index]) return bounds;
        return self.missing[index] orelse .{ .x = bounds.x, .y = bounds.y, .w = 0, .h = 0 };
    }
    pub fn accepted(self: *History, index: usize) void {
        self.valid[index] = true;
        self.missing[index] = null;
    }
    pub fn rejected(self: *History, index: usize) void {
        self.valid[index] = false;
    }
};

test "CPU output alternating images preserve old damage and rejected writes" {
    const t = std.testing;
    const bounds: Rect = .{ .x = -4, .y = 3, .w = 32, .h = 24 };
    var history: History = .{};
    var images: [3][32 * 24]u32 = @splat(@splat(0xdeadbeef));
    var reference: [32 * 24]u32 = @splat(0);
    var partial: usize = 0;
    for (0..100) |generation| {
        const index = generation % 3;
        const x = generation % 8; const y = generation % 6;
        const damage: Rect = .{ .x = @as(i32, @intCast(x)) - 4, .y = @as(i32, @intCast(y)) + 3, .w = 2, .h = 2 };
        for (y..y + 2) |row| @memset(reference[row * 32 + x ..][0..2], @intCast(generation + 1));
        history.invalidate(damage);
        const repair = history.required(index, bounds);
        if (repair.w * repair.h < bounds.w * bounds.h) partial += 1;
        for (@intCast(repair.y - bounds.y)..@intCast(repair.bottom() - bounds.y)) |row| {
            const start = row * 32 + @as(usize, @intCast(repair.x - bounds.x));
            @memcpy(images[index][start..][0..@intCast(repair.w)], reference[start..][0..@intCast(repair.w)]);
        }
        try t.expectEqualSlices(u32, &reference, &images[index]);
        if (generation == 20) {
            images[index][500] = 0xdeadbeef;
            history.rejected(index); // Partial color failure / discarded frame.
        } else history.accepted(index);
        if (generation == 60) history = .{}; // Profile/viewport/scale/rotation.
    }
    try t.expect(partial > 80);
    var other: History = .{};
    try t.expectEqual(bounds, other.required(0, bounds));
    try t.expect(history.valid[0]); // Independent output did not reset this one.
}
