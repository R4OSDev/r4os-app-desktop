//! Four display slots and their focus/pointer transitions. The caller applies
//! returned damage, WINSVC updates and client events after this short borrow.
//! Process handles, close acknowledgements and frame leases live with clients.
use super::window::{self, Kind, ResizeHandle, Window};
use crate::{input::event::Target, render::geometry::Rect};

pub const SLOT_COUNT: usize = 4;
const WINDOW_TARGETS: [Target; SLOT_COUNT] = [
    Target::TerminalWindow,
    Target::WmWindow,
    Target::App2Window,
    Target::App3Window,
];
const CLOSE_TARGETS: [Target; SLOT_COUNT] = [
    Target::TerminalClose,
    Target::WmClose,
    Target::App2Close,
    Target::App3Close,
];
const MIN_TARGETS: [Target; SLOT_COUNT] = [
    Target::TerminalMin,
    Target::WmMin,
    Target::App2Min,
    Target::App3Min,
];
const MAX_TARGETS: [[Target; 2]; SLOT_COUNT] = [
    [Target::TerminalMaxNormal, Target::TerminalMaxFull],
    [Target::WmMaxNormal, Target::WmMaxFull],
    [Target::App2MaxNormal, Target::App2MaxFull],
    [Target::App3MaxNormal, Target::App3MaxFull],
];
const INFO_TARGETS: [Target; SLOT_COUNT] = [
    Target::TerminalInfo,
    Target::WmInfo,
    Target::App2Info,
    Target::App3Info,
];
const TASKBAR_TARGETS: [Target; SLOT_COUNT] = [
    Target::TerminalTaskbar,
    Target::WmTaskbar,
    Target::App2Taskbar,
    Target::App3Taskbar,
];

/// Masks refer to the four display slots, never to process/PCI identities.
#[derive(Default, Debug)]
pub struct Changes {
    pub dirty: u8,
    pub geometry: u8,
    pub resized: u8,
    pub minimized: u8,
    pub restored: u8,
    pub maximized: u8,
    pub focus_lost: Option<usize>,
    pub focus_gained: Option<usize>,
    pub focus_mirror: Option<usize>,
    pub previous_frame: Option<Rect>,
    pub taskbar: bool,
    pub full_damage: bool,
    pub client_pointer_down: Option<usize>,
}
#[derive(Clone, Copy)]
enum Pointer {
    Drag {
        index: usize,
        grab_x: i32,
        grab_y: i32,
        last: (i32, i32),
    },
    Resize {
        index: usize,
        handle: ResizeHandle,
        start: Rect,
        mouse: (i32, i32),
        last: (i32, i32),
    },
}
impl Pointer {
    fn index(self) -> usize {
        match self {
            Self::Drag { index, .. } | Self::Resize { index, .. } => index,
        }
    }
}
pub struct Controller {
    windows: [Window; SLOT_COUNT],
    active: usize,
    keyboard_focus: Target,
    pointer: Option<Pointer>,
}
impl Default for Controller {
    fn default() -> Self {
        let mut windows = [
            Window::new(Kind::Terminal, Rect::new(78, 72, 470, 248)),
            Window::new(Kind::Manager, Rect::new(620, 110, 450, 250)),
            Window::new(Kind::App, Rect::new(680, 160, 430, 240)),
            Window::new(Kind::App, Rect::new(520, 250, 430, 240)),
        ];
        for win in &mut windows {
            win.close();
        }
        Self {
            windows,
            active: 0,
            keyboard_focus: Target::None,
            pointer: None,
        }
    }
}
impl Controller {
    pub fn windows(&self) -> &[Window; SLOT_COUNT] {
        &self.windows
    }
    pub fn window_mut(&mut self, index: usize) -> Option<&mut Window> {
        self.windows.get_mut(index)
    }
    pub fn active(&self) -> usize {
        self.active
    }
    pub fn keyboard_focus(&self) -> Target {
        self.keyboard_focus
    }
    pub fn set_keyboard_focus(&mut self, target: Target) {
        self.keyboard_focus = target;
    }
    pub fn pointer_window(&self) -> Option<usize> {
        self.pointer.map(Pointer::index)
    }
    pub fn window_target(index: usize) -> Target {
        WINDOW_TARGETS.get(index).copied().unwrap_or(Target::None)
    }
    pub fn close_target(index: usize) -> Target {
        CLOSE_TARGETS.get(index).copied().unwrap_or(Target::None)
    }
    pub fn info_target(index: usize) -> Target {
        INFO_TARGETS.get(index).copied().unwrap_or(Target::None)
    }
    pub fn min_target(index: usize) -> Target {
        MIN_TARGETS.get(index).copied().unwrap_or(Target::None)
    }
    pub fn taskbar_target(index: usize) -> Target {
        TASKBAR_TARGETS.get(index).copied().unwrap_or(Target::None)
    }
    pub fn max_target(&self, index: usize) -> Target {
        self.windows.get(index).map_or(Target::None, |win| {
            MAX_TARGETS[index][usize::from(win.maximized())]
        })
    }
    pub fn index_for_target(target: Target) -> Option<usize> {
        (0..SLOT_COUNT).find(|&i| {
            WINDOW_TARGETS[i] == target
                || CLOSE_TARGETS[i] == target
                || MIN_TARGETS[i] == target
                || MAX_TARGETS[i].contains(&target)
                || INFO_TARGETS[i] == target
                || TASKBAR_TARGETS[i] == target
        })
    }
    pub fn target_for_window(&self, index: usize, x: i32, y: i32) -> Target {
        let Some(win) = self.windows.get(index) else {
            return Target::None;
        };
        if win.close_hit(x, y) {
            Self::close_target(index)
        } else if win.min_hit(x, y) {
            Self::min_target(index)
        } else if win.max_hit(x, y) {
            self.max_target(index)
        } else {
            Self::window_target(index)
        }
    }
    pub fn target_at(&self, x: i32, y: i32) -> Option<Target> {
        window::topmost_at(&self.windows, self.active, x, y)
            .map(|index| self.target_for_window(index, x, y))
    }
    pub fn active_target_at(&self, x: i32, y: i32) -> Target {
        let win = &self.windows[self.active];
        if !win.visible() || win.minimized() {
            Target::None
        } else {
            self.target_for_window(self.active, x, y)
        }
    }
    fn bound_gui(&self, index: usize) -> bool {
        self.windows[index].kind() == Kind::App && self.windows[index].instance_id() != 0
    }
    fn hosted_gui(&self, index: usize) -> bool {
        self.bound_gui(index) && self.windows[index].visible() && !self.windows[index].minimized()
    }
    fn activate(&mut self, index: usize, announce_same: bool, change: &mut Changes) {
        let previous = self.active;
        if previous != index {
            change.full_damage |=
                self.windows[index].fullscreen() || self.windows[previous].fullscreen();
            if self.bound_gui(previous) {
                change.focus_lost = Some(previous);
            }
        }
        self.active = index;
        if (previous != index || announce_same) && self.hosted_gui(index) {
            change.focus_gained = Some(index);
        }
    }
    pub fn focus_or_restore(&mut self, index: usize) -> Changes {
        let mut change = Changes::default();
        if index >= SLOT_COUNT {
            return change;
        }
        change.dirty = (1 << self.active) | (1 << index);
        self.windows[index].restore();
        self.activate(index, false, &mut change);
        self.keyboard_focus = Self::window_target(index);
        change.focus_mirror = Some(index);
        change.taskbar = true;
        change
    }
    fn focus_first(&mut self, change: &mut Changes) {
        if let Some(index) = self
            .windows
            .iter()
            .position(|win| win.visible() && !win.minimized())
        {
            self.activate(index, false, change);
            self.keyboard_focus = Self::window_target(index);
            change.focus_mirror = Some(index);
            change.dirty |= 1 << index;
        } else {
            if self.bound_gui(self.active) {
                change.focus_lost = Some(self.active);
            }
            self.keyboard_focus = Target::None;
        }
    }
    pub fn cycle(&mut self, previous: bool) -> Changes {
        for step in 1..=SLOT_COUNT {
            let index =
                (self.active + if previous { SLOT_COUNT - step } else { step }) % SLOT_COUNT;
            if self.windows[index].visible() {
                // The original keyboard cycle restores minimized windows, but
                // leaves WINSVC focus publication to its normal reconciliation.
                let mut change = self.focus_or_restore(index);
                change.focus_mirror = None;
                return change;
            }
        }
        Changes::default()
    }
    pub fn minimize(&mut self, index: usize) -> Changes {
        let mut change = Changes::default();
        if index >= SLOT_COUNT {
            return change;
        }
        change.dirty = 1 << index;
        self.windows[index].minimize();
        change.minimized = 1 << index;
        change.geometry = 1 << index;
        if self.active == index {
            self.focus_first(&mut change);
        }
        change.taskbar = true;
        change
    }
    pub fn show_desktop(&mut self) -> Changes {
        let mut change = Changes::default();
        for (index, win) in self.windows.iter_mut().enumerate() {
            if !win.visible() || win.minimized() {
                continue;
            }
            win.minimize();
            change.dirty |= 1 << index;
            change.minimized |= 1 << index;
            change.geometry |= 1 << index;
        }
        if change.dirty != 0 {
            self.focus_first(&mut change);
            change.taskbar = true;
        }
        change
    }
    /// Called only after the client owner has cleared its instance binding.
    pub fn close_unbound(&mut self, index: usize) -> Option<Changes> {
        let win = self.windows.get_mut(index)?;
        if win.instance_id() != 0 {
            return None;
        }
        let mut change = Changes {
            dirty: 1 << index,
            taskbar: true,
            ..Changes::default()
        };
        win.close();
        if self.active == index {
            self.focus_first(&mut change);
        }
        Some(change)
    }
    pub fn toggle_maximize(&mut self, index: usize, work_area: Rect) -> Changes {
        let mut change = Changes::default();
        let Some(win) = self.windows.get_mut(index) else {
            return change;
        };
        if win.fullscreen() {
            return change;
        }
        change.previous_frame = Some(win.geometry());
        win.toggle_maximize_in(work_area);
        change.dirty = 1 << index;
        change.geometry = 1 << index;
        change.resized = 1 << index;
        if win.maximized() {
            change.maximized = 1 << index;
        } else {
            change.restored = 1 << index;
        }
        self.activate(index, false, &mut change);
        self.keyboard_focus = Self::window_target(index);
        change.focus_mirror = Some(index);
        change.taskbar = true;
        change
    }
    pub fn set_fullscreen(
        &mut self,
        index: usize,
        enabled: bool,
        output: Rect,
        work_area: Rect,
    ) -> Changes {
        let mut change = Changes::default();
        let Some(win) = self.windows.get_mut(index) else {
            return change;
        };
        let old = win.geometry();
        if !win.set_fullscreen(enabled, output, work_area) {
            return change;
        }
        if self.pointer_window() == Some(index) {
            self.pointer = None;
        }
        change.previous_frame = Some(old);
        change.dirty = 1 << index;
        change.geometry = 1 << index;
        change.resized = 1 << index;
        change.full_damage = true;
        change
    }
    pub fn pointer_down(&mut self, index: usize, x: i32, y: i32) -> Changes {
        let mut change = Changes::default();
        if index >= SLOT_COUNT {
            return change;
        }
        change.dirty = (1 << self.active) | (1 << index);
        self.activate(index, false, &mut change);
        self.keyboard_focus = self.target_for_window(index, x, y);
        change.focus_mirror = Some(index);
        change.taskbar = true;
        let win = &self.windows[index];
        if let Some(handle) = win.resize_hit(x, y) {
            self.pointer = Some(Pointer::Resize {
                index,
                handle,
                start: win.geometry(),
                mouse: (x, y),
                last: (x, y),
            });
        } else if win.title_hit(x, y) {
            let frame = win.geometry();
            self.pointer = Some(Pointer::Drag {
                index,
                grab_x: x - frame.x,
                grab_y: y - frame.y,
                last: (x, y),
            });
        } else if self.hosted_gui(index) {
            change.client_pointer_down = Some(index);
        }
        change
    }
    /// `rescue` is supplied by the output-layout owner. Single-output callers
    /// clamp to their work area; managed outputs use the existing R4GFX rescue.
    pub fn pointer_motion(
        &mut self,
        x: i32,
        y: i32,
        resize_area: Rect,
        mut rescue: impl FnMut(Rect) -> Option<Rect>,
    ) -> Changes {
        let mut change = Changes::default();
        let Some(pointer) = self.pointer else {
            return change;
        };
        let index = pointer.index();
        let win = &mut self.windows[index];
        let old = win.geometry();
        match pointer {
            Pointer::Drag {
                grab_x,
                grab_y,
                last,
                ..
            } => {
                if last == (x, y) {
                    return change;
                }
                self.pointer = Some(Pointer::Drag {
                    index,
                    grab_x,
                    grab_y,
                    last: (x, y),
                });
                if let Some(next) = rescue(Rect::new(x - grab_x, y - grab_y, old.w, old.h)) {
                    win.set_normal(next);
                }
            }
            Pointer::Resize {
                handle,
                start,
                mouse,
                last,
                ..
            } => {
                if last == (x, y) {
                    return change;
                }
                self.pointer = Some(Pointer::Resize {
                    index,
                    handle,
                    start,
                    mouse,
                    last: (x, y),
                });
                win.resize_in(start, handle, x - mouse.0, y - mouse.1, resize_area);
            }
        }
        if win.geometry() == old {
            return change;
        }
        change.previous_frame = Some(old);
        change.dirty = 1 << index;
        change.geometry = 1 << index;
        if matches!(pointer, Pointer::Resize { .. }) {
            change.resized = 1 << index;
        }
        self.activate(index, false, &mut change);
        change
    }
    /// Release coordinates are applied even without an intervening motion
    /// packet. The runtime must then force its geometry-publication queue.
    pub fn pointer_release(
        &mut self,
        x: i32,
        y: i32,
        resize_area: Rect,
        rescue: impl FnMut(Rect) -> Option<Rect>,
    ) -> Changes {
        let change = self.pointer_motion(x, y, resize_area, rescue);
        self.pointer = None;
        change
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const AREA: Rect = Rect::new(0, 0, 1280, 684);
    #[test]
    fn current_four_slot_focus_minimize_cycle_and_hosted_close_trace() {
        let mut windows = Controller::default();
        assert!(windows.windows().iter().all(|win| !win.visible()));
        windows.window_mut(0).unwrap().bind_console(10, c"Console");
        windows.window_mut(2).unwrap().bind_app(12, c"App");
        windows.focus_or_restore(0);
        let focused = windows.focus_or_restore(2);
        assert_eq!(focused.focus_gained, Some(2));
        let minimized = windows.minimize(2);
        assert_eq!(minimized.focus_lost, Some(2));
        assert_eq!(windows.active(), 0);
        assert_eq!(windows.keyboard_focus(), Target::TerminalWindow);
        let cycled = windows.cycle(false);
        assert_eq!(windows.active(), 2);
        assert!(!windows.windows()[2].minimized());
        assert_eq!(cycled.focus_gained, Some(2));
        assert_eq!(cycled.focus_mirror, None);
        assert!(windows.close_unbound(2).is_none());
        windows.window_mut(2).unwrap().unbind_instance();
        windows.close_unbound(2).unwrap();
        assert_eq!(windows.active(), 0);
        windows.show_desktop();
        assert_eq!(windows.keyboard_focus(), Target::None);
        assert!(windows.windows()[0].minimized());
        windows.cycle(true);
        assert!(!windows.windows()[0].minimized());
    }
    #[test]
    fn current_release_packet_applies_final_drag_and_resize_coordinates() {
        let mut windows = Controller::default();
        windows.focus_or_restore(0);
        windows.pointer_down(0, 100, 80);
        assert_eq!(windows.pointer_window(), Some(0));
        let moved = windows.pointer_release(200, 180, AREA, |rect| Some(rect.clamp_inside(AREA)));
        assert_eq!(moved.previous_frame, Some(Rect::new(78, 72, 470, 248)));
        assert_eq!(
            windows.windows()[0].geometry(),
            Rect::new(178, 172, 470, 248)
        );
        assert_eq!(windows.pointer_window(), None);
        windows.pointer_down(0, 647, 419);
        let resized = windows.pointer_release(700, 450, AREA, |_| panic!("resize uses bounds"));
        assert_eq!(resized.resized, 1);
        assert_eq!(
            windows.windows()[0].geometry(),
            Rect::new(178, 172, 523, 279)
        );
        let unchanged = windows.pointer_release(900, 600, AREA, |_| panic!("released"));
        assert_eq!(unchanged.dirty, 0);
    }
    #[test]
    fn current_fullscreen_focus_and_chrome_ids_keep_slot_identity() {
        let mut windows = Controller::default();
        windows.window_mut(2).unwrap().bind_app(20, c"Game");
        windows.focus_or_restore(2);
        for index in 0..SLOT_COUNT {
            for target in [
                Controller::window_target(index),
                Controller::close_target(index),
                Controller::min_target(index),
                windows.max_target(index),
                Controller::info_target(index),
                Controller::taskbar_target(index),
            ] {
                assert_eq!(Controller::index_for_target(target), Some(index));
            }
        }
        let output = Rect::new(-1280, 0, 1280, 720);
        let work = Rect::new(-1280, 0, 1280, 684);
        let changed = windows.set_fullscreen(2, true, output, work);
        assert!(changed.full_damage);
        assert_eq!(windows.target_at(-100, 100), Some(Target::App2Window));
        assert_eq!(windows.toggle_maximize(2, work).dirty, 0);
        assert_eq!(
            windows.pointer_down(2, -100, 100).client_pointer_down,
            Some(2)
        );
        assert_eq!(windows.pointer_window(), None);
        windows.focus_or_restore(0);
        assert_eq!(windows.windows()[2].geometry(), output);
        windows.set_fullscreen(2, false, output, work);
        windows.toggle_maximize(2, work);
        assert_eq!(windows.max_target(2), Target::App2MaxFull);
        assert_eq!(windows.windows()[2].geometry(), work);
    }
}
