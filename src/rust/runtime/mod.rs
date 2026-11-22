//! Main-task runtime integration. State-specific owners stay outside this
//! module; platform/service adapters borrow the admitted SDK contexts.
pub(crate) mod allocation;
pub(crate) mod audio_io;
pub mod bootstrap;
pub mod desktop;
mod drag_pacing;
pub mod environment;
pub(crate) mod file_checks;
pub(crate) mod icon_io;
pub mod library;
mod mode_io;
pub mod owned;
pub mod service;
mod shell_io;
mod test_start;
#[cfg(test)]
#[path = "../../../Tests/RustNative/layout_transition.rs"]
mod layout_transition;
#[cfg(test)]
#[path = "../../../Tests/RustNative/runtime_foundation.rs"]
mod tests;
pub mod views;
pub(crate) mod wire;
pub mod worker;

pub(crate) mod graphics_wire;
pub(crate) mod launch_io;
pub(crate) mod launch_wire;
