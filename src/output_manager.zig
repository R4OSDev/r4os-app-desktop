//! Desktop output ownership. Discovery is transactional, images are per
//! output, and old receivers retain their workers until jobs really drain.
const std = @import("std");
const r4os = @import("r4os");
const r4std = @import("r4std");
const gfx = @import("r4gfx");
const catalog = @import("r4gfx_desktop_outputs");
pub const topology = catalog.topology;
pub const preferences = catalog.preferences;
pub const color_preferences = catalog.color_preferences;
pub const refresh_preferences = catalog.refresh_preferences;
pub const brightness_preferences = catalog.brightness_preferences;
const geometry = @import("output_geometry.zig");
const surface = @import("surface.zig");
const worker = @import("composition_worker.zig");
const cpu = @import("output_cpu.zig");
const a = r4os.abi;
pub const Slot = struct {
    target: a.GfxOutputTarget = .{},
    color_revision: u64 = 0,
    view: topology.Viewport = .{ .pixel_w = 0, .pixel_h = 0 },
    logical_index: ?usize = null,
    disabled: bool = false,
    paused: bool = false,
    sleeping: bool = false,
    gpu: ?*worker.Worker = null,
    software: ?*cpu.Output = null,
    damage: surface.Dirty = .{},
    failed: bool = false,
    reconfiguring: bool = false,
    retry_after_ns: u64 = 0,
    reported: u64 = 0,
    discarded_reported: u64 = 0,
    refresh: catalog.refresh_client.Client = .{},
    brightness: catalog.brightness_client.Client = .{},
    activity: catalog.refresh_client.Activity = .{},
    pub fn occupied(self: *const Slot) bool { return self.target.connector_id != 0; }
    pub fn bounds(self: *const Slot) surface.Rect { return geometry.logical(self.view) catch unreachable; }
    pub fn dirty(self: *const Slot) bool { return self.logical_index != null and !self.failed and !self.paused and !self.sleeping and self.damage.active; }
    pub fn invalidate(self: *Slot) void { self.damage.invalidate(self.bounds()); }
};
pub const Manager = struct {
    allocator: std.mem.Allocator,
    raw: *const a.R4XStartContext,
    sys: r4os.r4sys.Context,
    draw: r4os.r4draw.Context,
    // At most eight current outputs plus one retained old set. A hotplug
    // storm cannot grow an unbounded list of quarantined workers.
    slots: [topology.capacity * 2]Slot = @splat(.{}),
    layout: topology.Layout = .{},
    snapshot: catalog.Snapshot = .{},
    saved: preferences.Config = .{},
    saved_colors: color_preferences.Config = .{},
    saved_refresh: refresh_preferences.Config = .{},
    saved_brightness: brightness_preferences.Config = .{},
    desired: ?topology.Layout = null,
    translation: topology.Point = .{},
    revision: u64 = 0,
    retry_ns: u64 = 0,
    reconcile: bool = true,
    discovery_error: ?anyerror = null,
    reported_failure: a.GfxOutputTarget = .{},
    software_targets: [topology.capacity]a.GfxOutputTarget = @splat(.{}),
    motion: ?[2]u32 = null,
    cursor: topology.Point = .{},

    pub fn create(allocator: std.mem.Allocator, raw: *const a.R4XStartContext, sys: r4os.r4sys.Context, draw: r4os.r4draw.Context) ?*Manager {
        @import("startup_diagnosis.zig").initialize(sys);
        const self = allocator.create(Manager) catch return null;
        self.* = .{ .allocator = allocator, .raw = raw, .sys = sys, .draw = draw };
        self.loadPreferences();
        self.reloadColors();
        self.reloadRefresh();
        self.reloadBrightness();
        return self;
    }
    pub fn reloadRefresh(self: *Manager) void {
        if (r4std.config.recoverDocumentSave(&self.sys, refresh_preferences.path) < 0) {
            self.sys.println("R4DESK refresh preferences: save recovery failed"); return;
        }
        var bytes: [refresh_preferences.max_bytes]u8 = undefined;
        const count = self.sys.fileRead(refresh_preferences.path, &bytes);
        var next: refresh_preferences.Config = .{};
        if (count != -3) {
            if (count <= 0 or count > bytes.len) { self.sys.println("R4DESK refresh preferences: read failed"); return; }
            next = refresh_preferences.Config.parse(bytes[0..@intCast(count)]) catch {
                self.sys.println("R4DESK refresh preferences: invalid document"); return;
            };
        }
        self.saved_refresh = next;
    }
    pub fn reloadBrightness(self: *Manager) void {
        if (r4std.config.recoverDocumentSave(&self.sys, brightness_preferences.path) < 0) {
            self.sys.println("R4DESK brightness preferences: save recovery failed"); return;
        }
        var bytes: [brightness_preferences.max_bytes]u8 = undefined;
        const count = self.sys.fileRead(brightness_preferences.path, &bytes);
        var next: brightness_preferences.Config = .{};
        if (count != -3) {
            if (count <= 0 or count > bytes.len) { self.sys.println("R4DESK brightness preferences: read failed"); return; }
            next = brightness_preferences.Config.parse(bytes[0..@intCast(count)]) catch {
                self.sys.println("R4DESK brightness preferences: invalid document"); return;
            };
        }
        self.saved_brightness = next;
    }
    pub fn contentFrame(self: *Manager, rect: surface.Rect) void {
        const now = self.sys.monotonicNanoseconds() orelse return;
        for (&self.slots) |*slot| if (slot.logical_index != null and !slot.disabled and
            geometry.intersect(slot.bounds(), rect) != null) slot.activity.frame(now);
    }
    pub fn reloadColors(self: *Manager) void {
        if (r4std.config.recoverDocumentSave(&self.sys, color_preferences.path) < 0) {
            self.sys.println("R4DESK color profiles: save recovery failed"); return;
        }
        var bytes: [color_preferences.max_bytes]u8 = undefined;
        const count = self.sys.fileRead(color_preferences.path, &bytes);
        var next: color_preferences.Config = .{};
        if (count != -3) {
            if (count <= 0 or count > bytes.len) { self.sys.println("R4DESK color profiles: read failed"); return; }
            next = color_preferences.Config.parse(bytes[0..@intCast(count)]) catch {
                self.sys.println("R4DESK color profiles: invalid document"); return;
            };
        }
        if (std.meta.eql(self.saved_colors, next)) return;
        // Old images finish under their original owner before a new profile
        // can acquire this output. Reconfiguration is not a GPU failure.
        for (&self.slots) |*slot| if (slot.occupied()) {
            const entry = for (self.snapshot.entries[0..self.snapshot.count]) |value| {
                if (std.meta.eql(value.target, slot.target)) break value;
            } else continue;
            if (std.meta.eql(self.saved_colors.find(entry.key), next.find(entry.key))) continue;
            slot.reconfiguring = true; slot.logical_index = null;
        };
        self.saved_colors = next; self.reconcile = true;
    }
    fn loadPreferences(self: *Manager) void {
        if (r4std.config.recoverDocumentSave(&self.sys, preferences.path) < 0) {
            self.sys.println("R4DESK display preferences: save recovery failed"); return;
        }
        var bytes: [preferences.max_bytes]u8 = undefined;
        const count = self.sys.fileRead(preferences.path, &bytes);
        if (count == -3) return;
        if (count <= 0 or count > bytes.len) {
            self.sys.println("R4DESK display preferences: read failed"); return;
        }
        self.saved = preferences.Config.parse(bytes[0..@intCast(count)]) catch {
            self.sys.println("R4DESK display preferences: invalid document"); return;
        };
    }
    /// Only the Desktop confirmation owner calls this after a tested layout
    /// is kept. Session-only/unknown receivers fail before changing the file.
    pub fn savePreferences(self: *Manager, layout: *const topology.Layout) !void {
        const next = try self.saved.remember(layout);
        var bytes: [preferences.max_bytes]u8 = undefined;
        const encoded = try next.encode(&bytes);
        if (r4std.config.saveDocument(&self.sys, preferences.path, encoded) < 0) return error.Save;
        self.saved = next;
    }
    pub fn saveColorPreferences(self: *Manager, choice: color_preferences.Choice) !void {
        // Merge with the current document: Appearance may have saved an ICC
        // choice for another monitor while this confirmation was pending.
        if (r4std.config.recoverDocumentSave(&self.sys, color_preferences.path) < 0) return error.Save;
        var bytes: [color_preferences.max_bytes]u8 = undefined;
        const count = self.sys.fileRead(color_preferences.path, &bytes);
        const previous: color_preferences.Config = if (count == -3) .{} else blk: {
            if (count <= 0 or count > bytes.len) return error.Save;
            break :blk try color_preferences.Config.parse(bytes[0..@intCast(count)]);
        };
        const next = try previous.remember(choice);
        const encoded = try next.encode(&bytes);
        if (r4std.config.saveDocument(&self.sys, color_preferences.path, encoded) < 0) return error.Save;
        self.saved_colors = next;
    }
    pub fn softwareOnly(self: *const Manager, target: a.GfxOutputTarget) bool {
        for (self.software_targets) |failed| if (failed.connector_id != 0 and failed.adapter_id == target.adapter_id and
            failed.connector_id == target.connector_id and failed.head_id == target.head_id and
            failed.device_generation == target.device_generation and failed.connection_generation == target.connection_generation) return true;
        return false;
    }
    pub fn colorReady(self: *Manager, entry: catalog.Entry) bool {
        if (self.softwareOnly(entry.target) or entry.presentation.flags & a.display_presentation_info_native == 0) return false;
        if (self.saved_colors.find(entry.key)) |choice| if (choice.enabled()) return false;
        for (&self.slots) |*slot| if (!slot.failed and !slot.reconfiguring and std.meta.eql(slot.target, entry.target)) {
            if (slot.gpu) |owner| if (owner.graphics.info()) |info| {
                const required = owner.engine.requiredOperations() | gfx.device_gpu_color;
                return info.gpu_operations & required == required;
            };
        };
        return false;
    }
    pub fn configure(self: *Manager, layout: ?topology.Layout) void {
        self.desired = layout; self.reconcile = true;
    }
    pub fn active(self: *const Manager) bool { return self.layout.count != 0; }
    pub fn refresh(self: *Manager, revision: u64) bool {
        self.poll();
        const now = self.sys.monotonicNanoseconds() orelse 0;
        if (revision == self.revision and !self.reconcile and now < self.retry_ns) return false;
        self.retry_ns = now +| std.time.ns_per_s;
        const next = catalog.Snapshot.read(&self.draw) catch |err| {
            if (if (self.discovery_error) |previous| previous != err else true) {
                self.sys.write("R4DESK output discovery: "); self.sys.println(@errorName(err));
                self.discovery_error = err;
            }
            return false;
        };
        self.discovery_error = null;
        if (next.sameOutputs(&self.snapshot) and !self.reconcile) return false;
        var values: [topology.capacity]topology.Output = undefined;
        var owners: [topology.capacity]*Slot = undefined;
        var count: usize = 0;
        var x: i32 = 0;
        var primary: ?usize = null;
        for (next.entries[0..next.count]) |entry| {
            if (entry.info.flags & a.gfx_output_flag_sleeping != 0) {
                // Preserve placement while CPU/GPU workers retire normally.
                // The new awake target will acquire fresh workers and images.
                for (&self.slots) |*slot| {
                    const previous_index = slot.logical_index orelse continue;
                    if (slot.target.adapter_id != entry.info.identity.adapter_id or slot.target.connector_id != entry.info.identity.connector_id or
                        slot.target.device_generation != entry.info.identity.device_generation or previous_index >= self.layout.count) continue;
                    const previous = self.layout.outputs[previous_index];
                    slot.sleeping = true;
                    values[count] = previous; owners[count] = slot;
                    if (previous.primary and previous.enabled) primary = count;
                    const bounds = previous.view.logical() catch continue;
                    x = @intCast(@min(std.math.maxInt(i32), @max(@as(i64, x), bounds.right())));
                    count += 1;
                    break;
                }
                continue;
            }
            if (!entry.active() or entry.presentation.flags & a.display_presentation_info_native == 0) continue;
            // A failed target stays quarantined until its owners drain and
            // its retry deadline expires. Other outputs continue meanwhile.
            const quarantined = for (&self.slots) |*slot| {
                if ((slot.failed or slot.reconfiguring) and std.meta.eql(slot.target, entry.target)) break true;
            } else false;
            if (quarantined) continue;
            var value: topology.Output = .{ .key = entry.key,
                .view = .{ .pixel_w = entry.presentation.width, .pixel_h = entry.presentation.height, .origin = .{ .x = x } },
                .refresh_millihz = if (entry.presentation.interval_ns != 0) @intCast(@min(1_000_000, 1_000_000_000_000 / entry.presentation.interval_ns)) else 0 };
            var was_primary = false;
            var found = false;
            if (self.desired) |desired| for (desired.outputs[0..desired.count]) |choice| {
                if (!std.meta.eql(choice.key, entry.key)) continue;
                value.view.origin = choice.view.origin; value.view.rotation = choice.view.rotation; value.view.scale = choice.view.scale;
                value.clone_group = choice.clone_group; value.enabled = choice.enabled;
                was_primary = choice.primary; found = true; break;
            };
            for (self.layout.outputs[0..self.layout.count], 0..) |old, old_index| {
                if (found) break;
                if (!std.meta.eql(old.key, entry.key)) continue;
                value.view.origin = old.view.origin; value.view.rotation = old.view.rotation; value.view.scale = old.view.scale;
                value.clone_group = old.clone_group; value.enabled = old.enabled;
                was_primary = self.layout.primary == old_index;
                found = true;
                break;
            }
            if (!found) if (self.saved.find(entry.key)) |choice| {
                // Native modes are restored by the confirmation owner. The
                // initial layout always uses the driver's actual geometry.
                value.view.origin = choice.view.origin; value.view.rotation = choice.view.rotation; value.view.scale = choice.view.scale;
                value.clone_group = choice.clone_group; value.enabled = choice.enabled; was_primary = choice.primary;
            };
            const bounds = value.view.logical() catch continue;
            const slot = self.acquire(entry, value.view) orelse continue;
            if (was_primary and value.enabled) primary = count;
            x = @intCast(@min(std.math.maxInt(i32), @max(@as(i64, x), bounds.right())));
            values[count] = value; owners[count] = slot; count += 1;
        }
        var layout: topology.Layout = .{};
        var translation: topology.Point = .{};
        if (count != 0) {
            const selected = primary orelse for (values[0..count], 0..) |value, i| {
                if (value.enabled) break i;
            } else blk: {
                // The saved primary may be absent. Always restore one
                // reachable output instead of booting an all-black desktop.
                values[0].enabled = true; break :blk 0;
            };
            translation = normalize(values[0..count], selected) catch .{};
            layout = topology.Layout.init(values[0..count], next.revision) catch blk: {
                // Changed native dimensions can invalidate an old placement.
                // Keep every admitted output reachable with a bounded default.
                var right: i32 = 0;
                for (values[0..count]) |*value| {
                    value.clone_group = 0; value.view.origin = .{ .x = right };
                    right += @intCast((value.view.logical() catch unreachable).w);
                }
                translation = normalize(values[0..count], selected) catch unreachable;
                break :blk topology.Layout.init(values[0..count], next.revision) catch unreachable;
            };
        }
        self.translation = translation; self.layout = layout;
        self.snapshot = next; self.revision = next.revision;
        self.reconcile = false;
        for (&self.slots) |*slot| slot.logical_index = null;
        for (values[0..count], 0..) |value, i| {
            const slot = owners[i]; slot.view = value.view;
            if (slot.software) |owner| owner.view = value.view;
            slot.logical_index = i; slot.disabled = !value.enabled; slot.invalidate();
        }
        self.poll();
        return true;
    }
    fn normalize(values: []topology.Output, selected: usize) !topology.Point {
        const origin = values[selected].view.origin;
        // Validate every subtraction before changing the candidate layout.
        for (values) |value| {
            _ = try std.math.sub(i32, value.view.origin.x, origin.x);
            _ = try std.math.sub(i32, value.view.origin.y, origin.y);
        }
        for (values, 0..) |*value, i| {
            value.primary = i == selected;
            value.view.origin.x -= origin.x; value.view.origin.y -= origin.y;
        }
        return origin;
    }
    fn acquire(self: *Manager, entry: catalog.Entry, view: topology.Viewport) ?*Slot {
        const slot = for (&self.slots) |*current| {
            if (!current.failed and !current.reconfiguring and std.meta.eql(current.target, entry.target) and
                current.color_revision == (if (entry.color) |value| value.revision else @as(u64, 0)) and
                current.view.pixel_w == view.pixel_w and current.view.pixel_h == view.pixel_h) break current;
        } else for (&self.slots) |*current| { if (!current.occupied()) break current; } else return null;
        if (slot.occupied()) {
            if (slot.sleeping) {
                if (slot.gpu != null or slot.software != null) { slot.reconfiguring = true; return null; }
                slot.* = .{};
            } else {
            if (slot.software) |owner| if (owner.acquired != null or owner.mapping.lease.id != 0) {
                self.fail(slot); return null;
            };
            return slot;
            }
        }
        slot.target = entry.target; slot.view = view;
        slot.color_revision = if (entry.color) |value| value.revision else 0;
        const choice = self.saved_colors.find(entry.key);
        const profile_requested = if (choice) |value| value.enabled() else false;
        const profile_supported = if (entry.color) |color| color.flags & 7 == 7 and
            catalog.color.profileEncoding(color) else false;
        // The general ICC path runs on the CPU until the GPU advertises the
        // matching profile transform. Ordinary SDR keeps its native worker.
        const profile_enabled = profile_requested and profile_supported;
        if (profile_requested and !profile_supported) self.sys.println("R4DESK color profile unavailable for this output encoding");
        const software_only = self.softwareOnly(entry.target);
        if (!software_only and !profile_enabled) slot.gpu = worker.Worker.createForOutput(self.allocator, self.raw, self.sys,
            entry.target.adapter_id, entry.target.head_id, entry.color, entry.presentation.format);
        const accelerated = if (slot.gpu) |owner| blk: {
            const info = owner.graphics.info() orelse break :blk false;
            const needed = owner.engine.requiredOperations();
            @import("startup_diagnosis.zig").record("candidate head={d} display={d} backend={d} operations={x} required={x}",
                .{entry.target.head_id, entry.target.display_generation, info.backend, info.gpu_operations, needed});
            break :blk info.gpu_operations & needed == needed;
        } else false;
        @import("startup_diagnosis.zig").record("select head={d} display={d} flags={x} worker={} accelerated={} failed-target={} icc={} size={d}x{d}",
            .{entry.target.head_id, entry.target.display_generation, entry.presentation.flags, slot.gpu != null, accelerated,
                software_only, profile_enabled, view.pixel_w, view.pixel_h});
        if (!accelerated) {
            if (slot.gpu) |owner| if (owner.tryDestroy()) { slot.gpu = null; };
            if (slot.gpu == null and entry.presentation.format == a.gfx_buffer_format_xrgb8888 and
                (entry.color == null or profile_supported) and entry.presentation.flags & a.display_presentation_info_system_source != 0)
                slot.software = cpu.Output.create(self.allocator, self.raw, self.draw, self.sys, view, entry.target);
            if (slot.software == null or slot.software.?.lost) {
                if (!std.meta.eql(self.reported_failure, entry.target)) {
                    var bytes: [256]u8 = undefined;
                    const owner = slot.software;
                    const text = std.fmt.bufPrint(&bytes, "R4DESK output unavailable: head={d} flags={x} stage={s} result={d} error={s}",
                        .{ entry.target.head_id, entry.presentation.flags, if (owner) |value| value.prepare_stage else "device-create",
                            if (owner) |value| value.last_status else @as(i32, 0),
                            if (owner) |value| if (value.prepare_error) |err| @errorName(err) else "none" else "unavailable" }) catch unreachable;
                    self.sys.println(text); self.reported_failure = entry.target;
                }
                self.fail(slot); return null;
            }
            slot.software.?.limited = if (entry.color) |value| value.range == 2 else false;
            if (profile_enabled) slot.software.?.setProfile(&self.sys, choice.?) catch |err| {
                self.sys.write("R4DESK color profile unavailable; using SDR: "); self.sys.println(@errorName(err));
            };
        }
        return slot;
    }
    pub fn fail(self: *Manager, slot: *Slot) void {
        if (!slot.failed) @import("startup_diagnosis.zig").record("manager-fail head={d} display={d} gpu={} cpu={} reconfiguring={}",
            .{slot.target.head_id, slot.target.display_generation, slot.gpu != null, slot.software != null, slot.reconfiguring});
        slot.failed = true;
        if (slot.gpu != null) {
            const index = for (self.software_targets, 0..) |target, i| {
                if (target.adapter_id == slot.target.adapter_id and target.head_id == slot.target.head_id) break i;
            } else for (self.software_targets, 0..) |target, i| {
                if (target.connector_id == 0) break i;
            } else 0;
            // A failed GPU worker may fall back to this exact generation's
            // admitted CPU source path after its resources really drain.
            self.software_targets[index] = slot.target;
        }
        if (slot.retry_after_ns == 0) {
            slot.retry_after_ns = (self.sys.monotonicNanoseconds() orelse 0) +| std.time.ns_per_s;
            self.reconcile = true;
        }
    }
    pub fn poll(self: *Manager) void {
        const now = self.sys.monotonicNanoseconds() orelse 0;
        for (&self.slots) |*slot| {
            if (!slot.occupied()) continue;
            var power: a.GfxOutputPower = .{};
            const identity: a.GfxOutputId = .{ .adapter_id = slot.target.adapter_id, .connector_id = slot.target.connector_id,
                .device_generation = slot.target.device_generation, .connection_generation = slot.target.connection_generation };
            if (self.draw.outputs().power(&identity, &power) == a.gfx_output_ok)
                slot.sleeping = power.phase >= a.gfx_power_phase_stopping and power.phase <= a.gfx_power_phase_waking;
            if (slot.sleeping and slot.logical_index != null) {
                slot.refresh.release(&self.draw, slot.target);
                if (slot.gpu) |owner| if (owner.tryDestroy()) { slot.gpu = null; };
                if (slot.software) |owner| if (owner.destroy()) { slot.software = null; };
                continue;
            }
            if (slot.logical_index == null or slot.failed or slot.reconfiguring) {
                slot.refresh.release(&self.draw, slot.target);
                if (slot.failed) self.fail(slot);
                if (slot.gpu) |owner| if (owner.tryDestroy()) { slot.gpu = null; };
                if (slot.software) |owner| if (owner.destroy()) { slot.software = null; };
                if (slot.gpu == null and slot.software == null and slot.logical_index == null and now >= slot.retry_after_ns) {
                    slot.* = .{}; self.reconcile = true;
                }
                continue;
            }
            if (slot.gpu) |owner| switch (owner.poll(&self.draw)) {
                .failed => self.fail(slot),
                .discarded => {
                    const completed = owner.completed_status;
                    if (completed == null or completed.?.result != 2) slot.invalidate();
                },
                else => {},
            };
            if (slot.software) |owner| {
                owner.poll(); if (owner.lost) self.fail(slot);
                if (owner.discarded != slot.discarded_reported) { slot.discarded_reported = owner.discarded; slot.invalidate(); }
            }
        }
    }
    pub fn invalidate(self: *Manager, regions: []const surface.Rect) void {
        for (&self.slots) |*slot| if (slot.logical_index != null and !slot.disabled) for (regions) |region| {
            if (geometry.intersect(slot.bounds(), region)) |clipped| slot.damage.invalidate(clipped);
        };
    }
    pub fn invalidateAll(self: *Manager) void { for (&self.slots) |*slot| if (slot.logical_index != null) { slot.invalidate(); }; }
    pub fn immediateWork(self: *const Manager) bool {
        for (&self.slots) |*slot| if (slot.gpu) |owner| if (owner.immediateWork()) return true;
        return false;
    }
    pub fn needsPolling(self: *const Manager) bool {
        for (&self.slots) |*slot| {
            if (slot.logical_index == null and (slot.gpu != null or slot.software != null)) return true;
            if (slot.gpu) |owner| if (owner.needsPolling()) return true;
            if (slot.software) |owner| if (owner.pending or owner.capture.needsPolling()) return true;
        }
        return false;
    }
    pub fn needsCapture(self: *const Manager) bool {
        for (&self.slots) |*slot| if (slot.dirty()) {
            if (slot.gpu) |owner| if (!owner.blocksCapture()) return true;
            if (slot.software) |owner| if (owner.ready and !owner.lost and !owner.pending) return true;
        };
        return false;
    }
    pub fn captureWaitTicks(self: *const Manager, now_ns: u64, hz: u32, limit: u64) u64 {
        var delay = limit;
        for (&self.slots) |*slot| if (slot.dirty()) {
            if (slot.gpu) |owner| delay = owner.engine.captureWaitTicks(now_ns, hz, delay);
        };
        return delay;
    }
    pub fn pointer(self: *Manager, sample: a.MouseMotion, old: topology.Point) topology.Point {
        const totals = [2]u32{ sample.motion_x, sample.motion_y };
        defer self.motion = totals;
        const previous = self.motion orelse { self.cursor = old; return old; };
        const dx: i32 = @bitCast(totals[0] -% previous[0]); const dy: i32 = @bitCast(totals[1] -% previous[1]);
        const candidate: topology.Point = .{ .x = old.x +| dx, .y = old.y +| dy };
        self.cursor = if (self.layout.nearest(candidate)) |value| value.point else old;
        return self.cursor;
    }
    pub fn destroy(self: *Manager) void {
        for (&self.slots) |*slot| slot.logical_index = null;
        self.poll();
        for (&self.slots) |*slot| if (slot.gpu != null or slot.software != null) return;
        self.allocator.destroy(self);
    }
};
