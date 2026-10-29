//! Primary software display transport. Source storage is borrowed only for
//! the native copy; presentation receipts retain their original semantics.
use super::{
    geometry::{MAX_DAMAGE_REGIONS, Rect},
    scene::CpuScene,
};
use r4os_contract as a;
use r4os_sdk::{c_facades as c, graphics::Graphics};
pub struct Display<'g, 's, 'a>(pub &'g Graphics<'s, 'a>);
pub struct Presentation {
    pub canonical: bool,
    pub copied: bool,
    pub presented: bool,
    pub result: a::DisplayPresentResult,
}
impl Presentation {
    pub fn accepted(&self) -> bool {
        self.copied && self.presented
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
impl Display<'_, '_, '_> {
    pub fn size(&self) -> Option<(i32, i32)> {
        let w = unsafe { code(c::R4SdkGuiDrawContext_screenWidth(0, self.0.native_draw())) };
        let h = unsafe { code(c::R4SdkGuiDrawContext_screenHeight(0, self.0.native_draw())) };
        (w > 0 && h > 0).then_some((w, h))
    }
    pub fn revision(&self) -> u32 {
        let slot = unsafe { c::R4SdkGuiDrawContext_displayRevision(0, self.0.native_draw()) };
        if slot.status < 0 || slot.flags & 2 != 0 {
            0
        } else {
            slot.bits as u32
        }
    }
    pub fn begin(&self, damage: Rect) -> i32 {
        if damage.is_empty() {
            return -1;
        }
        unsafe {
            code(c::R4SdkGuiDrawContext_displayBeginFrameRect(
                0,
                self.0.native_draw(),
                damage.x,
                damage.y,
                damage.w as u32,
                damage.h as u32,
            ))
        }
    }
    pub fn capabilities(&self, out: &mut a::DisplayPresentCapabilities) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_displayPresentCapabilities(
                0,
                self.0.native_draw(),
                out,
            ))
        }
    }
    pub fn completion(&self, fence: u64, out: &mut a::DisplayPresentCompletion) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_displayPresentCompletion(
                0,
                self.0.native_draw(),
                fence,
                out,
            ))
        }
    }
    pub fn present(
        &self,
        scene: &CpuScene<'_>,
        regions: &[Rect],
        generation: u64,
        input_tick: u64,
    ) -> Presentation {
        let bounds = scene.full_rect();
        let mut result = a::DisplayPresentResult::default();
        let failed = || Presentation {
            canonical: false,
            copied: false,
            presented: false,
            result: a::DisplayPresentResult::default(),
        };
        if scene.is_layer()
            || bounds.x != 0
            || bounds.y != 0
            || bounds.is_empty()
            || regions.is_empty()
            || regions.len() > MAX_DAMAGE_REGIONS
            || generation == 0
        {
            return failed();
        }
        let mut wire = [a::DisplayDamageRect::default(); MAX_DAMAGE_REGIONS];
        for (i, region) in regions.iter().enumerate() {
            let Some(region) = scene.clip_rect(*region) else {
                return failed();
            };
            wire[i] = a::DisplayDamageRect {
                x: region.x,
                y: region.y,
                w: region.w as u32,
                h: region.h as u32,
            };
        }
        let request = a::DisplayPresentRequest {
            flags: if input_tick != 0 {
                a::display_present_request_flag_input_tick_valid
            } else {
                0
            },
            source_width: bounds.w as u32,
            source_height: bounds.h as u32,
            source_stride_pixels: bounds.w as u32,
            source_generation: generation,
            input_tick,
            ..a::DisplayPresentRequest::default()
        };
        let rc = unsafe {
            code(c::R4SdkGuiDrawContext_displayPresentRegions(
                0,
                self.0.native_draw(),
                &request,
                scene.pixels().as_ptr(),
                scene.pixels().len() as u64,
                wire.as_ptr(),
                regions.len() as u64,
                &mut result,
            ))
        };
        if rc == 0 {
            return Presentation {
                canonical: true,
                copied: true,
                presented: true,
                result,
            };
        }
        // The current canonical provider already selects accelerated/native/
        // bootfb CPU copies under the exact write admission. Legacy blit calls
        // that same provider; its separate Present only resets surface mode.
        // Repeating an error there loses generation/regions without repairing
        // failed admission. Keep it rejected for the runtime's damage retry.
        Presentation { result, ..failed() }
    }
}
