//! Existing single-display native composition when the connector manager has
//! no active layout. It shares the native transaction/retirement owner; no
//! alternate GPU engine or second capture implementation lives here.
use super::native::{Output, Progress};
use crate::{
    hosted::{window_image::Memory, window_set::Set},
    render::gpu::{device::Device, receipt::Consumer},
};
use r4os_sdk::{App, c_facades as c, graphics::Graphics};
enum State<'a> {
    Empty,
    Native(Output<'a>),
    Retained(Device),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Cpu,
    Native,
    Pending,
}
pub struct Primary<'a> {
    state: State<'a>,
    attempted: bool,
    managed: bool,
    retiring: bool,
}
impl Default for Primary<'_> {
    fn default() -> Self {
        Self {
            state: State::Empty,
            attempted: false,
            managed: false,
            retiring: false,
        }
    }
}
impl<'a> Primary<'a> {
    pub fn output(&self) -> Option<&Output<'a>> {
        if self.managed || self.retiring {
            None
        } else {
            match &self.state {
                State::Native(o) => Some(o),
                _ => None,
            }
        }
    }
    pub fn output_mut(&mut self) -> Option<&mut Output<'a>> {
        if self.managed || self.retiring {
            None
        } else {
            match &mut self.state {
                State::Native(o) => Some(o),
                _ => None,
            }
        }
    }
    /// # Safety
    /// The actual loader/providers remain resident through `close`.
    pub unsafe fn poll<M: Memory>(
        &mut self,
        app: &App<'_>,
        graphics: &Graphics<'_, '_>,
        windows: &mut Set<M, Consumer>,
        managed: bool,
        time: u64,
    ) -> (bool, bool) {
        if self.managed && !managed {
            self.attempted = false
        }
        self.managed = managed;
        if managed {
            let _ = self.close(app, graphics, windows, time);
            return (false, false);
        }
        if self.retiring && !self.close(app, graphics, windows, time) {
            return (false, false);
        }
        if matches!(self.state, State::Empty) && !self.attempted {
            self.attempted = true;
            self.state = match unsafe { Output::open(app, 0, None, None, r4gfx::format_xrgb8888) } {
                Ok(output) => State::Native(output),
                Err(error) => error.retained.map_or(State::Empty, State::Retained),
            };
        }
        match &mut self.state {
            State::Native(output) => {
                let progress = output.poll(app, windows, graphics, time);
                (
                    progress == Progress::Failed || output.capture_redraw(time),
                    progress == Progress::Visible,
                )
            }
            State::Retained(device) => {
                if device.close(app).unwrap_or(false) {
                    self.state = State::Empty
                }
                (false, false)
            }
            State::Empty => (false, false),
        }
    }
    pub fn admit(
        &mut self,
        app: &App<'_>,
        graphics: &Graphics<'_, '_>,
        revision: u64,
        time: u64,
        input_ns: u64,
    ) -> Admission {
        if self.retiring {
            return Admission::Pending;
        }
        let Some(output) = self.output_mut() else {
            return Admission::Cpu;
        };
        let slot = unsafe {
            c::R4SdkGuiDrawContext_supportsDisplayPresentationStats(0, graphics.native_draw())
        };
        if slot.status == 0
            && slot.flags & 2 == 0
            && slot.bits != 0
            && output.available(app, revision, time, input_ns)
        {
            return Admission::Native;
        }
        if output.blocks_capture(time) {
            Admission::Pending
        } else {
            Admission::Cpu
        }
    }
    pub fn needs_polling(&self) -> bool {
        self.retiring
            || self.output().is_some_and(Output::needs_polling)
            || matches!(self.state, State::Retained(_))
    }
    pub fn immediate_work(&self) -> bool {
        self.output().is_some_and(Output::immediate_work)
    }
    pub fn wait_ticks(&self, time: u64, hz: u32, limit: u64) -> u64 {
        self.output()
            .map_or(limit, |o| o.capture_wait_ticks(time, hz, limit))
    }
    pub fn close<M: Memory>(
        &mut self,
        app: &App<'_>,
        graphics: &Graphics<'_, '_>,
        windows: &mut Set<M, Consumer>,
        time: u64,
    ) -> bool {
        let closed = match &mut self.state {
            State::Empty => true,
            State::Native(o) => o.close(app, windows, graphics, time),
            State::Retained(d) => d.close(app).unwrap_or(false),
        };
        self.retiring = !closed;
        if closed {
            self.state = State::Empty
        }
        closed
    }
}
