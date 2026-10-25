//! Real Desktop audio owner + general SDK owned I/O. Only the service/Kernel
//! peer is controlled; no physical audio or cursor latency is claimed.
use super::*;
use crate::runtime::wire;
use r4os_sdk::{App, ImportSpec, StartupInfo, resources::Resources, services::Services};
use std::sync::Mutex;
#[derive(Clone, Copy)]
struct Pending {
    id: u32,
    op: u16,
    request: usize,
    header: usize,
    response: usize,
    length: usize,
    capacity: usize,
    original: [u8; 96],
    ready: bool,
    delivered: bool,
    timeout: bool,
    close_busy: u32,
}
struct Peer {
    pending: Option<Pending>,
    submitted: u32,
    handles: u32,
    generation: u64,
    volume: u32,
    muted: bool,
    active: [u8; 64],
    revision: u64,
    selections: Vec<[u8; 64]>,
    masters: Vec<u32>,
    malformed: bool,
    refuse: bool,
    previous: [Option<(usize, usize, usize)>; 3],
    reuse: u32,
}
impl Default for Peer {
    fn default() -> Self {
        Self {
            pending: None,
            submitted: 0,
            handles: 0,
            generation: 7,
            volume: 65536,
            muted: false,
            active: [0; 64],
            revision: 1,
            selections: Vec::new(),
            masters: Vec::new(),
            malformed: false,
            refuse: false,
            previous: [None; 3],
            reuse: 0,
        }
    }
}
static CASE: Mutex<()> = Mutex::new(());
static STATE: Mutex<Option<Peer>> = Mutex::new(None);
fn peer<T>(f: impl FnOnce(&mut Peer) -> T) -> T {
    f(STATE.lock().unwrap().as_mut().unwrap())
}
fn bank(op: u16) -> usize {
    if op == a::audio_service_op_master_status {
        0
    } else if op == a::audio_service_op_set_master_state {
        1
    } else {
        2
    }
}
unsafe extern "C" fn open(_: *const u8, out: *mut a::ServiceInfo) -> i32 {
    peer(|p| p.handles += 1);
    unsafe {
        out.write(a::ServiceInfo {
            handle: 77,
            instance_id: 31,
            ..Default::default()
        })
    };
    0
}
unsafe extern "C" fn close(handle: u32) -> i32 {
    assert_eq!(handle, 77);
    peer(|p| {
        assert!(p.handles > 0);
        p.handles -= 1;
    });
    0
}
unsafe extern "C" fn process(id: u32, out: *mut a::ProgramProcessHandle) -> i32 {
    assert_eq!(id, 31);
    unsafe {
        out.write(a::ProgramProcessHandle {
            instance_id: id,
            generation: peer(|p| p.generation),
            reserved: 0,
        })
    };
    0
}
unsafe extern "C" fn process_status(
    h: *const a::ProgramProcessHandle,
    out: *mut a::ProgramInstanceInfo,
) -> i32 {
    let h = unsafe { *h };
    if h.instance_id != 31 || h.generation != peer(|p| p.generation) {
        return a::program_handle_error_stale;
    }
    unsafe {
        out.write(a::ProgramInstanceInfo {
            id: 31,
            state: a::ProgramInstanceState::running.0,
            ..Default::default()
        })
    };
    0
}
unsafe extern "C" fn call(
    _: u32,
    _: u16,
    _: *const u8,
    _: u32,
    _: *mut a::ServiceMessageHeader,
    _: *mut u8,
    _: u32,
    _: u64,
) -> i32 {
    panic!("ordinary audio blocked in a synchronous RPC")
}
unsafe extern "C" fn submit(
    endpoint: u32,
    op: u16,
    body: *const u8,
    length: u32,
    header: *mut a::ServiceMessageHeader,
    response: *mut u8,
    capacity: u32,
    timeout: u64,
    flags: u32,
    out: *mut u32,
) -> i32 {
    peer(|p| {
        assert!(p.pending.is_none());
        assert_eq!((endpoint, flags), (77, 0));
        if p.refuse {
            p.refuse = false;
            return a::io_error_busy;
        }
        assert_eq!(length as usize, [0, 32, 96][bank(op)]);
        assert_eq!(capacity as usize, if bank(op) == 2 { 1520 } else { 128 });
        assert_eq!(timeout, if bank(op) == 2 { 1500 } else { 100 });
        let pointers = (body as usize, header as usize, response as usize);
        if let Some(previous) = p.previous[bank(op)] {
            assert_eq!(previous, pointers);
            p.reuse += 1;
        }
        let mut original = [0; 96];
        original[..length as usize]
            .copy_from_slice(unsafe { core::slice::from_raw_parts(body, length as usize) });
        p.submitted += 1;
        let id = 100 + p.submitted;
        p.pending = Some(Pending {
            id,
            op,
            request: pointers.0,
            header: pointers.1,
            response: pointers.2,
            length: length as usize,
            capacity: capacity as usize,
            original,
            ready: false,
            delivered: false,
            timeout: false,
            close_busy: 0,
        });
        unsafe { out.write(id) };
        0
    })
}
fn deliver(p: &mut Peer) -> a::ProgramIoInfo {
    let pending = p.pending.unwrap();
    assert_eq!(
        unsafe { core::slice::from_raw_parts(pending.request as *const u8, pending.length) },
        &pending.original[..pending.length]
    );
    let mut info = a::ProgramIoInfo {
        request_id: pending.id,
        kind: a::io_kind_service_call,
        owner_instance: 88,
        state: a::io_state_running,
        ..Default::default()
    };
    if !pending.ready {
        return info;
    }
    if !pending.delivered {
        let input = &pending.original[..pending.length];
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(pending.response as *mut u8, pending.capacity)
        };
        if bank(pending.op) < 2 {
            if pending.op == a::audio_service_op_set_master_state {
                let update: a::AudioServiceMasterRequest = wire::decode(input).unwrap();
                if update.flags & a::audio_master_request_flag_set_volume != 0 {
                    p.volume = update.fixed_volume;
                    p.masters.push(p.volume);
                }
                if update.flags & a::audio_master_request_flag_set_muted != 0 {
                    p.muted = update.flags & a::audio_master_request_flag_muted != 0;
                }
            }
            let value = a::AudioServiceMasterState {
                service_epoch: 31,
                selected_volume_fixed: p.volume,
                effective_volume_fixed: if p.muted { 0 } else { p.volume },
                flags: if p.muted {
                    a::audio_master_state_flag_muted
                } else {
                    0
                },
                ..Default::default()
            };
            wire::encode(&value, bytes);
        } else {
            let query: a::AudioServiceOutputRequest = wire::decode(input).unwrap();
            if pending.op == a::audio_service_op_select_output {
                assert_eq!(query.service_epoch, 31);
                assert_eq!(query.revision, p.revision);
                p.active = query.id;
                p.revision += 1;
                p.selections.push(query.id);
            }
            let mut value = a::AudioServiceOutputState {
                service_epoch: 31,
                revision: p.revision,
                index: query.index,
                total: 9,
                count: (9 - query.index).min(8),
                desired_id: p.active,
                active_id: p.active,
                reason: a::audio_output_reason_auto_hdmi,
                ..Default::default()
            };
            for (i, row) in value.outputs[..value.count as usize].iter_mut().enumerate() {
                row.id[0] = b'A' + query.index as u8 + i as u8;
                row.name[0] = row.id[0];
                row.availability = a::audio_output_available;
            }
            wire::encode(&value, bytes);
        }
        unsafe {
            (pending.header as *mut a::ServiceMessageHeader).write(a::ServiceMessageHeader {
                op: pending.op,
                request_id: pending.id + 1000,
                client_id: 88,
                payload_len: if p.malformed {
                    0
                } else {
                    pending.capacity as u16
                },
                ..Default::default()
            })
        };
        p.pending.as_mut().unwrap().delivered = true;
    }
    // A timed-out mutation may already have run. The client must not replay it.
    info.state = if pending.timeout {
        a::io_state_failed
    } else {
        a::io_state_completed
    };
    info.result = if pending.timeout {
        a::service_api_result_timeout
    } else {
        pending.capacity as i32
    };
    info.processed_bytes = if pending.timeout {
        0
    } else {
        pending.capacity as u64
    };
    info
}
unsafe extern "C" fn status(id: u32, out: *mut a::ProgramIoInfo) -> i32 {
    peer(|p| {
        assert_eq!(id, p.pending.unwrap().id);
        unsafe { out.write(deliver(p)) };
    });
    0
}
unsafe extern "C" fn io_close(id: u32) -> i32 {
    peer(|p| {
        let pending = p.pending.as_mut().unwrap();
        assert_eq!(pending.id, id);
        if !pending.ready {
            return a::io_error_busy;
        }
        if pending.close_busy > 0 {
            pending.close_busy -= 1;
            return a::io_error_busy;
        }
        let _ = deliver(p);
        let pending = p.pending.take().unwrap();
        p.previous[bank(pending.op)] = Some((pending.request, pending.header, pending.response));
        0
    })
}
fn with(case: impl FnOnce(&Services<'_>, &Resources<'_>, a::ProgramProcessHandle)) {
    let _case = CASE.lock().unwrap();
    *STATE.lock().unwrap() = Some(Peer::default());
    let table = a::R4XStartR4Sys {
        service_open: open as *const () as usize,
        service_close: close as *const () as usize,
        service_call: call as *const () as usize,
        program_open_handle: process as *const () as usize,
        program_handle_status: process_status as *const () as usize,
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
    let app = unsafe { App::from_raw(&raw, &expected, c"AUDIOFIX") }.unwrap();
    let services = Services::new(&app).unwrap();
    let resources = Resources::new(&app).unwrap();
    case(
        &services,
        &resources,
        a::ProgramProcessHandle {
            instance_id: 88,
            generation: 34,
            reserved: 0,
        },
    );
    peer(|p| {
        assert!(p.pending.is_none());
        assert_eq!(p.handles, 0);
    });
}
fn step(owner: &mut Owner, io: &mut AudioIo<'_>, now: u64) {
    owner.synchronize(io, true, now, 1000);
}
fn finish(owner: &mut Owner, io: &mut AudioIo<'_>, now: u64) {
    peer(|p| p.pending.as_mut().unwrap().ready = true);
    step(owner, io, now);
}
fn connect(owner: &mut Owner, io: &mut AudioIo<'_>) {
    step(owner, io, 0);
    finish(owner, io, 1);
    assert!(owner.ui.reachable);
}
fn stop(owner: &mut Owner, io: &mut AudioIo<'_>) {
    owner.stop();
    peer(|p| {
        if let Some(v) = p.pending.as_mut() {
            v.ready = true;
            v.close_busy = 0;
        }
    });
    assert!(io.drain());
}
fn id(value: u8) -> [u8; 64] {
    let mut out = [0; 64];
    out[0] = value;
    out
}

#[test]
fn audio_pending_preserves_view_and_later_user_volume_intent() {
    with(|services, resources, handle| {
        let mut io = AudioIo::new(services, resources, handle).unwrap();
        let mut owner = Owner::default();
        peer(|p| p.refuse = true);
        step(&mut owner, &mut io, 0);
        assert!(!io.busy());
        assert!(owner.need_master);
        connect(&mut owner, &mut io);
        assert_eq!(owner.ui.percent, 100);
        step(&mut owner, &mut io, 500); // Delayed ordinary status read.
        assert!(owner.set_percent(42, true, 501));
        for _ in 0..1000 {
            step(&mut owner, &mut io, 501);
            assert_eq!(owner.ui.percent, 42);
        }
        assert_eq!(peer(|p| p.submitted), 2);
        finish(&mut owner, &mut io, 502); // Old status cannot replace42; write42 starts.
        assert_eq!(owner.ui.percent, 42);
        assert!(owner.set_percent(70, true, 503));
        finish(&mut owner, &mut io, 504);
        assert_eq!(owner.ui.percent, 70);
        finish(&mut owner, &mut io, 505);
        assert_eq!(owner.ui.percent, 70);
        assert_eq!(
            peer(|p| p.masters.clone()),
            vec![volume::gain_for_percent(42), volume::gain_for_percent(70)]
        );
        owner.set_popup(true);
        step(&mut owner, &mut io, 506);
        for _ in 0..1000 {
            step(&mut owner, &mut io, 507);
            assert!(owner.page.loading);
            assert_eq!(owner.ui.percent, 70);
        }
        step(&mut owner, &mut io, 1000); // another due refresh while the same query is pending
        finish(&mut owner, &mut io, 1001);
        assert!(owner.page.supported);
        assert!(!owner.page.loading);
        assert_eq!(
            peer(|p| p.pending.unwrap().op),
            a::audio_service_op_master_status
        );
        finish(&mut owner, &mut io, 1002);
        assert!(!io.busy()); // one fresh snapshot satisfies coalesced demand
        assert!(peer(|p| p.reuse) >= 2);
        stop(&mut owner, &mut io);
    });
}
#[test]
fn audio_popup_pages_and_quick_selections_discard_superseded_replies() {
    with(|services, resources, handle| {
        let mut io = AudioIo::new(services, resources, handle).unwrap();
        let mut owner = Owner::default();
        connect(&mut owner, &mut io);
        owner.set_popup(true);
        step(&mut owner, &mut io, 2);
        finish(&mut owner, &mut io, 3);
        owner.need_outputs = true;
        step(&mut owner, &mut io, 4);
        owner.set_popup(false);
        owner.set_popup(true);
        owner.change_page(true);
        finish(&mut owner, &mut io, 5);
        assert_eq!(owner.page.index, 0); // old complete view retained
        assert!(owner.page.loading);
        finish(&mut owner, &mut io, 6);
        assert_eq!(owner.page.index, 4);
        owner.select(id(b'A'));
        step(&mut owner, &mut io, 7);
        finish(&mut owner, &mut io, 8); // fresh catalogue -> A admitted
        owner.select(id(b'B'));
        finish(&mut owner, &mut io, 9); // A result discarded, B catalogue queued
        assert_eq!(owner.outputs.active_id, [0; 64]);
        finish(&mut owner, &mut io, 10);
        finish(&mut owner, &mut io, 11);
        finish(&mut owner, &mut io, 12);
        assert_eq!(peer(|p| p.selections.clone()), vec![id(b'A'), id(b'B')]);
        assert_eq!(owner.outputs.active_id, id(b'B'));
        assert_eq!(owner.page.index, 4);
        assert!(!owner.page.loading);
        owner.need_outputs = true;
        step(&mut owner, &mut io, 13);
        owner.set_popup(false);
        peer(|p| p.active = id(b'C'));
        finish(&mut owner, &mut io, 14);
        assert_eq!(owner.outputs.active_id, id(b'B')); // closed popup consumes no late view
        let ticket = owner.begin_probe();
        step(&mut owner, &mut io, 15);
        assert_eq!(owner.probe_result(ticket), None);
        finish(&mut owner, &mut io, 16);
        assert_eq!(owner.probe_result(ticket), Some(true));
        owner.end_probe(ticket);
        assert!(!owner.ui.popup_open);
        stop(&mut owner, &mut io);
    });
}
#[test]
fn audio_timeout_generation_and_partial_close_preserve_exact_owned_buffers() {
    with(|services, resources, handle| {
        let mut io = AudioIo::new(services, resources, handle).unwrap();
        let mut owner = Owner::default();
        connect(&mut owner, &mut io);
        owner.set_popup(true);
        step(&mut owner, &mut io, 2);
        finish(&mut owner, &mut io, 3);
        owner.select(id(b'A'));
        step(&mut owner, &mut io, 4);
        finish(&mut owner, &mut io, 5);
        peer(|p| {
            let v = p.pending.as_mut().unwrap();
            v.timeout = true;
            v.close_busy = 2;
            v.ready = true;
        });
        step(&mut owner, &mut io, 6);
        assert!(io.busy());
        assert!(owner.ui.reachable);
        step(&mut owner, &mut io, 7);
        assert!(io.busy());
        step(&mut owner, &mut io, 8);
        assert!(!owner.ui.reachable);
        assert!(owner.page.request_error);
        assert_eq!(owner.outputs.active_id, [0; 64]);
        step(&mut owner, &mut io, 9);
        step(&mut owner, &mut io, 508);
        finish(&mut owner, &mut io, 509);
        finish(&mut owner, &mut io, 510);
        assert_eq!(owner.outputs.active_id, id(b'A'));
        assert_eq!(peer(|p| p.selections.clone()), vec![id(b'A')]); // no timeout replay
        owner.need_master = true;
        step(&mut owner, &mut io, 511);
        owner.set_percent(30, true, 512);
        peer(|p| p.generation = 8);
        finish(&mut owner, &mut io, 513);
        assert_eq!(owner.provider.generation, 7);
        assert!(!owner.ui.reachable);
        step(&mut owner, &mut io, 514);
        step(&mut owner, &mut io, 1013);
        finish(&mut owner, &mut io, 1014);
        assert_eq!(owner.provider.generation, 8);
        assert_eq!(owner.pending_flags, 0);
        peer(|p| p.malformed = true);
        finish(&mut owner, &mut io, 1015);
        assert!(!owner.ui.reachable);
        peer(|p| p.malformed = false);
        step(&mut owner, &mut io, 1016);
        step(&mut owner, &mut io, 1515);
        owner.stop();
        assert!(!io.drain());
        assert!(io.busy()); // live buffer loan still exists
        peer(|p| {
            let v = p.pending.as_mut().unwrap();
            v.ready = true;
            v.close_busy = 1;
        });
        assert!(!io.drain());
        assert!(io.drain());
        assert!(!io.busy());
    });
}
