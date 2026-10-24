//! no_std has no sqrt/ceil methods. Use two compiler intrinsics through an
//! integer C ABI; no geometry algorithm or mutable native state lives there.
unsafe extern "C" {
    fn r4desk_sqrt_f32_bits(bits: u32) -> u32;
    fn r4desk_ceil_f32_bits(bits: u32) -> u32;
}
pub(super) fn sqrt(value: f32) -> f32 {
    f32::from_bits(unsafe { r4desk_sqrt_f32_bits(value.to_bits()) })
}
pub(super) fn ceil(value: f32) -> f32 {
    f32::from_bits(unsafe { r4desk_ceil_f32_bits(value.to_bits()) })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intrinsic_boundary_preserves_current_ieee_scalar_cases() {
        assert_eq!(sqrt(4.0), 2.0);
        assert_eq!(sqrt(-0.0).to_bits(), (-0.0f32).to_bits());
        assert_eq!(sqrt(f32::INFINITY), f32::INFINITY);
        assert!(sqrt(-1.0).is_nan());
        assert!(sqrt(f32::NAN).is_nan());
        assert_eq!(ceil(1.25), 2.0);
        assert_eq!(ceil(-1.25), -1.0);
        assert_eq!(ceil(-0.25).to_bits(), (-0.0f32).to_bits());
        assert_eq!(ceil(f32::INFINITY), f32::INFINITY);
        assert!(ceil(f32::NAN).is_nan());
    }
}
