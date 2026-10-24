//! Print Screen: one immutable CPU snapshot, BMP stream and separate joinable
//! task. Main-task polling never waits for file I/O or owns a GPU source.
use super::{
    file::{self, Output, Retained},
    source::{Loans, Source},
};
use crate::runtime::worker::{Control, Task, Worker};
use core::fmt::Write;
use r4os_sdk::{
    App, StartupInfo, abi as a, path::FilePath, resources::Resources, storage::System, time::Clock,
};
pub const DIRECTORY: &[u8] = b"C:\\SCREENSHOTS";
pub const USAGE: u32 = 0x46;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Startup,
    Clock,
    Cancelled,
    Capture,
    ImageSize,
    Directory,
    Path,
    Open,
    Write,
    Finish,
    Retirement,
}
pub struct Result {
    pub error: Option<Error>,
    pub path: [u8; 128],
    pub bytes: u64,
    pub file: Option<Retained>,
    pub loans: Loans,
}
impl Result {
    pub fn retained(&self) -> bool {
        self.file.is_some() || self.loans.retained()
    }
    pub fn retry_close(&mut self, app: &App<'_>) -> bool {
        if let Some(file) = &mut self.file {
            if file.abort(app) {
                self.file = None;
            }
        }
        if let Ok(source) = Source::new(app) {
            let _ = source.close(&mut self.loans);
        }
        !self.retained()
    }
}
pub struct Screenshot {
    raw: usize,
    expected: &'static StartupInfo<'static>,
    pub result: Result,
}
impl Screenshot {
    /// # Safety
    /// The exact loader context and code remain resident until the returned
    /// worker is joined. `expected` is the manifest-generated static record.
    pub unsafe fn start<'a>(
        app: &App<'_>,
        resources: &'a Resources<'a>,
        expected: &'static StartupInfo<'static>,
        path: &[u8],
    ) -> core::result::Result<Worker<'a, Self>, crate::runtime::worker::Error> {
        let mut job = Self {
            raw: app.start_context() as *const _ as usize,
            expected,
            result: Result {
                error: None,
                path: [0; 128],
                bytes: 0,
                file: None,
                loans: Loans::new(),
            },
        };
        if path.len() >= 128 || path.contains(&0) {
            return Err(crate::runtime::worker::Error::Native(
                r4os_sdk::storage::Error::Length,
            ));
        }
        job.result.path[..path.len()].copy_from_slice(path);
        unsafe { Worker::start(resources, job, 128 * 1024) }.map_err(|error| error.error)
    }
}
impl Task for Screenshot {
    fn run(&mut self, control: &Control) -> i32 {
        // No App, SDK mutable context, writer or pixel borrow crosses threads.
        let app = match unsafe {
            App::from_raw(
                self.raw as *const a::R4XStartContext,
                self.expected,
                c"R4DESK",
            )
        } {
            Ok(app) => app,
            Err(_) => {
                self.result.error = Some(Error::Startup);
                return -1;
            }
        };
        if let Err(error) = save(&app, control, &mut self.result) {
            self.result.error = Some(error);
        }
        if self.result.error.is_none() {
            let _ = app.write("Screenshot saved: ");
            let n = self.result.path.iter().position(|b| *b == 0).unwrap_or(128);
            if let Ok(path) = core::str::from_utf8(&self.result.path[..n]) {
                let _ = app.write(path);
            }
            let _ = app.write("\r\n");
        } else {
            let _ = app.write("Screenshot failed\r\n");
        }
        if self.result.error.is_none() { 0 } else { -1 }
    }
}
fn save(app: &App<'_>, control: &Control, result: &mut Result) -> core::result::Result<(), Error> {
    let source = Source::new(app).map_err(|_| Error::Capture)?;
    let work = (|| {
        source
            .demand(&mut result.loans)
            .map_err(|_| Error::Capture)?;
        let clock = Clock::new(app);
        let deadline = clock
            .precise_instant()
            .map_err(|_| Error::Clock)?
            .nanoseconds
            .saturating_add(5_000_000_000);
        loop {
            if control.stopped() {
                return Err(Error::Cancelled);
            }
            match source.snapshot(&mut result.loans) {
                Ok(()) => break,
                Err(_) if result.loans.info().is_some() => return Err(Error::Capture),
                Err(_) => {}
            }
            if clock
                .precise_instant()
                .map_err(|_| Error::Clock)?
                .nanoseconds
                >= deadline
            {
                return Err(Error::Capture);
            }
            app.sleep(1);
        }
        let _ = source.release_demand(&mut result.loans);
        let info = result.loans.info().ok_or(Error::Capture)?;
        if info.stride_pixels != info.width
            || info.frame_pixels as u64 != info.width as u64 * info.height as u64
        {
            return Err(Error::ImageSize);
        }
        let header = bitmap_header(info.width, info.height)?;
        let system = System::new(app).map_err(|_| Error::Open)?;
        let files = system.files();
        if result.path[0] == 0 {
            let dir = FilePath::parse(DIRECTORY).map_err(|_| Error::Path)?;
            if files.info(&dir).map_err(|_| Error::Directory)?.is_none() {
                if files.create_directory(&dir).map_err(|_| Error::Directory)?
                    != r4os_sdk::storage::Presence::Present
                {
                    return Err(Error::Directory);
                }
            }
            let now = clock
                .precise_instant()
                .map_err(|_| Error::Clock)?
                .nanoseconds;
            let mut path = PathBuffer {
                bytes: &mut result.path,
                len: 0,
            };
            write!(&mut path, "C:\\SCREENSHOTS\\SHOT-{now:x}.BMP").map_err(|_| Error::Path)?;
        }
        let len = result
            .path
            .iter()
            .position(|b| *b == 0)
            .ok_or(Error::Path)?;
        let names = file::names(&result.path[..len]).map_err(|_| Error::Path)?;
        let mut output = Output::open(&files, &names).map_err(|_| Error::Open)?;
        let saved = (|| {
            if !output.write(&header) {
                return Err(Error::Write);
            }
            let pixels = source.pixels(&result.loans).map_err(|_| Error::Capture)?;
            for chunk in pixels.chunks(64 * 1024) {
                if control.stopped() {
                    return Err(Error::Cancelled);
                }
                if !output.write(chunk) {
                    return Err(Error::Write);
                }
            }
            output.finish().map_err(|_| Error::Finish)?;
            result.bytes = output.bytes();
            Ok(())
        })();
        if !output.abort() {
            result.file = output.into_retention();
            return saved.and(Err(Error::Retirement));
        }
        saved
    })();
    if !source.close(&mut result.loans) {
        return work.and(Err(Error::Retirement));
    }
    work
}
struct PathBuffer<'a> {
    bytes: &'a mut [u8],
    len: usize,
}
impl Write for PathBuffer<'_> {
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
/// Original 54-byte 32-bit BI_RGB header: top-down, tightly packed XRGB SDR.
pub fn bitmap_header(width: u32, height: u32) -> core::result::Result<[u8; 54], Error> {
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err(Error::ImageSize);
    }
    let bytes = width as u64 * height as u64 * 4;
    if bytes > 64 * 1024 * 1024 {
        return Err(Error::ImageSize);
    }
    let mut h = [0; 54];
    h[..2].copy_from_slice(b"BM");
    h[2..6].copy_from_slice(&((bytes + 54) as u32).to_le_bytes());
    h[10..14].copy_from_slice(&54u32.to_le_bytes());
    h[14..18].copy_from_slice(&40u32.to_le_bytes());
    h[18..22].copy_from_slice(&width.to_le_bytes());
    h[22..26].copy_from_slice(&(-(height as i32)).to_le_bytes());
    h[26..28].copy_from_slice(&1u16.to_le_bytes());
    h[28..30].copy_from_slice(&32u16.to_le_bytes());
    h[34..38].copy_from_slice(&(bytes as u32).to_le_bytes());
    Ok(h)
}
