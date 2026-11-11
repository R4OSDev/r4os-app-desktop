//! A complete program generation is the only inventory evidence usable for
//! lifecycle changes. An equal current registry epoch can renew that evidence
//! without pages. A failed refresh keeps only the previous display snapshot.
use alloc::vec::Vec;
use r4os_contract as a;

const RESTART_LIMIT: usize = 8;
const PAGE_CAPACITY: usize = a::program_inventory_page_max as usize;
#[derive(Debug)]
enum ReadError {
    Memory,
    Unavailable,
}

// The SDK validates complete pages, restart/capacity, clock identity and the
// initialized prefix. Desktop owns allocation, the reusable UI snapshot and
// its bounded refresh policy.
fn collect<T: r4os_sdk::inventory::Row>(
    app: &r4os_sdk::App2<'_>,
    staging: &mut Vec<T>,
    mut reusable_epoch: u64,
    pages: &mut u64,
    restarts: &mut u64,
) -> core::result::Result<Option<u64>, ReadError> {
    use r4os_sdk::{
        inventory::{InventoryReader, Status},
        resources::Timeout,
        time::{Clock, checked::DeadlineBudget},
    };
    let clock = Clock::new2(app);
    let sample = clock.checked_sample().map_err(|_| ReadError::Unavailable)?;
    let deadline = DeadlineBudget::new(sample, Timeout::AfterNanos(500_000_000))
        .map_err(|_| ReadError::Unavailable)?;
    staging.clear();
    staging
        .try_reserve(PAGE_CAPACITY)
        .map_err(|_| ReadError::Memory)?;
    let mut attempts = 0;
    loop {
        let grow_to;
        {
            let mut reader = InventoryReader::with_deadline(
                app.context(),
                staging.spare_capacity_mut(),
                PAGE_CAPACITY as u32,
                &deadline,
            )
            .map_err(|_| ReadError::Unavailable)?;
            let mut first = true;
            loop {
                let was_first = first;
                let progress = reader
                    .step(app.should_close(), &mut || {
                        clock.checked_sample().map_err(|_| {
                            r4os_sdk::inventory::Error(a::waitset_error_clock_unavailable)
                        })
                    })
                    .map_err(|_| ReadError::Unavailable)?;
                if was_first {
                    *pages = 0;
                } else {
                    *pages = pages.saturating_add(1);
                }
                let cursor = reader.cursor();
                if was_first
                    && progress.status == Status::More
                    && reusable_epoch != 0
                    && cursor.program_epoch == reusable_epoch
                {
                    return Ok(None);
                }
                first = false;
                match progress.status {
                    Status::More => {}
                    Status::Complete => {
                        let count = reader.view().map_err(|_| ReadError::Unavailable)?.len();
                        drop(reader);
                        // The shared reader exposes this exact initialized
                        // prefix only after complete native validation.
                        unsafe {
                            staging.set_len(count);
                        }
                        return Ok(Some(cursor.program_epoch));
                    }
                    Status::Restart => {
                        *restarts = restarts.saturating_add(1);
                        attempts += 1;
                        if attempts == RESTART_LIMIT {
                            return Err(ReadError::Unavailable);
                        }
                        reusable_epoch = 0;
                        reader.restart().map_err(|_| ReadError::Unavailable)?;
                        first = true;
                    }
                    Status::Capacity => {
                        grow_to = progress.total.ok_or(ReadError::Unavailable)? as usize;
                        break;
                    }
                    _ => return Err(ReadError::Unavailable),
                }
            }
        }
        reusable_epoch = 0;
        if grow_to <= staging.capacity() {
            return Err(ReadError::Unavailable);
        }
        staging
            .try_reserve_exact(grow_to)
            .map_err(|_| ReadError::Memory)?;
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
    lifecycle: Vec<a::ProgramLifecycleSnapshot>,
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
        app: &r4os_sdk::App2<'_>,
        diagnostics: bool,
        tick: u64,
        interval: u64,
    ) -> bool {
        let full = self.diagnostic_refresh.due(diagnostics, tick);
        let complete = self.refresh_current(app, !full);
        if complete && full {
            self.diagnostic_refresh.captured(tick, interval);
        }
        complete
    }
    fn refresh_current(&mut self, app: &r4os_sdk::App2<'_>, reuse: bool) -> bool {
        let reusable_epoch = core::mem::replace(&mut self.complete_epoch, 0);
        self.authoritative = false;
        self.pages = 0;
        let result = if reuse {
            collect(
                app,
                &mut self.lifecycle,
                reusable_epoch,
                &mut self.pages,
                &mut self.restarts,
            )
        } else {
            collect(
                app,
                &mut self.staging,
                0,
                &mut self.pages,
                &mut self.restarts,
            )
        };
        let result = result.and_then(|epoch| {
            if reuse && epoch.is_some() {
                self.staging.clear();
                self.staging
                    .try_reserve(self.lifecycle.len())
                    .map_err(|_| ReadError::Memory)?;
                // This is the existing private UI/model cache. Lifecycle
                // facts do not infer diagnostic memory values from zero.
                self.staging
                    .extend(self.lifecycle.iter().map(|row| a::ProgramInstanceSnapshot {
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
                            ..Default::default()
                        },
                        ..Default::default()
                    }));
            }
            Ok(epoch)
        });
        match result {
            Ok(epoch) => {
                if let Some(epoch) = epoch {
                    core::mem::swap(&mut self.active, &mut self.staging);
                    self.complete_epoch = epoch;
                } else {
                    self.complete_epoch = reusable_epoch;
                }
                self.authoritative = true;
                self.restart_pending = false;
                self.out_of_memory = false;
                true
            }
            Err(error) => {
                self.restart_pending = true;
                self.out_of_memory = matches!(error, ReadError::Memory);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{collections::VecDeque, vec};
    use r4os_sdk::{resources::Result, storage::Error};
    trait Read {
        fn cursor(&self) -> &a::ProgramInventoryCursor;
        fn reserve_total(&self) -> usize;
        fn programs(
            &mut self,
            out: &mut [a::ProgramInstanceSnapshot],
        ) -> Result<a::ProgramInventoryPageInfo>;
    }
    // Retain the original owner scenarios at the actual shared reader entry.
    // The scoped bridge calls one serialized native peer synchronously. It
    // never publishes the closure or its borrowed state to another thread.
    static SOURCE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static SOURCE: std::sync::atomic::AtomicPtr<core::ffi::c_void> =
        std::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
    struct Bridge<S, F> {
        begin: F,
        source: Option<S>,
    }
    fn error_code(error: Error) -> i32 {
        match error {
            Error::Raw(code) => code,
            _ => a::err_no_fn,
        }
    }
    impl<S: Read, F: FnMut() -> Result<S>> Bridge<S, F> {
        unsafe fn current<'b>() -> &'b mut Self {
            unsafe {
                &mut *SOURCE
                    .load(std::sync::atomic::Ordering::Acquire)
                    .cast::<Self>()
            }
        }
        unsafe fn begin(
            cursor: *mut a::ProgramInventoryCursor,
            summary: Option<*mut a::ProgramInventorySummary>,
            life: bool,
        ) -> i32 {
            let state = unsafe { Self::current() };
            let source = match (state.begin)() {
                Ok(source) => source,
                Err(error) => return error_code(error),
            };
            let mut value = *source.cursor();
            if life {
                value.flags |= a::program_inventory_cursor_flag_lifecycle;
            }
            unsafe {
                cursor.write(value);
            }
            if let Some(summary) = summary {
                unsafe {
                    summary.write(a::ProgramInventorySummary {
                        snapshot_generation: value.snapshot_generation,
                        program_epoch: value.program_epoch,
                        task_epoch: value.task_epoch,
                        thread_epoch: value.thread_epoch,
                        program_total: source.reserve_total() as u32,
                        flags: a::program_inventory_summary_flag_stable,
                        ..Default::default()
                    });
                }
            }
            state.source = Some(source);
            0
        }
        unsafe extern "C" fn full_begin(
            cursor: *mut a::ProgramInventoryCursor,
            summary: *mut a::ProgramInventorySummary,
        ) -> i32 {
            unsafe { Self::begin(cursor, Some(summary), false) }
        }
        unsafe extern "C" fn life_begin(cursor: *mut a::ProgramInventoryCursor) -> i32 {
            unsafe { Self::begin(cursor, None, true) }
        }
        unsafe extern "C" fn full_page(
            cursor: *mut a::ProgramInventoryCursor,
            rows: *mut a::ProgramInstanceSnapshot,
            capacity: u32,
            output: *mut a::ProgramInventoryPageInfo,
        ) -> i32 {
            let source = unsafe { Self::current() }.source.as_mut().unwrap();
            let page = match source
                .programs(unsafe { core::slice::from_raw_parts_mut(rows, capacity as usize) })
            {
                Ok(page) => page,
                Err(error) => return error_code(error),
            };
            unsafe {
                cursor.write(*source.cursor());
                output.write(page);
            }
            0
        }
        unsafe extern "C" fn life_page(
            cursor: *mut a::ProgramInventoryCursor,
            rows: *mut a::ProgramLifecycleSnapshot,
            capacity: u32,
            output: *mut a::ProgramInventoryPageInfo,
        ) -> i32 {
            let source = unsafe { Self::current() }.source.as_mut().unwrap();
            let mut scratch = [a::ProgramInstanceSnapshot::default(); PAGE_CAPACITY];
            let mut page = match source.programs(&mut scratch[..capacity as usize]) {
                Ok(page) => page,
                Err(error) => return error_code(error),
            };
            for (i, row) in scratch[..page.returned as usize].iter().enumerate() {
                unsafe {
                    rows.add(i).write(a::ProgramLifecycleSnapshot {
                        handle: row.handle,
                        task_id: row.info.task_id,
                        role: row.info.role,
                        app_class: row.info.app_class,
                        state: row.info.state,
                        flags: row.info.flags,
                        exit_code: row.info.exit_code,
                        window_id: row.info.window_id,
                        ..Default::default()
                    });
                }
            }
            let mut value = *source.cursor();
            value.flags |= a::program_inventory_cursor_flag_lifecycle;
            page.kind = u32::from(a::program_inventory_kind_lifecycle);
            unsafe {
                cursor.write(value);
                output.write(page);
            }
            0
        }
    }
    unsafe extern "C" fn clock(out: *mut a::MonotonicClockInfo) -> i32 {
        unsafe {
            out.write(a::MonotonicClockInfo {
                flags: a::monotonic_clock_flag_valid,
                source: a::monotonic_clock_source_tsc,
                generation: 1,
                instant_ns: 1,
                resolution_ns: 1,
                event_requested_hz: 1000,
                event_effective_hz: 1000,
                ..Default::default()
            });
        }
        1
    }
    impl Inventory {
        fn refresh_with<S: Read>(&mut self, reuse: bool, begin: impl FnMut() -> Result<S>) -> bool {
            self.refresh_fixture(
                reuse,
                Bridge {
                    begin,
                    source: None::<S>,
                },
            )
        }
        fn refresh_fixture<S: Read, F: FnMut() -> Result<S>>(
            &mut self,
            reuse: bool,
            mut bridge: Bridge<S, F>,
        ) -> bool {
            let _guard = SOURCE_LOCK.lock().unwrap();
            SOURCE.store(
                core::ptr::from_mut(&mut bridge).cast(),
                std::sync::atomic::Ordering::Release,
            );
            let system = a::generation2::R4XStartR4Sys2 {
                program_inventory_begin: Bridge::<S, F>::full_begin as *const () as u64,
                program_inventory_programs: Bridge::<S, F>::full_page as *const () as u64,
                program_lifecycle_begin: Bridge::<S, F>::life_begin as *const () as u64,
                program_lifecycle_programs: Bridge::<S, F>::life_page as *const () as u64,
                monotonic_clock: clock as *const () as u64,
                ..Default::default()
            };
            let imports = [a::R4XStartImport {
                group_id: 1,
                min_version: 2,
                resolved_version: 2,
                flags: a::r4xstart_import_flag_group_interface,
                module_name: c"R4SYS".as_ptr() as u64,
                symbol_name: c"Query2".as_ptr() as u64,
                table: core::ptr::from_ref(&system) as u64,
            }];
            let raw = a::generation2::R4XStartContext2 {
                app_class: a::R4XStartAppClass::console.0,
                instance_id: 86,
                imports: imports.as_ptr() as u64,
                import_count: 1,
                flags: a::r4xstart_flag_imports_valid | a::r4xstart_flag_close_supported,
                should_close: crate::standard_fixture::keep_open as *const () as u64,
                ..Default::default()
            };
            let context = unsafe { r4os_sdk::platform2::Context::new(&raw) }.unwrap();
            let app = r4os_sdk::App2::from_context(&context, c"INVENTORYFIX").unwrap();
            let result = self.refresh_current(&app, reuse);
            SOURCE.store(core::ptr::null_mut(), std::sync::atomic::Ordering::Release);
            result
        }
    }

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
            cursor.flags = a::program_inventory_cursor_flag_initialized;
            let mut summary = a::ProgramInventorySummary::default();
            summary.program_total = 1;
            let mut entry = a::ProgramInstanceSnapshot::default();
            entry.handle.instance_id = id;
            entry.info.id = id;
            entry.handle.generation = generation;
            let mut page = a::ProgramInventoryPageInfo::default();
            page.snapshot_generation = generation;
            page.returned = 1;
            page.total = 1;
            page.kind = u32::from(a::program_inventory_kind_program);
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
            let (mut page, data) = self.pages.pop_front().ok_or(Error::Raw(-1))?;
            out[..data.len()].copy_from_slice(&data);
            if page.status != i32::from(a::program_inventory_status_restart) {
                if let Some(row) = data
                    .get(..page.returned as usize)
                    .and_then(|rows| rows.last())
                {
                    self.cursor.program_after_generation = row.handle.generation;
                }
                page.next_generation = self.cursor.program_after_generation;
            }
            if let Some(epoch) = self.changed_epoch {
                self.cursor.program_epoch = epoch;
            }
            Ok(page)
        }
    }
    #[test]
    fn shared_reader_grows_full_and_lifecycle_storage_before_publishing_complete_rows() {
        let count = PAGE_CAPACITY + 1;
        let source = || {
            let mut reader = Reader::one(77, 1);
            reader.summary.program_total = count as u32;
            reader.pages.clear();
            for first in (0..count).step_by(PAGE_CAPACITY) {
                let end = (first + PAGE_CAPACITY).min(count);
                let rows = (first..end)
                    .map(|index| a::ProgramInstanceSnapshot {
                        handle: a::ProgramProcessHandle {
                            instance_id: index as u32 + 1,
                            generation: index as u64 + 1,
                            ..Default::default()
                        },
                        info: a::ProgramInstanceInfo {
                            id: index as u32 + 1,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .collect();
                let page = a::ProgramInventoryPageInfo {
                    snapshot_generation: 77,
                    total: count as u32,
                    returned: (end - first) as u32,
                    kind: u32::from(a::program_inventory_kind_program),
                    has_more: u32::from(end != count),
                    status: i32::from(if end == count {
                        a::program_inventory_status_complete
                    } else {
                        a::program_inventory_status_more
                    }),
                    ..Default::default()
                };
                reader.pages.push_back((page, rows));
            }
            Ok(reader)
        };
        let mut inventory = Inventory::default();
        assert!(inventory.refresh_with(false, source));
        assert_eq!(inventory.pages, 2);
        assert_eq!(inventory.current().unwrap().len(), count);
        inventory.complete_epoch = 0;
        assert!(inventory.refresh_with(true, source));
        assert_eq!(inventory.pages, 2);
        let current = inventory.current().unwrap();
        assert_eq!(current.len(), count);
        assert_eq!(current[count - 1].handle.generation, count as u64);
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
                reader.pages[0].0.returned = 0;
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
            reader.pages[0].0.total = 0;
            reader.summary.program_total = 0;
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
            reader.pages[0].0.returned = 0;
            Ok(reader)
        }));
        assert_eq!(begins, 8);
        assert_eq!(inventory.restarts, 8);
        assert!(inventory.current().is_none());
        assert_eq!(inventory.visible()[0].handle.instance_id, 42);
        assert!(!inventory.refresh_with(true, || {
            let mut reader = Reader::one(12, 99);
            reader.pages[0].0.status = i32::from(a::program_inventory_status_more);
            reader.pages[0].0.has_more = 1;
            reader.pages[0].0.total = 2;
            reader.summary.program_total = 2;
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
            reader.pages[0].0.total = 0;
            reader.summary.program_total = 0;
            reader.pages[0].1.clear();
            reader.pages[0].0.status = i32::from(a::program_inventory_status_more);
            reader.pages[0].0.has_more = 1;
            reader.pages[0].0.total = 2;
            reader.summary.program_total = 2;
            Ok(reader)
        }));
        assert!(!inventory.refresh_with::<Reader>(true, || Err(Error::Raw(-9))));
        assert!(inventory.current().is_none());
        assert_eq!(inventory.visible()[0].handle.instance_id, 7);
        assert!(inventory.refresh_with(true, || {
            let mut reader = Reader::one(3, 0);
            reader.pages[0].0.returned = 0;
            reader.pages[0].0.total = 0;
            reader.summary.program_total = 0;
            reader.pages[0].1.clear();
            Ok(reader)
        }));
        assert!(inventory.current().unwrap().is_empty());
    }
}
