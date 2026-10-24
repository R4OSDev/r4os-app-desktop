//! Exact floor sampling for a bounded span inside the global viewport.
//! Callers validate nonzero dimensions and offset + span <= destination.
#[derive(Clone, Copy)]
pub(super) struct Axis {
    position: u32,
    remainder: u64,
    advance: u32,
    residual: u64,
    denominator: u64,
}
impl Axis {
    pub(super) fn new(source: u32, destination: u32, offset: u32) -> Self {
        let numerator = u64::from(offset) * u64::from(source);
        let denominator = u64::from(destination);
        Self {
            position: (numerator / denominator) as u32,
            remainder: numerator % denominator,
            advance: source / destination,
            residual: u64::from(source % destination),
            denominator,
        }
    }
    #[inline(always)]
    pub(super) fn next(&mut self) -> u32 {
        let position = self.position;
        self.position += self.advance;
        self.remainder += self.residual;
        if self.remainder >= self.denominator {
            self.remainder -= self.denominator;
            self.position += 1;
        }
        position
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_floor_recurrence_preserves_large_offsets_and_downscale_skips() {
        for source in [1, 2, 3, 7, 8, 255, 256, i32::MAX as u32] {
            for destination in [1u32, 2, 3, 5, 13, 742, 650, i32::MAX as u32] {
                for offset in [0, destination / 2, destination.saturating_sub(31)] {
                    let mut axis = Axis::new(source, destination, offset);
                    for i in 0..(destination - offset).min(31) {
                        let original =
                            u64::from(offset + i) * u64::from(source) / u64::from(destination);
                        assert_eq!(u64::from(axis.next()), original);
                    }
                }
            }
        }
    }
}
