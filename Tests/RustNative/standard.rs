//! One resident original R4STD provider for native Desktop settings fixtures.
//! Serialization belongs only to the host tests, never the Desktop runtime.
use r4os_library_helpers::standard::Standard;
use r4os_sdk::{App, ImportSpec, StartupInfo, abi};
use std::sync::{Mutex, OnceLock};
static LOCK: Mutex<()> = Mutex::new(());
static CONTEXT: OnceLock<usize> = OnceLock::new();
unsafe extern "C" {
    fn r4desk_standard_context() -> *const abi::R4XStartContext;
}
pub(crate) fn with<R>(case: impl FnOnce(&Standard<'_>) -> R) -> R {
    let _guard = LOCK.lock().expect("native R4STD fixture lock");
    let raw = *CONTEXT.get_or_init(|| unsafe { r4desk_standard_context() as usize })
        as *const abi::R4XStartContext;
    let context = unsafe { &*raw };
    let imports =
        unsafe { core::slice::from_raw_parts(context.imports as *const abi::R4XStartImport, 6) };
    let sys = unsafe { &*(imports[0].table as *const abi::R4XStartR4Sys) };
    let names: [&'static [u8]; 6] = [
        b"Query",
        b"TEXT_V1",
        b"SETTINGS_V1",
        b"DATE_V1",
        b"TIME_V1",
        b"CONFIG_V1",
    ];
    let specs: [ImportSpec; 6] = core::array::from_fn(|i| ImportSpec {
        module_name: if i == 0 { b"R4SYS" } else { b"R4STD" },
        symbol_name: names[i],
        minimum: 1,
        optional: false,
        group_id: if i == 0 { 1 } else { 0 },
        magic: if i == 0 { sys.magic } else { 0 },
        version: if i == 0 { sys.abi_version } else { 0 },
        size: if i == 0 { sys.size } else { 0 },
    });
    let expected = StartupInfo {
        app_class: abi::R4XStartAppClass::console,
        required_groups: 2,
        imports: &specs,
    };
    let app = unsafe { App::from_raw(raw, &expected, c"DESKTOPFIX") }
        .expect("original native loader context");
    let standard = Standard::open(&app).expect("original R4STD provider");
    case(&standard)
}
