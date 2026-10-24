//! A window broker may borrow a completed consumer fence after composition.
//! Its exact job and device survive until the broker's Return ACK allows
//! release. Drop never certifies release; foreign calls serialize per device.
use crate::hosted::window_image::Receipt;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use r4gfx as g;
use r4os_contract as a;
pub(crate) struct Loans {
    busy: AtomicBool,
    external: AtomicUsize,
}
impl Loans {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            busy: AtomicBool::new(false),
            external: AtomicUsize::new(0),
        })
    }
    pub fn enter(self: &Arc<Self>) -> Option<Call> {
        self.busy
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| Call(self.clone()))
    }
    pub fn external(&self) -> usize {
        self.external.load(Ordering::Acquire)
    }
}
pub(crate) struct Call(Arc<Loans>);
impl Drop for Call {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}
#[must_use = "release only after the exact broker fence loan has ended"]
pub struct Consumer {
    release: g::DeviceV1_job_releaseFn,
    device: g::R4GfxDevice,
    job: g::R4GfxJob,
    fence: a::GfxFence,
    result: u32,
    loans: Arc<Loans>,
    active: bool,
}
impl Consumer {
    /// # Safety
    /// Unique handoff of this actual physically retired job. The enclosing
    /// Device's unsafe loader-residence admission remains in force; caller
    /// never releases or duplicates the transferred native job handle.
    pub(crate) unsafe fn new(
        release: g::DeviceV1_job_releaseFn,
        device: g::R4GfxDevice,
        job: g::R4GfxJob,
        fence: g::R4GfxCopyFence,
        result: u32,
        loans: Arc<Loans>,
    ) -> Option<Self> {
        loans
            .external
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .ok()?;
        Some(Self {
            release,
            device,
            job,
            fence: a::GfxFence {
                slot: fence.slot,
                adapter_id: fence.adapter_id,
                timeline: fence.timeline,
                point: fence.point,
                device_generation: fence.device_generation,
                reset_generation: fence.reset_generation,
            },
            result,
            loans,
            active: true,
        })
    }
}
// Each actual callback is protected by the same device-local nonblocking
// call gate used by bind/close, including when cold preparation runs elsewhere.
unsafe impl Receipt for Consumer {
    fn release(&mut self) -> bool {
        if !self.active {
            return true;
        }
        let Some(_call) = self.loans.enter() else {
            return false;
        };
        if unsafe { (self.release)(&self.device, &self.job) } != g::status_ok {
            return false;
        }
        self.active = false;
        self.loans.external.fetch_sub(1, Ordering::AcqRel);
        true
    }
    fn fence(&self) -> a::GfxFence {
        self.fence
    }
    fn result(&self) -> u32 {
        self.result
    }
}
#[cfg(test)]
#[path = "../../../../Tests/RustNative/gpu_receipt.rs"]
mod tests;
