//! Read-only SDK capabilities shared by the main task. Domain owners receive
//! short loans; no provider/callback receives this coordinator or its state.
use crate::{runtime::library, shell::persistence};
use r4gfx::ColorV1Client;
use r4os_library_helpers::standard::Standard;
use r4os_sdk::{
    App, abi as a,
    graphics::Graphics,
    resources::Resources,
    storage::{Error as SdkError, System},
    time::Clock,
};
pub struct Environment<'a> {
    pub app: &'a App<'a>,
    pub graphics: &'a Graphics<'a, 'a>,
    pub resources: &'a Resources<'a>,
    pub standard: &'a Standard<'a>,
    pub colors: &'a ColorV1Client<'a>,
    pub libraries: library::Context<'a, 'a>,
    pub store: persistence::Store<'a, 'a>,
    pub system: System<'a>,
}
#[derive(Debug)]
pub enum Error {
    Library(library::Error),
    Store(persistence::Error),
    System(SdkError),
}
#[derive(Clone, Copy)]
pub struct Time {
    pub tick: u64,
    pub nanoseconds: u64,
    pub hz: u32,
}
pub fn precise(app: &App<'_>) -> Option<u64> {
    Clock::new(app)
        .precise_instant()
        .ok()
        .map(|v| v.nanoseconds)
}
impl<'a> Environment<'a> {
    pub fn new(
        app: &'a App<'a>,
        graphics: &'a Graphics<'a, 'a>,
        resources: &'a Resources<'a>,
        standard: &'a Standard<'a>,
        colors: &'a ColorV1Client<'a>,
    ) -> Result<Self, Error> {
        Ok(Self {
            app,
            graphics,
            resources,
            standard,
            colors,
            libraries: library::Context::new(app).map_err(Error::Library)?,
            store: persistence::Store::new(app).map_err(Error::Store)?,
            system: System::new(app).map_err(Error::System)?,
        })
    }
    pub fn time(&self) -> Result<Time, SdkError> {
        let clock = Clock::new(self.app);
        let current = clock.monotonic()?;
        let tick = clock.ticks()?;
        let hz = current.event_requested_hz;
        if hz == 0 {
            return Err(SdkError::InvalidProvider);
        }
        Ok(Time {
            tick,
            nanoseconds: current.instant_ns,
            hz,
        })
    }
    pub fn boot_ready(&self) -> i32 {
        let Some(table) = self.app.system_api() else {
            return a::err_no_fn;
        };
        let address = table.boot_ready;
        if address == 0 {
            return a::err_no_fn;
        }
        let call: a::R4SysFns::boot_ready = unsafe { core::mem::transmute(address) };
        unsafe { call() }
    }
}
