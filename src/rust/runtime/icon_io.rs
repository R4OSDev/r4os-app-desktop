//! One bounded copied icon request. Only an exact native join makes decoder
//! pixels accessible to the main owner; paint and worker never share a cache.
use super::worker::{Control, Task, Worker};
use crate::render::icons::{Cache, Native, Refresh, Request};
use r4os_sdk::{App2, abi as a, app2::StartupInfo, resources::Resources};
struct Load {
    request: Request,
    raw: usize,
    expected: &'static StartupInfo<'static>,
    result: Option<Refresh>,
}
impl Task for Load {
    fn run(&mut self, control: &Control) -> i32 {
        if control.stopped() {
            return 0;
        }
        let app = match unsafe {
            App2::from_raw(
                self.raw as *const a::generation2::R4XStartContext2,
                self.expected,
                c"R4DESK",
            )
        } {
            Ok(app) => app,
            Err(_) => return -1,
        };
        self.result = Some(self.request.read(&mut Native { app: &app }));
        0
    }
}
#[derive(Default)]
pub struct Owner<'a> {
    worker: Option<Worker<'a, Load>>,
    stopping: bool,
}
impl<'a> Owner<'a> {
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }
    /// # Safety
    /// Entry code/context remain resident until drain returns a real join.
    pub unsafe fn refresh(
        &mut self,
        app: &App2<'_>,
        resources: &'a Resources<'a>,
        expected: &'static StartupInfo<'static>,
        cache: &mut Cache,
    ) -> bool {
        let mut changed = false;
        if let Some(worker) = &mut self.worker {
            if let Ok(Some((code, mut task))) = worker.collect() {
                if !self.stopping {
                    changed |= cache.complete(
                        task.request,
                        if code == 0 {
                            task.result.take().unwrap_or(Refresh::Retry)
                        } else {
                            Refresh::Retry
                        },
                    );
                }
                self.worker = None;
            }
        }
        if !self.stopping && self.worker.is_none() {
            if let Some(request) = cache.take_request() {
                let task = Load {
                    request,
                    raw: app.start_context() as *const a::generation2::R4XStartContext2 as usize,
                    expected,
                    result: None,
                };
                match unsafe { Worker::start(resources, task, 256 * 1024) } {
                    Ok(worker) => self.worker = Some(worker),
                    Err(error) => changed |= cache.complete(error.task.request, Refresh::Retry),
                }
            }
        }
        changed
    }
    pub fn drain(&mut self, now: u64) -> bool {
        self.stopping = true;
        if let Some(worker) = &mut self.worker {
            worker.stop(now);
            if matches!(worker.collect(), Ok(Some(_))) {
                self.worker = None;
            }
        }
        self.worker.is_none()
    }
}
