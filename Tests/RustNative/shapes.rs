//! The six original GUI shape groups and their fixed Zig pixel hashes.
use super::*;
use crate::render::scene::CpuScene;
use alloc::{vec, vec::Vec};
fn put(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
macro_rules! field {
    ($b:expr,$n:ident,$v:expr) => {
        put($b, core::mem::offset_of!(a::GuiShapeResource, $n), $v)
    };
}
macro_rules! scalar {
    ($b:expr,$n:ident,$v:expr) => {
        field!($b, $n, ($v as f32).to_bits())
    };
}
fn header(kind: u32) -> Vec<u8> {
    let mut b = vec![0; core::mem::size_of::<a::GuiShapeResource>()];
    field!(&mut b, version, a::gui_shape_resource_version);
    field!(&mut b, size, a::gui_shape_resource_size);
    field!(&mut b, geometry_kind, kind);
    field!(&mut b, fill_rule, a::gui_shape_fill_rule_nonzero);
    b
}
fn rounded(x: f32, y: f32, w: f32, h: f32, radii: [f32; 8], fill: u32) -> Vec<u8> {
    let mut b = header(a::gui_shape_geometry_kind_rounded_rect);
    scalar!(&mut b, geometry_x_bits, x);
    scalar!(&mut b, geometry_y_bits, y);
    scalar!(&mut b, geometry_w_bits, w);
    scalar!(&mut b, geometry_h_bits, h);
    field!(&mut b, fill_argb, fill);
    for (i, v) in radii.into_iter().enumerate() {
        put(
            &mut b,
            core::mem::offset_of!(a::GuiShapeResource, radius_top_left_x_bits) + i * 4,
            v.to_bits(),
        );
    }
    b
}
fn path(fill: u32, stroke: u32, width: f32, round: bool) -> Vec<u8> {
    let mut b = header(a::gui_shape_geometry_kind_path);
    field!(&mut b, fill_argb, fill);
    field!(&mut b, stroke_argb, stroke);
    scalar!(&mut b, stroke_width_bits, width);
    scalar!(&mut b, miter_limit_bits, 4.0);
    field!(
        &mut b,
        line_join,
        if round {
            a::gui_shape_line_join_round
        } else {
            a::gui_shape_line_join_miter
        }
    );
    field!(
        &mut b,
        line_cap,
        if round {
            a::gui_shape_line_cap_round
        } else {
            a::gui_shape_line_cap_butt
        }
    );
    b
}
fn segment(bytes: &mut Vec<u8>, kind: u32, points: &[(f32, f32)]) {
    let start = bytes.len();
    bytes.resize(start + core::mem::size_of::<a::GuiPathSegment>(), 0);
    put(bytes, start, kind);
    for (i, (x, y)) in points.iter().enumerate() {
        put(bytes, start + 8 + i * 8, x.to_bits());
        put(bytes, start + 12 + i * 8, y.to_bits());
    }
    let count = ((bytes.len() - core::mem::size_of::<a::GuiShapeResource>())
        / core::mem::size_of::<a::GuiPathSegment>()) as u32;
    field!(bytes, segment_count, count);
}
fn command(kind: u32, w: u32, h: u32, bytes: &[u8]) -> a::GuiFrameCommand {
    a::GuiFrameCommand {
        kind,
        w,
        h,
        resource_bytes: bytes.len() as u64,
        ..Default::default()
    }
}
fn hash(pixels: &[u32]) -> u64 {
    let mut h = 1469598103934665603u64;
    for value in pixels {
        for byte in value.to_le_bytes() {
            h = (h ^ u64::from(byte)).wrapping_mul(1099511628211);
        }
    }
    h
}
fn apply(pixels: &mut [u32], w: i32, h: i32, command: &a::GuiFrameCommand, bytes: &[u8]) -> bool {
    let mut scene = CpuScene::new(pixels, w, h).unwrap();
    Renderer::default()
        .paint(
            &mut Painter::cpu(&mut scene),
            Rect::new(0, 0, w, h),
            command,
            bytes,
        )
        .unwrap()
}
fn dirty(mut command: a::GuiFrameCommand, bytes: &[u8]) {
    command.x = -5;
    command.y = -4;
    let client = Rect::new(3, 2, 40, 35);
    let mut reference = vec![0x203040; 48 * 40];
    let mut scene = CpuScene::new(&mut reference, 48, 40).unwrap();
    assert!(
        Renderer::default()
            .paint(&mut Painter::cpu(&mut scene), client, &command, bytes)
            .unwrap()
    );
    for y in (0..40).step_by(5) {
        for x in (0..48).step_by(6) {
            let clip = Rect::new(x, y, 6, 5);
            let mut actual = vec![0x203040; 48 * 40];
            let mut scene = CpuScene::new(&mut actual, 48, 40).unwrap();
            scene.set_paint_clip(clip);
            Renderer::default()
                .paint(&mut Painter::cpu(&mut scene), client, &command, bytes)
                .unwrap();
            for (i, pixel) in actual.into_iter().enumerate() {
                let expected = if clip.contains((i % 48) as i32, (i / 48) as i32) {
                    reference[i]
                } else {
                    0x203040
                };
                assert_eq!(pixel, expected, "dirty({x},{y}) pixel{i}");
            }
        }
    }
}
#[cfg_attr(test, test)]
fn original_tiny_damage_bounds_scratch_independently_of_command_dimensions() {
    let mut pixels = [0; 16 * 16];
    let mut scene = CpuScene::new(&mut pixels, 16, 16).unwrap();
    scene.set_paint_clip(Rect::new(8, 8, 1, 1));
    let b = rounded(0.0, 0.0, 2048.0, 2048.0, [0.0; 8], 0xffffffff);
    let c = command(a::gui_frame_command_kind_rounded_rect, 2048, 2048, &b);
    let crop = mask::area(scene.paint_bounds(), scene.full_rect(), &c, 0).unwrap();
    assert_eq!(crop.w * crop.h, 1);
    assert!(
        Renderer {
            mask_limit: 2,
            ..Renderer::default()
        }
        .paint(
            &mut Painter::cpu(&mut scene),
            Rect::new(0, 0, 16, 16),
            &c,
            &b
        )
        .unwrap()
    );
    assert_eq!(pixels[8 * 16 + 8], 0xffffff);
    assert_eq!(pixels.iter().filter(|p| **p != 0).count(), 1);
}
#[cfg_attr(test, test)]
fn original_rounded_golden_elliptical_corners_unequal_borders_clip_and_alpha() {
    let mut pixels = vec![0x203040; 48 * 36];
    let mut b = rounded(
        4.0,
        3.0,
        38.0,
        28.0,
        [2.0, 8.0, 12.0, 4.0, 7.0, 11.0, 14.0, 6.0],
        0xc04080c0,
    );
    scalar!(&mut b, border_top_bits, 1.0);
    scalar!(&mut b, border_right_bits, 3.0);
    scalar!(&mut b, border_bottom_bits, 5.0);
    scalar!(&mut b, border_left_bits, 2.0);
    field!(&mut b, stroke_argb, 0xe0f08020);
    field!(&mut b, clip_x, 2);
    field!(&mut b, clip_y, 1);
    field!(&mut b, clip_w, 44);
    field!(&mut b, clip_h, 32);
    let c = command(a::gui_frame_command_kind_rounded_rect, 48, 36, &b);
    assert!(apply(&mut pixels, 48, 36, &c, &b));
    assert_eq!(hash(&pixels), 0x42e1a133ae5b1f7e);
    dirty(c, &b);
}
#[cfg_attr(test, test)]
fn original_cubic_circle_fill_and_antialiased_stroke_golden() {
    let mut pixels = vec![0xf5f5f5; 40 * 40];
    let mut b = path(0xb02070d0, 0xf0d04020, 2.5, true);
    let k = (0.55228475f64 * 14.0) as f32;
    segment(&mut b, a::gui_path_segment_kind_move, &[(34.0, 20.0)]);
    segment(
        &mut b,
        a::gui_path_segment_kind_cubic,
        &[(34.0, 20.0 + k), (20.0 + k, 34.0), (20.0, 34.0)],
    );
    segment(
        &mut b,
        a::gui_path_segment_kind_cubic,
        &[(20.0 - k, 34.0), (6.0, 20.0 + k), (6.0, 20.0)],
    );
    segment(
        &mut b,
        a::gui_path_segment_kind_cubic,
        &[(6.0, 20.0 - k), (20.0 - k, 6.0), (20.0, 6.0)],
    );
    segment(
        &mut b,
        a::gui_path_segment_kind_cubic,
        &[(20.0 + k, 6.0), (34.0, 20.0 - k), (34.0, 20.0)],
    );
    segment(&mut b, a::gui_path_segment_kind_close, &[]);
    let fill = command(a::gui_frame_command_kind_path_fill, 40, 40, &b);
    let stroke = command(a::gui_frame_command_kind_path_stroke, 40, 40, &b);
    assert!(apply(&mut pixels, 40, 40, &fill, &b));
    assert!(apply(&mut pixels, 40, 40, &stroke, &b));
    assert_eq!(hash(&pixels), 0x60de7abf2dcf1cfb);
    dirty(fill, &b);
    dirty(stroke, &b);
}
#[cfg_attr(test, test)]
fn original_separate_outer_and_inset_shadow_layers_golden() {
    let mut pixels = vec![0xd8d8d8; 52 * 40];
    let mut outer = rounded(10.0, 8.0, 30.0, 21.0, [7.0; 8], 0xfff8f8f8);
    field!(&mut outer, stroke_argb, 0xff4070a0);
    for name in [
        core::mem::offset_of!(a::GuiShapeResource, border_top_bits),
        core::mem::offset_of!(a::GuiShapeResource, border_right_bits),
        core::mem::offset_of!(a::GuiShapeResource, border_bottom_bits),
        core::mem::offset_of!(a::GuiShapeResource, border_left_bits),
    ] {
        put(&mut outer, name, 1.0f32.to_bits());
    }
    field!(&mut outer, shadow_argb, 0x80000000);
    scalar!(&mut outer, shadow_offset_x_bits, 2.0);
    scalar!(&mut outer, shadow_offset_y_bits, 3.0);
    scalar!(&mut outer, shadow_spread_bits, 2.0);
    scalar!(&mut outer, shadow_blur_bits, 4.0);
    let shadow = command(a::gui_frame_command_kind_shadow, 52, 40, &outer);
    let rect = command(a::gui_frame_command_kind_rounded_rect, 52, 40, &outer);
    assert!(apply(&mut pixels, 52, 40, &shadow, &outer));
    assert!(apply(&mut pixels, 52, 40, &rect, &outer));
    let mut inset = rounded(10.0, 8.0, 30.0, 21.0, [7.0; 8], 0);
    field!(&mut inset, shadow_argb, 0x70000000);
    scalar!(&mut inset, shadow_offset_x_bits, 1.0);
    scalar!(&mut inset, shadow_offset_y_bits, 1.0);
    scalar!(&mut inset, shadow_blur_bits, 3.0);
    field!(&mut inset, flags, a::gui_shape_flag_shadow_inset);
    let inside = command(a::gui_frame_command_kind_shadow, 52, 40, &inset);
    assert!(apply(&mut pixels, 52, 40, &inside, &inset));
    assert_eq!(hash(&pixels), 0xc80d01c04d523d7d);
    dirty(shadow, &outer);
    dirty(inside, &inset);
}
#[cfg_attr(test, test)]
fn original_one_logical_command_tiny_and_large_rounded_goldens() {
    let mut tiny = [0; 16];
    let b = rounded(1.0, 1.0, 2.0, 2.0, [4.0; 8], 0xffffffff);
    let c = command(a::gui_frame_command_kind_rounded_rect, 4, 4, &b);
    assert!(apply(&mut tiny, 4, 4, &c, &b));
    assert_eq!(hash(&tiny), 0x67f14b9ed59a9be3);
    let mut large = vec![0x101010; 257 * 129];
    let b = rounded(
        2.0,
        2.0,
        253.0,
        125.0,
        [50.0, 30.0, 20.0, 55.0, 70.0, 18.0, 9.0, 45.0],
        0xff336699,
    );
    let c = command(a::gui_frame_command_kind_rounded_rect, 257, 129, &b);
    assert!(apply(&mut large, 257, 129, &c, &b));
    assert_eq!(hash(&large), 0xac46157755c83ac6);
}
#[cfg_attr(test, test)]
fn original_malformed_resources_rejected_before_replay() {
    let mut pixels = [0x123456; 16 * 16];
    let mut scene = CpuScene::new(&mut pixels, 16, 16).unwrap();
    let mut painter = Painter::cpu(&mut scene);
    let mut owner = Renderer::default();
    let bounds = Rect::new(0, 0, 16, 16);
    let mut b = path(0xffffffff, 0, 0.0, false);
    segment(&mut b, a::gui_path_segment_kind_move, &[(1.0, 1.0)]);
    segment(&mut b, a::gui_path_segment_kind_line, &[(8.0, 8.0)]);
    let c = command(a::gui_frame_command_kind_path_fill, 16, 16, &b);
    assert_eq!(
        owner.paint(&mut painter, bounds, &c, &b[..b.len() - 1]),
        Err(Error::Invalid)
    );
    let start = core::mem::size_of::<a::GuiShapeResource>();
    put(&mut b, start, 99);
    assert_eq!(
        owner.paint(&mut painter, bounds, &c, &b),
        Err(Error::Invalid)
    );
    put(&mut b, start, a::gui_path_segment_kind_move);
    put(&mut b, start + 8, 0x7f800000);
    assert_eq!(
        owner.paint(&mut painter, bounds, &c, &b),
        Err(Error::Invalid)
    );
    let mut oversized = c;
    oversized.w = a::gui_shape_max_dimension + 1;
    assert_eq!(
        owner.paint(&mut painter, bounds, &oversized, &b),
        Err(Error::Invalid)
    );
    let mut foreign = c;
    foreign.kind = a::gui_frame_command_kind_rounded_rect;
    assert_eq!(
        owner.paint(&mut painter, bounds, &foreign, &b),
        Err(Error::Invalid)
    );
    assert!(pixels.iter().all(|p| *p == 0x123456));
}
fn gradient_resource(kind: u32, first: u32, last: u32) -> Vec<u8> {
    let mut b = rounded(0.0, 0.0, 32.0, 24.0, [0.0; 8], 0);
    let start = b.len();
    b.resize(start + 32 + 16, 0);
    for (offset, value) in [
        (0, 1),
        (4, 32),
        (8, kind),
        (12, 2),
        (24, 16.0f32.to_bits()),
        (28, 12.0f32.to_bits()),
    ] {
        put(&mut b, start + offset, value);
    }
    put(&mut b, start + 36, first);
    put(&mut b, start + 40, 1.0f32.to_bits());
    put(&mut b, start + 44, last);
    b
}
#[cfg_attr(test, test)]
fn gradient_colors_alpha_clipping_and_damage_are_consistent() {
    let mut b = gradient_resource(a::gui_gradient_kind_linear, 0xffff0000, 0xff0000ff);
    let paint = gradient::parse(&b[160..]).unwrap();
    assert_eq!(paint.color(0.0, 0.0), 0xffff0000);
    assert_eq!(paint.color(16.0, 12.0), 0xff0000ff);
    assert_eq!(paint.color(8.0, 6.0), 0xff800080);
    let c = command(a::gui_frame_command_kind_gradient_rounded_rect, 32, 24, &b);
    dirty(c, &b);
    let mut pixels = [0x203040; 32 * 24];
    assert!(apply(&mut pixels, 32, 24, &c, &b));
    assert_ne!(pixels[0], pixels[31]);
    b = gradient_resource(a::gui_gradient_kind_radial, 0xffff0000, 0xff0000ff);
    let paint = gradient::parse(&b[160..]).unwrap();
    assert_eq!(paint.color(0.0, 0.0), 0xffff0000);
    assert_eq!(paint.color(16.0, 0.0), 0xff0000ff);
    assert_eq!(paint.color(0.0, 12.0), 0xff0000ff);
    dirty(
        command(a::gui_frame_command_kind_gradient_rounded_rect, 32, 24, &b),
        &b,
    );
    b = gradient_resource(a::gui_gradient_kind_linear, 0xffff0000, 0x000000ff);
    assert_eq!(
        gradient::parse(&b[160..]).unwrap().color(8.0, 6.0),
        0x80ff0000
    );
}
#[cfg_attr(test, test)]
fn malformed_or_overbudget_gradients_do_not_paint() {
    let original = gradient_resource(a::gui_gradient_kind_linear, 0xffff0000, 0xff0000ff);
    let c = command(
        a::gui_frame_command_kind_gradient_rounded_rect,
        32,
        24,
        &original,
    );
    for (offset, value) in [
        (160, 0),
        (164, 0),
        (168, 99),
        (172, 1),
        (184, f32::NAN.to_bits()),
        (200, 0),
    ] {
        let mut b = original.clone();
        put(&mut b, offset, value);
        let mut pixels = [0x123456; 32 * 24];
        let mut scene = CpuScene::new(&mut pixels, 32, 24).unwrap();
        assert_eq!(
            Renderer::default().paint(
                &mut Painter::cpu(&mut scene),
                Rect::new(0, 0, 32, 24),
                &c,
                &b
            ),
            Err(Error::Invalid)
        );
        assert!(pixels.iter().all(|p| *p == 0x123456));
    }
    let mut pixels = [0x123456; 32 * 24];
    let mut scene = CpuScene::new(&mut pixels, 32, 24).unwrap();
    let mut renderer = Renderer {
        mask_limit: 32 * 24 * 5 - 1,
        ..Renderer::default()
    };
    assert_eq!(
        renderer.paint(
            &mut Painter::cpu(&mut scene),
            Rect::new(0, 0, 32, 24),
            &c,
            &original
        ),
        Err(Error::OutOfMemory)
    );
    assert!(pixels.iter().all(|p| *p == 0x123456));
}

#[cfg_attr(test, test)]
fn scratch_reuse_keeps_mixed_pixels_and_recovers_after_invalid_input() {
    let mut resources = vec![gradient_resource(
        a::gui_gradient_kind_linear,
        0x90ff0000,
        0x300000ff,
    )];
    let mut solid = rounded(1.0, 1.0, 14.0, 12.0, [5.0; 8], 0x806080c0);
    scalar!(&mut solid, border_top_bits, 1.0);
    scalar!(&mut solid, border_right_bits, 1.0);
    scalar!(&mut solid, border_bottom_bits, 1.0);
    scalar!(&mut solid, border_left_bits, 1.0);
    field!(&mut solid, stroke_argb, 0xb0abcdef);
    resources.push(solid);
    let mut shadow = rounded(4.0, 4.0, 16.0, 12.0, [4.0; 8], 0);
    field!(&mut shadow, shadow_argb, 0x90602040);
    scalar!(&mut shadow, shadow_blur_bits, 2.0);
    resources.push(shadow);
    let kinds = [
        a::gui_frame_command_kind_gradient_rounded_rect,
        a::gui_frame_command_kind_rounded_rect,
        a::gui_frame_command_kind_shadow,
    ];
    let mut shared = Renderer::default();
    let mut expected = vec![0x203040; 48 * 40];
    let mut actual = expected.clone();
    for round in 0..3 {
        for (i, resource) in resources.iter().enumerate() {
            let c = command(kinds[i], 32, 24, resource);
            for (image, renderer) in [
                (&mut expected, &mut Renderer::default()),
                (&mut actual, &mut shared),
            ] {
                let mut scene = CpuScene::new(image, 48, 40).unwrap();
                if round == 1 {
                    scene.set_paint_clip(Rect::new(8, 8, 1, 1));
                }
                assert!(
                    renderer
                        .paint(
                            &mut Painter::cpu(&mut scene),
                            Rect::new(0, 0, 48, 40),
                            &c,
                            resource
                        )
                        .unwrap()
                );
            }
            assert_eq!(expected, actual, "mixed shape {i}, round {round}");
            let mut invalid = resource.clone();
            invalid[0] = 0;
            let mut scene = CpuScene::new(&mut actual, 48, 40).unwrap();
            assert_eq!(
                shared.paint(
                    &mut Painter::cpu(&mut scene),
                    Rect::new(0, 0, 48, 40),
                    &c,
                    &invalid
                ),
                Err(Error::Invalid)
            );
            assert_eq!(expected, actual);
        }
    }
    // A legal large draw exceeds retention, not the draw admission budget.
    let mut large = vec![0; 2048 * 513];
    let mut scene = CpuScene::new(&mut large, 2048, 513).unwrap();
    let resource = rounded(0.0, 0.0, 2048.0, 513.0, [0.0; 8], 0xffffffff);
    let c = command(a::gui_frame_command_kind_rounded_rect, 2048, 513, &resource);
    assert!(
        shared
            .paint(
                &mut Painter::cpu(&mut scene),
                Rect::new(0, 0, 2048, 513),
                &c,
                &resource
            )
            .unwrap()
    );
    assert!(large.iter().all(|pixel| *pixel == 0xffffff));
    assert!(shared.pixels.capacity() <= 1024 * 1024 && shared.scratch.capacity() <= 1024 * 1024);
}
#[cfg(feature = "render-probe")]
pub(super) fn conformance_probe() {
    original_tiny_damage_bounds_scratch_independently_of_command_dimensions();
    original_rounded_golden_elliptical_corners_unequal_borders_clip_and_alpha();
    original_cubic_circle_fill_and_antialiased_stroke_golden();
    original_separate_outer_and_inset_shadow_layers_golden();
    original_one_logical_command_tiny_and_large_rounded_goldens();
    original_malformed_resources_rejected_before_replay();
    gradient_colors_alpha_clipping_and_damage_are_consistent();
    malformed_or_overbudget_gradients_do_not_paint();
    scratch_reuse_keeps_mixed_pixels_and_recovers_after_invalid_input();
}

#[test]
fn large_rounded_panels_reuse_small_corners_without_slot_exhaustion() {
    use crate::render::{
        content_hash::{Blake3, work},
        primitive_frame::Frame,
    };
    let bounds = Rect::new(0, 0, 640, 360);
    let resource = rounded(0.0, 0.0, 512.0, 256.0, [12.0; 8], 0xff4080c0);
    let draw = command(a::gui_frame_command_kind_rounded_rect, 512, 256, &resource);
    let mut pixels = vec![0x203040; 640 * 360];
    let mut scene = CpuScene::new(&mut pixels, 640, 360).unwrap();
    let mut renderer = Renderer::default();
    mask::work::reset();
    let started = std::time::Instant::now();
    renderer
        .paint(&mut Painter::cpu(&mut scene), bounds, &draw, &resource)
        .unwrap();
    let elapsed = started.elapsed().as_nanos();
    let coverage = mask::work::get();
    let mut hash = Blake3::new();
    for p in &pixels {
        hash.update(&p.to_le_bytes());
    }
    let digest = hash.finish();
    assert_eq!(
        digest,
        core::array::from_fn(|i| u8::from_str_radix(
            &"fc204ac4fa864c900818963f763cc37f1c0edcf967a8f9f9c3cb5700172be87f"[i * 2..i * 2 + 2],
            16
        )
        .unwrap())
    );
    println!(
        "{{\"schema\":\"r4os.desktop-work\",\"scope\":\"shape CPU raster\",\"elapsed_ns\":{elapsed},\"coverage_pixels\":{coverage},\"pixel_blake3\":\"{}\",\"retained_mask_capacity\":{}}}",
        digest
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<alloc::string::String>(),
        renderer.pixels.capacity()
    );
    let mut frame = Frame::new().unwrap();
    frame.set_mirror(false).unwrap();
    frame.start(1).unwrap();
    frame.begin(0, bounds).unwrap();
    mask::work::reset();
    work::reset();
    let mut accepted = 0;
    let mut failure = None;
    for n in 0..48 {
        let resource = rounded(0.0, 0.0, (512 + n) as f32, 256.0, [12.0; 8], 0xff4080c0);
        let draw = command(
            a::gui_frame_command_kind_rounded_rect,
            512 + n,
            256,
            &resource,
        );
        let mut painter = Painter::recorded(None, &mut frame, bounds, bounds);
        match renderer.paint(&mut painter, bounds, &draw, &resource) {
            Ok(true) => accepted += 1,
            Ok(false) => panic!("visible panel missing"),
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }
    let mut upload_bytes = 0u64;
    let mut upload_jobs = 0u64;
    for index in 0..crate::render::primitive_assets::TEXTURE_CAPACITY {
        let texture = frame.assets.texture(index);
        if let Some(d) = texture.dirty {
            upload_bytes += d.w as u64 * d.h as u64 * 4;
            upload_jobs += ((d.w as u64 + 511) / 512) * ((d.h as u64 + 511) / 512);
        }
    }
    let (updates, bytes) = work::get();
    let coverage = mask::work::get();
    println!(
        "{{\"schema\":\"r4os.desktop-work\",\"scope\":\"shape native CPU recording\",\"panels\":{accepted},\"coverage_pixels\":{coverage},\"hash_updates\":{updates},\"hash_bytes\":{bytes},\"planned_upload_bytes\":{upload_bytes},\"planned_512_upload_jobs\":{upload_jobs},\"commands\":{},\"asset_misses\":{},\"asset_hits\":{},\"converted_pixels\":{},\"asset_reserved_bytes\":{},\"failure\":\"{failure:?}\"}}",
        frame.commands().len(),
        frame.assets.misses,
        frame.assets.hits,
        frame.assets.converted_pixels,
        frame.assets.reserved()
    );
    assert_eq!(accepted, 48);
    assert!(failure.is_none());
    frame.end().unwrap();
}

#[test]
fn partitioned_panels_match_original_raster_for_damage_offsets_radii_and_scale() {
    let bounds = Rect::new(13, 11, 600, 320);
    for radius in [0.0, 1.0, 12.0, 64.0] {
        for (dx, dy) in [(-7, -4), (17, 9)] {
            for clip in [
                Rect::new(0, 0, 640, 360),
                Rect::new(10, 5, 400, 200),
                Rect::new(3, 6, 1, 1),
                Rect::new(440, 210, 200, 150),
            ] {
                let resource = rounded(0.0, 0.0, 512.0, 256.0, [radius; 8], 0xff4080c0);
                let mut draw = command(a::gui_frame_command_kind_rounded_rect, 512, 256, &resource);
                draw.x = dx;
                draw.y = dy;
                let mut original = vec![0x203040; 640 * 360];
                let mut pixels = original.clone();
                let mut scene = CpuScene::new(&mut original, 640, 360).unwrap();
                scene.set_paint_clip(clip);
                let mut painter = Painter::cpu(&mut scene);
                if let Some(area) = mask::area(painter.clip(), bounds, &draw, 0) {
                    Renderer::default()
                        .paint_raster(
                            &mut painter,
                            bounds,
                            &draw,
                            format::parse(&draw, &resource).unwrap(),
                            area,
                            0,
                        )
                        .unwrap();
                }
                let mut scene = CpuScene::new(&mut pixels, 640, 360).unwrap();
                scene.set_paint_clip(clip);
                Renderer::default()
                    .paint(&mut Painter::cpu(&mut scene), bounds, &draw, &resource)
                    .unwrap();
                for (i, (actual, expected)) in pixels.iter().zip(&original).enumerate() {
                    assert_eq!(
                        actual,
                        expected,
                        "r{radius} offset{dx},{dy} clip{clip:?} pixel{},{}",
                        i % 640,
                        i / 640
                    );
                }
            }
        }
    }
    // Fractional output scale keeps the original full mask and sampling grid.
    use crate::render::{output_geometry::topology::Viewport, primitive_frame::Frame};
    let bounds = Rect::new(0, 0, 640, 360);
    let resource = rounded(0.0, 0.0, 512.0, 256.0, [12.0; 8], 0xff4080c0);
    let draw = command(a::gui_frame_command_kind_rounded_rect, 512, 256, &resource);
    let mut frame = Frame::new().unwrap();
    frame.set_mirror(false).unwrap();
    frame
        .set_view(Some(Viewport {
            pixel_w: 800,
            pixel_h: 600,
            scale: 150,
            ..Default::default()
        }))
        .unwrap();
    frame.start(1).unwrap();
    frame.begin(0, bounds).unwrap();
    mask::work::reset();
    Renderer::default()
        .paint(
            &mut Painter::recorded(None, &mut frame, bounds, bounds),
            bounds,
            &draw,
            &resource,
        )
        .unwrap();
    frame.end().unwrap();
    assert_eq!(frame.commands().len(), 1);
    assert_eq!(mask::work::get(), 512 * 256);
}

#[test]
fn complex_shape_masks_keep_original_admission_and_upload_ownership() {
    use crate::render::{content_hash::work, primitive_frame::Frame};
    let bounds = Rect::new(0, 0, 640, 360);
    for case in 0..4 {
        let (kind, resource) = match case {
            0 => {
                let mut b = rounded(0.0, 0.0, 512.0, 256.0, [12.0; 8], 0x804080c0);
                field!(&mut b, stroke_argb, 0xffff0022);
                for name in [
                    core::mem::offset_of!(a::GuiShapeResource, border_top_bits),
                    core::mem::offset_of!(a::GuiShapeResource, border_right_bits),
                    core::mem::offset_of!(a::GuiShapeResource, border_bottom_bits),
                    core::mem::offset_of!(a::GuiShapeResource, border_left_bits),
                ] {
                    put(&mut b, name, 2f32.to_bits());
                }
                (a::gui_frame_command_kind_rounded_rect, b)
            }
            1 => {
                let mut b = rounded(8.0, 8.0, 496.0, 240.0, [12.0; 8], 0);
                field!(&mut b, shadow_argb, 0x804080c0);
                scalar!(&mut b, shadow_blur_bits, 4.0);
                (a::gui_frame_command_kind_shadow, b)
            }
            2 => {
                let mut b = path(0xff4080c0, 0, 0.0, false);
                segment(&mut b, a::gui_path_segment_kind_move, &[(0.0, 0.0)]);
                segment(&mut b, a::gui_path_segment_kind_line, &[(512.0, 0.0)]);
                segment(&mut b, a::gui_path_segment_kind_line, &[(512.0, 256.0)]);
                segment(&mut b, a::gui_path_segment_kind_line, &[(0.0, 256.0)]);
                segment(&mut b, a::gui_path_segment_kind_close, &[]);
                (a::gui_frame_command_kind_path_fill, b)
            }
            _ => {
                let mut b = rounded(8.0, 8.0, 496.0, 240.0, [12.0; 8], 0);
                scalar!(&mut b, shadow_blur_bits, 4.0);
                (a::gui_frame_command_kind_shadow, b)
            }
        };
        let draw = command(kind, 512, 256, &resource);
        let mut frame = Frame::new().unwrap();
        frame.set_mirror(false).unwrap();
        frame.start(1).unwrap();
        frame.begin(0, bounds).unwrap();
        let mut owner = Renderer::default();
        mask::work::reset();
        work::reset();
        assert_eq!(
            owner
                .paint(
                    &mut Painter::recorded(None, &mut frame, bounds, bounds),
                    bounds,
                    &draw,
                    &resource
                )
                .unwrap(),
            case != 3
        );
        frame.end().unwrap();
        let (updates, bytes) = work::get();
        let coverage = mask::work::get();
        let mut upload_bytes = 0u64;
        let mut jobs = 0u64;
        for n in 0..crate::render::primitive_assets::TEXTURE_CAPACITY {
            if let Some(d) = frame.assets.texture(n).dirty {
                upload_bytes += d.w as u64 * d.h as u64 * 4;
                jobs += ((d.w as u64 + 511) / 512) * ((d.h as u64 + 511) / 512);
            }
        }
        println!(
            "{{\"schema\":\"r4os.desktop-work\",\"scope\":\"shape raster fallback and transparent elision\",\"case\":{case},\"coverage_pixels\":{coverage},\"hash_updates\":{updates},\"hash_bytes\":{bytes},\"planned_upload_bytes\":{upload_bytes},\"planned_512_upload_jobs\":{jobs},\"commands\":{},\"retained_mask_bytes\":{}}}",
            frame.commands().len(),
            owner.pixels.capacity() + owner.scratch.capacity()
        );
        assert_eq!(
            coverage,
            512 * 256
                * if case == 3 {
                    0
                } else if case == 0 {
                    2
                } else {
                    1
                }
        );
        if case == 3 {
            let mut original = Renderer::default();
            let mut prior = Frame::new().unwrap();
            prior.set_mirror(false).unwrap();
            prior.start(1).unwrap();
            prior.begin(0, bounds).unwrap();
            let mut painter = Painter::recorded(None, &mut prior, bounds, bounds);
            let parsed = format::parse(&draw, &resource).unwrap();
            let area = mask::area(bounds, bounds, &draw, 4).unwrap();
            mask::work::reset();
            assert!(
                !original
                    .paint_raster(&mut painter, bounds, &draw, parsed, area, 4)
                    .unwrap()
            );
            prior.end().unwrap();
            assert_eq!(mask::work::get(), 512 * 256);
            assert_eq!(prior.commands().len(), 0);
            assert_eq!(owner.pixels.capacity() + owner.scratch.capacity(), 0);
            println!(
                "{{\"schema\":\"r4os.desktop-work\",\"scope\":\"original transparent shadow raster reference\",\"coverage_pixels\":{},\"mask_capacity\":{}}}",
                mask::work::get(),
                original.pixels.capacity() + original.scratch.capacity()
            );
        }
        assert_eq!(
            frame.commands().len(),
            if case == 3 {
                0
            } else if case == 0 {
                2
            } else {
                1
            }
        );
    }
}

#[test]
fn cpu_panel_borders_and_horizontal_gradient_match_coverage_over_patterned_pixels() {
    let bounds = Rect::new(4, 3, 80, 52);
    for (w, h, radius, border) in [(70, 42, 8.0, 1.0), (29, 25, 2.0, 4.0), (61, 39, 0.0, 0.0)] {
        for (kind, fill) in [
            (a::gui_frame_command_kind_rounded_rect, 0xff102030),
            (a::gui_frame_command_kind_rounded_rect, 0x00102030),
            (a::gui_frame_command_kind_gradient_rounded_rect, 0),
        ] {
            let mut resource = rounded(0.0, 0.0, w as f32, h as f32, [radius; 8], fill);
            field!(&mut resource, stroke_argb, 0xff746da3);
            scalar!(&mut resource, border_top_bits, border);
            scalar!(&mut resource, border_left_bits, border);
            scalar!(&mut resource, border_bottom_bits, border);
            scalar!(&mut resource, border_right_bits, border);
            if kind == a::gui_frame_command_kind_gradient_rounded_rect {
                let mut gradient = gradient_resource(a::gui_gradient_kind_linear, 0xff194871, 0xffce073b);
                let offset = core::mem::size_of::<a::GuiShapeResource>();
                put(&mut gradient, offset + 28, 0);
                resource.extend_from_slice(&gradient[offset..]);
            }
            let mut draw = command(kind, w, h, &resource);
            draw.x = -3;
            draw.y = 2;
            for clip in [bounds, Rect::new(6, 8, 24, 26), Rect::new(43, 5, 30, 34)] {
                let pattern: Vec<u32> = (0..90*60u32).map(|v| v * 239 & 0xffffff).collect();
                let mut fast = pattern.clone();
                let mut expected = pattern;
                for (pixels, reference) in [(&mut expected, true), (&mut fast, false)] {
                    let mut scene = CpuScene::new(pixels, 90, 60).unwrap();
                    scene.set_paint_clip(clip);
                    let mut painter = Painter::cpu(&mut scene);
                    let mut renderer = Renderer::default();
                    if reference {
                        if let Some(area) = mask::area(painter.clip(), bounds, &draw, 0) {
                            renderer.paint_raster(&mut painter, bounds, &draw,
                                format::parse(&draw, &resource).unwrap(), area, 0).unwrap();
                        }
                    } else {
                        renderer.paint(&mut painter, bounds, &draw, &resource).unwrap();
                    }
                }
                assert_eq!(fast, expected, "kind {kind}, fill {fill}, border {border}, clip {clip:?}");
            }
        }
    }
}
