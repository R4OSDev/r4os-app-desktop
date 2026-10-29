//! One Desktop-owned font cache, keyed exactly by font/revision/scalar.
//! Coverage is color neutral; only a synchronous paint loan exposes its rows.
//! Bounded heap storage avoids constructing an 80 KB cache on the app stack.
use super::{
    font8,
    layout::{self, CELL, MAX_CELL, Visible},
};
use crate::render::{
    geometry::Rect,
    primitive_assets::{Error, Result},
    primitive_image::{Format, Identity, Paint, Picture, View},
};
use alloc::vec::Vec;
use r4os_contract::{self as abi, GuiFontInfo, GuiGlyphBitmap};
const LAYOUTS: usize = 16;
const GLYPHS: usize = 256;
pub trait Fonts {
    fn revision(&mut self) -> u32;
    fn has_glyph_row(&self) -> bool;
    fn info(&mut self, font: u32, out: &mut GuiFontInfo) -> i32;
    fn bitmap(&mut self, font: u32, scalar: u32, out: &mut GuiGlyphBitmap) -> i32;
    fn row(&mut self, font: u32, scalar: u32, row: u32) -> u64;
}
#[derive(Clone, Copy, Debug)]
struct Layout {
    imported: bool,
    width: usize,
    height: usize,
    bitmap_height: usize,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            imported: false,
            width: CELL,
            height: CELL,
            bitmap_height: CELL,
        }
    }
}
#[derive(Clone, Copy, Default)]
struct LayoutEntry {
    used: bool,
    font: u32,
    revision: u32,
    layout: Layout,
}
#[derive(Clone, Copy)]
struct Glyph {
    used: bool,
    font: u32,
    revision: u32,
    scalar: u32,
    rows: [u64; MAX_CELL],
}
impl Default for Glyph {
    fn default() -> Self {
        Self {
            used: false,
            font: 0,
            revision: 0,
            scalar: 0,
            rows: [0; MAX_CELL],
        }
    }
}
#[cfg(test)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Stats {
    pub layout_hits: u64,
    pub layout_misses: u64,
    pub glyph_hits: u64,
    pub glyph_misses: u64,
    pub glyph_loads: u64,
    pub invalidations: u64,
}
pub struct Cache {
    layouts: Vec<LayoutEntry>,
    glyphs: Vec<Glyph>,
    revision: u32,
    #[cfg(test)]
    stats: Stats,
}
fn mixed(font: u32, revision: u32) -> u32 {
    font.wrapping_mul(0x9e3779b1) ^ revision.wrapping_mul(0x85ebca6b)
}
impl Cache {
    pub fn new() -> Result<Self> {
        let mut layouts = Vec::new();
        layouts
            .try_reserve_exact(LAYOUTS)
            .map_err(|_| Error::OutOfMemory)?;
        layouts.resize(LAYOUTS, LayoutEntry::default());
        let mut glyphs = Vec::new();
        glyphs
            .try_reserve_exact(GLYPHS)
            .map_err(|_| Error::OutOfMemory)?;
        glyphs.resize(GLYPHS, Glyph::default());
        Ok(Self {
            layouts,
            glyphs,
            revision: 0,
            #[cfg(test)]
            stats: Stats::default(),
        })
    }
    #[cfg(test)]
    pub fn stats(&self) -> Stats {
        self.stats
    }
    /// Direct CPU text keeps the original XRGB words. Primitive capture uses
    /// `paint` and the separate premultiplied texture representation.
    pub fn paint_cpu(
        &mut self,
        source: &mut impl Fonts,
        font: u32,
        x: i32,
        y: i32,
        text: &[u8],
        fg: u32,
        bg: u32,
        bounds: Rect,
        scene: &mut crate::render::scene::CpuScene<'_>,
    ) -> Result<()> {
        self.paint(
            source,
            font,
            x,
            y,
            text,
            fg,
            bg,
            bounds,
            scene.full_rect(),
            |op| cpu(scene, op),
        )
    }
    pub fn reset(&mut self) {
        self.layouts.fill(LayoutEntry::default());
        self.glyphs.fill(Glyph::default());
        self.revision = 0;
        #[cfg(test)]
        {
            self.stats = Stats::default();
        }
    }
    pub fn observe_revision(&mut self, revision: u32) {
        let revision = revision.max(1);
        if self.revision == 0 {
            self.revision = revision;
            return;
        }
        if self.revision == revision {
            return;
        }
        self.layouts.fill(LayoutEntry::default());
        self.glyphs.fill(Glyph::default());
        self.revision = revision;
        #[cfg(test)]
        {
            self.stats.invalidations = self.stats.invalidations.wrapping_add(1);
        }
    }
    fn font_revision(&mut self, source: &mut impl Fonts, font: u32) -> u32 {
        if font == abi::gui_font_builtin_id {
            return self.revision.max(1);
        }
        let revision = source.revision();
        self.observe_revision(revision);
        revision
    }
    fn layout(&mut self, source: &mut impl Fonts, font: u32, revision: u32) -> Layout {
        let entry = &mut self.layouts[mixed(font, revision) as usize & (LAYOUTS - 1)];
        if entry.used && entry.font == font && entry.revision == revision {
            #[cfg(test)]
            {
                self.stats.layout_hits = self.stats.layout_hits.wrapping_add(1);
            }
            return entry.layout;
        }
        #[cfg(test)]
        {
            self.stats.layout_misses = self.stats.layout_misses.wrapping_add(1);
        }
        let mut layout = Layout::default();
        if font != abi::gui_font_builtin_id && source.has_glyph_row() {
            let mut info = GuiFontInfo::default();
            if source.info(font, &mut info) > 0 && info.flags & abi::gui_font_flag_renderable != 0 {
                let width = (info.max_advance as usize).clamp(1, MAX_CELL);
                let bitmap_height = (info.height as usize).clamp(1, MAX_CELL);
                let height = (info.line_height as usize).max(bitmap_height).min(MAX_CELL);
                layout = Layout {
                    imported: true,
                    width,
                    height,
                    bitmap_height,
                };
            }
        }
        *entry = LayoutEntry {
            used: true,
            font,
            revision,
            layout,
        };
        layout
    }
    fn coverage(
        &mut self,
        source: &mut impl Fonts,
        font: u32,
        revision: u32,
        layout: Layout,
        scalar: u32,
    ) -> &Glyph {
        let slot =
            (mixed(font, revision) ^ scalar.wrapping_mul(0xc2b2ae35)) as usize & (GLYPHS - 1);
        let entry = &mut self.glyphs[slot];
        if entry.used && entry.font == font && entry.revision == revision && entry.scalar == scalar
        {
            #[cfg(test)]
            {
                self.stats.glyph_hits = self.stats.glyph_hits.wrapping_add(1);
            }
            return entry;
        }
        #[cfg(test)]
        {
            self.stats.glyph_misses = self.stats.glyph_misses.wrapping_add(1);
        }
        *entry = Glyph {
            used: true,
            font,
            revision,
            scalar,
            ..Default::default()
        };
        if layout.imported {
            let mut bitmap = GuiGlyphBitmap::default();
            if source.bitmap(font, scalar, &mut bitmap) == 0 {
                let height = (bitmap.height as usize)
                    .min(layout.bitmap_height)
                    .min(MAX_CELL);
                entry.rows[..height].copy_from_slice(&bitmap.rows[..height]);
            } else {
                for row in 0..layout.bitmap_height {
                    entry.rows[row] = source.row(font, scalar, row as u32);
                }
            }
        } else {
            for (row, pattern) in font8::glyph_for_codepoint(scalar).into_iter().enumerate() {
                for col in 0..CELL {
                    if pattern & (0x80 >> col) != 0 {
                        entry.rows[row] |= 1 << col;
                    }
                }
            }
        }
        #[cfg(test)]
        {
            self.stats.glyph_loads = self.stats.glyph_loads.wrapping_add(1);
        }
        entry
    }
    /// Paint consumes a glyph view immediately. It cannot retain the cache
    /// loan, mutate the font owner or re-enter this cache while the rows live.
    pub fn paint(
        &mut self,
        source: &mut impl Fonts,
        font: u32,
        x: i32,
        y: i32,
        text: &[u8],
        fg: u32,
        bg: u32,
        bounds: Rect,
        scene: Rect,
        mut paint: impl FnMut(Paint<'_>) -> Result<()>,
    ) -> Result<()> {
        let revision = self.font_revision(source, font);
        let layout = self.layout(source, font, revision);
        let Some(bounds) = crate::render::scene::intersect(bounds, scene) else {
            return Ok(());
        };
        for cell in Visible::new(
            text,
            bounds,
            x,
            y,
            layout.width as i32,
            layout.height as i32,
        ) {
            let Some(clip) = layout::clip_cell(
                bounds,
                cell.x,
                cell.y,
                layout.width,
                layout.height,
                scene.right(),
                scene.bottom(),
            ) else {
                continue;
            };
            let coverage = self.coverage(source, font, revision, layout, cell.scalar);
            let mut view = View::new(
                Format::Glyph,
                layout.width as u32,
                layout.height as u32,
                0,
                &[],
            );
            view.rows = &coverage.rows;
            view.foreground = fg;
            view.background = bg;
            view.identity = Identity {
                font,
                revision,
                glyph: cell.scalar,
                ..Default::default()
            };
            paint(Paint::Picture(Picture {
                view,
                viewport: Rect::new(cell.x, cell.y, layout.width as i32, layout.height as i32),
                clip,
                guest_w: layout.width as u32,
                guest_h: layout.height as u32,
                source_x: 0,
                source_y: 0,
            }))?;
        }
        Ok(())
    }
}
fn cpu(painter: &mut crate::render::scene::CpuScene<'_>, operation: Paint<'_>) -> Result<()> {
    let Paint::Picture(picture) = operation else {
        return Err(Error::State);
    };
    let mut pixels = [0; MAX_CELL * MAX_CELL];
    for y in 0..picture.view.height as usize {
        for x in 0..picture.view.width as usize {
            pixels[y * picture.view.width as usize + x] = if picture.view.rows[y] & (1u64 << x) != 0
            {
                picture.view.foreground
            } else {
                picture.view.background
            };
        }
    }
    let previous = painter.paint_clip;
    painter.set_paint_clip(picture.clip);
    painter.blit_xrgb32(
        picture.viewport.x,
        picture.viewport.y,
        picture.view.width,
        picture.view.height,
        crate::render::pixels::PixelBytes::from_words(
            &pixels[..picture.view.width as usize * picture.view.height as usize],
        ),
    );
    painter.paint_clip = previous;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::scene::CpuScene;
    struct Source {
        revision: u32,
        rows: u32,
        bitmap: bool,
    }
    impl Fonts for Source {
        fn revision(&mut self) -> u32 {
            self.revision
        }
        fn has_glyph_row(&self) -> bool {
            true
        }
        fn info(&mut self, _: u32, out: &mut GuiFontInfo) -> i32 {
            out.flags = abi::gui_font_flag_renderable;
            out.max_advance = 64;
            out.height = 3;
            out.line_height = 7;
            1
        }
        fn bitmap(&mut self, _: u32, _: u32, out: &mut GuiGlyphBitmap) -> i32 {
            if !self.bitmap {
                return -1;
            }
            out.height = 2;
            out.rows[0] = 1;
            out.rows[1] = 2;
            0
        }
        fn row(&mut self, _: u32, _: u32, row: u32) -> u64 {
            self.rows += 1;
            1 << row
        }
    }
    #[test]
    fn original_color_neutral_generation_cache_and_utf8_cell() {
        let mut cache = Cache::new().unwrap();
        cache.observe_revision(7);
        let mut source = Source {
            revision: 7,
            rows: 0,
            bitmap: false,
        };
        let bounds = Rect::new(0, 0, 16, 8);
        let mut reference = [0; 128];
        let mut actual = [0; 128];
        for pixels in [&mut reference, &mut actual] {
            let mut scene = CpuScene::new(pixels, 16, 8).unwrap();
            cache
                .paint(
                    &mut source,
                    0,
                    0,
                    0,
                    b"AA",
                    0xffffff,
                    0,
                    bounds,
                    bounds,
                    |op| cpu(&mut scene, op),
                )
                .unwrap();
        }
        assert_eq!(reference, actual);
        assert_eq!(cache.stats().glyph_misses, 1);
        assert_eq!(cache.stats().glyph_hits, 3);
        assert_eq!(cache.stats().glyph_loads, 1);
        let mut scene = CpuScene::new(&mut actual, 16, 8).unwrap();
        cache
            .paint(
                &mut source,
                0,
                0,
                0,
                b"A",
                0x123456,
                0x654321,
                bounds,
                bounds,
                |op| cpu(&mut scene, op),
            )
            .unwrap();
        assert_eq!(cache.stats().glyph_loads, 1);
        assert!(actual.contains(&0x123456));
        assert!(actual.contains(&0x654321));
        cache.observe_revision(8);
        let mut scene = CpuScene::new(&mut actual, 16, 8).unwrap();
        cache
            .paint(
                &mut source,
                0,
                0,
                0,
                b"A",
                0xffffff,
                0,
                bounds,
                bounds,
                |op| cpu(&mut scene, op),
            )
            .unwrap();
        assert_eq!(cache.stats().invalidations, 1);
        assert_eq!(cache.stats().glyph_loads, 2);
        actual.fill(0x112233);
        let mut scene = CpuScene::new(&mut actual, 16, 8).unwrap();
        cache
            .paint(
                &mut source,
                0,
                0,
                0,
                "ä".as_bytes(),
                0xffffff,
                0,
                bounds,
                bounds,
                |op| cpu(&mut scene, op),
            )
            .unwrap();
        for y in 0..8 {
            assert!(
                actual[y * 16 + 8..y * 16 + 16]
                    .iter()
                    .all(|&p| p == 0x112233)
            );
        }
    }
    #[test]
    fn original_incremental_glyph_edges_untouched_exterior_and_long_text() {
        let mut cache = Cache::new().unwrap();
        let mut source = Source {
            revision: 1,
            rows: 0,
            bitmap: false,
        };
        let bounds = Rect::new(0, 0, 24, 24);
        let mut reference = [0x123456; 576];
        let mut scene = CpuScene::new(&mut reference, 24, 24).unwrap();
        cache
            .paint(
                &mut source,
                0,
                8,
                8,
                b"A",
                0xffffff,
                0,
                bounds,
                bounds,
                |op| cpu(&mut scene, op),
            )
            .unwrap();
        for clip in [
            Rect::new(8, 10, 2, 3),
            Rect::new(14, 10, 2, 3),
            Rect::new(10, 8, 3, 2),
            Rect::new(10, 14, 3, 2),
        ] {
            let mut actual = [0x123456; 576];
            let mut scene = CpuScene::new(&mut actual, 24, 24).unwrap();
            cache
                .paint(
                    &mut source,
                    0,
                    8,
                    8,
                    b"A",
                    0xffffff,
                    0,
                    clip,
                    bounds,
                    |op| cpu(&mut scene, op),
                )
                .unwrap();
            for y in 0..24 {
                for x in 0..24 {
                    assert_eq!(
                        actual[(y * 24 + x) as usize],
                        if clip.contains(x, y) {
                            reference[(y * 24 + x) as usize]
                        } else {
                            0x123456
                        }
                    );
                }
            }
        }
        let mut text = [b'\r'; 4097];
        text[4096] = b'A';
        let mut pixels = [0; 64];
        let mut scene = CpuScene::new(&mut pixels, 8, 8).unwrap();
        let b = Rect::new(0, 0, 8, 8);
        cache
            .paint(&mut source, 0, 0, 0, &text, 0xffffff, 0, b, b, |op| {
                cpu(&mut scene, op)
            })
            .unwrap();
        assert!(pixels.iter().any(|&p| p != 0));
    }
    #[test]
    fn imported_bitmap_row_fallback_layout_clamp_and_revision_replacement() {
        let mut cache = Cache::new().unwrap();
        let mut source = Source {
            revision: 5,
            rows: 0,
            bitmap: true,
        };
        let bounds = Rect::new(0, 0, 80, 7);
        let mut n = 0;
        cache
            .paint(&mut source, 9, 0, 0, b"AA", 1, 0, bounds, bounds, |op| {
                let Paint::Picture(p) = op else { panic!() };
                assert_eq!((p.view.width, p.view.height), (40, 7));
                assert_eq!(&p.view.rows[..4], &[1, 2, 0, 0]);
                n += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(source.rows, 0);
        assert_eq!(cache.stats().glyph_loads, 1);
        source.revision = 6;
        source.bitmap = false;
        cache
            .paint(&mut source, 9, 0, 0, b"A", 1, 0, bounds, bounds, |op| {
                let Paint::Picture(p) = op else { panic!() };
                assert_eq!(&p.view.rows[..4], &[1, 2, 4, 0]);
                Ok(())
            })
            .unwrap();
        assert_eq!(source.rows, 3);
        assert_eq!(cache.stats().invalidations, 1);
    }
}
