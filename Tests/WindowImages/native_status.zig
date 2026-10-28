//! Private explicit fixture evidence. Parent claims the nonce-qualified path
//! before Spawn; only its exact child writes it, and deletion follows Reap.
const std = @import("std");
pub const child_argument = "/DESKTOP-NATIVE-IMAGE-CHILD";
pub const success = "WINIMAGE native two device-local images, actual GPU fences/Take/Return/reuse/Close and exact own buffer/queue/fence release: OK";
pub const Identity = struct { instance: u32, generation: u64 };
pub const Ticket = struct { parent: Identity, nonce: u64 };

fn valid(ticket: Ticket) bool {
    return ticket.parent.instance != 0 and ticket.parent.generation != 0 and ticket.nonce != 0;
}
pub fn path(buffer: *[128:0]u8, ticket: Ticket) ![:0]const u8 {
    if (!valid(ticket)) return error.Invalid;
    return std.fmt.bufPrintZ(buffer, "C:\\TEMP\\NI{X:0>8}-{X:0>16}-{X:0>16}.TXT", .{ ticket.parent.instance, ticket.parent.generation, ticket.nonce });
}
pub fn arguments(buffer: *[128:0]u8, ticket: Ticket) ![:0]const u8 {
    if (!valid(ticket)) return error.Invalid;
    return std.fmt.bufPrintZ(buffer, child_argument ++ " {X:0>8} {X:0>16} {X:0>16}", .{ ticket.parent.instance, ticket.parent.generation, ticket.nonce });
}
pub fn parse(raw: []const u8) !Ticket {
    if (!std.mem.startsWith(u8, raw, child_argument ++ " ")) return error.Invalid;
    var parts = std.mem.tokenizeScalar(u8, raw[child_argument.len + 1 ..], ' ');
    const ticket: Ticket = .{ .parent = .{
        .instance = std.fmt.parseInt(u32, parts.next() orelse return error.Invalid, 16) catch return error.Invalid,
        .generation = std.fmt.parseInt(u64, parts.next() orelse return error.Invalid, 16) catch return error.Invalid,
    }, .nonce = std.fmt.parseInt(u64, parts.next() orelse return error.Invalid, 16) catch return error.Invalid };
    if (parts.next() != null or !valid(ticket)) return error.Invalid;
    return ticket;
}
pub fn encode(buffer: []u8, child: Identity, text: []const u8) ![]const u8 {
    if (child.instance == 0 or child.generation == 0) return error.Invalid;
    return std.fmt.bufPrint(buffer, "NI59 owner={d}/{d}\n{s}\n", .{ child.instance, child.generation, text });
}
pub fn owns(text: []const u8, child: Identity) bool {
    var buffer: [80]u8 = undefined;
    const prefix = std.fmt.bufPrint(&buffer, "NI59 owner={d}/{d}\n", .{ child.instance, child.generation }) catch return false;
    return child.instance != 0 and child.generation != 0 and std.mem.startsWith(u8, text, prefix);
}
pub fn succeeded(text: []const u8, child: Identity) bool {
    if (!owns(text, child)) return false;
    const body = text[(std.mem.indexOfScalar(u8, text, '\n') orelse return false) + 1 ..];
    return std.mem.eql(u8, body, success ++ "\n");
}

test "parent generation and nonce separate private paths; invalid child arguments never select a path" {
    const ticket: Ticket = .{ .parent = .{ .instance = 17, .generation = 40 }, .nonce = 99 };
    var args_buffer: [128:0]u8 = undefined;
    try std.testing.expectEqualDeep(ticket, try parse(try arguments(&args_buffer, ticket)));
    var buffers: [3][128:0]u8 = undefined;
    const first = try path(&buffers[0], ticket);
    try std.testing.expect(!std.mem.eql(u8, first, try path(&buffers[1], .{ .parent = .{ .instance = 17, .generation = 41 }, .nonce = 99 })));
    try std.testing.expect(!std.mem.eql(u8, first, try path(&buffers[2], .{ .parent = ticket.parent, .nonce = 100 })));
    for ([_][]const u8{ child_argument, child_argument ++ " 11 28 0", child_argument ++ " 11 28 63 extra", child_argument ++ " C:\\TEMP\\FOREIGN 28 63" }) |invalid| try std.testing.expectError(error.Invalid, parse(invalid));
}
test "terminal native status requires the exact child generation and actual success marker" {
    var buffer: [512]u8 = undefined;
    const child: Identity = .{ .instance = 23, .generation = 51 };
    const terminal = try encode(&buffer, child, success);
    try std.testing.expect(succeeded(terminal, child));
    try std.testing.expect(!succeeded(terminal, .{ .instance = 23, .generation = 52 }));
    try std.testing.expect(!succeeded(terminal, .{ .instance = 24, .generation = 51 }));
    const pending = try encode(&buffer, child, "WINIMAGE producer BOs attached");
    try std.testing.expect(owns(pending, child) and !succeeded(pending, child));
    const failed = try encode(&buffer, child, "WINIMAGE native FAILED: Receipt line=1 rpc=-3 phase=entry");
    try std.testing.expect(owns(failed, child) and !succeeded(failed, child));
}
