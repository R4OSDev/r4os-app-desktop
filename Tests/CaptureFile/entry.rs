//! Focused real-filesystem consumer of the current Rust capture owner.
//! Materialize as a temporary SDK app with exact current capture/file.rs,
//! render/content_hash.rs and the normal manifest C companions.
//! Screenshot/recording workers have their current Spectra/native owners.
#![no_std]
extern crate alloc;
mod capture_file;
mod render { pub mod content_hash; }
use capture_file::{names, FinishError, Output, Phase};
use r4os_sdk::{App, path::FilePath, storage::{AtomicReplace, Files, Presence, System}};

fn absent(files: &Files<'_>, path: &FilePath) -> Result<(), &'static str> {
    match files.info(path) {
        Ok(None) => Ok(()),
        _ => Err("unexpected-file-or-info-error"),
    }
}
fn exact(files: &Files<'_>, path: &FilePath, expected: &[u8]) -> Result<(), &'static str> {
    let info = files.info(path).map_err(|_| "info")?.ok_or("missing")?;
    let mut bytes = [0; 64];
    if info.size as u64 != expected.len() as u64 ||
        files.read(path, &mut bytes).map_err(|_| "read")? != Some(expected.len()) ||
        bytes[..expected.len()] != *expected { return Err("exact-bytes"); }
    Ok(())
}
fn exercise(app: &App<'_>) -> Result<(), &'static str> {
    let system = System::new(app).map_err(|_| "system")?;
    let files = system.files();
    let conflict = names(b"C:\\TEMP\\C86F-CONFLICT.BMP").map_err(|_| "conflict-names")?;
    let foreign = names(b"C:\\TEMP\\C86F-FOREIGN.BMP").map_err(|_| "foreign-names")?;
    let partial = names(b"C:\\TEMP\\C86F-PARTIAL.BMP").map_err(|_| "partial-names")?;
    let published = names(b"C:\\TEMP\\C86F-PUBLISHED.BMP").map_err(|_| "published-names")?;
    for paths in [&conflict, &foreign, &partial, &published] {
        for path in [&paths.target, &paths.stage, &paths.backup] { absent(&files, path.as_path())?; }
    }

    let mut out = Output::open(&files, &conflict).map_err(|_| "open-conflict")?;
    if !out.write(b"BMstage") { return Err("write-conflict"); }
    if files.write(conflict.target.as_path(), b"FOREIGN-TARGET").map_err(|_| "create-target")? != 14 {
        return Err("create-target-bytes");
    }
    if out.finish() != Err(FinishError::Publish) || out.phase != Phase::Finished || !out.retained() ||
        out.failure != Some(capture_file::Failure::Publish(AtomicReplace::Conflict)) {
        return Err("retained-create-only-conflict");
    }
    if !out.abort() || out.retained() { return Err("exact-abort"); }
    exact(&files, conflict.target.as_path(), b"FOREIGN-TARGET")?;
    absent(&files, conflict.stage.as_path())?;
    absent(&files, conflict.backup.as_path())?;
    if files.write(foreign.stage.as_path(), b"FOREIGN-STAGE").map_err(|_| "foreign-stage")? != 13 {
        return Err("foreign-stage-bytes");
    }
    if Output::open(&files, &foreign).is_ok() { return Err("foreign-stage-admitted"); }
    exact(&files, foreign.stage.as_path(), b"FOREIGN-STAGE")?;

    let mut out = Output::open(&files, &partial).map_err(|_| "open-partial")?;
    if !out.write(b"partial") || !out.abort() || out.retained() { return Err("partial-abort"); }
    absent(&files, partial.target.as_path())?;
    absent(&files, partial.stage.as_path())?;
    let mut out = Output::open(&files, &published).map_err(|_| "open-publish")?;
    if !out.write(b"BMRUST") || out.finish().is_err() || out.phase != Phase::Published || out.retained() {
        return Err("publication");
    }
    exact(&files, published.target.as_path(), b"BMRUST")?;
    absent(&files, published.stage.as_path())?;
    absent(&files, published.backup.as_path())?;
    for path in [&conflict.target, &foreign.stage, &published.target] {
        if files.delete(path.as_path()).map_err(|_| "cleanup")? != Presence::Present { return Err("cleanup-ack"); }
    }
    Ok(())
}
fn run(app: &mut App<'_>) -> i32 {
    match exercise(app) {
        Ok(()) => { app.write("CAPTURE86: real current Rust stage/flush/conflict/retained abort/foreign refusal/partial cleanup/publication OK\n"); 0 }
        Err(reason) => { app.write("CAPTURE86 FAILED: "); app.write(reason); app.write("\n"); 1 }
    }
}
r4os_sdk::app!(run);
