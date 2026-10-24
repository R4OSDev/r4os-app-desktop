//! Bounded frame-text traversal. Scalar decoding uses Rust's UTF-8 validator;
//! malformed or incomplete input advances one byte, as the original painter.
//! Font replacement cannot shrink the generation's conservative index bounds.
use crate::render::geometry::Rect;
pub const CELL: usize = 8;
pub const MAX_CELL: usize = 40;

pub fn decode(bytes: &[u8]) -> (u32, usize) {
    let Some(&first) = bytes.first() else {
        return (0xfffd, 1);
    };
    let length = match first {
        0..=0x7f => return (u32::from(first), 1),
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return (0xfffd, 1),
    };
    if let Some(text) = bytes
        .get(..length)
        .and_then(|b| core::str::from_utf8(b).ok())
    {
        (u32::from(text.chars().next().expect("one scalar")), length)
    } else {
        (0xfffd, 1)
    }
}
pub fn conservative_extent(font: u32, value: &[u8]) -> (i64, i64) {
    let cell = if font == r4os_contract::gui_font_builtin_id {
        CELL
    } else {
        MAX_CELL
    } as i64;
    let (mut width, mut line, mut height) = (0, 0_i64, cell);
    let mut offset = 0;
    while offset < value.len() {
        let (scalar, count) = decode(&value[offset..]);
        offset += count;
        match scalar {
            13 => (),
            10 => {
                line = 0;
                height = height.saturating_add(cell);
            }
            _ => {
                line = line.saturating_add(cell);
                width = core::cmp::max(width, line);
            }
        }
    }
    (width, height)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
    pub scalar: u32,
}
pub struct Visible<'a> {
    value: &'a [u8],
    bounds: Rect,
    start_x: i32,
    x: i32,
    y: i32,
    advance: i32,
    line_height: i32,
    offset: usize,
}
impl<'a> Visible<'a> {
    pub fn new(
        value: &'a [u8],
        bounds: Rect,
        x: i32,
        y: i32,
        advance: i32,
        line_height: i32,
    ) -> Self {
        Self {
            value,
            bounds,
            start_x: x,
            x,
            y,
            advance,
            line_height,
            offset: 0,
        }
    }
}
impl Iterator for Visible<'_> {
    type Item = Cell;
    fn next(&mut self) -> Option<Cell> {
        if self.bounds.is_empty() || self.start_x >= self.bounds.right() {
            return None;
        }
        while self.offset < self.value.len() {
            if self.y >= self.bounds.bottom() {
                return None;
            }
            if self.y.saturating_add(self.line_height) <= self.bounds.y
                || self.x >= self.bounds.right()
            {
                self.offset += self.value[self.offset..].iter().position(|&b| b == b'\n')? + 1;
                self.x = self.start_x;
                self.y = self.y.saturating_add(self.line_height);
                continue;
            }
            let (scalar, count) = decode(&self.value[self.offset..]);
            self.offset += count;
            match scalar {
                13 => continue,
                10 => {
                    self.x = self.start_x;
                    self.y = self.y.saturating_add(self.line_height);
                    continue;
                }
                _ => (),
            }
            let cell = Cell {
                x: self.x,
                y: self.y,
                scalar,
            };
            self.x = self.x.saturating_add(self.advance);
            if cell.x.saturating_add(self.advance) > self.bounds.x {
                return Some(cell);
            }
        }
        None
    }
}
/// The current painter intentionally clips glyph output at desktop zero as
/// well as the requested scene/output bounds. Keep that policy during the port.
pub fn clip_cell(
    bounds: Rect,
    x: i32,
    y: i32,
    w: usize,
    h: usize,
    right: i32,
    bottom: i32,
) -> Option<Rect> {
    if bounds.is_empty() || right <= 0 || bottom <= 0 {
        return None;
    }
    let left = i64::from(bounds.x).max(i64::from(x)).max(0);
    let top = i64::from(bounds.y).max(i64::from(y)).max(0);
    let end_x = i64::from(right)
        .min(i64::from(bounds.x) + i64::from(bounds.w))
        .min(i64::from(x) + i64::try_from(w).ok()?);
    let end_y = i64::from(bottom)
        .min(i64::from(bounds.y) + i64::from(bounds.h))
        .min(i64::from(y) + i64::try_from(h).ok()?);
    (end_x > left && end_y > top).then(|| {
        Rect::new(
            left as i32,
            top as i32,
            (end_x - left) as i32,
            (end_y - top) as i32,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_hidden_lines_unicode_edges_and_length_delimited_input() {
        let mut visible = Visible::new(
            "invisible ÄÖÜ\né中x\nsuffix".as_bytes(),
            Rect::new(9, 8, 1, 8),
            0,
            0,
            8,
            8,
        );
        assert_eq!(
            visible.next(),
            Some(Cell {
                x: 8,
                y: 8,
                scalar: '中' as u32
            })
        );
        assert_eq!(visible.next(), None);
        let bytes = [b'A'; 4096];
        let mut hidden = Visible::new(&bytes, Rect::new(0, 0, 1, 1), 20, 20, 8, 8);
        assert_eq!(hidden.next(), None);
        assert_eq!(hidden.offset, 0);
        let mut long = [b'\r'; 4097];
        long[4096] = b'A';
        assert_eq!(
            Visible::new(&long, Rect::new(0, 0, 8, 8), 0, 0, 8, 8).next(),
            Some(Cell {
                x: 0,
                y: 0,
                scalar: 65
            })
        );
        let mut edge = Visible::new(
            b"AA\nAA",
            Rect::new(0, 0, 1, 1),
            i32::MIN,
            i32::MAX - 4,
            8,
            8,
        );
        assert_eq!(edge.next(), None);
    }
    #[test]
    fn original_utf8_scalar_and_malformed_byte_advance() {
        for (bytes, expected) in [
            ("ä".as_bytes(), (0xe4, 2)),
            ("€".as_bytes(), (0x20ac, 3)),
            (b"\xc3(".as_slice(), (0xfffd, 1)),
        ] {
            assert_eq!(decode(bytes), expected);
        }
        for bytes in [
            b"\xed\xa0\x80".as_slice(),
            b"\xf4\x90\x80\x80",
            b"\xe0\x80\x80",
            b"\xf0\x9f",
            b"\x80",
        ] {
            assert_eq!(decode(bytes), (0xfffd, 1));
        }
        assert_eq!(conservative_extent(0, "é\n中".as_bytes()), (8, 16));
        assert_eq!(conservative_extent(1, "é\n中".as_bytes()), (40, 80));
    }
}
