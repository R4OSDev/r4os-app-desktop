//! Original R4GFX refresh/brightness state machines at their generated native
//! boundary. Desktop supplies policy and scene facts; each output keeps its
//! own request cadence, accepted generation and activity history.
use crate::runtime::library::{Context, required, result, scalar};
use core::mem::MaybeUninit;
use r4os_library_helpers::{Error, raw as c};
use r4os_sdk::abi as a;

pub struct Intents {
    refresh: c::R4GfxRefreshClientClient,
    brightness: c::R4GfxBrightnessClientClient,
    activity: c::R4GfxRefreshClientActivity,
}
fn optional<T>(value: Option<&T>) -> c::R4LibrariesSlot {
    let mut slot = unsafe { MaybeUninit::<c::R4LibrariesSlot>::zeroed().assume_init() };
    if let Some(value) = value {
        slot.ptr = (value as *const T).cast_mut().cast();
        slot.flags = 1;
    }
    slot
}
impl Intents {
    pub fn new() -> Result<Self, Error> {
        let mut refresh = MaybeUninit::uninit();
        let mut brightness = MaybeUninit::uninit();
        let mut activity = MaybeUninit::uninit();
        result(unsafe { c::R4Media_refreshInit(0, refresh.as_mut_ptr()) })?;
        result(unsafe { c::R4Media_brightnessInit(0, brightness.as_mut_ptr()) })?;
        required(unsafe { c::R4GfxRefreshClientActivity_create(activity.as_mut_ptr()) })?;
        Ok(Self {
            refresh: unsafe { refresh.assume_init() },
            brightness: unsafe { brightness.assume_init() },
            activity: unsafe { activity.assume_init() },
        })
    }
    pub fn frame(&mut self, now: u64) -> Result<(), Error> {
        result(unsafe { c::R4GfxRefreshClientActivity_frame(0, &mut self.activity, now) })?;
        Ok(())
    }
    pub fn animated(&self, now: u64) -> Result<bool, Error> {
        Ok(scalar(unsafe { c::R4GfxRefreshClientActivity_animated(0, &self.activity, now) })? != 0)
    }
    pub fn step(
        &mut self,
        context: &Context<'_, '_>,
        target: a::GfxOutputTarget,
        identity: a::GfxOutputId,
        refresh: &c::R4GfxRefreshPreferencesChoice,
        brightness: Option<&c::R4GfxBrightnessPreferencesChoice>,
        now: u64,
        scene: u32,
        ready: bool,
        sleeping: bool,
    ) -> Result<(), Error> {
        result(unsafe { c::R4Media_refreshSelect(0, &mut self.refresh, refresh) })?;
        result(unsafe {
            c::R4Media_brightnessSelect(0, &mut self.brightness, optional(brightness))
        })?;
        let draw = context.draw()?;
        result(unsafe {
            c::R4Media_refresh(
                0,
                &mut self.refresh,
                draw,
                (&target as *const a::GfxOutputTarget).cast(),
                now,
                scene,
                ready as u8,
            )
        })?;
        result(unsafe {
            c::R4Media_brightness(
                0,
                &mut self.brightness,
                draw,
                (&identity as *const a::GfxOutputId).cast(),
                now,
                (ready && !sleeping) as u8,
            )
        })?;
        Ok(())
    }
    pub fn release(
        &mut self,
        context: &Context<'_, '_>,
        target: a::GfxOutputTarget,
    ) -> Result<(), Error> {
        result(unsafe {
            c::R4Media_refreshRelease(
                0,
                &mut self.refresh,
                context.draw()?,
                (&target as *const a::GfxOutputTarget).cast(),
            )
        })?;
        Ok(())
    }
}
