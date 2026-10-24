//! Synchronous native Library allocation callbacks over the normal allocator.
use alloc::alloc::{Layout, alloc, dealloc};
use core::ffi::c_void;
pub(crate) unsafe extern "C" fn allocate(
    _: *mut c_void,
    bytes: usize,
    alignment: usize,
) -> *mut u8 {
    let Ok(layout) = Layout::from_size_align(bytes.max(1), alignment) else {
        return core::ptr::null_mut();
    };
    unsafe { alloc(layout) }
}
pub(crate) unsafe extern "C" fn release(
    _: *mut c_void,
    pointer: *mut u8,
    bytes: usize,
    alignment: usize,
) {
    if let Ok(layout) = Layout::from_size_align(bytes.max(1), alignment) {
        unsafe { dealloc(pointer, layout) };
    }
}
