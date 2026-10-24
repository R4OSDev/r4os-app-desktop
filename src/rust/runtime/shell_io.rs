//! Run's bounded current filesystem/clipboard operations. Program admission
//! and shell error presentation remain with their respective owners.
use super::environment::Environment;
use crate::{
    input::event::Target,
    shell::{configuration::bytes_z, run_command, state::State},
};
use r4os_sdk::{abi as a, gui::Session};
pub fn browse(shell: &mut State<'_>, env: &Environment<'_>) -> bool {
    let Some(sys) = env.app.system_api().filter(|s| s.dir_entry != 0) else {
        return false;
    };
    let entry: a::R4SysFns::dir_entry = unsafe { core::mem::transmute(sys.dir_entry) };
    let mut path = [0; r4os_sdk::path::FILE_PATH_MAX + 1];
    let mut text = [0; run_command::MAX_INPUT];
    let mut index = shell.run_browse;
    let mut wrapped = false;
    for _ in 0..512 {
        path.fill(0);
        let kind = unsafe {
            entry(
                c"C:\\R4OS\\SOFTWARE\\DESKTOP".as_ptr().cast(),
                index,
                path.as_mut_ptr(),
                path.len() as u32,
            )
        };
        if kind < 0 {
            if kind == -7 || wrapped {
                return false;
            }
            index = 2;
            wrapped = true;
            continue;
        }
        let name = bytes_z(&path);
        if kind == 0 && name.len() >= 4 && name[name.len() - 4..].eq_ignore_ascii_case(b".R4X") {
            let Some(value) = run_command::browse_text(name, &mut text) else {
                return false;
            };
            if !shell.run.set(value).unwrap_or(false) {
                return false;
            }
            shell.run_browse = index + 1;
            shell.dialog_focus = Target::RunInput;
            return true;
        }
        index = index.saturating_add(1);
    }
    false
}
pub fn paste(shell: &mut State<'_>, session: &mut Session<'_, '_>) -> bool {
    let mut data = [0; 4096];
    let capacity = data.len();
    let Ok(text) = session.clipboard_read(&mut data) else {
        return false;
    };
    if text.len() >= capacity {
        return false;
    }
    shell.run.append_text(text).unwrap_or(false)
}
pub fn copy(shell: &mut State<'_>, session: &mut Session<'_, '_>, cut: bool) {
    if let Ok(text) = shell.run.text() {
        if session.clipboard_write(text).is_ok_and(|code| code >= 0) && cut {
            let _ = shell.run.clear();
        }
    }
}
pub fn system(env: &Environment<'_>, action: crate::shell::state::SystemAction) -> i32 {
    use crate::shell::state::SystemAction;
    let Some(sys) = env
        .app
        .system_api()
        .filter(|s| s.system_power_transition != 0)
    else {
        return a::err_no_fn;
    };
    let call: a::R4SysFns::system_power_transition =
        unsafe { core::mem::transmute(sys.system_power_transition) };
    let Ok(time) = env.time() else {
        return a::err_no_fn;
    };
    unsafe {
        call(
            match action {
                SystemAction::Restart => a::power_transition_action_reboot,
                SystemAction::Poweroff => a::power_transition_action_poweroff,
                SystemAction::Halt => a::power_transition_action_halt,
            },
            u64::from(time.hz) * 5,
        )
    }
}
