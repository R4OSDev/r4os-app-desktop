//! Real parallel task through the ordinary SDK C resource facade; completion,
//! timeout and refusal must not move/free its resident payload before join.
use super::*;
use r4os_sdk::{App, ImportSpec, StartupInfo, abi as a};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicI32, AtomicUsize},
    },
    time::{Duration, Instant},
};
static THREAD: Mutex<Option<std::thread::JoinHandle<i32>>> = Mutex::new(None);
static MODE: AtomicI32 = AtomicI32::new(0);
unsafe extern "C" fn create(
    entry: a::ThreadEntryFn,
    arg: u64,
    stack: u64,
    flags: u32,
    out: *mut a::ProgramJoinHandle,
) -> i32 {
    assert_eq!((stack, flags), (128 * 1024, 0));
    if MODE.load(Ordering::Relaxed) == 3 {
        return a::thread_error_unsupported;
    }
    *THREAD.lock().unwrap() = Some(std::thread::spawn(move || unsafe { entry(arg) }));
    unsafe {
        out.write(a::ProgramJoinHandle {
            thread_id: 71,
            instance_id: 86,
            thread_generation: 17,
            instance_generation: 18,
            ..Default::default()
        });
    }
    0
}
unsafe extern "C" fn join(handle: *const a::ProgramJoinHandle, ticks: u64, code: *mut i32) -> i32 {
    let h = unsafe { &*handle };
    assert_eq!(
        (
            h.thread_id,
            h.instance_id,
            h.thread_generation,
            h.instance_generation,
            ticks
        ),
        (71, 86, 17, 18, 0)
    );
    match MODE.load(Ordering::Relaxed) {
        1 => return a::thread_error_timeout,
        2 => return a::thread_error_unsupported,
        _ => {}
    }
    let mut owner = THREAD.lock().unwrap();
    if !owner.as_ref().unwrap().is_finished() {
        return a::thread_error_timeout;
    }
    unsafe {
        code.write(owner.take().unwrap().join().unwrap());
    }
    0
}
struct Work {
    drops: Arc<AtomicUsize>,
    answer: u64,
}
impl Drop for Work {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}
impl Task for Work {
    fn run(&mut self, control: &Control) -> i32 {
        while !control.stopped() {
            std::thread::yield_now();
        }
        self.answer = control.stopped_at();
        17
    }
}
#[test]
fn actual_parallel_payload_requires_join_after_timeout_and_refusal() {
    let table = a::R4XStartR4Sys {
        thread_create_handle: create as *const () as usize,
        thread_handle_join: join as *const () as usize,
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
        instance_id: 86,
        flags: a::r4xstart_flag_imports_valid,
        imports: imports.as_ptr() as u64,
        import_count: 1,
        ..Default::default()
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
    let app = unsafe { App::from_raw(&raw, &expected, c"WORKFIX") }.unwrap();
    let resources = Resources::new(&app).unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let mut worker = unsafe {
        Worker::start(
            &resources,
            Work {
                drops: drops.clone(),
                answer: 0,
            },
            128 * 1024,
        )
    }
    .unwrap();
    assert!(worker.collect().unwrap().is_none());
    worker.stop(1000);
    worker.stop(2000);
    let end = Instant::now() + Duration::from_secs(2);
    while !worker.finished() {
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
    MODE.store(1, Ordering::Relaxed);
    assert!(worker.collect().unwrap().is_none());
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    MODE.store(2, Ordering::Relaxed);
    assert!(worker.collect().is_err());
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    MODE.store(0, Ordering::Relaxed);
    let (code, payload) = worker.collect().unwrap().unwrap();
    assert_eq!((code, payload.answer), (17, 1000));
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    assert!(worker.collect().unwrap().is_none());
    drop(payload);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    MODE.store(3, Ordering::Relaxed);
    let failed = unsafe {
        Worker::start(
            &resources,
            Work {
                drops: drops.clone(),
                answer: 0,
            },
            128 * 1024,
        )
    }
    .err()
    .unwrap();
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    assert_eq!(failed.task.answer, 0);
    drop(failed);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    MODE.store(0, Ordering::Relaxed);
}
