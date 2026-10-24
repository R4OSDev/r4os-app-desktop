const std = @import("std");
const t = std.testing;
const a = @import("r4os").abi;
const Owner = @import("screen_power.zig").Owner;
const second = std.time.ns_per_s;
const Model = struct {
    state: a.GfxOutputPower = .{ .identity = .{ .adapter_id = 1, .connector_id = 2, .device_generation = 3,
        .connection_generation = 4 }, .capabilities = 3, .phase = a.gfx_power_phase_on },
    intent: a.GfxPowerRequest = .{},
    calls: usize = 0,
    serial: u64 = 0,
    supported: bool = true,
    stale: bool = false,
    pub fn outputs(self: *Model) *Model { return self; }
    pub fn revision(_: *Model, out: *a.GfxDisplayRevision) i32 {
        out.* = .{ .revision = 1, .present = 1 }; return a.gfx_output_ok;
    }
    pub fn info(self: *Model, index: u32, out: *a.GfxOutputInfo) i32 {
        std.debug.assert(index == 0);
        out.* = .{ .identity = self.state.identity, .topology_revision = if (self.stale) 2 else 1 };
        return a.gfx_output_ok;
    }
    pub fn power(self: *Model, id: *const a.GfxOutputId, out: *a.GfxOutputPower) i32 {
        std.debug.assert(std.meta.eql(id.*, self.state.identity));
        if (!self.supported) return a.err_no_fn;
        out.* = self.state; return a.gfx_output_ok;
    }
    pub fn requestPower(self: *Model, request: *const a.GfxPowerRequest, out: *a.GfxPowerRequest) i32 {
        std.debug.assert(std.meta.eql(request.identity, self.state.identity) and request.sequence == 0);
        self.calls += 1;
        if (request.off == 0 or self.intent.sequence == 0 or self.intent.off != request.off or !std.meta.eql(self.intent.identity, request.identity)) self.serial += 1;
        self.intent = request.*; self.intent.sequence = self.serial;
        out.* = self.intent; return a.gfx_output_ok;
    }
};
pub fn check() !void {
    var draw: Model = .{};
    var owner: Owner = .{};
    draw.supported = false;
    try t.expectError(error.Unsupported, owner.sleep(&draw, second));
    try t.expect(!owner.input(&draw, second) and draw.calls == 0);
    draw.supported = true;
    try owner.sleep(&draw, 2 * second);
    owner.tick(&draw, 2 * second, 0);
    try t.expect(draw.intent.off == 1 and draw.calls == 1);
    // An input arriving before hardware starts must cancel the queued off.
    try t.expect(owner.input(&draw, 3 * second));
    try t.expect(draw.intent.off == 0 and draw.serial == 2 and owner.wake_pending);
    draw.state.request_sequence = draw.intent.sequence;
    owner.tick(&draw, 4 * second, 0);
    try t.expect(!owner.wake_pending and !owner.input(&draw, 5 * second));
    try owner.sleep(&draw, 6 * second);
    owner.tick(&draw, 6 * second, 0);
    draw.state.phase = a.gfx_power_phase_off;
    draw.state.request_sequence = draw.intent.sequence;
    owner.tick(&draw, 15 * second, 0);
    try t.expect(draw.calls == 3);
    owner.tick(&draw, 16 * second, 0);
    try t.expect(draw.calls == 4 and draw.serial == 3); // Renew, not a new transition.
    try t.expect(owner.input(&draw, 17 * second));
    draw.state.identity.connection_generation += 1;
    draw.state.phase = a.gfx_power_phase_on;
    draw.state.request_sequence = 0;
    owner.tick(&draw, 18 * second, 0);
    try t.expect(draw.intent.off == 0 and draw.serial == 5 and owner.wake_pending);
    draw.state.request_sequence = draw.intent.sequence;
    owner.tick(&draw, 19 * second, 0);
    try t.expect(!owner.wake_pending);
    // Failed wake stops retrying. Fresh input may make one new attempt.
    try owner.sleep(&draw, 20 * second);
    owner.tick(&draw, 20 * second, 0);
    draw.state.phase = a.gfx_power_phase_unavailable;
    draw.state.reason = a.gfx_power_reason_rejected;
    owner.tick(&draw, 21 * second, 0);
    try t.expect(!owner.want_off and owner.wake_pending and owner.faulted and draw.intent.off == 0);
    owner.tick(&draw, 57 * second, 0);
    const calls = draw.calls;
    owner.tick(&draw, 100 * second, 30);
    try t.expect(!owner.wake_pending and owner.faulted and draw.calls == calls);
    try t.expect(owner.input(&draw, 101 * second));
    try t.expect(owner.wake_pending and draw.calls == calls + 1);
    // A normal unplugged head must not swallow all Desktop input.
    owner = .{};
    draw.state.reason = a.gfx_power_reason_link;
    try t.expect(!owner.input(&draw, 102 * second));
    draw.state.phase = a.gfx_power_phase_on;
    owner.tick(&draw, 131 * second, 30);
    try t.expect(!owner.want_off);
    owner.tick(&draw, 132 * second, 30);
    try t.expect(owner.want_off and draw.intent.off == 1);
    // A torn catalog never turns a partial set into a successful discovery.
    draw.stale = true;
    try t.expectError(error.Stale, owner.sleep(&draw, 133 * second));
}
