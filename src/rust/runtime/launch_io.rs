//! Copied loader work. The Desktop's model/session never cross this boundary;
//! exact native join returns the only mutable task loan and any child handle.
use super::{
    launch_wire as wire,
    worker::{Control, Task, Worker},
};
use crate::{
    hosted::{
        launch::{self, Failure},
        process::Programs,
        runtime::{DESKTOP_ARGS, TERMINAL},
    },
    shell::launch::Launch,
};
use core::ffi::CStr;
use r4os_sdk::{App, StartupInfo, abi as a, resources::Resources, storage::Error, time::Clock};

pub struct Loaded {
    pub handle: a::ProgramProcessHandle,
    pub console: bool,
    pub args: [u8; 128],
}
pub struct Load {
    request: Launch,
    raw: usize,
    expected: &'static StartupInfo<'static>,
    index: usize,
    terminal_mode: bool,
    deadline: Option<u64>,
    pub result: Option<core::result::Result<Loaded, Failure>>,
    pub expired: bool,
}
impl Task for Load {
    fn run(&mut self, control: &Control) -> i32 {
        if control.stopped() {
            return 0;
        }
        let app = match unsafe {
            App::from_raw(
                self.raw as *const a::R4XStartContext,
                self.expected,
                c"R4DESK",
            )
        } {
            Ok(app) => app,
            Err(_) => {
                self.result = Some(Err(Failure::Loader(Error::InvalidProvider)));
                return -1;
            }
        };
        let result = (|| {
            let programs = Programs::new(&app).map_err(Failure::Loader)?;
            let path = launch::path(self.request.path()).map_err(Failure::Path)?;
            control.publish_phase(1);
            let class = programs
                .class(&path, self.request.policy)
                .map_err(Failure::Classification)?;
            let console = class == a::R4XStartAppClass::console;
            if !console && self.index == 0 {
                return Err(Failure::WindowCapacity { console });
            }
            let primary = console
                && (self.request.path().eq_ignore_ascii_case(TERMINAL)
                    || self
                        .request
                        .path()
                        .eq_ignore_ascii_case(b"/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X"));
            let args = if primary && self.request.args().is_empty() && !self.terminal_mode {
                DESKTOP_ARGS
            } else {
                self.request.args()
            };
            let mut copied = [0; 128];
            copied[..args.len()].copy_from_slice(args);
            let args = CStr::from_bytes_until_nul(&copied).map_err(|_| Failure::Arguments)?;
            if control.stopped() {
                return Ok(None);
            }
            if let Some(deadline) = self.deadline {
                if Clock::new(&app)
                    .ticks()
                    .ok()
                    .is_none_or(|now| now >= deadline)
                {
                    self.expired = true;
                    return Ok(None);
                }
            }
            control.publish_phase(2);
            let handle = programs
                .spawn(
                    &path,
                    args,
                    self.request.policy,
                    console.then_some(if self.index == 0 && self.terminal_mode {
                        a::ConsoleHostKind::terminal_mode
                    } else {
                        a::ConsoleHostKind::terminal_window
                    }),
                )
                .map_err(Failure::Loader)?;
            // Cancellation/timeout after this point never makes the handle
            // disappear. Main adopts it or retains its exact kill/reap duty.
            Ok(Some(Loaded {
                handle,
                console,
                args: copied,
            }))
        })();
        control.publish_phase(3);
        self.result = match result {
            Ok(Some(value)) => Some(Ok(value)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        };
        if self.result.as_ref().is_some_and(Result::is_err) {
            -1
        } else {
            0
        }
    }
}
struct Job<'a> {
    ticket: u64,
    request: Launch,
    origin: Option<wire::Message>,
    worker: Worker<'a, Load>,
}
pub struct Completed {
    pub index: usize,
    pub ticket: u64,
    pub request: Launch,
    pub origin: Option<wire::Message>,
    pub load: Load,
}
pub struct Owner<'a> {
    next: u64,
    jobs: [Option<Job<'a>>; 4],
}
impl Default for Owner<'_> {
    fn default() -> Self {
        Self {
            next: 0,
            jobs: core::array::from_fn(|_| None),
        }
    }
}
impl<'a> Owner<'a> {
    pub fn busy(&self) -> bool {
        self.jobs.iter().any(Option::is_some)
    }
    pub fn loading(&self) -> usize {
        self.jobs
            .iter()
            .flatten()
            .filter(|job| job.worker.phase() == 2 && !job.worker.finished())
            .count()
    }
    pub fn pending(&self) -> usize {
        self.jobs.iter().flatten().count()
    }
    pub fn ticket(&mut self) -> u64 {
        loop {
            self.next = self.next.wrapping_add(1).max(1);
            if !self
                .jobs
                .iter()
                .flatten()
                .any(|job| job.ticket == self.next)
            {
                return self.next;
            }
        }
    }
    /// # Safety
    /// Desktop entry/code/context remain resident until all jobs really join.
    pub unsafe fn start(
        &mut self,
        resources: &'a Resources<'a>,
        app: &App<'_>,
        expected: &'static StartupInfo<'static>,
        index: usize,
        ticket: u64,
        request: Launch,
        terminal_mode: bool,
        origin: Option<wire::Message>,
    ) -> bool {
        if self.jobs[index].is_some() {
            return false;
        }
        let task = Load {
            request: request.clone(),
            raw: app.start_context() as *const a::R4XStartContext as usize,
            expected,
            index,
            terminal_mode,
            deadline: origin.map(|v| v.deadline),
            result: None,
            expired: false,
        };
        match unsafe { Worker::start(resources, task, 256 * 1024) } {
            Ok(worker) => {
                self.jobs[index] = Some(Job {
                    ticket,
                    request,
                    origin,
                    worker,
                });
                true
            }
            Err(_) => false,
        }
    }
    pub fn stop(&self, index: usize, now: u64) {
        if let Some(job) = &self.jobs[index] {
            job.worker.stop(now);
        }
    }
    pub fn stop_all(&self, now: u64) {
        for i in 0..4 {
            self.stop(i, now);
        }
    }
    pub fn pending_path(
        &mut self,
        path: &[u8],
        origin: Option<wire::Message>,
        cancelled: [bool; 4],
    ) -> Option<usize> {
        let name = crate::hosted::runtime::single_instance_name(path);
        if !name {
            return None;
        }
        for (index, job) in self.jobs.iter_mut().enumerate() {
            if let Some(job) = job {
                fn name(value: &[u8]) -> &[u8] {
                    value
                        .rsplit(|b| *b == b'/' || *b == b'\\')
                        .next()
                        .unwrap_or(value)
                }
                if !cancelled[index]
                    && !job.worker.stopping()
                    && name(job.request.path()).eq_ignore_ascii_case(name(path))
                {
                    if origin.is_some() {
                        job.origin = origin;
                    }
                    return Some(index);
                }
            }
        }
        None
    }
    pub fn collect(&mut self, index: usize) -> Option<Completed> {
        let job = self.jobs[index].as_mut()?;
        // Timeout/error never releases a worker allocation or its unknown child.
        let (_, load) = job.worker.collect().ok()??;
        let job = self.jobs[index].take().unwrap();
        Some(Completed {
            index,
            ticket: job.ticket,
            request: job.request,
            origin: job.origin,
            load,
        })
    }
}
