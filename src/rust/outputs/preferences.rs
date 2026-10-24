//! Strict display documents remain R4GFX-owned. Recovery and atomic saves use
//! the existing shared document store; failed loads retain the previous value.
use super::topology_wire as wire;
use crate::{
    render::output_geometry::topology::{Key, Layout, Output},
    runtime::library::{required, result, scalar},
    shell::persistence::{Error as StoreError, Store},
};
use alloc::vec::Vec;
use core::{ffi::CStr, mem::MaybeUninit};
use r4os_library_helpers::{Error as NativeError, raw as c};
#[derive(Debug)]
pub enum Error {
    Memory,
    Invalid,
    Native(NativeError),
    Store(StoreError),
}
impl From<NativeError> for Error {
    fn from(e: NativeError) -> Self {
        Self::Native(e)
    }
}
impl From<StoreError> for Error {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}
type Result<T> = core::result::Result<T, Error>;
// These records contain fixed metadata only, never native resources. Native
// construction/parsing writes directly at the final heap address, including
// the roughly19KB color config, without a large caller-stack intermediate.
pub struct Record<T: Copy> {
    storage: Vec<MaybeUninit<T>>,
}
impl<T: Copy> Record<T> {
    fn make(call: impl FnOnce(*mut T) -> c::R4LibrariesSlot) -> Result<Self> {
        let mut storage = Vec::new();
        storage.try_reserve_exact(1).map_err(|_| Error::Memory)?;
        storage.push(MaybeUninit::<T>::uninit());
        required(call(storage[0].as_mut_ptr()))?;
        Ok(Self { storage })
    }
    pub(crate) fn get(&self) -> &T {
        unsafe { self.storage[0].assume_init_ref() }
    }
    fn pointer(&mut self) -> *mut T {
        self.storage[0].as_mut_ptr()
    }
}
fn path(slot: c::R4LibrariesSlot) -> Result<&'static [u8]> {
    let slot = required(slot)?;
    if slot.ptr.is_null() {
        return Err(Error::Invalid);
    }
    // Only the four native resident path constants call this function. Their
    // original Zig types are sentinel-terminated string literals.
    let value = unsafe { CStr::from_ptr(slot.ptr.cast()) }.to_bytes();
    if value.is_empty() || value.len() > r4os_sdk::path::FILE_PATH_MAX {
        return Err(Error::Invalid);
    }
    Ok(value)
}
fn load<T: Copy>(
    store: &Store<'_, '_>,
    path: &[u8],
    maximum: usize,
    create: impl FnOnce(*mut T) -> c::R4LibrariesSlot,
    parse: impl FnOnce(*const u8, u64, *mut T) -> c::R4LibrariesSlot,
) -> Result<Record<T>> {
    store.recover(path)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(maximum)
        .map_err(|_| Error::Memory)?;
    bytes.resize(maximum, 0);
    let mut value = Record::make(create)?;
    if let Some(count) = store.read(path, &mut bytes)? {
        if count == 0 || count > maximum {
            return Err(Error::Invalid);
        }
        required(parse(bytes.as_ptr(), count as u64, value.pointer()))?;
    }
    Ok(value)
}
fn save(
    store: &Store<'_, '_>,
    path: &[u8],
    maximum: usize,
    encode: impl FnOnce(*mut u8, u64) -> c::R4LibrariesSlot,
) -> Result<()> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(maximum)
        .map_err(|_| Error::Memory)?;
    bytes.resize(maximum, 0);
    let slot = required(encode(bytes.as_mut_ptr(), maximum as u64))?;
    let count = usize::try_from(slot.length).map_err(|_| Error::Invalid)?;
    let start = bytes.as_ptr() as usize;
    let pointer = slot.ptr as usize;
    if pointer < start
        || pointer
            .checked_add(count)
            .is_none_or(|end| end > start + bytes.len())
    {
        return Err(Error::Invalid);
    }
    store.save(path, &bytes[pointer - start..pointer - start + count])?;
    Ok(())
}
pub type Colors = Record<c::R4GfxColorPreferencesConfig>;
pub struct Preferences {
    layout: Record<c::R4GfxPreferencesConfig>,
    colors: Colors,
    refresh: Record<c::R4GfxRefreshPreferencesConfig>,
    brightness: Record<c::R4GfxBrightnessPreferencesConfig>,
}
impl Preferences {
    pub fn new() -> Result<Self> {
        Ok(Self {
            layout: Record::make(|p| unsafe { c::R4GfxPreferencesConfig_create(p) })?,
            colors: Record::make(|p| unsafe { c::R4GfxColorPreferencesConfig_create(p) })?,
            refresh: Record::make(|p| unsafe { c::R4GfxRefreshPreferencesConfig_create(p) })?,
            brightness: Record::make(|p| unsafe { c::R4GfxBrightnessPreferencesConfig_create(p) })?,
        })
    }
    pub fn reload_layout(&mut self, store: &Store<'_, '_>) -> Result<()> {
        self.layout = load(
            store,
            path(unsafe { c::R4GfxPreferences_constant_path() })?,
            c::R4GfxPreferences_max_bytes as usize,
            |p| unsafe { c::R4GfxPreferencesConfig_create(p) },
            |b, n, p| unsafe { c::R4GfxPreferencesConfig_parse(0, b, n, p) },
        )?;
        Ok(())
    }
    pub fn read_colors(store: &Store<'_, '_>) -> Result<Colors> {
        load(
            store,
            path(unsafe { c::R4GfxColorPreferences_constant_path() })?,
            c::R4GfxColorPreferences_max_bytes as usize,
            |p| unsafe { c::R4GfxColorPreferencesConfig_create(p) },
            |b, n, p| unsafe { c::R4GfxColorPreferencesConfig_parse(0, b, n, p) },
        )
    }
    pub fn same_colors(&self, colors: &Colors) -> Result<bool> {
        Ok(
            scalar(unsafe {
                c::R4Media_colorPreferencesEqual(0, self.colors.get(), colors.get())
            })? != 0,
        )
    }
    pub fn color_in(colors: &Colors, key: Key) -> Result<Option<c::R4GfxColorPreferencesChoice>> {
        let key = wire::key(key)?;
        let mut out = MaybeUninit::uninit();
        let slot = result(unsafe {
            c::R4GfxColorPreferencesConfig_find(0, colors.get(), &key, out.as_mut_ptr())
        })?;
        Ok(if slot.flags & 1 != 0 {
            Some(unsafe { out.assume_init() })
        } else {
            None
        })
    }
    pub fn color(&self, key: Key) -> Result<Option<c::R4GfxColorPreferencesChoice>> {
        Self::color_in(&self.colors, key)
    }
    pub fn replace_colors(&mut self, colors: Colors) {
        self.colors = colors;
    }
    pub fn same_choice(
        left: Option<&c::R4GfxColorPreferencesChoice>,
        right: Option<&c::R4GfxColorPreferencesChoice>,
    ) -> Result<bool> {
        fn slot(value: Option<&c::R4GfxColorPreferencesChoice>) -> c::R4LibrariesSlot {
            let mut slot: c::R4LibrariesSlot = unsafe { MaybeUninit::zeroed().assume_init() };
            if let Some(value) = value {
                slot.ptr = (value as *const c::R4GfxColorPreferencesChoice)
                    .cast_mut()
                    .cast();
                slot.flags = 1;
            }
            slot
        }
        Ok(scalar(unsafe { c::R4Media_colorChoiceEqual(0, slot(left), slot(right)) })? != 0)
    }
    pub fn reload_refresh(&mut self, store: &Store<'_, '_>) -> Result<()> {
        self.refresh = load(
            store,
            path(unsafe { c::R4GfxRefreshPreferences_constant_path() })?,
            c::R4GfxRefreshPreferences_max_bytes as usize,
            |p| unsafe { c::R4GfxRefreshPreferencesConfig_create(p) },
            |b, n, p| unsafe { c::R4GfxRefreshPreferencesConfig_parse(0, b, n, p) },
        )?;
        Ok(())
    }
    pub fn reload_brightness(&mut self, store: &Store<'_, '_>) -> Result<()> {
        self.brightness = load(
            store,
            path(unsafe { c::R4GfxBrightnessPreferences_constant_path() })?,
            c::R4GfxBrightnessPreferences_max_bytes as usize,
            |p| unsafe { c::R4GfxBrightnessPreferencesConfig_create(p) },
            |b, n, p| unsafe { c::R4GfxBrightnessPreferencesConfig_parse(0, b, n, p) },
        )?;
        Ok(())
    }
    pub fn layout(&self, key: Key) -> Result<Option<Output>> {
        let key = wire::key(key)?;
        let mut out = MaybeUninit::<c::R4GfxTopologyOutput>::uninit();
        let slot = result(unsafe {
            c::R4GfxPreferencesConfig_find(0, self.layout.get(), &key, out.as_mut_ptr())
        })?;
        if slot.flags & 1 == 0 {
            return Ok(None);
        }
        Ok(Some(wire::read_output(unsafe { out.assume_init_ref() })?))
    }
    pub fn refresh(&self, key: Key) -> Result<Option<c::R4GfxRefreshPreferencesChoice>> {
        let key = wire::key(key)?;
        let mut out = MaybeUninit::uninit();
        let slot = result(unsafe {
            c::R4GfxRefreshPreferencesConfig_find(0, self.refresh.get(), &key, out.as_mut_ptr())
        })?;
        Ok(if slot.flags & 1 != 0 {
            Some(unsafe { out.assume_init() })
        } else {
            None
        })
    }
    pub fn refresh_choice(&self, key: Key) -> Result<c::R4GfxRefreshPreferencesChoice> {
        if let Some(choice) = self.refresh(key)? {
            return Ok(choice);
        }
        let key = wire::key(key)?;
        let mut choice = MaybeUninit::uninit();
        required(unsafe { c::R4GfxRefreshPreferencesChoice_create(choice.as_mut_ptr()) })?;
        result(unsafe { c::R4GfxRefreshPreferencesChoice_set_key(choice.as_mut_ptr(), &key) })?;
        Ok(unsafe { choice.assume_init() })
    }
    pub fn brightness(&self, key: Key) -> Result<Option<c::R4GfxBrightnessPreferencesChoice>> {
        let key = wire::key(key)?;
        let mut out = MaybeUninit::uninit();
        let slot = result(unsafe {
            c::R4GfxBrightnessPreferencesConfig_find(
                0,
                self.brightness.get(),
                &key,
                out.as_mut_ptr(),
            )
        })?;
        Ok(if slot.flags & 1 != 0 {
            Some(unsafe { out.assume_init() })
        } else {
            None
        })
    }
    pub fn save_layout(&mut self, store: &Store<'_, '_>, layout: &Layout) -> Result<()> {
        let layout = wire::layout(layout)?;
        let next = Record::make(|p| unsafe {
            c::R4GfxPreferencesConfig_remember(0, self.layout.get(), &layout, p)
        })?;
        save(
            store,
            path(unsafe { c::R4GfxPreferences_constant_path() })?,
            c::R4GfxPreferences_max_bytes as usize,
            |b, n| unsafe { c::R4GfxPreferencesConfig_encode(0, next.get(), b, n) },
        )?;
        self.layout = next;
        Ok(())
    }
    pub fn save_color(
        &mut self,
        store: &Store<'_, '_>,
        choice: &c::R4GfxColorPreferencesChoice,
    ) -> Result<()> {
        // Merge the latest complete document; another settings consumer can
        // have changed a different receiver during this confirmation.
        let previous = Self::read_colors(store)?;
        let next = Record::make(|p| unsafe {
            c::R4GfxColorPreferencesConfig_remember(0, previous.get(), choice, p)
        })?;
        save(
            store,
            path(unsafe { c::R4GfxColorPreferences_constant_path() })?,
            c::R4GfxColorPreferences_max_bytes as usize,
            |b, n| unsafe { c::R4GfxColorPreferencesConfig_encode(0, next.get(), b, n) },
        )?;
        self.colors = next;
        Ok(())
    }
}
pub fn profile(
    choice: &c::R4GfxColorPreferencesChoice,
) -> Result<Option<([u8; r4os_sdk::path::FILE_PATH_MAX + 1], u32, u32)>> {
    if scalar(unsafe { c::R4GfxColorPreferencesChoice_enabled(0, choice) })? == 0 {
        return Ok(None);
    }
    let mut filename = [0; r4os_sdk::path::FILE_PATH_MAX + 1];
    required(unsafe {
        c::R4GfxColorPreferencesChoice_get_filename(choice, filename.as_mut_ptr().cast())
    })?;
    Ok(Some((
        filename,
        scalar(unsafe { c::R4GfxColorPreferencesChoice_get_intent(choice) })? as u32,
        scalar(unsafe { c::R4GfxColorPreferencesChoice_get_flags(choice) })? as u32,
    )))
}
