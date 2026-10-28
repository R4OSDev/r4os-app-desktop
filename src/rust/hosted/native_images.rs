//! Main-task read ledger for native composition. The worker receives only
//! immutable BO metadata; no window/frame reference crosses the task boundary.
use super::{
    window_image::{Identity, Memory},
    window_set::Set,
};
use crate::render::{
    geometry::Rect,
    gpu::{
        engine::{Engine, External, FinishExternal},
        receipt::Consumer,
    },
    layers::Cache,
    primitive_assets::{Error, Result},
};

#[derive(Clone, Copy)]
struct Read {
    entry: usize,
    window: usize,
    identity: Identity,
}
pub struct Reads {
    entries: [Option<Read>; crate::windows::controller::SLOT_COUNT],
    pub captured: u64,
    pub completed: u64,
    pub failed: u64,
}
impl Default for Reads {
    fn default() -> Self {
        Self {
            entries: [None; crate::windows::controller::SLOT_COUNT],
            captured: 0,
            completed: 0,
            failed: 0,
        }
    }
}
impl Reads {
    pub fn empty(&self) -> bool {
        self.entries.iter().all(Option::is_none)
    }
    pub fn available<M: Memory>(&self, windows: &mut Set<M, Consumer>) -> [bool; 4] {
        core::array::from_fn(|i| windows.front(i).is_some())
    }
    /// The caller keeps this ledger and the windows resident until each read
    /// is returned after physical retirement, including failed preparation.
    pub fn capture<M: Memory>(
        &mut self,
        windows: &mut Set<M, Consumer>,
        engine: &mut Engine,
        cache: &mut Cache,
        window: usize,
        key: u32,
        bounds: Rect,
        damage: &[Rect],
    ) -> Result<()> {
        let frame = windows.front(window).ok_or(Error::State)?;
        // Native sampling keeps the full producer extent independently of
        // logical client/output scale, rotation and pending resize geometry.
        let identity = frame.identity();
        let mut selected = None;
        for &region in damage {
            if let Some(index) = cache.external_native(key, bounds, region)? {
                if selected.is_some_and(|previous| previous != index) {
                    return Err(Error::State);
                }
                selected = Some(index);
            }
        }
        let Some(index) = selected else {
            return Ok(());
        };
        if let Some(previous) = self
            .entries
            .iter()
            .flatten()
            .find(|read| read.entry == index)
        {
            return if previous.window == window && previous.identity.same(&identity) {
                Ok(())
            } else {
                Err(Error::State)
            };
        }
        let slot = self
            .entries
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Capacity)?;
        let source = External {
            reference: *frame.reference(),
            descriptor: frame.message().descriptor,
            color: frame.description(),
            ready: frame.message().ready,
        };
        frame.borrow().map_err(|_| Error::State)?;
        // The exact retained read in entries owns the lifetime, also after
        // the front changes. Engine returns it only after native retirement.
        if unsafe { engine.set_external(index, source) }.is_err() {
            let released = frame.finish(None, false).is_ok();
            debug_assert!(released);
            return Err(Error::State);
        }
        self.entries[slot] = Some(Read {
            entry: index,
            window,
            identity,
        });
        self.captured = self.captured.saturating_add(1);
        Ok(())
    }
    pub fn completion<'a, M: Memory>(
        &'a mut self,
        windows: &'a mut Set<M, Consumer>,
    ) -> Completion<'a, M> {
        Completion {
            reads: self,
            windows,
        }
    }
}
pub struct Completion<'a, M: Memory> {
    reads: &'a mut Reads,
    windows: &'a mut Set<M, Consumer>,
}
impl<M: Memory> FinishExternal for Completion<'_, M> {
    fn finish(
        &mut self,
        entry: usize,
        receipt: Option<Consumer>,
        failed: bool,
    ) -> core::result::Result<(), Option<Consumer>> {
        let Some(slot) = self
            .reads
            .entries
            .iter()
            .position(|read| read.is_some_and(|r| r.entry == entry))
        else {
            return Err(receipt);
        };
        let read = self.reads.entries[slot].unwrap();
        self.windows
            .finish_read(read.window, read.identity, receipt, failed)?;
        self.reads.entries[slot] = None;
        self.reads.completed = self.reads.completed.saturating_add(1);
        self.reads.failed = self.reads.failed.saturating_add(u64::from(failed));
        Ok(())
    }
}
