//! AUDSVC interaction and current taskbar intent; geometry and gain remain in
//! volume.rs, service wire validation and handles in the ordinary SDK.
use super::volume::{self, OutputPage, OutputRow, View};
use crate::runtime::wire::Wire;
use core::mem::MaybeUninit;
use r4os_sdk::{abi as a, c_facades as c, platform::Audio};
pub struct Owner {
    pub ui: View<'static>,
    pub dragging: bool,
    pub page: OutputPage<'static>,
    outputs: a::AudioServiceOutputState,
    pub master: a::AudioServiceMasterState,
    have_page: bool,
    next_poll: u64,
    next_send: u64,
    pending_flags: u32,
    pending_percent: u8,
    pending_muted: bool,
}
impl Default for Owner {
    fn default() -> Self {
        Self {
            ui: View::default(),
            dragging: false,
            page: OutputPage::default(),
            outputs: a::AudioServiceOutputState::default(),
            master: a::AudioServiceMasterState::default(),
            have_page: false,
            next_poll: 0,
            next_send: 0,
            pending_flags: 0,
            pending_percent: 100,
            pending_muted: false,
        }
    }
}
fn valid(value: c::R4SdkGuiSlot) -> bool {
    value.status == 0 && value.flags & 2 == 0
}
fn accepted(value: *const c::R4SdkAudioResult) -> bool {
    let tag = unsafe { c::R4SdkAudioResult_inspect(value) };
    valid(tag) && tag.flags & 1 != 0 && tag.bits == c::R4SdkAudioResult_TAG_ok as u64
}
fn present(bits: Option<u64>) -> c::R4SdkGuiSlot {
    let mut slot = unsafe { MaybeUninit::<c::R4SdkGuiSlot>::zeroed().assume_init() };
    if let Some(bits) = bits {
        slot.flags = 1;
        slot.bits = bits;
    }
    slot
}
fn same(a: View<'_>, b: View<'_>) -> bool {
    a.installed == b.installed
        && a.reachable == b.reachable
        && a.muted == b.muted
        && a.percent == b.percent
        && a.popup_open == b.popup_open
        && a.output_serial == b.output_serial
}
fn master(
    audio: &Audio<'_>,
    volume: Option<u32>,
    muted: Option<bool>,
) -> Option<a::AudioServiceMasterState> {
    let timeout = unsafe {
        c::r4_timeout_finite(a::R4Duration {
            nanoseconds: 100_000_000,
        })
    };
    let mut out = a::AudioServiceMasterState::default();
    let mut result = MaybeUninit::uninit();
    let slot = if volume.is_none() && muted.is_none() {
        unsafe {
            c::R4SdkAudioAudio_masterState(
                0,
                audio.native(),
                &timeout,
                &mut out,
                result.as_mut_ptr(),
            )
        }
    } else {
        let mut update = MaybeUninit::uninit();
        if !valid(unsafe {
            c::R4SdkAudioMasterUpdate_from_fields(
                update.as_mut_ptr(),
                present(volume.map(u64::from)),
                present(muted.map(u64::from)),
                0,
            )
        }) {
            return None;
        }
        unsafe {
            c::R4SdkAudioAudio_setMasterState(
                0,
                audio.native(),
                update.as_ptr(),
                &timeout,
                &mut out,
                result.as_mut_ptr(),
            )
        }
    };
    (valid(slot) && accepted(result.as_ptr())).then_some(out)
}
fn outputs(
    audio: &Audio<'_>,
    request: &a::AudioServiceOutputRequest,
    select: bool,
) -> Option<a::AudioServiceOutputState> {
    let timeout = unsafe {
        c::r4_timeout_finite(a::R4Duration {
            nanoseconds: 1_500_000_000,
        })
    };
    let mut out = a::AudioServiceOutputState::default();
    let mut result = MaybeUninit::uninit();
    let slot = unsafe {
        if select {
            c::R4SdkAudioAudio_selectOutput(
                0,
                audio.native(),
                request,
                &timeout,
                &mut out,
                result.as_mut_ptr(),
            )
        } else {
            c::R4SdkAudioAudio_outputs(
                0,
                audio.native(),
                request,
                &timeout,
                &mut out,
                result.as_mut_ptr(),
            )
        }
    };
    (valid(slot) && accepted(result.as_ptr())).then_some(out)
}
impl Owner {
    /// Explicit Spectra qualification only. Always attempt the exact original
    /// audible/selected/mute restoration, including a failed intermediate call.
    pub(crate) fn qualify_master(&mut self, audio: &Audio<'_>) -> bool {
        let Some(original) = master(audio, None, None) else {
            return false;
        };
        self.accept_master(original);
        let checked = (|| {
            let muted = master(audio, Some(volume::gain_for_percent(35)), Some(true))?;
            if muted.flags & a::audio_master_state_flag_muted == 0
                || muted.selected_volume_fixed != volume::gain_for_percent(35)
                || muted.effective_volume_fixed != 0
            {
                return None;
            }
            let audible = master(audio, Some(volume::gain_for_percent(40)), None)?;
            (audible.flags & a::audio_master_state_flag_muted == 0
                && audible.selected_volume_fixed == volume::gain_for_percent(40)
                && audible.effective_volume_fixed == audible.selected_volume_fixed)
                .then_some(())
        })()
        .is_some();
        let _ = master(audio, Some(original.last_audible_volume_fixed), None);
        let restored = master(
            audio,
            Some(original.selected_volume_fixed),
            Some(original.flags & a::audio_master_state_flag_muted != 0),
        );
        let correct = restored.is_some_and(|v| {
            v.selected_volume_fixed == original.selected_volume_fixed
                && v.last_audible_volume_fixed == original.last_audible_volume_fixed
                && v.flags & a::audio_master_state_flag_muted
                    == original.flags & a::audio_master_state_flag_muted
        });
        if let Some(restored) = restored {
            self.accept_master(restored)
        }
        checked && correct
    }
    pub fn poll_due(&self, now: u64) -> bool {
        now >= self.next_poll
    }
    pub fn view(&self) -> View<'_> {
        View {
            outputs: if self.have_page {
                Some(&self.page)
            } else {
                None
            },
            ..self.ui
        }
    }
    pub fn synchronize(
        &mut self,
        audio: Option<&Audio<'_>>,
        installed: bool,
        now: u64,
        hz: u32,
    ) -> bool {
        let mut changed = false;
        if self.pending_flags != 0 && now >= self.next_send {
            changed |= self.flush(audio, now, hz, false);
        }
        if now < self.next_poll {
            return changed;
        }
        self.next_poll = now.saturating_add(u64::from(hz.max(1)).div_ceil(2));
        let old = self.ui;
        self.ui.installed = installed;
        if !installed {
            self.ui.reachable = false;
            self.ui.popup_open = false;
            self.dragging = false;
            self.pending_flags = 0;
        } else if let Some(state) = audio.and_then(|audio| master(audio, None, None)) {
            self.accept_master(state);
            if self.ui.popup_open {
                changed |= self.sync_outputs(audio);
            }
        } else {
            self.ui.reachable = false;
        }
        changed || !same(old, self.ui)
    }
    fn accept_master(&mut self, state: a::AudioServiceMasterState) {
        self.master = state;
        self.ui.installed = true;
        self.ui.reachable = true;
        self.ui.muted = state.flags & a::audio_master_state_flag_muted != 0;
        self.ui.percent = volume::percent_for_gain(state.selected_volume_fixed);
    }
    pub fn sync_outputs(&mut self, audio: Option<&Audio<'_>>) -> bool {
        let request = a::AudioServiceOutputRequest {
            index: self.page.index,
            ..Default::default()
        };
        let Some(state) = audio.and_then(|audio| outputs(audio, &request, false)) else {
            if !self.page.supported && self.have_page {
                return false;
            }
            self.page.supported = false;
            self.page.reason = b"Output selection unavailable";
            self.have_page = true;
            self.ui.output_serial = self.ui.output_serial.wrapping_add(1);
            return true;
        };
        let changed = !state.same(&self.outputs) || !self.page.supported;
        self.accept_outputs(state);
        if changed {
            self.ui.output_serial = self.ui.output_serial.wrapping_add(1);
        }
        changed
    }
    fn accept_outputs(&mut self, state: a::AudioServiceOutputState) {
        self.outputs = state;
        let failed = self.page.failed;
        self.page = OutputPage {
            supported: state.reason != a::audio_output_reason_api_unavailable,
            automatic: state.desired_id[0] == 0,
            pending: state.flags & a::audio_output_state_flag_persist_pending != 0,
            save_error: state.flags & a::audio_output_state_flag_config_error != 0,
            failed,
            index: state.index,
            total: state.total,
            count: state.count.min(volume::OUTPUT_ROWS as u32),
            active_name: state.active_name,
            reason: reason(state.reason),
            ..Default::default()
        };
        for (i, item) in state.outputs[..self.page.count as usize].iter().enumerate() {
            self.page.rows[i] = OutputRow {
                id: item.id,
                name: item.name,
                available: item.availability == a::audio_output_available,
                active: item.flags & a::audio_output_flag_active != 0,
                preferred: state.desired_id == item.id,
                detail: detail(*item),
            };
        }
        self.have_page = true;
    }
    pub fn select(&mut self, audio: Option<&Audio<'_>>, id: [u8; 64]) {
        if !self.ui.reachable || !self.page.supported {
            return;
        }
        let request = a::AudioServiceOutputRequest {
            id,
            service_epoch: self.outputs.service_epoch,
            revision: self.outputs.revision,
            ..Default::default()
        };
        let state = audio.and_then(|audio| outputs(audio, &request, true));
        self.page.failed = state.is_none();
        if let Some(state) = state {
            self.accept_outputs(state);
        } else {
            self.sync_outputs(audio);
        }
        self.ui.output_serial = self.ui.output_serial.wrapping_add(1);
    }
    pub fn change_page(&mut self, audio: Option<&Audio<'_>>, next: bool) {
        if next {
            if self.page.index + self.page.count >= self.page.total {
                return;
            }
            self.page.index += volume::OUTPUT_ROWS as u32;
        } else {
            self.page.index = self.page.index.saturating_sub(volume::OUTPUT_ROWS as u32);
        }
        self.page.failed = false;
        self.sync_outputs(audio);
    }
    pub fn set_percent(
        &mut self,
        audio: Option<&Audio<'_>>,
        percent: u8,
        immediate: bool,
        now: u64,
        hz: u32,
    ) -> bool {
        if !self.ui.installed || !self.ui.reachable {
            return false;
        }
        self.ui.percent = percent.min(100);
        if self.ui.percent != 0 {
            self.ui.muted = false;
        }
        if self.pending_flags == 0 {
            self.next_send = now;
        }
        self.pending_flags |= a::audio_master_request_flag_set_volume;
        self.pending_percent = self.ui.percent;
        if immediate || now >= self.next_send {
            self.flush(audio, now, hz, immediate);
        }
        true
    }
    pub fn toggle_mute(&mut self, audio: Option<&Audio<'_>>, now: u64, hz: u32) -> bool {
        if !self.ui.installed || !self.ui.reachable {
            return false;
        }
        self.ui.muted = !self.ui.muted;
        if self.pending_flags == 0 {
            self.next_send = now;
        }
        self.pending_flags |= a::audio_master_request_flag_set_muted;
        self.pending_muted = self.ui.muted;
        self.flush(audio, now, hz, true);
        true
    }
    pub fn flush(&mut self, audio: Option<&Audio<'_>>, now: u64, hz: u32, force: bool) -> bool {
        if self.pending_flags == 0 || !self.ui.installed || (!force && now < self.next_send) {
            return false;
        }
        let old = self.ui;
        let volume = (self.pending_flags & a::audio_master_request_flag_set_volume != 0)
            .then(|| volume::gain_for_percent(self.pending_percent));
        let muted = (self.pending_flags & a::audio_master_request_flag_set_muted != 0)
            .then_some(self.pending_muted);
        self.pending_flags = 0;
        self.next_send = now.saturating_add(u64::from(hz.max(1)).div_ceil(20));
        if let Some(state) = audio.and_then(|audio| master(audio, volume, muted)) {
            self.accept_master(state);
        } else {
            self.ui.reachable = false;
        }
        self.next_poll = now.saturating_add(u64::from(hz.max(1)).div_ceil(2));
        !same(old, self.ui)
    }
}
fn reason(value: u32) -> &'static [u8] {
    match value {
        a::audio_output_reason_preferred => b"Saved output selected",
        a::audio_output_reason_auto_hdmi => b"Automatic: connected HDMI",
        a::audio_output_reason_auto_display_port => b"Automatic: connected DisplayPort",
        a::audio_output_reason_auto_analog => b"Automatic: analog output",
        a::audio_output_reason_preferred_unavailable => b"Saved output unavailable; using fallback",
        a::audio_output_reason_none_available => b"No available audio output",
        a::audio_output_reason_activation_failed => b"Activation failed; previous output retained",
        a::audio_output_reason_catalog_busy => b"Devices busy; showing previous list",
        _ => b"Output selection unavailable",
    }
}
fn detail(item: a::AudioOutputInfo) -> &'static [u8] {
    match item.availability {
        a::audio_output_available => match item.kind {
            a::audio_output_kind_hdmi => b"HDMI | connected",
            a::audio_output_kind_headphone => b"Headphones | available",
            a::audio_output_kind_speaker => b"Speakers | available",
            _ => b"Line out | available",
        },
        a::audio_output_waiting_for_eld => b"Receiver detected; waiting for capabilities",
        a::audio_output_invalid_eld => b"Receiver capabilities unavailable",
        a::audio_output_unsupported => b"Receiver format not supported",
        a::audio_output_failed => b"Device could not be activated",
        _ => b"No receiver detected",
    }
}
