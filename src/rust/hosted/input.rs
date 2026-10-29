//! Delivery to the currently focused exact client. Console producer queues
//! retain rejected suffixes; GUI metadata and events use short session loans.
use super::{
    gui_pending::{Native, Receipt},
    runtime::State,
};
use crate::{
    input::{keys, physical},
    render::geometry::Dirty,
    windows::{controller::Controller, window::Kind},
};
use r4os_sdk::{
    App, abi as a,
    gui::{Session, raw as key},
};
impl State<'_> {
    pub fn active_console(&self) -> Option<usize> {
        let i = if self.terminal_mode {
            0
        } else {
            self.model.windows.active()
        };
        let win = &self.model.windows.windows()[i];
        (win.kind() == Kind::Terminal
            && win.instance_id() != 0
            && (self.terminal_mode
                || self.model.windows.keyboard_focus() == Controller::window_target(i)))
        .then_some(i)
    }
    pub fn active_gui(&self) -> Option<usize> {
        let i = self.model.windows.active();
        let win = &self.model.windows.windows()[i];
        (!self.terminal_mode
            && win.kind() == Kind::App
            && win.instance_id() != 0
            && win.visible()
            && !win.minimized()
            && self.model.windows.keyboard_focus() == Controller::window_target(i))
        .then_some(i)
    }
    pub fn invalidate_console(&mut self, i: usize, dirty: &mut Dirty) {
        self.consoles[i].invalidate();
        dirty.invalidate(if self.terminal_mode && i == 0 {
            self.bounds
        } else {
            self.model.windows.windows()[i].client_surface().rect
        });
    }
    pub fn scroll_console(&mut self, i: usize, lines: i32, dirty: &mut Dirty) -> bool {
        let Some(win) = self.model.windows.windows().get(i) else {
            return false;
        };
        if win.kind() != Kind::Terminal || win.instance_id() == 0 {
            return false;
        }
        let state = self.consoles[i].state;
        let max = state.scrollback_lines.saturating_sub(state.rows.max(1));
        let next = self.scroll[i].offset.saturating_add_signed(lines).min(max);
        if next != self.scroll[i].offset || (next == 0 && !self.scroll[i].follow_tail) {
            self.scroll[i].offset = next;
            self.scroll[i].follow_tail = next == 0;
            self.invalidate_console(i, dirty);
        }
        true
    }
    pub fn console_scroll_key(&mut self, value: u32, dirty: &mut Dirty) -> bool {
        let Some(i) = self.active_console() else {
            return false;
        };
        let page = self.consoles[i]
            .state
            .rows
            .saturating_sub(1)
            .max(1)
            .min(i32::MAX as u32) as i32;
        match value as i32 {
            key::R4SdkGuiKey_page_up => self.scroll_console(i, page, dirty),
            key::R4SdkGuiKey_page_down => self.scroll_console(i, -page, dirty),
            key::R4SdkGuiKey_end if self.scroll[i].offset != 0 => {
                self.scroll_console(i, -i32::MAX, dirty)
            }
            key::R4SdkGuiKey_home if self.scroll[i].offset != 0 => {
                self.scroll_console(i, i32::MAX, dirty)
            }
            _ => false,
        }
    }
    pub fn console_key(
        &mut self,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        value: u32,
        dirty: &mut Dirty,
    ) -> bool {
        let Some(i) = self.active_console() else {
            return false;
        };
        if !keys::is_app_key(value) {
            return false;
        }
        if self.scroll[i].offset != 0 || !self.scroll[i].follow_tail {
            self.scroll[i].offset = 0;
            self.scroll[i].follow_tail = true;
            self.invalidate_console(i, dirty);
        }
        if self.pending[i]
            .bind(self.model.clients.slots()[i].handle())
            .is_err()
        {
            return false;
        }
        let accepted = if value == key::R4SdkGuiKey_ctrl_v as u32 {
            let mut bytes = [0; 4096];
            let count = session.clipboard_read(&mut bytes).map_or(0, |v| v.len());
            let (mut at, mut previous_cr) = (0, false);
            for n in 0..count {
                let byte = bytes[n];
                if byte == 0 {
                    break;
                }
                let output = match byte {
                    b'\r' => Some(b'\r'),
                    b'\n' if !previous_cr => Some(b'\r'),
                    b'\t' => Some(b' '),
                    0x20..=0x7e | 0x80..=255 => Some(byte),
                    _ => None,
                };
                previous_cr = byte == b'\r';
                if let Some(byte) = output {
                    bytes[at] = byte;
                    at += 1;
                }
            }
            self.pending[i].bytes(&bytes[..at]).unwrap_or(false)
        } else {
            self.pending[i].key(value).unwrap_or(false)
        };
        let _ = self.pending[i]
            .flush(|handle, events| crate::input::native::push_console(app, handle, events));
        accepted
    }
    fn queue_gui(
        &mut self,
        session: &mut Session<'_, '_>,
        i: usize,
        event: &a::GuiEvent,
    ) -> Receipt {
        let owner = self.model.clients.slots()[i].handle();
        if owner.instance_id != self.model.windows.windows()[i].instance_id() {
            return Receipt::Cancelled;
        }
        self.model.gui_pending[i].send(&mut Native(session), owner, i as i32, *event)
    }
    pub fn flush_gui(&mut self, session: &mut Session<'_, '_>) -> bool {
        let mut changed = false;
        for pending in &mut self.model.gui_pending {
            changed |= pending.flush(&mut Native(session));
        }
        changed
    }
    pub fn gui_key(
        &mut self,
        _app: &App<'_>,
        session: &mut Session<'_, '_>,
        value: u32,
        tick: u64,
    ) -> bool {
        let Some(i) = self.active_gui() else {
            return false;
        };
        if !keys::is_app_key(value) {
            return false;
        }
        self.queue_gui(
            session,
            i,
            &a::GuiEvent {
                kind: a::GuiEventKind::key_down.0,
                window_id: i as i32,
                key: value,
                tick,
                ..a::GuiEvent::default()
            },
        );
        true
    }
    pub fn gui_physical(
        &mut self,
        _app: &App<'_>,
        session: &mut Session<'_, '_>,
        input: a::PhysicalKeyEvent,
    ) -> bool {
        let Some(i) = self.active_gui() else {
            return false;
        };
        let Some(event) = physical::to_gui_event(input, i as i32) else {
            return false;
        };
        self.queue_gui(session, i, &event);
        true
    }
    pub fn gui_mouse(
        &mut self,
        _app: &App<'_>,
        session: &mut Session<'_, '_>,
        i: usize,
        kind: a::GuiEventKind,
        x: i32,
        y: i32,
        buttons: u8,
        tick: u64,
    ) -> bool {
        let Some(win) = self.model.windows.windows().get(i) else {
            return false;
        };
        let client = win.client_surface().rect;
        if win.kind() != Kind::App
            || win.instance_id() == 0
            || !win.visible()
            || win.minimized()
            || (kind != a::GuiEventKind::mouse_up && !client.contains(x, y))
        {
            return false;
        }
        self.queue_gui(
            session,
            i,
            &a::GuiEvent {
                kind: kind.0,
                window_id: i as i32,
                x: x - client.x,
                y: y - client.y,
                buttons: buttons as u32,
                tick,
                ..a::GuiEvent::default()
            },
        );
        true
    }
}
