//! Bounded sweep of rounded scissor edges. Emits disjoint active tile blocks
//! with exact ordered command candidates, without visiting empty screen tiles.
const std = @import("std");
const Rect = @import("surface.zig").Rect;
pub const capacity = 512;
pub const side = 64;
pub const Mask = std.StaticBitSet(capacity);
const Edge = struct { at: i32, command: u16, enter: bool };
pub const Block = struct { x: i32, y: i32, right: i32, bottom: i32, candidates: Mask };
pub const Index = struct {
    horizontal: [capacity * 2]Edge = undefined,
    vertical: [capacity * 2]Edge = undefined,
    count: usize = 0,
    row: usize = 0,
    column: usize = 0,
    top: i32 = 0,
    bottom: i32 = 0,
    rows: Mask = Mask.initEmpty(),
    active: Mask = Mask.initEmpty(),
    fn less(_: void, left: Edge, right: Edge) bool { return left.at < right.at; }
    pub fn init(self: *Index, commands: anytype, screen: Rect) void {
        std.debug.assert(commands.len <= capacity);
        self.count = commands.len * 2; self.row = 0; self.column = self.count;
        self.rows = Mask.initEmpty(); self.active = Mask.initEmpty();
        for (commands, 0..) |command, i| {
            const rect = command.scissor;
            // paint() validates positive scissors contained in screen first.
            self.horizontal[i * 2] = .{ .at = @divTrunc(rect.x - screen.x, side), .command = @intCast(i), .enter = true };
            self.horizontal[i * 2 + 1] = .{ .at = @divTrunc(rect.right() - 1 - screen.x, side) + 1, .command = @intCast(i), .enter = false };
            self.vertical[i * 2] = .{ .at = @divTrunc(rect.y - screen.y, side), .command = @intCast(i), .enter = true };
            self.vertical[i * 2 + 1] = .{ .at = @divTrunc(rect.bottom() - 1 - screen.y, side) + 1, .command = @intCast(i), .enter = false };
        }
        std.mem.sort(Edge, self.horizontal[0..self.count], {}, less);
        std.mem.sort(Edge, self.vertical[0..self.count], {}, less);
    }
    pub fn next(self: *Index) ?Block {
        while (true) {
            if (self.column >= self.count) {
                if (self.row >= self.count) return null;
                self.top = self.vertical[self.row].at;
                while (self.row < self.count and self.vertical[self.row].at == self.top) : (self.row += 1) {
                    const edge = self.vertical[self.row];
                    self.rows.setValue(edge.command, edge.enter);
                }
                if (self.row >= self.count) return null;
                self.bottom = self.vertical[self.row].at;
                if (self.rows.count() == 0) continue;
                self.column = 0; self.active = Mask.initEmpty();
            }
            const left = self.horizontal[self.column].at;
            while (self.column < self.count and self.horizontal[self.column].at == left) : (self.column += 1) {
                const edge = self.horizontal[self.column];
                if (self.rows.isSet(edge.command)) self.active.setValue(edge.command, edge.enter);
            }
            if (self.column == self.count or self.active.count() == 0) continue;
            return .{ .x = left, .right = self.horizontal[self.column].at, .y = self.top, .bottom = self.bottom, .candidates = self.active };
        }
    }
};

test "tile sweep exactly partitions sparse overlapping scissors in painter order" {
    const Command = struct { scissor: Rect };
    const screen: Rect = .{ .x = -17, .y = 23, .w = 4096, .h = 4096 };
    var commands: [64]Command = undefined;
    for (&commands, 0..) |*command, i| command.* = .{ .scissor = .{
        .x = screen.x + @as(i32, @intCast(i % 8)) * 511,
        .y = screen.y + @as(i32, @intCast(i / 8)) * 511, .w = 31, .h = 33 } };
    commands[63] = commands[0];
    var index: Index = .{}; index.init(&commands, screen);
    var visited: [64 * 64]bool = @splat(false);
    var count: usize = 0;
    while (index.next()) |block| {
        for (@intCast(block.y)..@intCast(block.bottom)) |y| for (@intCast(block.x)..@intCast(block.right)) |x| {
            const offset = y * 64 + x;
            try std.testing.expect(!visited[offset]); visited[offset] = true; count += 1;
            const tile: Rect = .{ .x = screen.x + @as(i32, @intCast(x)) * side, .y = screen.y + @as(i32, @intCast(y)) * side, .w = side, .h = side };
            for (commands, 0..) |command, i| try std.testing.expectEqual(tile.intersects(command.scissor), block.candidates.isSet(i));
        };
    }
    try std.testing.expect(count < 256);
    for (commands) |command| {
        const x: usize = @intCast(@divTrunc(command.scissor.x - screen.x, side));
        const y: usize = @intCast(@divTrunc(command.scissor.y - screen.y, side));
        try std.testing.expect(visited[y * 64 + x]);
    }
    index.init(commands[0..0], screen);
    try std.testing.expect(index.next() == null);
}
