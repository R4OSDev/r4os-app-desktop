//! AUDSVC interaction and current taskbar intent; geometry and gain remain in
//! volume.rs, service wire validation and handles in the ordinary SDK.
use super::volume::{self, OutputPage, OutputRow, View};
use crate::runtime::{
    audio_io::{AudioIo, Completed, Kind, Reply, Request},
    wire::Wire,
};
use core::mem::MaybeUninit;
use r4os_sdk::{abi as a, c_facades as c, platform::Audio};
#[cfg(test)]
#[path = "../../../Tests/RustNative/audio_requests.rs"]
mod tests;
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
    provider: a::ProgramProcessHandle,
    view_epoch: u64,
    master_intent: u64,
    output_intent: u64,
    wanted_page: u32,
    need_master: bool,
    need_outputs: bool,
    selection: Option<Selection>,
    ready_selection: Option<(u64, u64, u64)>,
    active: Option<Request>,
    accepted_view: u64,
    selection_failed: bool,
}
#[derive(Clone, Copy)]
struct Selection {
    id: [u8; 64],
    intent: u64,
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
            provider: Default::default(),
            view_epoch: 1,
            master_intent: 1,
            output_intent: 1,
            wanted_page: 0,
            need_master: true,
            need_outputs: false,
            selection: None,
            ready_selection: None,
            active: None,
            accepted_view: 0,
            selection_failed: false,
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
impl Owner {
    /// Explicit Spectra qualification only. Always attempt the exact original
    /// audible/selected/mute restoration, including a failed intermediate call.
    pub(crate) fn qualify_master(&mut self, audio: &Audio<'_>) -> bool {
        if self.pending_flags != 0
            || self.selection.is_some()
            || self
                .active
                .is_some_and(|r| matches!(r.kind, Kind::SetMaster | Kind::Select))
        {
            return false;
        }
        self.master_intent = self.master_intent.wrapping_add(1);
        self.need_master = true;
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
    pub fn set_popup(&mut self, open: bool) {
        if self.ui.popup_open == open {
            return;
        }
        self.ui.popup_open = open;
        self.view_epoch = self.view_epoch.wrapping_add(1);
        self.need_outputs = open;
        self.page.loading = open;
        self.page.failed = false;
        self.page.request_error = false;
        self.selection_failed = false;
        if open {
            self.have_page = true;
        }
        self.ui.output_serial = self.ui.output_serial.wrapping_add(1);
    }
    pub fn stop(&mut self) {
        self.set_popup(false);
        self.pending_flags = 0;
        self.selection = None;
        self.ready_selection = None;
        self.need_master = false;
        self.need_outputs = false;
    }
    /// Existing isolated Spectra /AUDIO qualification, through the ordinary
    /// event-loop request. No waiting or diagnostic work on the normal path.
    pub(crate) fn begin_probe(&mut self) -> u64 {
        self.set_popup(true);
        self.view_epoch
    }
    pub(crate) fn probe_result(&self, ticket: u64) -> Option<bool> {
        if self.view_epoch != ticket || !self.ui.popup_open || self.page.request_error {
            return Some(false);
        }
        (self.accepted_view == ticket).then_some(true)
    }
    pub(crate) fn end_probe(&mut self, ticket: u64) {
        if self.view_epoch == ticket {
            self.set_popup(false);
        }
    }
    pub fn wait_ticks(&self, now: u64, delay: u64) -> u64 {
        let mut delay = delay.min(self.next_poll.saturating_sub(now).max(1));
        if self.ui.reachable && self.pending_flags != 0 {
            delay = delay.min(self.next_send.saturating_sub(now).max(1));
        }
        if self.need_master
            || (self.ui.reachable && (self.selection.is_some() || self.need_outputs))
        {
            delay = delay.min(1);
        }
        delay
    }
    pub fn synchronize(
        &mut self,
        io: &mut AudioIo<'_>,
        installed: bool,
        now: u64,
        hz: u32,
    ) -> bool {
        let old = self.ui;
        let old_loading = self.page.loading;
        self.ui.installed = installed;
        if !installed {
            self.stop();
            self.ui.reachable = false;
        }
        if let Some(done) = io.poll() {
            self.active = None;
            if installed {
                self.completed(done, now, hz);
            }
        }
        if now >= self.next_poll {
            self.next_poll = now.saturating_add(u64::from(hz.max(1)).div_ceil(2));
            self.need_master = installed;
            self.need_outputs |= installed && self.ui.popup_open;
        }
        if installed && !io.busy() {
            if let Some(request) = self.next_request(now) {
                match io.start(request, hz) {
                    Ok(true) => {
                        self.active = Some(request);
                        match request.kind {
                            Kind::Master => self.need_master = false,
                            Kind::SetMaster => {
                                self.pending_flags = 0;
                                self.next_send =
                                    now.saturating_add(u64::from(hz.max(1)).div_ceil(20));
                            }
                            Kind::Outputs => self.need_outputs = false,
                            Kind::Select => {
                                self.selection = None;
                                self.ready_selection = None;
                            }
                        }
                    }
                    Ok(false) => {}
                    Err(_) => self.failed(request, now, hz),
                }
            }
        }
        self.page.loading = self.ui.popup_open
            && (self.need_outputs
                || self.selection.is_some()
                || self.active.is_some_and(|r| {
                    r.view == self.view_epoch && matches!(r.kind, Kind::Outputs | Kind::Select)
                }));
        if old_loading != self.page.loading {
            self.ui.output_serial = self.ui.output_serial.wrapping_add(1);
        }
        !same(old, self.ui)
    }
    fn next_request(&self, now: u64) -> Option<Request> {
        if !self.ui.reachable {
            return self
                .need_master
                .then(|| Request::new(Kind::Master, 0, self.master_intent, Default::default()));
        }
        if let Some(selection) = self.selection {
            if let Some((intent, epoch, revision)) =
                self.ready_selection.filter(|v| v.0 == selection.intent)
            {
                let mut r = Request::new(Kind::Select, self.view_epoch, intent, self.provider);
                r.output.id = selection.id;
                r.output.service_epoch = epoch;
                r.output.revision = revision;
                return Some(r);
            }
            let mut r = Request::new(
                Kind::Outputs,
                self.view_epoch,
                selection.intent,
                self.provider,
            );
            r.output.index = self.wanted_page;
            return Some(r);
        }
        if self.pending_flags != 0 && now >= self.next_send {
            let mut r = Request::new(Kind::SetMaster, 0, self.master_intent, self.provider);
            r.master.flags = self.pending_flags;
            if self.pending_flags & a::audio_master_request_flag_set_muted != 0
                && self.pending_muted
            {
                r.master.flags |= a::audio_master_request_flag_muted;
            }
            r.master.fixed_volume = volume::gain_for_percent(self.pending_percent);
            return Some(r);
        }
        if self.need_master {
            return Some(Request::new(
                Kind::Master,
                0,
                self.master_intent,
                Default::default(),
            ));
        }
        if self.need_outputs && self.ui.popup_open {
            let mut r = Request::new(
                Kind::Outputs,
                self.view_epoch,
                self.output_intent,
                self.provider,
            );
            r.output.index = self.wanted_page;
            return Some(r);
        }
        None
    }
    fn failed(&mut self, request: Request, now: u64, hz: u32) {
        self.ui.reachable = false;
        self.ready_selection = None;
        self.need_master = false;
        self.need_outputs = false;
        self.next_poll = now.saturating_add(u64::from(hz.max(1)).div_ceil(2));
        // An admitted mutation is never replayed after an ambiguous timeout.
        // Newer, not-yet-admitted user intents survive only if the next read
        // proves the same exact provider generation.
        if matches!(request.kind, Kind::Outputs | Kind::Select)
            && self.ui.popup_open
            && request.view == self.view_epoch
        {
            self.page.request_error = true;
            self.selection_failed |= request.kind == Kind::Select;
            self.ui.output_serial = self.ui.output_serial.wrapping_add(1);
        }
    }
    fn completed(&mut self, done: Completed, now: u64, hz: u32) {
        let request = done.request;
        let Ok(reply) = done.reply else {
            self.failed(request, now, hz);
            return;
        };
        match reply {
            Reply::Master(state) => {
                if !self.provider.same(&done.provider) {
                    self.provider = done.provider;
                    self.pending_flags = 0;
                    self.selection = None;
                    self.ready_selection = None;
                    self.view_epoch = self.view_epoch.wrapping_add(1);
                    self.need_outputs = self.ui.popup_open;
                }
                self.ui.reachable = true;
                if request.intent == self.master_intent && self.pending_flags == 0 {
                    self.accept_master(state);
                    self.need_master = false;
                } else {
                    self.master = state;
                }
            }
            Reply::Outputs(state) => {
                if !self.provider.same(&done.provider) {
                    self.failed(request, now, hz);
                    return;
                }
                if request.kind == Kind::Select {
                    // Select replies describe page0; renew the requested page.
                    self.need_outputs = self.ui.popup_open;
                    return;
                }
                if self.selection.is_some_and(|s| s.intent == request.intent) {
                    self.ready_selection =
                        Some((request.intent, state.service_epoch, state.revision));
                }
                if self.ui.popup_open
                    && request.view == self.view_epoch
                    && state.index == self.wanted_page
                {
                    let changed = !state.same(&self.outputs) || !self.page.supported;
                    self.accept_outputs(state);
                    self.accepted_view = request.view;
                    self.need_outputs = false;
                    if changed {
                        self.ui.output_serial = self.ui.output_serial.wrapping_add(1);
                    }
                }
            }
        }
    }
    fn accept_master(&mut self, state: a::AudioServiceMasterState) {
        self.master = state;
        self.ui.installed = true;
        self.ui.reachable = true;
        self.ui.muted = state.flags & a::audio_master_state_flag_muted != 0;
        self.ui.percent = volume::percent_for_gain(state.selected_volume_fixed);
    }
    fn accept_outputs(&mut self, state: a::AudioServiceOutputState) {
        self.outputs = state;
        let failed = self.selection_failed;
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
    pub fn select(&mut self, id: [u8; 64]) {
        if !self.ui.reachable || !self.page.supported {
            return;
        }
        self.output_intent = self.output_intent.wrapping_add(1);
        self.view_epoch = self.view_epoch.wrapping_add(1);
        self.selection = Some(Selection {
            id,
            intent: self.output_intent,
        });
        self.ready_selection = None;
        self.page.failed = false;
        self.page.request_error = false;
        self.selection_failed = false;
        self.need_outputs = true;
        self.ui.output_serial = self.ui.output_serial.wrapping_add(1);
    }
    pub fn change_page(&mut self, next: bool) {
        if next {
            if self.wanted_page + volume::OUTPUT_ROWS as u32 >= self.page.total {
                return;
            }
            self.wanted_page += volume::OUTPUT_ROWS as u32;
        } else {
            self.wanted_page = self.wanted_page.saturating_sub(volume::OUTPUT_ROWS as u32);
        }
        self.view_epoch = self.view_epoch.wrapping_add(1);
        self.page.failed = false;
        self.need_outputs = true;
        self.ui.output_serial = self.ui.output_serial.wrapping_add(1);
    }
    pub fn set_percent(&mut self, percent: u8, immediate: bool, now: u64) -> bool {
        if !self.ui.installed || !self.ui.reachable {
            return false;
        }
        self.master_intent = self.master_intent.wrapping_add(1);
        self.ui.percent = percent.min(100);
        if self.ui.percent != 0 {
            self.ui.muted = false;
            self.pending_muted = false;
            self.pending_flags |= a::audio_master_request_flag_set_muted;
        }
        self.pending_flags |= a::audio_master_request_flag_set_volume;
        self.pending_percent = self.ui.percent;
        if immediate {
            self.next_send = now;
        }
        true
    }
    pub fn toggle_mute(&mut self, now: u64) -> bool {
        if !self.ui.installed || !self.ui.reachable {
            return false;
        }
        self.master_intent = self.master_intent.wrapping_add(1);
        self.ui.muted = !self.ui.muted;
        self.pending_flags |= a::audio_master_request_flag_set_muted;
        self.pending_muted = self.ui.muted;
        self.next_send = now;
        true
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
