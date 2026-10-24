//! Four-slot WINSVC mirror and coalesced geometry. Acknowledged service
//! reset is an explicit token; graphics retirement remains with its owner.
use super::{
    service_gate::{Gate, GeometryUpdates},
    window::{Kind, Window},
};
use crate::runtime::service::Host;
use r4os_contract as a;
#[derive(Default)]
pub struct Mirror {
    pub gate: Gate,
    updates: GeometryUpdates,
    mirrored: [bool; 4],
    pub window_count: u32,
    pub focused_window: u32,
    pub focused_instance: u32,
}
pub struct ResetAcknowledged {
    owner: a::ProgramProcessHandle,
    endpoint: u32,
}
impl ResetAcknowledged {
    pub fn owner(&self) -> a::ProgramProcessHandle {
        self.owner
    }
    pub fn endpoint(&self) -> u32 {
        self.endpoint
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Skipped,
    Acknowledged,
    Unavailable,
}
impl Mirror {
    pub fn reset(
        &mut self,
        host: &mut Host<'_>,
        now: u64,
        retry: u64,
    ) -> Option<ResetAcknowledged> {
        self.mirrored = [false; 4];
        self.updates = GeometryUpdates::default();
        if !host.claimed() && host.initialize().is_err() {
            self.gate.mark_unavailable(now, retry);
            return None;
        }
        let result = host.window_record(
            a::window_service_op_restart_cleanup,
            &a::WindowServiceRecord::default(),
        );
        let Ok(result) = result else {
            self.gate.mark_unavailable(now, retry);
            return None;
        };
        self.accept(&result);
        Some(ResetAcknowledged {
            owner: host.owner(),
            endpoint: host.endpoint(),
        })
    }
    pub fn unavailable(&mut self, now: u64, retry: u64) {
        self.mirrored = [false; 4];
        self.updates = GeometryUpdates::default();
        self.gate.mark_unavailable(now, retry);
    }
    pub fn mirrored(&self, index: usize) -> bool {
        self.mirrored.get(index).copied().unwrap_or(false)
    }
    pub fn geometry_due(&mut self, index: usize, now: u64, force: bool) -> bool {
        index < 4 && self.updates.take(index, now, force)
    }
    pub fn queue_geometry(&mut self, index: usize, now: u64, interval: u64) {
        if index < 4 && self.gate.available() && self.mirrored[index] {
            self.updates.queue(index, now, interval);
        }
    }
    pub fn delay(&self, now: u64, limit: u64) -> u64 {
        self.updates.delay(now, limit)
    }
    pub fn send(
        &mut self,
        host: &mut Host<'_>,
        index: usize,
        operation: u16,
        window: &Window,
        active: bool,
        terminal_mode: bool,
        path: &[u8],
        console_path: &[u8],
        now: u64,
        retry: u64,
    ) -> Outcome {
        if !self.gate.available() || index >= 4 {
            return Outcome::Skipped;
        }
        // Lifecycle publication supersedes the queued complete geometry.
        let _ = self.updates.take(index, now, true);
        if operation != a::window_service_op_remove && window.instance_id() == 0 {
            return Outcome::Skipped;
        }
        if operation != a::window_service_op_register && !self.mirrored[index] {
            return Outcome::Skipped;
        }
        let mut record = record(window, index, active, terminal_mode, path, console_path);
        if operation == a::window_service_op_focus {
            record.flags |= a::window_service_flag_focused;
        }
        if operation == a::window_service_op_minimize {
            record.flags |= a::window_service_flag_minimized;
        }
        if operation == a::window_service_op_close {
            record.flags |= a::window_service_flag_closing;
        }
        match host.window_record(operation, &record) {
            Ok(result) => {
                if operation == a::window_service_op_register {
                    self.mirrored[index] = true;
                } else if operation == a::window_service_op_remove {
                    self.mirrored[index] = false;
                }
                self.accept(&result);
                Outcome::Acknowledged
            }
            Err(_) => {
                self.unavailable(now, retry);
                Outcome::Unavailable
            }
        }
    }
    fn accept(&mut self, result: &a::WindowServiceResult) {
        self.gate.mark_available();
        self.window_count = result.window_count;
        self.focused_window = result.focused_window;
        self.focused_instance = result.focused_instance;
    }
}
pub fn record(
    window: &Window,
    index: usize,
    active: bool,
    terminal_mode: bool,
    path: &[u8],
    console_path: &[u8],
) -> a::WindowServiceRecord {
    let mut flags = 0;
    if window.visible() {
        flags |= a::window_service_flag_visible;
    }
    if window.minimized() {
        flags |= a::window_service_flag_minimized;
    }
    if window.maximized() {
        flags |= a::window_service_flag_maximized;
    }
    if active && window.visible() && !window.minimized() {
        flags |= a::window_service_flag_focused;
    }
    if window.close_requested().is_some() {
        flags |= a::window_service_flag_closing;
    }
    if window.fullscreen() || (terminal_mode && index == 0) {
        flags |= a::window_service_flag_fullscreen;
    }
    let kind = match window.kind() {
        Kind::Terminal => {
            flags |= a::window_service_flag_terminal;
            a::window_service_kind_terminal
        }
        Kind::Manager => {
            flags |= a::window_service_flag_gui;
            a::window_service_kind_manager
        }
        Kind::App => {
            flags |= a::window_service_flag_gui;
            a::window_service_kind_gui
        }
    };
    let rect = window.geometry();
    let normal = window.saved_geometry();
    let mut out = a::WindowServiceRecord {
        kind,
        window_id: index as u32,
        instance_id: window.instance_id(),
        flags,
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
        normal_x: normal.x,
        normal_y: normal.y,
        normal_w: normal.w,
        normal_h: normal.h,
        ..a::WindowServiceRecord::default()
    };
    copy(&mut out.title, window.title().to_bytes());
    copy(
        &mut out.path,
        if !path.is_empty() {
            path
        } else if window.kind() == Kind::Terminal {
            console_path
        } else {
            &[]
        },
    );
    out
}
fn copy(out: &mut [u8], input: &[u8]) {
    out.fill(0);
    let count = input
        .iter()
        .position(|b| *b == 0)
        .unwrap_or(input.len())
        .min(out.len() - 1);
    out[..count].copy_from_slice(&input[..count]);
}
