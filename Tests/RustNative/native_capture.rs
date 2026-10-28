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
#[derive(Clone, Copy, PartialEq, Eq)]
enum AssetChange {
    None,
    Append,
    Overwrite,
}
fn captured(
    cache: &mut Cache,
    frame: &mut Frame,
    engine: &mut Engine,
    mode: Mode,
    damage: Rect,
    right: Rect,
    asset_change: AssetChange,
    paints: &Cell<u32>,
) {
    recorded(
        cache,
        frame,
        engine,
        mode,
        damage,
        right,
        asset_change,
        paints,
        None,
    )
}
fn recorded(
    cache: &mut Cache,
    frame: &mut Frame,
    engine: &mut Engine,
    mode: Mode,
    damage: impl Into<Regions>,
    right: Rect,
    asset_change: AssetChange,
    paints: &Cell<u32>,
    mut replay: Option<&mut RecordingReplay>,
) {
    let damage = damage.into();
    let overwritten = if asset_change == AssetChange::Overwrite {
        assert!(!engine.active());
        let old = *frame
            .commands()
            .iter()
            .find(|c| c.texture.is_some())
            .unwrap();
        // Fill the real original entry table. On the next frame intern must
        // evict this old page and actually overwrite its first source tile.
        assert_eq!(frame.assets.evictions, 0);
        for n in frame.assets.misses as usize..crate::render::primitive_assets::ENTRY_CAPACITY {
            let pixel = [0xff200000 | n as u32];
            frame
                .assets
                .intern(View::new(
                    Format::Argb,
                    1,
                    1,
                    4,
                    PixelBytes::from_words(&pixel).bytes(),
                ))
                .unwrap();
        }
        assert_eq!(
            frame.assets.misses,
            crate::render::primitive_assets::ENTRY_CAPACITY as u64
        );
        Some(old)
    } else {
        None
    };
    engine.clear_sources().unwrap();
    if let Some(replay) = replay.as_deref_mut() {
        replay.save(cache, frame);
    }
    frame.set_mirror(false).unwrap();
    cache.start(mode, Some(frame)).unwrap();
    if asset_change != AssetChange::None {
        // Real atlas insertion changes an existing page's generation. Do not
        // edit an expected generation or the engine's reuse mask in the test.
        let pixel = [if asset_change == AssetChange::Append {
            0xffff0000
        } else {
            0xff0000ff
        }];
        let entry = frame
            .assets
            .intern(View::new(
                Format::Argb,
                1,
                1,
                4,
                PixelBytes::from_words(&pixel).bytes(),
            ))
            .unwrap();
        if let Some(old) = overwritten {
            assert_eq!(
                (entry.texture, entry.rect),
                (old.texture.unwrap(), old.source)
            );
            assert!(
                !frame
                    .assets
                    .contains(old.content, entry.texture, old.source)
            );
            assert_ne!(entry.content, old.content);
            assert!(frame.assets.evictions > 0);
        }
    }
    if matches!(mode, Mode::NativeOutput(_)) {
        if let Some(replay) = replay.as_deref_mut() {
            replay.prepare(cache, frame, Some(damage)).unwrap();
        } else {
            engine.reuse_capture(cache, frame, Some(damage)).unwrap();
        }
    }
    let mut reads = Reads::default();
    let mut windows = Set::new(NoExternal);
    let mut capture = Capture {
        cache,
        frame,
        engine,
        reads: &mut reads,
        windows: &mut windows,
        recording_replay: replay.as_deref(),
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
fn cpu_scene_replay_keeps_unchanged_tiles_without_inventing_gpu_residency() {
    gpu::with(|app, device| {
        let mut cache = Cache::new(1024 * 1024).unwrap();
        let mut frame = Frame::new().unwrap();
        let mut recorder = Engine::new().unwrap();
        let mut replay = RecordingReplay::new().unwrap();
        let paints = Cell::new(0);
        let view = super::super::output_geometry::topology::Viewport {
            pixel_w: 8,
            pixel_h: 8,
            ..Default::default()
        };
        for (damage, change, expected) in [
            (FULL, AssetChange::None, 1),
            (Rect::new(0, 0, 1, 1), AssetChange::Append, 1),
            (RIGHT, AssetChange::None, 2),
            (Rect::new(0, 0, 1, 1), AssetChange::Overwrite, 3),
        ] {
            recorded(
                &mut cache,
                &mut frame,
                &mut recorder,
                Mode::NativeOutput(view),
                damage,
                RIGHT,
                change,
                &paints,
                Some(&mut replay),
            );
            assert_eq!(paints.get(), expected);
            assert_eq!(gpu::stats()[0], 0, "CPU replay never submits GPU work");
            assert_eq!(
                recorder.atlas_reused_layers, 0,
                "no completed GPU reuse receipt exists"
            );
            assert!(cache.complete_scene && cache.captured());
        }
        assert!(cache.reused_captures != 0);
        recorded(
            &mut cache,
            &mut frame,
            &mut recorder,
            Mode::NativeOutput(view),
            Rect::new(0, 0, 1, 1),
            RIGHT,
            AssetChange::None,
            &paints,
            Some(&mut replay),
        );
        assert_eq!(paints.get(), 3);
        let reused = cache.reused_captures;
        recorded(
            &mut cache,
            &mut frame,
            &mut recorder,
            Mode::NativeOutput(view),
            Regions::from_slice(&[Rect::new(0, 0, 1, 1), Rect::new(7, 7, 1, 1)]),
            RIGHT,
            AssetChange::None,
            &paints,
            Some(&mut replay),
        );
        assert_eq!(
            paints.get(),
            3,
            "an unchanged layer between two damaged corners stays reusable"
        );
        assert_eq!(cache.reused_captures, reused + 1);
        recorded(
            &mut cache,
            &mut frame,
            &mut recorder,
            Mode::Desktop(FULL),
            FULL,
            RIGHT,
            AssetChange::None,
            &paints,
            Some(&mut replay),
        );
        assert_eq!(
            paints.get(),
            4,
            "a previous CPU native replay mask cannot cross Desktop mode"
        );
        let mut transaction = Engine::new().unwrap();
        transaction
            .adopt_recording(&mut recorder, Some(FULL.into()))
            .unwrap();
        gpu::begin(app, device, &mut transaction, &cache, Some(&frame));
        gpu::pump(
            app,
            device,
            &mut transaction,
            &mut cache,
            Some(&mut frame),
            Progress::Copied,
        );
        let mut expected = [0x203040; 64];
        for y in RIGHT.y..RIGHT.bottom() {
            for x in RIGHT.x..RIGHT.right() {
                expected[(y * 8 + x) as usize] = 0xffffff;
            }
        }
        assert_eq!(gpu::visible(), expected);
        let session = device.bind(app).unwrap();
        transaction.close(&session).unwrap();
    });
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
            (0, FULL, RIGHT, AssetChange::None, 1),
            (1, Rect::new(0, 0, 1, 1), RIGHT, AssetChange::None, 1),
            (2, RIGHT, RIGHT, AssetChange::None, 2),
            (3, FULL, Rect::new(3, 2, 2, 4), AssetChange::None, 3),
            (
                4,
                Rect::new(0, 0, 1, 1),
                Rect::new(3, 2, 2, 4),
                AssetChange::Append,
                3,
            ),
            (
                5,
                Rect::new(0, 0, 1, 1),
                Rect::new(3, 2, 2, 4),
                AssetChange::None,
                3,
            ),
            (
                6,
                Rect::new(0, 0, 1, 1),
                Rect::new(3, 2, 2, 4),
                AssetChange::Overwrite,
                4,
            ),
        ] {
            let reused = cache.reused_captures;
            let draws = engine.primitive_draws;
            let uploads = engine.jobs.uploaded_bytes;
            let atlas_reuses = engine.atlas_reused_layers;
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
            if round == 4 {
                assert!(cache.reused_captures > reused);
                assert_eq!(
                    engine.primitive_draws, draws,
                    "foreign tile does not reraster unchanged layers"
                );
                assert_eq!(
                    engine.jobs.uploaded_bytes - uploads,
                    4,
                    "real dirty tile upload remains required"
                );
                assert!(
                    engine.atlas_reused_layers > atlas_reuses,
                    "receipt follows successful GPU completion"
                );
            }
            if round == 6 {
                assert!(
                    engine.primitive_draws > draws,
                    "real tile overwrite/page reset must reraster"
                );
            }
        }
        captured(
            &mut cache,
            &mut frame,
            &mut engine,
            Mode::Desktop(FULL),
            Rect::new(0, 0, 1, 1),
            Rect::new(3, 2, 2, 4),
            AssetChange::None,
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
            AssetChange::None,
            &paints,
        );
        assert_eq!(paints.get(), 6, "cancel invalidates capture reuse");
        let session = device.bind(app).unwrap();
        engine.close(&session).unwrap();
    });
}
