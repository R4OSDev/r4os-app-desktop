//! Existing native color admission, signal codecs and one-attempt ledgers.
use super::{catalog::Entry, topology_wire};
use crate::{
    render::output_geometry::topology::Key,
    runtime::library::{Context, required, result, scalar},
};
use core::mem::MaybeUninit;
use r4os_library_helpers::{Error, raw as c};
use r4os_sdk::abi as a;
pub type Choice = c::R4GfxColorPreferencesChoice;
pub fn choice(key: Key) -> Result<Choice, Error> {
    let key = topology_wire::key(key)?;
    let mut out = MaybeUninit::uninit();
    required(unsafe { c::R4GfxColorPreferencesChoice_create(out.as_mut_ptr()) })?;
    result(unsafe { c::R4GfxColorPreferencesChoice_set_key(out.as_mut_ptr(), &key) })?;
    Ok(unsafe { out.assume_init() })
}
pub fn signal(choice: &Choice) -> Result<a::GfxColorSignal, Error> {
    let mut out = a::GfxColorSignal::default();
    required(unsafe {
        c::R4GfxColorPreferencesChoice_get_signal(
            choice,
            (&mut out as *mut a::GfxColorSignal).cast(),
        )
    })?;
    Ok(out)
}
pub fn set_signal(choice: &mut Choice, signal: a::GfxColorSignal) -> Result<(), Error> {
    result(unsafe {
        c::R4GfxColorPreferencesChoice_set_signal(
            choice,
            (&signal as *const a::GfxColorSignal).cast(),
        )
    })?;
    result(unsafe { c::R4GfxColorPreferencesChoice_validate(0, choice) })?;
    Ok(())
}
pub fn sdr() -> Result<a::GfxColorSignal, Error> {
    signal(&choice(Key::default())?)
}
pub fn canonical(state: a::GfxOutputColorState) -> Result<bool, Error> {
    Ok(scalar(unsafe {
        c::R4GfxColorControl_canonical(0, (&state as *const a::GfxOutputColorState).cast())
    })? != 0)
}
pub fn matches(state: a::GfxOutputColorState, signal: a::GfxColorSignal) -> Result<bool, Error> {
    Ok(scalar(unsafe {
        c::R4GfxColorControl_matches(
            0,
            (&state as *const a::GfxOutputColorState).cast(),
            (&signal as *const a::GfxColorSignal).cast(),
        )
    })? != 0)
}
pub fn profile_signal(signal: a::GfxColorSignal) -> Result<bool, Error> {
    Ok(scalar(unsafe {
        c::R4Media_profileSignal(0, (&signal as *const a::GfxColorSignal).cast())
    })? != 0)
}
pub fn requested(signal: a::GfxColorSignal) -> Result<(), Error> {
    let mut out = MaybeUninit::uninit();
    required(unsafe {
        c::R4Media_requestedSignal(
            0,
            (&signal as *const a::GfxColorSignal).cast(),
            out.as_mut_ptr(),
        )
    })?;
    Ok(())
}
pub fn validate(
    context: &Context<'_, '_>,
    entry: Entry,
    mode: a::GfxOutputMode,
    signal: a::GfxColorSignal,
) -> Result<(), Error> {
    let entry = entry.native()?;
    result(unsafe {
        c::R4Media_colorValidate(
            0,
            context.draw()?,
            &entry,
            (&mode as *const a::GfxOutputMode).cast(),
            (&signal as *const a::GfxColorSignal).cast(),
        )
    })?;
    Ok(())
}
pub fn published(state: a::GfxOutputColorState) -> Result<a::GfxColorSignal, Error> {
    let mut metadata = MaybeUninit::uninit();
    let mut optional = unsafe { MaybeUninit::<c::R4LibrariesSlot>::zeroed().assume_init() };
    if state.transfer != 1 {
        required(unsafe { c::R4GfxColorSignalMetadata_create(metadata.as_mut_ptr()) })?;
        optional.flags = 1;
        optional.ptr = metadata.as_mut_ptr().cast();
    }
    let mut logical = MaybeUninit::uninit();
    required(unsafe {
        c::R4Media_publishedSignal(
            0,
            (&state as *const a::GfxOutputColorState).cast(),
            optional,
            logical.as_mut_ptr(),
        )
    })?;
    let mut out = a::GfxColorSignal::default();
    required(unsafe {
        c::R4Media_signalRequest(
            0,
            logical.as_ptr(),
            (&mut out as *mut a::GfxColorSignal).cast(),
        )
    })?;
    Ok(out)
}
pub struct Restoration {
    native: c::R4GfxColorControlRestoration,
}
impl Restoration {
    pub fn new() -> Result<Self, Error> {
        let mut out = MaybeUninit::uninit();
        required(unsafe { c::R4GfxColorControlRestoration_create(out.as_mut_ptr()) })?;
        Ok(Self {
            native: unsafe { out.assume_init() },
        })
    }
    pub fn seen(&self, choice: &Choice, identity: a::GfxOutputId) -> Result<bool, Error> {
        Ok(scalar(unsafe {
            c::R4GfxColorControlRestoration_seen(
                0,
                &self.native,
                choice,
                (&identity as *const a::GfxOutputId).cast(),
            )
        })? != 0)
    }
    pub fn record(&mut self, choice: &Choice, identity: a::GfxOutputId) -> Result<(), Error> {
        result(unsafe {
            c::R4GfxColorControlRestoration_record(
                0,
                &mut self.native,
                choice,
                (&identity as *const a::GfxOutputId).cast(),
            )
        })?;
        Ok(())
    }
}
