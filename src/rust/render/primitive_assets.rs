//! Source-sized premultiplied texture pages, with the original bounded atlas
//! and LRU admission. Pages used by the current frame cannot be evicted.
use super::{geometry::Rect, primitive_image::View};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
pub const TEXTURE_CAPACITY: usize = 32;
pub const ENTRY_CAPACITY: usize = 2048;
pub const ATLAS_COUNT: usize = 8;
pub const ATLAS_DIMENSION: u32 = 512;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    OutOfMemory,
    State,
    Invalid,
    Capacity,
    Bounds,
    Overflow,
    Graphics,
}
pub type Result<T> = core::result::Result<T, Error>;
// New contents receive a unique version even across distinct output caches.
// Hits do not touch this counter. Overflow refuses admission, never aliases
// an old tile or an uploaded page after eviction/recreation.
static GENERATION: AtomicU64 = AtomicU64::new(0);
fn next_generation(counter: &AtomicU64) -> Result<u64> {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
        .map(|prior| prior + 1)
        .map_err(|_| Error::Overflow)
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Content {
    pub slot: u16,
    pub revision: u64,
}
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Entry {
    pub live: bool,
    pub opaque: bool,
    pub key: [u8; 32],
    pub texture: u8,
    pub rect: Rect,
    pub content: Content,
}
#[derive(Default)]
pub struct Texture {
    pixels: Vec<u32>,
    pub width: u32,
    pub height: u32,
    pub generation: u64,
    pub pinned: u64,
    pub touched: u64,
    x: u32,
    y: u32,
    row_height: u32,
    pub dirty: Option<Rect>,
}
impl Texture {
    pub fn rect(&self) -> Rect {
        Rect::new(0, 0, self.width as i32, self.height as i32)
    }
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }
    fn placement(&self, width: u32, height: u32) -> Option<Rect> {
        let (mut x, mut y) = (self.x, self.y);
        if x + width > self.width {
            x = 0;
            y += self.row_height;
        }
        if width > self.width || y + height > self.height {
            return None;
        }
        Some(Rect::new(x as i32, y as i32, width as i32, height as i32))
    }
}
pub struct Cache {
    entries: Vec<Entry>,
    textures: [Texture; TEXTURE_CAPACITY],
    budget: usize,
    reserved: usize,
    frame: u64,
    pub hits: u64,
    pub misses: u64,
    pub converted_pixels: u64,
    pub evictions: u64,
}
impl Cache {
    pub fn new(budget: usize) -> Result<Self> {
        let bytes = ENTRY_CAPACITY * core::mem::size_of::<Entry>();
        if bytes > budget {
            return Err(Error::OutOfMemory);
        }
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(ENTRY_CAPACITY)
            .map_err(|_| Error::OutOfMemory)?;
        entries.resize(ENTRY_CAPACITY, Entry::default());
        Ok(Self {
            entries,
            textures: core::array::from_fn(|_| Texture::default()),
            budget,
            reserved: bytes,
            frame: 0,
            hits: 0,
            misses: 0,
            converted_pixels: 0,
            evictions: 0,
        })
    }
    pub fn reserved(&self) -> usize {
        self.reserved
    }
    pub fn budget(&self) -> usize {
        self.budget
    }
    pub fn frame(&self) -> u64 {
        self.frame
    }
    pub fn texture(&self, index: usize) -> &Texture {
        &self.textures[index]
    }
    pub fn contains(&self, content: Content, texture: u8, source: Rect) -> bool {
        content.revision != 0
            && self
                .entries
                .get(content.slot as usize)
                .is_some_and(|entry| {
                    entry.live
                        && entry.content == content
                        && entry.texture == texture
                        && entry.rect == source
                })
    }
    pub fn resident_content(
        &self,
        content: Content,
        texture: u8,
        source: Rect,
        uploaded: u64,
    ) -> bool {
        self.contains(content, texture, source)
            && uploaded >= content.revision
            && uploaded <= self.textures[texture as usize].generation
    }
    /// The composition owner calls this only after the preceding frame's
    /// worker jobs have physically drained; a Rust drop is not that receipt.
    pub fn start(&mut self, frame: u64) -> Result<()> {
        if frame == 0 || frame <= self.frame {
            return Err(Error::State);
        }
        self.frame = frame;
        Ok(())
    }
    fn forget(&mut self, index: usize) {
        for entry in &mut self.entries {
            if entry.live && entry.texture as usize == index {
                entry.live = false;
            }
        }
        self.reserved -= self.textures[index].pixels.len() * 4;
        self.textures[index] = Texture::default();
        self.evictions = self.evictions.saturating_add(1);
    }
    fn oldest(&self, first: usize, end: usize, live_only: bool) -> Option<usize> {
        let mut chosen = None;
        for (index, texture) in self.textures.iter().enumerate().take(end).skip(first) {
            if texture.pinned == self.frame || (live_only && texture.pixels.is_empty()) {
                continue;
            }
            if chosen.is_none_or(|prior: usize| texture.touched < self.textures[prior].touched) {
                chosen = Some(index);
            }
        }
        chosen
    }
    fn allocate(&mut self, index: usize, width: u32, height: u32) -> Result<()> {
        if self.textures[index].pinned == self.frame {
            return Err(Error::State);
        }
        let bytes = width as usize * height as usize * 4;
        if bytes > self.budget - self.entries.len() * core::mem::size_of::<Entry>() {
            return Err(Error::OutOfMemory);
        }
        if !self.textures[index].pixels.is_empty() {
            self.forget(index);
        }
        while self.reserved > self.budget - bytes {
            self.forget(
                self.oldest(0, TEXTURE_CAPACITY, true)
                    .ok_or(Error::OutOfMemory)?,
            );
        }
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(bytes / 4)
            .map_err(|_| Error::OutOfMemory)?;
        pixels.resize(bytes / 4, 0);
        self.reserved += bytes;
        self.textures[index] = Texture {
            pixels,
            width,
            height,
            ..Texture::default()
        };
        Ok(())
    }
    pub fn intern(&mut self, view: View<'_>) -> Result<Entry> {
        if self.frame == 0 || !view.valid() {
            return Err(Error::Invalid);
        }
        let (width, height) = (view.raster_width(), view.raster_height());
        let key = view.key();
        if let Some(entry) = self
            .entries
            .iter()
            .find(|e| e.live && e.key == key)
            .copied()
        {
            let texture = &mut self.textures[entry.texture as usize];
            texture.pinned = self.frame;
            texture.touched = self.frame;
            self.hits = self.hits.saturating_add(1);
            return Ok(entry);
        }
        if self.entries.iter().all(|e| e.live) {
            self.forget(
                self.oldest(0, TEXTURE_CAPACITY, true)
                    .ok_or(Error::Capacity)?,
            );
        }
        let small = width <= 128 && height <= 128;
        let mut chosen = if small {
            self.textures[..ATLAS_COUNT]
                .iter()
                .position(|t| t.placement(width, height).is_some())
        } else {
            None
        };
        if chosen.is_none() {
            let index = self
                .oldest(
                    if small { 0 } else { ATLAS_COUNT },
                    if small { ATLAS_COUNT } else { TEXTURE_CAPACITY },
                    false,
                )
                .ok_or(Error::Capacity)?;
            self.allocate(
                index,
                if small { ATLAS_DIMENSION } else { width },
                if small { ATLAS_DIMENSION } else { height },
            )?;
            chosen = Some(index);
        }
        let index = chosen.expect("admitted atlas");
        self.textures[index].pinned = self.frame;
        let entry_index = self
            .entries
            .iter()
            .position(|e| !e.live)
            .ok_or(Error::Capacity)?;
        let texture = &mut self.textures[index];
        let rect = texture.placement(width, height).ok_or(Error::Capacity)?;
        let generation = next_generation(&GENERATION)?;
        let mut opaque = true;
        for y in 0..height as usize {
            for x in 0..width as usize {
                let pixel = view.raster_pixel(x, y);
                opaque &= pixel >> 24 == 255;
                texture.pixels
                    [(rect.y as usize + y) * texture.width as usize + rect.x as usize + x] = pixel;
            }
        }
        if texture.y != rect.y as u32 {
            texture.row_height = 0;
        }
        texture.x = rect.x as u32 + width;
        texture.y = rect.y as u32;
        texture.row_height = texture.row_height.max(height);
        texture.generation = generation;
        texture.pinned = self.frame;
        texture.touched = self.frame;
        texture.dirty = Some(texture.dirty.map_or(rect, |prior| prior.merged(rect)));
        let entry = Entry {
            live: true,
            opaque,
            key,
            texture: index as u8,
            rect,
            content: Content {
                slot: u16::try_from(entry_index).map_err(|_| Error::Capacity)?,
                revision: generation,
            },
        };
        self.entries[entry_index] = entry;
        self.misses = self.misses.saturating_add(1);
        self.converted_pixels = self
            .converted_pixels
            .saturating_add(u64::from(view.width) * u64::from(view.height));
        Ok(entry)
    }
    pub fn pin_texture(&mut self, index: usize) -> Result<()> {
        let texture = self.textures.get_mut(index).ok_or(Error::Bounds)?;
        if self.frame == 0 || texture.pixels.is_empty() {
            return Err(Error::State);
        }
        texture.pinned = self.frame;
        Ok(())
    }
    pub fn reuse_texture(&mut self, index: usize) -> Result<()> {
        self.pin_texture(index)?;
        self.textures[index].touched = self.frame;
        Ok(())
    }
    pub fn uploaded(&mut self, index: usize, generation: u64) {
        if let Some(texture) = self.textures.get_mut(index) {
            if texture.generation == generation {
                texture.dirty = None;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::super::{
        pixels::PixelBytes,
        primitive_image::{Format, Identity},
    };
    use super::*;
    #[test]
    fn original_assets_revision_dpi_pinned_budget_eviction_and_content() {
        let metadata = ENTRY_CAPACITY * core::mem::size_of::<Entry>();
        let mut atlas = Cache::new(metadata + 512 * 512 * 4).unwrap();
        atlas.start(1).unwrap();
        let rows = [1, 2];
        let mut view = View::new(Format::Glyph, 2, 2, 0, &[]);
        view.rows = &rows;
        view.foreground = 0x123456;
        view.background = 0x112233;
        view.identity = Identity {
            font: 8,
            revision: 1,
            glyph: 65,
            ..Identity::default()
        };
        let first = atlas.intern(view).unwrap();
        let generation = atlas.texture(first.texture as usize).generation;
        assert!(atlas.contains(first.content, first.texture, first.rect));
        atlas.intern(view).unwrap();
        assert_eq!(atlas.hits, 1);
        assert_eq!(atlas.converted_pixels, 4);
        view.identity.revision = 2;
        atlas.intern(view).unwrap();
        view.identity.dpi_x = 144;
        view.identity.dpi_y = 144;
        atlas.intern(view).unwrap();
        assert_eq!(atlas.misses, 3);
        assert!(atlas.texture(first.texture as usize).generation > generation);
        assert!(atlas.contains(first.content, first.texture, first.rect));
        assert!(atlas.resident_content(first.content, first.texture, first.rect, generation));
        assert!(!atlas.resident_content(first.content, first.texture, first.rect, 0));
        atlas.uploaded(first.texture as usize, generation);
        assert!(atlas.texture(first.texture as usize).dirty.is_some());
        let mut large = [0x123456u32; 129 * 129];
        let large_view = || {
            View::new(
                Format::Xrgb,
                129,
                129,
                129 * 4,
                PixelBytes::from_words(&large).bytes(),
            )
        };
        assert!(matches!(
            atlas.intern(large_view()),
            Err(Error::OutOfMemory)
        ));
        assert!(
            atlas.reserved <= atlas.budget
                && atlas.texture(first.texture as usize).generation > generation
        );
        atlas.start(2).unwrap();
        atlas.intern(large_view()).unwrap();
        assert!(atlas.evictions != 0 && atlas.reserved <= atlas.budget);
        assert!(!atlas.contains(first.content, first.texture, first.rect));
        let conversions = atlas.converted_pixels;
        large[7] ^= 1;
        atlas
            .intern(View::new(
                Format::Xrgb,
                129,
                129,
                129 * 4,
                PixelBytes::from_words(&large).bytes(),
            ))
            .unwrap();
        assert!(atlas.converted_pixels > conversions);
        // Opacity belongs to the exact interned entry, including cache hits;
        // transparent pixels elsewhere on the same atlas page are irrelevant.
        let mut opacity = Cache::new(metadata + 512 * 512 * 4).unwrap();
        opacity.start(1).unwrap();
        let duplicate = opacity.intern(view).unwrap();
        assert_ne!(
            duplicate.content, first.content,
            "new cache cannot alias an old content handle"
        );
        assert!(!opacity.contains(first.content, first.texture, first.rect));
        let exhausted = AtomicU64::new(u64::MAX - 1);
        assert_eq!(next_generation(&exhausted), Ok(u64::MAX));
        assert_eq!(next_generation(&exhausted), Err(Error::Overflow));
        assert_eq!(exhausted.load(Ordering::Relaxed), u64::MAX);
        let mixed = [0xff123456, 0x80123456];
        let argb = View::new(
            Format::Argb,
            2,
            1,
            8,
            PixelBytes::from_words(&mixed).bytes(),
        );
        let first = opacity.intern(argb).unwrap();
        assert!(!first.opaque);
        let xrgb = View::new(
            Format::Xrgb,
            2,
            1,
            8,
            PixelBytes::from_words(&mixed).bytes(),
        );
        let second = opacity.intern(xrgb).unwrap();
        assert!(second.opaque);
        assert_eq!(first.texture, second.texture);
        assert!(!opacity.intern(argb).unwrap().opaque);
    }
}
