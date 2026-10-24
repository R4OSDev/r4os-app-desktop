//! Recording worker's one NV12 input and one delayed compressed packet.
//! Codec work never borrows the UI; physical close precedes BO retirement.
use super::file::Output;
use crate::runtime::worker::Control;
use r4enc as e;
use r4os_library_helpers::media::{Mux, Nv12};
use r4os_sdk::{App, abi as a, c_facades as c, graphics::Graphics, time::Clock};
pub const INTERVAL: u64 = 1_000_000_000 / 30;
pub const WORK_NS: u64 = 5_000_000_000;
pub const MEMORY_LIMIT: u64 = 256 * 1024 * 1024;
const PACKET_LIMIT: u64 = 8 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Clock,
    Unavailable,
    Layout,
    Buffer,
    Map,
    Unmap,
    Cancelled,
    Send,
    Encode,
    Timeout,
    Timestamp,
    Write,
    Drain,
}
pub struct Codec<'a, 's> {
    app: &'a App<'a>,
    graphics: &'a Graphics<'s, 'a>,
    api: &'a e::EncodeV1Client<'a>,
    encoder: Option<e::R4EncEncoder>,
    buffer: a::GfxBufferReference,
    mapped: a::GfxBufferMap,
    layout: Option<Nv12>,
    pub width: u32,
    pub height: u32,
    pitch: u32,
    offset: u64,
    bytes: u64,
    pub backend: u32,
    pending: Option<e::R4EncPacket>,
    request: u64,
    tag: u64,
}
fn status(slot: c::R4SdkGuiSlot) -> i32 {
    if slot.status < 0 {
        slot.status
    } else if slot.flags & 2 != 0 {
        a::gfx_buffer_error_invalid
    } else {
        slot.bits as i32
    }
}
pub fn now(app: &App<'_>) -> Result<u64, Error> {
    Clock::new(app)
        .precise_instant()
        .map(|i| i.nanoseconds)
        .map_err(|_| Error::Clock)
}
impl<'a, 's> Codec<'a, 's> {
    pub fn new(
        app: &'a App<'a>,
        graphics: &'a Graphics<'s, 'a>,
        api: &'a e::EncodeV1Client<'a>,
    ) -> Self {
        Self {
            app,
            graphics,
            api,
            encoder: None,
            buffer: Default::default(),
            mapped: Default::default(),
            layout: None,
            width: 0,
            height: 0,
            pitch: 0,
            offset: 0,
            bytes: 0,
            backend: e::backend_software,
            pending: None,
            request: 0,
            tag: 0,
        }
    }
    pub fn native(&self) -> bool {
        self.backend != e::backend_software
    }
    pub fn open(
        &mut self,
        runtime: &e::R4EncRuntime,
        width: u32,
        height: u32,
        adapter: u32,
        software_only: bool,
    ) -> Result<(), Error> {
        // Validate the shared owner's exact dimensions before codec allocation.
        let _ = Nv12::new(width, height, false).map_err(|_| Error::Layout)?;
        for backend in [e::backend_amd, e::backend_nvidia, e::backend_software] {
            if (software_only || adapter == 0) && backend != e::backend_software {
                continue;
            }
            let config = config(width, height, backend, adapter);
            let mut caps = e::R4EncCaps::default();
            let mut handle = e::R4EncEncoder::default();
            if unsafe { self.api.query_caps(runtime, &config.query, &mut caps) } == e::ok
                && unsafe { self.api.create(runtime, &config, &mut handle) } == e::ok
            {
                self.encoder = Some(handle);
                self.backend = backend;
                break;
            }
        }
        if self.encoder.is_none() {
            return Err(Error::Unavailable);
        }
        let layout =
            Nv12::new(width, height, self.backend == e::backend_amd).map_err(|_| Error::Layout)?;
        self.bytes = layout.bytes().map_err(|_| Error::Layout)? as u64;
        self.pitch = layout.pitch().map_err(|_| Error::Layout)?;
        self.offset = layout.uv_offset().map_err(|_| Error::Layout)? as u64;
        self.layout = Some(layout);
        self.width = width;
        self.height = height;
        let descriptor = a::GfxBufferDescriptor {
            byte_length: self.bytes,
            alignment: 65536,
            location: a::gfx_buffer_location_system,
            format: a::gfx_buffer_format_nv12,
            width,
            height,
            plane_count: 2,
            plane_offsets: [0, self.offset, 0, 0],
            plane_pitches: [self.pitch as u64, self.pitch as u64, 0, 0],
            usage: a::gfx_buffer_usage_cpu_read
                | a::gfx_buffer_usage_cpu_write
                | a::gfx_buffer_usage_transfer_source,
            ..Default::default()
        };
        let rc = unsafe {
            status(c::R4SdkGuiDrawContext_gfxBufferCreate(
                0,
                self.graphics.native_draw(),
                &descriptor,
                &mut self.buffer,
            ))
        };
        if rc != a::gfx_buffer_result_ok {
            return Err(Error::Buffer);
        }
        Ok(())
    }
    pub fn convert(&mut self, source: &[u32], stride: u32, control: &Control) -> Result<(), Error> {
        let rc = unsafe {
            status(c::R4SdkGuiDrawContext_gfxBufferMap(
                0,
                self.graphics.native_draw(),
                &self.buffer.reference,
                a::gfx_buffer_map_write,
                0,
                self.bytes,
                &mut self.mapped,
            ))
        };
        if rc != a::gfx_buffer_result_ok {
            return Err(Error::Map);
        }
        if self.mapped.lease.id == 0
            || self.mapped.cpu_address == 0
            || self.mapped.byte_length < self.bytes
            || self.mapped.cpu_address.checked_add(self.bytes).is_none()
        {
            return Err(Error::Map);
        }
        let target = unsafe {
            core::slice::from_raw_parts_mut(self.mapped.cpu_address as *mut u8, self.bytes as usize)
        };
        let layout = self.layout.as_ref().ok_or(Error::Layout)?;
        for row in (0..self.height).step_by(16) {
            if control.stopped() {
                return Err(Error::Cancelled);
            }
            layout
                .rows(source, stride, target, row, 16.min(self.height - row))
                .map_err(|_| Error::Layout)?;
        }
        if self.graphics.buffers().unmap(&self.mapped.lease).is_err() {
            return Err(Error::Unmap);
        }
        self.mapped = Default::default();
        Ok(())
    }
    pub fn encode(
        &mut self,
        pts: u64,
        mux: &mut Mux,
        file: &mut Output<'_>,
    ) -> Result<bool, Error> {
        let handle = self.encoder.as_ref().ok_or(Error::Unavailable)?;
        self.tag = self.tag.saturating_add(1);
        let plane = e::R4EncPlane {
            buffer: enc_buffer(self.buffer.buffer),
            reference: enc_buffer(self.buffer.reference),
            offset: 0,
            pitch: self.pitch as u64,
            row_bytes: self.width as u64,
            rows: self.height,
            ..Default::default()
        };
        let frame = e::R4EncFrame {
            version: 1,
            size: core::mem::size_of::<e::R4EncFrame>() as u32,
            stream_generation: 1,
            tag: self.tag,
            pts_ns: i64::try_from(pts).map_err(|_| Error::Timestamp)?,
            duration_ns: INTERVAL,
            format: e::format_nv12,
            plane_count: 2,
            plane0: plane,
            plane1: e::R4EncPlane {
                offset: self.offset,
                rows: self.height / 2,
                ..plane
            },
            ..Default::default()
        };
        let end = pts.saturating_add(WORK_NS + 1_000_000_000);
        loop {
            let rc = unsafe { self.api.send(handle, &frame) };
            if rc == e::ok {
                break;
            }
            if rc != e::error_busy && rc != e::again {
                return Err(Error::Send);
            }
            if now(self.app)? >= end {
                return Err(Error::Timeout);
            }
            self.app.sleep(1);
        }
        let mut packet = e::R4EncPacket::default();
        loop {
            let rc = unsafe { self.api.receive(handle, &mut packet) };
            if rc == e::ok {
                break;
            }
            if rc != e::error_busy && rc != e::again {
                return Err(Error::Encode);
            }
            if now(self.app)? >= end {
                return Err(Error::Timeout);
            }
            self.app.sleep(1);
        }
        if packet.result != e::ok
            || packet.flags & e::packet_skipped != 0
            || packet.tag != frame.tag
            || packet.pts_ns != frame.pts_ns
            || packet.data_address == 0
            || packet.data_bytes == 0
            || packet.data_bytes > PACKET_LIMIT
            || packet.data_address.checked_add(packet.data_bytes).is_none()
        {
            self.release_packet(&packet);
            return Err(Error::Encode);
        }
        // The just-received packet proves this reusable BO's physical input
        // retirement. Disk output sees only the previous compressed lease.
        let previous = match self.write_pending(pts, mux, file) {
            Ok(previous) => previous,
            Err(error) => {
                self.release_packet(&packet);
                return Err(error);
            }
        };
        self.pending = Some(packet);
        Ok(previous)
    }
    pub fn write_pending(
        &mut self,
        until: u64,
        mux: &mut Mux,
        file: &mut Output<'_>,
    ) -> Result<bool, Error> {
        let Some(packet) = self.pending else {
            return Ok(false);
        };
        let pts = u64::try_from(packet.pts_ns).map_err(|_| Error::Timestamp)?;
        let bytes = unsafe {
            core::slice::from_raw_parts(
                packet.data_address as *const u8,
                packet.data_bytes as usize,
            )
        };
        mux.packet(
            bytes,
            packet.flags & e::packet_key != 0,
            packet.pts_ns,
            until.saturating_sub(pts).max(1),
            |part| file.write(part),
        )
        .map_err(|_| Error::Write)?;
        self.release_packet(&packet);
        self.pending = None;
        Ok(true)
    }
    fn release_packet(&self, packet: &e::R4EncPacket) {
        while unsafe { self.api.release(&packet.lease) } != e::ok {
            self.app.sleep(1);
        }
    }
    fn control(&self, op: u32, id: u64, out: &mut e::R4EncState) -> i32 {
        let Some(handle) = self.encoder.as_ref() else {
            return e::error_closed;
        };
        let request = e::R4EncControl {
            version: 1,
            size: core::mem::size_of::<e::R4EncControl>() as u32,
            operation: op,
            request_id: id,
            ..Default::default()
        };
        unsafe { self.api.control(handle, &request, out) }
    }
    pub fn drain(&mut self) -> Result<(), Error> {
        let end = now(self.app)?.saturating_add(WORK_NS);
        self.request = self.request.saturating_add(1);
        let mut state = e::R4EncState::default();
        loop {
            let rc = self.control(e::control_drain, self.request, &mut state);
            if rc == e::ok {
                break;
            }
            if rc != e::error_busy {
                return Err(Error::Drain);
            }
            if now(self.app)? >= end {
                return Err(Error::Timeout);
            }
            self.app.sleep(1);
        }
        while state.completed_request != self.request || state.phase != e::phase_drained {
            let rc = self.control(e::control_query, 0, &mut state);
            if rc != e::ok && rc != e::error_busy {
                return Err(Error::Drain);
            }
            if now(self.app)? >= end {
                return Err(Error::Timeout);
            }
            self.app.sleep(1);
        }
        Ok(())
    }
    /// Worker-only retirement. No timeout, cancellation, Drop or failed fence
    /// authorizes releasing storage still held by the actual codec/provider.
    pub fn retire(&mut self) {
        if let Some(packet) = self.pending {
            self.release_packet(&packet);
            self.pending = None;
        }
        if let Some(handle) = self.encoder {
            self.request = self.request.saturating_add(1);
            let mut state = e::R4EncState::default();
            while self.control(e::control_close, self.request, &mut state) != e::ok {
                self.app.sleep(1);
            }
            loop {
                let rc = self.control(e::control_query, 0, &mut state);
                if rc == e::ok
                    && state.phase == e::phase_closed
                    && state.completed_request == self.request
                {
                    break;
                }
                self.app.sleep(1);
            }
            while unsafe { self.api.destroy(&handle) } != e::ok {
                self.app.sleep(1);
            }
            self.encoder = None;
        }
        if self.mapped.lease.id != 0 {
            while self.graphics.buffers().unmap(&self.mapped.lease).is_err() {
                self.app.sleep(1);
            }
            self.mapped = Default::default();
        }
        if self.buffer.reference.id != 0 {
            while self
                .graphics
                .buffers()
                .release(&self.buffer.reference)
                .is_err()
            {
                self.app.sleep(1);
            }
            self.buffer = Default::default();
        }
    }
}
fn enc_buffer(h: a::GfxBufferHandle) -> e::R4EncBuffer {
    e::R4EncBuffer {
        id: h.id,
        reserved0: h.reserved0,
        generation: h.generation,
    }
}
fn config(width: u32, height: u32, backend: u32, adapter: u32) -> e::R4EncConfig {
    e::R4EncConfig {
        version: 1,
        size: core::mem::size_of::<e::R4EncConfig>() as u32,
        query: e::R4EncCapsQuery {
            version: 1,
            size: core::mem::size_of::<e::R4EncCapsQuery>() as u32,
            backend,
            adapter_id: if backend == e::backend_software {
                0
            } else {
                adapter
            },
            codec: e::codec_h264,
            profile: e::profile_h264_baseline,
            bit_depth: 8,
            chroma: e::chroma_420,
        },
        memory_limit: MEMORY_LIMIT,
        width,
        height,
        fps_num: 30,
        fps_den: 1,
        pending_frames: 1,
        packet_leases: 2,
        max_packet_bytes: PACKET_LIMIT,
        work_timeout_ns: WORK_NS,
        gop_frames: 60,
        rate: e::R4EncRate {
            version: 1,
            size: core::mem::size_of::<e::R4EncRate>() as u32,
            mode: e::rate_cqp,
            qp: 26,
            min_qp: 0,
            max_qp: 51,
            ..Default::default()
        },
        color: e::R4EncColor {
            version: 1,
            size: core::mem::size_of::<e::R4EncColor>() as u32,
            primaries: 1,
            transfer: 13,
            matrix: 1,
            range: 2,
            chroma_location: e::chroma_unspecified,
            bit_depth: 8,
            ..Default::default()
        },
        ..Default::default()
    }
}
