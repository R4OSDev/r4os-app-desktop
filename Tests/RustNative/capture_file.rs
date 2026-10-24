//! The five original SDK-bound capture fault groups, extended at Missing so
//! write/finish/abort cannot acknowledge a nonexistent native stream.
use super::*;
use core::ffi::CStr;
use r4os_sdk::{App, ImportSpec, StartupInfo, storage::System};
use std::sync::Mutex;
static CASE: Mutex<()> = Mutex::new(());
#[derive(Clone, Copy)]
struct Model {
    begin: i32,
    write: i32,
    finish: i32,
    publish: i32,
    abort: i32,
    count: u64,
    finishes: u32,
    publishes: u32,
    aborts: u32,
    stage: [u8; 64],
    stage_len: usize,
}
const EMPTY: Model = Model {
    begin: 0,
    write: 0,
    finish: 0,
    publish: 0,
    abort: 0,
    count: 0,
    finishes: 0,
    publishes: 0,
    aborts: 0,
    stage: [0; 64],
    stage_len: 0,
};
static STATE: Mutex<Model> = Mutex::new(EMPTY);
unsafe fn bytes<'a>(p: *const u8) -> &'a [u8] {
    unsafe { CStr::from_ptr(p.cast()) }.to_bytes()
}
unsafe extern "C" fn begin(p: *const u8, flags: u32) -> i32 {
    let value = unsafe { bytes(p) };
    assert_eq!(value.len(), 20);
    assert!(value.starts_with(b"C:\\TEMP\\") && value.ends_with(b".TMP"));
    assert!(value[8..16].iter().all(u8::is_ascii_hexdigit));
    assert_eq!(flags, a::file_stream_open_create);
    let mut m = STATE.lock().unwrap();
    m.stage[..value.len()].copy_from_slice(value);
    m.stage_len = value.len();
    m.begin
}
unsafe extern "C" fn write(p: *const u8, offset: u64, _: *const u8, len: u32, flags: u32) -> i32 {
    let mut m = STATE.lock().unwrap();
    assert_eq!(unsafe { bytes(p) }, &m.stage[..m.stage_len]);
    assert_eq!(offset, m.count);
    assert_eq!(flags, 0);
    if m.write != 0 {
        return m.write;
    }
    m.count += len as u64;
    len as i32
}
unsafe extern "C" fn finish(p: *const u8, expected: u64, flags: u32) -> i32 {
    let mut m = STATE.lock().unwrap();
    assert_eq!(unsafe { bytes(p) }, &m.stage[..m.stage_len]);
    assert_eq!(expected, m.count);
    assert!(flags == 0 || flags == 1);
    m.finishes += 1;
    m.finish
}
unsafe extern "C" fn publish(
    target: *const u8,
    stage: *const u8,
    backup: *const u8,
    flags: u32,
) -> i32 {
    let mut m = STATE.lock().unwrap();
    assert_eq!(unsafe { bytes(target) }, b"C:\\TEMP\\CAPTURE.BMP");
    assert_eq!(unsafe { bytes(stage) }, &m.stage[..m.stage_len]);
    let backup = unsafe { bytes(backup) };
    assert_eq!(backup.len(), 20);
    assert_eq!(&backup[..16], &m.stage[..16]);
    assert!(backup.ends_with(b".BAK"));
    assert_eq!(flags, 7);
    assert_eq!(m.finishes, 1);
    m.publishes += 1;
    m.publish
}
unsafe extern "C" fn abort(p: *const u8) -> i32 {
    let mut m = STATE.lock().unwrap();
    assert_eq!(unsafe { bytes(p) }, &m.stage[..m.stage_len]);
    m.aborts += 1;
    m.abort
}
fn with(case: impl FnOnce(&Files<'_>, &Names)) {
    with_app(|files, names, _| case(files, names));
}
fn with_app(case: impl FnOnce(&Files<'_>, &Names, &App<'_>)) {
    let _guard = CASE.lock().unwrap();
    *STATE.lock().unwrap() = EMPTY;
    let table = a::R4XStartR4Sys {
        file_stream_begin: begin as *const () as usize,
        file_stream_write: write as *const () as usize,
        file_stream_finish: finish as *const () as usize,
        file_stream_abort: abort as *const () as usize,
        file_replace_atomic: publish as *const () as usize,
        ..a::R4XStartR4Sys::default()
    };
    let imports = [a::R4XStartImport {
        group_id: 1,
        min_version: 1,
        resolved_version: 1,
        flags: a::r4xstart_import_flag_group_interface,
        module_name: c"R4SYS".as_ptr() as u64,
        symbol_name: c"Query".as_ptr() as u64,
        table: core::ptr::from_ref(&table) as u64,
        ..a::R4XStartImport::default()
    }];
    let raw = a::R4XStartContext {
        app_class: a::R4XStartAppClass::console.0,
        instance_id: 86,
        flags: a::r4xstart_flag_imports_valid,
        imports: imports.as_ptr() as u64,
        import_count: 1,
        ..a::R4XStartContext::default()
    };
    let spec = [ImportSpec {
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
        imports: &spec,
    };
    let app = unsafe { App::from_raw(&raw, &expected, c"CAPTUREFIX") }.unwrap();
    let system = System::new(&app).unwrap();
    let files = system.files();
    let paths = names(b"C:\\TEMP\\CAPTURE.BMP").unwrap();
    case(&files, &paths, &app);
}
#[test]
fn original_moved_capture_keeps_exact_stage_through_finish_and_create_only_publish() {
    with(|files, names| {
        let initial = Output::open(files, names).unwrap();
        let mut out = initial;
        assert!(out.write(b"BMdata"));
        out.finish().unwrap();
        assert_eq!(out.phase, Phase::Published);
        assert_eq!(out.bytes(), 6);
        assert!(!out.retained() && out.abort());
        let m = STATE.lock().unwrap();
        assert_eq!((m.finishes, m.publishes, m.aborts), (1, 1, 0));
    });
}
#[test]
fn original_publish_failure_after_flush_aborts_retained_stage() {
    with(|files, names| {
        let mut out = Output::open(files, names).unwrap();
        STATE.lock().unwrap().publish = -1;
        assert!(out.write(b"BMdata"));
        assert_eq!(out.finish(), Err(FinishError::Publish));
        assert_eq!(out.phase, Phase::Finished);
        assert!(out.retained() && out.writer.retained());
        assert!(out.abort());
        assert_eq!(out.phase, Phase::Aborted);
        assert_eq!(
            out.failure,
            Some(Failure::Publish(AtomicReplace::Failed(-1)))
        );
        assert_eq!(STATE.lock().unwrap().aborts, 1);
    });
}
#[test]
fn original_conflict_keeps_primary_and_only_exact_abort_ack_retires_owner() {
    with_app(|files, names, app| {
        let mut out = Output::open(files, names).unwrap();
        {
            let mut m = STATE.lock().unwrap();
            m.publish = -5;
            m.abort = a::file_stream_error_io;
        }
        assert!(out.write(b"BMdata"));
        assert_eq!(out.finish(), Err(FinishError::Publish));
        assert!(!out.abort() && out.retained());
        assert_eq!(out.failure, Some(Failure::Publish(AtomicReplace::Conflict)));
        assert_eq!(
            out.cleanup_error,
            Some(SdkError::Raw(a::file_stream_error_io))
        );
        STATE.lock().unwrap().abort = a::file_stream_error_not_found;
        assert!(!out.abort() && out.retained());
        assert!(out.writer.needs_abort());
        assert_eq!(
            out.cleanup_error,
            Some(SdkError::Raw(a::file_stream_error_not_found))
        );
        let mut retained = out.into_retention().unwrap();
        assert_eq!(
            retained.stage().as_bytes(),
            names.stage.as_path().as_bytes()
        );
        assert!(!retained.abort(app));
        STATE.lock().unwrap().abort = 0;
        assert!(retained.abort(app));
        let aborts = STATE.lock().unwrap().aborts;
        assert!(retained.abort(app));
        assert_eq!(STATE.lock().unwrap().aborts, aborts);
        assert_eq!(
            retained.failure,
            Some(Failure::Publish(AtomicReplace::Conflict))
        );
    });
}
#[test]
fn original_begin_failure_never_aborts_an_unowned_stage() {
    with(|files, names| {
        STATE.lock().unwrap().begin = a::file_stream_error_io;
        assert!(Output::open(files, names).is_err());
        let m = STATE.lock().unwrap();
        assert_eq!((m.finishes, m.publishes, m.aborts), (0, 0, 0));
    });
}
#[test]
fn original_partial_write_and_failed_finish_keep_owner_including_missing() {
    with(|files, names| {
        for code in [a::file_stream_error_io, a::file_stream_error_not_found, 2] {
            *STATE.lock().unwrap() = EMPTY;
            let mut out = Output::open(files, names).unwrap();
            STATE.lock().unwrap().write = code;
            assert!(!out.write(b"partial") && out.retained());
            assert_eq!(
                out.failure,
                Some(Failure::Write(SdkError::Raw(if code > 0 {
                    a::file_stream_error_io
                } else {
                    code
                })))
            );
            assert!(out.abort());
        }
        for code in [a::file_stream_error_io, a::file_stream_error_not_found] {
            *STATE.lock().unwrap() = EMPTY;
            let mut out = Output::open(files, names).unwrap();
            assert!(out.write(b"BMdata"));
            STATE.lock().unwrap().finish = code;
            assert_eq!(out.finish(), Err(FinishError::Flush));
            assert!(out.retained());
            assert_eq!(out.failure, Some(Failure::Finish(SdkError::Raw(code))));
            assert_eq!(STATE.lock().unwrap().publishes, 0);
            assert!(out.abort());
        }
        *STATE.lock().unwrap() = EMPTY;
        let mut writer = files
            .stream_writer(names.stage.as_path(), a::file_stream_open_create)
            .unwrap();
        STATE.lock().unwrap().finish = a::file_stream_error_not_found;
        assert_eq!(
            writer.finish(),
            Err(SdkError::Raw(a::file_stream_error_not_found))
        );
        assert!(writer.needs_abort());
        STATE.lock().unwrap().abort = a::file_stream_error_not_found;
        assert_eq!(
            writer.abort(),
            Err(SdkError::Raw(a::file_stream_error_not_found))
        );
        assert!(writer.needs_abort());
        STATE.lock().unwrap().abort = 0;
        writer.abort().unwrap();
    });
}
