//! Font loading stays in the existing general SDK/R4DRAW provider. The
//! Desktop only owns its bounded layout/coverage cache and pixel policy.
use super::cache::Fonts;
use r4os_contract::{GuiFontInfo, GuiGlyphBitmap};
use r4os_sdk::{
    App,
    gui::{Session, raw as c},
};
pub struct SessionFonts<'loan, 'app, 'storage> {
    session: &'loan mut Session<'app, 'storage>,
    has_rows: bool,
}
impl<'loan, 'app, 'storage> SessionFonts<'loan, 'app, 'storage> {
    pub fn new(session: &'loan mut Session<'app, 'storage>, app: &App<'_>) -> Self {
        let has_rows = app
            .drawing_api()
            .is_some_and(|table| table.font_glyph_row != 0);
        Self { session, has_rows }
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
impl Fonts for SessionFonts<'_, '_, '_> {
    fn revision(&mut self) -> u32 {
        let slot = unsafe { c::R4SdkGuiDrawContext_fontRevision(0, self.session.native_draw()) };
        if slot.status < 0 || slot.flags & 2 != 0 {
            0
        } else {
            slot.bits as u32
        }
    }
    fn has_glyph_row(&self) -> bool {
        self.has_rows
    }
    fn info(&mut self, font: u32, out: &mut GuiFontInfo) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_fontInfo(
                0,
                self.session.native_draw(),
                font,
                out,
            ))
        }
    }
    fn bitmap(&mut self, font: u32, scalar: u32, out: &mut GuiGlyphBitmap) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_fontGlyphBitmap(
                0,
                self.session.native_draw(),
                font,
                scalar,
                out,
            ))
        }
    }
    fn row(&mut self, font: u32, scalar: u32, row: u32) -> u64 {
        let slot = unsafe {
            c::R4SdkGuiDrawContext_fontGlyphRow(0, self.session.native_draw(), font, scalar, row)
        };
        if slot.status < 0 || slot.flags & 2 != 0 {
            0
        } else {
            slot.bits
        }
    }
}
