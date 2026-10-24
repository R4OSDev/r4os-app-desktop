//! Current UTF8 console rows. Font codepage does not reinterpret transcript
//! bytes; the existing R4STD decoder stops malformed/incomplete tails.
use r4os_text_core::{Decoded, Stream, printable};
#[derive(Clone, Copy, Debug)]
pub struct Range {
    pub total_lines: u32,
    pub visible_lines: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub effective_offset: u32,
}
impl Range {
    pub fn tail_visible(self) -> bool {
        self.effective_offset == 0
    }
}
pub fn count_lines(text: &[u8], cols_raw: u32, _codepage: u16) -> u32 {
    let cols = cols_raw.max(1);
    let (mut count, mut line_len) = (0, 0);
    let mut decoder = Stream::EMPTY;
    for byte in text {
        let ch = match decoder.push(*byte) {
            Decoded::Scalar(value) => value,
            Decoded::Incomplete => continue,
            Decoded::Invalid => break,
        };
        if ch == 0 {
            break;
        }
        if ch == u32::from(b'\r') {
            continue;
        }
        if ch == u32::from(b'\n') || line_len >= cols {
            count += 1;
            line_len = 0;
            if ch == u32::from(b'\n') {
                continue;
            }
        }
        if printable(ch) {
            line_len += 1;
        }
    }
    if line_len > 0 || count == 0 {
        count += 1;
    }
    count
}
pub fn clamp_offset(total: u32, visible_raw: u32, requested: u32) -> u32 {
    requested.min(total.saturating_sub(visible_raw.max(1)))
}
pub fn visible_range(
    text: &[u8],
    cols: u32,
    rows_raw: u32,
    requested: u32,
    codepage: u16,
) -> Range {
    let total = count_lines(text, cols, codepage);
    let visible = rows_raw.max(1);
    let offset = clamp_offset(total, visible, requested);
    let end = total - offset;
    Range {
        total_lines: total,
        visible_lines: visible,
        start_line: end.saturating_sub(visible),
        end_line: end,
        effective_offset: offset,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_wrap_and_newline_cases() {
        for (text, cols, expected) in [
            (b"".as_slice(), 8, 1),
            (b"ABC", 8, 1),
            (b"ABCDE", 4, 2),
            (b"AB\nCD", 8, 2),
            (b"AB\n", 8, 1),
        ] {
            assert_eq!(count_lines(text, cols, 437), expected);
        }
    }
    #[test]
    fn original_visible_range_clamps_offsets() {
        let text = b"L1\nL2\nL3\nL4\nL5";
        let tail = visible_range(text, 80, 3, 0, 437);
        assert!(
            tail.total_lines == 5
                && tail.start_line == 2
                && tail.end_line == 5
                && tail.tail_visible()
        );
        let up = visible_range(text, 80, 3, 1, 437);
        assert!(up.start_line == 1 && up.end_line == 4 && !up.tail_visible());
        let clamped = visible_range(text, 80, 3, 99, 437);
        assert!(clamped.start_line == 0 && clamped.end_line == 3 && clamped.effective_offset == 2);
    }
    #[test]
    fn original_utf8_columns_ignore_font_codepage_and_wait_on_incomplete_tails() {
        let text = "1234567ü─".as_bytes();
        assert_eq!(count_lines(text, 8, 437), 2);
        assert_eq!(count_lines(text, 8, 65001), 2);
        assert_eq!(count_lines(&text[..8], 8, 437), 1);
        assert_eq!(count_lines(&text[..9], 8, 1252), 1);
    }
}
