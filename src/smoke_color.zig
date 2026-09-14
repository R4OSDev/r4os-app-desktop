//! Two bounded probes in the existing desktop smoke: actual freestanding
//! R4IMG -> ICC/CMM -> SDR calls, then linear-light alpha composition.
const std = @import("std");
const img = @import("r4img");
const gfx = @import("r4gfx");
fn put(comptime T: type, bytes: []u8, at: usize, value: T) void {
    std.mem.writeInt(T, bytes[at..][0..@sizeOf(T)], value, .little);
}
pub fn check(allocator: std.mem.Allocator, images: *const img.Context, raster: *const img.RasterContext, colors: *const gfx.ColorV1Client) !void {
    // Original BMP V4: two RGB24 pixels128/255 and255/255, linear sRGB
    // primaries. R4IMG retains the calibration; only R4GFX constructs ICC.
    var bitmap: [130]u8 = @splat(0);
    @memcpy(bitmap[0..2], "BM");
    put(u32, &bitmap, 2, bitmap.len);
    put(u32, &bitmap, 10, 122);
    put(u32, &bitmap, 14, 108);
    put(u32, &bitmap, 18, 2);
    put(u32, &bitmap, 22, 1);
    put(u16, &bitmap, 26, 1);
    put(u16, &bitmap, 28, 24);
    put(u32, &bitmap, 34, 8);
    const matrix = [_]f64{ 0.4123908, 0.2126390, 0.0193308, 0.3575843, 0.7151687, 0.1191948, 0.1804808, 0.0721923, 0.9505322 };
    inline for (matrix, 0..) |value, i| put(i32, &bitmap, 74 + i * 4, @intFromFloat(@round(value * 1073741824.0)));
    inline for (0..3) |i| put(u32, &bitmap, 110 + i * 4, 65536);
    @memset(bitmap[122..125], 128);
    @memset(bitmap[125..128], 255);
    var pixels: [2]u32 = @splat(0);
    var target: gfx.R4GfxColorImage = .{
        .version = 1,
        .size = @sizeOf(gfx.R4GfxColorImage),
        .image = .{ .cpu_address = @intFromPtr(&pixels), .byte_length = 8, .pitch = 8, .width = 2, .height = 1, .format = gfx.format_xrgb8888, .reserved = 0 },
        .description = .{ .version = 1, .size = @sizeOf(gfx.R4GfxColorDescription), .primaries = gfx.color_primaries_srgb, .transfer = gfx.color_transfer_srgb, .range = gfx.color_range_full, .alpha = gfx.color_alpha_opaque, .precision = gfx.color_precision_unorm8, .flags = 0, .reference_white = 1000000, .peak = 1000000, .black = 0, .reserved = 0 },
        .profile = std.mem.zeroes(gfx.R4GfxColorProfile),
    };
    const rect: gfx.R4GfxRect = .{ .x = 0, .y = 0, .width = 2, .height = 1 };
    var request: gfx.R4GfxColorTransform = .{ .version = 1, .size = @sizeOf(gfx.R4GfxColorTransform), .source_rect = rect, .target_rect = rect, .sampler = gfx.render_sampler_nearest, .operation = gfx.render_operation_blit, .opacity = 65535, .flags = gfx.color_transform_output | gfx.color_transform_relative_white, .pixel_budget = 2 };
    const decoded = try img.ColorDecoder(gfx).decodeRaster(allocator, images, raster, colors, &bitmap, &target, &request, .{});
    if (decoded.assumed or pixels[1] != 0xffffff) return error.ProfilePixels;
    inline for (.{ 0, 8, 16 }) |shift| {
        const level = (pixels[0] >> shift) & 255;
        if (level < 187 or level > 189) return error.ProfilePixels;
    }
    const overlay = [_]u32{ 0x80ffffff, 0x00ffffff };
    var source = target;
    source.image.cpu_address = @intFromPtr(&overlay);
    source.image.format = gfx.format_argb8888;
    source.description.alpha = gfx.color_alpha_straight;
    pixels = .{ 0, 0x123456 };
    request.operation = gfx.render_operation_over;
    request.flags = 0;
    var stats: gfx.R4GfxCpuStats = undefined;
    if (colors.color_image_transform(&source, &target, &request, &stats) != gfx.status_ok or pixels[0] != 0xbcbcbc or pixels[1] != 0x123456) return error.LinearPixels;
}
