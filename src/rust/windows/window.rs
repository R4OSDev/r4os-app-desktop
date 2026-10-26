//! Window geometry, chrome and display state. Process lifetime remains with
//! the hosted-client owner; this display ID never substitutes for its handle.
use crate::render::geometry::{self, Rect, Surface, SurfaceKind};
use core::ffi::CStr;

pub const DEFAULT_MIN_W: i32 = 180;
pub const DEFAULT_MIN_H: i32 = 96;
use crate::shell::style::{BUTTON, TASKBAR_H, TITLE_H};
const RESIZE_MARGIN: i32 = 6;
const TITLE_MAX: usize = 40;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Terminal,
    Manager,
    App,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ResizeHandle {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}
impl ResizeHandle {
    fn left(self) -> bool {
        matches!(self, Self::Left | Self::TopLeft | Self::BottomLeft)
    }
    fn right(self) -> bool {
        matches!(self, Self::Right | Self::TopRight | Self::BottomRight)
    }
    fn top(self) -> bool {
        matches!(self, Self::Top | Self::TopLeft | Self::TopRight)
    }
    fn bottom(self) -> bool {
        matches!(self, Self::Bottom | Self::BottomLeft | Self::BottomRight)
    }
}
#[derive(Clone, Debug)]
pub struct Window {
    kind: Kind,
    starting: bool,
    current: Rect,
    normal: Rect,
    visible: bool,
    minimized: bool,
    maximized: bool,
    fullscreen: bool,
    fullscreen_restore: Rect,
    instance_id: u32,
    close_requested: bool,
    close_requested_tick: u64,
    gui_revision: u32,
    min_frame_w: i32,
    min_frame_h: i32,
    title: [u8; TITLE_MAX + 1],
}
impl Window {
    pub fn new(kind: Kind, rect: Rect) -> Self {
        Self {
            kind,
            starting: false,
            current: rect,
            normal: rect,
            visible: true,
            minimized: false,
            maximized: false,
            fullscreen: false,
            fullscreen_restore: Rect::ZERO,
            instance_id: 0,
            close_requested: false,
            close_requested_tick: 0,
            gui_revision: 0,
            min_frame_w: DEFAULT_MIN_W,
            min_frame_h: DEFAULT_MIN_H,
            title: [0; TITLE_MAX + 1],
        }
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn geometry(&self) -> Rect {
        self.current
    }
    pub fn saved_geometry(&self) -> Rect {
        self.normal
    }
    pub fn reconfigure(&mut self, transform: impl Fn(Rect) -> Rect, output: Rect, work: Rect) {
        self.normal = transform(self.normal);
        self.fullscreen_restore = transform(self.fullscreen_restore);
        self.current = if self.fullscreen {
            output
        } else if self.maximized {
            work
        } else {
            transform(self.current)
        };
    }
    pub fn visible(&self) -> bool {
        self.visible
    }
    pub fn minimized(&self) -> bool {
        self.minimized
    }
    pub fn maximized(&self) -> bool {
        self.maximized
    }
    pub fn fullscreen(&self) -> bool {
        self.fullscreen
    }
    pub fn instance_id(&self) -> u32 {
        self.instance_id
    }
    pub fn close_requested(&self) -> Option<u64> {
        self.close_requested.then_some(self.close_requested_tick)
    }
    pub fn gui_revision(&self) -> u32 {
        self.gui_revision
    }
    pub fn set_gui_revision(&mut self, revision: u32) {
        self.gui_revision = revision;
    }
    pub fn minimum_frame(&self) -> (i32, i32) {
        (self.min_frame_w, self.min_frame_h)
    }
    pub fn title(&self) -> &CStr {
        CStr::from_bytes_until_nul(&self.title).expect("terminated window title")
    }
    pub fn set_title(&mut self, title: &CStr) {
        self.set_title_bytes(title.to_bytes());
    }
    pub fn set_title_bytes(&mut self, title: &[u8]) {
        self.title.fill(0);
        let count = utf8_prefix_bytes(title, TITLE_MAX);
        self.title[..count].copy_from_slice(&title[..count]);
    }
    pub fn set_normal(&mut self, rect: Rect) {
        self.fullscreen = false;
        self.current = rect;
        self.normal = rect;
        self.maximized = false;
    }
    pub fn restore(&mut self) {
        self.visible = true;
        self.minimized = false;
    }
    pub fn minimize(&mut self) {
        self.minimized = true;
    }
    pub fn close(&mut self) {
        self.starting = false;
        self.leave_fullscreen();
        self.visible = false;
        self.minimized = false;
        self.close_requested = false;
        self.close_requested_tick = 0;
    }
    pub fn bind_instance(&mut self, id: u32) {
        self.instance_id = id;
    }
    pub fn starting(&self) -> bool {
        self.starting
    }
    pub fn begin_launch(&mut self, title: &CStr) {
        self.bind(Kind::App, 0, title);
        self.starting = true;
    }
    pub fn bind_app(&mut self, id: u32, title: &CStr) {
        self.bind(Kind::App, id, title);
    }
    pub fn bind_console(&mut self, id: u32, title: &CStr) {
        self.bind(Kind::Terminal, id, title);
    }
    fn bind(&mut self, kind: Kind, id: u32, title: &CStr) {
        self.leave_fullscreen();
        self.kind = kind;
        self.starting = false;
        self.bind_instance(id);
        self.set_title(title);
        self.reset_min_size();
        self.visible = true;
        self.minimized = false;
        self.close_requested = false;
        self.close_requested_tick = 0;
    }
    pub fn unbind_instance(&mut self) {
        self.starting = false;
        self.leave_fullscreen();
        self.instance_id = 0;
        self.close_requested = false;
        self.close_requested_tick = 0;
        self.gui_revision = 0;
    }
    pub fn request_close(&mut self, tick: u64) {
        self.close_requested = true;
        self.close_requested_tick = tick;
    }
    pub fn reset_min_size(&mut self) {
        self.min_frame_w = DEFAULT_MIN_W;
        self.min_frame_h = DEFAULT_MIN_H;
    }
    pub fn set_min_client_size(&mut self, w: i32, h: i32, screen_w: i32, screen_h: i32) -> bool {
        self.set_min_client_size_in(w, h, geometry::work_area(screen_w, screen_h, TASKBAR_H))
    }
    pub fn set_min_client_size_in(&mut self, w: i32, h: i32, area: Rect) -> bool {
        let next_w = w
            .saturating_add(16)
            .max(DEFAULT_MIN_W)
            .clamp(DEFAULT_MIN_W, area.w.max(DEFAULT_MIN_W));
        let next_h = h
            .saturating_add(TITLE_H + 18)
            .max(DEFAULT_MIN_H)
            .clamp(DEFAULT_MIN_H, area.h.max(DEFAULT_MIN_H));
        if (next_w, next_h) == (self.min_frame_w, self.min_frame_h) {
            return false;
        }
        self.min_frame_w = next_w;
        self.min_frame_h = next_h;
        if !self.maximized && !self.fullscreen {
            let r = self.current;
            self.set_normal(
                Rect::new(r.x, r.y, r.w.max(next_w), r.h.max(next_h)).clamp_inside(area),
            );
        }
        true
    }
    pub fn toggle_maximize(&mut self, screen_w: i32, screen_h: i32) {
        self.toggle_maximize_in(geometry::work_area(screen_w, screen_h, TASKBAR_H));
    }
    pub fn toggle_maximize_in(&mut self, area: Rect) {
        if self.fullscreen {
            return;
        }
        if self.maximized {
            self.current = self.normal_rect_in(area);
            self.maximized = false;
        } else {
            self.normal = self.current.clamp_inside(area);
            self.current = area;
            self.maximized = true;
        }
        self.visible = true;
        self.minimized = false;
    }
    pub fn fit_to_work_area(&mut self, screen_w: i32, screen_h: i32) {
        let area = geometry::work_area(screen_w, screen_h, TASKBAR_H);
        if self.fullscreen {
            self.fit_fullscreen(geometry::desktop(screen_w, screen_h).rect, area);
            return;
        }
        self.current = if self.maximized {
            area
        } else {
            self.normal_rect_in(area)
        };
        if !self.maximized {
            self.normal = self.current;
        }
    }
    pub fn move_to(&mut self, x: i32, y: i32, screen_w: i32, screen_h: i32) {
        if !self.visible || self.minimized || self.maximized || self.fullscreen {
            return;
        }
        let r = self.current;
        self.current = Rect::new(x, y, r.w, r.h)
            .clamp_inside(geometry::work_area(screen_w, screen_h, TASKBAR_H));
        self.normal = self.current;
    }
    pub fn set_fullscreen(&mut self, enabled: bool, output: Rect, area: Rect) -> bool {
        if enabled == self.fullscreen {
            return false;
        }
        if enabled {
            self.fullscreen = true;
            self.fit_fullscreen(output, area);
        } else {
            self.fullscreen_restore = if self.maximized {
                area
            } else {
                self.normal_rect_in(area)
            };
            self.leave_fullscreen();
        }
        true
    }
    pub fn fit_fullscreen(&mut self, output: Rect, area: Rect) {
        if !self.fullscreen {
            return;
        }
        self.fullscreen_restore = if self.maximized {
            area
        } else {
            self.normal_rect_in(area)
        };
        self.current = output;
    }
    fn leave_fullscreen(&mut self) {
        if self.fullscreen {
            self.fullscreen = false;
            self.current = self.fullscreen_restore;
        }
    }
    pub fn frame_surface(&self) -> Surface {
        Surface::new(SurfaceKind::WindowFrame, self.current)
    }
    pub fn title_surface(&self) -> Surface {
        let r = self.current;
        Surface::new(
            SurfaceKind::WindowTitle,
            Rect::new(r.x + 3, r.y + 3, r.w - 6, TITLE_H),
        )
    }
    pub fn title_drag_rect(&self) -> Rect {
        let r = self.current;
        Rect::new(r.x + 3, r.y + 3, r.w - 65, TITLE_H)
    }
    pub fn client_surface(&self) -> Surface {
        let r = self.current;
        Surface::new(
            SurfaceKind::WindowClient,
            if self.fullscreen {
                r
            } else {
                Rect::new(r.x + 8, r.y + TITLE_H + 10, r.w - 16, r.h - TITLE_H - 18)
            },
        )
    }
    pub fn gui_info(&self, index: usize) -> r4os_contract::GuiWindowInfo {
        use r4os_contract::{GuiWindowFlag as flags, gui_window_flag_fullscreen};
        let frame = self.geometry();
        let client = self.client_surface().rect;
        let mut state = 0;
        if self.visible {
            state |= flags::visible;
        }
        if self.minimized {
            state |= flags::minimized;
        }
        if self.maximized {
            state |= flags::maximized;
        }
        if self.close_requested {
            state |= flags::close_requested;
        }
        if self.fullscreen {
            state |= gui_window_flag_fullscreen;
        }
        r4os_contract::GuiWindowInfo {
            window_id: index as _,
            frame_x: frame.x,
            frame_y: frame.y,
            frame_w: frame.w,
            frame_h: frame.h,
            client_x: client.x,
            client_y: client.y,
            client_w: client.w,
            client_h: client.h,
            flags: state,
            ..Default::default()
        }
    }
    pub fn resize_from(
        &mut self,
        start: Rect,
        handle: ResizeHandle,
        dx: i32,
        dy: i32,
        screen_w: i32,
        screen_h: i32,
    ) {
        self.resize_in(
            start,
            handle,
            dx,
            dy,
            geometry::work_area(screen_w, screen_h, TASKBAR_H),
        );
    }
    pub fn resize_in(&mut self, start: Rect, handle: ResizeHandle, dx: i32, dy: i32, area: Rect) {
        if !self.visible || self.minimized || self.maximized || self.fullscreen || area.is_empty() {
            return;
        }
        let min_w = self.min_frame_w.min(area.w);
        let min_h = self.min_frame_h.min(area.h);
        let initial =
            Rect::new(start.x, start.y, start.w.max(min_w), start.h.max(min_h)).clamp_inside(area);
        let mut next = initial;
        if handle.left() {
            let max_x = i64::from(area.x).max(i64::from(initial.right()) - i64::from(min_w));
            next.x = (i64::from(initial.x) + i64::from(dx)).clamp(i64::from(area.x), max_x) as i32;
            next.w = initial.right() - next.x;
        } else if handle.right() {
            next.w = (i64::from(initial.w) + i64::from(dx)).clamp(
                i64::from(min_w),
                i64::from(min_w.max(area.right() - initial.x)),
            ) as i32;
        }
        if handle.top() {
            let max_y = i64::from(area.y).max(i64::from(initial.bottom()) - i64::from(min_h));
            next.y = (i64::from(initial.y) + i64::from(dy)).clamp(i64::from(area.y), max_y) as i32;
            next.h = initial.bottom() - next.y;
        } else if handle.bottom() {
            next.h = (i64::from(initial.h) + i64::from(dy)).clamp(
                i64::from(min_h),
                i64::from(min_h.max(area.bottom() - initial.y)),
            ) as i32;
        }
        self.set_normal(next);
    }
    fn normal_rect_in(&self, area: Rect) -> Rect {
        Rect::new(
            self.normal.x,
            self.normal.y,
            self.normal
                .w
                .clamp(self.min_frame_w, self.min_frame_w.max(area.w)),
            self.normal
                .h
                .clamp(self.min_frame_h, self.min_frame_h.max(area.h)),
        )
        .clamp_inside(area)
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.visible && !self.minimized && self.current.contains(x, y)
    }
    pub fn resize_hit(&self, x: i32, y: i32) -> Option<ResizeHandle> {
        if !self.contains(x, y) || self.maximized || self.fullscreen {
            return None;
        }
        let r = self.current;
        let (left, right, top, bottom) = (
            x < r.x + RESIZE_MARGIN,
            x >= r.right() - RESIZE_MARGIN,
            y < r.y + RESIZE_MARGIN,
            y >= r.bottom() - RESIZE_MARGIN,
        );
        Some(match (left, right, top, bottom) {
            (true, _, true, _) => ResizeHandle::TopLeft,
            (_, true, true, _) => ResizeHandle::TopRight,
            (true, _, _, true) => ResizeHandle::BottomLeft,
            (_, true, _, true) => ResizeHandle::BottomRight,
            (true, _, _, _) => ResizeHandle::Left,
            (_, true, _, _) => ResizeHandle::Right,
            (_, _, true, _) => ResizeHandle::Top,
            (_, _, _, true) => ResizeHandle::Bottom,
            _ => return None,
        })
    }
    pub fn title_hit(&self, x: i32, y: i32) -> bool {
        self.visible
            && !self.minimized
            && !self.maximized
            && !self.fullscreen
            && self.title_drag_rect().contains(x, y)
    }
    pub fn close_hit(&self, x: i32, y: i32) -> bool {
        self.button_hit(x, y, 1)
    }
    pub fn max_hit(&self, x: i32, y: i32) -> bool {
        self.button_hit(x, y, 2)
    }
    pub fn min_hit(&self, x: i32, y: i32) -> bool {
        self.button_hit(x, y, 3)
    }
    fn button_hit(&self, x: i32, y: i32, from_right: i32) -> bool {
        if !self.visible || self.minimized || self.fullscreen {
            return false;
        }
        let r = self.current;
        let bx = r.right() - 4 - from_right * (BUTTON + 2);
        x >= bx && x < bx + BUTTON && y >= r.y + 2 && y < r.y + 2 + BUTTON
    }
}
fn utf8_prefix_bytes(value: &[u8], limit: usize) -> usize {
    let limit = value.len().min(limit);
    let mut index = 0;
    while index < limit {
        let first = value[index];
        let n = match first {
            0..=0x7f => 1,
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => 1,
        };
        let valid =
            index + n <= value.len() && core::str::from_utf8(&value[index..index + n]).is_ok();
        let next = index + if valid { n } else { 1 };
        if next > limit {
            break;
        }
        index = next;
    }
    index
}
pub fn topmost_at(windows: &[Window], active: usize, x: i32, y: i32) -> Option<usize> {
    if active < windows.len() && windows[active].contains(x, y) {
        return Some(active);
    }
    (0..windows.len())
        .rev()
        .find(|&i| i != active && windows[i].contains(x, y))
}
pub fn fullscreen_covers(windows: &[Window], active: usize, rect: Rect) -> bool {
    let Some(win) = windows.get(active) else {
        return false;
    };
    !rect.is_empty()
        && win.fullscreen
        && win.contains(rect.x, rect.y)
        && win.contains(rect.right() - 1, rect.bottom() - 1)
}

#[cfg(test)]
mod tests {
    // Original twelve window.zig cases; keep current83 tiny/negative/extreme
    // work-area and captured-resize expectations with the window owner.
    use super::*;
    fn manager() -> Window {
        Window::new(Kind::Manager, Rect::new(100, 80, 300, 160))
    }
    #[test]
    fn title_hit_ignores_buttons_and_maximized() {
        let mut w = manager();
        assert!(w.title_hit(120, 90));
        assert!(!w.title_hit(384, 90));
        w.maximized = true;
        assert!(!w.title_hit(120, 90));
    }
    #[test]
    fn move_clamps_to_work_area() {
        let mut w = manager();
        w.move_to(-20, -10, 640, 480);
        assert_eq!((w.current.x, w.current.y), (0, 0));
        w.move_to(999, 999, 640, 480);
        assert_eq!((w.current.x, w.current.y), (340, 288));
        w.set_normal(Rect::new(0, 200, 640, 160));
        w.move_to(0, 200, 640, 480);
        assert_eq!(w.geometry(), Rect::new(0, 200, 640, 160));
        w.toggle_maximize(640, 480);
        w.toggle_maximize(640, 480);
        assert_eq!(w.geometry(), Rect::new(0, 200, 640, 160));
    }
    #[test]
    fn resize_hit_edges_and_maximized() {
        let mut w = manager();
        assert_eq!(w.resize_hit(102, 82), Some(ResizeHandle::TopLeft));
        assert_eq!(w.resize_hit(397, 120), Some(ResizeHandle::Right));
        assert_eq!(w.resize_hit(180, 238), Some(ResizeHandle::Bottom));
        assert_eq!(w.resize_hit(160, 120), None);
        w.maximized = true;
        assert_eq!(w.resize_hit(102, 82), None);
    }
    #[test]
    fn resize_minimum_work_area_and_captured_extremes() {
        let mut w = manager();
        w.resize_from(w.geometry(), ResizeHandle::Right, -400, 0, 640, 480);
        assert_eq!((w.current.w, w.current.x), (180, 100));
        w.set_normal(Rect::new(100, 80, 300, 160));
        w.resize_from(w.geometry(), ResizeHandle::BottomRight, 999, 999, 640, 480);
        assert_eq!((w.current.w, w.current.h), (540, 368));
        w.set_normal(Rect::new(100, 80, 300, 160));
        w.resize_from(w.geometry(), ResizeHandle::TopLeft, 260, 120, 640, 480);
        assert_eq!(w.geometry(), Rect::new(220, 144, 180, 96));
        let edge = Rect::new(i32::MIN, i32::MIN, 640, 480);
        w.set_normal(Rect::new(edge.x + 32, edge.y + 32, 300, 160));
        w.resize_in(
            w.geometry(),
            ResizeHandle::TopLeft,
            i32::MIN,
            i32::MIN,
            edge,
        );
        assert_eq!(w.geometry(), Rect::new(edge.x, edge.y, 332, 192));
        w.resize_in(
            w.geometry(),
            ResizeHandle::BottomRight,
            i32::MAX,
            i32::MAX,
            edge,
        );
        assert_eq!(w.geometry(), edge);
        w.resize_in(
            Rect::new(edge.x + 630, edge.y + 470, 10, 10),
            ResizeHandle::BottomRight,
            0,
            0,
            edge,
        );
        assert_eq!(w.geometry(), Rect::new(edge.x + 460, edge.y + 384, 180, 96));
    }
    #[test]
    fn maximize_restore_and_fullscreen_keep_original_state() {
        let mut w = Window::new(Kind::Manager, Rect::new(520, 400, 300, 160));
        let output = Rect::new(0, 0, 640, 480);
        let area = Rect::new(0, 0, 640, 448);
        w.toggle_maximize(640, 480);
        assert!(w.maximized());
        assert_eq!(w.geometry(), area);
        w.toggle_maximize(640, 480);
        assert!(!w.maximized());
        assert_eq!(w.geometry(), Rect::new(340, 288, 300, 160));
        let saved = w.geometry();
        assert!(w.set_fullscreen(true, output, area));
        assert_eq!(w.geometry(), output);
        assert_eq!(w.client_surface().rect, output);
        assert!(!w.set_fullscreen(true, output, area));
        assert!(w.resize_hit(0, 0).is_none() && !w.title_hit(10, 10));
        w.toggle_maximize_in(area);
        assert!(!w.maximized());
        w.move_to(30, 30, 640, 480);
        assert_eq!(w.geometry(), output);
        assert!(fullscreen_covers(
            core::slice::from_ref(&w),
            0,
            Rect::new(0, 448, 640, 32)
        ));
        w.minimize();
        assert!(!fullscreen_covers(core::slice::from_ref(&w), 0, output));
        w.restore();
        assert!(w.set_fullscreen(false, output, area));
        assert_eq!(w.geometry(), saved);
        w.toggle_maximize_in(area);
        assert!(w.set_fullscreen(true, output, area));
        let secondary = Rect::new(640, -100, 800, 600);
        w.fit_fullscreen(secondary, secondary);
        assert_eq!(w.client_surface().rect, secondary);
        assert!(!fullscreen_covers(core::slice::from_ref(&w), 0, output));
        assert!(w.set_fullscreen(false, secondary, secondary));
        assert!(w.maximized());
        assert_eq!(w.geometry(), secondary);
        w.set_fullscreen(true, secondary, secondary);
        w.unbind_instance();
        assert!(!w.fullscreen());
    }
    #[test]
    fn fit_normalizes_startup() {
        let mut w = Window::new(Kind::Manager, Rect::new(-40, 460, 80, 40));
        w.fit_to_work_area(640, 480);
        assert_eq!(w.geometry(), Rect::new(0, 352, 180, 96));
        assert_eq!(w.saved_geometry(), w.geometry());
    }
    #[test]
    fn client_minimum_signed_and_tiny_outputs() {
        let mut w = Window::new(Kind::App, Rect::new(100, 80, 300, 160));
        assert!(w.set_min_client_size(240, 170, 640, 480));
        assert_eq!(w.minimum_frame(), (256, 208));
        w.resize_from(
            w.geometry(),
            ResizeHandle::BottomRight,
            -400,
            -400,
            640,
            480,
        );
        assert_eq!((w.current.w, w.current.h), (256, 208));
        w.current.x = -750;
        w.current.y = 50;
        let secondary = Rect::new(-800, 0, 800, 600);
        assert!(w.set_min_client_size_in(300, 220, secondary));
        assert_eq!((w.current.x, w.current.w, w.current.h), (-750, 316, 258));
        let tiny = Rect::new(-100, 50, 120, 80);
        assert!(w.set_min_client_size_in(400, 300, tiny));
        assert_eq!(w.geometry(), tiny);
        assert_eq!(w.minimum_frame(), (180, 96));
        w.toggle_maximize_in(tiny);
        w.toggle_maximize_in(tiny);
        assert_eq!(w.geometry(), tiny);
        w.resize_in(
            w.geometry(),
            ResizeHandle::TopLeft,
            i32::MAX,
            i32::MIN,
            tiny,
        );
        assert_eq!(w.geometry(), tiny);
    }
    fn overlapping() -> [Window; 2] {
        [
            Window::new(Kind::Manager, Rect::new(20, 20, 300, 220)),
            Window::new(Kind::App, Rect::new(80, 60, 300, 220)),
        ]
    }
    #[test]
    fn active_window_is_full_input_barrier() {
        let windows = overlapping();
        assert_eq!(topmost_at(&windows, 0, 100, 100), Some(0));
    }
    #[test]
    fn inactive_windows_follow_compositor_order() {
        let mut windows = overlapping();
        assert_eq!(topmost_at(&windows, windows.len(), 100, 100), Some(1));
        windows[1].minimize();
        assert_eq!(topmost_at(&windows, 1, 100, 100), Some(0));
        assert_eq!(topmost_at(&windows, 0, 500, 400), None);
    }
    #[test]
    fn optional_display_instance_and_close_metadata() {
        let mut w = manager();
        assert_eq!(w.instance_id(), 0);
        w.bind_instance(42);
        assert_eq!(w.instance_id(), 42);
        w.request_close(1234);
        assert_eq!(w.close_requested(), Some(1234));
        w.unbind_instance();
        assert_eq!(w.instance_id(), 0);
        assert_eq!(w.close_requested(), None);
        assert_eq!(w.close_requested_tick, 0);
    }
    #[test]
    fn app_uses_bound_title() {
        let mut w = manager();
        w.bind_app(7, c"Notepad");
        assert_eq!(w.kind(), Kind::App);
        assert_eq!(w.instance_id(), 7);
        assert_eq!(w.title().to_bytes(), b"Notepad");
    }
    #[test]
    fn title_runtime_buffer_preserves_whole_utf8_prefix() {
        let mut w = manager();
        assert_eq!(w.title().to_bytes(), b"");
        w.set_title_bytes(b"Desktop Manager");
        assert_eq!(w.title().to_bytes(), b"Desktop Manager");
        w.set_title(c"123456789012345678901234567890123456789\xc3\xa4");
        assert_eq!(
            w.title().to_bytes(),
            b"123456789012345678901234567890123456789"
        );
        w.set_title(c"12345678901234567890123456789012345678\xc3\xa4");
        assert_eq!(
            w.title().to_bytes(),
            b"12345678901234567890123456789012345678\xc3\xa4"
        );
    }
}
