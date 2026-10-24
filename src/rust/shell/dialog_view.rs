//! Read-only modal painting. Focus/activation, subprocesses and persistence
//! belong to controllers; a paint scope cannot mutate those owners.
use super::{
    message_box::{Buttons, Kind, View},
    style as s,
};
use crate::{
    input::event::Target,
    render::{
        geometry::Rect,
        primitive_assets::Result,
        text::{cache::Fonts, layout::decode},
        ui::Ui,
    },
};
#[cfg(test)]
#[path = "../../../Tests/RustNative/shell_pixels.rs"]
mod tests;
pub fn frame(ui: &mut Ui<'_, '_, impl Fonts>, rect: Rect, title: &[u8]) -> Result<()> {
    ui.fill(rect, s::WINDOW_BG)?;
    ui.bevel(rect, false)?;
    ui.rect(
        rect.x + 3,
        rect.y + 3,
        rect.w - 6,
        s::TITLE_H,
        s::TITLE_ACTIVE,
    )?;
    clipped(
        ui,
        rect.x + 10,
        rect.y + 9,
        rect.w - 20,
        title,
        s::TITLE_TEXT,
        s::TITLE_ACTIVE,
    )
}
#[derive(Clone, Copy)]
pub struct Input {
    pub focus: Target,
    pub hover: Target,
    pub pressed: Target,
}
pub fn button(
    ui: &mut Ui<'_, '_, impl Fonts>,
    rect: Rect,
    label: &[u8],
    target: Target,
    input: Input,
    default: bool,
    cancel: bool,
) -> Result<()> {
    if default {
        ui.outline(Rect::new(rect.x - 1, rect.y - 1, rect.w + 2, 28), s::TEXT)?;
    }
    let down = input.pressed == target;
    ui.bevel(rect, down)?;
    let off = i32::from(down);
    ui.text(
        rect.x + 4.max((rect.w - label.len() as i32 * 8) / 2) + off,
        rect.y + 9 + off,
        label,
        s::TEXT,
        s::TASKBAR,
    )?;
    if input.focus == target || (input.hover == target && !down) {
        ui.outline(Rect::new(rect.x + 4, rect.y + 4, rect.w - 8, 18), s::TEXT)?;
    }
    if cancel {
        ui.rect(
            rect.x + 8,
            rect.y + 21,
            0.max(rect.w - 16),
            1,
            s::TASKBAR_DARK,
        )?;
    }
    Ok(())
}
pub fn run(
    ui: &mut Ui<'_, '_, impl Fonts>,
    width: i32,
    height: i32,
    path: &[u8],
    input: Input,
) -> Result<()> {
    let (x, y) = ((width - 420) / 2, (height - 178) / 2);
    frame(ui, Rect::new(x, y, 420, 178), b"Run")?;
    ui.text(
        x + 24,
        y + 48,
        b"Program and arguments:",
        s::TEXT,
        s::WINDOW_BG,
    )?;
    ui.bevel(Rect::new(x + 24, y + 68, 372, 24), true)?;
    let mut label = [0; 128];
    let path = super::run_editor::copy_tail_label(&mut label, path, 44);
    ui.text(x + 32, y + 76, path, s::TEXT, s::CLIENT_BG)?;
    if input.focus == Target::RunInput {
        ui.outline(Rect::new(x + 26, y + 70, 368, 20), s::TEXT)?;
    }
    for (dx, w, label, target, default, cancel) in [
        (
            24,
            88,
            b"Browse".as_slice(),
            Target::RunBrowse,
            false,
            false,
        ),
        (202, 76, b"OK", Target::RunOk, true, false),
        (288, 86, b"Cancel", Target::RunCancel, false, true),
    ] {
        button(
            ui,
            Rect::new(x + dx, y + 126, w, 26),
            label,
            target,
            input,
            default,
            cancel,
        )?;
    }
    Ok(())
}
pub fn settings(
    ui: &mut Ui<'_, '_, impl Fonts>,
    width: i32,
    height: i32,
    config: &super::configuration::Config,
    input: Input,
) -> Result<()> {
    let (x, y) = ((width - 500) / 2, (height - 294) / 2);
    frame(ui, Rect::new(x, y, 500, 294), b"Settings")?;
    ui.text(
        x + 24,
        y + 46,
        b"Desktop background:",
        s::TEXT,
        s::WINDOW_BG,
    )?;
    ui.rect(x + 198, y + 42, 42, 20, config.background)?;
    ui.outline(Rect::new(x + 198, y + 42, 42, 20), s::TEXT)?;
    let mut rgb = [0; 6];
    for (i, b) in rgb.iter_mut().enumerate() {
        *b = b"0123456789ABCDEF"[((config.background >> (4 * (5 - i))) & 15) as usize];
    }
    ui.text(x + 252, y + 48, &rgb, s::TEXT, s::WINDOW_BG)?;
    ui.text(x + 24, y + 74, b"Taskbar clock:", s::TEXT, s::WINDOW_BG)?;
    ui.text(
        x + 198,
        y + 74,
        if config.taskbar_clock { b"On" } else { b"Off" },
        s::TEXT,
        s::WINDOW_BG,
    )?;
    let mut font = [0; 38];
    let mut terminal = [0; 38];
    let font = super::run_editor::copy_tail_label(
        &mut font,
        super::configuration::bytes_z(&config.ui_font),
        37,
    );
    let terminal = super::run_editor::copy_tail_label(
        &mut terminal,
        super::configuration::bytes_z(&config.terminal_font),
        37,
    );
    let mut ui_size = [0; 3];
    let mut terminal_size = [0; 3];
    let ui_size = decimal(config.ui_size, &mut ui_size);
    let terminal_size = decimal(config.terminal_size, &mut terminal_size);
    for (dy, label, value) in [
        (102, b"UI font:".as_slice(), font),
        (126, b"UI font size:", ui_size),
        (150, b"Terminal font:", terminal),
        (174, b"Terminal font size:", terminal_size),
    ] {
        ui.text(x + 24, y + dy, label, s::TEXT, s::WINDOW_BG)?;
        ui.text(x + 198, y + dy, value, s::TEXT, s::WINDOW_BG)?;
    }
    for (dy, label, value) in [
        (
            202,
            b"Start menu:".as_slice(),
            b"C:\\R4OS\\SOFTWARE\\DESKTOP\\MENU.R4S".as_slice(),
        ),
        (222, b"Desktop folder:", b"C:\\R4OS\\DESKTOP"),
    ] {
        ui.text(x + 24, y + dy, label, s::TEXT, s::WINDOW_BG)?;
        ui.text(x + 136, y + dy, value, s::TEXT, s::WINDOW_BG)?;
    }
    ui.text(
        x + 24,
        y + 246,
        b"Config and desktop folder are read at startup with safe defaults.",
        s::TEXT,
        s::WINDOW_BG,
    )?;
    button(
        ui,
        Rect::new(x + 162, y + 262, 80, 26),
        b"OK",
        Target::SettingsOk,
        input,
        true,
        false,
    )?;
    button(
        ui,
        Rect::new(x + 254, y + 262, 86, 26),
        b"Cancel",
        Target::SettingsCancel,
        input,
        false,
        true,
    )
}
fn decimal(mut n: u8, out: &mut [u8; 3]) -> &[u8] {
    let mut at = 3;
    loop {
        at -= 1;
        out[at] = b'0' + n % 10;
        n /= 10;
        if n == 0 {
            return &out[at..];
        }
    }
}
pub fn message(
    ui: &mut Ui<'_, '_, impl Fonts>,
    width: i32,
    height: i32,
    view: View,
    hover: Target,
    pressed: Target,
) -> Result<()> {
    let (x, y) = ((width - 380) / 2, (height - 172) / 2);
    frame(ui, Rect::new(x, y, 380, 172), view.title)?;
    ui.rect(x + 3, y + s::TITLE_H + 4, 374, 1, s::TASKBAR_DARK)?;
    ui.rect(x + 3, y + s::TITLE_H + 5, 374, 1, s::TASKBAR_LIGHT)?;
    icon(ui, x + 24, y + 50, view.kind)?;
    wrapped(ui, x + 76, y + 52, 276, view.text, 3, s::TEXT, s::WINDOW_BG)?;
    ui.rect(x + 16, y + 112, 348, 1, s::TASKBAR_DARK)?;
    ui.rect(x + 16, y + 113, 348, 1, s::TASKBAR_LIGHT)?;
    let input = Input {
        focus: view.focus,
        hover,
        pressed,
    };
    if view.buttons == Buttons::Ok {
        button(
            ui,
            Rect::new(x + 150, y + 126, 80, 26),
            b"OK",
            Target::MessageOk,
            input,
            true,
            true,
        )?;
    } else {
        let yes = view.buttons == Buttons::YesNo;
        button(
            ui,
            Rect::new(x + 106, y + 126, 76, 26),
            if yes { b"Yes" } else { b"OK" },
            if yes {
                Target::MessageYes
            } else {
                Target::MessageOk
            },
            input,
            !yes,
            false,
        )?;
        button(
            ui,
            Rect::new(x + 198, y + 126, 76, 26),
            if yes { b"No" } else { b"Cancel" },
            Target::MessageNo,
            input,
            yes,
            true,
        )?;
    }
    Ok(())
}
fn icon(ui: &mut Ui<'_, '_, impl Fonts>, x: i32, y: i32, kind: Kind) -> Result<()> {
    if kind == Kind::Warning {
        for row in 0..16 {
            let w = row * 2 + 2;
            let start = 16 - w / 2;
            ui.rect(x + start - 1, y + row * 2, w + 2, 2, s::TEXT)?;
            if w > 2 {
                ui.rect(x + start, y + row * 2, w, 2, 0xffff00)?;
            }
        }
        ui.rect(x + 15, y + 11, 3, 11, s::TEXT)?;
        ui.rect(x + 15, y + 25, 3, 3, s::TEXT)?;
        return Ok(());
    }
    let color = match kind {
        Kind::Error => 0xa00000,
        _ => 0x000080,
    };
    for (i, (dx, w)) in [
        (11, 10),
        (7, 18),
        (5, 22),
        (3, 26),
        (2, 28),
        (1, 30),
        (0, 32),
        (0, 32),
        (0, 32),
        (0, 32),
        (1, 30),
        (2, 28),
        (3, 26),
        (5, 22),
        (7, 18),
        (11, 10),
    ]
    .into_iter()
    .enumerate()
    {
        ui.rect(x + dx, y + i as i32 * 2, w, 2, color)?;
    }
    ui.rect(x + 8, y + 4, 8, 2, 0x70b8ff)?;
    match kind {
        Kind::Info => {
            ui.rect(x + 15, y + 7, 3, 3, s::TITLE_TEXT)?;
            ui.rect(x + 15, y + 13, 3, 11, s::TITLE_TEXT)?;
            ui.rect(x + 12, y + 24, 9, 3, s::TITLE_TEXT)?;
        }
        Kind::Error => {
            for i in 0..8 {
                ui.rect(x + 8 + i * 2, y + 8 + i * 2, 4, 4, s::TITLE_TEXT)?;
                ui.rect(x + 20 - i * 2, y + 8 + i * 2, 4, 4, s::TITLE_TEXT)?;
            }
        }
        _ => {
            for (dx, dy, w, h) in [
                (11, 7, 10, 3),
                (19, 10, 4, 6),
                (16, 16, 5, 3),
                (14, 19, 4, 5),
                (14, 26, 4, 3),
            ] {
                ui.rect(x + dx, y + dy, w, h, s::TITLE_TEXT)?;
            }
        }
    }
    Ok(())
}
pub fn clipped(
    ui: &mut Ui<'_, '_, impl Fonts>,
    x: i32,
    y: i32,
    width: i32,
    value: &[u8],
    fg: u32,
    bg: u32,
) -> Result<()> {
    // Same 63-byte/scalar-safe text admission as the existing modal painter.
    let mut end = 0;
    let mut count = 0;
    let max = (width / 8).max(0) as usize;
    while end < value.len() && count < max && value[end] != 0 {
        let (_, n) = decode(&value[end..]);
        if end + n > 63 {
            break;
        }
        end += n;
        count += 1;
    }
    if end != 0 {
        ui.text(x, y, &value[..end], fg, bg)?;
    }
    Ok(())
}
pub fn wrapped(
    ui: &mut Ui<'_, '_, impl Fonts>,
    x: i32,
    y: i32,
    width: i32,
    value: &[u8],
    lines: usize,
    fg: u32,
    bg: u32,
) -> Result<()> {
    let max = (width / 8).clamp(1, 63) as usize;
    let mut pos = 0;
    for line in 0..lines {
        while value.get(pos) == Some(&b' ') {
            pos += 1;
        }
        if pos >= value.len() {
            break;
        }
        let (mut end, mut count, mut space) = (pos, 0, None);
        while end < value.len() && ![0, b'\n', b'\r'].contains(&value[end]) && count < max {
            if value[end] == b' ' {
                space = Some(end);
            }
            let (_, n) = decode(&value[end..]);
            if end + n - pos > 63 {
                break;
            }
            end += n;
            count += 1;
        }
        let stop = if end < value.len() && ![b'\n', b'\r'].contains(&value[end]) && count >= max {
            space.filter(|&p| p > pos).unwrap_or(end)
        } else {
            end
        };
        ui.text(x, y + line as i32 * 16, &value[pos..stop], fg, bg)?;
        pos = stop;
        while value.get(pos) == Some(&b' ') {
            pos += 1;
        }
        while value.get(pos).is_some_and(|b| [b'\n', b'\r'].contains(b)) {
            pos += 1;
        }
    }
    Ok(())
}
