//! Current bounded MENU.R4S policy, entries and popup geometry. Known launch
//! identities are shared by defaults and parsing; no program discovery/I/O.
use super::{configuration::bytes_z, style};
use crate::{
    input::event::Target,
    render::geometry::{self, Rect},
};
pub const MAX_ENTRIES: usize = 10;
pub const MAX_SUBMENUS: usize = 4;
pub const DEFAULT_MENU: &[u8] = include_bytes!("default_menu.r4s");
const LEGACY_MENU: &[u8] = include_bytes!("legacy_menu.r4s");

fn lines(bytes: &[u8]) -> impl Iterator<Item = &[u8]> + Clone {
    bytes
        .strip_prefix(b"\xef\xbb\xbf")
        .unwrap_or(bytes)
        .split(|&b| b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
}

/// These two shipped versions differ only by the retired Tasks action.
pub fn is_previous_default(bytes: &[u8]) -> bool {
    let previous = lines(LEGACY_MENU);
    lines(bytes).eq(previous.clone())
        || lines(bytes).eq(previous.filter(|line| *line != b"ACTION;title=Tasks;action=tasks"))
}
pub const MAX_CHILDREN: usize = 9;
pub const SUBMENU_W: i32 = 196;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Item,
    Action,
    Submenu,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmenuId {
    None,
    Programs,
    Internet,
    Settings,
    ShutDown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    Console,
    Gui,
    Auto,
    Action,
}
impl Policy {
    pub fn label(self) -> &'static [u8] {
        match self {
            Self::Console => b"console",
            Self::Gui => b"gui",
            Self::Auto => b"auto",
            Self::Action => b"action",
        }
    }
    pub fn parse(value: &[u8]) -> Self {
        Self::parse_checked(value).unwrap_or(Self::Auto)
    }
    pub fn parse_checked(value: &[u8]) -> Option<Self> {
        [Self::Console, Self::Gui, Self::Auto, Self::Action]
            .into_iter()
            .find(|policy| eq(value, policy.label()))
    }
}
#[derive(Clone, Copy)]
pub struct Launch<'a> {
    pub target: Target,
    pub kind: Kind,
    pub policy: Policy,
    pub title: &'a [u8],
    pub path: &'a [u8],
    pub args: &'a [u8],
}
#[derive(Clone, Copy)]
pub struct Entry {
    target: Target,
    kind: Kind,
    policy: Policy,
    submenu: SubmenuId,
    title: [u8; 32],
    path: [u8; 64],
    args: [u8; 128],
}
impl Entry {
    const EMPTY: Self = Self {
        target: Target::None,
        kind: Kind::Item,
        policy: Policy::Auto,
        submenu: SubmenuId::None,
        title: [0; 32],
        path: [0; 64],
        args: [0; 128],
    };
    fn known(
        target: Target,
        kind: Kind,
        title: &[u8],
        policy: Policy,
        path: &[u8],
        args: &[u8],
    ) -> Self {
        let mut e = Self {
            target,
            kind,
            policy,
            ..Self::EMPTY
        };
        copy_z(&mut e.title, title);
        copy_z(&mut e.path, path);
        copy_z(&mut e.args, args);
        e
    }
    fn child(target: Target, title: &[u8], id: SubmenuId) -> Self {
        let mut e = Self::known(target, Kind::Submenu, title, Policy::Action, b"", b"");
        e.submenu = id;
        e
    }
    pub fn title(&self) -> &[u8] {
        bytes_z(&self.title)
    }
    pub fn path(&self) -> &[u8] {
        bytes_z(&self.path)
    }
    pub fn args(&self) -> &[u8] {
        bytes_z(&self.args)
    }
    pub fn policy_label(&self) -> &[u8] {
        if self.kind == Kind::Submenu {
            b""
        } else {
            self.policy.label()
        }
    }
    pub fn target(&self) -> Target {
        self.target
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn submenu_id(&self) -> SubmenuId {
        self.submenu
    }
    pub fn launch(&self) -> Launch<'_> {
        Launch {
            target: self.target,
            kind: self.kind,
            policy: self.policy,
            title: self.title(),
            path: self.path(),
            args: self.args(),
        }
    }
}
struct Known {
    target: Target,
    title: &'static [u8],
    path: &'static [u8],
}
const KNOWN: [Known; 17] = [
    Known {
        target: Target::MenuTerminal,
        title: b"Terminal",
        path: b"/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X",
    },
    Known {
        target: Target::MenuNotepad,
        title: b"Notepad",
        path: b"/R4OS/SOFTWARE/DESKTOP/NOTEPAD.R4X",
    },
    Known {
        target: Target::MenuPaint,
        title: b"Paint",
        path: b"/R4OS/SOFTWARE/DESKTOP/PAINT.R4X",
    },
    Known {
        target: Target::MenuCalc,
        title: b"Calculator",
        path: b"/R4OS/SOFTWARE/DESKTOP/CALC.R4X",
    },
    Known {
        target: Target::MenuSynth,
        title: b"R4Synth",
        path: b"/R4OS/SOFTWARE/TERMINAL/SYNTH.R4X",
    },
    Known {
        target: Target::MenuDevmgr,
        title: b"Device Manager",
        path: b"/R4OS/SOFTWARE/DESKTOP/DEVMGR.R4X",
    },
    Known {
        target: Target::MenuR4code,
        title: b"R4Code",
        path: b"/SOFTWARE/R4CODE/R4CODE.R4X",
    },
    Known {
        target: Target::MenuKlickifax,
        title: b"Klickifax",
        path: b"/R4OS/SOFTWARE/INTERNET/KLICKIFAX.R4X",
    },
    Known {
        target: Target::MenuSettingsDefaultApps,
        title: b"Default Apps",
        path: b"/R4OS/SOFTWARE/DESKTOP/APPDEF.R4X",
    },
    Known {
        target: Target::MenuSettingsRegistry,
        title: b"Registry Editor",
        path: b"/R4OS/SOFTWARE/DESKTOP/REGEDIT.R4X",
    },
    Known {
        target: Target::MenuSettingsNetwork,
        title: b"Network",
        path: b"/R4OS/SOFTWARE/DESKTOP/NETCFG.R4X",
    },
    Known {
        target: Target::MenuSettingsServices,
        title: b"Services",
        path: b"/R4OS/SOFTWARE/DESKTOP/SERVICES.R4X",
    },
    Known {
        target: Target::MenuSettingsLogCenter,
        title: b"Log Center",
        path: b"/R4OS/SOFTWARE/DESKTOP/LOGCENTER.R4X",
    },
    Known {
        target: Target::MenuSettingsTime,
        title: b"Time Settings",
        path: b"/R4OS/SOFTWARE/DESKTOP/TIMESET.R4X",
    },
    Known {
        target: Target::MenuSettingsAppearance,
        title: b"Appearance",
        path: b"/R4OS/SOFTWARE/DESKTOP/APPEARANCE.R4X",
    },
    Known {
        target: Target::MenuSpectra,
        title: b"Spectra",
        path: b"/R4OS/SOFTWARE/TERMINAL/DIAG/SPECTRA.R4X",
    },
    Known {
        target: Target::MenuPortex,
        title: b"Portex",
        path: b"/R4OS/SOFTWARE/DESKTOP/PORTEX.R4X",
    },
];
impl Known {
    fn matches(&self, title: &[u8], path: &[u8]) -> bool {
        eq(title, self.title) || eq(path, self.path)
    }
}
pub struct Submenu {
    id: SubmenuId,
    entries: [Entry; MAX_CHILDREN],
    count: usize,
}
impl Submenu {
    const EMPTY: Self = Self {
        id: SubmenuId::None,
        entries: [Entry::EMPTY; MAX_CHILDREN],
        count: 0,
    };
    pub fn entries(&self) -> &[Entry] {
        &self.entries[..self.count]
    }
    pub fn id(&self) -> SubmenuId {
        self.id
    }
    fn add(&mut self, entry: Entry) {
        if self.count == MAX_CHILDREN || entry.target == Target::None {
            return;
        }
        self.entries[self.count] = entry;
        self.count += 1;
    }
    pub fn target(&self, index: usize) -> Target {
        self.entries()
            .get(index)
            .map_or(Target::None, Entry::target)
    }
    pub fn launch(&self, index: usize) -> Option<Launch<'_>> {
        Some(self.entries().get(index)?.launch())
    }
}
pub struct Menu {
    entries: [Entry; MAX_ENTRIES],
    submenus: [Submenu; MAX_SUBMENUS],
    count: usize,
    submenu_count: usize,
    separators: u16,
    pub loaded_from_config: bool,
}
impl Default for Menu {
    fn default() -> Self {
        Self {
            entries: [Entry::EMPTY; MAX_ENTRIES],
            submenus: core::array::from_fn(|_| Submenu::EMPTY),
            count: 0,
            submenu_count: 0,
            separators: 0,
            loaded_from_config: false,
        }
    }
}
impl Menu {
    pub fn defaults() -> Self {
        let mut menu = Self::default();
        menu.parse(DEFAULT_MENU);
        menu
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries[..self.count]
    }
    pub fn submenu_count(&self) -> usize {
        self.submenu_count
    }
    pub fn label(&self, index: usize) -> &[u8] {
        self.entries().get(index).map_or(b"", Entry::title)
    }
    pub fn policy_label(&self, index: usize) -> &[u8] {
        self.entries().get(index).map_or(b"", Entry::policy_label)
    }
    pub fn target(&self, index: usize) -> Target {
        self.entries()
            .get(index)
            .map_or(Target::None, Entry::target)
    }
    pub fn launch(&self, index: usize) -> Option<Launch<'_>> {
        Some(self.entries().get(index)?.launch())
    }
    pub fn submenu_by_id(&self, id: SubmenuId) -> Option<&Submenu> {
        if id == SubmenuId::None {
            return None;
        }
        self.submenus[..self.submenu_count]
            .iter()
            .find(|s| s.id == id)
    }
    pub fn submenu(&self, parent: usize) -> Option<&Submenu> {
        let entry = self.entries().get(parent)?;
        if entry.kind != Kind::Submenu {
            return None;
        }
        self.submenu_by_id(entry.submenu)
    }
    pub fn nested_submenu(&self, parent: usize, index: usize) -> Option<&Submenu> {
        let entry = self.submenu(parent)?.entries().get(index)?;
        if entry.kind != Kind::Submenu {
            return None;
        }
        self.submenu_by_id(entry.submenu)
    }
    pub fn has_submenu(&self, index: usize) -> bool {
        self.submenu(index).is_some_and(|s| s.count > 0)
    }
    pub fn submenu_target(&self, parent: usize, index: usize) -> Target {
        self.submenu(parent)
            .map_or(Target::None, |s| s.target(index))
    }
    pub fn submenu_launch(&self, parent: usize, index: usize) -> Option<Launch<'_>> {
        self.submenu(parent)?.launch(index)
    }
    pub fn nested_target(&self, parent: usize, index: usize, nested: usize) -> Target {
        self.nested_submenu(parent, index)
            .map_or(Target::None, |s| s.target(nested))
    }
    pub fn nested_launch(&self, parent: usize, index: usize, nested: usize) -> Option<Launch<'_>> {
        self.nested_submenu(parent, index)?.launch(nested)
    }
    pub fn submenu_has_submenu(&self, parent: usize, index: usize) -> bool {
        self.nested_submenu(parent, index)
            .is_some_and(|s| s.count > 0)
    }
    pub fn load(&mut self, bytes: &[u8]) -> bool {
        let mut parsed = Self::default();
        parsed.parse(bytes);
        if parsed.count == 0 {
            return false;
        }
        parsed.loaded_from_config = true;
        *self = parsed;
        true
    }
    fn parse(&mut self, bytes: &[u8]) {
        for line in lines(bytes).map(trim) {
            if !line.is_empty() && line[0] != b'#' {
                self.add_line(line);
            }
        }
    }
    fn add(&mut self, entry: Entry) {
        if self.count == MAX_ENTRIES || entry.target == Target::None {
            return;
        }
        self.entries[self.count] = entry;
        self.count += 1;
    }
    fn action(&mut self, target: Target, title: &[u8]) {
        self.add(Entry::known(
            target,
            Kind::Action,
            title,
            Policy::Action,
            b"",
            b"",
        ));
    }
    fn ensure(&mut self, id: SubmenuId) -> Option<usize> {
        if id == SubmenuId::None {
            return None;
        }
        if let Some(index) = self.submenus[..self.submenu_count]
            .iter()
            .position(|s| s.id == id)
        {
            return Some(index);
        }
        if self.submenu_count == MAX_SUBMENUS {
            return None;
        }
        let index = self.submenu_count;
        self.submenu_count += 1;
        self.submenus[index] = Submenu {
            id,
            ..Submenu::EMPTY
        };
        Some(index)
    }
    fn add_submenu(&mut self, title: &[u8], id: SubmenuId) {
        if self.count == MAX_ENTRIES || id == SubmenuId::None {
            return;
        }
        self.add(Entry::child(submenu_target(id), title, id));
        self.ensure(id);
    }
    fn add_line(&mut self, line: &[u8]) {
        if line == b"SEPARATOR" {
            if self.count > 0 && self.count < MAX_ENTRIES {
                self.separators |= 1 << self.count;
            }
        } else if line.starts_with(b"ITEM;") || line.starts_with(b"SUBITEM;") {
            let Some(title) = value(line, b"title=") else {
                return;
            };
            let path = value(line, b"path=").unwrap_or(b"");
            let args = value(line, b"args=").unwrap_or(b"");
            let policy = Policy::parse(
                value(line, b"policy=")
                    .or_else(|| value(line, b"class="))
                    .unwrap_or(b"auto"),
            );
            if line.starts_with(b"ITEM;") {
                self.add(Entry::known(
                    item_target(title, path),
                    Kind::Item,
                    title,
                    policy,
                    path,
                    args,
                ));
            } else {
                let Some(id) = value(line, b"menu=").map(submenu_id) else {
                    return;
                };
                if let Some(index) = self.ensure(id) {
                    self.submenus[index].add(Entry::known(
                        subitem_target(id, title, path),
                        Kind::Item,
                        title,
                        policy,
                        path,
                        args,
                    ));
                }
            }
        } else if line.starts_with(b"ACTION;") {
            let (Some(title), Some(action)) = (value(line, b"title="), value(line, b"action="))
            else {
                return;
            };
            self.action(action_target(action), title);
        } else if line.starts_with(b"SUBMENU;") {
            let Some(title) = value(line, b"title=") else {
                return;
            };
            let id = submenu_id(
                value(line, b"id=")
                    .or_else(|| value(line, b"menu="))
                    .unwrap_or(b""),
            );
            self.add_submenu(title, id);
        } else if line.starts_with(b"CHILDMENU;") {
            let (Some(parent), Some(child), Some(title)) = (
                value(line, b"menu="),
                value(line, b"id="),
                value(line, b"title="),
            ) else {
                return;
            };
            let (parent, child) = (submenu_id(parent), submenu_id(child));
            if let Some(index) = self.ensure(parent) {
                if child != SubmenuId::None {
                    self.submenus[index].add(Entry::child(submenu_target(child), title, child));
                }
                self.ensure(child);
            }
        } else if line.starts_with(b"SUBACTION;") {
            let (Some(id), Some(title), Some(action)) = (
                value(line, b"menu="),
                value(line, b"title="),
                value(line, b"action="),
            ) else {
                return;
            };
            let id = submenu_id(id);
            let target = action_target(action);
            if let Some(index) = self.ensure(id) {
                if id == SubmenuId::ShutDown
                    && matches!(target, Target::MenuRestart | Target::MenuPoweroff)
                {
                    self.submenus[index].add(Entry::known(
                        target,
                        Kind::Action,
                        title,
                        Policy::Action,
                        b"",
                        b"",
                    ));
                }
            }
        }
    }
    pub fn has_separator(&self, index: usize) -> bool {
        index < self.count && self.separators & (1 << index) != 0
    }
    fn separators_before(&self, end: usize) -> i32 {
        let mask = (1u16 << end.min(self.count)) - 1;
        (self.separators & mask).count_ones() as i32
    }
    pub fn root_rect(&self, height: i32) -> Rect {
        let h = 2 * style::MENU_PADDING
            + self.count as i32 * style::MENU_ITEM_H
            + self.separators_before(self.count) * style::MENU_SEPARATOR_H;
        Rect::new(0, height - style::TASKBAR_H - h, style::MENU_W, h)
    }
    pub fn item_y(&self, height: i32, index: usize) -> i32 {
        self.root_rect(height).y
            + style::MENU_PADDING
            + index.min(self.count) as i32 * style::MENU_ITEM_H
            + self.separators_before(index.saturating_add(1)) * style::MENU_SEPARATOR_H
    }
    pub fn hit(&self, height: i32, x: i32, y: i32) -> Option<usize> {
        if !self.root_rect(height).contains(x, y) {
            return None;
        }
        (0..self.count).find(|&i| {
            let top = self.item_y(height, i);
            y >= top && y < top + style::MENU_ITEM_H
        })
    }
    pub fn submenu_rect(&self, width: i32, height: i32, parent: usize) -> Option<Rect> {
        let child = self.submenu(parent)?;
        if child.count == 0 {
            return None;
        }
        Some(
            Rect::new(
                style::MENU_W - 4,
                self.item_y(height, parent) - 4,
                SUBMENU_W,
                child.count as i32 * style::MENU_ITEM_H + 8,
            )
            .clamp_inside(geometry::work_area(width, height, style::TASKBAR_H)),
        )
    }
    pub fn nested_rect(
        &self,
        width: i32,
        height: i32,
        parent: usize,
        index: usize,
    ) -> Option<Rect> {
        let child = self.nested_submenu(parent, index)?;
        if child.count == 0 {
            return None;
        }
        let first = self.submenu_rect(width, height, parent)?;
        Some(
            Rect::new(
                first.x + first.w - 4,
                submenu_item_y(first, index) - 4,
                SUBMENU_W,
                child.count as i32 * style::MENU_ITEM_H + 8,
            )
            .clamp_inside(geometry::work_area(width, height, style::TASKBAR_H)),
        )
    }
    pub fn submenu_hit(
        &self,
        width: i32,
        height: i32,
        parent: usize,
        x: i32,
        y: i32,
    ) -> Option<usize> {
        let child = self.submenu(parent)?;
        let rect = self.submenu_rect(width, height, parent)?;
        if !rect.contains(x, y) {
            return None;
        }
        row_hit(child.count, rect, y)
    }
    pub fn nested_hit(
        &self,
        width: i32,
        height: i32,
        parent: usize,
        index: usize,
        x: i32,
        y: i32,
    ) -> Option<usize> {
        let child = self.nested_submenu(parent, index)?;
        let rect = self.nested_rect(width, height, parent, index)?;
        if !rect.contains(x, y) {
            return None;
        }
        row_hit(child.count, rect, y)
    }
}
fn row_hit(count: usize, rect: Rect, y: i32) -> Option<usize> {
    (0..count).find(|&i| {
        let top = submenu_item_y(rect, i);
        y >= top && y < top + style::MENU_ITEM_H
    })
}
pub fn submenu_item_y(rect: Rect, index: usize) -> i32 {
    rect.y + 4 + index as i32 * style::MENU_ITEM_H
}
fn eq(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}
fn item_target(title: &[u8], path: &[u8]) -> Target {
    KNOWN
        .iter()
        .find(|entry| entry.matches(title, path))
        .map_or(Target::None, |entry| entry.target)
}
fn action_target(action: &[u8]) -> Target {
    [
        (b"run".as_slice(), Target::MenuRun),
        (b"terminal_mode", Target::MenuTerminalMode),
        (b"restart", Target::MenuRestart),
        (b"poweroff", Target::MenuPoweroff),
        (b"halt", Target::MenuHalt),
        (b"screen_off", Target::MenuScreenOff),
    ]
    .into_iter()
    .find(|(name, _)| eq(action, name))
    .map_or(Target::None, |(_, target)| target)
}
fn submenu_id(value: &[u8]) -> SubmenuId {
    if eq(value, b"programs") {
        SubmenuId::Programs
    } else if eq(value, b"internet") {
        SubmenuId::Internet
    } else if eq(value, b"shutdown") {
        SubmenuId::ShutDown
    } else if eq(value, b"settings") {
        SubmenuId::Settings
    } else {
        SubmenuId::None
    }
}
fn submenu_target(id: SubmenuId) -> Target {
    match id {
        SubmenuId::Programs => Target::MenuPrograms,
        SubmenuId::Internet => Target::MenuProgramsInternet,
        SubmenuId::Settings => Target::MenuSettings,
        SubmenuId::ShutDown => Target::MenuShutDown,
        SubmenuId::None => Target::None,
    }
}
fn subitem_target(id: SubmenuId, title: &[u8], path: &[u8]) -> Target {
    match id {
        SubmenuId::Programs => item_target(title, path),
        SubmenuId::Internet => {
            if KNOWN[7].matches(title, path) {
                KNOWN[7].target
            } else {
                Target::None
            }
        }
        SubmenuId::Settings => {
            if eq(title, b"Display") && eq(path, KNOWN[14].path) {
                return Target::MenuSettingsDisplay;
            }
            for i in [14, 8, 9, 10, 11, 12, 13] {
                if KNOWN[i].matches(title, path) {
                    return KNOWN[i].target;
                }
            }
            Target::None
        }
        SubmenuId::None | SubmenuId::ShutDown => Target::None,
    }
}
fn value<'a>(line: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    line.split(|&b| b == b';')
        .map(trim)
        .find_map(|part| part.strip_prefix(key))
}
fn trim(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b" \t\r\n".contains(b))
        .unwrap_or(bytes.len());
    let end = bytes[start..]
        .iter()
        .rposition(|b| !b" \t\r\n".contains(b))
        .map_or(start, |i| start + i + 1);
    &bytes[start..end]
}
fn copy_z(out: &mut [u8], value: &[u8]) {
    out.fill(0);
    if out.is_empty() {
        return;
    }
    let n = value.len().min(out.len() - 1);
    out[..n].copy_from_slice(&value[..n]);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_defaults_keep_programs_and_retire_standalone_update() {
        let menu = Menu::defaults();
        assert_eq!(menu.target(0), Target::MenuPortex);
        assert_eq!(menu.launch(0).unwrap().policy, Policy::Gui);
        assert_eq!(menu.entries().len(), 6);
        assert_eq!(menu.submenu_count(), 4);
        assert_eq!(menu.target(1), Target::MenuPrograms);
        assert_eq!(menu.launch(1).unwrap().kind, Kind::Submenu);
        assert!(menu.has_submenu(1));
        assert_eq!(menu.submenu_target(1, 0), Target::MenuTerminal);
        assert_eq!(menu.submenu_launch(1, 0).unwrap().policy, Policy::Console);
        assert_eq!(menu.submenu_target(1, 2), Target::MenuPaint);
        assert_eq!(menu.submenu_target(1, 6), Target::MenuR4code);
        assert_eq!(menu.submenu_target(1, 7), Target::MenuProgramsInternet);
        assert!(menu.submenu_has_submenu(1, 7));
        assert_eq!(menu.nested_target(1, 7, 0), Target::MenuKlickifax);
        assert_eq!(menu.submenu_target(1, 8), Target::MenuSpectra);
        assert_eq!(menu.submenu_launch(1, 8).unwrap().args, b"/GUI");
        assert_eq!(menu.submenu_launch(1, 8).unwrap().policy, Policy::Gui);
        assert_eq!(menu.target(2), Target::MenuSettings);
        assert_eq!(menu.target(3), Target::MenuRun);
        assert_eq!(menu.target(4), Target::MenuTerminalMode);
        assert_eq!(menu.target(5), Target::MenuShutDown);
        assert_eq!(menu.submenu(5).unwrap().entries().len(), 2);
        assert_eq!(menu.submenu_target(5, 0), Target::MenuRestart);
        assert_eq!(menu.submenu_target(5, 1), Target::MenuPoweroff);
        assert_eq!(menu.submenu_target(2, 0), Target::MenuSettingsAppearance);
        assert_eq!(menu.submenu_target(2, 7), Target::MenuSettingsDisplay);
        assert_eq!(menu.submenu_launch(2, 7).unwrap().args, b"/DISPLAY");
        assert!(
            menu.entries()
                .iter()
                .all(|e| e.target() != Target::MenuUpdate)
        );
    }
    #[test]
    fn original_menu_r4s_parser() {
        let mut menu = Menu::defaults();
        assert!(menu.load(b"# comment\nSUBMENU;title=Programs;id=programs\nSUBITEM;menu=programs;title=Terminal;path=/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X;class=console;policy=console\nSUBITEM;menu=programs;title=Notepad;path=/R4OS/SOFTWARE/DESKTOP/NOTEPAD.R4X;class=gui;policy=gui\nCHILDMENU;menu=programs;title=Internet;id=internet\nSUBITEM;menu=internet;title=Klickifax;path=/R4OS/SOFTWARE/INTERNET/KLICKIFAX.R4X;class=gui;policy=gui\nACTION;title=Terminal Mode;action=terminal_mode\nACTION;title=Run...;action=run\nSUBMENU;title=Settings;id=settings\nSUBITEM;menu=settings;title=Appearance;path=/R4OS/SOFTWARE/DESKTOP/APPEARANCE.R4X;class=gui;policy=gui\nSUBITEM;menu=settings;title=Display;path=/R4OS/SOFTWARE/DESKTOP/APPEARANCE.R4X;args=/DISPLAY;class=gui;policy=gui\nACTION;title=Tasks;action=tasks\nACTION;title=Restart;action=restart\nACTION;title=Poweroff;action=poweroff\nACTION;title=Halt;action=halt\n"));
        assert!(menu.loaded_from_config);
        assert_eq!(menu.entries().len(), 7);
        assert_eq!(menu.submenu_count(), 3);
        assert_eq!(menu.target(0), Target::MenuPrograms);
        assert_eq!(menu.submenu_target(0, 0), Target::MenuTerminal);
        assert_eq!(menu.submenu_target(0, 1), Target::MenuNotepad);
        assert_eq!(menu.submenu_target(0, 2), Target::MenuProgramsInternet);
        assert_eq!(menu.nested_target(0, 2, 0), Target::MenuKlickifax);
        assert_eq!(menu.nested_launch(0, 2, 0).unwrap().policy, Policy::Gui);
        assert_eq!(
            menu.nested_launch(0, 2, 0).unwrap().path,
            b"/R4OS/SOFTWARE/INTERNET/KLICKIFAX.R4X"
        );
        assert_eq!(menu.target(3), Target::MenuSettings);
        assert_eq!(menu.submenu_target(3, 0), Target::MenuSettingsAppearance);
        assert_eq!(menu.submenu_target(3, 1), Target::MenuSettingsDisplay);
        assert_eq!(menu.submenu_launch(3, 1).unwrap().args, b"/DISPLAY");
        assert_eq!(menu.target(5), Target::MenuPoweroff);
    }
    #[test]
    fn original_unknown_config_keeps_defaults() {
        let mut menu = Menu::defaults();
        assert!(!menu.load(b"ITEM;title=Unknown;path=/R4OS/SOFTWARE/DESKTOP/UNKNOWN.R4X;policy=gui\nACTION;title=Settings;action=settings\nACTION;title=Nope;action=unknown\n"));
        assert_eq!(menu.entries().len(), 6);
        assert_eq!(menu.target(0), Target::MenuPortex);
        assert!(
            !menu.load(
                b"ITEM;title=R4OS Update;path=/R4OS/SOFTWARE/DESKTOP/UPDATE.R4X;policy=gui\n"
            )
        );
        assert!(
            menu.entries()
                .iter()
                .all(|e| e.target() != Target::MenuUpdate)
        );
    }
    #[test]
    fn shipped_and_embedded_menus_match_and_only_original_files_migrate() {
        let template =
            include_bytes!("../../../../../Distribution/Injection/R4OS/SOFTWARE/DESKTOP/MENU.R4S");
        assert!(lines(DEFAULT_MENU).eq(lines(template)));
        assert!(is_previous_default(LEGACY_MENU));
        assert!(!is_previous_default(DEFAULT_MENU));
        let mut custom = LEGACY_MENU.to_vec();
        custom.extend_from_slice(b"# local setting\n");
        assert!(!is_previous_default(&custom));
    }
    #[test]
    fn root_separators_and_nested_hits() {
        let menu = Menu::defaults();
        let height = 720;
        let panel = menu.root_rect(height);
        assert_eq!(menu.item_y(height, 0), panel.y + style::MENU_PADDING);
        for i in 0..menu.entries().len() {
            assert_eq!(menu.has_separator(i), matches!(i, 1 | 4));
            let y = menu.item_y(height, i);
            assert_eq!(menu.hit(height, 40, y + 3), Some(i));
            if menu.has_separator(i) {
                assert_eq!(menu.hit(height, 40, y - 3), None);
            }
        }
        let last = menu.entries().len() - 1;
        assert_eq!(
            menu.hit(height, 40, menu.item_y(height, last) + 3),
            Some(last)
        );
        let rect = menu.submenu_rect(1280, height, 1).unwrap();
        assert_eq!(rect.x, style::MENU_W - 4);
        assert_eq!(
            menu.submenu_hit(1280, height, 1, rect.x + 8, submenu_item_y(rect, 7) + 3),
            Some(7)
        );
        let rect = menu.nested_rect(1280, height, 1, 7).unwrap();
        assert_eq!(
            menu.nested_hit(1280, height, 1, 7, rect.x + 8, submenu_item_y(rect, 0) + 3),
            Some(0)
        );
    }
}
