//! Physical-source sequencing stays independent of remote pointer activity.
//! Full current Contract records are forwarded without remapping HID usages.
use r4os_contract as abi;
#[derive(Default)]
pub struct MouseActivity {
    last: Option<abi::Mouse>,
    motion: Option<[u32; 2]>,
}
impl MouseActivity {
    pub fn changed(&mut self, mouse: abi::Mouse, totals: Option<[u32; 2]>) -> bool {
        let (prior, previous_motion) = (self.last, self.motion);
        self.last = Some(mouse);
        self.motion = totals;
        let Some(old) = prior else {
            return mouse.buttons != 0 || mouse.wheel != 0;
        };
        let fresh = mouse.packets != old.packets;
        let moved = if let (Some(totals), Some(previous)) = (totals, previous_motion) {
            totals != previous
        } else {
            fresh && (mouse.x != old.x || mouse.y != old.y || mouse.dx != 0 || mouse.dy != 0)
        };
        moved || mouse.buttons != old.buttons || (fresh && mouse.wheel != 0)
    }
}
pub fn valid(input: abi::PhysicalKeyEvent, last_sequence: u64) -> bool {
    input.magic == abi::physical_key_magic
        && input.version == abi::physical_key_version
        && input.size as usize == core::mem::size_of::<abi::PhysicalKeyEvent>()
        && input.sequence != 0
        && input.sequence > last_sequence
        && [
            abi::physical_key_kind_down,
            abi::physical_key_kind_up,
            abi::physical_key_kind_reset,
        ]
        .contains(&input.kind)
}
pub fn to_gui_event(input: abi::PhysicalKeyEvent, window_id: i32) -> Option<abi::GuiEvent> {
    let kind = if input.kind == abi::physical_key_kind_down {
        abi::GuiEventKind::physical_key_down
    } else if input.kind == abi::physical_key_kind_up {
        abi::GuiEventKind::physical_key_up
    } else if input.kind == abi::physical_key_kind_reset {
        abi::GuiEventKind::physical_key_reset
    } else {
        return None;
    };
    Some(abi::GuiEvent {
        kind: kind.0,
        window_id,
        key: input.key,
        modifiers: input.modifiers,
        buttons: input.flags,
        tick: input.tick,
        ..abi::GuiEvent::default()
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_keypad_and_right_control_fields_reach_gui_unchanged() {
        let usages = [
            abi::physical_key_usage_keypad_2,
            abi::physical_key_usage_keypad_4,
            abi::physical_key_usage_keypad_6,
            abi::physical_key_usage_keypad_7,
            abi::physical_key_usage_keypad_8,
            abi::physical_key_usage_keypad_9,
            abi::physical_key_usage_right_control,
        ];
        for (i, usage) in usages.into_iter().enumerate() {
            let input = abi::PhysicalKeyEvent {
                kind: abi::physical_key_kind_down,
                key: usage,
                modifiers: if usage == abi::physical_key_usage_right_control {
                    abi::physical_key_modifier_right_control
                } else {
                    0
                },
                flags: abi::physical_key_flag_repeat,
                sequence: i as u64 + 1,
                tick: 100 + i as u64,
                ..abi::PhysicalKeyEvent::default()
            };
            assert!(valid(input, i as u64));
            let event = to_gui_event(input, 7).unwrap();
            assert_eq!(event.kind, abi::GuiEventKind::physical_key_down.0);
            assert_eq!(event.key, usage);
            assert_eq!(event.modifiers, input.modifiers);
            assert_eq!(event.buttons, input.flags);
            assert_eq!(event.tick, input.tick);
        }
    }
    #[test]
    fn original_malformed_order_validation_and_up_transition() {
        let mut input = abi::PhysicalKeyEvent {
            kind: abi::physical_key_kind_up,
            key: abi::physical_key_usage_keypad_8,
            sequence: 9,
            tick: 55,
            ..abi::PhysicalKeyEvent::default()
        };
        assert!(valid(input, 8));
        assert_eq!(
            to_gui_event(input, 2).unwrap().kind,
            abi::GuiEventKind::physical_key_up.0
        );
        assert!(!valid(input, 9));
        input.magic = 0;
        assert!(!valid(input, 0));
        input.magic = abi::physical_key_magic;
        input.kind = 99;
        assert!(!valid(input, 0));
        assert!(to_gui_event(input, 0).is_none());
    }
    #[test]
    fn original_keypad_is_distinct_from_row_and_navigation() {
        assert_ne!(abi::physical_key_usage_keypad_8, 0x25);
        assert_ne!(abi::physical_key_usage_keypad_8, abi::physical_key_usage_up);
        assert_ne!(
            abi::physical_key_usage_right_control,
            abi::physical_key_usage_left_control
        );
    }
    #[test]
    fn original_unchanged_physical_mouse_does_not_undo_remote_input_or_activity() {
        let mut activity = MouseActivity::default();
        let mut mouse = abi::Mouse {
            x: 640,
            y: 360,
            dx: 0,
            dy: 0,
            wheel: 0,
            buttons: 0,
            present: 1,
            reserved: 0,
            packets: 99,
        };
        assert!(!activity.changed(mouse, Some([10, 20])));
        for _ in 0..4 {
            assert!(!activity.changed(mouse, Some([10, 20])));
        }
        mouse.packets += 1;
        assert!(!activity.changed(mouse, Some([10, 20])));
        mouse.buttons = 1;
        assert!(activity.changed(mouse, Some([10, 20])));
        assert!(!activity.changed(mouse, Some([10, 20])));
        mouse.buttons = 0;
        assert!(activity.changed(mouse, Some([10, 20])));
        mouse.packets = 0;
        mouse.wheel = 1;
        assert!(activity.changed(mouse, Some([10, 20])));
        mouse.wheel = 0;
        assert!(!activity.changed(mouse, Some([10, 20])));
        assert!(activity.changed(mouse, Some([11, 20])));
        mouse.packets = 1;
        mouse.dx = 2;
        assert!(activity.changed(mouse, None));
        mouse.dx = 0;
        assert!(!activity.changed(mouse, None));
    }
}
