//! Explicit Test-profile startup. Spectra owns the scenarios; this module
//! retains only its spawned process until the real completion/reap receipt.
use crate::hosted::process::{Programs, same};
use r4os_sdk::{App, abi as a, path::FilePath, resources::Wait, storage::{Error, Result}};

#[derive(Default)]
pub struct Owner {
    handle: Option<a::ProgramProcessHandle>,
    close_requested: bool,
}
pub fn requested(app: &App<'_>) -> bool {
    app.args().split(|v| v.is_ascii_whitespace())
        .any(|v| v.eq_ignore_ascii_case(b"/HEADLESS-SUBSYSTEM"))
}
impl Owner {
    pub fn start(programs: &Programs<'_>) -> Result<Self> {
        let path = FilePath::parse(b"C:\\R4OS\\SOFTWARE\\TERMINAL\\DIAG\\SPECTRA.R4X").map_err(|_| Error::Length)?;
        let handle = programs.spawn(&path,
            c"CHECK SUBSYSTEM /JSON /EXPORT:C:\\TEMP\\SUBSYS86.JSONL",
            a::LaunchPolicy::console, Some(a::ConsoleHostKind::none))?;
        Ok(Self { handle: Some(handle), close_requested: false })
    }
    pub fn active(&self) -> bool { self.handle.is_some() }
    pub fn stop(&mut self, programs: &Programs<'_>) {
        if !self.close_requested {
            if let Some(handle) = self.handle {
                if programs.request_close(handle).is_ok() { self.close_requested = true; }
            }
        }
    }
    pub fn poll(&mut self, programs: &Programs<'_>) -> Result<Option<i32>> {
        let Some(handle) = self.handle else { return Ok(None); };
        let Some(info) = programs.status(handle)? else { return Err(Error::InvalidProvider); };
        if info.state != a::ProgramInstanceState::done.0 { return Ok(None); }
        // This exact child was spawned above, so this caller has real reap
        // authority. No observed foreign instance is promoted to ownership.
        match unsafe { programs.reap_host_owned(handle) }? {
            Wait::Completed(done) if same(done.handle, handle) && done.sequence != 0 => {
                self.handle = None;
                Ok(Some(done.exit_code))
            }
            Wait::Completed(_) => Err(Error::InvalidProvider),
            Wait::WouldBlock | Wait::TimedOut => Ok(None),
        }
    }
}
