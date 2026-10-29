//! Admitted Desktop coordination. Each field is a concrete domain owner;
//! native calls borrow the SDK capabilities, never this coordinator.
use super::{
    bootstrap,
    environment::{Environment, Time},
    owned::Owned,
    service::Host,
    views,
};
use crate::{
    capture::{publisher::Publisher, remote::Cursor as CaptureCursor, runtime as capture},
    diagnostics::{self, snes::Sample as OutputSample},
    hosted::{
        frame_reader::SessionReader, modes::Modes, process::Programs, runtime as hosted,
        window_graphics::NativeTransport, window_image::NativeMemory, window_set::Set,
    },
    input::{
        self,
        event::Target,
        native::Input,
        router::{self, Router},
        runtime::{Poll, Sample},
    },
    outputs::{
        self,
        api::Native,
        bindings, control,
        cursor::{Controller as Cursor, CursorApi},
        manager::Manager,
        power::Owner as Power,
    },
    render::{
        display::Display,
        geometry::{Dirty, MAX_DAMAGE_REGIONS, Rect},
        gpu::receipt::Consumer,
        output_geometry,
        runtime::Frames,
    },
    shell::{
        self,
        controller::{self, Action},
        launch::{Associations, Launch},
        layout_writeback,
        message_box::{Buttons, Kind as MessageKind},
        state::{Dialog, State as Shell},
        status::Status,
        style as s,
        tray_sync::Mirror,
    },
    windows::{
        controller::{Changes, Controller as Windows},
        layout,
        window::Kind,
    },
};
use alloc::vec::Vec;
use core::pin::pin;
use r4gfx::ColorV1Client;
use r4os_library_helpers::standard::Standard;
use r4os_sdk::{
    App, StartupInfo, abi as a,
    graphics::Graphics,
    gui::{Session, raw as key},
    path::FilePath,
    platform::{Audio, Platform, PlatformStorage},
    resources::Resources,
    services::Services,
};
type Result<T> = core::result::Result<T, &'static str>;
#[path = "console_check.rs"]
mod console_check;
#[path = "desktop_launch.rs"]
mod external;
#[path = "desktop_load.rs"]
mod loading;
fn owned<T>(initialize: impl FnOnce() -> Result<T>, name: &'static str) -> Result<Owned<T>> {
    Owned::new_with(initialize).map_err(|error| match error {
        super::owned::InitError::Memory => name,
        super::owned::InitError::Value(error) => error,
    })
}
type WindowImages<'a> = Set<NativeMemory<'a, 'a, 'a>, Consumer>;
struct Desktop<'a, 'input> {
    expected: &'static StartupInfo<'static>,
    shell: Owned<Shell<'input>>,
    audio_io: super::audio_io::AudioIo<'a>,
    hosted: Owned<hosted::State<'input>>,
    frames: Owned<Frames>,
    outputs: Owned<Manager<'a>>,
    primary: outputs::primary::Primary<'a>,
    primary_admission: outputs::primary::Admission,
    control: Owned<control::Owner>,
    windows: Owned<WindowImages<'a>>,
    cursor: Cursor,
    power: Owned<Power>,
    input: input::runtime::State,
    router: Router,
    platform_input: input::platform::Owner,
    modes: Modes,
    tray: Mirror,
    status: Status,
    associations: Associations,
    layout: layout_writeback::Owner<'a>,
    capture: capture::Owner<'a>,
    publisher: Publisher,
    dirty: Dirty,
    directory: a::DirectoryChangeCursor,
    directory_active: bool,
    width: i32,
    height: i32,
    revision: u64,
    blink: bool,
    activity_sequence: u64,
    diagnostics: diagnostics::runtime::Owner,
    launches: external::Owner,
    loads: super::launch_io::Owner<'a>,
    cursor_queued_tick: u64,
}
impl<'a, 'input> Desktop<'a, 'input> {
    fn new(
        env: &Environment<'a>,
        services: &'a Services<'a>,
        expected: &'static StartupInfo<'static>,
        owner: a::ProgramProcessHandle,
        events: &'input mut [a::ConsoleInputEvent],
        run: &'input mut [u8],
        time: Time,
    ) -> Result<Self> {
        // The native driver can still hold the boot output at shell start.
        // Keep the original provisional layout; topology() adopts the real
        // dimensions once an output is published. This creates no scanout.
        let (width, height) = Display(env.graphics).size().unwrap_or((1280, 720));
        let shell = owned(
            || Shell::new(run).map_err(|_| "shell state"),
            "shell allocation",
        )?;
        let hosted = owned(
            || hosted::State::new(events).map_err(|_| "hosted clients"),
            "hosted allocation",
        )?;
        let frames = owned(
            || Frames::new().map_err(|_| "frame caches"),
            "frame allocation",
        )?;
        let outputs = owned(
            || Manager::new().map_err(|_| "output catalog"),
            "output allocation",
        )?;
        let control = owned(
            || control::Owner::new().map_err(|_| "display controls"),
            "control allocation",
        )?;
        let windows = owned(|| Ok(Set::new(NativeMemory(env.graphics))), "window images")?;
        Ok(Self {
            expected,
            shell,
            audio_io: super::audio_io::AudioIo::new(services, env.resources, owner)
                .map_err(|_| "audio requests")?,
            hosted,
            frames,
            outputs,
            primary: Default::default(),
            primary_admission: outputs::primary::Admission::Cpu,
            control,
            windows,
            cursor: Cursor::default(),
            power: owned(|| Ok(Power::default()), "screen power")?,
            input: input::runtime::State::default(),
            router: Router::default(),
            platform_input: input::platform::Owner::default(),
            modes: Modes::default(),
            tray: Mirror::new(owner).map_err(|_| "tray caches")?,
            status: Status::default(),
            associations: Associations::new().map_err(|_| "association config")?,
            layout: layout_writeback::Owner::new(env.standard, time.hz)
                .map_err(|_| "layout writeback")?,
            capture: capture::Owner::new(env.app, env.resources, expected),
            publisher: Publisher::default(),
            dirty: Dirty::default(),
            directory: a::DirectoryChangeCursor::default(),
            directory_active: false,
            width,
            height,
            revision: 0,
            blink: true,
            activity_sequence: 0,
            launches: external::Owner::default(),
            loads: super::launch_io::Owner::default(),
            diagnostics: diagnostics::runtime::Owner::new(
                super::test_start::requested(env.app)
                    || env
                        .app
                        .args()
                        .split(|v| v.is_ascii_whitespace())
                        .any(|v| v.eq_ignore_ascii_case(b"/CHECK-ISOLATED")),
            ),
            cursor_queued_tick: 0,
        })
    }
    fn bar(&mut self) {
        self.dirty.invalidate(Rect::new(
            0,
            self.height - s::TASKBAR_H,
            self.width,
            s::TASKBAR_H,
        ));
    }
    fn full(&mut self) {
        self.dirty.invalidate(self.hosted.bounds);
    }
    fn shell_damage(&mut self) {
        router::shell_damage(&self.shell, &mut self.dirty, self.width, self.height);
    }
    fn message(&mut self, kind: MessageKind, title: &[u8], text: &[u8]) {
        self.shell_damage();
        self.shell.show_message(kind, Buttons::Ok, title, text);
        self.shell_damage();
    }
    fn changes(
        &mut self,
        change: Changes,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        time: Time,
    ) {
        self.hosted
            .changes(&change, session, env.app, time.tick, &mut self.dirty);
    }
    fn areas(&self) -> [Rect; 4] {
        core::array::from_fn(|i| {
            layout::output(
                &self.outputs,
                self.hosted.model.windows.windows()[i].geometry(),
                self.width,
                self.height,
            )
            .1
        })
    }
    fn initialize(&mut self, env: &Environment<'a>, session: &mut Session<'_, '_>, time: Time) {
        self.loads.stop_all(0);
        self.publisher
            .reset(env.app, &mut self.outputs, self.primary.output_mut());
        if let Ok(path) = FilePath::parse(shell::desktop_folder::DIRECTORY) {
            self.directory_active = env
                .system
                .files()
                .begin_directory_changes(&path, &mut self.directory)
                .is_ok();
        }
        self.shell.load(env, self.width, self.height);
        self.associations.load(&env.store);
        let _ = self.outputs.preferences.reload_layout(&env.store);
        let _ = self.outputs.reload_colors(&env.store);
        self.hosted.bounds = Rect::new(0, 0, self.width, self.height);
        for i in 0..4 {
            self.hosted
                .model
                .windows
                .window_mut(i)
                .unwrap()
                .fit_to_work_area(self.width, self.height);
        }
        self.status.load(env);
        self.status.refresh(env, session, time.tick, time.hz);
        self.input.initial(session, self.hosted.bounds);
        Input::new(session).hide_legacy_cursor();
        let _ = self
            .input
            .pointer_capture
            .bind(&mut input::pointer_capture::Native(session), false);
        self.full();
    }
    fn topology(
        &mut self,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        host: &mut Host<'_>,
        time: Time,
    ) {
        let _ = self.control.step(
            env.app,
            &env.libraries,
            &mut self.outputs,
            host,
            &env.store,
            Some(time.nanoseconds),
        );
        let Ok(revision) = env.graphics.outputs().revision() else {
            self.outputs.poll(
                env.app,
                env.graphics,
                &env.libraries,
                &mut self.windows,
                time.nanoseconds,
            );
            return;
        };
        let changed = unsafe {
            self.outputs.refresh(
                revision.revision,
                env.app,
                env.graphics,
                &env.libraries,
                &mut self.windows,
                time.nanoseconds,
            )
        }
        .unwrap_or(false);
        if !changed && self.revision == revision.revision {
            return;
        }
        self.revision = revision.revision;
        let dimensions = self
            .outputs
            .layout
            .primary()
            .and_then(|i| self.outputs.layout.outputs()[i].view.logical().ok())
            .map(|r| (r.w as i32, r.h as i32))
            .or_else(|| Display(env.graphics).size());
        if let Some((w, h)) = dimensions {
            self.width = w;
            self.height = h;
        }
        self.hosted.bounds = if self.outputs.active() {
            self.outputs.desktop_bounds
        } else {
            Rect::new(0, 0, self.width, self.height)
        };
        if changed {
            let shift = self.outputs.translation;
            for i in 0..4 {
                let old = self.hosted.model.windows.windows()[i].geometry();
                let (output, work) = layout::output(&self.outputs, old, self.width, self.height);
                let layout = &self.outputs.layout;
                self.hosted
                    .model
                    .windows
                    .window_mut(i)
                    .unwrap()
                    .reconfigure(
                        |r| {
                            output_geometry::rebase_window(
                                layout,
                                r,
                                shift,
                                work,
                                s::TITLE_H as u32,
                            )
                        },
                        output,
                        work,
                    );
                let new = self.hosted.model.windows.windows()[i].geometry();
                if new != old {
                    self.changes(
                        Changes {
                            geometry: 1 << i,
                            resized: 1 << i,
                            dirty: 1 << i,
                            previous_frame: Some(old),
                            ..Default::default()
                        },
                        env,
                        session,
                        time,
                    );
                }
            }
        }
        self.publisher
            .reset(env.app, &mut self.outputs, self.primary.output_mut());
        self.full();
    }
    fn refresh(
        &mut self,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        host: &mut Host<'_>,
        programs: &Programs<'_>,
        time: Time,
    ) {
        let observation = self
            .platform_input
            .observe(input::platform::snapshot(env.app), time.nanoseconds);
        if observation.activity {
            self.input.last_input_ns = time.nanoseconds;
            self.diagnostics.input();
        }
        for action in observation.actions() {
            match action {
                input::platform::ScreenAction::Sleep => {
                    let _ = self
                        .power
                        .sleep(&mut Native(env.graphics), time.nanoseconds);
                }
                input::platform::ScreenAction::Wake => {
                    self.power
                        .input(&mut Native(env.graphics), time.nanoseconds);
                }
            }
        }
        self.platform_input
            .step(&mut Native(env.graphics), time.nanoseconds);
        self.topology(env, session, host, time);
        let (redraw, visible) = unsafe {
            self.primary.poll(
                env.app,
                env.graphics,
                &mut *self.windows,
                self.outputs.owns_output_route(),
                time.nanoseconds,
            )
        };
        if redraw {
            self.full()
        }
        if visible {
            self.frames.stats.presents = self.frames.stats.presents.saturating_add(1);
        }
        self.primary_admission = self.primary.admit(
            env.app,
            env.graphics,
            self.revision,
            time.nanoseconds,
            self.input.last_input_ns,
        );
        let areas = self.areas();
        unsafe {
            self.hosted.synchronize(
                env.app,
                env.resources,
                programs,
                session,
                &mut *self.windows,
                &mut self.dirty,
                time.tick,
                time.hz,
                &areas,
                true,
                self.shell.dialog == crate::shell::state::Dialog::Tasks,
                self.shell.config.terminal_size,
                self.shell.config.terminal_codepage,
            );
        }
        if let Some(request) =
            crate::hosted::launch_request::take(&self.hosted, session, time.nanoseconds)
        {
            self.launch(request, env, session, programs, time);
        }
        self.hosted
            .synchronize_mirror(host, &mut self.windows, time.tick, time.hz);
        self.modes.synchronize(
            &mut self.hosted,
            &self.outputs,
            host,
            env.app,
            session,
            &mut self.dirty,
            self.width,
            self.height,
            time.tick,
            time.hz,
        );
        self.poll_loads(env, session, programs, time, false);
        let bindings = core::array::from_fn(|i| {
            if !self.outputs.active()
                && self.primary_admission == outputs::primary::Admission::Native
            {
                return None;
            }
            bindings::select(
                &self.outputs,
                env.graphics,
                self.hosted.model.windows.windows()[i].client_surface().rect,
                self.revision,
                time.nanoseconds,
                self.primary_admission != outputs::primary::Admission::Pending,
            )
        });
        let mirrored = core::array::from_fn(|i| self.hosted.mirror.mirrored(i));
        let specs = WindowImages::specifications(
            &self.hosted.model.windows,
            &self.hosted.model.clients,
            mirrored,
            self.hosted.mirror.gate.available(),
            self.hosted.terminal_mode,
            bindings,
        );
        let mut transport = NativeTransport { host, programs };
        self.windows
            .refresh_markers(transport.host, self.activity_sequence);
        self.windows.poll(
            transport.host.owner(),
            specs,
            &mut transport,
            &self.hosted.model.windows,
            &mut self.dirty,
        );
        if self
            .tray
            .synchronize(transport.host, time.tick, u64::from(time.hz).div_ceil(20))
            .changed
        {
            self.bar();
        }
        self.external_launch(env, session, transport.host, programs, time);
        if self.status.refresh(env, session, time.tick, time.hz) {
            self.bar();
        }
        if self
            .shell
            .clock
            .update(&env.store, env.app, time.nanoseconds)
            .unwrap_or(false)
        {
            self.bar();
        }
        let installed = if self.shell.audio.poll_due(time.tick) {
            FilePath::parse(b"C:\\R4OS\\SERVICES\\AUDSVC.R4X")
                .ok()
                .is_some_and(|path| matches!(env.system.files().info(&path), Ok(Some(_))))
        } else {
            self.shell.audio.ui.installed
        };
        if self
            .shell
            .audio
            .synchronize(&mut self.audio_io, installed, time.tick, time.hz)
        {
            self.shell_damage();
            self.bar();
        }
        let visible = self
            .hosted
            .model
            .windows
            .windows()
            .iter()
            .filter(|w| w.visible())
            .count();
        if self.tray.layout(shell::tray::Layout {
            screen_w: self.width,
            screen_h: self.height,
            taskbar_h: s::TASKBAR_H,
            item_y: 4,
            system_left: shell::taskbar_view::layout_rect(
                self.width,
                self.height,
                self.shell.config.taskbar_clock,
                self.shell.audio.ui.installed,
            )
            .x,
            window_start: shell::quick_launch::taskbar_window_start_x(self.shell.quick.count()),
            visible_windows: visible,
        }) {
            self.bar();
        }
        self.tray
            .publish_visibility(transport.host, time.tick, u64::from(time.hz).div_ceil(20));
        if self.directory_active
            && env
                .system
                .files()
                .poll_directory_changes(&mut self.directory)
                .is_ok_and(|n| n > 0)
        {
            self.frames.icons.invalidate();
            let _ = self.shell.reload_folder(env, self.width, self.height);
            self.full();
        }
        if self.shell.folder.layout_rewrite {
            let _ = self.layout.mark(time.tick);
            self.shell.folder.layout_rewrite = false;
        }
        unsafe {
            self.layout.flush(
                env.app,
                env.resources,
                self.expected,
                env.standard,
                &self.shell.folder.items,
                time.tick,
                false,
            );
        }
        if self.capture.poll() {
            self.bar();
        }
        if self.publisher.refresh(env.app) {
            self.full();
        }
        self.publisher.suppressed = self.power.want_off()
            || self.power.wake_pending()
            || self.diagnostics.capture_override();
        self.publisher
            .sync(&mut self.outputs, self.primary.output_mut());
        let blink = (time.tick / u64::from(time.hz).div_ceil(2).max(1)) % 2 == 0;
        if self.blink != blink {
            self.blink = blink;
            if self.shell.dialog == Dialog::Run {
                self.shell_damage();
            }
            self.hosted.invalidate_console_cursors(&mut self.dirty);
        }
        self.power.tick(
            &mut Native(env.graphics),
            time.nanoseconds,
            self.shell.config.screen_off_seconds,
        );
    }
    fn diagnostic_work(
        &mut self,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        host: &mut Host<'_>,
        programs: &Programs<'_>,
        audio: Option<&Audio<'_>>,
        time: Time,
    ) {
        self.diagnostics.launch_status = (self.loads.pending(), self.loads.loading());
        if !self.diagnostics.pending() && host.diagnostic_work().is_none() {
            return;
        }
        let output_captures = if host
            .diagnostic_work()
            .is_some_and(|work| work.request.op == diagnostics::wire::Op::CaptureStats)
        {
            self.capture_observations()
        } else {
            [None; 4]
        };
        let work_areas = self.areas();
        let mut view = diagnostics::runtime::View {
            hosted: &mut self.hosted,
            shell: &mut self.shell,
            dirty: &mut self.dirty,
            tray: &self.tray,
            input: &self.input,
            status: &self.status,
            work_areas,
            width: self.width,
            height: self.height,
            output_captures,
            native_failures: if host
                .diagnostic_work()
                .is_some_and(|work| work.request.op == diagnostics::wire::Op::CaptureStats)
            {
                self.outputs.native_failures()
            } else {
                (0, [None; 4])
            },
        };
        let effects = self.diagnostics.poll(
            &mut view,
            env,
            session,
            host,
            programs,
            &mut self.capture,
            audio,
            time,
            &self.frames.stats,
            self.input.stats.cursor_moves,
        );
        if effects.audio_changed {
            self.shell_damage();
            self.bar();
        }
        if effects.reset_window_service {
            let was_relative = self.input.pointer_capture.relative();
            let was_active = self.input.pointer_capture.active();
            let _ = self.input.pointer_capture.reset(
                &mut input::pointer_capture::Native(session),
                time.tick,
                &mut self.input.cursor,
                self.hosted.bounds,
            );
            if was_relative {
                self.restore_capture_cursor();
            }
            if was_active {
                self.pointer_ownership_changed();
            }
            self.full();
            for (index, client) in self.hosted.model.clients.slots().iter().enumerate() {
                if crate::hosted::process::valid(client.handle()) {
                    let _ =
                        crate::hosted::gui::Host::new(session).associate(&client.handle(), index);
                }
            }
            self.windows.disconnect();
            self.tray.reset(time.tick, u64::from(time.hz) * 2);
        }
        if effects.reset_capture {
            self.publisher
                .reset(env.app, &mut self.outputs, self.primary.output_mut());
            self.full();
        }
        self.publisher.suppressed = self.power.want_off()
            || self.power.wake_pending()
            || self.diagnostics.capture_override();
        self.publisher
            .sync(&mut self.outputs, self.primary.output_mut());
    }
    fn capture_observations(&self) -> [Option<outputs::selection::CaptureObservation>; 4] {
        let mut observations = [None; 4];
        if self.outputs.active() {
            for (slot, choice) in observations.iter_mut().zip(
                self.outputs
                    .composition_choices(self.outputs.desktop_bounds),
            ) {
                *slot = Some(outputs::selection::CaptureObservation {
                    state: choice.target.and_then(|t| self.outputs.capture_state(t)),
                    choice,
                });
            }
        } else {
            let mut choice = outputs::selection::Choice::primary(self.primary_admission);
            let state = self.primary.output().and_then(|owner| {
                choice.execution = Some(owner.execution_observation());
                owner.capture_state()
            });
            observations[0] = Some(outputs::selection::CaptureObservation { choice, state });
        }
        observations
    }
    fn cursor(&mut self, env: &Environment<'a>, time: Time) {
        let mut info = a::DisplayCursorInfo::default();
        let mut native = Native(env.graphics);
        let wanted = !self.hosted.terminal_mode
            && !self.input.pointer_capture.relative()
            && native.info(&mut info) == a::gfx_output_ok
            && self
                .outputs
                .cursor_target(info, self.width as u32, self.height as u32)
                .is_some()
            && !self.power.want_off()
            && !self.power.wake_pending();
        if self.cursor.poll(
            &mut native,
            time.nanoseconds,
            self.input.cursor.x,
            self.input.cursor.y,
            wanted,
        ) {
            self.full();
        }
        for slot in &self.outputs.slots {
            if let Some(output) = slot.resource.gpu() {
                if let Some(mut key) = output.clean_frame(slot.target) {
                    key.frame = output.completed_frame;
                    self.cursor.completed(
                        key,
                        output
                            .completed_status
                            .is_some_and(|status| status.result == 1),
                    );
                }
            }
        }
    }
    fn render(
        &mut self,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        time: Time,
    ) -> Result<bool> {
        self.cursor(env, time);
        let pointer = CaptureCursor {
            x: self.input.cursor.x,
            y: self.input.cursor.y,
            visible: !self.hosted.terminal_mode && !self.input.pointer_capture.relative(),
            separate: !self.cursor.software(),
        };
        for slot in &mut self.outputs.slots {
            if let Some(output) = slot.resource.gpu_mut() {
                output.capture_cursor(pointer);
            }
            if let Some(output) = slot.resource.software_mut() {
                output.capture_cursor(pointer);
            }
        }
        if let Some(output) = self.primary.output_mut() {
            output.capture_cursor(pointer);
        }
        if !self.outputs.active() && self.primary_admission == outputs::primary::Admission::Pending
        {
            return Ok(false);
        }
        if !self.outputs.active()
            && self.primary_admission == outputs::primary::Admission::Cpu
            && !self.frames.current
            && self.dirty.active()
        {
            self.full();
        }
        let mut regions = [Rect::ZERO; MAX_DAMAGE_REGIONS];
        let count = self.dirty.take_regions(&mut regions);
        let trace_input = self.diagnostics.benchmark.trace.as_ref().and_then(|trace| {
            let i = trace.window_index;
            let slot = &self.hosted.model.clients.slots()[i];
            let target = self.diagnostics.benchmark.child;
            let handle = slot.handle();
            if !slot.bound()
                || slot.retiring()
                || handle.instance_id != target.instance
                || handle.generation != target.generation
            {
                return None;
            }
            let win = &self.hosted.model.windows.windows()[i];
            let frame = slot.frames().view();
            Some((
                win.geometry(),
                win.client_surface().rect,
                if frame.valid {
                    frame.info.committed_generation
                } else {
                    0
                },
                i,
            ))
        });
        if let Some((frame, client, generation, _)) = trace_input {
            if let Some(trace) = &mut self.diagnostics.benchmark.trace {
                trace.observe_context(
                    self.outputs.active()
                        || self.primary_admission != outputs::primary::Admission::Cpu,
                    frame,
                    client,
                    generation,
                );
                if self.outputs.active() {
                    for choice in self.outputs.composition_choices(client) {
                        trace.observe_choice(choice, generation);
                    }
                } else {
                    let mut choice = outputs::selection::Choice::primary(self.primary_admission);
                    if self.primary_admission == outputs::primary::Admission::Native {
                        choice.execution = self
                            .primary
                            .output()
                            .map(outputs::native::Output::execution_observation);
                    }
                    trace.observe_choice(choice, generation);
                }
            }
        }
        let software_cursor = self.cursor.software()
            && !self.cursor.clean_pending()
            && !self.hosted.terminal_mode
            && !self.input.pointer_capture.relative();
        let mut scene = views::scene(
            self.width,
            self.height,
            &self.hosted,
            &self.shell,
            &self.input,
            &self.tray,
            &self.status,
            &self.frames.stats,
            software_cursor,
            self.capture.recording_state(),
            self.blink,
        );
        let rendered = if self.outputs.active() {
            let before: [Option<outputs::cursor::CleanFrame>; 16] = core::array::from_fn(|i| {
                self.outputs
                    .slots
                    .get(i)
                    .and_then(|s| s.resource.gpu().and_then(|o| o.clean_frame(s.target)))
            });
            let rendered = unsafe {
                self.frames.outputs(
                    env.app,
                    session,
                    env.graphics,
                    env.resources,
                    self.expected,
                    &mut self.outputs,
                    &mut self.windows,
                    env.colors,
                    &mut scene,
                    &regions[..count],
                    time.nanoseconds,
                    self.input.last_input_ns,
                )
            };
            for (i, slot) in self.outputs.slots.iter_mut().enumerate() {
                if let Some(output) = slot.resource.gpu_mut() {
                    if let Some(key) = output.clean_frame(slot.target) {
                        if before.get(i).copied().flatten() != Some(key) {
                            self.cursor.captured(key);
                        }
                    }
                    if let Some(image) = output.capture_image() {
                        let published = self.publisher.image(
                            env.app,
                            &mut self.frames,
                            image,
                            self.width,
                            self.height,
                        );
                        output.acknowledge_capture(published);
                    }
                }
                if let Some(output) = slot.resource.software_mut() {
                    if let Some(image) = output.capture_image() {
                        let published = self.publisher.image(
                            env.app,
                            &mut self.frames,
                            image,
                            self.width,
                            self.height,
                        );
                        output.acknowledge_capture(published);
                    }
                }
            }
            rendered
        } else if self.primary_admission == outputs::primary::Admission::Native
            && (count != 0
                || self
                    .primary
                    .output()
                    .is_some_and(|o| o.pending_ready(time.nanoseconds)))
        {
            let captured = unsafe {
                self.frames.native_primary(
                    env.app,
                    session,
                    env.resources,
                    self.expected,
                    self.primary.output_mut().ok_or("native primary owner")?,
                    &mut self.windows,
                    &mut scene,
                    &regions[..count],
                    time.nanoseconds,
                )
            };
            if !captured {
                for region in &regions[..count] {
                    self.dirty.invalidate(*region)
                }
            }
            captured
        } else if count != 0 {
            let result = unsafe {
                self.frames.primary(
                    env.app,
                    session,
                    env.graphics,
                    env.colors,
                    &mut self.windows,
                    &mut scene,
                    &regions[..count],
                    self.input.last_event_tick,
                )
            }
            .map_err(|_| "software frame")?;
            self.cursor.presented(result.presentation.presented);
            if result.presentation.presented && result.presentation.copied {
                if let Some((frame, client, generation, _)) = trace_input.filter(|(_, _, _, i)| {
                    result.completion.cull.rendered_windows & (1u8 << i) != 0
                }) {
                    let ticks = if self.cursor_queued_tick != 0 {
                        r4os_sdk::time::Clock::new(env.app)
                            .ticks()
                            .ok()
                            .map(|now| now.saturating_sub(self.cursor_queued_tick))
                    } else {
                        None
                    };
                    if let Some(trace) = &mut self.diagnostics.benchmark.trace {
                        trace.record(
                            frame,
                            client,
                            OutputSample {
                                generation,
                                completed_ns: result.completed_ns,
                                frame_wall_ns: result
                                    .completed_ns
                                    .saturating_sub(result.started_ns),
                                compose_wall_ns: self.frames.stats.last_composition_ns,
                                present_wall_ns: self.frames.stats.last_present_ns,
                                damage_bounds: regions[..count]
                                    .iter()
                                    .copied()
                                    .reduce(Rect::merged)
                                    .unwrap(),
                                full_client_copy: regions[..count].iter().any(|r| {
                                    r.x <= client.x
                                        && r.y <= client.y
                                        && r.right() >= client.right()
                                        && r.bottom() >= client.bottom()
                                }),
                                cursor_queued_to_copy_ticks: ticks,
                            },
                        );
                    }
                }
                self.cursor_queued_tick = 0;
            }
            let _ = self
                .publisher
                .primary(env.app, &mut self.frames, &regions[..count], pointer);
            true
        } else {
            if self.publisher.cursor_damage.active() {
                let _ = self
                    .publisher
                    .primary(env.app, &mut self.frames, &[], pointer);
            }
            false
        };
        if rendered {
            self.frames.stats.last_damage = count as u8;
        }
        if let Some(output) = self.primary.output_mut() {
            if let Some(image) = output.capture_image() {
                let published =
                    self.publisher
                        .image(env.app, &mut self.frames, image, self.width, self.height);
                output.acknowledge_capture(published);
            }
        }
        Ok(rendered)
    }
    fn launch(
        &mut self,
        request: Launch,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        programs: &Programs<'_>,
        time: Time,
    ) {
        self.shell_damage();
        self.shell.close_top();
        if let Err(error) = self.begin_load(request, None, env, session, programs, time) {
            self.launch_error(error);
        }
    }
    fn launch_error(&mut self, error: crate::hosted::launch::Failure) {
        use crate::hosted::launch::Failure;
        let (kind, text) = match error {
            Failure::Path(_) => (MessageKind::Error, b"Invalid program path.".as_slice()),
            Failure::Arguments => (
                MessageKind::Error,
                b"Program arguments or title are too long.".as_slice(),
            ),
            // program_class has its own -1/missing and -2/invalid-header
            // contract; process-handle errors apply only after spawn.
            Failure::Classification(r4os_sdk::storage::Error::Raw(-1))
            | Failure::Loader(r4os_sdk::storage::Error::Raw(a::program_handle_error_not_found)) => {
                (MessageKind::Error, b"Program was not found.".as_slice())
            }
            Failure::Classification(_) => (
                MessageKind::Error,
                b"Program header could not be read or is invalid.".as_slice(),
            ),
            Failure::WindowCapacity { console } => (
                MessageKind::Warning,
                if console {
                    b"No free console window slot is available.".as_slice()
                } else {
                    b"No free GUI window slot is available.".as_slice()
                },
            ),
            Failure::Attachment => (
                MessageKind::Error,
                b"Program started but its window could not be attached.".as_slice(),
            ),
            Failure::Loader(_) => (
                MessageKind::Error,
                b"Program loader returned an error.".as_slice(),
            ),
        };
        self.message(kind, b"Run", text);
    }
    fn action(
        &mut self,
        action: Action,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        host: &mut Host<'_>,
        programs: &Programs<'_>,
        time: Time,
    ) {
        match action {
            Action::None => {}
            Action::Launch(value) => self.launch(value, env, session, programs, time),
            Action::RunSubmit => {
                if self.shell.run.text().is_ok_and(|v| v.is_empty()) {
                    return;
                }
                match self.shell.run.text().ok().and_then(|v| Launch::run(v).ok()) {
                    Some(value) => self.launch(value, env, session, programs, time),
                    None => self.message(MessageKind::Error, b"Run", b"Invalid command or path."),
                }
            }
            Action::RunBrowse => {
                if !super::shell_io::browse(&mut self.shell, env) {
                    self.message(MessageKind::Info, b"Run", b"No usable program was found.");
                }
            }
            Action::RunPaste => {
                if !super::shell_io::paste(&mut self.shell, session) {
                    self.message(
                        MessageKind::Error,
                        b"Run",
                        b"Invalid clipboard text or path is too long.",
                    );
                }
            }
            Action::RunCopy { cut } => super::shell_io::copy(&mut self.shell, session, cut),
            Action::RunInvalid => {
                self.message(MessageKind::Error, b"Run", b"Invalid command or path.")
            }
            Action::System(action) => {
                unsafe {
                    self.layout.flush(
                        env.app,
                        env.resources,
                        self.expected,
                        env.standard,
                        &self.shell.folder.items,
                        time.tick,
                        true,
                    );
                }
                while self.layout.busy() {
                    env.app.sleep(1);
                    let _ = self
                        .layout
                        .poll(env.app, env.time().map_or(time.tick, |v| v.tick));
                }
                let _ = super::shell_io::system(env, action);
                self.message(MessageKind::Info,b"System transition refused",b"CPU requests could not be returned. The system remains running; inspect POWERSVC before retrying.");
            }
            Action::ScreenOff => {
                if self
                    .power
                    .sleep(&mut Native(env.graphics), time.nanoseconds)
                    .is_err()
                {
                    self.message(
                        MessageKind::Info,
                        b"Screen off",
                        b"Screen sleep is unavailable for the current display.",
                    );
                }
            }
            Action::TerminalMode => {
                if self
                    .hosted
                    .enter_terminal(programs, env.app, session, time.tick, &mut self.dirty)
                    .is_err()
                {
                    self.message(
                        MessageKind::Warning,
                        b"Terminal Mode",
                        b"Close desktop apps before switching to Terminal Mode.",
                    );
                }
            }
            Action::ShowDesktop | Action::QuickComputer => {
                let i = if matches!(action, Action::ShowDesktop) {
                    0
                } else {
                    1
                };
                self.shell_damage();
                self.shell.close_top();
                if self
                    .shell
                    .quick
                    .items()
                    .get(i)
                    .is_some_and(|v| v.kind == shell::quick_launch::Kind::ShowDesktop)
                {
                    let change = self.hosted.model.windows.show_desktop();
                    self.changes(change, env, session, time);
                } else if let Some(request) = self
                    .shell
                    .quick
                    .launch(i)
                    .and_then(|v| Launch::menu(v).ok())
                {
                    self.launch(request, env, session, programs, time);
                }
            }
            Action::KeyboardLayout => {
                self.status.cycle_keyboard(session);
                self.bar();
            }
            Action::VolumeStep(step) => {
                let percent = shell::volume::stepped(self.shell.audio.ui.percent, step);
                self.shell.audio.set_percent(percent, true, time.tick);
                self.bar();
            }
            Action::VolumePercent(percent) => {
                self.shell.audio.set_percent(percent, true, time.tick);
                self.bar();
            }
            Action::VolumePointer { x, commit } => {
                let popup = shell::status_view::volume_popup_rect(
                    self.width,
                    self.height,
                    self.shell.config.taskbar_clock,
                );
                let percent = shell::volume::percent_at_x(shell::volume::track_rect(popup), x);
                self.shell.audio.set_percent(percent, commit, time.tick);
                self.bar();
            }
            Action::VolumeMute => {
                self.shell.audio.toggle_mute(time.tick);
                self.bar();
            }
            Action::VolumeOutput(index) => {
                let id = index
                    .and_then(|i| self.shell.audio.page.rows.get(i))
                    .map_or([0; 64], |r| r.id);
                self.shell.audio.select(id);
                self.bar();
            }
            Action::VolumePage(step) => {
                self.shell.audio.change_page(step > 0);
            }
            Action::LayoutChanged => {
                let _ = self.layout.mark(time.tick);
            }
            Action::Tray {
                identity,
                kind,
                wheel,
                x,
                y,
            } => {
                self.tray
                    .activate(host, identity, kind, wheel, x, y, time.tick);
                self.bar();
            }
            Action::Desktop(i) => {
                if let Some(item) = self.shell.folder.items.entries().get(i) {
                    match self.associations.item(item) {
                        Ok(request) => self.launch(request, env, session, programs, time),
                        Err(_) => self.message(
                            MessageKind::Error,
                            b"Open",
                            b"No usable application association.",
                        ),
                    }
                }
            }
            Action::Window(target) => {
                if let Some(i) = Windows::index_for_target(target) {
                    if target == Windows::close_target(i) {
                        self.hosted.close_window(
                            i,
                            programs,
                            session,
                            env.app,
                            time.tick,
                            &mut self.dirty,
                        );
                    } else if target == Windows::min_target(i) {
                        let change = self.hosted.model.windows.minimize(i);
                        self.changes(change, env, session, time);
                    } else if target == self.hosted.model.windows.max_target(i) {
                        let (_, work) = layout::output(
                            &self.outputs,
                            self.hosted.model.windows.windows()[i].geometry(),
                            self.width,
                            self.height,
                        );
                        let change = self.hosted.model.windows.toggle_maximize(i, work);
                        self.changes(change, env, session, time);
                    } else if target == Windows::info_target(i) {
                        let rect = self.hosted.model.windows.windows()[i].geometry();
                        let mut text = [0; 160];
                        self.message(
                            MessageKind::Info,
                            b"Window Info",
                            shell::message_box::window_info(&mut text, rect),
                        );
                    } else {
                        self.hosted
                            .focus(i, session, env.app, time.tick, &mut self.dirty);
                    }
                    self.shell.system_menu = None;
                }
            }
            Action::ReloadSettings => {
                let _ = env.store.load_config(env.standard, &mut self.shell.config);
                self.shell.reload_wallpaper(env);
                self.full();
            }
        }
    }
    fn key(
        &mut self,
        value: u32,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        host: &mut Host<'_>,
        programs: &Programs<'_>,
        time: Time,
    ) {
        self.input.last_event_tick = time.tick;
        if value == key::R4SdkGuiKey_ctrl_c as u32
            && !self.shell.blocked()
            && self.hosted.active_console().is_some()
        {
            for i in 0..4 {
                if self.hosted.model.windows.windows()[i].kind() == Kind::Terminal {
                    self.hosted.close_window(
                        i,
                        programs,
                        session,
                        env.app,
                        time.tick,
                        &mut self.dirty,
                    );
                }
            }
            return;
        }
        // The original terminal mode consumes every key even while its
        // console is still starting. A temporarily missing client must not
        // dispatch Run, Start or Escape into the hidden desktop shell.
        if self.hosted.terminal_mode {
            if !self.hosted.console_scroll_key(value, &mut self.dirty) {
                self.hosted
                    .console_key(env.app, session, value, &mut self.dirty);
            }
            return;
        }
        self.shell_damage();
        if let Some(action) = controller::key(
            &mut self.shell,
            value,
            self.hosted.model.inventory.visible().len(),
        ) {
            self.action(action, env, session, host, programs, time);
            self.shell_damage();
            return;
        }
        if !self.shell.blocked()
            && (self.hosted.console_scroll_key(value, &mut self.dirty)
                || self
                    .hosted
                    .console_key(env.app, session, value, &mut self.dirty)
                || self.hosted.gui_key(env.app, session, value, time.tick))
        {
            return;
        }
        let action = match value as i32 {
            0x83 | key::R4SdkGuiKey_start_menu => {
                Some(controller::activate(&mut self.shell, Target::StartButton))
            }
            0x85 | 0x87 => {
                let change = self.hosted.model.windows.cycle(value == 0x87);
                self.changes(change, env, session, time);
                None
            }
            0x86 => Some(Action::Window(Windows::close_target(
                self.hosted.model.windows.active(),
            ))),
            key::R4SdkGuiKey_escape => {
                if self.hosted.terminal_mode {
                    self.hosted.terminal_mode = false;
                    self.full();
                } else {
                    self.shell.close_top();
                }
                None
            }
            10 | 13 | 32 => Some(controller::activate(&mut self.shell, Target::StartButton)),
            82 | 114 => Some(controller::activate(&mut self.shell, Target::MenuRun)),
            84 | 116 => Some(controller::activate(&mut self.shell, Target::MenuTasks)),
            68 | 100 => Some(controller::activate(&mut self.shell, Target::MenuTerminal)),
            _ => None,
        };
        if let Some(action) = action {
            self.action(action, env, session, host, programs, time);
        }
        self.shell_damage();
        self.bar();
    }
    fn pointer_ownership_changed(&mut self) {
        self.input.pressed = input::event::Target::None;
        self.input.hover = input::event::Target::None;
        self.input.down_target = input::event::Target::None;
        self.input.double_click = false;
        self.input.tray_hover = Default::default();
        self.input.tray_pressed = Default::default();
        self.input.right_tray = Default::default();
        self.full();
    }
    fn restore_capture_cursor(&mut self) {
        if let Some((_, point)) = self.outputs.layout.nearest(self.input.cursor) {
            self.input.cursor = point;
        }
        self.outputs
            .rebase_pointer(self.input.pointer_capture.restoration_motion());
    }
    fn sync_pointer_capture(
        &mut self,
        session: &mut Session<'_, '_>,
        host: &mut Host<'_>,
        tick: u64,
    ) -> bool {
        if !self.input.pointer_capture.bound() && host.claimed() && host.pointer_broker_live() {
            if self
                .input
                .pointer_capture
                .bind(&mut input::pointer_capture::Native(session), false)
                == 0
            {
                for (index, client) in self.hosted.model.clients.slots().iter().enumerate() {
                    if crate::hosted::process::valid(client.handle()) {
                        let _ = crate::hosted::gui::Host::new(session)
                            .associate(&client.handle(), index);
                    }
                }
            }
        }
        let was_relative = self.input.pointer_capture.relative();
        let was_active = self.input.pointer_capture.active();
        let windows = &self.hosted.model.windows;
        let clients = &self.hosted.model.clients;
        let blocked = self.shell.blocked() || self.hosted.terminal_mode;
        let mut broker_lost = false;
        let changed = self.input.pointer_capture.sync(
            &mut input::pointer_capture::Native(session),
            |token| {
                if !host.pointer_broker_live() {
                    broker_lost = true;
                    Err(a::gui_pointer_reason_desktop_changed)
                } else {
                    input::pointer_capture::target(windows, clients, token, blocked)
                }
            },
            self.hosted.bounds,
            &mut self.input.cursor,
            [self.input.physical_buttons, self.input.remote_buttons],
            tick,
        );
        if broker_lost {
            self.input.pointer_capture.unbind(
                &mut input::pointer_capture::Native(session),
                &mut self.input.cursor,
                self.hosted.bounds,
            );
        }
        if was_relative && !self.input.pointer_capture.relative() {
            self.restore_capture_cursor();
        }
        if was_active != self.input.pointer_capture.active() {
            self.pointer_ownership_changed();
        }
        if changed {
            self.full();
        }
        changed
    }
    fn poll_input(
        &mut self,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        host: &mut Host<'_>,
        programs: &Programs<'_>,
        time: Time,
    ) -> bool {
        let mut active = self.sync_pointer_capture(session, host, time.tick);
        let mut power_sampled = false;
        for source in 0..4 {
            let limit = match source {
                0 => 16,
                1 => 32,
                _ => 1,
            };
            for _ in 0..limit {
                let sample = match source {
                    0 => self.input.remote(session),
                    1 => self.input.physical(session),
                    2 => self.input.keyboard(session),
                    _ => self
                        .input
                        .mouse(session, &mut self.outputs, self.hosted.bounds),
                };
                let sample = match sample {
                    Poll::Empty => break,
                    Poll::Ignored => continue,
                    Poll::Event(v) => v,
                };
                active = true;
                self.diagnostics.input();
                self.input.last_input_ns = time.nanoseconds;
                let waking = if power_sampled {
                    self.power
                        .input_cached(&mut Native(env.graphics), time.nanoseconds)
                } else {
                    power_sampled = true;
                    self.power
                        .input(&mut Native(env.graphics), time.nanoseconds)
                };
                if waking {
                    continue;
                }
                match sample {
                    Sample::Activity => {}
                    Sample::Key { key } => self.key(key, env, session, host, programs, time),
                    Sample::Physical(value) => {
                        if value.kind == a::physical_key_kind_down
                            && value.key == 0x46
                            && value.flags & a::physical_key_flag_repeat == 0
                        {
                            if value.modifiers
                                & (a::physical_key_modifier_left_control
                                    | a::physical_key_modifier_right_control)
                                != 0
                            {
                                unsafe {
                                    self.capture.toggle_recording(0);
                                }
                            } else {
                                if !self.diagnostics.screenshot_reserved() {
                                    unsafe {
                                        self.capture.screenshot(&[]);
                                    }
                                }
                            }
                            self.bar();
                        } else if !self.shell.blocked() {
                            self.hosted.gui_physical(env.app, session, value);
                        }
                    }
                    Sample::Mouse {
                        value,
                        remote,
                        motion,
                        remote_kind,
                    } => {
                        let was_relative = self.input.pointer_capture.relative();
                        let before = self.input.cursor;
                        if self.input.pointer_capture.mouse(
                            &mut input::pointer_capture::Native(session),
                            value,
                            remote,
                            remote_kind,
                            motion,
                            &mut self.input.cursor,
                            self.hosted.bounds,
                            time.tick,
                        ) {
                            if remote {
                                self.input.remote_buttons = value.buttons;
                            } else {
                                self.input.physical_buttons = value.buttons;
                            }
                            self.input.last_event_tick = time.tick;
                            if was_relative != self.input.pointer_capture.relative() {
                                self.restore_capture_cursor();
                                self.full();
                            }
                            if !self.input.pointer_capture.active() {
                                self.pointer_ownership_changed();
                            }
                            if before != self.input.cursor {
                                self.input.stats.cursor_moves =
                                    self.input.stats.cursor_moves.wrapping_add(1);
                                self.cursor_queued_tick = time.tick;
                                for point in [before, self.input.cursor] {
                                    let rect =
                                        output_geometry::cursor_rect(self.hosted.bounds, point);
                                    self.publisher.cursor_damage.invalidate(rect);
                                    if self.cursor.software() {
                                        self.dirty.invalidate(rect);
                                    }
                                }
                            }
                            continue;
                        }
                        self.input.last_event_tick = time.tick;
                        let before = self.input.cursor;
                        let captured = self.hosted.model.windows.pointer_window();
                        let scene = views::scene(
                            self.width,
                            self.height,
                            &self.hosted,
                            &self.shell,
                            &self.input,
                            &self.tray,
                            &self.status,
                            &self.frames.stats,
                            self.cursor.software(),
                            self.capture.recording_state(),
                            self.blink,
                        );
                        let target = input::hit::target(&scene, value.x, value.y);
                        let identity = self
                            .tray
                            .registry()
                            .hit(value.x, value.y)
                            .unwrap_or_default();
                        let actions = self.router.pointer(
                            &mut self.input,
                            &mut self.shell,
                            &mut self.hosted,
                            &self.outputs,
                            env.app,
                            session,
                            &mut self.dirty,
                            target,
                            identity,
                            self.tray.registry(),
                            value,
                            remote,
                            self.cursor.software(),
                            self.width,
                            self.height,
                            time.tick,
                            time.hz,
                        );
                        if before != self.input.cursor {
                            self.cursor_queued_tick = time.tick;
                            self.publisher
                                .cursor_damage
                                .invalidate(output_geometry::cursor_rect(
                                    self.hosted.bounds,
                                    before,
                                ));
                            self.publisher
                                .cursor_damage
                                .invalidate(output_geometry::cursor_rect(
                                    self.hosted.bounds,
                                    self.input.cursor,
                                ));
                        }
                        if let Some(index) = captured
                            .filter(|_| self.hosted.model.windows.pointer_window().is_none())
                        {
                            self.hosted.flush_geometry(index, host, time.tick, time.hz);
                        }
                        if let Some(wheel) = actions.wheel {
                            self.action(wheel.action(), env, session, host, programs, time);
                        }
                        if let Some(secondary) = actions.secondary {
                            self.action(secondary.action(), env, session, host, programs, time);
                        }
                        self.action(actions.primary, env, session, host, programs, time);
                    }
                }
            }
        }
        active
    }
    fn wait(
        &mut self,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        time: Time,
        active: bool,
    ) {
        if active || self.outputs.immediate_work() || self.primary.immediate_work() {
            // Even during continuous rendering, consume the published
            // notification sequence. Zero timeout never parks this task.
            let _ = Input::new(session).wait_activity(
                self.activity_sequence,
                0,
                &mut self.activity_sequence,
            );
            env.app.sleep(0);
            return;
        }
        let half = u64::from(time.hz).div_ceil(2).max(1);
        let mut delay = half - time.tick % half;
        delay = crate::windows::service_gate::deadline_delay(
            time.tick,
            delay,
            &[self.tray.next_tick, self.status.next_tick],
        );
        delay = self
            .shell
            .clock
            .wait_ticks(time.nanoseconds, time.hz, delay);
        delay = self.shell.audio.wait_ticks(time.tick, delay);
        delay = self
            .outputs
            .capture_wait_ticks(time.nanoseconds, time.hz, delay);
        delay = self.primary.wait_ticks(time.nanoseconds, time.hz, delay);
        if self.outputs.needs_polling()
            || self.primary.needs_polling()
            || self.windows.needs_polling()
            || self.cursor.retry_pending()
            || self.loads.busy()
            || self.capture.screenshot_busy()
            || self.capture.recording_state() != 0
            || self.layout.busy()
            || self.modes.busy()
            || self.audio_io.busy()
            || self.hosted.pending.iter().any(|p| p.queued() != 0)
        {
            delay = 1;
        }
        let code = Input::new(session).wait_activity(
            self.activity_sequence,
            delay.max(1),
            &mut self.activity_sequence,
        );
        if code < 0 {
            env.app.write("Desktop: activity wait failed\n");
            env.app.sleep(1);
        } else if code == 0 {
            self.diagnostics.idle_timeouts = self.diagnostics.idle_timeouts.wrapping_add(1);
        }
    }
    fn drain(
        &mut self,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        host: &mut Host<'_>,
        programs: &Programs<'_>,
    ) {
        let _ = self.input.pointer_capture.unbind(
            &mut input::pointer_capture::Native(session),
            &mut self.input.cursor,
            self.hosted.bounds,
        );
        self.publisher
            .reset(env.app, &mut self.outputs, self.primary.output_mut());
        self.windows.disconnect();
        self.shell.audio.stop();
        let mut reported = false;
        loop {
            let time = env.time().ok();
            let tick = time.map_or(0, |v| v.tick);
            let now = time.map_or(0, |v| v.nanoseconds);
            self.poll_loads(
                env,
                session,
                programs,
                time.unwrap_or(Time {
                    tick,
                    nanoseconds: now,
                    hz: 1000,
                }),
                true,
            );
            let diagnostic = if self.diagnostics.pending() {
                if let Some(time) = time {
                    let areas = self.areas();
                    unsafe {
                        self.hosted.model.synchronize(
                            env.app,
                            env.resources,
                            programs,
                            session,
                            &mut *self.windows,
                            &mut self.dirty,
                            time.tick,
                            time.hz,
                            0,
                            self.shell.dialog == crate::shell::state::Dialog::Tasks,
                            &areas,
                        );
                    }
                    let mut view = diagnostics::runtime::View {
                        hosted: &mut self.hosted,
                        shell: &mut self.shell,
                        dirty: &mut self.dirty,
                        tray: &self.tray,
                        input: &self.input,
                        status: &self.status,
                        work_areas: areas,
                        width: self.width,
                        height: self.height,
                        output_captures: [None; 4],
                        native_failures: (0, [None; 4]),
                    };
                    self.diagnostics.end(
                        &mut view,
                        env,
                        programs,
                        &mut self.capture,
                        time,
                        true,
                        &mut Default::default(),
                    )
                } else {
                    false
                }
            } else {
                true
            };
            let modes = host.drain_modes();
            let audio = self.audio_io.drain();
            let capture = self.capture.drain(now);
            let controls = self
                .control
                .close(&env.libraries, &mut self.outputs)
                .unwrap_or(false);
            let cursor = self.cursor.close(&mut Native(env.graphics), now);
            let mut transport = NativeTransport { host, programs };
            self.windows.poll(
                transport.host.owner(),
                [None; 4],
                &mut transport,
                &self.hosted.model.windows,
                &mut self.dirty,
            );
            let outputs = self.outputs.close(
                env.app,
                env.graphics,
                &env.libraries,
                &mut self.windows,
                now,
            );
            let primary = self
                .primary
                .close(env.app, env.graphics, &mut self.windows, now);
            let mut clients = true;
            if self.windows.detached() && outputs {
                let mut reader = SessionReader::new(session);
                for i in 0..4 {
                    clients &= self
                        .hosted
                        .model
                        .clients
                        .slot_mut(i)
                        .unwrap()
                        .frames_mut()
                        .close(&mut reader);
                }
            } else {
                clients = false;
            }
            unsafe {
                self.layout.flush(
                    env.app,
                    env.resources,
                    self.expected,
                    env.standard,
                    &self.shell.folder.items,
                    tick,
                    true,
                );
            }
            self.hosted.poll_failed_spawns(programs);
            if capture
                && modes
                && audio
                && diagnostic
                && primary
                && controls
                && cursor
                && outputs
                && clients
                && self.windows.detached()
                && !self.layout.busy()
                && self.hosted.failed_spawns_drained()
                && !self.loads.busy()
            {
                break;
            }
            if !reported {
                env.app
                    .write("Desktop: draining retained tasks and graphics receipts\n");
                reported = true;
            }
            env.app.sleep(1);
        }
    }
}

/// Ordinary SDK host admission calls this body. The manifest continues to
/// select the existing product until the complete port's acceptance.
pub struct Coordinator {
    pub expected: &'static StartupInfo<'static>,
}
impl bootstrap::Body for Coordinator {
    fn run(
        &mut self,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        graphics: &Graphics<'_, '_>,
        host: &mut Host<'_>,
        resources: &Resources<'_>,
        standard: &Standard<'_>,
        colors: &ColorV1Client<'_>,
    ) -> i32 {
        if !app.desktop_api().is_some_and(|table| {
            table.size as usize >= core::mem::size_of::<a::R4XStartR4Desk>()
                && table.gui_host_input != 0
        }) {
            app.write("Desktop: exact GUI host input API unavailable\n");
            return 2;
        }
        let env = match Environment::new(app, graphics, resources, standard, colors) {
            Ok(v) => v,
            Err(_) => {
                app.write("Desktop: SDK contexts unavailable\n");
                return 2;
            }
        };
        let time = match env.time() {
            Ok(v) => v,
            Err(_) => {
                app.write("Desktop: current clock unavailable\n");
                return 2;
            }
        };
        let mut events = Vec::new();
        if events
            .try_reserve_exact(4 * input::console::CAPACITY)
            .is_err()
        {
            app.write("Desktop: input allocation failed\n");
            return 2;
        }
        events.resize(
            4 * input::console::CAPACITY,
            a::ConsoleInputEvent::default(),
        );
        let mut run = [0; 128];
        let services = match Services::new(app) {
            Ok(v) => v,
            Err(_) => return 2,
        };
        let mut desktop_owner = match owned(
            || {
                Desktop::new(
                    &env,
                    &services,
                    self.expected,
                    host.owner(),
                    &mut events,
                    &mut run,
                    time,
                )
            },
            "desktop coordinator",
        ) {
            Ok(v) => v,
            Err(reason) => {
                app.write("Desktop: ");
                app.write(reason);
                app.write(" unavailable\n");
                return 2;
            }
        };
        let desktop = &mut *desktop_owner;
        let mut platform_storage = pin!(PlatformStorage::new());
        let platform = Platform::open(platform_storage.as_mut(), app).ok();
        let audio = platform.as_ref().and_then(|p| p.audio().ok());
        let programs = match Programs::new(app) {
            Ok(v) => v,
            Err(_) => return 2,
        };
        desktop.initialize(&env, session, time);
        // Terminal /INPUTTEST still launches this focused, standalone owner
        // check. It uses the productive console transport/snapshots below;
        // ordinary Desktop work continues through Spectra's byte contract.
        if app.args().eq_ignore_ascii_case(b"/INPUTTEST") {
            let result = console_check::run(desktop, &env, session, host, &programs);
            desktop.drain(&env, session, host, &programs);
            return result;
        }
        let mut result = 0;
        let mut announced = false;
        let test_requested = super::test_start::requested(app);
        let mut test_finished = false;
        let mut test_handoff = false;
        let mut test_owner = if test_requested {
            match super::test_start::Owner::start(&programs) {
                Ok(owner) => owner,
                Err(_) => {
                    test_failure(&env, b"controller-start");
                    test_finished = true;
                    super::test_start::Owner::default()
                }
            }
        } else {
            super::test_start::Owner::default()
        };
        while !app.should_close() {
            let time = match env.time() {
                Ok(v) => v,
                Err(_) => {
                    result = 2;
                    break;
                }
            };
            if test_handoff {
                // The explicit kernel terminal-mode receiver now renders
                // AUTOEXEC. Retire existing asynchronous owners without
                // painting the hidden Desktop over its transcript.
                desktop.shell.audio.stop();
                let _ = desktop.audio_io.drain();
                desktop.capture.poll();
                unsafe {
                    desktop.layout.flush(
                        app,
                        resources,
                        desktop.expected,
                        standard,
                        &desktop.shell.folder.items,
                        time.tick,
                        false,
                    );
                }
                desktop.hosted.poll_failed_spawns(&programs);
                app.sleep(1);
                continue;
            }
            desktop.refresh(&env, session, host, &programs, time);
            desktop.diagnostic_work(&env, session, host, &programs, audio.as_ref(), time);
            if test_owner.active() {
                match test_owner.poll(&programs) {
                    Ok(Some(code)) => {
                        if code != 0 {
                            test_failure(&env, b"controller-result");
                        }
                        test_finished = true;
                    }
                    Ok(None) => {}
                    Err(_) => {
                        test_owner.stop(&programs);
                        test_failure(&env, b"controller-completion");
                    }
                }
            }
            if test_requested && test_finished && !test_owner.active() {
                match desktop.hosted.enter_terminal_with_args(
                    &programs,
                    app,
                    session,
                    time.tick,
                    &mut desktop.dirty,
                    b"",
                ) {
                    Ok(()) => {
                        test_handoff = true;
                        continue;
                    }
                    Err(r4os_sdk::storage::Error::Raw(code))
                        if code == a::program_handle_error_would_block => {}
                    Err(_) => {
                        test_failure(&env, b"terminal-handoff");
                        result = 2;
                        break;
                    }
                }
            }
            let active = desktop.poll_input(&env, session, host, &programs, time);
            let gui_progress = desktop.hosted.flush_gui(session);
            let full = desktop
                .hosted
                .model
                .windows
                .windows()
                .get(desktop.hosted.model.windows.active())
                .filter(|w| w.fullscreen())
                .map(|w| w.geometry());
            let _ = desktop.outputs.sync_policy(
                &env.libraries,
                time.nanoseconds,
                full,
                desktop.publisher.consumers != 0,
                desktop.control.busy(),
            );
            let rendered = match desktop.render(&env, session, time) {
                Ok(v) => v,
                Err(_) => {
                    result = 2;
                    break;
                }
            };
            if !announced {
                env.boot_ready();
                announced = true;
            }
            desktop.wait(&env, session, time, active || rendered || gui_progress);
        }
        while test_owner.active() {
            test_owner.stop(&programs);
            if let Ok(time) = env.time() {
                desktop.refresh(&env, session, host, &programs, time);
                desktop.diagnostic_work(&env, session, host, &programs, audio.as_ref(), time);
            }
            let _ = test_owner.poll(&programs);
            app.sleep(1);
        }
        desktop.drain(&env, session, host, &programs);
        result
    }
}

fn test_failure(env: &Environment<'_>, reason: &[u8]) {
    let Ok(path) = FilePath::parse(b"C:\\TEMP\\SUBSYS.OK") else {
        return;
    };
    let files = env.system.files();
    let mut previous = [0; 192]; // The existing fixture's first-failure record is at most160 bytes.
    if files.read(&path, &mut previous).is_ok_and(|value| {
        value.is_some_and(|n| {
            previous[..n].starts_with(b"SUBSYSTEM ")
                && previous[..n]
                    .windows(b"FAILED".len())
                    .any(|part| part == b"FAILED")
        })
    }) {
        return;
    }
    let mut message = Vec::new();
    if message.try_reserve_exact(128).is_err() {
        return;
    }
    message.extend_from_slice(b"SUBSYSTEM runtime bootstrap FAILED: ");
    message.extend_from_slice(reason);
    message.extend_from_slice(b"\r\n");
    let _ = files.write(&path, &message);
}
