//! Real Desktop worker, R4STD writeback, atomic document save and layout codec.
//! The host runs each scheduled worker on the next sleep to select the ordering.
use crate::shell::{
    desktop_items::{FsKind, Item, Items},
    desktop_layout::{Layout, Position},
    layout_writeback::Owner,
    persistence::LAYOUT,
};
use r4os_sdk::{abi as a, resources::Resources};
use std::{
    collections::BTreeMap,
    ffi::CStr,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
    },
};
static FILES: LazyLock<Mutex<BTreeMap<Vec<u8>, Vec<u8>>>> = LazyLock::new(Mutex::default);
static PENDING: Mutex<Option<(a::ThreadEntryFn, u64)>> = Mutex::new(None);
static WRITES: AtomicUsize = AtomicUsize::new(0);
static FAIL: AtomicBool = AtomicBool::new(false);
static CODE: AtomicI32 = AtomicI32::new(0);
unsafe extern "C" fn create(
    entry: a::ThreadEntryFn,
    arg: u64,
    _: u64,
    _: u32,
    out: *mut a::ProgramJoinHandle,
) -> i32 {
    assert!(PENDING.lock().unwrap().replace((entry, arg)).is_none());
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
unsafe extern "C" fn sleep(_: u64) {
    let (entry, arg) = PENDING
        .lock()
        .unwrap()
        .take()
        .expect("a pending layout worker");
    CODE.store(unsafe { entry(arg) }, Ordering::Relaxed);
}
unsafe extern "C" fn join(_: *const a::ProgramJoinHandle, _: u64, code: *mut i32) -> i32 {
    unsafe {
        code.write(CODE.load(Ordering::Relaxed));
    }
    0
}
unsafe extern "C" fn read(path: *const u8, out: *mut u8, cap: u32) -> i32 {
    let files = FILES.lock().unwrap();
    let Some(data) = files.get(unsafe { CStr::from_ptr(path.cast()) }.to_bytes()) else {
        return -3;
    };
    let n = data.len().min(cap as usize);
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), out, n);
    }
    n as i32
}
unsafe extern "C" fn write(path: *const u8, data: *const u8, len: u32) -> i32 {
    WRITES.fetch_add(1, Ordering::Relaxed);
    if FAIL.load(Ordering::Relaxed) {
        return -1;
    }
    FILES.lock().unwrap().insert(
        unsafe { CStr::from_ptr(path.cast()) }.to_bytes().to_vec(),
        unsafe { core::slice::from_raw_parts(data, len as usize) }.to_vec(),
    );
    len as i32
}
unsafe extern "C" fn remove(path: *const u8) -> i32 {
    FILES
        .lock()
        .unwrap()
        .remove(unsafe { CStr::from_ptr(path.cast()) }.to_bytes())
        .map_or(-3, |_| 0)
}
unsafe extern "C" fn rename(from: *const u8, to: *const u8) -> i32 {
    let mut files = FILES.lock().unwrap();
    let Some(bytes) = files.remove(unsafe { CStr::from_ptr(from.cast()) }.to_bytes()) else {
        return -3;
    };
    files.insert(
        unsafe { CStr::from_ptr(to.cast()) }.to_bytes().to_vec(),
        bytes,
    );
    0
}
unsafe extern "C" fn directory(_: *const u8) -> i32 {
    0
}
#[test]
fn newest_layout_is_read_back_after_system_transition() {
    unsafe extern "C" {
        fn r4desk_swap_test_system(table: *mut a::generation2::R4XStartR4Sys2);
    }
    struct Restore(a::generation2::R4XStartR4Sys2);
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe { r4desk_swap_test_system(&mut self.0) };
        }
    }
    crate::standard_fixture::with_app(|app, standard| {
        let sys = a::generation2::R4XStartR4Sys2 {
            thread_create_handle: create as *const () as _,
            thread_handle_join: join as *const () as _,
            sleep_ticks: sleep as *const () as _,
            file_read: read as *const () as _,
            file_write: write as *const () as _,
            file_rename: rename as *const () as _,
            file_delete: remove as *const () as _,
            dir_create: directory as *const () as _,
            ..Default::default()
        };
        let mut saved = Restore(sys);
        unsafe { r4desk_swap_test_system(&mut saved.0) };
        let expected = &crate::standard_fixture::EXPECTED;
        let resources = Resources::new2(app).unwrap();
        let mut owner = Owner::new(standard, 100).unwrap();
        let mut items = Items::new().unwrap();
        let path = br"C:\R4OS\DESKTOP\ORDER.TXT";
        items.add(Item::new(path, FsKind::File));
        let point = |items: &mut Items, x, y| {
            let item = items.get_mut(0).unwrap();
            item.x = x;
            item.y = y;
        };
        macro_rules! transition {
            () => {
                unsafe {
                    owner.save_before_transition(app, &resources, standard, expected, &items, 100)
                }
            };
        }
        let read_position = || {
            let mut reopened = Layout::default();
            let files = FILES.lock().unwrap();
            reopened.load(standard, &files[LAYOUT]).unwrap();
            reopened.position(path).unwrap()
        };
        point(&mut items, 10, 20);
        owner.mark(1).unwrap();
        unsafe {
            owner.flush(app, &resources, expected, standard, &items, 1, true);
        }
        assert!(owner.busy());
        point(&mut items, 50, 90);
        owner.mark(2).unwrap();
        assert!(transition!());
        assert_eq!(read_position(), Position { x: 50, y: 90 });
        assert_eq!(WRITES.load(Ordering::Relaxed), 2);
        assert!(transition!());
        assert_eq!(WRITES.load(Ordering::Relaxed), 2);
        point(&mut items, 80, 100);
        owner.mark(3).unwrap();
        FAIL.store(true, Ordering::Relaxed);
        assert!(!transition!());
        assert!(owner.dirty());
        assert_eq!(read_position(), Position { x: 50, y: 90 });
        FAIL.store(false, Ordering::Relaxed);
        assert!(transition!());
        assert_eq!(read_position(), Position { x: 80, y: 100 });
    });
}
