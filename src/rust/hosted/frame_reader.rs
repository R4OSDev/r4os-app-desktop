//! Synchronous snapshot adapter over the general SDK host session. Each call
//! borrows its current native context briefly; no App/context/out pointer is
//! stored by the Desktop adapter or allowed to outlive that call.
use super::frame_snapshot::Reader;
use super::gui::code;
use r4os_contract::{
    DisplayDamageRect, GuiFrameCommand, GuiFrameGenerationInfo, GuiFrameInfo,
    GuiSharedRasterHandle, GuiSharedRasterLease, GuiSharedRasterMap, ProgramProcessHandle,
};
use r4os_sdk::gui::{Session, raw as c};
pub struct SessionReader<'loan, 'app, 'storage> {
    session: &'loan mut Session<'app, 'storage>,
}
impl<'loan, 'app, 'storage> SessionReader<'loan, 'app, 'storage> {
    pub fn new(session: &'loan mut Session<'app, 'storage>) -> Self {
        Self { session }
    }
}
impl Reader for SessionReader<'_, '_, '_> {
    fn info(&mut self, owner: &ProgramProcessHandle, out: &mut GuiFrameInfo) -> i32 {
        // The compiler-projected optional-handle transport borrows this exact
        // record. Native decoding reads it; no mutable Rust loan is aliased.
        let handle = c::R4SdkGuiSlot {
            ptr: core::ptr::from_ref(owner).cast_mut().cast(),
            length: 0,
            bits: 0,
            status: 0,
            flags: 1,
        };
        unsafe {
            code(c::R4SdkGuiDrawContext_guiFrameInfo(
                0,
                self.session.native_draw(),
                handle,
                out,
            ))
        }
    }
    fn read(
        &mut self,
        owner: &ProgramProcessHandle,
        generation: u64,
        commands: &mut [GuiFrameCommand],
        resources: &mut [u8],
        out: &mut GuiFrameInfo,
    ) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_guiFrameRead(
                0,
                self.session.native_draw(),
                owner,
                generation,
                commands.as_mut_ptr(),
                commands.len() as u64,
                resources.as_mut_ptr(),
                resources.len() as u64,
                out,
            ))
        }
    }
    fn generation_info(
        &mut self,
        owner: &ProgramProcessHandle,
        generation: u64,
        out: &mut GuiFrameGenerationInfo,
    ) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_guiFrameGenerationInfo(
                0,
                self.session.native_draw(),
                owner,
                generation,
                out,
            ))
        }
    }
    fn generation_read(
        &mut self,
        owner: &ProgramProcessHandle,
        generation: u64,
        commands: &mut [GuiFrameCommand],
        resources: &mut [u8],
        damage: &mut [DisplayDamageRect],
        out: &mut GuiFrameGenerationInfo,
    ) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_guiFrameGenerationRead(
                0,
                self.session.native_draw(),
                owner,
                generation,
                commands.as_mut_ptr(),
                commands.len() as u64,
                resources.as_mut_ptr(),
                resources.len() as u64,
                damage.as_mut_ptr(),
                damage.len() as u64,
                out,
            ))
        }
    }
    fn acquire(
        &mut self,
        owner: &ProgramProcessHandle,
        generation: u64,
        handle: &GuiSharedRasterHandle,
        raster: u64,
        out: &mut GuiSharedRasterMap,
    ) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_guiSharedRasterAcquire(
                0,
                self.session.native_draw(),
                owner,
                generation,
                handle,
                raster,
                out,
            ))
        }
    }
    fn release(&mut self, lease: &GuiSharedRasterLease) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_guiSharedRasterRelease(
                0,
                self.session.native_draw(),
                lease,
            ))
        }
    }
}
