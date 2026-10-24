//! Bounded desktop-folder view: item identity, stable ordering and the current
//! saved-position/grid policy. File discovery and icon I/O stay with loaders.
use super::{
    configuration::bytes_z,
    desktop_layout::{Layout, Position},
};
use crate::input::event::Target;
use alloc::vec::Vec;
use core::cmp::Ordering;
use r4os_library_helpers::{raw as c, standard::Shortcut};
use r4os_sdk::abi::LaunchPolicy;
pub const CAPACITY: usize = 32;
pub const ICON_W: i32 = 72;
pub const ICON_H: i32 = 62;
pub const CELL_W: i32 = 108;
pub const CELL_H: i32 = 76;
pub const START_X: i32 = 24;
pub const START_Y: i32 = 76;
pub const FOLDER_ICON: &[u8] = b"C:\\R4OS\\Media\\Icons\\Folder.ico";
pub const EXPLORER: &[u8] = b"C:\\R4OS\\SOFTWARE\\DESKTOP\\EXPLORER.R4X";
const TARGETS: [Target; CAPACITY] = [
    Target::DesktopIcon1,
    Target::DesktopIcon2,
    Target::DesktopIcon3,
    Target::DesktopIcon4,
    Target::DesktopIcon5,
    Target::DesktopIcon6,
    Target::DesktopIcon7,
    Target::DesktopIcon8,
    Target::DesktopItem9,
    Target::DesktopItem10,
    Target::DesktopItem11,
    Target::DesktopItem12,
    Target::DesktopItem13,
    Target::DesktopItem14,
    Target::DesktopItem15,
    Target::DesktopItem16,
    Target::DesktopItem17,
    Target::DesktopItem18,
    Target::DesktopItem19,
    Target::DesktopItem20,
    Target::DesktopItem21,
    Target::DesktopItem22,
    Target::DesktopItem23,
    Target::DesktopItem24,
    Target::DesktopItem25,
    Target::DesktopItem26,
    Target::DesktopItem27,
    Target::DesktopItem28,
    Target::DesktopItem29,
    Target::DesktopItem30,
    Target::DesktopItem31,
    Target::DesktopItem32,
];
pub fn target_for_index(index: usize) -> Target {
    TARGETS.get(index).copied().unwrap_or(Target::None)
}
pub fn index_for_target(target: Target) -> Option<usize> {
    let n = (target as u16).checked_sub(Target::DesktopIcon1 as u16)?;
    (n < 32).then_some(n as usize)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsKind {
    File,
    Directory,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory,
    Program,
    Shortcut,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchKind {
    File,
    Directory,
    Program,
}
#[derive(Clone, Copy)]
pub struct Item {
    target: Target,
    pub kind: Kind,
    pub launch_kind: LaunchKind,
    pub policy: LaunchPolicy,
    pub x: i32,
    pub y: i32,
    title: [u8; 32],
    path: [u8; 160],
    launch_path: [u8; 160],
    args: [u8; 128],
    icon: [u8; 128],
}
fn copy_z(out: &mut [u8], value: &[u8]) {
    out.fill(0);
    if out.is_empty() {
        return;
    }
    let count = value.len().min(out.len() - 1);
    out[..count].copy_from_slice(&value[..count]);
}
fn ends(value: &[u8], suffix: &[u8]) -> bool {
    value.len() >= suffix.len() && value[value.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}
fn title(path: &[u8], kind: Kind) -> &[u8] {
    let start = path
        .iter()
        .rposition(|&b| b == b'\\' || b == b'/')
        .map_or(0, |i| i + 1);
    let tail = &path[start..];
    if (kind == Kind::Shortcut && ends(tail, b".LNK"))
        || (kind == Kind::Program && ends(tail, b".R4X"))
    {
        &tail[..tail.len() - 4]
    } else {
        tail
    }
}
impl Item {
    pub fn new(path: &[u8], fs: FsKind) -> Self {
        let kind = if fs == FsKind::Directory {
            Kind::Directory
        } else if ends(path, b".R4X") {
            Kind::Program
        } else {
            Kind::File
        };
        let launch_kind = match kind {
            Kind::Directory => LaunchKind::Directory,
            Kind::Program => LaunchKind::Program,
            _ => LaunchKind::File,
        };
        let mut item = Self {
            target: Target::None,
            kind,
            launch_kind,
            policy: LaunchPolicy::auto,
            x: 0,
            y: 0,
            title: [0; 32],
            path: [0; 160],
            launch_path: [0; 160],
            args: [0; 128],
            icon: [0; 128],
        };
        copy_z(&mut item.path, path);
        copy_z(&mut item.launch_path, path);
        copy_z(&mut item.title, title(path, kind));
        if kind == Kind::Directory {
            copy_z(&mut item.icon, FOLDER_ICON)
        }
        item
    }
    pub fn target(&self) -> Target {
        self.target
    }
    pub fn title(&self) -> &[u8] {
        bytes_z(&self.title)
    }
    pub fn path(&self) -> &[u8] {
        bytes_z(&self.path)
    }
    pub fn launch_path(&self) -> &[u8] {
        bytes_z(&self.launch_path)
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
    pub fn set_launch_path(&mut self, value: &[u8]) {
        copy_z(&mut self.launch_path, value)
    }
    pub fn set_args(&mut self, value: &[u8]) {
        copy_z(&mut self.args, value)
    }
    pub fn set_icon(&mut self, value: &[u8]) {
        copy_z(&mut self.icon, value)
    }
    pub fn apply_shortcut(&mut self, link: &Shortcut<'_>) -> bool {
        self.kind = Kind::Shortcut;
        let Ok(resolved) = link.resolve() else {
            return false;
        };
        self.launch_kind = match resolved.kind {
            c::R4StdShortcutTargetKind_program => LaunchKind::Program,
            c::R4StdShortcutTargetKind_directory => LaunchKind::Directory,
            c::R4StdShortcutTargetKind_file => LaunchKind::File,
            _ => return false,
        };
        self.policy = LaunchPolicy(resolved.policy);
        copy_z(&mut self.launch_path, resolved.target);
        copy_z(&mut self.args, resolved.args);
        if !resolved.icon.is_empty() {
            copy_z(&mut self.icon, resolved.icon)
        }
        if !resolved.title.is_empty() {
            copy_z(&mut self.title, resolved.title)
        } else {
            let path = self.path;
            copy_z(&mut self.title, title(bytes_z(&path), Kind::Shortcut))
        }
        true
    }
}
pub struct Items {
    entries: Vec<Item>,
    pub truncated: bool,
}
impl Items {
    pub fn new() -> Result<Self, alloc::collections::TryReserveError> {
        let mut entries = Vec::new();
        entries.try_reserve_exact(CAPACITY)?;
        Ok(Self {
            entries,
            truncated: false,
        })
    }
    pub fn entries(&self) -> &[Item] {
        &self.entries
    }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Item> {
        self.entries.get_mut(index)
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.truncated = false
    }
    pub fn add(&mut self, mut item: Item) -> bool {
        if self.entries.len() == CAPACITY {
            self.truncated = true;
            return false;
        }
        item.target = target_for_index(self.entries.len());
        self.entries.push(item);
        true
    }
    pub fn sort_by_title(&mut self) {
        fn rank(kind: Kind) -> u8 {
            match kind {
                Kind::Directory => 0,
                Kind::Shortcut => 1,
                Kind::Program => 2,
                Kind::File => 3,
            }
        }
        fn cmp(a: &Item, b: &Item) -> Ordering {
            rank(a.kind).cmp(&rank(b.kind)).then_with(|| {
                a.title()
                    .iter()
                    .map(u8::to_ascii_uppercase)
                    .cmp(b.title().iter().map(u8::to_ascii_uppercase))
            })
        }
        for i in 1..self.entries.len() {
            let value = self.entries[i];
            let mut j = i;
            while j > 0 && cmp(&value, &self.entries[j - 1]) == Ordering::Less {
                self.entries[j] = self.entries[j - 1];
                j -= 1;
            }
            self.entries[j] = value;
        }
        for (i, item) in self.entries.iter_mut().enumerate() {
            item.target = target_for_index(i);
        }
    }
    pub fn layout(&mut self, width: i32, height: i32, bar: i32) {
        self.layout_with(&Layout::default(), width, height, bar)
    }
    pub fn layout_with(&mut self, saved: &Layout, width: i32, height: i32, bar: i32) {
        let grid = Grid::new(width, height, bar);
        let mut occupied = [Position::default(); CAPACITY];
        for (i, item) in self.entries.iter_mut().enumerate() {
            let desired = saved
                .position(item.path())
                .unwrap_or_else(|| grid.auto_position(i));
            let position = grid.first_free(desired, &occupied[..i]);
            item.x = position.x;
            item.y = position.y;
            occupied[i] = position;
        }
    }
    pub fn snap_preview(
        &self,
        index: usize,
        width: i32,
        height: i32,
        bar: i32,
    ) -> Option<Position> {
        let item = self.entries.get(index)?;
        let mut occupied = [Position::default(); CAPACITY];
        let mut count = 0;
        for (i, entry) in self.entries.iter().enumerate() {
            if i != index {
                occupied[count] = Position {
                    x: entry.x,
                    y: entry.y,
                };
                count += 1;
            }
        }
        Some(Grid::new(width, height, bar).first_free(
            Position {
                x: item.x,
                y: item.y,
            },
            &occupied[..count],
        ))
    }
    pub fn snap(&mut self, index: usize, width: i32, height: i32, bar: i32) {
        if let Some(position) = self.snap_preview(index, width, height, bar) {
            self.entries[index].x = position.x;
            self.entries[index].y = position.y;
        }
    }
    pub fn hit(&self, x: i32, y: i32) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| x >= e.x && x < e.x + ICON_W && y >= e.y && y < e.y + ICON_H)
    }
    pub fn target(&self, index: usize) -> Target {
        self.entries.get(index).map_or(Target::None, |e| e.target)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub row: i32,
    pub column: i32,
}
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    rows: usize,
    max_x: i32,
    max_y: i32,
}
impl Grid {
    /// Input dimensions belong to the admitted Desktop/output domain.
    pub fn new(width: i32, height: i32, bar: i32) -> Self {
        let bottom = (START_Y + CELL_H).max(height - bar - 8);
        Self {
            rows: ((bottom - START_Y) / CELL_H).max(1) as usize,
            max_x: 0.max(width - ICON_W),
            max_y: 0.max(height - bar - ICON_H),
        }
    }
    pub fn auto_position(self, index: usize) -> Position {
        self.position_for_cell(Cell {
            row: (index % self.rows) as i32,
            column: (index / self.rows) as i32,
        })
    }
    pub fn first_free(self, desired: Position, occupied: &[Position]) -> Position {
        let cell = self.cell_for_position(desired);
        let mut linear = cell.column.max(0) as usize * self.rows + cell.row.max(0) as usize;
        for _ in 0..CAPACITY * 2 {
            let candidate = self.auto_position(linear);
            if !occupied.contains(&candidate) {
                return candidate;
            }
            linear += 1;
        }
        self.position_for_cell(self.cell_for_position(desired))
    }
    pub fn cell_for_position(self, p: Position) -> Cell {
        let x = p.x.clamp(0, self.max_x);
        let y = p.y.clamp(0, self.max_y);
        Cell {
            row: ((y - START_Y).max(0) / CELL_H).min((self.rows - 1) as i32),
            column: (x - START_X).max(0) / CELL_W,
        }
    }
    pub fn position_for_cell(self, c: Cell) -> Position {
        Position {
            x: (START_X + c.column * CELL_W).clamp(0, self.max_x),
            y: (START_Y + c.row * CELL_H).clamp(0, self.max_y),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn items(names: &[&[u8]]) -> Items {
        let mut items = Items::new().unwrap();
        for name in names {
            assert!(items.add(Item::new(name, FsKind::File)));
        }
        items
    }
    #[test]
    fn original_more_than_eight_stable_targets() {
        assert!(CAPACITY > 8);
        assert_eq!(target_for_index(0), Target::DesktopIcon1);
        assert_eq!(target_for_index(8), Target::DesktopItem9);
        assert_eq!(target_for_index(31), Target::DesktopItem32);
        assert_eq!(index_for_target(Target::DesktopItem32), Some(31));
        for i in 0..32 {
            assert_eq!(index_for_target(target_for_index(i)), Some(i));
        }
        assert_eq!(index_for_target(Target::None), None);
    }
    #[test]
    fn original_directory_sort_and_layout() {
        let mut items = Items::new().unwrap();
        items.add(Item::new(b"C:\\R4OS\\DESKTOP\\ZETA.TXT", FsKind::File));
        items.add(Item::new(b"C:\\R4OS\\DESKTOP\\TOOLS", FsKind::Directory));
        items.add(Item::new(b"C:\\R4OS\\DESKTOP\\APP.R4X", FsKind::File));
        items.sort_by_title();
        items.layout(1280, 720, 28);
        assert_eq!(items.entries()[0].kind, Kind::Directory);
        assert_eq!(items.entries()[0].title(), b"TOOLS");
        assert_eq!(items.entries()[1].kind, Kind::Program);
        assert_eq!(items.entries()[1].title(), b"APP");
        assert_eq!(items.hit(START_X + 4, START_Y + 4), Some(0));
    }
    #[test]
    fn original_restore_and_collision_next_cell() {
        let mut items = items(&[
            b"C:\\R4OS\\DESKTOP\\ALPHA.LNK",
            b"C:\\R4OS\\DESKTOP\\BETA.LNK",
            b"C:\\R4OS\\DESKTOP\\GAMMA.LNK",
        ]);
        let mut saved = Layout::default();
        saved.add(items.entries[0].path(), START_X, START_Y);
        saved.add(items.entries[1].path(), START_X, START_Y);
        items.layout_with(&saved, 1280, 720, 28);
        assert_eq!((items.entries[0].x, items.entries[0].y), (START_X, START_Y));
        assert_eq!(
            (items.entries[1].x, items.entries[1].y),
            (START_X, START_Y + CELL_H)
        );
        assert_ne!(
            (items.entries[2].x, items.entries[2].y),
            (items.entries[0].x, items.entries[0].y)
        );
    }
    #[test]
    fn original_saved_position_clamped_visible() {
        let mut items = items(&[b"C:\\R4OS\\DESKTOP\\ALPHA.LNK"]);
        let mut saved = Layout::default();
        saved.add(items.entries[0].path(), -500, 5000);
        items.layout_with(&saved, 320, 240, 28);
        assert!((0..=320 - ICON_W).contains(&items.entries[0].x));
        assert!((0..=240 - 28 - ICON_H).contains(&items.entries[0].y));
    }
    #[test]
    fn original_resize_visible_and_separated() {
        let mut items = items(&[
            b"C:\\R4OS\\DESKTOP\\ALPHA.LNK",
            b"C:\\R4OS\\DESKTOP\\BETA.LNK",
            b"C:\\R4OS\\DESKTOP\\GAMMA.LNK",
            b"C:\\R4OS\\DESKTOP\\DELTA.LNK",
        ]);
        let mut saved = Layout::default();
        saved.add(items.entries[0].path(), 4000, 4000);
        saved.add(items.entries[1].path(), 4000, 4000);
        saved.add(items.entries[2].path(), -200, -200);
        items.layout_with(&saved, 400, 340, 28);
        for (i, item) in items.entries.iter().enumerate() {
            assert!((0..=400 - ICON_W).contains(&item.x));
            assert!((0..=340 - 28 - ICON_H).contains(&item.y));
            for other in &items.entries[i + 1..] {
                assert_ne!((item.x, item.y), (other.x, other.y));
            }
        }
    }
    #[test]
    fn original_snap_keeps_other_items() {
        let mut items = items(&[
            b"C:\\R4OS\\DESKTOP\\ALPHA.LNK",
            b"C:\\R4OS\\DESKTOP\\BETA.LNK",
            b"C:\\R4OS\\DESKTOP\\GAMMA.LNK",
        ]);
        items.layout(1280, 720, 28);
        let beta = (items.entries[1].x, items.entries[1].y);
        items.entries[2].x = items.entries[0].x + 9;
        items.entries[2].y = items.entries[0].y + 7;
        items.snap(2, 1280, 720, 28);
        assert_eq!((items.entries[0].x, items.entries[0].y), (START_X, START_Y));
        assert_eq!((items.entries[1].x, items.entries[1].y), beta);
        assert_ne!((items.entries[2].x, items.entries[2].y), (START_X, START_Y));
        assert_ne!((items.entries[2].x, items.entries[2].y), beta);
    }
    #[test]
    fn original_grid_metrics_match_preview() {
        let mut items = items(&[
            b"C:\\R4OS\\DESKTOP\\ALPHA.LNK",
            b"C:\\R4OS\\DESKTOP\\BETA.LNK",
            b"C:\\R4OS\\DESKTOP\\GAMMA.LNK",
        ]);
        items.layout(1280, 720, 28);
        let grid = Grid::new(1280, 720, 28);
        assert_eq!(
            grid.auto_position(0),
            Position {
                x: START_X,
                y: START_Y
            }
        );
        assert_eq!(
            grid.auto_position(1),
            Position {
                x: START_X,
                y: START_Y + CELL_H
            }
        );
        items.entries[2].x = items.entries[0].x + 12;
        items.entries[2].y = items.entries[0].y + 10;
        let preview = items.snap_preview(2, 1280, 720, 28).unwrap();
        items.snap(2, 1280, 720, 28);
        assert_eq!(
            preview,
            Position {
                x: items.entries[2].x,
                y: items.entries[2].y
            }
        );
    }
    #[test]
    fn original_shortcut_metadata() {
        crate::standard_fixture::with(|standard| {
            let mut item = Item::new(b"C:\\R4OS\\DESKTOP\\NOTEPAD.LNK", FsKind::File);
            let mut link = standard
                .shortcut(b"C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X")
                .unwrap();
            link.set_title(b"Notepad").unwrap();
            link.set_policy(LaunchPolicy::gui).unwrap();
            link.set_icon(b"C:\\R4OS\\Media\\Icons\\Notepad.ico")
                .unwrap();
            assert!(item.apply_shortcut(&link));
            assert_eq!(item.kind, Kind::Shortcut);
            assert_eq!(item.launch_kind, LaunchKind::Program);
            assert_eq!(item.policy.0, LaunchPolicy::gui.0);
            assert_eq!(item.title(), b"Notepad");
            assert_eq!(
                item.launch_path(),
                b"C:\\R4OS\\SOFTWARE\\DESKTOP\\NOTEPAD.R4X"
            );
            assert_eq!(item.icon(), b"C:\\R4OS\\Media\\Icons\\Notepad.ico");
        });
    }
}
