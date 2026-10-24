//! Retained monitor profile using the original R4GFX file/ICC owner. Refused
//! Close keeps its storage, source bytes, color client and allocator callbacks.
use crate::runtime::library::{Context, required, result, scalar};
use alloc::alloc::{alloc, alloc_zeroed, dealloc};
use core::{alloc::Layout, ffi::c_void, ptr::NonNull};
use r4gfx as g;
use r4os_library_helpers::{Error, raw as c};
use r4os_sdk::{App, path::FilePath};
struct Native {
    memory: c::R4LibraryMemory,
    allocator: c::R4LibraryAllocator,
    colors: c::R4LibraryColorClient,
    owner: c::R4GfxDisplayProfileOwner,
}
pub struct Profile {
    native: NonNull<Native>,
    closed: bool,
}
unsafe extern "C" fn allocate(_: *mut c_void, bytes: usize, alignment: usize) -> *mut u8 {
    let Ok(layout) = Layout::from_size_align(bytes.max(1), alignment) else {
        return core::ptr::null_mut();
    };
    unsafe { alloc(layout) }
}
unsafe extern "C" fn release(_: *mut c_void, pointer: *mut u8, bytes: usize, alignment: usize) {
    if let Ok(layout) = Layout::from_size_align(bytes.max(1), alignment) {
        unsafe {
            dealloc(pointer, layout);
        }
    }
}
impl Profile {
    /// # Safety
    /// The actual library/provider/callback code remains resident until Close.
    pub unsafe fn open_file(
        app: &App<'_>,
        context: &Context<'_, '_>,
        path: &[u8],
        intent: u32,
        flags: u32,
    ) -> Result<Self, Error> {
        let path = FilePath::parse(path).map_err(|_| Error::Native("File"))?;
        let pointer =
            NonNull::new(unsafe { alloc_zeroed(Layout::new::<Native>()) }.cast::<Native>())
                .ok_or(Error::Native("Memory"))?;
        let mut profile = Self {
            native: pointer,
            closed: true,
        };
        let n = unsafe { &mut *pointer.as_ptr() };
        result(unsafe {
            c::R4LibraryMemory_create(
                &mut n.memory,
                allocate as *const () as usize as u64,
                release as *const () as usize as u64,
            )
        })?;
        required(unsafe { c::R4LibraryMemory_allocator(0, &mut n.memory, &mut n.allocator) })?;
        required(unsafe { c::R4Library_colorClient(0, app.start_context(), &mut n.colors) })?;
        required(unsafe {
            c::R4Media_profileOpenFile(
                0,
                &n.allocator,
                &n.colors,
                context.system(),
                path.as_c_str().as_ptr().cast(),
                intent,
                flags,
                &mut n.owner,
            )
        })?;
        profile.closed = false;
        Ok(profile)
    }
    pub fn handle(&self) -> Result<g::R4GfxColorProfile, Error> {
        let mut out = g::R4GfxColorProfile::default();
        if self.closed {
            return Ok(out);
        }
        required(unsafe {
            c::R4GfxDisplayProfileOwner_get_handle(
                &(*self.native.as_ptr()).owner,
                (&mut out as *mut g::R4GfxColorProfile).cast(),
            )
        })?;
        Ok(out)
    }
    pub fn apply(&self, pixels: &mut [u32], width: u32, height: u32) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Native("Closed"));
        }
        result(unsafe {
            c::R4GfxDisplayProfileOwner_applySdr(
                0,
                &(*self.native.as_ptr()).owner,
                pixels.as_mut_ptr(),
                pixels.len() as u64,
                width,
                height,
            )
        })?;
        Ok(())
    }
    pub fn close(&mut self) -> Result<bool, Error> {
        if self.closed {
            return Ok(true);
        }
        let closed = scalar(unsafe {
            c::R4GfxDisplayProfileOwner_close(0, &mut (*self.native.as_ptr()).owner)
        })? == 1;
        self.closed = closed;
        Ok(closed)
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        if self.closed {
            unsafe { dealloc(self.native.as_ptr().cast(), Layout::new::<Native>()) };
        }
    }
}
