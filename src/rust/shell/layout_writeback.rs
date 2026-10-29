//! Delayed layout snapshots follow the original R4STD retry policy. A save
//! task owns its immutable encoded document through the exact join receipt.
use super::{
    desktop_items::Items,
    desktop_layout::{Layout, MAX_BYTES},
    persistence::{LAYOUT, Store},
};
use crate::runtime::{
    library::{self, required, result, scalar},
    worker::{Control, Task, Worker},
};
use alloc::vec::Vec;
use core::{marker::PhantomData, mem::MaybeUninit};
use r4os_library_helpers::{raw as c, standard::Standard};
use r4os_sdk::{App, StartupInfo, resources::Resources};
struct Save {
    raw: usize,
    expected: &'static StartupInfo<'static>,
    bytes: Vec<u8>,
    generation: u32,
}
impl Task for Save {
    fn run(&mut self, _: &Control) -> i32 {
        let app = match unsafe { App::from_raw(self.raw as *const _, self.expected, c"R4DESK") } {
            Ok(v) => v,
            Err(_) => return c::R4StdConfig_error_write_failed,
        };
        let context = match library::Context::new(&app) {
            Ok(v) => v,
            Err(_) => return c::R4StdConfig_error_write_failed,
        };
        if result(unsafe { c::R4StdAdaptersSettings_ensureDesktopDirs(0, context.system()) })
            .is_err()
        {
            return c::R4StdConfig_error_write_failed;
        }
        match Store::new(&app).and_then(|store| store.save(LAYOUT, &self.bytes)) {
            Ok(code) => code,
            Err(_) => c::R4StdConfig_error_write_failed,
        }
    }
}
pub struct Owner<'a> {
    policy: c::R4StdSettingsWriteback,
    generation: u32,
    worker: Option<Worker<'a, Save>>,
    _standard: PhantomData<&'a Standard<'a>>,
}
impl<'a> Owner<'a> {
    pub fn new(_standard: &'a Standard<'a>, hz: u32) -> Result<Self, r4os_library_helpers::Error> {
        let mut policy = MaybeUninit::uninit();
        let mut state = MaybeUninit::uninit();
        required(unsafe {
            c::R4StdSettingsWritebackPolicy_forHz(
                0,
                c::R4StdSettingsWritebackDurability_lazy,
                hz,
                policy.as_mut_ptr(),
            )
        })?;
        required(unsafe {
            c::R4StdSettingsWriteback_init(0, policy.as_ptr(), state.as_mut_ptr())
        })?;
        Ok(Self {
            policy: unsafe { state.assume_init() },
            generation: 0,
            worker: None,
            _standard: PhantomData,
        })
    }
    pub fn mark(&mut self, tick: u64) -> Result<(), r4os_library_helpers::Error> {
        self.generation = self.generation.wrapping_add(1);
        result(unsafe { c::R4StdSettingsWriteback_markDirty(0, &mut self.policy, tick) })?;
        Ok(())
    }
    pub fn dirty(&self) -> bool {
        scalar(unsafe { c::R4StdSettingsWriteback_isDirty(0, &self.policy) }).is_ok_and(|v| v != 0)
    }
    pub fn due(&self, tick: u64) -> bool {
        scalar(unsafe { c::R4StdSettingsWriteback_isDue(0, &self.policy, tick) })
            .is_ok_and(|v| v != 0)
    }
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    fn complete(&mut self, app: &App<'_>, tick: u64, code: i32) -> bool {
        let mut outcome = MaybeUninit::uninit();
        if required(unsafe {
            c::R4StdSettingsWriteback_complete(
                0,
                &mut self.policy,
                tick,
                code,
                outcome.as_mut_ptr(),
            )
        })
        .is_err()
        {
            return false;
        }
        let outcome = unsafe { outcome.assume_init() };
        if code < 0 {
            if scalar(unsafe { c::R4StdSettingsWritebackFlush_get_first_failure(&outcome) })
                .is_ok_and(|v| v != 0)
            {
                app.write("Desktop layout save failed; retry scheduled\n");
            }
        } else if scalar(unsafe {
            c::R4StdSettingsWritebackFlush_get_recovered_after_failure(&outcome)
        })
        .is_ok_and(|v| v != 0)
        {
            app.write("Desktop layout save: OK\n");
        }
        scalar(unsafe { c::R4StdSettingsWritebackFlush_attempted(0, &outcome) })
            .is_ok_and(|v| v != 0)
    }
    pub fn poll(&mut self, app: &App<'_>, tick: u64) -> bool {
        let Some(worker) = self.worker.as_mut() else {
            return false;
        };
        let Ok(Some((code, save))) = worker.collect() else {
            return false;
        };
        self.worker = None;
        let changed = self.complete(app, tick, code);
        if code >= 0 && save.generation != self.generation {
            let _ =
                result(unsafe { c::R4StdSettingsWriteback_markDirty(0, &mut self.policy, tick) });
        }
        changed
    }
    /// # Safety
    /// Loader context and this module's task code remain resident until `busy`
    /// becomes false after an actual successful native join.
    pub unsafe fn flush(
        &mut self,
        app: &App<'_>,
        resources: &'a Resources<'a>,
        expected: &'static StartupInfo<'static>,
        standard: &Standard<'_>,
        items: &Items,
        tick: u64,
        force: bool,
    ) -> bool {
        let changed = self.poll(app, tick);
        if self.busy()
            || !self.dirty()
            || (!force
                && !scalar(unsafe { c::R4StdSettingsWriteback_isDue(0, &self.policy, tick) })
                    .is_ok_and(|v| v != 0))
        {
            return changed;
        }
        let mut layout = Layout::default();
        for item in items.entries() {
            layout.add(item.path(), item.x, item.y);
        }
        let mut bytes = Vec::new();
        if bytes.try_reserve_exact(MAX_BYTES).is_err() {
            return self.complete(app, tick, c::R4StdConfig_error_write_failed) || changed;
        }
        bytes.resize(MAX_BYTES, 0);
        let count = match layout.write(standard, &mut bytes) {
            Ok(value) if !value.is_empty() => value.len(),
            _ => return self.complete(app, tick, c::R4StdConfig_error_buffer_too_small) || changed,
        };
        bytes.truncate(count);
        let save = Save {
            raw: app.start_context() as *const _ as usize,
            expected,
            bytes,
            generation: self.generation,
        };
        match unsafe { Worker::start(resources, save, 128 * 1024) } {
            Ok(worker) => {
                self.worker = Some(worker);
                changed
            }
            Err(_) => self.complete(app, tick, c::R4StdConfig_error_write_failed) || changed,
        }
    }
}
