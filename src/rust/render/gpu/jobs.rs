//! One bounded GPU dependency ledger. Metadata completion never substitutes
//! for physical retirement; exact external jobs move to broker receipts in
//! a second step after the device-call session has ended.
use super::{Error, Result, accepted, device::Session, receipt::Consumer};
use crate::render::layers::CAPACITY;
use alloc::vec::Vec;
use r4gfx as g;
use r4os_contract as a;
pub struct Job {
    pub handle: g::R4GfxJob,
    pub fence: g::R4GfxCopyFence,
    pub upload: Option<usize>,
    pub asset_upload: Option<usize>,
    pub generation: u64,
    pub bytes: u64,
    pub complete: bool,
    pub result: u32,
    fence_known: bool,
    consumer: Option<Consumer>,
    external_taken: bool,
}
#[derive(Clone, Copy)]
pub struct Uploaded {
    pub layer: Option<usize>,
    pub asset: Option<usize>,
    pub generation: u64,
    pub bytes: u64,
}
pub struct Jobs {
    slots: Vec<Option<Job>>,
    pub last: Option<usize>,
    pub stage: Option<usize>,
    pub depth: usize,
    external: [Option<usize>; CAPACITY],
    layer_writes: [Option<usize>; CAPACITY],
    pub retirement_epoch: u64,
    pub uploaded_bytes: u64,
}
impl Jobs {
    pub fn new() -> Result<Self> {
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(g::device_job_capacity as usize)
            .map_err(|_| Error::Limit)?;
        slots.resize_with(g::device_job_capacity as usize, || None);
        Ok(Self {
            slots,
            last: None,
            stage: None,
            depth: 0,
            external: [None; CAPACITY],
            layer_writes: [None; CAPACITY],
            retirement_epoch: 0,
            uploaded_bytes: 0,
        })
    }
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }
    pub fn drained(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }
    pub fn physically_retired(&self) -> bool {
        self.slots.iter().flatten().all(|job| job.complete)
    }
    pub fn reserve(&self) -> Result<usize> {
        self.slots
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Busy)
    }
    pub fn job(&self, index: usize) -> Option<&Job> {
        self.slots.get(index)?.as_ref()
    }
    pub fn job_mut(&mut self, index: usize) -> Option<&mut Job> {
        self.slots.get_mut(index)?.as_mut()
    }
    pub fn dependency(&self) -> Result<&[g::R4GfxCopyFence]> {
        match self.last {
            None => Ok(&[]),
            Some(index) => {
                let job = self.job(index).ok_or(Error::State)?;
                if !job.fence_known {
                    return Err(Error::Graphics);
                }
                Ok(core::slice::from_ref(&job.fence))
            }
        }
    }
    pub fn layer_dependency(&self, layer: usize) -> Result<Option<g::R4GfxCopyFence>> {
        let index = *self.layer_writes.get(layer).ok_or(Error::State)?;
        match index {
            None => Ok(None),
            Some(index) => {
                let job = self.job(index).ok_or(Error::State)?;
                if !job.fence_known {
                    return Err(Error::Graphics);
                }
                Ok(Some(job.fence))
            }
        }
    }
    pub fn mark_layer_write(&mut self, layer: usize, index: usize) -> Result<()> {
        if self.job(index).is_none() {
            return Err(Error::State);
        }
        *self.layer_writes.get_mut(layer).ok_or(Error::State)? = Some(index);
        Ok(())
    }
    pub fn track(
        &mut self,
        s: &Session<'_, '_>,
        index: usize,
        handle: g::R4GfxJob,
        upload: Option<usize>,
        generation: u64,
        bytes: u64,
    ) -> Result<()> {
        let slot = self
            .slots
            .get_mut(index)
            .filter(|slot| slot.is_none())
            .ok_or(Error::State)?;
        // Store the admitted operation before querying its fence. A failed
        // query retains the job so cancellation/physical drain still sees it.
        *slot = Some(Job {
            handle,
            fence: Default::default(),
            upload,
            asset_upload: None,
            generation,
            bytes,
            complete: false,
            result: a::gfx_queue_result_pending,
            fence_known: false,
            consumer: None,
            external_taken: false,
        });
        let job = slot.as_mut().unwrap();
        accepted(unsafe { s.client.job_fence(s.raw(), &handle, &mut job.fence) })?;
        job.fence_known = true;
        self.last = Some(index);
        self.depth += 1;
        if bytes != 0 {
            self.stage = Some(index);
        }
        Ok(())
    }
    pub fn mark_external(&mut self, layer: usize, index: usize) -> Result<()> {
        if layer >= CAPACITY || self.job(index).is_none() {
            return Err(Error::State);
        }
        if self
            .external
            .iter()
            .enumerate()
            .any(|(other, value)| other != layer && *value == Some(index))
        {
            return Err(Error::State);
        }
        self.external[layer] = Some(index);
        Ok(())
    }
    /// Returns true on an actually retired failed operation. The transaction
    /// then cancels remaining work; uploaded callbacks are data-only cache ACKs
    /// and must never reenter this device or window/broker cleanup.
    pub fn collect(
        &mut self,
        s: &Session<'_, '_>,
        mut uploaded: impl FnMut(Uploaded),
    ) -> Result<bool> {
        let mut failed = false;
        for index in 0..self.slots.len() {
            let Some(job) = self.slots[index].as_mut() else {
                continue;
            };
            let mut info = g::R4GfxJobInfo::default();
            accepted(unsafe { s.client.job_info(s.raw(), &job.handle, &mut info) })?;
            if info.phase != a::gfx_queue_phase_terminal || info.flags != 0 {
                continue;
            }
            failed |= info.result != a::gfx_queue_result_complete;
            if !job.complete {
                job.complete = true;
                job.result = info.result;
                self.retirement_epoch = self.retirement_epoch.wrapping_add(1);
                if info.result == a::gfx_queue_result_complete {
                    self.uploaded_bytes = self.uploaded_bytes.saturating_add(job.bytes);
                    uploaded(Uploaded {
                        layer: job.upload,
                        asset: job.asset_upload,
                        generation: job.generation,
                        bytes: job.bytes,
                    });
                }
            }
            if self.stage == Some(index) {
                self.stage = None;
            }
            // Observation of physical retirement orders later submissions;
            // keeping an already completed dependency graph is unnecessary.
            if self.last == Some(index) {
                self.last = None;
                self.depth = 0;
            }
            if self.external.contains(&Some(index)) {
                continue;
            }
            let rc = unsafe { s.client.job_release(s.raw(), &job.handle) };
            if rc == g::status_busy {
                continue;
            }
            accepted(rc)?;
            // Clear the exact writer before its slot can name another job.
            for writer in &mut self.layer_writes {
                if *writer == Some(index) {
                    *writer = None;
                }
            }
            self.slots[index] = None;
            self.retirement_epoch = self.retirement_epoch.wrapping_add(1);
        }
        Ok(failed)
    }
    pub fn cancel(&self, s: &Session<'_, '_>) {
        for job in self.slots.iter().flatten() {
            let _ = unsafe { s.client.job_cancel(s.raw(), &job.handle) };
        }
    }
    /// Native phase only. A refused query/handoff preserves the same job.
    /// Drop Session before calling take/restore/commit around Frame.finish().
    pub fn prepare_external(&mut self, s: &Session<'_, '_>) -> Result<()> {
        if !self.physically_retired() {
            return Err(Error::Busy);
        }
        for index in self.external.into_iter().flatten() {
            let job = self.slots[index].as_mut().ok_or(Error::State)?;
            if job.external_taken {
                return Err(Error::Busy);
            }
            if job.consumer.is_some() {
                continue;
            }
            accepted(unsafe { s.client.job_fence(s.raw(), &job.handle, &mut job.fence) })?;
            job.fence_known = true;
            job.consumer =
                Some(unsafe { s.consumer(job.handle, job.fence, job.result) }.ok_or(Error::Limit)?);
        }
        Ok(())
    }
    pub fn take_external(&mut self, layer: usize) -> Result<Option<Consumer>> {
        if !self.physically_retired() {
            return Err(Error::Busy);
        }
        let index = *self.external.get(layer).ok_or(Error::State)?;
        match index {
            None => Ok(None),
            Some(index) => {
                let job = self.slots[index].as_mut().ok_or(Error::State)?;
                if job.external_taken {
                    return Err(Error::State);
                }
                let receipt = job.consumer.take().ok_or(Error::State)?;
                job.external_taken = true;
                Ok(Some(receipt))
            }
        }
    }
    pub fn restore_external(&mut self, layer: usize, receipt: Option<Consumer>) -> Result<()> {
        let index = *self.external.get(layer).ok_or(Error::State)?;
        match (index, receipt) {
            (None, None) => Ok(()),
            (Some(index), Some(receipt)) => {
                let job = self.slots[index].as_mut().ok_or(Error::State)?;
                if job.consumer.is_some() || !job.external_taken {
                    return Err(Error::State);
                }
                job.consumer = Some(receipt);
                job.external_taken = false;
                Ok(())
            }
            _ => Err(Error::State),
        }
    }
    pub fn commit_external(&mut self, layer: usize) -> Result<()> {
        let slot = self.external.get_mut(layer).ok_or(Error::State)?;
        if let Some(index) = *slot {
            let job = self.slots[index].as_ref().ok_or(Error::State)?;
            if !job.complete || job.consumer.is_some() || !job.external_taken {
                return Err(Error::State);
            }
            self.slots[index] = None;
            if self.last == Some(index) {
                self.last = None;
            }
        }
        *slot = None;
        Ok(())
    }
}
