//! Canonical Desktop arrow: software spans, hardware ARGB and capture.
pub const width: usize = 12;
pub const height: usize = 18;
pub fn pixel(x: usize, y: usize) u32 {
    if (x >= width or y >= height) return 0;
    if (bitSet(whiteBits(y), x)) return 0xffffffff;
    return if (bitSet(blackBits(y), x)) 0xff000000 else 0;
}
pub fn bitSet(bits: u16, col: usize) bool {
    const shift: u4 = @intCast(11 - col);
    return (bits & (@as(u16, 1) << shift)) != 0;
}

pub fn blackBits(row: usize) u16 {
    return switch (row) {
        0 => 0b100000000000,
        1 => 0b110000000000,
        2 => 0b101000000000,
        3 => 0b100100000000,
        4 => 0b100010000000,
        5 => 0b100001000000,
        6 => 0b100000100000,
        7 => 0b100000010000,
        8 => 0b100000001000,
        9 => 0b100000000100,
        10 => 0b100001111100,
        11 => 0b100101000000,
        12 => 0b101001000000,
        13 => 0b110010100000,
        14 => 0b100010100000,
        15 => 0b000010010000,
        16 => 0b000010010000,
        17 => 0b000001100000,
        else => 0,
    };
}

pub fn whiteBits(row: usize) u16 {
    return switch (row) {
        2 => 0b010000000000,
        3 => 0b011000000000,
        4 => 0b011100000000,
        5 => 0b011110000000,
        6 => 0b011111000000,
        7 => 0b011111100000,
        8 => 0b011111110000,
        9 => 0b011111111000,
        10 => 0b011110000000,
        11 => 0b011010000000,
        12 => 0b010010000000,
        13 => 0b000001000000,
        14 => 0b000001000000,
        15 => 0b000001100000,
        16 => 0b000001100000,
        else => 0,
    };
}

