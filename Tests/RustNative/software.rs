//! Retained Zig compositor cases, using the same actual R4GFX COLOR_V1
//! implementation. No test-side color approximation or replacement math.
use super::*;
use crate::render::{
    pixels::PixelBytes,
    primitive_image::{Format, Picture, View},
};
use alloc::vec;
unsafe extern "C" {
    fn r4desk_color_transform(
        source: *const g::R4GfxColorImage,
        target: *const g::R4GfxColorImage,
        request: *const g::R4GfxColorTransform,
        stats: *mut g::R4GfxCpuStats,
    ) -> i32;
    fn r4desk_fixture_sha256(bytes: *const u8, count: usize, out: *mut u8);
}
pub(crate) struct OriginalColors;
unsafe impl Colors for OriginalColors {
    unsafe fn transform(
        &self,
        source: &g::R4GfxColorImage,
        target: &g::R4GfxColorImage,
        request: &g::R4GfxColorTransform,
        stats: &mut g::R4GfxCpuStats,
    ) -> i32 {
        unsafe { r4desk_color_transform(source, target, request, stats) }
    }
}
#[test]
fn original_partial_tiles_match_linear_reference_origins_holes_and_translucent_cursor() {
    for (ox, oy) in [(0, 0), (-79, 23)] {
        for cursor in [false, true] {
            let bounds = Rect::new(ox, oy, 130, 97);
            let a = Rect::new(ox + 5, oy + 7, 101, 75);
            let b = Rect::new(ox + 112, oy + 76, 7, 9);
            let alpha = Rect::new(a.x + 3, a.y + 4, 2, 2);
            let mut outputs = [vec![0x123456u32; 130 * 97], vec![0x123456; 130 * 97]];
            let mut counts = [Stats::default(); 2];
            for reference in 0..2 {
                let mut owner = Owner::new(1024 * 1024).unwrap();
                owner.begin(bounds).unwrap();
                for (damage, color) in [(a, 0), (b, 0x24a6df)] {
                    owner
                        .cache
                        .paint(1, bounds, damage, None, None, |p| p.fill(damage, color))
                        .unwrap();
                }
                if cursor {
                    let white = [0x80ffffffu32; 4];
                    owner
                        .cache
                        .paint(2, alpha, alpha, None, None, |p| {
                            p.picture(Picture {
                                view: View::new(
                                    Format::Argb,
                                    2,
                                    2,
                                    8,
                                    PixelBytes::from_words(&white).bytes(),
                                ),
                                viewport: alpha,
                                clip: alpha,
                                source_x: 0,
                                source_y: 0,
                                guest_w: 2,
                                guest_h: 2,
                            })
                        })
                        .unwrap();
                }
                if reference != 0 {
                    // Transparent final damage disables only the opaque copy.
                    for damage in [a, b] {
                        owner
                            .cache
                            .paint(99, bounds, damage, None, None, |_| Ok(()))
                            .unwrap();
                    }
                }
                let mut canvas = CpuScene::output(&mut outputs[reference], bounds).unwrap();
                counts[reference] = owner.finish(&OriginalColors, &mut canvas).unwrap();
            }
            assert!(counts[0].direct_pixels > 0 && counts[1].direct_pixels == 0);
            assert!(counts[0].pixels < counts[1].pixels);
            assert_eq!(outputs[0], outputs[1]);
            for (i, pixel) in outputs[0].iter().enumerate() {
                let x = ox + (i % 130) as i32;
                let y = oy + (i / 130) as i32;
                let expected = if cursor && alpha.contains(x, y) {
                    0xbcbcbc
                } else if b.contains(x, y) {
                    0x24a6df
                } else if a.contains(x, y) {
                    0
                } else {
                    0x123456
                };
                assert_eq!(
                    *pixel, expected,
                    "origin {ox},{oy}; cursor {cursor}; pixel {i}"
                );
            }
        }
    }
}
#[test]
fn original_whole_1080p_software_output_hashes_at_both_client_sizes() {
    // Exact public synthetic source and two digests in software_output_fixture.zig.
    // No ROM and no wall-clock benchmark. Culling is tested by omitting the
    // covered background; lifecycle admission is a separate owner check.
    let expected = [
        "075b3f7143ecee7d15bc34d60a3de7a8ad185ff38e63ec3d91a5ac10e9be7873",
        "1e9e4db23bbbbdfb7b880cca679056f1b7670d5624e04bbbeeb586380a440c38",
    ];
    use crate::diagnostics::software_profile as fixture;
    assert_eq!(fixture::HASHES, expected);
    assert_eq!((fixture::WIDTH, fixture::HEIGHT), (1920, 1080));
    let source = (0usize..256 * 224)
        .map(fixture::source_pixel)
        .collect::<alloc::vec::Vec<_>>();
    let bounds = Rect::new(0, 0, 1920, 1080);
    for culling in [false, true] {
        let mut pixels = vec![0x123456u32; 1920 * 1080];
        let mut owner = Owner::new(128 * 1024 * 1024).unwrap();
        for (i, client) in fixture::CLIENTS.into_iter().enumerate() {
            owner.begin(bounds).unwrap();
            if !culling {
                owner
                    .cache
                    .paint(1, bounds, client, None, None, |p| p.fill(bounds, 0x2468ac))
                    .unwrap();
            }
            let frame = Rect::new(client.x - 8, client.y - 30, client.w + 16, client.h + 38);
            // calculateViewport's original integer preference85% selects1x/4x.
            let scale = if i == 0 { 1 } else { 4 };
            let viewport = Rect::new(
                client.x + (client.w - 256 * scale) / 2,
                client.y + (client.h - 224 * scale) / 2,
                256 * scale,
                224 * scale,
            );
            assert_eq!(fixture::viewport(client).unwrap(), viewport);
            owner
                .cache
                .paint(64, frame, client, None, None, |p| {
                    p.fill(frame, 0x102030)?;
                    p.picture(Picture {
                        view: View::new(
                            Format::Xrgb,
                            256,
                            224,
                            256 * 4,
                            PixelBytes::from_words(&source).bytes(),
                        ),
                        viewport,
                        clip: frame,
                        source_x: 0,
                        source_y: 0,
                        guest_w: 256,
                        guest_h: 224,
                    })
                })
                .unwrap();
            let mut canvas = CpuScene::output(&mut pixels, bounds).unwrap();
            owner.finish(&OriginalColors, &mut canvas).unwrap();
            let mut digest = [0u8; 32];
            unsafe {
                r4desk_fixture_sha256(
                    pixels.as_ptr().cast(),
                    pixels.len() * 4,
                    digest.as_mut_ptr(),
                );
            }
            let actual = digest
                .iter()
                .map(|b| alloc::format!("{b:02x}"))
                .collect::<alloc::string::String>();
            assert_eq!(actual, expected[i], "client {i}; culling {culling}");
        }
    }
}
