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
fn begin(
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
fn pump(
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
                    Some(match turn {
                        0 => FULL,
                        1 => Rect::new(6, 6, 1, 1),
                        _ => Rect::new(5, 5, 2, 2),
                    }),
                )
                .unwrap();
            let draws = engine.primitive_draws;
            let jobs = engine.primitive_jobs;
            let uploads = engine.jobs.uploaded_bytes;
            begin(app, device, &mut engine, &cache, Some(&frame));
            if turn == 2 {
                assert_eq!(engine.capture_damage, Some(Rect::new(5, 5, 2, 2)));
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
