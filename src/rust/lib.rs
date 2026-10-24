#![cfg_attr(not(test), no_std)]
//! Modular no_std/alloc R4OS Desktop. The normal module manifest and general
//! SDK own startup, imports, build inputs and the R4M0 artifact.
extern crate alloc;
#[cfg(feature = "desktop-entry")]
mod entry;
pub mod capture;
pub mod diagnostics;
pub mod hosted;
pub mod input;
pub mod outputs;
pub mod render;
pub mod runtime;
pub mod shell;
#[cfg(test)]
#[path = "../../Tests/RustNative/standard.rs"]
pub(crate) mod standard_fixture;
pub mod windows;
