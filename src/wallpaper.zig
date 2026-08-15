const std = @import("std");
const r4os = @import("r4os");
const r4img = @import("r4img");

pub const max_file_bytes: usize = 32 * 1024 * 1024;
pub const max_width: u32 = 4096;
pub const max_height: u32 = 2160;
pub const max_pixels: usize = @as(usize, max_width) * @as(usize, max_height);

pub const View = struct {
    width: u32 = 0,
    height: u32 = 0,
    pixels: []const u32 = &.{},

    pub fn origin(self: View, screen_w: i32, screen_h: i32) struct { x: i32, y: i32 } {
        const image_w: i64 = self.width;
        const image_h: i64 = self.height;
        const x64 = @divTrunc(@as(i64, screen_w) - image_w, 2);
        const y64 = @divTrunc(@as(i64, screen_h) - image_h, 2);
        return .{
            .x = @intCast(std.math.clamp(x64, std.math.minInt(i32), std.math.maxInt(i32))),
            .y = @intCast(std.math.clamp(y64, std.math.minInt(i32), std.math.maxInt(i32))),
        };
    }
};

pub const State = struct {
    memory: ?[]u32 = null,
    width: u32 = 0,
    height: u32 = 0,

    pub fn clear(self: *State, allocator: std.mem.Allocator) void {
        if (self.memory) |pixels| allocator.free(pixels);
        self.* = .{};
    }

    pub fn view(self: *const State) View {
        return .{
            .width = self.width,
            .height = self.height,
            .pixels = self.memory orelse &.{},
        };
    }

    pub fn load(self: *State, sys: *const r4os.r4sys.Context, images: *const r4img.Context, allocator: std.mem.Allocator, path: [*:0]const u8) bool {
        const info = sys.fileInfo(path) orelse return false;
        if (info.is_dir != 0 or info.size == 0 or info.size > max_file_bytes) return false;
        const file_len: usize = @intCast(info.size);
        const bytes = allocator.alloc(u8, file_len) catch return false;
        defer allocator.free(bytes);
        const read = sys.fileRead(path, bytes);
        if (read != @as(i32, @intCast(file_len))) return false;

        const image_info = images.probe(bytes, "image/bmp") catch return false;
        if (image_info.format != .bmp or !dimensionsAllowed(image_info.width, image_info.height)) return false;
        const count = image_info.pixelCount() catch return false;
        const pixels = allocator.alloc(u32, count) catch return false;
        const scratch_bytes = images.scratchBytesFor(image_info, bytes.len) catch {
            allocator.free(pixels);
            return false;
        };
        const scratch = allocator.alloc(u8, scratch_bytes) catch {
            allocator.free(pixels);
            return false;
        };
        defer allocator.free(scratch);
        const decoded = images.decode(bytes, "image/bmp", pixels, scratch) catch {
            allocator.free(pixels);
            return false;
        };
        if (decoded.info.format != .bmp or decoded.pixels.len != count) {
            allocator.free(pixels);
            return false;
        }

        if (self.memory) |old| allocator.free(old);
        self.memory = decoded.pixels;
        self.width = decoded.info.width;
        self.height = decoded.info.height;
        return true;
    }
};

pub fn dimensionsAllowed(width: u32, height: u32) bool {
    if (width == 0 or height == 0 or width > max_width or height > max_height) return false;
    const count = pixelCount(width, height) orelse return false;
    return count <= max_pixels;
}

fn pixelCount(width: u32, height: u32) ?usize {
    return std.math.mul(usize, @as(usize, width), @as(usize, height)) catch null;
}

test "wallpaper is centered with deterministic clipping origins" {
    const small = View{ .width = 320, .height = 200 };
    try std.testing.expectEqual(@as(i32, 480), small.origin(1280, 720).x);
    try std.testing.expectEqual(@as(i32, 260), small.origin(1280, 720).y);

    const large = View{ .width = 1920, .height = 1080 };
    try std.testing.expectEqual(@as(i32, -320), large.origin(1280, 720).x);
    try std.testing.expectEqual(@as(i32, -180), large.origin(1280, 720).y);
}

test "wallpaper bounds include Full HD and reject oversized images" {
    try std.testing.expect(dimensionsAllowed(1920, 1080));
    try std.testing.expect(dimensionsAllowed(4096, 2160));
    try std.testing.expect(!dimensionsAllowed(4097, 2160));
    try std.testing.expect(!dimensionsAllowed(1920, 0));
}
