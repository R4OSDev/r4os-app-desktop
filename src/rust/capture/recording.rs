//! Explicit recorder. Runtime identity moves into each joinable worker and
//! back only after join; Finish remains process-final at Desktop shutdown.
use super::{
    encoder::{self, Codec, INTERVAL},
    file::{self, Output, Retained},
    source::{Loans, Source},
};
use crate::runtime::worker::{Control, Task, Worker};
use core::{fmt::Write, pin::pin};
use r4enc as e;
use r4os_library_helpers::media::Mux;
use r4os_sdk::{
    App, StartupInfo, abi as a,
    graphics::{Graphics, GraphicsStorage},
    path::FilePath,
    resources::Resources,
    storage::{Presence, System},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Startup,
    Unavailable,
    Clock,
    Directory,
    Capture,
    Path,
    Open,
    Write,
    Finish,
    Retirement,
    Codec(encoder::Error),
}
impl From<encoder::Error> for Error {
    fn from(e: encoder::Error) -> Self {
        Self::Codec(e)
    }
}
#[derive(Clone, Copy, Default)]
pub struct Report {
    pub ok: bool,
    pub parts: u32,
    pub frames: u64,
    pub bytes: u64,
    pub fallback: bool,
    pub max_work_ns: u64,
    pub error: Option<Error>,
}
pub struct Manager<'a> {
    app: &'a App<'a>,
    resources: &'a Resources<'a>,
    expected: &'static StartupInfo<'static>,
    runtime: Option<e::R4EncRuntime>,
    job: Option<Worker<'a, Job>>,
    retained: Option<Job>,
}
impl<'a> Manager<'a> {
    pub fn new(
        app: &'a App<'a>,
        resources: &'a Resources<'a>,
        expected: &'static StartupInfo<'static>,
    ) -> Option<Self> {
        e::EncodeV1Client::optional(app).ok()??;
        Source::new(app).ok()?;
        Some(Self {
            app,
            resources,
            expected,
            runtime: None,
            job: None,
            retained: None,
        })
    }
    pub fn active(&self) -> bool {
        self.job.is_some() || self.retained.is_some()
    }
    pub fn stopping(&self) -> bool {
        self.retained.is_some() || self.job.as_ref().is_some_and(Worker::stopping)
    }
    /// # Safety
    /// The caller drains this Manager's worker before its R4X entry returns.
    pub unsafe fn start(&mut self, adapter: u32) -> bool {
        if self.active() {
            return false;
        }
        let job = Job {
            raw: self.app.start_context() as *const _ as usize,
            expected: self.expected,
            runtime: self.runtime.take(),
            adapter,
            report: Report::default(),
            file: None,
            loans: Loans::new(),
            started: 0,
            next: 0,
            last: 0,
            part: 0,
            software_only: false,
        };
        match unsafe { Worker::start(self.resources, job, 256 * 1024) } {
            Ok(worker) => {
                self.job = Some(worker);
                true
            }
            Err(error) => {
                self.runtime = error.task.runtime;
                false
            }
        }
    }
    pub fn stop(&self) {
        if let Some(job) = &self.job {
            job.stop(encoder::now(self.app).unwrap_or(1));
        }
    }
    pub fn collect(&mut self) -> Option<Report> {
        let (_, mut job) = self.job.as_mut()?.collect().ok()??;
        self.job = None;
        self.runtime = job.runtime.take();
        let report = job.report;
        if job.retained() {
            self.retained = Some(job);
        }
        Some(report)
    }
    pub fn retry_close(&mut self) -> bool {
        if let Some(job) = &mut self.retained {
            if let Some(file) = &mut job.file {
                if file.abort(self.app) {
                    job.file = None;
                }
            }
            if let Ok(source) = Source::new(self.app) {
                let _ = source.close(&mut job.loans);
            }
            if !job.retained() {
                self.retained = None;
            }
        }
        !self.active()
    }
    /// Shutdown only, after every recording has actually joined and its exact
    /// file/snapshot identities have retired. A refused Finish retains runtime.
    pub fn finish(&mut self) -> bool {
        if self.active() {
            return false;
        }
        let Some(runtime) = self.runtime else {
            return true;
        };
        let Ok(api) = e::EncodeV1Client::init(self.app) else {
            return false;
        };
        if unsafe { api.finish(&runtime, 1_000_000_000) } != e::ok {
            return false;
        }
        self.runtime = None;
        true
    }
}
struct Job {
    raw: usize,
    expected: &'static StartupInfo<'static>,
    runtime: Option<e::R4EncRuntime>,
    adapter: u32,
    report: Report,
    file: Option<Retained>,
    loans: Loans,
    started: u64,
    next: u64,
    last: u64,
    part: u32,
    software_only: bool,
}
impl Job {
    fn retained(&self) -> bool {
        self.file.is_some() || self.loans.retained()
    }
}
impl Task for Job {
    fn run(&mut self, control: &Control) -> i32 {
        let app = match unsafe {
            App::from_raw(
                self.raw as *const a::R4XStartContext,
                self.expected,
                c"R4DESK",
            )
        } {
            Ok(app) => app,
            Err(_) => {
                self.report.error = Some(Error::Startup);
                return -1;
            }
        };
        let result = self.record(&app, control);
        if let Err(error) = result {
            self.report.error = Some(error);
            self.report.ok = false;
            let _ = app.write("Recording failed\r\n");
        }
        if self.report.ok { 0 } else { -1 }
    }
}
#[derive(Clone, Copy)]
enum Next {
    Rotate,
    Fallback,
    Stopped,
}
impl Job {
    fn record(&mut self, app: &App<'_>, control: &Control) -> Result<(), Error> {
        let api = e::EncodeV1Client::init(app).map_err(|_| Error::Unavailable)?;
        let source = Source::new(app).map_err(|_| Error::Capture)?;
        let mut storage = pin!(GraphicsStorage::new());
        let graphics = Graphics::open(storage.as_mut(), app).map_err(|_| Error::Unavailable)?;
        let result = (|| {
            if self.runtime.is_none() {
                let startup = e::R4EncStartup {
                    version: 1,
                    size: core::mem::size_of::<e::R4EncStartup>() as u32,
                    application: self.raw as u64,
                    memory_limit: encoder::MEMORY_LIMIT,
                    max_encoders: 1,
                    thread_limit: 1,
                };
                let mut runtime = e::R4EncRuntime::default();
                if unsafe { api.open(&startup, &mut runtime) } != e::ok {
                    return Err(Error::Unavailable);
                }
                self.runtime = Some(runtime);
            }
            self.started = encoder::now(app)?;
            let system = System::new(app).map_err(|_| Error::Directory)?;
            let files = system.files();
            let directory = FilePath::parse(b"C:\\RECORDINGS").map_err(|_| Error::Path)?;
            if files
                .info(&directory)
                .map_err(|_| Error::Directory)?
                .is_none()
                && files
                    .create_directory(&directory)
                    .map_err(|_| Error::Directory)?
                    != Presence::Present
            {
                return Err(Error::Directory);
            }
            source.demand(&mut self.loans).map_err(|_| Error::Capture)?;
            while !control.stopped() {
                // Cold preparation owns no capture lease. The first snapshot
                // chooses geometry/epoch; conversion begins on the next pass.
                let (info, epoch) = self.wait_first(app, &source, control)?;
                if control.stopped() {
                    break;
                }
                let mut codec = Codec::new(app, &graphics, &api);
                let opened = codec.open(
                    self.runtime.as_ref().unwrap(),
                    info.width,
                    info.height,
                    self.adapter,
                    self.software_only,
                );
                let outcome = match opened {
                    Ok(()) => {
                        if !codec.native() && self.adapter != 0 {
                            self.report.fallback = true;
                            self.software_only = true;
                        }
                        self.part(app, &source, control, &mut codec, epoch)
                    }
                    Err(error) => Err(Error::Codec(error)),
                };
                if outcome.is_err() {
                    let _ = source.release_demand(&mut self.loans);
                }
                codec.retire();
                match outcome? {
                    Next::Stopped => break,
                    Next::Rotate => {}
                    Next::Fallback => {
                        self.software_only = true;
                        self.report.fallback = true;
                        let _ = app.write(
                            "Recording: switching to software; starting a new file part.\r\n",
                        );
                    }
                }
            }
            self.report.ok = self.report.parts != 0;
            Ok(())
        })();
        if !source.close(&mut self.loans) {
            return result.and(Err(Error::Retirement));
        }
        result
    }
    fn wait_first(
        &mut self,
        app: &App<'_>,
        source: &Source<'_>,
        control: &Control,
    ) -> Result<(a::RemoteFrameInfo, u64), Error> {
        let end = self.started.saturating_add(5_000_000_000);
        loop {
            if control.stopped() {
                return Ok((a::RemoteFrameInfo::default(), 0));
            }
            if source.snapshot(&mut self.loans).is_ok() {
                let info = self.loans.info().ok_or(Error::Capture)?;
                let epoch = self.loans.epoch();
                if !source.release_snapshot(&mut self.loans) {
                    return Err(Error::Retirement);
                }
                return Ok((info, epoch));
            }
            if self.loans.info().is_some() || encoder::now(app)? >= end {
                return Err(Error::Capture);
            }
            app.sleep(1);
        }
    }
    fn part(
        &mut self,
        app: &App<'_>,
        source: &Source<'_>,
        control: &Control,
        codec: &mut Codec<'_, '_>,
        epoch: u64,
    ) -> Result<Next, Error> {
        let mut mux =
            Mux::with_color(codec.width, codec.height, 13, false).map_err(|_| Error::Open)?;
        self.part = self.part.checked_add(1).ok_or(Error::Path)?;
        let number = self.part;
        let mut bytes = [0u8; 160];
        let mut path = Name {
            bytes: &mut bytes,
            len: 0,
        };
        write!(
            &mut path,
            "C:\\RECORDINGS\\REC-{:x}-{number}.MKV",
            self.started
        )
        .map_err(|_| Error::Path)?;
        let names = file::names(&path.bytes[..path.len]).map_err(|_| Error::Path)?;
        let system = System::new(app).map_err(|_| Error::Open)?;
        let files = system.files();
        let mut output = Output::open(&files, &names).map_err(|_| Error::Open)?;
        let _ = app.write(match codec.backend {
            e::backend_amd => "Recording: AMD H.264\r\n",
            e::backend_nvidia => "Recording: NVIDIA H.264\r\n",
            _ => "Recording: software H.264\r\n",
        });
        let written = (|| {
            let next = self.frames(app, source, control, codec, epoch, &mut mux, &mut output)?;
            let time = encoder::now(app)?;
            if matches!(next, Next::Stopped) {
                let _ = source.release_demand(&mut self.loans);
                codec.drain()?;
            }
            let until = if matches!(next, Next::Stopped) {
                control.stopped_at().max(self.last.saturating_add(1))
            } else {
                time
            };
            if codec.write_pending(until, &mut mux, &mut output)? {
                self.report.frames = self.report.frames.saturating_add(1);
            }
            if mux.frames().map_err(|_| Error::Write)? != 0 {
                output.finish().map_err(|_| Error::Finish)?;
                self.report.parts = self.report.parts.saturating_add(1);
                self.report.bytes = self.report.bytes.saturating_add(output.bytes());
                let _ = app.write("Recording saved: ");
                if let Ok(path) = core::str::from_utf8(names.target.as_path().as_bytes()) {
                    let _ = app.write(path);
                }
                let _ = app.write("\r\n");
            }
            Ok(next)
        })();
        if !output.abort() {
            self.file = output.into_retention();
            return written.and(Err(Error::Retirement));
        }
        written
    }
    fn frames(
        &mut self,
        app: &App<'_>,
        source: &Source<'_>,
        control: &Control,
        codec: &mut Codec<'_, '_>,
        epoch: u64,
        mux: &mut Mux,
        output: &mut Output<'_>,
    ) -> Result<Next, Error> {
        let mut revision = None;
        loop {
            if control.stopped() {
                return Ok(Next::Stopped);
            }
            let time = encoder::now(app)?;
            if time < self.next {
                app.sleep(1);
                continue;
            }
            if source.snapshot(&mut self.loans).is_err() {
                if self.loans.info().is_some() {
                    return Err(Error::Capture);
                }
                app.sleep(1);
                continue;
            }
            let info = self.loans.info().ok_or(Error::Capture)?;
            if self.loans.epoch() != epoch
                || info.width != codec.width
                || info.height != codec.height
            {
                if !source.release_snapshot(&mut self.loans) {
                    return Err(Error::Retirement);
                }
                return Ok(Next::Rotate);
            }
            if revision == Some(info.revision) {
                if !source.release_snapshot(&mut self.loans) {
                    return Err(Error::Retirement);
                }
                self.next = time.saturating_add(INTERVAL);
                continue;
            }
            let converted = codec.convert(
                source.words(&self.loans).map_err(|_| Error::Capture)?,
                info.stride_pixels,
                control,
            );
            if !source.release_snapshot(&mut self.loans) {
                return Err(Error::Retirement);
            }
            if converted == Err(encoder::Error::Cancelled) && control.stopped() {
                return Ok(Next::Stopped);
            }
            converted?;
            revision = Some(info.revision);
            if control.stopped() {
                return Ok(Next::Stopped);
            }
            let pts = encoder::now(app)?;
            self.last = pts;
            self.next = pts.saturating_add(INTERVAL);
            match codec.encode(pts, mux, output) {
                Ok(previous) => {
                    if previous {
                        self.report.frames = self.report.frames.saturating_add(1);
                    }
                }
                Err(error) => {
                    if codec.native()
                        && matches!(
                            error,
                            encoder::Error::Encode | encoder::Error::Send | encoder::Error::Timeout
                        )
                    {
                        return Ok(Next::Fallback);
                    }
                    return Err(Error::Codec(error));
                }
            }
            self.report.max_work_ns = self
                .report
                .max_work_ns
                .max(encoder::now(app)?.saturating_sub(pts));
        }
    }
}
struct Name<'a> {
    bytes: &'a mut [u8],
    len: usize,
}
impl Write for Name<'_> {
    fn write_str(&mut self, value: &str) -> core::fmt::Result {
        let end = self
            .len
            .checked_add(value.len())
            .filter(|n| *n < self.bytes.len())
            .ok_or(core::fmt::Error)?;
        self.bytes[self.len..end].copy_from_slice(value.as_bytes());
        self.len = end;
        self.bytes[end] = 0;
        Ok(())
    }
}
