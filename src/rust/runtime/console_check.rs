//! The retained Terminal /INPUTTEST consumer. Two original scalar/clipboard
//! cases use real hosted terminals, their production snapshots and exact
//! completion. No normal event-loop sampler or replacement console parser.
use super::*;
use crate::hosted::process::same;
use r4os_sdk::gui::raw as key;

const UNICODE: &str = "ü€ЁĀ😀";
const CHANGED: &str = "ü€ЁĀ!";

fn contains(bytes: &[u8], wanted: &[u8]) -> bool {
    bytes.windows(wanted.len()).any(|part| part == wanted)
}

fn snapshot_valid(snapshot: &crate::hosted::console_snapshot::Snapshot) -> bool {
    snapshot.valid
        && core::str::from_utf8(snapshot.raw()).is_ok()
        && contains(snapshot.text(), UNICODE.as_bytes())
        && snapshot.lines().iter().all(|line| {
            snapshot
                .text()
                .get(line.offset as usize..line.offset as usize + line.len as usize)
                .and_then(|bytes| core::str::from_utf8(bytes).ok())
                .is_some_and(|text| text.chars().count() == line.columns as usize)
        })
}

pub(super) fn run<'a>(
    desktop: &mut Desktop<'a, '_>,
    env: &Environment<'a>,
    session: &mut Session<'_, '_>,
    host: &mut Host<'_>,
    programs: &Programs<'_>,
    audio: Option<&Audio<'_>>,
) -> i32 {
    for paste in [false, true] {
        let Ok(time) = env.time() else {
            return 1;
        };
        let index = match desktop.hosted.launch(
            programs,
            env.app,
            session,
            crate::hosted::runtime::TERMINAL,
            b"/NOAUTOEXEC",
            b"Input test",
            a::LaunchPolicy::console,
            time.tick,
            &mut desktop.dirty,
        ) {
            Ok(index) => index,
            Err(_) => return 1,
        };
        let handle = desktop.hosted.model.clients.slots()[index].handle();
        let result = (|| {
            for _ in 0..4 {
                if !pump(desktop, env, session, host, programs, audio) {
                    return Err(1);
                }
            }
            let time = env.time().map_err(|_| 1)?;
            desktop
                .hosted
                .focus(index, session, env.app, time.tick, &mut desktop.dirty);
            if paste {
                if !session
                    .clipboard_write("ECHO ü€ЁĀ😀\r\n".as_bytes())
                    .is_ok_and(|code| code >= 0)
                    || !send(desktop, env, session, key::R4SdkGuiKey_ctrl_v as u32)
                {
                    return Err(2);
                }
            } else {
                for value in "ECHO "
                    .chars()
                    .chain(UNICODE.chars())
                    .chain(core::iter::once('\r'))
                {
                    if !send(desktop, env, session, value as u32) {
                        return Err(3);
                    }
                }
            }
            for value in [
                key::R4SdkGuiKey_up as u32,
                key::R4SdkGuiKey_backspace as u32,
                b'!' as u32,
                13,
                key::R4SdkGuiKey_f3 as u32,
                13,
            ] {
                if !send(desktop, env, session, value) {
                    return Err(6);
                }
            }
            for byte in b"ECHO INPUT-END\r" {
                if !send(desktop, env, session, *byte as u32) {
                    return Err(7);
                }
            }
            let mut ready = false;
            for _ in 0..100 {
                if env.app.should_close() || !pump(desktop, env, session, host, programs, audio) {
                    return Err(8);
                }
                let snapshot = &desktop.hosted.consoles[index];
                let bytes = snapshot.raw();
                if contains(bytes, UNICODE.as_bytes()) && contains(bytes, CHANGED.as_bytes()) {
                    if let Some(at) = bytes.windows(9).position(|part| part == b"INPUT-END") {
                        if contains(&bytes[at + 9..], b"INPUT-END") {
                            if !snapshot_valid(snapshot) {
                                return Err(12);
                            }
                            ready = true;
                            break;
                        }
                    }
                }
            }
            if !ready {
                return Err(8);
            }
            for byte in b"EXIT\r" {
                if !send(desktop, env, session, *byte as u32) {
                    return Err(9);
                }
            }
            for _ in 0..100 {
                if !pump(desktop, env, session, host, programs, audio) {
                    return Err(10);
                }
                let client = &desktop.hosted.model.clients.slots()[index];
                if !same(client.handle(), handle) {
                    return if client
                        .completion()
                        .is_some_and(|done| same(done.handle, handle) && done.exit_code == 0)
                    {
                        Ok(())
                    } else {
                        Err(11)
                    };
                }
            }
            Err(10)
        })();
        if let Err(code) = result {
            // Only this check's exact spawned generation may be stopped. Its
            // actual lifecycle/resource retirement precedes host teardown.
            if same(desktop.hosted.model.clients.slots()[index].handle(), handle) {
                let _ = programs.kill(handle);
                while same(desktop.hosted.model.clients.slots()[index].handle(), handle) {
                    let _ = pump(desktop, env, session, host, programs, audio);
                }
            }
            env.app.write("Desktop typed input check failed\n");
            return code;
        }
    }
    env.app
        .write("Desktop typed scalar/clipboard UTF8 input and real history/backspace: OK\n");
    0
}

fn send(
    desktop: &mut Desktop<'_, '_>,
    env: &Environment<'_>,
    session: &mut Session<'_, '_>,
    value: u32,
) -> bool {
    desktop
        .hosted
        .console_key(env.app, session, value, &mut desktop.dirty)
}

fn pump<'a>(
    desktop: &mut Desktop<'a, '_>,
    env: &Environment<'a>,
    session: &mut Session<'_, '_>,
    host: &mut Host<'_>,
    programs: &Programs<'_>,
    audio: Option<&Audio<'_>>,
) -> bool {
    let Ok(time) = env.time() else {
        env.app.sleep(1);
        return false;
    };
    // Original check advances the real owners in 10ms steps, at most100
    // observations per phase. No synthetic timer or independent state mirror.
    env.app.sleep((u64::from(time.hz) / 100).max(1));
    let Ok(time) = env.time() else {
        return false;
    };
    desktop.refresh(env, session, host, programs, audio, time);
    let ok = desktop.render(env, session, time).is_ok();
    env.boot_ready();
    ok
}
