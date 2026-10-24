//! A complete program generation is the only inventory evidence usable for
//! lifecycle changes. A failed refresh keeps the previous display snapshot.
use alloc::vec::Vec;
use r4os_contract as a;
use r4os_sdk::resources::{Inventory as NativeInventory, Resources, Result};

const RESTART_LIMIT: usize = 8;
const PAGE_CAPACITY: usize = a::program_inventory_page_max as usize;
trait Read {
    fn cursor(&self) -> &a::ProgramInventoryCursor;
    fn summary(&self) -> &a::ProgramInventorySummary;
    fn programs(
        &mut self,
        out: &mut [a::ProgramInstanceSnapshot],
    ) -> Result<a::ProgramInventoryPageInfo>;
}
impl Read for NativeInventory<'_> {
    fn cursor(&self) -> &a::ProgramInventoryCursor {
        NativeInventory::cursor(self)
    }
    fn summary(&self) -> &a::ProgramInventorySummary {
        NativeInventory::summary(self)
    }
    fn programs(
        &mut self,
        out: &mut [a::ProgramInstanceSnapshot],
    ) -> Result<a::ProgramInventoryPageInfo> {
        NativeInventory::programs(self, out)
    }
}
#[derive(Default)]
pub struct Inventory {
    active: Vec<a::ProgramInstanceSnapshot>,
    staging: Vec<a::ProgramInstanceSnapshot>,
    authoritative: bool,
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
    pub fn refresh(&mut self, resources: &Resources<'_>) -> bool {
        self.refresh_with(|| resources.inventory())
    }
    fn refresh_with<S: Read>(&mut self, mut begin: impl FnMut() -> Result<S>) -> bool {
        self.authoritative = false;
        'refresh: for _ in 0..RESTART_LIMIT {
            self.staging.clear();
            let Ok(mut reader) = begin() else {
                self.restart_pending = true;
                return false;
            };
            let total = reader.summary().program_total as usize;
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
                    continue 'refresh;
                }
                if page.snapshot_generation != reader.cursor().snapshot_generation
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
    }
    impl Reader {
        fn one(generation: u64, id: u32) -> Self {
            let mut cursor = a::ProgramInventoryCursor::default();
            cursor.snapshot_generation = generation;
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
            }
        }
    }
    impl Read for Reader {
        fn cursor(&self) -> &a::ProgramInventoryCursor {
            &self.cursor
        }
        fn summary(&self) -> &a::ProgramInventorySummary {
            &self.summary
        }
        fn programs(
            &mut self,
            out: &mut [a::ProgramInstanceSnapshot],
        ) -> Result<a::ProgramInventoryPageInfo> {
            let (page, data) = self.pages.pop_front().ok_or(Error::Raw(-1))?;
            out[..data.len()].copy_from_slice(&data);
            Ok(page)
        }
    }
    #[test]
    fn current_restart_or_partial_failure_never_replaces_complete_generation() {
        let mut inventory = Inventory::default();
        assert!(inventory.refresh_with(|| Ok(Reader::one(10, 42))));
        assert_eq!(inventory.current().unwrap()[0].handle.generation, 10);
        let handle = inventory.current().unwrap()[0].handle;
        assert!(inventory.current().unwrap().find(handle).is_some());
        let mut reused = handle;
        reused.generation += 1;
        assert!(inventory.current().unwrap().find(reused).is_none());
        let mut begins = 0;
        assert!(!inventory.refresh_with(|| {
            begins += 1;
            let mut reader = Reader::one(11, 99);
            reader.pages[0].0.status = i32::from(a::program_inventory_status_restart);
            Ok(reader)
        }));
        assert_eq!(begins, 8);
        assert_eq!(inventory.restarts, 8);
        assert!(inventory.current().is_none());
        assert_eq!(inventory.visible()[0].handle.instance_id, 42);
        assert!(!inventory.refresh_with(|| {
            let mut reader = Reader::one(12, 99);
            reader.pages[0].0.status = i32::from(a::program_inventory_status_more);
            Ok(reader) // the next page fails
        }));
        assert_eq!(inventory.visible()[0].handle.instance_id, 42);
        assert!(inventory.current().is_none());
        assert!(inventory.refresh_with(|| Ok(Reader::one(12, 42))));
        assert_eq!(inventory.current().unwrap()[0].handle.generation, 12);
        assert!(!inventory.restart_pending);
    }
    #[test]
    fn current_page_generation_and_progress_are_required_for_commit() {
        let mut inventory = Inventory::default();
        assert!(inventory.refresh_with(|| Ok(Reader::one(1, 7))));
        assert!(!inventory.refresh_with(|| {
            let mut reader = Reader::one(2, 8);
            reader.pages[0].0.snapshot_generation = 3;
            Ok(reader)
        }));
        assert!(!inventory.refresh_with(|| {
            let mut reader = Reader::one(2, 8);
            reader.pages[0].0.returned = 0;
            reader.pages[0].1.clear();
            reader.pages[0].0.status = i32::from(a::program_inventory_status_more);
            Ok(reader)
        }));
        assert!(!inventory.refresh_with::<Reader>(|| Err(Error::Raw(-9))));
        assert!(inventory.current().is_none());
        assert_eq!(inventory.visible()[0].handle.instance_id, 7);
        assert!(inventory.refresh_with(|| {
            let mut reader = Reader::one(3, 0);
            reader.pages[0].0.returned = 0;
            reader.pages[0].1.clear();
            Ok(reader)
        }));
        assert!(inventory.current().unwrap().is_empty());
    }
}
