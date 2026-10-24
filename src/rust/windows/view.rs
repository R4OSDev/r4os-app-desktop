//! Current window chrome. Hosted app/console content is painted separately.
use super::{
    controller::Controller,
    window::{Kind, Window},
};
use crate::{
    input::event::Target,
    render::{geometry::Rect, primitive_assets::Result, text::cache::Fonts, ui::Ui},
    shell::style as s,
};
pub fn system_menu(
    ui: &mut Ui<'_, '_, impl Fonts>,
    x: i32,
    y: i32,
    controller: &Controller,
    index: usize,
    hover: Target,
    pressed: Target,
) -> Result<()> {
    let Some(win) = controller.windows().get(index) else {
        return Ok(());
    };
    ui.rect(x, y, 150, 108, s::WINDOW_BG)?;
    ui.bevel(Rect::new(x, y, 150, 108), false)?;
    for (dy, label, target) in [
        (4, b"Restore".as_slice(), Controller::window_target(index)),
        (24, b"Minimize", Controller::min_target(index)),
        (
            44,
            if win.maximized() {
                b"Restore Size".as_slice()
            } else {
                b"Maximize"
            },
            controller.max_target(index),
        ),
        (64, b"Info", Controller::info_target(index)),
        (86, b"Close", Controller::close_target(index)),
    ] {
        let hot = hover == target || pressed == target;
        let (fg, bg) = if hot {
            (s::TITLE_TEXT, s::SELECT_BG)
        } else {
            (s::TEXT, s::WINDOW_BG)
        };
        ui.rect(x + 3, y + dy, 144, 20, bg)?;
        ui.text(x + 12, y + dy + 6, label, fg, bg)?;
    }
    ui.rect(x + 4, y + 85, 142, 1, s::TASKBAR_DARK)
}
pub fn chrome(
    ui: &mut Ui<'_, '_, impl Fonts>,
    win: &Window,
    index: usize,
    active: bool,
    console_title: &[u8],
    hover: Target,
    pressed: Target,
) -> Result<()> {
    if !win.visible() || win.minimized() {
        return Ok(());
    }
    let frame = win.geometry();
    if !win.fullscreen() {
        ui.fill(frame, s::WINDOW_BG)?;
        ui.bevel(frame, false)?;
        let title = win.title_surface().rect;
        let color = if active {
            s::TITLE_ACTIVE
        } else {
            s::TITLE_INACTIVE
        };
        let text = if win.kind() == Kind::Terminal && index == 0 && !console_title.is_empty() {
            console_title
        } else {
            win.title().to_bytes()
        };
        ui.fill(title, color)?;
        ui.rect(
            title.x,
            title.y,
            title.w,
            1,
            if active {
                s::TITLE_LIGHT
            } else {
                s::TASKBAR_LIGHT
            },
        )?;
        ui.text(title.x + 5, title.y + 6, text, s::TITLE_TEXT, color)?;
        let max = if win.maximized() {
            [
                Target::TerminalMaxFull,
                Target::WmMaxFull,
                Target::App2MaxFull,
                Target::App3MaxFull,
            ][index]
        } else {
            [
                Target::TerminalMaxNormal,
                Target::WmMaxNormal,
                Target::App2MaxNormal,
                Target::App3MaxNormal,
            ][index]
        };
        let buttons = [
            (
                Controller::close_target(index),
                frame.right() - 22,
                Glyph::Close,
            ),
            (max, frame.right() - 40, Glyph::Maximize),
            (
                Controller::min_target(index),
                frame.right() - 58,
                Glyph::Minimize,
            ),
        ];
        for (target, x, glyph) in buttons {
            button(ui, x, frame.y + 5, glyph, target == pressed)?;
        }
        // Only one hover outline is drawn, in close/maximize/minimize order.
        if let Some((_, x, _)) = buttons
            .iter()
            .find(|(target, _, _)| *target == hover && *target != pressed)
        {
            ui.outline(
                Rect::new(*x + 2, frame.y + 7, s::BUTTON - 4, s::BUTTON - 4),
                s::TEXT,
            )?;
        }
    }
    ui.fill(win.client_surface().rect, s::CLIENT_BG)
}
#[derive(Clone, Copy)]
enum Glyph {
    Close,
    Maximize,
    Minimize,
}
fn button(
    ui: &mut Ui<'_, '_, impl Fonts>,
    x: i32,
    y: i32,
    glyph: Glyph,
    pressed: bool,
) -> Result<()> {
    ui.rect(
        x + 1,
        y + 1,
        s::BUTTON - 2,
        s::BUTTON - 2,
        if pressed {
            s::TASKBAR_PRESSED
        } else {
            s::TASKBAR
        },
    )?;
    ui.bevel(Rect::new(x, y, s::BUTTON, s::BUTTON), pressed)?;
    let off = i32::from(pressed);
    match glyph {
        Glyph::Close => {
            for (row, bits) in [
                0b1000001u8,
                0b0100010,
                0b0010100,
                0b0001000,
                0b0010100,
                0b0100010,
                0b1000001,
            ]
            .into_iter()
            .enumerate()
            {
                for column in 0..7 {
                    if bits & (1 << (6 - column)) != 0 {
                        ui.rect(
                            x + off + 5 + column,
                            y + off + 5 + row as i32,
                            1,
                            1,
                            s::TEXT,
                        )?;
                    }
                }
            }
        }
        Glyph::Maximize => {
            ui.rect(x + 4 + off, y + 4 + off, 9, 2, s::TEXT)?;
            ui.rect(x + 4 + off, y + 6 + off, 1, 7, s::TEXT)?;
            ui.rect(x + 12 + off, y + 6 + off, 1, 7, s::TEXT)?;
            ui.rect(x + 4 + off, y + 12 + off, 9, 1, s::TEXT)?;
        }
        Glyph::Minimize => ui.rect(x + 4 + off, y + 11 + off, 9, 2, s::TEXT)?,
    }
    Ok(())
}
