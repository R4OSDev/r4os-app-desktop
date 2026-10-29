//! Main-task launch admission/completion. Providers receive only copied work;
//! window generations, frame leases, focus and reap authority stay here.
use super::*;
use crate::hosted::{
    launch::{self, Failure},
    process::valid,
};
use crate::runtime::launch_wire as wire;
use core::ffi::CStr;

pub(super) enum Accepted {
    Pending,
    Window(usize),
}
impl<'a, 'input> Desktop<'a, 'input> {
    pub(super) fn begin_load(
        &mut self,
        request: Launch,
        origin: Option<wire::Message>,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        _programs: &Programs<'_>,
        time: Time,
    ) -> core::result::Result<Accepted, Failure> {
        // Path syntax is cheap local validation. Header classification is I/O
        // and belongs exclusively to the copied worker request.
        launch::path(request.path()).map_err(Failure::Path)?;
        if request.policy != a::LaunchPolicy::console {
            if let Some(index) = self.hosted.single_instance(request.path()) {
                self.hosted
                    .focus(index, session, env.app, time.tick, &mut self.dirty);
                return Ok(Accepted::Window(index));
            }
            let cancelled = core::array::from_fn(|i| self.hosted.model.launch_cancelled(i));
            if let Some(index) = self.loads.pending_path(request.path(), origin, cancelled) {
                self.hosted
                    .focus(index, session, env.app, time.tick, &mut self.dirty);
                return Ok(Accepted::Pending);
            }
        }
        if !self.hosted.launch_capacity() {
            return Err(Failure::WindowCapacity {
                console: request.policy == a::LaunchPolicy::console,
            });
        }
        let primary = request.policy != a::LaunchPolicy::gui
            && (request.path().eq_ignore_ascii_case(hosted::TERMINAL)
                || request
                    .path()
                    .eq_ignore_ascii_case(b"/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X"));
        let index = if primary && self.hosted.model.free(0) {
            Some(0)
        } else {
            (1..4).find(|&i| self.hosted.model.free(i)).or_else(|| {
                (request.policy != a::LaunchPolicy::gui && self.hosted.model.free(0)).then_some(0)
            })
        }
        .ok_or(Failure::WindowCapacity {
            console: request.policy == a::LaunchPolicy::console,
        })?;
        let ticket = self.loads.ticket();
        if !self.hosted.model.reserve_launch(index, ticket) {
            return Err(Failure::WindowCapacity {
                console: request.policy == a::LaunchPolicy::console,
            });
        }
        let mut caption = [0; 41];
        let title = if request.title().is_empty() {
            b"Starting...".as_slice()
        } else {
            request.title()
        };
        caption[..title.len()].copy_from_slice(title);
        self.hosted
            .model
            .windows
            .window_mut(index)
            .unwrap()
            .begin_launch(CStr::from_bytes_until_nul(&caption).unwrap());
        self.hosted
            .focus(index, session, env.app, time.tick, &mut self.dirty);
        self.bar();
        if !unsafe {
            self.loads.start(
                env.resources,
                env.app,
                self.expected,
                index,
                ticket,
                request,
                self.hosted.terminal_mode,
                origin,
            )
        } {
            self.release_load(index, ticket);
            if let Some(change) = self.hosted.model.windows.close_unbound(index) {
                self.hosted
                    .changes(&change, session, env.app, time.tick, &mut self.dirty);
            }
            return Err(Failure::Loader(r4os_sdk::storage::Error::InvalidProvider));
        }
        Ok(Accepted::Pending)
    }
    fn release_load(&mut self, index: usize, ticket: u64) -> bool {
        if !self.hosted.model.release_launch(index, ticket) {
            return false;
        }
        let win = self.hosted.model.windows.window_mut(index).unwrap();
        self.dirty.invalidate(win.frame_surface().rect);
        self.bar();
        true
    }
    pub(super) fn poll_loads(
        &mut self,
        env: &Environment<'a>,
        session: &mut Session<'_, '_>,
        programs: &Programs<'_>,
        time: Time,
        draining: bool,
    ) {
        for index in 0..4 {
            let cancelled = draining || self.hosted.model.launch_cancelled(index);
            if cancelled {
                self.loads.stop(index, time.nanoseconds);
            }
            let Some(mut done) = self.loads.collect(index) else {
                continue;
            };
            let focused = self.hosted.model.windows.active() == index;
            let minimized = self.hosted.model.windows.windows()[index].minimized();
            let exact = self.release_load(index, done.ticket);
            let cancelled = cancelled || !exact;
            let mut process = None;
            let mut window = None;
            let mut error = None;
            let mut code = if cancelled {
                11
            } else if done.load.expired {
                9
            } else {
                0
            };
            match done.load.result.take() {
                Some(Ok(loaded)) => {
                    assert!(valid(loaded.handle));
                    if let Some(origin) = done.origin {
                        self.launches
                            .retain_evidence(programs, origin, loaded.handle);
                    }
                    process = Some(loaded.handle);
                    if cancelled {
                        // Join returned the actual child. Its exact generation
                        // stays in the existing retrying kill/reap owner.
                        self.hosted.discard_loaded(loaded.handle);
                    } else {
                        let args = CStr::from_bytes_until_nul(&loaded.args).unwrap().to_bytes();
                        match self.hosted.accept_spawned(
                            programs,
                            env.app,
                            session,
                            index,
                            loaded.handle,
                            loaded.console,
                            done.request.path(),
                            args,
                            done.request.title(),
                            time.tick,
                            focused && !minimized,
                            &mut self.dirty,
                        ) {
                            Ok(index) => {
                                window = Some(index);
                                if minimized {
                                    self.hosted
                                        .model
                                        .windows
                                        .window_mut(index)
                                        .unwrap()
                                        .minimize();
                                }
                            }
                            Err(value) => error = Some(value),
                        }
                    }
                }
                Some(Err(value)) => {
                    if !cancelled {
                        error = Some(value);
                    }
                }
                None => {
                    if code == 0 {
                        error = Some(Failure::Loader(r4os_sdk::storage::Error::InvalidProvider));
                    }
                }
            }
            if exact && window.is_none() {
                if let Some(change) = self.hosted.model.windows.close_unbound(index) {
                    self.hosted
                        .changes(&change, session, env.app, time.tick, &mut self.dirty);
                }
            }
            let mut raw = 0;
            if let Some(value) = &error {
                (code, raw) = external::failure_code(value);
            }
            if !draining {
                if let Some(origin) = done.origin {
                    self.launches
                        .complete(programs, origin, process, window, code, raw);
                } else if let Some(error) = error {
                    self.launch_error(error);
                }
            }
        }
    }
}
