//! One resident original R4STD provider for native Desktop settings fixtures.
//! Serialization belongs only to the host tests, never the Desktop runtime.
use r4os_library_helpers::standard::Standard;
use r4os_sdk::{
    App2, abi,
    app2::{ImportSpec, StartupInfo},
};
use std::sync::{Mutex, OnceLock};
static LOCK: Mutex<()> = Mutex::new(());
static CONTEXT: OnceLock<usize> = OnceLock::new();
pub(crate) unsafe extern "C" fn keep_open(_: *const abi::generation2::R4XStartContext2) -> u32 {
    0
}
unsafe extern "C" {
    fn r4desk_standard_context2() -> *const abi::generation2::R4XStartContext2;
}
pub(crate) fn with<R>(case: impl FnOnce(&Standard<'_>) -> R) -> R {
    let _guard = LOCK.lock().expect("native R4STD fixture lock");
    let raw = *CONTEXT.get_or_init(|| unsafe { r4desk_standard_context2() as usize })
        as *const abi::generation2::R4XStartContext2;
    let context = unsafe { &*raw };
    let imports =
        unsafe { core::slice::from_raw_parts(context.imports as *const abi::R4XStartImport, 7) };
    assert_eq!(imports.len(), context.import_count as usize);
    let names = [
        c"Query2",
        c"TEXT_V1",
        c"SETTINGS_V1",
        c"DATE_V1",
        c"TIME_V1",
        c"CONFIG_V1",
        c"Query2",
    ];
    let specs: [ImportSpec; 7] = core::array::from_fn(|i| ImportSpec {
        module_name: if i == 0 { c"R4SYS" } else { c"R4STD" }.as_ptr().cast(),
        symbol_name: names[i].as_ptr().cast(),
        minimum: if i == 0 || i == 6 { 2 } else { 1 },
        optional: 0,
        group_id: if i == 0 { 1 } else { 0 },
        reserved: 0,
    });
    let expected = StartupInfo {
        profile: 0,
        app_class: abi::R4XStartAppClass::console.0,
        required_groups: 2,
        imports: &specs,
    };
    let app = unsafe { App2::from_raw(raw, &expected, c"DESKTOPFIX") }
        .expect("original native loader context");
    let standard = Standard::open2(&app).expect("original R4STD provider");
    case(&standard)
}
