//! Calls the actual ordinary Desktop router with a native SDK host session.
//! It does not replace routing with a second transition model.
use super::*;
use std::sync::Mutex;
static GUI_EVENTS: Mutex<Vec<a::GuiEvent>> = Mutex::new(Vec::new());
static GUI_BLOCKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
unsafe extern "C" fn gui_push(instance: u32, event: *const a::GuiEvent) -> i32 {
    assert_eq!(instance, 12);
    GUI_EVENTS.lock().unwrap().push(unsafe { *event });
    0
}
unsafe extern "C" fn gui_host_input(
    request: *const a::GuiHostInputRequest,
    out: *mut a::GuiPointerWindowIdentity,
) -> i32 {
    let request = unsafe { *request };
    if request.window.owner.instance_id != 12 || request.window.owner.generation != 55 {
        return a::gui_host_input_error_stale;
    }
    let identity = a::GuiPointerWindowIdentity {
        epoch: 91,
        ..request.window
    };
    if request.operation != a::gui_host_input_operation_query && request.window.epoch != 91 {
        return a::gui_host_input_error_stale;
    }
    if request.operation == a::gui_host_input_operation_post {
        if GUI_BLOCKED.load(std::sync::atomic::Ordering::Relaxed) {
            return a::gui_host_input_error_busy;
        }
        unsafe { gui_push(12, &request.event) };
    }
    unsafe { *out = identity };
    0
}
use r4os_sdk::{
    ImportSpec, StartupInfo,
    gui::{Role, SessionStorage},
};
fn with_router(
    case: impl FnOnce(&App<'_>, &mut Session<'_, '_>, &mut Shell<'_>, &mut Hosted<'_>, &Manager<'_>),
) {
    crate::standard_fixture::with(|_| {
        let sys = a::R4XStartR4Sys::default();
        let mut desk = a::R4XStartR4Desk::default();
        desk.gui_push_event = gui_push as *const () as _;
        desk.gui_host_input = gui_host_input as *const () as _;
        let draw = a::R4XStartR4Draw::default();
        let headers = [
            (
                1u32,
                sys.magic,
                sys.abi_version,
                sys.size,
                c"R4SYS".as_ptr(),
                &sys as *const _ as usize,
            ),
            (
                2,
                desk.magic,
                desk.abi_version,
                desk.size,
                c"R4DESK".as_ptr(),
                &desk as *const _ as usize,
            ),
            (
                3,
                draw.magic,
                draw.abi_version,
                draw.size,
                c"R4DRAW".as_ptr(),
                &draw as *const _ as usize,
            ),
        ];
        let imports: [a::R4XStartImport; 3] = core::array::from_fn(|i| {
            let h = headers[i];
            a::R4XStartImport {
                group_id: h.0,
                min_version: 1,
                resolved_version: h.2,
                flags: a::r4xstart_import_flag_group_interface,
                module_name: h.4 as _,
                symbol_name: c"Query".as_ptr() as _,
                table: h.5 as _,
                ..Default::default()
            }
        });
        let names: [&[u8]; 3] = [b"R4SYS", b"R4DESK", b"R4DRAW"];
        let specs: [ImportSpec; 3] = core::array::from_fn(|i| {
            let h = headers[i];
            ImportSpec {
                module_name: names[i],
                symbol_name: b"Query",
                minimum: 1,
                optional: false,
                group_id: h.0,
                magic: h.1,
                version: h.2,
                size: h.3,
            }
        });
        let raw = a::R4XStartContext {
            instance_id: 30,
            app_class: a::R4XStartAppClass::console.0 as _,
            flags: a::r4xstart_flag_imports_valid,
            imports: imports.as_ptr() as _,
            import_count: 3,
            ..Default::default()
        };
        let expected = StartupInfo {
            app_class: a::R4XStartAppClass::console,
            required_groups: 14,
            imports: &specs,
        };
        let app = unsafe { App::from_raw(&raw, &expected, c"POINTER30") }.unwrap();
        let mut storage = std::pin::pin!(SessionStorage::new());
        let mut timers = [];
        let mut session =
            Session::open(storage.as_mut(), &app, Role::DesktopHost, &mut timers).unwrap();
        let mut shell_input = [0; 128];
        let mut shell = Shell::new(&mut shell_input).unwrap();
        let mut console_input =
            vec![a::ConsoleInputEvent::default(); 4 * crate::input::console::CAPACITY];
        let mut hosted = Hosted::new(&mut console_input).unwrap();
        let outputs = Manager::new().unwrap();
        case(&app, &mut session, &mut shell, &mut hosted, &outputs);
    });
}
fn sample(x: i32, y: i32, buttons: u8, wheel: i32) -> a::Mouse {
    a::Mouse {
        present: 1,
        x,
        y,
        buttons,
        wheel,
        dx: 0,
        dy: 0,
        reserved: 0,
        packets: 1,
    }
}
fn held(input: &mut State, remote: bool, buttons: u8) {
    if remote {
        input.remote_buttons = buttons
    } else {
        input.physical_buttons = buttons
    }
}
#[test]
fn wheel_down_motion_and_release_finish_volume_drag_for_each_source() {
    with_router(|app, session, shell, hosted, outputs| {
        for remote in [false, true] {
            let mut router = Router::default();
            let mut input = State::default();
            let mut dirty = Dirty::default();
            let registry = crate::shell::tray::Registry::new().unwrap();
            shell.audio.ui.popup_open = true;
            shell.audio.dragging = false;
            let down = router.pointer(
                &mut input,
                shell,
                hosted,
                outputs,
                app,
                session,
                &mut dirty,
                Target::VolumePopupSlider,
                Identity::default(),
                &registry,
                sample(111, 222, 1, -1),
                remote,
                false,
                1280,
                720,
                100,
                1000,
            );
            assert!(shell.audio.dragging);
            assert!(matches!(down.wheel, Some(PointerEffect::VolumeStep(-1))));
            assert!(matches!(
                down.primary,
                Action::VolumePointer { commit: false, .. }
            ));
            let moving = router.pointer(
                &mut input,
                shell,
                hosted,
                outputs,
                app,
                session,
                &mut dirty,
                Target::VolumePopupSlider,
                Identity::default(),
                &registry,
                sample(144, 222, 1, 2),
                remote,
                false,
                1280,
                720,
                101,
                1000,
            );
            assert!(matches!(
                moving.primary,
                Action::VolumePointer {
                    x: 144,
                    commit: false
                }
            ));
            let up = router.pointer(
                &mut input,
                shell,
                hosted,
                outputs,
                app,
                session,
                &mut dirty,
                Target::VolumePopupSlider,
                Identity::default(),
                &registry,
                sample(166, 222, 0, 1),
                remote,
                false,
                1280,
                720,
                102,
                1000,
            );
            assert!(
                !shell.audio.dragging,
                "wheel-bearing Up must finish existing volume drag"
            );
            assert_eq!(input.pressed, Target::None);
            assert!(matches!(up.wheel, Some(PointerEffect::VolumeStep(1))));
            assert!(matches!(
                up.primary,
                Action::VolumePointer {
                    x: 166,
                    commit: true
                }
            ));
            let next = router.pointer(
                &mut input,
                shell,
                hosted,
                outputs,
                app,
                session,
                &mut dirty,
                Target::None,
                Identity::default(),
                &registry,
                sample(180, 222, 0, 0),
                remote,
                false,
                1280,
                720,
                103,
                1000,
            );
            assert!(matches!(next.primary, Action::None));
            assert!(!shell.audio.dragging);
        }
    });
}
#[test]
fn wheel_snapshots_move_and_release_actual_window_drag_without_later_motion() {
    with_router(|app, session, shell, hosted, outputs| {
        for remote in [false, true] {
            let mut router = Router::default();
            let mut input = State::default();
            let mut dirty = Dirty::default();
            let registry = crate::shell::tray::Registry::new().unwrap();
            shell.audio.ui.popup_open = false;
            hosted
                .model
                .windows
                .window_mut(0)
                .unwrap()
                .bind_console(10, c"Console");
            let frame = hosted.model.windows.windows()[0].geometry();
            let (x, y) = (frame.x + 40, frame.y + 10);
            held(&mut input, remote, 1);
            input.pressed = Controller::window_target(0);
            hosted.model.windows.pointer_down(0, x, y);
            assert_eq!(hosted.model.windows.pointer_window(), Some(0));
            router.pointer(
                &mut input,
                shell,
                hosted,
                outputs,
                app,
                session,
                &mut dirty,
                Controller::window_target(0),
                Identity::default(),
                &registry,
                sample(x + 37, y + 18, 1, 1),
                remote,
                false,
                1280,
                720,
                200,
                1000,
            );
            assert_eq!(hosted.model.windows.windows()[0].geometry().x, frame.x + 37);
            router.pointer(
                &mut input,
                shell,
                hosted,
                outputs,
                app,
                session,
                &mut dirty,
                Controller::window_target(0),
                Identity::default(),
                &registry,
                sample(x + 51, y + 23, 0, -1),
                remote,
                false,
                1280,
                720,
                201,
                1000,
            );
            assert_eq!(hosted.model.windows.pointer_window(), None);
            assert_eq!(input.pressed, Target::None);
            let end = hosted.model.windows.windows()[0].geometry();
            assert_eq!(end.x, frame.x + 51);
            router.pointer(
                &mut input,
                shell,
                hosted,
                outputs,
                app,
                session,
                &mut dirty,
                Target::None,
                Identity::default(),
                &registry,
                sample(x + 100, y + 100, 0, 0),
                remote,
                false,
                1280,
                720,
                202,
                1000,
            );
            assert_eq!(hosted.model.windows.windows()[0].geometry(), end);
        }
    });
}
#[test]
fn wheel_release_finishes_real_icon_layout_drag_and_returns_its_commit() {
    with_router(|app, session, shell, hosted, outputs| {
        assert!(shell.folder.items.add(desktop_items::Item::new(
            b"C:\\A.R4X",
            desktop_items::FsKind::File
        )));
        for remote in [false, true] {
            let mut router = Router::default();
            let mut input = State::default();
            let mut dirty = Dirty::default();
            let registry = crate::shell::tray::Registry::new().unwrap();
            let item = &shell.folder.items.entries()[0];
            let start = Point {
                x: item.x,
                y: item.y,
            };
            let mouse = Point {
                x: start.x + 5,
                y: start.y + 5,
            };
            router.item = Some(ItemDrag {
                index: 0,
                mouse,
                start,
                active: true,
            });
            shell.item_drag = Some(0);
            held(&mut input, remote, 1);
            input.pressed = Target::DesktopIcon1;
            let up = router.pointer(
                &mut input,
                shell,
                hosted,
                outputs,
                app,
                session,
                &mut dirty,
                Target::None,
                Identity::default(),
                &registry,
                sample(mouse.x + 120, mouse.y + 80, 0, 1),
                remote,
                false,
                1280,
                720,
                300,
                1000,
            );
            assert!(matches!(up.primary, Action::LayoutChanged));
            assert!(router.item.is_none());
            assert_eq!(shell.item_drag, None);
            assert_eq!(input.pressed, Target::None);
        }
    });
}
#[test]
fn one_wheel_packet_preserves_both_tray_button_releases_exactly_once() {
    with_router(|app, session, shell, hosted, outputs| {
        for remote in [false, true] {
            let mut router = Router::default();
            let mut input = State::default();
            let mut dirty = Dirty::default();
            let registry = crate::shell::tray::Registry::new().unwrap();
            let identity = Identity {
                owner: a::ProgramProcessHandle {
                    instance_id: 112,
                    reserved: 0,
                    generation: 7,
                },
                item_id: 19,
                serial: 1,
            };
            held(&mut input, remote, 3);
            input.right_tray = identity;
            input.tray_pressed = identity;
            input.pressed = Target::None;
            let up = router.pointer(
                &mut input,
                shell,
                hosted,
                outputs,
                app,
                session,
                &mut dirty,
                Target::None,
                identity,
                &registry,
                sample(1100, 700, 0, -2),
                remote,
                false,
                1280,
                720,
                400,
                1000,
            );
            assert!(
                matches!(up.wheel,Some(PointerEffect::Tray{kind,wheel:-2,..}) if kind==a::tray_event_kind_wheel)
            );
            assert!(
                matches!(up.secondary,Some(PointerEffect::Tray{kind,..}) if kind==a::tray_event_kind_context)
            );
            assert!(matches!(up.primary,Action::Tray{kind,..} if kind==a::tray_event_kind_primary));
            assert!(!input.right_tray.valid());
            assert!(!input.tray_pressed.valid());
            let only_wheel = router.pointer(
                &mut input,
                shell,
                hosted,
                outputs,
                app,
                session,
                &mut dirty,
                Target::None,
                identity,
                &registry,
                sample(1100, 700, 0, 1),
                remote,
                false,
                1280,
                720,
                401,
                1000,
            );
            assert!(only_wheel.wheel.is_some());
            assert!(only_wheel.secondary.is_none());
            assert!(matches!(only_wheel.primary, Action::None));
        }
    });
}
#[test]
fn wheel_down_and_up_reach_the_original_native_gui_delivery() {
    with_router(|app, session, shell, hosted, outputs| {
        let owner = a::ProgramProcessHandle {
            instance_id: 12,
            generation: 55,
            reserved: 0,
        };
        assert!(hosted.model.clients.slot_mut(2).unwrap().bind(
            owner,
            crate::hosted::client::Origin::Diagnostic,
            &[],
            &mut crate::hosted::frame_reader::SessionReader::new(session)
        ));
        for remote in [false, true] {
            GUI_EVENTS.lock().unwrap().clear();
            let mut router = Router::default();
            let mut input = State::default();
            let mut dirty = Dirty::default();
            let registry = crate::shell::tray::Registry::new().unwrap();
            hosted
                .model
                .windows
                .window_mut(2)
                .unwrap()
                .bind_app(12, c"GUI");
            let c = hosted.model.windows.windows()[2].client_surface().rect;
            let target = Controller::window_target(2);
            router.pointer(
                &mut input,
                shell,
                hosted,
                outputs,
                app,
                session,
                &mut dirty,
                target,
                Identity::default(),
                &registry,
                sample(c.x + 10, c.y + 11, 1, 1),
                remote,
                false,
                1280,
                720,
                500,
                1000,
            );
            router.pointer(
                &mut input,
                shell,
                hosted,
                outputs,
                app,
                session,
                &mut dirty,
                target,
                Identity::default(),
                &registry,
                sample(c.x + 12, c.y + 13, 0, -1),
                remote,
                false,
                1280,
                720,
                501,
                1000,
            );
            let e = GUI_EVENTS.lock().unwrap();
            let mouse: Vec<_> = e
                .iter()
                .filter(|e| {
                    e.kind == a::GuiEventKind::mouse_down.0 || e.kind == a::GuiEventKind::mouse_up.0
                })
                .collect();
            assert_eq!(mouse.len(), 2);
            assert_eq!(
                (mouse[0].kind, mouse[0].x, mouse[0].y, mouse[0].buttons),
                (a::GuiEventKind::mouse_down.0, 10, 11, 1)
            );
            assert_eq!(
                (mouse[1].kind, mouse[1].x, mouse[1].y, mouse[1].buttons),
                (a::GuiEventKind::mouse_up.0, 12, 13, 0)
            );
        }
    });
}

#[test]
fn an_owned_logical_key_stays_gui_owned_when_the_actual_native_admission_is_busy() {
    with_router(|app, session, _, hosted, _| {
        let owner = a::ProgramProcessHandle {
            instance_id: 12,
            generation: 55,
            reserved: 0,
        };
        assert!(hosted.model.clients.slot_mut(2).unwrap().bind(
            owner,
            crate::hosted::client::Origin::Diagnostic,
            &[],
            &mut crate::hosted::frame_reader::SessionReader::new(session)
        ));
        hosted
            .model
            .windows
            .window_mut(2)
            .unwrap()
            .bind_app(12, c"GUI");
        hosted.model.windows.focus_or_restore(2);
        GUI_EVENTS.lock().unwrap().clear();
        GUI_BLOCKED.store(true, std::sync::atomic::Ordering::Relaxed);
        let handled = hosted.gui_key(
            app,
            session,
            r4os_sdk::gui::raw::R4SdkGuiKey_escape as u32,
            100,
        );
        GUI_BLOCKED.store(false, std::sync::atomic::Ordering::Relaxed);
        assert!(
            handled,
            "busy admission is still GUI ownership; no shell fallback"
        );
        assert!(GUI_EVENTS.lock().unwrap().is_empty());
        assert!(hosted.model.gui_pending[2].busy());
        assert!(hosted.flush_gui(session));
        let received = GUI_EVENTS.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(
            (received[0].kind, received[0].key, received[0].tick),
            (
                a::GuiEventKind::key_down.0,
                r4os_sdk::gui::raw::R4SdkGuiKey_escape as u32,
                100
            )
        );
    });
}
