//! Current two-slot quick-launch policy and geometry. Registry loading and
//! painting borrow this owner's entries; launch data uses the menu contract.
use super::{
    configuration::bytes_z,
    start_menu::{Kind as EntryKind, Launch, Policy},
    style,
};
use crate::{input::event::Target, render::geometry::Rect};
pub const CAPACITY: usize = 2;
pub const BUTTON_W: i32 = 24;
pub const BUTTON_H: i32 = 22;
pub const BUTTON_GAP: i32 = 2;
pub const LEFT_GAP: i32 = 4;
pub const RIGHT_GAP: i32 = 8;
pub const SEPARATOR_W: i32 = 4;
pub const ICON_SIZE: u16 = 16;
pub const REGISTRY_ROOT: &[u8] = b"SYSTEM\\Shell\\Taskbar\\QuickLaunch";
pub fn registry_key(index: usize) -> &'static [u8] {
    match index {
        0 => b"SYSTEM\\Shell\\Taskbar\\QuickLaunch\\Item0",
        1 => b"SYSTEM\\Shell\\Taskbar\\QuickLaunch\\Item1",
        _ => b"",
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    ShowDesktop,
    Program,
}
#[derive(Clone, Copy)]
pub struct Item {
    pub kind: Kind,
    pub policy: Policy,
    title: [u8; 32],
    path: [u8; 64],
    args: [u8; 128],
    icon: [u8; 64],
}
impl Default for Item {
    fn default() -> Self {
        Self {
            kind: Kind::Program,
            policy: Policy::Auto,
            title: [0; 32],
            path: [0; 64],
            args: [0; 128],
            icon: [0; 64],
        }
    }
}
fn copy_z(out: &mut [u8], bytes: &[u8]) {
    out.fill(0);
    if out.is_empty() {
        return;
    }
    let n = bytes.len().min(out.len() - 1);
    out[..n].copy_from_slice(&bytes[..n]);
}
impl Item {
    pub fn title(&self) -> &[u8] {
        bytes_z(&self.title)
    }
    pub fn path(&self) -> &[u8] {
        bytes_z(&self.path)
    }
    pub fn args(&self) -> &[u8] {
        bytes_z(&self.args)
    }
    pub fn icon(&self) -> &[u8] {
        bytes_z(&self.icon)
    }
    pub fn set_title(&mut self, value: &[u8]) {
        copy_z(&mut self.title, value)
    }
    pub fn set_path(&mut self, value: &[u8]) {
        copy_z(&mut self.path, value)
    }
    pub fn set_args(&mut self, value: &[u8]) {
        copy_z(&mut self.args, value)
    }
    pub fn set_icon(&mut self, value: &[u8]) {
        copy_z(&mut self.icon, value)
    }
    pub fn valid(&self) -> bool {
        !self.title().is_empty()
            && match self.kind {
                Kind::ShowDesktop => self.policy == Policy::Action,
                Kind::Program => !self.path().is_empty() && self.policy != Policy::Action,
            }
    }
    pub fn set_kind_text(&mut self, value: &[u8]) -> bool {
        if value.eq_ignore_ascii_case(b"show_desktop") || value.eq_ignore_ascii_case(b"desktop") {
            self.kind = Kind::ShowDesktop;
            self.policy = Policy::Action;
            true
        } else if value.eq_ignore_ascii_case(b"program") || value.eq_ignore_ascii_case(b"app") {
            self.kind = Kind::Program;
            if self.policy == Policy::Action {
                self.policy = Policy::Auto
            }
            true
        } else {
            false
        }
    }
    pub fn set_policy_text(&mut self, value: &[u8]) -> bool {
        let Some(policy) = Policy::parse_checked(value) else {
            return false;
        };
        self.policy = policy;
        true
    }
    pub fn launch(&self, target: Target) -> Launch<'_> {
        Launch {
            target,
            kind: if self.kind == Kind::ShowDesktop {
                EntryKind::Action
            } else {
                EntryKind::Item
            },
            policy: if self.kind == Kind::ShowDesktop {
                Policy::Action
            } else {
                self.policy
            },
            title: self.title(),
            path: self.path(),
            args: self.args(),
        }
    }
}
pub struct Bar {
    items: [Item; CAPACITY],
    count: usize,
}
impl Bar {
    pub fn defaults() -> Self {
        let mut items = [Item::default(); 2];
        items[0].kind = Kind::ShowDesktop;
        items[0].policy = Policy::Action;
        items[0].set_title(b"Desktop anzeigen");
        items[1].policy = Policy::Gui;
        items[1].set_title(b"Computer");
        items[1].set_path(b"/R4OS/SOFTWARE/DESKTOP/EXPLORER.R4X");
        items[1].set_icon(b"/R4OS/Media/Icons/Folder.ico");
        Self { items, count: 2 }
    }
    pub fn count(&self) -> usize {
        self.count
    }
    pub fn items(&self) -> &[Item] {
        &self.items[..self.count]
    }
    pub fn item_mut(&mut self, index: usize) -> Option<&mut Item> {
        self.items.get_mut(index)
    }
    pub fn set_count(&mut self, count: u32) {
        self.count = (count as usize).min(CAPACITY)
    }
    pub fn target(&self, index: usize) -> Target {
        target_for_index(index)
    }
    pub fn launch(&self, index: usize) -> Option<Launch<'_>> {
        Some(self.items().get(index)?.launch(target_for_index(index)))
    }
    pub fn hit(&self, height: i32, x: i32, y: i32) -> Option<usize> {
        (0..self.count).find(|&index| button_rect(height, index).contains(x, y))
    }
}
pub fn target_for_index(index: usize) -> Target {
    match index {
        0 => Target::QuickShowDesktop,
        1 => Target::QuickComputer,
        _ => Target::None,
    }
}
pub fn index_for_target(target: Target) -> Option<usize> {
    match target {
        Target::QuickShowDesktop => Some(0),
        Target::QuickComputer => Some(1),
        _ => None,
    }
}
pub fn bar_x() -> i32 {
    style::START_W + 6
}
pub fn bar_width(count: usize) -> i32 {
    if count == 0 {
        return 0;
    }
    let visible = count.min(CAPACITY) as i32;
    LEFT_GAP + visible * BUTTON_W + (visible - 1) * BUTTON_GAP + RIGHT_GAP + SEPARATOR_W
}
pub fn bar_rect(height: i32, count: usize) -> Rect {
    Rect::new(
        bar_x(),
        height - style::TASKBAR_H + 4,
        bar_width(count),
        style::START_H,
    )
}
pub fn button_rect(height: i32, index: usize) -> Rect {
    Rect::new(
        bar_x() + LEFT_GAP + index as i32 * (BUTTON_W + BUTTON_GAP),
        height - style::TASKBAR_H + 5,
        BUTTON_W,
        BUTTON_H,
    )
}
pub fn separator_x(count: usize) -> i32 {
    bar_x() + bar_width(count) - SEPARATOR_W + 1
}
pub fn taskbar_window_start_x(count: usize) -> i32 {
    style::START_W + 10 + bar_width(count)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_default_desktop_and_computer() {
        let bar = Bar::defaults();
        assert_eq!(bar.count(), 2);
        assert_eq!(bar.items()[0].kind, Kind::ShowDesktop);
        assert_eq!(bar.items()[1].kind, Kind::Program);
        assert_eq!(bar.target(0), Target::QuickShowDesktop);
        assert_eq!(bar.target(1), Target::QuickComputer);
        assert_eq!(bar.items()[0].title(), b"Desktop anzeigen");
        assert_eq!(
            bar.items()[1].path(),
            b"/R4OS/SOFTWARE/DESKTOP/EXPLORER.R4X"
        );
        assert_eq!(bar.items()[1].icon(), b"/R4OS/Media/Icons/Folder.ico");
    }
    #[test]
    fn original_hit_taskbar_geometry() {
        let bar = Bar::defaults();
        let first = button_rect(720, 0);
        let second = button_rect(720, 1);
        assert_eq!(bar.hit(720, first.x + 2, first.y + 2), Some(0));
        assert_eq!(bar.hit(720, second.x + 2, second.y + 2), Some(1));
        assert_eq!(bar.hit(720, first.x - 1, first.y + 2), None);
        assert!(taskbar_window_start_x(bar.count()) > style::START_W + 10);
    }
    #[test]
    fn original_registry_kind_and_policy() {
        let mut item = Item::default();
        assert!(item.set_kind_text(b"show_desktop"));
        assert_eq!(item.kind, Kind::ShowDesktop);
        assert_eq!(item.policy, Policy::Action);
        assert!(item.set_kind_text(b"program"));
        assert!(item.set_policy_text(b"gui"));
        assert_eq!(item.kind, Kind::Program);
        assert_eq!(item.policy, Policy::Gui);
        assert!(!item.set_policy_text(b"unknown"));
        assert_eq!(item.policy, Policy::Gui);
    }
    #[test]
    fn original_reject_broken_registry_entries() {
        let mut item = Item::default();
        item.set_title(b"Computer");
        item.set_path(b"/R4OS/SOFTWARE/DESKTOP/EXPLORER.R4X");
        item.policy = Policy::Gui;
        assert!(item.valid());
        item.set_path(b"");
        assert!(!item.valid());
        item.set_path(b"/R4OS/SOFTWARE/DESKTOP/EXPLORER.R4X");
        item.policy = Policy::Action;
        assert!(!item.valid());
        item.kind = Kind::ShowDesktop;
        assert!(item.valid());
    }
}
