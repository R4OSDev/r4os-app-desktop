const r4os = @import("r4os");
const glyphs = @import("font8.zig");
const scene_buffer = @import("scene_buffer.zig");
const std = @import("std");
const surface = @import("surface.zig");

const rect_chunk_w: usize = 128;
const rect_chunk_h: usize = 8;
const glyph_base_w: usize = 8;
const glyph_base_h: usize = 8;
const glyph_max_w: usize = 40;
const glyph_max_h: usize = 40;
const glyph_cache_slots: usize = 256;
const font_layout_cache_slots: usize = 16;
const text_max_bytes: usize = 4096;

pub fn rect(draw: *const r4os.r4draw.Context, x: i32, y: i32, w: u32, h: u32, rgb: u32) void {
    const clipped = clipRect(draw, x, y, w, h) orelse return;
    var pixels: [rect_chunk_w * rect_chunk_h]u32 = undefined;
    var yy = clipped.y0;
    while (yy < clipped.y1) {
        const bh: u32 = @intCast(@min(@as(i64, rect_chunk_h), clipped.y1 - yy));
        var xx = clipped.x0;
        while (xx < clipped.x1) {
            const bw: u32 = @intCast(@min(@as(i64, rect_chunk_w), clipped.x1 - xx));
            const count: usize = @intCast(@as(u64, bw) * bh);
            @memset(pixels[0..count], rgb);
            _ = draw.displayBlitXrgb32(@intCast(xx), @intCast(yy), bw, bh, pixels[0..count]);
            xx += bw;
        }
        yy += bh;
    }
}

pub fn rectScene(scene: *scene_buffer.SceneBuffer, x: i32, y: i32, w: u32, h: u32, rgb: u32) void {
    if (w > @as(u32, @intCast(std.math.maxInt(i32))) or h > @as(u32, @intCast(std.math.maxInt(i32)))) return;
    scene.fillRect(.{ .x = x, .y = y, .w = @intCast(w), .h = @intCast(h) }, rgb);
}

pub fn xrgb32(draw: *const r4os.r4draw.Context, x: i32, y: i32, w: u32, h: u32, pixels: []const u32) void {
    const clipped = clipRect(draw, x, y, w, h) orelse return;
    const needed = std.math.mul(usize, @as(usize, w), @as(usize, h)) catch return;
    if (pixels.len < needed) return;
    const source_stride: usize = @intCast(w);
    const source_x: usize = @intCast(clipped.x0 - x);
    const source_y: usize = @intCast(clipped.y0 - y);
    const copy_width: usize = @intCast(clipped.x1 - clipped.x0);
    var row: usize = 0;
    while (row < @as(usize, @intCast(clipped.y1 - clipped.y0))) : (row += 1) {
        const offset = (source_y + row) * source_stride + source_x;
        _ = draw.displayBlitXrgb32(
            @intCast(clipped.x0),
            @intCast(clipped.y0 + @as(i64, @intCast(row))),
            @intCast(copy_width),
            1,
            pixels[offset .. offset + copy_width],
        );
    }
}

pub fn xrgb32Scene(scene: *scene_buffer.SceneBuffer, x: i32, y: i32, w: u32, h: u32, pixels: []const u32) void {
    scene.blitXrgb32(x, y, w, h, pixels);
}

pub fn text(draw: *const r4os.r4draw.Context, x: i32, y: i32, value: [*:0]const u8, fg: u32, bg: u32) void {
    textFont(draw, r4os.abi.gui_font_builtin_id, x, y, value, fg, bg);
}

pub fn textScene(scene: *scene_buffer.SceneBuffer, draw: *const r4os.r4draw.Context, x: i32, y: i32, value: [*:0]const u8, fg: u32, bg: u32) void {
    textFontScene(scene, draw, r4os.abi.gui_font_builtin_id, x, y, value, fg, bg);
}

pub fn textFont(draw: *const r4os.r4draw.Context, font_id: u32, x: i32, y: i32, value: [*:0]const u8, fg: u32, bg: u32) void {
    if (@intFromPtr(value) == 0) return;
    const revision = fontCacheRevision(draw, font_id);
    const layout = fontLayout(draw, font_id, revision);
    const advance: i32 = @intCast(layout.cell_width);
    const line_h: i32 = @intCast(layout.cell_height);
    const start_x = x;
    var px = x;
    var py = y;
    var i: usize = 0;
    while (i < text_max_bytes and value[i] != 0) {
        const decoded = decodeUtf8Z(value, i, text_max_bytes);
        i += decoded.consumed;
        if (decoded.codepoint == '\r') continue;
        if (decoded.codepoint == '\n') {
            px = start_x;
            py += line_h;
            continue;
        }
        glyph(draw, font_id, revision, layout, px, py, decoded.codepoint, fg, bg);
        px += advance;
    }
}

pub fn textFontScene(scene: *scene_buffer.SceneBuffer, draw: *const r4os.r4draw.Context, font_id: u32, x: i32, y: i32, value: [*:0]const u8, fg: u32, bg: u32) void {
    if (@intFromPtr(value) == 0) return;
    const revision = fontCacheRevision(draw, font_id);
    const layout = fontLayout(draw, font_id, revision);
    const advance: i32 = @intCast(layout.cell_width);
    const line_h: i32 = @intCast(layout.cell_height);
    const start_x = x;
    var px = x;
    var py = y;
    var i: usize = 0;
    while (i < text_max_bytes and value[i] != 0) {
        const decoded = decodeUtf8Z(value, i, text_max_bytes);
        i += decoded.consumed;
        if (decoded.codepoint == '\r') continue;
        if (decoded.codepoint == '\n') {
            px = start_x;
            py += line_h;
            continue;
        }
        glyphScene(scene, draw, font_id, revision, layout, px, py, decoded.codepoint, fg, bg);
        px += advance;
    }
}

pub fn textFontSlice(
    draw: *const r4os.r4draw.Context,
    font_id: u32,
    x: i32,
    y: i32,
    value: []const u8,
    fg: u32,
    bg: u32,
    bounds: surface.Rect,
) void {
    const revision = fontCacheRevision(draw, font_id);
    const layout = fontLayout(draw, font_id, revision);
    const advance: i32 = @intCast(layout.cell_width);
    const line_h: i32 = @intCast(layout.cell_height);
    const start_x = x;
    var px = x;
    var py = y;
    var i: usize = 0;
    while (i < value.len) {
        const decoded = decodeUtf8Slice(value, i);
        i += decoded.consumed;
        if (decoded.codepoint == '\r') continue;
        if (decoded.codepoint == '\n') {
            px = start_x;
            py = addCoordinate(py, line_h);
            continue;
        }
        glyphClipped(draw, font_id, revision, layout, px, py, decoded.codepoint, fg, bg, bounds);
        px = addCoordinate(px, advance);
    }
}

pub fn textFontSliceScene(
    scene: *scene_buffer.SceneBuffer,
    draw: *const r4os.r4draw.Context,
    font_id: u32,
    x: i32,
    y: i32,
    value: []const u8,
    fg: u32,
    bg: u32,
    bounds: surface.Rect,
) void {
    const revision = fontCacheRevision(draw, font_id);
    const layout = fontLayout(draw, font_id, revision);
    const advance: i32 = @intCast(layout.cell_width);
    const line_h: i32 = @intCast(layout.cell_height);
    const start_x = x;
    var px = x;
    var py = y;
    var i: usize = 0;
    while (i < value.len) {
        const decoded = decodeUtf8Slice(value, i);
        i += decoded.consumed;
        if (decoded.codepoint == '\r') continue;
        if (decoded.codepoint == '\n') {
            px = start_x;
            py = addCoordinate(py, line_h);
            continue;
        }
        glyphSceneClipped(scene, draw, font_id, revision, layout, px, py, decoded.codepoint, fg, bg, bounds);
        px = addCoordinate(px, advance);
    }
}

fn addCoordinate(value: i32, delta: i32) i32 {
    return std.math.add(i32, value, delta) catch
        if (delta >= 0) std.math.maxInt(i32) else std.math.minInt(i32);
}

const FontLayout = struct {
    imported: bool = false,
    cell_width: usize = glyph_base_w,
    cell_height: usize = glyph_base_h,
    bitmap_height: usize = glyph_base_h,
};

pub const FontCacheStats = struct {
    layout_hits: u64 = 0,
    layout_misses: u64 = 0,
    glyph_hits: u64 = 0,
    glyph_misses: u64 = 0,
    glyph_loads: u64 = 0,
    invalidations: u64 = 0,
};

const FontLayoutCacheEntry = struct {
    used: bool = false,
    font_id: u32 = 0,
    revision: u32 = 0,
    layout: FontLayout = .{},
};

const GlyphCoverageCacheEntry = struct {
    used: bool = false,
    font_id: u32 = 0,
    revision: u32 = 0,
    codepoint: u32 = 0,
    rows: [glyph_max_h]u64 = .{0} ** glyph_max_h,
};

var font_layout_cache: [font_layout_cache_slots]FontLayoutCacheEntry =
    .{FontLayoutCacheEntry{}} ** font_layout_cache_slots;
var glyph_coverage_cache: [glyph_cache_slots]GlyphCoverageCacheEntry =
    .{GlyphCoverageCacheEntry{}} ** glyph_cache_slots;
var observed_font_revision: u32 = 0;
var font_cache_stats: FontCacheStats = .{};

pub fn currentFontCacheStats() FontCacheStats {
    return font_cache_stats;
}

pub fn resetFontCache() void {
    font_layout_cache = .{FontLayoutCacheEntry{}} ** font_layout_cache_slots;
    glyph_coverage_cache = .{GlyphCoverageCacheEntry{}} ** glyph_cache_slots;
    observed_font_revision = 0;
    font_cache_stats = .{};
}

fn fontCacheRevision(draw: *const r4os.r4draw.Context, font_id: u32) u32 {
    if (font_id == r4os.abi.gui_font_builtin_id) return if (observed_font_revision == 0) 1 else observed_font_revision;
    const revision = draw.fontRevision();
    observeFontRevision(revision);
    return revision;
}

fn observeFontRevision(revision_raw: u32) void {
    const revision = if (revision_raw == 0) 1 else revision_raw;
    if (observed_font_revision == 0) {
        observed_font_revision = revision;
        return;
    }
    if (observed_font_revision == revision) return;
    font_layout_cache = .{FontLayoutCacheEntry{}} ** font_layout_cache_slots;
    glyph_coverage_cache = .{GlyphCoverageCacheEntry{}} ** glyph_cache_slots;
    observed_font_revision = revision;
    font_cache_stats.invalidations +%= 1;
}

fn fontLayout(draw: *const r4os.r4draw.Context, font_id: u32, revision: u32) FontLayout {
    const slot = fontLayoutSlot(font_id, revision);
    const cached = &font_layout_cache[slot];
    if (cached.used and cached.font_id == font_id and cached.revision == revision) {
        font_cache_stats.layout_hits +%= 1;
        return cached.layout;
    }
    font_cache_stats.layout_misses +%= 1;
    var layout = FontLayout{};
    if (font_id != r4os.abi.gui_font_builtin_id and draw.hasFn("font_glyph_row")) {
        var info: r4os.abi.GuiFontInfo = .{};
        if (draw.fontInfo(font_id, &info) > 0 and (info.flags & r4os.abi.gui_font_flag_renderable) != 0) {
            const cell_width = @min(@max(@as(usize, 1), @as(usize, info.max_advance)), glyph_max_w);
            const bitmap_height = @min(@max(@as(usize, 1), @as(usize, info.height)), glyph_max_h);
            const cell_height = @min(@max(bitmap_height, @as(usize, info.line_height)), glyph_max_h);
            layout = .{ .imported = true, .cell_width = cell_width, .cell_height = cell_height, .bitmap_height = bitmap_height };
        }
    }
    cached.* = .{ .used = true, .font_id = font_id, .revision = revision, .layout = layout };
    return layout;
}

fn fontLayoutSlot(font_id: u32, revision: u32) usize {
    const mixed = (font_id *% 0x9E37_79B1) ^ (revision *% 0x85EB_CA6B);
    return @as(usize, mixed) & (font_layout_cache_slots - 1);
}

fn glyphCoverageSlot(font_id: u32, revision: u32, codepoint: u32) usize {
    const mixed = (font_id *% 0x9E37_79B1) ^ (revision *% 0x85EB_CA6B) ^ (codepoint *% 0xC2B2_AE35);
    return @as(usize, mixed) & (glyph_cache_slots - 1);
}

fn glyphCoverage(draw: *const r4os.r4draw.Context, font_id: u32, revision: u32, layout: FontLayout, codepoint: u32) *const GlyphCoverageCacheEntry {
    const slot = glyphCoverageSlot(font_id, revision, codepoint);
    const cached = &glyph_coverage_cache[slot];
    if (cached.used and cached.font_id == font_id and cached.revision == revision and cached.codepoint == codepoint) {
        font_cache_stats.glyph_hits +%= 1;
        return cached;
    }

    font_cache_stats.glyph_misses +%= 1;
    cached.* = .{ .used = true, .font_id = font_id, .revision = revision, .codepoint = codepoint };
    if (layout.imported) {
        var bitmap: r4os.abi.GuiGlyphBitmap = .{};
        if (draw.fontGlyphBitmap(font_id, codepoint, &bitmap) == 0) {
            const height = @min(@as(usize, bitmap.height), @min(layout.bitmap_height, cached.rows.len));
            @memcpy(cached.rows[0..height], bitmap.rows[0..height]);
        } else {
            var row: usize = 0;
            while (row < layout.bitmap_height) : (row += 1) cached.rows[row] = draw.fontGlyphRow(font_id, codepoint, @intCast(row));
        }
    } else {
        const pattern = glyphPattern(codepoint);
        var row: usize = 0;
        while (row < glyph_base_h) : (row += 1) {
            var col: usize = 0;
            while (col < glyph_base_w) : (col += 1) {
                if ((pattern[row] & (@as(u8, 0x80) >> @intCast(col))) != 0) cached.rows[row] |= @as(u64, 1) << @intCast(col);
            }
        }
    }
    font_cache_stats.glyph_loads +%= 1;
    return cached;
}

fn glyph(draw: *const r4os.r4draw.Context, font_id: u32, revision: u32, layout: FontLayout, x: i32, y: i32, codepoint: u32, fg: u32, bg: u32) void {
    if (x < 0 or y < 0) return;
    const w = layout.cell_width;
    const h = layout.cell_height;
    var pixels: [glyph_max_w * glyph_max_h]u32 = undefined;
    const screen_w: i32 = @intCast(draw.screenWidth());
    const screen_h: i32 = @intCast(draw.screenHeight());
    if (x >= screen_w or y >= screen_h) return;

    rasterizeGlyph(draw, font_id, revision, layout, codepoint, fg, bg, pixels[0 .. w * h]);

    _ = draw.displayBlitXrgb32(x, y, @intCast(w), @intCast(h), pixels[0 .. w * h]);
}

fn glyphScene(scene: *scene_buffer.SceneBuffer, draw: *const r4os.r4draw.Context, font_id: u32, revision: u32, layout: FontLayout, x: i32, y: i32, codepoint: u32, fg: u32, bg: u32) void {
    if (x < 0 or y < 0) return;
    const w = layout.cell_width;
    const h = layout.cell_height;
    var pixels: [glyph_max_w * glyph_max_h]u32 = undefined;
    if (x >= scene.width or y >= scene.height) return;

    rasterizeGlyph(draw, font_id, revision, layout, codepoint, fg, bg, pixels[0 .. w * h]);

    scene.blitXrgb32(x, y, @intCast(w), @intCast(h), pixels[0 .. w * h]);
}

fn glyphClipped(draw: *const r4os.r4draw.Context, font_id: u32, revision: u32, layout: FontLayout, x: i32, y: i32, codepoint: u32, fg: u32, bg: u32, bounds: surface.Rect) void {
    _ = clipGlyphCell(bounds, x, y, layout.cell_width, layout.cell_height, std.math.maxInt(i32), std.math.maxInt(i32)) orelse return;
    const screen_w: i32 = @intCast(@min(draw.screenWidth(), @as(u32, @intCast(std.math.maxInt(i32)))));
    const screen_h: i32 = @intCast(@min(draw.screenHeight(), @as(u32, @intCast(std.math.maxInt(i32)))));
    const clipped = clipGlyphCell(bounds, x, y, layout.cell_width, layout.cell_height, screen_w, screen_h) orelse return;
    var pixels: [glyph_max_w * glyph_max_h]u32 = undefined;
    const count = layout.cell_width * layout.cell_height;
    rasterizeGlyph(draw, font_id, revision, layout, codepoint, fg, bg, pixels[0..count]);
    blitClippedGlyph(draw, pixels[0..count], layout.cell_width, x, y, clipped);
}

fn glyphSceneClipped(scene: *scene_buffer.SceneBuffer, draw: *const r4os.r4draw.Context, font_id: u32, revision: u32, layout: FontLayout, x: i32, y: i32, codepoint: u32, fg: u32, bg: u32, bounds: surface.Rect) void {
    const clipped = clipGlyphCell(bounds, x, y, layout.cell_width, layout.cell_height, scene.width, scene.height) orelse return;
    var pixels: [glyph_max_w * glyph_max_h]u32 = undefined;
    const count = layout.cell_width * layout.cell_height;
    rasterizeGlyph(draw, font_id, revision, layout, codepoint, fg, bg, pixels[0..count]);
    blitClippedGlyphScene(scene, pixels[0..count], layout.cell_width, x, y, clipped);
}

fn rasterizeGlyph(draw: *const r4os.r4draw.Context, font_id: u32, revision: u32, layout: FontLayout, codepoint: u32, fg: u32, bg: u32, pixels: []u32) void {
    const coverage = glyphCoverage(draw, font_id, revision, layout, codepoint);
    var row: usize = 0;
    while (row < layout.cell_height) : (row += 1) {
        const source_row: u64 = coverage.rows[row];
        var col: usize = 0;
        while (col < layout.cell_width) : (col += 1) {
            const bit = (source_row & (@as(u64, 1) << @intCast(col))) != 0;
            pixels[row * layout.cell_width + col] = if (bit) fg else bg;
        }
    }
}

fn clipGlyphCell(bounds: surface.Rect, x: i32, y: i32, width: usize, height: usize, limit_w: i32, limit_h: i32) ?surface.Rect {
    if (bounds.isEmpty() or limit_w <= 0 or limit_h <= 0) return null;
    const left = @max(@as(i64, 0), @max(@as(i64, bounds.x), @as(i64, x)));
    const top = @max(@as(i64, 0), @max(@as(i64, bounds.y), @as(i64, y)));
    const right = @min(
        @as(i64, limit_w),
        @min(@as(i64, bounds.x) + @as(i64, bounds.w), @as(i64, x) + @as(i64, @intCast(width))),
    );
    const bottom = @min(
        @as(i64, limit_h),
        @min(@as(i64, bounds.y) + @as(i64, bounds.h), @as(i64, y) + @as(i64, @intCast(height))),
    );
    if (right <= left or bottom <= top) return null;
    return .{ .x = @intCast(left), .y = @intCast(top), .w = @intCast(right - left), .h = @intCast(bottom - top) };
}

fn blitClippedGlyph(draw: *const r4os.r4draw.Context, pixels: []const u32, source_width: usize, x: i32, y: i32, clipped: surface.Rect) void {
    const source_height = pixels.len / source_width;
    if (clipped.x == x and clipped.y == y and clipped.w == source_width and clipped.h == source_height) {
        _ = draw.displayBlitXrgb32(x, y, @intCast(source_width), @intCast(source_height), pixels);
        return;
    }
    const source_x: usize = @intCast(clipped.x - x);
    const source_y: usize = @intCast(clipped.y - y);
    const copy_width: usize = @intCast(clipped.w);
    var row: usize = 0;
    while (row < @as(usize, @intCast(clipped.h))) : (row += 1) {
        const offset = (source_y + row) * source_width + source_x;
        _ = draw.displayBlitXrgb32(clipped.x, clipped.y + @as(i32, @intCast(row)), @intCast(copy_width), 1, pixels[offset .. offset + copy_width]);
    }
}

fn blitClippedGlyphScene(scene: *scene_buffer.SceneBuffer, pixels: []const u32, source_width: usize, x: i32, y: i32, clipped: surface.Rect) void {
    const source_height = pixels.len / source_width;
    if (clipped.x == x and clipped.y == y and clipped.w == source_width and clipped.h == source_height) {
        scene.blitXrgb32(x, y, @intCast(source_width), @intCast(source_height), pixels);
        return;
    }
    const source_x: usize = @intCast(clipped.x - x);
    const source_y: usize = @intCast(clipped.y - y);
    const copy_width: usize = @intCast(clipped.w);
    var row: usize = 0;
    while (row < @as(usize, @intCast(clipped.h))) : (row += 1) {
        const offset = (source_y + row) * source_width + source_x;
        scene.blitXrgb32(clipped.x, clipped.y + @as(i32, @intCast(row)), @intCast(copy_width), 1, pixels[offset .. offset + copy_width]);
    }
}

fn glyphPattern(codepoint: u32) [8]u8 {
    return glyphs.glyphForCodepoint(codepoint);
}

const DecodedScalar = struct {
    codepoint: u32,
    consumed: usize,
};

fn decodeUtf8Z(value: [*:0]const u8, start: usize, limit: usize) DecodedScalar {
    const first = value[start];
    if (first < 0x80) return .{ .codepoint = first, .consumed = 1 };
    const expected: usize = if (first >= 0xC2 and first <= 0xDF)
        2
    else if (first >= 0xE0 and first <= 0xEF)
        3
    else if (first >= 0xF0 and first <= 0xF4)
        4
    else
        return .{ .codepoint = 0xFFFD, .consumed = 1 };
    if (expected > limit -| start) return .{ .codepoint = 0xFFFD, .consumed = 1 };
    var codepoint: u32 = first & (@as(u8, 0x7F) >> @intCast(expected));
    var offset: usize = 1;
    while (offset < expected) : (offset += 1) {
        const byte = value[start + offset];
        if (byte == 0 or (byte & 0xC0) != 0x80) return .{ .codepoint = 0xFFFD, .consumed = 1 };
        codepoint = (codepoint << 6) | (byte & 0x3F);
    }
    if ((expected == 3 and codepoint < 0x800) or
        (expected == 4 and codepoint < 0x10000) or
        codepoint > 0x10FFFF or
        (codepoint >= 0xD800 and codepoint <= 0xDFFF))
    {
        return .{ .codepoint = 0xFFFD, .consumed = 1 };
    }
    return .{ .codepoint = codepoint, .consumed = expected };
}

fn decodeUtf8Slice(value: []const u8, start: usize) DecodedScalar {
    if (start >= value.len) return .{ .codepoint = 0xFFFD, .consumed = 1 };
    const first = value[start];
    if (first < 0x80) return .{ .codepoint = first, .consumed = 1 };
    const expected: usize = if (first >= 0xC2 and first <= 0xDF)
        2
    else if (first >= 0xE0 and first <= 0xEF)
        3
    else if (first >= 0xF0 and first <= 0xF4)
        4
    else
        return .{ .codepoint = 0xFFFD, .consumed = 1 };
    if (expected > value.len - start) return .{ .codepoint = 0xFFFD, .consumed = 1 };
    var codepoint: u32 = first & (@as(u8, 0x7F) >> @intCast(expected));
    var offset: usize = 1;
    while (offset < expected) : (offset += 1) {
        const byte = value[start + offset];
        if ((byte & 0xC0) != 0x80) return .{ .codepoint = 0xFFFD, .consumed = 1 };
        codepoint = (codepoint << 6) | (byte & 0x3F);
    }
    if ((expected == 3 and codepoint < 0x800) or
        (expected == 4 and codepoint < 0x10000) or
        codepoint > 0x10FFFF or
        (codepoint >= 0xD800 and codepoint <= 0xDFFF))
    {
        return .{ .codepoint = 0xFFFD, .consumed = 1 };
    }
    return .{ .codepoint = codepoint, .consumed = expected };
}

const ClippedRect = struct {
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
};

fn clipRect(draw: *const r4os.r4draw.Context, x: i32, y: i32, w: u32, h: u32) ?ClippedRect {
    if (w == 0 or h == 0) return null;
    const screen_w: i64 = draw.screenWidth();
    const screen_h: i64 = draw.screenHeight();
    var x0: i64 = x;
    var y0: i64 = y;
    var x1 = x0 + @as(i64, w);
    var y1 = y0 + @as(i64, h);
    if (x0 < 0) x0 = 0;
    if (y0 < 0) y0 = 0;
    if (x1 > screen_w) x1 = screen_w;
    if (y1 > screen_h) y1 = screen_h;
    if (x1 <= x0 or y1 <= y0) return null;
    return .{ .x0 = x0, .y0 = y0, .x1 = x1, .y1 = y1 };
}

test "scene text writes into buffer" {
    var pixels: [8 * 8]u32 = .{0} ** (8 * 8);
    var scene = scene_buffer.SceneBuffer{};
    try std.testing.expect(scene.attach(std.mem.sliceAsBytes(pixels[0..]), 8, 8));
    scene.fillRect(surface.Rect{ .x = 0, .y = 0, .w = 8, .h = 8 }, 0x000000);
    glyphScene(&scene, undefined, r4os.abi.gui_font_builtin_id, 1, .{}, 0, 0, 'A', 0xFFFFFF, 0x000000);
    try std.testing.expect(pixels[0] != 0 or pixels[1] != 0 or pixels[2] != 0 or pixels[3] != 0 or pixels[4] != 0 or pixels[5] != 0 or pixels[6] != 0 or pixels[7] != 0);
}

test "glyph coverage cache is generation keyed and color neutral" {
    resetFontCache();
    defer resetFontCache();
    observeFontRevision(7);

    var reference: [16 * 8]u32 = .{0} ** (16 * 8);
    var cached_pixels: [16 * 8]u32 = .{0} ** (16 * 8);
    var reference_scene = scene_buffer.SceneBuffer{};
    var cached_scene = scene_buffer.SceneBuffer{};
    try std.testing.expect(reference_scene.attach(std.mem.sliceAsBytes(reference[0..]), 16, 8));
    try std.testing.expect(cached_scene.attach(std.mem.sliceAsBytes(cached_pixels[0..]), 16, 8));

    textFontScene(&reference_scene, undefined, r4os.abi.gui_font_builtin_id, 0, 0, "AA", 0x00FF_FFFF, 0x0000_0000);
    const after_first = currentFontCacheStats();
    try std.testing.expectEqual(@as(u64, 1), after_first.glyph_misses);
    try std.testing.expectEqual(@as(u64, 1), after_first.glyph_hits);
    try std.testing.expectEqual(@as(u64, 1), after_first.glyph_loads);

    textFontScene(&cached_scene, undefined, r4os.abi.gui_font_builtin_id, 0, 0, "AA", 0x00FF_FFFF, 0x0000_0000);
    try std.testing.expectEqualSlices(u32, reference[0..], cached_pixels[0..]);
    const after_second = currentFontCacheStats();
    try std.testing.expectEqual(@as(u64, 3), after_second.glyph_hits);
    try std.testing.expectEqual(after_first.glyph_loads, after_second.glyph_loads);

    @memset(cached_pixels[0..], 0);
    textFontScene(&cached_scene, undefined, r4os.abi.gui_font_builtin_id, 0, 0, "A", 0x0012_3456, 0x0065_4321);
    const after_recolor = currentFontCacheStats();
    try std.testing.expectEqual(after_second.glyph_loads, after_recolor.glyph_loads);
    try std.testing.expect(std.mem.indexOfScalar(u32, cached_pixels[0..64], 0x0012_3456) != null);
    try std.testing.expect(std.mem.indexOfScalar(u32, cached_pixels[0..64], 0x0065_4321) != null);

    observeFontRevision(8);
    @memset(cached_pixels[0..], 0);
    textFontScene(&cached_scene, undefined, r4os.abi.gui_font_builtin_id, 0, 0, "A", 0x00FF_FFFF, 0);
    const after_generation = currentFontCacheStats();
    try std.testing.expectEqual(@as(u64, 1), after_generation.invalidations);
    try std.testing.expectEqual(after_recolor.glyph_loads + 1, after_generation.glyph_loads);
}

test "utf8 text renders one cell per scalar" {
    const decoded = decodeUtf8Z("\xc3\xa4", 0, 2);
    try std.testing.expectEqual(@as(u32, 0xE4), decoded.codepoint);
    try std.testing.expectEqual(@as(usize, 2), decoded.consumed);
    const bmp = decodeUtf8Z("\xe2\x82\xac", 0, 3);
    try std.testing.expectEqual(@as(u32, 0x20AC), bmp.codepoint);
    try std.testing.expectEqual(@as(usize, 3), bmp.consumed);
    const malformed = decodeUtf8Z("\xc3(", 0, 2);
    try std.testing.expectEqual(@as(u32, 0xFFFD), malformed.codepoint);
    try std.testing.expectEqual(@as(usize, 1), malformed.consumed);

    var pixels: [16 * 8]u32 = .{0x112233} ** (16 * 8);
    var scene = scene_buffer.SceneBuffer{};
    try std.testing.expect(scene.attach(std.mem.sliceAsBytes(pixels[0..]), 16, 8));
    textFontScene(&scene, undefined, r4os.abi.gui_font_builtin_id, 0, 0, "\xc3\xa4", 0xFFFFFF, 0x000000);
    var second_cell_untouched = true;
    var row: usize = 0;
    while (row < 8) : (row += 1) {
        for (pixels[row * 16 + 8 .. row * 16 + 16]) |pixel| {
            if (pixel != 0x112233) second_cell_untouched = false;
        }
    }
    try std.testing.expect(second_cell_untouched);
}

test "length delimited frame text has no legacy byte cap" {
    var pixels: [8 * 8]u32 = .{0} ** (8 * 8);
    var scene = scene_buffer.SceneBuffer{};
    try std.testing.expect(scene.attach(std.mem.sliceAsBytes(pixels[0..]), 8, 8));
    var text_bytes: [4097]u8 = .{'\r'} ** 4097;
    text_bytes[text_bytes.len - 1] = 'A';
    textFontSliceScene(
        &scene,
        undefined,
        r4os.abi.gui_font_builtin_id,
        0,
        0,
        text_bytes[0..],
        0xFFFFFF,
        0x000000,
        .{ .x = 0, .y = 0, .w = 8, .h = 8 },
    );
    var painted = false;
    for (pixels) |pixel| {
        if (pixel != 0) painted = true;
    }
    try std.testing.expect(painted);
}

test "length delimited text coordinates saturate at i32 edges" {
    const coord_max = std.math.maxInt(i32);
    const coord_min = std.math.minInt(i32);
    try std.testing.expectEqual(coord_max, addCoordinate(coord_max - 8, 8));
    try std.testing.expectEqual(coord_max, addCoordinate(coord_max - 7, 8));
    try std.testing.expectEqual(coord_min, addCoordinate(coord_min + 8, -8));
    try std.testing.expectEqual(coord_min, addCoordinate(coord_min + 7, -8));
    try std.testing.expectEqual(@as(i32, 17), addCoordinate(9, 8));

    var pixels = [_]u32{0x0012_3456};
    var scene = scene_buffer.SceneBuffer{};
    try std.testing.expect(scene.attach(std.mem.sliceAsBytes(pixels[0..]), 1, 1));
    const bounds = surface.Rect{ .x = 0, .y = 0, .w = 1, .h = 1 };
    textFontSlice(
        undefined,
        r4os.abi.gui_font_builtin_id,
        coord_max - 4,
        coord_max - 4,
        "AA\nAA",
        0xFFFFFF,
        0,
        bounds,
    );
    textFontSliceScene(
        &scene,
        undefined,
        r4os.abi.gui_font_builtin_id,
        coord_min,
        coord_max - 4,
        "AA\nAA",
        0xFFFFFF,
        0,
        bounds,
    );
    try std.testing.expectEqual(@as(u32, 0x0012_3456), pixels[0]);
}

test "frame text replays every intersecting glyph edge inside incremental damage" {
    const width = 24;
    const height = 24;
    const untouched: u32 = 0x0012_3456;
    const bounds = surface.Rect{ .x = 0, .y = 0, .w = width, .h = height };

    var reference: [width * height]u32 = .{untouched} ** (width * height);
    var reference_scene = scene_buffer.SceneBuffer{};
    try std.testing.expect(reference_scene.attach(std.mem.sliceAsBytes(reference[0..]), width, height));
    textFontSliceScene(&reference_scene, undefined, r4os.abi.gui_font_builtin_id, 8, 8, "A", 0x00FF_FFFF, 0, bounds);

    const clips = [_]surface.Rect{
        .{ .x = 8, .y = 10, .w = 2, .h = 3 },
        .{ .x = 14, .y = 10, .w = 2, .h = 3 },
        .{ .x = 10, .y = 8, .w = 3, .h = 2 },
        .{ .x = 10, .y = 14, .w = 3, .h = 2 },
    };
    for (clips) |clip| {
        var actual: [width * height]u32 = .{untouched} ** (width * height);
        var scene = scene_buffer.SceneBuffer{};
        try std.testing.expect(scene.attach(std.mem.sliceAsBytes(actual[0..]), width, height));
        scene.setPaintClip(clip);
        textFontSliceScene(&scene, undefined, r4os.abi.gui_font_builtin_id, 8, 8, "A", 0x00FF_FFFF, 0, clip);

        var py: i32 = 0;
        while (py < height) : (py += 1) {
            var px: i32 = 0;
            while (px < width) : (px += 1) {
                const index = @as(usize, @intCast(py * width + px));
                if (clip.contains(px, py)) {
                    try std.testing.expectEqual(reference[index], actual[index]);
                } else {
                    try std.testing.expectEqual(untouched, actual[index]);
                }
            }
        }
    }
}
