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
const fn import(
    module: &'static core::ffi::CStr,
    symbol: &'static core::ffi::CStr,
    minimum: u32,
    group_id: u32,
) -> ImportSpec {
    ImportSpec {
        module_name: module.as_ptr().cast(),
        symbol_name: symbol.as_ptr().cast(),
        minimum,
        optional: 0,
        group_id,
        reserved: 0,
    }
}
pub(crate) static EXPECTED: StartupInfo<'static> = StartupInfo {
    profile: 0,
    app_class: abi::R4XStartAppClass::console.0,
    required_groups: 2,
    imports: &[
        import(c"R4SYS", c"Query2", 2, 1),
        import(c"R4STD", c"TEXT_V1", 1, 0),
        import(c"R4STD", c"SETTINGS_V1", 1, 0),
        import(c"R4STD", c"DATE_V1", 1, 0),
        import(c"R4STD", c"TIME_V1", 1, 0),
        import(c"R4STD", c"CONFIG_V1", 1, 0),
        import(c"R4STD", c"Query2", 2, 0),
    ],
};
pub(crate) fn with<R>(case: impl FnOnce(&Standard<'_>) -> R) -> R {
    with_app(|_, standard| case(standard))
}
pub(crate) fn with_app<R>(case: impl FnOnce(&App2<'_>, &Standard<'_>) -> R) -> R {
    let _guard = LOCK.lock().expect("native R4STD fixture lock");
    let raw = *CONTEXT.get_or_init(|| unsafe { r4desk_standard_context2() as usize })
        as *const abi::generation2::R4XStartContext2;
    let context = unsafe { &*raw };
    let imports =
        unsafe { core::slice::from_raw_parts(context.imports as *const abi::R4XStartImport, 7) };
    assert_eq!(imports.len(), context.import_count as usize);
    let app = unsafe { App2::from_raw(raw, &EXPECTED, c"DESKTOPFIX") }
        .expect("original native loader context");
    let standard = Standard::open2(&app).expect("original R4STD provider");
    case(&app, &standard)
}
