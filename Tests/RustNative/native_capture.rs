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
        rasterize: false,
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
fn local_native_capture_retains_complete_maximized_layer_and_skips_unaffected_picture_work() {
    let full = Rect::new(0, 0, 1280, 720);
    let local = Rect::new(5, 5, 1, 1);
    let view = super::super::output_geometry::topology::Viewport {
        pixel_w: 1280,
        pixel_h: 720,
        ..Default::default()
    };
    let mut cache = Cache::new(16 * 1024 * 1024).unwrap();
    let mut frame = Frame::new().unwrap();
    let mut engine = Engine::new().unwrap();
    let mut replay = RecordingReplay::new().unwrap();
    let mut full_rebuilds = 0;
    let damage_sequence = [full, full]
        .into_iter()
        .chain((0..180).map(|n| Rect::new((n % 60) * 20 + local.x, (n / 60) * 24 + local.y, 1, 1)));
    for (sample, damage) in damage_sequence.enumerate() {
        replay.save(&cache, &mut frame);
        frame.set_mirror(false).unwrap();
        cache
            .start(Mode::NativeOutput(view), Some(&mut frame))
            .unwrap();
        replay
            .prepare(&cache, &mut frame, Some(damage.into()))
            .unwrap();
        let original_interns = frame.assets.hits + frame.assets.misses;
        let calls = Cell::new(0);
        let clip = Cell::new(Rect::ZERO);
        let mut reads = Reads::default();
        let mut windows = Set::new(NoExternal);
        let start = std::time::Instant::now();
        let mut capture = Capture {
            cache: &mut cache,
            frame: &mut frame,
            engine: &mut engine,
            reads: &mut reads,
            windows: &mut windows,
            recording_replay: Some(&replay),
            rasterize: false,
            finished: false,
        };
        capture
            .paint(1, full, full, |p| p.fill(full, 0x203040))
            .unwrap();
        capture
            .paint(2, full, full, |p| {
                clip.set(p.clip());
                for y in 0..20 {
                    for x in 0..60 {
                        let rect = Rect::new(x * 20, y * 24, 16, 16);
                        if !p.visible(rect) {
                            continue;
                        }
                        calls.set(calls.get() + 1);
                        let white = [0xffffffffu32];
                        p.picture(Picture {
                            view: View::new(
                                Format::Argb,
                                1,
                                1,
                                4,
                                PixelBytes::from_words(&white).bytes(),
                            ),
                            viewport: rect,
                            clip: full,
                            source_x: 0,
                            source_y: 0,
                            guest_w: 1,
                            guest_h: 1,
                        })?;
                    }
                }
                for n in 0..sample.saturating_sub(1) {
                    p.fill(
                        Rect::new(
                            (n % 60) as i32 * 20 + local.x,
                            (n / 60) as i32 * 24 + local.y,
                            1,
                            1,
                        ),
                        0x00ff00,
                    )?;
                }
                Ok(())
            })
            .unwrap();
        capture.cache.finish(Some(capture.frame)).unwrap();
        capture.cache.complete_scene = true;
        capture.finished = true;
        let elapsed = start.elapsed().as_nanos();
        // Independent whole-scene oracle: these source images are solid
        // white. Replay the retained local scissors into two separate layers,
        // then compose their opaque pixels over the original solid backdrop.
        if sample < 3 || sample == 181 {
            let mut layers = [alloc::vec![0u32; 1280 * 720], alloc::vec![0u32; 1280 * 720]];
            for command in capture.frame.commands() {
                let color = if command.texture.is_some() {
                    0xffffffff
                } else {
                    command.color
                };
                let clip =
                    crate::render::scene::intersect(command.target, command.scissor).unwrap();
                for y in clip.y..clip.bottom() {
                    let first = (y * 1280 + clip.x) as usize;
                    layers[command.layer as usize][first..first + clip.w as usize].fill(color);
                }
            }
            for y in 0..720usize {
                for x in 0..1280usize {
                    let i = y * 1280 + x;
                    let actual = if layers[1][i] >> 24 == 255 {
                        layers[1][i]
                    } else {
                        layers[0][i]
                    };
                    let changed_pixel = x >= 5
                        && y >= 5
                        && (x - 5) % 20 == 0
                        && (y - 5) % 24 == 0
                        && (x - 5) / 20 < 60
                        && (y - 5) / 24 * 60 + (x - 5) / 20 < sample.saturating_sub(1);
                    let expected = if changed_pixel {
                        0xff00ff00
                    } else if x < 1200 && x % 20 < 16 && y < 480 && y % 24 < 16 {
                        0xffffffff
                    } else {
                        0xff203040
                    };
                    assert_eq!(
                        actual, expected,
                        "complete scene sample{sample}, pixel{x},{y}"
                    );
                }
            }
        }
        let interns = capture.frame.assets.hits + capture.frame.assets.misses - original_interns;
        if sample < 3 || clip.get() == full || sample == 181 {
            println!(
                "{{\"schema\":\"r4os.desktop-work\",\"scope\":\"native CPU recording; no GPU submission\",\"sample\":{sample},\"capture_ns\":{elapsed},\"picture_calls\":{},\"asset_interns\":{interns},\"scene_commands\":{}}}",
                calls.get(),
                capture.frame.commands().len()
            );
        }
        if sample >= 2 && clip.get() != full {
            assert_eq!(clip.get(), damage);
            assert_eq!(calls.get(), 1);
            assert_eq!(interns, 1);
            assert!(
                capture.frame.commands().len() >= 1200,
                "complete immutable scene remains available"
            );
        } else {
            assert_eq!(calls.get(), 1200);
            assert_eq!(interns, 1200);
            if sample >= 2 {
                full_rebuilds += 1;
            }
        }
        assert!(capture.frame.commands().len() < crate::render::primitive_frame::CAPACITY);
    }
    assert!(
        full_rebuilds > 0,
        "disjoint updates must compact retained fragments with a full native rebuild"
    );
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
fn local_lower_update_keeps_unchanged_upper_alpha_and_complete_outside_pixels() {
    gpu::with(|app, device| {
        let mut cache = Cache::new(1024 * 1024).unwrap();
        let mut frame = Frame::new().unwrap();
        let mut engine = Engine::new().unwrap();
        let view = super::super::output_geometry::topology::Viewport {
            pixel_w: 8,
            pixel_h: 8,
            ..Default::default()
        };
        let lower = Rect::new(2, 2, 4, 4);
        let upper = Rect::new(3, 3, 2, 2);
        let pixel = Rect::new(3, 3, 1, 1);
        for (round, updated) in [false, true, true].into_iter().enumerate() {
            let draws = engine.primitive_draws;
            let damage = if updated { pixel } else { FULL };
            engine.clear_sources().unwrap();
            frame.set_mirror(false).unwrap();
            cache
                .start(Mode::NativeOutput(view), Some(&mut frame))
                .unwrap();
            engine
                .reuse_capture(&cache, &mut frame, Some(damage.into()))
                .unwrap();
            let mut reads = Reads::default();
            let mut windows = Set::new(NoExternal);
            let mut capture = Capture {
                cache: &mut cache,
                frame: &mut frame,
                engine: &mut engine,
                reads: &mut reads,
                windows: &mut windows,
                recording_replay: None,
                rasterize: false,
                finished: false,
            };
            capture
                .paint(1, FULL, FULL, |p| p.fill(FULL, 0x203040))
                .unwrap();
            capture
                .paint(2, lower, FULL, |p| {
                    let red = [0xffff0000u32];
                    p.picture(Picture {
                        view: View::new(
                            Format::Argb,
                            1,
                            1,
                            4,
                            PixelBytes::from_words(&red).bytes(),
                        ),
                        viewport: lower,
                        clip: FULL,
                        source_x: 0,
                        source_y: 0,
                        guest_w: 1,
                        guest_h: 1,
                    })?;
                    if updated {
                        p.fill(pixel, 0x00ff00)?;
                    }
                    Ok(())
                })
                .unwrap();
            capture
                .paint(3, upper, FULL, |p| {
                    let white = [0x80ffffffu32];
                    p.picture(Picture {
                        view: View::new(
                            Format::Argb,
                            1,
                            1,
                            4,
                            PixelBytes::from_words(&white).bytes(),
                        ),
                        viewport: upper,
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
            capture
                .engine
                .composition_damage(view, Some(damage.into()))
                .unwrap();
            capture.finished = true;
            drop(capture);
            gpu::begin(app, device, &mut engine, &cache, Some(&frame));
            gpu::pump(
                app,
                device,
                &mut engine,
                &mut cache,
                Some(&mut frame),
                Progress::Copied,
            );
            if round == 2 {
                assert_eq!(
                    engine.primitive_draws, draws,
                    "identical local content keeps prior GPU recordings"
                );
            }
            let mut expected = [0x203040; 64];
            for y in 2..6 {
                for x in 2..6 {
                    expected[y * 8 + x] = if updated && x == 3 && y == 3 {
                        0x00ff00
                    } else {
                        0xff0000
                    };
                }
            }
            for y in 3..5 {
                for x in 3..5 {
                    expected[y * 8 + x] = if updated && x == 3 && y == 3 {
                        0xbcffbc
                    } else {
                        0xffbcbc
                    };
                }
            }
            assert_eq!(
                gpu::visible(),
                expected,
                "updated{updated}: whole scene including upper alpha"
            );
        }
        let session = device.bind(app).unwrap();
        engine.close(&session).unwrap();
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

#[test]
fn capacity_fallback_keeps_exact_premultiplied_channels_and_native_output_recovers() {
    for alpha in 0..=255u32 {
        for channel in 0..=alpha {
            let premultiplied = (alpha << 24) | channel * 0x010101;
            let straight = [straight_alpha(premultiplied).unwrap()];
            let view = View::new(
                Format::Argb,
                1,
                1,
                4,
                PixelBytes::from_words(&straight).bytes(),
            );
            assert_eq!(view.pixel(0, 0), premultiplied);
        }
    }
    assert_eq!(straight_alpha(1), Err(Error::Invalid));
    assert_eq!(straight_alpha(0x01020000), Err(Error::Invalid));
    gpu::with(|app, device| {
        let mut cache = Cache::new(1024 * 1024).unwrap();
        let mut frame = Frame::new().unwrap();
        let mut engine = Engine::new().unwrap();
        let mut reads = Reads::default();
        let mut windows = Set::new(NoExternal);
        let view = super::super::output_geometry::topology::Viewport {
            pixel_w: 8,
            pixel_h: 8,
            ..Default::default()
        };
        for count in [9000, 3] {
            let paint = |p: &mut Painter<'_, '_>, first| {
                for n in first..count {
                    p.fill(Rect::new(n % 2, 0, 2, 2), n as u32 * 77)?;
                }
                let argb = [0x80ff4000];
                p.picture(Picture {
                    view: View::new(Format::Argb, 1, 1, 4, PixelBytes::from_words(&argb).bytes()),
                    viewport: Rect::new(4, 2, 2, 2),
                    clip: FULL,
                    source_x: 0,
                    source_y: 0,
                    guest_w: 1,
                    guest_h: 1,
                })
            };
            // The last two opaque overlapping writes determine these pixels.
            // Keep the independent original GPU/CMM oracle for the transparent
            // layer: blending display RGB bytes directly would skip its linear
            // color conversion and is not the output composition contract.
            let mut ref_cache = Cache::new(1024 * 1024).unwrap();
            let mut ref_frame = Frame::new().unwrap();
            let mut reference = Engine::new().unwrap();
            reference.destination = crate::render::gpu::engine::Destination::Readback;
            ref_frame.set_mirror(false).unwrap();
            ref_cache
                .start(Mode::NativeOutput(view), Some(&mut ref_frame))
                .unwrap();
            ref_cache
                .paint(1, FULL, FULL, Some(&mut ref_frame), None, |p| {
                    p.fill(FULL, 0x203040)
                })
                .unwrap();
            ref_cache
                .paint(2, FULL, FULL, Some(&mut ref_frame), None, |p| {
                    paint(p, count - 2)
                })
                .unwrap();
            ref_cache.finish(Some(&ref_frame)).unwrap();
            ref_cache.complete_scene = true;
            reference
                .composition_damage(view, Some(FULL.into()))
                .unwrap();
            gpu::begin(app, device, &mut reference, &ref_cache, Some(&ref_frame));
            gpu::pump(
                app,
                device,
                &mut reference,
                &mut ref_cache,
                Some(&mut ref_frame),
                Progress::Copied,
            );
            let expected = gpu::pixels(app, device, &reference).map(|pixel| pixel & 0xffffff);
            reference.close(&device.bind(app).unwrap()).unwrap();
            let mut attempts = Vec::new();
            let submissions = gpu::stats()[0];
            with_capacity_fallback(
                &mut cache,
                &mut frame,
                &mut engine,
                &mut reads,
                &mut windows,
                |cache, frame, engine, reads, windows, rasterize| {
                    attempts.push(rasterize);
                    assert_eq!(
                        gpu::stats()[0],
                        submissions,
                        "failed recording never submitted work"
                    );
                    frame.set_mirror(false)?;
                    cache.start(Mode::NativeOutput(view), Some(frame))?;
                    let mut capture = Capture {
                        cache,
                        frame,
                        engine,
                        reads,
                        windows,
                        recording_replay: None,
                        rasterize,
                        finished: false,
                    };
                    capture.paint(1, FULL, FULL, |p| p.fill(FULL, 0x203040))?;
                    capture.paint(2, FULL, FULL, |p| paint(p, 0))?;
                    capture.cache.finish(Some(capture.frame))?;
                    capture.cache.complete_scene = true;
                    capture
                        .engine
                        .composition_damage(view, Some(FULL.into()))
                        .unwrap();
                    capture.finished = true;
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(
                attempts.as_slice(),
                if count == 9000 {
                    &[false, true][..]
                } else {
                    &[false][..]
                }
            );
            assert_eq!(
                cache.reserved(),
                0,
                "fallback scratch leaves no retained layer pixels"
            );
            assert!(frame.commands().len() < 8);
            gpu::begin(app, device, &mut engine, &cache, Some(&frame));
            gpu::pump(
                app,
                device,
                &mut engine,
                &mut cache,
                Some(&mut frame),
                Progress::Copied,
            );
            assert_eq!(gpu::visible(), expected);
            assert!(reads.empty() && engine.fault.is_none());
        }
        for error in [Error::Graphics, Error::Capacity] {
            let mut attempts = 0;
            let result = with_capacity_fallback(
                &mut cache,
                &mut frame,
                &mut engine,
                &mut reads,
                &mut windows,
                |_, _, _, _, _, _| {
                    attempts += 1;
                    Err::<(), _>(error)
                },
            );
            assert_eq!(result, Err(error));
            assert_eq!(attempts, if error == Error::Capacity { 2 } else { 1 });
        }
        engine.close(&device.bind(app).unwrap()).unwrap();
    });
}

#[test]
fn solid_shell_backdrop_keeps_original_pixels_and_revalidates_replay_dependencies() {
    use super::super::output_geometry::{
        self as geometry,
        topology::{Rotation, Viewport},
    };
    fn scene(
        cache: &mut Cache,
        frame: &mut Frame,
        engine: &mut Engine,
        replay: &mut RecordingReplay,
        view: Viewport,
        turn: usize,
        optimized: bool,
        paints: &Cell<u32>,
    ) {
        engine.clear_sources().unwrap();
        replay.save(cache, frame);
        frame.set_mirror(false).unwrap();
        cache.start(Mode::NativeOutput(view), Some(frame)).unwrap();
        let bounds = geometry::logical(view).unwrap();
        // At turns2/3 an old CPU hint is deliberately outside the icon.
        // New background color or overlap must independently revoke its seed.
        let hint = if matches!(turn, 1 | 2 | 3) {
            Rect::new(0, 0, 1, 1)
        } else {
            bounds
        };
        replay.prepare(cache, frame, Some(hint.into())).unwrap();
        let mut reads = Reads::default();
        let mut windows = Set::new(NoExternal);
        let mut c = Capture {
            cache,
            frame,
            engine,
            reads: &mut reads,
            windows: &mut windows,
            recording_replay: Some(replay),
            rasterize: false,
            finished: false,
        };
        let background = if turn >= 2 { 0x7821af } else { 0x123456 };
        c.paint(1, bounds, bounds, |p| {
            p.fill(bounds, background)?;
            if turn == 8 {
                p.fill(Rect::new(1, 1, 2, 2), 0x40a077)?;
            }
            Ok(())
        })
        .unwrap();
        let overlay = Rect::new(if turn == 7 { 0 } else { 1 }, 1, bounds.w - 2, bounds.h - 2);
        if turn == 3 {
            let under = Rect::new(overlay.x, overlay.y, 1, 1);
            c.paint(9, under, bounds, |p| p.fill(under, 0x40a077))
                .unwrap();
        }
        let paint = |p: &mut Painter<'_, '_>| {
            paints.set(paints.get() + 1);
            // Same opaque ICO runs and actual foreground/background glyph
            // format used by the shell, with genuinely transparent gaps.
            p.fill(Rect::new(overlay.x, overlay.y + 1, 1, 1), 0xfedcba)?;
            let rows = [1u64, 2];
            let mut glyph = View::new(Format::Glyph, 2, 2, 0, &[]);
            glyph.rows = &rows;
            glyph.foreground = 0xffffff;
            glyph.background = 0x225599;
            glyph.identity.revision = 1;
            let cell = Rect::new(overlay.x + 1, overlay.y, 2, 2);
            p.picture(Picture {
                view: glyph,
                viewport: cell,
                clip: overlay,
                source_x: 0,
                source_y: 0,
                guest_w: 2,
                guest_h: 2,
            })?;
            if matches!(turn, 4 | 5) {
                let white = [if turn == 4 { 0x80ffffff } else { 0u32 }];
                let pixel = Rect::new(overlay.x, overlay.bottom() - 1, 1, 1);
                p.picture(Picture {
                    view: View::new(
                        Format::Argb,
                        1,
                        1,
                        4,
                        PixelBytes::from_words(&white).bytes(),
                    ),
                    viewport: pixel,
                    clip: pixel,
                    source_x: 0,
                    source_y: 0,
                    guest_w: 1,
                    guest_h: 1,
                })?;
            }
            if turn == 6 {
                p.clear(Rect::new(overlay.x + 1, overlay.y, 1, 1))?;
            }
            Ok(())
        };
        let before = paints.get();
        if optimized {
            c.paint_over_solid(32, overlay, bounds, background, paint)
                .unwrap();
        } else {
            c.paint(32, overlay, bounds, paint).unwrap();
        }
        if turn == 1 {
            assert_eq!(paints.get(), before, "unchanged shell paint really reused");
        }
        if matches!(turn, 2 | 3) && optimized && view.scale == 120 {
            assert_eq!(
                paints.get(),
                before + 1,
                "current backdrop color/geometry overrides stale reusable hint"
            );
        }
        c.cache.finish(Some(c.frame)).unwrap();
        c.cache.complete_scene = true;
        // Full composition independently checks every backdrop pixel. The
        // deliberately stale CPU hint above is not a GPU damage oracle.
        c.engine.composition_damage(view, None).unwrap();
        c.finished = true;
    }
    for rotation in [
        Rotation::Normal,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ] {
        for scale in [60, 120, 150, 180] {
            gpu::with(|app, device| {
                let mut engine = Engine::new().unwrap();
                engine.destination = crate::render::gpu::engine::Destination::Readback;
                let mut reference = Engine::new().unwrap();
                reference.destination = crate::render::gpu::engine::Destination::Readback;
                let mut cache = Cache::new(1024 * 1024).unwrap();
                let mut ref_cache = Cache::new(1024 * 1024).unwrap();
                let mut frame = Frame::new().unwrap();
                let mut ref_frame = Frame::new().unwrap();
                let mut replay = RecordingReplay::new().unwrap();
                let mut ref_replay = RecordingReplay::new().unwrap();
                let paints = Cell::new(0);
                let ref_paints = Cell::new(0);
                for turn in 0..11 {
                    let view = Viewport {
                        pixel_w: 8,
                        pixel_h: 8,
                        scale: if turn == 10 {
                            if scale == 120 { 150 } else { 120 }
                        } else {
                            scale
                        },
                        rotation,
                        ..Default::default()
                    };
                    scene(
                        &mut cache,
                        &mut frame,
                        &mut engine,
                        &mut replay,
                        view,
                        turn,
                        true,
                        &paints,
                    );
                    scene(
                        &mut ref_cache,
                        &mut ref_frame,
                        &mut reference,
                        &mut ref_replay,
                        view,
                        turn,
                        false,
                        &ref_paints,
                    );
                    reference.invalidate();
                    gpu::begin(app, device, &mut reference, &ref_cache, Some(&ref_frame));
                    gpu::pump(
                        app,
                        device,
                        &mut reference,
                        &mut ref_cache,
                        Some(&mut ref_frame),
                        Progress::Copied,
                    );
                    let expected = gpu::pixels(app, device, &reference);
                    let direct = view.scale == 120 && !matches!(turn, 3 | 4 | 5 | 6 | 8);
                    let entry = cache.entries().iter().find(|e| e.key == 32).unwrap();
                    assert_eq!(entry.background_applied, direct, "turn={turn}, {view:?}");
                    let jobs = engine.render_jobs - engine.primitive_jobs;
                    gpu::begin(app, device, &mut engine, &cache, Some(&frame));
                    if !direct {
                        assert_ne!(
                            engine.resources.workings[engine.output_index].resource.slot,
                            0
                        );
                    }
                    assert!(engine.resources.charged <= engine.resources.budget);
                    for working in &engine.resources.workings {
                        if working.resource.slot != 0 {
                            assert_eq!(working.info.image.format, r4gfx::format_abgr16161616f);
                        }
                    }
                    gpu::pump(
                        app,
                        device,
                        &mut engine,
                        &mut cache,
                        Some(&mut frame),
                        Progress::Copied,
                    );
                    assert_eq!(
                        gpu::pixels(app, device, &engine),
                        expected,
                        "turn={turn}, {view:?}"
                    );
                    assert_eq!(
                        engine.render_jobs - engine.primitive_jobs - jobs,
                        (if turn == 3 { 3 } else { 2 }) + u64::from(!direct),
                        "same ordered layers; encode removed only with actual opaque proof"
                    );
                }
                assert_eq!(
                    engine.solid_background_sdr_frames,
                    if scale == 120 { 5 } else { 1 }
                );
                assert_eq!(engine.direct_sdr_frames, engine.solid_background_sdr_frames);
                assert_eq!(engine.linear_frames, if scale == 120 { 6 } else { 10 });
                let s = device.bind(app).unwrap();
                engine.close(&s).unwrap();
                reference.close(&s).unwrap();
            });
        }
    }
}
