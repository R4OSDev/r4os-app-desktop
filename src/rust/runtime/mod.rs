//! Main-task runtime integration. State-specific owners stay outside this
//! module; platform/service adapters borrow the admitted SDK contexts.
pub(crate) mod allocation;
pub mod bootstrap;
pub mod desktop;
pub mod environment;
pub mod library;
mod mode_io;
pub mod owned;
pub mod service;
mod shell_io;
mod test_start;
pub mod views;
pub(crate) mod wire;
pub mod worker;
#[cfg(test)]
#[path = "../../../Tests/RustNative/runtime_foundation.rs"]
mod tests;

pub(crate) mod launch_wire;
pub(crate) mod graphics_wire;
