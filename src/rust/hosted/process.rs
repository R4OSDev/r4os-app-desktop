//! Instance-local process operations through the normal SDK resource owner.
//! Every operation carries the exact handle generation. No display-slot ID
//! or inventory-row ID is promoted into a lifecycle identity.
use core::{ffi::CStr, marker::PhantomData};
use r4os_contract as a;
use r4os_sdk::{
    App, c_facades as c,
    path::FilePath,
    resources::Wait,
    storage::{Error, Result},
};

pub struct Programs<'app> {
    facade: c::R4App,
    _app: PhantomData<&'app App<'app>>,
    _local: PhantomData<*mut ()>,
}
pub fn valid(handle: a::ProgramProcessHandle) -> bool {
    handle.instance_id != 0 && handle.generation != 0 && handle.reserved == 0
}
pub fn same(a: a::ProgramProcessHandle, b: a::ProgramProcessHandle) -> bool {
    a.instance_id == b.instance_id && a.generation == b.generation && a.reserved == b.reserved
}
pub fn gone(code: i32) -> bool {
    code == a::program_handle_error_stale || code == a::program_handle_error_not_found
}
fn exact(code: i32) -> Result<()> {
    if code == a::program_handle_ok {
        Ok(())
    } else {
        Err(Error::Raw(code))
    }
}
impl<'app> Programs<'app> {
    pub fn new(app: &'app App<'_>) -> Result<Self> {
        let mut facade = unsafe { core::mem::zeroed() };
        exact(unsafe { c::R4SdkAppFacade(app.start_context(), &mut facade) })?;
        Ok(Self {
            facade,
            _app: PhantomData,
            _local: PhantomData,
        })
    }
    fn resources(&self) -> c::R4Resources {
        // Native SDK calls read this admitted App facade. They do not retain
        // the resources temporary or this Rust reference after the call.
        c::R4Resources {
            app: core::ptr::from_ref(&self.facade).cast_mut(),
        }
    }
    pub fn class(&self, path: &FilePath, policy: a::LaunchPolicy) -> Result<a::R4XStartAppClass> {
        match unsafe {
            c::r4_resources_program_class(
                &mut self.resources(),
                path.as_c_str().as_ptr().cast(),
                policy.0,
            )
        } {
            1 => Ok(a::R4XStartAppClass::console),
            2 => Ok(a::R4XStartAppClass::gui),
            code if code < 0 => Err(Error::Raw(code)),
            _ => Err(Error::InvalidProvider),
        }
    }
    fn borrowed(&self, handle: a::ProgramProcessHandle) -> Result<c::R4Process> {
        if !valid(handle) {
            return Err(Error::Raw(a::program_handle_error_stale));
        }
        let mut process = unsafe { core::mem::zeroed() };
        exact(unsafe {
            c::r4_resources_borrow_process(&mut self.resources(), &handle, &mut process)
        })?;
        Ok(process)
    }
    pub fn spawn(
        &self,
        path: &FilePath,
        args: &CStr,
        policy: a::LaunchPolicy,
        console_host: Option<a::ConsoleHostKind>,
    ) -> Result<a::ProgramProcessHandle> {
        let path = c::R4PathZ {
            ptr: path.as_c_str().as_ptr().cast(),
            length: u16::try_from(path.as_bytes().len()).map_err(|_| Error::Length)?,
        };
        let mut process = unsafe { core::mem::zeroed() };
        let code = unsafe {
            if let Some(host) = console_host {
                c::r4os_spawn_with_console_host(
                    &mut self.resources(),
                    path,
                    args.as_ptr().cast(),
                    policy.0,
                    host.0,
                    &mut process,
                )
            } else {
                c::r4_resources_spawn(
                    &mut self.resources(),
                    path,
                    args.as_ptr().cast(),
                    policy.0,
                    &mut process,
                )
            }
        };
        exact(code)?;
        let handle = a::ProgramProcessHandle {
            instance_id: process.raw,
            reserved: process.handle_reserved,
            generation: process.generation,
        };
        if !valid(handle) {
            return Err(Error::InvalidProvider);
        }
        Ok(handle)
    }
    pub fn status(
        &self,
        handle: a::ProgramProcessHandle,
    ) -> Result<Option<a::ProgramInstanceInfo>> {
        let mut process = self.borrowed(handle)?;
        let mut out = a::ProgramInstanceInfo::default();
        let code = unsafe { c::r4_process_status(&mut process, &mut out) };
        if gone(code) {
            return Ok(None);
        }
        exact(code)?;
        Ok(Some(out))
    }
    pub fn request_close(&self, handle: a::ProgramProcessHandle) -> Result<()> {
        let mut process = self.borrowed(handle)?;
        exact(unsafe { c::r4_process_request_close(&mut process) })
    }
    pub fn kill(&self, handle: a::ProgramProcessHandle) -> Result<()> {
        let mut process = self.borrowed(handle)?;
        exact(unsafe { c::r4_process_kill(&mut process) })
    }
    /// # Safety
    /// The caller owns this exact process lifecycle: it spawned the child or
    /// completed the current Desktop host adoption protocol. The diagnostic
    /// owner keeps its own reap authority. All consumers must retire their
    /// process-owned native state before treating a reap as final cleanup.
    /// Merely borrowing a handle or seeing an inventory row is insufficient.
    pub unsafe fn reap_host_owned(
        &self,
        handle: a::ProgramProcessHandle,
    ) -> Result<Wait<a::ProgramProcessCompletion>> {
        let mut process = self.borrowed(handle)?;
        // This explicit authority admission is local to the Desktop owner.
        // The generic SDK keeps ordinary borrowed processes non-owning.
        process.owned = 1;
        let mut out = a::ProgramProcessCompletion::default();
        let code = unsafe { c::r4os_process_reap(&mut process, &mut out) };
        if code == a::program_handle_error_would_block {
            return Ok(Wait::WouldBlock);
        }
        exact(code)?;
        Ok(Wait::Completed(out))
    }
}
