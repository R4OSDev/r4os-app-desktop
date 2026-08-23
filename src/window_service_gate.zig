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
