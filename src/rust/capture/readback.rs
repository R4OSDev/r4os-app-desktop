//! Stable adapter to the original R4GFX readback owner. Copy, conversion,
//! bounds, epochs and retirement remain in that library's single algorithm.
use crate::render::gpu::{Error, Result, device::Session};
use alloc::alloc::{alloc, alloc_zeroed, dealloc};
use core::{alloc::Layout, ffi::c_void, mem::MaybeUninit, ptr::NonNull};
use r4gfx as g;
use r4os_library_helpers::raw as c;
use r4os_sdk::App;
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
pub const CONVERSION_PIXELS: usize = 64 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Phase {
    Empty,
    Idle,
    Copying,
    Converting,
    Ready,
    Draining,
    Failed,
}
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub frames: u64,
    pub failed: u64,
    pub copy_bytes: u64,
    pub cpu_read_bytes: u64,
    pub cpu_write_bytes: u64,
    pub last_latency_ns: u64,
    pub max_latency_ns: u64,
}
struct Native {
    memory: c::R4LibraryMemory,
    allocator: c::R4LibraryAllocator,
    client: c::R4LibraryDeviceClient,
    colors: c::R4LibraryColorClient,
    device: g::R4GfxDevice,
    owner: c::R4GfxReadbackOwner,
}
pub struct Readback {
    native: NonNull<Native>,
    closed: bool,
}
// All native self-pointers target the same stable allocation. Allocator
// callbacks use Rust's global allocator and no App or local thread state.
// Actual device calls require a matching, serialized Session on that task.
unsafe impl Send for Readback {}
fn checked(slot: c::R4LibrariesSlot) -> Result<c::R4LibrariesSlot> {
    if slot.status != 0 {
        return Err(Error::Graphics);
    }
    if slot.flags & 2 != 0 {
        if slot.ptr.is_null() || slot.length > 256 {
            return Err(Error::Graphics);
        }
        let name =
            unsafe { core::slice::from_raw_parts(slot.ptr.cast::<u8>(), slot.length as usize) };
        return Err(match name {
            b"Busy" => Error::Busy,
            b"Unsupported" => Error::Unsupported,
            b"Stale" => Error::Stale,
            b"Limit" => Error::Limit,
            b"Deadline" => Error::Deadline,
            _ => Error::Graphics,
        });
    }
    Ok(slot)
}
fn scalar(slot: c::R4LibrariesSlot) -> Result<u64> {
    let slot = checked(slot)?;
    if slot.flags & 1 == 0 {
        return Err(Error::Unsupported);
    }
    Ok(slot.bits)
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
impl Readback {
    /// # Safety
    /// The exact R4X/provider and callback code remain resident until close,
    /// including serialized moves between main and preparation tasks.
    pub unsafe fn new(app: &App<'_>, session: &Session<'_, '_>) -> Result<Self> {
        let pointer =
            NonNull::new(unsafe { alloc_zeroed(Layout::new::<Native>()) }.cast::<Native>())
                .ok_or(Error::Limit)?;
        let value = Self {
            native: pointer,
            closed: true,
        };
        let n = unsafe { &mut *pointer.as_ptr() };
        n.device = *session.raw();
        checked(unsafe {
            c::R4LibraryMemory_create(
                &mut n.memory,
                allocate as *const () as usize as u64,
                release as *const () as usize as u64,
            )
        })?;
        checked(unsafe { c::R4LibraryMemory_allocator(0, &mut n.memory, &mut n.allocator) })?;
        checked(unsafe { c::R4Library_deviceClient(0, app.start_context(), &mut n.client) })?;
        checked(unsafe { c::R4Library_colorClient(0, app.start_context(), &mut n.colors) })?;
        checked(unsafe {
            c::R4GfxReadbackOwner_init(
                0,
                &n.allocator,
                &n.client,
                &n.colors,
                (&n.device as *const g::R4GfxDevice).cast(),
                &mut n.owner,
            )
        })?;
        Ok(value)
    }
    fn pointer(&self) -> *const c::R4GfxReadbackOwner {
        unsafe { core::ptr::addr_of!((*self.native.as_ptr()).owner) }
    }
    fn matching(&mut self, session: &Session<'_, '_>) -> Result<*mut c::R4GfxReadbackOwner> {
        let n = unsafe { &*self.native.as_ptr() };
        if n.device.address != session.raw().address
            || n.device.generation != session.raw().generation
        {
            return Err(Error::Stale);
        }
        Ok(self.pointer().cast_mut())
    }
    pub fn phase(&self) -> Result<Phase> {
        match scalar(unsafe { c::R4GfxReadbackOwner_get_phase(self.pointer()) })? {
            0 => Ok(Phase::Empty),
            1 => Ok(Phase::Idle),
            2 => Ok(Phase::Copying),
            3 => Ok(Phase::Converting),
            4 => Ok(Phase::Ready),
            5 => Ok(Phase::Draining),
            6 => Ok(Phase::Failed),
            _ => Err(Error::State),
        }
    }
    pub fn pending(&self) -> bool {
        matches!(
            self.phase(),
            Ok(Phase::Copying | Phase::Converting | Phase::Draining)
        )
    }
    pub fn source(&self) -> Result<g::R4GfxResource> {
        let mut out = g::R4GfxResource::default();
        checked(unsafe {
            c::R4GfxReadbackOwner_get_source(
                self.pointer(),
                (&mut out as *mut g::R4GfxResource).cast(),
            )
        })?;
        Ok(out)
    }
    pub fn valid(&self) -> bool {
        scalar(unsafe { c::R4GfxReadbackOwner_get_valid(self.pointer()) }) == Ok(1)
    }
    pub fn acknowledged(&self) -> u64 {
        scalar(unsafe { c::R4GfxReadbackOwner_get_acknowledged(self.pointer()) }).unwrap_or(0)
    }
    pub fn invalidate(&mut self) {
        let _ = checked(unsafe { c::R4GfxReadbackOwner_set_valid(self.pointer().cast_mut(), 0) });
    }
    pub fn matches(
        &self,
        width: u32,
        height: u32,
        format: u32,
        color: g::R4GfxColorDescription,
    ) -> bool {
        scalar(unsafe {
            c::R4GfxReadbackOwner_matches(
                0,
                self.pointer(),
                width,
                height,
                format,
                (&color as *const g::R4GfxColorDescription).cast(),
            )
        }) == Ok(1)
    }
    pub fn prepare(
        &mut self,
        session: &Session<'_, '_>,
        width: u32,
        height: u32,
        format: u32,
        color: g::R4GfxColorDescription,
    ) -> Result<()> {
        let pointer = self.matching(session)?;
        self.closed = false; // a failed prepare may still own partial resources
        checked(unsafe {
            c::R4GfxReadbackOwner_prepare(
                0,
                pointer,
                width,
                height,
                format,
                (&color as *const g::R4GfxColorDescription).cast(),
            )
        })?;
        Ok(())
    }
    pub fn begin(
        &mut self,
        session: &Session<'_, '_>,
        source: g::R4GfxResource,
        epoch: u64,
        frame: u64,
        base: u64,
        regions: &[g::R4GfxRect],
        now: u64,
        deadline: u64,
    ) -> Result<()> {
        let pointer = self.matching(session)?;
        let mut request = MaybeUninit::<c::R4GfxReadbackRequest>::uninit();
        checked(unsafe {
            c::R4GfxReadbackRequest_from_fields(
                request.as_mut_ptr(),
                (&source as *const g::R4GfxResource).cast(),
                epoch,
                frame,
                base,
                regions.as_ptr().cast(),
                regions.len() as u64,
                core::ptr::null(),
                0,
                now,
                deadline,
            )
        })?;
        checked(unsafe { c::R4GfxReadbackOwner_begin(0, pointer, request.as_ptr()) })?;
        Ok(())
    }
    pub fn cancel(&mut self, session: &Session<'_, '_>, deadline: bool) -> Result<()> {
        let name = if deadline {
            &b"Deadline"[..]
        } else {
            &b"Stale"[..]
        };
        checked(unsafe {
            c::R4GfxReadbackOwner_cancel(
                0,
                self.matching(session)?,
                name.as_ptr(),
                name.len() as u64,
            )
        })?;
        Ok(())
    }
    pub fn poll(&mut self, session: &Session<'_, '_>, now: u64) -> Result<()> {
        checked(unsafe { c::R4GfxReadbackOwner_poll(0, self.matching(session)?, now) })?;
        Ok(())
    }
    pub fn acknowledge(&mut self, published: bool) -> Result<()> {
        checked(unsafe {
            c::R4GfxReadbackOwner_acknowledge(0, self.pointer().cast_mut(), u8::from(published))
        })?;
        Ok(())
    }
    pub fn pixels(&self) -> Result<&[u32]> {
        if self.phase()? != Phase::Ready {
            return Err(Error::Busy);
        }
        let slot = checked(unsafe { c::R4GfxReadbackOwner_get_pixels(self.pointer()) })?;
        let count = usize::try_from(slot.length).map_err(|_| Error::Limit)?;
        if slot.flags & 1 == 0
            || count == 0
            || count > MAX_BYTES / 4
            || slot.ptr.is_null()
            || !(slot.ptr as usize).is_multiple_of(4)
            || (slot.ptr as usize).checked_add(count * 4).is_none()
        {
            return Err(Error::State);
        }
        // Original prepare initializes the whole shadow. An immutable borrow
        // prevents poll/close/mutation for the duration of this view.
        Ok(unsafe { core::slice::from_raw_parts(slot.ptr.cast::<u32>(), count) })
    }
    pub fn stats(&self) -> Result<Stats> {
        let mut out = MaybeUninit::<c::R4GfxReadbackStats>::uninit();
        checked(unsafe { c::R4GfxReadbackOwner_get_stats(self.pointer(), out.as_mut_ptr()) })?;
        Ok(Stats {
            frames: scalar(unsafe { c::R4GfxReadbackStats_get_frames(out.as_ptr()) })?,
            failed: scalar(unsafe { c::R4GfxReadbackStats_get_failed(out.as_ptr()) })?,
            copy_bytes: scalar(unsafe { c::R4GfxReadbackStats_get_copy_bytes(out.as_ptr()) })?,
            cpu_read_bytes: scalar(unsafe {
                c::R4GfxReadbackStats_get_cpu_read_bytes(out.as_ptr())
            })?,
            cpu_write_bytes: scalar(unsafe {
                c::R4GfxReadbackStats_get_cpu_write_bytes(out.as_ptr())
            })?,
            last_latency_ns: scalar(unsafe {
                c::R4GfxReadbackStats_get_last_latency_ns(out.as_ptr())
            })?,
            max_latency_ns: scalar(unsafe {
                c::R4GfxReadbackStats_get_max_latency_ns(out.as_ptr())
            })?,
        })
    }
    pub fn first_region(&self) -> Result<g::R4GfxRect> {
        let mut regions = [g::R4GfxRect::default(); 8];
        checked(unsafe {
            c::R4GfxReadbackOwner_get_regions(
                self.pointer(),
                (&mut regions as *mut [g::R4GfxRect; 8]).cast(),
            )
        })?;
        if scalar(unsafe { c::R4GfxReadbackOwner_get_region_count(self.pointer()) })? == 0 {
            return Err(Error::State);
        }
        Ok(regions[0])
    }
    pub fn close(&mut self, session: &Session<'_, '_>) -> Result<()> {
        checked(unsafe { c::R4GfxReadbackOwner_close(0, self.matching(session)?) })?;
        self.closed = true;
        Ok(())
    }
}
impl Drop for Readback {
    fn drop(&mut self) {
        if self.closed {
            unsafe { dealloc(self.native.as_ptr().cast(), Layout::new::<Native>()) };
        }
    }
}
