//! Read-only volume/output, tray tooltip and clock-menu painting.
use super::{
    configuration::bytes_z,
    style as s, taskbar_view as taskbar,
    tray::{Identity, Registry},
    volume::{self, View},
};
use crate::{
    input::event::Target,
    render::{
        geometry::{Rect, work_area},
        primitive_assets::Result,
        text::cache::Fonts,
        ui::Ui,
    },
};
use core::fmt::{self, Write};
pub fn volume_tooltip_rect(w: i32, h: i32, clock: bool) -> Rect {
    let icon = taskbar::volume_rect(w, h, clock);
    Rect::new(icon.x - 74, icon.y - 34, 176, 28).clamp_inside(work_area(w, h, s::TASKBAR_H))
}
pub fn volume_popup_rect(w: i32, h: i32, clock: bool) -> Rect {
    volume::popup_rect(
        taskbar::volume_rect(w, h, clock),
        work_area(w, h, s::TASKBAR_H),
    )
}
pub fn time_menu_rect(w: i32, h: i32) -> Rect {
    let clock = taskbar::clock_rect(w, h);
    Rect::new(clock.right() - 160, h - s::TASKBAR_H - 54, 160, 52).clamp_inside(work_area(
        w,
        h,
        s::TASKBAR_H,
    ))
}
fn tooltip(ui: &mut Ui<'_, '_, impl Fonts>, r: Rect) -> Result<()> {
    ui.rect(r.x + 2, r.y + 2, r.w, r.h, s::SHADOW)?;
    ui.fill(r, s::TEXT)?;
    ui.fill(r.inset(1, 1), 0xffffe1)
}
pub fn tray_tooltip(
    ui: &mut Ui<'_, '_, impl Fonts>,
    registry: &Registry,
    identity: Identity,
    w: i32,
    h: i32,
) -> Result<()> {
    let (Some(e), Some(r)) = (
        registry.find(identity),
        registry.tooltip_rect(identity, w, h, s::TASKBAR_H),
    ) else {
        return Ok(());
    };
    tooltip(ui, r)?;
    ui.text_slice(
        r4os_contract::gui_font_builtin_id,
        r.x + 6,
        r.y + 7,
        e.tooltip(),
        s::TEXT,
        0xffffe1,
        r.inset(4, 3),
    )
}
pub fn volume_tooltip(
    ui: &mut Ui<'_, '_, impl Fonts>,
    w: i32,
    h: i32,
    clock: bool,
    view: View<'_>,
) -> Result<()> {
    if !view.installed {
        return Ok(());
    }
    let r = volume_tooltip_rect(w, h, clock);
    tooltip(ui, r)?;
    let label: &[u8] = if !view.reachable {
        b"Audio service unavailable".as_slice()
    } else if view.muted {
        b"System volume: muted"
    } else {
        b"System volume:"
    };
    ui.text(r.x + 6, r.y + 10, label, s::TEXT, 0xffffe1)?;
    if view.reachable && !view.muted {
        let mut number = Label::new();
        write!(number, "{}", view.percent).ok();
        ui.text(r.x + 118, r.y + 10, number.bytes(), s::TEXT, 0xffffe1)?;
        ui.text(r.x + 143, r.y + 10, b"%", s::TEXT, 0xffffe1)?;
    }
    Ok(())
}
pub fn time_menu(
    ui: &mut Ui<'_, '_, impl Fonts>,
    w: i32,
    h: i32,
    hover: Target,
    pressed: Target,
) -> Result<()> {
    let r = time_menu_rect(w, h);
    ui.fill(r, s::WINDOW_BG)?;
    ui.bevel(r, false)?;
    for (dy, label, target) in [
        (4, b"Clock".as_slice(), Target::TimeMenuClock),
        (26, b"Settings", Target::TimeMenuSettings),
    ] {
        let hot = hover == target || pressed == target;
        let (fg, bg) = if hot {
            (s::TITLE_TEXT, s::SELECT_BG)
        } else {
            (s::TEXT, s::WINDOW_BG)
        };
        let y = r.y + dy;
        ui.rect(r.x + 3, y, 154, 22, bg)?;
        if target == Target::TimeMenuClock {
            for (dx, dy, w, h, c) in [
                (12, 5, 10, 10, fg),
                (14, 7, 6, 6, bg),
                (17, 8, 1, 4, fg),
                (17, 11, 4, 1, fg),
            ] {
                ui.rect(r.x + dx, y + dy, w, h, c)?;
            }
        } else {
            for dy in [6, 10, 14] {
                ui.rect(r.x + 12, y + dy, 10, 2, fg)?;
            }
        }
        ui.text(r.x + 30, y + 7, label, fg, bg)?;
    }
    Ok(())
}
pub fn volume_popup(
    ui: &mut Ui<'_, '_, impl Fonts>,
    w: i32,
    h: i32,
    clock: bool,
    view: View<'_>,
    hover: Target,
    pressed: Target,
) -> Result<()> {
    if !view.installed || !view.popup_open {
        return Ok(());
    }
    let r = volume_popup_rect(w, h, clock);
    let track = volume::track_rect(r);
    let mute = volume::mute_rect(r);
    ui.rect(r.x + 3, r.y + 3, r.w, r.h, s::SHADOW)?;
    ui.fill(r, s::WINDOW_BG)?;
    ui.bevel(r, false)?;
    ui.text(r.x + 14, r.y + 13, b"System volume", s::TEXT, s::WINDOW_BG)?;
    let mut number = Label::new();
    write!(number, "{}", view.percent).ok();
    ui.text(
        r.right() - 42,
        r.y + 13,
        number.bytes(),
        s::TEXT,
        s::WINDOW_BG,
    )?;
    ui.text(r.right() - 16, r.y + 13, b"%", s::TEXT, s::WINDOW_BG)?;
    let enabled = view.reachable;
    let track_bg = if enabled { s::CLIENT_BG } else { s::TASKBAR };
    ui.rect(track.x, track.y + 5, track.w, 4, s::TASKBAR_DARK)?;
    ui.rect(track.x + 1, track.y + 6, 0.max(track.w - 2), 2, track_bg)?;
    let fill = if track.w <= 1 {
        0
    } else {
        ((track.w - 1) as i64 * view.percent as i64 / 100) as i32
    };
    if enabled && fill > 0 {
        ui.rect(track.x + 1, track.y + 6, fill, 2, s::TITLE_ACTIVE)?;
    }
    let thumb = Rect::new(track.x + fill - 4, track.y, 9, track.h);
    ui.fill(thumb, if enabled { s::WINDOW_BG } else { s::TASKBAR })?;
    ui.bevel(thumb, pressed == Target::VolumePopupSlider)?;
    if hover == Target::VolumePopupSlider && enabled {
        ui.outline(
            Rect::new(track.x - 2, track.y - 2, track.w + 4, track.h + 4),
            s::TEXT,
        )?;
    }
    let down = pressed == Target::VolumePopupMute || view.muted;
    let hot = hover == Target::VolumePopupMute && !down;
    let bg = if !enabled {
        s::TASKBAR
    } else if down {
        s::TASKBAR_PRESSED
    } else if hot {
        s::TASKBAR_HOVER
    } else {
        s::WINDOW_BG
    };
    ui.bevel(mute, down)?;
    ui.fill(mute.inset(2, 2), bg)?;
    ui.rect(mute.x + 10, mute.y + 7, 10, 10, s::CLIENT_BG)?;
    ui.bevel(Rect::new(mute.x + 9, mute.y + 6, 12, 12), true)?;
    if view.muted {
        for (dx, dy) in [(11, 11), (13, 13), (15, 9)] {
            ui.rect(mute.x + dx, mute.y + dy, 3, 2, s::TEXT)?;
        }
    }
    ui.text(
        mute.x + 29,
        mute.y + 9,
        b"Mute",
        if enabled { s::TEXT } else { s::TASKBAR_DARK },
        bg,
    )?;
    if !enabled {
        ui.text(
            r.x + 119,
            mute.y + 9,
            b"Unavailable",
            s::TASKBAR_DARK,
            s::WINDOW_BG,
        )?;
    }
    let Some(page) = view.outputs else {
        ui.text(
            r.x + 14,
            r.y + 129,
            b"Output selection unavailable",
            s::TASKBAR_DARK,
            s::WINDOW_BG,
        )?;
        return Ok(());
    };
    let font = r4os_contract::gui_font_builtin_id;
    ui.text(r.x + 14, r.y + 110, b"Output:", s::TEXT, s::WINDOW_BG)?;
    ui.text_slice(
        font,
        r.x + 77,
        r.y + 110,
        if page.active_name[0] == 0 {
            b"None available"
        } else {
            bytes_z(&page.active_name)
        },
        s::TEXT,
        s::WINDOW_BG,
        Rect::new(r.x + 77, r.y + 108, r.w - 91, 18),
    )?;
    ui.text_slice(
        font,
        r.x + 14,
        r.y + 129,
        if page.failed {
            b"Switch not confirmed; list refreshed"
        } else {
            page.reason
        },
        s::TASKBAR_DARK,
        s::WINDOW_BG,
        Rect::new(r.x + 14, r.y + 127, r.w - 28, 18),
    )?;
    let targets = [
        Target::VolumeOutput0,
        Target::VolumeOutput1,
        Target::VolumeOutput2,
        Target::VolumeOutput3,
    ];
    for (i, row) in page.rows.iter().take(page.count as usize).enumerate() {
        let area = volume::output_rect(r, i);
        let target = targets[i];
        let down = row.active || pressed == target;
        let bg = if down {
            s::TASKBAR_PRESSED
        } else if hover == target && row.available {
            s::TASKBAR_HOVER
        } else {
            s::CLIENT_BG
        };
        ui.fill(area, bg)?;
        ui.bevel(area, down)?;
        ui.text_slice(
            font,
            area.x + 8,
            area.y + 5,
            bytes_z(&row.name),
            if row.available {
                s::TEXT
            } else {
                s::TASKBAR_DARK
            },
            bg,
            Rect::new(area.x + 8, area.y + 3, area.w - 78, 16),
        )?;
        ui.text_slice(
            font,
            area.x + 8,
            area.y + 19,
            row.detail,
            s::TASKBAR_DARK,
            bg,
            area.inset(4, 2),
        )?;
        if row.active || row.preferred {
            ui.text(
                area.right() - 60,
                area.y + 5,
                if row.active { b"Active" } else { b"Saved" },
                s::TEXT,
                bg,
            )?;
        }
    }
    output_button(
        ui,
        volume::output_auto_rect(r),
        b"Automatic",
        page.automatic,
        enabled && page.supported,
    )?;
    output_button(
        ui,
        volume::output_page_rect(r, false),
        b"<",
        pressed == Target::VolumeOutputPrevious,
        page.index > 0,
    )?;
    output_button(
        ui,
        volume::output_page_rect(r, true),
        b">",
        pressed == Target::VolumeOutputNext,
        page.index.saturating_add(page.count) < page.total,
    )?;
    let mut page_text = Label::new();
    let valid = write!(
        page_text,
        "{}-{} / {}",
        if page.count == 0 {
            0
        } else {
            page.index.saturating_add(1)
        },
        page.index.saturating_add(page.count),
        page.total
    )
    .is_ok();
    ui.text_slice(
        font,
        r.x + 145,
        r.y + 309,
        if valid { page_text.bytes() } else { b"" },
        s::TEXT,
        s::WINDOW_BG,
        r.inset(4, 4),
    )?;
    if page.save_error || page.pending {
        ui.text(
            r.x + 14,
            r.y + 335,
            if page.save_error {
                b"Could not save selection"
            } else {
                b"Saving..."
            },
            s::TASKBAR_DARK,
            s::WINDOW_BG,
        )?;
    }
    Ok(())
}
fn output_button(
    ui: &mut Ui<'_, '_, impl Fonts>,
    r: Rect,
    label: &[u8],
    pressed: bool,
    enabled: bool,
) -> Result<()> {
    let bg = if pressed && enabled {
        s::TASKBAR_PRESSED
    } else {
        s::WINDOW_BG
    };
    ui.fill(r, bg)?;
    ui.bevel(r, pressed && enabled)?;
    ui.text(
        r.x + 9,
        r.y + 9,
        label,
        if enabled { s::TEXT } else { s::TASKBAR_DARK },
        bg,
    )
}
struct Label {
    bytes: [u8; 32],
    len: usize,
}
impl Label {
    fn new() -> Self {
        Self {
            bytes: [0; 32],
            len: 0,
        }
    }
    fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
impl Write for Label {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if value.len() > self.bytes.len() - self.len {
            return Err(fmt::Error);
        }
        self.bytes[self.len..self.len + value.len()].copy_from_slice(value.as_bytes());
        self.len += value.len();
        Ok(())
    }
}
