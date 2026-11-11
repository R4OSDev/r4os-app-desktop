//! Run's bounded current filesystem/clipboard operations. Program admission
//! and shell error presentation remain with their respective owners.
use super::environment::Environment;
use crate::{
    input::event::Target,
    shell::{configuration::bytes_z, run_command, state::State},
};
use r4os_sdk::abi as a;
pub fn browse(shell: &mut State<'_>, env: &Environment<'_>) -> bool {
    let Ok(address) = env.app.context().lookup(
        1,
        core::mem::offset_of!(a::generation2::R4XStartR4Sys2, dir_entry) as u32,
    ) else {
        return false;
    };
    let entry: a::R4SysFns::dir_entry = unsafe { core::mem::transmute(address) };
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
pub fn paste(shell: &mut State<'_>, app: &r4os_sdk::App2<'_>) -> bool {
    use r4os_sdk::clipboard_offer::{Client, Format};
    let Ok(mut client) = Client::connect(app.context()) else {
        return false;
    };
    let Ok(snapshot) = client.snapshot() else {
        let _ = client.close();
        return false;
    };
    let mut data = [0; 4096];
    let text = (|| {
        let mut formats = [Format::default(); a::clipboard_offer_max_formats as usize];
        let page = client.enumerate(snapshot, 0, &mut formats).ok()?;
        if page.next != page.total {
            return None;
        }
        // Preserve CLIPSVC's text-view preference while borrowing one exact
        // immutable offer. The Run editor still owns UTF8/path admission.
        let selected = formats[..page.count as usize]
            .iter()
            .filter(|format| {
                matches!(
                    format.format,
                    a::clipboard_offer_format_text_utf8 | a::clipboard_offer_format_text_bytes
                ) && format.total_bytes <= u64::from(a::clipboard_max_text_bytes)
            })
            .min_by_key(|format| {
                format.item * 2 + u32::from(format.format == a::clipboard_offer_format_text_bytes)
            });
        let Some(format) = selected else {
            return Some(0);
        };
        let length = usize::try_from(format.total_bytes).ok()?;
        if length >= data.len() {
            return None;
        }
        let mut offset = 0;
        while offset < length {
            let end = (offset + a::clipboard_offer_read_chunk as usize).min(length);
            let chunk = client
                .read(
                    snapshot,
                    format.item,
                    format.format,
                    offset as u64,
                    &mut data[offset..end],
                )
                .ok()?;
            if chunk.total_bytes != format.total_bytes || chunk.bytes == 0 {
                return None;
            }
            offset += chunk.bytes as usize;
        }
        Some(length)
    })();
    let released = client.release(snapshot).is_ok();
    let _ = client.close();
    match text {
        Some(length) if released => shell.run.append_text(&data[..length]).unwrap_or(false),
        _ => false,
    }
}
pub fn copy(shell: &mut State<'_>, app: &r4os_sdk::App2<'_>, cut: bool) {
    use r4os_sdk::clipboard_offer::Client;
    let Ok(text) = shell.run.text() else {
        return;
    };
    let Ok(mut client) = Client::connect(app.context()) else {
        return;
    };
    let stage = client
        .info()
        .and_then(|info| client.begin(info, text.len() as u64));
    let committed = match stage {
        Ok(stage) => {
            let result = client
                .declare(
                    stage,
                    0,
                    a::clipboard_offer_format_text_utf8,
                    text.len() as u64,
                )
                .and_then(|()| {
                    client.put(
                        stage,
                        0,
                        a::clipboard_offer_format_text_utf8,
                        0,
                        text.len() as u64,
                        text,
                    )
                })
                .and_then(|()| client.commit(stage));
            if result.is_err() {
                // A lost Commit reply can already have published the copy.
                // Preserve the source text and never repeat the mutation.
                let _ = client.abort(stage);
            }
            result.is_ok()
        }
        Err(_) => false,
    };
    let _ = client.close();
    if committed && cut {
        let _ = shell.run.clear();
    }
}
pub fn system(env: &Environment<'_>, action: crate::shell::state::SystemAction) -> i32 {
    use crate::shell::state::SystemAction;
    let Ok(address) = env.app.context().lookup(
        1,
        core::mem::offset_of!(a::generation2::R4XStartR4Sys2, system_power_transition) as u32,
    ) else {
        return a::err_no_fn;
    };
    let call: a::R4SysFns::system_power_transition = unsafe { core::mem::transmute(address) };
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
