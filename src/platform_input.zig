//! User policy for confirmed ACPI/USB facts. No AML, EC ports or scancodes.
const std = @import("std");
const a = @import("r4os").abi;
pub const Owner = struct {
    previous: ?a.PlatformInputSnapshot = null,
    pending: i16 = 0,
    target: ?a.GfxOutputId = null,
    accepted: u64 = 0,
    deadline: u64 = 0,
    next_ns: u64 = 0,
    fn clear(self: *Owner) void { self.pending = 0; self.target = null; self.accepted = 0; self.next_ns = 0; }
    pub fn poll(self: *Owner, sys: anytype, draw: anytype, screen: anytype, now: u64) bool {
        if (now == 0) return false;
        var value: a.PlatformInputSnapshot = .{};
        if (sys.platformInputSnapshot(&value) <= 0 or value.version != 1 or value.size != @sizeOf(a.PlatformInputSnapshot) or
            value.capabilities > 3 or value.sources > 3 or value.lid_state > 2 or value.reserved != 0) {
            const closed = if (self.previous) |old| old.lid_state == 2 else false;
            self.previous = null; self.clear();
            if (closed) _ = screen.input(draw, now);
            return closed;
        }
        const prior = self.previous; self.previous = value;
        const fresh = if (prior) |old| value.sequence >= old.sequence and value.brightness_up >= old.brightness_up and value.brightness_down >= old.brightness_down else false;
        var activity = false;
        if (prior == null or !fresh or prior.?.lid_sequence != value.lid_sequence) {
            if (value.lid_state == 2) { screen.sleep(draw, now) catch {}; activity = true; }
            else if (prior != null and prior.?.lid_state == 2) { _ = screen.input(draw, now); activity = true; }
        }
        if (!fresh) { self.clear(); return activity; } // no historical key replay
        const old = prior.?;
        const up = @min(20, value.brightness_up - old.brightness_up);
        const down = @min(20, value.brightness_down - old.brightness_down);
        if (up != 0 or down != 0) {
            self.pending = std.math.clamp(self.pending + @as(i16, @intCast(up)) - @as(i16, @intCast(down)), -20, 20);
            self.deadline = now +| 35 * std.time.ns_per_s;
            self.next_ns = 0;
            _ = screen.input(draw, now); activity = true;
        }
        return activity;
    }
    pub fn step(self: *Owner, draw: anytype, now: u64) void {
        if (self.pending == 0 and self.accepted == 0) return;
        if (now == 0 or now >= self.deadline) { self.clear(); return; }
        if (now < self.next_ns) return;
        self.next_ns = now +| 100 * std.time.ns_per_ms;
        const outputs = draw.outputs();
        var before: a.GfxDisplayRevision = .{};
        if (outputs.revision(&before) != a.gfx_output_ok or before.present > 32) return;
        var panel: ?a.GfxOutputId = null;
        for (0..before.present) |index| {
            var info: a.GfxOutputInfo = .{};
            if (outputs.info(@intCast(index), &info) != a.gfx_output_ok or info.topology_revision != before.revision) return;
            if (info.connector_kind == a.gfx_output_kind_edp and info.flags & a.gfx_output_flag_connected != 0) {
                // Do not guess between two internal panels.
                if (panel != null) { self.clear(); return; }
                panel = info.identity;
            }
        }
        var after: a.GfxDisplayRevision = .{};
        if (outputs.revision(&after) != a.gfx_output_ok or after.revision != before.revision) return;
        const identity = panel orelse { self.clear(); return; };
        if (self.target) |old| if (!std.meta.eql(old, identity)) { self.clear(); return; };
        self.target = identity;
        var power: a.GfxOutputPower = .{};
        if (outputs.power(&identity, &power) == a.gfx_output_ok and power.phase != a.gfx_power_phase_on) return;
        var state: a.GfxOutputBrightness = .{};
        const rc = outputs.brightness(&identity, &state);
        if (rc == a.gfx_output_error_unsupported or rc == a.err_no_fn) { self.clear(); return; }
        if (rc != a.gfx_output_ok or !std.meta.eql(state.identity, identity)) return;
        if (state.phase != a.gfx_brightness_phase_ready or state.flags & a.gfx_brightness_flag_current_known == 0 or
            state.minimum >= state.maximum or state.maximum > 65535) { self.clear(); return; }
        if (self.accepted != 0) {
            if (state.request_sequence < self.accepted) return;
            self.accepted = 0;
        }
        if (self.pending == 0) { self.clear(); return; }
        const level: u32 = @intCast(std.math.clamp(@as(i64, state.current) + @as(i64, self.pending) * 3277, state.minimum, state.maximum));
        if (level == state.current) { self.clear(); return; }
        var accepted: a.GfxBrightnessRequest = .{};
        const result = outputs.requestBrightness(&.{ .identity = identity, .level = level }, &accepted);
        if (result == a.gfx_output_error_busy) return;
        if (result != a.gfx_output_ok or !std.meta.eql(accepted.identity, identity) or accepted.sequence == 0) { self.clear(); return; }
        self.accepted = accepted.sequence; self.pending = 0;
    }
};
