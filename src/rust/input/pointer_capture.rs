//! Desktop pointer policy over exact native control tokens. Raw motion stays
//! independent of output scaling/position; remote absolute motion cannot
//! masquerade as physical relative input.
use crate::{
    hosted::{client::Clients, process::same},
    render::{geometry::Rect, output_geometry::topology::Point},
    windows::{controller::Controller, window::Kind},
};
use r4os_sdk::{
    abi as a,
    gui::{Session, raw as c},
};

pub trait Provider {
    fn control(
        &mut self,
        request: &a::GuiPointerHostRequest,
        out: &mut a::GuiPointerCaptureInfo,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Backend {
        value: a::GuiPointerCaptureInfo,
        events: std::vec::Vec<a::GuiEvent>,
        overflow: bool,
        grants: u32,
    }
    impl Backend {
        fn pending(mode: u32) -> Self {
            Self {
                value: a::GuiPointerCaptureInfo {
                    state: a::gui_pointer_state_pending,
                    mode,
                    capabilities: a::gui_pointer_capability_drag
                        | a::gui_pointer_capability_relative_physical,
                    capture: a::GuiPointerCaptureHandle {
                        id: 9,
                        ..Default::default()
                    },
                    motion_x: u32::MAX - 15,
                    motion_y: 10,
                    ..Default::default()
                },
                events: std::vec::Vec::new(),
                overflow: false,
                grants: 0,
            }
        }
    }
    impl Provider for Backend {
        fn control(
            &mut self,
            r: &a::GuiPointerHostRequest,
            out: &mut a::GuiPointerCaptureInfo,
        ) -> i32 {
            match r.operation {
                a::gui_pointer_host_grant => {
                    self.value.state = a::gui_pointer_state_active;
                    self.grants += 1;
                }
                a::gui_pointer_host_cancel => {
                    self.value.state = a::gui_pointer_state_cancelled;
                    self.value.reason = r.reason;
                }
                a::gui_pointer_host_deliver => {
                    if self.overflow {
                        return a::gui_pointer_error_queue_full;
                    }
                    self.events.push(r.event);
                }
                _ => {}
            }
            *out = self.value;
            0
        }
    }
    const BOUNDS: Rect = Rect::new(0, 0, 640, 480);
    fn start(backend: &mut Backend, buttons: [u8; 2]) -> (Capture, Point) {
        let mut capture = Capture::default();
        let mut cursor = Point { x: 150, y: 250 };
        assert_eq!(capture.bind(backend, false), 0);
        capture.sync(
            backend,
            |_| Ok(Rect::new(100, 200, 320, 240)),
            BOUNDS,
            &mut cursor,
            buttons,
            1,
        );
        assert_eq!(backend.grants, 1);
        assert!(capture.active());
        (capture, cursor)
    }
    fn mouse(x: i32, y: i32, buttons: u8, wheel: i32) -> a::Mouse {
        a::Mouse {
            x,
            y,
            dx: 0,
            dy: 0,
            buttons,
            wheel,
            present: 1,
            reserved: 0,
            packets: 0,
        }
    }
    #[test]
    fn physical_relative_wraps_keeps_full_counts_and_restores_clamped_entry() {
        let mut b = Backend::pending(a::gui_pointer_mode_relative);
        let (mut capture, mut cursor) = start(&mut b, [0, 0]);
        assert!(capture.relative());
        assert!(capture.mouse(
            &mut b,
            mouse(639, 479, 0, 0),
            false,
            0,
            Some([16, 4]),
            &mut cursor,
            BOUNDS,
            2
        ));
        assert!(capture.mouse(
            &mut b,
            mouse(0, 0, 0, 0),
            false,
            0,
            Some([1_048_592, 1_048_580]),
            &mut cursor,
            BOUNDS,
            3
        ));
        assert_eq!(cursor, Point { x: 150, y: 250 });
        assert_eq!(b.events.len(), 2);
        assert_eq!(
            (b.events[0].kind, b.events[0].x, b.events[0].y),
            (a::GuiEventKind::mouse_relative.0, 32, -6)
        );
        assert_eq!((b.events[1].x, b.events[1].y), (1_048_576, 1_048_576));
        b.value.state = a::gui_pointer_state_released;
        assert!(capture.sync(
            &mut b,
            |_| panic!("terminal lease has no target"),
            Rect::new(0, 0, 120, 100),
            &mut cursor,
            [0, 0],
            4
        ));
        assert_eq!(cursor, Point { x: 119, y: 99 });
        assert!(!capture.relative());
        assert!(!capture.mouse(
            &mut b,
            mouse(639, 479, 0, 0),
            false,
            0,
            Some([2_000_000, 2_000_000]),
            &mut cursor,
            BOUNDS,
            5
        ));
        assert_eq!(b.events.len(), 2);
    }
    #[test]
    fn outside_drag_keeps_negative_client_coordinates_and_up_with_wheel() {
        let mut b = Backend::pending(a::gui_pointer_mode_drag);
        let (mut capture, mut cursor) = start(&mut b, [1, 0]);
        capture.mouse(
            &mut b,
            mouse(20, 470, 1, 0),
            false,
            0,
            Some([16, 4]),
            &mut cursor,
            BOUNDS,
            2,
        );
        capture.mouse(
            &mut b,
            mouse(20, 470, 0, -1),
            false,
            0,
            Some([16, 4]),
            &mut cursor,
            BOUNDS,
            3,
        );
        assert_eq!(b.events.len(), 3);
        assert_eq!(
            (b.events[0].kind, b.events[0].x, b.events[0].y),
            (a::GuiEventKind::mouse_move.0, -80, 270)
        );
        assert_eq!(
            (b.events[1].kind, b.events[1].buttons),
            (a::GuiEventKind::mouse_up.0, 0)
        );
        assert_eq!(
            (b.events[2].kind, b.events[2].key as i32),
            (a::GuiEventKind::mouse_wheel.0, -1)
        );
        // Independent sources holding the same button release it only once.
        b.events.clear();
        capture.mouse(
            &mut b,
            mouse(20, 470, 1, 0),
            false,
            0,
            Some([16, 4]),
            &mut cursor,
            BOUNDS,
            4,
        );
        capture.mouse(
            &mut b,
            mouse(20, 470, 1, 0),
            true,
            a::remote_input_kind_mouse_buttons,
            None,
            &mut cursor,
            BOUNDS,
            5,
        );
        capture.mouse(
            &mut b,
            mouse(20, 470, 0, 0),
            false,
            0,
            Some([16, 4]),
            &mut cursor,
            BOUNDS,
            6,
        );
        assert_eq!(b.events.len(), 1);
        capture.mouse(
            &mut b,
            mouse(20, 470, 0, 0),
            true,
            a::remote_input_kind_mouse_buttons,
            None,
            &mut cursor,
            BOUNDS,
            7,
        );
        assert_eq!(b.events.len(), 2);
        assert_eq!(b.events[1].kind, a::GuiEventKind::mouse_up.0);
    }
    #[test]
    fn unsupported_sources_overflow_and_policy_rejection_cancel_without_motion_or_hidden_cursor() {
        let mut b = Backend::pending(a::gui_pointer_mode_relative);
        let (mut capture, mut cursor) = start(&mut b, [0, 0]);
        capture.mouse(
            &mut b,
            mouse(500, 400, 0, 0),
            true,
            a::remote_input_kind_mouse_move,
            None,
            &mut cursor,
            BOUNDS,
            2,
        );
        assert_eq!(b.value.reason, a::gui_pointer_reason_source_unsupported);
        assert!(b.events.is_empty());
        assert!(!capture.relative());
        assert_eq!(cursor, Point { x: 150, y: 250 });
        let mut b = Backend::pending(a::gui_pointer_mode_relative);
        let (mut capture, mut cursor) = start(&mut b, [0, 0]);
        b.overflow = true;
        capture.mouse(
            &mut b,
            mouse(639, 479, 0, 0),
            false,
            0,
            Some([16, 4]),
            &mut cursor,
            BOUNDS,
            2,
        );
        assert_eq!(b.value.reason, a::gui_pointer_reason_input_overflow);
        assert!(!capture.relative());
        assert_eq!(cursor, Point { x: 150, y: 250 });
        assert!(b.events.is_empty());
        let mut b = Backend::pending(a::gui_pointer_mode_relative);
        let mut capture = Capture::default();
        let mut cursor = Point { x: 15, y: 25 };
        assert_eq!(capture.bind(&mut b, false), 0);
        capture.sync(
            &mut b,
            |_| Err(a::gui_pointer_reason_focus_lost),
            BOUNDS,
            &mut cursor,
            [0, 0],
            2,
        );
        assert_eq!(b.grants, 0);
        assert_eq!(b.value.reason, a::gui_pointer_reason_focus_lost);
        assert!(!capture.relative());
        assert_eq!(cursor, Point { x: 15, y: 25 });
    }
}
pub struct Native<'s, 'app, 'storage>(pub &'s mut Session<'app, 'storage>);
impl Provider for Native<'_, '_, '_> {
    fn control(
        &mut self,
        request: &a::GuiPointerHostRequest,
        out: &mut a::GuiPointerCaptureInfo,
    ) -> i32 {
        let value = unsafe {
            c::R4SdkGuiDesktopContext_guiPointerCaptureHost(
                0,
                self.0.native_desktop(),
                request,
                out,
            )
        };
        if value.status < 0 {
            value.status
        } else if value.flags & 2 != 0 {
            a::gui_pointer_error_invalid
        } else {
            value.bits as i32
        }
    }
}
pub fn target(
    windows: &Controller,
    clients: &Clients,
    token: a::GuiPointerCaptureHandle,
    blocked: bool,
) -> Result<Rect, u32> {
    let index =
        usize::try_from(token.window.window_id).map_err(|_| a::gui_pointer_reason_window_closed)?;
    let window = windows
        .windows()
        .get(index)
        .ok_or(a::gui_pointer_reason_window_closed)?;
    let client = clients
        .slots()
        .get(index)
        .ok_or(a::gui_pointer_reason_window_closed)?;
    if window.kind() != Kind::App
        || !same(client.handle(), token.window.owner)
        || window.instance_id() != token.window.owner.instance_id
        || !window.visible()
        || window.close_requested().is_some()
    {
        return Err(a::gui_pointer_reason_window_closed);
    }
    if window.minimized() {
        return Err(a::gui_pointer_reason_minimized);
    }
    if blocked || windows.keyboard_focus() != Controller::window_target(index) {
        return Err(a::gui_pointer_reason_focus_lost);
    }
    if windows.pointer_window().is_some() {
        return Err(a::gui_pointer_reason_wm_conflict);
    }
    Ok(window.client_surface().rect)
}
pub struct Capture {
    info: a::GuiPointerCaptureInfo,
    registered: bool,
    fence: Option<[u32; 2]>,
    relative: bool,
    saved: Point,
    client: Rect,
    totals: [u32; 2],
    buttons: [u8; 2],
}
impl Default for Capture {
    fn default() -> Self {
        Self {
            info: Default::default(),
            registered: false,
            fence: None,
            relative: false,
            saved: Point { x: 0, y: 0 },
            client: Rect::new(0, 0, 0, 0),
            totals: [0; 2],
            buttons: [0; 2],
        }
    }
}
fn clamp(point: Point, bounds: Rect) -> Point {
    Point {
        x: point
            .x
            .clamp(bounds.x, bounds.right().saturating_sub(1).max(bounds.x)),
        y: point
            .y
            .clamp(bounds.y, bounds.bottom().saturating_sub(1).max(bounds.y)),
    }
}
impl Capture {
    pub fn bound(&self) -> bool {
        self.registered
    }
    pub fn relative(&self) -> bool {
        self.relative
    }
    pub fn restoration_motion(&self) -> Option<[u32; 2]> {
        self.fence
    }
    pub fn active(&self) -> bool {
        self.info.state == a::gui_pointer_state_active
    }
    pub fn bind(&mut self, provider: &mut impl Provider, rebind: bool) -> i32 {
        let request = a::GuiPointerHostRequest {
            operation: a::gui_pointer_host_bind,
            reason: if rebind {
                a::gui_pointer_reason_desktop_changed
            } else {
                0
            },
            flags: a::gui_pointer_capability_drag
                | a::gui_pointer_capability_relative_physical
                | a::gui_pointer_capability_absolute_remote_drag
                | a::gui_pointer_capability_wheel_events,
            ..Default::default()
        };
        let mut out = Default::default();
        let rc = provider.control(&request, &mut out);
        self.registered = rc == 0;
        if rebind && rc == 0 {
            self.info = out;
            self.fence = Some([out.motion_x, out.motion_y]);
        }
        rc
    }
    pub fn unbind(
        &mut self,
        provider: &mut impl Provider,
        cursor: &mut Point,
        bounds: Rect,
    ) -> bool {
        let mut out = Default::default();
        let _ = provider.control(
            &a::GuiPointerHostRequest {
                operation: a::gui_pointer_host_unbind,
                ..Default::default()
            },
            &mut out,
        );
        self.registered = false;
        self.info = out;
        self.restore(cursor, bounds)
    }
    fn restore(&mut self, cursor: &mut Point, bounds: Rect) -> bool {
        if !self.relative {
            return false;
        }
        *cursor = clamp(self.saved, bounds);
        self.relative = false;
        true
    }
    fn cancel(
        &mut self,
        provider: &mut impl Provider,
        reason: u32,
        tick: u64,
        cursor: &mut Point,
        bounds: Rect,
    ) -> bool {
        let request = a::GuiPointerHostRequest {
            operation: a::gui_pointer_host_cancel,
            reason,
            capture: self.info.capture,
            event: a::GuiEvent {
                tick,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut out = Default::default();
        if provider.control(&request, &mut out) == 0 {
            self.fence = Some([out.motion_x, out.motion_y]);
            self.info = out;
        } else {
            self.fence = None;
            self.info.state = a::gui_pointer_state_cancelled;
            self.info.reason = reason;
        }
        self.restore(cursor, bounds)
    }
    pub fn reset(
        &mut self,
        provider: &mut impl Provider,
        tick: u64,
        cursor: &mut Point,
        bounds: Rect,
    ) -> bool {
        let repaint = self.cancel(
            provider,
            a::gui_pointer_reason_desktop_changed,
            tick,
            cursor,
            bounds,
        );
        let _ = self.bind(provider, true);
        repaint
    }
    /// All policy is applied before GRANT. The native ACK supplies a coherent,
    /// nonconsuming physical baseline; no warp or first-move delta is invented.
    pub fn sync(
        &mut self,
        provider: &mut impl Provider,
        mut target: impl FnMut(a::GuiPointerCaptureHandle) -> Result<Rect, u32>,
        bounds: Rect,
        cursor: &mut Point,
        buttons: [u8; 2],
        tick: u64,
    ) -> bool {
        if !self.registered {
            return false;
        }
        let mut out = Default::default();
        if provider.control(
            &a::GuiPointerHostRequest {
                operation: a::gui_pointer_host_snapshot,
                flags: if self.relative {
                    a::gui_pointer_host_flag_motion_snapshot
                } else {
                    0
                },
                ..Default::default()
            },
            &mut out,
        ) != 0
        {
            self.fence = None;
            self.info.state = a::gui_pointer_state_cancelled;
            return self.restore(cursor, bounds);
        }
        self.fence = if self.relative {
            Some([out.motion_x, out.motion_y])
        } else {
            None
        };
        let mut repaint = false;
        if self.info.capture.id != out.capture.id || out.state != a::gui_pointer_state_active {
            repaint |= self.restore(cursor, bounds);
        }
        let was_active = self.active() && self.info.capture.id == out.capture.id;
        self.info = out;
        if ![a::gui_pointer_state_pending, a::gui_pointer_state_active].contains(&out.state) {
            return repaint;
        }
        if out.mode == a::gui_pointer_mode_relative
            && out.capabilities & a::gui_pointer_capability_relative_physical == 0
        {
            return repaint
                | self.cancel(
                    provider,
                    a::gui_pointer_reason_source_unsupported,
                    tick,
                    cursor,
                    bounds,
                );
        }
        self.client = match target(out.capture) {
            Ok(client) => client,
            Err(reason) => return repaint | self.cancel(provider, reason, tick, cursor, bounds),
        };
        if out.state == a::gui_pointer_state_pending {
            self.saved = *cursor;
            self.buttons = buttons;
            self.relative = out.mode == a::gui_pointer_mode_relative;
            repaint |= self.relative;
            let request = a::GuiPointerHostRequest {
                operation: a::gui_pointer_host_grant,
                capture: out.capture,
                event: a::GuiEvent {
                    x: cursor.x,
                    y: cursor.y,
                    tick,
                    ..Default::default()
                },
                ..Default::default()
            };
            let rc = provider.control(&request, &mut out);
            if rc != 0 || out.state != a::gui_pointer_state_active {
                if rc == 0 {
                    self.info = out;
                } else {
                    self.info.state = a::gui_pointer_state_cancelled;
                }
                return repaint | self.restore(cursor, bounds);
            }
            self.info = out;
            self.totals = [out.motion_x, out.motion_y];
        } else if !was_active {
            // A restarted host never adopts another host's active policy.
            return repaint
                | self.cancel(
                    provider,
                    a::gui_pointer_reason_desktop_changed,
                    tick,
                    cursor,
                    bounds,
                );
        }
        repaint
    }
    fn send(
        &mut self,
        provider: &mut impl Provider,
        event: a::GuiEvent,
        source: u32,
        cursor: &mut Point,
        bounds: Rect,
    ) -> bool {
        let request = a::GuiPointerHostRequest {
            operation: a::gui_pointer_host_deliver,
            flags: source,
            capture: self.info.capture,
            event,
            ..Default::default()
        };
        let mut out = Default::default();
        let rc = provider.control(&request, &mut out);
        if rc == 0 && out.state == a::gui_pointer_state_active {
            self.info = out;
            true
        } else {
            self.cancel(
                provider,
                if rc == a::gui_pointer_error_queue_full {
                    a::gui_pointer_reason_input_overflow
                } else {
                    a::gui_pointer_reason_window_closed
                },
                event.tick,
                cursor,
                bounds,
            );
            false
        }
    }
    /// Consumes captured packets completely. Order is motion, button-down,
    /// button-up, wheel, so a wheel-bearing packet cannot erase an Up.
    pub fn mouse(
        &mut self,
        provider: &mut impl Provider,
        value: a::Mouse,
        remote: bool,
        remote_kind: u32,
        totals: Option<[u32; 2]>,
        cursor: &mut Point,
        bounds: Rect,
        tick: u64,
    ) -> bool {
        if !self.active() {
            return false;
        }
        if self.relative
            && ((remote && remote_kind == a::remote_input_kind_mouse_move)
                || (!remote && totals.is_none()))
        {
            self.cancel(
                provider,
                a::gui_pointer_reason_source_unsupported,
                tick,
                cursor,
                bounds,
            );
            return true;
        }
        let source = if remote {
            a::gui_pointer_source_remote_absolute
        } else {
            a::gui_pointer_source_physical_relative
        };
        let old = self.buttons[0] | self.buttons[1];
        self.buttons[usize::from(remote)] = value.buttons;
        let buttons = u32::from(self.buttons[0] | self.buttons[1]);
        let down = buttons & !u32::from(old);
        let up = u32::from(old) & !buttons;
        let before = *cursor;
        if !self.relative {
            *cursor = clamp(
                Point {
                    x: value.x,
                    y: value.y,
                },
                bounds,
            );
        }
        let position = if self.relative { self.saved } else { *cursor };
        let base = a::GuiEvent {
            window_id: self.info.capture.window.window_id,
            x: position.x.saturating_sub(self.client.x),
            y: position.y.saturating_sub(self.client.y),
            buttons,
            tick,
            ..Default::default()
        };
        if self.relative && !remote {
            let totals = totals.expect("checked physical relative source");
            let dx = totals[0].wrapping_sub(self.totals[0]) as i32;
            let dy = totals[1].wrapping_sub(self.totals[1]) as i32;
            self.totals = totals;
            if (dx != 0 || dy != 0)
                && !self.send(
                    provider,
                    a::GuiEvent {
                        kind: a::GuiEventKind::mouse_relative.0,
                        x: dx,
                        y: dy,
                        key: source,
                        ..base
                    },
                    source,
                    cursor,
                    bounds,
                )
            {
                return true;
            }
        } else if !self.relative && (before.x != cursor.x || before.y != cursor.y) {
            if !self.send(
                provider,
                a::GuiEvent {
                    kind: a::GuiEventKind::mouse_move.0,
                    ..base
                },
                source,
                cursor,
                bounds,
            ) {
                return true;
            }
        }
        for (kind, changed) in [
            (a::GuiEventKind::mouse_down.0, down),
            (a::GuiEventKind::mouse_up.0, up),
        ] {
            if changed != 0
                && !self.send(
                    provider,
                    a::GuiEvent {
                        kind,
                        key: changed,
                        ..base
                    },
                    source,
                    cursor,
                    bounds,
                )
            {
                return true;
            }
        }
        if value.wheel != 0 {
            let _ = self.send(
                provider,
                a::GuiEvent {
                    kind: a::GuiEventKind::mouse_wheel.0,
                    key: value.wheel as u32,
                    ..base
                },
                source,
                cursor,
                bounds,
            );
        }
        true
    }
}
