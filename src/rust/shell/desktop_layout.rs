//! Current DESKTOPLAYOUT persistence, independent of icon loading or painting.
//! R4STD owns document admission, entries, numbers and canonical writing.
use super::configuration::bytes_z;
use r4os_library_helpers::{Result, raw as c, standard::Standard};
pub const CAPACITY: usize = 32;
pub const PATH_MAX: usize = 159;
pub const MAX_BYTES: usize = 4096;
pub const PREFIX: &[u8] = b"C:\\R4OS\\DESKTOP\\";
const SCHEMA: &[u8] = b"DESKTOPLAYOUT";
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}
#[derive(Clone, Copy)]
pub struct Entry {
    path: [u8; PATH_MAX + 1],
    pub position: Position,
}
impl Default for Entry {
    fn default() -> Self {
        Self {
            path: [0; PATH_MAX + 1],
            position: Position::default(),
        }
    }
}
impl Entry {
    pub fn path(&self) -> &[u8] {
        bytes_z(&self.path)
    }
}
pub struct Layout {
    entries: [Entry; CAPACITY],
    count: usize,
    pub truncated: bool,
    pub parse_errors: bool,
    pub duplicate_paths: bool,
    pub invalid_schema: bool,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            entries: [Entry::default(); CAPACITY],
            count: 0,
            truncated: false,
            parse_errors: false,
            duplicate_paths: false,
            invalid_schema: false,
        }
    }
}
#[derive(Clone, Copy, Default)]
struct Slot {
    entry: Entry,
    x: bool,
    y: bool,
    path: bool,
}
enum Field {
    Path,
    X,
    Y,
}
fn trim(bytes: &[u8]) -> &[u8] {
    // R4STD trims exactly these four bytes. Rust's general ASCII whitespace
    // helper would also admit leading VT/FF which the original rejects.
    let space = |b: &u8| b" \t\r\n".contains(b);
    let start = bytes.iter().position(|b| !space(b)).unwrap_or(bytes.len());
    let end = bytes[start..]
        .iter()
        .rposition(|b| !space(b))
        .map_or(start, |i| start + i + 1);
    &bytes[start..end]
}
fn number(value: &[u8]) -> Option<i32> {
    let v = unsafe { c::R4StdSettings_parseI32(0, value.as_ptr(), value.len() as u64) };
    (v.status == 0 && v.flags & 2 == 0 && v.flags & 1 != 0).then_some(v.bits as i32)
}
fn key(bytes: &[u8]) -> Option<(usize, Field)> {
    let text = trim(bytes);
    if text.len() < 5 || !text[..5].eq_ignore_ascii_case(b"ITEM.") {
        return None;
    }
    let rest = &text[5..];
    let split = rest.iter().position(|&b| b == b'.')?;
    let mut index = 0usize;
    if split == 0 {
        return None;
    }
    for &b in &rest[..split] {
        if !b.is_ascii_digit() {
            return None;
        }
        index = index.checked_mul(10)?.checked_add(usize::from(b - b'0'))?;
    }
    let field = &rest[split + 1..];
    Some((
        index,
        if field.eq_ignore_ascii_case(b"PATH") {
            Field::Path
        } else if field.eq_ignore_ascii_case(b"X") {
            Field::X
        } else if field.eq_ignore_ascii_case(b"Y") {
            Field::Y
        } else {
            return None;
        },
    ))
}
pub fn valid_path(value: &[u8]) -> bool {
    let text = trim(value);
    text.len() > PREFIX.len()
        && text.len() <= PATH_MAX
        && text[..PREFIX.len()].eq_ignore_ascii_case(PREFIX)
        && !text.iter().any(|&b| b < 0x20 || b == b'/' || b == b'=')
        && !text[PREFIX.len()..]
            .split(|&b| b == b'\\')
            .any(|part| part == b"..")
}
fn copy_path(out: &mut [u8; PATH_MAX + 1], bytes: &[u8]) {
    out.fill(0);
    let bytes = trim(bytes);
    let n = bytes.len().min(PATH_MAX);
    out[..n].copy_from_slice(&bytes[..n]);
}
impl Layout {
    pub fn entries(&self) -> &[Entry] {
        &self.entries[..self.count]
    }
    pub fn clear(&mut self) {
        *self = Self::default()
    }
    fn find(&self, path: &[u8]) -> Option<usize> {
        self.entries()
            .iter()
            .position(|e| e.path().eq_ignore_ascii_case(path))
    }
    pub fn position(&self, path: &[u8]) -> Option<Position> {
        Some(self.entries[self.find(path)?].position)
    }
    pub fn add(&mut self, path: &[u8], x: i32, y: i32) -> bool {
        if !valid_path(path) {
            self.parse_errors = true;
            return false;
        }
        if let Some(index) = self.find(path) {
            self.entries[index].position = Position { x, y };
            self.duplicate_paths = true;
            return true;
        }
        if self.count == CAPACITY {
            self.truncated = true;
            return false;
        }
        let entry = &mut self.entries[self.count];
        copy_path(&mut entry.path, path);
        entry.position = Position { x, y };
        self.count += 1;
        true
    }
    pub fn load(&mut self, standard: &Standard<'_>, bytes: &[u8]) -> Result<bool> {
        self.clear();
        let document = standard.settings(bytes)?;
        if !document.has_supported_format()?
            || !document
                .schema_name()?
                .is_some_and(|name| name.eq_ignore_ascii_case(SCHEMA))
        {
            self.invalid_schema = true;
            return Ok(false);
        }
        let mut slots = [Slot::default(); CAPACITY];
        for entry in standard.entries(bytes)? {
            let (name, value) = entry?;
            let Some((index, field)) = key(name) else {
                continue;
            };
            if index >= CAPACITY {
                self.truncated = true;
                continue;
            }
            let slot = &mut slots[index];
            match field {
                Field::Path => {
                    if !valid_path(value) {
                        self.parse_errors = true;
                        slot.path = false;
                        continue;
                    }
                    copy_path(&mut slot.entry.path, value);
                    slot.path = true;
                }
                Field::X => {
                    if let Some(x) = number(value) {
                        slot.entry.position.x = x;
                        slot.x = true
                    } else {
                        self.parse_errors = true;
                        slot.x = false
                    }
                }
                Field::Y => {
                    if let Some(y) = number(value) {
                        slot.entry.position.y = y;
                        slot.y = true
                    } else {
                        self.parse_errors = true;
                        slot.y = false
                    }
                }
            }
        }
        for slot in slots {
            if !slot.path && !slot.x && !slot.y {
                continue;
            }
            if !slot.path || !slot.x || !slot.y {
                self.parse_errors = true;
                continue;
            }
            self.add(
                slot.entry.path(),
                slot.entry.position.x,
                slot.entry.position.y,
            );
        }
        Ok(true)
    }
    pub fn write<'out>(&self, standard: &Standard<'_>, out: &'out mut [u8]) -> Result<&'out [u8]> {
        use core::fmt::Write;
        struct Key {
            bytes: [u8; 24],
            len: usize,
        }
        impl Write for Key {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                if s.len() > self.bytes.len() - self.len {
                    return Err(core::fmt::Error);
                }
                self.bytes[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
                self.len += s.len();
                Ok(())
            }
        }
        let count = {
            let mut writer = standard.writer(out)?;
            writer.header(SCHEMA)?;
            writer.u32_pair(b"ITEM_COUNT", self.count as u32)?;
            for (index, entry) in self.entries().iter().enumerate() {
                for field in ["PATH", "X", "Y"] {
                    let mut key = Key {
                        bytes: [0; 24],
                        len: 0,
                    };
                    write!(&mut key, "ITEM.{index}.{field}").expect("bounded32-item key");
                    let key = &key.bytes[..key.len];
                    match field {
                        "PATH" => writer.pair(key, entry.path())?,
                        "X" => writer.i32_pair(key, entry.position.x)?,
                        _ => writer.i32_pair(key, entry.position.y)?,
                    }
                }
            }
            writer.bytes()?.len()
        };
        Ok(&out[..count])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_canonical_positions() {
        crate::standard_fixture::with(|standard| {
            let mut layout = Layout::default();
            assert!(layout.load(standard,b"R4S_FORMAT=1\nSCHEMA=DESKTOPLAYOUT\nITEM_COUNT=2\nITEM.0.PATH=C:\\R4OS\\DESKTOP\\TERMINAL.LNK\nITEM.0.X=24\nITEM.0.Y=76\nITEM.1.PATH=C:\\R4OS\\DESKTOP\\TOOLS\nITEM.1.X=132\nITEM.1.Y=76\n").unwrap());
            assert_eq!(layout.entries().len(), 2);
            assert_eq!(
                layout.position(b"c:\\r4os\\desktop\\terminal.lnk"),
                Some(Position { x: 24, y: 76 })
            );
        });
    }
    #[test]
    fn original_invalid_schema_refused() {
        crate::standard_fixture::with(|standard| {
            let mut layout = Layout::default();
            assert!(!layout.load(standard,b"R4S_FORMAT=1\nSCHEMA=DESKTOP\nITEM.0.PATH=C:\\R4OS\\DESKTOP\\TERMINAL.LNK\nITEM.0.X=24\nITEM.0.Y=76\n").unwrap());
            assert!(layout.invalid_schema);
            assert!(layout.entries().is_empty());
        });
    }
    #[test]
    fn original_invalid_paths_and_numbers() {
        assert!(!valid_path(b"\x0bC:\\R4OS\\DESKTOP\\A.LNK"));
        assert!(!valid_path(b"\x0cC:\\R4OS\\DESKTOP\\A.LNK"));
        assert!(valid_path(b" \tC:\\R4OS\\DESKTOP\\A.LNK\r\n"));
        crate::standard_fixture::with(|standard| {
            let mut layout = Layout::default();
            assert!(layout.load(standard,b"R4S_FORMAT=1\nSCHEMA=DESKTOPLAYOUT\nITEM.0.PATH=C:\\OTHER\\ESCAPE.LNK\nITEM.0.X=24\nITEM.0.Y=76\nITEM.1.PATH=C:\\R4OS\\DESKTOP\\BROKEN.LNK\nITEM.1.X=abc\nITEM.1.Y=76\nITEM.2.PATH=C:\\R4OS\\DESKTOP\\..\\BAD.LNK\nITEM.2.X=24\nITEM.2.Y=152\n").unwrap());
            assert!(layout.parse_errors);
            assert!(layout.entries().is_empty());
        });
    }
    #[test]
    fn original_later_duplicate_path_wins() {
        crate::standard_fixture::with(|standard| {
            let mut layout = Layout::default();
            assert!(layout.load(standard,b"R4S_FORMAT=1\nSCHEMA=DESKTOPLAYOUT\nITEM.0.PATH=C:\\R4OS\\DESKTOP\\TERMINAL.LNK\nITEM.0.X=24\nITEM.0.Y=76\nITEM.1.PATH=C:\\R4OS\\DESKTOP\\TERMINAL.LNK\nITEM.1.X=132\nITEM.1.Y=152\n").unwrap());
            assert!(layout.duplicate_paths);
            assert_eq!(layout.entries().len(), 1);
            assert_eq!(
                layout.position(b"C:\\R4OS\\DESKTOP\\TERMINAL.LNK"),
                Some(Position { x: 132, y: 152 })
            );
        });
    }
    #[test]
    fn original_canonical_write_and_roundtrip() {
        crate::standard_fixture::with(|standard| {
            let mut layout = Layout::default();
            assert!(layout.add(b"C:\\R4OS\\DESKTOP\\TERMINAL.LNK", 24, 76));
            assert!(layout.add(b"C:\\R4OS\\DESKTOP\\TOOLS", 132, 76));
            let mut out = [0; 512];
            let bytes = layout.write(standard, &mut out).unwrap();
            assert!(bytes.starts_with(&[0xef, 0xbb, 0xbf]));
            for value in [
                b"SCHEMA=DESKTOPLAYOUT".as_slice(),
                b"ITEM.0.PATH=C:\\R4OS\\DESKTOP\\TERMINAL.LNK",
            ] {
                assert!(bytes.windows(value.len()).any(|v| v == value));
            }
            let mut parsed = Layout::default();
            assert!(parsed.load(standard, bytes).unwrap());
            assert_eq!(parsed.entries().len(), 2);
            assert!(parsed.position(b"C:\\R4OS\\DESKTOP\\TOOLS").is_some());
        });
    }
}
