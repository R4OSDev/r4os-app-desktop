//! Current Desktop colors and chrome metrics, with one Rust source for both
//! window geometry and Shell painters. Optional loaded fonts remain separate.
pub const DESKTOP_BG: u32 = 0x008080;
pub const TASKBAR: u32 = 0xc0c0c0;
pub const TASKBAR_HOVER: u32 = 0xd8d8d8;
pub const TASKBAR_PRESSED: u32 = 0xa8a8a8;
pub const TASKBAR_DARK: u32 = 0x808080;
pub const TASKBAR_LIGHT: u32 = 0xffffff;
pub const TEXT: u32 = 0x000000;
pub const TITLE_ACTIVE: u32 = 0x000080;
pub const TITLE_LIGHT: u32 = 0x1084d0;
pub const TITLE_INACTIVE: u32 = 0x808080;
pub const TITLE_TEXT: u32 = 0xffffff;
pub const WINDOW_BG: u32 = 0xd8d8d8;
pub const CLIENT_BG: u32 = 0xffffff;
pub const SELECT_BG: u32 = 0x000080;
pub const SHADOW: u32 = 0x404040;
pub const TASKBAR_H: i32 = 32;
pub const START_W: i32 = 82;
pub const START_H: i32 = 24;
pub const MENU_W: i32 = 236;
pub const MENU_H: i32 = 416;
pub const MENU_TOP_PAD: i32 = 34;
pub const MENU_GROUP_H: i32 = 18;
pub const MENU_ITEM_H: i32 = 24;
pub const TITLE_H: i32 = 20;
pub const BUTTON: i32 = 16;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LabelColors {
    pub foreground: u32,
    pub background: u32,
}
pub fn desktop_icon_label(
    active: bool,
    desktop_background: u32,
    configured_text: u32,
) -> LabelColors {
    if active {
        LabelColors {
            foreground: 0xffffff,
            background: SELECT_BG,
        }
    } else {
        LabelColors {
            foreground: configured_text & 0xffffff,
            background: desktop_background & 0xffffff,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_normal_icon_uses_configured_text_and_desktop_background() {
        assert_eq!(
            desktop_icon_label(false, 0x123456, 0xabcdef),
            LabelColors {
                foreground: 0xabcdef,
                background: 0x123456
            }
        );
    }
    #[test]
    fn original_active_icon_retains_white_on_blue() {
        assert_eq!(
            desktop_icon_label(true, 0x123456, 0x000000),
            LabelColors {
                foreground: 0xffffff,
                background: 0x000080
            }
        );
    }
}
