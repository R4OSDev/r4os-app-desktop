const std = @import("std");
const t = std.testing;
const a = @import("r4os").abi;
const Owner = @import("platform_input.zig").Owner;
const second = std.time.ns_per_s;
const Model = struct {
    input_state: a.PlatformInputSnapshot = .{ .sequence = 2, .brightness_up = 7, .brightness_down = 3, .lid_sequence = 1,
        .lid_state = 1, .capabilities = 3, .sources = 3 },
    state: a.GfxOutputBrightness = .{ .identity = .{ .adapter_id = 1, .connector_id = 2, .device_generation = 3,
        .connection_generation = 4 }, .phase = a.gfx_brightness_phase_ready,
        .flags = a.gfx_brightness_flag_current_known, .minimum = 100, .maximum = 65535, .current = 30000 },
    power_phase: u32 = a.gfx_power_phase_on,
    count: u32 = 1,
    sleeps: usize = 0,
    wakes: usize = 0,
    calls: usize = 0,
    last: a.GfxBrightnessRequest = .{},
    available: bool = true,
    busy: bool = false,
    torn: bool = false,
    pub fn outputs(self: *Model) *Model { return self; }
    pub fn platformInputSnapshot(self: *Model, out: *a.PlatformInputSnapshot) i32 {
        out.* = self.input_state; return if (self.available) 1 else -1;
    }
    pub fn sleep(self: *Model, _: *Model, _: u64) !void { self.sleeps += 1; }
    pub fn input(self: *Model, _: *Model, _: u64) bool { self.wakes += 1; return true; }
    pub fn revision(self: *Model, out: *a.GfxDisplayRevision) i32 {
        out.* = .{ .revision = 1, .present = self.count }; return a.gfx_output_ok;
    }
    pub fn info(self: *Model, _: u32, out: *a.GfxOutputInfo) i32 {
        out.* = .{ .identity = self.state.identity, .topology_revision = if (self.torn) 2 else 1,
            .connector_kind = a.gfx_output_kind_edp, .flags = a.gfx_output_flag_connected };
        return a.gfx_output_ok;
    }
    pub fn power(self: *Model, id: *const a.GfxOutputId, out: *a.GfxOutputPower) i32 {
        out.* = .{ .identity = id.*, .phase = self.power_phase }; return a.gfx_output_ok;
    }
    pub fn brightness(self: *Model, _: *const a.GfxOutputId, out: *a.GfxOutputBrightness) i32 {
        out.* = self.state; return a.gfx_output_ok;
    }
    pub fn requestBrightness(self: *Model, request: *const a.GfxBrightnessRequest, out: *a.GfxBrightnessRequest) i32 {
        self.calls += 1;
        if (self.busy) return a.gfx_output_error_busy;
        self.last = request.*; self.last.sequence = self.calls;
        out.* = self.last; return a.gfx_output_ok;
    }
    fn key(self: *Model, owner: *Owner, now: u64, up: u64, down: u64) !void {
        self.input_state.sequence += 1; self.input_state.brightness_up += up; self.input_state.brightness_down += down;
        try t.expect(owner.poll(self, self, self, now));
    }
};
pub fn check() !void {
    var model: Model = .{};
    var owner: Owner = .{};
    try t.expect(!owner.poll(&model, &model, &model, second));
    owner.step(&model, second);
    try t.expect(model.calls == 0 and model.wakes == 0); // old counters are a baseline
    model.input_state.lid_state = 2; model.input_state.lid_sequence += 1;
    try t.expect(owner.poll(&model, &model, &model, 2 * second));
    try t.expect(!owner.poll(&model, &model, &model, 3 * second));
    try t.expect(model.sleeps == 1);
    model.input_state.lid_state = 0; model.input_state.lid_sequence += 1; // source removal releases the lid
    try t.expect(owner.poll(&model, &model, &model, 4 * second));
    try t.expect(model.wakes == 1);
    try model.key(&owner, 5 * second, 1, 0);
    model.power_phase = a.gfx_power_phase_waking;
    owner.step(&model, 5 * second);
    try t.expect(model.calls == 0);
    model.power_phase = a.gfx_power_phase_on; model.busy = true;
    owner.step(&model, 6 * second);
    try t.expect(model.calls == 1 and owner.pending == 1 and owner.accepted == 0);
    model.busy = false;
    owner.step(&model, 7 * second);
    try t.expect(model.calls == 2 and model.last.level == 33277 and model.state.current == 30000);
    try model.key(&owner, 8 * second, 2, 0);
    owner.step(&model, 8 * second);
    try t.expect(model.calls == 2); // no second request before confirmed first receipt
    model.state.current = model.last.level; model.state.request_sequence = model.last.sequence;
    owner.step(&model, 9 * second);
    try t.expect(model.calls == 3 and model.last.level == 39831);
    model.state.current = model.last.level; model.state.request_sequence = model.last.sequence;
    owner.step(&model, 10 * second);
    try t.expect(owner.accepted == 0 and owner.pending == 0);
    try model.key(&owner, 11 * second, 0, 20);
    owner.step(&model, 11 * second);
    try t.expect(model.last.level == model.state.minimum);
    model.state.phase = a.gfx_brightness_phase_failed;
    owner.step(&model, 12 * second);
    const calls = model.calls;
    owner.step(&model, 13 * second);
    try t.expect(owner.accepted == 0 and model.calls == calls);
    model.state.phase = a.gfx_brightness_phase_ready;
    try model.key(&owner, 14 * second, 1, 0);
    model.power_phase = a.gfx_power_phase_off;
    owner.step(&model, 14 * second);
    model.state.identity.connection_generation += 1;
    model.power_phase = a.gfx_power_phase_on;
    owner.step(&model, 15 * second);
    try t.expect(owner.pending == 0 and model.calls == calls); // no transfer to replugged panel
    try model.key(&owner, 16 * second, 1, 0);
    model.count = 0; owner.step(&model, 16 * second);
    try t.expect(owner.pending == 0);
    try model.key(&owner, 17 * second, 1, 0);
    model.count = 2; owner.step(&model, 17 * second);
    try t.expect(owner.pending == 0); // ambiguous internal panel
    try model.key(&owner, 18 * second, 1, 0);
    model.count = 1; model.torn = true; owner.step(&model, 18 * second);
    try t.expect(model.calls == calls and owner.pending == 1);
    owner.step(&model, 54 * second);
    try t.expect(owner.pending == 0); // bounded retry on unstable catalog
    model.input_state.sequence = 1; model.input_state.brightness_up = 0; model.input_state.brightness_down = 0;
    try t.expect(!owner.poll(&model, &model, &model, 55 * second));
    model.input_state.lid_state = 2; model.input_state.lid_sequence += 1;
    try t.expect(owner.poll(&model, &model, &model, 56 * second));
    const wakes = model.wakes;
    model.available = false;
    try t.expect(owner.poll(&model, &model, &model, 57 * second));
    try t.expect(model.wakes == wakes + 1 and owner.previous == null);
    try t.expect(!owner.poll(&model, &model, &model, 58 * second));
}
