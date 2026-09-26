//! Desktop-owned CPU layer contents and ordered composition records. Existing
//! painters produce small premultiplied surfaces; GPU storage belongs to R4GFX.
const std = @import("std");
const scene = @import("scene_buffer.zig");
const surface = @import("surface.zig");
const primitives = @import("primitive_frame.zig");
const geometry = @import("output_geometry.zig");
const window_image = @import("window_image.zig");
pub const capacity = 64;
pub const command_capacity = capacity * surface.max_damage_regions;
pub const Entry = struct {
    key: u32 = 0,
    pixels: []u32 = &.{},
    bounds: surface.Rect = .{ .x = 0, .y = 0, .w = 0, .h = 0 },
    capture_bounds: surface.Rect = .{ .x = 0, .y = 0, .w = 0, .h = 0 },
    generation: u64 = 0,
    frame: u64 = 0,
    dirty: ?surface.Rect = null,
    initialized: bool = false,
    external: ?*window_image.Frame = null,
    borrowed: bool = false,
};
pub const Command = struct { entry: u8, scissor: surface.Rect };
pub const ColorScratch = struct {
    pub const side = @import("composition_tiles.zig").side;
    tiles: @import("composition_tiles.zig").Index = .{},
    linear: [side * side * 4]u16 = undefined,
    encoded: [side * side]u32 = undefined,
    touched: [side]u64 = @splat(0),
};
pub const Cache = struct {
    allocator: std.mem.Allocator,
    budget: usize,
    reserved: usize = 0,
    entries: [capacity]Entry = @splat(.{}),
    commands: [command_capacity]Command = undefined,
    command_count: usize = 0,
    scratch: []u32 = &.{},
    color_scratch: ?*ColorScratch = null,
    painter: scene.SceneBuffer = .{},
    active: ?struct { index: usize, scissor: surface.Rect } = null,
    screen: surface.Rect = .{ .x = 0, .y = 0, .w = 0, .h = 0 },
    frame: u64 = 0,
    collecting: bool = false,
    failure: ?anyerror = null,
    changed_bytes: u64 = 0,
    unchanged_layers: u64 = 0,
    recording: ?*primitives.Frame = null,
    // Borrowed only while collecting, from Engine's last retired snapshot.
    // Retained texture pages are pinned before any painter can evict them.
    replay_commands: []const primitives.Command = &.{},
    replay_layers: [capacity]bool = @splat(false),
    reused_captures: u64 = 0,
    view: ?geometry.topology.Viewport = null,
    logical_screen: surface.Rect = .{ .x = 0, .y = 0, .w = 0, .h = 0 },

    pub fn init(allocator: std.mem.Allocator, budget: usize) Cache { return .{ .allocator = allocator, .budget = budget }; }
    pub fn deinit(self: *Cache) void {
        self.releaseUnsubmitted();
        for (&self.entries) |*entry| { self.allocator.free(entry.pixels); entry.* = .{}; }
        self.allocator.free(self.scratch);
        if (self.color_scratch) |storage| self.allocator.destroy(storage);
        self.color_scratch = null;
        self.scratch = &.{}; self.reserved = 0; self.painter = .{}; self.active = null; self.collecting = false;
    }
    pub fn start(self: *Cache, bounds: surface.Rect) !void {
        if (bounds.x != 0 or bounds.y != 0) return error.Invalid;
        self.view = null;
        if (self.recording) |recording| recording.view = null;
        try self.startInternal(bounds, bounds);
    }
    /// CPU canvases may cover a monitor at a nonzero logical origin. Layer
    /// bounds retain those coordinates; only pixel offsets are normalized.
    pub fn startCpuOutput(self: *Cache, bounds: surface.Rect) !void {
        if (self.recording != null) return error.State;
        self.view = null;
        try self.startInternal(bounds, bounds);
    }
    pub fn colorStorage(self: *Cache) !*ColorScratch {
        if (self.color_scratch) |storage| return storage;
        const bytes = @sizeOf(ColorScratch);
        if (bytes > self.budget or self.reserved > self.budget - bytes) return error.OutOfMemory;
        const storage = try self.allocator.create(ColorScratch);
        self.color_scratch = storage;
        self.reserved += bytes;
        return storage;
    }
    pub fn startOutput(self: *Cache, view: geometry.topology.Viewport) !void {
        const recording = self.recording orelse return error.State;
        if (recording.mirror) return error.State;
        // Changing scale/origin/rotation invalidates layer and font resource
        // generations even when their rounded native dimensions coincide.
        if (self.view == null or !std.meta.eql(self.view.?, view)) for (&self.entries) |*entry| { entry.initialized = false; };
        const bounds = try geometry.logical(view);
        self.view = view; recording.view = view;
        try self.startInternal(geometry.native(view), bounds);
    }
    fn startInternal(self: *Cache, bounds: surface.Rect, logical_bounds: surface.Rect) !void {
        if (self.collecting or self.active != null) return error.Busy;
        for (&self.entries) |*entry| if (entry.borrowed) return error.Busy;
        if (bounds.isEmpty()) return error.Invalid;
        self.frame = try std.math.add(u64,self.frame,1);
        if (self.recording) |recording| try recording.start(self.frame);
        self.screen = bounds; self.logical_screen = logical_bounds;
        self.command_count = 0; self.failure = null; self.collecting = true;
        self.replay_commands = &.{}; self.replay_layers = @splat(false);
    }
    pub fn finish(self: *Cache) ![]const Command {
        if (!self.collecting or self.active != null) return error.State;
        self.collecting = false;
        if (self.failure) |failure| return failure;
        if (self.recording) |recording| if (recording.failure) |failure| return failure;
        return self.commands[0..self.command_count];
    }
    pub fn hook(self: *Cache) scene.SceneBuffer.LayerHook {
        return .{ .context = @intFromPtr(self), .begin = beginHook, .end = endHook,
            .external = if (self.recording) |recording| if (!recording.mirror) externalHook else null else externalHook,
            .external_cpu = self.recording == null };
    }
    fn externalHook(raw: usize, key: u32, bounds: surface.Rect, damage: surface.Rect, frame: *anyopaque) void {
        const self: *Cache = @ptrFromInt(raw);
        self.external(key, bounds, damage, @ptrCast(@alignCast(frame))) catch |err| { self.failure = err; };
    }
    /// A window BO is its own ordered layer. CPU composition reads the
    /// producer's read-only lease directly, without a second painter copy.
    pub fn external(self: *Cache, key: u32, bounds: surface.Rect, damage: surface.Rect, frame: *window_image.Frame) !void {
        if (!self.collecting or self.active != null) return error.State;
        if (self.recording) |recording| {
            if (recording.mirror or frame.isCpu()) return error.Unsupported;
        } else if (frame.cpuImage() == null or frame.message.descriptor.width != bounds.w or
            frame.message.descriptor.height != bounds.h) return error.Unsupported;
        if (self.failure != null) return error.State;
        if (key == 0 or self.command_count == self.commands.len) return error.Capacity;
        const logical_clip = intersect(intersect(self.logical_screen, bounds) orelse return, damage) orelse return;
        const raster = if (self.view) |view| try geometry.rasterRect(view, bounds) else bounds;
        const clip = if (self.view) |view| intersect(try geometry.rasterRect(view, logical_clip), self.screen) orelse return else logical_clip;
        const index = for (&self.entries, 0..) |*entry, i| { if (entry.key == key) break i; } else
            for (&self.entries, 0..) |*entry, i| { if (entry.key == 0) break i; } else return error.Capacity;
        const entry = &self.entries[index];
        if (entry.frame == self.frame and entry.borrowed and (entry.external != frame or !std.meta.eql(entry.capture_bounds, bounds))) return error.State;
        if (!entry.borrowed) {
            try frame.borrow();
            self.reserved -= entry.pixels.len * 4;
            self.allocator.free(entry.pixels);
            entry.* = .{ .key = key, .external = frame, .borrowed = true, .bounds = raster, .capture_bounds = bounds,
                .frame = self.frame, .generation = self.frame, .initialized = true };
        }
        self.commands[self.command_count] = .{ .entry = @intCast(index), .scissor = clip };
        self.command_count += 1;
    }
    /// Used only before GPU admission, or after Engine has drained every job.
    pub fn releaseUnsubmitted(self: *Cache) void {
        for (&self.entries) |*entry| if (entry.borrowed) {
            std.debug.assert(entry.external.?.finish(null, false));
            entry.borrowed = false;
        };
    }
    pub fn hasBorrowed(self: *const Cache) bool {
        for (&self.entries) |*entry| if (entry.borrowed) return true;
        return false;
    }
    fn beginHook(raw: usize, key: u32, bounds: surface.Rect, damage: surface.Rect) ?*scene.SceneBuffer {
        const self: *Cache = @ptrFromInt(raw);
        return self.begin(key,bounds,damage) catch |err| { self.failure = err; return null; };
    }
    fn endHook(raw: usize, key: u32) void {
        const self: *Cache = @ptrFromInt(raw);
        self.end(key) catch |err| { self.failure = err; self.active = null; };
    }
    fn resize(self: *Cache, pixels: *[]u32, count: usize) !void {
        if (pixels.len >= count) return;
        const bytes = try std.math.mul(usize,count,4);
        // The old allocation is still live during replacement. Include both
        // in peak admission instead of reporting only the final reservation.
        if (bytes > self.budget or self.reserved > self.budget-bytes) return error.OutOfMemory;
        const replacement = try self.allocator.alloc(u32,count);
        self.reserved += bytes;
        self.reserved -= pixels.len*4;
        self.allocator.free(pixels.*); pixels.* = replacement;
    }
    pub fn begin(self: *Cache, key: u32, bounds: surface.Rect, damage: surface.Rect) !?*scene.SceneBuffer {
        if (!self.collecting or self.active != null) return error.State;
        if (self.failure != null) return null;
        if (key == 0 or self.command_count == self.commands.len) return error.Capacity;
        const clipped = intersect(self.logical_screen,bounds) orelse return null;
        const logical_scissor = intersect(clipped,damage) orelse return null;
        const raster = if (self.view) |view| intersect(try geometry.rasterRect(view, clipped), self.screen) orelse return null else clipped;
        const scissor = if (self.view) |view| intersect(try geometry.rasterRect(view, logical_scissor), raster) orelse return null else logical_scissor;
        const index = for (&self.entries,0..) |*entry,i| { if (entry.key == key) break i; } else
            for (&self.entries,0..) |*entry,i| { if (entry.key == 0) break i; } else return error.Capacity;
        const entry = &self.entries[index];
        if (entry.external != null) return error.State;
        const mirror = if (self.recording) |recording| recording.mirror else true;
        if (!mirror and self.replay_layers[index] and entry.initialized and
            entry.frame == self.frame - 1 and std.meta.eql(entry.bounds, raster) and
            std.meta.eql(entry.capture_bounds, clipped) and std.meta.eql(logical_scissor, clipped)) {
            const recording = self.recording.?;
            var count: usize = 0;
            for (self.replay_commands) |command| { if (command.layer == index) count += 1; }
            if (count != 0) {
                if (count > recording.commands.len - recording.count) return error.Capacity;
                for (self.replay_commands) |command| if (command.layer == index) {
                    recording.commands[recording.count] = command; recording.count += 1;
                };
                entry.frame = self.frame;
                self.commands[self.command_count] = .{ .entry = @intCast(index), .scissor = scissor };
                self.command_count += 1; self.reused_captures +|= 1;
                return null;
            }
        }
        if (!std.meta.eql(entry.bounds,raster) or !std.meta.eql(entry.capture_bounds,clipped)) {
            if (entry.frame == self.frame) return error.State;
            const bytes = scene.SceneBuffer.requiredBytes(clipped.w,clipped.h) orelse return error.Bounds;
            if (mirror) try self.resize(&entry.pixels,bytes/4);
            entry.key = key; entry.bounds = raster; entry.capture_bounds = clipped; entry.initialized = false; entry.dirty = null;
        }
        const repaint = if (entry.initialized) logical_scissor else clipped;
        const bytes = scene.SceneBuffer.requiredBytes(repaint.w,repaint.h) orelse return error.Bounds;
        if (mirror) {
            try self.resize(&entry.pixels,scene.SceneBuffer.requiredBytes(clipped.w,clipped.h).?/4);
            try self.resize(&self.scratch,bytes/4);
            if (!self.painter.attachLayer(std.mem.sliceAsBytes(self.scratch),repaint)) return error.Bounds;
        } else self.painter = .{ .width = repaint.w, .height = repaint.h, .origin_x = repaint.x, .origin_y = repaint.y, .premultiplied = true };
        if (self.recording) |recording| {
            try recording.begin(@intCast(index),clipped);
            self.painter.primitive_hook = recording.hook();
        }
        self.painter.clearLayer(repaint);
        entry.frame = self.frame;
        self.active = .{ .index = index, .scissor = scissor };
        return &self.painter;
    }
    pub fn end(self: *Cache, key: u32) !void {
        const active = self.active orelse return error.State;
        errdefer self.active = null;
        const entry = &self.entries[active.index];
        if (entry.key != key) return error.State;
        self.painter.flushPending();
        if (self.recording) |recording| {
            try recording.end();
            if (!recording.mirror) {
                entry.generation = try std.math.add(u64,entry.generation,1);
                entry.initialized = true; entry.dirty = null;
                self.commands[self.command_count] = .{ .entry = @intCast(active.index), .scissor = active.scissor };
                self.command_count += 1; self.active = null;
                return;
            }
        }
        const area = self.painter.fullRect();
        const generation = try std.math.add(u64,entry.generation,1);
        const source = self.painter.pixels.?;
        var changed: ?surface.Rect = null;
        for (0..@intCast(area.h)) |row| {
            const offset = (@as(usize,@intCast(area.y-entry.bounds.y))+row)*@as(usize,@intCast(entry.bounds.w)) + @as(usize,@intCast(area.x-entry.bounds.x));
            const width: usize = @intCast(area.w);
            const pixels = source[row*width..][0..width];
            const destination = entry.pixels[offset..][0..width];
            if (entry.initialized and std.mem.eql(u32,pixels,destination)) continue;
            @memcpy(destination,pixels);
            self.changed_bytes +|= width*4;
            const line: surface.Rect = .{ .x = area.x, .y = area.y+@as(i32,@intCast(row)), .w = area.w, .h = 1 };
            changed = if (changed) |prior| prior.merged(line) else line;
        }
        if (changed) |rect| {
            entry.generation = generation;
            entry.dirty = if (entry.dirty) |prior| prior.merged(rect) else rect;
        } else self.unchanged_layers +|= 1;
        entry.initialized = true;
        self.commands[self.command_count] = .{ .entry = @intCast(active.index), .scissor = active.scissor };
        self.command_count += 1; self.active = null;
    }
    pub fn uploaded(self: *Cache, index: usize, generation: u64) void {
        if (index < self.entries.len and self.entries[index].generation == generation) self.entries[index].dirty = null;
    }
};
pub fn intersect(a: surface.Rect, b: surface.Rect) ?surface.Rect {
    const x = @max(a.x,b.x); const y = @max(a.y,b.y);
    const right = @min(a.right(),b.right()); const bottom = @min(a.bottom(),b.bottom());
    if (a.isEmpty() or b.isEmpty() or right <= x or bottom <= y) return null;
    return .{ .x = x, .y = y, .w = right-x, .h = bottom-y };
}
