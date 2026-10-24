//! The existing five default desktop links. R4STD owns R4LNK parsing, byte
//! limits, target resolution and canonical serialization.
use r4os_library_helpers::{
    Result,
    standard::{Shortcut, Standard},
};
use r4os_sdk::abi::LaunchPolicy;
pub const DIRECTORY: &[u8] = b"C:\\R4OS\\DESKTOP";
pub const DISPLAY_DIRECTORY: &[u8] = b"C:\\R4OS\\DESKTOP\\";
pub const MAX_LINK_PATH: usize = 159;
pub const MAX_LINK_BYTES: usize = 768;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultLink {
    Terminal,
    Notepad,
    Synth,
    Devices,
    Computer,
}
pub const DEFAULTS: [DefaultLink; 5] = [
    DefaultLink::Terminal,
    DefaultLink::Notepad,
    DefaultLink::Synth,
    DefaultLink::Devices,
    DefaultLink::Computer,
];
impl DefaultLink {
    pub fn file(self) -> &'static [u8] {
        match self {
            Self::Terminal => b"TERMINAL.LNK",
            Self::Notepad => b"NOTEPAD.LNK",
            Self::Synth => b"R4SYNTH.LNK",
            Self::Devices => b"DEVICES.LNK",
            Self::Computer => b"COMPUTER.LNK",
        }
    }
    pub fn title(self) -> &'static [u8] {
        match self {
            Self::Terminal => b"Terminal",
            Self::Notepad => b"Notepad",
            Self::Synth => b"R4Synth",
            Self::Devices => b"Devices",
            Self::Computer => b"Computer",
        }
    }
    pub fn target(self) -> &'static [u8] {
        match self {
            Self::Terminal => b"C:\\R4OS\\SOFTWARE\\TERMINAL\\TERMINAL.R4X",
            Self::Notepad => b"C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X",
            Self::Synth => b"C:\\R4OS\\SOFTWARE\\TERMINAL\\SYNTH.R4X",
            Self::Devices => b"C:\\R4OS\\SOFTWARE\\DESKTOP\\DEVMGR.R4X",
            Self::Computer => b"C:\\R4OS\\SOFTWARE\\DESKTOP\\EXPLORER.R4X",
        }
    }
    pub fn args(self) -> &'static [u8] {
        if self == Self::Synth {
            b"C:\\TEMP\\TADA.WAV"
        } else {
            b""
        }
    }
    pub fn policy(self) -> LaunchPolicy {
        match self {
            Self::Terminal | Self::Synth => LaunchPolicy::console,
            _ => LaunchPolicy::gui,
        }
    }
    pub fn icon(self) -> &'static [u8] {
        match self {
            Self::Terminal => b"C:\\R4OS\\Media\\Icons\\Terminal.ico",
            Self::Notepad => b"C:\\R4OS\\Media\\Icons\\Notepad.ico",
            Self::Synth => b"C:\\R4OS\\Media\\Icons\\Synth.ico",
            Self::Devices => b"C:\\R4OS\\Media\\Icons\\Config.ico",
            Self::Computer => b"C:\\R4OS\\Media\\Icons\\Computer.ico",
        }
    }
    pub fn write<'out>(self, standard: &Standard<'_>, out: &'out mut [u8]) -> Result<&'out [u8]> {
        let mut link = standard.shortcut(self.target())?;
        link.set_title(self.title())?;
        link.set_args(self.args())?;
        link.set_policy(self.policy())?;
        link.set_icon(self.icon())?;
        link.write(out)
    }
    pub fn matches(self, link: &Shortcut<'_>) -> Result<bool> {
        let value = link.resolve()?;
        Ok(value.title == self.title()
            && value.target == self.target()
            && value.args == self.args()
            && value.policy == self.policy().0
            && value.icon == self.icon())
    }
}
pub fn default_path<'out>(out: &'out mut [u8], file: &[u8]) -> Option<&'out [u8]> {
    if out.is_empty() {
        return None;
    }
    out.fill(0);
    let count = DIRECTORY.len().checked_add(1)?.checked_add(file.len())?;
    if count >= out.len() {
        return None;
    }
    out[..DIRECTORY.len()].copy_from_slice(DIRECTORY);
    out[DIRECTORY.len()] = b'\\';
    out[DIRECTORY.len() + 1..count].copy_from_slice(file);
    Some(&out[..count])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_five_canonical_r4lnk_files() {
        crate::standard_fixture::with(|standard| {
            assert_eq!(DEFAULTS.len(), 5);
            for spec in DEFAULTS {
                let mut buffer = [0; MAX_LINK_BYTES];
                let bytes = spec.write(standard, &mut buffer).unwrap();
                for text in [b"SCHEMA=R4LNK".as_slice(), b"TARGET="] {
                    assert!(bytes.windows(text.len()).any(|v| v == text));
                }
                let parsed = standard.parse_shortcut(bytes).unwrap();
                assert!(spec.matches(&parsed).unwrap());
                assert_eq!(parsed.target().unwrap(), spec.target());
                assert_eq!(parsed.title().unwrap(), spec.title());
                assert_eq!(parsed.args().unwrap(), spec.args());
                assert_eq!(parsed.icon().unwrap(), spec.icon());
            }
            assert!(standard.shortcut(b"").is_err());
            assert!(
                standard
                    .parse_shortcut(b"R4S_FORMAT=1\nSCHEMA=DESKTOP\nTARGET=C:\\A.R4X\n")
                    .is_err()
            );
            let mut link = standard.shortcut(b"C:\\A.R4X").unwrap();
            assert!(link.set_title(&[b'x'; 64]).is_err());
            link.set_workdir(b"C:\\TEMP").unwrap();
            assert_eq!(link.resolve().unwrap().workdir, b"C:\\TEMP");
            assert!(link.write(&mut [0; 4]).is_err());
        });
    }
    #[test]
    fn original_default_paths_under_canonical_folder() {
        for spec in DEFAULTS {
            let mut out = [0; MAX_LINK_PATH + 1];
            let text = default_path(&mut out, spec.file()).unwrap();
            assert!(text.starts_with(DISPLAY_DIRECTORY));
            assert!(text.ends_with(b".LNK"));
        }
    }
}
