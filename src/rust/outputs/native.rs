//! One native composition owner per output. Only cold preparation moves to a
//! joinable task; normal frames advance in bounded steps on the main task.
use crate::{
    capture::remote::{Capture as RemoteCapture, Cursor as CaptureCursor, Image as CaptureImage},
    hosted::{native_images::Reads, replay::Shapes, window_image::Memory, window_set::Set},
    render::{
        compositor::{Scene, Stats},
        geometry::Rect,
        gpu::{
            self,
            device::{Device, Error as DeviceError},
            engine::{Engine, Progress as GpuProgress},
            receipt::Consumer,
        },
        layers::{Cache, Mode},
        native_scene,
        primitive_assets::Error as CaptureError,
        primitive_frame::Frame,
        text::cache::{Cache as FontCache, Fonts},
    },
    runtime::{
        owned::Owned,
        worker::{Control, Task, Worker},
    },
    shell::{desktop_view, taskbar_view},
};
use alloc::sync::Arc;
use core::mem;
use r4gfx as g;
use r4os_sdk::{
    App, StartupInfo, abi as a, c_facades as c, graphics::Graphics, resources::Resources,
    time::Clock,
};
#[path = "native_recording.rs"]
mod recording;
use recording::Recording;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Idle,
    Pending,
    Visible,
    Discarded,
    Failed,
}
#[derive(Debug)]
pub enum Error {
    Memory,
    Device(DeviceError),
    Gpu(gpu::Error),
}
pub struct OpenError {
    pub error: Error,
    pub retained: Option<Device>,
}
struct State {
    device: Device,
    engine: Engine,
    cache: Cache,
    frame: Frame,
    reads: Reads,
    capture: RemoteCapture,
    capture_damage: Option<Rect>,
    capture_cursor: CaptureCursor,
}
impl State {
    fn capture_view(&self) -> crate::render::output_geometry::topology::Viewport {
        self.cache
            .view()
            .unwrap_or(crate::render::output_geometry::topology::Viewport {
                pixel_w: self.cache.screen().w as u32,
                pixel_h: self.cache.screen().h as u32,
                ..Default::default()
            })
    }
    fn record_capture(&mut self) {
        let view = self.capture_view();
        if let Ok(bounds) = crate::render::output_geometry::logical(view) {
            self.capture.record(
                self.engine.output_index,
                self.cache.frame(),
                self.capture_damage.unwrap_or(bounds),
                view,
                self.capture_cursor,
            );
        }
    }
}
struct Preparation {
    state: Owned<State>,
    raw: usize,
    expected: &'static StartupInfo<'static>,
    deadline: u64,
    error: Option<gpu::Error>,
    prepare_capture: bool,
}
impl Task for Preparation {
    fn run(&mut self, control: &Control) -> i32 {
        let app = match unsafe {
            App::from_raw(
                self.raw as *const a::R4XStartContext,
                self.expected,
                c"R4DESK",
            )
        } {
            Ok(value) => value,
            Err(_) => {
                self.error = Some(gpu::Error::State);
                return -1;
            }
        };
        let state = &mut *self.state;
        loop {
            let view = state.capture_view();
            let time = || now(&app).unwrap_or(self.deadline);
            if control.stopped() || time() >= self.deadline {
                self.error = Some(if control.stopped() {
                    gpu::Error::State
                } else {
                    gpu::Error::Deadline
                });
                return -1;
            }
            let result = match state.device.bind(&app) {
                Ok(s) => {
                    let _ = state.capture.poll(&s, time());
                    match state.capture.source() {
                        Ok(source) => state.engine.readback_pin = source,
                        Err(_) => {
                            self.error = Some(gpu::Error::Graphics);
                            return -1;
                        }
                    }
                    let prepared = state.engine.prepare(
                        &s,
                        &state.cache,
                        Some(&state.frame),
                        self.deadline,
                        time,
                        || control.stopped(),
                    );
                    if prepared.is_ok()
                        && self.prepare_capture
                        && !control.stopped()
                        && time() < self.deadline
                    {
                        if unsafe {
                            state.capture.prepare(
                                &app,
                                &s,
                                view,
                                state.engine.output_format,
                                state.engine.output_color,
                            )
                        }
                        .is_err()
                        {
                            state.capture.retry(time());
                        }
                    }
                    prepared
                }
                Err(_) => Err(gpu::Error::Graphics),
            };
            match result {
                Ok(()) if !control.stopped() && time() < self.deadline => return 0,
                Ok(()) => {
                    self.error = Some(if control.stopped() {
                        gpu::Error::State
                    } else {
                        gpu::Error::Deadline
                    });
                    return -1;
                }
                Err(gpu::Error::Busy) if time() < self.deadline => app.sleep(1),
                Err(error) => {
                    self.error = Some(error);
                    return -1;
                }
            }
        }
    }
}
fn now(app: &App<'_>) -> Option<u64> {
    Clock::new(app)
        .precise_instant()
        .ok()
        .map(|v| v.nanoseconds)
}
pub struct Output<'a> {
    state: Option<Owned<State>>,
    preparation: Option<Worker<'a, Preparation>>,
    seed: Option<Owned<Recording>>,
    newest: Option<Owned<Recording>>,
    latest_input_ns: u64,
    latest_time: u64,
    capture_error: Option<CaptureError>,
    failure_stage: &'static str,
    latest_cursor: CaptureCursor,
    pub scenes_prepared_while_busy: u64,
    pub scenes_coalesced: u64,
    pub scenes_prepared_with_gpu_jobs: u64,
    pub scenes_prepared_during_worker: u64,
    deadline: u64,
    revision: u64,
    failed_revision: Option<u64>,
    awaiting_visible: bool,
    failure_reported: bool,
    device_generation: u64,
    reset_generation: u64,
    pub gpu_operations: u32,
    pub prepared_threads: u64,
    pub frames_visible: u64,
    pub frames_completed: u64,
    pub completed_frame: u64,
    pub completed_status: Option<g::R4GfxSwapchainFrameStatus>,
    capture_demand: bool,
    capture_reset: bool,
    receipts: Arc<crate::render::gpu::receipt::Loans>,
    device_address: u64,
    device_handle_generation: u64,
    captured_frame: u64,
}
#[derive(Clone, Copy)]
pub struct ConsumerState {
    pub device_generation: u64,
    pub reset_generation: u64,
    pub operations: u32,
    pub ready: bool,
    pub receipts: usize,
}
impl<'a> Output<'a> {
    /// # Safety
    /// The actual loader/provider tables remain resident through successful
    /// close, including all cold tasks and broker consumer receipts.
    pub unsafe fn open(
        app: &App<'_>,
        adapter: u32,
        head: Option<u32>,
        encoding: Option<a::GfxOutputColorState>,
        format: u32,
    ) -> Result<Self, OpenError> {
        let plain = |error| OpenError {
            error,
            retained: None,
        };
        if encoding.is_some_and(|v| v.identity.adapter_id != adapter) {
            return Err(plain(Error::Gpu(gpu::Error::Stale)));
        }
        let cache = Cache::new(128 * 1024 * 1024).map_err(|_| plain(Error::Memory))?;
        let mut frame = Frame::new().map_err(|_| plain(Error::Memory))?;
        frame.set_mirror(false).map_err(|_| plain(Error::Memory))?;
        let mut engine = Engine::new().map_err(Error::Gpu).map_err(plain)?;
        engine.head = head;
        let mut device = unsafe { Device::open(app, adapter) }.map_err(|error| OpenError {
            error: Error::Device(error.error),
            retained: error.retained,
        })?;
        let configure = match device.bind(app) {
            Ok(s) => engine.configure_output(&s, encoding, format),
            Err(_) => Err(gpu::Error::Graphics),
        };
        if let Err(error) = configure {
            let closed = device.close(app).unwrap_or(false);
            return Err(OpenError {
                error: Error::Gpu(error),
                retained: (!closed).then_some(device),
            });
        }
        let receipts = device.receipt_counts();
        let device_address = device.identity().map_or(0, |handle| handle.address);
        let device_handle_generation = device.identity().map_or(0, |handle| handle.generation);
        let state = State {
            device,
            engine,
            cache,
            frame,
            reads: Reads::default(),
            capture: RemoteCapture::default(),
            capture_damage: None,
            capture_cursor: CaptureCursor::default(),
        };
        let state = match Owned::new(state) {
            Ok(value) => value,
            Err(mut state) => {
                let closed = state.device.close(app).unwrap_or(false);
                return Err(OpenError {
                    error: Error::Memory,
                    retained: (!closed).then_some(state.device),
                });
            }
        };
        Ok(Self {
            state: Some(state),
            preparation: None,
            seed: None,
            newest: None,
            latest_input_ns: 0,
            latest_time: 0,
            capture_error: None,
            failure_stage: "none",
            latest_cursor: CaptureCursor::default(),
            scenes_prepared_while_busy: 0,
            scenes_coalesced: 0,
            scenes_prepared_with_gpu_jobs: 0,
            scenes_prepared_during_worker: 0,
            deadline: 0,
            revision: 0,
            failed_revision: None,
            awaiting_visible: false,
            failure_reported: false,
            device_generation: 0,
            reset_generation: 0,
            gpu_operations: 0,
            prepared_threads: 0,
            frames_visible: 0,
            frames_completed: 0,
            completed_frame: 0,
            completed_status: None,
            capture_demand: false,
            capture_reset: false,
            receipts,
            device_address,
            device_handle_generation,
            captured_frame: 0,
        })
    }
    pub fn consumer_state(&self, time: u64) -> Option<ConsumerState> {
        if self.failed_revision.is_some()
            || self.device_generation == 0
            || self.reset_generation == 0
        {
            return None;
        }
        Some(ConsumerState {
            device_generation: self.device_generation,
            reset_generation: self.reset_generation,
            operations: self.gpu_operations,
            ready: !self.blocks_capture(time),
            receipts: self.receipts.external(),
        })
    }
    pub fn external_receipts(&self) -> usize {
        self.receipts.external()
    }
    pub fn capture_state(&self) -> Option<super::selection::CaptureState> {
        let state = self.state.as_ref()?;
        let newest = self.newest.as_deref();
        Some(super::selection::CaptureState {
            frame: state.cache.frame(),
            reused_captures: state.cache.reused_captures,
            atlas_reused_layers: state.engine.atlas_reused_layers,
            asset_misses: state.frame.assets.misses,
            captured_reads: state
                .reads
                .captured
                .saturating_add(newest.map_or(0, |s| s.reads.captured)),
            completed_reads: state
                .reads
                .completed
                .saturating_add(newest.map_or(0, |s| s.reads.completed)),
            failed_reads: state
                .reads
                .failed
                .saturating_add(newest.map_or(0, |s| s.reads.failed)),
            reads_empty: state.reads.empty() && newest.is_none_or(|s| s.reads.empty()),
            external_receipts: self.receipts.external(),
            prepared_while_busy: self.scenes_prepared_while_busy,
            coalesced_scenes: self.scenes_coalesced,
            prepared_with_gpu_jobs: self.scenes_prepared_with_gpu_jobs,
            prepared_during_worker: self.scenes_prepared_during_worker,
            pending_scene: newest.is_some_and(|s| s.captured),
        })
    }
    pub fn execution_observation(&self) -> super::selection::Execution {
        let time = |value: u64| (value != 0).then_some(value);
        super::selection::Execution {
            device_address: self.device_address,
            device_handle_generation: self.device_handle_generation,
            backend_device_generation: self.device_generation,
            reset_generation: self.reset_generation,
            completed_frames: self.frames_completed,
            visible_frames: self.frames_visible,
            render_submit_jobs: self.state.as_ref().map(|s| s.engine.render_jobs),
            primitive_submit_jobs: self.state.as_ref().map(|s| s.engine.primitive_jobs),
            completed_frame: self.completed_frame,
            result: self.completed_status.map(|s| s.result),
            phase: self.completed_status.map(|s| s.phase),
            frame_generation: self.completed_status.map(|s| s.frame.generation),
            frame_serial: self.completed_status.map(|s| s.frame.serial),
            render_end_ns: self.completed_status.and_then(|s| time(s.render_end_ns)),
            copied_ns: self.completed_status.and_then(|s| time(s.copied_ns)),
            visible_ns: self.completed_status.and_then(|s| time(s.visible_ns)),
        }
    }
    pub fn failure_observation(
        &self,
        target: a::GfxOutputTarget,
    ) -> super::selection::NativeFailure {
        let state = self.state.as_deref();
        let latest = self.newest.as_deref();
        super::selection::NativeFailure {
            target: target.into(),
            stage: self.failure_stage,
            gpu_error: state.and_then(|s| s.engine.fault),
            capture_error: self.capture_error,
            frame: state.map(|s| s.cache.frame()),
            latest_frame: latest.map(|s| s.cache.frame()),
            worker_pending: self.preparation.is_some(),
            awaiting_visible: self.awaiting_visible,
            latest_captured: latest.is_some_and(|s| s.captured),
            old_reads_empty: state.map(|s| s.reads.empty()),
            latest_reads_empty: latest.is_none_or(|s| s.reads.empty()),
            gpu_physically_retired: state.map(|s| s.engine.jobs.physically_retired()),
        }
    }
    pub fn clean_frame(
        &self,
        target: a::GfxOutputTarget,
    ) -> Option<crate::outputs::cursor::CleanFrame> {
        (self.captured_frame != 0).then_some(crate::outputs::cursor::CleanFrame {
            target,
            device_address: self.device_address,
            device_generation: self.device_handle_generation,
            frame: self.captured_frame,
        })
    }
    pub fn busy(&self) -> bool {
        self.newest
            .as_ref()
            .is_some_and(|s| s.captured || !s.reads.empty())
            || self.preparation.is_some()
            || self.awaiting_visible
            || self
                .state
                .as_ref()
                .is_some_and(|s| s.engine.active() || s.engine.pending())
    }
    pub fn admission(&mut self, app: &App<'_>, additional: u32) -> super::selection::Decision {
        use super::selection::{Decision, Reason};
        let Some(state) = self.state.as_deref_mut() else {
            return Decision::new(Reason::NativeState);
        };
        let session = match state.device.bind(app) {
            Ok(value) => value,
            Err(error) => return Decision::device(Reason::NativeBinding, error),
        };
        let info = match session.refresh() {
            Ok(value) => value,
            Err(error) => return Decision::device(Reason::NativeRefresh, error),
        };
        let required = state.engine.required_operations() | additional;
        Decision::operations(required, info.gpu_operations)
    }
    pub fn needs_polling(&self) -> bool {
        self.newest
            .as_ref()
            .is_some_and(|s| s.captured || !s.reads.empty())
            || self.preparation.is_some()
            || self.awaiting_visible
            || self.state.as_ref().is_some_and(|s| {
                s.engine.needs_polling() || !s.reads.empty() || s.capture.needs_polling()
            })
    }
    pub fn immediate_work(&self) -> bool {
        self.state.as_ref().is_some_and(|s| {
            s.engine.active() && s.engine.fault.is_none() && s.engine.immediate_work
        })
    }
    pub fn blocks_capture(&self, time: u64) -> bool {
        if !self.failure_reported
            && (self.newest.is_some()
                || self.seed.is_some()
                || self
                    .state
                    .as_ref()
                    .is_some_and(|s| s.cache.captured() && s.engine.fault.is_none()))
        {
            return false;
        }
        !self.failure_reported
            && (self.preparation.is_some()
                || self.awaiting_visible
                || self
                    .state
                    .as_ref()
                    .is_none_or(|s| s.engine.capture_blocked(time)))
    }
    /// The event loop gives newly dirty input precedence over this one draft.
    /// Only physical retirement and visibility allow promotion to a transaction.
    pub fn pending_ready(&self, time: u64) -> bool {
        self.failed_revision.is_none()
            && self.preparation.is_none()
            && !self.awaiting_visible
            && self.newest.as_ref().is_some_and(|s| s.captured)
            && self.state.as_ref().is_some_and(|s| {
                s.reads.empty() && s.engine.fault.is_none() && !s.engine.capture_blocked(time)
            })
    }
    pub fn capture_wait_ticks(&self, time: u64, hz: u32, limit: u64) -> u64 {
        self.state
            .as_ref()
            .map_or(limit, |s| s.engine.capture_wait_ticks(time, hz, limit))
    }
    pub fn available(&mut self, app: &App<'_>, revision: u64, time: u64, input_ns: u64) -> bool {
        if self.blocks_capture(time) || self.failed_revision == Some(revision) {
            return false;
        }
        self.revision = revision;
        self.latest_input_ns = input_ns;
        self.latest_time = time;
        // The worker owns the old device, not the independent CPU recording.
        // Actual device/reset/capability admission is repeated before submit.
        if self.preparation.is_some() {
            return self.seed.is_some() || self.newest.is_some();
        }
        let Some(state) = self.state.as_deref_mut() else {
            return false;
        };
        // CPU recording uses the last admitted generation and never calls the
        // device. Fresh capability/reset/image admission belongs to promotion.
        if self.newest.is_some() || self.awaiting_visible || state.engine.capture_blocked(time) {
            return self.seed.is_some() || self.newest.is_some() || state.cache.captured();
        }
        let Ok(session) = state.device.bind(app) else {
            return false;
        };
        let Ok(info) = session.refresh() else {
            if state.engine.head.is_some() {
                self.failed_revision = Some(revision);
            }
            return false;
        };
        self.gpu_operations = info.gpu_operations;
        let required = state.engine.required_operations();
        if info.gpu_operations & required != required {
            if state.engine.head.is_some() {
                self.failed_revision = Some(revision);
            }
            return false;
        }
        if (self.device_generation, self.reset_generation)
            != (info.device_generation, info.reset_generation)
        {
            state.capture.invalidate();
            state.engine.invalidate();
            self.device_generation = info.device_generation;
            self.reset_generation = info.reset_generation;
            if self
                .newest
                .as_ref()
                .is_some_and(|s| s.generation != (self.device_generation, self.reset_generation))
            {
                self.failed_revision = Some(revision);
                return false;
            }
            self.seed = None;
        }
        state.engine.input_ns = input_ns;
        match state.engine.acquire(&session, input_ns) {
            Ok(()) => true,
            Err(gpu::Error::Busy) => false,
            Err(error) => {
                state.engine.cancel(&session, error);
                self.failed_revision = Some(revision);
                false
            }
        }
    }
    /// # Safety
    /// Hosted GUI maps remain immutable throughout the scene walk. All window
    /// owners and their BO imports stay resident through finish/close.
    pub unsafe fn capture<M: Memory>(
        &mut self,
        windows: &mut Set<M, Consumer>,
        mode: Mode,
        scene: &Scene<'_>,
        damage: &[Rect],
        fonts_cache: &mut FontCache,
        fonts: &mut impl Fonts,
        icons: &mut (impl desktop_view::Icons + taskbar_view::Icons),
        shapes: &mut impl Shapes,
    ) -> Result<Stats, CaptureError> {
        self.capture_error = None;
        self.failure_stage = "capture";
        let older_busy = self.preparation.is_some()
            || self.awaiting_visible
            || self
                .state
                .as_ref()
                .is_none_or(|s| s.engine.capture_blocked(self.latest_time) || !s.reads.empty());
        let unretired_gpu_jobs = self
            .state
            .as_ref()
            .is_some_and(|s| s.engine.active() && !s.engine.jobs.physically_retired());
        if older_busy || self.newest.is_some() {
            if self.newest.is_none() {
                self.newest = self.seed.take();
                if self.newest.is_none() {
                    if let Some(state) = self.state.as_deref() {
                        self.newest = Owned::new_with(|| {
                            Recording::seed(
                                &state.cache,
                                &state.frame,
                                (self.device_generation, self.reset_generation),
                            )
                        })
                        .ok();
                    }
                }
            }
            let newest = self.newest.as_mut().ok_or(CaptureError::State)?;
            let replacing = newest.captured;
            self.failure_stage = "capture-newest";
            let result = unsafe {
                newest.capture(
                    windows,
                    mode,
                    scene,
                    damage,
                    fonts_cache,
                    fonts,
                    icons,
                    shapes,
                    self.latest_input_ns,
                    self.latest_cursor,
                )
            };
            self.capture_error = result.as_ref().err().copied();
            if result.is_ok() {
                if unretired_gpu_jobs {
                    self.scenes_prepared_with_gpu_jobs =
                        self.scenes_prepared_with_gpu_jobs.saturating_add(1);
                }
                if self.preparation.is_some() {
                    self.scenes_prepared_during_worker =
                        self.scenes_prepared_during_worker.saturating_add(1);
                }
                if older_busy {
                    self.scenes_prepared_while_busy =
                        self.scenes_prepared_while_busy.saturating_add(1);
                }
                if replacing {
                    self.scenes_coalesced = self.scenes_coalesced.saturating_add(1);
                }
            }
            return result;
        }
        // A retained visible front still makes lifetime teardown busy. It
        // does not block capture into the distinct back buffer admitted by
        // available(). The scene owner separately checks active GPU work and
        // exact outstanding source reads before changing its capture.
        if self.preparation.is_some() || self.awaiting_visible {
            return Err(CaptureError::State);
        }
        // Nothing still samples these CPU pages; avoid COW on the hot idle path.
        self.seed = None;
        let state = self.state.as_deref_mut().ok_or(CaptureError::State)?;
        // The original single-display caller reconstructs a new/rotating
        // target before capture. Keep that rule with the native target owner;
        // a one-pixel invalidation cannot initialize the rest of its image.
        let full = match mode {
            Mode::Desktop(bounds) => Some(bounds),
            _ => None,
        };
        let damage = if full.is_some_and(|bounds| state.engine.needs_full(bounds.w, bounds.h)) {
            core::slice::from_ref(full.as_ref().unwrap())
        } else {
            damage
        };
        let result = unsafe {
            native_scene::capture(
                &mut state.cache,
                &mut state.frame,
                &mut state.engine,
                &mut state.reads,
                windows,
                mode,
                scene,
                damage,
                fonts_cache,
                fonts,
                icons,
                shapes,
            )
        };
        self.capture_error = result.as_ref().err().copied();
        if result.is_ok() {
            state.capture_damage = damage.iter().copied().reduce(Rect::merged);
            self.captured_frame = state.cache.frame();
        }
        result
    }
    /// # Safety
    /// Drain this output, its exact thread and consumer receipts before return
    /// from the module entry. Only immutable owned capture moves to the task.
    pub unsafe fn start(
        &mut self,
        app: &App<'_>,
        resources: &'a Resources<'a>,
        expected: &'static StartupInfo<'static>,
        time: u64,
    ) -> bool {
        if self.preparation.is_some() || self.awaiting_visible {
            return self.newest.as_ref().is_some_and(|s| s.captured);
        }
        if self.state.as_ref().is_some_and(|s| s.engine.active()) {
            return self.newest.as_ref().is_some_and(|s| s.captured);
        }
        if self.newest.as_ref().is_some_and(|s| s.captured) {
            if !self.pending_ready(time) {
                return true;
            }
            let state = match self.state.as_deref_mut() {
                Some(s) => s,
                None => return false,
            };
            let newest = self.newest.as_mut().unwrap();
            {
                let Ok(session) = state.device.bind(app) else {
                    return true;
                };
                let Ok(info) = session.refresh() else {
                    return true;
                };
                let required = state.engine.required_operations();
                if newest.generation != (info.device_generation, info.reset_generation)
                    || info.gpu_operations & required != required
                {
                    self.failure_stage = "promote-admission";
                    self.failed_revision = Some(self.revision);
                    return false;
                }
                match state.engine.acquire(&session, newest.input_ns) {
                    Ok(()) => (),
                    Err(gpu::Error::Busy) => return true,
                    Err(error) => {
                        state.engine.cancel(&session, error);
                        self.failed_revision = Some(self.revision);
                        return false;
                    }
                }
            }
            let damage = newest.damage;
            self.failure_stage = "promote-sources";
            if state
                .engine
                .adopt_recording(&mut newest.recorder, damage)
                .is_err()
            {
                return false;
            }
            core::mem::swap(&mut state.cache, &mut newest.cache);
            core::mem::swap(&mut state.frame, &mut newest.frame);
            newest.reads.captured = newest.reads.captured.saturating_add(state.reads.captured);
            newest.reads.completed = newest.reads.completed.saturating_add(state.reads.completed);
            newest.reads.failed = newest.reads.failed.saturating_add(state.reads.failed);
            core::mem::swap(&mut state.reads, &mut newest.reads);
            state.capture_damage = newest.damage;
            state.capture_cursor = newest.cursor;
            state.engine.input_ns = newest.input_ns;
            self.captured_frame = state.cache.frame();
            self.newest = None;
        }
        let Some(state) = self.state.as_deref_mut() else {
            return false;
        };
        if state.engine.active() {
            return false;
        }
        let Some(deadline) = time.checked_add(5_000_000_000) else {
            return false;
        };
        self.deadline = deadline;
        self.failure_reported = false;
        state.capture.demand(self.capture_demand);
        let prepare_capture = self.capture_demand && time >= state.capture.retry_ns;
        let view = state.capture_view();
        self.failure_stage = "start-device";
        let Ok(session) = state.device.bind(app) else {
            return false;
        };
        if state
            .engine
            .prepared(&session, &state.cache, Some(&state.frame))
            && (!prepare_capture
                || state.capture.prepared(
                    view,
                    state.engine.output_format,
                    state.engine.output_color,
                ))
        {
            return match unsafe {
                state
                    .engine
                    .begin(&session, &state.cache, Some(&state.frame), deadline)
            } {
                Ok(()) => {
                    drop(session);
                    state.record_capture();
                    true
                }
                Err(error) => {
                    self.failure_stage = "begin";
                    state.engine.cancel(&session, error);
                    self.failed_revision = Some(self.revision);
                    false
                }
            };
        }
        drop(session);
        // Only the cold worker removes State from the event loop. Preserve a
        // metadata/COW seed first; ordinary active frames fork lazily on input.
        self.seed = Owned::new_with(|| {
            Recording::seed(
                &state.cache,
                &state.frame,
                (self.device_generation, self.reset_generation),
            )
        })
        .ok();
        let task = Preparation {
            state: self.state.take().unwrap(),
            raw: app.start_context() as *const _ as usize,
            expected,
            deadline,
            error: None,
            prepare_capture,
        };
        match unsafe { Worker::start(resources, task, 256 * 1024) } {
            Ok(worker) => {
                self.preparation = Some(worker);
                self.prepared_threads = self.prepared_threads.saturating_add(1);
                true
            }
            Err(error) => {
                self.failure_stage = "create-preparation-worker";
                self.state = Some(error.task.state);
                self.failed_revision = Some(self.revision);
                false
            }
        }
    }
    fn collect(&mut self, app: &App<'_>, resume: bool) -> bool {
        let Some(worker) = self.preparation.as_mut() else {
            return true;
        };
        let stopping = worker.stopping();
        let Ok(Some((_, mut task))) = worker.collect() else {
            return false;
        };
        self.preparation = None;
        if !resume || stopping {
            self.state = Some(task.state);
            return true;
        }
        let mut begun = false;
        let state = &mut *task.state;
        if let Ok(session) = state.device.bind(app) {
            let error = task.error.or_else(|| {
                unsafe {
                    state
                        .engine
                        .begin(&session, &state.cache, Some(&state.frame), task.deadline)
                }
                .err()
            });
            if let Some(error) = error {
                self.failure_stage = if task.error.is_some() {
                    "prepare-worker"
                } else {
                    "begin-after-worker"
                };
                state.engine.cancel(&session, error);
                self.failed_revision = Some(self.revision);
            } else {
                begun = true;
            }
        } else {
            self.failed_revision = Some(self.revision);
        }
        if begun {
            state.record_capture();
        }
        self.state = Some(task.state);
        true
    }
    pub fn poll<M: Memory>(
        &mut self,
        app: &App<'_>,
        windows: &mut Set<M, Consumer>,
        graphics: &Graphics<'_, '_>,
        time: u64,
    ) -> Progress {
        if !self.collect(app, true) {
            return Progress::Pending;
        }
        let Some(state) = self.state.as_deref_mut() else {
            return Progress::Failed;
        };
        {
            let Ok(session) = state.device.bind(app) else {
                return Progress::Pending;
            };
            if self.capture_reset {
                state.capture.invalidate();
                self.capture_reset = false;
            }
            state.capture.demand(self.capture_demand);
            if state.capture.poll(&session, time).is_err() {
                state.capture.retry(time);
            }
            match state.capture.source() {
                Ok(source) => state.engine.readback_pin = source,
                Err(_) => return Progress::Pending,
            }
            if state.engine.active() {
                let copied = state.engine.advance(
                    &session,
                    &mut state.cache,
                    Some(&mut state.frame),
                    || now(app).unwrap_or(self.deadline),
                ) == GpuProgress::Copied;
                if copied && !state.engine.uses_swapchain() {
                    self.awaiting_visible = true;
                }
                if state.engine.fault.is_some() {
                    self.failure_stage = "advance";
                    self.failed_revision = Some(self.revision);
                }
            } else if let Err(error) = state.engine.poll_presentation(&session) {
                self.failure_stage = "poll-presentation";
                state.engine.cancel(&session, error);
                self.failed_revision = Some(self.revision);
            }
        }
        // An old broker receipt may call this same device's job_release.
        // No Session/device-call gate or exclusive App loan spans this call.
        let failed = state.engine.fault.is_some();
        let _ = state
            .engine
            .finish_external(&mut state.reads.completion(windows), failed);
        if let Some(done) = state.engine.completion() {
            self.completed_frame = done.frame;
            self.completed_status = Some(done.status);
            if done.status.result == 1 || done.status.result == 2 {
                if let Ok(session) = state.device.bind(app) {
                    state.capture.complete(
                        &session,
                        done.status.frame.slot as usize - 1,
                        done.status.frame.image,
                        time,
                    );
                    if let Ok(source) = state.capture.source() {
                        state.engine.readback_pin = source;
                    }
                }
            } else {
                state.capture.discarded(done.status.frame.slot as usize - 1);
            }
            match done.status.result {
                1 => {
                    self.frames_visible = self.frames_visible.saturating_add(1);
                    self.frames_completed = self.frames_completed.saturating_add(1);
                    return Progress::Visible;
                }
                2 => {
                    self.frames_completed = self.frames_completed.saturating_add(1);
                    return Progress::Discarded;
                }
                3 => return Progress::Discarded,
                _ => {
                    self.failed_revision = Some(self.revision);
                }
            }
        }
        if self.awaiting_visible {
            if let Some(fence) = state.engine.present_fence {
                for head in 0..8 {
                    if state.engine.head.is_some_and(|wanted| wanted != head) {
                        continue;
                    }
                    let mut info = a::DisplayPresentationStats::default();
                    let slot = unsafe {
                        c::R4SdkGuiDrawContext_displayPresentationStats(
                            0,
                            graphics.native_draw(),
                            head,
                            &mut info,
                        )
                    };
                    if slot.status < 0
                        || slot.flags & 2 != 0
                        || slot.bits as i32 != a::gfx_output_ok
                        || info.backend.adapter_id != fence.adapter_id
                        || info.backend.device_generation != fence.device_generation
                        || info.backend.reset_generation != fence.reset_generation
                    {
                        continue;
                    }
                    if info.flags & a::display_presentation_flag_lost != 0 {
                        self.failed_revision = Some(self.revision);
                        break;
                    }
                    if info.visible_ns != 0
                        && info.source_timeline == fence.timeline
                        && info.source_point == fence.point
                    {
                        if let Ok(session) = state.device.bind(app) {
                            state.capture.complete(
                                &session,
                                state.engine.output_index,
                                state.engine.resources.outputs[state.engine.output_index].resource,
                                time,
                            );
                            if let Ok(source) = state.capture.source() {
                                state.engine.readback_pin = source;
                            }
                        }
                        self.completed_frame = state.engine.frame;
                        self.completed_status = None;
                        self.awaiting_visible = false;
                        self.frames_visible = self.frames_visible.saturating_add(1);
                        self.frames_completed = self.frames_completed.saturating_add(1);
                        return Progress::Visible;
                    }
                }
            } else {
                self.failed_revision = Some(self.revision);
            }
            if time >= self.deadline {
                self.failed_revision = Some(self.revision);
            }
        }
        if self.failed_revision == Some(self.revision) {
            self.awaiting_visible = false;
            if !self.failure_reported {
                self.failure_reported = true;
                return Progress::Failed;
            }
        }
        if self.busy() {
            Progress::Pending
        } else {
            Progress::Idle
        }
    }
    pub fn reject(&mut self, app: &App<'_>) {
        if let Some(state) = self.state.as_deref_mut() {
            if let Ok(session) = state.device.bind(app) {
                state.engine.cancel(&session, gpu::Error::State);
            }
        }
        self.failed_revision = Some(self.revision);
        self.failure_reported = true;
    }
    pub fn capture_demand(&mut self, wanted: bool) {
        self.capture_demand = wanted;
    }
    pub fn invalidate_capture(&mut self) {
        self.capture_reset = true;
    }
    pub fn capture_cursor(&mut self, cursor: CaptureCursor) {
        self.latest_cursor = cursor;
        if let Some(state) = self.state.as_deref_mut() {
            if !state.engine.active() {
                state.capture_cursor = cursor;
            }
        }
    }
    pub fn capture_image(&self) -> Option<CaptureImage<'_>> {
        self.state.as_ref()?.capture.image()
    }
    pub fn acknowledge_capture(&mut self, published: bool) -> bool {
        self.state
            .as_mut()
            .is_some_and(|state| state.capture.acknowledge(published).is_ok())
    }
    pub fn capture_redraw(&mut self, time: u64) -> bool {
        self.state
            .as_mut()
            .is_some_and(|state| state.capture.take_redraw(time))
    }
    /// Retry from the event loop; timeout or a stop request never frees backing.
    pub fn close<M: Memory>(
        &mut self,
        app: &App<'_>,
        windows: &mut Set<M, Consumer>,
        graphics: &Graphics<'_, '_>,
        time: u64,
    ) -> bool {
        self.capture_demand = false;
        if let Some(worker) = &self.preparation {
            worker.stop(time);
        }
        if let Some(newest) = self.newest.as_deref_mut() {
            if !newest.release(windows) {
                return false;
            }
        }
        self.newest = None;
        self.seed = None;
        if !self.collect(app, false) {
            return false;
        }
        self.awaiting_visible = false;
        if let Some(state) = self.state.as_deref_mut() {
            if state.engine.active() && state.engine.fault.is_none() {
                let Ok(session) = state.device.bind(app) else {
                    return false;
                };
                state.engine.cancel(&session, gpu::Error::State);
            }
        }
        let _ = self.poll(app, windows, graphics, time);
        let Some(state) = self.state.as_deref_mut() else {
            return true;
        };
        {
            let Ok(session) = state.device.bind(app) else {
                return false;
            };
            if state.capture.close(&session).is_err() {
                return false;
            }
            match state.capture.source() {
                Ok(source) => state.engine.readback_pin = source,
                Err(_) => return false,
            }
            if state.engine.close(&session).is_err() || !state.reads.empty() {
                return false;
            }
        }
        if !state.device.close(app).unwrap_or(false) {
            return false;
        }
        // All native source/job/broker/device receipts have actually ended.
        drop(self.state.take());
        true
    }
}
impl Drop for Output<'_> {
    fn drop(&mut self) {
        // Native jobs may still point into cache/assets. Keep the complete
        // owner on an unacknowledged drop, rather than only device storage.
        if let Some(state) = self.state.take() {
            mem::forget(state);
        }
        if let Some(newest) = self.newest.take() {
            if !newest.reads.empty() {
                mem::forget(newest);
            }
        }
    }
}
#[cfg(test)]
#[path = "../../../Tests/RustNative/output_worker.rs"]
mod tests;
