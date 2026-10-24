//! Per-output system-memory FIFO. Each connector owns its BO pool, damage
//! history, maps and actual presentation/capture receipts independently.
use super::profile::Profile;
use crate::{
    capture::remote::{Capture, Cursor, Image},
    render::{
        geometry::Rect,
        gpu::{
            Error, Result, accepted,
            device::{Device, OpenError},
        },
        output_damage::History,
        output_geometry::topology::Viewport,
        software,
    },
    runtime::{library::Context, owned::Owned},
};
use alloc::vec::Vec;
use core::mem;
use r4gfx as g;
use r4os_sdk::{App, abi as a, c_facades as c, graphics::Graphics};
#[path = "cpu_frame.rs"]
mod frame;
#[cfg(test)]
#[path = "../../../Tests/RustNative/cpu_output.rs"]
mod tests;
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub completed: u64,
    pub visible: u64,
    pub failed: u64,
    pub discarded: u64,
    pub last_submitted: Option<u32>,
    pub frame_submit_last_ns: u64,
    pub frame_submit_max_ns: u64,
}
struct State {
    device: Device,
    capture: Capture,
    capture_cursor: Cursor,
    capture_damage: Option<Rect>,
    history: History,
    repaint: Rect,
    history_view: Option<Viewport>,
    history_profile: g::R4GfxColorProfile,
    history_limited: bool,
    serial: u64,
    reported: [bool; 3],
    view: Viewport,
    target: a::GfxOutputTarget,
    references: [a::GfxBufferReference; 3],
    images: [g::R4GfxResource; 3],
    chain: g::R4GfxSwapchain,
    acquired: Option<g::R4GfxSwapchainFrame>,
    mapping: a::GfxBufferMap,
    scratch: Vec<u32>,
    composition: software::Owner,
    profile: Option<Profile>,
    limited: bool,
    count: u32,
    ready: bool,
    pending: bool,
    lost: bool,
    stats: Stats,
    prepare_stage: &'static str,
    prepare_error: Option<Error>,
    last_status: i32,
}
pub struct Output {
    state: Option<Owned<State>>,
}
fn code(slot: c::R4SdkGuiSlot) -> i32 {
    if slot.status < 0 {
        slot.status
    } else if slot.flags & 2 != 0 {
        a::gfx_buffer_error_invalid
    } else {
        slot.bits as i32
    }
}
fn same_resource(a: g::R4GfxResource, b: g::R4GfxResource) -> bool {
    a.slot == b.slot
        && a.kind == b.kind
        && a.generation == b.generation
        && a.device_address == b.device_address
        && a.device_generation == b.device_generation
}
impl Output {
    /// # Safety
    /// Keep actual provider/loader/callback code resident through successful
    /// close. A partial failed output is returned and retains its exact owners.
    pub unsafe fn open(
        app: &App<'_>,
        graphics: &Graphics<'_, '_>,
        view: Viewport,
        target: a::GfxOutputTarget,
    ) -> core::result::Result<Self, OpenError> {
        let composition = software::Owner::new(128 * 1024 * 1024).map_err(|_| OpenError {
            error: super::super::render::gpu::device::Error::Memory,
            retained: None,
        })?;
        let device = unsafe { Device::open(app, target.adapter_id) }?;
        let state = State {
            device,
            capture: Capture::default(),
            capture_cursor: Cursor::default(),
            capture_damage: None,
            history: History::default(),
            repaint: Rect::ZERO,
            history_view: None,
            history_profile: g::R4GfxColorProfile::default(),
            history_limited: false,
            serial: 0,
            reported: [false; 3],
            view,
            target,
            references: [a::GfxBufferReference::default(); 3],
            images: [g::R4GfxResource::default(); 3],
            chain: g::R4GfxSwapchain::default(),
            acquired: None,
            mapping: a::GfxBufferMap::default(),
            scratch: Vec::new(),
            composition,
            profile: None,
            limited: false,
            count: 0,
            ready: false,
            pending: false,
            lost: false,
            stats: Stats::default(),
            prepare_stage: "presentation-info",
            prepare_error: None,
            last_status: 0,
        };
        let mut state = match Owned::new(state) {
            Ok(value) => value,
            Err(mut state) => {
                let closed = state.device.close(app).unwrap_or(false);
                return Err(OpenError {
                    error: crate::render::gpu::device::Error::Memory,
                    retained: (!closed).then_some(state.device),
                });
            }
        };
        if let Err(error) = state.prepare(app, graphics) {
            state.lost = true;
            state.prepare_error = Some(error);
        }
        Ok(Self { state: Some(state) })
    }
    pub fn ready(&self) -> bool {
        self.state.as_ref().is_some_and(|s| s.ready && !s.lost)
    }
    pub fn lost(&self) -> bool {
        self.state.as_ref().is_some_and(|s| s.lost)
    }
    pub fn pending(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(|s| s.pending || s.capture.needs_polling())
    }
    pub fn acquired(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(|s| s.acquired.is_some() || s.mapping.lease.id != 0)
    }
    pub fn view(&self) -> Option<Viewport> {
        self.state.as_ref().map(|s| s.view)
    }
    pub fn set_view(&mut self, view: Viewport) -> bool {
        let Some(state) = self.state.as_deref_mut() else {
            return false;
        };
        if state.acquired.is_some()
            || state.mapping.lease.id != 0
            || view.validate().is_err()
            || view.pixel_w != state.view.pixel_w
            || view.pixel_h != state.view.pixel_h
        {
            return false;
        }
        state.view = view;
        true
    }
    pub fn limited(&mut self, limited: bool) {
        if let Some(s) = &mut self.state {
            s.limited = limited;
        }
    }
    pub fn stats(&self) -> Stats {
        self.state.as_ref().map_or(Stats::default(), |s| s.stats)
    }
    pub fn preparation(&self) -> Option<(&'static str, Option<Error>, i32)> {
        self.state
            .as_ref()
            .map(|s| (s.prepare_stage, s.prepare_error, s.last_status))
    }
    /// # Safety
    /// Library/profile callback code stays resident until this output closes.
    pub unsafe fn set_profile(
        &mut self,
        app: &App<'_>,
        context: &Context<'_, '_>,
        path: &[u8],
        intent: u32,
        flags: u32,
    ) -> Result<()> {
        let state = self.state.as_deref_mut().ok_or(Error::State)?;
        if state.profile.is_some()
            || state.acquired.is_some()
            || state.pending
            || state.stats.completed != 0
        {
            return Err(Error::State);
        }
        state.profile = Some(
            unsafe { Profile::open_file(app, context, path, intent, flags) }
                .map_err(|_| Error::Graphics)?,
        );
        Ok(())
    }
    pub fn poll(&mut self, app: &App<'_>, time: u64) {
        if let Some(state) = self.state.as_deref_mut() {
            state.poll(app, time);
        }
    }
    pub fn capture_demand(&mut self, wanted: bool) {
        if let Some(s) = &mut self.state {
            s.capture.demand(wanted);
        }
    }
    pub fn invalidate_capture(&mut self) {
        if let Some(s) = &mut self.state {
            s.capture.invalidate();
        }
    }
    pub fn capture_cursor(&mut self, cursor: Cursor) {
        if let Some(s) = &mut self.state {
            s.capture_cursor = cursor;
        }
    }
    pub fn capture_damage(&mut self, damage: Option<Rect>) {
        if let Some(s) = &mut self.state {
            s.capture_damage = damage;
        }
    }
    pub fn capture_image(&self) -> Option<Image<'_>> {
        self.state.as_ref()?.capture.image()
    }
    pub fn acknowledge_capture(&mut self, published: bool) -> bool {
        self.state
            .as_mut()
            .is_some_and(|s| s.capture.acknowledge(published).is_ok())
    }
    pub fn capture_redraw(&mut self, time: u64) -> bool {
        self.state
            .as_mut()
            .is_some_and(|s| s.capture.take_redraw(time))
    }
    /// Refusal preserves each surviving map/acquisition/chain/image/reference.
    pub fn close(&mut self, app: &App<'_>, graphics: &Graphics<'_, '_>, time: u64) -> bool {
        let Some(state) = self.state.as_deref_mut() else {
            return true;
        };
        state.capture.demand(false);
        state.poll(app, time);
        {
            let Ok(session) = state.device.bind(app) else {
                return false;
            };
            if state.capture.close(&session).is_err() {
                return false;
            }
        }
        state.abandon(app, graphics);
        if state.mapping.lease.id != 0 || state.acquired.is_some() {
            return false;
        }
        {
            let Ok(session) = state.device.bind(app) else {
                return false;
            };
            if state.chain.slot != 0 {
                if unsafe { session.client.swapchain_close(session.raw(), &state.chain) }
                    != g::status_ok
                {
                    return false;
                }
                state.chain = g::R4GfxSwapchain::default();
            }
            for i in 0..3 {
                if state.images[i].slot != 0 {
                    if unsafe {
                        session
                            .client
                            .resource_release(session.raw(), &state.images[i])
                    } != g::status_ok
                    {
                        return false;
                    }
                    state.images[i] = g::R4GfxResource::default();
                }
                if state.references[i].reference.id != 0 {
                    if unsafe {
                        code(c::R4SdkGuiDrawContext_gfxBufferRelease(
                            0,
                            graphics.native_draw(),
                            &state.references[i].reference,
                        ))
                    } != a::gfx_buffer_result_ok
                    {
                        return false;
                    }
                    state.references[i] = a::GfxBufferReference::default();
                }
            }
        }
        if let Some(profile) = &mut state.profile {
            if !profile.close().unwrap_or(false) {
                return false;
            }
        }
        state.profile = None;
        if !state.device.close(app).unwrap_or(false) {
            return false;
        }
        drop(self.state.take());
        true
    }
}
impl State {
    fn prepare(&mut self, app: &App<'_>, graphics: &Graphics<'_, '_>) -> Result<()> {
        self.view.validate().map_err(|_| Error::State)?;
        let session = self.device.bind(app).map_err(|_| Error::Graphics)?;
        let mut info = g::R4GfxPresentationInfo::default();
        self.last_status = unsafe {
            session
                .client
                .presentation_info(session.raw(), self.target.head_id, &mut info)
        };
        accepted(self.last_status)?;
        if info.display_generation != self.target.display_generation
            || info.width != self.view.pixel_w
            || info.height != self.view.pixel_h
            || info.flags & g::present_native == 0
            || info.buffer_count < 2
            || info.buffer_count > 3
            || info.format != g::format_xrgb8888
        {
            return Err(Error::Stale);
        }
        let pitch = u64::from(info.width) * 4;
        let bytes = pitch * u64::from(info.height);
        if bytes > 64 * 1024 * 1024 {
            return Err(Error::Limit);
        }
        self.count = info.buffer_count;
        for i in 0..self.count as usize {
            self.prepare_stage = "image-allocation";
            let descriptor = a::GfxBufferDescriptor {
                width: info.width,
                height: info.height,
                byte_length: bytes,
                format: a::gfx_buffer_format_xrgb8888,
                plane_count: 1,
                plane_pitches: [pitch, 0, 0, 0],
                usage: a::gfx_buffer_usage_cpu_read
                    | a::gfx_buffer_usage_cpu_write
                    | a::gfx_buffer_usage_transfer_source
                    | a::gfx_buffer_usage_render,
                ..Default::default()
            };
            self.last_status = unsafe {
                code(c::R4SdkGuiDrawContext_gfxBufferCreate(
                    0,
                    graphics.native_draw(),
                    &descriptor,
                    &mut self.references[i],
                ))
            };
            if self.last_status != a::gfx_buffer_result_ok {
                return Err(Error::Graphics);
            }
            self.prepare_stage = "image-import";
            let descriptor = g::R4GfxResourceDesc {
                version: 1,
                size: core::mem::size_of::<g::R4GfxResourceDesc>() as u32,
                kind: g::resource_image,
                flags: g::image_target,
                source_kind: g::source_import_buffer,
                source_address: &self.references[i].reference as *const _ as u64,
                ..Default::default()
            };
            self.last_status = unsafe {
                session
                    .client
                    .resource_create(session.raw(), &descriptor, &mut self.images[i])
            };
            accepted(self.last_status)?;
        }
        self.prepare_stage = "swapchain-open";
        let descriptor = g::R4GfxSwapchainDesc {
            version: 1,
            size: core::mem::size_of::<g::R4GfxSwapchainDesc>() as u32,
            head_id: self.target.head_id,
            policy: g::present_policy_fifo,
            display_generation: info.display_generation,
            count: self.count,
            images: self.images.as_ptr() as u64,
            ..Default::default()
        };
        self.last_status = unsafe {
            session
                .client
                .swapchain_open(session.raw(), &descriptor, &mut self.chain)
        };
        accepted(self.last_status)?;
        self.ready = true;
        Ok(())
    }
    fn poll(&mut self, app: &App<'_>, time: u64) {
        let Ok(session) = self.device.bind(app) else {
            return;
        };
        if self.capture.poll(&session, time).is_err() {
            self.capture.retry(time);
        }
        if self.chain.slot == 0 {
            return;
        }
        let mut status = g::R4GfxSwapchainStatus::default();
        let rc = unsafe {
            session
                .client
                .swapchain_poll(session.raw(), &self.chain, &mut status)
        };
        if rc != g::status_ok {
            if rc != g::status_busy {
                self.lost = true;
            }
            return;
        }
        if status.life >= 2 {
            self.lost = true;
        }
        self.pending = false;
        for (i, frame) in [status.frame0, status.frame1, status.frame2]
            .into_iter()
            .enumerate()
        {
            if frame.phase <= 1 {
                continue;
            }
            if frame.phase != 4 {
                self.pending = true;
                continue;
            }
            if !self.reported[i] {
                self.reported[i] = true;
                if frame.result == 1 || frame.result == 2 {
                    self.stats.completed = self.stats.completed.saturating_add(1);
                    self.capture.complete(&session, i, frame.frame.image, time);
                } else {
                    self.capture.discarded(i);
                    self.history.rejected(i);
                    if frame.result == 3 {
                        self.stats.discarded = self.stats.discarded.saturating_add(1);
                    } else {
                        self.stats.failed = self.stats.failed.saturating_add(1);
                        self.lost = true;
                    }
                }
                if frame.result == 1 && frame.visible_ns != 0 {
                    self.stats.visible = self.stats.visible.saturating_add(1);
                }
            }
            let held_source = match self.capture.source() {
                Ok(source) => same_resource(source, frame.frame.image),
                Err(_) => true,
            };
            if frame.held_flags != 0 || held_source {
                self.pending = true;
                continue;
            }
            if unsafe {
                session
                    .client
                    .swapchain_release(session.raw(), &self.chain, &frame.frame)
            } != g::status_ok
            {
                self.pending = true;
                continue;
            }
            self.reported[i] = false;
        }
    }
    fn abandon(&mut self, app: &App<'_>, graphics: &Graphics<'_, '_>) {
        if let Some(frame) = self.acquired {
            if frame.slot > 0 && frame.slot <= 3 {
                self.history.rejected(frame.slot as usize - 1);
            }
        }
        self.composition.cancel();
        if self.mapping.lease.id != 0 {
            if unsafe {
                code(c::R4SdkGuiDrawContext_gfxBufferUnmap(
                    0,
                    graphics.native_draw(),
                    &self.mapping.lease,
                ))
            } != a::gfx_buffer_result_ok
            {
                self.lost = true;
                return;
            }
            self.mapping = a::GfxBufferMap::default();
        }
        if let Some(frame) = self.acquired {
            self.capture.discarded(frame.slot.wrapping_sub(1) as usize);
            let Ok(session) = self.device.bind(app) else {
                self.lost = true;
                return;
            };
            if unsafe {
                session
                    .client
                    .swapchain_release(session.raw(), &self.chain, &frame)
            } != g::status_ok
            {
                self.lost = true;
                return;
            }
            self.acquired = None;
        }
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            mem::forget(state);
        }
    }
}
