//! Current reusable Desktop modal semantics. Painting, actions and retained
//! title/text storage are independent owners.
use crate::input::event::Target;
pub const TITLE_CAPACITY: usize = 64;
pub const TEXT_CAPACITY: usize = 160;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    #[default]
    Info,
    Warning,
    Error,
    Question,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Buttons {
    #[default]
    Ok,
    OkCancel,
    YesNo,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Result {
    #[default]
    None,
    Ok,
    Cancel,
    Yes,
    No,
}
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub kind: Kind,
    pub buttons: Buttons,
    pub title: &'a [u8],
    pub text: &'a [u8],
    pub focus: Target,
}
pub fn default_focus(buttons: Buttons) -> Target {
    if buttons == Buttons::YesNo {
        Target::MessageNo
    } else {
        Target::MessageOk
    }
}
pub fn cancel_result(buttons: Buttons) -> Result {
    match buttons {
        Buttons::Ok => Result::Ok,
        Buttons::OkCancel => Result::Cancel,
        Buttons::YesNo => Result::No,
    }
}
pub fn cancel_target(buttons: Buttons) -> Target {
    if buttons == Buttons::Ok {
        Target::MessageOk
    } else {
        Target::MessageNo
    }
}
pub fn target_result(buttons: Buttons, target: Target) -> Result {
    match (buttons, target) {
        (Buttons::Ok, Target::MessageOk | Target::MessageBackdrop) => Result::Ok,
        (Buttons::OkCancel, Target::MessageOk) => Result::Ok,
        (Buttons::OkCancel, Target::MessageNo | Target::MessageBackdrop) => Result::Cancel,
        (Buttons::YesNo, Target::MessageYes) => Result::Yes,
        (Buttons::YesNo, Target::MessageNo | Target::MessageBackdrop) => Result::No,
        _ => Result::None,
    }
}
pub fn window_info(out: &mut [u8], rect: crate::render::geometry::Rect) -> &[u8] {
    use core::fmt::Write;
    struct Text<'a> {
        bytes: &'a mut [u8],
        len: usize,
    }
    impl Write for Text<'_> {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            let count = text
                .len()
                .min(self.bytes.len().saturating_sub(self.len + 1));
            self.bytes[self.len..self.len + count].copy_from_slice(&text.as_bytes()[..count]);
            self.len += count;
            Ok(())
        }
    }
    out.fill(0);
    let mut text = Text { bytes: out, len: 0 };
    let _ = write!(
        &mut text,
        "Window size: {} x {}\nPosition: {}, {}",
        rect.w, rect.h, rect.x, rect.y
    );
    let len = text.len;
    &out[..len]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_default_focus_follows_button_set() {
        assert_eq!(default_focus(Buttons::Ok), Target::MessageOk);
        assert_eq!(default_focus(Buttons::OkCancel), Target::MessageOk);
        assert_eq!(default_focus(Buttons::YesNo), Target::MessageNo);
    }
    #[test]
    fn original_target_results_reusable_modal_targets() {
        assert_eq!(target_result(Buttons::Ok, Target::MessageOk), Result::Ok);
        assert_eq!(
            target_result(Buttons::OkCancel, Target::MessageNo),
            Result::Cancel
        );
        assert_eq!(
            target_result(Buttons::YesNo, Target::MessageYes),
            Result::Yes
        );
        assert_eq!(
            target_result(Buttons::YesNo, Target::MessageBackdrop),
            Result::No
        );
        assert_eq!(
            target_result(Buttons::YesNo, Target::MessageOk),
            Result::None
        );
    }
    #[test]
    fn original_cancel_target_button_semantics() {
        assert_eq!(cancel_target(Buttons::Ok), Target::MessageOk);
        assert_eq!(cancel_target(Buttons::OkCancel), Target::MessageNo);
        assert_eq!(cancel_target(Buttons::YesNo), Target::MessageNo);
    }
    #[test]
    fn original_cancel_result_button_semantics() {
        assert_eq!(cancel_result(Buttons::Ok), Result::Ok);
        assert_eq!(cancel_result(Buttons::OkCancel), Result::Cancel);
        assert_eq!(cancel_result(Buttons::YesNo), Result::No);
    }
}
