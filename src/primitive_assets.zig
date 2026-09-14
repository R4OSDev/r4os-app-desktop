//! Bounded premultiplied texture pages. Source-sized conversion happens on a
//! content miss, independently of target scale. Current-frame pages are pinned
//! until the composition worker physically drains its jobs.
const std = @import("std");
const image = @import("primitive_image.zig");
const surface = @import("surface.zig");
pub const texture_capacity = 32;
pub const entry_capacity = 2048;
pub const atlas_count = 8;
pub const atlas_dimension = 512;
pub const Entry = struct { live: bool = false, key: [32]u8 = undefined, texture: u8 = 0, rect: surface.Rect = undefined };
pub const Texture = struct {
    pixels: []u32 = &.{}, width: u32 = 0, height: u32 = 0,
    generation: u64 = 0, pinned: u64 = 0, touched: u64 = 0,
    x: u32 = 0, y: u32 = 0, row_height: u32 = 0,
    dirty: ?surface.Rect = null,
    pub fn rect(self: *const Texture) surface.Rect { return .{ .x = 0, .y = 0, .w = @intCast(self.width), .h = @intCast(self.height) }; }
};
pub const Cache = struct {
    allocator: std.mem.Allocator,
    entries: []Entry,
    textures: [texture_capacity]Texture = @splat(.{}),
    budget: usize,
    reserved: usize,
    frame: u64 = 0,
    generation: u64 = 0,
    hits: u64 = 0, misses: u64 = 0, converted_pixels: u64 = 0, evictions: u64 = 0,

    pub fn init(allocator: std.mem.Allocator, budget: usize) !Cache {
        const bytes = entry_capacity * @sizeOf(Entry);
        if (bytes > budget) return error.OutOfMemory;
        const entries = try allocator.alloc(Entry, entry_capacity);
        for (entries) |*entry| entry.* = .{};
        return .{ .allocator = allocator, .entries = entries, .budget = budget, .reserved = bytes };
    }
    pub fn deinit(self: *Cache) void {
        for (&self.textures) |*texture| { self.allocator.free(texture.pixels); texture.* = .{}; }
        self.allocator.free(self.entries); self.entries = &.{}; self.reserved = 0;
    }
    pub fn start(self: *Cache, frame: u64) !void {
        if (frame == 0 or frame <= self.frame) return error.State;
        self.frame = frame;
    }
    fn forget(self: *Cache, index: usize) void {
        for (self.entries) |*entry| if (entry.live and entry.texture == index) { entry.live = false; };
        const texture = &self.textures[index];
        self.reserved -= texture.pixels.len * 4;
        self.allocator.free(texture.pixels); texture.* = .{}; self.evictions +|= 1;
    }
    fn oldest(self: *Cache, first: usize, end: usize, live_only: bool) ?usize {
        var chosen: ?usize = null;
        for (self.textures[first..end], first..) |texture, index| {
            if (texture.pinned == self.frame or (live_only and texture.pixels.len == 0)) continue;
            if (chosen == null or texture.touched < self.textures[chosen.?].touched) chosen = index;
        }
        return chosen;
    }
    fn allocate(self: *Cache, index: usize, width: u32, height: u32) !void {
        if (self.textures[index].pinned == self.frame) return error.State;
        const bytes = @as(usize, width) * height * 4;
        if (bytes > self.budget - self.entries.len * @sizeOf(Entry)) return error.OutOfMemory;
        if (self.textures[index].pixels.len != 0) self.forget(index);
        while (self.reserved > self.budget - bytes) self.forget(self.oldest(0, texture_capacity, true) orelse return error.OutOfMemory);
        const pixels = try self.allocator.alloc(u32, bytes / 4);
        @memset(pixels, 0); self.reserved += bytes;
        self.textures[index] = .{ .pixels = pixels, .width = width, .height = height };
    }
    fn placement(texture: *const Texture, width: u32, height: u32) ?surface.Rect {
        var x = texture.x; var y = texture.y;
        if (x + width > texture.width) { x = 0; y += texture.row_height; }
        if (width > texture.width or y + height > texture.height) return null;
        return .{ .x = @intCast(x), .y = @intCast(y), .w = @intCast(width), .h = @intCast(height) };
    }
    pub fn intern(self: *Cache, view: image.View) !Entry {
        if (self.frame == 0 or !view.valid()) return error.Invalid;
        const width = view.rasterWidth(); const height = view.rasterHeight();
        const key = view.key();
        for (self.entries) |entry| if (entry.live and std.mem.eql(u8, &entry.key, &key)) {
            const texture = &self.textures[entry.texture];
            texture.pinned = self.frame; texture.touched = self.frame; self.hits +|= 1;
            return entry;
        };
        if (for (self.entries) |entry| { if (!entry.live) break false; } else true)
            self.forget(self.oldest(0, texture_capacity, true) orelse return error.Capacity);
        const small = width <= 128 and height <= 128;
        var chosen: ?usize = null;
        if (small) for (self.textures[0..atlas_count], 0..) |*texture, index| {
            if (placement(texture, width, height) != null) { chosen = index; break; }
        };
        if (chosen == null) {
            const index = self.oldest(if (small) 0 else atlas_count, if (small) atlas_count else texture_capacity, false) orelse return error.Capacity;
            try self.allocate(index, if (small) atlas_dimension else width, if (small) atlas_dimension else height);
            chosen = index;
        }
        const index = chosen.?;
        self.textures[index].pinned = self.frame;
        const entry_index = for (self.entries, 0..) |entry, i| { if (!entry.live) break i; } else return error.Capacity;
        const texture = &self.textures[index];
        const rect = placement(texture, width, height) orelse return error.Capacity;
        const generation = try std.math.add(u64, self.generation, 1);
        for (0..height) |y| for (0..width) |x| {
            texture.pixels[(@as(usize, @intCast(rect.y)) + y) * texture.width + @as(usize, @intCast(rect.x)) + x] = view.rasterPixel(x, y);
        };
        if (texture.y != rect.y) texture.row_height = 0;
        texture.x = @as(u32, @intCast(rect.x)) + width; texture.y = @intCast(rect.y);
        texture.row_height = @max(texture.row_height, height);
        texture.generation = generation; self.generation = generation;
        texture.pinned = self.frame; texture.touched = self.frame;
        texture.dirty = if (texture.dirty) |prior| prior.merged(rect) else rect;
        const entry: Entry = .{ .live = true, .key = key, .texture = @intCast(index), .rect = rect };
        self.entries[entry_index] = entry;
        self.misses +|= 1; self.converted_pixels +|= @as(u64, view.width) * view.height;
        return entry;
    }
    pub fn uploaded(self: *Cache, index: usize, generation: u64) void {
        if (index < texture_capacity and self.textures[index].generation == generation) self.textures[index].dirty = null;
    }
};
