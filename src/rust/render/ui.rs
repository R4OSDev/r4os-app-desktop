//! A short UI paint scope. No process/input/service state is reachable here.
use super::{
    geometry::Rect,
    painter::Painter,
    primitive_assets::Result,
    text::cache::{Cache, Fonts},
};
use crate::shell::style;
pub struct Ui<'a, 'pixels, F: Fonts> {
    pub painter: Painter<'a, 'pixels>,
    cache: &'a mut Cache,
    fonts: &'a mut F,
    font: u32,
}
#[cfg(test)]
#[path = "../../../Tests/RustNative/ui_pixels.rs"]
mod tests;
impl<'a, 'pixels, F: Fonts> Ui<'a, 'pixels, F> {
    pub fn new(painter: Painter<'a, 'pixels>, cache: &'a mut Cache, fonts: &'a mut F) -> Self {
        Self {
            painter,
            cache,
            fonts,
            font: r4os_contract::gui_font_builtin_id,
        }
    }
    pub fn set_font(&mut self, font: u32) {
        self.font = font;
    }
    pub fn fill(&mut self, rect: Rect, rgb: u32) -> Result<()> {
        self.painter.fill(rect, rgb)
    }
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32) -> Result<()> {
        self.painter.rect(x, y, w, h, rgb)
    }
    pub fn outline(&mut self, rect: Rect, rgb: u32) -> Result<()> {
        self.painter.outline(rect, rgb)
    }
    pub fn text(&mut self, x: i32, y: i32, value: &[u8], fg: u32, bg: u32) -> Result<()> {
        // Original zero-terminated UI text has a 4096-byte admission bound;
        // hosted UTF8 resources use the separate length-delimited method.
        let end = value
            .iter()
            .take(4096)
            .position(|b| *b == 0)
            .unwrap_or(value.len().min(4096));
        self.text_slice(
            self.font,
            x,
            y,
            &value[..end],
            fg,
            bg,
            self.painter.bounds(),
        )
    }
    pub fn text_slice(
        &mut self,
        font: u32,
        x: i32,
        y: i32,
        value: &[u8],
        fg: u32,
        bg: u32,
        bounds: Rect,
    ) -> Result<()> {
        self.painter
            .text(self.cache, self.fonts, font, x, y, value, fg, bg, bounds)
    }
    pub fn bevel(&mut self, rect: Rect, pressed: bool) -> Result<()> {
        if rect.is_empty() {
            return Ok(());
        }
        let (hi, lo) = if pressed {
            (style::TASKBAR_DARK, style::TASKBAR_LIGHT)
        } else {
            (style::TASKBAR_LIGHT, style::TASKBAR_DARK)
        };
        self.rect(rect.x, rect.y, rect.w, 1, hi)?;
        self.rect(rect.x, rect.y, 1, rect.h, hi)?;
        self.rect(rect.x, rect.bottom() - 1, rect.w, 1, lo)?;
        self.rect(rect.right() - 1, rect.y, 1, rect.h, lo)
    }
}
