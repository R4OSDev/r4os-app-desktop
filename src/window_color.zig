//! Window color contracts consumed by the common compositor. Absolute HDR
//! values are independent of the current monitor; only output encoding maps
//! them to that monitor's confirmed range.
const gfx = @import("r4gfx");
const abi = @import("r4os").abi;
const sdr = @import("composition_software.zig").description;

pub fn scrgb(opaque_alpha: bool) gfx.R4GfxColorDescription {
    var value = sdr(true, opaque_alpha);
    // IEC scRGB: 1.0 = 80 cd/m2, 125.0 = 10000 cd/m2.
    value.reference_white = 800000;
    value.peak = 100000000;
    return value;
}
pub fn pq(opaque_alpha: bool) gfx.R4GfxColorDescription {
    var value = sdr(false, opaque_alpha);
    value.primaries = gfx.color_primaries_bt2020;
    value.transfer = gfx.color_transfer_pq;
    value.precision = gfx.color_precision_unorm10;
    value.reference_white = 2030000;
    value.peak = 100000000;
    return value;
}
pub fn absolute(value: gfx.R4GfxColorDescription) bool {
    return value.peak > value.reference_white;
}
pub fn working(output: gfx.R4GfxColorDescription) gfx.R4GfxColorDescription {
    var value = sdr(true, false);
    // Ordinary desktop layers retain their normalized SDR values, so 1.0
    // follows the output's SDR white. Absolute window colors are converted
    // into this scale without changing their luminance.
    value.reference_white = output.reference_white;
    value.peak = 100000000;
    return value;
}
pub fn publish(config: *abi.WindowGraphicsConfig) void {
    config.format_count = 8;
    config.formats[0] = .{ .format = gfx.format_xrgb8888, .color = @bitCast(sdr(false, true)) };
    config.formats[1] = .{ .format = gfx.format_argb8888, .color = @bitCast(sdr(false, false)) };
    config.formats[2] = .{ .format = gfx.format_abgr16161616f, .color = @bitCast(sdr(true, false)) };
    config.formats[3] = .{ .format = gfx.format_abgr16161616f, .color = @bitCast(sdr(true, true)) };
    config.formats[4] = .{ .format = gfx.format_abgr16161616f, .color = @bitCast(scrgb(false)) };
    config.formats[5] = .{ .format = gfx.format_abgr16161616f, .color = @bitCast(scrgb(true)) };
    config.formats[6] = .{ .format = gfx.format_argb2101010, .color = @bitCast(pq(false)) };
    config.formats[7] = .{ .format = gfx.format_xrgb2101010, .color = @bitCast(pq(true)) };
}
