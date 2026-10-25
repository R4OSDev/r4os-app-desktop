//! Calendar/config reads are admitted once per monotonic second. The kernel
//! owns the wallclock anchor; this owner retains only presentation and its due.
use super::{configuration::bytes_z, persistence::Store};
use crate::runtime::library::{required, scalar};
use core::mem::MaybeUninit;
use r4os_library_helpers::{Error, raw as c};
use r4os_sdk::abi as a;
const PATH: &[u8] = b"C:\\R4OS\\CONFIG\\TIME.R4S";
const SECOND: u64 = 1_000_000_000;
pub struct Clock {
    config: c::R4StdTimeHelpersConfig,
    label: [u8; 9],
    next_ns: u64,
    last_ns: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(day: u8, hour: u8, minute: u8) -> a::TimeState {
        a::TimeState {
            valid: 1,
            year: 2026,
            month: 3,
            day,
            hour,
            minute,
            seconds_since_midnight: u32::from(hour) * 3600 + u32::from(minute) * 60,
            ..a::TimeState::default()
        }
    }
    #[test]
    fn hot_progress_never_calls_calendar_or_preferences_before_the_due() {
        crate::standard_fixture::with(|_| {
            let mut clock = Clock::new().unwrap();
            assert!(
                clock
                    .refresh(100, |_| Ok(false), || Some(state(28, 23, 59)))
                    .unwrap()
            );
            assert_eq!(clock.label(), b"23:59");
            for now in 101..1101 {
                assert!(
                    !clock
                        .refresh(
                            now,
                            |_| panic!("hot preferences read"),
                            || panic!("hot calendar read")
                        )
                        .unwrap()
                );
            }
            assert_eq!(clock.wait_ticks(SECOND / 2 + 100, 1000, 1000), 500);
            assert_eq!(clock.wait_ticks(SECOND / 2 + 100, 250, 1000), 125);
            assert_eq!(clock.wait_ticks(SECOND + 99, 250, 1000), 1);
            assert!(
                clock
                    .refresh(SECOND + 100, |_| Ok(false), || Some(state(29, 0, 0)))
                    .unwrap()
            );
            assert_eq!(clock.label(), b"00:00");
            // Explicit wallclock changes are observed at the next due, without
            // deriving a second calendar anchor from the previous displayed time.
            assert!(
                clock
                    .refresh(2 * SECOND + 100, |_| Ok(false), || Some(state(20, 12, 34)))
                    .unwrap()
            );
            assert_eq!(clock.label(), b"12:34");
        });
    }
    #[test]
    fn invalid_calendar_and_clock_regression_do_not_freeze_a_valid_label() {
        crate::standard_fixture::with(|_| {
            let mut clock = Clock::new().unwrap();
            clock
                .refresh(SECOND, |_| Ok(false), || Some(state(28, 12, 34)))
                .unwrap();
            assert!(
                clock
                    .refresh(2 * SECOND, |_| Ok(false), || Some(a::TimeState::default()))
                    .unwrap()
            );
            assert_eq!(clock.label(), b"--:--");
            assert!(!clock.refresh(3 * SECOND, |_| Ok(false), || None).unwrap());
            assert!(
                clock
                    .refresh(1, |_| Ok(false), || Some(state(29, 8, 5)))
                    .unwrap()
            );
            assert_eq!(clock.label(), b"08:05");
        });
    }
    #[test]
    fn current_date_still_controls_timezone_and_format_after_reload() {
        crate::standard_fixture::with(|_| {
            let mut clock = Clock::new().unwrap();
            let config = b"R4S_FORMAT=1\nSCHEMA=TIME\nTIMEZONE=Europe/Berlin\n";
            let load = |clock: &mut Clock| {
                assert_eq!(
                    scalar(unsafe {
                        c::R4StdTimeHelpersConfig_loadFromBytes(
                            0,
                            &mut clock.config,
                            config.as_ptr(),
                            config.len() as u64,
                        )
                    })
                    .unwrap(),
                    1
                );
                Ok(true)
            };
            clock.refresh(0, load, || Some(state(28, 12, 34))).unwrap();
            assert_eq!(clock.label(), b"13:34");
            clock
                .refresh(SECOND, |_| Ok(false), || Some(state(29, 12, 34)))
                .unwrap();
            assert_eq!(clock.label(), b"14:34");
            clock
                .refresh(
                    2 * SECOND,
                    |clock| {
                        crate::runtime::library::result(unsafe {
                            c::R4StdTimeHelpersConfig_setClockFormat(
                                0,
                                &mut clock.config,
                                a::clock_format_12h,
                            )
                        })?;
                        Ok(true)
                    },
                    || Some(state(29, 12, 34)),
                )
                .unwrap();
            assert_eq!(clock.label(), b"02:34 PM");
        });
    }
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
            label: [b'-', b'-', b':', b'-', b'-', 0, 0, 0, 0],
            next_ns: 0,
            last_ns: 0,
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
    fn reload(&mut self, store: &Store<'_, '_>) -> Result<bool, Error> {
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
        Ok(true)
    }
    pub fn update(
        &mut self,
        store: &Store<'_, '_>,
        app: &r4os_sdk::App<'_>,
        now_ns: u64,
    ) -> Result<bool, Error> {
        self.refresh(
            now_ns,
            |clock| clock.reload(store),
            || r4os_sdk::time::Clock::new(app).state().ok(),
        )
    }
    /// Include the calendar due in the ordinary activity wait. Read the live
    /// tick scale at the caller; a timer-frequency change cannot delay refresh.
    pub fn wait_ticks(&self, now_ns: u64, hz: u32, maximum: u64) -> u64 {
        let delta = self.next_ns.saturating_sub(now_ns);
        let ticks = (u128::from(delta) * u128::from(hz)).div_ceil(u128::from(SECOND));
        ticks.max(1).min(u128::from(maximum)) as u64
    }
    fn refresh(
        &mut self,
        now_ns: u64,
        reload: impl FnOnce(&mut Self) -> Result<bool, Error>,
        calendar: impl FnOnce() -> Option<a::TimeState>,
    ) -> Result<bool, Error> {
        if now_ns < self.last_ns {
            self.next_ns = 0;
        }
        self.last_ns = now_ns;
        if now_ns < self.next_ns {
            return Ok(false);
        }
        self.next_ns = now_ns.saturating_add(SECOND);
        let reloaded = reload(self)?;
        let Some(state) = calendar().filter(|state| state.valid != 0) else {
            let unknown = [b'-', b'-', b':', b'-', b'-', 0, 0, 0, 0];
            let changed = self.label != unknown;
            self.label = unknown;
            return Ok(changed || reloaded);
        };
        let offset = scalar(unsafe {
            c::R4StdTimeHelpersConfig_offsetMinutesForState(
                0,
                &self.config,
                (&state as *const a::TimeState).cast(),
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
