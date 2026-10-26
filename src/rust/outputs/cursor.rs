//! Optional hardware plane, driven by exact receipts and a clean-frame
//! barrier. Timeout/lost status never proves that the old plane disappeared.
use crate::render::cursor as asset;
use r4os_contract as a;
#[derive(Clone, Copy)]
pub struct CleanFrame {
    pub target: a::GfxOutputTarget,
    pub device_address: u64,
    pub device_generation: u64,
    pub frame: u64,
}
impl PartialEq for CleanFrame {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.target, other.target);
        self.device_address == other.device_address
            && self.device_generation == other.device_generation
            && self.frame == other.frame
            && a.version == b.version
            && a.size == b.size
            && a.adapter_id == b.adapter_id
            && a.connector_id == b.connector_id
            && a.device_generation == b.device_generation
            && a.connection_generation == b.connection_generation
            && a.display_generation == b.display_generation
            && a.head_id == b.head_id
            && a.reserved0 == b.reserved0
    }
}
impl Eq for CleanFrame {}
/// # Safety
/// Native map success grants the requested exclusive writable range until
/// the exact unmap ACK. Output records may contain retained handles even on
/// failure. Calls borrow their out records synchronously and cannot reenter
/// this Controller or retain their Rust references. Implement with the
/// general SDK's existing buffer/cursor boundary, preserving raw results.
pub unsafe trait CursorApi {
    fn supported(&mut self) -> bool;
    fn status(&mut self, out: &mut a::DisplayCursorStatus) -> i32;
    fn info(&mut self, out: &mut a::DisplayCursorInfo) -> i32;
    fn submit(
        &mut self,
        request: &a::DisplayCursorRequest,
        out: &mut a::DisplayCursorStatus,
    ) -> i32;
    fn create(
        &mut self,
        descriptor: &a::GfxBufferDescriptor,
        out: &mut a::GfxBufferReference,
    ) -> i32;
    fn map(
        &mut self,
        reference: &a::GfxBufferHandle,
        access: u32,
        offset: u64,
        bytes: u64,
        out: &mut a::GfxBufferMap,
    ) -> i32;
    fn unmap(&mut self, lease: &a::GfxBufferHandle) -> i32;
    fn release(&mut self, reference: &a::GfxBufferHandle) -> i32;
}
pub struct Controller {
    software: bool,
    disabled: bool,
    info: a::DisplayCursorInfo,
    acquired: bool,
    image_sequence: u64,
    pending_sequence: u64,
    pending_operation: u32,
    clean_pending: bool,
    clean_ready: bool,
    clean_frame: Option<CleanFrame>,
    retry_at: u64,
    reference: a::GfxBufferReference,
    mapping: a::GfxBufferMap,
    source_ready: bool,
}
impl Default for Controller {
    fn default() -> Self {
        Self {
            software: true,
            disabled: false,
            info: a::DisplayCursorInfo::default(),
            acquired: false,
            image_sequence: 0,
            pending_sequence: 0,
            pending_operation: 0,
            clean_pending: false,
            clean_ready: false,
            clean_frame: None,
            retry_at: 0,
            reference: a::GfxBufferReference::default(),
            mapping: a::GfxBufferMap::default(),
            source_ready: false,
        }
    }
}
impl Controller {
    pub fn software(&self) -> bool {
        self.software
    }
    pub fn acquired(&self) -> bool {
        self.acquired
    }
    pub fn clean_pending(&self) -> bool {
        self.clean_pending
    }
    pub fn clean_ready(&self) -> bool {
        self.clean_ready
    }
    pub fn poll(
        &mut self,
        api: &mut impl CursorApi,
        now: u64,
        x: i32,
        y: i32,
        wanted: bool,
    ) -> bool {
        let before = self.software;
        self.advance(api, now, x, y, wanted);
        before != self.software
    }
    pub fn presented(&mut self, success: bool) {
        if self.clean_pending && success {
            self.clean_ready = true;
        }
    }
    pub fn captured(&mut self, key: CleanFrame) {
        if !self.clean_pending
            || self.clean_ready
            || self.software
            || self.clean_frame.is_some()
            || key.device_address == 0
            || key.device_generation == 0
            || key.frame == 0
            || key.target.display_generation != self.info.display_generation
            || key.target.head_id != self.info.head_id
            || key.target.adapter_id != self.info.backend.adapter_id
            || key.target.device_generation != self.info.backend.device_generation
        {
            return;
        }
        self.clean_frame = Some(key);
    }
    pub fn completed(&mut self, key: CleanFrame, visible: bool) -> bool {
        if self.clean_frame != Some(key) {
            return false;
        }
        self.clean_frame = None;
        self.presented(visible);
        true
    }
    pub fn retry_pending(&self) -> bool {
        self.pending_sequence != 0 || self.clean_pending || self.mapping.lease.id != 0
    }
    pub fn close(&mut self, api: &mut impl CursorApi, now: u64) -> bool {
        self.disabled = true;
        self.advance(api, now, 0, 0, false);
        if !self.acquired && self.pending_sequence == 0 {
            self.discard_source(api);
        }
        !self.acquired
            && self.pending_sequence == 0
            && self.mapping.lease.id == 0
            && self.reference.reference.id == 0
    }
    fn advance(&mut self, api: &mut impl CursorApi, now: u64, x: i32, y: i32, wanted: bool) {
        if !api.supported() {
            self.disabled = true;
            return;
        }
        let mut status = a::DisplayCursorStatus::default();
        let rc = api.status(&mut status);
        if rc == a::gfx_output_error_busy {
            return;
        }
        let known = rc == a::gfx_output_ok;
        if known
            && (status.phase == a::display_cursor_phase_lost
                || status.flags & a::display_cursor_state_unknown != 0)
        {
            self.software = false;
            return;
        }
        let shown = known && status.flags & a::display_cursor_state_visible != 0;
        if self.pending_sequence != 0 {
            if !known
                || status.display_generation != self.info.display_generation
                || status.completed < self.pending_sequence
            {
                return;
            }
            self.pending_sequence = 0;
            self.image_sequence = status.image_sequence;
            if status.error_code != 0 {
                self.disabled = true;
            }
            if self.pending_operation == a::display_cursor_operation_release
                && status.error_code == 0
            {
                self.acquired = false;
            }
            self.discard_source(api);
            self.software = !shown;
        }
        if self.acquired && (!known || status.display_generation != self.info.display_generation) {
            self.software = false;
            return;
        }
        if self.acquired && status.flags & a::display_cursor_state_claimed == 0 {
            self.acquired = false;
            self.image_sequence = 0;
        }
        let allowed = wanted
            && (!known || status.flags & a::display_cursor_state_suspended == 0)
            && (self.info.display_generation == 0
                || (x >= self.info.min_x
                    && x <= self.info.max_x
                    && y >= self.info.min_y
                    && y <= self.info.max_y));
        if self.disabled || !allowed {
            self.clean_pending = false;
            self.clean_ready = false;
            self.clean_frame = None;
            self.software = !shown;
            if self.acquired && (shown || self.disabled) {
                self.send(
                    api,
                    if self.disabled {
                        a::display_cursor_operation_release
                    } else {
                        a::display_cursor_operation_hide
                    },
                    0,
                    0,
                );
            } else if !self.acquired {
                self.discard_source(api);
            }
            return;
        }
        if !self.acquired {
            self.software = !shown;
            if shown || now < self.retry_at {
                return;
            }
            self.retry_at = now.saturating_add(1_000_000_000);
            let mut info = a::DisplayCursorInfo::default();
            if api.info(&mut info) != a::gfx_output_ok
                || info.version != 1
                || (info.size as usize) < core::mem::size_of::<a::DisplayCursorInfo>()
                || info.flags & 15 != 15
                || info.max_width < asset::WIDTH as u32
                || info.max_height < asset::HEIGHT as u32
            {
                return;
            }
            self.info = info;
            if !self.prepare_source(api) {
                self.retry_at = now;
                return;
            }
            if self.send(api, a::display_cursor_operation_prepare, 0, 0) {
                self.acquired = true;
            } else {
                self.retry_at = now;
            }
            return;
        }
        self.discard_source(api);
        if shown {
            self.software = false;
            self.clean_pending = false;
            self.clean_ready = false;
            self.clean_frame = None;
            if status.x != x || status.y != y {
                self.send(api, a::display_cursor_operation_move, x, y);
            }
            return;
        }
        if !self.clean_pending {
            self.software = false;
            self.clean_pending = true;
            self.clean_ready = false;
            return;
        }
        if self.clean_ready && self.send(api, a::display_cursor_operation_show, x, y) {
            self.clean_pending = false;
            self.clean_ready = false;
        }
    }
    fn send(&mut self, api: &mut impl CursorApi, operation: u32, x: i32, y: i32) -> bool {
        let mut request = a::DisplayCursorRequest {
            operation,
            display_generation: self.info.display_generation,
            head_id: self.info.head_id,
            ..a::DisplayCursorRequest::default()
        };
        if operation == a::display_cursor_operation_prepare {
            request.reference = self.reference.reference;
            request.width = asset::WIDTH as u32;
            request.height = asset::HEIGHT as u32;
            request.pitch = asset::WIDTH as u64 * 4;
            request.byte_length = (asset::WIDTH * asset::HEIGHT * 4) as u64;
        } else if operation == a::display_cursor_operation_show
            || operation == a::display_cursor_operation_move
        {
            request.image_sequence = self.image_sequence;
            request.x = x;
            request.y = y;
        }
        let mut status = a::DisplayCursorStatus::default();
        let rc = api.submit(&request, &mut status);
        if rc != a::gfx_output_ok {
            if rc != a::gfx_output_error_busy {
                self.disabled = true;
            }
            return false;
        }
        self.pending_sequence = status.sequence;
        self.pending_operation = operation;
        true
    }
    fn prepare_source(&mut self, api: &mut impl CursorApi) -> bool {
        const BYTES: usize = asset::WIDTH * asset::HEIGHT * 4;
        if self.source_ready {
            return true;
        }
        if self.reference.reference.id == 0 {
            let descriptor = a::GfxBufferDescriptor {
                byte_length: BYTES as u64,
                width: asset::WIDTH as u32,
                height: asset::HEIGHT as u32,
                format: a::gfx_buffer_format_argb8888,
                plane_count: 1,
                plane_pitches: [asset::WIDTH as u64 * 4, 0, 0, 0],
                location: a::gfx_buffer_location_system,
                usage: a::gfx_buffer_usage_cpu_read | a::gfx_buffer_usage_cpu_write,
                ..a::GfxBufferDescriptor::default()
            };
            let rc = api.create(&descriptor, &mut self.reference);
            if rc != a::gfx_buffer_result_ok {
                if rc != a::gfx_buffer_error_busy {
                    self.disabled = true;
                }
                return false;
            }
        }
        if self.mapping.lease.id == 0 {
            let rc = api.map(
                &self.reference.reference,
                a::gfx_buffer_map_write,
                0,
                BYTES as u64,
                &mut self.mapping,
            );
            if rc != a::gfx_buffer_result_ok {
                if rc != a::gfx_buffer_error_busy {
                    self.disabled = true;
                }
                return false;
            }
            if self.mapping.cpu_address == 0
                || self.mapping.cpu_address > usize::MAX as u64 - BYTES as u64
                || self.mapping.cpu_address & 3 != 0
                || self.mapping.byte_length != BYTES as u64
                || self.mapping.cache_policy != a::gfx_buffer_cache_write_back
            {
                self.disabled = true;
                return false;
            }
            // The exact native write lease is live. No Rust reference to its
            // provider is active while these bounded pixel stores execute.
            let pixels = self.mapping.cpu_address as *mut u32;
            for row in 0..asset::HEIGHT {
                for column in 0..asset::WIDTH {
                    unsafe {
                        pixels
                            .add(row * asset::WIDTH + column)
                            .write(asset::pixel(column, row));
                    }
                }
            }
        }
        if api.unmap(&self.mapping.lease) != a::gfx_buffer_result_ok {
            return false;
        }
        self.mapping = a::GfxBufferMap::default();
        self.source_ready = true;
        true
    }
    fn discard_source(&mut self, api: &mut impl CursorApi) {
        if self.mapping.lease.id != 0 {
            if api.unmap(&self.mapping.lease) != a::gfx_buffer_result_ok {
                return;
            }
            self.mapping = a::GfxBufferMap::default();
        }
        if self.reference.reference.id != 0 {
            if api.release(&self.reference.reference) != a::gfx_buffer_result_ok {
                return;
            }
            self.reference = a::GfxBufferReference::default();
            self.source_ready = false;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Model {
        supported: bool,
        status: a::DisplayCursorStatus,
        request: a::DisplayCursorRequest,
        calls: usize,
        creates: usize,
        mapped: bool,
        released: bool,
        busy: bool,
        pixels: [u32; asset::WIDTH * asset::HEIGHT],
    }
    impl Default for Model {
        fn default() -> Self {
            Self {
                supported: true,
                status: a::DisplayCursorStatus {
                    display_generation: 7,
                    ..a::DisplayCursorStatus::default()
                },
                request: a::DisplayCursorRequest::default(),
                calls: 0,
                creates: 0,
                mapped: false,
                released: false,
                busy: false,
                pixels: [0; asset::WIDTH * asset::HEIGHT],
            }
        }
    }
    // The model's array is stable between map and immediate unmap; it grants
    // the same exact write loan as the original native cursor fixture.
    unsafe impl CursorApi for Model {
        fn supported(&mut self) -> bool {
            self.supported
        }
        fn info(&mut self, out: &mut a::DisplayCursorInfo) -> i32 {
            *out = a::DisplayCursorInfo {
                display_generation: 7,
                flags: 15,
                backend: a::GfxBackendBinding {
                    adapter_id: 1,
                    device_generation: 3,
                    reset_generation: 1,
                    ..a::GfxBackendBinding::default()
                },
                max_width: 256,
                max_height: 256,
                min_x: -32768,
                min_y: -32768,
                max_x: 32767,
                max_y: 32767,
                ..a::DisplayCursorInfo::default()
            };
            1
        }
        fn status(&mut self, out: &mut a::DisplayCursorStatus) -> i32 {
            *out = self.status;
            1
        }
        fn submit(
            &mut self,
            request: &a::DisplayCursorRequest,
            out: &mut a::DisplayCursorStatus,
        ) -> i32 {
            if self.busy {
                return a::gfx_output_error_busy;
            }
            assert!(
                !self.mapped
                    && request.display_generation == 7
                    && self.status.sequence == self.status.completed
            );
            self.request = *request;
            self.calls += 1;
            self.status.sequence += 1;
            self.status.phase = a::display_cursor_phase_queued;
            self.status.flags |= a::display_cursor_state_claimed;
            *out = self.status;
            1
        }
        fn create(&mut self, d: &a::GfxBufferDescriptor, out: &mut a::GfxBufferReference) -> i32 {
            assert!(
                self.creates == 0
                    && d.byte_length == core::mem::size_of_val(&self.pixels) as u64
                    && d.format == a::gfx_buffer_format_argb8888
            );
            self.creates += 1;
            *out = a::GfxBufferReference {
                reference: a::GfxBufferHandle {
                    id: 4,
                    generation: 9,
                    ..a::GfxBufferHandle::default()
                },
                ..a::GfxBufferReference::default()
            };
            1
        }
        fn map(
            &mut self,
            reference: &a::GfxBufferHandle,
            access: u32,
            offset: u64,
            bytes: u64,
            out: &mut a::GfxBufferMap,
        ) -> i32 {
            assert!(
                reference.id == 4
                    && access == a::gfx_buffer_map_write
                    && offset == 0
                    && bytes == core::mem::size_of_val(&self.pixels) as u64
            );
            self.mapped = true;
            *out = a::GfxBufferMap {
                lease: a::GfxBufferHandle {
                    id: 5,
                    generation: 9,
                    ..a::GfxBufferHandle::default()
                },
                cpu_address: self.pixels.as_mut_ptr() as u64,
                byte_length: bytes,
                cache_policy: a::gfx_buffer_cache_write_back,
                ..a::GfxBufferMap::default()
            };
            1
        }
        fn unmap(&mut self, _: &a::GfxBufferHandle) -> i32 {
            assert!(self.mapped);
            self.mapped = false;
            1
        }
        fn release(&mut self, _: &a::GfxBufferHandle) -> i32 {
            assert!(!self.mapped && self.status.completed >= 1);
            self.released = true;
            1
        }
    }
    impl Model {
        fn finish(&mut self) {
            self.status.completed = self.status.sequence;
            self.status.phase = a::display_cursor_phase_complete;
            match self.request.operation {
                a::display_cursor_operation_prepare => {
                    self.status.image_sequence = self.status.sequence;
                    self.status.flags |= a::display_cursor_state_image_ready;
                }
                a::display_cursor_operation_show | a::display_cursor_operation_move => {
                    self.status.flags |= a::display_cursor_state_visible;
                    self.status.x = self.request.x;
                    self.status.y = self.request.y;
                }
                a::display_cursor_operation_hide => {
                    self.status.flags &= !a::display_cursor_state_visible
                }
                a::display_cursor_operation_release => {
                    self.status.flags = 0;
                    self.status.image_sequence = 0;
                }
                _ => {}
            }
        }
    }
    #[test]
    fn close_waits_for_prepare_release_and_lost_state_without_freeing_early() {
        let mut model = Model::default();
        let mut cursor = Controller::default();
        cursor.poll(&mut model, 1, 10, 20, true);
        assert!(!cursor.close(&mut model, 2));
        assert!(cursor.acquired && !model.released && model.calls == 1);
        model.finish();
        assert!(!cursor.close(&mut model, 3));
        assert!(model.request.operation == a::display_cursor_operation_release && model.calls == 2);
        assert!(!cursor.close(&mut model, 4));
        model.status.phase = a::display_cursor_phase_lost;
        assert!(!cursor.close(&mut model, 5) && cursor.acquired);
        model.finish();
        assert!(cursor.close(&mut model, 6));
        assert!(
            !cursor.acquired && cursor.reference.reference.id == 0 && cursor.mapping.lease.id == 0
        );
        let mut absent = Model {
            supported: false,
            ..Model::default()
        };
        assert!(Controller::default().close(&mut absent, 1));
    }
    #[test]
    fn original_plane_prepare_clean_frame_busy_move_hide_lost_late_and_exact_owner_receipts() {
        let mut old = Model {
            supported: false,
            ..Model::default()
        };
        let mut fallback = Controller::default();
        assert!(
            !fallback.poll(&mut old, 1, 10, 20, true)
                && fallback.software
                && old.calls == 0
                && old.creates == 0
        );
        let mut model = Model::default();
        let mut cursor = Controller::default();
        assert!(
            !cursor.poll(&mut model, 1, 10, 20, true)
                && cursor.software
                && model.request.operation == a::display_cursor_operation_prepare
        );
        assert!(
            model.pixels[0] == 0xff000000
                && model.pixels[2 * asset::WIDTH + 1] == 0xffffffff
                && model.pixels[asset::WIDTH - 1] == 0
        );
        cursor.poll(&mut model, 2, 12, 21, true);
        assert!(model.calls == 1 && !model.released && cursor.software);
        model.finish();
        assert!(
            cursor.poll(&mut model, 3, 12, 21, true)
                && !cursor.software
                && cursor.clean_pending
                && model.released
        );
        cursor.presented(false);
        cursor.poll(&mut model, 4, 12, 21, true);
        assert_eq!(model.calls, 1);
        cursor.presented(true);
        model.busy = true;
        cursor.poll(&mut model, 5, 12, 21, true);
        assert!(cursor.clean_ready && model.calls == 1);
        model.busy = false;
        cursor.poll(&mut model, 6, 12, 21, true);
        assert!(
            model.request.operation == a::display_cursor_operation_show
                && model.request.image_sequence == 1
                && !cursor.software
        );
        cursor.poll(&mut model, 7, 15, 24, true);
        assert_eq!(model.calls, 2);
        model.finish();
        cursor.poll(&mut model, 8, 17, 27, true);
        assert!(
            model.calls == 3
                && model.request.operation == a::display_cursor_operation_move
                && model.request.x == 17
                && !cursor.software
        );
        cursor.poll(&mut model, 9, 22, 29, true);
        assert_eq!(model.calls, 3);
        model.finish();
        cursor.poll(&mut model, 10, 22, 29, true);
        assert!(model.calls == 4 && model.request.x == 22 && model.creates == 1);
        model.finish();
        cursor.poll(&mut model, 11, 22, 29, false);
        assert!(model.request.operation == a::display_cursor_operation_hide && !cursor.software);
        cursor.poll(&mut model, 12, 22, 29, false);
        assert!(!cursor.software);
        model.finish();
        assert!(cursor.poll(&mut model, 13, 22, 29, false) && cursor.software);
        model.status.flags |= a::display_cursor_state_suspended;
        cursor.poll(&mut model, 14, 22, 29, true);
        assert!(cursor.software && model.calls == 5);
        model.status.flags &= !a::display_cursor_state_suspended;
        assert!(cursor.poll(&mut model, 15, 22, 29, true) && cursor.clean_pending);
        cursor.poll(&mut model, 16, 22, 29, true);
        assert_eq!(model.calls, 5);
        cursor.presented(true);
        cursor.poll(&mut model, 17, 22, 29, true);
        model.status.phase = a::display_cursor_phase_lost;
        model.status.flags |= a::display_cursor_state_unknown;
        cursor.poll(&mut model, 18, 22, 29, false);
        assert!(!cursor.software && model.calls == 6);
        model.status.flags &= !a::display_cursor_state_unknown;
        model.finish();
        cursor.poll(&mut model, 19, 22, 29, false);
        assert!(
            model.calls == 7
                && model.request.operation == a::display_cursor_operation_hide
                && !cursor.software
        );
        model.finish();
        cursor.poll(&mut model, 20, 22, 29, false);
        assert!(cursor.software);
        let mut managed = Model::default();
        let mut exact = Controller::default();
        exact.poll(&mut managed, 1, 10, 20, true);
        managed.finish();
        exact.poll(&mut managed, 2, 10, 20, true);
        assert!(exact.clean_pending && !exact.software);
        assert!(exact.poll(&mut managed, 3, 10, 20, false));
        assert!(!exact.clean_pending && !exact.clean_ready && exact.software && managed.calls == 1);
        for now in 4..8 {
            assert!(!exact.poll(&mut managed, now, 10, 20, false) && managed.calls == 1);
        }
        assert!(exact.poll(&mut managed, 8, 10, 20, true));
        assert!(exact.clean_pending && !exact.clean_ready && managed.calls == 1);
        let key = CleanFrame {
            target: a::GfxOutputTarget {
                adapter_id: 1,
                connector_id: 2,
                device_generation: 3,
                connection_generation: 4,
                display_generation: 7,
                ..a::GfxOutputTarget::default()
            },
            device_address: 0x1000,
            device_generation: 11,
            frame: 19,
        };
        exact.captured(key);
        for change in 0..5 {
            let mut wrong = key;
            match change {
                0 => wrong.target.connection_generation += 1,
                1 => wrong.target.head_id += 1,
                2 => wrong.device_generation += 1,
                3 => wrong.frame -= 1,
                4 => wrong.device_address += 1,
                _ => unreachable!(),
            }
            assert!(!exact.completed(wrong, true));
            exact.poll(&mut managed, 10 + change, 10, 20, true);
            assert!(!exact.clean_ready && managed.calls == 1);
        }
        assert!(exact.completed(key, false) && !exact.clean_ready && exact.clean_frame.is_none());
        exact.poll(&mut managed, 20, 10, 20, true);
        assert_eq!(managed.calls, 1);
        let retry = CleanFrame {
            frame: key.frame + 1,
            ..key
        };
        exact.captured(retry);
        assert!(exact.completed(retry, true));
        exact.poll(&mut managed, 21, 10, 20, true);
        assert!(
            managed.calls == 2 && managed.request.operation == a::display_cursor_operation_show
        );
    }
}
