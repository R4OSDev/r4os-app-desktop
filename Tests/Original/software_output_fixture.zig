//! Public synthetic input for the bounded software-output comparison.
//! The hashes bind complete canvases, including untouched pixels. No ROM data.
const surface = @import("surface.zig");
pub const width = 1920;
pub const height = 1080;
pub const source_width = 256;
pub const source_height = 224;
pub const canvas_rgb = 0x123456;
pub const background_rgb = 0x2468ac;
pub const window_rgb = 0x102030;
pub const clients = [_]surface.Rect{
    .{ .x = 70, .y = 70, .w = 414, .h = 224 },
    .{ .x = 8, .y = 38, .w = 1904, .h = 1010 },
};
pub const hashes = [_][]const u8{
    "075b3f7143ecee7d15bc34d60a3de7a8ad185ff38e63ec3d91a5ac10e9be7873",
    "1e9e4db23bbbbdfb7b880cca679056f1b7670d5624e04bbbeeb586380a440c38",
};
pub fn sourcePixel(index: usize) u32 {
    return @intCast((index * 65537 + 0x173954) & 0xffffff);
}
pub fn frame(client: surface.Rect) surface.Rect {
    return .{ .x = client.x - 8, .y = client.y - 30, .w = client.w + 16, .h = client.h + 38 };
}
