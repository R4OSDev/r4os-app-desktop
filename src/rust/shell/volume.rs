//! Existing volume geometry and perceptual gain policy, independent of the
//! AUDSVC connection, output selection and painting owners.
use crate::render::geometry::Rect;
pub const ICON_W: i32 = 30;
pub const ICON_GAP: i32 = 4;
pub const POPUP_W: i32 = 396;
pub const POPUP_H: i32 = 354;
pub const POPUP_GAP: i32 = 6;
pub const OUTPUT_ROWS: usize = 4;
pub const PERCENT_STEP: u8 = 5;
#[derive(Clone)]
pub struct OutputRow<'a> {
    pub id: [u8; 64],
    pub name: [u8; 64],
    pub detail: &'a [u8],
    pub available: bool,
    pub active: bool,
    pub preferred: bool,
}
impl Default for OutputRow<'_> {
    fn default() -> Self {
        Self {
            id: [0; 64],
            name: [0; 64],
            detail: &[],
            available: false,
            active: false,
            preferred: false,
        }
    }
}
pub struct OutputPage<'a> {
    pub supported: bool,
    pub automatic: bool,
    pub pending: bool,
    pub save_error: bool,
    pub failed: bool,
    pub loading: bool,
    pub request_error: bool,
    pub index: u32,
    pub total: u32,
    pub count: u32,
    pub active_name: [u8; 64],
    pub reason: &'a [u8],
    pub rows: [OutputRow<'a>; OUTPUT_ROWS],
}
impl Default for OutputPage<'_> {
    fn default() -> Self {
        Self {
            supported: false,
            automatic: true,
            pending: false,
            save_error: false,
            failed: false,
            loading: false,
            request_error: false,
            index: 0,
            total: 0,
            count: 0,
            active_name: [0; 64],
            reason: b"Output selection unavailable",
            rows: core::array::from_fn(|_| OutputRow::default()),
        }
    }
}
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub installed: bool,
    pub reachable: bool,
    pub muted: bool,
    pub percent: u8,
    pub popup_open: bool,
    pub outputs: Option<&'a OutputPage<'a>>,
    pub output_serial: u64,
}
impl Default for View<'_> {
    fn default() -> Self {
        Self {
            installed: false,
            reachable: false,
            muted: false,
            percent: 100,
            popup_open: false,
            outputs: None,
            output_serial: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconState {
    Hidden,
    Unavailable,
    Muted,
    Quiet,
    Normal,
    Loud,
}
pub fn icon_state(view: View<'_>) -> IconState {
    if !view.installed {
        IconState::Hidden
    } else if !view.reachable {
        IconState::Unavailable
    } else if view.muted || view.percent == 0 {
        IconState::Muted
    } else if view.percent <= 25 {
        IconState::Quiet
    } else if view.percent <= 70 {
        IconState::Normal
    } else {
        IconState::Loud
    }
}
pub fn icon_rect(right: i32, top: i32, height: i32) -> Rect {
    Rect::new(right - ICON_W, top, ICON_W, height)
}
pub fn popup_rect(icon: Rect, work: Rect) -> Rect {
    Rect::new(
        icon.x + (icon.w - POPUP_W) / 2,
        icon.y - POPUP_H - POPUP_GAP,
        POPUP_W,
        POPUP_H,
    )
    .clamp_inside(work)
}
pub fn track_rect(popup: Rect) -> Rect {
    Rect::new(popup.x + 20, popup.y + 45, popup.w - 40, 14)
}
pub fn mute_rect(popup: Rect) -> Rect {
    Rect::new(popup.x + 16, popup.y + 78, 92, 24)
}
pub fn output_rect(popup: Rect, index: usize) -> Rect {
    Rect::new(
        popup.x + 12,
        popup.y + 150 + index as i32 * 34,
        popup.w - 24,
        32,
    )
}
pub fn output_auto_rect(popup: Rect) -> Rect {
    Rect::new(popup.x + 12, popup.y + 300, 116, 26)
}
pub fn output_page_rect(popup: Rect, next: bool) -> Rect {
    Rect::new(
        popup.right() - if next { 40 } else { 74 },
        popup.y + 300,
        28,
        26,
    )
}
pub fn percent_at_x(track: Rect, x: i32) -> u8 {
    if track.w <= 1 || x <= track.x {
        return 0;
    }
    if x >= track.right() - 1 {
        return 100;
    }
    ((i64::from(x - track.x) * 100 + i64::from((track.w - 1) / 2)) / i64::from(track.w - 1)) as u8
}
pub fn gain_for_percent(percent: u8) -> u32 {
    let p = u64::from(percent.min(100));
    ((p * p * 0x10000 + 5000) / 10000) as u32
}
pub fn percent_for_gain(gain: u32) -> u8 {
    let mut best = 0;
    let mut distance = u32::MAX;
    for candidate in 0..=100u8 {
        let next = gain_for_percent(candidate).abs_diff(gain);
        if next < distance {
            distance = next;
            best = candidate;
        }
    }
    best
}
pub fn stepped(percent: u8, direction: i32) -> u8 {
    if direction > 0 {
        percent.saturating_add(PERCENT_STEP).min(100)
    } else if direction < 0 {
        percent.saturating_sub(PERCENT_STEP)
    } else {
        percent
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_perceptual_curve_monotonic_stable_ui_roundtrips() {
        let mut previous = 0;
        for percent in 0..=100 {
            let gain = gain_for_percent(percent);
            assert!(gain >= previous);
            assert!(percent_for_gain(gain).abs_diff(percent) <= 1);
            previous = gain;
        }
        assert_eq!(gain_for_percent(0), 0);
        assert_eq!(gain_for_percent(100), 0x10000);
    }
    #[test]
    fn original_popup_work_area_and_stable_controls() {
        let work = Rect::new(0, 0, 640, 440);
        let popup = popup_rect(icon_rect(540, 440, 28), work);
        assert!(
            popup.x >= work.x
                && popup.right() <= work.right()
                && popup.y >= work.y
                && popup.bottom() <= work.bottom()
        );
        let track = track_rect(popup);
        let mute = mute_rect(popup);
        assert!(track.contains(track.x, track.y) && mute.contains(mute.x, mute.y));
        assert_eq!(percent_at_x(track, track.x), 0);
        assert_eq!(percent_at_x(track, track.right() - 1), 100);
        assert!(output_rect(popup, OUTPUT_ROWS - 1).bottom() < output_auto_rect(popup).y);
        assert!(output_page_rect(popup, true).right() < popup.right());
        assert!(output_auto_rect(popup).bottom() < popup.bottom());
    }
    #[test]
    fn original_icon_states_unavailable_mute_and_audible_ranges() {
        assert_eq!(icon_state(View::default()), IconState::Hidden);
        assert_eq!(
            icon_state(View {
                installed: true,
                ..View::default()
            }),
            IconState::Unavailable
        );
        let v = View {
            installed: true,
            reachable: true,
            ..View::default()
        };
        assert_eq!(icon_state(View { muted: true, ..v }), IconState::Muted);
        assert_eq!(icon_state(View { percent: 20, ..v }), IconState::Quiet);
        assert_eq!(icon_state(View { percent: 50, ..v }), IconState::Normal);
        assert_eq!(icon_state(View { percent: 90, ..v }), IconState::Loud);
    }
}
