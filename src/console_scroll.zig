const std = @import("std");

pub const Range = struct {
    total_lines: u32,
    visible_lines: u32,
    start_line: u32,
    end_line: u32,
    effective_offset: u32,

    pub fn tailVisible(self: Range) bool {
        return self.effective_offset == 0;
    }
};

pub fn countLines(text: []const u8, cols_raw: u32, codepage: u16) u32 {
    const cols = @max(@as(u32, 1), cols_raw);
    var count: u32 = 0;
    var line_len: u32 = 0;
    var i: usize = 0;
    while (i < text.len and text[i] != 0) : (i += 1) {
        const ch = text[i];
        if (ch == '\r') continue;
        if (ch == '\n' or line_len >= cols) {
            count += 1;
            line_len = 0;
            if (ch == '\n') continue;
        }
        if (printable(ch, codepage)) line_len += 1;
    }
    if (line_len > 0 or count == 0) count += 1;
    return count;
}

pub fn visibleRange(text: []const u8, cols: u32, rows_raw: u32, requested_offset: u32, codepage: u16) Range {
    const total = countLines(text, cols, codepage);
    const visible = @max(@as(u32, 1), rows_raw);
    const offset = clampOffset(total, visible, requested_offset);
    const end = total - offset;
    const start = if (end > visible) end - visible else 0;
    return .{
        .total_lines = total,
        .visible_lines = visible,
        .start_line = start,
        .end_line = end,
        .effective_offset = offset,
    };
}

pub fn clampOffset(total_lines: u32, visible_lines_raw: u32, requested_offset: u32) u32 {
    const visible = @max(@as(u32, 1), visible_lines_raw);
    const max_offset = if (total_lines > visible) total_lines - visible else 0;
    return @min(requested_offset, max_offset);
}

pub fn printable(ch: u8, codepage: u16) bool {
    if (ch >= 0x20 and ch <= 0x7E) return true;
    return codepage == 437 and ch >= 0x80;
}

test "line counting wraps and respects newlines" {
    try std.testing.expectEqual(@as(u32, 1), countLines("", 8, 437));
    try std.testing.expectEqual(@as(u32, 1), countLines("ABC", 8, 437));
    try std.testing.expectEqual(@as(u32, 2), countLines("ABCDE", 4, 437));
    try std.testing.expectEqual(@as(u32, 2), countLines("AB\nCD", 8, 437));
    try std.testing.expectEqual(@as(u32, 1), countLines("AB\n", 8, 437));
}

test "visible range clamps scroll offsets" {
    const text = "L1\nL2\nL3\nL4\nL5";
    const tail = visibleRange(text, 80, 3, 0, 437);
    try std.testing.expectEqual(@as(u32, 5), tail.total_lines);
    try std.testing.expectEqual(@as(u32, 2), tail.start_line);
    try std.testing.expectEqual(@as(u32, 5), tail.end_line);
    try std.testing.expect(tail.tailVisible());

    const up = visibleRange(text, 80, 3, 1, 437);
    try std.testing.expectEqual(@as(u32, 1), up.start_line);
    try std.testing.expectEqual(@as(u32, 4), up.end_line);
    try std.testing.expect(!up.tailVisible());

    const clamped = visibleRange(text, 80, 3, 99, 437);
    try std.testing.expectEqual(@as(u32, 0), clamped.start_line);
    try std.testing.expectEqual(@as(u32, 3), clamped.end_line);
    try std.testing.expectEqual(@as(u32, 2), clamped.effective_offset);
}
