//! Same original Zig scheduling model and actual R4GFX resource/CMM provider.
//! Fixed existing pixel/work/cancel oracles; no driver or scanout qualification.
use super::super::device::Device;
use super::*;
use crate::render::{
    layers::Mode,
    pixels::PixelBytes,
    primitive_image::{Format, Picture, View},
};
use r4os_sdk::{App, ImportSpec, StartupInfo};
use std::sync::Mutex;
static CASE: Mutex<()> = Mutex::new(());
#[path = "gpu_windows.rs"]
mod windows;
unsafe extern "C" {
    fn r4desk_gpu_context() -> *const a::R4XStartContext;
    fn r4desk_gpu_control(op: u32, value: u64, device: *const g::R4GfxDevice) -> i32;
    fn r4desk_gpu_visible(out: *mut u32);
    fn r4desk_gpu_stats(out: *mut u64);
}
pub(crate) fn visible() -> [u32; 64] {
    let mut out = [0; 64];
    unsafe {
        r4desk_gpu_visible(out.as_mut_ptr());
    }
    out
}
pub(crate) fn stats() -> [u64; 16] {
    let mut out = [0; 16];
    unsafe {
        r4desk_gpu_stats(out.as_mut_ptr());
    }
    out
}
pub(crate) fn control(session: &Session<'_, '_>, op: u32, value: u64) {
    assert_eq!(unsafe { r4desk_gpu_control(op, value, session.raw()) }, 0);
}
pub(crate) fn with(case: impl FnOnce(&App<'_>, &mut Device)) {
    let _guard = CASE.lock().unwrap();
    let raw = unsafe { r4desk_gpu_context() };
    assert!(!raw.is_null(), "previous native model not fully retired");
    let context = unsafe { &*raw };
    let imports = unsafe {
        core::slice::from_raw_parts(
            context.imports as *const a::R4XStartImport,
            context.import_count as usize,
        )
    };
    assert_eq!(imports.len(), 5);
    let sys = unsafe { &*(imports[0].table as *const a::R4XStartR4Sys) };
    let draw = unsafe { &*(imports[1].table as *const a::R4XStartR4Draw) };
    let specs = core::array::from_fn::<_, 5, _>(|i| ImportSpec {
        module_name: match i {
            0 => b"R4SYS",
            1 => b"R4DRAW",
            3 => b"R4NV",
            _ => b"R4GFX",
        },
        symbol_name: match i {
            0 | 1 => b"Query",
            2 => b"DEVICE_V1",
            3 => b"BACKEND_V1",
            _ => b"COLOR_V1",
        },
        minimum: match i {
            0 | 1 => 1,
            2 => g::device_v1_expectation.min_revision as u32,
            3 => 3,
            _ => g::color_v1_expectation.min_revision as u32,
        },
        optional: i == 3,
        group_id: match i {
            0 => 1,
            1 => 3,
            _ => 0,
        },
        magic: match i {
            0 => sys.magic,
            1 => draw.magic,
            _ => 0,
        },
        version: match i {
            0 => sys.abi_version,
            1 => draw.abi_version,
            _ => 0,
        },
        size: match i {
            0 => sys.size,
            1 => draw.size,
            _ => 0,
        },
    });
    let expected = StartupInfo {
        app_class: a::R4XStartAppClass::console,
        required_groups: 10,
        imports: &specs,
    };
    let app = unsafe { App::from_raw(raw, &expected, c"GPUFIX") }.unwrap();
    let mut device = unsafe { Device::open(&app, 0) }.unwrap();
    case(&app, &mut device);
    assert!(device.close(&app).unwrap());
    assert_eq!(&stats()[6..7], &[0]);
    assert_eq!(&stats()[9..13], &[0, 0, 0, 0]);
}
struct NoExternal;
impl FinishExternal for NoExternal {
    fn finish(
        &mut self,
        _: usize,
        _: Option<Consumer>,
        _: bool,
    ) -> core::result::Result<(), Option<Consumer>> {
        panic!("no window BO in this original scene")
    }
}
const FULL: Rect = Rect::new(0, 0, 8, 8);
const PIXEL: Rect = Rect::new(3, 3, 1, 1);
pub(crate) fn capture(cache: &mut Cache, damage: Rect, color: u32, mut frame: Option<&mut Frame>) {
    cache
        .start(Mode::Desktop(FULL), frame.as_deref_mut())
        .unwrap();
    cache
        .paint(1, FULL, damage, frame.as_deref_mut(), None, |p| {
            p.fill(p.bounds(), 0x203040)
        })
        .unwrap();
    cache
        .paint(
            2,
            Rect::new(2, 2, 4, 4),
            damage,
            frame.as_deref_mut(),
            None,
            |p| p.fill(p.bounds(), color),
        )
        .unwrap();
    cache.finish(frame.as_deref()).unwrap();
}
fn expected(color: u32) -> [u32; 64] {
    let mut out = [0x203040; 64];
    for y in 2..6 {
        for x in 2..6 {
            out[y * 8 + x] = color;
        }
    }
    out
}
pub(crate) fn begin(
    app: &App<'_>,
    device: &mut Device,
    engine: &mut Engine,
    cache: &Cache,
    frame: Option<&Frame>,
) {
    let s = device.bind(app).unwrap();
    engine
        .prepare(&s, cache, frame, 1000, || 1, || false)
        .unwrap();
    unsafe { engine.begin(&s, cache, frame, 1000) }.unwrap();
}
pub(crate) fn pump(
    app: &App<'_>,
    device: &mut Device,
    engine: &mut Engine,
    cache: &mut Cache,
    mut frame: Option<&mut Frame>,
    expected: Progress,
) {
    for _ in 0..256 {
        let progress = {
            let s = device.bind(app).unwrap();
            let progress = engine.advance(&s, cache, frame.as_deref_mut(), || 1);
            control(&s, 0, 0);
            progress
        };
        let _ = engine.finish_external(&mut NoExternal, engine.fault.is_some());
        if progress != Progress::Pending {
            assert_eq!(progress, expected);
            return;
        }
    }
    panic!("original scheduling model stalled");
}
#[test]
fn bounded_packet_skips_only_clipped_compatible_draws() {
    with(|app, device| {
        let mut cache = Cache::new(1024 * 1024).unwrap();
        let mut engine = Engine::new().unwrap();
        let mut frame = Frame::new().unwrap();
        frame.set_mirror(false).unwrap();
        let white = [0xffffff];
        let green = [0x00ff00];
        for turn in 0..2 {
            cache.start(Mode::Desktop(FULL), Some(&mut frame)).unwrap();
            cache
                .paint(1, FULL, FULL, Some(&mut frame), None, |p| {
                    p.fill(FULL, 0)?;
                    for (x, y) in [(1, 1), (6, 6), (3, 1)] {
                        let pixels = if turn == 0 || y == 6 { &white } else { &green };
                        p.picture(Picture {
                            view: View::new(
                                Format::Xrgb,
                                1,
                                1,
                                4,
                                PixelBytes::from_words(pixels).bytes(),
                            ),
                            viewport: Rect::new(x, y, 1, 1),
                            clip: FULL,
                            source_x: 0,
                            source_y: 0,
                            guest_w: 1,
                            guest_h: 1,
                        })?;
                    }
                    Ok(())
                })
                .unwrap();
            cache.finish(Some(&frame)).unwrap();
            engine
                .composition_damage(
                    geometry::topology::Viewport {
                        pixel_w: 8,
                        pixel_h: 8,
                        scale: 120,
                        ..Default::default()
                    },
                    Some(
                        (if turn == 0 {
                            FULL
                        } else {
                            Rect::new(1, 1, 3, 1)
                        })
                        .into(),
                    ),
                )
                .unwrap();
            let jobs = engine.primitive_jobs;
            let draws = engine.primitive_draws;
            begin(app, device, &mut engine, &cache, Some(&frame));
            pump(
                app,
                device,
                &mut engine,
                &mut cache,
                Some(&mut frame),
                Progress::Copied,
            );
            let mut pixels = [0; 64];
            for index in [9, 11] {
                pixels[index] = if turn == 0 { 0xffffff } else { 0x00ff00 };
            }
            pixels[54] = 0xffffff;
            assert_eq!(visible(), pixels, "same visible and preserved pixels");
            assert_eq!(
                engine.primitive_draws - draws,
                if turn == 0 { 4 } else { 3 }
            );
            assert_eq!(
                engine.primitive_jobs - jobs,
                2,
                "one fill packet and one immutable image packet"
            );
        }
        let s = device.bind(app).unwrap();
        engine.close(&s).unwrap();
    });
}

#[test]
fn original_upload_partial_cancel_alpha_and_primitive_reuse_oracles() {
    with(|app, device| {
        let mut cache = Cache::new(1024 * 1024).unwrap();
        let mut engine = Engine::new().unwrap();
        capture(&mut cache, FULL, 0x882244, None);
        begin(app, device, &mut engine, &cache, None);
        pump(app, device, &mut engine, &mut cache, None, Progress::Copied);
        assert_eq!(visible(), expected(0x882244));
        assert_eq!(
            (stats()[2], stats()[1], engine.jobs.uploaded_bytes),
            (1, 4, 320)
        );
        let serial = stats()[0];
        for _ in 0..8 {
            let s = device.bind(app).unwrap();
            assert_eq!(engine.advance(&s, &mut cache, None, || 1), Progress::Copied);
        }
        assert_eq!(stats()[0], serial);
        capture(&mut cache, PIXEL, 0x445566, None);
        {
            let s = device.bind(app).unwrap();
            assert!(engine.prepared(&s, &cache, None));
            unsafe { engine.begin(&s, &cache, None, 1000) }.unwrap();
        }
        pump(app, device, &mut engine, &mut cache, None, Progress::Copied);
        let mut current = expected(0x882244);
        current[3 * 8 + 3] = 0x445566;
        assert_eq!(visible(), current);
        assert_eq!(
            (stats()[2], stats()[1], engine.jobs.uploaded_bytes),
            (2, 4, 324)
        );
        capture(&mut cache, PIXEL, 0x998877, None);
        {
            let s = device.bind(app).unwrap();
            control(&s, 2, 1);
            unsafe { engine.begin(&s, &cache, None, 1000) }.unwrap();
        }
        pump(app, device, &mut engine, &mut cache, None, Progress::Failed);
        assert_eq!(visible(), current);
        assert!(engine.needs_full(8, 8));
        {
            let s = device.bind(app).unwrap();
            assert_eq!(
                unsafe { engine.begin(&s, &cache, None, 1000) },
                Err(Error::Incomplete)
            );
            control(&s, 2, 0);
        }
        capture(&mut cache, FULL, 0x998877, None);
        begin(app, device, &mut engine, &cache, None);
        {
            let s = device.bind(app).unwrap();
            let _ = engine.advance(&s, &mut cache, None, || 1);
            engine.cancel(&s, Error::Deadline);
            assert_eq!(engine.close(&s), Err(Error::Busy));
        }
        pump(app, device, &mut engine, &mut cache, None, Progress::Failed);
        assert_eq!(stats()[2], 2);
        capture(&mut cache, FULL, 0x998877, None);
        begin(app, device, &mut engine, &cache, None);
        pump(app, device, &mut engine, &mut cache, None, Progress::Copied);
        assert_eq!(visible(), expected(0x998877));
        cache.start(Mode::Desktop(FULL), None).unwrap();
        cache
            .paint(1, FULL, FULL, None, None, |p| p.fill(FULL, 0))
            .unwrap();
        let white = [0x80ffffff];
        cache
            .paint(2, PIXEL, PIXEL, None, None, |p| {
                p.picture(Picture {
                    view: View::new(
                        Format::Argb,
                        1,
                        1,
                        4,
                        PixelBytes::from_words(&white).bytes(),
                    ),
                    viewport: PIXEL,
                    clip: PIXEL,
                    source_x: 0,
                    source_y: 0,
                    guest_w: 1,
                    guest_h: 1,
                })
            })
            .unwrap();
        cache.finish(None).unwrap();
        begin(app, device, &mut engine, &cache, None);
        pump(app, device, &mut engine, &mut cache, None, Progress::Copied);
        let mut alpha = [0; 64];
        alpha[3 * 8 + 3] = 0xbcbcbc;
        assert_eq!(visible(), alpha);
        {
            let s = device.bind(app).unwrap();
            engine.close(&s).unwrap();
        }
        let mut engine = Engine::new().unwrap();
        let mut cache = Cache::new(1024 * 1024).unwrap();
        let mut frame = Frame::new().unwrap();
        frame.set_mirror(false).unwrap();
        capture(&mut cache, FULL, 0x882244, Some(&mut frame));
        begin(app, device, &mut engine, &cache, Some(&frame));
        pump(
            app,
            device,
            &mut engine,
            &mut cache,
            Some(&mut frame),
            Progress::Copied,
        );
        assert_eq!(visible(), expected(0x882244));
        assert_eq!(engine.jobs.uploaded_bytes, 0);
        assert!(engine.primitive_draws != 0);
        let jobs = engine.primitive_jobs;
        capture(&mut cache, FULL, 0x882244, Some(&mut frame));
        begin(app, device, &mut engine, &cache, Some(&frame));
        pump(
            app,
            device,
            &mut engine,
            &mut cache,
            Some(&mut frame),
            Progress::Copied,
        );
        assert_eq!(visible(), expected(0x882244));
        assert_eq!(engine.primitive_jobs, jobs);
        {
            let s = device.bind(app).unwrap();
            engine.close(&s).unwrap();
        }
    });
}

#[test]
fn changed_primitive_footprint_and_disjoint_batch_preserve_original_pixels() {
    with(|app, device| {
        let mut cache = Cache::new(1024 * 1024).unwrap();
        let mut engine = Engine::new().unwrap();
        let mut frame = Frame::new().unwrap();
        frame.set_mirror(false).unwrap();
        let white = [0x80ffffff];
        for (turn, color) in [0xff0000, 0x00ff00, 0x0000ff].into_iter().enumerate() {
            cache.start(Mode::Desktop(FULL), Some(&mut frame)).unwrap();
            cache
                .paint(1, FULL, FULL, Some(&mut frame), None, |p| {
                    p.fill(FULL, 0)?;
                    for x in [1, 3] {
                        p.picture(Picture {
                            view: View::new(
                                Format::Argb,
                                1,
                                1,
                                4,
                                PixelBytes::from_words(&white).bytes(),
                            ),
                            viewport: Rect::new(x, 1, 1, 1),
                            clip: FULL,
                            source_x: 0,
                            source_y: 0,
                            guest_w: 1,
                            guest_h: 1,
                        })?;
                        if x == 1 {
                            p.fill(Rect::new(6, 6, 1, 1), color)?;
                        }
                    }
                    if turn == 2 {
                        p.fill(Rect::new(5, 5, 1, 1), 0xffffff)?;
                    }
                    Ok(())
                })
                .unwrap();
            cache.finish(Some(&frame)).unwrap();
            engine
                .composition_damage(
                    geometry::topology::Viewport {
                        pixel_w: 8,
                        pixel_h: 8,
                        scale: 120,
                        ..Default::default()
                    },
                    Some(
                        (match turn {
                            0 => FULL,
                            1 => Rect::new(6, 6, 1, 1),
                            _ => Rect::new(5, 5, 2, 2),
                        })
                        .into(),
                    ),
                )
                .unwrap();
            let draws = engine.primitive_draws;
            let jobs = engine.primitive_jobs;
            let uploads = engine.jobs.uploaded_bytes;
            begin(app, device, &mut engine, &cache, Some(&frame));
            assert!(engine.direct_sdr);
            assert!(
                engine
                    .resources
                    .workings
                    .iter()
                    .all(|image| image.resource.slot == 0)
            );
            if turn == 2 {
                assert_eq!(engine.capture_damage, Some(Rect::new(5, 5, 2, 2).into()));
                let layer = frame.commands()[0].layer as usize;
                assert_eq!(
                    engine.redraw_regions[layer],
                    Some(Rect::new(5, 5, 2, 2)),
                    "retained image, command-count change and original output damage"
                );
            }
            pump(
                app,
                device,
                &mut engine,
                &mut cache,
                Some(&mut frame),
                Progress::Copied,
            );
            let mut expected = [0; 64];
            // These two images blend into an opaque ARGB layer before its
            // sRGB decode, unlike the separate linear-alpha layer oracle.
            expected[9] = 0x808080;
            expected[11] = 0x808080;
            expected[54] = color;
            if turn == 2 {
                expected[45] = 0xffffff;
            }
            assert_eq!(visible(), expected, "ordered alpha and unchanged pixels");
            assert_eq!(
                engine.render_jobs - engine.primitive_jobs,
                turn as u64 + 1,
                "one composition draw per frame, no encode job; primitive jobs counted separately"
            );
            assert_eq!(
                stats()[2],
                turn as u64 + 1,
                "actual presentation still required"
            );
            if turn == 0 {
                assert_eq!(engine.primitive_draws - draws, 4);
                assert_eq!(
                    engine.primitive_jobs - jobs,
                    2,
                    "disjoint fill can cross the first alpha image"
                );
            } else {
                assert_eq!(
                    engine.primitive_draws - draws,
                    if turn == 1 { 2 } else { 3 },
                    "complete ordered replay only inside actual changed pixels"
                );
                assert_eq!(
                    engine.jobs.uploaded_bytes, uploads,
                    "unchanged alpha texture stays resident"
                );
            }
        }
        // A separately translucent layer must return to linear composition.
        cache.start(Mode::Desktop(FULL), Some(&mut frame)).unwrap();
        cache
            .paint(1, FULL, FULL, Some(&mut frame), None, |p| p.fill(FULL, 0))
            .unwrap();
        cache
            .paint(2, PIXEL, PIXEL, Some(&mut frame), None, |p| {
                p.clear(PIXEL)?;
                p.picture(Picture {
                    view: View::new(
                        Format::Argb,
                        1,
                        1,
                        4,
                        PixelBytes::from_words(&white).bytes(),
                    ),
                    viewport: PIXEL,
                    clip: PIXEL,
                    source_x: 0,
                    source_y: 0,
                    guest_w: 1,
                    guest_h: 1,
                })
            })
            .unwrap();
        cache.finish(Some(&frame)).unwrap();
        begin(app, device, &mut engine, &cache, Some(&frame));
        assert!(!engine.direct_sdr);
        assert_ne!(
            engine.resources.workings[engine.output_index].resource.slot,
            0
        );
        pump(
            app,
            device,
            &mut engine,
            &mut cache,
            Some(&mut frame),
            Progress::Copied,
        );
        let mut expected = [0; 64];
        expected[27] = 0xbcbcbc;
        assert_eq!(visible(), expected, "original separate linear-alpha oracle");
        cache.start(Mode::Desktop(FULL), Some(&mut frame)).unwrap();
        cache
            .paint(1, FULL, FULL, Some(&mut frame), None, |p| {
                p.fill(FULL, 0x225588)
            })
            .unwrap();
        cache.finish(Some(&frame)).unwrap();
        begin(app, device, &mut engine, &cache, Some(&frame));
        assert!(engine.direct_sdr);
        assert!(
            engine
                .resources
                .workings
                .iter()
                .all(|image| image.resource.slot == 0)
        );
        pump(
            app,
            device,
            &mut engine,
            &mut cache,
            Some(&mut frame),
            Progress::Copied,
        );
        assert_eq!(visible(), [0x225588; 64]);
        let original = engine.output_color;
        for field in 0..4 {
            engine.output_color = original;
            match field {
                0 => engine.output_color.transfer = g::color_transfer_pq,
                1 => engine.output_color.flags = 1,
                2 => engine.output_color.reference_white += 1,
                _ => engine.output_color.range = g::color_range_limited,
            }
            assert!(!engine.allows_direct_sdr(&cache, Some(&frame)));
        }
        engine.output_color = original;
        let s = device.bind(app).unwrap();
        engine.close(&s).unwrap();
    });
}

#[test]
fn original_dependency_pool_and_64_step_250us_admission_oracles() {
    with(|app, device| {
        let mut cache = Cache::new(1024 * 1024).unwrap();
        let mut frame = Frame::new().unwrap();
        frame.set_mirror(false).unwrap();
        cache.start(Mode::Desktop(FULL), Some(&mut frame)).unwrap();
        let green = [0x8000ff00; 64];
        cache
            .paint(1, FULL, FULL, Some(&mut frame), None, |p| {
                p.fill(FULL, 0)?;
                for _ in 0..1536 {
                    p.picture(Picture {
                        view: View::new(
                            Format::Argb,
                            8,
                            8,
                            32,
                            PixelBytes::from_words(&green).bytes(),
                        ),
                        viewport: FULL,
                        clip: FULL,
                        source_x: 0,
                        source_y: 0,
                        guest_w: 8,
                        guest_h: 8,
                    })?;
                }
                Ok(())
            })
            .unwrap();
        cache.finish(Some(&frame)).unwrap();
        for mode in 0..7 {
            let failed = mode == 1 || mode == 5;
            let mut engine = Engine::new().unwrap();
            {
                let s = device.bind(app).unwrap();
                // Earlier original cases had consumed the two injected
                // startup Busy replies before entering the admission case.
                control(&s, 1, 0);
                control(&s, 10, if mode == 3 { 24 } else { 0 });
                control(&s, 6, u64::from(matches!(mode, 0 | 1 | 4 | 5)));
            }
            {
                let s = device.bind(app).unwrap();
                engine
                    .prepare(&s, &cache, Some(&frame), 1_000_000_000, || 1, || false)
                    .unwrap();
                unsafe { engine.begin(&s, &cache, Some(&frame), 1_000_000_000) }.unwrap();
            }
            {
                let s = device.bind(app).unwrap();
                let previous_retirement = if mode == 6 {
                    assert_eq!(
                        engine.advance(&s, &mut cache, Some(&mut frame), || 1),
                        Progress::Pending
                    );
                    assert!(engine.jobs.depth > 0);
                    engine.jobs.retirement_epoch
                } else {
                    0
                };
                let mut clock = 1;
                let mut arrival = mode == 6;
                let result = engine.advance(&s, &mut cache, Some(&mut frame), || {
                    // The event becomes visible on the first actual GPU
                    // clock sample, before its introductory completion scan.
                    if arrival {
                        control(&s, 0, 0);
                        arrival = false;
                    }
                    if mode == 5 {
                        return 1_000_000_000;
                    }
                    if mode == 4 || mode == 6 {
                        return 1_000_001;
                    }
                    let now = clock;
                    if mode == 1 {
                        clock += 100_000;
                    }
                    now
                });
                if mode != 5 {
                    assert_eq!(result, Progress::Pending);
                }
                match mode {
                    0 | 4 => {
                        assert_eq!(engine.last_admission_steps, Engine::ADMISSION_LIMIT);
                        assert!(engine.immediate_work);
                    }
                    1 => {
                        assert!(engine.last_admission_steps <= 3 && engine.immediate_work);
                        engine.cancel(&s, Error::Deadline);
                    }
                    2 => assert!(!engine.immediate_work),
                    5 => {
                        assert_eq!(engine.fault, Some(Error::Deadline));
                        assert_eq!(engine.last_admission_steps, 0);
                        assert_eq!(engine.render_jobs, 0);
                    }
                    6 => assert!(engine.jobs.retirement_epoch > previous_retirement),
                    _ => (),
                }
                control(&s, 6, 0);
            }
            // The original 1536-command case needs at most 512 progress turns.
            let mut completed = false;
            for _ in 0..512 {
                let progress = {
                    let s = device.bind(app).unwrap();
                    let value = engine.advance(&s, &mut cache, Some(&mut frame), || 1);
                    control(&s, 0, 0);
                    value
                };
                let _ = engine.finish_external(&mut NoExternal, failed);
                if progress != Progress::Pending {
                    assert_eq!(
                        progress,
                        if failed {
                            Progress::Failed
                        } else {
                            Progress::Copied
                        }
                    );
                    completed = true;
                    break;
                }
            }
            assert!(completed, "finite original receipt pool must make progress");
            if !failed {
                assert_eq!(visible(), [0x00ff00; 64]);
            }
            if mode == 3 {
                assert!(engine.primitive_jobs > 24);
                assert!(stats()[7] <= g::device_job_capacity as u64);
            }
            let s = device.bind(app).unwrap();
            engine.close(&s).unwrap();
            assert_eq!(stats()[6], 0);
            control(&s, 10, 0);
        }
    });
}

#[test]
fn original_offscreen_and_shared_readback_epoch_damage_deadline_oracles() {
    use crate::capture::readback::{Phase as ReadPhase, Readback};
    with(|app, device| {
        let mut cache = Cache::new(1024 * 1024).unwrap();
        let mut engine = Engine::new().unwrap();
        capture(&mut cache, FULL, 0x225588, None);
        {
            let s = device.bind(app).unwrap();
            control(&s, 7, 0);
            assert_eq!(
                engine.prepare(&s, &cache, None, 1000, || 1, || false),
                Err(Error::Unsupported)
            );
        }
        engine.destination = Destination::Readback;
        begin(app, device, &mut engine, &cache, None);
        pump(app, device, &mut engine, &mut cache, None, Progress::Copied);
        assert!(engine.present_fence.is_none() && !engine.uses_swapchain());
        assert_eq!((stats()[2], stats()[13]), (0, 0));
        let mut reader = {
            let s = device.bind(app).unwrap();
            let mut reader = unsafe { Readback::new(app, &s) }.unwrap();
            reader
                .prepare(&s, 8, 8, engine.output_format, engine.output_color)
                .unwrap();
            reader
        };
        let full = [g::R4GfxRect {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
        }];
        let point = [g::R4GfxRect {
            x: 3,
            y: 3,
            width: 1,
            height: 1,
        }];
        let read = |reader: &mut Readback, device: &mut Device| {
            for _ in 0..32 {
                let s = device.bind(app).unwrap();
                control(&s, 0, 0);
                reader.poll(&s, 2).unwrap();
                if !reader.pending() {
                    return;
                }
            }
            panic!("original readback failed to retire");
        };
        {
            let s = device.bind(app).unwrap();
            reader
                .begin(
                    &s,
                    engine.resources.outputs[0].resource,
                    1,
                    1,
                    0,
                    &full,
                    1,
                    1000,
                )
                .unwrap();
            engine.readback_pin = reader.source().unwrap();
            assert!(engine.capture_blocked(1));
            assert_eq!(engine.close(&s), Err(Error::Busy));
        }
        read(&mut reader, device);
        assert_eq!(reader.phase(), Ok(ReadPhase::Ready));
        assert_eq!(reader.pixels().unwrap(), expected(0x225588));
        reader.acknowledge(true).unwrap();
        assert!(reader.valid());
        assert_eq!(reader.acknowledged(), 1);
        engine.readback_pin = reader.source().unwrap();
        assert!(!engine.capture_blocked(2));
        capture(&mut cache, PIXEL, 0x998877, None);
        begin(app, device, &mut engine, &cache, None);
        pump(app, device, &mut engine, &mut cache, None, Progress::Copied);
        {
            let s = device.bind(app).unwrap();
            reader
                .begin(
                    &s,
                    engine.resources.outputs[0].resource,
                    1,
                    2,
                    1,
                    &point,
                    1,
                    1000,
                )
                .unwrap();
        }
        read(&mut reader, device);
        let mut pixels = expected(0x225588);
        pixels[27] = 0x998877;
        assert_eq!(reader.pixels().unwrap(), pixels);
        assert_eq!(reader.stats().unwrap().copy_bytes, 260);
        reader.acknowledge(true).unwrap();
        {
            let s = device.bind(app).unwrap();
            reader
                .begin(
                    &s,
                    engine.resources.outputs[0].resource,
                    1,
                    3,
                    2,
                    &point,
                    2,
                    3,
                )
                .unwrap();
            reader.poll(&s, 3).unwrap();
            assert_eq!(reader.phase(), Ok(ReadPhase::Draining));
            assert_ne!(reader.source().unwrap().slot, 0);
            assert_eq!(reader.close(&s), Err(Error::Busy));
            control(&s, 0, 0);
            reader.poll(&s, 3).unwrap();
        }
        assert_eq!(reader.phase(), Ok(ReadPhase::Failed));
        assert_eq!(reader.source().unwrap().slot, 0);
        assert!(!reader.valid());
        reader.acknowledge(false).unwrap();
        {
            let s = device.bind(app).unwrap();
            reader
                .begin(
                    &s,
                    engine.resources.outputs[0].resource,
                    2,
                    1,
                    0,
                    &point,
                    1,
                    1000,
                )
                .unwrap();
        }
        read(&mut reader, device);
        assert_eq!(reader.pixels().unwrap(), pixels);
        assert_eq!(reader.stats().unwrap().copy_bytes, 516);
        {
            let s = device.bind(app).unwrap();
            reader.close(&s).unwrap();
        }
        // Original demand/cursor/skipped-frame/cancel and pre-profile cases,
        // using the same library owner, never a copied readback algorithm.
        use crate::capture::remote::{Capture as Remote, Cursor};
        let mut remote = Remote::default();
        let view = crate::render::output_geometry::topology::Viewport {
            pixel_w: 8,
            pixel_h: 8,
            ..Default::default()
        };
        remote.demand(true);
        {
            let s = device.bind(app).unwrap();
            unsafe { remote.prepare(app, &s, view, engine.output_format, engine.output_color) }
                .unwrap();
            remote.record(
                0,
                1,
                PIXEL,
                view,
                Cursor {
                    x: 2,
                    y: 3,
                    visible: true,
                    separate: true,
                },
            );
            remote.complete(&s, 0, engine.resources.outputs[0].resource, 1);
            engine.readback_pin = remote.source().unwrap();
            assert!(engine.capture_blocked(1));
            assert_eq!(engine.close(&s), Err(Error::Busy));
            remote.record(1, 2, PIXEL, view, Cursor::default());
            remote.complete(&s, 1, engine.resources.outputs[0].resource, 1);
            assert_eq!(remote.skipped, 1);
            control(&s, 0, 0);
            remote.poll(&s, 2).unwrap();
        }
        engine.readback_pin = remote.source().unwrap();
        assert!(!engine.capture_blocked(2));
        let image = remote.image().unwrap();
        assert_eq!(image.pixels, pixels);
        assert_eq!(
            (
                image.cursor.x,
                image.cursor.y,
                image.cursor.visible,
                image.cursor.separate
            ),
            (2, 3, true, true)
        );
        remote.acknowledge(true).unwrap();
        assert!(remote.take_redraw(2));
        {
            let s = device.bind(app).unwrap();
            remote.record(0, 3, PIXEL, view, Cursor::default());
            remote.complete(&s, 0, engine.resources.outputs[0].resource, 3);
            remote.demand(false);
            remote.poll(&s, 3).unwrap();
            assert_ne!(remote.source().unwrap().slot, 0);
            control(&s, 0, 0);
            remote.poll(&s, 4).unwrap();
            assert_eq!(remote.source().unwrap().slot, 0);
            assert!(remote.image().is_none());
            remote.close(&s).unwrap();
            remote.demand(true);
            remote.prepare_profile(&s, view).unwrap();
            let mut canonical = pixels;
            remote.record(0, 4, FULL, view, Cursor::default());
            remote.stage_profile(0, &canonical, 4);
            canonical.fill(0x373737);
            assert!(remote.image().is_none());
            remote.complete(&s, 0, engine.resources.outputs[0].resource, 5);
            assert_eq!(remote.image().unwrap().pixels, pixels);
            remote.acknowledge(true).unwrap();
            remote.demand(false);
            remote.poll(&s, 6).unwrap();
            remote.close(&s).unwrap();
            engine.readback_pin = remote.source().unwrap();
            engine.close(&s).unwrap();
        }
        assert_eq!((stats()[2], stats()[13]), (0, 0));
    });
}

#[test]
fn original_fifo_distinguishes_copy_from_visible_and_retains_both_outputs() {
    with(|app, device| {
        let mut cache = Cache::new(1024 * 1024).unwrap();
        let mut engine = Engine::new().unwrap();
        {
            let s = device.bind(app).unwrap();
            control(&s, 4, 1);
            control(&s, 13, 0xabcdef);
        }
        capture(&mut cache, FULL, 0x882244, None);
        begin(app, device, &mut engine, &cache, None);
        pump(app, device, &mut engine, &mut cache, None, Progress::Copied);
        {
            let s = device.bind(app).unwrap();
            for _ in 0..4 {
                control(&s, 0, 0);
                engine.poll_presentation(&s).unwrap();
            }
        }
        assert!(engine.uses_swapchain() && engine.pending() && !engine.active());
        assert!(engine.completion().is_none());
        let status = engine.chain_status.unwrap();
        assert_ne!(status.frame0.copied_ns, 0);
        assert_eq!(status.frame0.visible_ns, 0);
        assert_eq!(visible(), [0xabcdef; 64]);
        capture(&mut cache, FULL, 0x445566, None);
        {
            let s = device.bind(app).unwrap();
            unsafe { engine.begin(&s, &cache, None, 1000) }.unwrap();
        }
        pump(app, device, &mut engine, &mut cache, None, Progress::Copied);
        {
            let s = device.bind(app).unwrap();
            assert_eq!(engine.acquire(&s, 0), Err(Error::Busy));
            assert_eq!(stats()[2], 1);
            control(&s, 5, 1);
        }
        let mut receipts = 0;
        for _ in 0..32 {
            let s = device.bind(app).unwrap();
            control(&s, 0, 0);
            engine.poll_presentation(&s).unwrap();
            if let Some(done) = engine.completion() {
                receipts += 1;
                assert_eq!(done.frame, receipts);
                assert_eq!(
                    (done.status.result, done.status.path),
                    (1, g::present_path_composition)
                );
                assert!(
                    done.status.visible_ns >= done.status.copied_ns
                        && done.status.copied_ns >= done.status.submitted_ns
                );
            }
            if !engine.pending() {
                break;
            }
        }
        assert_eq!((receipts, stats()[2]), (2, 2));
        assert!(!engine.pending());
        assert_eq!(visible(), expected(0x445566));
        let s = device.bind(app).unwrap();
        engine.close(&s).unwrap();
    });
}

#[test]
fn separated_damage_repairs_both_target_ages_and_matches_full_scaled_rotated_pixels() {
    use geometry::topology::{Point, Rotation, Viewport};

    fn scene(cache: &mut Cache, frame: &mut Frame, view: Viewport, native: bool, colors: [u32; 2]) {
        let bounds = geometry::logical(view).unwrap();
        cache
            .start(
                if native {
                    Mode::NativeOutput(view)
                } else {
                    Mode::Desktop(FULL)
                },
                Some(frame),
            )
            .unwrap();
        cache
            .paint(1, bounds, bounds, Some(frame), None, |p| {
                if native {
                    // Real atlas sampling and the float composition/encode
                    // path, including a translucent first layer. The later
                    // Desktop variant independently exercises direct SDR.
                    let count = (bounds.w * bounds.h) as usize;
                    let mut source: Vec<u32> = (0..count)
                        .map(|i| ((i as u32 % 4) * 85 << 24) | 0x203040)
                        .collect();
                    source[0] = 0xff000000 | colors[0];
                    source[count - 1] = 0xff000000 | colors[1];
                    source[(bounds.h / 2 * bounds.w + bounds.w / 2) as usize] = 0xffaabbcc;
                    return p.picture(Picture {
                        view: View::new(
                            Format::Argb,
                            bounds.w as u32,
                            bounds.h as u32,
                            bounds.w as usize * 4,
                            PixelBytes::from_words(&source).bytes(),
                        ),
                        viewport: bounds,
                        clip: bounds,
                        source_x: 0,
                        source_y: 0,
                        guest_w: bounds.w as u32,
                        guest_h: bounds.h as u32,
                    });
                }
                p.fill(bounds, 0x203040)?;
                p.fill(Rect::new(bounds.x, bounds.y, 1, 1), colors[0])?;
                p.fill(
                    Rect::new(bounds.right() - 1, bounds.bottom() - 1, 1, 1),
                    colors[1],
                )?;
                p.fill(
                    Rect::new(bounds.x + bounds.w / 2, bounds.y + bounds.h / 2, 1, 1),
                    0xaabbcc,
                )
            })
            .unwrap();
        cache.finish(Some(frame)).unwrap();
    }
    let variants = [
        Rotation::Normal,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ]
    .into_iter()
    .flat_map(|rotation| {
        [60, 120, 150, 180]
            .into_iter()
            .map(move |scale| (true, rotation, scale))
    })
    .chain([(false, Rotation::Normal, 120)]);
    for (native, rotation, scale) in variants {
        with(|app, device| {
            let view = Viewport {
                pixel_w: 8,
                pixel_h: 8,
                scale,
                rotation,
                origin: if native {
                    Point { x: -19, y: 7 }
                } else {
                    Point::default()
                },
            };
            let bounds = geometry::logical(view).unwrap();
            let points = [
                Rect::new(bounds.x, bounds.y, 1, 1),
                Rect::new(bounds.right() - 1, bounds.bottom() - 1, 1, 1),
            ];
            let mut cache = Cache::new(1024 * 1024).unwrap();
            let mut reference_cache = Cache::new(1024 * 1024).unwrap();
            let mut frame = Frame::new().unwrap();
            let mut reference_frame = Frame::new().unwrap();
            frame.set_mirror(false).unwrap();
            reference_frame.set_mirror(false).unwrap();
            let mut engine = Engine::new().unwrap();
            let mut reference = Engine::new().unwrap();
            reference.destination = Destination::Readback;
            let mut colors = [0x113355, 0x7799bb];
            let s = device.bind(app).unwrap();
            control(&s, 4, 1);
            drop(s);
            for pair in 0..4 {
                let s = device.bind(app).unwrap();
                control(&s, 5, 0);
                drop(s);
                for member in 0..2 {
                    let step = pair * 2 + member;
                    let changed = if step == 0 {
                        bounds
                    } else {
                        let index = (step + 1) % 2;
                        colors[index] ^= 0x010307 * step as u32;
                        points[index]
                    };
                    scene(&mut cache, &mut frame, view, native, colors);
                    scene(
                        &mut reference_cache,
                        &mut reference_frame,
                        view,
                        native,
                        colors,
                    );
                    engine
                        .composition_damage(view, Some(changed.into()))
                        .unwrap();
                    // The reference always recomposes the entire output, with
                    // no damage hint or history from the implementation above.
                    reference.invalidate();
                    begin(
                        app,
                        device,
                        &mut reference,
                        &reference_cache,
                        Some(&reference_frame),
                    );
                    pump(
                        app,
                        device,
                        &mut reference,
                        &mut reference_cache,
                        Some(&mut reference_frame),
                        Progress::Copied,
                    );
                    let expected = pixels(app, device, &reference);
                    let before_jobs = engine.render_jobs;
                    let before_primitives = engine.primitive_jobs;
                    let before_pixels = engine.composed_pixels;
                    begin(app, device, &mut engine, &cache, Some(&frame));
                    assert_eq!(engine.output_index, member);
                    if step < 2 {
                        assert_eq!(engine.encode_regions, FULL.into());
                    } else {
                        assert_eq!(engine.encode_regions.len(), 2, "{view:?}");
                        assert!(engine.encode_regions.area() < 64, "{view:?}");
                        assert!(
                            !engine.encode_regions.intersects(Rect::new(3, 3, 1, 1))
                                || scale != 120
                        );
                    }
                    let area = engine.encode_regions.area();
                    let regions = engine.encode_regions.len() as u64;
                    let jobs_per_region = if engine.direct_sdr { 1 } else { 2 };
                    assert_eq!(engine.direct_sdr, !native);
                    pump(
                        app,
                        device,
                        &mut engine,
                        &mut cache,
                        Some(&mut frame),
                        Progress::Copied,
                    );
                    assert_eq!(
                        engine.render_jobs
                            - before_jobs
                            - (engine.primitive_jobs - before_primitives),
                        regions * jobs_per_region,
                        "composition/encode jobs exclude the separately counted layer raster jobs"
                    );
                    assert_eq!(engine.composed_pixels - before_pixels, area);
                    assert_eq!(
                        pixels(app, device, &engine),
                        expected,
                        "step={step}, native={native}, {view:?}"
                    );
                    if step >= 2 && scale == 120 {
                        assert_eq!(area, 2, "two pixels replace the previous 64-pixel envelope");
                    }
                }
                let s = device.bind(app).unwrap();
                control(&s, 5, 1);
                drop(s);
                let mut receipts = 0;
                for _ in 0..32 {
                    let s = device.bind(app).unwrap();
                    control(&s, 0, 0);
                    engine.poll_presentation(&s).unwrap();
                    if let Some(done) = engine.completion() {
                        assert_eq!(done.status.result, 1);
                        receipts += 1;
                    }
                    if !engine.pending() {
                        break;
                    }
                }
                assert_eq!(receipts, 2);
                assert!(!engine.pending());
            }
            assert_eq!(engine.multi_region_frames, 6);
            assert!(engine.composed_pixels < engine.composition_envelope_pixels);
            if !native {
                let expected = pixels(app, device, &reference);
                // First catch up the older image; an unchanged subsequent
                // capture must really present without a fabricated GPU job.
                for unchanged in 0..2 {
                    scene(&mut cache, &mut frame, view, native, colors);
                    engine
                        .composition_damage(view, Some(Regions::default()))
                        .unwrap();
                    let before_jobs = engine.render_jobs;
                    begin(app, device, &mut engine, &cache, Some(&frame));
                    if unchanged == 1 {
                        assert!(engine.encode_regions.is_empty());
                    }
                    pump(
                        app,
                        device,
                        &mut engine,
                        &mut cache,
                        Some(&mut frame),
                        Progress::Copied,
                    );
                    if unchanged == 1 {
                        assert_eq!(engine.render_jobs, before_jobs);
                    }
                    assert_eq!(pixels(app, device, &engine), expected);
                    let mut receipts = 0;
                    for _ in 0..32 {
                        let s = device.bind(app).unwrap();
                        control(&s, 0, 0);
                        engine.poll_presentation(&s).unwrap();
                        if let Some(done) = engine.completion() {
                            assert_eq!(done.status.result, 1);
                            receipts += 1;
                        }
                        if !engine.pending() {
                            break;
                        }
                    }
                    assert_eq!(receipts, 1);
                    assert!(!engine.pending());
                }
            }
            let s = device.bind(app).unwrap();
            engine.close(&s).unwrap();
            reference.close(&s).unwrap();
        });
    }
}

pub(crate) fn pixels(app: &App<'_>, device: &mut Device, engine: &Engine) -> [u32; 64] {
    use crate::capture::readback::Readback;
    let mut reader = {
        let s = device.bind(app).unwrap();
        let mut value = unsafe { Readback::new(app, &s) }.unwrap();
        value
            .prepare(&s, 8, 8, engine.output_format, engine.output_color)
            .unwrap();
        value
            .begin(
                &s,
                engine.resources.outputs[engine.output_index].resource,
                1,
                engine.frame,
                0,
                &[g::R4GfxRect {
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 8,
                }],
                1,
                1000,
            )
            .unwrap();
        value
    };
    for _ in 0..32 {
        let s = device.bind(app).unwrap();
        control(&s, 0, 0);
        reader.poll(&s, 2).unwrap();
        if !reader.pending() {
            break;
        }
    }
    let result = reader.pixels().unwrap().try_into().unwrap();
    reader.acknowledge(true).unwrap();
    let s = device.bind(app).unwrap();
    assert_eq!(reader.source().unwrap().slot, 0);
    reader.close(&s).unwrap();
    result
}
