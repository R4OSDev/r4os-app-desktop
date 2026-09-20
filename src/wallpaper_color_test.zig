//! End-to-end color cases in the existing wallpaper group, using both actual
//! runtime providers. Tiny PNGs and their ICC profiles are generated in memory.
const std = @import("std");
const t = std.testing;
const r4os = @import("r4os");
const img = @import("r4img");
const gfx = @import("r4gfx");
const image_provider = @import("image_provider");
const color_provider = @import("color_provider");
const fixture = @import("png_fixture");
pub fn check(comptime decode: anytype) !void {
    var imports = [_]r4os.abi.R4XStartImport{
        .{ .module_name = @intFromPtr("R4IMG"), .symbol_name = @intFromPtr("API_V1"), .min_version = 1, .resolved_version = 1, .table = @intFromPtr(&image_provider.r4img_api_v1) },
        .{ .module_name = @intFromPtr("R4IMG"), .symbol_name = @intFromPtr("PNG_V1"), .min_version = 1, .resolved_version = 1, .table = @intFromPtr(&image_provider.r4img_png_v1) },
        .{ .module_name = @intFromPtr("R4IMG"), .symbol_name = @intFromPtr("RASTER_V1"), .min_version = 1, .resolved_version = 1, .table = @intFromPtr(&image_provider.r4img_raster_v1) },
        .{ .module_name = @intFromPtr("R4GFX"), .symbol_name = @intFromPtr("COLOR_V1"), .min_version = gfx.color_v1_revision, .resolved_version = color_provider.c.color_v1_header.abi_minor, .table = @intFromPtr(&color_provider.color_api.table) },
    };
    var raw: r4os.abi.R4XStartContext = .{ .flags = r4os.abi.r4xstart_flag_imports_valid, .imports = @intFromPtr(&imports), .import_count = imports.len };
    const images = img.Context.init(&raw) orelse return error.Images;
    const png = img.PngContext.init(&raw) orelse return error.Png;
    const raster = img.RasterContext.init(&raw) orelse return error.Raster;
    const colors = try gfx.ColorV1Client.init(&raw);
    const scratch = try t.allocator.alignedAlloc(u8, .@"16", 2 * 1024 * 1024);
    defer t.allocator.free(scratch);
    // The actual image decoder accepts screenshots with correct row order,
    // channels and opaque XRGB; no second encoder/parser imitation is used.
    var bitmap: [54 + 4 * 4]u8 = undefined;
    const screenshot = @import("screenshot.zig");
    @memcpy(bitmap[0..54], &(try screenshot.bitmapHeader(2, 2)));
    const corners = [_]u32{ 0xff0000, 0x00ff00, 0x0000ff, 0xffffff };
    @memcpy(bitmap[54..], std.mem.sliceAsBytes(&corners));
    var decoded: [4]u32 = undefined;
    _ = try images.decode(&bitmap, "image/bmp", &decoded, scratch);
    try t.expectEqualSlices(u32, &.{ 0xffff0000, 0xff00ff00, 0xff0000ff, 0xffffffff }, &decoded);
    try t.expectError(error.ImageSize, screenshot.bitmapHeader(0xffffffff, 0xffffffff));
    var profile: [2048]u8 = undefined;
    var profile_bytes: u64 = 0;
    const definition: gfx.R4GfxColorProfileDefinition = .{ .version = 1, .size = @sizeOf(gfx.R4GfxColorProfileDefinition), .color_model = gfx.color_model_rgb, .curve = gfx.color_curve_power, .white_x = 31270, .white_y = 32900, .red_x = 64000, .red_y = 33000, .green_x = 30000, .green_y = 60000, .blue_x = 15000, .blue_y = 6000, .gamma_red = 100000, .gamma_green = 100000, .gamma_blue = 100000, .reserved = 0 };
    try t.expectEqual(gfx.status_ok, colors.color_profile_generate(&definition, @intFromPtr(scratch.ptr), scratch.len, @intFromPtr(&profile), profile.len, &profile_bytes));
    // The real final-output owner uses the same ICC engine as image decode.
    // A linear-gamma monitor needs55/255 for sRGB128/255, independently of
    // composition.65pixels crosses the64-pixel conversion batch boundary.
    const Profile = @import("r4gfx_desktop_outputs").profiles.Owner(gfx);
    var display_profile = try Profile.open(t.allocator, colors, profile[0..profile_bytes], gfx.color_intent_relative, 0);
    defer std.debug.assert(display_profile.close());
    var canonical: [65]u32 = @splat(0x808080);
    canonical[0] = 0;
    canonical[64] = 0xffffff;
    var output = canonical;
    try display_profile.applySdr(&output, 13, 5);
    try t.expectEqual(@as(u32, 0), output[0]);
    try t.expectEqual(@as(u32, 0xffffff), output[64]);
    for (output[1..64]) |pixel| try t.expectEqual(@as(u32, 0x373737), pixel);
    try t.expectEqual(@as(u32, 0x808080), canonical[32]);
    const prior = output;
    output = canonical;
    try display_profile.applySdr(&output, 13, 5);
    try t.expectEqualSlices(u32, &prior, &output);
    try t.expectError(error.Invalid, display_profile.applySdr(&output, 12, 5));
    const profile_fixtures = @import("profile_fixtures");
    var calibrated = try Profile.open(t.allocator, colors, profile_fixtures.calibrated, gfx.color_intent_relative, gfx.color_profile_calibration);
    defer std.debug.assert(calibrated.close());
    var white = [_]u32{0xffffff};
    try calibrated.applySdr(&white, 1, 1);
    try t.expectEqual(@as(u32, 0xbf80ff), white[0]);
    try t.expectError(error.Profile, Profile.open(t.allocator, colors, profile_fixtures.descending, gfx.color_intent_relative, gfx.color_profile_calibration));
    for (0..5) |kind| {
        var source = fixture.Builder.init();
        source.header();
        switch (kind) {
            1 => source.chunk("gAMA", &.{ 0, 1, 0x86, 0xa0 }), // Encoding gamma1.
            2 => {
                var compressed: [2200]u8 = undefined;
                const bytes = fixture.stored(profile[0..profile_bytes], &compressed);
                var iccp: [2300]u8 = undefined;
                @memcpy(iccp[0..10], "Original\x00\x00");
                @memcpy(iccp[10..][0..bytes.len], bytes);
                source.chunk("iCCP", iccp[0 .. bytes.len + 10]);
            },
            3 => source.chunk("cICP", &.{ 9, 16, 0, 1 }), // PQ.
            4 => source.chunk("cICP", &.{ 2, 255, 0, 1 }), // Unknown, no fallback tag.
            else => {},
        }
        const value: u16 = if (kind == 3) 49271 else 32768;
        source.rgba16(.{ value, value, value, 65535, 65535, 65535, 65535, 0 });
        var pixels = [_]u32{ 0, 0xffffff };
        const info = try images.probe(source.view(), "image/png");
        if (kind == 4) {
            try t.expectError(error.UnsupportedColor, decode(t.allocator, &images, &png, &raster, &colors, source.view(), &pixels, info));
            try t.expectEqualSlices(u32, &.{ 0, 0xffffff }, &pixels);
            continue;
        }
        try decode(t.allocator, &images, &png, &raster, &colors, source.view(), &pixels, info);
        const level = pixels[0] & 255;
        if (kind == 0) try t.expect(level >= 127 and level <= 128) else if (kind == 3) try t.expect(level > 240) else try t.expect(level >= 187 and level <= 188);
        try t.expectEqual(level, (pixels[0] >> 8) & 255);
        try t.expectEqual(level, (pixels[0] >> 16) & 255);
        // Transparent HDR leaves an SDR white background exactly white.
        try t.expectEqual(@as(u32, 0xffffff), pixels[1]);
    }
    const raster_fixture = @import("raster_fixture");
    var encoded: [32768]u8 = undefined;
    for ([_]u32{ 0x73524742, 0, 0x4d424544, 0x4c494e4b, 0x12345678 }) |space| {
        const bytes = raster_fixture.bmp(&encoded, space, if (space == 0x4d424544) profile[0..profile_bytes] else if (space == 0x4c494e4b) "C:\\external.icc\x00" else &.{});
        const info = try images.probe(bytes, "image/bmp");
        var pixels = [_]u32{ 0x010203, 0x040506 };
        if (space == 0x4c494e4b or space == 0x12345678) {
            try t.expectError(error.UnsupportedColor, decode(t.allocator, &images, &png, &raster, &colors, bytes, &pixels, info));
            try t.expectEqualSlices(u32, &.{ 0x010203, 0x040506 }, &pixels);
            continue;
        }
        try decode(t.allocator, &images, &png, &raster, &colors, bytes, &pixels, info);
        const level = pixels[0] & 255;
        if (space == 0x73524742) try t.expectEqual(@as(u32, 128), level) else try t.expect(level >= 187 and level <= 189);
        try t.expectEqual(@as(u32, 0xffffff), pixels[1]);
    }
    // A complete JPEG passes through both real providers and the same ICC
    // transform; compare its channel samples to the sRGB encoding equation.
    const jpeg = raster_fixture.jpegWithSource(&encoded, profile[0..profile_bytes], raster_fixture.progressive);
    const info = try images.probe(jpeg, "image/jpeg");
    const plain = try t.allocator.alloc(u32, try info.pixelCount());
    defer t.allocator.free(plain);
    const decoder_scratch = try t.allocator.alloc(u8, try images.scratchBytesFor(info, jpeg.len));
    defer t.allocator.free(decoder_scratch);
    _ = try images.decode(jpeg, "image/jpeg", plain, decoder_scratch);
    const converted = try t.allocator.alloc(u32, plain.len);
    defer t.allocator.free(converted);
    @memset(converted, 0);
    try decode(t.allocator, &images, &png, &raster, &colors, jpeg, converted, info);
    var checked: usize = 0;
    for (plain, converted) |raw_pixel, color_pixel| {
        inline for (.{ 0, 8, 16 }) |shift| {
            const level = (raw_pixel >> shift) & 255;
            if (level >= 16 and level <= 239) {
                const linear: f64 = @as(f64, @floatFromInt(level)) / 255.0;
                const expected: i32 = @intFromFloat(@round((1.055 * std.math.pow(f64, linear, 1.0 / 2.4) - 0.055) * 255.0));
                try t.expect(@abs(@as(i32, @intCast((color_pixel >> shift) & 255)) - expected) <= 1);
                checked += 1;
            }
        }
    }
    try t.expect(checked > 0);
    var meta = try raster.color(jpeg);
    meta.color_model = img.abi.raster_model_cmyk;
    try t.expectError(error.UnsupportedColor, img.ColorDecoder(gfx).characterizeRaster(meta, .{ .untagged_srgb = true }));
}
