//! One immutable scene view, assembled only after input/service/lifetime work
//! completes. The compositor receives no mutable coordinator or SDK context.
use crate::{
    hosted::runtime as hosted,
    input::runtime as input,
    render::{
        compositor::{Hosted, Interaction, Overlay, Scene, Shell},
        coverage::Path,
        runtime::Stats,
    },
    shell::{
        state::{Dialog, State},
        status::Status,
        tasks,
        tray_sync::Mirror,
    },
};
pub fn scene<'a>(
    width: i32,
    height: i32,
    hosted: &'a hosted::State<'_>,
    shell: &'a State<'_>,
    input: &'a input::State,
    tray: &'a Mirror,
    status: &'a Status,
    stats: &Stats,
    software_cursor: bool,
    recording: u32,
    blink: bool,
) -> Scene<'a> {
    let overlay = if shell.dialog == Dialog::Tasks {
        let entries = hosted.model.inventory.visible();
        let start = shell.task_scroll.min(entries.len());
        let end = (start + tasks::PAGE_ROWS).min(entries.len());
        Overlay::Tasks(tasks::View {
            windows: hosted.model.windows.windows(),
            active: hosted.model.windows.active(),
            status: status.program,
            instances: &entries[start..end],
            render: tasks::Render {
                redraws: stats.redraws as u32,
                cursor_moves: input.stats.cursor_moves as u32,
                cursor_presents: stats.cursor as u32,
                full: stats.full as u32,
                last_damage: stats.last_damage,
            },
            focus: shell.dialog_focus,
        })
    } else {
        shell.overlay()
    };
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
            overlay,
            cursor_x: input.cursor.x,
            cursor_y: input.cursor.y,
            cursor_visible: software_cursor,
            recording,
        },
    }
}
