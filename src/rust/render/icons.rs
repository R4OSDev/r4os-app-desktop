//! The Desktop's bounded ICO cache. Selection, BMP parsing and pixel decoding
//! stay in the original SDK data owner. This owner only loads and places them.
use super::{
    primitive_assets::{Error, Result},
    text::cache::Fonts,
    ui::Ui,
};
use alloc::vec::Vec;
use r4os_sdk::{App2, data::raw as c, path::FilePath, storage::System};
#[cfg(test)]
std::thread_local! {
    static PIXEL_BRIDGE_CALLS: core::cell::Cell<u32> = const { core::cell::Cell::new(0) };
}
// Count the actual native pixel boundary only in the existing host fixture.
// Product expansion is just the original expression, with no observer.
macro_rules! pixel_bridge {
    ($call:expr) => {{
        #[cfg(test)]
        PIXEL_BRIDGE_CALLS.with(|count| count.set(count.get() + 1));
        $call
    }};
}
mod misses;
pub const PATH_MAX: usize = 127;
pub const SIZE_MAX: usize = 32;
const CAPACITY: usize = 32;
const INPUT_MAX: usize = 8192;
pub const PROGRAM_ICON: &[u8] = b"C:\\R4OS\\Media\\Icons\\Application.ico";
pub const TEXT_ICON: &[u8] = b"C:\\R4OS\\Media\\Icons\\Textfile.ico";

pub trait Loader {
    fn watch(&mut self, _path: &[u8]) -> Option<Watch> {
        None
    }
    fn changed(&mut self, _watch: &mut Watch) -> Option<bool> {
        None
    }
    fn file(&mut self, path: &[u8], output: &mut [u8]) -> Option<usize>;
    fn container(&mut self, path: &[u8], output: &mut [u8]) -> Option<usize>;
}
pub struct Native<'a, 'start> {
    pub app: &'a App2<'start>,
}
impl Loader for Native<'_, '_> {
    fn watch(&mut self, path: &[u8]) -> Option<Watch> {
        let path = FilePath::parse(path).ok()?;
        let bytes = path.as_bytes();
        let separator = bytes
            .iter()
            .rposition(|byte| matches!(byte, b'/' | b'\\'))?;
        let parent = FilePath::parse(&bytes[..=separator]).ok()?;
        let mut cursor = r4os_sdk::abi::DirectoryChangeCursor::default();
        System::new2(self.app)
            .ok()?
            .files()
            .begin_directory_changes(&parent, &mut cursor)
            .ok()?;
        Some(Watch(cursor))
    }
    fn changed(&mut self, watch: &mut Watch) -> Option<bool> {
        System::new2(self.app)
            .ok()?
            .files()
            .poll_directory_changes(&mut watch.0)
            .ok()
            .map(|n| n > 0)
    }
    fn file(&mut self, path: &[u8], output: &mut [u8]) -> Option<usize> {
        let path = FilePath::parse(path).ok()?;
        System::new2(self.app)
            .ok()?
            .files()
            .read(&path, output)
            .ok()
            .flatten()
    }
    fn container(&mut self, path: &[u8], output: &mut [u8]) -> Option<usize> {
        if path.is_empty() || path.len() > PATH_MAX || path.contains(&0) {
            return None;
        }
        let pointer = self
            .app
            .context()
            .lookup(
                1,
                core::mem::offset_of!(
                    r4os_sdk::abi::generation2::R4XStartR4Sys2,
                    module_resource_read
                ) as u32,
            )
            .ok()?;
        let mut name = [0; PATH_MAX + 1];
        name[..path.len()].copy_from_slice(path);
        // Exact current R4SYS signature from the platform contract; no native
        // pointer or unvalidated length escapes this synchronous call.
        type Read = unsafe extern "C" fn(*const u8, u32, u32, *const u8, *mut u8, u32) -> i32;
        let read: Read = unsafe { core::mem::transmute(pointer) };
        let rc = unsafe {
            read(
                name.as_ptr(),
                1,
                0,
                core::ptr::null(),
                output.as_mut_ptr(),
                u32::try_from(output.len()).ok()?,
            )
        };
        let n = usize::try_from(rc).ok()?;
        (n != 0 && n <= output.len()).then_some(n)
    }
}
pub struct Entry {
    used: bool,
    path: [u8; PATH_MAX],
    path_len: usize,
    size: u16,
    pixels: [u32; SIZE_MAX * SIZE_MAX],
    alpha: [u8; SIZE_MAX * SIZE_MAX],
    loaded_ns: Option<u64>,
    watch: Option<Watch>,
}
impl Entry {
    fn empty() -> Self {
        Self {
            used: false,
            path: [0; PATH_MAX],
            path_len: 0,
            size: 0,
            pixels: [0; 1024],
            alpha: [0; 1024],
            loaded_ns: None,
            watch: None,
        }
    }
    pub fn paint<F: Fonts>(&self, ui: &mut Ui<'_, '_, F>, x: i32, y: i32) -> Result<()> {
        let Some(area) = super::scene::intersect(
            ui.painter.clip(),
            super::geometry::Rect::new(x, y, self.size as i32, self.size as i32),
        ) else {
            return Ok(());
        };
        let left = (area.x - x) as usize;
        let right = left + area.w as usize;
        for py in (area.y - y) as usize..(area.bottom() - y) as usize {
            let mut px = left;
            while px < right {
                let n = py * SIZE_MAX + px;
                if self.alpha[n] < 128 {
                    px += 1;
                    continue;
                }
                let first = px;
                let rgb = self.pixels[n] & 0xffffff;
                px += 1;
                while px < right
                    && self.alpha[py * SIZE_MAX + px] >= 128
                    && self.pixels[py * SIZE_MAX + px] & 0xffffff == rgb
                {
                    px += 1;
                }
                // Same opaque threshold/color as each original pixel. The
                // existing recorder may additionally merge equal whole rows.
                ui.rect(x + first as i32, y + py as i32, (px - first) as i32, 1, rgb)?;
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub struct Watch(r4os_sdk::abi::DirectoryChangeCursor);
impl PartialEq for Watch {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (&self.0, &other.0);
        a.version == b.version
            && a.size == b.size
            && a.sequence == b.sequence
            && a.node == b.node
            && a.mount_generation == b.mount_generation
            && a.mount_slot == b.mount_slot
            && a.reserved == b.reserved
    }
}
impl Eq for Watch {}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Request {
    key: [u8; PATH_MAX],
    len: usize,
    size: u16,
    container: bool,
    epoch: u64,
    watch: Option<Watch>,
}
impl Request {
    fn new(path: &[u8], size: u16, container: bool, epoch: u64) -> Option<Self> {
        if size == 0 || size > SIZE_MAX as u16 || path.is_empty() || path.contains(&0) {
            return None;
        }
        let len = path.len().checked_add(if container { 5 } else { 0 })?;
        if len > PATH_MAX {
            return None;
        }
        let mut key = [0; PATH_MAX];
        let start = if container {
            key[..5].copy_from_slice(b"rsrc:");
            5
        } else {
            0
        };
        key[start..len].copy_from_slice(path);
        Some(Self {
            key,
            len,
            size,
            container,
            epoch,
            watch: None,
        })
    }
    fn same_source(&self, other: &Self) -> bool {
        self.epoch == other.epoch
            && self.size == other.size
            && self.len == other.len
            && self.key[..self.len] == other.key[..other.len]
    }
    fn matches(&self, entry: &Entry) -> bool {
        entry.used
            && entry.size == self.size
            && entry.path_len == self.len
            && entry.path[..self.len] == self.key[..self.len]
    }
    /// Runs only in the copied I/O task (or the existing owner test adapter).
    /// An admitted parent watch polls the existing bounded RAM history. Actual
    /// changes, overflow, retired mounts and unavailable watches require a read.
    pub(crate) fn read(&self, loader: &mut impl Loader) -> Refresh {
        let path = &self.key[if self.container { 5 } else { 0 }..self.len];
        if let Some(mut watch) = self.watch {
            if loader.changed(&mut watch) == Some(false) {
                return Refresh::Unchanged(watch);
            }
        }
        // Admit before reading so a concurrent mutation cannot bless old bytes.
        let mut watch = loader.watch(path);
        let mut input = Vec::new();
        if input.try_reserve_exact(INPUT_MAX).is_err() {
            return Refresh::Retry;
        }
        input.resize(INPUT_MAX, 0);
        let loaded = (|| -> Option<Entry> {
            let n = if self.container {
                loader.container(path, &mut input)
            } else {
                loader.file(path, &mut input)
            }?;
            if n == 0 || n > input.len() {
                return None;
            }
            let image = r4os_sdk::data::Icon::open(&input[..n])
                .ok()?
                .best_bitmap(self.size)
                .ok()?;
            let (width, height) = image.dimensions().ok()?;
            if width == 0 || height == 0 {
                return None;
            }
            let mut entry = Entry::empty();
            entry.path = self.key;
            entry.path_len = self.len;
            entry.size = self.size;
            let w = u32::from(self.size).min(width);
            let h = u32::from(self.size).min(height);
            let ox = ((u32::from(self.size) - w) / 2) as usize;
            let oy = ((u32::from(self.size) - h) / 2) as usize;
            let destination = oy * SIZE_MAX + ox;
            pixel_bridge!(image.copy_scaled_pixels(
                &mut entry.pixels[destination..],
                &mut entry.alpha[destination..],
                w,
                h,
                SIZE_MAX,
            ))
            .ok()?;
            entry.watch = watch.take().and_then(|mut watch| {
                (loader.changed(&mut watch) == Some(false)).then_some(watch)
            });
            entry.used = true;
            Some(entry)
        })();
        loaded.map_or(Refresh::Failed, Refresh::Loaded)
    }
}
pub(crate) enum Refresh {
    Unchanged(Watch),
    Loaded(Entry),
    Failed,
    Retry,
}
pub struct Cache {
    entries: Vec<Entry>,
    next: usize,
    now_ns: Option<u64>,
    misses: misses::Ledger,
    pending: Vec<Option<Request>>,
    pending_next: usize,
    in_flight: Option<Request>,
    epoch: u64,
}
impl Cache {
    pub fn new() -> Result<Self> {
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(CAPACITY)
            .map_err(|_| Error::OutOfMemory)?;
        entries.resize_with(CAPACITY, Entry::empty);
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(CAPACITY)
            .map_err(|_| Error::OutOfMemory)?;
        pending.resize(CAPACITY, None);
        Ok(Self {
            entries,
            next: 0,
            now_ns: None,
            misses: misses::Ledger::default(),
            pending,
            pending_next: 0,
            in_flight: None,
            epoch: 0,
        })
    }
    /// One copied current turn instant, supplied before I/O completion/painting.
    pub fn begin_frame(&mut self, now_ns: u64) {
        if self.now_ns.is_some_and(|prior| now_ns < prior) {
            self.invalidate();
        }
        self.now_ns = Some(now_ns);
    }
    /// Epoch invalidation also rejects a late joined result from the old folder.
    pub fn invalidate(&mut self) {
        for entry in &mut self.entries {
            entry.used = false;
            entry.loaded_ns = None;
        }
        self.next = 0;
        self.misses.clear();
        self.pending.fill(None);
        self.pending_next = 0;
        self.in_flight = None;
        self.epoch = self.epoch.wrapping_add(1);
    }
    /// Paint only reads ready pixels and copies bounded requests. It never has
    /// a file/resource/decoder capability. A stale positive stays visible until
    /// its replacement or an actual failed read is joined and published.
    pub fn request(&mut self, path: &[u8], size: u16, container: bool) -> Option<&Entry> {
        let mut request = Request::new(path, size, container, self.epoch)?;
        let index = self.entries.iter().position(|e| request.matches(e));
        let fresh = index.is_some_and(|index| match (self.now_ns, self.entries[index].loaded_ns) {
            (Some(now), Some(loaded)) => now >= loaded && now - loaded < 5_000_000_000,
            (None, _) => true,
            _ => false,
        });
        if !fresh
            && !self
                .now_ns
                .is_some_and(|now| self.misses.blocked(&request.key[..request.len], size, now))
            && !self.in_flight.is_some_and(|p| p.same_source(&request))
            && !self
                .pending
                .iter()
                .flatten()
                .any(|p| p.same_source(&request))
        {
            request.watch = index.and_then(|i| self.entries[i].watch);
            if let Some(slot) = self.pending.iter_mut().find(|slot| slot.is_none()) {
                *slot = Some(request);
            }
        }
        index.map(|i| &self.entries[i])
    }
    pub fn busy(&self) -> bool {
        self.in_flight.is_some() || self.pending.iter().any(Option::is_some)
    }
    pub(crate) fn take_request(&mut self) -> Option<Request> {
        if self.in_flight.is_some() {
            return None;
        }
        for offset in 0..CAPACITY {
            let i = (self.pending_next + offset) % CAPACITY;
            if let Some(request) = self.pending[i].take() {
                self.pending_next = (i + 1) % CAPACITY;
                self.in_flight = Some(request);
                return Some(request);
            }
        }
        None
    }
    /// The caller must hold the exact native join receipt before passing data.
    pub(crate) fn complete(&mut self, request: Request, result: Refresh) -> bool {
        if request.epoch != self.epoch || self.in_flight != Some(request) {
            return false;
        }
        self.in_flight = None;
        let prior = self.entries.iter().position(|e| request.matches(e));
        match result {
            Refresh::Unchanged(watch) => {
                if let Some(i) = prior {
                    self.entries[i].loaded_ns = self.now_ns;
                    self.entries[i].watch = Some(watch);
                }
                self.misses
                    .forget(&request.key[..request.len], request.size);
                false
            }
            Refresh::Loaded(mut entry) => {
                let changed = prior.is_none_or(|i| {
                    self.entries[i].pixels != entry.pixels || self.entries[i].alpha != entry.alpha
                });
                let index = prior
                    .or_else(|| self.entries.iter().position(|e| !e.used))
                    .unwrap_or_else(|| {
                        let i = self.next;
                        self.next = (i + 1) % CAPACITY;
                        i
                    });
                entry.loaded_ns = self.now_ns;
                self.entries[index] = entry;
                self.misses
                    .forget(&request.key[..request.len], request.size);
                changed
            }
            Refresh::Retry => {
                if let Some(now) = self.now_ns {
                    self.misses
                        .remember(&request.key[..request.len], request.size, now);
                }
                false
            }
            Refresh::Failed => {
                if let Some(i) = prior {
                    self.entries[i].used = false;
                }
                if let Some(now) = self.now_ns {
                    self.misses
                        .remember(&request.key[..request.len], request.size, now);
                }
                prior.is_some()
            }
        }
    }
    // Existing synchronous owner oracles drive the same request/read/complete
    // states; this adapter and its loader capability are absent from R4DESK.
    #[cfg(test)]
    pub fn load(
        &mut self,
        loader: &mut impl Loader,
        path: &[u8],
        size: u16,
        container: bool,
    ) -> Option<&Entry> {
        let _ = self.request(path, size, container);
        while let Some(request) = self.take_request() {
            self.complete(request, request.read(loader));
        }
        let request = Request::new(path, size, container, self.epoch)?;
        self.entries.iter().find(|entry| request.matches(entry))
    }
}
fn present(v: c::R4SdkDataSlot) -> bool {
    v.status == 0 && v.flags & 3 == 1
}
fn scalar(v: c::R4SdkDataSlot) -> Option<u64> {
    present(v).then_some(v.bits)
}
pub struct Paint<'a> {
    pub cache: &'a mut Cache,
}
impl Paint<'_> {
    pub fn desktop<F: Fonts>(
        &mut self,
        ui: &mut Ui<'_, '_, F>,
        item: &crate::shell::desktop_items::Item,
        x: i32,
        y: i32,
    ) -> Result<bool> {
        use crate::shell::{
            desktop_items::LaunchKind,
            icon_source::{self, Source, Stage},
        };
        if !ui
            .painter
            .clip()
            .intersects(super::geometry::Rect::new(x, y, 32, 32))
        {
            return Ok(true);
        }
        let source = icon_source::classify(item.icon());
        let plan = icon_source::plan(source, item.launch_kind == LaunchKind::Program);
        for stage in plan.stages() {
            let (path, container) = match stage {
                Stage::External => match source {
                    Source::External(path) => (path, false),
                    _ => continue,
                },
                Stage::Container => (item.launch_path(), true),
                Stage::Standard => (PROGRAM_ICON, false),
            };
            if let Some(icon) = self.cache.request(path, 32, container) {
                icon.paint(ui, x, y)?;
                return Ok(true);
            }
        }
        if item.launch_kind == LaunchKind::File
            && scalar(unsafe {
                c::R4SdkDataIco_isTextFileName(0, item.path().as_ptr(), item.path().len() as u64)
            })
            .is_some_and(|v| v != 0)
        {
            if let Some(icon) = self.cache.request(TEXT_ICON, 32, false) {
                icon.paint(ui, x, y)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}
impl crate::shell::taskbar_view::Icons for Paint<'_> {
    fn quick<F: Fonts>(
        &mut self,
        ui: &mut Ui<'_, '_, F>,
        path: &[u8],
        x: i32,
        y: i32,
    ) -> Result<bool> {
        if !ui
            .painter
            .clip()
            .intersects(super::geometry::Rect::new(x, y, 16, 16))
        {
            return Ok(true);
        }
        match self.cache.request(path, 16, false) {
            Some(e) => {
                e.paint(ui, x, y)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Source {
        reads: usize,
        bad: bool,
    }
    impl Loader for Source {
        fn file(&mut self, _: &[u8], out: &mut [u8]) -> Option<usize> {
            self.reads += 1;
            let bytes = if self.bad {
                b"bad".as_slice()
            } else {
                include_bytes!("../../../../../SDK/Tests/Fixtures/Icons/windows32.ico").as_slice()
            };
            out[..bytes.len()].copy_from_slice(bytes);
            Some(bytes.len())
        }
        fn container(&mut self, p: &[u8], out: &mut [u8]) -> Option<usize> {
            self.file(p, out)
        }
    }
    #[test]
    fn original_cache_keys_capacity_centering_and_negative_misses() {
        let mut cache = Cache::new().unwrap();
        let mut source = Source {
            reads: 0,
            bad: false,
        };
        let e = cache.load(&mut source, b"C:\\X.ICO", 16, false).unwrap();
        assert_eq!(e.pixels[7 * 32 + 7], 0xff);
        assert_eq!(e.alpha[7 * 32 + 7], 255);
        assert_eq!(e.alpha[7 * 32 + 8], 0);
        assert_eq!(e.alpha[0], 0);
        assert!(cache.load(&mut source, b"C:\\X.ICO", 16, false).is_some());
        assert_eq!(source.reads, 1);
        assert!(cache.load(&mut source, b"c:\\X.ICO", 16, false).is_some());
        assert!(cache.load(&mut source, b"C:\\X.ICO", 32, false).is_some());
        assert!(cache.load(&mut source, b"C:\\X.ICO", 16, true).is_some());
        assert_eq!(source.reads, 4);
        for i in 0..32u8 {
            let path = [b'A' + i, b':', b'\\', b'X'];
            assert!(cache.load(&mut source, &path, 16, false).is_some());
        }
        assert_eq!(cache.entries.iter().filter(|e| e.used).count(), 32);
        let before = source.reads;
        assert!(cache.load(&mut source, b"C:\\X.ICO", 16, false).is_some());
        assert_eq!(source.reads, before + 1);
        source.bad = true;
        for _ in 0..2 {
            assert!(cache.load(&mut source, b"C:\\BAD.ICO", 16, false).is_none());
        }
        let n = source.reads;
        assert!(cache.load(&mut source, b"C:\\X", 33, false).is_none());
        assert!(cache.load(&mut source, &[b'x'; 123], 16, true).is_none());
        assert_eq!(source.reads, n);
    }
    struct NoFonts;
    impl Fonts for NoFonts {
        fn revision(&mut self) -> u32 {
            0
        }
        fn has_glyph_row(&self) -> bool {
            false
        }
        fn info(&mut self, _: u32, _: &mut r4os_contract::GuiFontInfo) -> i32 {
            -1
        }
        fn bitmap(&mut self, _: u32, _: u32, _: &mut r4os_contract::GuiGlyphBitmap) -> i32 {
            -1
        }
        fn row(&mut self, _: u32, _: u32, _: u32) -> u64 {
            0
        }
    }
    struct AssetSource {
        bytes: &'static [u8],
        reads: usize,
    }
    impl Loader for AssetSource {
        fn file(&mut self, _: &[u8], out: &mut [u8]) -> Option<usize> {
            self.reads += 1;
            out[..self.bytes.len()].copy_from_slice(self.bytes);
            Some(self.bytes.len())
        }
        fn container(&mut self, path: &[u8], out: &mut [u8]) -> Option<usize> {
            self.file(path, out)
        }
    }
    #[test]
    fn complete_32px_icon_uses_one_native_pixel_transfer() {
        let mut cache = Cache::new().unwrap();
        let mut source = AssetSource {
            bytes: include_bytes!(
                "../../../../../Distribution/Injection/R4OS/Media/Icons/Application.ico"
            ),
            reads: 0,
        };
        PIXEL_BRIDGE_CALLS.with(|count| count.set(0));
        let icon = cache.load(&mut source, b"C:\\APP.ICO", 32, false).unwrap();
        assert_eq!(icon.size, 32);
        assert!(icon.alpha.iter().any(|&alpha| alpha != 0));
        assert_eq!(source.reads, 1);
        assert_eq!(
            PIXEL_BRIDGE_CALLS.with(|count| count.get()),
            1,
            "one native pixel transfer instead of per-pixel transport and field getters"
        );
    }
    #[test]
    fn d091_current_icon_source_reads_and_real_asset_paint_work() {
        use crate::render::{
            geometry::Rect, painter::Painter, primitive_frame::Frame, scene::CpuScene,
            text::cache::Cache as FontCache,
        };
        let mut miss_cache = Cache::new().unwrap();
        let mut bad = Source {
            reads: 0,
            bad: true,
        };
        for i in 0..32 {
            miss_cache.begin_frame(1_000_000_000 + i * 1_000_000);
            assert!(
                miss_cache
                    .load(&mut bad, b"C:\\BAD.ICO", 32, false)
                    .is_none()
            );
        }
        let mut warm_cache = Cache::new().unwrap();
        let mut good = Source {
            reads: 0,
            bad: false,
        };
        for i in 0..32 {
            warm_cache.begin_frame(2_000_000_000 + i * 1_000_000);
            assert!(
                warm_cache
                    .load(&mut good, b"C:\\GOOD.ICO", 32, false)
                    .is_some()
            );
        }
        println!(
            "{{\"schema\":\"r4os.desktop-work\",\"case\":\"d091-icon-io\",\"repaints\":32,\"bad_source_reads\":{},\"warm_good_reads\":{}}}",
            bad.reads, good.reads
        );
        for (name, bytes) in [
            (
                "Application",
                include_bytes!(
                    "../../../../../Distribution/Injection/R4OS/Media/Icons/Application.ico"
                )
                .as_slice(),
            ),
            (
                "Terminal",
                include_bytes!(
                    "../../../../../Distribution/Injection/R4OS/Media/Icons/Terminal.ico"
                )
                .as_slice(),
            ),
        ] {
            let mut cache = Cache::new().unwrap();
            let mut source = AssetSource { bytes, reads: 0 };
            cache.begin_frame(3_000_000_000);
            let icon = cache
                .load(&mut source, b"C:\\ACTUAL.ICO", 32, false)
                .unwrap();
            let bounds = Rect::new(0, 0, 64, 64);
            let clip = Rect::new(9, 11, 27, 26);
            let mut pixels = alloc::vec![0x123456u32; 64 * 64];
            let mut expected = pixels.clone();
            for y in 0..32usize {
                for x in 0..32usize {
                    let (gx, gy) = (7 + x as i32, 9 + y as i32);
                    if icon.alpha[y * 32 + x] >= 128 && clip.contains(gx, gy) {
                        expected[gy as usize * 64 + gx as usize] =
                            icon.pixels[y * 32 + x] & 0xffffff;
                    }
                }
            }
            let mut canvas = CpuScene::new(&mut pixels, 64, 64).unwrap();
            let mut frame = Frame::new().unwrap();
            frame.start(1).unwrap();
            frame.begin(0, bounds).unwrap();
            let mut fonts = NoFonts;
            let mut font_cache = FontCache::new().unwrap();
            let mut ui = Ui::new(
                Painter::recorded(Some(&mut canvas), &mut frame, bounds, clip),
                &mut font_cache,
                &mut fonts,
            );
            icon.paint(&mut ui, 7, 9).unwrap();
            frame.end().unwrap();
            assert_eq!(
                pixels, expected,
                "original per-pixel threshold/colors/clip, real asset {name}"
            );
            let commands = frame.commands().len();
            let requests = commands as u64 + frame.merged_fills;
            println!(
                "{{\"schema\":\"r4os.desktop-work\",\"case\":\"d091-icon-paint\",\"asset\":\"{name}\",\"paint_requests\":{requests},\"primitive_commands\":{commands}}}"
            );
        }
    }

    struct ChangingSource {
        kind: u8,
        reads: usize,
    }
    impl Loader for ChangingSource {
        fn file(&mut self, _: &[u8], out: &mut [u8]) -> Option<usize> {
            self.reads += 1;
            let bytes: &[u8] = match self.kind {
                0 => return None,
                1 => b"bad",
                2 => include_bytes!(
                    "../../../../../Distribution/Injection/R4OS/Media/Icons/Application.ico"
                ),
                _ => include_bytes!(
                    "../../../../../Distribution/Injection/R4OS/Media/Icons/Terminal.ico"
                ),
            };
            out[..bytes.len()].copy_from_slice(bytes);
            Some(bytes.len())
        }
        fn container(&mut self, path: &[u8], out: &mut [u8]) -> Option<usize> {
            self.file(path, out)
        }
    }
    #[test]
    fn d091_retry_arrival_replacement_invalidation_clock_and_independent_budgets() {
        let mut cache = Cache::new().unwrap();
        let mut source = ChangingSource { kind: 0, reads: 0 };
        cache.begin_frame(1_000_000_000);
        assert!(
            cache
                .load(&mut source, b"C:\\LATE.ICO", 32, false)
                .is_none()
        );
        source.kind = 2;
        cache.begin_frame(1_100_000_000);
        assert!(
            cache
                .load(&mut source, b"C:\\LATE.ICO", 32, false)
                .is_none()
        );
        assert_eq!(source.reads, 1, "no repeat until exact bounded retry");
        cache.begin_frame(2_000_000_000);
        let application = cache
            .load(&mut source, b"C:\\LATE.ICO", 32, false)
            .unwrap()
            .pixels;
        assert_eq!(source.reads, 2);
        source.kind = 3;
        cache.begin_frame(2_100_000_000);
        assert_eq!(
            cache
                .load(&mut source, b"C:\\LATE.ICO", 32, false)
                .unwrap()
                .pixels,
            application
        );
        assert_eq!(source.reads, 2);
        cache.begin_frame(7_000_000_000);
        let terminal = cache
            .load(&mut source, b"C:\\LATE.ICO", 32, false)
            .unwrap()
            .pixels;
        assert_ne!(
            terminal, application,
            "valid same-path replacement is revalidated"
        );
        cache.invalidate();
        source.kind = 2;
        assert_eq!(
            cache
                .load(&mut source, b"C:\\LATE.ICO", 32, false)
                .unwrap()
                .pixels,
            application
        );
        cache.begin_frame(6_000_000_000);
        source.kind = 3;
        assert_eq!(
            cache
                .load(&mut source, b"C:\\LATE.ICO", 32, false)
                .unwrap()
                .pixels,
            terminal
        );
        assert_eq!(
            source.reads, 5,
            "explicit hint and backwards clock admit fresh source"
        );
        source.kind = 1;
        cache.begin_frame(10_000_000_000);
        assert!(
            cache
                .load(&mut source, b"C:\\BROKEN.ICO", 16, false)
                .is_none()
        );
        source.kind = 2;
        cache.begin_frame(10_900_000_000);
        assert!(
            cache
                .load(&mut source, b"C:\\BROKEN.ICO", 16, false)
                .is_none()
        );
        cache.begin_frame(11_000_000_000);
        assert!(
            cache
                .load(&mut source, b"C:\\BROKEN.ICO", 16, false)
                .is_some()
        );
        let mut cache = Cache::new().unwrap();
        source.kind = 2;
        cache.begin_frame(100_000_000_000);
        for i in 0..32 {
            let path = alloc::format!("C:\\P{i}.ICO");
            assert!(
                cache
                    .load(&mut source, path.as_bytes(), 16, false)
                    .is_some()
            );
        }
        let positive = source.reads;
        source.kind = 0;
        for i in 0..80 {
            let path = alloc::format!("C:\\M{i}.ICO");
            assert!(
                cache
                    .load(&mut source, path.as_bytes(), 16, false)
                    .is_none()
            );
            assert!(cache.misses.len() <= CAPACITY);
        }
        assert_eq!(cache.entries.iter().filter(|e| e.used).count(), 32);
        for i in 0..32 {
            let path = alloc::format!("C:\\P{i}.ICO");
            assert!(
                cache
                    .load(&mut source, path.as_bytes(), 16, false)
                    .is_some()
            );
        }
        assert_eq!(
            source.reads,
            positive + 80,
            "failed keys cannot evict positive reservations"
        );
        let before = source.reads;
        assert!(cache.load(&mut source, b"C:\\M0.ICO", 16, false).is_none());
        assert_eq!(source.reads, before + 1, "old bounded miss can be evicted");
        assert!(cache.load(&mut source, b"C:\\M79.ICO", 32, false).is_none());
        assert!(cache.load(&mut source, b"C:\\M79.ICO", 16, true).is_none());
        assert_eq!(
            source.reads,
            before + 3,
            "size and container identities stay separate"
        );
    }
    #[test]
    fn d091_binary_alpha_threshold_partial_clip_and_invisible_source_cull() {
        use crate::render::{
            geometry::Rect, painter::Painter, scene::CpuScene, text::cache::Cache as FontCache,
        };
        let bounds = Rect::new(0, 0, 48, 48);
        let clip = Rect::new(2, 3, 29, 28);
        for size in [16u16, 32] {
            for (x, y) in [(-3, 7), (7, -4)] {
                let mut entry = Entry::empty();
                entry.size = size;
                for py in 0..size as usize {
                    for px in 0..size as usize {
                        let at = py * SIZE_MAX + px;
                        entry.pixels[at] = if (px / 4 + py / 4) % 2 == 0 {
                            0x80a0c0
                        } else {
                            0x030507
                        };
                        entry.alpha[at] = match (px + py) % 5 {
                            0 => 127,
                            1 => 128,
                            _ => 255,
                        };
                    }
                }
                let mut pixels = alloc::vec![0x123456; 48 * 48];
                let mut expected = pixels.clone();
                for py in 0..size as usize {
                    for px in 0..size as usize {
                        let (gx, gy) = (x + px as i32, y + py as i32);
                        if bounds.contains(gx, gy)
                            && clip.contains(gx, gy)
                            && entry.alpha[py * SIZE_MAX + px] >= 128
                        {
                            expected[gy as usize * 48 + gx as usize] =
                                entry.pixels[py * SIZE_MAX + px];
                        }
                    }
                }
                let mut canvas = CpuScene::new(&mut pixels, 48, 48).unwrap();
                canvas.set_paint_clip(clip);
                let mut fonts = NoFonts;
                let mut cache = FontCache::new().unwrap();
                let mut ui = Ui::new(Painter::cpu(&mut canvas), &mut cache, &mut fonts);
                entry.paint(&mut ui, x, y).unwrap();
                assert_eq!(pixels, expected);
            }
        }
        let mut pixels = alloc::vec![0x123456; 48 * 48];
        let mut canvas = CpuScene::new(&mut pixels, 48, 48).unwrap();
        let mut fonts = NoFonts;
        let mut font_cache = FontCache::new().unwrap();
        let mut ui = Ui::new(Painter::cpu(&mut canvas), &mut font_cache, &mut fonts);
        let mut cache = Cache::new().unwrap();
        cache.begin_frame(1);
        let mut paint = Paint { cache: &mut cache };
        let item = crate::shell::desktop_items::Item::new(
            b"C:\\MISSING.R4X",
            crate::shell::desktop_items::FsKind::File,
        );
        assert!(paint.desktop(&mut ui, &item, 100, 100).unwrap());
        assert!(
            crate::shell::taskbar_view::Icons::quick(
                &mut paint,
                &mut ui,
                b"C:\\MISSING.ICO",
                100,
                100
            )
            .unwrap()
        );
        assert!(
            !cache.busy(),
            "unaffected icon must not request source work"
        );
    }
    struct WatchedSource {
        source: ChangingSource,
        revision: Option<u64>,
        queries: usize,
        mutate_during_read: bool,
    }
    impl WatchedSource {
        fn new() -> Self {
            Self {
                source: ChangingSource { kind: 2, reads: 0 },
                revision: Some(1),
                queries: 0,
                mutate_during_read: false,
            }
        }
    }
    impl Loader for WatchedSource {
        fn watch(&mut self, _: &[u8]) -> Option<Watch> {
            self.queries += 1;
            self.revision.map(|sequence| {
                Watch(r4os_sdk::abi::DirectoryChangeCursor {
                    sequence,
                    node: 5,
                    mount_generation: 7,
                    mount_slot: 1,
                    ..Default::default()
                })
            })
        }
        fn changed(&mut self, watch: &mut Watch) -> Option<bool> {
            self.queries += 1;
            self.revision.map(|revision| {
                let changed = watch.0.sequence != revision;
                watch.0.sequence = revision;
                changed
            })
        }
        fn file(&mut self, path: &[u8], out: &mut [u8]) -> Option<usize> {
            let result = self.source.file(path, out);
            if self.mutate_during_read {
                self.revision = self.revision.map(|v| v + 1);
            }
            result
        }
        fn container(&mut self, path: &[u8], out: &mut [u8]) -> Option<usize> {
            self.file(path, out)
        }
    }
    #[test]
    fn d091_deferred_minute_reads_and_replacement_keep_paint_free_of_io() {
        for (scenario, step_ns) in [
            ("quiet", 60_000_000_000),
            ("visible_clock", 1_000_000_000),
            ("moving", 16_000_000),
        ] {
            let mut cache = Cache::new().unwrap();
            let mut source = WatchedSource::new();
            let mut now = 0;
            while now < 60_000_000_000 {
                cache.begin_frame(now);
                let (reads, queries) = (source.source.reads, source.queries);
                let _ = cache.request(b"C:\\ACTUAL.ICO", 32, false);
                assert_eq!(
                    (source.source.reads, source.queries),
                    (reads, queries),
                    "paint cannot access a source"
                );
                while let Some(request) = cache.take_request() {
                    let result = request.read(&mut source);
                    cache.complete(request, result);
                }
                assert!(cache.request(b"C:\\ACTUAL.ICO", 32, false).is_some());
                now += step_ns;
            }
            assert_eq!(
                source.source.reads, 1,
                "unchanged admitted watches avoid full content reloads"
            );
            println!(
                "{{\"schema\":\"r4os.desktop-work\",\"case\":\"d091-deferred-minute\",\"scenario\":\"{scenario}\",\"source_reads\":{},\"watch_calls\":{}}}",
                source.source.reads, source.queries
            );
        }
        let mut cache = Cache::new().unwrap();
        let mut source = WatchedSource::new();
        cache.begin_frame(0);
        assert!(cache.request(b"C:\\ACTUAL.ICO", 32, false).is_none());
        let request = cache.take_request().unwrap();
        assert!(cache.take_request().is_none());
        let loaded = request.read(&mut source);
        for _ in 0..100 {
            assert!(cache.request(b"C:\\ACTUAL.ICO", 32, false).is_none());
        }
        assert!(
            cache.pending.iter().all(Option::is_none),
            "in-flight source deduplicated"
        );
        assert!(cache.complete(request, loaded));
        let application = cache.request(b"C:\\ACTUAL.ICO", 32, false).unwrap().pixels;
        source.source.kind = 3;
        source.revision = Some(2);
        cache.begin_frame(5_000_000_000);
        assert_eq!(
            cache.request(b"C:\\ACTUAL.ICO", 32, false).unwrap().pixels,
            application
        );
        let request = cache.take_request().unwrap();
        let loaded = request.read(&mut source);
        assert_eq!(
            cache.request(b"C:\\ACTUAL.ICO", 32, false).unwrap().pixels,
            application,
            "old pixels live until joined publication"
        );
        assert!(cache.complete(request, loaded));
        assert_ne!(
            cache.request(b"C:\\ACTUAL.ICO", 32, false).unwrap().pixels,
            application,
            "same-path parent publication updates pixels"
        );
        assert_eq!(source.source.reads, 2);
    }
    #[test]
    fn d091_deferred_epoch_capacity_and_failed_worker_backoff() {
        let mut cache = Cache::new().unwrap();
        let mut source = WatchedSource::new();
        cache.begin_frame(0);
        for i in 0..80 {
            let _ = cache.request(alloc::format!("C:\\P{i}.ICO").as_bytes(), 16, false);
        }
        assert_eq!(cache.pending.iter().flatten().count(), 32);
        while let Some(request) = cache.take_request() {
            cache.complete(request, request.read(&mut source));
        }
        assert_eq!(source.source.reads, 32);
        cache.begin_frame(5_000_000_000);
        let old = cache.request(b"C:\\P0.ICO", 16, false).unwrap().pixels;
        let request = cache.take_request().unwrap();
        assert!(!cache.complete(request, Refresh::Retry));
        for now in [5_000_000_001, 5_999_999_999] {
            cache.begin_frame(now);
            assert_eq!(cache.request(b"C:\\P0.ICO", 16, false).unwrap().pixels, old);
            assert!(
                !cache.busy(),
                "worker creation failure retains pixels and full retry backoff"
            );
        }
        cache.begin_frame(6_000_000_000);
        let _ = cache.request(b"C:\\P0.ICO", 16, false);
        let old_request = cache.take_request().unwrap();
        let old_result = old_request.read(&mut source);
        cache.invalidate();
        assert!(cache.request(b"C:\\P0.ICO", 16, false).is_none());
        let current = cache.take_request().unwrap();
        assert!(
            !cache.complete(old_request, old_result),
            "late joined old epoch cannot publish or clear new work"
        );
        assert!(cache.in_flight == Some(current));
        assert!(cache.complete(current, current.read(&mut source)));
        cache.begin_frame(11_000_000_000);
        let _ = cache.request(b"C:\\P0.ICO", 16, false);
        let request = cache.take_request().unwrap();
        assert!(cache.complete(request, Refresh::Failed));
        assert!(
            cache.request(b"C:\\P0.ICO", 16, false).is_none(),
            "a failed source replaces stale pixels with the ordinary fallback"
        );
        assert!(!cache.busy());
    }
    #[test]
    fn d091_changed_during_read_and_unavailable_watch_never_bless_cached_bytes() {
        let mut cache = Cache::new().unwrap();
        let mut source = WatchedSource::new();
        source.mutate_during_read = true;
        cache.begin_frame(0);
        let _ = cache.request(b"C:\\RACE.ICO", 32, false);
        let request = cache.take_request().unwrap();
        assert!(cache.complete(request, request.read(&mut source)));
        assert!(
            cache
                .request(b"C:\\RACE.ICO", 32, false)
                .unwrap()
                .watch
                .is_none()
        );
        source.mutate_during_read = false;
        cache.begin_frame(5_000_000_000);
        let _ = cache.request(b"C:\\RACE.ICO", 32, false);
        let request = cache.take_request().unwrap();
        cache.complete(request, request.read(&mut source));
        assert_eq!(
            source.source.reads, 2,
            "a change during reading cannot skip the next read"
        );
        source.revision = None;
        source.source.kind = 3;
        cache.begin_frame(10_000_000_000);
        let old = cache.request(b"C:\\RACE.ICO", 32, false).unwrap().pixels;
        let request = cache.take_request().unwrap();
        assert!(cache.complete(request, request.read(&mut source)));
        assert_ne!(
            cache.request(b"C:\\RACE.ICO", 32, false).unwrap().pixels,
            old
        );
        assert_eq!(
            source.source.reads, 3,
            "unavailable watches retain actual content revalidation"
        );
    }
}
