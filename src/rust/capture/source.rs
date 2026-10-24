//! Current R4DESK immutable CPU snapshots. Demand and snapshot identities are
//! independent; failed release leaves the exact identity available for retry.
use core::marker::PhantomData;
use r4os_sdk::{App, abi as a};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unavailable,
    Native(i32),
    InvalidSnapshot,
}
type Demand = unsafe extern "C" fn() -> i32;
type Acquire = unsafe extern "C" fn(u32, *mut a::RemoteFrameInfo, *mut a::RemoteFrameLease) -> i32;
type Release = unsafe extern "C" fn(*const a::RemoteFrameLease) -> i32;
pub struct Source<'a> {
    acquire: Demand,
    release: Demand,
    snapshot: Acquire,
    retire: Release,
    _loan: PhantomData<&'a App<'a>>,
}
#[must_use = "release demand and the exact snapshot; retain refused identities"]
pub struct Loans {
    demand: bool,
    readable: bool,
    lease: a::RemoteFrameLease,
    info: a::RemoteFrameInfo,
}
impl Loans {
    pub fn new() -> Self {
        Self {
            demand: false,
            readable: false,
            lease: a::RemoteFrameLease::default(),
            info: a::RemoteFrameInfo::default(),
        }
    }
    pub fn retained(&self) -> bool {
        self.demand || self.lease.id != 0
    }
    pub fn info(&self) -> Option<a::RemoteFrameInfo> {
        (self.lease.id != 0).then_some(self.info)
    }
    pub fn epoch(&self) -> u64 {
        self.lease.epoch
    }
}
impl<'a> Source<'a> {
    pub fn new(app: &'a App<'_>) -> Result<Self, Error> {
        let t = app.desktop_api().ok_or(Error::Unavailable)?;
        if [
            t.remote_frame_acquire,
            t.remote_frame_release,
            t.remote_frame_snapshot_acquire,
            t.remote_frame_snapshot_release,
        ]
        .contains(&0)
        {
            return Err(Error::Unavailable);
        }
        Ok(unsafe {
            Self {
                acquire: core::mem::transmute::<usize, Demand>(t.remote_frame_acquire),
                release: core::mem::transmute::<usize, Demand>(t.remote_frame_release),
                snapshot: core::mem::transmute::<usize, Acquire>(t.remote_frame_snapshot_acquire),
                retire: core::mem::transmute::<usize, Release>(t.remote_frame_snapshot_release),
                _loan: PhantomData,
            }
        })
    }
    pub fn demand(&self, loans: &mut Loans) -> Result<(), Error> {
        if loans.demand {
            return Ok(());
        }
        let rc = unsafe { (self.acquire)() };
        if rc <= 0 {
            return Err(Error::Native(rc));
        }
        loans.demand = true;
        Ok(())
    }
    pub fn snapshot(&self, loans: &mut Loans) -> Result<(), Error> {
        if loans.lease.id != 0 {
            return Err(Error::InvalidSnapshot);
        }
        let mut lease = a::RemoteFrameLease::default();
        let mut info = a::RemoteFrameInfo::default();
        let rc = unsafe { (self.snapshot)(0, &mut info, &mut lease) };
        // Even a malformed returned image retains any actual lease identity.
        if lease.id != 0 {
            loans.lease = lease;
            loans.info = info;
        }
        if rc != 0 {
            return Err(Error::Native(rc));
        }
        loans.readable = true;
        if let Err(error) = self.pixels(loans) {
            loans.readable = false;
            return Err(error);
        }
        Ok(())
    }
    pub fn pixels<'l>(&self, loans: &'l Loans) -> Result<&'l [u8], Error> {
        let l = &loans.lease;
        let i = &loans.info;
        let bytes = (i.frame_pixels as usize)
            .checked_mul(4)
            .filter(|n| *n <= 64 * 1024 * 1024)
            .ok_or(Error::InvalidSnapshot)?;
        l.pixels_addr
            .checked_add(bytes as u64)
            .ok_or(Error::InvalidSnapshot)?;
        if !loans.readable
            || l.id == 0
            || l.pixels_addr == 0
            || !l.pixels_addr.is_multiple_of(4)
            || l.capacity_pixels < i.frame_pixels as u64
            || i.width == 0
            || i.height == 0
            || i.stride_pixels < i.width
            || i.format != a::remote_frame_format_xrgb32
            || (i.frame_pixels as u64) < i.stride_pixels as u64 * i.height as u64
        {
            return Err(Error::InvalidSnapshot);
        }
        // A successful current kernel snapshot owns this immutable allocation.
        // The returned borrow prevents a mutable release of its Loans owner.
        Ok(unsafe { core::slice::from_raw_parts(l.pixels_addr as *const u8, bytes) })
    }
    pub fn words<'l>(&self, loans: &'l Loans) -> Result<&'l [u32], Error> {
        let bytes = self.pixels(loans)?;
        Ok(unsafe { core::slice::from_raw_parts(bytes.as_ptr().cast(), bytes.len() / 4) })
    }
    pub fn release_snapshot(&self, loans: &mut Loans) -> bool {
        if loans.lease.id == 0 {
            return true;
        }
        if unsafe { (self.retire)(&loans.lease) } != 0 {
            return false;
        }
        loans.lease = a::RemoteFrameLease::default();
        loans.readable = false;
        true
    }
    pub fn release_demand(&self, loans: &mut Loans) -> bool {
        if !loans.demand {
            return true;
        }
        if unsafe { (self.release)() } < 0 {
            return false;
        }
        loans.demand = false;
        true
    }
    pub fn close(&self, loans: &mut Loans) -> bool {
        let snapshot = self.release_snapshot(loans);
        let demand = self.release_demand(loans);
        snapshot && demand
    }
}
#[cfg(test)]
#[path = "../../../Tests/RustNative/capture_source.rs"]
mod tests;
