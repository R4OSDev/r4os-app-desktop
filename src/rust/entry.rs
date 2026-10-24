//! Normal SDK application entry; the manifest supplies all runtime imports.
use crate::runtime::{bootstrap, desktop::Coordinator};
use r4os_sdk as __r4os_startup;
static EXPECTED: r4os_sdk::StartupInfo<'static> = include!(env!("R4OS_STARTUP_INFO_FILE"));
r4os_sdk::app!(run);
fn run(app: &mut r4os_sdk::App<'_>) -> i32 {
    if !bootstrap::arguments_supported(app.args()) {
        app.write("Desktop: unsupported arguments; use Spectra for diagnostics\n");
        return 2;
    }
    match bootstrap::admitted(
        app,
        &mut Coordinator {
            expected: &EXPECTED,
        },
    ) {
        Ok(code) => code,
        Err(_) => {
            app.write("Desktop: host admission failed\n");
            2
        }
    }
}
