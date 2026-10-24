//! Interactive selection over the existing MENU.R4S model. Drawing reads one
//! copied view; keyboard and pointer handling share the same selection state.
use super::start_menu::Menu;
use crate::input::event::Target;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Root,
    Submenu,
    Nested,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub open: bool,
    pub selected: usize,
    pub submenu_open: bool,
    pub submenu_parent: usize,
    pub submenu_selected: usize,
    pub nested_open: bool,
    pub nested_parent: usize,
    pub nested_selected: usize,
    pub focus: Focus,
}
#[derive(Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}
#[derive(Default)]
pub struct Controller {
    view: View,
}
impl Controller {
    pub fn view(&self) -> View {
        self.view
    }
    pub fn toggle(&mut self) {
        self.set_open(!self.view.open);
    }
    pub fn set_open(&mut self, open: bool) {
        self.view.open = open;
        self.view.submenu_open = false;
        self.view.submenu_parent = self.view.selected;
        self.view.submenu_selected = 0;
        self.view.nested_open = false;
        self.view.focus = Focus::Root;
    }
    pub fn selected_target(&self, menu: &Menu) -> Target {
        if !self.view.open {
            return Target::None;
        }
        let v = self.view;
        match v.focus {
            Focus::Nested if v.nested_open => {
                menu.nested_target(v.submenu_parent, v.nested_parent, v.nested_selected)
            }
            Focus::Submenu if v.submenu_open => {
                menu.submenu_target(v.submenu_parent, v.submenu_selected)
            }
            _ => menu.target(v.selected),
        }
    }
    fn open_submenu(&mut self, menu: &Menu, parent: usize, focus: bool) -> bool {
        if !menu.has_submenu(parent) {
            return false;
        }
        self.view.submenu_open = true;
        self.view.submenu_parent = parent;
        self.view.submenu_selected = 0;
        self.view.focus = if focus { Focus::Submenu } else { Focus::Root };
        self.view.nested_open = false;
        true
    }
    fn sync_nested(&mut self, menu: &Menu) {
        let v = &mut self.view;
        if menu.submenu_has_submenu(v.submenu_parent, v.submenu_selected) {
            v.nested_open = true;
            v.nested_parent = v.submenu_selected;
            v.nested_selected = 0;
        } else {
            v.nested_open = false;
        }
    }
    pub fn navigate(&mut self, menu: &Menu, direction: Direction) -> bool {
        if !self.view.open {
            return false;
        }
        match direction {
            Direction::Up | Direction::Down => {
                let up = matches!(direction, Direction::Up);
                let v = &mut self.view;
                if v.focus == Focus::Nested && v.nested_open {
                    let Some(nested) = menu.nested_submenu(v.submenu_parent, v.nested_parent)
                    else {
                        return false;
                    };
                    return step(&mut v.nested_selected, nested.entries().len(), up);
                }
                if v.focus == Focus::Submenu && v.submenu_open {
                    let Some(submenu) = menu.submenu(v.submenu_parent) else {
                        return false;
                    };
                    if !step(&mut v.submenu_selected, submenu.entries().len(), up) {
                        return false;
                    }
                    self.sync_nested(menu);
                    return true;
                }
                if !step(&mut v.selected, menu.entries().len(), up) {
                    return false;
                }
                v.focus = Focus::Root;
                v.nested_open = false;
                let parent = v.selected;
                if !self.open_submenu(menu, parent, false) {
                    self.view.submenu_open = false;
                }
                true
            }
            Direction::Left => {
                let v = &mut self.view;
                match v.focus {
                    Focus::Nested => {
                        v.focus = Focus::Submenu;
                        v.submenu_selected = v.nested_parent;
                    }
                    Focus::Submenu => {
                        v.focus = Focus::Root;
                        v.nested_open = false;
                        v.selected = v.submenu_parent;
                    }
                    Focus::Root => return false,
                }
                true
            }
            Direction::Right => {
                let v = &mut self.view;
                match v.focus {
                    Focus::Nested => false,
                    Focus::Submenu => {
                        if !menu.submenu_has_submenu(v.submenu_parent, v.submenu_selected) {
                            return false;
                        }
                        v.nested_open = true;
                        v.nested_parent = v.submenu_selected;
                        v.nested_selected = 0;
                        v.focus = Focus::Nested;
                        true
                    }
                    Focus::Root => self.open_submenu(menu, self.view.selected, true),
                }
            }
        }
    }
    pub fn hover(&mut self, menu: &Menu, target: Target) -> bool {
        if !self.view.open {
            return false;
        }
        let before = self.view;
        // The original hit resolution gives nested items precedence over
        // submenu items, then root items with the same command identity.
        for parent in 0..menu.entries().len() {
            let Some(submenu) = menu.submenu(parent) else {
                continue;
            };
            for child in 0..submenu.entries().len() {
                let Some(nested) = menu.nested_submenu(parent, child) else {
                    continue;
                };
                if let Some(index) = nested
                    .entries()
                    .iter()
                    .position(|entry| entry.target() == target)
                {
                    self.view.submenu_open = true;
                    self.view.submenu_parent = parent;
                    self.view.submenu_selected = child;
                    self.view.nested_open = true;
                    self.view.nested_parent = child;
                    self.view.nested_selected = index;
                    self.view.focus = Focus::Nested;
                    return self.view != before;
                }
            }
        }
        for parent in 0..menu.entries().len() {
            let Some(submenu) = menu.submenu(parent) else {
                continue;
            };
            if let Some(index) = submenu
                .entries()
                .iter()
                .position(|entry| entry.target() == target)
            {
                self.view.submenu_open = true;
                self.view.submenu_parent = parent;
                self.view.submenu_selected = index;
                self.view.focus = Focus::Submenu;
                self.sync_nested(menu);
                return self.view != before;
            }
        }
        if let Some(index) = menu
            .entries()
            .iter()
            .position(|entry| entry.target() == target)
        {
            self.view.selected = index;
            self.view.focus = Focus::Root;
            self.view.nested_open = false;
            if !self.open_submenu(menu, index, false) {
                self.view.submenu_open = false;
            }
        }
        self.view != before
    }
}
fn step(index: &mut usize, count: usize, up: bool) -> bool {
    if count == 0 {
        return false;
    }
    *index = if up {
        if *index == 0 { count - 1 } else { *index - 1 }
    } else {
        (*index + 1) % count
    };
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_keyboard_and_hover_share_three_level_menu_selection() {
        let menu = Menu::defaults();
        let mut state = Controller::default();
        assert!(!state.navigate(&menu, Direction::Down));
        state.toggle();
        assert_eq!(state.selected_target(&menu), Target::MenuUpdate);
        state.navigate(&menu, Direction::Down);
        assert_eq!(state.selected_target(&menu), Target::MenuPrograms);
        assert!(state.view().submenu_open);
        state.navigate(&menu, Direction::Right);
        assert_eq!(state.view().focus, Focus::Submenu);
        state.hover(&menu, Target::MenuProgramsInternet);
        assert_eq!(state.selected_target(&menu), Target::MenuProgramsInternet);
        assert!(state.view().nested_open);
        state.navigate(&menu, Direction::Right);
        assert_eq!(state.selected_target(&menu), Target::MenuKlickifax);
        assert_eq!(state.view().focus, Focus::Nested);
        state.navigate(&menu, Direction::Left);
        assert_eq!(state.selected_target(&menu), Target::MenuProgramsInternet);
        assert!(state.view().nested_open);
        state.navigate(&menu, Direction::Left);
        assert_eq!(state.selected_target(&menu), Target::MenuPrograms);
        assert!(!state.view().nested_open);
        state.hover(&menu, Target::MenuKlickifax);
        assert_eq!(state.view().focus, Focus::Nested);
        state.set_open(false);
        assert_eq!(state.selected_target(&menu), Target::None);
        state.toggle();
        assert_eq!(state.view().focus, Focus::Root);
    }
    #[test]
    fn current_empty_menu_and_root_wrap_preserve_bounded_navigation() {
        let mut state = Controller::default();
        state.toggle();
        assert!(!state.navigate(&Menu::default(), Direction::Up));
        let menu = Menu::defaults();
        state.navigate(&menu, Direction::Up);
        assert_eq!(state.selected_target(&menu), Target::MenuScreenOff);
        state.navigate(&menu, Direction::Down);
        assert_eq!(state.selected_target(&menu), Target::MenuUpdate);
        assert!(!state.hover(&menu, Target::None));
    }
}
