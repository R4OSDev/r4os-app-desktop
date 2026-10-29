//! Scene assembly from immutable shell/client snapshots. Layers have the
//! original stable IDs; native calls and lifetime changes happen outside this
//! traversal. CPU BO loans end synchronously with the complete capture.
#[cfg(test)]
#[path = "../../../Tests/RustNative/scene_pixels.rs"]
mod tests;
use super::{
    coverage::{Cover, Path},
    cursor,
    geometry::{MAX_DAMAGE_REGIONS, Rect, work_area},
    painter::Painter,
    pixels::PixelBytes,
    primitive_assets::{Error, Result},
    scene::{CpuScene, intersect},
    software::{self, Colors, ExternalFrame},
    text::cache::{Cache, Fonts},
    ui::Ui,
};
use crate::{
    hosted::{
        client::Clients, console_snapshot::Snapshot, replay::Shapes, text_surface::Surface,
        window_view,
    },
    input::event::Target,
    shell::{
        configuration::Config,
        desktop_items::Items,
        desktop_view, dialog_view, menu_controller, menu_view, message_box,
        quick_launch::Bar,
        start_menu::Menu,
        status_view, style as s, taskbar_view,
        tray::{Identity, Registry},
        volume,
    },
    windows::{
        controller::Controller,
        view as window_chrome,
        window::{Window, fullscreen_covers},
    },
};
#[derive(Clone, Copy)]
pub struct Wallpaper<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: PixelBytes<'a>,
}
impl Wallpaper<'_> {
    fn bounds(self, w: i32, h: i32) -> Rect {
        Rect::new(
            ((w as i64 - self.width as i64) / 2) as i32,
            ((h as i64 - self.height as i64) / 2) as i32,
            self.width as i32,
            self.height as i32,
        )
    }
}
#[derive(Clone, Copy)]
pub struct SystemMenu {
    pub x: i32,
    pub y: i32,
    pub index: usize,
}
#[derive(Clone, Copy)]
pub enum Overlay<'a> {
    None,
    Run { path: &'a [u8], focus: Target },
    Settings { config: &'a Config, focus: Target },
    Message(message_box::View<'a>),
    Tasks(crate::shell::tasks::View<'a>),
}
pub struct Hosted<'a> {
    pub windows: &'a Controller,
    pub clients: &'a Clients,
    pub consoles: &'a [Snapshot; 4],
    pub texts: &'a [Surface; 4],
    pub console_title: &'a [u8],
    pub console_path: &'a [u8],
    pub console_args: &'a [u8],
    pub font: u32,
    pub terminal_mode: bool,
    pub blink: bool,
}
pub struct Shell<'a> {
    pub config: &'a Config,
    pub items: &'a Items,
    pub quick: &'a Bar,
    pub menu: &'a Menu,
    pub menu_view: menu_controller::View,
    pub volume: volume::View<'a>,
    pub tray: &'a Registry,
    pub info: &'a [u8],
    pub clock: Option<&'a [u8]>,
    pub keyboard: Option<&'a [u8]>,
    pub wallpaper: Option<Wallpaper<'a>>,
    pub font: u32,
}
pub struct Interaction<'a> {
    pub selected: usize,
    pub drag: Option<usize>,
    pub hover: Target,
    pub pressed: Target,
    pub tray_hover: Identity,
    pub tray_pressed: Identity,
    pub system: Option<SystemMenu>,
    pub time_menu: bool,
    pub overlay: Overlay<'a>,
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub cursor_visible: bool,
    pub recording: u32,
}
pub struct Scene<'a> {
    pub width: i32,
    pub height: i32,
    pub path: Path,
    pub disabled: bool,
    pub hosted: Hosted<'a>,
    pub shell: Shell<'a>,
    pub input: Interaction<'a>,
}
#[derive(Default)]
pub struct Stats {
    pub rendered_windows: u8,
}
pub struct Completed {
    pub cull: Stats,
    pub composition: software::Stats,
}
/// Both output paths use this same layer traversal and the same paint calls.
/// Only pixel/primitive storage and the external BO admission differ.
pub(crate) trait Capture {
    fn paint(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
        paint: impl FnOnce(&mut Painter<'_, '_>) -> Result<()>,
    ) -> Result<bool>;
    fn paint_over_solid(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
        _background: u32,
        paint: impl FnOnce(&mut Painter<'_, '_>) -> Result<()>,
    ) -> Result<bool> {
        self.paint(key, bounds, damage, paint)
    }
    fn external(&mut self, window: usize, key: u32, bounds: Rect, damage: &[Rect]) -> Result<()>;
}
struct CpuCapture<'o, 'f> {
    capture: software::Capture<'o, 'f>,
    external: [Option<&'f mut dyn ExternalFrame>; 4],
}
impl Capture for CpuCapture<'_, '_> {
    fn paint(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
        paint: impl FnOnce(&mut Painter<'_, '_>) -> Result<()>,
    ) -> Result<bool> {
        self.capture.paint(key, bounds, damage, paint)
    }
    fn external(&mut self, window: usize, key: u32, bounds: Rect, damage: &[Rect]) -> Result<()> {
        if let Some(frame) = self.external[window].take() {
            self.capture.external(key, bounds, damage, frame)?;
        }
        Ok(())
    }
}
fn layer<F: Fonts>(
    capture: &mut impl Capture,
    key: u32,
    bounds: Rect,
    damage: Rect,
    cache: &mut Cache,
    fonts: &mut F,
    font: u32,
    paint: impl FnOnce(&mut Ui<'_, '_, F>) -> Result<()>,
) -> Result<()> {
    layer_with_background(
        capture, key, bounds, damage, cache, fonts, font, None, paint,
    )
}
fn layer_with_background<F: Fonts>(
    capture: &mut impl Capture,
    key: u32,
    bounds: Rect,
    damage: Rect,
    cache: &mut Cache,
    fonts: &mut F,
    font: u32,
    background: Option<u32>,
    paint: impl FnOnce(&mut Ui<'_, '_, F>) -> Result<()>,
) -> Result<()> {
    let paint = |p: &mut Painter<'_, '_>| {
        let mut ui = Ui::new(p.reborrow(), cache, fonts);
        ui.set_font(font);
        paint(&mut ui)
    };
    if let Some(rgb) = background {
        capture.paint_over_solid(key, bounds, damage, rgb, paint)
    } else {
        capture.paint(key, bounds, damage, paint)
    }
    .map(|_| ())
}
fn contains(outer: Rect, inner: Rect) -> bool {
    outer.x <= inner.x
        && outer.y <= inner.y
        && outer.right() >= inner.right()
        && outer.bottom() >= inner.bottom()
}
pub fn covered_by_higher(windows: &[Window], index: usize, active: usize, damage: Rect) -> bool {
    let Some(win) = windows.get(index) else {
        return false;
    };
    if !win.visible() || win.minimized() || index == active {
        return false;
    }
    let Some(area) = intersect(win.frame_surface().rect, damage) else {
        return false;
    };
    windows.iter().enumerate().any(|(i, w)| {
        i != index
            && (i == active || i >= index)
            && w.visible()
            && !w.minimized()
            && contains(w.frame_surface().rect, area)
    })
}
fn menu_bounds(scene: &Scene<'_>) -> Rect {
    let shell = &scene.shell;
    let view = shell.menu_view;
    let mut bounds = Rect::new(
        0,
        scene.height - s::TASKBAR_H - s::MENU_H,
        s::MENU_W,
        s::MENU_H,
    );
    if view.submenu_open {
        if let Some(r) = shell
            .menu
            .submenu_rect(scene.width, scene.height, view.submenu_parent)
        {
            bounds = bounds.merged(r);
        }
    }
    if view.nested_open {
        if let Some(r) = shell.menu.nested_rect(
            scene.width,
            scene.height,
            view.submenu_parent,
            view.nested_parent,
        ) {
            bounds = bounds.merged(r);
        }
    }
    bounds
}
fn overlay_bounds(scene: &Scene<'_>) -> Option<Rect> {
    overlay_rect(scene.input.overlay, scene.width, scene.height)
}
pub fn overlay_rect(overlay: Overlay<'_>, width: i32, height: i32) -> Option<Rect> {
    let (w, h) = match overlay {
        Overlay::None => return None,
        Overlay::Run { .. } => (420, 178),
        Overlay::Settings { .. } => (500, 294),
        Overlay::Message(_) => (380, 172),
        Overlay::Tasks(_) => (crate::shell::tasks::WIDTH, crate::shell::tasks::HEIGHT),
    };
    Some(Rect::new((width - w) / 2, (height - h) / 2, w, h))
}
/// # Safety
/// Every client's committed raster maps remain resident immutable native read
/// leases for the call. `external` supplies disjoint exact current BO fronts.
/// Neither service reset, client-cache replacement nor native unmap may run
/// during this scope. Caller owns those leases and polls lifetimes separately.
pub unsafe fn compose<'frames>(
    owner: &mut software::Owner,
    colors: &impl Colors,
    target: &mut CpuScene<'_>,
    scene: &Scene<'_>,
    external: [Option<&'frames mut dyn ExternalFrame>; 4],
    damage: &[Rect],
    cache: &mut Cache,
    fonts: &mut impl Fonts,
    icons: &mut (impl desktop_view::Icons + taskbar_view::Icons),
    shapes: &mut impl Shapes,
) -> Result<Completed> {
    let is_external = external.each_ref().map(Option::is_some);
    let mut capture = CpuCapture {
        capture: owner.capture(target.full_rect())?,
        external,
    };
    let cull = unsafe {
        draw(
            &mut capture,
            target.full_rect(),
            is_external,
            scene,
            damage,
            cache,
            fonts,
            icons,
            shapes,
        )
    }?;
    let composition = capture.capture.finish(colors, target)?;
    Ok(Completed { cull, composition })
}
/// # Safety
/// Hosted snapshot maps remain immutable and resident through all paint calls.
pub(crate) unsafe fn draw(
    capture: &mut impl Capture,
    desktop: Rect,
    is_external: [bool; 4],
    scene: &Scene<'_>,
    damage: &[Rect],
    cache: &mut Cache,
    fonts: &mut impl Fonts,
    icons: &mut (impl desktop_view::Icons + taskbar_view::Icons),
    shapes: &mut impl Shapes,
) -> Result<Stats> {
    if scene.width <= 0 || scene.height <= 0 || damage.len() > MAX_DAMAGE_REGIONS {
        return Err(Error::Bounds);
    }
    let mut regions = [Rect::ZERO; MAX_DAMAGE_REGIONS];
    let mut count = 0;
    for d in damage {
        if let Some(r) = intersect(*d, desktop) {
            if regions[..count].iter().any(|p: &Rect| p.intersects(r)) {
                return Err(Error::Invalid);
            }
            regions[count] = r;
            count += 1;
        }
    }
    let regions = &regions[..count];
    if regions.is_empty() {
        return Err(Error::Invalid);
    }
    if scene.disabled {
        for &damage in regions {
            capture.paint(1, desktop, damage, |paint| paint.fill(desktop, 0))?;
        }
        return Ok(Stats::default());
    }
    let frames =
        core::array::from_fn::<_, 4, _>(|i| scene.hosted.clients.slots()[i].frames().view());
    let mut stats = Stats::default();
    let shell = &scene.shell;
    let input = &scene.input;
    let hosted = &scene.hosted;
    let windows = hosted.windows.windows();
    let active = hosted.windows.active();
    if hosted.terminal_mode {
        let snapshot = &hosted.consoles[0];
        let state = if snapshot.valid {
            snapshot.state
        } else {
            r4os_contract::ConsoleState::default()
        };
        for &d in regions {
            layer(capture, 14, desktop, d, cache, fonts, hosted.font, |ui| {
                ui.fill(desktop, state.bg)?;
                if snapshot.valid && snapshot.has_output {
                    snapshot.paint(ui, Rect::new(0, 0, scene.width, scene.height), hosted.font)?;
                    snapshot.cursor(
                        ui,
                        Rect::new(0, 0, scene.width, scene.height),
                        hosted.blink,
                    )?;
                }
                Ok(())
            })?;
        }
    } else {
        for &d in regions {
            let cover = Cover::new(
                scene.path,
                windows.get(active),
                frames.get(active).is_some_and(|f| f.valid),
                is_external.get(active).copied().unwrap_or(false),
                d,
                d,
            );
            if cover.visible(d, desktop) {
                layer(capture, 1, desktop, d, cache, fonts, shell.font, |ui| {
                    ui.fill(desktop, shell.config.background)
                })?;
            }
            let info = &shell.info[..shell.info.len().min(96)];
            let r = Rect::new(
                8.max(scene.width - info.len() as i32 * 8 - 10),
                8.max(scene.height - s::TASKBAR_H - 16),
                info.len() as i32 * 8,
                8,
            );
            if cover.visible(d, r) {
                layer_with_background(
                    capture,
                    2,
                    r,
                    d,
                    cache,
                    fonts,
                    shell.font,
                    Some(shell.config.background),
                    |ui| ui.text(r.x, r.y, info, s::TITLE_TEXT, shell.config.background),
                )?;
            }
            if let Some(wall) = shell.wallpaper {
                let r = wall.bounds(scene.width, scene.height);
                if cover.visible(d, r) {
                    layer_with_background(
                        capture,
                        3,
                        r,
                        d,
                        cache,
                        fonts,
                        shell.font,
                        Some(shell.config.background),
                        |ui| {
                            ui.painter
                                .xrgb(r.x, r.y, wall.width, wall.height, wall.pixels)
                        },
                    )?;
                }
            }
        }
        let work = work_area(scene.width, scene.height, s::TASKBAR_H);
        if let Some(index) = input.drag {
            for &d in regions {
                if d.intersects(work) {
                    layer(capture, 4, work, d, cache, fonts, shell.font, |ui| {
                        desktop_view::grid(ui, scene.width, scene.height, shell.items, index)
                    })?;
                }
            }
        }
        for (i, item) in shell.items.entries().iter().enumerate() {
            let bounds = desktop_view::item_rect(item);
            for &d in regions {
                if !d.intersects(work) || !d.intersects(bounds) {
                    continue;
                }
                layer_with_background(
                    capture,
                    32 + i as u32,
                    bounds,
                    d,
                    cache,
                    fonts,
                    shell.font,
                    Some(shell.config.background),
                    |ui| {
                        desktop_view::paint_item(
                            ui,
                            shell.items,
                            icons,
                            i,
                            input.selected,
                            input.hover,
                            input.pressed,
                            shell.config.background,
                            shell.config.icon_text,
                        )
                    },
                )?;
            }
        }
        for i in (0..windows.len())
            .filter(|&i| i != active)
            .chain(core::iter::once(active).filter(|&i| i < windows.len()))
        {
            let window = &windows[i];
            let bounds = window.frame_surface().rect;
            let mut external_regions = [Rect::ZERO; MAX_DAMAGE_REGIONS];
            let mut n = 0;
            for &d in regions {
                if !window.visible()
                    || window.minimized()
                    || !d.intersects(bounds)
                    || covered_by_higher(windows, i, active, d)
                {
                    continue;
                }
                layer(
                    capture,
                    64 + i as u32,
                    bounds,
                    d,
                    cache,
                    fonts,
                    shell.font,
                    |ui| {
                        let replay = unsafe {
                            window_view::paint(
                                ui,
                                window,
                                i,
                                i == active,
                                &frames[i],
                                &hosted.texts[i],
                                is_external[i],
                                window_view::Console {
                                    title: hosted.console_title,
                                    path: hosted.console_path,
                                    args: hosted.console_args,
                                    snapshot: Some(&hosted.consoles[i]),
                                    font: hosted.font,
                                    blink: hosted.blink,
                                },
                                shapes,
                                input.hover,
                                input.pressed,
                            )
                        }?;
                        if replay.any {
                            stats.rendered_windows |= 1u8 << i;
                        }
                        Ok(())
                    },
                )?;
                if is_external[i] {
                    external_regions[n] = d;
                    n += 1;
                }
            }
            if n != 0 {
                capture.external(
                    i,
                    80 + i as u32,
                    window.client_surface().rect,
                    &external_regions[..n],
                )?;
            }
        }
        let taskbar = Rect::new(0, scene.height - s::TASKBAR_H, scene.width, s::TASKBAR_H);
        if !fullscreen_covers(windows, active, taskbar) {
            for &d in regions {
                if d.intersects(taskbar) {
                    layer(capture, 5, taskbar, d, cache, fonts, shell.font, |ui| {
                        taskbar_view::paint(
                            ui,
                            icons,
                            scene.width,
                            scene.height,
                            taskbar_view::State {
                                windows,
                                quick: Some(shell.quick),
                                active,
                                clock: if shell.config.taskbar_clock {
                                    shell.clock
                                } else {
                                    None
                                },
                                keyboard_layout: shell.keyboard,
                                volume: shell.volume,
                                tray: Some(shell.tray),
                                tray_hover: input.tray_hover,
                                tray_pressed: input.tray_pressed,
                                hover: input.hover,
                                pressed: input.pressed,
                                recording: input.recording,
                            },
                        )
                    })?;
                }
            }
        }
        if let Some(r) =
            shell
                .tray
                .tooltip_rect(input.tray_hover, scene.width, scene.height, s::TASKBAR_H)
        {
            for &d in regions {
                if d.intersects(r) {
                    layer(capture, 6, r, d, cache, fonts, shell.font, |ui| {
                        status_view::tray_tooltip(
                            ui,
                            shell.tray,
                            input.tray_hover,
                            scene.width,
                            scene.height,
                        )
                    })?;
                }
            }
        }
        if input.hover == Target::TaskbarVolume && !shell.volume.popup_open {
            let r = status_view::volume_tooltip_rect(
                scene.width,
                scene.height,
                shell.config.taskbar_clock,
            );
            for &d in regions {
                if d.intersects(r) {
                    layer(capture, 7, r, d, cache, fonts, shell.font, |ui| {
                        status_view::volume_tooltip(
                            ui,
                            scene.width,
                            scene.height,
                            shell.config.taskbar_clock,
                            shell.volume,
                        )
                    })?;
                }
            }
        }
        if shell.menu_view.open {
            let r = menu_bounds(scene);
            for &d in regions {
                if d.intersects(r) {
                    layer(capture, 8, r, d, cache, fonts, shell.font, |ui| {
                        menu_view::paint(
                            ui,
                            scene.width,
                            scene.height,
                            shell.menu,
                            shell.menu_view,
                            input.hover,
                            input.pressed,
                        )
                    })?;
                }
            }
        }
        if let Some(menu) = input.system {
            if menu.index < windows.len() {
                let r = Rect::new(menu.x, menu.y, 150, 108);
                for &d in regions {
                    if d.intersects(r) {
                        layer(capture, 9, r, d, cache, fonts, shell.font, |ui| {
                            window_chrome::system_menu(
                                ui,
                                menu.x,
                                menu.y,
                                hosted.windows,
                                menu.index,
                                input.hover,
                                input.pressed,
                            )
                        })?;
                    }
                }
            }
        }
        if input.time_menu {
            let r = status_view::time_menu_rect(scene.width, scene.height);
            for &d in regions {
                if d.intersects(r) {
                    layer(capture, 10, r, d, cache, fonts, shell.font, |ui| {
                        status_view::time_menu(
                            ui,
                            scene.width,
                            scene.height,
                            input.hover,
                            input.pressed,
                        )
                    })?;
                }
            }
        }
        if shell.volume.popup_open {
            let r = status_view::volume_popup_rect(
                scene.width,
                scene.height,
                shell.config.taskbar_clock,
            );
            for &d in regions {
                if d.intersects(r) {
                    layer(capture, 11, r, d, cache, fonts, shell.font, |ui| {
                        status_view::volume_popup(
                            ui,
                            scene.width,
                            scene.height,
                            shell.config.taskbar_clock,
                            shell.volume,
                            input.hover,
                            input.pressed,
                        )
                    })?;
                }
            }
        }
        if let Some(r) = overlay_bounds(scene) {
            for &d in regions {
                if d.intersects(r) {
                    layer(
                        capture,
                        12,
                        r,
                        d,
                        cache,
                        fonts,
                        shell.font,
                        |ui| match input.overlay {
                            Overlay::Run { path, focus } => dialog_view::run(
                                ui,
                                scene.width,
                                scene.height,
                                path,
                                dialog_view::Input {
                                    focus,
                                    hover: input.hover,
                                    pressed: input.pressed,
                                },
                            ),
                            Overlay::Settings { config, focus } => dialog_view::settings(
                                ui,
                                scene.width,
                                scene.height,
                                config,
                                dialog_view::Input {
                                    focus,
                                    hover: input.hover,
                                    pressed: input.pressed,
                                },
                            ),
                            Overlay::Message(view) => dialog_view::message(
                                ui,
                                scene.width,
                                scene.height,
                                view,
                                input.hover,
                                input.pressed,
                            ),
                            Overlay::Tasks(view) => crate::shell::tasks::paint(
                                ui,
                                scene.width,
                                scene.height,
                                view,
                                input.hover,
                                input.pressed,
                            ),
                            Overlay::None => Ok(()),
                        },
                    )?;
                }
            }
        }
        if input.cursor_visible {
            let r = Rect::new(
                input.cursor_x,
                input.cursor_y,
                cursor::WIDTH as i32,
                cursor::HEIGHT as i32,
            );
            for &d in regions {
                if d.intersects(r) {
                    layer_with_background(
                        capture,
                        13,
                        r,
                        d,
                        cache,
                        fonts,
                        shell.font,
                        Some(shell.config.background),
                        |ui| paint_cursor(&mut ui.painter, input.cursor_x, input.cursor_y),
                    )?;
                }
            }
        }
    }
    Ok(stats)
}
fn paint_cursor(p: &mut Painter<'_, '_>, x: i32, y: i32) -> Result<()> {
    for row in 0..cursor::HEIGHT {
        for (bits, color) in [
            (cursor::black_bits(row), 0),
            (cursor::white_bits(row), 0xffffff),
        ] {
            let mut col = 0;
            while col < cursor::WIDTH {
                if !cursor::bit_set(bits, col) {
                    col += 1;
                    continue;
                }
                let start = col;
                while col < cursor::WIDTH && cursor::bit_set(bits, col) {
                    col += 1;
                }
                p.rect(
                    x + start as i32,
                    y + row as i32,
                    (col - start) as i32,
                    1,
                    color,
                )?;
            }
        }
    }
    Ok(())
}
