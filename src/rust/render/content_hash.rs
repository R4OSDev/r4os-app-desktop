//! Existing external BLAKE3, with compile-time C size/alignment assertions.
//! Rust never forms a reference to the opaque native record or its padding.
use core::mem::MaybeUninit;
unsafe extern "C" {
    fn r4desk_content_init(storage: *mut core::ffi::c_void);
    fn r4desk_content_update(storage: *mut core::ffi::c_void, bytes: *const u8, count: usize);
    fn r4desk_content_finish(storage: *const core::ffi::c_void, out: *mut u8);
}
pub struct Blake3 {
    storage: MaybeUninit<[u64; 239]>,
}
impl Blake3 {
    pub fn new() -> Self {
        let mut result = Self {
            storage: MaybeUninit::uninit(),
        };
        unsafe {
            r4desk_content_init(result.storage.as_mut_ptr().cast());
        }
        result
    }
    pub fn update(&mut self, bytes: &[u8]) {
        unsafe {
            r4desk_content_update(
                self.storage.as_mut_ptr().cast(),
                bytes.as_ptr(),
                bytes.len(),
            );
        }
    }
    pub fn finish(self) -> [u8; 32] {
        let mut result = [0; 32];
        unsafe {
            r4desk_content_finish(self.storage.as_ptr().cast(), result.as_mut_ptr());
        }
        result
    }
}
