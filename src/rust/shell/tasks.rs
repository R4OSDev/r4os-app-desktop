//! Current paged task overview. The process owner supplies a copied complete
//! inventory generation; painting cannot spawn, reap or query a process.
use super::{
    dialog_view::{self, Input},
    style as s,
};
use crate::{
    input::event::Target,
    render::{geometry::Rect, primitive_assets::Result, text::cache::Fonts, ui::Ui},
    windows::window::Window,
};
use r4os_sdk::abi as a;
pub const WIDTH: i32 = 500;
pub const HEIGHT: i32 = 420;
pub const PAGE_ROWS: usize = 5;
#[derive(Clone, Copy, Default)]
pub struct Render {
    pub redraws: u32,
    pub cursor_moves: u32,
    pub cursor_presents: u32,
    pub full: u32,
    pub last_damage: u8,
}
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub windows: &'a [Window],
    pub active: usize,
    pub status: a::ProgramStatus,
    pub instances: &'a [a::ProgramInstanceSnapshot],
    pub render: Render,
    pub focus: Target,
}
fn number(mut value: u64, negative: bool, out: &mut [u8; 22]) -> &[u8] {
    let mut at = out.len();
    loop {
        at -= 1;
        out[at] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    if negative {
        at -= 1;
        out[at] = b'-';
    }
    &out[at..]
}
fn signed(value: i32, out: &mut [u8; 22]) -> &[u8] {
    number(i64::from(value).unsigned_abs(), value < 0, out)
}
fn panel(ui: &mut Ui<'_, '_, impl Fonts>, rect: Rect, title: &[u8]) -> Result<()> {
    ui.bevel(rect, true)?;
    ui.rect(rect.x + 8, rect.y, 88, 10, s::WINDOW_BG)?;
    ui.text(rect.x + 12, rect.y - 1, title, s::TEXT, s::WINDOW_BG)
}
pub fn paint(
    ui: &mut Ui<'_, '_, impl Fonts>,
    width: i32,
    height: i32,
    view: View<'_>,
    hover: Target,
    pressed: Target,
) -> Result<()> {
    let (x, y) = ((width - WIDTH) / 2, (height - HEIGHT) / 2);
    dialog_view::frame(ui, Rect::new(x, y, WIDTH, HEIGHT), b"Tasks")?;
    panel(ui, Rect::new(x + 18, y + 38, 464, 106), b"Desktop windows")?;
    for (dx, label) in [
        (36, b"Active".as_slice()),
        (88, b"Title"),
        (286, b"State"),
        (390, b"Instance"),
    ] {
        ui.text(x + dx, y + 58, label, s::TITLE_INACTIVE, s::WINDOW_BG)?;
    }
    let mut row_y = y + 78;
    let mut numeric = [0; 22];
    for (i, win) in view.windows.iter().enumerate() {
        if !win.visible() {
            continue;
        }
        let active = i == view.active;
        let (bg, fg) = if active {
            (s::SELECT_BG, s::TITLE_TEXT)
        } else {
            (s::CLIENT_BG, s::TEXT)
        };
        ui.rect(x + 28, row_y - 3, 444, 16, bg)?;
        ui.text(x + 42, row_y, if active { b"yes" } else { b"-" }, fg, bg)?;
        ui.text(x + 88, row_y, win.title().to_bytes(), fg, bg)?;
        ui.text(
            x + 286,
            row_y,
            if win.close_requested().is_some() {
                b"closing"
            } else if win.minimized() {
                b"minimized"
            } else {
                b"visible"
            },
            fg,
            bg,
        )?;
        ui.text(
            x + 390,
            row_y,
            if win.instance_id() == 0 {
                b"-"
            } else {
                number(u64::from(win.instance_id()), false, &mut numeric)
            },
            fg,
            bg,
        )?;
        row_y += 18;
    }
    panel(ui, Rect::new(x + 18, y + 156, 464, 154), b"R4X runtime")?;
    row_y = y + 178;
    ui.text(
        x + 36,
        row_y,
        if view.status.shell_running != 0 {
            b"Shell task: running"
        } else {
            b"Shell task: idle"
        },
        s::TEXT,
        s::WINDOW_BG,
    )?;
    ui.text(
        x + 260,
        row_y,
        if view.status.foreground_running != 0 {
            b"Foreground R4X: running"
        } else {
            b"Foreground R4X: idle"
        },
        s::TEXT,
        s::WINDOW_BG,
    )?;
    row_y += 18;
    ui.text(x + 36, row_y, b"Active instances:", s::TEXT, s::WINDOW_BG)?;
    ui.text(
        x + 172,
        row_y,
        number(u64::from(view.status.instance_count), false, &mut numeric),
        s::TEXT,
        s::WINDOW_BG,
    )?;
    ui.text(
        x + 260,
        row_y,
        if view.status.display_used != 0 {
            b"Last display: app framebuffer"
        } else {
            b"Last display: console/text"
        },
        s::TEXT,
        s::WINDOW_BG,
    )?;
    row_y += 18;
    for (dx, label) in [
        (36, b"ID".as_slice()),
        (90, b"Role"),
        (158, b"Class"),
        (234, b"State"),
        (328, b"Window"),
        (408, b"Exit"),
    ] {
        ui.text(x + dx, row_y, label, s::TITLE_INACTIVE, s::WINDOW_BG)?;
    }
    row_y += 18;
    if view.instances.is_empty() {
        ui.text(
            x + 36,
            row_y,
            b"No instance details",
            s::TITLE_INACTIVE,
            s::WINDOW_BG,
        )?;
    }
    for (row, snapshot) in view.instances.iter().take(PAGE_ROWS).enumerate() {
        let info = snapshot.info;
        let bg = if row & 1 == 1 { 0xe8e8e8 } else { s::WINDOW_BG };
        if row & 1 == 1 {
            ui.rect(x + 28, row_y - 3, 444, 16, bg)?;
        }
        ui.text(
            x + 36,
            row_y,
            number(u64::from(info.id), false, &mut numeric),
            s::TEXT,
            bg,
        )?;
        let role: &[u8] = if info.role == a::ProgramInstanceRole::foreground.0 {
            b"fg"
        } else if info.role == a::ProgramInstanceRole::shell.0 {
            b"shell"
        } else if info.role == a::ProgramInstanceRole::background.0 {
            b"bg"
        } else {
            b"?"
        };
        let class: &[u8] = if info.app_class == a::ProgramInstanceClass::console.0 {
            b"console"
        } else if info.app_class == a::ProgramInstanceClass::gui.0 {
            b"gui"
        } else if info.app_class == a::ProgramInstanceClass::service.0 {
            b"service"
        } else {
            b"?"
        };
        let state: &[u8] = if info.state == a::ProgramInstanceState::running.0 {
            b"running"
        } else if info.state == a::ProgramInstanceState::close_requested.0 {
            b"closing"
        } else if info.state == a::ProgramInstanceState::done.0 {
            b"done"
        } else {
            b"?"
        };
        for (dx, label) in [(90, role), (158, class), (234, state)] {
            ui.text(x + dx, row_y, label, s::TEXT, bg)?;
        }
        ui.text(
            x + 328,
            row_y,
            signed(info.window_id, &mut numeric),
            s::TEXT,
            bg,
        )?;
        ui.text(
            x + 408,
            row_y,
            signed(view.status.last_exit_code, &mut numeric),
            s::TEXT,
            bg,
        )?;
        row_y += 18;
    }
    panel(ui, Rect::new(x + 18, y + 320, 464, 52), b"Desktop render")?;
    for (dx, dy, label, value_x, value) in [
        (36, 340, b"Redraws:".as_slice(), 112, view.render.redraws),
        (178, 340, b"Cursor moves:", 292, view.render.cursor_moves),
        (
            36,
            356,
            b"Cursor presents:",
            174,
            view.render.cursor_presents,
        ),
        (246, 356, b"Full:", 298, view.render.full),
    ] {
        ui.text(x + dx, y + dy, label, s::TEXT, s::WINDOW_BG)?;
        ui.text(
            x + value_x,
            y + dy,
            number(u64::from(value), false, &mut numeric),
            s::TEXT,
            s::WINDOW_BG,
        )?;
    }
    ui.text(x + 350, y + 356, b"Last:", s::TEXT, s::WINDOW_BG)?;
    ui.text(
        x + 400,
        y + 356,
        match view.render.last_damage {
            1 => b"cursor",
            2 => b"mixed",
            3 => b"full",
            _ => b"none",
        },
        s::TEXT,
        s::WINDOW_BG,
    )?;
    dialog_view::button(
        ui,
        Rect::new(x + 210, y + 378, 80, 26),
        b"OK",
        Target::TaskOverviewOk,
        Input {
            focus: view.focus,
            hover,
            pressed,
        },
        true,
        true,
    )
}
