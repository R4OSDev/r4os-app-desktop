//! Short main-task metadata/event calls over the general SDK host session.
//! The session context is never stored in a client snapshot or paint scope.
use core::ffi::CStr;
use r4os_contract as a;
use r4os_sdk::gui::{Session, raw as c};
pub(super) fn code(value: c::R4SdkGuiSlot) -> i32 {
    if value.status < 0 {
        return value.status;
    }
    if value.flags & 2 != 0 {
        return a::gui_frame_error_invalid;
    }
    value.bits as i32
}
pub struct Host<'loan, 'app, 'storage> {
    session: &'loan mut Session<'app, 'storage>,
}
impl<'loan, 'app, 'storage> Host<'loan, 'app, 'storage> {
    pub fn new(session: &'loan mut Session<'app, 'storage>) -> Self {
        Self { session }
    }
    pub fn associate(&mut self, handle: &a::ProgramProcessHandle, index: usize) -> i32 {
        if !super::process::valid(*handle) || index >= 4 {
            return a::program_handle_error_stale;
        }
        unsafe {
            code(c::R4SdkGuiDesktopContext_programSetWindowHandle(
                0,
                self.session.native_desktop(),
                handle,
                index as i32,
            ))
        }
    }
    pub fn publish_info(&mut self, instance: u32, info: &a::GuiWindowInfo) -> i32 {
        unsafe {
            code(c::R4SdkGuiDesktopContext_guiSetWindowInfo(
                0,
                self.session.native_desktop(),
                instance,
                info,
            ))
        }
    }
    pub fn console_host(&mut self, instance: u32, kind: a::ConsoleHostKind) -> i32 {
        unsafe {
            code(c::R4SdkGuiDesktopContext_programSetConsoleHost(
                0,
                self.session.native_desktop(),
                instance,
                kind.0,
            ))
        }
    }
    pub fn take_launch(&mut self, instance: u32) -> Option<a::ProgramHostLaunchRequest> {
        let mut out = a::ProgramHostLaunchRequest::default();
        let result = unsafe {
            code(c::R4SdkGuiDesktopContext_programTakeHostLaunch(
                0,
                self.session.native_desktop(),
                instance,
                (&mut out as *mut a::ProgramHostLaunchRequest).cast(),
            ))
        };
        (result > 0).then_some(out)
    }
    pub fn revision(&mut self, instance: u32) -> core::result::Result<u32, i32> {
        let result = unsafe {
            c::R4SdkGuiDesktopContext_guiRevision(0, self.session.native_desktop(), instance)
        };
        if result.status < 0 {
            Err(result.status)
        } else if result.flags & 2 != 0 {
            Err(a::gui_frame_error_invalid)
        } else {
            Ok(result.bits as u32)
        }
    }
    pub fn console_revision(&mut self, instance: u32) -> core::result::Result<u32, i32> {
        let result = unsafe {
            c::R4SdkGuiDesktopContext_consoleRevision(0, self.session.native_desktop(), instance)
        };
        if result.status < 0 {
            Err(result.status)
        } else if result.flags & 2 != 0 {
            Err(a::gui_frame_error_invalid)
        } else {
            Ok(result.bits as u32)
        }
    }
    pub fn title<'b>(&mut self, instance: u32, buffer: &'b mut [u8; 41]) -> Option<&'b CStr> {
        buffer.fill(0);
        let result = unsafe {
            code(c::R4SdkGuiDesktopContext_guiTitle(
                0,
                self.session.native_desktop(),
                instance,
                buffer.as_mut_ptr(),
                buffer.len() as u64,
            ))
        };
        if result <= 0 {
            return None;
        }
        CStr::from_bytes_until_nul(buffer).ok()
    }
    pub fn minimum_size(&mut self, instance: u32) -> Option<a::GuiSize> {
        let mut size = a::GuiSize::default();
        let result = unsafe {
            code(c::R4SdkGuiDesktopContext_guiMinSize(
                0,
                self.session.native_desktop(),
                instance,
                &mut size,
            ))
        };
        (result >= 0 && (size.w > 0 || size.h > 0)).then_some(size)
    }
    pub fn push(
        &mut self,
        instance: u32,
        event: &a::GuiEvent,
        ordering_sensitive: bool,
        mut yield_task: impl FnMut(),
    ) -> bool {
        let attempts = if ordering_sensitive { 4 } else { 1 };
        for attempt in 0..attempts {
            let result = unsafe {
                code(c::R4SdkGuiDesktopContext_guiPushEvent(
                    0,
                    self.session.native_desktop(),
                    instance,
                    event,
                ))
            };
            if result == 0 {
                return true;
            }
            if result != -3 || attempt + 1 == attempts {
                return false;
            }
            yield_task();
        }
        false
    }
}
