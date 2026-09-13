//! RemoteFrame copies pixels synchronously. Overlay only the bounded arrow
//! during publication, then restore the canonical scene before scanout.
const asset = @import("cursor_asset.zig");
const scene_buffer = @import("scene_buffer.zig");
const surface = @import("surface.zig");
pub const Overlay = struct {
    saved: [asset.width * asset.height]u32 = undefined,
    rect: surface.Rect = .{ .x=0,.y=0,.w=0,.h=0 },
    pub fn apply(self: *Overlay, scene: *scene_buffer.SceneBuffer, x: i32, y: i32) void {
        self.rect = scene.clipRect(.{ .x=x,.y=y,.w=asset.width,.h=asset.height }) orelse return;
        const pixels = scene.pixels.?;
        for (0..@intCast(self.rect.h)) |row| for (0..@intCast(self.rect.w)) |col| {
            const index = (@as(usize,@intCast(self.rect.y)) + row) * @as(usize,@intCast(scene.width)) + @as(usize,@intCast(self.rect.x)) + col;
            self.saved[row * asset.width + col] = pixels[index];
            const argb = asset.pixel(col + @as(usize,@intCast(self.rect.x - x)), row + @as(usize,@intCast(self.rect.y - y)));
            if (argb >> 24 != 0) pixels[index] = argb & 0xffffff;
        };
    }
    pub fn restore(self: *const Overlay, scene: *scene_buffer.SceneBuffer) void {
        const pixels = scene.pixels orelse return;
        for (0..@intCast(self.rect.h)) |row| for (0..@intCast(self.rect.w)) |col| {
            const index = (@as(usize,@intCast(self.rect.y)) + row) * @as(usize,@intCast(scene.width)) + @as(usize,@intCast(self.rect.x)) + col;
            pixels[index] = self.saved[row * asset.width + col];
        };
    }
};
