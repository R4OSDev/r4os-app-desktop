//! Current84 text admission. Function-key routing is kept separate from
//! Unicode scalar text, including the reserved C1 key range.
use r4os_sdk::gui::raw as key;
pub fn is_text_key(key: u32) -> bool {
    (0x20..=0x10ffff).contains(&key)
        && !(0x7f..=0x9f).contains(&key)
        && !(0xd800..=0xdfff).contains(&key)
}
pub fn valid_text(value: &[u8]) -> bool {
    core::str::from_utf8(value).is_ok_and(|text| text.chars().all(|c| is_text_key(c as u32)))
}
pub fn is_app_key(value: u32) -> bool {
    is_text_key(value)
        || [
            key::R4SdkGuiKey_ctrl_a as u32,
            key::R4SdkGuiKey_ctrl_c as u32,
            key::R4SdkGuiKey_ctrl_v as u32,
            key::R4SdkGuiKey_ctrl_x as u32,
            key::R4SdkGuiKey_backspace as u32,
            key::R4SdkGuiKey_tab as u32,
            key::R4SdkGuiKey_enter as u32,
            u32::from(b'\n'),
            key::R4SdkGuiKey_escape as u32,
            key::R4SdkGuiKey_delete as u32,
            key::R4SdkGuiKey_up as u32,
            key::R4SdkGuiKey_down as u32,
            key::R4SdkGuiKey_f3 as u32,
            key::R4SdkGuiKey_shift_tab as u32,
            key::R4SdkGuiKey_left as u32,
            key::R4SdkGuiKey_right as u32,
            key::R4SdkGuiKey_home as u32,
            key::R4SdkGuiKey_end as u32,
            key::R4SdkGuiKey_page_up as u32,
            key::R4SdkGuiKey_page_down as u32,
            key::R4SdkGuiKey_f10 as u32,
        ]
        .contains(&value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_text_scalar_policy() {
        for key in [0xe4, 0x4e2d, 0x1f680] {
            assert!(is_text_key(key));
        }
        for key in [0, 0x80, 0xd800, 0x110000] {
            assert!(!is_text_key(key));
        }
        assert!(valid_text("ä中A\u{308}".as_bytes()));
        assert!(!valid_text(b"a\0b") && !valid_text(b"a\xffb"));
        for value in [
            key::R4SdkGuiKey_tab as u32,
            key::R4SdkGuiKey_shift_tab as u32,
            key::R4SdkGuiKey_enter as u32,
            key::R4SdkGuiKey_escape as u32,
            key::R4SdkGuiKey_ctrl_v as u32,
            key::R4SdkGuiKey_left as u32,
            key::R4SdkGuiKey_f10 as u32,
            0xe4,
            0x4e2d,
            0x1f680,
        ] {
            assert!(is_app_key(value));
        }
        for value in [0, 0xd800, 0x110000] {
            assert!(!is_app_key(value));
        }
    }
}
