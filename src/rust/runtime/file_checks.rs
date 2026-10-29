//! Periodic file checks use the existing asynchronous file owner. Directory
//! hints are RAM-only polls; admission of their cursors occurs at startup.
//! No reply or byte buffer is consumed before the exact native Close ACK.
use super::service::{Error, Result};
use crate::shell::clock::{CONFIG_BYTES, Clock, PATH as TIME_FILE};
use alloc::{boxed::Box, vec::Vec};
use r4os_sdk::{
    abi as a,
    path::FilePath,
    resources::{IoBuffers, IoControl, IoRequest, OwnedIo, Resources},
    storage::{Error as SdkError, Files},
};
#[cfg(test)]
#[path = "../../../Tests/RustNative/file_checks.rs"]
mod tests;
const SECOND: u64 = 1_000_000_000;
const HALF_SECOND: u64 = SECOND / 2;
// FileRead returns the original file API codes through async I/O unchanged:
// -3 is missing; -5 is an oversized file, unlike FileInfo's IO_NOT_FOUND.
const READ_NOT_FOUND: i32 = a::file_stream_error_not_found;
const READ_TOO_LARGE: i32 = -5;
const AUDIO_FILE: &[u8] = b"C:\\R4OS\\SERVICES\\AUDSVC.R4X";
const CONFIG_DIR: &[u8] = b"C:\\R4OS\\CONFIG";
const SERVICES_DIR: &[u8] = b"C:\\R4OS\\SERVICES";

fn buffer() -> Result<Box<[u8]>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(CONFIG_BYTES + 1)
        .map_err(|_| Error::OutOfMemory)?;
    bytes.resize(CONFIG_BYTES + 1, 0);
    Ok(bytes.into_boxed_slice())
}
#[derive(Default)]
struct Check {
    cursor: a::DirectoryChangeCursor,
    watched: bool,
    dirty: bool,
    epoch: u64,
    next_hint: u64,
    retry_after: u64,
}
impl Check {
    fn initialize(
        &mut self,
        files: &Files<'_>,
        directory: &[u8],
        dirty: bool,
        now: u64,
        interval: u64,
    ) {
        self.watched = FilePath::parse(directory).ok().is_some_and(|path| {
            files
                .begin_directory_changes(&path, &mut self.cursor)
                .is_ok()
        });
        self.dirty = dirty;
        self.epoch = self.epoch.wrapping_add(1);
        self.next_hint = now.saturating_add(interval);
        self.retry_after = 0;
    }
    fn invalidate(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.dirty = true;
        self.retry_after = 0;
    }
    fn poll(&mut self, files: &Files<'_>, now: u64, interval: u64, pending: bool) {
        if now < self.next_hint {
            return;
        }
        self.next_hint = now.saturating_add(interval);
        if self.watched {
            match files.poll_directory_changes(&mut self.cursor) {
                Ok(0) => {}
                Ok(_) => self.invalidate(),
                Err(_) => {
                    self.watched = false;
                    self.invalidate();
                }
            }
        } else if !pending {
            // A fallback timer is not evidence that an in-flight snapshot is
            // obsolete. Slow replies must still be allowed to finish.
            self.dirty = true;
        }
    }
    fn retry(&mut self, now: u64, interval: u64) {
        self.dirty = true;
        self.retry_after = now.saturating_add(interval);
    }
}
enum Request<'a> {
    Read(OwnedIo<'a>),
    Info(IoRequest<'a, 'static>),
}
impl Request<'_> {
    fn id(&self) -> u32 {
        match self {
            Self::Read(r) => r.id(),
            Self::Info(r) => r.id(),
        }
    }
    fn kind(&self) -> u32 {
        match self {
            Self::Read(_) => a::io_kind_file_read,
            Self::Info(_) => a::io_kind_file_info,
        }
    }
    fn valid(&self) -> bool {
        match self {
            Self::Read(r) => r.valid(),
            Self::Info(r) => r.valid(),
        }
    }
    fn status(&mut self) -> core::result::Result<a::ProgramIoInfo, SdkError> {
        match self {
            Self::Read(r) => r.status(),
            Self::Info(r) => r.status(),
        }
    }
    fn close(&mut self) {
        let _ = match self {
            Self::Read(r) => r.close(),
            Self::Info(r) => r.close(),
        };
    }
}
struct Flight<'a> {
    request: Request<'a>,
    epoch: u64,
    terminal: Option<a::ProgramIoInfo>,
    failed: bool,
}
fn retire<'a>(slot: &mut Option<Flight<'a>>, owner: u32, stopping: bool) -> Option<Flight<'a>> {
    let flight = slot.as_mut()?;
    if !stopping && flight.terminal.is_none() && !flight.failed {
        match flight.request.status() {
            Ok(info)
                if info.request_id != flight.request.id()
                    || info.kind != flight.request.kind()
                    || info.owner_instance != owner =>
            {
                flight.failed = true
            }
            Ok(info) if info.state == a::io_state_pending || info.state == a::io_state_running => {
                return None;
            }
            Ok(info) if info.state == a::io_state_completed || info.state == a::io_state_failed => {
                flight.terminal = Some(info)
            }
            Ok(_) => flight.failed = true,
            Err(SdkError::Raw(code)) if code == a::io_error_busy => return None,
            Err(_) => flight.failed = true,
        }
    }
    flight.request.close();
    if flight.request.valid() {
        return None;
    }
    slot.take()
}
fn success(info: &a::ProgramIoInfo) -> bool {
    info.state == a::io_state_completed && info.status == 0 && info.result >= 0
}
pub struct Owner<'a> {
    owner: u32,
    time: Check,
    audio: Check,
    read: Option<Flight<'a>>,
    info: Option<Flight<'a>>,
    bytes: Option<Box<[u8]>>,
    installed: Option<bool>,
    visible: bool,
    last_ns: u64,
    stopping: bool,
}
impl<'a> Owner<'a> {
    pub fn new(owner: u32) -> Result<Self> {
        Ok(Self {
            owner,
            time: Check::default(),
            audio: Check::default(),
            read: None,
            info: None,
            bytes: Some(buffer()?),
            installed: None,
            visible: false,
            last_ns: 0,
            stopping: false,
        })
    }
    pub fn initialize(&mut self, files: &Files<'_>, visible: bool, now: u64) {
        self.visible = visible;
        self.last_ns = now;
        // Bind both hints only at startup. The first visible async snapshot
        // closes the gap between the startup load and cursor admission; a
        // successful load is not evidence of the file's current version.
        self.time.initialize(files, CONFIG_DIR, true, now, SECOND);
        self.audio
            .initialize(files, SERVICES_DIR, true, now, HALF_SECOND);
    }
    pub fn busy(&self) -> bool {
        self.read.is_some() || self.info.is_some()
    }
    pub fn refresh(
        &mut self,
        resources: &'a Resources<'a>,
        files: &Files<'_>,
        clock: &mut Clock,
        visible: bool,
        now: u64,
    ) -> Option<bool> {
        clock.set_visible(visible);
        if visible && !self.visible {
            self.time.invalidate();
        }
        self.visible = visible;
        if now < self.last_ns {
            self.time.next_hint = 0;
            self.audio.next_hint = 0;
            self.time.invalidate();
            self.audio.invalidate();
        }
        self.last_ns = now;
        if !self.stopping {
            if visible {
                self.time.poll(files, now, SECOND, self.read.is_some());
            }
            self.audio
                .poll(files, now, HALF_SECOND, self.info.is_some());
        }
        if let Some(mut done) = retire(&mut self.read, self.owner, self.stopping) {
            if let Request::Read(io) = &mut done.request {
                if let Ok(IoBuffers::Bytes(bytes)) = io.take_buffers() {
                    if !self.stopping && done.epoch == self.time.epoch {
                        match done.terminal.filter(|_| !done.failed) {
                            Some(info)
                                if success(&info)
                                    && info.requested_bytes == bytes.len() as u64
                                    && info.processed_bytes == info.result as u64
                                    && info.processed_bytes <= bytes.len() as u64 =>
                            {
                                if clock
                                    .apply_bytes(&bytes[..info.processed_bytes as usize])
                                    .is_err()
                                {
                                    self.time.retry(now, SECOND);
                                }
                            }
                            Some(info)
                                if info.result == READ_NOT_FOUND
                                    || info.result == READ_TOO_LARGE => {}
                            _ => self.time.retry(now, SECOND),
                        }
                    }
                    self.bytes = Some(bytes);
                } else if !self.stopping {
                    self.time.retry(now, SECOND);
                }
            }
        }
        if let Some(done) = retire(&mut self.info, self.owner, self.stopping) {
            if !self.stopping && done.epoch == self.audio.epoch {
                match done.terminal.filter(|_| !done.failed) {
                    Some(info) if success(&info) => self.installed = Some(true),
                    Some(info) if info.result == a::io_error_not_found => {
                        self.installed = Some(false)
                    }
                    _ => self.audio.retry(now, HALF_SECOND),
                }
            }
        }
        if !self.stopping {
            if visible && self.time.dirty && self.read.is_none() && now >= self.time.retry_after {
                if self.bytes.is_none() {
                    self.bytes = buffer().ok();
                }
                if let (Some(bytes), Ok(path)) = (self.bytes.take(), FilePath::parse(TIME_FILE)) {
                    match resources.io_read(&path, bytes, None, 0) {
                        Ok(io) => {
                            self.time.dirty = false;
                            self.read = Some(Flight {
                                request: Request::Read(io),
                                epoch: self.time.epoch,
                                terminal: None,
                                failed: false,
                            });
                        }
                        Err(error) => {
                            if let IoBuffers::Bytes(bytes) = error.buffers {
                                self.bytes = Some(bytes);
                            }
                            self.time.retry(now, SECOND);
                        }
                    }
                } else {
                    self.time.retry(now, SECOND);
                }
            }
            if self.audio.dirty && self.info.is_none() && now >= self.audio.retry_after {
                match FilePath::parse(AUDIO_FILE)
                    .ok()
                    .and_then(|path| resources.io_control(&path, IoControl::FileInfo, 0).ok())
                {
                    Some(io) => {
                        self.audio.dirty = false;
                        self.info = Some(Flight {
                            request: Request::Info(io),
                            epoch: self.audio.epoch,
                            terminal: None,
                            failed: false,
                        });
                    }
                    None => self.audio.retry(now, HALF_SECOND),
                }
            }
        }
        self.installed
    }
    pub fn drain(&mut self) -> bool {
        self.stopping = true;
        if let Some(mut done) = retire(&mut self.read, self.owner, true) {
            if let Request::Read(io) = &mut done.request {
                if let Ok(IoBuffers::Bytes(bytes)) = io.take_buffers() {
                    self.bytes = Some(bytes);
                }
            }
        }
        let _ = retire(&mut self.info, self.owner, true);
        !self.busy()
    }
}
