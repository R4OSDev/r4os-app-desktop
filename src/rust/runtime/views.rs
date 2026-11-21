//! One immutable scene view, assembled only after input/service/lifetime work
//! completes. The compositor receives no mutable coordinator or SDK context.
use crate::{
    hosted::runtime as hosted,
    input::runtime as input,
    render::{
        compositor::{Hosted, Interaction, Scene, Shell},
        coverage::Path,
    },
    shell::{state::State, status::Status, tray_sync::Mirror},
};
pub fn scene<'a>(
    width: i32,
    height: i32,
    hosted: &'a hosted::State<'_>,
    shell: &'a State<'_>,
    input: &'a input::State,
    tray: &'a Mirror,
    status: &'a Status,
    software_cursor: bool,
    recording: u32,
    blink: bool,
) -> Scene<'a> {
    Scene {
        width,
        height,
        path: Path::PrimaryCpu,
        disabled: false,
        hosted: Hosted {
            ..hosted.view(0, blink)
        },
        shell: Shell {
            config: &shell.config,
            items: &shell.folder.items,
            quick: &shell.quick,
            menu: &shell.menu,
            menu_view: shell.menu_control.view(),
            volume: shell.audio.view(),
            tray: tray.registry(),
            info: status.info(),
            clock: shell.config.taskbar_clock.then(|| shell.clock.label()),
            keyboard: status.keyboard(),
            wallpaper: shell.wallpaper.view(),
            font: 0,
        },
        input: Interaction {
            selected: shell.folder.selected,
            drag: shell.item_drag,
            hover: input.hover,
            pressed: input.pressed,
            tray_hover: input.tray_hover,
            tray_pressed: input.tray_pressed,
            system: shell.system_menu,
            time_menu: shell.time_menu,
            overlay: shell.overlay(),
            cursor_x: input.cursor.x,
            cursor_y: input.cursor.y,
            cursor_visible: software_cursor,
            recording,
        },
    }
}
