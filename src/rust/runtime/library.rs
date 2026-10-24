//! Checked library-helper call protocol and stable ordinary native contexts.
//! No private Zig App/Draw/Bundle layout is reconstructed by Rust callers.
use alloc::vec::Vec;
use core::{mem::MaybeUninit, ptr};
use r4os_library_helpers::{Error as CallError, raw as c};
use r4os_sdk::{App, abi as a};
pub(crate) fn result(slot: c::R4LibrariesSlot) -> Result<c::R4LibrariesSlot, CallError> {
    if slot.status != 0 {
        return Err(CallError::Transport(slot.status));
    }
    if slot.flags & 2 != 0 {
        if slot.ptr.is_null() || slot.length > 256 {
            return Err(CallError::InvalidProvider);
        }
        // This boundary returns resident native error-name literals, as the
        // library's ordinary higher Rust consumers also require.
        let bytes =
            unsafe { core::slice::from_raw_parts(slot.ptr.cast::<u8>(), slot.length as usize) };
        return Err(CallError::Native(
            core::str::from_utf8(bytes).map_err(|_| CallError::InvalidProvider)?,
        ));
    }
    Ok(slot)
}
pub(crate) fn required(slot: c::R4LibrariesSlot) -> Result<c::R4LibrariesSlot, CallError> {
    let slot = result(slot)?;
    if slot.flags & 1 == 0 {
        return Err(CallError::Unavailable);
    }
    Ok(slot)
}
pub(crate) fn scalar(slot: c::R4LibrariesSlot) -> Result<u64, CallError> {
    Ok(required(slot)?.bits)
}
#[derive(Debug)]
pub enum Error {
    Memory,
    Call(CallError),
}
impl From<CallError> for Error {
    fn from(value: CallError) -> Self {
        Self::Call(value)
    }
}
struct Native {
    app: MaybeUninit<c::R4Lib_r4os_app_contract_App>,
    system: MaybeUninit<c::R4Lib_r4os_r4sys_Context>,
    draw: MaybeUninit<c::R4Lib_r4os_r4draw_Context>,
}
pub struct Context<'app, 'start> {
    _app: &'app App<'start>,
    native: Vec<Native>,
    drawing: bool,
}
impl<'app, 'start> Context<'app, 'start> {
    pub fn new(app: &'app App<'start>) -> Result<Self, Error> {
        let mut native = Vec::new();
        native.try_reserve_exact(1).map_err(|_| Error::Memory)?;
        native.push(Native {
            app: MaybeUninit::uninit(),
            system: MaybeUninit::uninit(),
            draw: MaybeUninit::uninit(),
        });
        let pointer = native.as_mut_ptr();
        let n = unsafe { &mut *pointer };
        let status = scalar(unsafe {
            c::R4Library_application(
                0,
                n.app.as_mut_ptr(),
                app.start_context(),
                u32::from(a::R4AppProfile::console.0),
            )
        })? as i32;
        if status != 0 {
            return Err(Error::Call(CallError::Transport(status)));
        }
        required(unsafe {
            c::R4Library_applicationSystem(0, n.app.as_ptr(), n.system.as_mut_ptr())
        })?;
        let drawing = result(unsafe {
            c::R4Library_applicationDraw(0, n.app.as_ptr(), n.draw.as_mut_ptr())
        })?
        .flags
            & 1
            != 0;
        Ok(Self {
            _app: app,
            native,
            drawing,
        })
    }
    pub(crate) fn system(&self) -> *mut c::R4Lib_r4os_r4sys_Context {
        // The original native context stores an immutable pointer into this
        // allocation. Calls borrow only its context, not the Rust App/owner.
        unsafe {
            ptr::addr_of!((*self.native.as_ptr()).system)
                .cast::<c::R4Lib_r4os_r4sys_Context>()
                .cast_mut()
        }
    }
    pub(crate) fn draw(&self) -> Result<*const c::R4Lib_r4os_r4draw_Context, CallError> {
        if !self.drawing {
            return Err(CallError::Unavailable);
        }
        Ok(unsafe {
            ptr::addr_of!((*self.native.as_ptr()).draw).cast::<c::R4Lib_r4os_r4draw_Context>()
        })
    }
}
