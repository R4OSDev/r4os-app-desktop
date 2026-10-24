//! Drawing admission shared by shell/chrome and hosted command replay.
//! CPU pixels and primitive recording receive short, disjoint loans. No hook
//! stores an exclusive scene pointer across a library/provider call.
use super::{
    geometry::Rect,
    pixels::PixelBytes,
    primitive_assets::{Error, Result},
    primitive_frame::Frame,
    primitive_image::{Format, Paint, Picture, View},
    scene::{CpuScene, intersect},
    text::cache::{Cache as FontCache, Fonts},
};

pub struct Painter<'a, 'pixels> {
    scene: Option<&'a mut CpuScene<'pixels>>,
    recording: Option<&'a mut Frame>,
    bounds: Rect,
    base_clip: Rect,
    clip: Rect,
}
impl<'a, 'pixels> Painter<'a, 'pixels> {
    pub fn cpu(scene: &'a mut CpuScene<'pixels>) -> Self {
        let bounds = scene.full_rect();
        let clip = scene.paint_bounds();
        Self {
            scene: Some(scene),
            recording: None,
            bounds,
            base_clip: clip,
            clip,
        }
    }
    /// The layer owner starts/ends `recording` outside this painting loan.
    pub fn recorded(
        scene: Option<&'a mut CpuScene<'pixels>>,
        recording: &'a mut Frame,
        bounds: Rect,
        clip: Rect,
    ) -> Self {
        let clip = intersect(bounds, clip).unwrap_or(Rect::ZERO);
        Self {
            scene,
            recording: Some(recording),
            bounds,
            base_clip: clip,
            clip,
        }
    }
    pub fn bounds(&self) -> Rect {
        self.bounds
    }
    /// Reborrow this current layer for a UI scope. Neither scene nor command
    /// recorder is aliased, and the original clip remains the admission bound.
    pub fn reborrow(&mut self) -> Painter<'_, 'pixels> {
        Painter {
            scene: self.scene.as_deref_mut(),
            recording: self.recording.as_deref_mut(),
            bounds: self.bounds,
            base_clip: self.base_clip,
            clip: self.clip,
        }
    }
    pub fn clip(&self) -> Rect {
        self.clip
    }
    pub fn set_clip(&mut self, rect: Rect) {
        self.clip = intersect(self.base_clip, rect).unwrap_or(Rect::ZERO);
    }
    pub fn visible(&self, rect: Rect) -> bool {
        intersect(self.clip, rect).is_some()
    }
    fn apply(&mut self, operation: Paint<'_>) -> Result<()> {
        if let Some(recording) = &mut self.recording {
            if let Some(scene) = self.scene.as_deref_mut() {
                let old = scene.paint_clip;
                scene.set_paint_clip(self.clip);
                let result = recording.paint(Some(scene), operation);
                scene.paint_clip = old;
                result
            } else {
                recording.paint(None, operation)
            }
        } else if let Some(scene) = &mut self.scene {
            // Scene clipping is temporarily narrowed, without losing its
            // existing incremental repaint boundary after this operation.
            let old = scene.paint_clip;
            scene.set_paint_clip(self.clip);
            Frame::software(scene, operation);
            scene.paint_clip = old;
            Ok(())
        } else {
            Err(Error::State)
        }
    }
    pub fn clear(&mut self, rect: Rect) -> Result<()> {
        let Some(rect) = intersect(self.clip, rect) else {
            return Ok(());
        };
        self.apply(Paint::Clear(rect))
    }
    pub fn fill(&mut self, rect: Rect, rgb: u32) -> Result<()> {
        let Some(rect) = intersect(self.clip, rect) else {
            return Ok(());
        };
        self.apply(Paint::Fill { rect, rgb })
    }
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32) -> Result<()> {
        self.fill(Rect::new(x, y, w, h), rgb)
    }
    pub fn picture(&mut self, mut picture: Picture<'_>) -> Result<()> {
        let Some(clip) = intersect(self.clip, picture.clip) else {
            return Ok(());
        };
        if !picture.view.valid() {
            return Err(Error::Invalid);
        }
        picture.clip = clip;
        self.apply(Paint::Picture(picture))
    }
    pub fn xrgb(&mut self, x: i32, y: i32, w: u32, h: u32, pixels: PixelBytes<'_>) -> Result<()> {
        if w > i32::MAX as u32 || h > i32::MAX as u32 {
            return Err(Error::Bounds);
        }
        let viewport = Rect::new(x, y, w as i32, h as i32);
        self.picture(Picture {
            view: View::new(Format::Xrgb, w, h, w as usize * 4, pixels.bytes()),
            viewport,
            clip: viewport,
            source_x: 0,
            source_y: 0,
            guest_w: w,
            guest_h: h,
        })
    }
    pub fn text(
        &mut self,
        cache: &mut FontCache,
        fonts: &mut impl Fonts,
        font: u32,
        x: i32,
        y: i32,
        value: &[u8],
        fg: u32,
        bg: u32,
        bounds: Rect,
    ) -> Result<()> {
        let Some(bounds) = intersect(self.clip, bounds) else {
            return Ok(());
        };
        if self.recording.is_none() {
            let scene = self.scene.as_deref_mut().ok_or(Error::State)?;
            cache.paint_cpu(fonts, font, x, y, value, fg, bg, bounds, scene)
        } else {
            cache.paint(
                fonts,
                font,
                x,
                y,
                value,
                fg,
                bg,
                bounds,
                self.bounds,
                |op| self.apply(op),
            )
        }
    }
    pub fn outline(&mut self, rect: Rect, rgb: u32) -> Result<()> {
        if rect.is_empty() {
            return Ok(());
        }
        self.rect(rect.x, rect.y, rect.w, 1, rgb)?;
        self.rect(rect.x, rect.bottom() - 1, rect.w, 1, rgb)?;
        self.rect(rect.x, rect.y, 1, rect.h, rgb)?;
        self.rect(rect.right() - 1, rect.y, 1, rect.h, rgb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use r4os_contract::{GuiFontInfo, GuiGlyphBitmap};
    struct Builtin;
    impl Fonts for Builtin {
        fn revision(&mut self) -> u32 {
            1
        }
        fn has_glyph_row(&self) -> bool {
            false
        }
        fn info(&mut self, _: u32, _: &mut GuiFontInfo) -> i32 {
            -1
        }
        fn bitmap(&mut self, _: u32, _: u32, _: &mut GuiGlyphBitmap) -> i32 {
            -1
        }
        fn row(&mut self, _: u32, _: u32, _: u32) -> u64 {
            0
        }
    }
    #[test]
    fn current_signed_output_origin_and_damage_clip_are_shared_by_drawing() {
        let mut words = [0xaabbcc; 16];
        let bounds = Rect::new(-4, 8, 4, 4);
        let mut scene = CpuScene::output(&mut words, bounds).unwrap();
        assert!(!scene.is_layer());
        scene.set_paint_clip(Rect::new(-3, 9, 2, 2));
        {
            let mut painter = Painter::cpu(&mut scene);
            painter.fill(bounds, 0x123456).unwrap();
            painter.set_clip(bounds); // cannot expand the capture's damage
            painter
                .xrgb(-3, 10, 2, 1, PixelBytes::from_words(&[0x998877, 0x665544]))
                .unwrap();
            painter.set_clip(Rect::ZERO);
            painter.clear(bounds).unwrap();
        }
        assert_eq!(scene.paint_bounds(), Rect::new(-3, 9, 2, 2));
        assert_eq!(
            words,
            [
                0xaabbcc, 0xaabbcc, 0xaabbcc, 0xaabbcc, 0xaabbcc, 0x123456, 0x123456, 0xaabbcc,
                0xaabbcc, 0x998877, 0x665544, 0xaabbcc, 0xaabbcc, 0xaabbcc, 0xaabbcc, 0xaabbcc
            ]
        );
    }
    #[test]
    fn current_text_and_native_recording_use_their_original_pixel_encodings() {
        let bounds = Rect::new(0, 0, 8, 8);
        let mut cache = FontCache::new().unwrap();
        let mut cpu = [0; 64];
        let mut mirrored = [0; 64];
        let mut scene = CpuScene::new(&mut cpu, 8, 8).unwrap();
        Painter::cpu(&mut scene)
            .text(
                &mut cache,
                &mut Builtin,
                0,
                0,
                0,
                b"A",
                0xffffff,
                0x123456,
                bounds,
            )
            .unwrap();
        let mut other = CpuScene::new(&mut mirrored, 8, 8).unwrap();
        let mut frame = Frame::new().unwrap();
        frame.start(1).unwrap();
        frame.begin(0, bounds).unwrap();
        Painter::recorded(Some(&mut other), &mut frame, bounds, bounds)
            .text(
                &mut cache,
                &mut Builtin,
                0,
                0,
                0,
                b"A",
                0xffffff,
                0x123456,
                bounds,
            )
            .unwrap();
        frame.end().unwrap();
        assert!(!frame.commands().is_empty());
        assert!(cpu.iter().all(|p| p >> 24 == 0));
        assert!(mirrored.iter().all(|p| p >> 24 == 255));
        for (a, b) in cpu.iter().zip(mirrored) {
            assert_eq!(*a, b & 0xffffff);
        }
    }
}
