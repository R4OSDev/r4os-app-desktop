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
        #[cfg(test)]
        work::add(bytes.len());
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

#[cfg(test)]
pub(crate) mod work {
    std::thread_local! { static COUNT: core::cell::Cell<(u64,u64)> = const { core::cell::Cell::new((0,0)) }; }
    pub fn reset() {
        COUNT.with(|c| c.set((0, 0)));
    }
    pub fn add(bytes: usize) {
        COUNT.with(|c| {
            let (calls, n) = c.get();
            c.set((calls + 1, n + bytes as u64));
        });
    }
    pub fn get() -> (u64, u64) {
        COUNT.with(|c| c.get())
    }
}
