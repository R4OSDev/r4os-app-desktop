//! Exact existing CPU output transform and limited-range quantization oracles.
use super::*;
use crate::render::output_geometry::{
    self as geometry,
    topology::{Point, Rotation},
};
use r4os_library_helpers::raw;
unsafe extern "C" {
    fn r4desk_cpu_transform(
        parameters: *const [i64; 11],
        source: *const u32,
        count: usize,
        output: *mut u32,
        length: usize,
    );
}
#[test]
fn original_cpu_output_rotations_scales_partial_pixels_and_limited_channels() {
    for rotation in [
        Rotation::Normal,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ] {
        for scale in [60, 120, 150, 180, 240, 960] {
            let view = Viewport {
                pixel_w: 12,
                pixel_h: 8,
                scale,
                rotation,
                origin: Point { x: -19, y: 7 },
            };
            let bounds = geometry::logical(view).unwrap();
            let source: Vec<u32> = (0..bounds.w * bounds.h)
                .map(|i| (i as u32 * 1997) ^ 0x112233)
                .collect();
            for damage in [Rect::new(0, 0, 12, 8), Rect::new(1, 1, 9, 5)] {
                let mut expected = [0xfefefefe; 96];
                let mut actual = expected;
                let parameters = [
                    12,
                    8,
                    i64::from(scale),
                    rotation as i64,
                    -19,
                    7,
                    i64::from(damage.x),
                    i64::from(damage.y),
                    i64::from(damage.w),
                    i64::from(damage.h),
                    i64::from(bounds.w),
                ];
                unsafe {
                    r4desk_cpu_transform(
                        &parameters,
                        source.as_ptr(),
                        source.len(),
                        expected.as_mut_ptr(),
                        expected.len(),
                    );
                }
                frame::transform_region(view, &source, bounds.w as usize, &mut actual, damage)
                    .unwrap();
                assert_eq!(actual, expected);
            }
        }
    }
    for value in 0u32..=255 {
        let pixel =
            value | ((255 - value) << 8) | (((value * 73) % 256) << 16) | ((value / 2) << 24);
        let original =
            crate::runtime::library::scalar(unsafe { raw::R4GfxColorSignal_limitedRgb8(0, pixel) })
                .unwrap() as u32;
        assert_eq!(frame::limited_rgb8(pixel), original);
    }
}
