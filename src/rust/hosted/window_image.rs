//! One imported window BO. Producer completion, CPU mapping, readers and the
//! final consumer job retain independent identities until their exact ACKs.
use crate::runtime::wire::Wire;
use r4gfx as g;
use r4os_contract as a;
use r4os_sdk::{c_facades as c, graphics::Graphics};

/// A read may outlive replacement of the front slot. Match the complete
/// broker frame and this consumer's import, including every generation.
#[derive(Clone, Copy)]
pub struct Identity {
    message: a::WindowGraphicsFrame,
    reference: a::GfxBufferReference,
}
impl Identity {
    pub fn same(&self, other: &Self) -> bool {
        self.message.same(&other.message) && self.reference.same(&other.reference)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Busy,
    Invalid,
    Graphics,
    Stale,
    Overflow,
}
pub type Result<T> = core::result::Result<T, Error>;
/// # Safety
/// Successful read maps name initialized immutable storage until exact Unmap.
/// Returned partial import/map handles remain valid for the same close path,
/// even on error. No method silently releases a returned ownership receipt.
pub unsafe trait Memory {
    fn import(&self, source: &a::GfxBufferHandle, out: &mut a::GfxBufferReference) -> i32;
    fn describe(&self, reference: &a::GfxBufferHandle, out: &mut a::GfxBufferDescriptor) -> i32;
    fn release(&self, reference: &a::GfxBufferHandle) -> i32;
    fn map(&self, reference: &a::GfxBufferHandle, bytes: u64, out: &mut a::GfxBufferMap) -> i32;
    fn unmap(&self, lease: &a::GfxBufferHandle) -> i32;
    fn query(&self, fence: &a::GfxFence, out: &mut a::GfxFenceStatus) -> i32;
}
/// Uses the SDK's ordinary graphics context. Raw out-parameters deliberately
/// retain partial handles which a Result-only query cannot return on error.
#[derive(Clone, Copy)]
pub struct NativeMemory<'g, 's, 'a>(pub &'g Graphics<'s, 'a>);
fn code(slot: c::R4SdkGuiSlot) -> i32 {
    if slot.status < 0 {
        slot.status
    } else if slot.flags & 2 != 0 {
        a::gfx_buffer_error_invalid
    } else {
        slot.bits as i32
    }
}
unsafe impl Memory for NativeMemory<'_, '_, '_> {
    fn import(&self, source: &a::GfxBufferHandle, out: &mut a::GfxBufferReference) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_gfxBufferImport(
                0,
                self.0.native_draw(),
                source,
                out,
            ))
        }
    }
    fn describe(&self, reference: &a::GfxBufferHandle, out: &mut a::GfxBufferDescriptor) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_gfxBufferDescribe(
                0,
                self.0.native_draw(),
                reference,
                out,
            ))
        }
    }
    fn release(&self, reference: &a::GfxBufferHandle) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_gfxBufferRelease(
                0,
                self.0.native_draw(),
                reference,
            ))
        }
    }
    fn map(&self, reference: &a::GfxBufferHandle, bytes: u64, out: &mut a::GfxBufferMap) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_gfxBufferMap(
                0,
                self.0.native_draw(),
                reference,
                a::gfx_buffer_map_read,
                0,
                bytes,
                out,
            ))
        }
    }
    fn unmap(&self, lease: &a::GfxBufferHandle) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_gfxBufferUnmap(
                0,
                self.0.native_draw(),
                lease,
            ))
        }
    }
    fn query(&self, fence: &a::GfxFence, out: &mut a::GfxFenceStatus) -> i32 {
        unsafe {
            code(c::R4SdkGuiDrawContext_gfxFenceQuery(
                0,
                self.0.native_draw(),
                fence,
                out,
            ))
        }
    }
}
/// # Safety
/// A receipt retains its originating job/device/worker until release() ACKs.
/// Refusal and Drop must not free backing or decrement the worker's hold.
pub unsafe trait Receipt {
    fn release(&mut self) -> bool;
    fn fence(&self) -> a::GfxFence;
    fn result(&self) -> u32;
}
#[derive(Debug)]
pub enum NoReceipt {}
unsafe impl Receipt for NoReceipt {
    fn release(&mut self) -> bool {
        match *self {}
    }
    fn fence(&self) -> a::GfxFence {
        match *self {}
    }
    fn result(&self) -> u32 {
        match *self {}
    }
}
pub struct Frame<M: Memory, R: Receipt = NoReceipt> {
    memory: M,
    message: a::WindowGraphicsFrame,
    reference: a::GfxBufferReference,
    mapping: a::GfxBufferMap,
    readers: usize,
    retired: bool,
    failed: bool,
    receipt: Option<R>,
    cpu_consumed: bool,
    software_consumer: bool,
    imported: bool,
    mapping_valid: bool,
}
impl<M: Memory, R: Receipt> Frame<M, R> {
    pub fn new(memory: M) -> Self {
        Self {
            memory,
            message: a::WindowGraphicsFrame::default(),
            reference: a::GfxBufferReference::default(),
            mapping: a::GfxBufferMap::default(),
            readers: 0,
            retired: false,
            failed: false,
            receipt: None,
            cpu_consumed: false,
            software_consumer: false,
            imported: false,
            mapping_valid: false,
        }
    }
    pub fn message(&self) -> &a::WindowGraphicsFrame {
        &self.message
    }
    pub fn reference(&self) -> &a::GfxBufferReference {
        &self.reference
    }
    pub fn identity(&self) -> Identity {
        Identity {
            message: self.message,
            reference: self.reference,
        }
    }
    pub fn mapping(&self) -> &a::GfxBufferMap {
        &self.mapping
    }
    pub fn readers(&self) -> usize {
        self.readers
    }
    pub fn failed(&self) -> bool {
        self.failed
    }
    pub fn retired(&self) -> bool {
        self.retired
    }
    pub fn fail(&mut self) {
        self.failed = true;
        self.retired = true;
    }
    pub fn has_receipt(&self) -> bool {
        self.receipt.is_some()
    }
    pub fn retire(&mut self) {
        self.retired = true;
    }
    pub fn cpu_consumed(&self) -> bool {
        self.cpu_consumed
    }
    pub fn mark_cpu_consumed(&mut self) {
        self.cpu_consumed = true;
    }
    pub fn software_consumer(&mut self) -> Result<()> {
        if self.readers != 0 || self.mapping.lease.id != 0 || self.receipt.is_some() {
            return Err(Error::Busy);
        }
        self.software_consumer = true;
        Ok(())
    }
    pub fn open(&mut self, message: a::WindowGraphicsFrame) -> Result<()> {
        if self.reference.reference.id != 0
            || self.readers != 0
            || self.receipt.is_some()
            || self.mapping.lease.id != 0
        {
            return Err(Error::Busy);
        }
        if message.version != 1
            || message.size != core::mem::size_of::<a::WindowGraphicsFrame>() as u32
            || message.surface.serial == 0
            || message.chain == 0
            || message.acquire_token == 0
            || message.descriptor.width == 0
            || message.descriptor.height == 0
            || message.descriptor.width > 32768
            || message.descriptor.height > 32768
            || message.format.format != message.descriptor.format
            || message.format.reserved != 0
            || message.ready.timeline == 0
            || message.ready.point == 0
        {
            return Err(Error::Invalid);
        }
        self.message = message;
        self.retired = false;
        self.failed = false;
        self.cpu_consumed = false;
        self.software_consumer = false;
        self.imported = false;
        self.mapping_valid = false;
        if self
            .memory
            .import(&message.source.reference, &mut self.reference)
            != a::gfx_buffer_result_ok
        {
            return Err(Error::Graphics);
        }
        let mut actual = a::GfxBufferDescriptor::default();
        if self.reference.flags != 0
            || !same_handle(self.reference.buffer, message.source.buffer)
            || same_handle(self.reference.reference, message.source.reference)
            || self.memory.describe(&self.reference.reference, &mut actual)
                != a::gfx_buffer_result_ok
            || !same_descriptor(actual, message.descriptor)
        {
            return Err(Error::Stale);
        }
        self.imported = true;
        Ok(())
    }
    pub fn is_cpu(&self) -> bool {
        self.software_consumer || self.message.ready.adapter_id == 0
    }
    /// Never wait for a producer or CPU-map admission. A pending replacement
    /// leaves the preceding front available to the transport owner.
    pub fn prepare_cpu(&mut self) -> Result<bool> {
        if !self.imported {
            return Err(Error::Stale);
        }
        if !self.is_cpu() {
            return Ok(true);
        }
        if self.mapping.lease.id != 0 {
            return if self.mapping_valid {
                Ok(true)
            } else {
                Err(Error::Invalid)
            };
        }
        let desc = self.message.descriptor;
        let pixel_bytes = match desc.format {
            g::format_xrgb8888
            | g::format_argb8888
            | g::format_xrgb2101010
            | g::format_argb2101010 => 4,
            g::format_abgr16161616f => 8,
            _ => return Err(Error::Invalid),
        };
        let needed = desc.plane_pitches[0]
            .checked_mul(u64::from(desc.height))
            .ok_or(Error::Overflow)?;
        if desc.location != a::gfx_buffer_location_system
            || desc.modifier != 0
            || desc.plane_count != 1
            || desc.usage & a::gfx_buffer_usage_cpu_read == 0
            || desc.plane_offsets[0] != 0
            || desc.plane_pitches[0] < u64::from(desc.width) * pixel_bytes
            || desc.byte_length < needed
        {
            return Err(Error::Invalid);
        }
        let mut status = a::GfxFenceStatus::default();
        if self.memory.query(&self.message.ready, &mut status) != a::gfx_queue_ok
            || status.version != 1
            || status.size != core::mem::size_of::<a::GfxFenceStatus>() as u32
            || !same_fence(status.fence, self.message.ready)
            || status.milestone
                != if self.message.ready.adapter_id == 0 {
                    a::gfx_queue_milestone_cpu_stores
                } else {
                    a::gfx_queue_milestone_device_execution
                }
        {
            return Err(Error::Graphics);
        }
        if status.result == a::gfx_queue_result_pending {
            return Ok(false);
        }
        if status.result != a::gfx_queue_result_complete {
            return Err(Error::Graphics);
        }
        let rc = self.memory.map(
            &self.reference.reference,
            desc.byte_length,
            &mut self.mapping,
        );
        if rc == a::gfx_buffer_error_busy {
            return Ok(false);
        }
        if rc != a::gfx_buffer_result_ok {
            return Err(Error::Graphics);
        }
        if self.mapping.version != 1
            || self.mapping.size != core::mem::size_of::<a::GfxBufferMap>() as u32
            || self.mapping.reserved0 != 0
            || self.mapping.lease.id == 0
            || self.mapping.lease.generation == 0
            || self.mapping.lease.reserved0 != 0
            || self.mapping.cpu_address == 0
            || self.mapping.byte_length < desc.byte_length
            || self
                .mapping
                .cpu_address
                .checked_add(desc.byte_length)
                .is_none()
        {
            return Err(Error::Invalid);
        }
        self.mapping_valid = true;
        Ok(true)
    }
    /// Native image metadata only: no safe slice escapes the read lease.
    pub fn cpu_image(&self) -> Option<g::R4GfxColorImage> {
        if !self.imported || !self.mapping_valid || !self.is_cpu() || self.mapping.lease.id == 0 {
            return None;
        }
        let desc = self.message.descriptor;
        Some(g::R4GfxColorImage {
            version: 1,
            size: core::mem::size_of::<g::R4GfxColorImage>() as u32,
            image: g::R4GfxCpuImage {
                cpu_address: self.mapping.cpu_address,
                byte_length: self.mapping.byte_length,
                pitch: desc.plane_pitches[0],
                width: desc.width,
                height: desc.height,
                format: desc.format,
                reserved: 0,
            },
            description: self.description(),
            profile: g::R4GfxColorProfile::default(),
        })
    }
    pub fn release_cpu_mapping(&mut self) -> bool {
        if self.readers != 0 {
            return false;
        }
        if self.mapping.lease.id != 0 {
            if self.memory.unmap(&self.mapping.lease) != a::gfx_buffer_result_ok {
                return false;
            }
            self.mapping = a::GfxBufferMap::default();
            self.mapping_valid = false;
        }
        true
    }
    pub fn borrow(&mut self) -> Result<()> {
        if !self.imported || self.retired || self.failed || self.reference.reference.id == 0 {
            return Err(Error::Stale);
        }
        self.readers = self.readers.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    /// Refusal returns the incoming receipt intact to its original owner.
    /// No reader is retired and no incoming job is lost on a failed swap.
    pub fn finish(&mut self, next: Option<R>, failed: bool) -> core::result::Result<(), Option<R>> {
        if self.readers == 0 {
            return Err(next);
        }
        if next.is_some() {
            if let Some(old) = self.receipt.as_mut() {
                if !old.release() {
                    return Err(next);
                }
            }
            self.receipt = next;
            if self.receipt.as_ref().unwrap().result() != a::gfx_queue_result_complete {
                self.failed = true;
            }
        }
        self.failed |= failed;
        self.readers -= 1;
        Ok(())
    }
    pub fn consumer_fence(&self) -> a::GfxFence {
        self.receipt
            .as_ref()
            .map_or_else(a::GfxFence::default, Receipt::fence)
    }
    /// # Safety
    /// WINSVC has ended this precise consumer lease and borrowed fence,
    /// or its exact service generation is confirmed dead by the transport.
    pub unsafe fn close_acknowledged(&mut self) -> bool {
        if !self.release_cpu_mapping() {
            return false;
        }
        if let Some(receipt) = self.receipt.as_mut() {
            if !receipt.release() {
                return false;
            }
        }
        self.receipt = None;
        if self.reference.reference.id != 0
            && self.memory.release(&self.reference.reference) != a::gfx_buffer_result_ok
        {
            return false;
        }
        self.message = a::WindowGraphicsFrame::default();
        self.reference = a::GfxBufferReference::default();
        self.retired = false;
        self.failed = false;
        self.cpu_consumed = false;
        self.software_consumer = false;
        self.imported = false;
        self.mapping_valid = false;
        true
    }
    pub fn description(&self) -> g::R4GfxColorDescription {
        // Both compiler-projected records are exactly twelve native u32s.
        // Every bit pattern is valid; no pointer, reference or padding read.
        unsafe { core::mem::transmute(self.message.format.color) }
    }
}

#[cfg(test)]
#[path = "../../../Tests/RustNative/window_image.rs"]
pub(crate) mod tests;
pub fn same_handle(a: a::GfxBufferHandle, b: a::GfxBufferHandle) -> bool {
    a.id == b.id && a.reserved0 == b.reserved0 && a.generation == b.generation
}
pub fn same_fence(a: a::GfxFence, b: a::GfxFence) -> bool {
    a.slot == b.slot
        && a.adapter_id == b.adapter_id
        && a.timeline == b.timeline
        && a.point == b.point
        && a.device_generation == b.device_generation
        && a.reset_generation == b.reset_generation
}
fn same_descriptor(a: a::GfxBufferDescriptor, b: a::GfxBufferDescriptor) -> bool {
    a.version == b.version
        && a.size == b.size
        && a.byte_length == b.byte_length
        && a.alignment == b.alignment
        && a.modifier == b.modifier
        && a.width == b.width
        && a.height == b.height
        && a.format == b.format
        && a.plane_count == b.plane_count
        && a.usage == b.usage
        && a.location == b.location
        && a.adapter_id == b.adapter_id
        && a.driver_owner == b.driver_owner
        && a.device_generation == b.device_generation
        && a.plane_offsets == b.plane_offsets
        && a.plane_pitches == b.plane_pitches
        && a.reserved0 == b.reserved0
}
