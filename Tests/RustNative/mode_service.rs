//! The actual Host and owned SDK I/O boundary. A controlled peer retains the
//! original addresses until exact Close ACK while main-task input progresses.
use super::*;
use r4os_sdk::{App, ImportSpec, StartupInfo};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
#[derive(Clone, Copy)]
struct Pending {
    id: u32,
    request: usize,
    header: usize,
    response: usize,
    ready: bool,
    cancelled: bool,
    close_busy: u32,
}
#[derive(Default)]
struct Peer {
    open: u32,
    submitted: u32,
    sync_mode_calls: u32,
    status: u32,
    pending: Option<Pending>,
    previous: Option<(usize, usize, usize)>,
    reuse: u32,
    refuse: bool,
    malformed: bool,
    image_calls: u32,
}
static CASE: Mutex<()> = Mutex::new(());
static STATE: Mutex<Peer> = Mutex::new(Peer {
    open: 0,
    submitted: 0,
    sync_mode_calls: 0,
    status: 0,
    pending: None,
    previous: None,
    reuse: 0,
    refuse: false,
    malformed: false,
    image_calls: 0,
});
unsafe extern "C" fn open(_: *const u8, out: *mut a::ServiceInfo) -> i32 {
    let mut peer = STATE.lock().unwrap();
    peer.open += 1;
    unsafe {
        out.write(a::ServiceInfo {
            handle: 76 + peer.open,
            instance_id: 31,
            ..Default::default()
        })
    };
    0
}
unsafe extern "C" fn close(_: u32) -> i32 {
    // The actual Kernel endpoint control0 decrements open_handles only.
    // It does not cancel a queued call or acknowledge its buffer lifetime.
    0
}

unsafe extern "C" fn precise(out: *mut a::MonotonicClockInfo) -> i32 {
    unsafe {
        out.write(a::MonotonicClockInfo {
            flags: a::monotonic_clock_flag_valid,
            instant_ns: 1,
            event_requested_hz: 1000,
            event_effective_hz: 1000,
            ..Default::default()
        })
    }
    1
}
unsafe extern "C" fn call(
    _: u32,
    op: u16,
    request: *const u8,
    count: u32,
    header: *mut a::ServiceMessageHeader,
    response: *mut u8,
    capacity: u32,
    _: u64,
) -> i32 {
    assert!(
        op != a::window_mode_op_exchange,
        "mode work blocked in synchronous call"
    );
    let source = unsafe { core::slice::from_raw_parts(request, count as usize) };
    let out = unsafe { core::slice::from_raw_parts_mut(response, capacity as usize) };
    match op {
        images::OPERATION => {
            let mut peer = STATE.lock().unwrap();
            peer.image_calls += 1;
            let owner: a::ProgramProcessHandle = wire::decode(source).unwrap();
            assert_eq!(out.len(), images::SIZE);
            out.fill(0);
            0x31474952u32.write(out); 1u16.write(&mut out[4..]); (images::SIZE as u16).write(&mut out[6..]);
            31u32.write(&mut out[8..]); 1u32.write(&mut out[12..]); 9u64.write(&mut out[16..]);
            owner.instance_id.write(&mut out[24..]); owner.generation.write(&mut out[32..]);
            u64::from(peer.image_calls).write(&mut out[40..]);
            42u32.write(&mut out[48..]); images::NATIVE.write(&mut out[52..]); 56u64.write(&mut out[56..]);
            19u64.write(&mut out[64..]); 1u32.write(&mut out[72..]); u64::from(peer.image_calls).write(&mut out[80..]);
            if peer.malformed { out[28] = 1; }
        }
        a::tray_service_op_desktop_sync => {
            let mut value: a::TrayDesktopExchange = wire::decode(source).unwrap();
            value.desktop_epoch = value.desktop_owner.generation;
            wire::encode(&value, out);
        }
        a::display_control_op_color_exchange => {
            let value: a::DisplayColorExchange = wire::decode(source).unwrap();
            wire::encode(&value, out);
        }
        _ => panic!("unexpected operation {op}"),
    }
    unsafe {
        header.write(a::ServiceMessageHeader {
            op,
            ..Default::default()
        })
    };
    capacity as i32
}
unsafe extern "C" fn submit(
    _: u32,
    op: u16,
    request: *const u8,
    len: u32,
    header: *mut a::ServiceMessageHeader,
    response: *mut u8,
    capacity: u32,
    timeout: u64,
    flags: u32,
    out: *mut u32,
) -> i32 {
    assert_eq!(
        (op, len as usize, capacity as usize, timeout, flags),
        (
            a::window_mode_op_exchange,
            a::WindowModeExchange::SIZE,
            a::WindowModeReply::SIZE,
            250,
            0
        )
    );
    let mut peer = STATE.lock().unwrap();
    assert!(peer.pending.is_none());
    if peer.refuse {
        peer.refuse = false;
        return a::io_error_no_slots;
    }
    let addresses = (request.addr(), header.addr(), response.addr());
    if let Some(before) = peer.previous {
        assert_eq!(addresses, before);
        peer.reuse += 1
    }
    peer.submitted += 1;
    let id = 40 + peer.submitted;
    peer.pending = Some(Pending {
        id,
        request: addresses.0,
        header: addresses.1,
        response: addresses.2,
        ready: false,
        cancelled: false,
        close_busy: 0,
    });
    unsafe { out.write(id) };
    0
}
unsafe fn complete(p: Pending, malformed: bool) -> a::ProgramIoInfo {
    let request: a::WindowModeExchange = wire::decode(unsafe {
        core::slice::from_raw_parts(p.request as *const u8, a::WindowModeExchange::SIZE)
    })
    .unwrap();
    assert_eq!(
        (
            request.identity.desktop.instance_id,
            request.identity.owner.instance_id,
            request.identity.owner.generation
        ),
        (88, 42, 56)
    );
    let mut info = a::ProgramIoInfo {
        request_id: p.id,
        kind: a::io_kind_service_call,
        owner_instance: 88,
        state: a::io_state_running,
        ..Default::default()
    };
    if !p.ready {
        return info;
    }
    if p.cancelled {
        info.state = a::io_state_failed;
        info.result = a::io_error_cancelled;
        return info;
    }
    let reply = a::WindowModeReply {
        identity: a::WindowModeIdentity {
            serial: 19,
            service: a::ProgramProcessHandle {
                instance_id: 31,
                generation: 7,
                ..Default::default()
            },
            ..request.identity
        },
        mode: request.mode,
        ..Default::default()
    };
    unsafe {
        (p.header as *mut a::ServiceMessageHeader).write(a::ServiceMessageHeader {
            op: a::window_mode_op_exchange,
            request_id: 100 + p.id,
            client_id: 88,
            payload_len: if malformed {
                0
            } else {
                a::WindowModeReply::SIZE as u16
            },
            ..Default::default()
        });
        wire::encode(
            &reply,
            core::slice::from_raw_parts_mut(p.response as *mut u8, a::WindowModeReply::SIZE),
        );
    }
    info.state = a::io_state_completed;
    info.result = a::WindowModeReply::SIZE as i32;
    info.processed_bytes = a::WindowModeReply::SIZE as u64;
    info
}
unsafe extern "C" fn status(id: u32, out: *mut a::ProgramIoInfo) -> i32 {
    let mut peer = STATE.lock().unwrap();
    peer.status += 1;
    let p = peer.pending.unwrap();
    assert_eq!(id, p.id);
    unsafe { out.write(complete(p, peer.malformed)) };
    0
}
unsafe extern "C" fn io_close(id: u32) -> i32 {
    let mut peer = STATE.lock().unwrap();
    let p = peer.pending.as_mut().unwrap();
    assert_eq!(p.id, id);
    if !p.ready {
        return a::io_error_busy;
    }
    if p.close_busy != 0 {
        p.close_busy -= 1;
        return a::io_error_busy;
    }
    let p = peer.pending.take().unwrap();
    let _ = unsafe { complete(p, peer.malformed) };
    peer.previous = Some((p.request, p.header, p.response));
    0
}
fn with(case: impl FnOnce(&Services<'_>, &Resources<'_>, a::ProgramProcessHandle)) {
    let _case = CASE.lock().unwrap();
    let table = a::R4XStartR4Sys {
        service_open: open as *const () as usize,
        service_close: close as *const () as usize,
        service_call: call as *const () as usize,
        monotonic_clock: precise as *const () as usize,
        io_service_call: submit as *const () as usize,
        io_status: status as *const () as usize,
        io_close: io_close as *const () as usize,
        ..Default::default()
    };
    let imports = [a::R4XStartImport {
        group_id: 1,
        min_version: 1,
        resolved_version: 1,
        flags: a::r4xstart_import_flag_group_interface,
        module_name: c"R4SYS".as_ptr() as u64,
        symbol_name: c"Query".as_ptr() as u64,
        table: &table as *const _ as u64,
        ..Default::default()
    }];
    let raw = a::R4XStartContext {
        app_class: a::R4XStartAppClass::console.0,
        instance_id: 88,
        flags: a::r4xstart_flag_imports_valid,
        imports: imports.as_ptr() as u64,
        import_count: 1,
        ..Default::default()
    };
    let specs = [ImportSpec {
        module_name: b"R4SYS",
        symbol_name: b"Query",
        minimum: 1,
        optional: false,
        group_id: 1,
        magic: table.magic,
        version: table.abi_version,
        size: table.size,
    }];
    let expected = StartupInfo {
        app_class: a::R4XStartAppClass::console,
        required_groups: 2,
        imports: &specs,
    };
    let app = unsafe { App::from_raw(&raw, &expected, c"MODEFIX") }.unwrap();
    let services = Services::new(&app).unwrap();
    let resources = Resources::new(&app).unwrap();
    case(
        &services,
        &resources,
        a::ProgramProcessHandle {
            instance_id: 88,
            generation: 34,
            ..Default::default()
        },
    );
}
#[test]
fn image_markers_follow_activity_without_tray_delay_and_close_revokes_snapshot() {
    with(|services, resources, owner| {
        *STATE.lock().unwrap() = Peer::default();
        let mut host = Host::new(services, resources, owner).unwrap();
        host.initialize().unwrap();
        let surface = a::WindowGraphicsSurface {
            service: a::ProgramProcessHandle { instance_id:31, generation:9, reserved:0 }, desktop:owner,
            owner:a::ProgramProcessHandle { instance_id:42, generation:56, reserved:0 },
            serial:19, window_id:1, ..a::WindowGraphicsSurface::default()
        };
        assert!(host.image_marker(surface).is_none());
        host.refresh_images(7, false).unwrap();
        for _ in 0..1000 { host.refresh_images(7, false).unwrap(); }
        assert_eq!(STATE.lock().unwrap().image_calls, 1);
        assert_eq!(host.image_marker(surface).unwrap().revision, 1);
        // No timer/tray RPC participates. A notification after the previous
        // request's captured sequence causes a new read on the next round.
        host.refresh_images(8, false).unwrap();
        assert_eq!(host.image_marker(surface).unwrap().revision, 2);
        host.refresh_images(8, true).unwrap();
        assert_eq!(host.image_marker(surface).unwrap().revision, 3);
        host.close().unwrap();
        assert!(host.image_marker(surface).is_none());
        host.initialize().unwrap();
        STATE.lock().unwrap().malformed = true;
        assert!(host.refresh_images(8, false).is_err());
        assert!(!host.claimed());
        assert!(host.image_marker(surface).is_none());
    });
}
#[test]
fn d088_owned_mode_delay_late_ack_retirement_reconnect_and_queue_bound() {
    with(|services, resources, owner| {
        *STATE.lock().unwrap() = Peer::default();
        let mut host = Host::new(services, resources, owner).unwrap();
        host.initialize().unwrap();
        let request = a::WindowModeExchange {
            identity: a::WindowModeIdentity {
                desktop: owner,
                owner: a::ProgramProcessHandle {
                    instance_id: 42,
                    generation: 56,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let start = Instant::now();
        assert!(host.mode_start(request, 250).unwrap());
        let mut input = 0;
        for _ in 0..32 {
            input += 1;
            assert!(host.mode_poll().unwrap().is_none());
            assert!(!host.mode_start(request, 250).unwrap());
        }
        std::thread::sleep(Duration::from_millis(20));
        {
            let mut s = STATE.lock().unwrap();
            s.pending.as_mut().unwrap().ready = true;
            s.pending.as_mut().unwrap().close_busy = 2;
        }
        assert!(host.mode_poll().unwrap().is_none());
        assert!(host.mode_busy());
        assert!(!host.mode_start(request, 250).unwrap());
        assert!(host.mode_poll().unwrap().is_none());
        let answer = host.mode_poll().unwrap().unwrap();
        assert_eq!(answer.reply.identity.serial, 19);
        assert!(answer.request.identity.same(&request.identity));
        assert!(!host.mode_busy());
        let delayed = start.elapsed().as_nanos();
        assert!(delayed >= 20_000_000);
        // Close is not a buffer ACK and cannot admit a new connection early.
        assert!(host.mode_start(request, 250).unwrap());
        assert!(host.close().is_err());
        assert!(host.mode_busy());
        assert!(host.mode_start(request, 250).is_err());
        {
            let mut s = STATE.lock().unwrap();
            s.pending.as_mut().unwrap().ready = true;
            s.pending.as_mut().unwrap().close_busy = 1;
        }
        assert!(host.close().is_err());
        assert!(host.mode_busy());
        host.close().unwrap();
        assert!(!host.mode_busy());
        host.initialize().unwrap();
        STATE.lock().unwrap().refuse = true;
        assert!(!host.mode_start(request, 250).unwrap());
        assert!(!host.mode_busy());
        assert!(host.mode_start(request, 250).unwrap());
        {
            let mut s = STATE.lock().unwrap();
            s.pending.as_mut().unwrap().ready = true;
            s.malformed = true;
        }
        assert!(host.mode_poll().is_err());
        assert!(!host.claimed());
        assert!(!host.mode_busy());
        {
            STATE.lock().unwrap().malformed = false;
        }
        // A genuine terminal cancellation/error is separate from endpoint
        // reference close and still needs the native I/O Close acknowledgement.
        host.initialize().unwrap();
        assert!(host.mode_start(request, 250).unwrap());
        {
            let mut s = STATE.lock().unwrap();
            s.pending.as_mut().unwrap().ready = true;
            s.pending.as_mut().unwrap().cancelled = true;
        }
        assert!(host.mode_poll().is_err());
        assert!(!host.mode_busy() && !host.claimed());
        host.initialize().unwrap();
        assert!(host.mode_start(request, 250).unwrap());
        STATE.lock().unwrap().pending.as_mut().unwrap().ready = true;
        assert!(host.mode_poll().unwrap().is_some());
        host.close().unwrap();
        let s = STATE.lock().unwrap();
        assert!(s.pending.is_none());
        assert_eq!(s.submitted, 5);
        assert_eq!(s.reuse, 4);
        assert_eq!(s.sync_mode_calls, 0);
        std::println!(
            "{{\"schema\":\"r4os.desktop-work\",\"case\":\"mode-async-after88\",\"pending_input_iterations\":{},\"sync_mode_calls\":{},\"max_in_flight\":1,\"buffer_reuses\":{},\"controlled_delay_ms\":20,\"transaction_wall_ns\":{}}}",
            input,
            s.sync_mode_calls,
            s.reuse,
            delayed
        );
    });
}
