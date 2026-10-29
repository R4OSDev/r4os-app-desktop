//! Pointer routing owns only transient input state. It returns shell effects
//! after short window/GUI loans and damages the affected surfaces explicitly.
use super::{event::Target, runtime::State};
use crate::{
    hosted::runtime::State as Hosted,
    outputs::manager::Manager,
    render::{
        compositor::SystemMenu,
        geometry::{Dirty, Rect},
        output_geometry::{self, topology::Point},
    },
    shell::{
        controller::{self, Action},
        desktop_items,
        state::{Dialog, State as Shell},
        style as s,
        tray::Identity,
    },
    windows::{controller::Controller, layout},
};
use r4os_sdk::{App, abi as a, gui::Session};
#[derive(Clone, Copy)]
struct ItemDrag {
    index: usize,
    mouse: Point,
    start: Point,
    active: bool,
}
// A snapshot can carry wheel, right release and one primary shell action.
// Keep extra effects compact; the general Action can contain a large Launch.
#[derive(Clone, Copy)]
pub enum PointerEffect {
    VolumeStep(i32),
    Tray {
        identity: Identity,
        kind: u16,
        wheel: i32,
        x: i32,
        y: i32,
    },
}
impl PointerEffect {
    pub fn action(self) -> Action {
        match self {
            Self::VolumeStep(step) => Action::VolumeStep(step),
            Self::Tray {
                identity,
                kind,
                wheel,
                x,
                y,
            } => Action::Tray {
                identity,
                kind,
                wheel,
                x,
                y,
            },
        }
    }
}
pub struct PointerActions {
    pub wheel: Option<PointerEffect>,
    pub secondary: Option<PointerEffect>,
    pub primary: Action,
}
impl PointerActions {
    fn none() -> Self {
        Self {
            wheel: None,
            secondary: None,
            primary: Action::None,
        }
    }
}
#[derive(Default)]
pub struct Router {
    item: Option<ItemDrag>,
}
pub fn shell_damage(shell: &Shell<'_>, dirty: &mut Dirty, width: i32, height: i32) {
    if let Some(rect) = shell.dialog_rect(width, height) {
        dirty.invalidate(rect);
    }
    let menu = shell.menu_control.view();
    if menu.open {
        dirty.invalidate(Rect::new(
            0,
            height - s::TASKBAR_H - s::MENU_H,
            s::MENU_W,
            s::MENU_H,
        ));
        if menu.submenu_open {
            if let Some(r) = shell.menu.submenu_rect(width, height, menu.submenu_parent) {
                dirty.invalidate(r);
            }
        }
        if menu.nested_open {
            if let Some(r) =
                shell
                    .menu
                    .nested_rect(width, height, menu.submenu_parent, menu.nested_parent)
            {
                dirty.invalidate(r);
            }
        }
    }
    if let Some(menu) = shell.system_menu {
        dirty.invalidate(Rect::new(menu.x, menu.y, 150, 108));
    }
    if shell.time_menu {
        dirty.invalidate(crate::shell::status_view::time_menu_rect(width, height));
    }
    if shell.audio.ui.popup_open {
        dirty.invalidate(crate::shell::status_view::volume_popup_rect(
            width,
            height,
            shell.config.taskbar_clock,
        ));
    }
}
fn item_rect(item: &desktop_items::Item) -> Rect {
    Rect::new(item.x, item.y, desktop_items::ICON_W, desktop_items::ICON_H)
}
fn target_damage(
    target: Target,
    hosted: &Hosted<'_>,
    shell: &Shell<'_>,
    dirty: &mut Dirty,
    w: i32,
    h: i32,
) {
    if let Some(i) = Controller::index_for_target(target) {
        dirty.invalidate(hosted.model.windows.windows()[i].frame_surface().rect);
    } else if (140..=149).contains(&(target as u16)) || (120..=133).contains(&(target as u16)) {
        dirty.invalidate(Rect::new(0, h - s::TASKBAR_H, w, s::TASKBAR_H));
    } else if (150..=181).contains(&(target as u16)) {
        if let Some(item) = shell.folder.items.entries().get(target as usize - 150) {
            dirty.invalidate(item_rect(item));
        }
    } else {
        shell_damage(shell, dirty, w, h);
    }
}
impl Router {
    pub fn pointer(
        &mut self,
        input: &mut State,
        shell: &mut Shell<'_>,
        hosted: &mut Hosted<'_>,
        outputs: &Manager<'_>,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        dirty: &mut Dirty,
        target: Target,
        identity: Identity,
        registry: &crate::shell::tray::Registry,
        mouse: a::Mouse,
        remote: bool,
        software_cursor: bool,
        width: i32,
        height: i32,
        tick: u64,
        hz: u32,
    ) -> PointerActions {
        let previous = if remote {
            input.remote_buttons
        } else {
            input.physical_buttons
        };
        if remote {
            input.remote_buttons = mouse.buttons;
        } else {
            input.physical_buttons = mouse.buttons;
        }
        // Terminal mode has no pointer UI. Preserve wheel scroll and button
        // history without repainting hidden shell chrome or a software cursor.
        if hosted.terminal_mode {
            if mouse.wheel != 0 {
                hosted.scroll_console(0, mouse.wheel.saturating_mul(3), dirty);
            }
            return PointerActions::none();
        }
        let point = Point {
            x: mouse.x,
            y: mouse.y,
        };
        if input.cursor != point {
            if software_cursor {
                dirty.invalidate(output_geometry::cursor_rect(hosted.bounds, input.cursor));
                dirty.invalidate(output_geometry::cursor_rect(hosted.bounds, point));
            }
            input.stats.cursor_moves = input.stats.cursor_moves.wrapping_add(1);
            input.cursor = point;
        }
        if input.hover != target || !input.tray_hover.same(identity) {
            if input.hover == Target::TaskbarVolume || target == Target::TaskbarVolume {
                dirty.invalidate(crate::shell::status_view::volume_tooltip_rect(
                    width,
                    height,
                    shell.config.taskbar_clock,
                ));
            }
            for identity in [input.tray_hover, identity] {
                if let Some(rect) = registry.tooltip_rect(identity, width, height, s::TASKBAR_H) {
                    dirty.invalidate(rect);
                }
            }
            target_damage(input.hover, hosted, shell, dirty, width, height);
            target_damage(target, hosted, shell, dirty, width, height);
            shell.menu_control.hover(&shell.menu, target);
            input.hover = target;
            input.tray_hover = identity;
        }
        let wheel = if mouse.wheel == 0 {
            None
        } else if target == Target::TaskbarVolume || shell.audio.ui.popup_open {
            Some(PointerEffect::VolumeStep(if mouse.wheel > 0 {
                1
            } else {
                -1
            }))
        } else if shell.dialog == Dialog::Tasks {
            shell_damage(shell, dirty, width, height);
            controller::task_scroll(
                shell,
                if mouse.wheel > 0 { -3 } else { 3 },
                hosted.model.inventory.visible().len(),
            );
            None
        } else if identity.valid() {
            Some(PointerEffect::Tray {
                identity,
                kind: a::tray_event_kind_wheel,
                wheel: mouse.wheel,
                x: mouse.x,
                y: mouse.y,
            })
        } else {
            if let Some(i) = Controller::index_for_target(target) {
                hosted.scroll_console(i, mouse.wheel.saturating_mul(3), dirty);
            }
            None
        };
        let mut secondary = None;
        let primary = (|| {
            if mouse.buttons & 2 != 0 && previous & 2 == 0 {
                if identity.valid() {
                    input.right_tray = identity;
                } else {
                    shell_damage(shell, dirty, width, height);
                    let was_time = shell.time_menu;
                    shell.close_top();
                    if !was_time && target == Target::TaskbarClock {
                        shell.time_menu = true;
                    } else if let Some(index) = Controller::index_for_target(target) {
                        hosted.focus(index, session, app, tick, dirty);
                        shell.system_menu = Some(SystemMenu {
                            x: mouse.x.clamp(0, (width - 150).max(0)),
                            y: mouse.y.clamp(0, (height - 108).max(0)),
                            index,
                        });
                    }
                    shell_damage(shell, dirty, width, height);
                }
            }
            if mouse.buttons & 2 == 0 && previous & 2 != 0 {
                let old = core::mem::take(&mut input.right_tray);
                if old.valid() && old.same(identity) {
                    secondary = Some(PointerEffect::Tray {
                        identity,
                        kind: a::tray_event_kind_context,
                        wheel: 0,
                        x: mouse.x,
                        y: mouse.y,
                    });
                }
            }
            if mouse.buttons & 1 != 0 {
                if previous & 1 == 0 {
                    input.double_click = input.down_target == target
                        && tick.saturating_sub(input.down_tick) <= u64::from(hz).div_ceil(4);
                    input.down_target = target;
                    input.down_tick = tick;
                    input.pressed = target;
                    input.tray_pressed = identity;
                    target_damage(target, hosted, shell, dirty, width, height);
                    if target == Target::VolumePopupSlider {
                        shell.audio.dragging = true;
                        return Action::VolumePointer {
                            x: mouse.x,
                            commit: false,
                        };
                    }
                    if let Some(index) = Controller::index_for_target(target) {
                        if target == Controller::window_target(index) {
                            let change = hosted.model.windows.pointer_down(index, mouse.x, mouse.y);
                            if let Some(i) = change.client_pointer_down {
                                hosted.gui_mouse(
                                    app,
                                    session,
                                    i,
                                    a::GuiEventKind::mouse_down,
                                    mouse.x,
                                    mouse.y,
                                    mouse.buttons,
                                    tick,
                                );
                            }
                            hosted.changes(&change, session, app, tick, dirty);
                        }
                    } else if (150..=181).contains(&(target as u16)) && !shell.blocked() {
                        let i = target as usize - 150;
                        if let Some(item) = shell.folder.items.entries().get(i) {
                            self.item = Some(ItemDrag {
                                index: i,
                                mouse: point,
                                start: Point {
                                    x: item.x,
                                    y: item.y,
                                },
                                active: false,
                            });
                            if let Some(old) =
                                shell.folder.items.entries().get(shell.folder.selected)
                            {
                                dirty.invalidate(item_rect(old));
                            }
                            shell.folder.selected = i;
                        }
                    }
                } else if shell.audio.dragging {
                    return Action::VolumePointer {
                        x: mouse.x,
                        commit: false,
                    };
                } else if let Some(mut drag) = self.item {
                    let dx = mouse.x.saturating_sub(drag.mouse.x);
                    let dy = mouse.y.saturating_sub(drag.mouse.y);
                    drag.active |= dx.unsigned_abs() >= 4 || dy.unsigned_abs() >= 4;
                    if drag.active {
                        if let Some(item) = shell.folder.items.get_mut(drag.index) {
                            dirty.invalidate(item_rect(item));
                            item.x = drag
                                .start
                                .x
                                .saturating_add(dx)
                                .clamp(0, (width - desktop_items::ICON_W).max(0));
                            item.y = drag
                                .start
                                .y
                                .saturating_add(dy)
                                .clamp(0, (height - s::TASKBAR_H - desktop_items::ICON_H).max(0));
                            dirty.invalidate(item_rect(item));
                            shell.item_drag = Some(drag.index);
                        }
                    }
                    self.item = Some(drag);
                } else if let Some(i) = hosted.model.windows.pointer_window() {
                    let (_, area) = layout::output(
                        outputs,
                        hosted.model.windows.windows()[i].geometry(),
                        width,
                        height,
                    );
                    let change = hosted
                        .model
                        .windows
                        .pointer_motion(mouse.x, mouse.y, area, |r| {
                            Some(output_geometry::rebase_window(
                                &outputs.layout,
                                r,
                                Point::default(),
                                area,
                                s::TITLE_H as u32,
                            ))
                        });
                    hosted.changes(&change, session, app, tick, dirty);
                }
            } else if previous & 1 != 0 {
                let pressed = core::mem::replace(&mut input.pressed, Target::None);
                let tray = core::mem::take(&mut input.tray_pressed);
                target_damage(pressed, hosted, shell, dirty, width, height);
                if shell.audio.dragging {
                    shell.audio.dragging = false;
                    return Action::VolumePointer {
                        x: mouse.x,
                        commit: true,
                    };
                }
                if let Some(drag) = self.item.take() {
                    if drag.active {
                        // Apply the final release coordinate even without motion.
                        if let Some(item) = shell.folder.items.get_mut(drag.index) {
                            dirty.invalidate(item_rect(item));
                            item.x = drag
                                .start
                                .x
                                .saturating_add(mouse.x.saturating_sub(drag.mouse.x));
                            item.y = drag
                                .start
                                .y
                                .saturating_add(mouse.y.saturating_sub(drag.mouse.y));
                        }
                        shell
                            .folder
                            .items
                            .snap(drag.index, width, height, s::TASKBAR_H);
                        shell.item_drag = None;
                        if let Some(item) = shell.folder.items.entries().get(drag.index) {
                            dirty.invalidate(item_rect(item));
                        }
                        return Action::LayoutChanged;
                    }
                }
                if let Some(i) = hosted.model.windows.pointer_window() {
                    let (_, area) = layout::output(
                        outputs,
                        hosted.model.windows.windows()[i].geometry(),
                        width,
                        height,
                    );
                    let change =
                        hosted
                            .model
                            .windows
                            .pointer_release(mouse.x, mouse.y, area, |r| {
                                Some(output_geometry::rebase_window(
                                    &outputs.layout,
                                    r,
                                    Point::default(),
                                    area,
                                    s::TITLE_H as u32,
                                ))
                            });
                    hosted.changes(&change, session, app, tick, dirty);
                    return Action::None;
                }
                if tray.valid() && tray.same(identity) {
                    return Action::Tray {
                        identity,
                        kind: if input.double_click {
                            a::tray_event_kind_double
                        } else {
                            a::tray_event_kind_primary
                        },
                        wheel: 0,
                        x: mouse.x,
                        y: mouse.y,
                    };
                }
                if let Some(i) = Controller::index_for_target(pressed) {
                    hosted.gui_mouse(
                        app,
                        session,
                        i,
                        a::GuiEventKind::mouse_up,
                        mouse.x,
                        mouse.y,
                        mouse.buttons,
                        tick,
                    );
                }
                if pressed == target {
                    if (150..=181).contains(&(target as u16)) && !input.double_click {
                        return Action::None;
                    }
                    shell_damage(shell, dirty, width, height);
                    let action = controller::activate(shell, target);
                    shell_damage(shell, dirty, width, height);
                    return action;
                }
            } else if !shell.blocked() {
                if let Some(i) = hosted.active_gui() {
                    hosted.gui_mouse(
                        app,
                        session,
                        i,
                        a::GuiEventKind::mouse_move,
                        mouse.x,
                        mouse.y,
                        mouse.buttons,
                        tick,
                    );
                }
            }
            Action::None
        })();
        PointerActions {
            wheel,
            secondary,
            primary,
        }
    }
}

#[cfg(test)]
#[path = "../../../Tests/RustNative/pointer_router.rs"]
mod tests;
