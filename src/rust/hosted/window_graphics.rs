//! Three retained image slots and immutable retry requests per exact surface.
//! CPU maps end before Return; metadata/imports and the last real GPU receipt
//! survive lost replies until the broker explicitly releases their lease.
use super::{
    process::{Programs, gone},
    window_image::{Frame, Identity, Memory, NoReceipt, Receipt},
};
use crate::runtime::graphics_wire::{Marker, NATIVE, REMOVED};
use crate::runtime::{service::Host, wire::Wire};
use r4os_contract as a;
#[cfg(test)]
#[path = "../../../Tests/RustNative/window_graphics.rs"]
mod tests;
#[derive(Clone, Copy)]
pub struct Spec {
    pub owner: a::ProgramProcessHandle,
    pub config: a::WindowGraphicsConfig,
    pub consumer_ready: bool,
}
/// # Safety
/// Replies acknowledge the exact synchronous request. service_dead must
/// prove death of this exact generation; RPC failure/invalidate is no ACK.
pub unsafe trait Transport {
    fn marker(&self, surface: a::WindowGraphicsSurface) -> Option<Marker>;
    fn publish(&mut self, request: &a::WindowGraphicsPublication)
    -> Option<a::WindowGraphicsReply>;
    fn consumer(&mut self, request: &a::WindowGraphicsConsumer) -> Option<a::WindowGraphicsReply>;
    fn service_dead(&mut self, handle: a::ProgramProcessHandle) -> bool;
    fn invalidate(&mut self);
}
pub struct NativeTransport<'loan, 'host, 'app> {
    pub host: &'loan mut Host<'host>,
    pub programs: &'loan Programs<'app>,
}
unsafe impl Transport for NativeTransport<'_, '_, '_> {
    fn marker(&self, surface: a::WindowGraphicsSurface) -> Option<Marker> {
        self.host.image_marker(surface)
    }
    fn publish(&mut self, r: &a::WindowGraphicsPublication) -> Option<a::WindowGraphicsReply> {
        self.host.graphics_publish(r).ok()
    }
    fn consumer(&mut self, r: &a::WindowGraphicsConsumer) -> Option<a::WindowGraphicsReply> {
        self.host.graphics_consumer(r).ok()
    }
    fn service_dead(&mut self, h: a::ProgramProcessHandle) -> bool {
        match self.programs.status(h) {
            Ok(None) => true,
            Ok(Some(info)) => info.id == h.instance_id && info.state == 2,
            Err(r4os_sdk::storage::Error::Raw(code)) => gone(code),
            Err(_) => false,
        }
    }
    fn invalidate(&mut self) {
        self.host.invalidate();
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Empty,
    Taking,
    Preparing,
    Live,
    Returning,
    Acknowledged,
}
struct Slot<M: Memory, R: Receipt> {
    frame: Frame<M, R>,
    lease: a::WindowGraphicsFrame,
    state: State,
    inspected_revision: u64,
    inspected_image_revision: u64,
}
pub struct Window<M: Memory, R: Receipt = NoReceipt> {
    owner: a::ProgramProcessHandle,
    surface: a::WindowGraphicsSurface,
    config: a::WindowGraphicsConfig,
    closing: bool,
    removed: bool,
    ended: bool,
    retry_take: bool,
    serial: u64,
    publication: Option<a::WindowGraphicsPublication>,
    request: Option<a::WindowGraphicsConsumer>,
    request_slot: usize,
    request_image_revision: Option<u64>,
    empty_image_revision: Option<u64>,
    slots: [Slot<M, R>; 3],
    current: Option<usize>,
    inspect_slot: Option<usize>,
}
impl<M: Memory, R: Receipt> Window<M, R> {
    pub fn new(memory: M) -> Self
    where
        M: Clone,
    {
        Self {
            owner: a::ProgramProcessHandle::default(),
            surface: a::WindowGraphicsSurface::default(),
            config: a::WindowGraphicsConfig::default(),
            closing: false,
            removed: false,
            ended: false,
            retry_take: false,
            serial: 0,
            publication: None,
            request: None,
            request_slot: 0,
            request_image_revision: None,
            empty_image_revision: None,
            slots: core::array::from_fn(|_| Slot {
                frame: Frame::new(memory.clone()),
                lease: a::WindowGraphicsFrame::default(),
                state: State::Empty,
                inspected_revision: 0,
                inspected_image_revision: 0,
            }),
            current: None,
            inspect_slot: None,
        }
    }
    pub fn surface(&self) -> a::WindowGraphicsSurface {
        self.surface
    }
    pub fn observes_images(&self) -> bool {
        self.surface.serial != 0 && !self.closing && !self.ended
    }
    pub fn owner(&self) -> a::ProgramProcessHandle {
        self.owner
    }
    pub fn front(&mut self) -> Option<&mut Frame<M, R>> {
        let i = self.current?;
        let frame = &mut self.slots[i].frame;
        if self.closing || frame.retired() || frame.failed() {
            None
        } else {
            Some(frame)
        }
    }
    pub fn front_message(&self) -> Option<a::WindowGraphicsFrame> {
        let frame = &self.slots[self.current?].frame;
        (!self.closing && !frame.retired() && !frame.failed()).then(|| *frame.message())
    }
    /// A previous front can be retired or acknowledged while its reader is
    /// still active. End that exact read, never whichever slot is front now.
    pub fn finish_read(
        &mut self,
        identity: Identity,
        receipt: Option<R>,
        failed: bool,
    ) -> core::result::Result<(), Option<R>> {
        for slot in &mut self.slots {
            if slot.state != State::Empty && slot.frame.identity().same(&identity) {
                return slot.frame.finish(receipt, failed);
            }
        }
        Err(receipt)
    }
    pub fn retire(&mut self) {
        self.retry_take = false;
        self.inspect_slot = None;
        self.current = None;
        for slot in &mut self.slots {
            if slot.state != State::Empty {
                slot.frame.retire();
            }
        }
    }
    /// # Safety
    /// Successful restart-cleanup ended these exact metadata/fence loans, or
    /// the exact service generation is confirmed dead. Failure is no proof.
    pub unsafe fn acknowledge_reset(&mut self) {
        self.retire();
        self.closing = true;
        self.ended = true;
        self.publication = None;
        self.request = None;
        for s in &mut self.slots {
            if s.state != State::Empty {
                s.state = State::Acknowledged;
            }
        }
    }
    pub fn disconnect(&mut self) {
        self.retire();
        self.closing = true;
    }
    pub fn empty(&self) -> bool {
        self.slots.iter().all(|s| s.state == State::Empty)
    }
    pub fn detached(&self) -> bool {
        self.owner.instance_id == 0 && self.empty() && self.surface.serial == 0
    }
    fn reset_metadata(&mut self) {
        self.owner = a::ProgramProcessHandle::default();
        self.surface = a::WindowGraphicsSurface::default();
        self.config = a::WindowGraphicsConfig::default();
        self.closing = false;
        self.removed = false;
        self.ended = false;
        self.retry_take = false;
        self.serial = 0;
        self.publication = None;
        self.request = None;
        self.request_slot = 0;
        self.request_image_revision = None;
        self.empty_image_revision = None;
        self.current = None;
        self.inspect_slot = None;
    }
    fn clear_slot(&mut self, index: usize) {
        let s = &mut self.slots[index];
        s.lease = a::WindowGraphicsFrame::default();
        s.state = State::Empty;
        s.inspected_revision = 0;
        s.inspected_image_revision = 0;
    }
    fn acknowledge(&mut self, index: usize) {
        self.slots[index].frame.retire();
        self.slots[index].state = State::Acknowledged;
        if self.current == Some(index) {
            self.current = None;
        }
    }
    fn next_request(
        &mut self,
        action: u32,
        index: usize,
        desktop: a::ProgramProcessHandle,
    ) -> bool {
        let Some(serial) = self.serial.checked_add(1) else {
            self.disconnect();
            return false;
        };
        self.serial = serial;
        self.request_slot = index;
        self.request_image_revision = None;
        let mut r = a::WindowGraphicsConsumer {
            desktop,
            surface: self.surface,
            action,
            request_serial: serial,
            ..a::WindowGraphicsConsumer::default()
        };
        if action == a::window_graphics_take {
            self.slots[index].state = State::Taking;
        } else {
            let slot = &self.slots[index];
            r.chain = slot.lease.chain;
            r.image_slot = slot.lease.image_slot;
            r.acquire_token = slot.lease.acquire_token;
            r.fence = slot.frame.consumer_fence();
            r.result = if slot.frame.failed() {
                a::window_graphics_failed
            } else {
                a::window_graphics_ok
            };
        }
        self.request = Some(r);
        true
    }
    fn consume(&mut self, t: &mut impl Transport) {
        let Some(request) = self.request else {
            return;
        };
        let Some(reply) = t.consumer(&request) else {
            return;
        };
        let i = self.request_slot;
        if reply.result == a::window_graphics_ok {
            if !reply.surface.same(&self.surface) {
                self.disconnect();
                t.invalidate();
                return;
            }
            if request.action == a::window_graphics_take {
                // The raw lease is separate from Frame's validated image, so
                // malformed metadata can still be returned with its identity.
                self.slots[i].lease = reply.frame;
                self.slots[i].state = State::Live;
                let valid = reply.frame.surface.same(&self.surface)
                    && reply.chain == reply.frame.chain
                    && reply.image_slot == reply.frame.image_slot
                    && reply.acquire_token == reply.frame.acquire_token
                    && reply.frame.config_revision == self.config.revision
                    && reply.frame.flags == 0;
                if !valid {
                    self.slots[i].frame.fail();
                    self.disconnect();
                } else {
                    let frame = &mut self.slots[i].frame;
                    if frame.open(reply.frame).is_err() {
                        frame.fail();
                    }
                    if self.config.backend.binding.adapter_id == 0
                        && frame.software_consumer().is_err()
                    {
                        frame.fail();
                    }
                }
                let frame = &mut self.slots[i].frame;
                if self.closing
                    || self.removed
                    || self.config.flags & a::window_graphics_visible == 0
                {
                    frame.retire();
                }
                if !frame.failed() && !frame.retired() {
                    if frame.is_cpu() {
                        self.slots[i].state = State::Preparing;
                    } else {
                        if let Some(old) = self.current {
                            self.slots[old].frame.retire();
                        }
                        self.current = Some(i);
                    }
                }
            } else {
                if reply.chain != request.chain
                    || reply.image_slot != request.image_slot
                    || reply.acquire_token != request.acquire_token
                {
                    self.disconnect();
                    t.invalidate();
                    return;
                }
                if reply.flags & a::window_graphics_fence_released != 0 {
                    self.acknowledge(i);
                } else {
                    self.slots[i].state = State::Returning;
                }
            }
            self.request = None;
        } else if [
            a::window_graphics_not_ready,
            a::window_graphics_busy,
            a::window_graphics_capacity,
        ]
        .contains(&reply.result)
        {
            if request.action == a::window_graphics_take {
                if reply.result == a::window_graphics_not_ready {
                    // This is the marker at request creation, never a newer
                    // snapshot observed while retrying an immutable request.
                    self.empty_image_revision = self.request_image_revision;
                } else {
                    // Busy/capacity are resource waits, not an empty queue.
                    self.retry_take = true;
                }
                self.clear_slot(i);
                self.request = None;
                if let Some(current) = self.current {
                    if reply.revision != self.slots[current].inspected_revision {
                        self.inspect_slot = Some(current);
                        self.inspect(request.desktop, t);
                    }
                }
            }
        } else if reply.result == a::window_graphics_closed
            || (reply.result == a::window_graphics_stale && reply.surface.serial == 0)
        {
            if request.action == a::window_graphics_take {
                self.clear_slot(i);
                self.disconnect();
            } else {
                self.acknowledge(i);
            }
            self.request = None;
        } else if request.action == a::window_graphics_take {
            self.clear_slot(i);
            self.request = None;
            self.disconnect();
        } else if reply.result == a::window_graphics_device_lost
            && request.action == a::window_graphics_return
        {
            self.slots[i].frame.fail();
            self.request = None;
        } else if reply.result == a::window_graphics_device_lost
            && request.action == a::window_graphics_release_fence
        {
            self.acknowledge(i);
            self.request = None;
        } else {
            self.disconnect();
            t.invalidate();
        }
    }
    fn inspect(&mut self, desktop: a::ProgramProcessHandle, t: &mut impl Transport) {
        let Some(i) = self.inspect_slot else {
            return;
        };
        let slot = &self.slots[i];
        if slot.frame.retired() || ![State::Live, State::Preparing].contains(&slot.state) {
            self.inspect_slot = None;
            return;
        }
        let m = slot.lease;
        let image_revision = t.marker(self.surface).map_or(0, |m| m.revision);
        let request = a::WindowGraphicsConsumer {
            desktop,
            surface: self.surface,
            action: a::window_graphics_inspect,
            chain: m.chain,
            image_slot: m.image_slot,
            acquire_token: m.acquire_token,
            ..a::WindowGraphicsConsumer::default()
        };
        let Some(reply) = t.consumer(&request) else {
            return;
        };
        self.inspect_slot = None;
        if !reply.surface.same(&self.surface)
            || reply.chain != m.chain
            || reply.image_slot != m.image_slot
            || reply.acquire_token != m.acquire_token
        {
            self.disconnect();
            t.invalidate();
            return;
        }
        self.slots[i].inspected_revision = reply.revision;
        self.slots[i].inspected_image_revision = image_revision;
        if reply.result == a::window_graphics_ok {
            return;
        }
        if [
            a::window_graphics_closed,
            a::window_graphics_device_lost,
            a::window_graphics_failed,
        ]
        .contains(&reply.result)
        {
            self.slots[i].frame.retire();
            if reply.result != a::window_graphics_closed {
                self.slots[i].frame.fail();
            }
            if self.current == Some(i) {
                self.current = None;
            }
        } else {
            self.disconnect();
            t.invalidate();
        }
    }
    fn publish(&mut self, t: &mut impl Transport) {
        let Some(request) = self.publication else {
            return;
        };
        let Some(reply) = t.publish(&request) else {
            return;
        };
        if reply.result == a::window_graphics_ok {
            if reply.surface.serial == 0
                || reply.surface.window_id != request.window_id
                || !reply.surface.owner.same(&request.owner)
                || !reply.surface.desktop.same(&request.desktop)
                || (request.surface.serial != 0 && !reply.surface.same(&request.surface))
            {
                self.disconnect();
                t.invalidate();
                return;
            }
            self.surface = reply.surface;
            if request.action == a::window_graphics_remove {
                self.removed = true;
            } else {
                self.config = request.config;
            }
            self.publication = None;
        } else if reply.result == a::window_graphics_closed
            || (reply.result == a::window_graphics_stale && reply.surface.serial == 0)
        {
            if reply.surface.serial == 0 {
                unsafe {
                    self.acknowledge_reset();
                }
            } else {
                self.publication = None;
                self.removed = true;
                self.disconnect();
            }
        } else if reply.result == a::window_graphics_not_owner && self.surface.serial == 0 {
            self.publication = None;
            unsafe {
                self.acknowledge_reset();
            }
        } else if ![
            a::window_graphics_busy,
            a::window_graphics_capacity,
            a::window_graphics_not_ready,
        ]
        .contains(&reply.result)
        {
            self.disconnect();
            t.invalidate();
        }
    }
    pub fn poll(
        &mut self,
        desktop: a::ProgramProcessHandle,
        window_id: u32,
        wanted: Option<Spec>,
        t: &mut impl Transport,
    ) {
        self.poll_inner(desktop, window_id, wanted, t);
        let matches = wanted.is_some_and(|s| {
            let mut expected = s.config;
            let mut actual = self.config;
            expected.revision = 0;
            actual.revision = 0;
            self.owner.same(&s.owner) && expected.same(&actual)
        });
        if !matches {
            self.retire();
        }
    }
    fn poll_inner(
        &mut self,
        desktop: a::ProgramProcessHandle,
        window_id: u32,
        wanted: Option<Spec>,
        t: &mut impl Transport,
    ) {
        if self.surface.serial != 0 && t.service_dead(self.surface.service) {
            unsafe {
                self.acknowledge_reset();
            }
        }
        if self.current.is_some_and(|i| self.slots[i].frame.failed()) {
            self.retire();
        }
        if self.owner.instance_id != 0 && wanted.is_none_or(|s| !self.owner.same(&s.owner)) {
            self.disconnect();
        }
        if let Some(s) = wanted {
            let mut old = self.config;
            let mut next = s.config;
            old.revision = 0;
            next.revision = 0;
            if !old.same(&next) {
                self.retire();
            }
        }
        for i in 0..self.slots.len() {
            if self.slots[i].state == State::Acknowledged
                && unsafe { self.slots[i].frame.close_acknowledged() }
            {
                self.clear_slot(i);
            }
        }
        if self.ended && self.empty() {
            self.reset_metadata();
        }
        if self.ended {
            return;
        }
        if self.request.is_some() {
            self.consume(t);
            return;
        }
        if self.publication.is_some() {
            self.publish(t);
            return;
        }
        if self.inspect_slot.is_some() {
            self.inspect(desktop, t);
            return;
        }
        for i in 0..self.slots.len() {
            if self.slots[i].state == State::Preparing {
                if !self.slots[i].frame.retired() {
                    match self.slots[i].frame.prepare_cpu() {
                        Err(_) => {
                            self.slots[i].frame.fail();
                            self.slots[i].state = State::Live;
                            return;
                        }
                        Ok(false) => {
                            self.inspect_slot = Some(i);
                            self.inspect(desktop, t);
                            return;
                        }
                        Ok(true) => {
                            if let Some(old) = self.current {
                                self.slots[old].frame.retire();
                            }
                            self.current = Some(i);
                        }
                    }
                }
                self.slots[i].state = State::Live;
            }
        }
        for i in 0..self.slots.len() {
            let s = &self.slots[i];
            if (s.state == State::Live && s.frame.retired() && s.frame.readers() == 0)
                || s.state == State::Returning
            {
                if !self.slots[i].frame.release_cpu_mapping() {
                    return;
                }
                let action = if self.slots[i].state == State::Returning {
                    a::window_graphics_release_fence
                } else {
                    a::window_graphics_return
                };
                if self.next_request(action, i, desktop) {
                    self.consume(t);
                }
                return;
            }
        }
        if self.closing {
            if self.surface.serial != 0 && !self.removed {
                self.publication = Some(a::WindowGraphicsPublication {
                    desktop,
                    owner: self.owner,
                    window_id,
                    action: a::window_graphics_remove,
                    surface: self.surface,
                    ..a::WindowGraphicsPublication::default()
                });
                self.publish(t);
            } else if self.empty() {
                self.reset_metadata();
            }
            return;
        }
        let Some(spec) = wanted else {
            return;
        };
        if spec.owner.instance_id == 0 || spec.owner.generation == 0 {
            return;
        }
        let mut old = self.config;
        let mut next = spec.config;
        old.revision = 0;
        next.revision = 0;
        if self.surface.serial == 0 || !old.same(&next) {
            self.owner = spec.owner;
            let mut old_geometry = old;
            let mut new_geometry = next;
            old_geometry.flags = 0;
            new_geometry.flags = 0;
            next.revision = if self.surface.serial != 0 && old_geometry.same(&new_geometry) {
                self.config.revision
            } else {
                let Some(v) = self.config.revision.checked_add(1) else {
                    self.disconnect();
                    return;
                };
                v
            };
            self.publication = Some(a::WindowGraphicsPublication {
                desktop,
                owner: spec.owner,
                window_id,
                action: a::window_graphics_publish,
                surface: self.surface,
                config: next,
                ..a::WindowGraphicsPublication::default()
            });
            self.publish(t);
            return;
        }
        if self.config.flags & a::window_graphics_visible == 0 {
            return;
        }
        let marker = t.marker(self.surface);
        if marker.is_some_and(|m| m.flags & REMOVED != 0) {
            // Metadata removal is no release ACK. Existing leases still
            // follow Return, release_fence and actual local close receipts.
            self.disconnect();
            return;
        }
        if let Some(i) = self.current {
            if !self.slots[i].frame.has_receipt() && !self.slots[i].frame.cpu_consumed() {
                self.inspect_slot = Some(i);
                self.inspect(desktop, t);
                return;
            }
            if marker.is_some_and(|m| {
                m.revision == u64::MAX || m.revision != self.slots[i].inspected_image_revision
            }) {
                self.retry_take |= marker.is_some_and(|m| m.flags & NATIVE != 0);
                self.inspect_slot = Some(i);
                self.inspect(desktop, t);
                if self.inspect_slot.is_some() || self.current.is_none() || self.closing {
                    return;
                }
            }
        }
        let Some(marker) = marker else {
            self.retry_take = false;
            return;
        };
        if marker.flags & NATIVE == 0
            || (marker.revision != u64::MAX && self.empty_image_revision == Some(marker.revision))
        {
            self.retry_take = false;
            return;
        }
        self.retry_take = !spec.consumer_ready;
        if self.retry_take {
            if let Some(i) = self.current {
                self.inspect_slot = Some(i);
                self.inspect(desktop, t);
            }
            return;
        }
        if let Some(i) = self.slots.iter().position(|s| s.state == State::Empty) {
            if self.next_request(a::window_graphics_take, i, desktop) {
                self.request_image_revision = Some(marker.revision);
                self.consume(t);
            }
        }
    }
    pub fn needs_polling(&self) -> bool {
        self.publication.is_some()
            || self.request.is_some()
            || self.inspect_slot.is_some()
            || self.closing
            || self.ended
            || self.retry_take
            || self.slots.iter().any(|s| {
                [State::Preparing, State::Returning, State::Acknowledged].contains(&s.state)
                    || (s.state == State::Live && s.frame.retired())
            })
    }
}
