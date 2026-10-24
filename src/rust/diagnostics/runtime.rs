//! Main-task execution of the existing versioned Spectra work. Each call
//! borrows concrete domain owners; no callback receives the Desktop root.
use super::{
    reservation::Session,
    snes::{self, FinishFailure, Ledger, Trace},
    wire::{self as w, Identity, Op, Request, Response, Work},
};
use crate::{
    capture::runtime as capture,
    hosted::{self, process::Programs, runtime as hosted_runtime},
    render::{
        geometry::{Dirty, Rect},
        runtime::Stats,
    },
    runtime::{
        environment::{Environment, Time},
        service::Host,
    },
    shell::{self, state::State as Shell, tray_sync::Mirror},
    windows::controller::Changes,
};
use alloc::string::String;
use r4os_sdk::{
    abi as a, gui::Session as GuiSession, path::FilePath, platform::Audio, storage::Error,
};
pub struct View<'call, 'input> {
    pub hosted: &'call mut hosted_runtime::State<'input>,
    pub shell: &'call mut Shell<'input>,
    pub dirty: &'call mut Dirty,
    pub tray: &'call Mirror,
    pub input: &'call crate::input::runtime::State,
    pub status: &'call shell::status::Status,
    pub work_areas: [Rect; 4],
    pub width: i32,
    pub height: i32,
}
#[derive(Default)]
pub struct Effects {
    pub reset_capture: bool,
    pub reset_window_service: bool,
}
#[derive(Default)]
pub struct Owner {
    session: Session,
    pub benchmark: Ledger,
    pub input_generation: u64,
    pub idle_timeouts: u64,
    capture_override: bool,
    screenshot_path: Option<String>,
    completion: Option<(Work, Response)>,
    restart_at: u64,
    restart_state: u32,
    restart_old: u32,
    restart_result: i32,
    isolated: bool,
}
impl Owner {
    pub fn new(isolated: bool) -> Self {
        Self {
            isolated,
            ..Self::default()
        }
    }
    pub fn input(&mut self) {
        self.input_generation = self.input_generation.wrapping_add(1)
    }
    pub fn capture_override(&self) -> bool {
        self.capture_override
    }
    pub fn screenshot_reserved(&self) -> bool {
        self.screenshot_path.is_some()
    }
    pub fn pending(&self) -> bool {
        self.session.caller.valid() || self.completion.is_some() || self.restart_at != 0
    }
    pub fn poll(
        &mut self,
        view: &mut View<'_, '_>,
        env: &Environment<'_>,
        gui: &mut GuiSession<'_, '_>,
        host: &mut Host<'_>,
        programs: &Programs<'_>,
        capture: &mut capture::Owner<'_>,
        audio: Option<&Audio<'_>>,
        time: Time,
        stats: &Stats,
        cursor_moves: u64,
    ) -> Effects {
        let mut effects = Effects::default();
        if self.session.caller.valid()
            && (time.tick >= self.session.deadline || snes::gone(programs, self.session.caller))
        {
            let _ = self.end(view, env, programs, capture, time, true, &mut effects);
        }
        if self.restart_at != 0 && time.tick >= self.restart_at {
            self.restart_at = 0;
            if !self.session.caller.valid()
                || self.input_generation != self.session.input_generation
            {
                self.restart_state = 3;
                self.restart_result = a::service_api_result_busy;
            } else {
                self.restart_old = host.endpoint();
                self.restart_result = host.restart_broker();
                // The exact old connection must reject a current snapshot.
                if self.restart_result == 0 {
                    if host.window_snapshot().is_ok() {
                        self.restart_result = -1
                    }
                    host.invalidate();
                    effects.reset_window_service = true;
                    view.hosted
                        .mirror
                        .unavailable(time.tick, u64::from(time.hz) * 2);
                }
                self.restart_state = if self.restart_result == 0 { 2 } else { 3 };
            }
        }
        let Some(work) = host.diagnostic_work() else {
            if let Some((old, response)) = self.completion.take() {
                if old.request.op == Op::Restart && response.result == 0 {
                    self.restart_state = 3;
                    self.restart_result = a::service_api_result_timeout
                }
            }
            return effects;
        };
        if self
            .completion
            .as_ref()
            .is_some_and(|(old, _)| *old != work)
        {
            self.completion = None
        }
        if self.completion.is_none() {
            let response = if time.tick >= work.deadline_tick
                || host.broker_instance() != Some(work.broker.instance)
                || !snes::live(programs, work.broker)
            {
                let mut response = Response::new(work.request, Identity::of(host.owner()));
                response.result = a::service_api_result_timeout;
                response
            } else {
                self.step(
                    work.request,
                    view,
                    env,
                    gui,
                    host,
                    programs,
                    capture,
                    audio,
                    time,
                    stats,
                    cursor_moves,
                    &mut effects,
                )
            };
            self.completion = Some((work, response));
        }
        let (_, response) = self.completion.unwrap();
        if host.diagnostic_complete(work, response).is_ok() {
            self.completion = None;
            if work.request.op == Op::Restart && response.result == 0 {
                self.restart_at = time
                    .tick
                    .saturating_add(u64::from(time.hz).div_ceil(10).max(1));
            }
        }
        effects
    }
    fn step(
        &mut self,
        r: Request,
        v: &mut View<'_, '_>,
        env: &Environment<'_>,
        gui: &mut GuiSession<'_, '_>,
        host: &mut Host<'_>,
        programs: &Programs<'_>,
        capture: &mut capture::Owner<'_>,
        audio: Option<&Audio<'_>>,
        time: Time,
        stats: &Stats,
        cursor_moves: u64,
        effects: &mut Effects,
    ) -> Response {
        let mut out = Response::new(r, Identity::of(host.owner()));
        out.idle_timeouts = self.idle_timeouts;
        out.redraws = stats.redraws;
        out.present_failures = stats.present_errors;
        out.present_lost_frames = stats.present_lost_frames;
        out.input_generation = self.input_generation;
        out.service_handle = host.endpoint();
        out.restart_state = self.restart_state;
        out.restart_old_handle = self.restart_old;
        out.restart_result = self.restart_result;
        if self.isolated {
            out.flags |= w::ISOLATED
        }
        if v.hosted.mirror.gate.available() {
            out.flags |= w::SERVICE_AVAILABLE
        }
        let foreign = self.session.foreign(
            v.hosted
                .model
                .clients
                .slots()
                .iter()
                .map(|c| Identity::of(c.handle())),
            if r.op == Op::Bind {
                r.target
            } else {
                Identity::default()
            },
        );
        let interaction = v.hosted.model.windows.pointer_window().is_some()
            || v.shell.blocked()
            || v.shell.audio.ui.popup_open
            || v.shell.item_drag.is_some();
        if !self.session.admit(
            r,
            time.tick,
            time.hz,
            self.input_generation,
            snes::live(programs, r.caller),
            foreign,
            interaction,
        ) {
            out.result = a::service_api_result_busy;
            return out;
        }
        match r.op {
            Op::Begin | Op::BeginBenchmark => {
                self.restart_state = 0;
                self.restart_at = 0;
                self.restart_old = 0;
                self.restart_result = 0;
            }
            Op::End => {
                if !self.end(v, env, programs, capture, time, false, effects) {
                    out.result = a::service_api_result_busy
                }
            }
            Op::Bind => {
                let Ok(Some(info)) = programs.status(r.target.handle()) else {
                    out.result = -1;
                    return out;
                };
                if info.state != a::ProgramInstanceState::running.0
                    || info.app_class != a::ProgramInstanceClass::gui.0
                    || info.flags
                        & (a::ProgramInstanceFlag::close_requested
                            | a::ProgramInstanceFlag::desktop_host)
                        != 0
                    || !self.session.bind(r.target)
                {
                    out.result = -1;
                    return out;
                }
                let existing = v.hosted.model.clients.find(r.target.handle());
                let index = existing
                    .or_else(|| self.benchmark.trace.as_ref().map(|t| t.window_index))
                    .or_else(|| (1..4).find(|i| v.hosted.model.free(*i)));
                let Some(index) = index else {
                    out.result = a::service_api_result_full;
                    return out;
                };
                if existing.is_none() && info.window_id >= 0 {
                    out.result = -1;
                    return out;
                }
                if existing.is_none() && self.session.benchmark && self.benchmark.trace.is_some() {
                    if self.benchmark.child.valid() {
                        out.result = a::service_api_result_busy;
                        return out;
                    }
                    self.benchmark.child = r.target;
                    self.benchmark.started_ns = time.nanoseconds;
                    self.benchmark.presents_before = stats.presents;
                    self.benchmark.cursor_before = cursor_moves;
                }
                if !unsafe {
                    v.hosted.model.bind_diagnostic(
                        r.target.handle(),
                        index,
                        gui,
                        env.app,
                        time.tick,
                        v.work_areas[index],
                        v.dirty,
                    )
                } {
                    out.result = -1
                }
            }
            Op::Snapshot | Op::Drag | Op::Resize | Op::Focus | Op::Close | Op::Maximize => {
                if r.op == Op::Snapshot && !r.target.valid() {
                    return out;
                }
                if !self.session.owns(r.target) {
                    out.result = a::service_api_result_invalid;
                    return out;
                }
                let Some(index) = v.hosted.model.clients.find(r.target.handle()) else {
                    out.result = a::service_api_result_not_found;
                    return out;
                };
                if i64::from(r.dx).abs() > 128 || i64::from(r.dy).abs() > 128 {
                    out.result = -1;
                    return out;
                }
                match r.op {
                    Op::Drag | Op::Resize => {
                        let rect = v.hosted.model.windows.windows()[index].geometry();
                        let (x, y) = if r.op == Op::Drag {
                            (rect.x + 20, rect.y + 10)
                        } else {
                            (rect.right() - 1, rect.bottom() - 1)
                        };
                        let change = v.hosted.model.windows.pointer_down(index, x, y);
                        apply(v, change, env, gui, time);
                        if r.op == Op::Drag {
                            for step in 0..100 {
                                let area = v.work_areas[index];
                                let change = v.hosted.model.windows.pointer_motion(
                                    x + r.dx * step / 100,
                                    y + r.dy * step / 100,
                                    area,
                                    |rect| Some(rect.clamp_inside(area)),
                                );
                                apply(v, change, env, gui, time);
                            }
                        } else {
                            let area = v.work_areas[index];
                            let change = v.hosted.model.windows.pointer_motion(
                                x + r.dx / 2,
                                y + r.dy / 2,
                                area,
                                |rect| Some(rect.clamp_inside(area)),
                            );
                            apply(v, change, env, gui, time);
                        }
                        let area = v.work_areas[index];
                        let change = v.hosted.model.windows.pointer_release(
                            x + r.dx,
                            y + r.dy,
                            area,
                            |rect| Some(rect.clamp_inside(area)),
                        );
                        apply(v, change, env, gui, time);
                        v.hosted.flush_geometry(index, host, time.tick, time.hz);
                    }
                    Op::Focus => v.hosted.focus(index, gui, env.app, time.tick, v.dirty),
                    Op::Maximize => {
                        let change = v.hosted.model.windows.toggle_maximize(index, v.work_areas[index]);
                        apply(v, change, env, gui, time);
                        v.hosted.flush_geometry(index, host, time.tick, time.hz);
                    }
                    Op::Close => {
                        out.result = programs.request_close(r.target.handle()).err().map_or(
                            0,
                            |e| match e {
                                Error::Raw(code) => code,
                                _ => -1,
                            },
                        );
                    }
                    _ => {}
                }
                let win = &v.hosted.model.windows.windows()[index];
                let rect = win.geometry();
                out.x = rect.x;
                out.y = rect.y;
                out.w = rect.w;
                out.h = rect.h;
                out.gui_revision = hosted::gui::Host::new(gui)
                    .revision(r.target.instance)
                    .unwrap_or(0);
                if v.hosted.model.clients.slots()[index].frames().view().valid {
                    out.flags |= w::FRAME_VALID;
                }
                if geometry_matches(rect, index, r.target, host) {
                    out.flags |= w::GEOMETRY_MATCHES
                }
                if v.hosted.model.windows.active() == index {
                    out.flags |= w::FOCUSED
                }
            }
            Op::Tray => {
                let hit = v.tray.registry().entries().iter().find(|e| {
                    e.used
                        && Identity::of(e.owner).same(r.caller)
                        && e.item_id == r.item
                        && e.layout_visible
                });
                if let Some(item) = hit {
                    let x = item.rect.x + item.rect.w / 2;
                    let y = item.rect.y + item.rect.h / 2;
                    if let Some(identity) = v.tray.registry().hit(x, y) {
                        if v.tray.activate(
                            host,
                            identity,
                            a::tray_event_kind_primary,
                            0,
                            x,
                            y,
                            time.tick,
                        ) {
                            out.flags |= w::TRAY_HIT
                        } else {
                            out.result = -1
                        }
                    } else {
                        out.result = -1
                    }
                } else {
                    out.result = a::service_api_result_not_found
                }
            }
            Op::Volume => {
                if volume_targets(v) {
                    out.flags |= w::VOLUME_TARGETS
                } else {
                    out.result = -1
                }
            }
            Op::Restart => {
                if !self.isolated {
                    out.result = a::service_api_result_disabled
                } else if self.restart_state == 1 {
                    out.result = a::service_api_result_busy
                } else {
                    self.restart_state = 1;
                    self.restart_result = 0;
                    out.restart_state = 1;
                }
            }
            Op::Audio => {
                if !self.isolated {
                    out.result = a::service_api_result_disabled
                } else if !volume_targets(v)
                    || !audio.is_some_and(|audio| v.shell.audio.qualify_master(audio))
                {
                    out.result = -1
                }
            }
            Op::Capture => {
                if !self.isolated {
                    out.result = a::service_api_result_disabled
                } else if r.dx == 0 {
                    if self.capture_override {
                        self.capture_override = false;
                        effects.reset_capture = true
                    }
                } else {
                    out.result = super::capture::probe(env.app, r.dx);
                    if out.result == 0 {
                        self.capture_override = true
                    }
                }
            }
            Op::Screenshot | Op::ScreenshotStatus | Op::ScreenshotCancel => {
                if !self.isolated {
                    out.result = a::service_api_result_disabled;
                    return out;
                }
                if r.op == Op::Screenshot {
                    if self.screenshot_path.is_some() || capture.screenshot_busy() {
                        out.result = a::service_api_result_busy;
                        return out;
                    }
                    let Some(path) = w::screenshot_path(r.caller) else {
                        out.result = -1;
                        return out;
                    };
                    if unsafe { capture.screenshot(path.as_bytes()) } {
                        self.screenshot_path = Some(path)
                    } else {
                        out.result = -1;
                        return out;
                    }
                }
                if self.screenshot_path.is_some() {
                    if r.op == Op::ScreenshotCancel {
                        capture.cancel_screenshot(time.nanoseconds)
                    }
                    if capture.screenshot_busy() {
                        out.screenshot_state = 1
                    } else if let Some(last) = &capture.last {
                        out.screenshot_state = 2;
                        out.screenshot_bytes = last.bytes;
                        out.w = v.width;
                        out.h = v.height;
                        if last.error.is_some() && r.op != Op::ScreenshotCancel {
                            out.result = -1
                        }
                    } else {
                        out.result = -1
                    }
                }
            }
            Op::SnesPrepare => {
                if !self.isolated {
                    out.result = a::service_api_result_disabled;
                    return out;
                }
                let Some(options) = w::SnesOptions::decode(r.item) else {
                    out.result = a::service_api_result_invalid;
                    return out;
                };
                if !self.session.benchmark || r.target.valid() {
                    out.result = a::service_api_result_invalid;
                    return out;
                }
                if self.benchmark.trace.is_some()
                    || self.benchmark.child.valid()
                {
                    out.result = -1;
                    return out;
                }
                if snes::exists(env, b"C:\\R4OS\\SUBSYSTEMS\\r4os.snes\\R4SNES.R4X") != Ok(true) {
                    out.result = -1;
                    return out;
                }
                let Some(index) = (1..4).find(|i| v.hosted.model.free(*i)) else {
                    out.result = -1;
                    return out;
                };
                if !self.benchmark.reserve_snes(env) {
                    out.result = -1;
                    return out;
                }
                let Ok(trace) = Trace::new(index) else {
                    out.result = -1;
                    return out;
                };
                self.benchmark.options = options;
                self.benchmark.trace = Some(trace);
                let win = v.hosted.model.windows.window_mut(index).unwrap();
                win.set_normal(Rect::new(62, 40, 430, 262));
                if options.maximized {
                    win.toggle_maximize_in(v.work_areas[index]);
                }
            }
            Op::SnesFinish => {
                if !self.session.benchmark || !self.benchmark.child.same(r.target) {
                    out.result = a::service_api_result_invalid;
                    return out;
                }
                out.result = match self.finish_snes(v, env, programs, time, stats, cursor_moves) {
                    Ok(true) => 0,
                    Ok(false) => w::SNES_FINISH_PENDING,
                    Err(failure) => failure as i32,
                };
            }
            Op::SoftwareProfile => {
                if !self.session.benchmark
                    || self.benchmark.software_reserved
                    || self.benchmark.child.valid()
                {
                    out.result = a::service_api_result_busy;
                    return out;
                }
                let Some(path) = w::software_path(r.caller) else {
                    out.result = -1;
                    return out;
                };
                if snes::exists(env, path.as_bytes()) != Ok(false) {
                    out.result = a::service_api_result_busy;
                    return out;
                }
                self.benchmark.software_reserved = true;
                if super::software_profile::run(env, path.as_bytes(), r.caller, r.deadline_tick)
                    .is_err()
                {
                    out.result = -1
                }
            }
        }
        out
    }
    fn finish_snes(
        &mut self,
        v: &View<'_, '_>,
        env: &Environment<'_>,
        programs: &Programs<'_>,
        time: Time,
        stats: &Stats,
        cursor_moves: u64,
    ) -> Result<bool, FinishFailure> {
        if self.benchmark.report_ready {
            return Ok(true);
        }
        let Some(trace) = &self.benchmark.trace else {
            return Err(FinishFailure::MissingTrace);
        };
        if !matches!(programs.status(self.benchmark.child.handle()),Ok(Some(info)) if info.state==a::ProgramInstanceState::done.0 && info.exit_code==0)
        {
            return Err(FinishFailure::ChildNotComplete);
        }
        let client = &v.hosted.model.clients.slots()[trace.window_index];
        if client.bound() {
            if !Identity::of(client.handle()).same(self.benchmark.child) {
                return Err(FinishFailure::ClientIdentity);
            }
            // Process completion precedes exact native-image retirement.
            // Keep the real client/graphics owners until detach acknowledges it.
            return Ok(false);
        }
        if !client.completion().is_some_and(|c| {
                Identity::of(c.handle).same(self.benchmark.child) && c.exit_code == 0
            })
        {
            return Err(FinishFailure::ClientCompletion);
        }
        let report = trace.report(snes::Report {
            maximized: self.benchmark.options.maximized,
            process: self.benchmark.child,
            screen_w: v.width as u32,
            screen_h: v.height as u32,
            started_ns: self.benchmark.started_ns,
            ended_ns: time.nanoseconds,
            presents: stats.presents.wrapping_sub(self.benchmark.presents_before),
            cursor_moves: cursor_moves.wrapping_sub(self.benchmark.cursor_before),
            tick_hz: u64::from(time.hz),
        });
        let Ok(report) = report else { return Err(FinishFailure::TraceInvalid) };
        let Ok(path) = FilePath::parse(w::SNES_PATHS[2]) else {
            return Err(FinishFailure::ReportPath);
        };
        if env.system.files().write(&path, report.as_bytes()) != Ok(report.len()) {
            return Err(FinishFailure::ReportWrite);
        }
        self.benchmark.report_ready = true;
        Ok(true)
    }
    pub fn end(
        &mut self,
        v: &mut View<'_, '_>,
        env: &Environment<'_>,
        programs: &Programs<'_>,
        capture: &mut capture::Owner<'_>,
        time: Time,
        abandoned: bool,
        effects: &mut Effects,
    ) -> bool {
        self.restart_at = 0;
        if self.capture_override {
            self.capture_override = false;
            effects.reset_capture = true
        }
        if let Some(path) = &self.screenshot_path {
            capture.cancel_screenshot(time.nanoseconds);
            if capture.screenshot_busy() || !snes::remove(env, path.as_bytes()) {
                return false;
            }
            self.screenshot_path = None;
        }
        if self.benchmark.child.valid() && !snes::gone(programs, self.benchmark.child) {
            let _ = programs.request_close(self.benchmark.child.handle());
            return false;
        }
        self.benchmark.trace = None;
        if !self.benchmark.cleanup(env, self.session.caller) {
            return false;
        }
        let mut retired = true;
        for target in self.session.targets().iter().copied().filter(|v| v.valid()) {
            match programs.status(target.handle()) {
                Ok(None) => {}
                Err(Error::Raw(code)) if hosted::process::gone(code) => {}
                Ok(Some(info)) if abandoned => {
                    if info.state == a::ProgramInstanceState::done.0
                        && snes::gone(programs, self.session.caller)
                    {
                        if !matches!(
                            unsafe { programs.reap_host_owned(target.handle()) },
                            Ok(r4os_sdk::resources::Wait::Completed(_))
                        ) {
                            retired = false
                        }
                    } else {
                        let _ = programs.request_close(target.handle());
                        retired = false
                    }
                }
                _ => retired = false,
            }
            if v.hosted.model.clients.find(target.handle()).is_some() {
                retired = false
            }
        }
        if !retired {
            return false;
        }
        self.benchmark = Ledger::default();
        self.session = Session::default();
        true
    }
}
fn apply(
    v: &mut View<'_, '_>,
    change: Changes,
    env: &Environment<'_>,
    gui: &mut GuiSession<'_, '_>,
    time: Time,
) {
    v.hosted.changes(&change, gui, env.app, time.tick, v.dirty)
}
fn geometry_matches(expected: Rect, index: usize, target: Identity, host: &mut Host<'_>) -> bool {
    let Ok(snapshot) = host.window_snapshot() else {
        return false;
    };
    snapshot.records[..snapshot.status.window_count as usize]
        .iter()
        .any(|record| {
            record.window_id == index as u32
                && record.instance_id == target.instance
                && expected == Rect::new(record.x, record.y, record.w, record.h)
        })
}
fn volume_targets(v: &mut View<'_, '_>) -> bool {
    if !v.shell.config.taskbar_clock || !v.shell.audio.ui.installed {
        return false;
    }
    let old = v.shell.audio.ui.popup_open;
    v.shell.audio.ui.popup_open = true;
    let popup =
        shell::status_view::volume_popup_rect(v.width, v.height, v.shell.config.taskbar_clock);
    let track = shell::volume::track_rect(popup);
    let mute = shell::volume::mute_rect(popup);
    let icon = shell::taskbar_view::volume_rect(v.width, v.height, true);
    let clock = shell::taskbar_view::clock_rect(v.width, v.height);
    let keyboard = shell::taskbar_view::layout_rect(v.width, v.height, true, true);
    let scene = crate::runtime::views::scene(
        v.width,
        v.height,
        v.hosted,
        v.shell,
        v.input,
        v.tray,
        v.status,
        &Stats::default(),
        true,
        0,
        true,
    );
    let target = |r: Rect| crate::input::hit::target(&scene, r.x + r.w / 2, r.y + r.h / 2);
    use crate::input::event::Target;
    let valid = icon.right() + shell::volume::ICON_GAP == clock.x
        && keyboard.right() + shell::taskbar_view::LAYOUT_GAP == icon.x
        && target(track) == Target::VolumePopupSlider
        && target(mute) == Target::VolumePopupMute
        && target(icon) == Target::TaskbarVolume
        && crate::input::hit::target(&scene, 0, 0) == Target::VolumePopupBackdrop;
    v.shell.audio.ui.popup_open = old;
    valid
}
