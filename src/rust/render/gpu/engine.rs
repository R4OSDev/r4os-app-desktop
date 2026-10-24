//! The original bounded native transaction, with short provider-call sessions.
//! Window fence handoff is deliberately performed after those sessions end.
use super::{
    Error, Result, accepted,
    device::Session,
    jobs::Jobs,
    receipt::Consumer,
    resources::{Image, Store, descriptor},
};
use crate::render::{
    color,
    geometry::Rect,
    layers::{self, Cache},
    output_geometry as geometry, primitive_assets,
    primitive_frame::{Command, Frame},
    scene::intersect,
    software,
};
use alloc::vec::Vec;
use r4gfx as g;
use r4os_contract as a;
#[path = "prepare.rs"]
mod prepare;
#[path = "submit.rs"]
mod submit;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    Display,
    Readback,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Upload,
    AssetUpload,
    Primitives,
    LinearClear,
    Draw,
    Encode,
    Present,
    Drain,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Pending,
    Copied,
    Failed,
}
#[derive(Clone, Copy)]
pub struct External {
    pub reference: a::GfxBufferReference,
    pub descriptor: a::GfxBufferDescriptor,
    pub color: g::R4GfxColorDescription,
    pub ready: a::GfxFence,
}
#[derive(Default)]
struct Source {
    value: Option<External>,
    borrowed: bool,
}
pub trait FinishExternal {
    /// This runs outside the provider-call session. Refusal returns the exact
    /// incoming receipt without retiring the window's capture read lease.
    fn finish(
        &mut self,
        entry: usize,
        receipt: Option<Consumer>,
        failed: bool,
    ) -> core::result::Result<(), Option<Consumer>>;
}
pub struct Completion {
    pub frame: u64,
    pub status: g::R4GfxSwapchainFrameStatus,
}
pub struct Engine {
    pub resources: Store,
    pub jobs: Jobs,
    pub head: Option<u32>,
    pub destination: Destination,
    pub output_format: u32,
    pub output_color: g::R4GfxColorDescription,
    color_output: bool,
    hdr_frame: bool,
    pub output_index: usize,
    chain: g::R4GfxSwapchain,
    presentation: Option<g::R4GfxPresentationInfo>,
    chain_status: Option<g::R4GfxSwapchainStatus>,
    acquired: Option<g::R4GfxSwapchainFrame>,
    chain_frames: [u64; 3],
    reported: [bool; 3],
    chain_closing: bool,
    pub readback_pin: g::R4GfxResource,
    pub input_ns: u64,
    pub present_intent: u32,
    pub present_blockers: u32,
    phase: Phase,
    pub frame: u64,
    deadline: u64,
    pub fault: Option<Error>,
    pub present_fence: Option<g::R4GfxCopyFence>,
    stage_source: g::R4GfxResource,
    next_image: usize,
    next_command: usize,
    next_asset: usize,
    asset_x: i32,
    asset_y: i32,
    next_primitive: usize,
    clear_working: bool,
    replace_background: bool,
    encode_area: Rect,
    capture_damage: Option<Rect>,
    capture_view: Option<geometry::topology::Viewport>,
    target_damage: [Option<Rect>; 3],
    sources: Vec<Source>,
    snapshot: Vec<Command>,
    snapshot_images: [g::R4GfxResource; layers::CAPACITY],
    snapshot_assets: [u64; primitive_assets::TEXTURE_CAPACITY],
    reuse_layers: [bool; layers::CAPACITY],
    redraw_regions: [Option<Rect>; layers::CAPACITY],
    batch_enabled: bool,
    grid_enabled: bool,
    pub immediate_work: bool,
    pub last_admission_steps: usize,
    pub render_jobs: u64,
    pub primitive_jobs: u64,
    pub primitive_draws: u64,
    pub primitive_batch_peak: usize,
}
impl Engine {
    pub const ADMISSION_LIMIT: usize = 64;
    pub const ADMISSION_NS: u64 = 250_000;
    pub fn new() -> Result<Self> {
        let mut sources = Vec::new();
        sources
            .try_reserve_exact(layers::CAPACITY)
            .map_err(|_| Error::Limit)?;
        sources.resize_with(layers::CAPACITY, Source::default);
        Ok(Self {
            resources: Store::new()?,
            jobs: Jobs::new()?,
            head: None,
            destination: Destination::Display,
            output_format: g::format_xrgb8888,
            output_color: software::description(false, true),
            color_output: false,
            hdr_frame: false,
            output_index: 0,
            chain: Default::default(),
            presentation: None,
            chain_status: None,
            acquired: None,
            chain_frames: [0; 3],
            reported: [false; 3],
            chain_closing: false,
            readback_pin: Default::default(),
            input_ns: 0,
            present_intent: 0,
            present_blockers: 0,
            phase: Phase::Idle,
            frame: 0,
            deadline: 0,
            fault: None,
            present_fence: None,
            stage_source: Default::default(),
            next_image: 0,
            next_command: 0,
            next_asset: 0,
            asset_x: 0,
            asset_y: 0,
            next_primitive: 0,
            clear_working: true,
            replace_background: false,
            encode_area: Rect::ZERO,
            capture_damage: None,
            capture_view: None,
            target_damage: [None; 3],
            sources,
            snapshot: Vec::new(),
            snapshot_images: [g::R4GfxResource::default(); layers::CAPACITY],
            snapshot_assets: [0; primitive_assets::TEXTURE_CAPACITY],
            reuse_layers: [false; layers::CAPACITY],
            redraw_regions: [None; layers::CAPACITY],
            batch_enabled: false,
            grid_enabled: false,
            immediate_work: false,
            last_admission_steps: 0,
            render_jobs: 0,
            primitive_jobs: 0,
            primitive_draws: 0,
            primitive_batch_peak: 0,
        })
    }
    pub fn active(&self) -> bool {
        self.phase != Phase::Idle
    }
    pub fn pending(&self) -> bool {
        self.chain_frames.iter().any(|v| *v != 0)
    }
    pub fn uses_swapchain(&self) -> bool {
        self.chain.slot != 0
    }
    pub fn required_operations(&self) -> u32 {
        g::device_gpu_render
            | g::device_gpu_copy_rows
            | if self.destination == Destination::Display {
                g::device_gpu_present
            } else {
                0
            }
            | if self.color_output {
                g::device_gpu_color
            } else {
                0
            }
    }
    pub fn configure_output(
        &mut self,
        s: &Session<'_, '_>,
        state: Option<a::GfxOutputColorState>,
        format: u32,
    ) -> Result<()> {
        if self.active() || self.resources.charged != 0 || self.chain.slot != 0 {
            return Err(Error::Busy);
        }
        let mut description = software::description(false, true);
        if let Some(v) = state {
            if v.version != 1
                || v.size < 128
                || v.flags & 7 != 7
                || v.format != format
                || v.reference_white == 0
                || v.peak < v.reference_white
                || v.black >= v.reference_white
                || ![g::color_primaries_srgb, g::color_primaries_bt2020].contains(&v.primaries)
                || ![
                    g::color_transfer_srgb,
                    g::color_transfer_pq,
                    g::color_transfer_hlg,
                ]
                .contains(&v.transfer)
                || ![g::color_range_full, g::color_range_limited].contains(&v.range)
            {
                return Err(Error::Unsupported);
            }
            description.primaries = v.primaries;
            description.transfer = v.transfer;
            description.range = v.range;
            description.reference_white = v.reference_white;
            description.peak = v.peak;
            description.black = v.black;
            description.precision = match format {
                g::format_xrgb8888 if v.bpc == 8 => g::color_precision_unorm8,
                g::format_xrgb2101010 if v.bpc == 10 => g::color_precision_unorm10,
                _ => return Err(Error::Unsupported),
            };
        } else if format != g::format_xrgb8888 {
            return Err(Error::Unsupported);
        }
        accepted(unsafe { s.colors.color_description_validate(&description) })?;
        self.output_format = format;
        self.output_color = description;
        self.color_output =
            !color::same_description(description, software::description(false, true));
        Ok(())
    }
    /// # Safety
    /// The exact imported frame has one capture read lease until
    /// finish_external/discard_external calls the matching window owner.
    pub unsafe fn set_external(&mut self, entry: usize, source: External) -> Result<()> {
        if self.active() || !self.jobs.drained() {
            return Err(Error::Busy);
        }
        let out = self.sources.get_mut(entry).ok_or(Error::State)?;
        if out.borrowed {
            return Err(Error::Busy);
        }
        out.value = Some(source);
        out.borrowed = true;
        Ok(())
    }
    pub fn clear_sources(&mut self) -> Result<()> {
        if self.active() || !self.jobs.drained() || self.sources.iter().any(|s| s.borrowed) {
            return Err(Error::Busy);
        }
        for source in &mut self.sources {
            source.value = None;
        }
        Ok(())
    }
    fn external_source(&self, index: usize) -> Option<External> {
        let source = self.sources.get(index)?;
        if source.borrowed { source.value } else { None }
    }
    fn has_hdr(&self, cache: &Cache) -> bool {
        cache.commands().iter().any(|command| {
            self.external_source(command.entry as usize)
                .is_some_and(|s| color::absolute(s.color))
        })
    }
    pub fn invalidate(&mut self) {
        for value in &mut self.resources.outputs {
            value.generation = 0;
        }
        self.target_damage = [None; 3];
        self.snapshot.clear();
    }
    pub fn composition_damage(
        &mut self,
        view: geometry::topology::Viewport,
        damage: Option<Rect>,
    ) -> Result<()> {
        if self.active() {
            return Err(Error::Busy);
        }
        if self.capture_view != Some(view) {
            self.invalidate();
        }
        self.capture_view = Some(view);
        self.capture_damage = None;
        if view.scale != 120 || view.rotation != geometry::topology::Rotation::Normal {
            return Ok(());
        }
        let bounds = geometry::logical(view).map_err(|_| Error::State)?;
        if let Some(clipped) = damage.and_then(|d| intersect(bounds, d)) {
            self.capture_damage = Some(Rect::new(
                clipped.x - bounds.x,
                clipped.y - bounds.y,
                clipped.w,
                clipped.h,
            ));
        }
        Ok(())
    }
    pub fn acquire(&mut self, s: &Session<'_, '_>, input_ns: u64) -> Result<()> {
        if self.chain.slot == 0 || self.acquired.is_some() {
            return Ok(());
        }
        let mut value = g::R4GfxSwapchainFrame::default();
        accepted(unsafe {
            s.client
                .swapchain_acquire(s.raw(), &self.chain, input_ns, &mut value)
        })?;
        if value.slot == 0
            || value.slot as usize > self.resources.outputs.len()
            || !same_resource(
                value.image,
                self.resources.outputs[value.slot as usize - 1].resource,
            )
        {
            return Err(Error::Stale);
        }
        self.output_index = value.slot as usize - 1;
        self.acquired = Some(value);
        Ok(())
    }
    pub fn needs_full(&self, w: i32, h: i32) -> bool {
        let target = &self.resources.outputs[self.output_index];
        self.chain.slot != 0
            || target.resource.slot == 0
            || target.generation == 0
            || target.info.image.width != w as u32
            || target.info.image.height != h as u32
    }
    pub fn needs_polling(&self) -> bool {
        if self.active() || self.chain_closing {
            return true;
        }
        let Some(status) = self.chain_status else {
            return self.pending();
        };
        let mut fronts = 0;
        for (i, v) in frames(status).into_iter().enumerate() {
            if self.chain_frames[i] == 0 {
                continue;
            }
            if !self.reported[i]
                || v.phase != 4
                || v.result != 1
                || v.path != g::present_path_direct
            {
                return true;
            }
            fronts += 1;
        }
        fronts > 1
    }
    pub fn capture_blocked(&self, now: u64) -> bool {
        if self.active() {
            return true;
        }
        if self.chain.slot == 0 && self.readback_pin.slot != 0 {
            return true;
        }
        if self.chain.slot == 0 || self.acquired.is_some() {
            return false;
        }
        let Some(status) = self.chain_status else {
            return false;
        };
        if status.life >= 2 {
            return false;
        }
        if status.life == 1 || now < status.next_start_ns {
            return true;
        }
        !frames(status)
            .iter()
            .enumerate()
            .any(|(i, v)| i < status.count as usize && v.phase == 0)
    }
    pub fn capture_wait_ticks(&self, now: u64, hz: u32, limit: u64) -> u64 {
        if self.active() || self.fault.is_some() || self.chain.slot == 0 || self.acquired.is_some()
        {
            return limit;
        }
        let Some(status) = self.chain_status else {
            return limit;
        };
        if status.life != 0 {
            return limit;
        }
        if !frames(status)
            .iter()
            .enumerate()
            .any(|(i, v)| i < status.count as usize && v.phase == 0)
        {
            return limit;
        }
        r4os_sdk::time::duration_to_ticks(
            a::R4Duration {
                nanoseconds: status.next_start_ns.saturating_sub(now),
            },
            hz,
        )
        .map_or(limit, |ticks| ticks.max(1).min(limit))
    }
    pub fn poll_presentation(&mut self, s: &Session<'_, '_>) -> Result<()> {
        if self.chain.slot == 0 {
            return Ok(());
        }
        if self.chain_closing {
            return match self.close_chain(s) {
                Err(Error::Busy) => Ok(()),
                value => value,
            };
        }
        let mut status = g::R4GfxSwapchainStatus::default();
        accepted(unsafe { s.client.swapchain_poll(s.raw(), &self.chain, &mut status) })?;
        self.chain_status = Some(status);
        if status.life >= 2 {
            return Err(Error::Stale);
        }
        for (i, value) in frames(status).into_iter().enumerate() {
            if value.phase != 4
                || value.held_flags != 0
                || !self.reported[i]
                || same_resource(value.frame.image, self.readback_pin)
            {
                continue;
            }
            let rc = unsafe {
                s.client
                    .swapchain_release(s.raw(), &self.chain, &value.frame)
            };
            if rc == g::status_busy {
                continue;
            }
            accepted(rc)?;
            self.chain_frames[i] = 0;
            self.reported[i] = false;
            let status = self.chain_status.as_mut().unwrap();
            match i {
                0 => status.frame0.phase = 0,
                1 => status.frame1.phase = 0,
                2 => status.frame2.phase = 0,
                _ => unreachable!(),
            }
        }
        Ok(())
    }
    pub fn completion(&mut self) -> Option<Completion> {
        let status = self.chain_status?;
        for (i, value) in frames(status).into_iter().enumerate() {
            if value.phase == 4 && self.chain_frames[i] != 0 && !self.reported[i] {
                self.reported[i] = true;
                return Some(Completion {
                    frame: self.chain_frames[i],
                    status: value,
                });
            }
        }
        None
    }
    /// # Safety
    /// Capture pixels/assets and admitted external window read loans stay
    /// stable and resident through completion/discard, including a refused
    /// temporary CPU-source release. A stop or dropped Engine is no ACK.
    pub unsafe fn begin(
        &mut self,
        s: &Session<'_, '_>,
        cache: &Cache,
        recording: Option<&Frame>,
        deadline: u64,
    ) -> Result<()> {
        if self.active()
            || !self.jobs.drained()
            || (self.chain.slot == 0 && self.readback_pin.slot != 0)
        {
            return Err(Error::Busy);
        }
        if !cache.captured()
            || cache.commands().is_empty()
            || self.resources.outputs[self.output_index].resource.slot == 0
            || self.resources.staging.resource.slot == 0
        {
            return Err(Error::State);
        }
        if recording.is_some_and(|r| r.commands().len() > self.snapshot.capacity()) {
            return Err(Error::State);
        }
        if self.needs_full(cache.screen().w, cache.screen().h)
            && cache.commands()[0].scissor != cache.screen()
        {
            return Err(Error::Incomplete);
        }
        self.acquire(s, self.input_ns)?;
        if self.hdr_frame != self.has_hdr(cache) {
            return Err(Error::State);
        }
        self.clear_working = self.needs_full(cache.screen().w, cache.screen().h);
        self.encode_area = cache
            .commands()
            .iter()
            .fold(Rect::ZERO, |area, c| area.merged(c.scissor));
        if let Some(damage) = self.capture_damage {
            if cache.commands()[0].scissor != cache.screen() {
                return Err(Error::Incomplete);
            }
            for old in &mut self.target_damage {
                *old = Some(old.map_or(damage, |v| v.merged(damage)));
            }
            if self.resources.outputs[self.output_index].generation != 0 {
                self.encode_area = self.target_damage[self.output_index].unwrap();
            }
            self.clear_working = true;
        } else {
            self.target_damage = [Some(cache.screen()); 3];
        }
        let first = cache.commands()[0];
        self.replace_background = self.clear_working
            && !self.hdr_frame
            && !cache.entries()[first.entry as usize].external
            && intersect(first.scissor, self.encode_area) == Some(self.encode_area);
        if let Some(acquired) = self.acquired {
            self.chain_frames[acquired.slot as usize - 1] = cache.frame();
            self.reported[acquired.slot as usize - 1] = false;
        }
        self.frame = cache.frame();
        self.deadline = deadline;
        self.phase = if recording.is_some() {
            Phase::AssetUpload
        } else {
            Phase::Upload
        };
        self.next_image = 0;
        self.next_command = 0;
        self.next_asset = 0;
        self.asset_x = 0;
        self.asset_y = 0;
        self.next_primitive = 0;
        self.fault = None;
        self.present_fence = None;
        self.jobs.last = None;
        self.jobs.stage = None;
        self.jobs.depth = 0;
        self.plan_reuse(cache, recording);
        Ok(())
    }
    pub fn cancel(&mut self, s: &Session<'_, '_>, reason: Error) {
        if self.fault.is_none() {
            self.fault = Some(reason);
        }
        self.phase = Phase::Drain;
        self.invalidate();
        if let Some(value) = self.acquired {
            if unsafe { s.client.swapchain_release(s.raw(), &self.chain, &value) } == g::status_ok {
                self.chain_frames[value.slot as usize - 1] = 0;
                self.acquired = None;
            }
        }
        self.jobs.cancel(s);
        if self.chain.slot != 0 {
            self.chain_closing = true;
            let _ = self.close_chain(s);
        }
    }
    pub fn advance(
        &mut self,
        s: &Session<'_, '_>,
        cache: &mut Cache,
        mut recording: Option<&mut Frame>,
        mut clock: impl FnMut() -> u64,
    ) -> Progress {
        // This turn owns its budget, including its initial completion scan.
        // Earlier topology/input/service work cannot consume it.
        let entered = clock();
        self.immediate_work = false;
        self.last_admission_steps = 0;
        if let Err(error) = self.poll_presentation(s) {
            if self.active() {
                self.cancel(s, error);
            }
        }
        if !self.active() {
            return if self.fault.is_none() {
                Progress::Copied
            } else {
                Progress::Failed
            };
        }
        if cache.frame() != self.frame || !cache.captured() {
            self.cancel(s, Error::State);
        }
        if entered >= self.deadline && self.fault.is_none() {
            self.cancel(s, Error::Deadline);
        }
        if let Err(error) = self.collect(s, cache, recording.as_deref_mut()) {
            self.cancel(s, error);
        }
        if self.phase == Phase::Drain {
            if self.release_stage_source(s).is_err() {
                return Progress::Pending;
            }
            if !self.jobs.physically_retired() {
                return Progress::Pending;
            }
            if let Err(error) = self.jobs.prepare_external(s) {
                if error != Error::Busy {
                    self.cancel(s, error);
                }
                return Progress::Pending;
            }
            // Coordinator ends this Session and performs finish_external;
            // the next native advance observes actual receipt transfer.
            if self.sources.iter().any(|source| source.borrowed) || !self.jobs.drained() {
                return Progress::Pending;
            }
            if self.fault.is_some() {
                if let Some(value) = self.acquired {
                    let rc = unsafe { s.client.swapchain_release(s.raw(), &self.chain, &value) };
                    if rc == g::status_busy {
                        return Progress::Pending;
                    }
                    self.acquired = None;
                    self.chain_frames[value.slot as usize - 1] = 0;
                }
            }
            self.phase = Phase::Idle;
            if self.fault.is_none() {
                self.resources.outputs[self.output_index].generation = self.frame;
                self.target_damage[self.output_index] = None;
                if recording.is_some() {
                    for (i, entry) in cache.entries().iter().enumerate() {
                        if entry.frame == cache.frame() {
                            self.resources.layers[i].generation = entry.generation;
                        }
                    }
                }
                self.remember(cache, recording.as_deref());
            }
            return if self.fault.is_none() {
                Progress::Copied
            } else {
                Progress::Failed
            };
        }
        while self.last_admission_steps < Self::ADMISSION_LIMIT {
            let instant = clock();
            if instant >= self.deadline {
                self.cancel(s, Error::Deadline);
                break;
            }
            if self.last_admission_steps != 0
                && instant.saturating_sub(entered) >= Self::ADMISSION_NS
            {
                self.immediate_work = true;
                break;
            }
            self.last_admission_steps += 1;
            if self.jobs.depth >= self.jobs.capacity() {
                let before = self.jobs.retirement_epoch;
                if let Err(error) = self.collect(s, cache, recording.as_deref_mut()) {
                    self.cancel(s, error);
                    return Progress::Pending;
                }
                if self.fault.is_some() || before == self.jobs.retirement_epoch {
                    return Progress::Pending;
                }
                continue;
            }
            if let Err(error) = self.step(s, cache, recording.as_deref()) {
                if error != Error::Busy {
                    self.cancel(s, error);
                    return Progress::Pending;
                }
                let before = self.jobs.retirement_epoch;
                if let Err(error) = self.collect(s, cache, recording.as_deref_mut()) {
                    self.cancel(s, error);
                    return Progress::Pending;
                }
                if self.fault.is_some() || before == self.jobs.retirement_epoch {
                    return Progress::Pending;
                }
                continue;
            }
            if self.phase == Phase::Drain {
                break;
            }
        }
        if self.last_admission_steps == Self::ADMISSION_LIMIT && self.phase != Phase::Drain {
            self.immediate_work = true;
        }
        Progress::Pending
    }
    fn collect(
        &mut self,
        s: &Session<'_, '_>,
        cache: &mut Cache,
        mut recording: Option<&mut Frame>,
    ) -> Result<()> {
        let resources = &mut self.resources;
        let failed = self.jobs.collect(s, |event| {
            if let Some(i) = event.layer {
                resources.layers[i].generation = event.generation;
                cache.uploaded(i, event.generation);
            }
            if let Some(i) = event.asset {
                resources.assets[i].generation = event.generation;
                if let Some(frame) = recording.as_deref_mut() {
                    frame.assets.uploaded(i, event.generation);
                }
            }
        })?;
        if failed && self.fault.is_none() {
            self.cancel(s, Error::Graphics);
        }
        Ok(())
    }
    pub fn finish_external(&mut self, finish: &mut impl FinishExternal, failed: bool) -> bool {
        if !self.jobs.physically_retired() {
            return false;
        }
        for i in 0..self.sources.len() {
            if !self.sources[i].borrowed {
                continue;
            }
            let receipt = match self.jobs.take_external(i) {
                Ok(value) => value,
                Err(_) => return false,
            };
            match finish.finish(i, receipt, failed) {
                Ok(()) => {
                    if self.jobs.commit_external(i).is_err() {
                        return false;
                    }
                    self.sources[i].borrowed = false;
                }
                Err(receipt) => {
                    let _ = self.jobs.restore_external(i, receipt);
                    return false;
                }
            }
        }
        true
    }
    pub fn discard_external(&mut self, finish: &mut impl FinishExternal) -> bool {
        if self.active() || !self.jobs.drained() {
            return false;
        }
        self.finish_external(finish, true)
    }
    pub fn close(&mut self, s: &Session<'_, '_>) -> Result<()> {
        if self.active()
            || !self.jobs.drained()
            || self.readback_pin.slot != 0
            || s.external_receipts() != 0
            || self.sources.iter().any(|source| source.borrowed)
        {
            return Err(Error::Busy);
        }
        self.release_stage_source(s)?;
        self.close_chain(s)?;
        self.resources.close(s)?;
        self.snapshot = Vec::new();
        Ok(())
    }
    fn close_chain(&mut self, s: &Session<'_, '_>) -> Result<()> {
        if self.readback_pin.slot != 0 {
            return Err(Error::Busy);
        }
        if self.chain.slot == 0 {
            return Ok(());
        }
        accepted(unsafe { s.client.swapchain_close(s.raw(), &self.chain) })?;
        self.chain = Default::default();
        self.presentation = None;
        self.chain_status = None;
        self.acquired = None;
        self.chain_frames = [0; 3];
        self.reported = [false; 3];
        self.output_index = 0;
        self.chain_closing = false;
        self.invalidate();
        Ok(())
    }
}
fn same_resource(x: g::R4GfxResource, y: g::R4GfxResource) -> bool {
    x.slot == y.slot
        && x.kind == y.kind
        && x.generation == y.generation
        && x.device_generation == y.device_generation
        && x.device_address == y.device_address
}
fn frames(status: g::R4GfxSwapchainStatus) -> [g::R4GfxSwapchainFrameStatus; 3] {
    [status.frame0, status.frame1, status.frame2]
}
fn rect(r: Rect) -> g::R4GfxSignedRect {
    g::R4GfxSignedRect {
        x: r.x,
        y: r.y,
        width: r.w as u32,
        height: r.h as u32,
    }
}
#[cfg(test)]
#[path = "../../../../Tests/RustNative/gpu_engine.rs"]
pub(crate) mod tests;
