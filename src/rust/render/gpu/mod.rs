//! Native composition owners. Provider sessions stay on the executing thread;
//! serialized opaque device storage can move only with its owning transaction.
pub mod device;
pub mod engine;
pub mod jobs;
pub mod receipt;
pub mod resources;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Busy,
    Unsupported,
    Stale,
    Graphics,
    Limit,
    State,
    Deadline,
    Incomplete,
}
pub type Result<T> = core::result::Result<T, Error>;
pub(crate) fn accepted(status: i32) -> Result<()> {
    use r4gfx as g;
    match status {
        g::status_ok => Ok(()),
        g::status_busy | g::status_occluded => Err(Error::Busy),
        g::status_stale | g::status_suboptimal | g::status_lost => Err(Error::Stale),
        g::status_unsupported | g::status_unavailable => Err(Error::Unsupported),
        g::status_limit => Err(Error::Limit),
        _ => Err(Error::Graphics),
    }
}
