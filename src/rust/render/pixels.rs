//! Immutable native XRGB/ARGB transport bytes. A source may be unaligned;
//! never manufacture an unaligned Rust u32 slice from a GUI command buffer.
#[derive(Clone, Copy, Debug)]
pub struct PixelBytes<'a> {
    bytes: &'a [u8],
}
impl<'a> PixelBytes<'a> {
    pub fn new(bytes: &'a [u8]) -> Option<Self> {
        (bytes.len() % 4 == 0).then_some(Self { bytes })
    }
    pub fn from_words(words: &'a [u32]) -> Self {
        // x86_64 R4OS uses native little endian. The immutable byte view
        // retains the exact word loan and does not strengthen its alignment.
        Self {
            bytes: unsafe {
                core::slice::from_raw_parts(words.as_ptr().cast(), core::mem::size_of_val(words))
            },
        }
    }
    pub fn len(self) -> usize {
        self.bytes.len() / 4
    }
    pub fn bytes(self) -> &'a [u8] {
        self.bytes
    }
    #[inline(always)]
    pub fn word(self, index: usize) -> u32 {
        assert!(index < self.len(), "bounded pixel");
        // Construction admits complete native little-endian words. The
        // explicit bound keeps this safe entry checked, including overflowing
        // indices, without an out-of-line Result conversion for every pixel.
        unsafe { core::ptr::read_unaligned(self.bytes.as_ptr().cast::<u32>().add(index)) }
    }
    pub fn copy_to(self, index: usize, target: &mut [u32]) {
        let source = &self.bytes[index * 4..(index + target.len()) * 4];
        // Both loans are valid and disjoint; byte-copy keeps unaligned
        // transports valid while retaining the original contiguous row path.
        unsafe {
            core::ptr::copy_nonoverlapping(
                source.as_ptr(),
                target.as_mut_ptr().cast(),
                source.len(),
            );
        }
    }
}
