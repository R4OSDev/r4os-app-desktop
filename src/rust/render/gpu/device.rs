//! Stable caller-owned R4GFX storage. Thread handoff moves opaque identities,
//! never an App/Interface loan. A refused Close retains the actual allocation.
use super::receipt::{Call, Consumer, Loans};
use alloc::alloc::{alloc_zeroed, dealloc};
use alloc::sync::Arc;
use core::{alloc::Layout, ptr::NonNull};
use r4gfx as g;
use r4os_sdk::App;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unavailable,
    Memory,
    WrongOwner,
    Closed,
    Invalid,
    Native(i32),
}
pub struct OpenError {
    pub error: Error,
    pub retained: Option<Device>,
}
impl core::fmt::Debug for OpenError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OpenError")
            .field("error", &self.error)
            .field("retained", &self.retained.is_some())
            .finish()
    }
}
#[must_use = "close the actual provider device before releasing its stable storage"]
pub struct Device {
    context: usize,
    storage: Option<NonNull<u8>>,
    layout: Layout,
    handle: Option<g::R4GfxDevice>,
    loans: Arc<Loans>,
    release_job: g::DeviceV1_job_releaseFn,
}
// R4GFX declares handle-serialized operations. Device contains no Rust App or
// Interface loan or borrowed caller buffers. Only its owner moves
// between tasks; bind() needs exclusive access and creates a local Interface.
// The enclosing R4X keeps loader providers resident until every task joins.
unsafe impl Send for Device {}
/// Short native-call scope. Drop this scope before invoking window/broker
/// cleanup callbacks: an older Consumer may need this same device call gate.
/// Native progress and external-fence handoff are separate coordinator steps.
pub struct Session<'o, 'a> {
    owner: &'o mut Device,
    pub(crate) client: g::DeviceV1Client<'a>,
    pub(crate) colors: g::ColorV1Client<'a>,
    _call: Call,
}
impl Device {
    /// # Safety
    /// The actual loader context, provider tables and callbacks remain
    /// resident until successful Close, including across serialized task
    /// handoff. Drain the owning transaction before its R4X entry returns.
    pub unsafe fn open(app: &App<'_>, adapter: u32) -> Result<Self, OpenError> {
        let fail = |error| OpenError {
            error,
            retained: None,
        };
        let client = g::DeviceV1Client::init(app).map_err(|_| fail(Error::Unavailable))?;
        let _ = g::ColorV1Client::init(app).map_err(|_| fail(Error::Unavailable))?;
        let interface = r4os_sdk::r4l::Interface::new(
            app,
            g::MODULE_NAME,
            g::device_v1_export_name,
            g::device_v1_expectation,
        )
        .map_err(|_| fail(Error::Unavailable))?;
        let release_job = unsafe {
            interface.function::<g::DeviceV1_job_releaseFn>(core::mem::offset_of!(
                g::DeviceV1,
                job_release
            ) as u32)
        }
        .ok_or_else(|| fail(Error::Unavailable))?;
        let loans = Loans::new();
        let count = usize::try_from(unsafe { client.storage_size() })
            .ok()
            .filter(|v| *v != 0)
            .ok_or_else(|| fail(Error::Memory))?;
        let layout = Layout::from_size_align(count, g::device_storage_alignment as usize)
            .map_err(|_| fail(Error::Memory))?;
        let storage =
            NonNull::new(unsafe { alloc_zeroed(layout) }).ok_or_else(|| fail(Error::Memory))?;
        let mut owner = Self {
            context: app.start_context() as *const _ as usize,
            storage: Some(storage),
            layout,
            handle: None,
            loans,
            release_job,
        };
        let config = g::R4GfxDeviceConfig {
            version: 1,
            size: core::mem::size_of::<g::R4GfxDeviceConfig>() as u32,
            storage_address: storage.as_ptr() as u64,
            storage_bytes: count as u64,
            start_context: owner.context as u64,
            preferred_adapter: adapter,
            flags: 0,
        };
        let mut handle = g::R4GfxDevice::default();
        let rc = unsafe { client.device_open(&config, &mut handle) };
        if handle.address != 0 {
            owner.handle = Some(handle);
        }
        if rc != g::status_ok {
            return Err(OpenError {
                error: Error::Native(rc),
                retained: owner.handle.is_some().then_some(owner),
            });
        }
        if handle.address != storage.as_ptr() as u64 || handle.generation == 0 {
            return Err(OpenError {
                error: Error::Invalid,
                retained: Some(owner),
            });
        }
        Ok(owner)
    }
    pub fn bind<'o, 'a>(&'o mut self, app: &'a App<'_>) -> Result<Session<'o, 'a>, Error> {
        if self.context != app.start_context() as *const _ as usize {
            return Err(Error::WrongOwner);
        }
        if self.handle.is_none() {
            return Err(Error::Closed);
        }
        let client = g::DeviceV1Client::init(app).map_err(|_| Error::Unavailable)?;
        let colors = g::ColorV1Client::init(app).map_err(|_| Error::Unavailable)?;
        let call = self.loans.enter().ok_or(Error::Native(g::status_busy))?;
        Ok(Session {
            owner: self,
            client,
            colors,
            _call: call,
        })
    }
    pub fn close(&mut self, app: &App<'_>) -> Result<bool, Error> {
        let Some(handle) = self.handle else {
            return Ok(true);
        };
        if self.context != app.start_context() as *const _ as usize {
            return Err(Error::WrongOwner);
        }
        if self.loans.external() != 0 {
            return Ok(false);
        }
        let Some(_call) = self.loans.enter() else {
            return Ok(false);
        };
        let client = g::DeviceV1Client::init(app).map_err(|_| Error::Unavailable)?;
        let rc = unsafe { client.device_close(&handle) };
        if rc == g::status_busy {
            return Ok(false);
        }
        if rc != g::status_ok {
            return Err(Error::Native(rc));
        }
        self.handle = None;
        self.free_storage();
        Ok(true)
    }
    pub fn live(&self) -> bool {
        self.handle.is_some()
    }
    pub(crate) fn receipt_counts(&self) -> Arc<Loans> { self.loans.clone() }
    pub(crate) fn identity(&self) -> Option<g::R4GfxDevice> { self.handle }
    fn free_storage(&mut self) {
        if let Some(storage) = self.storage.take() {
            unsafe {
                dealloc(storage.as_ptr(), self.layout);
            }
        }
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        if self.handle.is_none() {
            self.free_storage();
        }
    }
}
impl Session<'_, '_> {
    pub(crate) fn raw(&self) -> &g::R4GfxDevice {
        self.owner.handle.as_ref().unwrap()
    }
    pub fn external_receipts(&self) -> usize {
        self.owner.loans.external()
    }
    /// # Safety
    /// The exact job is terminal with no held physical resources; unique
    /// ownership transfers to this receipt, then to the window broker loan.
    pub(crate) unsafe fn consumer(
        &self,
        job: g::R4GfxJob,
        fence: g::R4GfxCopyFence,
        result: u32,
    ) -> Option<Consumer> {
        unsafe {
            Consumer::new(
                self.owner.release_job,
                *self.raw(),
                job,
                fence,
                result,
                self.owner.loans.clone(),
            )
        }
    }
    pub fn refresh(&self) -> Result<g::R4GfxDeviceInfo, Error> {
        let mut value = g::R4GfxDeviceInfo::default();
        let rc = unsafe { self.client.device_refresh(self.raw(), &mut value) };
        if rc == g::status_ok {
            Ok(value)
        } else {
            Err(Error::Native(rc))
        }
    }
}
