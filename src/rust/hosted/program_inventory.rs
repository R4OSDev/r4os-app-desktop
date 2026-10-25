//! A complete program generation is the only inventory evidence usable for
//! lifecycle changes. An equal current registry epoch can renew that evidence
//! without pages. A failed refresh keeps only the previous display snapshot.
use alloc::vec::Vec;
use r4os_contract as a;
use r4os_sdk::resources::{Inventory as NativeInventory, Resources, Result};

const RESTART_LIMIT: usize = 8;
const PAGE_CAPACITY: usize = a::program_inventory_page_max as usize;
trait Read {
    fn cursor(&self) -> &a::ProgramInventoryCursor;
    fn reserve_total(&self) -> usize;
    fn programs(
        &mut self,
        out: &mut [a::ProgramInstanceSnapshot],
    ) -> Result<a::ProgramInventoryPageInfo>;
}
impl Read for NativeInventory<'_> {
    fn cursor(&self) -> &a::ProgramInventoryCursor {
        NativeInventory::cursor(self)
    }
    fn reserve_total(&self) -> usize {
        NativeInventory::summary(self).program_total as usize
    }
    fn programs(
        &mut self,
        out: &mut [a::ProgramInstanceSnapshot],
    ) -> Result<a::ProgramInventoryPageInfo> {
        NativeInventory::programs(self, out)
    }
}
struct LifecycleReader<'a>(r4os_sdk::resources::Lifecycle<'a>);
impl Read for LifecycleReader<'_> {
    fn cursor(&self) -> &a::ProgramInventoryCursor {
        self.0.cursor()
    }
    fn reserve_total(&self) -> usize {
        0
    }
    fn programs(
        &mut self,
        out: &mut [a::ProgramInstanceSnapshot],
    ) -> Result<a::ProgramInventoryPageInfo> {
        let mut rows = [a::ProgramLifecycleSnapshot::default(); PAGE_CAPACITY];
        let capacity = out.len().min(rows.len());
        let page = self.0.programs(&mut rows[..capacity])?;
        // This is the existing private UI/model cache, never a diagnostic export.
        // Lifecycle uses only these state fields. The explicit Tasks path still
        // requests full native inventory; no memory value is inferred from zero.
        for (dst, row) in out.iter_mut().zip(&rows[..page.returned as usize]) {
            *dst = a::ProgramInstanceSnapshot {
                handle: row.handle,
                state_generation: row.handle.generation,
                info: a::ProgramInstanceInfo {
                    id: row.handle.instance_id,
                    task_id: row.task_id,
                    role: row.role,
                    app_class: row.app_class,
                    state: row.state,
                    flags: row.flags,
                    exit_code: row.exit_code,
                    window_id: row.window_id,
                    ..a::ProgramInstanceInfo::default()
                },
                ..a::ProgramInstanceSnapshot::default()
            };
        }
        Ok(page)
    }
}
// Full diagnostic acquisition is opt-in and bounded while Tasks is visible.
// Process metadata checks the current registry epoch independently each round.
#[derive(Default)]
struct DiagnosticRefresh {
    next: Option<u64>,
    last_tick: Option<u64>,
}
impl DiagnosticRefresh {
    fn due(&mut self, requested: bool, tick: u64) -> bool {
        if !requested || self.last_tick.is_some_and(|last| tick < last) {
            self.next = None;
        }
        self.last_tick = Some(tick);
        requested && self.next.is_none_or(|next| tick >= next)
    }
    fn captured(&mut self, tick: u64, interval: u64) {
        self.next = Some(tick.saturating_add(interval.max(1)));
    }
}
#[derive(Default)]
pub struct Inventory {
    diagnostic_refresh: DiagnosticRefresh,
    active: Vec<a::ProgramInstanceSnapshot>,
    staging: Vec<a::ProgramInstanceSnapshot>,
    authoritative: bool,
    complete_epoch: u64,
    pub restart_pending: bool,
    pub out_of_memory: bool,
    pub restarts: u64,
    pub pages: u64,
}
/// Only Inventory can construct this view, after a complete refresh. Ordinary
/// task-list slices cannot accidentally authorize a lifecycle transition.
#[derive(Clone, Copy)]
pub struct CurrentGeneration<'a> {
    items: &'a [a::ProgramInstanceSnapshot],
}
impl CurrentGeneration<'_> {
    pub fn find(&self, handle: a::ProgramProcessHandle) -> Option<&a::ProgramInstanceSnapshot> {
        if !super::process::valid(handle) {
            return None;
        }
        self.items.iter().find(|row| {
            row.info.id == handle.instance_id && super::process::same(row.handle, handle)
        })
    }
}
impl core::ops::Deref for CurrentGeneration<'_> {
    type Target = [a::ProgramInstanceSnapshot];
    fn deref(&self) -> &Self::Target {
        self.items
    }
}
impl Inventory {
    /// The last complete snapshot remains suitable for task-list display.
    pub fn visible(&self) -> &[a::ProgramInstanceSnapshot] {
        &self.active
    }
    /// `None` defers adoption, disappearance and close/reap decisions together.
    pub fn current(&self) -> Option<CurrentGeneration<'_>> {
        self.authoritative.then_some(CurrentGeneration {
            items: &self.active,
        })
    }
    pub fn refresh(
        &mut self,
        resources: &Resources<'_>,
        diagnostics: bool,
        tick: u64,
        interval: u64,
    ) -> bool {
        if self.diagnostic_refresh.due(diagnostics, tick) {
            let complete = self.refresh_with(false, || resources.inventory());
            if complete {
                self.diagnostic_refresh.captured(tick, interval);
            }
            complete
        } else {
            self.refresh_with(true, || resources.lifecycle().map(LifecycleReader))
        }
    }
    fn refresh_with<S: Read>(&mut self, reuse: bool, mut begin: impl FnMut() -> Result<S>) -> bool {
        // Every failure revokes reuse as well as authority. Visible rows remain
        // display-only until a new complete acquisition succeeds.
        let mut reusable_epoch = core::mem::replace(&mut self.complete_epoch, 0);
        self.authoritative = false;
        self.pages = 0;
        'refresh: for _ in 0..RESTART_LIMIT {
            self.staging.clear();
            let Ok(mut reader) = begin() else {
                self.restart_pending = true;
                return false;
            };
            let epoch = reader.cursor().program_epoch;
            let generation = reader.cursor().snapshot_generation;
            if epoch == 0 || generation == 0 {
                self.restart_pending = true;
                return false;
            }
            if reuse && reusable_epoch == epoch {
                self.complete_epoch = epoch;
                self.restart_pending = false;
                self.out_of_memory = false;
                self.authoritative = true;
                return true;
            }
            let total = reader.reserve_total();
            if self.staging.try_reserve(total).is_err() {
                self.out_of_memory = true;
                return false;
            }
            self.pages = 0;
            loop {
                let mut entries = [a::ProgramInstanceSnapshot::default(); PAGE_CAPACITY];
                let Ok(page) = reader.programs(&mut entries) else {
                    self.restart_pending = true;
                    return false;
                };
                if page.status == i32::from(a::program_inventory_status_restart) {
                    self.restarts = self.restarts.saturating_add(1);
                    self.restart_pending = true;
                    reusable_epoch = 0;
                    continue 'refresh;
                }
                if reader.cursor().program_epoch != epoch
                    || reader.cursor().snapshot_generation != generation
                    || page.snapshot_generation != generation
                    || page.returned as usize > entries.len()
                    || (page.status != i32::from(a::program_inventory_status_complete)
                        && page.status != i32::from(a::program_inventory_status_more))
                {
                    self.restart_pending = true;
                    return false;
                }
                let count = page.returned as usize;
                if self.staging.try_reserve(count).is_err() {
                    self.out_of_memory = true;
                    return false;
                }
                self.staging.extend_from_slice(&entries[..count]);
                self.pages = self.pages.saturating_add(1);
                if page.status == i32::from(a::program_inventory_status_complete) {
                    break;
                }
                if count == 0 {
                    self.restart_pending = true;
                    return false;
                }
            }
            core::mem::swap(&mut self.active, &mut self.staging);
            self.restart_pending = false;
            self.out_of_memory = false;
            self.complete_epoch = epoch;
            self.authoritative = true;
            return true;
        }
        self.restart_pending = true;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{collections::VecDeque, vec};
    use r4os_sdk::storage::Error;
    struct Reader {
        cursor: a::ProgramInventoryCursor,
        summary: a::ProgramInventorySummary,
        pages: VecDeque<(a::ProgramInventoryPageInfo, Vec<a::ProgramInstanceSnapshot>)>,
        changed_epoch: Option<u64>,
    }
    impl Reader {
        fn one(generation: u64, id: u32) -> Self {
            let mut cursor = a::ProgramInventoryCursor::default();
            cursor.snapshot_generation = generation;
            cursor.program_epoch = generation;
            let mut summary = a::ProgramInventorySummary::default();
            summary.program_total = 1;
            let mut entry = a::ProgramInstanceSnapshot::default();
            entry.handle.instance_id = id;
            entry.info.id = id;
            entry.handle.generation = generation;
            let mut page = a::ProgramInventoryPageInfo::default();
            page.snapshot_generation = generation;
            page.returned = 1;
            page.status = i32::from(a::program_inventory_status_complete);
            Self {
                cursor,
                summary,
                pages: vec![(page, vec![entry])].into(),
                changed_epoch: None,
            }
        }
    }
    impl Read for Reader {
        fn cursor(&self) -> &a::ProgramInventoryCursor {
            &self.cursor
        }
        fn reserve_total(&self) -> usize {
            self.summary.program_total as usize
        }
        fn programs(
            &mut self,
            out: &mut [a::ProgramInstanceSnapshot],
        ) -> Result<a::ProgramInventoryPageInfo> {
            let (page, data) = self.pages.pop_front().ok_or(Error::Raw(-1))?;
            out[..data.len()].copy_from_slice(&data);
            if let Some(epoch) = self.changed_epoch {
                self.cursor.program_epoch = epoch;
            }
            Ok(page)
        }
    }
    #[test]
    fn complete_epoch_reuses_without_pages_but_diagnostics_read_again() {
        let mut inventory = Inventory::default();
        assert!(inventory.refresh_with(true, || Ok(Reader::one(10, 42))));
        let handle = inventory.current().unwrap()[0].handle;
        for generation in 11..1011 {
            assert!(inventory.refresh_with(true, || {
                let mut reader = Reader::one(generation, 99);
                reader.cursor.program_epoch = 10;
                reader.pages.clear(); // a page call would fail
                Ok(reader)
            }));
            assert_eq!(inventory.pages, 0);
            assert!(inventory.current().unwrap().find(handle).is_some());
        }
        // A full diagnostic snapshot may change memory values at the same
        // lifecycle epoch. Its independent generation is never the cache key.
        assert!(inventory.refresh_with(false, || {
            let mut reader = Reader::one(2000, 42);
            reader.cursor.program_epoch = 10;
            reader.pages[0].1[0].handle = handle;
            reader.pages[0].1[0].info.memory_committed_bytes = 4096;
            Ok(reader)
        }));
        assert_eq!(inventory.pages, 1);
        assert_eq!(
            inventory.current().unwrap()[0].info.memory_committed_bytes,
            4096
        );
        assert!(inventory.current().unwrap().find(handle).is_some());
    }
    #[test]
    fn busy_or_restart_requires_complete_resync_even_at_the_old_epoch() {
        let mut inventory = Inventory::default();
        assert!(inventory.refresh_with(true, || Ok(Reader::one(10, 42))));
        assert!(!inventory.refresh_with::<Reader>(true, || Err(Error::Raw(-5))));
        assert!(inventory.current().is_none());
        assert_eq!(inventory.visible()[0].handle.instance_id, 42);
        assert!(!inventory.refresh_with(true, || {
            let mut reader = Reader::one(10, 42);
            reader.pages.clear();
            Ok(reader)
        }));
        assert!(inventory.refresh_with(true, || Ok(Reader::one(10, 42))));
        let mut attempt = 0;
        assert!(!inventory.refresh_with(true, || {
            attempt += 1;
            let mut reader = Reader::one(if attempt == 1 { 11 } else { 10 }, 99);
            if attempt == 1 {
                reader.pages[0].0.status = i32::from(a::program_inventory_status_restart);
            } else {
                reader.pages.clear();
            }
            Ok(reader)
        }));
        assert_eq!(attempt, 2);
        assert!(inventory.current().is_none());
        assert_eq!(inventory.visible()[0].handle.instance_id, 42);
        assert!(inventory.refresh_with(true, || Ok(Reader::one(10, 42))));
        assert_eq!(inventory.pages, 1);
    }
    #[test]
    fn complete_empty_snapshot_is_reusable_and_changed_epoch_refreshes() {
        let mut inventory = Inventory::default();
        assert!(inventory.refresh_with(true, || {
            let mut reader = Reader::one(1, 0);
            reader.pages[0].0.returned = 0;
            reader.pages[0].1.clear();
            Ok(reader)
        }));
        assert!(inventory.current().unwrap().is_empty());
        assert!(inventory.refresh_with(true, || {
            let mut reader = Reader::one(1, 99);
            reader.pages.clear();
            Ok(reader)
        }));
        assert_eq!(inventory.pages, 0);
        assert!(inventory.current().unwrap().is_empty());
        assert!(inventory.refresh_with(true, || Ok(Reader::one(2, 42))));
        assert_eq!(inventory.pages, 1);
        assert_eq!(inventory.current().unwrap()[0].handle.instance_id, 42);
    }
    #[test]
    fn invalid_or_changing_cursor_epoch_cannot_authorize_a_snapshot() {
        let mut inventory = Inventory::default();
        assert!(inventory.refresh_with(true, || Ok(Reader::one(1, 42))));
        assert!(!inventory.refresh_with(true, || {
            let mut reader = Reader::one(2, 99);
            reader.cursor.program_epoch = 0;
            Ok(reader)
        }));
        assert!(!inventory.refresh_with(true, || {
            let mut reader = Reader::one(2, 99);
            reader.changed_epoch = Some(3);
            Ok(reader)
        }));
        assert!(inventory.current().is_none());
        assert_eq!(inventory.visible()[0].handle.instance_id, 42);
    }
    #[test]
    fn explicit_task_diagnostics_are_bounded_retry_without_authority_and_reopen_fresh() {
        let mut schedule = DiagnosticRefresh::default();
        for tick in 0..100 {
            assert!(!schedule.due(false, tick));
        }
        assert!(schedule.due(true, 100));
        schedule.captured(100, 25);
        for tick in 101..125 {
            assert!(!schedule.due(true, tick));
        }
        assert!(schedule.due(true, 125));
        // A failed/Busy full snapshot does not acknowledge a refresh or defer
        // the next attempt. Inventory's existing cases below check authority.
        assert!(schedule.due(true, 126));
        schedule.captured(126, 25);
        assert!(!schedule.due(true, 127));
        assert!(!schedule.due(false, 128));
        assert!(schedule.due(true, 129));
        schedule.captured(129, 25);
        assert!(schedule.due(true, 1)); // clock rollback cannot freeze Tasks
        schedule.captured(1, 0);
        assert!(schedule.due(true, 2));
    }
    #[test]
    fn current_restart_or_partial_failure_never_replaces_complete_generation() {
        let mut inventory = Inventory::default();
        assert!(inventory.refresh_with(true, || Ok(Reader::one(10, 42))));
        assert_eq!(inventory.current().unwrap()[0].handle.generation, 10);
        let handle = inventory.current().unwrap()[0].handle;
        assert!(inventory.current().unwrap().find(handle).is_some());
        let mut reused = handle;
        reused.generation += 1;
        assert!(inventory.current().unwrap().find(reused).is_none());
        let mut begins = 0;
        assert!(!inventory.refresh_with(true, || {
            begins += 1;
            let mut reader = Reader::one(11, 99);
            reader.pages[0].0.status = i32::from(a::program_inventory_status_restart);
            Ok(reader)
        }));
        assert_eq!(begins, 8);
        assert_eq!(inventory.restarts, 8);
        assert!(inventory.current().is_none());
        assert_eq!(inventory.visible()[0].handle.instance_id, 42);
        assert!(!inventory.refresh_with(true, || {
            let mut reader = Reader::one(12, 99);
            reader.pages[0].0.status = i32::from(a::program_inventory_status_more);
            Ok(reader) // the next page fails
        }));
        assert_eq!(inventory.visible()[0].handle.instance_id, 42);
        assert!(inventory.current().is_none());
        assert!(inventory.refresh_with(true, || Ok(Reader::one(12, 42))));
        assert_eq!(inventory.current().unwrap()[0].handle.generation, 12);
        assert!(!inventory.restart_pending);
    }
    #[test]
    fn current_page_generation_and_progress_are_required_for_commit() {
        let mut inventory = Inventory::default();
        assert!(inventory.refresh_with(true, || Ok(Reader::one(1, 7))));
        assert!(!inventory.refresh_with(true, || {
            let mut reader = Reader::one(2, 8);
            reader.pages[0].0.snapshot_generation = 3;
            Ok(reader)
        }));
        assert!(!inventory.refresh_with(true, || {
            let mut reader = Reader::one(2, 8);
            reader.pages[0].0.returned = 0;
            reader.pages[0].1.clear();
            reader.pages[0].0.status = i32::from(a::program_inventory_status_more);
            Ok(reader)
        }));
        assert!(!inventory.refresh_with::<Reader>(true, || Err(Error::Raw(-9))));
        assert!(inventory.current().is_none());
        assert_eq!(inventory.visible()[0].handle.instance_id, 7);
        assert!(inventory.refresh_with(true, || {
            let mut reader = Reader::one(3, 0);
            reader.pages[0].0.returned = 0;
            reader.pages[0].1.clear();
            Ok(reader)
        }));
        assert!(inventory.current().unwrap().is_empty());
    }
}
