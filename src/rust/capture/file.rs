//! Create-only capture publication. A borrowed stable path/SDK writer keeps
//! the exact identity through finish, publication and explicit abort ACK.
use crate::render::content_hash::Blake3;
use r4os_sdk::{
    abi as a,
    path::{AbsoluteFilePath, Error as PathError, FILE_PATH_MAX},
    storage::{AtomicReplace, Error as SdkError, Files, StreamWriter},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Writing,
    Finished,
    Published,
    Aborted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Write(SdkError),
    Finish(SdkError),
    Publish(AtomicReplace),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishError {
    State,
    Flush,
    Publish,
}
#[derive(Clone)]
pub struct Names {
    pub target: AbsoluteFilePath,
    pub stage: AbsoluteFilePath,
    pub backup: AbsoluteFilePath,
}
pub fn names(path: &[u8]) -> core::result::Result<Names, PathError> {
    let target = AbsoluteFilePath::parse(path)?;
    let value = target.as_path().as_bytes();
    let parent = &value[..value
        .iter()
        .rposition(|&b| b == b'\\')
        .ok_or(PathError::WrongKind)?];
    // Private8.3 stage names use the already linked original BLAKE3 owner.
    // Filenames never establish ownership; create/retained-publish/abort do.
    // The target name and its bytes retain their existing product contract.
    let mut hash = Blake3::new();
    hash.update(value);
    let hash = hash.finish();
    let mut scratch = [0u8; FILE_PATH_MAX + 1];
    if parent.len() + 13 > FILE_PATH_MAX {
        return Err(PathError::TooLong);
    }
    scratch[..parent.len()].copy_from_slice(parent);
    scratch[parent.len()] = b'\\';
    for (i, b) in hash[..4].iter().enumerate() {
        scratch[parent.len() + 1 + i * 2] = b"0123456789abcdef"[(b >> 4) as usize];
        scratch[parent.len() + 2 + i * 2] = b"0123456789abcdef"[(b & 15) as usize];
    }
    let end = parent.len() + 13;
    scratch[parent.len() + 9..end].copy_from_slice(b".TMP");
    let stage = AbsoluteFilePath::parse(&scratch[..end])?;
    scratch[parent.len() + 9..end].copy_from_slice(b".BAK");
    let backup = AbsoluteFilePath::parse(&scratch[..end])?;
    if value.eq_ignore_ascii_case(stage.as_path().as_bytes())
        || value.eq_ignore_ascii_case(backup.as_path().as_bytes())
    {
        return Err(PathError::WrongKind);
    }
    Ok(Names {
        target,
        stage,
        backup,
    })
}
pub struct Output<'a> {
    paths: &'a Names,
    writer: StreamWriter<'a>,
    pub phase: Phase,
    pub failure: Option<Failure>,
    pub cleanup_error: Option<SdkError>,
}
impl<'a> Output<'a> {
    pub fn open(files: &'a Files<'a>, paths: &'a Names) -> core::result::Result<Self, SdkError> {
        // Ordinary CREATE atomically refuses a pre-existing stage. The lease
        // open API can replace a private name and is inappropriate before we
        // own any identity. A failed begin never authorizes an abort/delete.
        let writer = files
            .stream_writer(paths.stage.as_path(), a::file_stream_open_create)
            .map_err(|e| e.error)?;
        Ok(Self {
            paths,
            writer,
            phase: Phase::Writing,
            failure: None,
            cleanup_error: None,
        })
    }
    pub fn bytes(&self) -> u64 {
        self.writer.offset()
    }
    pub fn retained(&self) -> bool {
        matches!(self.phase, Phase::Writing | Phase::Finished)
    }
    fn fail(&mut self, failure: Failure) {
        if self.failure.is_none() {
            self.failure = Some(failure);
        }
    }
    pub fn write(&mut self, bytes: &[u8]) -> bool {
        if self.phase != Phase::Writing || self.failure.is_some() {
            return false;
        }
        match self.writer.write(bytes) {
            Ok(()) => true,
            Err(e) => {
                self.fail(Failure::Write(e));
                false
            }
        }
    }
    pub fn finish(&mut self) -> core::result::Result<(), FinishError> {
        if self.phase != Phase::Writing || self.failure.is_some() {
            return Err(FinishError::State);
        }
        if let Err(e) = self.writer.finish_keep_ownership() {
            self.fail(Failure::Finish(e));
            return Err(FinishError::Flush);
        }
        self.phase = Phase::Finished;
        let result = self
            .writer
            .publish_create(self.paths.target.as_path(), self.paths.backup.as_path());
        if result == AtomicReplace::Committed {
            self.phase = Phase::Published;
            Ok(())
        } else {
            self.fail(Failure::Publish(result));
            Err(FinishError::Publish)
        }
    }
    /// Only the actual native abort acknowledgement retires this identity.
    /// Missing/refusal retains the original phase and primary failure even
    /// when the SDK's native convenience writer has cleared its active bit.
    pub fn abort(&mut self) -> bool {
        if !self.retained() {
            return true;
        }
        match self.writer.abort() {
            Ok(()) => {
                self.phase = Phase::Aborted;
                self.cleanup_error = None;
                true
            }
            Err(e) => {
                self.cleanup_error = Some(e);
                false
            }
        }
    }
    /// Move the unresolved identity into the completed worker's owned result.
    /// The native convenience writer has no destructor and no retained pointer
    /// is used after this move; retry uses the exact owned stage path.
    pub fn into_retention(self) -> Option<Retained> {
        self.retained().then(|| Retained {
            retired: false,
            paths: self.paths.clone(),
            bytes: self.bytes(),
            failure: self.failure,
            cleanup_error: self.cleanup_error,
        })
    }
}
#[must_use = "retry the exact stage abort; a completed worker is not file retirement"]
pub struct Retained {
    retired: bool,
    paths: Names,
    pub bytes: u64,
    pub failure: Option<Failure>,
    pub cleanup_error: Option<SdkError>,
}
impl Retained {
    pub fn stage(&self) -> &r4os_sdk::path::FilePath {
        self.paths.stage.as_path()
    }
    pub fn abort(&mut self, app: &r4os_sdk::App<'_>) -> bool {
        if self.retired {
            return true;
        }
        let Some(table) = app.system_api() else {
            self.cleanup_error = Some(SdkError::Unavailable);
            return false;
        };
        if table.file_stream_abort == 0 {
            self.cleanup_error = Some(SdkError::Unavailable);
            return false;
        }
        let abort: unsafe extern "C" fn(*const u8) -> i32 =
            unsafe { core::mem::transmute(table.file_stream_abort) };
        let raw = unsafe { abort(self.paths.stage.as_path().as_c_str().as_ptr().cast()) };
        if raw == 0 {
            self.retired = true;
            self.cleanup_error = None;
            true
        } else {
            self.cleanup_error = Some(SdkError::Raw(raw));
            false
        }
    }
}
#[cfg(test)]
#[path = "../../../Tests/RustNative/capture_file.rs"]
mod tests;
