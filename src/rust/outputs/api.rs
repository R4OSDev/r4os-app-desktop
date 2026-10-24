//! Short calls through the ordinary SDK graphics context. Raw output records
//! retain partial BO/map receipts which Result-only allocation calls hide.
use super::{NativeResult, OutputCatalog, cursor::CursorApi, power::OutputPower};
use r4os_sdk::{abi as a, c_facades as c, graphics::Graphics, storage::Error};
pub struct Native<'g, 's, 'a>(pub &'g Graphics<'s, 'a>);
fn raw(error: Error) -> i32 {
    match error {
        Error::Raw(code) => code,
        _ => a::gfx_output_error_invalid,
    }
}
fn code(slot: c::R4SdkGuiSlot) -> i32 {
    if slot.status < 0 {
        slot.status
    } else if slot.flags & 2 != 0 {
        a::gfx_buffer_error_invalid
    } else {
        slot.bits as i32
    }
}
impl OutputCatalog for Native<'_, '_, '_> {
    fn revision(&mut self) -> NativeResult<a::GfxDisplayRevision> {
        self.0.outputs().revision().map_err(raw)
    }
    fn info(&mut self, index: u32) -> NativeResult<a::GfxOutputInfo> {
        self.0.outputs().info(index).map_err(raw)
    }
    fn power(&mut self, id: &a::GfxOutputId) -> NativeResult<a::GfxOutputPower> {
        self.0.outputs().power(id).map_err(raw)
    }
}
impl OutputPower for Native<'_, '_, '_> {
    fn request_power(&mut self, request: &a::GfxPowerRequest) -> NativeResult<a::GfxPowerRequest> {
        let mut out = a::GfxPowerRequest::default();
        let rc = unsafe {
            code(c::R4SdkGuiDrawContext_gfxPowerRequest(
                0,
                self.0.native_draw(),
                request,
                &mut out,
            ))
        };
        if rc == a::gfx_output_ok {
            Ok(out)
        } else {
            Err(rc)
        }
    }
}
impl crate::input::platform::OutputBrightness for Native<'_, '_, '_> {
    fn brightness(&mut self, id: &a::GfxOutputId) -> NativeResult<a::GfxOutputBrightness> {
        self.0.outputs().brightness(id).map_err(raw)
    }
    fn request_brightness(
        &mut self,
        request: &a::GfxBrightnessRequest,
    ) -> NativeResult<a::GfxBrightnessRequest> {
        let mut out = a::GfxBrightnessRequest::default();
        let rc = unsafe {
            code(c::R4SdkGuiDrawContext_gfxBrightnessRequest(
                0,
                self.0.native_draw(),
                request,
                &mut out,
            ))
        };
        if rc == a::gfx_output_ok {
            Ok(out)
        } else {
            Err(rc)
        }
    }
}
unsafe impl CursorApi for Native<'_, '_, '_> {
    fn supported(&mut self) -> bool {
        let value =
            unsafe { c::R4SdkGuiDrawContext_supportsDisplayCursor(0, self.0.native_draw()) };
        value.status == 0 && value.flags & 3 == 1 && value.bits != 0
    }
    fn status(&mut self, out: &mut a::DisplayCursorStatus) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_displayCursorStatus(
                0,
                self.0.native_draw(),
                out,
            ))
        }
    }
    fn info(&mut self, out: &mut a::DisplayCursorInfo) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_displayCursorInfo(
                0,
                self.0.native_draw(),
                out,
            ))
        }
    }
    fn submit(
        &mut self,
        request: &a::DisplayCursorRequest,
        out: &mut a::DisplayCursorStatus,
    ) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_displayCursorSubmit(
                0,
                self.0.native_draw(),
                request,
                out,
            ))
        }
    }
    fn create(
        &mut self,
        descriptor: &a::GfxBufferDescriptor,
        out: &mut a::GfxBufferReference,
    ) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_gfxBufferCreate(
                0,
                self.0.native_draw(),
                descriptor,
                out,
            ))
        }
    }
    fn map(
        &mut self,
        reference: &a::GfxBufferHandle,
        access: u32,
        offset: u64,
        bytes: u64,
        out: &mut a::GfxBufferMap,
    ) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_gfxBufferMap(
                0,
                self.0.native_draw(),
                reference,
                access,
                offset,
                bytes,
                out,
            ))
        }
    }
    fn unmap(&mut self, lease: &a::GfxBufferHandle) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_gfxBufferUnmap(
                0,
                self.0.native_draw(),
                lease,
            ))
        }
    }
    fn release(&mut self, reference: &a::GfxBufferHandle) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_gfxBufferRelease(
                0,
                self.0.native_draw(),
                reference,
            ))
        }
    }
}
