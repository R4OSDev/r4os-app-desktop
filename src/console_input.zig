const r4os = @import("r4os");

/// One bounded producer per exact window/process generation. Queue admission
/// is all-or-nothing; API acknowledgements retire only the accepted prefix.
pub const Pending = struct {
    handle: r4os.abi.ProgramProcessHandle = .{},
    queue: r4os.input_events.Queue(4096) = .{},
    last_error: i32 = 0,
    pub fn bind(self: *Pending, handle: r4os.abi.ProgramProcessHandle) void {
        if (self.handle.instance_id == handle.instance_id and self.handle.generation == handle.generation) return;
        self.queue.clear();
        self.last_error = 0;
        self.handle = handle;
    }
    pub fn flush(self: *Pending, sys: *const r4os.r4sys.Context) void {
        if (self.handle.instance_id == 0 or self.queue.len == 0) return;
        const batch = self.queue.peek();
        const result = sys.consolePushEvents(self.handle, batch[0..@min(batch.len, 64)]);
        if (result < 0 or result > @min(batch.len, 64)) {
            self.last_error = if (result < 0) result else -1;
            return;
        }
        _ = self.queue.consume(@intCast(result));
        self.last_error = 0;
    }
    pub fn bytes(self: *Pending, input: []const u8) bool {
        if (input.len > 4096 - self.queue.len) return false;
        // Capacity is preflighted, so no accepted clipboard suffix is lost.
        for (input) |byte| {
            const event = r4os.input_events.utf8Byte(byte);
            if (!self.queue.append(&.{event})) unreachable;
        }
        return true;
    }
};

var fixture_result: i32 = 0;
var fixture_value: u32 = 0;
var fixture_kind: u32 = 0;
fn fixturePush(handle: *const r4os.abi.ProgramProcessHandle, events: [*]const r4os.abi.ConsoleInputEvent, count: u32) callconv(.c) i32 {
    if (handle.instance_id != 37 or handle.generation != 1 or count == 0) return -1;
    fixture_value = events[0].value;
    fixture_kind = events[0].kind;
    return fixture_result;
}
test "desktop keeps zero/partial input suffix until acknowledgement and closes exact old binding" {
    const std = @import("std");
    const raw: r4os.abi.R4XStartContext = .{};
    const table: r4os.abi.R4XStartR4Desk = .{ .console_push_events = @intFromPtr(&fixturePush) };
    const bundle: r4os.program.Bundle = .{ .raw = &raw, .desk = &table };
    const sys = r4os.r4sys.Context.init(&bundle);
    var pending: Pending = .{};
    pending.bind(.{ .instance_id = 37, .generation = 1 });
    try std.testing.expect(pending.bytes("\xd0"));
    fixture_result = 0;
    pending.flush(&sys);
    try std.testing.expectEqual(@as(usize, 1), pending.queue.len);
    try std.testing.expect(pending.bytes("\x81\xc4\x80"));
    fixture_result = 1;
    pending.flush(&sys);
    try std.testing.expectEqual(@as(usize, 3), pending.queue.len);
    try std.testing.expectEqual(@as(u32, 0xd0), fixture_value);
    pending.flush(&sys);
    try std.testing.expectEqual(@as(u32, 0x81), fixture_value);
    try std.testing.expectEqual(r4os.abi.console_input_event_utf8_byte, fixture_kind);
    fixture_result = -1;
    pending.flush(&sys);
    try std.testing.expectEqual(@as(usize, 2), pending.queue.len);
    pending.bind(.{ .instance_id = 37, .generation = 2 });
    try std.testing.expectEqual(@as(usize, 0), pending.queue.len);
    const full: [4096]u8 = @splat(0x80);
    try std.testing.expect(pending.bytes(&full));
    try std.testing.expect(!pending.bytes("\x81"));
    try std.testing.expectEqual(@as(usize, 4096), pending.queue.len);
}
