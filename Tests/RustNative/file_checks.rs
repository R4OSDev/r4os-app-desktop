//! Real SDK async admission/status/Close and resident original R4STD decoder.
//! Only the file provider and RAM directory journal are controlled here.
use super::*;
use r4os_sdk::{App, ImportSpec, StartupInfo, storage::System};
use std::{ffi::CStr, sync::Mutex};
const UTC: &[u8] = b"R4S_FORMAT=1\nSCHEMA=TIME\nTIMEZONE=UTC\n";
const BERLIN: &[u8] = b"R4S_FORMAT=1\nSCHEMA=TIME\nTIMEZONE=Europe/Berlin\n";
const READ: usize = 0;
const INFO: usize = 1;
static CASE: Mutex<()> = Mutex::new(());
static STATE: Mutex<Option<Peer>> = Mutex::new(None);
#[derive(Clone, Copy)]
struct Pending {
    id: u32,
    pointer: usize,
    capacity: usize,
    result: Option<i32>,
    close_busy: u32,
    wrong_owner: bool,
}
struct Peer {
    pending: [Option<Pending>; 2],
    next_id: u32,
    submitted: [u32; 2],
    polls: [u32; 2],
    sequence: [u64; 2],
    begins: u32,
    synchronous: u32,
    calendars: u32,
    watched: bool,
    invalid_cursor: bool,
    bytes: Vec<u8>,
}
impl Default for Peer {
    fn default() -> Self {
        Self {
            pending: [None; 2],
            next_id: 1,
            submitted: [0; 2],
            polls: [0; 2],
            sequence: [0; 2],
            begins: 0,
            synchronous: 0,
            calendars: 0,
            watched: true,
            invalid_cursor: false,
            bytes: BERLIN.to_vec(),
        }
    }
}
fn peer<R>(case: impl FnOnce(&mut Peer) -> R) -> R {
    case(STATE.lock().unwrap().as_mut().unwrap())
}
unsafe fn path(raw: *const u8) -> &'static [u8] {
    unsafe { CStr::from_ptr(raw.cast()).to_bytes() }
}
unsafe extern "C" fn begin(raw: *const u8, out: *mut a::DirectoryChangeCursor) -> i32 {
    peer(|p| {
        p.begins += 1;
        if !p.watched {
            return -1;
        }
        let index = if unsafe { path(raw) } == CONFIG_DIR {
            READ
        } else {
            assert_eq!(unsafe { path(raw) }, SERVICES_DIR);
            INFO
        };
        unsafe {
            (*out).node = index as u64 + 1;
            (*out).sequence = p.sequence[index];
        }
        0
    })
}
unsafe extern "C" fn poll(raw: *mut a::DirectoryChangeCursor) -> i32 {
    peer(|p| {
        let cursor = unsafe { &mut *raw };
        let index = cursor.node as usize - 1;
        p.polls[index] += 1;
        if p.invalid_cursor {
            return -3;
        }
        let changed = cursor.sequence != p.sequence[index];
        cursor.sequence = p.sequence[index];
        i32::from(changed)
    })
}
fn submit(index: usize, pointer: usize, capacity: usize, out: *mut u32) -> i32 {
    peer(|p| {
        assert!(p.pending[index].is_none(), "one bounded request per check");
        let id = p.next_id;
        p.next_id += 1;
        p.pending[index] = Some(Pending {
            id,
            pointer,
            capacity,
            result: None,
            close_busy: 0,
            wrong_owner: false,
        });
        p.submitted[index] += 1;
        unsafe { out.write(id) };
        0
    })
}
unsafe extern "C" fn read(
    raw: *const u8,
    bytes: *mut u8,
    len: u64,
    flags: u32,
    out: *mut u32,
) -> i32 {
    assert_eq!(unsafe { path(raw) }, TIME_FILE);
    assert_eq!(len, (CONFIG_BYTES + 1) as u64);
    assert_eq!(flags, 0);
    submit(READ, bytes as usize, len as usize, out)
}
unsafe extern "C" fn info(raw: *const u8, flags: u32, out: *mut u32) -> i32 {
    assert_eq!(unsafe { path(raw) }, AUDIO_FILE);
    assert_eq!(flags, 0);
    submit(INFO, 0, 0, out)
}
unsafe extern "C" fn sync_read(_: *const u8, _: *mut u8, _: u32) -> i32 {
    peer(|p| p.synchronous += 1);
    -1
}
unsafe extern "C" fn sync_info(_: *const u8, _: *mut a::FileInfo) -> i32 {
    peer(|p| p.synchronous += 1);
    -1
}
unsafe extern "C" fn calendar(out: *mut a::TimeState) {
    peer(|p| p.calendars += 1);
    unsafe {
        out.write(a::TimeState {
            valid: 1,
            year: 2026,
            month: 3,
            day: 29,
            hour: 12,
            minute: 34,
            seconds_since_midnight: 12 * 3600 + 34 * 60,
            ..Default::default()
        })
    };
}
unsafe extern "C" fn status(id: u32, out: *mut a::ProgramIoInfo) -> i32 {
    peer(|p| {
        let (index, request) = p
            .pending
            .iter()
            .enumerate()
            .find_map(|(i, r)| r.filter(|r| r.id == id).map(|r| (i, r)))
            .unwrap();
        let result = request.result.unwrap_or(0);
        unsafe {
            out.write(a::ProgramIoInfo {
                request_id: id,
                kind: if index == READ {
                    a::io_kind_file_read
                } else {
                    a::io_kind_file_info
                },
                state: if request.result.is_some() {
                    a::io_state_completed
                } else {
                    a::io_state_running
                },
                result,
                status: 0,
                owner_instance: if request.wrong_owner { 89 } else { 88 },
                requested_bytes: request.capacity as u64,
                processed_bytes: result.max(0) as u64,
                ..Default::default()
            })
        };
    });
    0
}
unsafe extern "C" fn close(id: u32) -> i32 {
    peer(|p| {
        let index = p
            .pending
            .iter()
            .position(|r| r.is_some_and(|r| r.id == id))
            .unwrap();
        let request = p.pending[index].as_mut().unwrap();
        if request.result.is_none() {
            return a::io_error_busy;
        }
        if request.close_busy != 0 {
            request.close_busy -= 1;
            return a::io_error_busy;
        }
        p.pending[index] = None;
        0
    })
}
fn complete(index: usize, result: Option<i32>, close_busy: u32) {
    peer(|p| {
        let request = p.pending[index].as_mut().unwrap();
        let result = result.unwrap_or(p.bytes.len().min(request.capacity) as i32);
        if index == READ && result > 0 {
            assert!(result as usize <= request.capacity && result as usize <= p.bytes.len());
            unsafe {
                core::ptr::copy_nonoverlapping(
                    p.bytes.as_ptr(),
                    request.pointer as *mut u8,
                    result as usize,
                )
            };
        }
        request.result = Some(result);
        request.close_busy = close_busy;
    });
}
fn with(case: impl FnOnce(&App<'_>, &Resources<'_>, &Files<'_>)) {
    let _serial = CASE.lock().unwrap();
    crate::standard_fixture::with(|_| {
        *STATE.lock().unwrap() = Some(Peer::default());
        let table = a::R4XStartR4Sys {
            io_file_read: read as *const () as usize,
            io_file_info: info as *const () as usize,
            io_status: status as *const () as usize,
            io_close: close as *const () as usize,
            directory_change_begin: begin as *const () as usize,
            directory_change_poll: poll as *const () as usize,
            file_read: sync_read as *const () as usize,
            file_info: sync_info as *const () as usize,
            time_state: calendar as *const () as usize,
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
        let app = unsafe { App::from_raw(&raw, &expected, c"FILEFIX") }.unwrap();
        let resources = Resources::new(&app).unwrap();
        let system = System::new(&app).unwrap();
        case(&app, &resources, &system.files());
        peer(|p| {
            assert!(p.pending.iter().all(Option::is_none));
            assert_eq!(p.synchronous, 0);
        });
    });
}
fn stop(owner: &mut Owner<'_>) {
    peer(|p| {
        for request in p.pending.iter_mut().flatten() {
            request.result = Some(a::io_error_cancelled);
            request.close_busy = 0;
        }
    });
    assert!(owner.drain());
}
#[test]
fn unchanged_journal_avoids_reads_info_and_hidden_calendar_work() {
    with(|app, resources, files| {
        let mut clock = Clock::new().unwrap();
        clock.apply_bytes(UTC).unwrap();
        let mut owner = Owner::new(88).unwrap();
        owner.initialize(files, true, 0);
        owner.refresh(resources, files, &mut clock, true, 0);
        peer(|p| p.bytes = UTC.to_vec());
        complete(READ, None, 0);
        complete(INFO, Some(123), 0);
        for n in 1..=50 {
            assert_eq!(
                owner.refresh(resources, files, &mut clock, true, n * SECOND / 10),
                Some(true)
            );
            clock.update(app, n * SECOND / 10).unwrap();
        }
        peer(|p| {
            assert_eq!(p.submitted, [1, 1]);
            assert_eq!(p.polls, [5, 10]);
            assert_eq!(p.begins, 2);
        });
        owner.refresh(resources, files, &mut clock, false, 6 * SECOND);
        let before = peer(|p| (p.polls[READ], p.calendars));
        for n in 7..=10 {
            owner.refresh(resources, files, &mut clock, false, n * SECOND);
            clock.update(app, n * SECOND).unwrap();
        }
        assert_eq!(peer(|p| (p.polls[READ], p.calendars)), before);
        assert_eq!(clock.wait_ticks(10 * SECOND, 1000, 500), 500);
        stop(&mut owner);
    });
}
#[test]
fn slow_read_keeps_ui_progress_and_waits_for_exact_close_before_live_dst_label() {
    with(|app, resources, files| {
        let mut clock = Clock::new().unwrap();
        let mut owner = Owner::new(88).unwrap();
        owner.initialize(files, true, 0);
        owner.refresh(resources, files, &mut clock, true, 0);
        complete(INFO, Some(0), 0);
        let pointer = peer(|p| p.pending[READ].unwrap().pointer);
        let mut turns = 0;
        for n in 1..=100 {
            owner.refresh(resources, files, &mut clock, true, n * SECOND / 100);
            clock.update(app, n * SECOND / 100).unwrap();
            turns += 1;
        }
        assert_eq!(turns, 100);
        assert_eq!(clock.label(), b"12:34");
        complete(READ, None, 2);
        for n in 1..=2 {
            owner.refresh(resources, files, &mut clock, true, SECOND + n);
            assert_eq!(clock.label(), b"12:34");
            assert!(owner.bytes.is_none());
            assert_eq!(peer(|p| p.pending[READ].unwrap().pointer), pointer);
        }
        owner.refresh(resources, files, &mut clock, true, SECOND + 3);
        clock.update(app, SECOND + 3).unwrap();
        assert_eq!(clock.label(), b"14:34");
        assert!(!owner.busy());
        assert_eq!(peer(|p| p.submitted), [1, 1]);
        stop(&mut owner);
    });
}
#[test]
fn known_directory_change_discards_old_reply_and_show_rechecks_current_config() {
    with(|app, resources, files| {
        let mut clock = Clock::new().unwrap();
        clock.apply_bytes(UTC).unwrap();
        let mut owner = Owner::new(88).unwrap();
        owner.initialize(files, true, 0);
        owner.refresh(resources, files, &mut clock, true, 0);
        peer(|p| p.bytes = UTC.to_vec());
        complete(READ, None, 0);
        complete(INFO, Some(4), 0);
        owner.refresh(resources, files, &mut clock, true, 1);
        peer(|p| p.bytes = BERLIN.to_vec());
        peer(|p| p.sequence[READ] += 1);
        owner.refresh(resources, files, &mut clock, true, SECOND);
        peer(|p| p.sequence[READ] += 1);
        complete(READ, None, 1);
        owner.refresh(resources, files, &mut clock, true, 2 * SECOND);
        owner.refresh(resources, files, &mut clock, true, 2 * SECOND + 1);
        clock.update(app, 2 * SECOND + 1).unwrap();
        assert_eq!(clock.label(), b"12:34");
        assert_eq!(peer(|p| p.submitted[READ]), 3);
        complete(READ, None, 0);
        owner.refresh(resources, files, &mut clock, true, 2 * SECOND + 2);
        clock.update(app, 2 * SECOND + 2).unwrap();
        assert_eq!(clock.label(), b"14:34");
        owner.refresh(resources, files, &mut clock, false, 3 * SECOND);
        peer(|p| {
            p.bytes = UTC.to_vec();
            p.sequence[READ] += 1;
        });
        owner.refresh(resources, files, &mut clock, true, 3 * SECOND + 1);
        complete(READ, None, 0);
        owner.refresh(resources, files, &mut clock, true, 3 * SECOND + 2);
        clock.update(app, 3 * SECOND + 2).unwrap();
        assert_eq!(clock.label(), b"12:34");
        stop(&mut owner);
    });
}
#[test]
fn unsupported_journal_uses_bounded_async_fallback_without_starving_slow_reply() {
    with(|app, resources, files| {
        peer(|p| p.watched = false);
        let mut clock = Clock::new().unwrap();
        let mut owner = Owner::new(88).unwrap();
        owner.initialize(files, true, 0);
        owner.refresh(resources, files, &mut clock, true, 0);
        for n in 1..=5 {
            owner.refresh(resources, files, &mut clock, true, n * SECOND);
        }
        assert_eq!(peer(|p| p.submitted), [1, 1]);
        complete(READ, None, 0);
        complete(INFO, Some(a::io_error_not_found), 0);
        assert_eq!(
            owner.refresh(resources, files, &mut clock, true, 5 * SECOND + 1),
            Some(false)
        );
        clock.update(app, 5 * SECOND + 1).unwrap();
        assert_eq!(clock.label(), b"14:34");
        owner.refresh(resources, files, &mut clock, true, 6 * SECOND);
        assert_eq!(peer(|p| p.submitted), [2, 2]);
        complete(READ, Some(READ_NOT_FOUND), 0);
        complete(INFO, Some(44), 0);
        assert_eq!(
            owner.refresh(resources, files, &mut clock, true, 6 * SECOND + 1),
            Some(true)
        );
        clock.update(app, 6 * SECOND + 1).unwrap();
        assert_eq!(clock.label(), b"14:34");
        stop(&mut owner);
    });
}
#[test]
fn malformed_missing_and_wrong_owner_preserve_live_data_and_drain_waits_for_ack() {
    with(|app, resources, files| {
        let mut clock = Clock::new().unwrap();
        clock.apply_bytes(BERLIN).unwrap();
        let mut owner = Owner::new(88).unwrap();
        owner.initialize(files, true, 0);
        owner.refresh(resources, files, &mut clock, true, 0);
        complete(READ, None, 0);
        complete(INFO, Some(22), 0);
        owner.refresh(resources, files, &mut clock, true, 1);
        peer(|p| {
            p.bytes = vec![b'x'; CONFIG_BYTES + 1];
            p.sequence[READ] += 1;
            p.sequence[INFO] += 1;
        });
        owner.refresh(resources, files, &mut clock, true, SECOND);
        complete(READ, None, 0);
        complete(INFO, Some(a::io_error_busy), 0);
        assert_eq!(
            owner.refresh(resources, files, &mut clock, true, SECOND + 1),
            Some(true)
        );
        clock.update(app, SECOND + 1).unwrap();
        assert_eq!(clock.label(), b"14:34");
        owner.refresh(resources, files, &mut clock, true, SECOND + HALF_SECOND + 1);
        complete(INFO, Some(a::io_error_not_found), 0);
        peer(|p| p.pending[INFO].as_mut().unwrap().wrong_owner = true);
        assert_eq!(
            owner.refresh(resources, files, &mut clock, true, SECOND + HALF_SECOND + 2),
            Some(true)
        );
        peer(|p| {
            p.sequence[READ] += 1;
            p.sequence[INFO] += 1;
            p.bytes = UTC.to_vec();
        });
        // Both the retry and directory-poll deadlines have now elapsed.
        owner.refresh(resources, files, &mut clock, true, 2 * SECOND + 3);
        complete(READ, None, 1);
        complete(INFO, Some(a::io_error_not_found), 0);
        assert!(!owner.drain());
        assert!(owner.bytes.is_none());
        assert!(owner.drain());
        assert!(owner.bytes.is_some());
        clock.update(app, 2 * SECOND + 4).unwrap();
        assert_eq!(clock.label(), b"14:34");
    });
}
#[test]
fn stale_mount_cursor_falls_back_without_rebinding_or_reusing_old_reply() {
    with(|app, resources, files| {
        let mut clock = Clock::new().unwrap();
        let mut owner = Owner::new(88).unwrap();
        owner.initialize(files, true, 0);
        owner.refresh(resources, files, &mut clock, true, 0);
        peer(|p| p.invalid_cursor = true);
        owner.refresh(resources, files, &mut clock, true, SECOND);
        assert!(!owner.time.watched && !owner.audio.watched);
        complete(READ, None, 0);
        complete(INFO, Some(99), 0);
        assert_eq!(
            owner.refresh(resources, files, &mut clock, true, SECOND + 1),
            None
        );
        clock.update(app, SECOND + 1).unwrap();
        assert_eq!(clock.label(), b"12:34");
        assert_eq!(peer(|p| p.submitted), [2, 2]);
        complete(READ, None, 0);
        complete(INFO, Some(99), 0);
        assert_eq!(
            owner.refresh(resources, files, &mut clock, true, SECOND + 2),
            Some(true)
        );
        clock.update(app, SECOND + 2).unwrap();
        assert_eq!(clock.label(), b"14:34");
        assert_eq!(peer(|p| p.begins), 2);
        stop(&mut owner);
    });
}
#[test]
fn hidden_startup_has_no_read_and_show_closes_the_startup_snapshot_gap() {
    with(|app, resources, files| {
        let mut clock = Clock::new().unwrap();
        clock.apply_bytes(UTC).unwrap();
        let mut owner = Owner::new(88).unwrap();
        owner.initialize(files, false, 0);
        owner.refresh(resources, files, &mut clock, false, 0);
        complete(INFO, Some(1), 0);
        owner.refresh(resources, files, &mut clock, false, 5 * SECOND);
        clock.update(app, 5 * SECOND).unwrap();
        assert_eq!(
            peer(|p| (p.submitted[READ], p.polls[READ], p.calendars)),
            (0, 0, 0)
        );
        // The initial config was observed before journal admission. Even with
        // no subsequent journal event, showing must read the current bytes.
        owner.refresh(resources, files, &mut clock, true, 5 * SECOND + 1);
        complete(READ, None, 0);
        owner.refresh(resources, files, &mut clock, true, 5 * SECOND + 2);
        clock.update(app, 5 * SECOND + 2).unwrap();
        assert_eq!(clock.label(), b"14:34");
        assert_eq!(peer(|p| p.submitted[READ]), 1);
        stop(&mut owner);
    });
}
#[test]
fn unchanged_oversized_file_waits_for_change_and_replacement_recovers() {
    with(|app, resources, files| {
        let mut clock = Clock::new().unwrap();
        clock.apply_bytes(UTC).unwrap();
        let mut owner = Owner::new(88).unwrap();
        owner.initialize(files, true, 0);
        owner.refresh(resources, files, &mut clock, true, 0);
        complete(READ, Some(READ_TOO_LARGE), 0);
        complete(INFO, Some(10), 0);
        for n in 1..=5 {
            owner.refresh(resources, files, &mut clock, true, n * SECOND);
            clock.update(app, n * SECOND).unwrap();
        }
        assert_eq!(clock.label(), b"12:34");
        assert_eq!(peer(|p| p.submitted), [1, 1]);
        peer(|p| p.sequence[READ] += 1);
        owner.refresh(resources, files, &mut clock, true, 6 * SECOND);
        complete(READ, None, 0);
        owner.refresh(resources, files, &mut clock, true, 6 * SECOND + 1);
        clock.update(app, 6 * SECOND + 1).unwrap();
        assert_eq!(clock.label(), b"14:34");
        assert_eq!(peer(|p| p.submitted), [2, 1]);
        stop(&mut owner);
    });
}
