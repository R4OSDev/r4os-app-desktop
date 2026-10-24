//! Pointer priority over immutable views. Geometry matches the current shell
//! painters; client focus/dispatch and native input access happen elsewhere.
use super::event::Target;
use crate::{
    render::{
        compositor::{self, Overlay, Scene},
        geometry::Rect,
    },
    shell::{
        message_box::Buttons, quick_launch, status_view, style as s, taskbar_view as bar, tray,
        volume,
    },
    windows::controller::Controller,
};
pub fn target(scene: &Scene<'_>, x: i32, y: i32) -> Target {
    if scene.hosted.terminal_mode {
        return Target::None;
    }
    let (w, h) = (scene.width, scene.height);
    let input = &scene.input;
    let shell = &scene.shell;
    let windows = scene.hosted.windows;
    if let Some(menu) = input.system {
        let inside = Rect::new(menu.x, menu.y, 150, 108).contains(x, y);
        if inside {
            for (top, target) in [
                (4, Controller::window_target(menu.index)),
                (24, Controller::min_target(menu.index)),
                (44, windows.max_target(menu.index)),
                (64, Controller::info_target(menu.index)),
                (86, Controller::close_target(menu.index)),
            ] {
                if (menu.y + top..menu.y + top + 20).contains(&y) {
                    return target;
                }
            }
        }
        return Target::StartMenuBackdrop;
    }
    if input.time_menu {
        let r = status_view::time_menu_rect(w, h);
        return if r.contains(x, y) {
            if (4..26).contains(&(y - r.y)) {
                Target::TimeMenuClock
            } else if (26..48).contains(&(y - r.y)) {
                Target::TimeMenuSettings
            } else {
                Target::TimeMenuBackdrop
            }
        } else {
            Target::TimeMenuBackdrop
        };
    }
    let volume_icon = bar::volume_rect(w, h, shell.clock.is_some());
    if shell.volume.popup_open {
        if volume_icon.contains(x, y) {
            return Target::TaskbarVolume;
        }
        let popup = status_view::volume_popup_rect(w, h, shell.clock.is_some());
        if volume::track_rect(popup).contains(x, y) {
            return Target::VolumePopupSlider;
        }
        if volume::mute_rect(popup).contains(x, y) {
            return Target::VolumePopupMute;
        }
        if let Some(outputs) = shell.volume.outputs {
            let targets = [
                Target::VolumeOutput0,
                Target::VolumeOutput1,
                Target::VolumeOutput2,
                Target::VolumeOutput3,
            ];
            for (i, target) in targets.into_iter().enumerate().take(outputs.count as usize) {
                if volume::output_rect(popup, i).contains(x, y) {
                    return target;
                }
            }
        }
        for (target, r) in [
            (Target::VolumeOutputAuto, volume::output_auto_rect(popup)),
            (
                Target::VolumeOutputPrevious,
                volume::output_page_rect(popup, false),
            ),
            (
                Target::VolumeOutputNext,
                volume::output_page_rect(popup, true),
            ),
        ] {
            if r.contains(x, y) {
                return target;
            }
        }
        return if popup.contains(x, y) {
            Target::VolumePopupSurface
        } else {
            Target::VolumePopupBackdrop
        };
    }
    if let Some(r) = compositor::overlay_rect(input.overlay, w, h) {
        return match input.overlay {
            Overlay::Run { .. } => {
                for (dx, width, target) in [
                    (24, 88, Target::RunBrowse),
                    (202, 76, Target::RunOk),
                    (288, 86, Target::RunCancel),
                ] {
                    if Rect::new(r.x + dx, r.y + 126, width, 26).contains(x, y) {
                        return target;
                    }
                }
                if Rect::new(r.x + 24, r.y + 68, 372, 24).contains(x, y) {
                    Target::RunInput
                } else {
                    Target::RunBackdrop
                }
            }
            Overlay::Settings { .. } => {
                if Rect::new(r.x + 162, r.y + 262, 80, 26).contains(x, y) {
                    Target::SettingsOk
                } else if Rect::new(r.x + 254, r.y + 262, 86, 26).contains(x, y) {
                    Target::SettingsCancel
                } else {
                    Target::MessageBackdrop
                }
            }
            Overlay::Message(view) => {
                if view.buttons == Buttons::Ok {
                    if Rect::new(r.x + 150, r.y + 126, 80, 26).contains(x, y) {
                        return Target::MessageOk;
                    }
                } else {
                    if Rect::new(r.x + 106, r.y + 126, 76, 26).contains(x, y) {
                        return if view.buttons == Buttons::YesNo {
                            Target::MessageYes
                        } else {
                            Target::MessageOk
                        };
                    }
                    if Rect::new(r.x + 198, r.y + 126, 76, 26).contains(x, y) {
                        return Target::MessageNo;
                    }
                }
                Target::MessageBackdrop
            }
            Overlay::Tasks(_) => {
                if Rect::new(r.x + 210, r.y + 378, 80, 26).contains(x, y) {
                    Target::TaskOverviewOk
                } else {
                    Target::MessageBackdrop
                }
            }
            Overlay::None => Target::None,
        };
    }
    let menu = shell.menu_view;
    if menu.open {
        if menu.nested_open {
            if let Some(i) =
                shell
                    .menu
                    .nested_hit(w, h, menu.submenu_parent, menu.nested_parent, x, y)
            {
                return shell
                    .menu
                    .nested_target(menu.submenu_parent, menu.nested_parent, i);
            }
        }
        if menu.submenu_open {
            if let Some(i) = shell.menu.submenu_hit(w, h, menu.submenu_parent, x, y) {
                return shell.menu.submenu_target(menu.submenu_parent, i);
            }
        }
        if let Some(i) = shell.menu.hit(h, x, y) {
            return shell.menu.target(i);
        }
        return if Rect::new(2, h - s::TASKBAR_H + 4, s::START_W, s::START_H).contains(x, y) {
            Target::StartButton
        } else {
            Target::StartMenuBackdrop
        };
    }
    let active = &windows.windows()[windows.active()];
    if active.fullscreen() && active.contains(x, y) {
        return windows.active_target_at(x, y);
    }
    if Rect::new(2, h - s::TASKBAR_H + 4, s::START_W, s::START_H).contains(x, y) {
        return Target::StartButton;
    }
    if let Some(i) = shell.quick.hit(h, x, y) {
        return shell.quick.target(i);
    }
    if shell.tray.hit(x, y).is_some() {
        return Target::TaskbarTrayExternal;
    }
    if shell.volume.installed && volume_icon.contains(x, y) {
        return Target::TaskbarVolume;
    }
    if shell.keyboard.is_some()
        && bar::layout_rect(w, h, shell.clock.is_some(), shell.volume.installed).contains(x, y)
    {
        return Target::TaskbarKeyboardLayout;
    }
    if shell.clock.is_some() && bar::clock_rect(w, h).contains(x, y) {
        return Target::TaskbarClock;
    }
    let left = quick_launch::taskbar_window_start_x(shell.quick.count());
    let count = windows.windows().iter().filter(|w| w.visible()).count();
    let mut ordinal = 0;
    for (i, win) in windows.windows().iter().enumerate() {
        if !win.visible() {
            continue;
        }
        let r = tray::window_button_rect(
            left,
            shell.tray.window_right,
            count,
            ordinal,
            h - s::TASKBAR_H + 4,
            s::START_H,
        );
        ordinal += 1;
        if r.contains(x, y) {
            return Controller::taskbar_target(i);
        }
    }
    if let Some(target) = windows.target_at(x, y) {
        return target;
    }
    shell
        .items
        .hit(x, y)
        .map_or(Target::None, |i| shell.items.target(i))
}
