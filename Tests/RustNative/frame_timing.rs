//! Actual CPU Frames -> SDK -> native clock/present boundary. The existing
//! independent complete scene/pixel oracles remain in scene_pixels.rs.
use super::*;
use crate::{
    hosted::{
        client::Clients, console_snapshot::Snapshot, text_surface::Surface,
        window_image::NativeMemory,
    },
    input::event::Target,
    render::software::tests::OriginalColors,
    shell::{
        configuration::Config,
        desktop_items::Items,
        menu_controller,
        quick_launch::Bar,
        start_menu::Menu,
        tray::{Identity, Registry},
        volume,
    },
    windows::controller::Controller,
};
use r4os_contract as a;
use r4os_sdk::{
    ImportSpec, StartupInfo,
    graphics::GraphicsStorage,
    gui::{Role, SessionStorage},
};
use std::{pin::pin, sync::Mutex};

#[derive(Default)]
struct Probe {
    clocks: u32,
    fail_clock: u32,
    begins: u32,
    presents: u32,
    shape: Option<Shape>,
    canonical_status: i32,
    copy_status: i32,
    legacy_status: i32,
    blits: u32,
    legacy_presents: u32,
    front: Vec<u32>,
}
#[derive(Clone, Copy)]
struct Shape {
    width: u32,
    height: u32,
    bounds: Rect,
    regions: u32,
}
const DEFAULT_SHAPE: Shape = Shape {
    width: 32,
    height: 32,
    bounds: Rect::new(0, 0, 32, 32),
    regions: 1,
};
static CASE: Mutex<()> = Mutex::new(());
static PROBE: Mutex<Probe> = Mutex::new(Probe {
    clocks: 0,
    fail_clock: 0,
    begins: 0,
    presents: 0,
    shape: None,
    canonical_status: 0,
    copy_status: 0,
    legacy_status: 0,
    blits: 0,
    legacy_presents: 0,
    front: Vec::new(),
});
unsafe extern "C" fn precise(out: *mut a::MonotonicClockInfo) -> i32 {
    let mut p = PROBE.lock().unwrap();
    p.clocks += 1;
    if p.clocks == p.fail_clock {
        return 0;
    }
    unsafe {
        out.write(a::MonotonicClockInfo {
            flags: a::monotonic_clock_flag_valid,
            instant_ns: u64::from(p.clocks) * 10,
            event_requested_hz: 1000,
            event_effective_hz: 1000,
            ..Default::default()
        })
    };
    1
}
unsafe extern "C" fn begin(x: i32, y: i32, w: u32, h: u32) -> i32 {
    let mut p = PROBE.lock().unwrap();
    let e = p.shape.unwrap_or(DEFAULT_SHAPE);
    assert_eq!(
        (x, y, w, h),
        (e.bounds.x, e.bounds.y, e.bounds.w as u32, e.bounds.h as u32)
    );
    p.begins += 1;
    0
}
unsafe extern "C" fn present(
    request: *const a::DisplayPresentRequest,
    pixels: *const u32,
    count: u32,
    damage: *const a::DisplayDamageRect,
    damage_count: u32,
    out: *mut a::DisplayPresentResult,
) -> i32 {
    let r = unsafe { &*request };
    let e = PROBE.lock().unwrap().shape.unwrap_or(DEFAULT_SHAPE);
    assert_eq!(
        (r.source_width, r.source_height, r.source_stride_pixels),
        (e.width, e.height, e.width)
    );
    assert_eq!(r.input_tick, 77);
    assert_eq!((count, damage_count), (e.width * e.height, e.regions));
    let regions = unsafe { core::slice::from_raw_parts(damage, damage_count as usize) };
    // Keep the original exact full32x32 oracle for the four timing cases;
    // larger Damage cases must also carry exactly the expected wire bounds.
    let left = regions.iter().map(|r| i64::from(r.x)).min().unwrap();
    let top = regions.iter().map(|r| i64::from(r.y)).min().unwrap();
    let right = regions
        .iter()
        .map(|r| i64::from(r.x) + i64::from(r.w))
        .max()
        .unwrap();
    let bottom = regions
        .iter()
        .map(|r| i64::from(r.y) + i64::from(r.h))
        .max()
        .unwrap();
    assert_eq!(
        (left, top, right - left, bottom - top),
        (
            i64::from(e.bounds.x),
            i64::from(e.bounds.y),
            i64::from(e.bounds.w),
            i64::from(e.bounds.h)
        )
    );
    for (i, a) in regions.iter().enumerate() {
        assert!(a.w > 0 && a.h > 0);
        for b in &regions[i + 1..] {
            assert!(
                !(i64::from(a.x) < i64::from(b.x) + i64::from(b.w)
                    && i64::from(b.x) < i64::from(a.x) + i64::from(a.w)
                    && i64::from(a.y) < i64::from(b.y) + i64::from(b.h)
                    && i64::from(b.y) < i64::from(a.y) + i64::from(a.h)),
                "overlap reached native present"
            );
        }
    }
    assert!(
        unsafe { core::slice::from_raw_parts(pixels, count as usize) }
            .iter()
            .all(|x| x & 0xffffff == 0)
    );
    {
        let mut p = PROBE.lock().unwrap();
        p.presents += 1;
        if p.canonical_status != 0 {
            unsafe {
                out.write(a::DisplayPresentResult {
                    source_generation: r.source_generation,
                    present_generation: 999,
                    ..Default::default()
                });
            }
            return p.canonical_status;
        }
        let source = unsafe { core::slice::from_raw_parts(pixels, count as usize) };
        p.front.resize(source.len(), 0x336699);
        for region in regions {
            for y in region.y as usize..region.y as usize + region.h as usize {
                let start = y * r.source_stride_pixels as usize + region.x as usize;
                let end = start + region.w as usize;
                p.front[start..end].copy_from_slice(&source[start..end]);
            }
        }
    }
    unsafe {
        out.write(a::DisplayPresentResult {
            present_generation: r.source_generation,
            ..Default::default()
        })
    };
    0
}
unsafe extern "C" fn legacy_blit(
    _x: i32,
    _y: i32,
    _w: u32,
    _h: u32,
    pixels: *const u32,
    count: u32,
    _stride: u32,
) -> i32 {
    let mut p = PROBE.lock().unwrap();
    p.blits += 1;
    if p.copy_status >= 0 {
        p.front = unsafe { core::slice::from_raw_parts(pixels, count as usize) }.to_vec();
    }
    p.copy_status
}
unsafe extern "C" fn legacy_present() -> i32 {
    let mut p = PROBE.lock().unwrap();
    p.legacy_presents += 1;
    p.legacy_status
}
fn with(case: impl FnOnce(&App<'_>, &mut Session<'_, '_>, &Graphics<'_, '_>)) {
    let _case = CASE.lock().unwrap();
    *PROBE.lock().unwrap() = Probe::default();
    let sys = a::R4XStartR4Sys {
        monotonic_clock: precise as *const () as usize,
        ..Default::default()
    };
    let desk = a::R4XStartR4Desk::default();
    let draw = a::R4XStartR4Draw {
        display_begin_frame_rect: begin as *const () as usize,
        display_present_regions: present as *const () as usize,
        display_blit_xrgb32_stride: legacy_blit as *const () as usize,
        display_present: legacy_present as *const () as usize,
        ..Default::default()
    };
    let tables = [
        (
            1,
            c"R4SYS",
            &sys as *const _ as u64,
            sys.magic,
            sys.abi_version,
            sys.size,
        ),
        (
            2,
            c"R4DESK",
            &desk as *const _ as u64,
            desk.magic,
            desk.abi_version,
            desk.size,
        ),
        (
            3,
            c"R4DRAW",
            &draw as *const _ as u64,
            draw.magic,
            draw.abi_version,
            draw.size,
        ),
    ];
    let imports = tables.map(|(id, name, table, _, _, _)| a::R4XStartImport {
        group_id: id,
        min_version: 1,
        resolved_version: 1,
        flags: a::r4xstart_import_flag_group_interface,
        module_name: name.as_ptr() as u64,
        symbol_name: c"Query".as_ptr() as u64,
        table,
        ..Default::default()
    });
    let specs = tables.map(|(id, name, _, magic, version, size)| ImportSpec {
        module_name: name.to_bytes(),
        symbol_name: b"Query",
        minimum: 1,
        optional: false,
        group_id: id,
        magic,
        version,
        size,
    });
    let raw = a::R4XStartContext {
        app_class: a::R4XStartAppClass::gui.0,
        instance_id: 88,
        flags: a::r4xstart_flag_imports_valid,
        imports: imports.as_ptr() as u64,
        import_count: 3,
        ..Default::default()
    };
    let app = unsafe {
        App::from_raw(
            &raw,
            &StartupInfo {
                app_class: a::R4XStartAppClass::gui,
                required_groups: 14,
                imports: &specs,
            },
            c"FRAMEFIX",
        )
    }
    .unwrap();
    let mut graphics_storage = pin!(GraphicsStorage::new());
    let graphics = Graphics::open(graphics_storage.as_mut(), &app).unwrap();
    let mut session_storage = pin!(SessionStorage::new());
    let mut timers = [];
    let mut session = Session::open(
        session_storage.as_mut(),
        &app,
        Role::DesktopHost,
        &mut timers,
    )
    .unwrap();
    case(&app, &mut session, &graphics);
}
fn frame(
    owner: &mut Frames,
    app: &App<'_>,
    session: &mut Session<'_, '_>,
    graphics: &Graphics<'_, '_>,
    trace: bool,
) -> Result<Frame> {
    frame_damage(
        owner,
        app,
        session,
        graphics,
        trace,
        &[Rect::new(0, 0, 32, 32)],
    )
}
fn frame_damage(
    owner: &mut Frames,
    app: &App<'_>,
    session: &mut Session<'_, '_>,
    graphics: &Graphics<'_, '_>,
    trace: bool,
    regions: &[Rect],
) -> Result<Frame> {
    use compositor::{Hosted, Interaction, Overlay, Shell};
    let controller = Controller::default();
    let clients = Clients::default();
    let consoles = core::array::from_fn(|_| Snapshot::new().unwrap());
    let texts = core::array::from_fn(|_| Surface::default());
    let config = Config::default();
    let items = Items::new().unwrap();
    let quick = Bar::defaults();
    let menu = Menu::defaults();
    let tray = Registry::new().unwrap();
    let e = PROBE.lock().unwrap().shape.unwrap_or(DEFAULT_SHAPE);
    let mut scene = Scene {
        width: e.width as i32,
        height: e.height as i32,
        path: Path::PrimaryCpu,
        disabled: true,
        hosted: Hosted {
            windows: &controller,
            clients: &clients,
            consoles: &consoles,
            texts: &texts,
            console_title: b"",
            console_path: b"",
            console_args: b"",
            font: a::gui_font_builtin_id,
            terminal_mode: false,
            blink: false,
        },
        shell: Shell {
            config: &config,
            items: &items,
            quick: &quick,
            menu: &menu,
            menu_view: menu_controller::View::default(),
            volume: volume::View::default(),
            tray: &tray,
            info: b"",
            clock: None,
            keyboard: None,
            wallpaper: None,
            font: a::gui_font_builtin_id,
        },
        input: Interaction {
            selected: 0,
            drag: None,
            hover: Target::None,
            pressed: Target::None,
            tray_hover: Identity::default(),
            tray_pressed: Identity::default(),
            system: None,
            time_menu: false,
            overlay: Overlay::None,
            cursor_x: 0,
            cursor_y: 0,
            cursor_visible: false,
            recording: 0,
        },
    };
    let mut windows = Set::<_, Consumer>::new(NativeMemory(graphics));
    unsafe {
        owner.primary(
            app,
            session,
            graphics,
            &OriginalColors,
            &mut windows,
            &mut scene,
            regions,
            77,
            trace,
        )
    }
}
#[test]
fn software_present_copy_failure_cannot_acknowledge_a_stale_front() {
    with(|app, session, graphics| {
        {
            let mut p = PROBE.lock().unwrap();
            p.canonical_status = a::display_present_error_unavailable;
            p.copy_status = -3;
            p.legacy_status = 0;
            p.front = vec![0x336699; 32 * 32];
        }
        let mut owner = Frames::new().unwrap();
        let mut dirty = Dirty::default();
        dirty.invalidate(DEFAULT_SHAPE.bounds);
        let mut regions = [Rect::ZERO; MAX_DAMAGE_REGIONS];
        let n = dirty.take_regions(&mut regions);
        let failed =
            frame_damage(&mut owner, app, session, graphics, false, &regions[..n]).unwrap();
        assert!(!failed.presentation.copied);
        assert_eq!(
            failed.presentation.result.source_generation,
            failed.generation
        );
        assert_eq!(failed.presentation.result.present_generation, 999);
        assert_eq!(PROBE.lock().unwrap().front, [0x336699; 32 * 32]);
        assert!(
            !failed.presentation.presented,
            "a successful last Present after rejected copies must not acknowledge a complete frame"
        );
        let mut cursor = crate::outputs::cursor::tests::pending_clean_controller();
        assert!(!crate::runtime::desktop::complete_cpu_frame(
            &mut cursor,
            &mut dirty,
            &regions[..n],
            &failed.presentation
        ));
        assert!(cursor.clean_pending() && !cursor.clean_ready());
        assert_eq!(dirty.regions(), &[DEFAULT_SHAPE.bounds]);
        assert!(
            owner.current,
            "CPU pixels remain valid independently of target acceptance"
        );
        PROBE.lock().unwrap().canonical_status = 0;
        let n = dirty.take_regions(&mut regions);
        assert_eq!(&regions[..n], &[DEFAULT_SHAPE.bounds]);
        let recovered =
            frame_damage(&mut owner, app, session, graphics, false, &regions[..n]).unwrap();
        assert_eq!((failed.generation, recovered.generation), (1, 2));
        assert!(crate::runtime::desktop::complete_cpu_frame(
            &mut cursor,
            &mut dirty,
            &regions[..n],
            &recovered.presentation
        ));
        assert!(cursor.clean_ready() && !dirty.active());
        let p = PROBE.lock().unwrap();
        assert_eq!(p.front, [0; 32 * 32]);
        assert_eq!((p.presents, p.blits, p.legacy_presents), (2, 0, 0));
        assert_eq!(
            (
                owner.stats.redraws,
                owner.stats.presents,
                owner.stats.present_errors
            ),
            (2, 1, 1)
        );
        assert_eq!(owner.stats.present_lost_frames, 0);
    });
}

#[test]
fn software_present_provider_errors_preserve_partial_damage_until_acceptance() {
    with(|app, session, graphics| {
        use crate::runtime::desktop::complete_cpu_frame;
        let pending = Rect::new(3, 4, 2, 3);
        let mut dirty = Dirty::default();
        dirty.invalidate(pending);
        let mut cursor = crate::outputs::cursor::tests::pending_clean_controller();
        let mut owner = Frames::new().unwrap();
        // A coherent previous canonical/front image. Only these six pixels
        // change to the disabled-scene black; unrelated pixels were black.
        let mut previous = vec![0; 32 * 32];
        for y in 4..7 {
            for x in 3..5 {
                previous[y * 32 + x] = 0x336699;
            }
        }
        owner.capture_shadow(&previous, 32, 32, |_| ()).unwrap();
        owner.current = true;
        PROBE.lock().unwrap().shape = Some(Shape {
            bounds: pending,
            ..DEFAULT_SHAPE
        });
        PROBE.lock().unwrap().front = previous.clone();
        let mut regions = [Rect::ZERO; MAX_DAMAGE_REGIONS];
        for (canonical, copy, present) in [
            (a::display_present_error_invalid, 0, 0),
            (a::display_present_error_out_of_range, 0, -1),
            (a::display_present_error_unavailable, -3, -1),
            (a::display_present_error_unavailable, 0, 0),
        ] {
            {
                let mut p = PROBE.lock().unwrap();
                p.canonical_status = canonical;
                p.copy_status = copy;
                p.legacy_status = present;
            }
            let n = dirty.take_regions(&mut regions);
            assert_eq!(&regions[..n], &[pending]);
            let rejected =
                frame_damage(&mut owner, app, session, graphics, false, &regions[..n]).unwrap();
            assert!(!complete_cpu_frame(
                &mut cursor,
                &mut dirty,
                &regions[..n],
                &rejected.presentation
            ));
            assert!(!cursor.clean_ready());
            assert_eq!(
                rejected.presentation.result.source_generation,
                rejected.generation
            );
            assert_eq!(owner.stats.present_lost_frames, 0);
            assert_eq!(dirty.regions(), &[pending]);
            let p = PROBE.lock().unwrap();
            assert_eq!((p.blits, p.legacy_presents), (0, 0));
            assert_eq!(p.front, previous);
        }
        // Independently reject the old partial receipt even if a future
        // transport again reports Present without a completed copy.
        let partial = crate::render::display::Presentation {
            canonical: false,
            copied: false,
            presented: true,
            result: a::DisplayPresentResult::default(),
        };
        assert!(!complete_cpu_frame(
            &mut cursor,
            &mut dirty,
            &[pending],
            &partial
        ));
        assert!(!cursor.clean_ready());
        PROBE.lock().unwrap().canonical_status = 0;
        let n = dirty.take_regions(&mut regions);
        assert_eq!(&regions[..n], &[pending]);
        let recovered =
            frame_damage(&mut owner, app, session, graphics, false, &regions[..n]).unwrap();
        assert_eq!(recovered.generation, 5);
        assert!(complete_cpu_frame(
            &mut cursor,
            &mut dirty,
            &regions[..n],
            &recovered.presentation
        ));
        assert!(cursor.clean_ready() && !dirty.active());
        let p = PROBE.lock().unwrap();
        // Six repaired pixels; every unrelated pixel remains the old front.
        assert_eq!(p.front, [0; 32 * 32]);
        assert_eq!(
            p.front
                .iter()
                .zip(&previous)
                .filter(|(a, b)| a != b)
                .count(),
            6
        );
        assert_eq!((owner.stats.presents, owner.stats.present_errors), (1, 4));
        assert_eq!(owner.stats.present_lost_frames, 0);
    });
}

#[test]
fn software_present_output_reset_replaces_previous_target_damage() {
    with(|app, session, graphics| {
        use crate::runtime::desktop::{complete_cpu_frame, reset_output_damage};
        let old_damage = Rect::new(40, 4, 2, 2);
        let new_bounds = Rect::new(0, 0, 16, 16);
        let mut regions = [Rect::ZERO; MAX_DAMAGE_REGIONS];
        // The previous full() only appended the new screen rectangle. A
        // disjoint old region survives and cannot be accepted on this target.
        PROBE.lock().unwrap().shape = Some(Shape {
            width: 16,
            height: 16,
            bounds: Rect::new(0, 0, 42, 16),
            regions: 2,
        });
        let mut old_flow = Dirty::default();
        old_flow.invalidate(old_damage);
        old_flow.invalidate(new_bounds);
        let n = old_flow.take_regions(&mut regions);
        let mut stale = Frames::new().unwrap();
        if let Ok(rejected) = frame_damage(&mut stale, app, session, graphics, false, &regions[..n])
        {
            assert!(!rejected.presentation.accepted());
        }
        assert_eq!(PROBE.lock().unwrap().presents, 0);

        // Actual old-target failure, then the real coordinator reset policy
        // and a current-sized complete accepted frame with a fresh generation.
        let mut owner = Frames::new().unwrap();
        owner.capture_shadow(&[0; 64 * 32], 64, 32, |_| ()).unwrap();
        owner.current = true;
        let mut dirty = Dirty::default();
        dirty.invalidate(old_damage);
        {
            let mut p = PROBE.lock().unwrap();
            p.shape = Some(Shape {
                width: 64,
                height: 32,
                bounds: old_damage,
                regions: 1,
            });
            p.canonical_status = a::display_present_error_unavailable;
        }
        let mut cursor = crate::outputs::cursor::Controller::default();
        let n = dirty.take_regions(&mut regions);
        let rejected =
            frame_damage(&mut owner, app, session, graphics, false, &regions[..n]).unwrap();
        assert!(!complete_cpu_frame(
            &mut cursor,
            &mut dirty,
            &regions[..n],
            &rejected.presentation
        ));
        assert_eq!(dirty.regions(), &[old_damage]);
        reset_output_damage(&mut dirty, new_bounds);
        let n = dirty.take_regions(&mut regions);
        assert_eq!(&regions[..n], &[new_bounds]);
        {
            let mut p = PROBE.lock().unwrap();
            p.shape = Some(Shape {
                width: 16,
                height: 16,
                bounds: new_bounds,
                regions: 1,
            });
            p.canonical_status = 0;
            p.front = vec![0x336699; 16 * 16];
        }
        let accepted =
            frame_damage(&mut owner, app, session, graphics, false, &regions[..n]).unwrap();
        assert_eq!((rejected.generation, accepted.generation), (1, 2));
        assert!(complete_cpu_frame(
            &mut cursor,
            &mut dirty,
            &regions[..n],
            &accepted.presentation
        ));
        assert!(!dirty.active());
        let (_, w, h, generation) = owner.pixels().unwrap();
        assert_eq!((w, h, generation), (16, 16, 2));
        assert_eq!(PROBE.lock().unwrap().front, [0; 16 * 16]);
        assert_eq!(owner.stats.present_lost_frames, 0);
    });
}

#[test]
fn normal_cpu_frame_keeps_cache_clock_without_unused_detail_reads() {
    with(|app, session, graphics| {
        let mut owner = Frames::new().unwrap();
        let result = frame(&mut owner, app, session, graphics, false).unwrap();
        assert!(result.timing.is_none());
        assert!(
            result.presentation.canonical
                && result.presentation.copied
                && result.presentation.presented
        );
        assert_eq!(result.generation, 1);
        assert_eq!(owner.stats.presents, 1);
        assert_eq!(owner.stats.redraws, 1);
        let p = PROBE.lock().unwrap();
        assert_eq!((p.begins, p.presents), (1, 1));
        assert_eq!(
            p.clocks, 1,
            "one functional icon/cache clock; detail reads require a trace"
        );
    });
}

#[test]
fn requested_trace_keeps_exact_three_times_and_next_frame_is_explicitly_unmeasured() {
    with(|app, session, graphics| {
        let mut owner = Frames::new().unwrap();
        let traced = frame(&mut owner, app, session, graphics, true).unwrap();
        let timing = traced.timing.unwrap();
        assert_eq!(
            (
                timing.started_ns,
                timing.completed_ns,
                timing.composition_ns,
                timing.present_ns
            ),
            (10, 30, 10, 10)
        );
        let pixels = owner.pixels().unwrap().0.to_vec();
        let ordinary = frame(&mut owner, app, session, graphics, false).unwrap();
        assert!(ordinary.timing.is_none());
        assert!(ordinary.presentation.presented && ordinary.presentation.copied);
        assert_eq!(ordinary.generation, 2);
        assert_eq!(owner.pixels().unwrap().0, pixels);
        assert_eq!(
            (
                owner.stats.redraws,
                owner.stats.presents,
                owner.stats.present_errors
            ),
            (2, 2, 0)
        );
        let p = PROBE.lock().unwrap();
        assert_eq!((p.clocks, p.begins, p.presents), (4, 2, 2));
    });
}

#[test]
fn an_unused_detail_clock_failure_cannot_fail_an_ordinary_present() {
    with(|app, session, graphics| {
        PROBE.lock().unwrap().fail_clock = 2;
        let mut owner = Frames::new().unwrap();
        let ordinary = frame(&mut owner, app, session, graphics, false).unwrap();
        assert!(ordinary.timing.is_none() && ordinary.presentation.presented);
        let p = PROBE.lock().unwrap();
        assert_eq!((p.clocks, p.begins, p.presents), (1, 1, 1));
    });
    with(|app, session, graphics| {
        PROBE.lock().unwrap().fail_clock = 2;
        let mut owner = Frames::new().unwrap();
        assert!(frame(&mut owner, app, session, graphics, true).is_err());
        let p = PROBE.lock().unwrap();
        assert_eq!((p.clocks, p.begins, p.presents), (2, 0, 0));
    });
}

#[test]
fn mandatory_cache_clock_and_requested_completion_clock_keep_their_errors() {
    for trace in [false, true] {
        with(|app, session, graphics| {
            PROBE.lock().unwrap().fail_clock = 1;
            let mut owner = Frames::new().unwrap();
            assert!(frame(&mut owner, app, session, graphics, trace).is_err());
            let p = PROBE.lock().unwrap();
            assert_eq!((p.clocks, p.begins, p.presents), (1, 0, 0));
        });
    }
    with(|app, session, graphics| {
        PROBE.lock().unwrap().fail_clock = 3;
        let mut owner = Frames::new().unwrap();
        assert!(frame(&mut owner, app, session, graphics, true).is_err());
        let p = PROBE.lock().unwrap();
        assert_eq!((p.clocks, p.begins, p.presents), (3, 1, 1));
    });
}

#[test]
fn dirty_capacity_merge_reaches_cpu_present_and_the_next_frame() {
    with(|app, session, graphics| {
        let mut owner = Frames::new().unwrap();
        let mut dirty = Dirty::default();
        for r in [
            Rect::new(48, 12, 2, 3),
            Rect::new(69, 44, 7, 2),
            Rect::new(12, 23, 2, 8),
            Rect::new(23, 45, 5, 2),
            Rect::new(37, 48, 6, 3),
            Rect::new(46, 56, 5, 1),
            Rect::new(27, 2, 3, 1),
            Rect::new(10, 71, 4, 4),
            Rect::new(40, 36, 4, 7),
            Rect::new(58, 69, 8, 5),
        ] {
            dirty.invalidate(r);
        }
        let mut regions = [Rect::ZERO; MAX_DAMAGE_REGIONS];
        let n = dirty.take_regions(&mut regions);
        PROBE.lock().unwrap().shape = Some(Shape {
            width: 96,
            height: 96,
            bounds: Rect::new(10, 2, 66, 73),
            regions: n as u32,
        });
        let first = frame_damage(&mut owner, app, session, graphics, false, &regions[..n]);
        assert!(
            first.is_ok(),
            "actual CPU compositor rejected delivered Dirty damage: {:?}",
            first.as_ref().err()
        );
        assert!(first.unwrap().presentation.presented);
        PROBE.lock().unwrap().shape = None;
        let next = frame(&mut owner, app, session, graphics, false).unwrap();
        assert!(next.presentation.presented && next.timing.is_none());
        assert_eq!(next.generation, 2);
        assert_eq!(
            (
                owner.stats.redraws,
                owner.stats.presents,
                owner.stats.present_errors
            ),
            (2, 2, 0)
        );
        let p = PROBE.lock().unwrap();
        assert_eq!((p.clocks, p.begins, p.presents), (2, 2, 2));
    });
}
