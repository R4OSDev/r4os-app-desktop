//! Real cold output tasks over the existing original GPU/provider peer. This
//! tests the owning Output coordinator, not another fake preparation machine.
use super::*;
use crate::{hosted::window_image::NativeMemory, render::gpu::engine::tests as fixture};
use core::pin::pin;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
    time::{Duration, Instant},
};
static THREAD: Mutex<Option<std::thread::JoinHandle<i32>>> = Mutex::new(None);
static ALLOW: AtomicBool = AtomicBool::new(true);
static MODE: AtomicI32 = AtomicI32::new(0);
static SPECS: [r4os_sdk::ImportSpec; 5] = [
    r4os_sdk::ImportSpec {
        module_name: b"R4SYS",
        symbol_name: b"Query",
        minimum: 1,
        optional: false,
        group_id: 1,
        magic: a::r4xstart_r4sys_magic,
        version: a::r4xstart_r4sys_version,
        size: core::mem::size_of::<a::R4XStartR4Sys>() as u32,
    },
    r4os_sdk::ImportSpec {
        module_name: b"R4DRAW",
        symbol_name: b"Query",
        minimum: 1,
        optional: false,
        group_id: 3,
        magic: a::r4xstart_r4draw_magic,
        version: a::r4xstart_r4draw_version,
        size: core::mem::size_of::<a::R4XStartR4Draw>() as u32,
    },
    r4os_sdk::ImportSpec {
        module_name: b"R4GFX",
        symbol_name: b"DEVICE_V1",
        minimum: 10,
        optional: false,
        group_id: 0,
        magic: 0,
        version: 0,
        size: 0,
    },
    r4os_sdk::ImportSpec {
        module_name: b"R4NV",
        symbol_name: b"BACKEND_V1",
        minimum: 3,
        optional: true,
        group_id: 0,
        magic: 0,
        version: 0,
        size: 0,
    },
    r4os_sdk::ImportSpec {
        module_name: b"R4GFX",
        symbol_name: b"COLOR_V1",
        minimum: 4,
        optional: false,
        group_id: 0,
        magic: 0,
        version: 0,
        size: 0,
    },
];
static EXPECTED: StartupInfo<'static> = StartupInfo {
    app_class: a::R4XStartAppClass::console,
    required_groups: 10,
    imports: &SPECS,
};
unsafe extern "C" fn create(
    entry: a::ThreadEntryFn,
    arg: u64,
    stack: u64,
    flags: u32,
    out: *mut a::ProgramJoinHandle,
) -> i32 {
    assert_eq!((stack, flags), (256 * 1024, 0));
    if MODE.load(Ordering::Acquire) == 3 {
        return a::thread_error_unsupported;
    }
    let mut owner = THREAD.lock().unwrap();
    assert!(owner.is_none());
    *owner = Some(std::thread::spawn(move || {
        while !ALLOW.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        unsafe { entry(arg) }
    }));
    unsafe {
        out.write(a::ProgramJoinHandle {
            thread_id: 88,
            instance_id: 86,
            thread_generation: 17,
            instance_generation: 18,
            ..Default::default()
        });
    }
    0
}
unsafe extern "C" fn join(handle: *const a::ProgramJoinHandle, ticks: u64, out: *mut i32) -> i32 {
    let h = unsafe { &*handle };
    assert_eq!(
        (
            h.thread_id,
            h.instance_id,
            h.thread_generation,
            h.instance_generation,
            ticks
        ),
        (88, 86, 17, 18, 0)
    );
    if MODE.load(Ordering::Acquire) == 1 {
        return a::thread_error_timeout;
    }
    let mut owner = THREAD.lock().unwrap();
    if !owner.as_ref().unwrap().is_finished() {
        return a::thread_error_timeout;
    }
    unsafe {
        out.write(owner.take().unwrap().join().unwrap());
    }
    0
}
unsafe extern "C" fn precise(out: *mut a::MonotonicClockInfo) -> i32 {
    unsafe {
        out.write(a::MonotonicClockInfo {
            flags: a::monotonic_clock_flag_valid,
            instant_ns: 1,
            event_effective_hz: 100,
            ..Default::default()
        });
    }
    1
}
fn until(phase: &str, mut complete: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(3);
    while !complete() {
        assert!(
            Instant::now() < end,
            "original output coordinator did not retire: {phase}"
        );
        std::thread::yield_now();
    }
}
fn scene<M: Memory>(
    app: &App<'_>,
    owner: &mut Output<'_>,
    windows: &mut Set<M, Consumer>,
    primary: bool,
    background: u32,
) {
    use crate::{
        hosted::runtime as hosted,
        input::runtime as input,
        render::{icons, runtime::Stats as FrameStats, shapes::Renderer, text::cache::Fonts},
        runtime::views,
        shell::{state::State, status::Status, tray_sync::Mirror},
    };
    struct Builtin;
    impl Fonts for Builtin {
        fn revision(&mut self) -> u32 {
            0
        }
        fn has_glyph_row(&self) -> bool {
            false
        }
        fn info(&mut self, _: u32, _: &mut a::GuiFontInfo) -> i32 {
            -1
        }
        fn bitmap(&mut self, _: u32, _: u32, _: &mut a::GuiGlyphBitmap) -> i32 {
            -1
        }
        fn row(&mut self, _: u32, _: u32, _: u32) -> u64 {
            0
        }
    }
    let mut keys = vec![a::ConsoleInputEvent::default(); 4 * crate::input::console::CAPACITY];
    let hosted = hosted::State::new(&mut keys).unwrap();
    let mut run = [0; 128];
    let mut shell = State::new(&mut run).unwrap();
    shell.config.background = background;
    let input = input::State::default();
    let tray = Mirror::new(a::ProgramProcessHandle {
        instance_id: 1,
        generation: 1,
        ..Default::default()
    })
    .unwrap();
    let status = Status::default();
    let mut scene = views::scene(
        640,
        480,
        &hosted,
        &shell,
        &input,
        &tray,
        &status,
        &FrameStats::default(),
        false,
        0,
        false,
    );
    scene.path = crate::render::coverage::Path::Native;
    let mut fonts = FontCache::new().unwrap();
    let mut icons = icons::Cache::new().unwrap();
    let mut loader = icons::Native { app };
    let mut paint = icons::Paint {
        cache: &mut icons,
        loader: &mut loader,
    };
    let view = crate::render::output_geometry::topology::Viewport {
        pixel_w: 8,
        pixel_h: 8,
        ..Default::default()
    };
    // A new output still needs its complete scene when only one logical
    // pixel was invalidated. Final composition keeps that damage separately.
    unsafe {
        owner.capture(
            windows,
            if primary {
                Mode::Desktop(Rect::new(0, 0, 8, 8))
            } else {
                Mode::NativeOutput(view)
            },
            &scene,
            &[Rect::new(2, 2, 1, 1)],
            &mut fonts,
            &mut Builtin,
            &mut paint,
            &mut Renderer::default(),
        )
    }
    .unwrap();
    let state = owner.state.as_deref().unwrap();
    assert!(state.cache.complete_scene);
    assert_eq!(state.cache.commands().len(), 1);
    assert_eq!(state.cache.commands()[0].scissor, Rect::new(0, 0, 8, 8));
}
#[test]
fn actual_cold_output_join_cancel_and_creation_refusal_keep_complete_state() {
    fixture::with(|parent, device| {
        {
            let s = device.bind(parent).unwrap();
            fixture::control(&s, 1, 0);
            fixture::control(&s, 4, 1);
            fixture::control(&s, 5, 1);
            fixture::control(&s, 6, 1);
        }
        let parent_raw = parent.start_context();
        let count = parent_raw.import_count as usize;
        let mut imports = unsafe {
            core::slice::from_raw_parts((*parent_raw).imports as *const a::R4XStartImport, count)
        }
        .to_vec();
        let mut sys = Box::new(unsafe { *(imports[0].table as *const a::R4XStartR4Sys) });
        sys.thread_create_handle = create as *const () as usize;
        sys.thread_handle_join = join as *const () as usize;
        sys.monotonic_clock = precise as *const () as usize;
        imports[0].table = &*sys as *const _ as u64;
        let mut raw = *parent_raw;
        raw.imports = imports.as_ptr() as u64;
        let app = unsafe { App::from_raw(&raw, &EXPECTED, c"OUTWORK") }.unwrap();
        let resources = Resources::new(&app).unwrap();
        let mut storage = pin!(r4os_sdk::graphics::GraphicsStorage::new());
        let graphics = Graphics::open(storage.as_mut(), &app).unwrap();
        let mut windows = Set::<_, Consumer>::new(NativeMemory(&graphics));
        let mut owner = unsafe { Output::open(&app, 0, None, None, g::format_xrgb8888) }
            .unwrap_or_else(|_| panic!("original device open"));
        scene(&app, &mut owner, &mut windows, false, 0x882244);
        assert!(unsafe { owner.start(&app, &resources, &EXPECTED, 1) });
        assert!(owner.state.is_none());
        MODE.store(1, Ordering::Release);
        until("cold preparation", || {
            owner.preparation.as_ref().unwrap().finished()
        });
        assert!(!owner.collect(&app, true));
        assert!(owner.state.is_none());
        MODE.store(0, Ordering::Release);
        until("first visible frame", || {
            let progress = owner.poll(&app, &mut windows, &graphics, 1);
            assert_ne!(progress, Progress::Failed);
            owner.frames_completed != 0
        });
        assert!(owner.preparation.is_none());
        assert!(owner.state.is_some());
        assert!(owner.frames_visible != 0);
        assert_eq!(fixture::visible(), [0x882244; 64]);
        // A visible front still owns its scanout lease. The original swapchain
        // nevertheless admits a distinct free back buffer for the next frame.
        assert!(owner.busy());
        assert!(owner.state.as_ref().unwrap().engine.pending());
        assert!(!owner.state.as_ref().unwrap().engine.active());
        let next_time = 1_000_000_000;
        assert!(owner.available(&app, 2, next_time, 0));
        scene(&app, &mut owner, &mut windows, false, 0x445566);
        assert!(unsafe { owner.start(&app, &resources, &EXPECTED, next_time) });
        until("second visible frame with retained front", || {
            let progress = owner.poll(&app, &mut windows, &graphics, next_time);
            assert_ne!(progress, Progress::Failed);
            owner.frames_completed == 2
        });
        assert_eq!(fixture::visible(), [0x445566; 64]);
        assert!(owner.busy(), "the visible front remains a real lifetime/close barrier");
        let observation = owner.execution_observation();
        assert_eq!(observation.completed_frames, 2);
        assert_eq!(observation.visible_frames, 2);
        assert!(observation.primitive_submit_jobs.unwrap() >= 2);
        assert_eq!(observation.result, Some(1));
        assert!(observation.visible_ns.is_some());
        until("first close", || {
            owner.close(&app, &mut windows, &graphics, 1)
        });
        assert!(owner.state.is_none());
        // The same original FIFO and complete pixel oracle also exercise the
        // ordinary single-display capture without a connector transformation.
        let mut primary_frame = unsafe { Output::open(&app, 0, None, None, g::format_xrgb8888) }
            .unwrap_or_else(|_| panic!("primary device"));
        scene(&app, &mut primary_frame, &mut windows, true, 0x882244);
        assert!(unsafe { primary_frame.start(&app, &resources, &EXPECTED, 1) });
        until("primary visible", || {
            let progress = primary_frame.poll(&app, &mut windows, &graphics, 1);
            assert_ne!(progress, Progress::Failed);
            primary_frame.frames_completed != 0
        });
        assert_eq!(fixture::visible(), [0x882244; 64]);
        until("primary close", || {
            primary_frame.close(&app, &mut windows, &graphics, 1)
        });
        // Missing display-presentation capability selects CPU, while real
        // device state drains before managed output admission/recreation.
        let mut route = crate::outputs::primary::Primary::default();
        unsafe {
            route.poll(&app, &graphics, &mut windows, false, 1);
        }
        assert!(route.output().is_some());
        assert!(route.admit(&app, &graphics, 1, 1, 0) == crate::outputs::primary::Admission::Cpu);
        let mut manager = crate::outputs::manager::Manager::new().unwrap();
        assert!(!manager.owns_output_route());
        manager.slots[0].target = a::GfxOutputTarget { adapter_id: 1, connector_id: 2048,
            device_generation: 2, connection_generation: 9, display_generation: 2, ..Default::default() };
        assert!(!manager.active());
        assert!(manager.owns_output_route(), "temporarily empty layout still owns its bound output");
        unsafe {
            route.poll(&app, &graphics, &mut windows, manager.owns_output_route(), 1);
        }
        assert!(route.output().is_none());
        unsafe {
            route.poll(&app, &graphics, &mut windows, false, 1);
        }
        assert!(route.output().is_some());
        until("primary route close", || {
            route.close(&app, &graphics, &mut windows, 1)
        });
        // Stop while the real native thread is still parked before entry.
        // A stop request and even completed-but-unjoined work own no free.
        let mut cancelled = unsafe { Output::open(&app, 0, None, None, g::format_xrgb8888) }
            .unwrap_or_else(|_| panic!("cancel device"));
        scene(&app, &mut cancelled, &mut windows, false, 0x882244);
        ALLOW.store(false, Ordering::Release);
        assert!(unsafe { cancelled.start(&app, &resources, &EXPECTED, 1) });
        assert!(!cancelled.close(&app, &mut windows, &graphics, 1));
        assert!(cancelled.state.is_none());
        ALLOW.store(true, Ordering::Release);
        MODE.store(1, Ordering::Release);
        until("cancelled preparation", || {
            cancelled.preparation.as_ref().unwrap().finished()
        });
        assert!(!cancelled.close(&app, &mut windows, &graphics, 1));
        assert!(cancelled.state.is_none());
        MODE.store(0, Ordering::Release);
        until("cancelled close", || {
            cancelled.close(&app, &mut windows, &graphics, 1)
        });
        assert_eq!(cancelled.frames_completed, 0);
        // Native create refusal returns the same captured state to its owner.
        let mut refused = unsafe { Output::open(&app, 0, None, None, g::format_xrgb8888) }
            .unwrap_or_else(|_| panic!("refusal device"));
        scene(&app, &mut refused, &mut windows, false, 0x882244);
        MODE.store(3, Ordering::Release);
        assert!(!unsafe { refused.start(&app, &resources, &EXPECTED, 1) });
        assert!(refused.state.is_some());
        assert!(refused.preparation.is_none());
        MODE.store(0, Ordering::Release);
        until("refused close", || {
            refused.close(&app, &mut windows, &graphics, 1)
        });
        assert!(windows.detached());
        assert!(THREAD.lock().unwrap().is_none());
        assert_eq!(&fixture::stats()[9..13], &[0, 0, 0, 0]);
    });
}
