//! Taskbar pixels read copied shell/window state. Custom icon loading is a
//! separate asset owner and cannot reach service or process state here.
use super::{
    quick_launch::{self, Bar, Item},
    style as s,
    tray::{self, Identity, Registry},
    volume::{self, IconState, View},
};
#[cfg(test)]
#[path = "../../../Tests/RustNative/taskbar_pixels.rs"]
mod tests;
use crate::{
    input::event::Target,
    render::{
        geometry::Rect,
        pixels::PixelBytes,
        primitive_assets::Result,
        primitive_image::{Format, Picture, View as ImageView},
        text::cache::Fonts,
        ui::Ui,
    },
    windows::{controller::Controller, window::Window},
};
pub const CLOCK_W: i32 = 88;
pub const CLOCK_RIGHT_MARGIN: i32 = 8;
pub const LAYOUT_W: i32 = 34;
pub const LAYOUT_GAP: i32 = 6;
/// The refresh owner and compositor use the same global taskbar coverage.
pub fn visible(windows: &[Window], active: usize, width: i32, height: i32) -> bool {
    let bar = Rect::new(0, height - s::TASKBAR_H, width, s::TASKBAR_H);
    !bar.is_empty() && !crate::windows::window::fullscreen_covers(windows, active, bar)
}
pub trait Icons {
    fn quick<F: Fonts>(
        &mut self,
        ui: &mut Ui<'_, '_, F>,
        path: &[u8],
        x: i32,
        y: i32,
    ) -> Result<bool>;
}
pub struct State<'a> {
    pub windows: &'a [Window],
    pub quick: Option<&'a Bar>,
    pub active: usize,
    pub clock: Option<&'a [u8]>,
    pub keyboard_layout: Option<&'a [u8]>,
    pub volume: View<'a>,
    pub tray: Option<&'a Registry>,
    pub tray_hover: Identity,
    pub tray_pressed: Identity,
    pub hover: Target,
    pub pressed: Target,
    pub recording: u32,
}
pub fn clock_rect(w: i32, h: i32) -> Rect {
    Rect::new(
        w - CLOCK_RIGHT_MARGIN - CLOCK_W,
        h - s::TASKBAR_H + 4,
        CLOCK_W,
        s::START_H,
    )
}
pub fn volume_rect(w: i32, h: i32, clock: bool) -> Rect {
    let right = if clock {
        clock_rect(w, h).x - volume::ICON_GAP
    } else {
        w - CLOCK_RIGHT_MARGIN
    };
    volume::icon_rect(right, h - s::TASKBAR_H + 4, s::START_H)
}
pub fn layout_rect(w: i32, h: i32, clock: bool, volume: bool) -> Rect {
    let right = if volume {
        volume_rect(w, h, clock).x - LAYOUT_GAP
    } else if clock {
        clock_rect(w, h).x - LAYOUT_GAP
    } else {
        w - CLOCK_RIGHT_MARGIN
    };
    Rect::new(right - LAYOUT_W, h - s::TASKBAR_H + 4, LAYOUT_W, s::START_H)
}
fn focus<F: Fonts>(ui: &mut Ui<'_, '_, F>, rect: Rect) -> Result<()> {
    ui.outline(rect, s::TEXT)
}
fn start<F: Fonts>(ui: &mut Ui<'_, '_, F>, x: i32, y: i32, pressed: bool) -> Result<()> {
    let off = i32::from(pressed);
    for (dx, dy, color) in [
        (0, 0, 0xd04040),
        (6, 0, 0x40a050),
        (0, 6, 0x4080d0),
        (6, 6, 0xf0c040),
    ] {
        ui.rect(x + dx + off, y + dy + off, 5, 5, color)?;
    }
    ui.rect(x + 12 + off, y + 1 + off, 1, 11, s::SHADOW)
}
fn status<F: Fonts>(
    ui: &mut Ui<'_, '_, F>,
    x: i32,
    y: i32,
    active: bool,
    minimized: bool,
) -> Result<()> {
    let color = if active {
        s::TITLE_ACTIVE
    } else if minimized {
        s::TASKBAR_DARK
    } else {
        0x4080d0
    };
    ui.rect(x, y, 9, 9, s::TASKBAR_LIGHT)?;
    ui.rect(x + 1, y + 1, 7, 7, color)?;
    ui.rect(x + 8, y + 1, 1, 8, s::SHADOW)?;
    ui.rect(x + 1, y + 8, 8, 1, s::SHADOW)
}
fn clipped_title(title: &[u8], width: i32) -> &[u8] {
    if width < 8 {
        return &[];
    }
    let count = (width / 8).min(63) as usize;
    let Ok(text) = core::str::from_utf8(title) else {
        return &[];
    };
    &title[..text
        .char_indices()
        .nth(count)
        .map_or(title.len(), |(i, _)| i)]
}
pub fn paint<F: Fonts>(
    ui: &mut Ui<'_, '_, F>,
    icons: &mut impl Icons,
    w: i32,
    h: i32,
    state: State<'_>,
) -> Result<()> {
    let top = h - s::TASKBAR_H;
    ui.rect(0, top, w, s::TASKBAR_H, s::TASKBAR)?;
    let recording_color = if state.recording == 2 {
        0xb06000
    } else {
        0xc00000
    };
    ui.rect(
        0,
        top,
        w,
        if state.recording != 0 { 3 } else { 1 },
        if state.recording != 0 {
            recording_color
        } else {
            s::TASKBAR_LIGHT
        },
    )?;
    ui.bevel(
        Rect::new(2, top + 4, s::START_W, s::START_H),
        state.pressed == Target::StartButton,
    )?;
    if state.hover == Target::StartButton && state.pressed != Target::StartButton {
        focus(ui, Rect::new(5, top + 7, s::START_W - 6, s::START_H - 6))?;
    }
    start(ui, 10, top + 10, state.pressed == Target::StartButton)?;
    ui.text(28, top + 11, b"R4OS", s::TEXT, s::TASKBAR)?;
    let quick_count = state.quick.map_or(0, Bar::count);
    if let Some(bar) = state.quick {
        quick_bar(ui, icons, h, bar, state.hover, state.pressed)?;
    }
    let left = quick_launch::taskbar_window_start_x(quick_count);
    let right = state.tray.map_or_else(
        || layout_rect(w, h, state.clock.is_some(), state.volume.installed).x - tray::WINDOW_GAP,
        |r| r.window_right,
    );
    let count = state.windows.iter().filter(|w| w.visible()).count();
    let mut ordinal = 0;
    for (i, window) in state.windows.iter().enumerate() {
        if !window.visible() {
            continue;
        }
        let rect = tray::window_button_rect(left, right, count, ordinal, top + 4, s::START_H);
        ordinal += 1;
        if rect.is_empty() {
            continue;
        }
        let target = Controller::taskbar_target(i);
        let active = i == state.active && !window.minimized();
        let hovered = state.hover == target && state.pressed != target;
        let pressed = active || state.pressed == target;
        let bg = background(hovered, pressed);
        ui.bevel(rect, pressed)?;
        ui.rect(
            rect.x + 2,
            rect.y + 2,
            (rect.w - 4).max(0),
            (rect.h - 4).max(0),
            bg,
        )?;
        if rect.w >= 20 {
            status(ui, rect.x + 8, top + 11, active, window.minimized())?;
        }
        if hovered && rect.w >= 8 {
            focus(ui, rect.inset(3, 3))?;
        }
        if rect.w >= 30 {
            ui.text(
                rect.x + 22,
                top + 11,
                clipped_title(window.title().to_bytes(), (rect.w - 34).max(0)),
                s::TEXT,
                bg,
            )?;
        }
    }
    if let Some(registry) = state.tray {
        for entry in registry.entries() {
            if !entry.used || !entry.layout_visible {
                continue;
            }
            let id = entry.identity();
            let pressed = state.tray_pressed.same(id);
            let hovered = state.tray_hover.same(id) && !pressed;
            let bg = background(hovered, pressed);
            if pressed || hovered {
                ui.bevel(entry.rect, pressed)?;
                ui.fill(entry.rect.inset(2, 2), bg)?;
            }
            let target = Rect::new(
                entry.rect.x + 4,
                entry.rect.y + 4,
                r4os_contract::tray_icon_width as i32,
                r4os_contract::tray_icon_height as i32,
            );
            ui.painter.picture(Picture {
                view: ImageView::new(
                    Format::Argb,
                    target.w as u32,
                    target.h as u32,
                    target.w as usize * 4,
                    PixelBytes::from_words(&entry.icon).bytes(),
                ),
                viewport: target,
                clip: target,
                source_x: 0,
                source_y: 0,
                guest_w: target.w as u32,
                guest_h: target.h as u32,
            })?;
            if entry.flags & r4os_contract::tray_item_flag_attention != 0 {
                ui.rect(
                    entry.rect.x + 3,
                    entry.rect.bottom() - 3,
                    entry.rect.w - 6,
                    1,
                    0xe08020,
                )?;
            }
        }
    }
    if state.volume.installed {
        let rect = volume_rect(w, h, state.clock.is_some());
        let pressed = state.pressed == Target::TaskbarVolume;
        let hovered = state.hover == Target::TaskbarVolume && !pressed;
        let bg = background(hovered, pressed);
        if pressed || hovered {
            ui.bevel(rect, pressed)?;
            ui.fill(rect.inset(2, 2), bg)?;
        }
        volume_glyph(
            ui,
            rect.x + 5,
            rect.y + 5,
            volume::icon_state(state.volume),
            bg,
        )?;
        if hovered {
            focus(ui, rect.inset(3, 3))?;
        }
    }
    if let Some(value) = state.keyboard_layout {
        let rect = layout_rect(w, h, state.clock.is_some(), state.volume.installed);
        let pressed = state.pressed == Target::TaskbarKeyboardLayout;
        let hovered = state.hover == Target::TaskbarKeyboardLayout && !pressed;
        let bg = background(hovered, pressed);
        ui.bevel(rect, pressed)?;
        ui.fill(rect.inset(2, 2), bg)?;
        if hovered {
            focus(ui, rect.inset(3, 3))?;
        }
        ui.text(rect.x + (rect.w - 16) / 2, top + 11, value, s::TEXT, bg)?;
    }
    if let Some(value) = state.clock {
        let rect = clock_rect(w, h);
        let pressed = state.pressed == Target::TaskbarClock;
        let hovered = state.hover == Target::TaskbarClock && !pressed;
        let bg = background(hovered, pressed);
        ui.bevel(rect, true)?;
        ui.fill(rect.inset(2, 2), bg)?;
        if hovered {
            focus(ui, rect.inset(3, 3))?;
        }
        ui.text(
            rect.x + 10,
            top + 11,
            if state.recording == 1 {
                b"REC"
            } else if state.recording == 2 {
                b"Saving"
            } else {
                value
            },
            if state.recording != 0 {
                recording_color
            } else {
                s::TEXT
            },
            bg,
        )?;
    }
    Ok(())
}
fn background(hovered: bool, pressed: bool) -> u32 {
    if pressed {
        s::TASKBAR_PRESSED
    } else if hovered {
        s::TASKBAR_HOVER
    } else {
        s::TASKBAR
    }
}
pub fn volume_glyph<F: Fonts>(
    ui: &mut Ui<'_, '_, F>,
    x: i32,
    y: i32,
    state: IconState,
    bg: u32,
) -> Result<()> {
    let color = if state == IconState::Unavailable {
        s::TASKBAR_DARK
    } else {
        s::TEXT
    };
    ui.rect(x, y + 6, 4, 7, color)?;
    ui.rect(x + 4, y + 4, 2, 11, color)?;
    ui.rect(x + 6, y + 2, 2, 15, color)?;
    match state {
        IconState::Normal | IconState::Loud => {
            ui.rect(x + 10, y + 6, 1, 7, color)?;
            ui.rect(x + 12, y + 4, 1, 11, color)?;
            if state == IconState::Loud {
                ui.rect(x + 14, y + 2, 1, 15, color)?;
                ui.rect(x + 16, y + 5, 1, 9, color)?;
            }
        }
        IconState::Quiet => ui.rect(x + 10, y + 7, 1, 5, color)?,
        IconState::Muted | IconState::Unavailable => {
            let mark = if state == IconState::Unavailable {
                0xa00000
            } else {
                color
            };
            for off in 0..7 {
                ui.rect(x + 10 + off, y + 5 + off, 1, 1, mark)?;
                ui.rect(x + 16 - off, y + 5 + off, 1, 1, mark)?;
            }
        }
        IconState::Hidden => ui.rect(x, y, 1, 1, bg)?,
    }
    Ok(())
}
fn quick_bar<F: Fonts>(
    ui: &mut Ui<'_, '_, F>,
    icons: &mut impl Icons,
    h: i32,
    bar: &Bar,
    hover: Target,
    pressed: Target,
) -> Result<()> {
    if bar.count() == 0 {
        return Ok(());
    }
    let rect = quick_launch::bar_rect(h, bar.count());
    let sep = quick_launch::separator_x(bar.count());
    for x in [rect.x, sep] {
        ui.rect(x, rect.y + 3, 1, rect.h - 6, s::TASKBAR_DARK)?;
        ui.rect(x + 1, rect.y + 3, 1, rect.h - 6, s::TASKBAR_LIGHT)?;
    }
    for (i, item) in bar.items().iter().enumerate() {
        let target = bar.target(i);
        let rect = quick_launch::button_rect(h, i);
        let down = pressed == target;
        let hot = hover == target && !down;
        ui.bevel(rect, down)?;
        ui.fill(rect.inset(2, 2), background(hot, down))?;
        if hot {
            focus(ui, rect.inset(3, 3))?;
        }
        quick_glyph(ui, icons, rect, item, down)?;
    }
    Ok(())
}
fn quick_glyph<F: Fonts>(
    ui: &mut Ui<'_, '_, F>,
    icons: &mut impl Icons,
    button: Rect,
    item: &Item,
    pressed: bool,
) -> Result<()> {
    let size = i32::from(quick_launch::ICON_SIZE);
    let off = i32::from(pressed);
    let x = button.x + (button.w - size) / 2 + off;
    let y = button.y + (button.h - size) / 2 + off;
    if !item.icon().is_empty() && icons.quick(ui, item.icon(), x, y)? {
        return Ok(());
    }
    if item.kind == quick_launch::Kind::ShowDesktop {
        ui.rect(x + 1, y + 2, 14, 10, s::TASKBAR_LIGHT)?;
        ui.rect(x + 2, y + 3, 12, 8, s::TITLE_ACTIVE)?;
        ui.rect(x + 3, y + 4, 10, 1, 0x40a0e0)?;
        ui.rect(x + 3, y + 8, 7, 1, 0x40a0e0)?;
        ui.rect(x + 5, y + 12, 6, 1, s::SHADOW)?;
        ui.rect(x + 4, y + 13, 8, 2, s::TASKBAR_DARK)?;
    } else {
        ui.rect(x + 1, y + 2, 12, 9, s::TASKBAR_LIGHT)?;
        ui.rect(x + 2, y + 3, 10, 7, 0x008080)?;
        ui.rect(x + 4, y + 11, 6, 1, s::SHADOW)?;
        ui.rect(x + 3, y + 12, 8, 2, s::WINDOW_BG)?;
        ui.rect(x + 11, y + 6, 4, 7, s::TASKBAR_DARK)?;
        ui.rect(x + 12, y + 7, 2, 1, s::TASKBAR_LIGHT)?;
        ui.rect(x + 12, y + 10, 2, 1, s::TASKBAR_LIGHT)?;
    }
    Ok(())
}
