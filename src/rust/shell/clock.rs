//! Once-per-second taskbar clock using the live kernel time and cached R4STD
//! timezone/format preferences. No per-frame TIMESVC task/RPC is created.
use super::{configuration::bytes_z, persistence::Store};
use crate::runtime::library::{required, scalar};
use core::mem::MaybeUninit;
use r4os_library_helpers::{Error, raw as c};
use r4os_sdk::abi as a;
const PATH: &[u8] = b"C:\\R4OS\\CONFIG\\TIME.R4S";
pub struct Clock {
    config: c::R4StdTimeHelpersConfig,
    label: [u8; 9],
    pub next_tick: u64,
    next_reload: u64,
}
fn view<'a>(slot: c::R4LibrariesSlot, bytes: &'a [u8]) -> Result<&'a [u8], Error> {
    let slot = required(slot)?;
    let start = bytes.as_ptr() as usize;
    let pointer = slot.ptr as usize;
    let count = usize::try_from(slot.length).map_err(|_| Error::InvalidProvider)?;
    if pointer < start
        || pointer
            .checked_add(count)
            .is_none_or(|end| end > start + bytes.len())
    {
        return Err(Error::InvalidProvider);
    }
    Ok(&bytes[pointer - start..pointer - start + count])
}
impl Clock {
    pub fn new() -> Result<Self, Error> {
        let mut config = MaybeUninit::uninit();
        required(unsafe { c::R4StdTimeHelpersConfig_create(config.as_mut_ptr()) })?;
        Ok(Self {
            config: unsafe { config.assume_init() },
            label: [b'0', b'0', b':', b'0', b'0', 0, 0, 0, 0],
            next_tick: 0,
            next_reload: 0,
        })
    }
    pub fn label(&self) -> &[u8] {
        bytes_z(&self.label)
    }
    pub fn load(&mut self, store: &Store<'_, '_>, state: &a::TimeState) -> bool {
        if store.recover(PATH).is_err() {
            return false;
        }
        let mut bytes = [0; 768];
        match store.read(PATH, &mut bytes) {
            Ok(None) => {
                let mut bytes = [0; 384];
                let encoded = unsafe {
                    c::R4StdTimeHelpersConfig_writeToForState(
                        0,
                        &self.config,
                        bytes.as_mut_ptr(),
                        bytes.len() as u64,
                        (state as *const a::TimeState).cast(),
                    )
                };
                view(encoded, &bytes)
                    .ok()
                    .filter(|v| !v.is_empty())
                    .is_some_and(|v| store.save(PATH, v).is_ok())
            }
            Ok(Some(0)) => true,
            Ok(Some(n)) if n <= bytes.len() => scalar(unsafe {
                c::R4StdTimeHelpersConfig_loadFromBytes(
                    0,
                    &mut self.config,
                    bytes.as_ptr(),
                    n as u64,
                )
            })
            .is_ok_and(|v| v != 0),
            _ => false,
        }
    }
    fn reload(&mut self, store: &Store<'_, '_>, now: u64, hz: u32) -> Result<bool, Error> {
        if now < self.next_reload {
            return Ok(false);
        }
        self.next_reload = now.saturating_add(u64::from(hz.max(1)));
        let mut next = self.config;
        let mut bytes = [0; 768];
        let Ok(Some(n)) = store.read(PATH, &mut bytes) else {
            return Ok(false);
        };
        if n == 0
            || n > bytes.len()
            || scalar(unsafe {
                c::R4StdTimeHelpersConfig_loadFromBytes(0, &mut next, bytes.as_ptr(), n as u64)
            })? == 0
        {
            return Ok(false);
        }
        let current_zone =
            scalar(unsafe { c::R4StdTimeHelpersConfig_selectedIndex(0, &self.config) })?;
        let next_zone = scalar(unsafe { c::R4StdTimeHelpersConfig_selectedIndex(0, &next) })?;
        let current_format =
            scalar(unsafe { c::R4StdTimeHelpersConfig_selectedClockFormat(0, &self.config) })?;
        let next_format =
            scalar(unsafe { c::R4StdTimeHelpersConfig_selectedClockFormat(0, &next) })?;
        if current_zone == next_zone && current_format == next_format {
            return Ok(false);
        }
        self.config = next;
        self.next_tick = 0;
        Ok(true)
    }
    pub fn update(
        &mut self,
        store: &Store<'_, '_>,
        state: &a::TimeState,
        now: u64,
        hz: u32,
    ) -> Result<bool, Error> {
        let reloaded = self.reload(store, now, hz)?;
        if now < self.next_tick {
            return Ok(reloaded);
        }
        self.next_tick = now.saturating_add(u64::from(hz.max(1)));
        let offset = scalar(unsafe {
            c::R4StdTimeHelpersConfig_offsetMinutesForState(
                0,
                &self.config,
                (state as *const a::TimeState).cast(),
            )
        })? as i16;
        let seconds =
            scalar(unsafe { c::R4StdTime_secondsInZone(0, state.seconds_since_midnight, offset) })?
                as u32;
        let format =
            scalar(unsafe { c::R4StdTimeHelpersConfig_selectedClockFormat(0, &self.config) })?
                as u32;
        let mut time = MaybeUninit::uninit();
        required(unsafe { c::R4StdTime_splitTime(0, seconds, time.as_mut_ptr()) })?;
        let mut next = [0; 9];
        let formatted = unsafe {
            c::R4StdTime_formatHmDisplay(
                0,
                next.as_mut_ptr(),
                next.len() as u64,
                time.as_ptr(),
                format,
            )
        };
        let value = view(formatted, &next)?;
        if value.is_empty() || value == self.label() {
            return Ok(reloaded);
        }
        let mut label = [0; 9];
        if value.len() >= label.len() {
            return Err(Error::InvalidProvider);
        }
        label[..value.len()].copy_from_slice(value);
        self.label = label;
        Ok(true)
    }
}
