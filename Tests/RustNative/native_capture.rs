//! The actual native Capture adapter with the existing independent GPU model.
//! A painter invocation witnesses CPU work, not only skipped GPU submissions.
use super::*;
use crate::render::{
    compositor::Capture as _,
    gpu::engine::{Progress, tests as gpu},
    pixels::PixelBytes,
    primitive_image::{Format, Picture, View},
};
use core::cell::Cell;
use r4os_contract as a;

#[derive(Clone)]
struct NoExternal;
unsafe impl Memory for NoExternal {
    fn import(&self, _: &a::GfxBufferHandle, _: &mut a::GfxBufferReference) -> i32 {
        panic!("no external image in this capture case")
    }
    fn describe(&self, _: &a::GfxBufferHandle, _: &mut a::GfxBufferDescriptor) -> i32 {
        panic!("no external image in this capture case")
    }
    fn release(&self, _: &a::GfxBufferHandle) -> i32 {
        panic!("no external image in this capture case")
    }
    fn map(&self, _: &a::GfxBufferHandle, _: u64, _: &mut a::GfxBufferMap) -> i32 {
        panic!("no external image in this capture case")
    }
    fn unmap(&self, _: &a::GfxBufferHandle) -> i32 {
        panic!("no external image in this capture case")
    }
    fn query(&self, _: &a::GfxFence, _: &mut a::GfxFenceStatus) -> i32 {
        panic!("no external image in this capture case")
    }
}
const FULL: Rect = Rect::new(0, 0, 8, 8);
const RIGHT: Rect = Rect::new(4, 2, 2, 4);
fn captured(
    cache: &mut Cache,
    frame: &mut Frame,
    engine: &mut Engine,
    mode: Mode,
    damage: Rect,
    right: Rect,
    invalidate_asset: bool,
    paints: &Cell<u32>,
) {
    engine.clear_sources().unwrap();
    frame.set_mirror(false).unwrap();
    cache.start(mode, Some(frame)).unwrap();
    if invalidate_asset {
        // Real atlas insertion changes an existing page's generation. Do not
        // edit an expected generation or the engine's reuse mask in the test.
        let red = [0xffff0000];
        frame
            .assets
            .intern(View::new(
                Format::Argb,
                1,
                1,
                4,
                PixelBytes::from_words(&red).bytes(),
            ))
            .unwrap();
    }
    if matches!(mode, Mode::NativeOutput(_)) {
        engine.reuse_capture(cache, frame, Some(damage)).unwrap();
    }
    let mut reads = Reads::default();
    let mut windows = Set::new(NoExternal);
    let mut capture = Capture {
        cache,
        frame,
        engine,
        reads: &mut reads,
        windows: &mut windows,
        finished: false,
    };
    capture
        .paint(1, FULL, FULL, |p| p.fill(FULL, 0x203040))
        .unwrap();
    capture
        .paint(2, right, FULL, |p| {
            paints.set(paints.get() + 1);
            let white = [0xffffffff];
            p.picture(Picture {
                view: View::new(
                    Format::Argb,
                    1,
                    1,
                    4,
                    PixelBytes::from_words(&white).bytes(),
                ),
                viewport: right,
                clip: FULL,
                source_x: 0,
                source_y: 0,
                guest_w: 1,
                guest_h: 1,
            })
        })
        .unwrap();
    capture.cache.finish(Some(capture.frame)).unwrap();
    capture.cache.complete_scene = true;
    if let Mode::NativeOutput(view) = mode {
        capture
            .engine
            .composition_damage(view, Some(damage))
            .unwrap();
    }
    capture.finished = true;
}

#[test]
fn productive_adapter_reuses_cpu_capture_and_rejects_damage_geometry_asset_and_cancel() {
    gpu::with(|app, device| {
        let mut cache = Cache::new(1024 * 1024).unwrap();
        let mut frame = Frame::new().unwrap();
        let mut engine = Engine::new().unwrap();
        let view = super::super::output_geometry::topology::Viewport {
            pixel_w: 8,
            pixel_h: 8,
            ..Default::default()
        };
        let mode = Mode::NativeOutput(view);
        let paints = Cell::new(0);
        for (round, damage, bounds, asset, expected_paints) in [
            (0, FULL, RIGHT, false, 1),
            (1, Rect::new(0, 0, 1, 1), RIGHT, false, 1),
            (2, RIGHT, RIGHT, false, 2),
            (3, FULL, Rect::new(3, 2, 2, 4), false, 3),
            (4, Rect::new(0, 0, 1, 1), Rect::new(3, 2, 2, 4), true, 4),
            (5, Rect::new(0, 0, 1, 1), Rect::new(3, 2, 2, 4), false, 4),
        ] {
            let reused = cache.reused_captures;
            captured(
                &mut cache,
                &mut frame,
                &mut engine,
                mode,
                damage,
                bounds,
                asset,
                &paints,
            );
            assert_eq!(paints.get(), expected_paints, "round{round}");
            if round == 1 {
                assert!(cache.reused_captures > reused);
            }
            gpu::begin(app, device, &mut engine, &cache, Some(&frame));
            gpu::pump(
                app,
                device,
                &mut engine,
                &mut cache,
                Some(&mut frame),
                Progress::Copied,
            );
            let mut expected = [0x203040; 64];
            for y in bounds.y..bounds.bottom() {
                for x in bounds.x..bounds.right() {
                    expected[(y * 8 + x) as usize] = 0xffffff;
                }
            }
            assert_eq!(gpu::visible(), expected, "round{round} original pixels");
        }
        captured(
            &mut cache,
            &mut frame,
            &mut engine,
            Mode::Desktop(FULL),
            Rect::new(0, 0, 1, 1),
            Rect::new(3, 2, 2, 4),
            false,
            &paints,
        );
        assert_eq!(paints.get(), 5, "old native mask cannot cross CPU mode");
        engine.invalidate();
        cache.cancel();
        captured(
            &mut cache,
            &mut frame,
            &mut engine,
            mode,
            FULL,
            RIGHT,
            false,
            &paints,
        );
        assert_eq!(paints.get(), 6, "cancel invalidates capture reuse");
        let session = device.bind(app).unwrap();
        engine.close(&session).unwrap();
    });
}
