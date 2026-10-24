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
        Renderer { mask_limit: 2 }
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
#[cfg(feature = "render-probe")]
pub(super) fn conformance_probe() {
    original_tiny_damage_bounds_scratch_independently_of_command_dimensions();
    original_rounded_golden_elliptical_corners_unequal_borders_clip_and_alpha();
    original_cubic_circle_fill_and_antialiased_stroke_golden();
    original_separate_outer_and_inset_shadow_layers_golden();
    original_one_logical_command_tiny_and_large_rounded_goldens();
    original_malformed_resources_rejected_before_replay();
}
