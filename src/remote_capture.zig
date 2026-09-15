//! One demand-driven capture per output. GPU source ownership ends after the
//! copy receipt, before CPU orientation, publication, or any network reader.
const std = @import("std");
const gfx = @import("r4gfx");
const readback = @import("r4gfx_readback");
const geometry = @import("output_geometry.zig");
const surface = @import("surface.zig");
const scene = @import("scene_buffer.zig");
pub const Cursor = struct { x: i32 = 0, y: i32 = 0, visible: bool = false, separate: bool = false };
const Record = struct { key: u64, damage: surface.Rect, view: geometry.topology.Viewport, cursor: Cursor };
pub const Capture = struct {
    reader: readback.Owner,
    cpu_profile: bool = false,
    wanted: bool = false,
    epoch: u64 = 1,
    view: ?geometry.topology.Viewport = null,
    records: [gfx.swapchain_image_capacity]?Record = @splat(null),
    current: ?Record = null,
    changes: surface.Dirty = .{},
    last_seen: u64 = 0,
    redraw: bool = false,
    retry_ns: u64 = 0,
    logical_pixels: []u32 = &.{},
    logical_damage: surface.Rect = .{ .x = 0, .y = 0, .w = 0, .h = 0 },
    orient_offset: usize = 0,
    ready: bool = false,
    skipped: u64 = 0,
    oriented_bytes: u64 = 0,

    pub fn init(allocator: std.mem.Allocator, client: *const gfx.DeviceV1Client,
        colors: *const gfx.ColorV1Client, device: *const gfx.R4GfxDevice) Capture
    { return .{ .reader = readback.Owner.init(allocator, client, colors, device) }; }
    pub fn setDemand(self: *Capture, wanted: bool) void {
        if (self.wanted == wanted) return;
        self.wanted = wanted; self.epoch +%= 1; if (self.epoch == 0) self.epoch = 1;
        self.reader.valid = false; self.changes = .{}; self.last_seen = 0; self.redraw = wanted;
        self.ready = false; self.current = null; self.records = @splat(null);
        self.reader.cancel(error.Stale);
    }
    pub fn invalidate(self: *Capture) void {
        const wanted = self.wanted;
        self.setDemand(false);
        self.setDemand(wanted);
    }
    pub fn prepared(self: *const Capture, view: geometry.topology.Viewport, format: u32, color: gfx.R4GfxColorDescription) bool {
        return self.view != null and std.meta.eql(self.view.?, view) and
            self.reader.matches(view.pixel_w, view.pixel_h, format, color);
    }
    /// Cold output preparation, never a per-frame allocation or GPU wait.
    pub fn prepare(self: *Capture, view: geometry.topology.Viewport, format: u32, color: gfx.R4GfxColorDescription) !void {
        if (self.prepared(view, format, color)) return;
        if (self.reader.pending() or self.current != null or self.ready or self.reader.sourceHeld()) return error.Busy;
        const bounds = try geometry.logical(view);
        const bytes = scene.SceneBuffer.requiredBytes(bounds.w, bounds.h) orelse return error.Limit;
        if (bytes > readback.max_image_bytes) return error.Limit;
        try self.reader.close();
        try self.reader.prepare(view.pixel_w, view.pixel_h, format, color);
        if (view.rotation != .normal or view.scale != 120) {
            if (self.logical_pixels.len != bytes / 4) {
                const replacement = try self.reader.allocator.alloc(u32, bytes / 4);
                self.reader.allocator.free(self.logical_pixels); self.logical_pixels = replacement;
            }
        } else { self.reader.allocator.free(self.logical_pixels); self.logical_pixels = &.{}; }
        self.view = view; self.reader.valid = false; self.changes = .{}; self.last_seen = 0;
    }
    /// ICC output profiles/calibration belong to the monitor, not a capture.
    /// Freeze the CPU compositor's already completed sRGB scene before those
    /// transforms. Publication still requires this frame's present receipt.
    pub fn prepareProfile(self: *Capture, view: geometry.topology.Viewport) !void {
        if (self.cpu_profile and self.view != null and std.meta.eql(self.view.?, view)) return;
        if (self.current != null or self.reader.pending()) return error.Busy;
        try self.close();
        const bounds = try geometry.logical(view);
        const bytes = scene.SceneBuffer.requiredBytes(bounds.w, bounds.h) orelse return error.Limit;
        if (bytes > readback.max_image_bytes) return error.Limit;
        self.logical_pixels = try self.reader.allocator.alloc(u32, bytes / 4);
        self.cpu_profile = true; self.view = view; self.reader.phase = .idle;
    }
    pub fn stageProfile(self: *Capture, slot: usize, pixels: []const u32, now: u64) void {
        if (!self.wanted or !self.cpu_profile) return;
        const value = self.records[slot] orelse return;
        if (self.current != null or self.ready or self.logical_pixels.len != pixels.len) { self.redraw = true; self.skipped +|= 1; return; }
        @memcpy(self.logical_pixels, pixels);
        self.reader.stats.cpu_read_bytes +|= pixels.len * 4; self.reader.stats.cpu_write_bytes +|= pixels.len * 4;
        self.current = value; self.reader.started_ns = now; self.reader.frame = value.key; self.reader.epoch = self.epoch;
        self.logical_damage = .{ .x = 0, .y = 0, .w = (geometry.logical(value.view) catch return).w, .h = (geometry.logical(value.view) catch return).h };
        self.redraw = false;
    }
    pub fn discarded(self: *Capture, slot: usize) void {
        if (self.cpu_profile) if (self.records[slot]) |recorded| if (self.current) |value| if (value.key == recorded.key) {
            self.current = null; self.ready = false; self.redraw = self.wanted;
        };
        self.records[slot] = null;
    }
    pub fn record(self: *Capture, slot: usize, key: u64, damage: surface.Rect, view: geometry.topology.Viewport, pointer: Cursor) void {
        self.records[slot] = .{ .key = key, .damage = damage, .view = view, .cursor = pointer };
    }
    pub fn complete(self: *Capture, slot: usize, source: gfx.R4GfxResource, now: u64) void {
        const value = self.records[slot] orelse return;
        self.records[slot] = null;
        if (!self.wanted) return;
        if (self.cpu_profile) {
            if (self.current) |current| if (current.key == value.key) {
                self.ready = true; self.reader.phase = .ready; self.reader.stats.frames +|= 1;
                self.reader.stats.last_latency_ns = now -| self.reader.started_ns;
                self.reader.stats.max_latency_ns = @max(self.reader.stats.max_latency_ns, self.reader.stats.last_latency_ns);
            };
            self.last_seen = @max(self.last_seen, value.key); return;
        }
        if (value.key <= self.last_seen) return;
        if (value.key != self.last_seen +| 1) self.reader.valid = false;
        self.last_seen = value.key;
        self.changes.invalidate(value.damage);
        if (self.reader.phase != .idle or self.ready or self.view == null or !std.meta.eql(self.view.?, value.view) or now < self.retry_ns) {
            self.skipped +|= 1; self.redraw = true; return;
        }
        const bounds = geometry.logical(value.view) catch return;
        const dirty = if (self.reader.valid) self.changes.bounds else bounds;
        const clipped = geometry.intersect(dirty, bounds) orelse return;
        const native = value.view.physicalDamage(.{ .x = clipped.x, .y = clipped.y, .w = @intCast(clipped.w), .h = @intCast(clipped.h) }) catch return;
        const rect = native orelse return;
        self.reader.begin(.{ .source = source, .epoch = self.epoch, .frame = value.key,
            .base_frame = self.reader.acknowledged, .regions = &.{.{ .x = @intCast(rect.x), .y = @intCast(rect.y), .width = rect.w, .height = rect.h }},
            .now_ns = now, .deadline_ns = now +| 2 * std.time.ns_per_s }) catch {
                self.redraw = true; self.retry_ns = now +| std.time.ns_per_s; return;
            };
        self.current = value;
        self.logical_damage = if (self.reader.region_count == 1 and self.reader.regions[0].width == value.view.pixel_w and
            self.reader.regions[0].height == value.view.pixel_h) bounds else clipped;
        self.logical_damage.x -= bounds.x; self.logical_damage.y -= bounds.y;
        self.changes = .{}; self.orient_offset = 0; self.ready = false; self.redraw = false;
    }
    pub fn poll(self: *Capture, now: u64) void {
        if (self.cpu_profile) { if (!self.wanted) self.close() catch {}; return; }
        self.reader.poll(now);
        if (self.reader.phase == .failed) {
            self.reader.acknowledge(false) catch {}; self.current = null; self.ready = false;
            self.redraw = self.wanted; self.retry_ns = now +| std.time.ns_per_s;
        }
        if (!self.wanted) { self.close() catch {}; return; }
        if (self.reader.phase != .ready or self.ready) return;
        const value = self.current orelse { self.reader.acknowledge(false) catch {}; return; };
        if (self.logical_pixels.len == 0) { self.ready = true; return; }
        const bounds = geometry.logical(value.view) catch return;
        const damage = self.logical_damage;
        const count = @as(usize, @intCast(damage.w)) * @as(usize, @intCast(damage.h));
        const end = @min(count, self.orient_offset + readback.conversion_pixels);
        for (self.orient_offset..end) |i| {
            const x = @as(usize, @intCast(damage.x)) + i % @as(usize, @intCast(damage.w));
            const y = @as(usize, @intCast(damage.y)) + i / @as(usize, @intCast(damage.w));
            self.logical_pixels[y * @as(usize, @intCast(bounds.w)) + x] = self.reader.pixels[nativeIndex(value.view, x, y)];
        }
        self.oriented_bytes +|= (end - self.orient_offset) * 4;
        self.orient_offset = end; self.ready = end == count;
    }
    pub fn image(self: *Capture) ?scene.SceneBuffer {
        if (!self.ready or self.current == null) return null;
        const bounds = geometry.logical(self.current.?.view) catch return null;
        return .{ .width = bounds.w, .height = bounds.h,
            .pixels = if (self.logical_pixels.len != 0) self.logical_pixels else self.reader.pixels };
    }
    pub fn cursor(self: *const Capture) Cursor {
        const value = self.current orelse return .{};
        var result = value.cursor; result.x -= value.view.origin.x; result.y -= value.view.origin.y;
        return result;
    }
    pub fn acknowledge(self: *Capture, published: bool) void {
        self.reader.acknowledge(published) catch return;
        self.ready = false; self.current = null;
        if (!published) self.redraw = self.wanted;
    }
    pub fn needsPolling(self: *const Capture) bool { return self.reader.pending() or (self.reader.phase == .ready and !self.ready); }
    pub fn takeRedraw(self: *Capture, now: u64) bool {
        if (!self.wanted or !self.redraw or self.reader.pending() or self.current != null or self.ready or now < self.retry_ns) return false;
        self.redraw = false; return true;
    }
    pub fn close(self: *Capture) !void {
        try self.reader.close();
        self.reader.allocator.free(self.logical_pixels); self.logical_pixels = &.{};
        self.view = null; self.current = null; self.ready = false; self.records = @splat(null); self.cpu_profile = false;
    }
};

/// Capture is upright in primary logical desktop coordinates, including
/// fractional scale. Remote input therefore uses the same coordinate space.
pub fn nativeIndex(view: geometry.topology.Viewport, x: usize, y: usize) usize {
    const swapped = view.rotation == .clockwise90 or view.rotation == .clockwise270;
    const w = if (swapped) view.pixel_h else view.pixel_w;
    const h = if (swapped) view.pixel_w else view.pixel_h;
    const ox = @min(w - 1, ((2 * x + 1) * view.scale) / 240);
    const oy = @min(h - 1, ((2 * y + 1) * view.scale) / 240);
    const point: [2]usize = switch (view.rotation) {
        .normal => .{ ox, oy }, .clockwise90 => .{ oy, view.pixel_h - 1 - ox },
        .clockwise180 => .{ view.pixel_w - 1 - ox, view.pixel_h - 1 - oy },
        .clockwise270 => .{ view.pixel_w - 1 - oy, ox },
    };
    return point[1] * view.pixel_w + point[0];
}
