//! Exact current84 Run/Browse identity. This parser never clips labels,
//! normalizes paths or reinterprets UTF8 as a legacy byte codepage.
use crate::input::keys::valid_text;
use core::ffi::CStr;
pub const MAX_INPUT: usize = 127;
pub const MAX_PATH: usize = 127;
pub const MAX_ARGS: usize = 127;
#[derive(Clone, Debug)]
pub struct Command {
    path: [u8; MAX_PATH + 1],
    args: [u8; MAX_ARGS + 1],
}
impl Command {
    pub fn path(&self) -> &CStr {
        CStr::from_bytes_until_nul(&self.path).expect("terminated command path")
    }
    pub fn args(&self) -> &CStr {
        CStr::from_bytes_until_nul(&self.args).expect("terminated command args")
    }
}
pub fn parse(input: &[u8]) -> Option<Command> {
    if input.len() > MAX_INPUT || !valid_text(input) {
        return None;
    }
    let value = trim(input);
    if value.is_empty() {
        return None;
    }
    let (path, rest) = if value[0] == b'"' {
        let end = value[1..].iter().position(|&b| b == b'"')?;
        (&value[1..1 + end], &value[end + 2..])
    } else {
        let end = value
            .iter()
            .position(|&b| is_space(b))
            .unwrap_or(value.len());
        (&value[..end], &value[end..])
    };
    let args = trim(rest);
    if path.is_empty() || path.len() > MAX_PATH || args.len() > MAX_ARGS {
        return None;
    }
    let mut result = Command {
        path: [0; MAX_PATH + 1],
        args: [0; MAX_ARGS + 1],
    };
    result.path[..path.len()].copy_from_slice(path);
    result.args[..args.len()].copy_from_slice(args);
    Some(result)
}
pub fn browse_text<'a>(path: &[u8], out: &'a mut [u8]) -> Option<&'a [u8]> {
    if path.is_empty() || path.len() > MAX_PATH || !valid_text(path) || path.contains(&b'"') {
        return None;
    }
    let quoted = path.contains(&b' ');
    let count = path.len() + if quoted { 2 } else { 0 };
    if count > MAX_INPUT || count > out.len() {
        return None;
    }
    let offset = usize::from(quoted);
    if quoted {
        out[0] = b'"';
    }
    out[offset..offset + path.len()].copy_from_slice(path);
    if quoted {
        out[count - 1] = b'"';
    }
    Some(&out[..count])
}
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}
fn trim(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(|&b| is_space(b)) {
        value = &value[1..];
    }
    while value.last().is_some_and(|&b| is_space(b)) {
        value = &value[..value.len() - 1];
    }
    value
}
#[cfg(test)]
mod tests {
    // Original parser/Browse cases. Their original cluster-Backspace subcase
    // remains pending integration of the common SDK text editor and the
    // existing real RunInput source consumer; it is not claimed by this parser.
    use super::*;
    #[test]
    fn path_without_args() {
        let c = parse(b"C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X").unwrap();
        assert_eq!(
            c.path().to_bytes(),
            b"C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X"
        );
        assert_eq!(c.args().to_bytes(), b"");
    }
    #[test]
    fn path_with_args() {
        let c = parse(b"C:\\R4OS\\SOFTWARE\\TERMINAL\\SYNTH.R4X C:\\TEMP\\TADA.WAV").unwrap();
        assert_eq!(
            c.path().to_bytes(),
            b"C:\\R4OS\\SOFTWARE\\TERMINAL\\SYNTH.R4X"
        );
        assert_eq!(c.args().to_bytes(), b"C:\\TEMP\\TADA.WAV");
    }
    #[test]
    fn quoted_path_and_exact_unicode_browse() {
        let c =
            parse(b"\"C:\\R4OS\\SOFTWARE\\DESKTOP\\MY APP.R4X\" /v C:\\TEMP\\TADA.WAV").unwrap();
        assert_eq!(
            c.path().to_bytes(),
            b"C:\\R4OS\\SOFTWARE\\DESKTOP\\MY APP.R4X"
        );
        assert_eq!(c.args().to_bytes(), b"/v C:\\TEMP\\TADA.WAV");
        let path = "C:\\ä\\中\\A\u{308} demo.R4X".as_bytes();
        let mut text = [0; MAX_INPUT];
        let exact = parse(browse_text(path, &mut text).unwrap()).unwrap();
        assert_eq!(exact.path().to_bytes(), path);
        assert_eq!(exact.args().to_bytes(), b"");
    }
    #[test]
    fn refuse_invalid_unterminated_and_oversized_without_mutation() {
        for value in [
            &b"   "[..],
            &b"\"C:\\R4OS\\SOFTWARE\\DESKTOP\\BROKEN.R4X"[..],
            &b"C:\\bad\xff.R4X"[..],
            &b"C:\\ok.R4X\0different"[..],
            &[b'a'; MAX_INPUT + 1][..],
        ] {
            assert!(parse(value).is_none());
        }
        let mut input = [b'b'; MAX_INPUT];
        input[0] = b'a';
        input[1] = b' ';
        let mut out = [0x5a; MAX_INPUT];
        assert!(browse_text(&input, &mut out).is_none());
        assert_eq!(out, [0x5a; MAX_INPUT]);
    }
}
