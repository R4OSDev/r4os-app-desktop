//! Existing absolute window colors and output working-space policy. Color
//! conversion stays with R4GFX; these functions only publish metadata.
use super::software::description;
use r4gfx as g;
use r4os_contract as a;
pub fn scrgb(opaque: bool) -> g::R4GfxColorDescription {
    let mut value = description(true, opaque);
    value.reference_white = 800000;
    value.peak = 100000000;
    value
}
pub fn pq(opaque: bool) -> g::R4GfxColorDescription {
    let mut value = description(false, opaque);
    value.primaries = g::color_primaries_bt2020;
    value.transfer = g::color_transfer_pq;
    value.precision = g::color_precision_unorm10;
    value.reference_white = 2030000;
    value.peak = 100000000;
    value
}
pub fn absolute(value: g::R4GfxColorDescription) -> bool {
    value.peak > value.reference_white
}
pub fn same_description(x: g::R4GfxColorDescription, y: g::R4GfxColorDescription) -> bool {
    let fields = |v: g::R4GfxColorDescription| {
        [
            v.version,
            v.size,
            v.primaries,
            v.transfer,
            v.range,
            v.alpha,
            v.precision,
            v.flags,
            v.reference_white,
            v.peak,
            v.black,
            v.reserved,
        ]
    };
    fields(x) == fields(y)
}
pub fn working(output: g::R4GfxColorDescription) -> g::R4GfxColorDescription {
    let mut value = description(true, false);
    value.reference_white = output.reference_white;
    value.peak = 100000000;
    value
}
fn format(format: u32, color: g::R4GfxColorDescription) -> a::WindowGraphicsFormat {
    a::WindowGraphicsFormat {
        format,
        reserved: 0,
        // Compiler-projected twelve-u32 record, all bytes initialized.
        color: unsafe { core::mem::transmute(color) },
    }
}
pub fn publish(config: &mut a::WindowGraphicsConfig) {
    config.format_count = 8;
    config.formats = [
        format(g::format_xrgb8888, description(false, true)),
        format(g::format_argb8888, description(false, false)),
        format(g::format_abgr16161616f, description(true, false)),
        format(g::format_abgr16161616f, description(true, true)),
        format(g::format_abgr16161616f, scrgb(false)),
        format(g::format_abgr16161616f, scrgb(true)),
        format(g::format_argb2101010, pq(false)),
        format(g::format_xrgb2101010, pq(true)),
    ];
}
pub fn publish_cpu(config: &mut a::WindowGraphicsConfig) {
    if config.flags & a::window_graphics_headless != 0 {
        publish(config);
        return;
    }
    config.format_count = 2;
    config.formats = [a::WindowGraphicsFormat::default(); 8];
    config.formats[0] = format(g::format_xrgb8888, description(false, true));
    config.formats[1] = format(g::format_argb8888, description(false, false));
}
