//! Host-launch data stays opaque except for the existing explicit BASIC
//! first-frame trace. Parsing/encoding belongs to the shared SDK owner.
use crate::shell::{configuration::bytes_z, launch::Launch};
use core::fmt::Write;
use r4os_sdk::{
    abi as a,
    subsystems::{Launch as Envelope, LaunchOption, encode_launch},
};
pub fn request(value: &a::ProgramHostLaunchRequest, now: u64) -> Option<Launch> {
    let policy = match value.policy {
        0 => a::LaunchPolicy::auto,
        1 => a::LaunchPolicy::console,
        2 => a::LaunchPolicy::gui,
        _ => return None,
    };
    let mut encoded = [0; 128];
    let args =
        trace(bytes_z(&value.args), now, &mut encoded).unwrap_or_else(|| bytes_z(&value.args));
    Launch::program(bytes_z(&value.path), args, policy).ok()
}
pub fn take(
    hosted: &super::runtime::State<'_>,
    session: &mut r4os_sdk::gui::Session<'_, '_>,
    now: u64,
) -> Option<Launch> {
    for window in &hosted.model.windows.windows()[1..] {
        if window.instance_id() != 0 {
            if let Some(value) = super::gui::Host::new(session).take_launch(window.instance_id()) {
                return request(&value, now);
            }
        }
    }
    None
}
fn trace<'o>(encoded: &[u8], now: u64, out: &'o mut [u8]) -> Option<&'o [u8]> {
    let request = Envelope::parse(encoded).ok()?;
    if request.option_count().ok()? != 3 {
        return None;
    }
    let id = request.option(b"T").ok()??;
    let mode = request.option(b"M").ok()??;
    let phases = request.option(b"P").ok()??;
    if id.len() != 16 || !mode.eq_ignore_ascii_case(b"G") {
        return None;
    }
    let start = u64::from_str_radix(core::str::from_utf8(id).ok()?, 16).ok()?;
    let elapsed = now.checked_sub(start)?;
    let mut fields = phases.split(|&v| v == b',');
    let probe = fields.next()?;
    let resolve = fields.next()?;
    fields.next()?;
    if fields.next().is_some() {
        return None;
    }
    core::str::from_utf8(probe).ok()?.parse::<u64>().ok()?;
    core::str::from_utf8(resolve).ok()?.parse::<u64>().ok()?;
    let mut bytes = [0; 64];
    struct Text<'a> {
        bytes: &'a mut [u8],
        len: usize,
    }
    impl Write for Text<'_> {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            if self.len + text.len() > self.bytes.len() {
                return Err(core::fmt::Error);
            }
            self.bytes[self.len..self.len + text.len()].copy_from_slice(text.as_bytes());
            self.len += text.len();
            Ok(())
        }
    }
    let mut text = Text {
        bytes: &mut bytes,
        len: 0,
    };
    write!(
        &mut text,
        "{},{},{}",
        core::str::from_utf8(probe).ok()?,
        core::str::from_utf8(resolve).ok()?,
        elapsed
    )
    .ok()?;
    let n = text.len;
    let options = [
        LaunchOption::new(b"T", id).ok()?,
        LaunchOption::new(b"M", mode).ok()?,
        LaunchOption::new(b"P", &bytes[..n]).ok()?,
    ];
    encode_launch(request.guest().ok()?, &options, out).ok()
}
