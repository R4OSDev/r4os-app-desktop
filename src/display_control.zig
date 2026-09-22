//! Desktop owns display preferences and their confirmation lifetime. The
//! window service only carries copied requests; native receipts stay in the
//! common mode owner. Logical changes never alter the firmware framebuffer.
const std = @import("std");
const r4os = @import("r4os");
const a = r4os.abi;
const gfx = @import("r4gfx");
const api = @import("api.zig");
const outputs = @import("output_manager.zig");
const catalog = @import("r4gfx_desktop_outputs");
const policy = catalog.control;
const topology = catalog.topology;
pub const Owner = struct {
    state: a.DisplayControlStatus = .{ .revision = 1 },
    before: ?topology.Layout = null,
    desired: ?topology.Layout = null,
    mode: catalog.modes.Controller = .{},
    mode_key: ?topology.Key = null,
    mode_started: bool = false,
    configured: bool = false,
    confirming: bool = false,
    restoring: bool = false,
    restoration: catalog.modes.Restoration = .{},
    color_restoration: catalog.color_control.Restoration = .{},
    color_recovery: catalog.color_control.Restoration = .{},
    mode_color: ?a.GfxColorSignal = null,
    color_choice: ?catalog.color_preferences.Choice = null,
    next_transaction: u64 = 1,
    next_sync_ns: u64 = 0,
    frame_baseline: [topology.capacity * 2]u64 = @splat(0),
    frame_targets: [topology.capacity * 2]a.GfxOutputTarget = @splat(.{}),

    pub fn busy(self: *const Owner) bool { return policy.active(self.state.phase); }
    pub fn step(self: *Owner, ctx: *api.Context, manager: *outputs.Manager, service_available: bool) bool {
        const now = ctx.sys.monotonicNanoseconds() orelse {
            if (self.busy()) self.rollback(ctx, manager, -3); return false;
        };
        const was_busy = self.busy();
        // Lost native work remains owned by the common mode service. Keep
        // following its eventual receipt even after the UI reports failure.
        if (!was_busy and self.mode.busy()) _ = self.mode.poll(&ctx.draw);
        if (was_busy) {
            if (!service_available) self.rollback(ctx, manager, -3)
            else if (self.state.deadline_ns != 0 and now >= self.state.deadline_ns) self.rollback(ctx, manager, if (self.state.phase == 2) 0 else -3);
            self.progress(ctx, manager, now);
        }
        const layout = policy.encode(&manager.layout);
        const changed = !std.meta.eql(layout, self.state.layout);
        if (changed) { self.state.layout = layout; self.state.revision +|= 1; }
        self.state.desktop_epoch = ctx.self_handle.generation;
        self.state.flags = @as(u32, @intFromBool(manager.active())) | (@as(u32, @intFromBool(policy.persistent(&manager.layout))) << 1);
        if (!service_available or (!changed and now < self.next_sync_ns)) return changed;
        self.next_sync_ns = now +| (if (self.busy()) @as(u64, 100) else 500) * std.time.ns_per_ms;
        var color_reply: a.DisplayColorExchange = .{};
        const request: a.DisplayColorExchange = .{ .base = .{ .desktop_owner = ctx.self_handle, .status = self.state } };
        const rc = ctx.displayColorExchange(&request, &color_reply);
        const reply = color_reply.base;
        if (rc != 0 or reply.magic != a.display_control_exchange_magic or reply.version != 1 or reply.size != @sizeOf(a.DisplayControlExchange) or
            reply.status.desktop_epoch != self.state.desktop_epoch) {
            if (self.busy()) self.rollback(ctx, manager, -3);
            return changed;
        }
        if (reply.flags & 1 != 0 and was_busy) self.rollback(ctx, manager, -4);
        if (reply.request.action != 0) self.handle(ctx, manager, &reply.request, color_reply.color, now)
        else if (!self.busy() and !self.mode.busy() and manager.active() and !manager.reconcile) {
            if (!self.restoreColor(ctx, manager, now)) self.restore(ctx, manager, now);
        }
        return changed;
    }
    fn handle(self: *Owner, ctx: *api.Context, manager: *outputs.Manager, request: *const a.DisplayControlRequest, color: a.DisplayColorSelection, now: u64) void {
        if (request.request_id == self.state.request_id and std.meta.eql(request.owner, self.state.owner)) return;
        // Preserve the transaction owner: another caller cannot acknowledge
        // or cancel a pending change even if it knows the numeric ticket.
        if (self.busy() and !std.meta.eql(request.owner, self.state.owner)) return;
        self.state.owner = request.owner; self.state.request_id = request.request_id;
        if (request.desktop_epoch != self.state.desktop_epoch or request.base_revision != self.state.revision) { self.state.result = -4; return; }
        switch (request.action) {
            1, 4 => {
                if (self.busy() or self.next_transaction == std.math.maxInt(u64)) { self.state.result = -2; return; }
                const decoded = policy.decode(&request.layout) catch { self.reject(-1); return; };
                const desired = policy.normalized(decoded) catch { self.reject(-1); return; };
                if (desired.revision != manager.revision or desired.count != manager.layout.count) { self.reject(-4); return; }
                if (self.mode.busy() or !self.mode.cleanup(&ctx.draw)) { self.reject(-2); return; }
                self.mode = .{}; self.mode_key = null; self.mode_color = null; self.color_choice = null;
                self.mode_started = false; self.configured = false; self.confirming = false;
                if (request.action == 4) {
                    if (!policy.matches(&manager.layout, &desired) or color.version != 1 or color.size != @sizeOf(a.DisplayColorSelection)) { self.reject(-4); return; }
                    const entry = for (manager.snapshot.entries[0..manager.snapshot.count]) |entry| {
                        if (std.meta.eql(entry.info.identity, color.output)) break entry;
                    } else { self.reject(-4); return; };
                    const index = policy.find(&manager.layout, entry.key) orelse { self.reject(-4); return; };
                    if (!self.selectMode(ctx, manager, manager.layout.outputs[index]) or !self.selectColor(ctx, manager, entry, color.signal)) { self.reject(-7); return; }
                    var choice = manager.saved_colors.find(entry.key) orelse catalog.color_preferences.Choice{ .key = entry.key };
                    choice.signal = color.signal;
                    choice.validate() catch { self.reject(-7); return; };
                    self.color_choice = choice;
                    self.color_restoration.record(choice, entry.info.identity);
                } else if (!std.meta.eql(color, a.DisplayColorSelection{})) { self.reject(-1); return; }
                for (desired.outputs[0..desired.count]) |choice| {
                    const index = policy.find(&manager.layout, choice.key) orelse { self.reject(-4); return; };
                    const old = manager.layout.outputs[index];
                    if (!catalog.modes.sameTiming(old, choice)) {
                        // A common native mode ticket currently names one
                        // output. Reject a batch before any physical change.
                        if (self.mode_key != null or !self.selectMode(ctx, manager, choice)) { self.reject(-7); return; }
                    }
                }
                if (request.action == 1 and !self.preserveColor(ctx, manager)) { self.reject(-7); return; }
                // A user's test supersedes automatic restoration for this
                // connected set, including a later user-requested rollback.
                for (manager.snapshot.entries[0..manager.snapshot.count]) |entry| if (manager.saved.find(entry.key)) |saved|
                    self.restoration.record(saved, entry.info.identity);
                self.start(manager, desired, now, false);
            },
            2 => {
                if (request.transaction_id != self.state.transaction_id or self.state.phase != 2 or now >= self.state.deadline_ns) { self.state.result = -4; return; }
                if (self.mode_started) {
                    if (!self.mode.resolve(&ctx.draw, true)) { self.state.result = self.mode.error_code; return; }
                    self.confirming = true; self.state.phase = 1; self.state.deadline_ns = now +| policy.operation_ns;
                } else self.keep(manager);
            },
            3 => {
                if (request.transaction_id != self.state.transaction_id) { self.state.result = -4; return; }
                self.rollback(ctx, manager, 0);
            },
            else => self.state.result = -1,
        }
    }
    fn start(self: *Owner, manager: *outputs.Manager, desired: topology.Layout, now: u64, restoring: bool) void {
        self.before = manager.layout; self.desired = desired; self.restoring = restoring;
        self.mode_started = false; self.configured = false; self.confirming = false;
        self.state.transaction_id = self.next_transaction; self.next_transaction += 1;
        self.state.phase = 1; self.state.result = 0; self.state.deadline_ns = now +| policy.operation_ns;
        for (manager.slots, 0..) |slot, i| { self.frame_targets[i] = slot.target; self.frame_baseline[i] = slot.reported; }
        if (self.mode_key == null) self.configure(manager);
    }
    fn restore(self: *Owner, ctx: *api.Context, manager: *outputs.Manager, now: u64) void {
        if (self.next_transaction == std.math.maxInt(u64) or !self.mode.cleanup(&ctx.draw)) return;
        for (manager.snapshot.entries[0..manager.snapshot.count]) |entry| {
            const saved = manager.saved.find(entry.key) orelse continue;
            const index = policy.find(&manager.layout, entry.key) orelse continue;
            if (!saved.enabled or catalog.modes.sameTiming(manager.layout.outputs[index], saved) or
                self.restoration.seen(saved, entry.info.identity) or entry.info.limits.flags & a.gfx_output_limit_modeset == 0 or
                entry.info.flags & a.gfx_output_flag_fixed_geometry != 0 or entry.info.mode_count == 0) continue;
            self.mode = .{}; self.mode_key = null; self.mode_color = null; self.color_choice = null;
            if (!self.selectMode(ctx, manager, saved)) {
                // A complete admitted catalog without this timing is final
                // for this connection. Transient discovery may be retried.
                if (self.mode.error_code == 0 and self.mode.count != 0) self.restoration.record(saved, entry.info.identity);
                continue;
            }
            if (!self.preserveColor(ctx, manager)) { self.restoration.record(saved, entry.info.identity); continue; }
            var desired = manager.layout;
            desired.outputs[index].view.pixel_w = saved.view.pixel_w;
            desired.outputs[index].view.pixel_h = saved.view.pixel_h;
            desired.outputs[index].refresh_millihz = saved.refresh_millihz;
            desired = topology.Layout.init(desired.outputs[0..desired.count], desired.revision) catch blk: {
                // Restore timings one output at a time. Intermediate sizes
                // can invalidate a clone or old arrangement; keep it reachable.
                var x: i32 = 0;
                for (desired.outputs[0..desired.count]) |*value| {
                    value.clone_group = 0; value.view.origin = .{ .x = x };
                    x += @intCast((value.view.logical() catch return).w);
                }
                break :blk policy.normalized(desired) catch return;
            };
            self.restoration.record(saved, entry.info.identity);
            self.state.owner = ctx.self_handle; self.state.request_id = 0;
            self.start(manager, desired, now, true);
            return;
        }
    }
    fn selectMode(self: *Owner, ctx: *api.Context, manager: *outputs.Manager, choice: topology.Output) bool {
        const entry = for (manager.snapshot.entries[0..manager.snapshot.count]) |entry| {
            if (std.meta.eql(entry.key, choice.key)) break entry;
        } else return false;
        self.mode.selected_output = entry.info.identity;
        if (!self.mode.refresh(&ctx.draw)) return false;
        for (self.mode.modes[0..self.mode.count], 0..) |mode, index| {
            if (mode.width == choice.view.pixel_w and mode.height == choice.view.pixel_h and
                mode.refresh_millihz == choice.refresh_millihz) {
                self.mode.selected = index; self.mode_key = choice.key; return true;
            }
        }
        return false;
    }
    fn configure(self: *Owner, manager: *outputs.Manager) void {
        manager.configure(self.desired); self.configured = true;
        for (&manager.slots) |*slot| slot.paused = false;
    }
    fn selectColor(self: *Owner, ctx: *api.Context, manager: *outputs.Manager, entry: catalog.Entry, signal: a.GfxColorSignal) bool {
        _ = catalog.color.requestedSignal(signal) catch return false;
        if (std.meta.eql(signal, catalog.color_preferences.sdr) and
            (entry.color == null or catalog.color_control.canonical(entry.color.?))) { self.mode_color = null; return true; }
        const cpu_color = catalog.color.profileEncoding(signal) and entry.presentation.flags & a.display_presentation_info_system_source != 0;
        if ((!cpu_color and !manager.colorReady(entry)) or self.mode.count == 0) return false;
        catalog.color_control.validate(&ctx.draw, &entry, self.mode.modes[self.mode.selected], signal) catch return false;
        _ = gfx.ColorV1Client.init(manager.raw) catch return false;
        self.mode_color = signal; return true;
    }
    fn preserveColor(self: *Owner, ctx: *api.Context, manager: *outputs.Manager) bool {
        const key = self.mode_key orelse return true;
        const entry = for (manager.snapshot.entries[0..manager.snapshot.count]) |entry| {
            if (std.meta.eql(entry.key, key)) break entry;
        } else return false;
        const state = entry.color orelse return true;
        if (catalog.color_control.canonical(state)) return true;
        const metadata: ?catalog.color.Metadata = if (state.transfer == 1) null else .{};
        var signal = catalog.color.signalRequest(a.GfxColorSignal, catalog.color.publishedSignal(state, metadata) catch return false) catch return false;
        if (manager.saved_colors.find(key)) |choice| if (catalog.color_control.matches(state, choice.signal)) { signal = choice.signal; };
        return self.selectColor(ctx, manager, entry, signal);
    }
    fn restoreColor(self: *Owner, ctx: *api.Context, manager: *outputs.Manager, now: u64) bool {
        if (self.next_transaction == std.math.maxInt(u64) or !self.mode.cleanup(&ctx.draw)) return false;
        for (manager.snapshot.entries[0..manager.snapshot.count]) |entry| {
            const state = entry.color orelse continue;
            const saved = manager.saved_colors.find(entry.key);
            const recovery = manager.softwareOnly(entry.target) and !(state.flags & 7 == 7 and catalog.color.profileEncoding(state));
            const fallback: catalog.color_preferences.Choice = .{ .key = entry.key };
            if (recovery and self.color_recovery.seen(fallback, entry.info.identity)) continue;
            if (!recovery and (saved == null or catalog.color_control.matches(state, saved.?.signal) or
                self.color_restoration.seen(saved.?, entry.info.identity) or manager.softwareOnly(entry.target))) continue;
            if (!recovery and !std.meta.eql(saved.?.signal, catalog.color_preferences.sdr) and !manager.colorReady(entry)) continue;
            const index = policy.find(&manager.layout, entry.key) orelse continue;
            if (!manager.layout.outputs[index].enabled) continue;
            self.mode = .{}; self.mode_key = null; self.mode_color = null; self.color_choice = null;
            if (!self.selectMode(ctx, manager, manager.layout.outputs[index])) continue;
            const signal = if (recovery) catalog.color_preferences.sdr else saved.?.signal;
            if (!self.selectColor(ctx, manager, entry, signal)) {
                if (saved) |choice| self.color_restoration.record(choice, entry.info.identity);
                continue;
            }
            if (saved) |choice| self.color_restoration.record(choice, entry.info.identity);
            if (recovery) self.color_recovery.record(fallback, entry.info.identity);
            self.state.owner = ctx.self_handle; self.state.request_id = 0;
            self.start(manager, manager.layout, now, true);
            return true;
        }
        return false;
    }
    fn progress(self: *Owner, ctx: *api.Context, manager: *outputs.Manager, now: u64) void {
        if (!self.busy()) return;
        if (self.mode_started) _ = self.mode.poll(&ctx.draw);
        if (self.state.phase == 3) {
            if (self.mode_started and self.mode.status.phase == a.gfx_mode_phase_lost) {
                // The kernel retains unresolved buffers. Report a failed
                // restore, never successful rollback or invented quiescence.
                self.state.phase = 6; self.state.result = -3; self.state.deadline_ns = 0;
                self.before = null; self.desired = null; self.restoring = false;
                return;
            }
            if ((!self.mode_started or !self.mode.busy()) and !manager.reconcile and manager.active() and settled(manager)) {
                self.state.phase = 5; self.state.deadline_ns = 0;
                self.before = null; self.desired = null; self.restoring = false;
            }
            return;
        }
        if (self.confirming) {
            if (self.mode.status.phase == a.gfx_mode_phase_confirmed) self.keep(manager)
            else if (self.mode.status.phase == a.gfx_mode_phase_reverted or self.mode.status.phase == a.gfx_mode_phase_lost) self.rollback(ctx, manager, -3);
            return;
        }
        if (self.mode_started and (self.mode.status.phase == a.gfx_mode_phase_reverted or self.mode.status.phase == a.gfx_mode_phase_lost)) {
            self.rollback(ctx, manager, self.mode.error_code); return;
        }
        if (!self.mode_started) if (self.mode_key) |key| {
            var waiting = false;
            for (&manager.slots) |*slot| {
                const index = slot.logical_index orelse continue;
                if (!std.meta.eql(manager.layout.outputs[index].key, key)) continue;
                slot.paused = true;
                if (slot.gpu) |gpu| if (gpu.busy()) { waiting = true; };
                if (slot.software) |cpu| if (cpu.pending or cpu.acquired != null) { waiting = true; };
            }
            if (waiting) return;
            const accepted = if (self.mode_color) |signal| blk: {
                const colors = gfx.ColorV1Client.init(manager.raw) catch { self.rollback(ctx, manager, -7); return; };
                break :blk self.mode.applyColor(gfx, &ctx.draw, colors, signal);
            } else self.mode.apply(&ctx.draw);
            if (!accepted) {
                if (self.mode.error_code != a.gfx_output_error_busy) self.rollback(ctx, manager, self.mode.error_code);
                return;
            }
            self.mode_started = true;
        };
        if (self.mode_started and self.mode.status.phase != a.gfx_mode_phase_awaiting_confirmation) return;
        if (!self.configured) self.configure(manager);
        const desired = self.desired orelse return;
        if (!policy.matches(&manager.layout, &desired)) return;
        for (&manager.slots, 0..) |*slot, i| if (slot.logical_index != null) {
            if (slot.failed) { self.rollback(ctx, manager, -3); return; }
            const before = if (std.meta.eql(self.frame_targets[i], slot.target)) self.frame_baseline[i] else 0;
            if (slot.reported <= before) return;
        };
        if (self.state.phase == 1) {
            if (self.restoring) {
                // This exact timing was previously confirmed by the user.
                // Confirm only after the actual mode receipt and new frames.
                if (!self.mode.resolve(&ctx.draw, true)) { self.rollback(ctx, manager, self.mode.error_code); return; }
                self.confirming = true; self.state.deadline_ns = now +| policy.operation_ns;
                return;
            }
            self.state.phase = 2;
            self.state.deadline_ns = now +| policy.confirmation_ns;
            if (self.mode_started) self.state.deadline_ns = @min(self.state.deadline_ns, self.mode.status.confirmation_deadline_ns);
        }
    }
    fn settled(manager: *const outputs.Manager) bool {
        for (&manager.slots) |*slot| if (slot.logical_index != null) {
            if (slot.failed or slot.dirty() or slot.reported == 0) return false;
            if (slot.software) |owner| if (owner.pending or owner.acquired != null) return false;
            if (slot.gpu) |owner| if (owner.busy()) return false;
        };
        return true;
    }
    fn keep(self: *Owner, manager: *outputs.Manager) void {
        if (!self.restoring) if (self.color_choice) |choice| manager.saveColorPreferences(choice) catch {
            self.state.result = -6; self.state.phase = 4; self.state.deadline_ns = 0;
            self.before = null; self.desired = null; self.confirming = false; self.color_choice = null; return;
        };
        if (self.desired) |desired| {
            if (!policy.matches(&manager.layout, &desired)) { self.state.result = -4; return; }
            // Unidentified receivers can be kept for this session; the UI
            // reports that limitation from the explicit persistent flag.
            if (!self.restoring and self.color_choice == null and policy.persistent(&manager.layout)) manager.savePreferences(&manager.layout) catch {
                self.state.result = -6; self.state.phase = 4; self.state.deadline_ns = 0;
                self.before = null; self.desired = null; self.confirming = false; return;
            };
        }
        if (self.restoring) self.restoreArrangement(manager);
        self.state.phase = 4; self.state.result = 0; self.state.deadline_ns = 0;
        self.before = null; self.desired = null; self.confirming = false; self.restoring = false; self.color_choice = null; self.mode_color = null;
    }
    fn restoreArrangement(_: *Owner, manager: *outputs.Manager) void {
        var desired = manager.layout;
        for (desired.outputs[0..desired.count]) |*value| {
            const saved = manager.saved.find(value.key) orelse return;
            if (!catalog.modes.sameTiming(value.*, saved)) return;
            value.view.origin = saved.view.origin; value.view.scale = saved.view.scale; value.view.rotation = saved.view.rotation;
            value.clone_group = saved.clone_group; value.enabled = saved.enabled; value.primary = saved.primary;
        }
        // An absent saved primary or a changed set keeps the reachable layout.
        const valid = topology.Layout.init(desired.outputs[0..desired.count], desired.revision) catch return;
        manager.configure(policy.normalized(valid) catch return);
    }
    fn rollback(self: *Owner, ctx: *api.Context, manager: *outputs.Manager, result: i32) void {
        if (!self.busy() or self.state.phase == 3) return;
        if (self.mode_started and !self.confirming) self.mode.close(&ctx.draw);
        manager.configure(self.before);
        for (&manager.slots) |*slot| slot.paused = false;
        self.state.phase = 3; self.state.result = result; self.state.deadline_ns = 0;
        self.confirming = false;
    }
    fn reject(self: *Owner, result: i32) void { self.state.phase = 6; self.state.result = result; self.state.deadline_ns = 0; }
};
