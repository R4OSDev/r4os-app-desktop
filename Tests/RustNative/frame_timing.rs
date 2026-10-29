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
}
static CASE: Mutex<()> = Mutex::new(());
static PROBE: Mutex<Probe> = Mutex::new(Probe {
    clocks: 0,
    fail_clock: 0,
    begins: 0,
    presents: 0,
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
    assert_eq!((x, y, w, h), (0, 0, 32, 32));
    PROBE.lock().unwrap().begins += 1;
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
    assert_eq!(
        (r.source_width, r.source_height, r.source_stride_pixels),
        (32, 32, 32)
    );
    assert_eq!(r.input_tick, 77);
    assert_eq!((count, damage_count), (1024, 1));
    let d = unsafe { &*damage };
    assert_eq!((d.x, d.y, d.w, d.h), (0, 0, 32, 32));
    assert!(
        unsafe { core::slice::from_raw_parts(pixels, count as usize) }
            .iter()
            .all(|x| x & 0xffffff == 0)
    );
    PROBE.lock().unwrap().presents += 1;
    unsafe {
        out.write(a::DisplayPresentResult {
            present_generation: r.source_generation,
            ..Default::default()
        })
    };
    0
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
    let mut scene = Scene {
        width: 32,
        height: 32,
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
            &[Rect::new(0, 0, 32, 32)],
            77,
            trace,
        )
    }
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
