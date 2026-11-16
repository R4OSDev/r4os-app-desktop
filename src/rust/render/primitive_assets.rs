//! Source-sized premultiplied texture pages, with the original bounded atlas
//! and LRU admission. Pages used by the current frame cannot be evicted.
use super::{geometry::Rect, primitive_image::View};
use alloc::{sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicU64, Ordering};
pub const TEXTURE_CAPACITY: usize = 32;
pub const ENTRY_CAPACITY: usize = 2048;
const _: () = assert!(ENTRY_CAPACITY.is_power_of_two() && ENTRY_CAPACITY < u16::MAX as usize);
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
    // Bucket heads live alongside slots; zero links mean no entry.
    head: u16,
    next: u16,
}
#[derive(Clone, Default)]
pub struct Texture {
    pixels: Option<Arc<Vec<u32>>>,
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
        self.pixels.as_deref().map_or(&[], |p| p.as_slice())
    }
    fn writable_pixels(&mut self) -> Result<&mut [u32]> {
        let shared = self.pixels.as_mut().ok_or(Error::State)?;
        if Arc::get_mut(shared).is_none() {
            let mut copy = Vec::new();
            copy.try_reserve_exact(shared.len())
                .map_err(|_| Error::OutOfMemory)?;
            copy.extend_from_slice(shared);
            *shared = Arc::new(copy);
        }
        Ok(Arc::get_mut(shared).ok_or(Error::State)?.as_mut_slice())
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
    live_entries: usize,
    first_free: usize,
    #[cfg(test)]
    pub search_steps: u64,
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
            live_entries: 0,
            first_free: 0,
            #[cfg(test)]
            search_steps: 0,
            hits: 0,
            misses: 0,
            converted_pixels: 0,
            evictions: 0,
        })
    }
    pub fn reserved(&self) -> usize {
        self.reserved
    }
    /// An immutable submitted scene and the one mutable newest recording
    /// have independent metadata. Pixel pages are shared until a real write;
    /// eviction and upload acknowledgments cannot change the older snapshot.
    pub fn fork(&self) -> Result<Self> {
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(ENTRY_CAPACITY)
            .map_err(|_| Error::OutOfMemory)?;
        entries.extend_from_slice(&self.entries);
        Ok(Self {
            entries,
            textures: self.textures.clone(),
            budget: self.budget,
            reserved: self.reserved,
            frame: self.frame,
            live_entries: self.live_entries,
            first_free: self.first_free,
            #[cfg(test)]
            search_steps: self.search_steps,
            hits: self.hits,
            misses: self.misses,
            converted_pixels: self.converted_pixels,
            evictions: self.evictions,
        })
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
        for slot in 0..self.entries.len() {
            if self.entries[slot].live && self.entries[slot].texture as usize == index {
                let bucket = Self::bucket(self.entries[slot].key);
                let mut link = self.entries[bucket].head;
                let mut previous = 0;
                while link != 0 {
                    let i = usize::from(link - 1);
                    if i == slot {
                        let next = self.entries[i].next;
                        if previous == 0 {
                            self.entries[bucket].head = next;
                        } else {
                            self.entries[usize::from(previous - 1)].next = next;
                        }
                        break;
                    }
                    previous = link;
                    link = self.entries[i].next;
                }
                self.entries[slot].live = false;
                self.entries[slot].next = 0;
                self.live_entries -= 1;
                self.first_free = self.first_free.min(slot);
            }
        }
        self.reserved -= self.textures[index].pixels().len() * 4;
        self.textures[index] = Texture::default();
        self.evictions = self.evictions.saturating_add(1);
    }
    fn oldest(&self, first: usize, end: usize, live_only: bool) -> Option<usize> {
        let mut chosen = None;
        for (index, texture) in self.textures.iter().enumerate().take(end).skip(first) {
            if texture.pinned == self.frame || (live_only && texture.pixels().is_empty()) {
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
        if !self.textures[index].pixels().is_empty() {
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
            pixels: Some(Arc::new(pixels)),
            width,
            height,
            ..Texture::default()
        };
        Ok(())
    }
    fn bucket(key: [u8; 32]) -> usize {
        u64::from_le_bytes(key[..8].try_into().unwrap()) as usize & (ENTRY_CAPACITY - 1)
    }
    fn find_entry(&mut self, key: [u8; 32]) -> Option<Entry> {
        let mut link = self.entries[Self::bucket(key)].head;
        while link != 0 {
            let entry = self.entries[usize::from(link - 1)];
            #[cfg(test)]
            {
                self.search_steps += 1;
            }
            if entry.key == key {
                return Some(entry);
            }
            link = entry.next;
        }
        None
    }
    pub fn intern(&mut self, view: View<'_>) -> Result<Entry> {
        if self.frame == 0 || !view.valid() {
            return Err(Error::Invalid);
        }
        let (width, height) = (view.raster_width(), view.raster_height());
        let key = view.key();
        if let Some(entry) = self.find_entry(key) {
            let texture = &mut self.textures[entry.texture as usize];
            texture.pinned = self.frame;
            texture.touched = self.frame;
            self.hits = self.hits.saturating_add(1);
            return Ok(entry);
        }
        if self.live_entries == ENTRY_CAPACITY {
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
        let entry_index = (self.first_free..self.entries.len())
            .find(|i| !self.entries[*i].live)
            .ok_or(Error::Capacity)?;
        let texture = &mut self.textures[index];
        let rect = texture.placement(width, height).ok_or(Error::Capacity)?;
        let generation = next_generation(&GENERATION)?;
        let pitch = texture.width as usize;
        let pixels = texture.writable_pixels()?;
        let mut opaque = true;
        for y in 0..height as usize {
            for x in 0..width as usize {
                let pixel = view.raster_pixel(x, y);
                opaque &= pixel >> 24 == 255;
                pixels[(rect.y as usize + y) * pitch + rect.x as usize + x] = pixel;
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
        let bucket = Self::bucket(key);
        let next = self.entries[bucket].head;
        let entry = Entry {
            live: true,
            head: self.entries[entry_index].head,
            next,
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
        self.entries[bucket].head = (entry_index + 1) as u16;
        self.live_entries += 1;
        self.first_free = entry_index + 1;
        self.misses = self.misses.saturating_add(1);
        self.converted_pixels = self
            .converted_pixels
            .saturating_add(u64::from(view.width) * u64::from(view.height));
        Ok(entry)
    }
    pub fn pin_texture(&mut self, index: usize) -> Result<()> {
        let texture = self.textures.get_mut(index).ok_or(Error::Bounds)?;
        if self.frame == 0 || texture.pixels().is_empty() {
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
    fn submitted_page_snapshot_survives_latest_scene_write_ack_and_eviction() {
        let metadata = ENTRY_CAPACITY * core::mem::size_of::<Entry>();
        let mut submitted = Cache::new(metadata + 512 * 512 * 4).unwrap();
        submitted.start(1).unwrap();
        let blue = [0xff123456];
        let old = submitted
            .intern(View::new(
                Format::Argb,
                1,
                1,
                4,
                PixelBytes::from_words(&blue).bytes(),
            ))
            .unwrap();
        let index = old.texture as usize;
        let original = submitted.texture(index).pixels().to_vec();
        let version = submitted.texture(index).generation;
        let mut latest = submitted.fork().unwrap();
        assert_eq!(
            submitted.texture(index).pixels().as_ptr(),
            latest.texture(index).pixels().as_ptr()
        );
        latest.start(2).unwrap();
        let green = [0xff00ff00];
        let new = latest
            .intern(View::new(
                Format::Argb,
                1,
                1,
                4,
                PixelBytes::from_words(&green).bytes(),
            ))
            .unwrap();
        assert_eq!(old.texture, new.texture);
        assert_ne!(
            submitted.texture(index).pixels().as_ptr(),
            latest.texture(index).pixels().as_ptr()
        );
        assert_eq!(submitted.texture(index).pixels(), original);
        assert_eq!(submitted.texture(index).generation, version);
        assert!(new.content.revision > old.content.revision);
        assert!(submitted.contains(old.content, old.texture, old.rect));
        assert!(latest.contains(old.content, old.texture, old.rect));
        latest.uploaded(index, new.content.revision);
        assert!(latest.texture(index).dirty.is_none());
        assert!(submitted.texture(index).dirty.is_some());
        latest.start(3).unwrap();
        let large = [0xffabcdef; 129 * 129];
        latest
            .intern(View::new(
                Format::Argb,
                129,
                129,
                129 * 4,
                PixelBytes::from_words(&large).bytes(),
            ))
            .unwrap();
        assert!(!latest.contains(old.content, old.texture, old.rect));
        assert!(submitted.contains(old.content, old.texture, old.rect));
        assert_eq!(submitted.texture(index).pixels(), original);
        assert!(submitted.reserved() <= submitted.budget());
        assert!(latest.reserved() <= latest.budget());
    }
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

#[cfg(test)]
mod work_cases {
    use super::super::primitive_image::{Format, Identity};
    use super::*;
    #[test]
    fn indexed_asset_work_keeps_exact_content_and_pinned_pages() {
        let mut cache = Cache::new(8 * 1024 * 1024).unwrap();
        let mut originals = alloc::vec::Vec::new();
        for phase in 0..2 {
            cache.start(phase + 1).unwrap();
            let search = cache.search_steps;
            super::super::content_hash::work::reset();
            let started = std::time::Instant::now();
            for repeat in 0..if phase == 0 { 1 } else { 4 } {
                for n in 0..512u32 {
                    let rows = [u64::from(n); 8];
                    let mut view = View::new(Format::Glyph, 8, 8, 0, &[]);
                    view.rows = &rows;
                    view.foreground = 0xffffff;
                    view.identity = Identity {
                        font: 9,
                        revision: 11,
                        glyph: n,
                        ..Default::default()
                    };
                    let entry = cache.intern(view).unwrap();
                    if phase == 0 {
                        originals.push(entry);
                    } else {
                        assert_eq!(entry.content, originals[n as usize].content);
                        assert_eq!(entry.rect, originals[n as usize].rect);
                        assert_eq!(entry.texture, originals[n as usize].texture);
                    }
                    let tex = cache.texture(entry.texture as usize);
                    assert_eq!(tex.pinned, phase + 1);
                    let expected = if rows[0] & 1 != 0 {
                        view.foreground
                    } else {
                        view.background
                    } | 0xff000000;
                    assert_eq!(
                        tex.pixels()
                            [entry.rect.y as usize * tex.width as usize + entry.rect.x as usize],
                        expected
                    );
                }
                assert!(repeat < 4);
            }
            let elapsed = started.elapsed().as_nanos();
            let (updates, bytes) = super::super::content_hash::work::get();
            println!(
                "{{\"schema\":\"r4os.desktop-work\",\"scope\":\"asset CPU model\",\"phase\":{phase},\"elapsed_ns\":{elapsed},\"hash_updates\":{updates},\"hash_bytes\":{bytes},\"search_steps\":{},\"hits\":{},\"misses\":{},\"evictions\":{}}}",
                cache.search_steps - search,
                cache.hits,
                cache.misses,
                cache.evictions
            );
        }
        assert_eq!(cache.misses, 512);
        assert_eq!(cache.hits, 2048);
        assert_eq!(cache.evictions, 0);
    }
}
