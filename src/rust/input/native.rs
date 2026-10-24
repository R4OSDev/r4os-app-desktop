//! Short input loans over the admitted general SDK host session. Physical,
//! legacy and remote records keep their native status/sequence distinctions.
use r4os_contract as a;
use r4os_sdk::gui::{Session, raw as c};
pub fn push_console(
    app: &r4os_sdk::App<'_>,
    owner: a::ProgramProcessHandle,
    events: &[a::ConsoleInputEvent],
) -> i32 {
    let Some(table) = app.desktop_api() else {
        return a::err_no_group;
    };
    if table.console_push_events == 0 {
        return a::err_no_fn;
    }
    let Ok(count) = u32::try_from(events.len()) else {
        return -1;
    };
    let call: a::R4DeskFns::console_push_events =
        unsafe { core::mem::transmute(table.console_push_events) };
    // The native contract copies only an accepted prefix during this call.
    unsafe { call(&owner, events.as_ptr(), count) }
}
pub struct Input<'l, 'a, 's> {
    session: &'l mut Session<'a, 's>,
}
impl<'l, 'a, 's> Input<'l, 'a, 's> {
    pub fn new(session: &'l mut Session<'a, 's>) -> Self {
        Self { session }
    }
    pub fn codepoint(&mut self) -> core::result::Result<u32, i32> {
        let slot =
            unsafe { c::R4SdkGuiDesktopContext_readKeyCodepoint(0, self.session.native_desktop()) };
        let rc = code(slot);
        if rc < 0 {
            Err(rc)
        } else {
            Ok(slot.bits as u32)
        }
    }
    pub fn mouse(&mut self, out: &mut a::Mouse) -> i32 {
        unsafe {
            code(c::R4SdkGuiDesktopContext_mouseState(
                0,
                self.session.native_desktop(),
                out,
            ))
        }
    }
    pub fn motion(&mut self, out: &mut a::MouseMotion) -> i32 {
        unsafe {
            code(c::R4SdkGuiDesktopContext_mouseMotion(
                0,
                self.session.native_desktop(),
                out,
            ))
        }
    }
    pub fn physical(&mut self, out: &mut a::PhysicalKeyEvent) -> i32 {
        unsafe {
            code(c::R4SdkGuiDesktopContext_physicalKeyPoll(
                0,
                self.session.native_desktop(),
                out,
            ))
        }
    }
    pub fn remote(&mut self, out: &mut a::RemoteInputEvent) -> i32 {
        unsafe {
            code(c::R4SdkGuiDesktopContext_remoteInputPoll(
                0,
                self.session.native_desktop(),
                out,
            ))
        }
    }
    pub fn hide_legacy_cursor(&mut self) -> i32 {
        unsafe {
            code(c::R4SdkGuiDesktopContext_mouseHide(
                0,
                self.session.native_desktop(),
            ))
        }
    }
    pub fn keyboard_layout(&mut self, out: &mut a::KeyboardLayoutInfo) -> i32 {
        unsafe {
            code(c::R4SdkGuiDesktopContext_keyboardLayoutCurrent(
                0,
                self.session.native_desktop(),
                out,
            ))
        }
    }
    pub fn wait_activity(&mut self, sequence: u64, ticks: u64, out: &mut u64) -> i32 {
        unsafe {
            code(c::R4SdkGuiDesktopContext_desktopActivityWait(
                0,
                self.session.native_desktop(),
                sequence,
                ticks,
                out,
            ))
        }
    }
}
fn code(slot: c::R4SdkGuiSlot) -> i32 {
    if slot.status < 0 {
        slot.status
    } else if slot.flags & 2 != 0 {
        -1
    } else {
        slot.bits as i32
    }
}
