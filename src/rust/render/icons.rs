//! The Desktop's bounded ICO cache. Selection, BMP parsing and pixel decoding
//! stay in the original SDK data owner. This owner only loads and places them.
use super::{
    primitive_assets::{Error, Result},
    text::cache::Fonts,
    ui::Ui,
};
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use r4os_sdk::{App, data::raw as c, path::FilePath, storage::System};
pub const PATH_MAX: usize = 127;
pub const SIZE_MAX: usize = 32;
const CAPACITY: usize = 32;
const INPUT_MAX: usize = 8192;
pub const PROGRAM_ICON: &[u8] = b"C:\\R4OS\\Media\\Icons\\Application.ico";
pub const TEXT_ICON: &[u8] = b"C:\\R4OS\\Media\\Icons\\Textfile.ico";

pub trait Loader {
    fn file(&mut self, path: &[u8], output: &mut [u8]) -> Option<usize>;
    fn container(&mut self, path: &[u8], output: &mut [u8]) -> Option<usize>;
}
pub struct Native<'a, 'start> {
    pub app: &'a App<'start>,
}
impl Loader for Native<'_, '_> {
    fn file(&mut self, path: &[u8], output: &mut [u8]) -> Option<usize> {
        let path = FilePath::parse(path).ok()?;
        System::new(self.app)
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
        let pointer = self.app.system_api()?.module_resource_read;
        if pointer == 0 {
            return None;
        }
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
        }
    }
    pub fn paint<F: Fonts>(&self, ui: &mut Ui<'_, '_, F>, x: i32, y: i32) -> Result<()> {
        for py in 0..self.size as usize {
            for px in 0..self.size as usize {
                let n = py * SIZE_MAX + px;
                // Original Desktop policy is binary visibility at alpha 128.
                if self.alpha[n] >= 128 {
                    ui.rect(x + px as i32, y + py as i32, 1, 1, self.pixels[n])?;
                }
            }
        }
        Ok(())
    }
}
pub struct Cache {
    entries: Vec<Entry>,
    input: Vec<u8>,
    next: usize,
}
impl Cache {
    pub fn new() -> Result<Self> {
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(CAPACITY)
            .map_err(|_| Error::OutOfMemory)?;
        entries.resize_with(CAPACITY, Entry::empty);
        let mut input = Vec::new();
        input
            .try_reserve_exact(INPUT_MAX)
            .map_err(|_| Error::OutOfMemory)?;
        input.resize(INPUT_MAX, 0);
        Ok(Self {
            entries,
            input,
            next: 0,
        })
    }
    pub fn load(
        &mut self,
        loader: &mut impl Loader,
        path: &[u8],
        size: u16,
        container: bool,
    ) -> Option<&Entry> {
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
        if let Some(index) = self.entries.iter().position(|e| {
            e.used && e.size == size && e.path_len == len && e.path[..len] == key[..len]
        }) {
            return Some(&self.entries[index]);
        }
        let n = if container {
            loader.container(path, &mut self.input)
        } else {
            loader.file(path, &mut self.input)
        }?;
        if n == 0 || n > self.input.len() {
            return None;
        }
        let bytes = &self.input[..n];
        let mut choice = MaybeUninit::uninit();
        if !present(unsafe {
            c::R4SdkDataIco_chooseBest(0, bytes.as_ptr(), n as u64, size, choice.as_mut_ptr())
        }) {
            return None;
        }
        let mut image = MaybeUninit::uninit();
        if !present(unsafe {
            c::R4SdkDataIco_parseBmpImage(
                0,
                bytes.as_ptr(),
                n as u64,
                choice.as_ptr(),
                image.as_mut_ptr(),
            )
        }) {
            return None;
        }
        let width = scalar(unsafe { c::R4SdkDataIcoBitmapImage_get_width(image.as_ptr()) })?;
        let height = scalar(unsafe { c::R4SdkDataIcoBitmapImage_get_height(image.as_ptr()) })?;
        if width == 0 || height == 0 || width > u32::MAX as u64 || height > u32::MAX as u64 {
            return None;
        }
        let index = if let Some(i) = self.entries.iter().position(|e| !e.used) {
            i
        } else {
            let i = self.next;
            self.next = (i + 1) % CAPACITY;
            i
        };
        let e = &mut self.entries[index];
        e.used = false;
        e.path = key;
        e.path_len = len;
        e.size = size;
        e.pixels.fill(0);
        e.alpha.fill(0);
        let w = (size as u64).min(width);
        let h = (size as u64).min(height);
        let ox = (size as u64 - w) / 2;
        let oy = (size as u64 - h) / 2;
        for py in 0..h {
            for px in 0..w {
                let mut pixel = MaybeUninit::uninit();
                if !present(unsafe {
                    c::R4SdkDataIco_pixelAt(
                        0,
                        bytes.as_ptr(),
                        n as u64,
                        image.as_ptr(),
                        (px * width / w) as u32,
                        (py * height / h) as u32,
                        pixel.as_mut_ptr(),
                    )
                }) {
                    continue;
                }
                if let (Some(rgb), Some(alpha)) = (
                    scalar(unsafe { c::R4SdkDataIcoPixel_get_rgb(pixel.as_ptr()) }),
                    scalar(unsafe { c::R4SdkDataIcoPixel_get_alpha(pixel.as_ptr()) }),
                ) {
                    let dst = ((oy + py) * SIZE_MAX as u64 + ox + px) as usize;
                    e.pixels[dst] = rgb as u32;
                    e.alpha[dst] = alpha as u8;
                }
            }
        }
        e.used = true;
        Some(e)
    }
}
fn present(v: c::R4SdkDataSlot) -> bool {
    v.status == 0 && v.flags & 3 == 1
}
fn scalar(v: c::R4SdkDataSlot) -> Option<u64> {
    present(v).then_some(v.bits)
}
pub struct Paint<'a, L> {
    pub cache: &'a mut Cache,
    pub loader: &'a mut L,
}
impl<L: Loader> Paint<'_, L> {
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
            if let Some(icon) = self.cache.load(self.loader, path, 32, container) {
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
            if let Some(icon) = self.cache.load(self.loader, TEXT_ICON, 32, false) {
                icon.paint(ui, x, y)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}
impl<L: Loader> crate::shell::taskbar_view::Icons for Paint<'_, L> {
    fn quick<F: Fonts>(
        &mut self,
        ui: &mut Ui<'_, '_, F>,
        path: &[u8],
        x: i32,
        y: i32,
    ) -> Result<bool> {
        match self.cache.load(self.loader, path, 16, false) {
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
}
