//! Main-task coordination of the four hosted clients. Painting borrows copied
//! console/text snapshots; process synchronization never spans that paint loan.
use super::{
    client::Origin,
    console_snapshot::{Key, Reader, Snapshot},
    frame_reader::SessionReader,
    gui::Host,
    model::{self, GraphicsRetirement, Model},
    process::Programs,
    text_surface::Surface,
};
use crate::{
    input::console::{CAPACITY, Pending},
    render::{
        compositor,
        geometry::{Dirty, Rect},
    },
    shell::{configuration::bytes_z, style::TASKBAR_H},
    windows::{controller::Changes, service_mirror::Mirror, window::Kind},
};
use alloc::vec::Vec;
use core::ffi::CStr;
use r4os_sdk::{
    App, abi as a,
    gui::Session,
    path::FilePath,
    resources::Resources,
    storage::{Error, Result},
};
pub const TERMINAL: &[u8] = b"C:\\R4OS\\SOFTWARE\\TERMINAL\\TERMINAL.R4X";
pub const DESKTOP_ARGS: &[u8] = b"/NOAUTOEXEC";
#[derive(Clone, Copy, Default)]
pub struct Scroll {
    pub owner: a::ProgramProcessHandle,
    pub offset: u32,
    pub follow_tail: bool,
    pub clear_count: u32,
}
pub struct ConsoleLaunch {
    title: [u8; 32],
    path: [u8; 160],
    args: [u8; 128],
}
fn copy(out: &mut [u8], value: &[u8]) -> bool {
    if value.len() >= out.len() || value.contains(&0) {
        return false;
    }
    out.fill(0);
    out[..value.len()].copy_from_slice(value);
    true
}
impl Default for ConsoleLaunch {
    fn default() -> Self {
        let mut v = Self {
            title: [0; 32],
            path: [0; 160],
            args: [0; 128],
        };
        v.set(b"Terminal", TERMINAL, DESKTOP_ARGS);
        v
    }
}
impl ConsoleLaunch {
    fn set(&mut self, title: &[u8], path: &[u8], args: &[u8]) {
        copy(&mut self.title, title);
        copy(&mut self.path, path);
        copy(&mut self.args, args);
    }
}
#[derive(Debug)]
pub enum OpenError {
    Memory,
    Sdk(Error),
    Render(crate::render::primitive_assets::Error),
}
impl From<Error> for OpenError {
    fn from(v: Error) -> Self {
        Self::Sdk(v)
    }
}
impl From<crate::render::primitive_assets::Error> for OpenError {
    fn from(v: crate::render::primitive_assets::Error) -> Self {
        Self::Render(v)
    }
}
fn filename(path: &[u8]) -> &[u8] {
    path.rsplit(|&b| b == b'\\' || b == b'/')
        .next()
        .unwrap_or(&[])
}
pub struct State<'input> {
    pub model: Model,
    pub consoles: Vec<Snapshot>,
    pub texts: [Surface; 4],
    pub pending: Vec<Pending<'input>>,
    reader: Reader,
    pub scroll: [Scroll; 4],
    pub terminal_mode: bool,
    pub console: ConsoleLaunch,
    pub mirror: Mirror,
    pub bounds: Rect,
    reset: Option<crate::windows::service_mirror::ResetAcknowledged>,
    focus_dirty: bool,
    failed_spawns: Vec<a::ProgramProcessHandle>,
}
impl<'input> State<'input> {
    pub fn new(input: &'input mut [a::ConsoleInputEvent]) -> core::result::Result<Self, OpenError> {
        if input.len() != 4 * CAPACITY {
            return Err(Error::Length.into());
        }
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(4)
            .map_err(|_| OpenError::Memory)?;
        let mut failed_spawns = Vec::new();
        failed_spawns
            .try_reserve_exact(4)
            .map_err(|_| OpenError::Memory)?;
        for chunk in input.chunks_exact_mut(CAPACITY) {
            pending.push(Pending::new(chunk.try_into().map_err(|_| Error::Length)?)?);
        }
        let mut consoles = Vec::new();
        consoles
            .try_reserve_exact(4)
            .map_err(|_| OpenError::Memory)?;
        for _ in 0..4 {
            consoles.push(Snapshot::new()?);
        }
        Ok(Self {
            model: Model::default(),
            consoles,
            texts: core::array::from_fn(|_| Surface::default()),
            pending,
            reader: Reader::new()?,
            scroll: [Scroll::default(); 4],
            terminal_mode: false,
            console: ConsoleLaunch::default(),
            mirror: Mirror::default(),
            bounds: Rect::new(0, 0, 1280, 720),
            reset: None,
            focus_dirty: false,
            failed_spawns,
        })
    }
    pub fn view(&self, font: u32, blink: bool) -> compositor::Hosted<'_> {
        compositor::Hosted {
            windows: &self.model.windows,
            clients: &self.model.clients,
            consoles: self
                .consoles
                .as_slice()
                .try_into()
                .expect("four retained console slots"),
            texts: &self.texts,
            console_title: bytes_z(&self.console.title),
            console_path: bytes_z(&self.console.path),
            console_args: bytes_z(&self.console.args),
            font,
            terminal_mode: self.terminal_mode,
            blink,
        }
    }
    pub fn changes(
        &mut self,
        change: &Changes,
        session: &mut Session<'_, '_>,
        app: &App<'_>,
        tick: u64,
        dirty: &mut Dirty,
    ) {
        if let Some(before) = change.previous_frame {
            dirty.invalidate(before);
        }
        for (i, w) in self.model.windows.windows().iter().enumerate() {
            if change.dirty & (1 << i) != 0 {
                dirty.invalidate(w.frame_surface().rect);
            }
        }
        if change.taskbar {
            let area = self.bounds;
            dirty.invalidate(Rect::new(
                area.x,
                area.bottom() - TASKBAR_H,
                area.w,
                TASKBAR_H,
            ));
        }
        if change.full_damage {
            dirty.invalidate(self.bounds);
        }
        model::announce(&self.model.windows, change, session, app, tick);
        self.focus_dirty |= change.focus_mirror.is_some();
        for i in 0..4 {
            if change.geometry & (1 << i) != 0 {
                self.mirror.queue_geometry(i, tick, 1);
            }
        }
    }
    pub fn synchronize_mirror<M: super::window_image::Memory>(
        &mut self,
        host: &mut crate::runtime::service::Host<'_>,
        graphics: &mut super::window_set::Set<M, crate::render::gpu::receipt::Consumer>,
        tick: u64,
        hz: u32,
    ) -> bool {
        use crate::windows::service_mirror::Outcome;
        let retry = u64::from(hz).max(1) * 2;
        if !host.claimed() && self.mirror.gate.available() {
            self.mirror.unavailable(tick, retry);
            graphics.disconnect();
        }
        if self.mirror.gate.retry_due(tick, true) {
            self.reset = self.mirror.reset(host, tick, retry);
        }
        if let Some(proof) = &self.reset {
            if !graphics.acknowledge_reset(proof, host) {
                return false;
            }
            self.reset = None;
            self.focus_dirty = true;
        }
        if !self.mirror.gate.available() {
            return false;
        }
        let mut changed = false;
        for index in 0..4 {
            let win = &self.model.windows.windows()[index];
            let client = &self.model.clients.slots()[index];
            // Preserve the current browser's local authoritative host path.
            if filename(client.launch_path()).eq_ignore_ascii_case(b"KLICKIFAX.R4X") {
                continue;
            }
            let operation = if win.instance_id() == 0 {
                if !self.mirror.mirrored(index) {
                    continue;
                }
                a::window_service_op_remove
            } else if !self.mirror.mirrored(index) {
                a::window_service_op_register
            } else if self.mirror.geometry_due(index, tick, false) {
                a::window_service_op_update
            } else {
                continue;
            };
            match self.mirror.send(
                host,
                index,
                operation,
                win,
                index == self.model.windows.active(),
                self.terminal_mode,
                client.launch_path(),
                bytes_z(&self.console.path),
                tick,
                retry,
            ) {
                Outcome::Acknowledged => changed = true,
                Outcome::Unavailable => {
                    graphics.disconnect();
                    return changed;
                }
                Outcome::Skipped => {}
            }
        }
        if self.focus_dirty {
            let index = self.model.windows.active();
            let win = &self.model.windows.windows()[index];
            if win.instance_id() != 0 && self.mirror.mirrored(index) {
                match self.mirror.send(
                    host,
                    index,
                    a::window_service_op_focus,
                    win,
                    true,
                    self.terminal_mode,
                    self.model.clients.slots()[index].launch_path(),
                    bytes_z(&self.console.path),
                    tick,
                    retry,
                ) {
                    Outcome::Acknowledged => self.focus_dirty = false,
                    Outcome::Unavailable => graphics.disconnect(),
                    Outcome::Skipped => {}
                }
            }
        }
        changed
    }
    /// A release carries the final coordinate even without preceding motion.
    /// Publish that exact geometry immediately through the existing mirror.
    pub fn flush_geometry(
        &mut self,
        index: usize,
        host: &mut crate::runtime::service::Host<'_>,
        tick: u64,
        hz: u32,
    ) -> bool {
        if index >= 4 || !self.mirror.geometry_due(index, tick, true) {
            return true;
        }
        matches!(
            self.mirror.send(
                host,
                index,
                a::window_service_op_update,
                &self.model.windows.windows()[index],
                index == self.model.windows.active(),
                self.terminal_mode,
                self.model.clients.slots()[index].launch_path(),
                bytes_z(&self.console.path),
                tick,
                u64::from(hz) * 2
            ),
            crate::windows::service_mirror::Outcome::Acknowledged
        )
    }
    /// # Safety
    /// Caller holds the actual current Desktop host claim and keeps exact
    /// process/resource retirement in `graphics` through every detach.
    pub unsafe fn synchronize(
        &mut self,
        app: &App<'_>,
        resources: &Resources<'_>,
        programs: &Programs<'_>,
        session: &mut Session<'_, '_>,
        graphics: &mut impl GraphicsRetirement,
        dirty: &mut Dirty,
        tick: u64,
        hz: u32,
        areas: &[Rect; 4],
        lifecycle_due: bool,
        font_size: u8,
        codepage: u16,
    ) -> bool {
        self.poll_failed_spawns(programs);
        let last = app
            .system_api()
            .filter(|s| s.program_status != 0)
            .map_or(0, |s| {
                let call: a::R4SysFns::program_status =
                    unsafe { core::mem::transmute(s.program_status) };
                let mut status = a::ProgramStatus::default();
                unsafe { call(&mut status) };
                status.last_exit_code
            });
        let report = if lifecycle_due {
            unsafe {
                self.model.synchronize(
                    app,
                    resources,
                    programs,
                    session,
                    graphics,
                    dirty,
                    tick,
                    u64::from(hz) * 2,
                    last,
                    areas,
                )
            }
        } else {
            model::Report::default()
        };
        self.focus_dirty |= report.focus_mirror.is_some();
        if report.leave_terminal_mode {
            self.terminal_mode = false;
            dirty.invalidate(self.bounds);
        }
        for i in 0..4 {
            if report.retired[i].is_some() {
                self.texts[i].clear();
                self.consoles[i].invalidate();
                self.scroll[i] = Scroll::default();
            }
        }
        let frames = self.model.refresh_frames(app, session, areas, dirty, tick);
        let mut changed = report.changed || frames.changed;
        for i in 0..4 {
            let win = &self.model.windows.windows()[i];
            let handle = self.model.clients.slots()[i].handle();
            let _ = self.pending[i].bind(handle);
            let _ = self.pending[i]
                .flush(|owner, events| crate::input::native::push_console(app, owner, events));
            if handle.instance_id == 0 || !win.visible() || win.minimized() {
                continue;
            }
            if win.kind() == Kind::Terminal {
                if let Ok(revision) = Host::new(session).console_revision(handle.instance_id) {
                    let rect = if self.terminal_mode && i == 0 {
                        self.bounds
                    } else {
                        win.client_surface().rect
                    };
                    let scroll = &mut self.scroll[i];
                    if !super::process::same(scroll.owner, handle) {
                        *scroll = Scroll {
                            owner: handle,
                            follow_tail: true,
                            ..Default::default()
                        };
                    }
                    let key = Key {
                        owner: handle,
                        revision,
                        width: rect.w,
                        height: rect.h,
                        font_size,
                        codepage,
                        scroll_offset: if scroll.follow_tail { 0 } else { scroll.offset },
                    };
                    if !self.consoles[i].matches(key) {
                        let previous_cursor = self.consoles[i].cursor_damage(rect);
                        let refresh = self.reader.refresh(session, &mut self.consoles[i], key);
                        scroll.offset = self.consoles[i].effective_offset;
                        if self.consoles[i].state.clear_count != scroll.clear_count {
                            scroll.clear_count = self.consoles[i].state.clear_count;
                            scroll.follow_tail = true;
                            scroll.offset = 0;
                        }
                        self.consoles[i].invalidate_refresh(rect, previous_cursor, &refresh, dirty);
                        changed = true;
                    }
                }
            } else if win.kind() == Kind::App
                && self.model.clients.slots()[i]
                    .frames()
                    .view()
                    .commands
                    .is_empty()
            {
                if self.texts[i].refresh(session, handle, win.gui_revision()) {
                    dirty.invalidate(win.client_surface().rect);
                    changed = true;
                }
            }
        }
        changed
    }
    pub fn invalidate_console_cursors(&self, dirty: &mut Dirty) {
        for i in 0..4 {
            let win = &self.model.windows.windows()[i];
            let visible = if self.terminal_mode {
                i == 0 && win.instance_id() != 0
            } else {
                win.kind() == Kind::Terminal && win.visible() && !win.minimized()
            };
            if !visible
                || !super::process::same(
                    self.consoles[i].key.owner,
                    self.model.clients.slots()[i].handle(),
                )
            {
                continue;
            }
            let rect = if self.terminal_mode {
                self.bounds
            } else {
                win.client_surface().rect
            };
            if let Some(cursor) = self.consoles[i].cursor_damage(rect) {
                dirty.invalidate(cursor);
            }
        }
    }
    pub fn focus(
        &mut self,
        index: usize,
        session: &mut Session<'_, '_>,
        app: &App<'_>,
        tick: u64,
        dirty: &mut Dirty,
    ) {
        let change = self.model.windows.focus_or_restore(index);
        self.changes(&change, session, app, tick, dirty);
    }
    pub fn launch(
        &mut self,
        programs: &Programs<'_>,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        path: &[u8],
        args: &[u8],
        title: &[u8],
        policy: a::LaunchPolicy,
        tick: u64,
        dirty: &mut Dirty,
    ) -> Result<usize> {
        if self.failed_spawns.len() == 4 {
            return Err(Error::Raw(a::program_handle_error_would_block));
        }
        let name = FilePath::parse(path).map_err(|_| Error::InvalidProvider)?;
        let class = programs.class(&name, policy)?;
        let console = class == a::R4XStartAppClass::console;
        if !console {
            if let Some(existing) = self.single_instance(path) {
                self.focus(existing, session, app, tick, dirty);
                return Ok(existing);
            }
        }
        let primary = console
            && (path.eq_ignore_ascii_case(TERMINAL)
                || path.eq_ignore_ascii_case(b"/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X"));
        let index = if primary && self.model.free(0) {
            Some(0)
        } else {
            (1..4)
                .find(|&i| self.model.free(i))
                .or_else(|| (console && self.model.free(0)).then_some(0))
        }
        .ok_or(Error::Raw(a::program_handle_error_would_block))?;
        // The normal primary window suppresses AUTOEXEC. Explicit terminal
        // mode may intentionally pass empty arguments for the Test handoff.
        let args = if primary && args.is_empty() && !self.terminal_mode {
            DESKTOP_ARGS
        } else {
            args
        };
        let mut arg_z = [0; 128];
        let mut title_z = [0; 41];
        if !copy(&mut arg_z, args) || !copy(&mut title_z, title) {
            return Err(Error::Length);
        }
        let handle = programs.spawn(
            &name,
            CStr::from_bytes_until_nul(&arg_z).map_err(|_| Error::InvalidProvider)?,
            policy,
            console.then_some(if index == 0 && self.terminal_mode {
                a::ConsoleHostKind::terminal_mode
            } else {
                a::ConsoleHostKind::terminal_window
            }),
        )?;
        // Reap authority is retained even if old frame retirement refuses this
        // otherwise free slot. Never lose a successfully spawned generation.
        if !self.model.clients.slot_mut(index).unwrap().bind(
            handle,
            Origin::Spawned,
            path,
            &mut SessionReader::new(session),
        ) {
            self.failed_spawns.push(handle);
            return Err(Error::InvalidProvider);
        }
        let title = CStr::from_bytes_until_nul(&title_z).map_err(|_| Error::InvalidProvider)?;
        let win = self.model.windows.window_mut(index).unwrap();
        if console {
            win.bind_console(handle.instance_id, title);
        } else {
            win.bind_app(handle.instance_id, title);
        }
        let _ = Host::new(session).associate(&handle, index);
        if !console {
            let change = Changes {
                geometry: 1 << index,
                resized: 1 << index,
                ..Default::default()
            };
            self.changes(&change, session, app, tick, dirty);
        }
        if console && index == 0 {
            self.console.set(title.to_bytes(), path, args);
        }
        self.scroll[index] = Scroll {
            owner: handle,
            follow_tail: true,
            ..Default::default()
        };
        self.focus(index, session, app, tick, dirty);
        Ok(index)
    }
    pub fn failed_spawns_drained(&self) -> bool {
        self.failed_spawns.is_empty()
    }
    pub fn enter_terminal(
        &mut self,
        programs: &Programs<'_>,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        tick: u64,
        dirty: &mut Dirty,
    ) -> Result<()> {
        self.enter_terminal_with_args(programs, app, session, tick, dirty, DESKTOP_ARGS)
    }
    pub fn enter_terminal_with_args(
        &mut self,
        programs: &Programs<'_>,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        tick: u64,
        dirty: &mut Dirty,
        args: &[u8],
    ) -> Result<()> {
        if self.model.clients.slots()[1..].iter().any(|c| c.bound()) {
            return Err(Error::Raw(a::program_handle_error_would_block));
        }
        if self.model.clients.slots()[0].bound() {
            let path = self.model.clients.slots()[0].launch_path();
            if !path.eq_ignore_ascii_case(TERMINAL)
                && !path.eq_ignore_ascii_case(b"/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X")
            {
                return Err(Error::InvalidProvider);
            }
            let code = Host::new(session).console_host(
                self.model.clients.slots()[0].handle().instance_id,
                a::ConsoleHostKind::terminal_mode,
            );
            if code < 0 {
                return Err(Error::Raw(code));
            }
        } else {
            self.terminal_mode = true;
            if let Err(error) = self.launch(
                programs,
                app,
                session,
                TERMINAL,
                args,
                b"Terminal Mode",
                a::LaunchPolicy::console,
                tick,
                dirty,
            ) {
                self.terminal_mode = false;
                return Err(error);
            }
        }
        self.console.set(b"Terminal Mode", TERMINAL, args);
        self.model
            .windows
            .window_mut(0)
            .unwrap()
            .set_title(c"Terminal Mode");
        self.terminal_mode = true;
        self.focus(0, session, app, tick, dirty);
        dirty.invalidate(self.bounds);
        Ok(())
    }
    pub fn poll_failed_spawns(&mut self, programs: &Programs<'_>) {
        for index in (0..self.failed_spawns.len()).rev() {
            let handle = self.failed_spawns[index];
            let gone = match programs.status(handle) {
                Ok(None) => true,
                Err(Error::Raw(code)) if super::process::gone(code) => true,
                Ok(Some(info)) if info.state == a::ProgramInstanceState::done.0 => {
                    match unsafe { programs.reap_host_owned(handle) } {
                        Ok(r4os_sdk::resources::Wait::Completed(_)) => true,
                        Err(Error::Raw(code)) if super::process::gone(code) => true,
                        _ => false,
                    }
                }
                _ => {
                    let _ = programs.kill(handle);
                    false
                }
            };
            if gone {
                self.failed_spawns.swap_remove(index);
            }
        }
    }
    fn single_instance(&self, path: &[u8]) -> Option<usize> {
        let name = filename(path);
        if ![
            b"NETCFG.R4X".as_slice(),
            b"CLOCK.R4X",
            b"TIMESET.R4X",
            b"REGEDIT.R4X",
            b"CALC.R4X",
            b"APPDEF.R4X",
            b"LOGCENTER.R4X",
        ]
        .iter()
        .any(|v| name.eq_ignore_ascii_case(v))
        {
            return None;
        }
        (1..4).find(|&i| {
            let c = &self.model.clients.slots()[i];
            c.bound() && !c.retiring() && filename(c.launch_path()).eq_ignore_ascii_case(name)
        })
    }
    pub fn close_window(
        &mut self,
        index: usize,
        programs: &Programs<'_>,
        session: &mut Session<'_, '_>,
        app: &App<'_>,
        tick: u64,
        dirty: &mut Dirty,
    ) {
        let Some(client) = self.model.clients.slots().get(index) else {
            return;
        };
        if !client.bound() {
            return;
        }
        let handle = client.handle();
        let _ = programs.request_close(handle);
        self.model
            .windows
            .window_mut(index)
            .unwrap()
            .request_close(tick);
        let event = a::GuiEvent {
            kind: a::GuiEventKind::close.0,
            window_id: index as i32,
            tick,
            ..Default::default()
        };
        if self.model.windows.windows()[index].kind() == Kind::App {
            Host::new(session).push(handle.instance_id, &event, true, || app.sleep(1));
        }
        dirty.invalidate(self.model.windows.windows()[index].frame_surface().rect);
    }
}
