//! The current Spectra owner protocol. Work is explicit, generation-bound and
//! copied through WINSVC; normal painting never runs a diagnostic controller.
mod capture;
mod capture_stats;
pub mod reservation;
pub mod runtime;
pub mod snes;
pub(crate) mod software_profile;
pub mod wire;
