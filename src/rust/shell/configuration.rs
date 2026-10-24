//! Current Desktop settings. R4STD retains entry parsing, value codecs and
//! output formatting; this owner chooses the existing Desktop fields/policy.
use r4os_library_helpers::{Result, raw as c, standard::Standard};
use r4os_sdk::path::{AbsoluteFilePath, FILE_PATH_MAX};
const FONT_MAX: usize = 63;
const FONT: &[u8] = b"C:\\R4OS\\FONTS\\TERMINAL8.R4F";
pub const BACKGROUND: u32 = 0x008080;
pub const ICON_TEXT: u32 = 0xffffff;
#[derive(Clone)]
pub struct Config {
    pub ui_font: [u8; FONT_MAX + 1],
    pub ui_size: u8,
    pub terminal_font: [u8; FONT_MAX + 1],
    pub terminal_size: u8,
    pub terminal_codepage: u16,
    pub background: u32,
    pub icon_text: u32,
    pub wallpaper: [u8; FILE_PATH_MAX + 1],
    pub taskbar_clock: bool,
    pub screen_off_seconds: u32,
}
fn copy_z(out: &mut [u8], value: &[u8]) {
    out.fill(0);
    if out.is_empty() {
        return;
    }
    let n = value.len().min(out.len() - 1);
    out[..n].copy_from_slice(&value[..n]);
}
pub fn bytes_z(value: &[u8]) -> &[u8] {
    &value[..value.iter().position(|&b| b == 0).unwrap_or(value.len())]
}
impl Default for Config {
    fn default() -> Self {
        let mut font = [0; FONT_MAX + 1];
        copy_z(&mut font, FONT);
        Self {
            ui_font: font,
            ui_size: 8,
            terminal_font: font,
            terminal_size: 8,
            terminal_codepage: 437,
            background: BACKGROUND,
            icon_text: ICON_TEXT,
            wallpaper: [0; FILE_PATH_MAX + 1],
            taskbar_clock: true,
            screen_off_seconds: 0,
        }
    }
}
fn parsed(value: c::R4LibrariesSlot) -> Option<u32> {
    (value.status == 0 && value.flags & 2 == 0 && value.flags & 1 != 0).then_some(value.bits as u32)
}
fn number(bytes: &[u8]) -> Option<u32> {
    parsed(unsafe { c::R4StdSettings_parseU32(0, bytes.as_ptr(), bytes.len() as u64) })
}
fn color(bytes: &[u8]) -> Option<u32> {
    parsed(unsafe { c::R4StdSettings_parseRgb24(0, bytes.as_ptr(), bytes.len() as u64) })
}
fn boolean(bytes: &[u8]) -> Option<bool> {
    parsed(unsafe { c::R4StdSettings_parseBool(0, bytes.as_ptr(), bytes.len() as u64) })
        .map(|n| n != 0)
}
fn eq(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}
fn font_size(value: &[u8]) -> Option<u8> {
    if eq(value, b"8") || eq(value, b"8X8") {
        Some(8)
    } else if eq(value, b"16") || eq(value, b"8X16") {
        Some(16)
    } else {
        None
    }
}
impl Config {
    pub fn load(&mut self, standard: &Standard<'_>, bytes: &[u8]) -> Result<bool> {
        let mut loaded = false;
        for entry in standard.entries(bytes)? {
            let (key, value) = entry?;
            loaded |= self.entry(key, value);
        }
        Ok(loaded)
    }
    fn entry(&mut self, key: &[u8], value: &[u8]) -> bool {
        if value.is_empty() {
            return false;
        }
        if eq(key, b"SCREEN_OFF_SECONDS") {
            let Some(seconds) = number(value) else {
                return false;
            };
            if seconds != 0 && !(30..=86400).contains(&seconds) {
                return false;
            }
            self.screen_off_seconds = seconds;
            return true;
        }
        if eq(key, b"UI_FONT") || eq(key, b"DESKTOP_FONT") {
            copy_z(&mut self.ui_font, value);
            return true;
        }
        if eq(key, b"UI_FONT_SIZE") || eq(key, b"DESKTOP_FONT_SIZE") {
            let Some(size) = font_size(value) else {
                return false;
            };
            self.ui_size = size;
            return true;
        }
        if eq(key, b"TERMINAL_FONT") {
            copy_z(&mut self.terminal_font, value);
            return true;
        }
        if eq(key, b"TERMINAL_FONT_SIZE") {
            let Some(size) = font_size(value) else {
                return false;
            };
            self.terminal_size = size;
            return true;
        }
        if eq(key, b"TERMINAL_CODEPAGE") {
            if !eq(value, b"437") && !eq(value, b"CP437") {
                return false;
            }
            self.terminal_codepage = 437;
            return true;
        }
        if eq(key, b"DESKTOP_BG") || eq(key, b"BACKGROUND") || eq(key, b"BACKGROUND_COLOR") {
            let Some(rgb) = color(value) else {
                return false;
            };
            self.background = rgb;
            return true;
        }
        if eq(key, b"DESKTOP_ICON_TEXT") {
            let Some(rgb) = color(value) else {
                return false;
            };
            self.icon_text = rgb;
            return true;
        }
        if eq(key, b"WALLPAPER") {
            if eq(value, b"NONE") {
                self.wallpaper.fill(0);
                return true;
            }
            if value.len() > FILE_PATH_MAX {
                return false;
            }
            let Ok(path) = AbsoluteFilePath::parse(value) else {
                return false;
            };
            copy_z(&mut self.wallpaper, path.as_path().as_bytes());
            return true;
        }
        if eq(key, b"WALLPAPER_MODE") {
            return eq(value, b"CENTER");
        }
        if eq(key, b"TASKBAR_CLOCK") || eq(key, b"CLOCK") {
            let Some(enabled) = boolean(value) else {
                return false;
            };
            self.taskbar_clock = enabled;
            return true;
        }
        false
    }
    pub fn write<'out>(&self, standard: &Standard<'_>, out: &'out mut [u8]) -> Result<&'out [u8]> {
        let count = {
            let mut writer = standard.writer(out)?;
            writer.header(b"DESKTOP")?;
            writer.rgb24_pair(b"DESKTOP_BG", self.background)?;
            writer.rgb24_pair(b"DESKTOP_ICON_TEXT", self.icon_text)?;
            writer.pair(
                b"WALLPAPER",
                if self.wallpaper[0] == 0 {
                    b"NONE"
                } else {
                    bytes_z(&self.wallpaper)
                },
            )?;
            writer.pair(b"WALLPAPER_MODE", b"CENTER")?;
            writer.bool_pair(b"TASKBAR_CLOCK", self.taskbar_clock)?;
            writer.u32_pair(b"SCREEN_OFF_SECONDS", self.screen_off_seconds)?;
            writer.pair(b"UI_FONT", bytes_z(&self.ui_font))?;
            writer.u32_pair(b"UI_FONT_SIZE", if self.ui_size == 16 { 16 } else { 8 })?;
            writer.pair(b"TERMINAL_FONT", bytes_z(&self.terminal_font))?;
            writer.u32_pair(
                b"TERMINAL_FONT_SIZE",
                if self.terminal_size == 16 { 16 } else { 8 },
            )?;
            writer.u32_pair(b"TERMINAL_CODEPAGE", 437)?;
            writer.bytes()?.len()
        };
        Ok(&out[..count])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_seven_configuration_groups_with_original_r4std_provider() {
        crate::standard_fixture::with(|standard| {
            let mut config = Config::default();
            assert!(config.load(&standard,b"# Desktop\r\nTERMINAL_FONT=C:\\R4OS\\FONTS\\TERMINAL16.R4F\r\nTERMINAL_FONT_SIZE=16\r\nTERMINAL_CODEPAGE=CP437\r\n").unwrap());
            assert_eq!(
                bytes_z(&config.terminal_font),
                b"C:\\R4OS\\FONTS\\TERMINAL16.R4F"
            );
            assert_eq!((config.terminal_size, config.terminal_codepage), (16, 437));
            let mut config = Config::default();
            assert!(config.load(&standard,b"DESKTOP_BG=#004080\r\nDESKTOP_ICON_TEXT=FFD700\r\nWALLPAPER=C:\\WALLPAPER.BMP\r\nWALLPAPER_MODE=CENTER\r\nTASKBAR_CLOCK=off\r\n").unwrap());
            assert_eq!((config.background, config.icon_text), (0x004080, 0xffd700));
            assert_eq!(bytes_z(&config.wallpaper), b"C:\\WALLPAPER.BMP");
            assert!(!config.taskbar_clock);
            let mut out = [0; 384];
            let written = Config::default().write(&standard, &mut out).unwrap();
            for value in [
                b"DESKTOP_BG=008080".as_slice(),
                b"DESKTOP_ICON_TEXT=FFFFFF",
                b"WALLPAPER=NONE",
                b"WALLPAPER_MODE=CENTER",
                b"TASKBAR_CLOCK=ON",
                b"SCREEN_OFF_SECONDS=0",
                b"UI_FONT=C:\\R4OS\\FONTS\\TERMINAL8.R4F",
                b"UI_FONT_SIZE=8",
                b"TERMINAL_FONT=C:\\R4OS\\FONTS\\TERMINAL8.R4F",
            ] {
                assert!(written.windows(value.len()).any(|b| b == value));
            }
            let mut missing = Config::default();
            assert!(missing.load(&standard, b"DESKTOP_BG=000080\r\n").unwrap());
            assert_eq!(missing.icon_text, ICON_TEXT);
            let mut invalid = Config::default();
            assert!(
                !invalid
                    .load(&standard, b"DESKTOP_ICON_TEXT=GGGGGG\r\n")
                    .unwrap()
            );
            assert_eq!(invalid.icon_text, ICON_TEXT);
            assert!(
                !invalid
                    .load(
                        &standard,
                        b"WALLPAPER=RELATIVE.BMP\r\nWALLPAPER_MODE=STRETCH\r\n"
                    )
                    .unwrap()
            );
            assert_eq!(bytes_z(&invalid.wallpaper), b"");
            let mut config = Config::default();
            assert!(
                config
                    .load(
                        &standard,
                        b"UI_FONT=C:\\R4OS\\FONTS\\TERMINAL16.R4F\r\nUI_FONT_SIZE=16\r\n"
                    )
                    .unwrap()
            );
            assert_eq!(bytes_z(&config.ui_font), b"C:\\R4OS\\FONTS\\TERMINAL16.R4F");
            assert_eq!(config.ui_size, 16);
            let mut config = Config::default();
            assert!(
                !config
                    .load(
                        &standard,
                        b"FONT=C:\\R4OS\\FONTS\\TERMINAL8.R4F\r\nFONT_SIZE=8\r\nCODEPAGE=437\r\n"
                    )
                    .unwrap()
            );
            assert_eq!(bytes_z(&config.terminal_font), FONT);
            assert_eq!((config.terminal_size, config.terminal_codepage), (8, 437));
            // The existing screen-power fixture also owns these parser bounds.
            for value in [b"1".as_slice(), b"29", b"86401", b"4294967296"] {
                assert!(!config.entry(b"SCREEN_OFF_SECONDS", value));
                assert_eq!(config.screen_off_seconds, 0);
            }
            for (value, seconds) in [(b"30".as_slice(), 30), (b"86400", 86400), (b"0", 0)] {
                assert!(config.entry(b"SCREEN_OFF_SECONDS", value));
                assert_eq!(config.screen_off_seconds, seconds);
            }
        });
    }
}
