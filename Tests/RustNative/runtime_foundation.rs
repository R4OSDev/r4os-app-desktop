//! Actual shared native R4STD/editor calls for the runtime's new owners. The
//! large coordinator itself remains subject to the later real guest flow.
use crate::shell::{
    launch::{Associations, Launch},
    layout_writeback::Owner,
    state::State,
};
use r4os_sdk::abi as a;
#[test]
fn actual_association_copied_launch_editor_and_writeback_policy_admission() {
    for args in [
        b"".as_slice(),
        b"/CHECK-ISOLATED",
        b"/INPUTTEST",
        b"/HEADLESS-SUBSYSTEM /CHECK-ISOLATED",
    ] {
        assert!(crate::runtime::bootstrap::arguments_supported(args));
    }
    for args in [
        b"/NOHOSTTEST".as_slice(),
        b"/COMPOSITIONVERIFY",
        b"/SMOKE-WINDOW-IDLE",
        b"/CHECK-ISOLATED /CHECK-ISOLATED",
    ] {
        assert!(!crate::runtime::bootstrap::arguments_supported(args));
    }
    crate::standard_fixture::with(|standard| {
        let associations = Associations::new().unwrap();
        let direct = associations.resolve(b"C:\\APPS\\DIRECT.R4X").unwrap();
        assert_eq!(direct.path(), b"C:\\APPS\\DIRECT.R4X");
        assert!(direct.args().is_empty());
        assert_eq!(direct.policy, a::LaunchPolicy::auto);
        let file = associations.resolve(b"C:\\notes.txt").unwrap();
        assert!(file.path().ends_with(b"NOTEPAD.R4X"));
        assert_eq!(file.args(), b"C:\\notes.txt");
        let run = Launch::run(b"\"C:\\My Apps\\Editor.R4X\" /v").unwrap();
        assert_eq!(run.title(), b"Editor");
        assert_eq!(run.args(), b"/v");
        let mut input = [0; 128];
        let mut shell = State::new(&mut input).unwrap();
        assert!(!shell.blocked());
        shell.open_run();
        assert!(shell.blocked());
        shell.run.set("C:\\A\u{308}中.R4X".as_bytes()).unwrap();
        shell.run.backspace().unwrap();
        assert_eq!(shell.run.text().unwrap(), "C:\\A\u{308}中.R4".as_bytes());
        use crate::{
            input::event::Target,
            shell::{
                controller::{self, Action},
                state::{Dialog, SystemAction},
            },
        };
        use r4os_sdk::gui::raw as key;
        shell.dialog_focus = Target::RunInput;
        controller::key(&mut shell, key::R4SdkGuiKey_backspace as u32, 0);
        assert_eq!(shell.run.text().unwrap(), "C:\\A\u{308}中.R".as_bytes());
        controller::key(&mut shell, key::R4SdkGuiKey_tab as u32, 0);
        assert_eq!(shell.dialog_focus, Target::RunBrowse);
        assert!(matches!(
            controller::key(&mut shell, 13, 0),
            Some(Action::RunBrowse)
        ));
        controller::activate(&mut shell, Target::MenuTasks);
        assert_eq!(shell.dialog, Dialog::Tasks);
        assert_eq!(
            shell.dialog_rect(1280, 720),
            Some(crate::render::geometry::Rect::new(390, 150, 500, 420))
        );
        controller::key(&mut shell, key::R4SdkGuiKey_end as u32, 18);
        assert_eq!(shell.task_scroll, 13);
        controller::key(&mut shell, key::R4SdkGuiKey_home as u32, 18);
        assert_eq!(shell.task_scroll, 0);
        controller::activate(&mut shell, Target::MenuPoweroff);
        assert_eq!(shell.dialog_focus, Target::MessageNo);
        controller::key(&mut shell, key::R4SdkGuiKey_tab as u32, 0);
        assert!(matches!(
            controller::key(&mut shell, 13, 0),
            Some(Action::System(SystemAction::Poweroff))
        ));
        let mut info = [0; 160];
        assert_eq!(
            crate::shell::message_box::window_info(
                &mut info,
                crate::render::geometry::Rect::new(-18, 27, 414, 224)
            ),
            b"Window size: 414 x 224\nPosition: -18, 27"
        );
        let options = [
            r4os_sdk::subsystems::LaunchOption::new(b"T", b"0000000000000064").unwrap(),
            r4os_sdk::subsystems::LaunchOption::new(b"M", b"G").unwrap(),
            r4os_sdk::subsystems::LaunchOption::new(b"P", b"12,34,0").unwrap(),
        ];
        let mut args = [0; 128];
        let encoded =
            r4os_sdk::subsystems::encode_launch(b"C:\\TEST.BAS", &options, &mut args).unwrap();
        assert_eq!(
            r4os_sdk::subsystems::Launch::parse(encoded)
                .unwrap()
                .option_count()
                .unwrap(),
            3
        );
        let mut request = a::ProgramHostLaunchRequest::default();
        let path = b"C:\\R4OS\\SUBSYS\\R4BASIC.R4X";
        request.path[..path.len()].copy_from_slice(path);
        request.args[..encoded.len()].copy_from_slice(encoded);
        let launch = crate::hosted::launch_request::request(&request, 200).unwrap();
        assert_eq!(launch.path(), path);
        let decoded = r4os_sdk::subsystems::Launch::parse(launch.args()).unwrap();
        assert_eq!(decoded.guest().unwrap(), b"C:\\TEST.BAS");
        assert_eq!(decoded.option(b"P").unwrap().unwrap(), b"12,34,100");
        shell.close_top();
        assert!(!shell.blocked());
        let mut save = Owner::new(standard, 100).unwrap();
        assert!(!save.dirty());
        save.mark(10).unwrap();
        assert!(save.dirty());
        assert!(!save.due(9));
        assert!(save.due(u64::MAX));
        assert!(!save.busy());
        // Keep the active Spectra byte contract under the independent original
        // Zig decoder, including reserved-byte and exact-generation refusal.
        use crate::diagnostics::{reservation, wire as d};
        unsafe extern "C" {
            fn r4desk_wire_check(kind: u32, bytes: *const u8, count: usize) -> u32;
        }
        let caller = d::Identity {
            instance: 86,
            generation: 0x123456789abcdef0,
        };
        let target = d::Identity {
            instance: 9,
            generation: 77,
        };
        let request = d::Request {
            op: d::Op::Resize,
            caller,
            target,
            dx: -32,
            dy: 24,
            item: 0x1122334455667788,
            deadline_tick: 500,
        };
        let mut bytes = [0; d::REQUEST_SIZE];
        d::encode_request(&mut bytes, request);
        assert_eq!(
            unsafe { r4desk_wire_check(5, bytes.as_ptr(), bytes.len()) },
            1
        );
        assert_eq!(d::decode_request(&bytes), Some(request));
        let mut old_version = bytes;
        old_version[4] = 1;
        assert!(d::decode_request(&old_version).is_none());
        assert_eq!(
            unsafe { r4desk_wire_check(5, old_version.as_ptr(), old_version.len()) },
            0
        );
        bytes[72] = 1;
        assert_eq!(
            unsafe { r4desk_wire_check(5, bytes.as_ptr(), bytes.len()) },
            0
        );
        assert!(d::decode_request(&bytes).is_none());
        let work = d::Work {
            ticket: 90,
            deadline_tick: 100,
            broker: caller,
            request,
        };
        let mut wb = [0; d::WORK_SIZE];
        d::encode_work(&mut wb, Some(work));
        assert_eq!(unsafe { r4desk_wire_check(6, wb.as_ptr(), wb.len()) }, 1);
        assert_eq!(d::decode_work(&wb), Some(Some(work)));
        d::encode_work(&mut wb, None);
        assert_eq!(d::decode_work(&wb), Some(None));
        wb[119] = 1;
        assert!(d::decode_work(&wb).is_none());
        let mut response = d::Response::new(
            d::Request {
                op: d::Op::Snapshot,
                ..request
            },
            caller,
        );
        response.x = -3;
        response.flags = d::FOCUSED | d::FRAME_VALID;
        response.screenshot_state = 2;
        response.screenshot_bytes = 3686454;
        response.restart_state = 3;
        response.restart_result = -17;
        response.idle_timeouts = 0xfedcba9876543210;
        response.present_failures = 0xfedcba9876543210;
        response.present_lost_frames = 0x8877665544332211;
        let mut rb = [0; d::RESPONSE_SIZE];
        d::encode_response(&mut rb, response);
        assert_eq!(unsafe { r4desk_wire_check(7, rb.as_ptr(), rb.len()) }, 1);
        assert_eq!(d::decode_response(&rb), Some(response));
        assert!(d::decode_response(&rb[..128]).is_none());
        assert_eq!(unsafe { r4desk_wire_check(7, rb.as_ptr(), 128) }, 0);
        let mut unknown_flag = rb;
        unknown_flag[68] |= 128;
        assert!(d::decode_response(&unknown_flag).is_none());
        assert_eq!(
            unsafe { r4desk_wire_check(7, unknown_flag.as_ptr(), unknown_flag.len()) },
            0
        );
        rb[127] = 1;
        assert_eq!(unsafe { r4desk_wire_check(7, rb.as_ptr(), rb.len()) }, 0);
        assert!(d::decode_response(&rb).is_none());
        let mut cb = [0; d::COMPLETION_SIZE];
        d::encode_completion(&mut cb, 90, caller, response);
        assert_eq!(unsafe { r4desk_wire_check(8, cb.as_ptr(), cb.len()) }, 1);
        assert_eq!(
            d::SnesOptions::decode(29),
            Some(d::SnesOptions {
                pal: true,
                mode: 2,
                maximized: true,
                check_only: true
            })
        );
        assert!(d::SnesOptions::decode(31).is_none());
        assert!(d::SnesOptions::decode(6).is_none());
        assert!(d::SnesOptions::decode(32).is_none());
        assert_eq!(
            d::screenshot_path(caller).unwrap(),
            "C:\\TEMP\\SS00000056-123456789ABCDEF0.BMP"
        );
        let mut reservation = reservation::Session::default();
        let begin = d::Request {
            op: d::Op::Begin,
            deadline_tick: 500,
            ..request
        };
        assert!(!reservation.admit(begin, 1, 100, 7, true, true, false));
        assert!(reservation.admit(begin, 1, 100, 7, true, false, false));
        assert!(!reservation.bind(caller));
        assert!(reservation.bind(target));
        assert!(!reservation.owns(d::Identity {
            generation: 78,
            ..target
        }));
        assert!(reservation.foreign(
            [d::Identity {
                generation: 78,
                ..target
            }],
            d::Identity::default()
        ));
        assert!(!reservation.admit(request, 2, 100, 8, true, false, false));
        assert!(reservation.admit(
            d::Request {
                op: d::Op::ScreenshotStatus,
                ..request
            },
            2,
            100,
            8,
            true,
            true,
            false
        ));
        assert!(reservation.admit(
            d::Request {
                op: d::Op::End,
                ..request
            },
            2,
            100,
            8,
            true,
            true,
            false
        ));
    });
}
