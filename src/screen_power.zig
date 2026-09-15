//! Desktop screen policy only. Hardware and receipts stay in the R4D;
//! connector generations, short off leases and input wake bound our intent.
const std = @import("std");
const a = @import("r4os").abi;
const Entry = struct { state: a.GfxOutputPower = .{}, request: a.GfxPowerRequest = .{}, renew_ns: u64 = 0 };
pub const Owner = struct {
    entries: [32]Entry = @splat(.{}),
    count: usize = 0,
    last_activity_ns: u64 = 0,
    poll_ns: u64 = 0,
    transition_deadline: u64 = 0,
    want_off: bool = false,
    wake_pending: bool = false,
    faulted: bool = false,

    fn discover(self: *Owner, draw: anytype) !void {
        const outputs = draw.outputs();
        var before: a.GfxDisplayRevision = .{};
        if (outputs.revision(&before) != a.gfx_output_ok or before.present > self.entries.len) return error.Unavailable;
        var found: [32]Entry = @splat(.{});
        var count: usize = 0;
        for (0..before.present) |index| {
            var info: a.GfxOutputInfo = .{};
            if (outputs.info(@intCast(index), &info) != a.gfx_output_ok or info.topology_revision != before.revision) return error.Stale;
            var state: a.GfxOutputPower = .{};
            const result = outputs.power(&info.identity, &state);
            if (result == a.err_no_fn or result == a.err_no_group or result == a.gfx_output_error_unsupported) continue;
            if (result != a.gfx_output_ok or state.version != 1 or state.size != @sizeOf(a.GfxOutputPower) or
                !std.meta.eql(state.identity, info.identity)) return error.Stale;
            if (state.capabilities & a.gfx_power_cap_signal == 0) continue;
            var entry: Entry = .{ .state = state };
            for (self.entries[0..self.count]) |old| if (std.meta.eql(old.state.identity, state.identity)) {
                entry.request = old.request; entry.renew_ns = old.renew_ns; break;
            };
            found[count] = entry; count += 1;
        }
        var after: a.GfxDisplayRevision = .{};
        if (outputs.revision(&after) != a.gfx_output_ok or before.revision != after.revision) return error.Stale;
        self.entries = found; self.count = count;
    }
    pub fn sleep(self: *Owner, draw: anytype, now: u64) !void {
        if (now == 0) return error.Clock;
        try self.discover(draw);
        if (self.count == 0) return error.Unsupported;
        self.want_off = true; self.wake_pending = false; self.faulted = false;
        self.transition_deadline = now +| 35 * std.time.ns_per_s;
        self.poll_ns = 0;
    }
    /// Consume the waking event so it cannot also activate a hidden control.
    pub fn input(self: *Owner, draw: anytype, now: u64) bool {
        self.last_activity_ns = now;
        self.discover(draw) catch {};
        var sleeping = self.want_off or self.wake_pending;
        for (self.entries[0..self.count]) |entry| if ((entry.state.phase >= a.gfx_power_phase_stopping and
            entry.state.phase <= a.gfx_power_phase_waking) or entry.request.off != 0 or
            (self.faulted and entry.request.sequence != 0 and entry.state.phase == a.gfx_power_phase_unavailable and
                entry.state.reason != a.gfx_power_reason_link)) {
            sleeping = true; break;
        };
        self.want_off = false; self.faulted = false;
        if (sleeping) {
            if (!self.wake_pending) {
                self.transition_deadline = now +| 35 * std.time.ns_per_s;
                for (self.entries[0..self.count]) |*entry| entry.request = .{};
            }
            self.wake_pending = true; self.poll_ns = 0;
            self.tick(draw, now, 0);
        }
        return sleeping;
    }
    pub fn tick(self: *Owner, draw: anytype, now: u64, idle_seconds: u32) void {
        if (now == 0 or now < self.poll_ns) return;
        self.poll_ns = now +| 500 * std.time.ns_per_ms;
        if (self.last_activity_ns == 0 or now < self.last_activity_ns) self.last_activity_ns = now;
        self.discover(draw) catch return;
        if (!self.want_off and !self.wake_pending and !self.faulted and idle_seconds != 0 and
            now - self.last_activity_ns >= @as(u64, idle_seconds) * std.time.ns_per_s) self.sleep(draw, now) catch {};
        if (!self.want_off and !self.wake_pending) return;
        var complete = self.count != 0;
        for (self.entries[0..self.count]) |entry| {
            const wanted: u32 = if (self.want_off) a.gfx_power_phase_off else a.gfx_power_phase_on;
            if (entry.state.phase != wanted) complete = false;
            if (!self.want_off and (entry.request.off != 0 or entry.request.sequence == 0 or
                entry.state.request_sequence < entry.request.sequence)) complete = false;
            if (self.want_off and entry.state.phase == a.gfx_power_phase_unavailable) {
                self.want_off = false; self.wake_pending = true; self.faulted = true;
                self.transition_deadline = now +| 35 * std.time.ns_per_s;
                complete = false;
                break;
            }
        }
        if (complete and self.wake_pending) {
            self.wake_pending = false;
            return;
        }
        if (!complete and now >= self.transition_deadline) {
            if (self.want_off) {
                self.want_off = false; self.wake_pending = true;
                self.transition_deadline = now +| 35 * std.time.ns_per_s;
            } else self.wake_pending = false;
            self.faulted = true;
        }
        const outputs = draw.outputs();
        for (self.entries[0..self.count]) |*entry| {
            const off: u32 = @intFromBool(self.want_off);
            if (entry.request.sequence != 0 and entry.request.off == off and
                (off == 0 or now < entry.renew_ns)) continue;
            var result: a.GfxPowerRequest = .{};
            if (outputs.requestPower(&.{ .identity = entry.state.identity, .off = off }, &result) != a.gfx_output_ok) continue;
            entry.request = result; entry.renew_ns = now +| 10 * std.time.ns_per_s;
        }
    }
};
