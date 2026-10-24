//! Exact Run editing over the SDK's original R4STD grapheme implementation.
//! The Shell storage retains the exclusive text-buffer loan. This owner
//! admits append/backspace only; navigation and selection remain unchanged
//! from the current Run dialog rather than acquiring generic widget policy.
use super::run_command::MAX_INPUT;
use crate::input::keys::{is_text_key, valid_text};
use r4os_sdk::{
    gui::{TextField, raw as key},
    storage::{Error, Result},
};
pub struct Editor<'b> {
    field: TextField<'b>,
    capacity: usize,
}

/// Presentation-only tail. The original scalar-cell width and R4STD cluster
/// boundaries clip the label without changing the editor or launch identity.
pub fn copy_tail_label<'o>(out: &'o mut [u8], value: &[u8], columns: usize) -> &'o [u8] {
    out.fill(0);
    if out.is_empty() || columns == 0 || core::str::from_utf8(value).is_err() {
        return &out[..0];
    }
    fn width(value: &[u8]) -> Option<i32> {
        let result = unsafe { key::R4SdkGui_textWidth(0, value.as_ptr(), value.len() as u64) };
        (result.status == 0 && result.flags & 2 == 0).then_some(result.bits as i32)
    }
    let width_limit = (columns.min(i32::MAX as usize / 8) * 8) as i32;
    let Some(full_width) = width(value) else {
        return &out[..0];
    };
    if value.len() < out.len() && full_width <= width_limit {
        out[..value.len()].copy_from_slice(value);
        return &out[..value.len()];
    }
    let dots = 2.min(columns.min(out.len() - 1));
    let tail_width = width_limit - (dots * 8) as i32;
    let mut start = 0;
    while start < value.len() {
        let Some(current_width) = width(&value[start..]) else {
            return &out[..0];
        };
        if value.len() - start <= out.len() - 1 - dots && current_width <= tail_width {
            break;
        }
        let Some(next) = r4os_sdk::bounded_text::next_boundary(value, start) else {
            return &out[..0];
        };
        if next <= start || next > value.len() {
            return &out[..0];
        }
        start = next;
    }
    out[..dots].fill(b'.');
    let count = value.len() - start;
    out[dots..dots + count].copy_from_slice(&value[start..]);
    &out[..dots + count]
}
impl<'b> Editor<'b> {
    pub fn new(buffer: &'b mut [u8]) -> Result<Self> {
        if buffer.is_empty() || buffer.len() > MAX_INPUT + 1 {
            return Err(Error::Length);
        }
        let capacity = buffer.len() - 1;
        Ok(Self {
            field: TextField::new(buffer)?,
            capacity,
        })
    }
    pub fn text(&self) -> Result<&[u8]> {
        self.field.value()
    }
    pub fn set(&mut self, text: &[u8]) -> Result<bool> {
        if text.len() > self.capacity || !valid_text(text) {
            return Ok(false);
        }
        self.field.set_exact(text)
    }
    pub fn append_text(&mut self, text: &[u8]) -> Result<bool> {
        if text.len() > self.capacity - self.field.value()?.len() || !valid_text(text) {
            return Ok(false);
        }
        if text.is_empty() {
            return Ok(true);
        }
        self.field.paste(text)
    }
    pub fn append(&mut self, codepoint: u32) -> Result<bool> {
        if !is_text_key(codepoint) {
            return Ok(false);
        }
        self.field.handle_codepoint(codepoint)
    }
    pub fn backspace(&mut self) -> Result<bool> {
        self.field
            .handle_codepoint(key::R4SdkGuiKey_backspace as u32)
    }
    pub fn clear(&mut self) -> Result<bool> {
        self.field.set_exact(&[])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" {
        fn sdk86_text_boundaries() -> i32;
    }
    #[test]
    fn current_shared_sdk_grapheme_boundaries_and_public_c_boundary() {
        use r4os_sdk::bounded_text::{next_boundary, previous_boundary};
        let text = "A\u{308}中🚀".as_bytes();
        for (offset, expected) in [(0, 3), (1, 3), (3, 6), (6, 10), (10, 10), (usize::MAX, 10)] {
            assert_eq!(next_boundary(text, offset), Some(expected));
        }
        for (offset, expected) in [
            (0, 0),
            (1, 0),
            (3, 0),
            (4, 3),
            (6, 3),
            (10, 6),
            (usize::MAX, 6),
        ] {
            assert_eq!(previous_boundary(text, offset), Some(expected));
        }
        assert_eq!(next_boundary(b"A\xcc", 0), None);
        assert_eq!(previous_boundary(b"A\xcc", 2), None);
        assert_eq!(next_boundary(b"", usize::MAX), Some(0));
        assert_eq!(unsafe { sdk86_text_boundaries() }, 0);
    }
    #[test]
    fn original_append_atomic_capacity_invalid_input_and_cluster_backspace() {
        let mut buffer = [0; 6];
        let mut field = Editor::new(&mut buffer).unwrap();
        assert!(field.set(b"ABC").unwrap());
        assert_eq!(field.text().unwrap(), b"ABC");
        assert!(field.append(u32::from(b'D')).unwrap());
        assert!(field.append(u32::from(b'E')).unwrap());
        assert!(!field.append(u32::from(b'F')).unwrap());
        assert_eq!(field.text().unwrap(), b"ABCDE");
        assert!(field.backspace().unwrap());
        assert_eq!(field.text().unwrap(), b"ABCD");
        assert!(!field.append_text("ä".as_bytes()).unwrap());
        assert!(!field.set("abcdä".as_bytes()).unwrap());
        assert!(!field.set(b"a\xff").unwrap());
        assert_eq!(field.text().unwrap(), b"ABCD");
        assert!(field.set("A\u{308}".as_bytes()).unwrap());
        assert!(field.backspace().unwrap());
        assert_eq!(field.text().unwrap(), b"");
    }
    #[test]
    fn original_controls_full_scalars_and_quoted_unicode_path_cluster_deletion() {
        let mut buffer = [0; 9];
        let mut field = Editor::new(&mut buffer).unwrap();
        for value in [u32::from(b'\n'), 0x1b] {
            assert!(!field.append(value).unwrap());
        }
        assert!(field.append(u32::from(b'X')).unwrap());
        assert_eq!(field.text().unwrap(), b"X");
        assert!(field.append(0x1f680).unwrap());
        assert!(field.append(0x4e2d).unwrap());
        assert_eq!(field.text().unwrap(), "X🚀中".as_bytes());
        for value in [0xd800, 0x110000] {
            assert!(!field.append(value).unwrap());
        }
        assert!(!field.append_text("ä".as_bytes()).unwrap());
        assert!(!field.set(b"a\0b").unwrap());
        assert_eq!(field.text().unwrap(), "X🚀中".as_bytes());
        drop(field);
        assert_eq!(buffer[8], 0);
        let mut buffer = [0; MAX_INPUT + 1];
        let mut field = Editor::new(&mut buffer).unwrap();
        assert!(field.set("C:\\ä\\中\\A\u{308}.R4X".as_bytes()).unwrap());
        for _ in 0..5 {
            assert!(field.backspace().unwrap());
        }
        assert_eq!(field.text().unwrap(), "C:\\ä\\中\\".as_bytes());
        let mut label = [0u8; 8];
        assert_eq!(
            copy_tail_label(&mut label, "prefixA\u{308}中".as_bytes(), 4),
            "..中".as_bytes()
        );
    }
}
