//! Main-task lifecycle for capture jobs. Completed payloads return only after
//! native join; failed file/snapshot retirement remains retryable here.
use super::{
    recording,
    screenshot::{Result as ScreenshotResult, Screenshot},
};
use crate::runtime::worker::Worker;
use r4os_sdk::{App, StartupInfo, resources::Resources};
pub struct Owner<'a> {
    app: &'a App<'a>,
    resources: &'a Resources<'a>,
    expected: &'static StartupInfo<'static>,
    pub recorder: Option<recording::Manager<'a>>,
    screenshot: Option<Worker<'a, Screenshot>>,
    pub last: Option<ScreenshotResult>,
    pub recording_report: Option<recording::Report>,
}
impl<'a> Owner<'a> {
    pub fn new(
        app: &'a App<'a>,
        resources: &'a Resources<'a>,
        expected: &'static StartupInfo<'static>,
    ) -> Self {
        Self {
            app,
            resources,
            expected,
            recorder: recording::Manager::new(app, resources, expected),
            screenshot: None,
            last: None,
            recording_report: None,
        }
    }
    pub fn recording_state(&self) -> u32 {
        self.recorder.as_ref().map_or(0, |owner| {
            if owner.stopping() {
                2
            } else if owner.active() {
                1
            } else {
                0
            }
        })
    }
    pub fn screenshot_busy(&self) -> bool {
        self.screenshot.is_some() || self.last.as_ref().is_some_and(ScreenshotResult::retained)
    }
    /// # Safety
    /// Drain this owner before returning from the R4X entry.
    pub unsafe fn screenshot(&mut self, path: &[u8]) -> bool {
        if self.screenshot_busy() {
            return false;
        }
        self.last = None;
        match unsafe { Screenshot::start(self.app, self.resources, self.expected, path) } {
            Ok(worker) => {
                self.screenshot = Some(worker);
                true
            }
            Err(_) => false,
        }
    }
    /// # Safety
    /// The exact loader, provider and code remain resident through `drain`.
    pub unsafe fn toggle_recording(&mut self, adapter: u32) -> bool {
        let Some(owner) = &mut self.recorder else {
            return false;
        };
        if owner.active() {
            owner.stop();
            true
        } else {
            unsafe { owner.start(adapter) }
        }
    }
    pub fn cancel_screenshot(&self, time: u64) {
        if let Some(worker) = &self.screenshot {
            worker.stop(time);
        }
    }
    pub fn poll(&mut self) -> bool {
        let before = self.recording_state();
        let mut changed = false;
        if let Some(worker) = &mut self.screenshot {
            if let Ok(Some((_, task))) = worker.collect() {
                self.screenshot = None;
                self.last = Some(task.result);
                changed = true;
            }
        }
        if let Some(last) = &mut self.last {
            if last.retained() {
                let _ = last.retry_close(self.app);
            }
        }
        if let Some(owner) = &mut self.recorder {
            if let Some(report) = owner.collect() {
                self.recording_report = Some(report);
                changed = true;
            }
            let _ = owner.retry_close();
        }
        changed || before != self.recording_state()
    }
    pub fn drain(&mut self, time: u64) -> bool {
        self.cancel_screenshot(time);
        if let Some(owner) = &mut self.recorder {
            owner.stop();
        }
        let _ = self.poll();
        if self.screenshot_busy() {
            return false;
        }
        self.recorder
            .as_mut()
            .is_none_or(|owner| owner.retry_close() && owner.finish())
    }
}
