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
    /// Stop exact captured input and enqueue its ordered terminal notice before
    /// the close flag/message can make the receiver return from its UI loop.
    pub fn cancel_pointer_for_close(
        &mut self,
        owner: a::ProgramProcessHandle,
        index: usize,
        tick: u64,
    ) {
        let mut info = a::GuiPointerCaptureInfo::default();
        let snapshot = a::GuiPointerHostRequest {
            operation: a::gui_pointer_host_snapshot,
            ..Default::default()
        };
        let rc = unsafe {
            code(c::R4SdkGuiDesktopContext_guiPointerCaptureHost(
                0,
                self.session.native_desktop(),
                &snapshot,
                &mut info,
            ))
        };
        if rc != 0
            || ![a::gui_pointer_state_pending, a::gui_pointer_state_active].contains(&info.state)
            || !super::process::same(info.capture.window.owner, owner)
            || info.capture.window.window_id != index as i32
        {
            return;
        }
        let request = a::GuiPointerHostRequest {
            operation: a::gui_pointer_host_cancel,
            reason: a::gui_pointer_reason_window_closed,
            capture: info.capture,
            event: a::GuiEvent {
                tick,
                ..Default::default()
            },
            ..Default::default()
        };
        let _ = unsafe {
            c::R4SdkGuiDesktopContext_guiPointerCaptureHost(
                0,
                self.session.native_desktop(),
                &request,
                &mut info,
            )
        };
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
    pub fn input(
        &mut self,
        request: &a::GuiHostInputRequest,
        out: &mut a::GuiPointerWindowIdentity,
    ) -> i32 {
        unsafe {
            code(c::R4SdkGuiDesktopContext_guiHostInput(
                0,
                self.session.native_desktop(),
                request,
                out,
            ))
        }
    }
}
