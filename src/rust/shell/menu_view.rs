//! MENU.R4S views use the same model and selection as keyboard/pointer routing.
use super::{
    menu_controller::View,
    start_menu::{self, Menu, Submenu},
    style as s,
};
use crate::{
    input::event::Target,
    render::{geometry::Rect, primitive_assets::Result, text::cache::Fonts, ui::Ui},
};
pub fn paint(
    ui: &mut Ui<'_, '_, impl Fonts>,
    width: i32,
    height: i32,
    menu: &Menu,
    view: View,
    hover: Target,
    pressed: Target,
) -> Result<()> {
    if !view.open {
        return Ok(());
    }
    let panel = menu.root_rect(height);
    ui.fill(panel, s::WINDOW_BG)?;
    ui.bevel(panel, false)?;
    ui.rect(4, panel.y + 4, 28, panel.h - 8, s::TITLE_ACTIVE)?;
    for (i, entry) in menu.entries().iter().enumerate() {
        let target = entry.target();
        let down = target == pressed;
        let selected = i == view.selected || down || target == hover;
        let y = menu.item_y(height, i);
        if menu.has_separator(i) {
            let line = y - s::MENU_SEPARATOR_H / 2 - 1;
            ui.rect(40, line, s::MENU_W - 48, 1, s::TASKBAR_DARK)?;
            ui.rect(40, line + 1, s::MENU_W - 48, 1, s::TASKBAR_LIGHT)?;
        }
        let (fg, bg) = if selected {
            (s::TITLE_TEXT, s::SELECT_BG)
        } else {
            (s::TEXT, s::WINDOW_BG)
        };
        if selected {
            let rect = Rect::new(34, y, s::MENU_W - 40, s::MENU_ITEM_H);
            ui.fill(rect, bg)?;
            if down {
                ui.bevel(rect, true)?;
            }
        }
        ui.text(44, y + 4, entry.title(), fg, bg)?;
        if menu.has_submenu(i) {
            ui.text(s::MENU_W - 18, y + 4, b">", fg, bg)?;
        }
    }
    if view.submenu_open {
        if let (Some(rect), Some(submenu)) = (
            menu.submenu_rect(width, height, view.submenu_parent),
            menu.submenu(view.submenu_parent),
        ) {
            submenu_view(
                ui,
                rect,
                submenu,
                view.submenu_selected,
                hover,
                pressed,
                |index| menu.submenu_has_submenu(view.submenu_parent, index),
            )?;
        }
        if view.nested_open {
            if let (Some(rect), Some(nested)) = (
                menu.nested_rect(width, height, view.submenu_parent, view.nested_parent),
                menu.nested_submenu(view.submenu_parent, view.nested_parent),
            ) {
                submenu_view(
                    ui,
                    rect,
                    nested,
                    view.nested_selected,
                    hover,
                    pressed,
                    |_| false,
                )?;
            }
        }
    }
    Ok(())
}
fn submenu_view(
    ui: &mut Ui<'_, '_, impl Fonts>,
    rect: Rect,
    menu: &Submenu,
    selection: usize,
    hover: Target,
    pressed: Target,
    has_children: impl Fn(usize) -> bool,
) -> Result<()> {
    ui.fill(rect, s::WINDOW_BG)?;
    ui.bevel(rect, false)?;
    for (index, entry) in menu.entries().iter().enumerate() {
        let down = entry.target() == pressed;
        let active = index == selection || down || entry.target() == hover;
        let y = start_menu::submenu_item_y(rect, index);
        let (fg, bg) = if active {
            (s::TITLE_TEXT, s::SELECT_BG)
        } else {
            (s::TEXT, s::WINDOW_BG)
        };
        if active {
            let row = Rect::new(rect.x + 3, y, rect.w - 6, s::MENU_ITEM_H);
            ui.fill(row, bg)?;
            if down {
                ui.bevel(row, true)?;
            }
        }
        ui.text(rect.x + 12, y + 4, entry.title(), fg, bg)?;
        if menu.id() != start_menu::SubmenuId::ShutDown {
            ui.text(rect.right() - 70, y + 4, entry.policy_label(), fg, bg)?;
        }
        if has_children(index) {
            ui.text(rect.right() - 18, y + 4, b">", fg, bg)?;
        }
    }
    Ok(())
}
