//! Shell interaction changes only shell state. Effects carry copied values to
//! the process, audio, storage and window owners after the state loan ends.
use super::{
    launch::{self, Launch},
    menu_controller::Direction,
    message_box::Buttons,
    start_menu::Kind,
    state::{Dialog, State, SystemAction},
    tasks::PAGE_ROWS,
};
use crate::input::{event::Target, keys::is_text_key};
use r4os_sdk::{abi::LaunchPolicy, gui::raw as key};

pub enum Action {
    None,
    Launch(Launch),
    RunSubmit,
    RunBrowse,
    RunPaste,
    RunCopy {
        cut: bool,
    },
    RunInvalid,
    System(SystemAction),
    ScreenOff,
    TerminalMode,
    ShowDesktop,
    KeyboardLayout,
    VolumeStep(i32),
    VolumePercent(u8),
    VolumeMute,
    VolumeOutput(Option<usize>),
    VolumePage(i32),
    VolumePointer {
        x: i32,
        commit: bool,
    },
    LayoutChanged,
    Tray {
        identity: super::tray::Identity,
        kind: u16,
        wheel: i32,
        x: i32,
        y: i32,
    },
    Window(Target),
    Desktop(usize),
    QuickComputer,
    ReloadSettings,
}
fn fixed(path: &[u8], title: &[u8]) -> Action {
    Launch::new(path, &[], title, LaunchPolicy::gui)
        .map(Action::Launch)
        .unwrap_or(Action::None)
}
pub fn activate(state: &mut State<'_>, target: Target) -> Action {
    match target {
        Target::StartButton => {
            let open = !state.menu_control.view().open;
            state.close_top();
            state.menu_control.set_open(open);
        }
        Target::MenuPrograms | Target::MenuProgramsInternet => {
            state.menu_control.hover(&state.menu, target);
            state.menu_control.navigate(&state.menu, Direction::Right);
        }
        Target::MenuRun => state.open_run(),
        Target::MenuTasks => {
            state.close_top();
            state.dialog = Dialog::Tasks;
            state.dialog_focus = Target::TaskOverviewOk;
            state.task_scroll = 0;
        }
        Target::MenuSettings => {
            // A configured submenu is navigated; the action form opens the
            // existing read-only settings summary.
            if launch::menu_entry(&state.menu, target).is_some_and(|v| v.kind == Kind::Submenu) {
                state.menu_control.hover(&state.menu, target);
                state.menu_control.navigate(&state.menu, Direction::Right);
            } else {
                state.close_top();
                state.dialog = Dialog::Settings;
                state.dialog_focus = Target::SettingsOk;
            }
        }
        Target::MenuRestart => state.confirm(SystemAction::Restart),
        Target::MenuPoweroff => state.confirm(SystemAction::Poweroff),
        Target::MenuHalt => state.confirm(SystemAction::Halt),
        Target::MenuScreenOff => {
            state.close_top();
            return Action::ScreenOff;
        }
        Target::MenuTerminalMode => {
            state.close_top();
            return Action::TerminalMode;
        }
        Target::RunInput => state.dialog_focus = target,
        Target::RunBrowse => return Action::RunBrowse,
        Target::RunOk => return Action::RunSubmit,
        Target::MessageYes => {
            if let Dialog::Confirm(action) = state.dialog {
                return Action::System(action);
            }
            state.close_top();
        }
        Target::SettingsOk => {
            state.close_top();
            return Action::ReloadSettings;
        }
        Target::RunClose
        | Target::RunCancel
        | Target::RunBackdrop
        | Target::MessageClose
        | Target::MessageOk
        | Target::MessageNo
        | Target::MessageBackdrop
        | Target::SettingsCancel
        | Target::TaskOverviewClose
        | Target::TaskOverviewOk
        | Target::StartMenuBackdrop
        | Target::TimeMenuBackdrop
        | Target::VolumePopupBackdrop => state.close_top(),
        Target::TaskbarVolume => {
            let open = !state.audio.ui.popup_open;
            state.close_top();
            state.audio.set_popup(open);
        }
        Target::VolumePopupMute => return Action::VolumeMute,
        Target::VolumeOutputAuto => return Action::VolumeOutput(None),
        Target::VolumeOutput0
        | Target::VolumeOutput1
        | Target::VolumeOutput2
        | Target::VolumeOutput3 => {
            return Action::VolumeOutput(Some(target as usize - Target::VolumeOutput0 as usize));
        }
        Target::VolumeOutputPrevious => return Action::VolumePage(-1),
        Target::VolumeOutputNext => return Action::VolumePage(1),
        Target::TaskbarKeyboardLayout => return Action::KeyboardLayout,
        Target::TaskbarClock | Target::TimeMenuClock => {
            state.time_menu = false;
            return fixed(b"C:\\R4OS\\SOFTWARE\\DESKTOP\\CLOCK.R4X", b"Clock");
        }
        Target::TimeMenuSettings => {
            state.time_menu = false;
            return fixed(
                b"C:\\R4OS\\SOFTWARE\\DESKTOP\\TIMESET.R4X",
                b"Time Settings",
            );
        }
        Target::QuickShowDesktop => return Action::ShowDesktop,
        Target::QuickComputer => return Action::QuickComputer,
        _ => {
            let id = target as u16;
            if (80..=123).contains(&id) {
                return Action::Window(target);
            }
            if (150..=181).contains(&id) {
                return Action::Desktop(id as usize - Target::DesktopIcon1 as usize);
            }
            if let Some(value) = launch::menu_entry(&state.menu, target) {
                let copied = Launch::menu(value).ok();
                state.close_top();
                return copied.map(Action::Launch).unwrap_or(Action::None);
            }
        }
    }
    Action::None
}
fn cycle_focus(state: &mut State<'_>, backwards: bool) {
    use Target::*;
    state.dialog_focus = match state.dialog {
        Dialog::Run => {
            let targets = [RunInput, RunBrowse, RunOk, RunCancel];
            let i = targets
                .iter()
                .position(|&v| v == state.dialog_focus)
                .unwrap_or(0);
            targets[(i + if backwards { 3 } else { 1 }) % 4]
        }
        Dialog::Settings => {
            if state.dialog_focus == SettingsOk {
                SettingsCancel
            } else {
                SettingsOk
            }
        }
        Dialog::Tasks => TaskOverviewOk,
        Dialog::Message | Dialog::Confirm(_) => match state.message.view().buttons {
            Buttons::Ok => MessageOk,
            Buttons::OkCancel => {
                if state.dialog_focus == MessageOk {
                    MessageNo
                } else {
                    MessageOk
                }
            }
            Buttons::YesNo => {
                if state.dialog_focus == MessageYes {
                    MessageNo
                } else {
                    MessageYes
                }
            }
        },
        Dialog::None => state.dialog_focus,
    };
    state.message.focus = state.dialog_focus;
}
pub fn task_scroll(state: &mut State<'_>, delta: i32, total: usize) {
    let limit = total.saturating_sub(PAGE_ROWS);
    state.task_scroll = state
        .task_scroll
        .saturating_add_signed(delta as isize)
        .min(limit);
}
/// None leaves delivery to the window owner; Some(None) consumed the key.
pub fn key(state: &mut State<'_>, value: u32, tasks: usize) -> Option<Action> {
    let value = i32::try_from(value).ok()?;
    if state.audio.ui.popup_open {
        return Some(match value {
            key::R4SdkGuiKey_escape => {
                state.close_top();
                Action::None
            }
            key::R4SdkGuiKey_left | key::R4SdkGuiKey_down => Action::VolumeStep(-1),
            key::R4SdkGuiKey_right | key::R4SdkGuiKey_up => Action::VolumeStep(1),
            key::R4SdkGuiKey_home => Action::VolumePercent(0),
            key::R4SdkGuiKey_end => Action::VolumePercent(100),
            32 => Action::VolumeMute,
            _ => return None,
        });
    }
    if state.dialog != Dialog::None {
        match value {
            0x86 => state.close_top(),
            key::R4SdkGuiKey_escape => state.close_top(),
            key::R4SdkGuiKey_tab | key::R4SdkGuiKey_shift_tab => {
                cycle_focus(state, value == key::R4SdkGuiKey_shift_tab)
            }
            10 | key::R4SdkGuiKey_enter | 32 if state.dialog != Dialog::Run || value != 32 => {
                let target = match state.dialog {
                    Dialog::Run => match state.dialog_focus {
                        Target::RunBrowse | Target::RunCancel => state.dialog_focus,
                        _ => Target::RunOk,
                    },
                    Dialog::Tasks => Target::TaskOverviewOk,
                    _ => state.dialog_focus,
                };
                return Some(activate(state, target));
            }
            _ if state.dialog == Dialog::Run => match value {
                key::R4SdkGuiKey_ctrl_c | key::R4SdkGuiKey_ctrl_x
                    if state.dialog_focus == Target::RunInput =>
                {
                    return Some(Action::RunCopy {
                        cut: value == key::R4SdkGuiKey_ctrl_x,
                    });
                }
                key::R4SdkGuiKey_ctrl_v if state.dialog_focus == Target::RunInput => {
                    return Some(Action::RunPaste);
                }
                key::R4SdkGuiKey_backspace if state.dialog_focus == Target::RunInput => {
                    let _ = state.run.backspace();
                }
                _ if is_text_key(value as u32) => {
                    state.dialog_focus = Target::RunInput;
                    if !state.run.append(value as u32).unwrap_or(false) {
                        return Some(Action::RunInvalid);
                    }
                }
                _ => return None,
            },
            _ if state.dialog == Dialog::Tasks => {
                let delta = match value {
                    key::R4SdkGuiKey_up => -1,
                    key::R4SdkGuiKey_down => 1,
                    key::R4SdkGuiKey_page_up => -(PAGE_ROWS as i32),
                    key::R4SdkGuiKey_page_down => PAGE_ROWS as i32,
                    key::R4SdkGuiKey_home => i32::MIN,
                    key::R4SdkGuiKey_end => i32::MAX,
                    _ => return None,
                };
                task_scroll(state, delta, tasks);
            }
            _ => return None,
        }
        return Some(Action::None);
    }
    if state.menu_control.view().open {
        match value {
            key::R4SdkGuiKey_escape => state.close_top(),
            key::R4SdkGuiKey_up
            | key::R4SdkGuiKey_down
            | key::R4SdkGuiKey_left
            | key::R4SdkGuiKey_right => {
                let direction = match value {
                    key::R4SdkGuiKey_up => Direction::Up,
                    key::R4SdkGuiKey_down => Direction::Down,
                    key::R4SdkGuiKey_left => Direction::Left,
                    _ => Direction::Right,
                };
                state.menu_control.navigate(&state.menu, direction);
            }
            10 | key::R4SdkGuiKey_enter => {
                return Some(activate(
                    state,
                    state.menu_control.selected_target(&state.menu),
                ));
            }
            _ => return None,
        }
        return Some(Action::None);
    }
    None
}
