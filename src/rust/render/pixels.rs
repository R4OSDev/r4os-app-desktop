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
    fn row_bytes(self, index: usize, count: usize) -> &'a [u8] {
        let start = index.checked_mul(4).expect("bounded pixel row start");
        let end = index
            .checked_add(count)
            .and_then(|n| n.checked_mul(4))
            .expect("bounded pixel row end");
        &self.bytes[start..end]
    }
    /// Complete checked row, with two native pixels per integer transfer. No
    /// unaligned Rust word slice or stronger source alignment is manufactured.
    /// The SDK's qualified Rust target-default feature policy is unchanged.
    pub fn copy_masked_to(self, index: usize, target: &mut [u32], mask: u32, set: u32) {
        let source = self.row_bytes(index, target.len());
        let mask64 = u64::from(mask) | (u64::from(mask) << 32);
        let set64 = u64::from(set) | (u64::from(set) << 32);
        let pairs = target.len() / 2;
        for i in 0..pairs {
            // Exact row admission proves both complete 8-byte accesses. The
            // safe immutable source and exclusive target loans are disjoint;
            // both native addresses may be unaligned for u64.
            unsafe {
                let word = core::ptr::read_unaligned(source.as_ptr().cast::<u64>().add(i));
                core::ptr::write_unaligned(
                    target.as_mut_ptr().cast::<u64>().add(i),
                    (word & mask64) | set64,
                );
            }
        }
        if target.len() % 2 != 0 {
            let i = pairs * 2;
            target[i] =
                (unsafe { core::ptr::read_unaligned(source.as_ptr().cast::<u32>().add(i)) } & mask)
                    | set;
        }
    }
    pub fn copy_to(self, index: usize, target: &mut [u32]) {
        let source = self.row_bytes(index, target.len());
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
