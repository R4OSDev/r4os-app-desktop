const std = @import("std");
const r4os = @import("r4os");
const paint = @import("paint.zig");
const surface = @import("surface.zig");

// An ordered tree over small command blocks. Bounds are client-local and
// generation-owned, so moves and repeated dirty regions do not rebuild it.
// The optional tree costs fewer than eight bytes per command. On allocation
// failure the same iterator falls back to the complete ordered command list.
pub const block_size: usize = 16;
pub const Bounds = struct {
    left: i64 = 0,
    top: i64 = 0,
    right: i64 = 0,
    bottom: i64 = 0,

    fn empty(self: Bounds) bool {
        return self.right <= self.left or self.bottom <= self.top;
    }

    fn merged(a: Bounds, b: Bounds) Bounds {
        if (a.empty()) return b;
        if (b.empty()) return a;
        return .{ .left = @min(a.left, b.left), .top = @min(a.top, b.top), .right = @max(a.right, b.right), .bottom = @max(a.bottom, b.bottom) };
    }

    fn intersects(a: Bounds, b: Bounds) bool {
        return !a.empty() and !b.empty() and a.left < b.right and b.left < a.right and a.top < b.bottom and b.top < a.bottom;
    }

    pub fn localClip(client: surface.Rect, clip: surface.Rect) Bounds {
        return .{
            .left = @as(i64, @max(client.x, clip.x)) - client.x,
            .top = @as(i64, @max(client.y, clip.y)) - client.y,
            .right = @min(@as(i64, client.x) + client.w, @as(i64, clip.x) + clip.w) - client.x,
            .bottom = @min(@as(i64, client.y) + client.h, @as(i64, clip.y) + clip.h) - client.y,
        };
    }
};

fn commandBounds(command: r4os.abi.GuiFrameCommand, resources: []const u8) Bounds {
    if (command.version != r4os.abi.gui_frame_command_version or command.size != r4os.abi.gui_frame_command_size) return .{};
    var width: i64 = command.w;
    var height: i64 = command.h;
    switch (command.kind) {
        r4os.abi.gui_frame_command_kind_clear => return .{
            .left = std.math.minInt(i32),
            .top = std.math.minInt(i32),
            .right = std.math.maxInt(i64),
            .bottom = std.math.maxInt(i64),
        },
        r4os.abi.gui_frame_command_kind_text => {
            const end = std.math.add(u64, command.resource_offset, command.resource_bytes) catch return .{};
            if (end > resources.len) return .{};
            const extent = paint.conservativeTextExtent(command.font_id, resources[@intCast(command.resource_offset)..@intCast(end)]);
            width = extent.width;
            height = extent.height;
        },
        r4os.abi.gui_frame_command_kind_raster,
        r4os.abi.gui_frame_command_kind_argb32,
        => {
            if (command.parameter0 == 0 or command.parameter0 > 16) return .{};
            width *= @intCast(command.parameter0);
            height *= @intCast(command.parameter0);
        },
        r4os.abi.gui_frame_command_kind_rect,
        r4os.abi.gui_frame_command_kind_indexed8,
        r4os.abi.gui_frame_command_kind_xrgb32_nearest,
        r4os.abi.gui_frame_command_kind_shared_raster,
        r4os.abi.gui_frame_command_kind_alpha8,
        r4os.abi.gui_frame_command_kind_path_fill,
        r4os.abi.gui_frame_command_kind_path_stroke,
        r4os.abi.gui_frame_command_kind_rounded_rect,
        r4os.abi.gui_frame_command_kind_shadow,
        => {},
        else => return .{},
    }
    return .{ .left = command.x, .top = command.y, .right = @as(i64, command.x) +| width, .bottom = @as(i64, command.y) +| height };
}

pub const View = struct {
    nodes: []const Bounds = &.{},
    leaf_count: usize = 0,
    command_count: usize = 0,

    pub fn select(self: View, commands: usize, clip: Bounds) Iterator {
        const indexed = self.nodes.len != 0 and self.command_count == commands;
        return .{
            .view = self,
            .clip = clip,
            .command_count = commands,
            .node = if (indexed and !clip.empty()) 1 else 0,
            .end = if (!indexed and !clip.empty()) commands else 0,
        };
    }
};

pub const Index = struct {
    nodes: ?[]Bounds = null,
    leaf_count: usize = 0,
    command_count: usize = 0,

    pub fn deinit(self: *Index, allocator: std.mem.Allocator) void {
        if (self.nodes) |memory| allocator.free(memory);
        self.* = .{};
    }

    pub fn view(self: *const Index) View {
        return .{ .nodes = self.nodes orelse &.{}, .leaf_count = self.leaf_count, .command_count = self.command_count };
    }

    pub fn update(self: *Index, allocator: std.mem.Allocator, commands: []const r4os.abi.GuiFrameCommand, resources: []const u8, appended_from: usize) void {
        const blocks = commands.len / block_size + @intFromBool(commands.len % block_size != 0);
        const leaves = std.math.ceilPowerOfTwo(usize, @max(1, blocks)) catch {
            self.deinit(allocator);
            return;
        };
        const append = appended_from != 0 and appended_from == self.command_count;
        var first_block = if (append) appended_from / block_size else 0;
        var last_block = if (append) blocks else leaves;
        if (self.nodes == null or leaves != self.leaf_count) {
            self.deinit(allocator);
            const count = std.math.mul(usize, leaves, 2) catch return;
            self.nodes = allocator.alloc(Bounds, count) catch return;
            self.leaf_count = leaves;
            first_block = 0;
            last_block = leaves;
            @memset(self.nodes.?, Bounds{});
        }
        const nodes = self.nodes.?;
        for (first_block..last_block) |block| {
            var bounds = Bounds{};
            const begin = @min(block * block_size, commands.len);
            const end = @min(begin +| block_size, commands.len);
            for (commands[begin..end]) |command| bounds = bounds.merged(commandBounds(command, resources));
            nodes[leaves + block] = bounds;
        }
        // Rebuild only ancestors of the changed suffix for append generations.
        var first = leaves + first_block;
        var end = leaves + last_block;
        while (first > 1) {
            first /= 2;
            end = end / 2 + end % 2;
            for (first..end) |node| nodes[node] = nodes[2 * node].merged(nodes[2 * node + 1]);
        }
        self.command_count = commands.len;
    }
};

pub const Iterator = struct {
    view: View,
    clip: Bounds,
    command_count: usize,
    node: usize = 0,
    next_command: usize = 0,
    end: usize = 0,
    nodes_visited: usize = 0,
    commands_visited: usize = 0,

    pub fn next(self: *Iterator) ?usize {
        while (self.next_command == self.end) {
            if (self.node == 0) return null;
            const node = self.node;
            self.nodes_visited += 1;
            if (!self.view.nodes[node].intersects(self.clip)) {
                self.advance();
            } else if (node < self.view.leaf_count) {
                self.node *= 2;
            } else {
                self.next_command = @min((node - self.view.leaf_count) * block_size, self.command_count);
                self.end = @min(self.next_command +| block_size, self.command_count);
                self.advance();
            }
        }
        const result = self.next_command;
        self.next_command += 1;
        self.commands_visited += 1;
        return result;
    }

    fn advance(self: *Iterator) void {
        while (self.node > 1 and self.node % 2 != 0) self.node /= 2;
        self.node = if (self.node <= 1) 0 else self.node + 1;
    }
};

test "spatial selection preserves painter order and reduces eight-region replay" {
    var commands: [4096]r4os.abi.GuiFrameCommand = undefined;
    for (&commands, 0..) |*command, i| command.* = .{ .kind = r4os.abi.gui_frame_command_kind_rect, .x = @intCast((i % 64) * 8), .y = @intCast((i / 64) * 8), .w = 8, .h = 8 };
    commands[0] = .{ .kind = r4os.abi.gui_frame_command_kind_clear };
    var index = Index{};
    defer index.deinit(std.testing.allocator);
    index.update(std.testing.allocator, &commands, &.{}, 0);
    var visits: usize = 0;
    var nodes: usize = 0;
    for (0..8) |region| {
        const position: i64 = @intCast(region * 64);
        var it = index.view().select(commands.len, .{ .left = position, .top = position, .right = position + 1, .bottom = position + 1 });
        var previous: ?usize = null;
        var found_clear = false;
        var found_pixel = false;
        while (it.next()) |i| {
            if (previous) |p| try std.testing.expect(i > p);
            previous = i;
            found_clear = found_clear or i == 0;
            found_pixel = found_pixel or i == region * 8 * 65;
        }
        try std.testing.expect(found_clear and found_pixel);
        visits += it.commands_visited;
        nodes += it.nodes_visited;
    }
    try std.testing.expectEqual(@as(usize, 240), visits);
    try std.testing.expect(nodes < 256);
    try std.testing.expect(index.nodes.?.len * @sizeOf(Bounds) <= commands.len * 8);
    std.debug.print("[desktop-work] eight clips: command visits 32768 -> {d}, tree nodes {d}\n", .{ visits, nodes });
}

test "index append replacement resource text bounds and allocation fallback stay correct" {
    const commands = [_]r4os.abi.GuiFrameCommand{
        .{ .kind = r4os.abi.gui_frame_command_kind_rect, .x = 0, .y = 0, .w = 1, .h = 1 },
        .{ .kind = r4os.abi.gui_frame_command_kind_text, .x = 100, .y = 100, .resource_bytes = 6 },
    };
    const clip = Bounds{ .left = 108, .top = 108, .right = 109, .bottom = 109 };
    var index = Index{};
    defer index.deinit(std.testing.allocator);
    index.update(std.testing.allocator, commands[0..1], &.{}, 0);
    var before = index.view().select(1, clip);
    try std.testing.expect(before.next() == null);
    index.update(std.testing.allocator, &commands, "é\n中", 1);
    var after = index.view().select(2, .{ .left = 100, .top = 108, .right = 101, .bottom = 109 });
    try std.testing.expect(after.next() != null);
    index.update(std.testing.allocator, commands[0..1], &.{}, 0);
    var replaced = index.view().select(1, clip);
    try std.testing.expect(replaced.next() == null);
    var no_memory: [0]u8 = .{};
    var allocator = std.heap.FixedBufferAllocator.init(&no_memory);
    var fallback = Index{};
    fallback.update(allocator.allocator(), &commands, "é\n中", 0);
    var all = fallback.view().select(2, clip);
    try std.testing.expectEqual(@as(?usize, 0), all.next());
    try std.testing.expectEqual(@as(?usize, 1), all.next());
    try std.testing.expect(all.next() == null);
}
