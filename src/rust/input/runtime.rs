//! Input sources keep independent sequences and button histories. A stale
//! physical mouse snapshot cannot overwrite a newer remote pointer position.
use super::{
    event::{self, Target},
    keys,
    native::Input,
    physical::{self, MouseActivity},
};
use crate::{
    outputs::manager::Manager,
    render::{geometry::Rect, output_geometry::topology::Point},
    shell::tray::Identity,
};
use r4os_sdk::{
    abi as a,
    gui::{Session, raw as key},
};
#[derive(Clone, Copy)]
pub enum Sample {
    Activity,
    Key {
        key: u32,
        modifiers: u8,
    },
    Physical(a::PhysicalKeyEvent),
    Mouse {
        value: a::Mouse,
        remote: bool,
        motion: Option<[u32; 2]>,
        remote_kind: u32,
    },
}
pub enum Poll {
    Empty,
    Ignored,
    Event(Sample),
}
#[derive(Default)]
pub struct Stats {
    pub remote_events: u32,
    pub remote_keys: u32,
    pub remote_mouse: u32,
    pub physical_events: u64,
    pub physical_forwarded: u64,
    pub physical_invalid: u64,
    pub cursor_moves: u64,
}
#[derive(Default)]
pub struct State {
    pub cursor: Point,
    pub pointer_capture: super::pointer_capture::Capture,
    pub hover: Target,
    pub pressed: Target,
    pub tray_hover: Identity,
    pub tray_pressed: Identity,
    pub right_tray: Identity,
    pub physical_sequence: u64,
    pub remote_sequence: u32,
    pub physical_buttons: u8,
    pub remote_buttons: u8,
    pub last_input_ns: u64,
    pub down_tick: u64,
    pub down_target: Target,
    pub double_click: bool,
    pub repeat_tick: u64,
    pub repeat_armed: bool,
    pub event: event::Event,
    pub stats: Stats,
    mouse: MouseActivity,
    pub next_event: u32,
}
pub fn modifiers(key: u32) -> u8 {
    match key {
        0x83 => event::CTRL,
        0x84 => event::SHIFT,
        0x85 | 0x86 => event::ALT,
        0x87 => event::ALT | event::SHIFT,
        _ => 0,
    }
}
fn empty_mouse() -> a::Mouse {
    a::Mouse {
        x: 0,
        y: 0,
        dx: 0,
        dy: 0,
        wheel: 0,
        buttons: 0,
        present: 0,
        reserved: 0,
        packets: 0,
    }
}
impl State {
    pub fn remote(&mut self, session: &mut Session<'_, '_>) -> Poll {
        let mut value = a::RemoteInputEvent::default();
        if Input::new(session).remote(&mut value) <= 0 {
            return Poll::Empty;
        }
        if value.magic != a::remote_input_magic || value.version != a::remote_input_version {
            return Poll::Ignored;
        }
        self.stats.remote_events = self.stats.remote_events.wrapping_add(1);
        self.remote_sequence = value.sequence;
        match value.kind {
            a::remote_input_kind_key_down => {
                self.stats.remote_keys = self.stats.remote_keys.wrapping_add(1);
                if !keys::is_app_key(value.key) && value.key != key::R4SdkGuiKey_start_menu as u32 {
                    return Poll::Event(Sample::Activity);
                }
                let mut flags = modifiers(value.key);
                if value.modifiers & a::remote_input_modifier_ctrl != 0 {
                    flags |= event::CTRL;
                }
                if value.modifiers & a::remote_input_modifier_shift != 0 {
                    flags |= event::SHIFT;
                }
                if value.modifiers & a::remote_input_modifier_alt != 0 {
                    flags |= event::ALT;
                }
                Poll::Event(Sample::Key {
                    key: value.key,
                    modifiers: flags,
                })
            }
            a::remote_input_kind_key_up => {
                self.stats.remote_keys = self.stats.remote_keys.wrapping_add(1);
                Poll::Event(Sample::Activity)
            }
            a::remote_input_kind_mouse_move
            | a::remote_input_kind_mouse_buttons
            | a::remote_input_kind_mouse_wheel => {
                self.stats.remote_mouse = self.stats.remote_mouse.wrapping_add(1);
                Poll::Event(Sample::Mouse {
                    remote: true,
                    motion: None,
                    remote_kind: value.kind,
                    value: a::Mouse {
                        x: value.x,
                        y: value.y,
                        dx: value.x.saturating_sub(self.cursor.x),
                        dy: value.y.saturating_sub(self.cursor.y),
                        wheel: value.wheel,
                        buttons: value.buttons as u8,
                        present: 1,
                        reserved: 0,
                        packets: u64::from(value.sequence),
                    },
                })
            }
            _ => Poll::Event(Sample::Activity),
        }
    }
    pub fn physical(&mut self, session: &mut Session<'_, '_>) -> Poll {
        let mut input = a::PhysicalKeyEvent::default();
        if Input::new(session).physical(&mut input) <= 0 {
            return Poll::Empty;
        }
        self.stats.physical_events = self.stats.physical_events.wrapping_add(1);
        if !physical::valid(input, self.physical_sequence) {
            self.stats.physical_invalid = self.stats.physical_invalid.wrapping_add(1);
            return Poll::Ignored;
        }
        self.physical_sequence = input.sequence;
        Poll::Event(Sample::Physical(input))
    }
    pub fn keyboard(&mut self, session: &mut Session<'_, '_>) -> Poll {
        match Input::new(session).codepoint() {
            Ok(value) if value != 0 => Poll::Event(Sample::Key {
                key: value,
                modifiers: modifiers(value),
            }),
            _ => Poll::Empty,
        }
    }
    pub fn mouse(
        &mut self,
        session: &mut Session<'_, '_>,
        outputs: &mut Manager<'_>,
        bounds: Rect,
    ) -> Poll {
        let mut value = empty_mouse();
        let mut sample = a::MouseMotion {
            version: 1,
            size: core::mem::size_of::<a::MouseMotion>() as u32,
            mouse: empty_mouse(),
            motion_x: 0,
            motion_y: 0,
        };
        let mut motion = None;
        if Input::new(session).motion(&mut sample) == 0 {
            // Always establish a requested restore fence, even in an otherwise
            // unchanged packet. This is the normal single consuming read.
            let point = outputs.pointer(sample, self.cursor, bounds);
            if !self
                .mouse
                .changed(sample.mouse, Some([sample.motion_x, sample.motion_y]))
            {
                return Poll::Empty;
            }
            motion = Some([sample.motion_x, sample.motion_y]);
            value = sample.mouse;
            value.x = point.x;
            value.y = point.y;
        } else {
            if Input::new(session).mouse(&mut value) < 0 || !self.mouse.changed(value, None) {
                return Poll::Empty;
            }
        }
        Poll::Event(Sample::Mouse {
            value,
            remote: false,
            motion,
            remote_kind: 0,
        })
    }
    pub fn initial(&mut self, session: &mut Session<'_, '_>, bounds: Rect) {
        let mut value = empty_mouse();
        if Input::new(session).mouse(&mut value) >= 0 {
            self.cursor = Point {
                x: value.x.clamp(bounds.x, bounds.right() - 1),
                y: value.y.clamp(bounds.y, bounds.bottom() - 1),
            };
            self.physical_buttons = value.buttons;
            let _ = self.mouse.changed(value, None);
        }
    }
}
