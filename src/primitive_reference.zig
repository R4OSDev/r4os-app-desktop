//! Original 0.79.21 painter fixtures shared by the queue model and the explicit
//! hardware readback consumer. Geometry, source values and tolerances are kept.
const std = @import("std");
const r4os = @import("r4os");
const a = r4os.abi;
const scene = @import("scene_buffer.zig");
pub const basic_tolerance = 1;
pub const shapes_tolerance = 3;
fn require(ok: bool) !void { if (!ok) return error.ReferencePaint; }

pub fn basic(painter: *scene.SceneBuffer, draw: *const r4os.r4draw.Context) !void {
    painter.fillRect(.{ .x = 0, .y = 0, .w = 8, .h = 8 }, 0x203040);
    @import("paint.zig").textScene(painter, draw, 0, 0, "A", 0xffffff, 0x203040);
    const indices = [_]u8{ 0, 1, 1, 0 };
    var palette: [256]u32 = @splat(0); palette[0] = 0x773311; palette[1] = 0x229955;
    try require(painter.blitIndexed8Nearest(.{ .x = 0, .y = 4, .w = 4, .h = 4 }, .{ .indices = &indices, .palette = &palette,
        .source_x = 0, .source_y = 0, .source_w = 2, .source_h = 2, .source_stride = 2, .guest_w = 2, .guest_h = 2,
        .viewport = .{ .x = 0, .y = 4, .w = 4, .h = 4 } }));
    try require(painter.blendAlpha8(4, 4, 2, 2, 2, 0x669933, &.{ 0, 128, 255, 64 }));
    const argb = [_]u32{ 0x80ff0000, 0xff445566 };
    try require(painter.blendArgb32(painter.fullRect(), 5, 6, 2, 1, 1, std.mem.sliceAsBytes(&argb)));
}

pub fn shapes(painter: *scene.SceneBuffer, pixels: []const u32, allocator: std.mem.Allocator) !void {
    const raster = @import("gui_shape_renderer.zig");
    try require(painter.blitXrgb32Nearest(painter.fullRect(), .{ .pixels = pixels, .source_x = 0, .source_y = 0,
        .source_w = 1024, .source_h = 512, .source_stride = 1024, .guest_w = 1024, .guest_h = 512,
        .viewport = .{ .x = -300, .y = -200, .w = 1024, .h = 512 } }));
    for (0..16) |index| painter.fillRect(.{ .x = @intCast(index % 8), .y = @intCast(index / 8), .w = 1, .h = 1 },
        0x102030 + @as(u32, @intCast(index)) * 0x010101);
    var bytes: [@sizeOf(a.GuiShapeResource) + 4 * @sizeOf(a.GuiPathSegment)]u8 = undefined;
    const rounded = try r4os.gui_shapes.roundedRect(&bytes, .{ .x = 2, .y = 2, .w = 4, .h = 4,
        .radii = .{ .top_left_x = 2, .top_left_y = 2, .bottom_right_x = 2, .bottom_right_y = 2 },
        .fill_argb = 0x8070b010, .shadow = .{ .argb = 0x90000000, .offset_x = 1, .offset_y = 1, .blur = 1 } });
    for ([_]u32{ a.gui_frame_command_kind_shadow, a.gui_frame_command_kind_rounded_rect }) |kind| {
        const command = try r4os.gui_shapes.command(kind, 0, 0, 8, 8, 0, rounded.len);
        try require(raster.replay(allocator, painter, painter.fullRect(), command, rounded) == .drawn);
    }
    var path = try r4os.gui_shapes.PathBuilder.init(&bytes, .{ .stroke_argb = 0xc0e030a0, .stroke_width = 1.5, .line_cap = .round });
    try path.moveTo(.{ .x = 0, .y = 7 });
    try path.cubicTo(.{ .x = 1, .y = 2 }, .{ .x = 6, .y = 2 }, .{ .x = 7, .y = 7 });
    const curve = try path.finish();
    const command = try r4os.gui_shapes.command(a.gui_frame_command_kind_path_stroke, 0, 0, 8, 8, 0, curve.len);
    try require(raster.replay(allocator, painter, painter.fullRect(), command, curve) == .drawn);
}
