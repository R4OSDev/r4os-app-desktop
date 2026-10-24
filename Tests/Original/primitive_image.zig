//! Immutable source views consumed during desktop capture. Decode and font
//! ownership stay with their existing providers; these views never escape it.
const std = @import("std");
const surface = @import("surface.zig");
pub const Format = enum(u32) { xrgb, argb, alpha, indexed, glyph };
pub const Identity = struct {
    font: u32 = 0,
    revision: u32 = 0,
    glyph: u32 = 0,
    dpi_x: u32 = 96,
    dpi_y: u32 = 96,
};
pub const View = struct {
    format: Format,
    width: u32,
    height: u32,
    stride: usize,
    bytes: []const u8 = &.{},
    palette: []align(1) const u32 = &.{},
    rows: []const u64 = &.{},
    foreground: u32 = 0,
    background: u32 = 0,
    identity: Identity = .{},
    // Source-space rotation is part of the cache key. It is applied once
    // on an asset miss, never by downloading a final GPU framebuffer.
    rotation: u2 = 0,

    pub fn rasterWidth(self: View) u32 { return if (self.rotation & 1 != 0) self.height else self.width; }
    pub fn rasterHeight(self: View) u32 { return if (self.rotation & 1 != 0) self.width else self.height; }
    pub fn rasterPixel(self: View, x: usize, y: usize) u32 {
        return switch (self.rotation) {
            0 => self.pixel(x, y),
            1 => self.pixel(self.width - 1 - y, x),
            2 => self.pixel(self.width - 1 - x, self.height - 1 - y),
            3 => self.pixel(y, self.height - 1 - x),
        };
    }

    pub fn valid(self: View) bool {
        if (self.width == 0 or self.height == 0 or self.width > 8192 or self.height > 8192 or
            @as(u64, self.width) * self.height > 16 * 1024 * 1024 or self.identity.dpi_x == 0 or self.identity.dpi_y == 0) return false;
        if (self.format == .glyph) return self.width <= 64 and self.height <= 64 and self.rows.len >= self.height and self.identity.revision != 0;
        if (self.format == .indexed and self.palette.len < 256) return false;
        const row = @as(usize, self.width) * self.bytesPerPixel();
        const prior = std.math.mul(usize, self.height - 1, self.stride) catch return false;
        const length = std.math.add(usize, prior, row) catch return false;
        return self.stride >= row and self.bytes.len >= length;
    }
    pub fn bytesPerPixel(self: View) usize { return if (self.format == .xrgb or self.format == .argb) 4 else 1; }
    pub fn key(self: View) [32]u8 {
        var hash = std.crypto.hash.Blake3.init(.{});
        const values = [_]u32{ @intFromEnum(self.format), self.width, self.height, self.foreground, self.background,
            self.identity.font, self.identity.revision, self.identity.glyph, self.identity.dpi_x, self.identity.dpi_y, self.rotation };
        hash.update(std.mem.asBytes(&values));
        if (self.format == .glyph) {
            // Ignore provider row padding above the actual cell width.
            const mask: u64 = if (self.width == 64) std.math.maxInt(u64) else (@as(u64, 1) << @intCast(self.width)) - 1;
            for (self.rows[0..self.height]) |row| { const bits = row & mask; hash.update(std.mem.asBytes(&bits)); }
        } else {
            for (0..self.height) |row| hash.update(self.bytes[row * self.stride..][0..@as(usize, self.width) * self.bytesPerPixel()]);
            if (self.format == .indexed) hash.update(std.mem.sliceAsBytes(self.palette[0..256]));
        }
        var result: [32]u8 = undefined; hash.final(&result); return result;
    }
    pub fn pixel(self: View, x: usize, y: usize) u32 {
        if (self.format == .glyph) return 0xff000000 | (0xffffff &
            (if (self.rows[y] & (@as(u64, 1) << @intCast(x)) != 0) self.foreground else self.background));
        const offset = y * self.stride + x * self.bytesPerPixel();
        return switch (self.format) {
            .xrgb => 0xff000000 | read(self.bytes[offset..][0..4]),
            .argb => premultiply(read(self.bytes[offset..][0..4])),
            .alpha => premultiply((self.foreground & 0xffffff) | (@as(u32, self.bytes[offset]) << 24)),
            .indexed => 0xff000000 | (self.palette[self.bytes[offset]] & 0xffffff),
            .glyph => unreachable,
        };
    }
};
pub const Picture = struct {
    view: View,
    viewport: surface.Rect,
    clip: surface.Rect,
    source_x: u32 = 0,
    source_y: u32 = 0,
    guest_w: u32,
    guest_h: u32,
};
pub const Paint = union(enum) {
    clear: surface.Rect,
    fill: struct { rect: surface.Rect, rgb: u32 },
    picture: Picture,
};
fn read(bytes: []const u8) u32 { return std.mem.readInt(u32, bytes[0..4], .little); }
fn premultiply(value: u32) u32 {
    const alpha = value >> 24;
    var result = alpha << 24;
    inline for (.{ 0, 8, 16 }) |shift| result |= ((((value >> shift) & 255) * alpha + 127) / 255) << shift;
    return result;
}
