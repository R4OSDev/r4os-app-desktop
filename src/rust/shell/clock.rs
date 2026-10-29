//! Visible calendar reads are admitted once per monotonic second. File checks
//! deliver closed async snapshots; unchanged bytes do not re-enter the parser.
//! The kernel owns the wallclock anchor; this owner retains presentation/due.
use super::{configuration::bytes_z, persistence::Store};
use crate::runtime::library::{required, scalar};
use core::mem::MaybeUninit;
use r4os_library_helpers::{Error, raw as c};
use r4os_sdk::abi as a;
pub(crate) const PATH: &[u8] = b"C:\\R4OS\\CONFIG\\TIME.R4S";
const SECOND: u64 = 1_000_000_000;
pub(crate) const CONFIG_BYTES: usize = 768;
pub struct Clock {
    config: c::R4StdTimeHelpersConfig,
    label: [u8; 9],
    next_ns: u64,
    last_ns: u64,
    visible: bool,
    config_dirty: bool,
    last_config: [u8; CONFIG_BYTES],
    last_config_len: Option<usize>,
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
    #[test]
    fn unchanged_file_bytes_do_not_repeat_native_config_parse() {
        crate::standard_fixture::with(|_| {
            let mut clock = Clock::new().unwrap();
            let bytes = b"R4S_FORMAT=1\nSCHEMA=TIME\nTIMEZONE=Europe/Berlin\n";
            let mut calls = 0;
            for _ in 0..5 {
                clock
                    .apply_bytes_with(bytes, |next, source| {
                        calls += 1;
                        scalar(unsafe {
                            c::R4StdTimeHelpersConfig_loadFromBytes(
                                0,
                                next,
                                source.as_ptr(),
                                source.len() as u64,
                            )
                        })
                        .map(|value| value != 0)
                    })
                    .unwrap();
            }
            assert_eq!(
                calls, 1,
                "unchanged file bytes must not re-enter the native configuration parser"
            );
            clock
                .refresh(0, |_| Ok(false), || Some(state(28, 12, 34)))
                .unwrap();
            assert_eq!(clock.label(), b"13:34");
        });
    }
    #[test]
    fn hidden_clock_does_not_load_or_arm_a_due_and_show_uses_the_live_calendar() {
        crate::standard_fixture::with(|_| {
            let mut clock = Clock::new().unwrap();
            clock
                .refresh(0, |_| Ok(false), || Some(state(28, 12, 34)))
                .unwrap();
            clock.set_visible(false);
            for now in [SECOND, 2 * SECOND, 20 * SECOND] {
                assert!(
                    !clock
                        .refresh(
                            now,
                            |_| panic!("hidden preferences"),
                            || panic!("hidden calendar")
                        )
                        .unwrap()
                );
                assert_eq!(clock.wait_ticks(now, 1000, 500), 500);
            }
            clock.set_visible(true);
            assert!(
                clock
                    .refresh(20 * SECOND, |_| Ok(false), || Some(state(29, 0, 1)))
                    .unwrap()
            );
            assert_eq!(clock.label(), b"00:01");
            // A configuration delivered before the old calendar due must be
            // applied immediately, without replacing the kernel date anchor.
            clock
                .apply_bytes(b"R4S_FORMAT=1\nSCHEMA=TIME\nTIMEZONE=Europe/Berlin\n")
                .unwrap();
            clock
                .refresh(20 * SECOND + 1, |_| Ok(false), || Some(state(29, 12, 34)))
                .unwrap();
            assert_eq!(clock.label(), b"14:34");
        });
    }
    #[test]
    fn refused_bytes_are_cached_without_overwriting_valid_configuration_and_errors_retry() {
        crate::standard_fixture::with(|_| {
            let mut clock = Clock::new().unwrap();
            clock
                .apply_bytes(b"R4S_FORMAT=1\nSCHEMA=TIME\nTIMEZONE=Europe/Berlin\n")
                .unwrap();
            let utc = b"R4S_FORMAT=1\nSCHEMA=TIME\nTIMEZONE=UTC\n";
            let mut refused = 0;
            for _ in 0..5 {
                assert!(
                    !clock
                        .apply_bytes_with(b"refused snapshot", |next, _| {
                            refused += 1;
                            // Even a decoder that altered its candidate before
                            // reporting refusal must not overwrite the live value.
                            scalar(unsafe {
                                c::R4StdTimeHelpersConfig_loadFromBytes(
                                    0,
                                    next,
                                    utc.as_ptr(),
                                    utc.len() as u64,
                                )
                            })?;
                            Ok(false)
                        })
                        .unwrap()
                );
            }
            assert_eq!(refused, 1);
            clock
                .refresh(0, |_| Ok(false), || Some(state(28, 12, 34)))
                .unwrap();
            assert_eq!(clock.label(), b"13:34");
            assert!(
                clock
                    .apply_bytes_with(utc, |_, _| Err(Error::InvalidProvider))
                    .is_err()
            );
            let mut retried = 0;
            assert!(
                clock
                    .apply_bytes_with(utc, |next, bytes| {
                        retried += 1;
                        scalar(unsafe {
                            c::R4StdTimeHelpersConfig_loadFromBytes(
                                0,
                                next,
                                bytes.as_ptr(),
                                bytes.len() as u64,
                            )
                        })
                        .map(|value| value != 0)
                    })
                    .unwrap()
            );
            assert_eq!(retried, 1);
            clock
                .refresh(1, |_| Ok(false), || Some(state(28, 12, 34)))
                .unwrap();
            assert_eq!(clock.label(), b"12:34");
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
            visible: true,
            config_dirty: false,
            last_config: [0; CONFIG_BYTES],
            last_config_len: None,
        })
    }
    pub fn label(&self) -> &[u8] {
        bytes_z(&self.label)
    }
    pub fn set_visible(&mut self, visible: bool) {
        if self.visible != visible {
            self.visible = visible;
            self.next_ns = 0;
        }
    }
    fn remember(&mut self, bytes: &[u8]) {
        self.last_config[..bytes.len()].copy_from_slice(bytes);
        self.last_config_len = Some(bytes.len());
    }
    pub fn load(&mut self, store: &Store<'_, '_>, state: &a::TimeState) -> bool {
        if store.recover(PATH).is_err() {
            return false;
        }
        let mut bytes = [0; CONFIG_BYTES];
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
                    .is_some_and(|v| {
                        if store.save(PATH, v).is_err() {
                            return false;
                        }
                        self.remember(v);
                        true
                    })
            }
            Ok(Some(0)) => true,
            Ok(Some(n)) if n <= bytes.len() => {
                let mut next = self.config;
                match scalar(unsafe {
                    c::R4StdTimeHelpersConfig_loadFromBytes(0, &mut next, bytes.as_ptr(), n as u64)
                }) {
                    Ok(valid) => {
                        self.remember(&bytes[..n]);
                        if valid != 0 {
                            self.config = next;
                        }
                        valid != 0
                    }
                    Err(_) => false,
                }
            }
            _ => false,
        }
    }
    pub(crate) fn apply_bytes(&mut self, bytes: &[u8]) -> Result<bool, Error> {
        self.apply_bytes_with(bytes, |next, bytes| {
            scalar(unsafe {
                c::R4StdTimeHelpersConfig_loadFromBytes(0, next, bytes.as_ptr(), bytes.len() as u64)
            })
            .map(|value| value != 0)
        })
    }
    fn apply_bytes_with(
        &mut self,
        bytes: &[u8],
        decode: impl FnOnce(&mut c::R4StdTimeHelpersConfig, &[u8]) -> Result<bool, Error>,
    ) -> Result<bool, Error> {
        if bytes.is_empty() || bytes.len() > CONFIG_BYTES {
            return Ok(false);
        }
        if self.last_config_len == Some(bytes.len()) && self.last_config[..bytes.len()] == *bytes {
            return Ok(false);
        }
        let mut next = self.config;
        let valid = decode(&mut next, bytes)?;
        self.remember(bytes);
        if !valid {
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
        self.config_dirty = true;
        Ok(true)
    }
    pub fn update(&mut self, app: &r4os_sdk::App<'_>, now_ns: u64) -> Result<bool, Error> {
        self.refresh(
            now_ns,
            |_| Ok(false),
            || r4os_sdk::time::Clock::new(app).state().ok(),
        )
    }
    /// Include the calendar due in the ordinary activity wait. Read the live
    /// tick scale at the caller; a timer-frequency change cannot delay refresh.
    pub fn wait_ticks(&self, now_ns: u64, hz: u32, maximum: u64) -> u64 {
        if !self.visible {
            return maximum;
        }
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
        if !self.visible {
            return Ok(false);
        }
        if now_ns < self.last_ns {
            self.next_ns = 0;
        }
        self.last_ns = now_ns;
        if now_ns < self.next_ns && !self.config_dirty {
            return Ok(false);
        }
        self.next_ns = now_ns.saturating_add(SECOND);
        let reloaded = reload(self)?;
        let reconfigured = core::mem::replace(&mut self.config_dirty, false);
        let Some(state) = calendar().filter(|state| state.valid != 0) else {
            let unknown = [b'-', b'-', b':', b'-', b'-', 0, 0, 0, 0];
            let changed = self.label != unknown;
            self.label = unknown;
            return Ok(changed || reloaded || reconfigured);
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
            return Ok(reloaded || reconfigured);
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
