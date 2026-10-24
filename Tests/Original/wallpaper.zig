const std = @import("std");
const r4os = @import("r4os");
const r4img = @import("r4img");
const gfx = @import("r4gfx");

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

    pub fn load(self: *State, sys: *const r4os.r4sys.Context, images: *const r4img.Context, png: *const r4img.PngContext, raster: *const r4img.RasterContext, colors: *const gfx.ColorV1Client, allocator: std.mem.Allocator, path: [*:0]const u8, background: u32) bool {
        const info = sys.fileInfo(path) orelse return false;
        if (info.is_dir != 0 or info.size == 0 or info.size > max_file_bytes) return false;
        const file_len: usize = @intCast(info.size);
        const bytes = allocator.alloc(u8, file_len) catch return false;
        defer allocator.free(bytes);
        const read = sys.fileRead(path, bytes);
        if (read != @as(i32, @intCast(file_len))) return false;

        const image_info = images.probe(bytes, "") catch return false;
        if ((image_info.format != .bmp and image_info.format != .png) or !dimensionsAllowed(image_info.width, image_info.height)) return false;
        const count = image_info.pixelCount() catch return false;
        const pixels = allocator.alloc(u32, count) catch return false;
        @memset(pixels, background & 0x00ffffff);
        decodeColor(allocator, images, png, raster, colors, bytes, pixels, image_info) catch {
            allocator.free(pixels);
            return false;
        };
        if (self.memory) |old| allocator.free(old);
        self.memory = pixels;
        self.width = image_info.width;
        self.height = image_info.height;
        return true;
    }
};

fn decodeColor(allocator: std.mem.Allocator, images: *const r4img.Context, png: *const r4img.PngContext, raster: *const r4img.RasterContext, colors: *const gfx.ColorV1Client, bytes: []const u8, pixels: []u32, info: r4img.Info) !void {
    // Wallpaper is an SDR desktop asset. HDR source content is converted to
    // the same defined sRGB paper white as UI colors before it is cached.
    // Tone-map the source asset before blending with the existing SDR
    // background. Otherwise a transparent HDR pixel would tone-map an
    // unrelated background a second time. Both passes use the shared CMM.
    const converted = try allocator.alloc(u32, pixels.len);
    defer allocator.free(converted);
    const source: gfx.R4GfxColorImage = .{
        .version = 1,
        .size = @sizeOf(gfx.R4GfxColorImage),
        .image = .{ .cpu_address = @intFromPtr(converted.ptr), .byte_length = converted.len * 4, .pitch = @as(u64, info.width) * 4, .width = info.width, .height = info.height, .format = gfx.format_argb8888, .reserved = 0 },
        .description = .{ .version = 1, .size = @sizeOf(gfx.R4GfxColorDescription), .primaries = gfx.color_primaries_srgb, .transfer = gfx.color_transfer_srgb, .range = gfx.color_range_full, .alpha = gfx.color_alpha_straight, .precision = gfx.color_precision_unorm8, .flags = 0, .reference_white = 1000000, .peak = 1000000, .black = 0, .reserved = 0 },
        .profile = std.mem.zeroes(gfx.R4GfxColorProfile),
    };
    const rect: gfx.R4GfxRect = .{ .x = 0, .y = 0, .width = info.width, .height = info.height };
    var request: gfx.R4GfxColorTransform = .{ .version = 1, .size = @sizeOf(gfx.R4GfxColorTransform), .source_rect = rect, .target_rect = rect, .sampler = gfx.render_sampler_nearest, .operation = gfx.render_operation_blit, .opacity = 65535, .flags = gfx.color_transform_output | gfx.color_transform_relative_white | gfx.color_transform_dither, .pixel_budget = @as(u64, info.width) * info.height };
    const Decoder = r4img.ColorDecoder(gfx);
    const policy: Decoder.Policy = .{ .untagged_srgb = true, .missing_chromaticities_srgb = true, .missing_gamma_srgb = true };
    if (info.format == .png) _ = try Decoder.decode(allocator, images, png, colors, bytes, &source, &request, policy) else _ = try Decoder.decodeRaster(allocator, images, raster, colors, bytes, &source, &request, policy);
    var target = source;
    target.image.cpu_address = @intFromPtr(pixels.ptr);
    target.image.format = gfx.format_xrgb8888;
    target.description.alpha = gfx.color_alpha_opaque;
    request.operation = gfx.render_operation_over;
    request.flags = gfx.color_transform_dither;
    var stats: gfx.R4GfxCpuStats = undefined;
    if (colors.color_image_transform(&source, &target, &request, &stats) != gfx.status_ok) return error.ColorTransform;
}

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
    try @import("wallpaper_color_test.zig").check(decodeColor);
    try std.testing.expect(dimensionsAllowed(1920, 1080));
    try std.testing.expect(dimensionsAllowed(4096, 2160));
    try std.testing.expect(!dimensionsAllowed(4097, 2160));
    try std.testing.expect(!dimensionsAllowed(1920, 0));
}
