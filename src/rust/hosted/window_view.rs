//! Window chrome and current hosted content receive disjoint, short loans.
//! No service/process access is permitted while a layer is being painted.
use super::{
    console_snapshot::Snapshot,
    frame_snapshot::View,
    replay::{self, LeasedMaps, Stats},
    text_surface::Surface,
};
use crate::{
    input::event::Target,
    render::{
        geometry::Rect,
        primitive_assets::Result,
        text::{cache::Fonts, layout::decode},
        ui::Ui,
    },
    shell::style as s,
    windows::{
        view,
        window::{Kind, Window},
    },
};
pub struct Console<'a> {
    pub title: &'a [u8],
    pub path: &'a [u8],
    pub args: &'a [u8],
    pub snapshot: Option<&'a Snapshot>,
    pub font: u32,
    pub blink: bool,
}
/// # Safety
/// `frame.rasters` contains the exact admitted native read maps of this
/// immutable cache generation. Their storage remains resident and unchanged
/// for the entire call; no concurrent reset/unmap or writer can invalidate it.
pub unsafe fn paint(
    ui: &mut Ui<'_, '_, impl Fonts>,
    win: &Window,
    index: usize,
    active: bool,
    frame: &View<'_>,
    text: &Surface,
    external: bool,
    console: Console<'_>,
    shapes: &mut impl replay::Shapes,
    hover: Target,
    pressed: Target,
) -> Result<Stats> {
    if !win.visible() || win.minimized() {
        return Ok(Stats::default());
    }
    view::chrome(ui, win, index, active, console.title, hover, pressed)?;
    if external {
        return Ok(Stats::default());
    }
    let client = win.client_surface().rect;
    match win.kind() {
        Kind::Terminal => {
            let state = console.snapshot.filter(|s| s.valid).map_or(
                r4os_contract::ConsoleState {
                    fg: s::TEXT,
                    bg: s::CLIENT_BG,
                    ..r4os_contract::ConsoleState::default()
                },
                |s| s.state,
            );
            ui.fill(client, state.bg)?;
            if let Some(snapshot) = console.snapshot.filter(|s| s.valid && s.has_output) {
                snapshot.paint(ui, client, console.font)?;
                snapshot.cursor(ui, client, console.blink)?;
            } else {
                for (dy, label) in [
                    (6, b"Console app host".as_slice()),
                    (62, b"Waiting for console output."),
                    (86, b"C:\\>"),
                ] {
                    ui.text(client.x + 10, client.y + dy, label, state.fg, state.bg)?;
                }
                if index == 0 {
                    ui.text(
                        client.x + 10,
                        client.y + 22,
                        console.path,
                        state.fg,
                        state.bg,
                    )?;
                    if !console.args.is_empty() {
                        ui.text(
                            client.x + 10,
                            client.y + 38,
                            console.args,
                            state.fg,
                            state.bg,
                        )?;
                    }
                }
            }
        }
        Kind::Manager => {
            for (dy, label) in [
                (6, b"Desktop window manager".as_slice()),
                (22, b"Focus, move, resize and buttons are active."),
                (46, b"R4X window binding is prepared."),
            ] {
                ui.text(client.x + 10, client.y + dy, label, s::TEXT, s::CLIENT_BG)?;
            }
        }
        Kind::App => {
            if win.starting() {
                ui.text(
                    client.x + 10,
                    client.y + 22,
                    b"Starting program...",
                    s::TEXT,
                    s::CLIENT_BG,
                )?;
            } else if win.close_requested().is_some() {
                ui.text(
                    client.x + 10,
                    client.y + 46,
                    b"Close requested...",
                    s::TEXT,
                    s::CLIENT_BG,
                )?;
            } else if frame.valid && !frame.commands.is_empty() {
                let maps = unsafe { LeasedMaps::new(frame.rasters) };
                return replay::replay(ui, client, frame, &maps, shapes);
            } else {
                let mut id = [0u8; 10];
                let value = decimal(win.instance_id(), &mut id);
                ui.text(
                    client.x + 10,
                    client.y + 6,
                    b"R4X GUI instance",
                    s::TEXT,
                    s::CLIENT_BG,
                )?;
                ui.text(
                    client.x + 10,
                    client.y + 22,
                    b"Instance:",
                    s::TEXT,
                    s::CLIENT_BG,
                )?;
                ui.text(client.x + 88, client.y + 22, value, s::TEXT, s::CLIENT_BG)?;
                if !text.text().is_empty() {
                    hosted_text(ui, client, text.text())?;
                } else {
                    ui.text(
                        client.x + 10,
                        client.y + 46,
                        b"Hosted text surface waiting.",
                        s::TEXT,
                        s::CLIENT_BG,
                    )?;
                }
            }
        }
    }
    Ok(Stats::default())
}
fn decimal(mut n: u32, out: &mut [u8; 10]) -> &[u8] {
    let mut i = out.len();
    loop {
        i -= 1;
        out[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            return &out[i..];
        }
    }
}
fn hosted_text(ui: &mut Ui<'_, '_, impl Fonts>, rect: Rect, text: &[u8]) -> Result<()> {
    let x = rect.x + 10;
    let mut y = rect.y + 46;
    let bottom = rect.bottom() - 8;
    let max = ((rect.w - 20) / 8).max(8) as usize;
    let mut line = [0; 96];
    let (mut len, mut columns, mut i) = (0, 0, 0);
    while i < text.len() && text[i] != 0 && y + 12 <= bottom {
        if text[i] == b'\r' {
            i += 1;
            continue;
        }
        let (_, n) = decode(&text[i..]);
        if text[i] == b'\n' || columns >= max || len + n >= line.len() {
            ui.text(x, y, &line[..len], s::TEXT, s::CLIENT_BG)?;
            len = 0;
            columns = 0;
            y += 16;
            if text[i] == b'\n' {
                i += 1;
            }
            continue;
        }
        line[len..len + n].copy_from_slice(&text[i..i + n]);
        len += n;
        columns += 1;
        i += n;
    }
    if len > 0 && y + 12 <= bottom {
        ui.text(x, y, &line[..len], s::TEXT, s::CLIENT_BG)?;
    }
    Ok(())
}
