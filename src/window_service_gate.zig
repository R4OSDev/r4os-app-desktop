pub const Gate = struct {
    available: bool = false,
    retry_at_tick: u64 = 0,

    pub fn markAvailable(self: *Gate) void {
        self.available = true;
        self.retry_at_tick = 0;
    }

    pub fn markUnavailable(self: *Gate, now: u64, retry_ticks: u64) void {
        self.available = false;
        self.retry_at_tick = now +| @max(retry_ticks, 1);
    }

    pub fn retryDue(self: *const Gate, now: u64, idle: bool) bool {
        return !self.available and idle and now >= self.retry_at_tick;
    }
};

/// One endpoint handle for the Desktop lifetime. Failed calls drop it; only
/// the explicit restart/registration sequence may open the next generation.
pub const Session = struct {
    handle: u32 = 0,

    pub fn open(self: *Session, backend: anytype) bool {
        if (self.handle == 0) self.handle = backend.openWindowEndpoint();
        return self.handle != 0;
    }

    pub fn close(self: *Session, backend: anytype) void {
        const handle = self.handle;
        self.handle = 0;
        if (handle != 0) backend.closeWindowEndpoint(handle);
    }

    pub fn call(self: *Session, backend: anytype, op: u16, request: []const u8, response: []u8) i32 {
        if (self.handle == 0) return -9;
        const result = backend.callWindowEndpoint(self.handle, op, request, response);
        if (result < 0) self.close(backend);
        return result;
    }
};

/// A pending slot always refers to the current App window, never an old
/// copied record. Lifecycle transitions discard it before slot reuse.
pub const GeometryUpdates = struct {
    pending: [4]bool = .{false} ** 4,
    deadlines: [4]u64 = .{0} ** 4,

    pub fn queue(self: *GeometryUpdates, index: usize, now: u64, interval: u64) void {
        if (!self.pending[index]) self.deadlines[index] = now +| @max(interval, 1);
        self.pending[index] = true;
    }

    pub fn take(self: *GeometryUpdates, index: usize, now: u64, force: bool) bool {
        if (!self.pending[index] or (!force and now < self.deadlines[index])) return false;
        self.pending[index] = false;
        return true;
    }

    pub fn delay(self: *const GeometryUpdates, now: u64, limit: u64) u64 {
        var result = limit;
        for (self.pending, self.deadlines) |pending, deadline| {
            if (pending) result = @min(result, deadline -| now);
        }
        return result;
    }
};

pub fn deadlineDelay(now: u64, limit: u64, deadlines: []const u64) u64 {
    var result = limit;
    for (deadlines) |deadline| result = @min(result, deadline -| now);
    return result;
}

test "failure closes the critical path until an idle retry is due" {
    var gate = Gate{ .available = true };
    gate.markUnavailable(100, 25);
    try @import("std").testing.expect(!gate.available);
    try @import("std").testing.expect(!gate.retryDue(124, true));
    try @import("std").testing.expect(!gate.retryDue(125, false));
    try @import("std").testing.expect(gate.retryDue(125, true));
    gate.markAvailable();
    try @import("std").testing.expect(gate.available);
    try @import("std").testing.expect(!gate.retryDue(1000, true));
}

test "geometry bursts retain the end position and session failure requires explicit reopen" {
    const std = @import("std");
    const Backend = struct {
        generation: u32 = 1,
        opens: usize = 0,
        closes: usize = 0,
        calls: usize = 0,
        last: u8 = 0,
        pub fn openWindowEndpoint(self: *@This()) u32 {
            self.opens += 1;
            return self.generation;
        }
        pub fn closeWindowEndpoint(self: *@This(), _: u32) void {
            self.closes += 1;
        }
        pub fn callWindowEndpoint(self: *@This(), handle: u32, _: u16, request: []const u8, _: []u8) i32 {
            self.calls += 1;
            if (handle != self.generation) return -9;
            self.last = request[0];
            return 0;
        }
    };
    var backend: Backend = .{};
    var session: Session = .{};
    var geometry: GeometryUpdates = .{};
    try std.testing.expect(session.open(&backend));
    for (0..100) |position| {
        geometry.queue(0, position, 5);
        if (geometry.take(0, position, false)) {
            try std.testing.expectEqual(@as(i32, 0), session.call(&backend, 1, &.{@intCast(position)}, &.{}));
        }
    }
    try std.testing.expect(geometry.take(0, 100, true));
    _ = session.call(&backend, 1, &.{100}, &.{});
    try std.testing.expectEqual(@as(u8, 100), backend.last);
    try std.testing.expectEqual(@as(usize, 17), backend.calls);
    try std.testing.expectEqual(@as(usize, 1), backend.opens);
    try std.testing.expectEqual(@as(usize, 0), backend.closes);
    geometry.queue(0, 101, 5);
    backend.generation = 2;
    try std.testing.expectEqual(@as(i32, -9), session.call(&backend, 1, &.{101}, &.{}));
    geometry = .{}; // the same loss/re-registration boundary as App
    try std.testing.expectEqual(@as(u32, 0), session.handle);
    try std.testing.expect(!geometry.take(0, 200, true));
    try std.testing.expectEqual(@as(i32, -9), session.call(&backend, 1, &.{0}, &.{}));
    try std.testing.expectEqual(@as(usize, 18), backend.calls);
    try std.testing.expect(session.open(&backend));
    _ = session.call(&backend, 1, &.{102}, &.{});
    try std.testing.expectEqual(@as(u8, 102), backend.last);
    session.close(&backend);
    session.close(&backend);
    try std.testing.expectEqual(@as(usize, 2), backend.opens);
    try std.testing.expectEqual(@as(usize, 2), backend.closes);
    // A connected tray waits for its real five-tick deadline. An absent
    // service waits for retry or clock/blink, not a ten-ms revision poll.
    try std.testing.expectEqual(@as(u64, 5), deadlineDelay(100, 50, &.{ 105, 200 }));
    try std.testing.expectEqual(@as(u64, 50), deadlineDelay(100, 50, &.{ 300, 200 }));
}
