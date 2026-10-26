//! Explicit bounded observations for one benchmark window. CPU copy receipts
//! and managed output context stay separate; neither proves physical visibility.
use super::wire::{Identity, SNES_PATHS, SnesOptions};
use crate::outputs::selection::Choice;
use crate::{hosted::process::Programs, render::geometry::Rect, runtime::environment::Environment};
use alloc::{string::String, vec::Vec};
use core::fmt::Write;
use r4os_sdk::{abi as a, path::FilePath, storage::Error};
struct Selection {
    choice: Choice,
    first_generation: u64,
    last_generation: u64,
}
#[derive(Clone, Copy)]
pub struct Sample {
    pub generation: u64,
    pub completed_ns: u64,
    pub frame_wall_ns: u64,
    pub compose_wall_ns: u64,
    pub present_wall_ns: u64,
    pub damage_bounds: Rect,
    pub full_client_copy: bool,
    pub cursor_queued_to_copy_ticks: Option<u64>,
}
#[repr(i32)]
pub enum FinishFailure {
    MissingTrace = -1001,
    ChildNotComplete = -1002,
    ClientIdentity = -1003,
    ClientCompletion = -1004,
    TraceInvalid = -1005,
    ReportPath = -1006,
    ReportWrite = -1007,
}
pub struct Trace {
    pub window_index: usize,
    samples: Vec<Sample>,
    overflow: bool,
    frame: Option<Rect>,
    client: Option<Rect>,
    managed_output: Option<bool>,
    observed_generation: u64,
    observed_generations: u64,
    selections: Vec<Selection>,
}
impl Trace {
    pub fn new(window_index: usize) -> Result<Self, ()> {
        let mut samples = Vec::new();
        samples.try_reserve_exact(4096).map_err(|_| ())?;
        let mut selections = Vec::new();
        selections.try_reserve_exact(64).map_err(|_| ())?;
        Ok(Self {
            window_index,
            samples,
            overflow: false,
            frame: None,
            client: None,
            managed_output: None,
            observed_generation: 0,
            observed_generations: 0,
            selections,
        })
    }
    /// Admission plus optional actual provider receipts for the exact output
    /// owner. Those lifetime observations never bind a window generation.
    pub fn observe_choice(&mut self, choice: Choice, generation: u64) {
        if generation == 0 {
            return;
        }
        if let Some(previous) = self
            .selections
            .iter_mut()
            .rev()
            .find(|s| s.choice.target == choice.target)
        {
            if previous.choice.same_selection(choice) {
                if generation < previous.last_generation {
                    self.overflow = true;
                }
                previous.last_generation = generation;
                previous.choice.execution = choice.execution;
                return;
            }
        }
        if self.selections.len() == 64 {
            self.overflow = true;
            return;
        }
        self.selections.push(Selection {
            choice,
            first_generation: generation,
            last_generation: generation,
        });
    }
    /// A published window generation supplies geometry/context only. No GPU
    /// submission, completion, scanout timestamp or copy duration is inferred.
    pub fn observe_context(&mut self, managed: bool, frame: Rect, client: Rect, generation: u64) {
        if generation == 0 {
            return;
        }
        if self.managed_output.is_some_and(|source| managed != source) {
            self.overflow = true;
            return;
        }
        if !managed {
            return;
        }
        self.managed_output = Some(true);
        if self
            .frame
            .is_some_and(|old| old != frame || self.client != Some(client))
            || generation < self.observed_generation
        {
            self.overflow = true;
            return;
        }
        self.frame = Some(frame);
        self.client = Some(client);
        if generation != self.observed_generation {
            self.observed_generation = generation;
            self.observed_generations = self.observed_generations.saturating_add(1);
        }
    }
    pub fn record(&mut self, frame: Rect, client: Rect, sample: Sample) {
        if sample.generation == 0 {
            return;
        }
        if self.managed_output == Some(true) {
            self.overflow = true;
            return;
        }
        self.managed_output = Some(false);
        if self
            .frame
            .is_some_and(|old| old != frame || self.client != Some(client))
        {
            self.overflow = true;
            return;
        }
        self.frame = Some(frame);
        self.client = Some(client);
        if self
            .samples
            .last()
            .is_some_and(|old| old.generation == sample.generation)
        {
            return;
        }
        if self.samples.len() == 4096
            || self
                .samples
                .last()
                .is_some_and(|old| sample.generation < old.generation)
        {
            self.overflow = true;
            return;
        }
        self.samples.push(sample)
    }
    pub fn report(&self, context: Report) -> Result<String, ()> {
        if self.overflow
            || self.frame.is_none()
            || self.client.is_none()
            || (if self.managed_output == Some(true) {
                self.observed_generations == 0
            } else {
                self.samples.is_empty()
            })
            || context.ended_ns <= context.started_ns
        {
            return Err(());
        }
        let mut out = String::new();
        out.try_reserve(self.samples.len() * 360 + 1800)
            .map_err(|_| ())?;
        let case = if context.maximized {
            "maximized"
        } else {
            "small"
        };
        let receipt = if self.managed_output == Some(true) {
            "managed-output-context-only"
        } else {
            "software-display-copy-present-success"
        };
        write!(out,"{{\"contract\":\"desktop-window-output-v1\",\"receipt\":\"{}\",\"window_case\":\"{}\",\"process\":{{\"instance_id\":{},\"reserved\":0,\"generation\":{}}},\"screen\":{{\"w\":{},\"h\":{}}},\"frame\":",receipt,case,context.process.instance,context.process.generation,context.screen_w,context.screen_h).unwrap();
        rectangle(&mut out, self.frame.unwrap());
        out.push_str(",\"client\":");
        rectangle(&mut out, self.client.unwrap());
        write!(out,",\"session_start_ns\":{},\"session_end_ns\":{},\"global_desktop_presents\":{},\"cursor_moves\":{},\"cursor_tick_hz\":{},\"physical_visibility\":null,\"composition_selection_scope\":\"Selected output owner and original admission decision at published window generations; not GPU execution or physical visibility. Primary target identity unavailable.\",\"composition_choices\":[",context.started_ns,context.ended_ns,context.presents,context.cursor_moves,context.tick_hz).unwrap();
        for (i, s) in self.selections.iter().enumerate() {
            if i != 0 {
                out.push(',');
            }
            let c = s.choice;
            write!(out,"{{\"first_published_generation\":{},\"last_published_generation\":{},\"owner\":\"{}\",\"active\":{},\"reason\":\"{}\",\"target\":",s.first_generation,s.last_generation,c.owner,c.active,c.decision.reason.name()).unwrap();
            if let Some(t) = c.target {
                write!(out,"{{\"adapter_id\":{},\"connector_id\":{},\"head_id\":{},\"device_generation\":{},\"connection_generation\":{},\"display_generation\":{}}}",t.adapter_id,t.connector_id,t.head_id,t.device_generation,t.connection_generation,t.display_generation).unwrap();
            } else {
                out.push_str("null");
            }
            out.push_str(",\"error\":");
            if let Some(error) = c.decision.error {
                write!(out, "\"{}\"", error).unwrap();
            } else {
                out.push_str("null");
            }
            out.push_str(",\"provider_status\":");
            optional(&mut out, c.decision.status);
            out.push_str(",\"required_operations\":");
            optional(&mut out, c.decision.required_operations);
            out.push_str(",\"available_operations\":");
            optional(&mut out, c.decision.available_operations);
            out.push_str(",\"owner_execution\":");
            if let Some(e) = c.execution {
                write!(out,"{{\"scope\":\"Exact native owner lifetime counters; admitted render jobs and actual provider frame receipts, including background/warmup work. Not per-window generations or optical visibility.\",\"device_address\":{},\"device_handle_generation\":{},\"backend_device_generation\":{},\"reset_generation\":{},\"completed_frames\":{},\"visible_frames\":{},\"completed_frame\":{},\"render_submit_jobs\":",e.device_address,e.device_handle_generation,e.backend_device_generation,e.reset_generation,e.completed_frames,e.visible_frames,e.completed_frame).unwrap();
                optional(&mut out, e.render_submit_jobs);
                out.push_str(",\"primitive_submit_jobs\":");
                optional(&mut out, e.primitive_submit_jobs);
                out.push_str(",\"result\":");
                optional(&mut out, e.result);
                out.push_str(",\"phase\":");
                optional(&mut out, e.phase);
                out.push_str(",\"frame_generation\":");
                optional(&mut out, e.frame_generation);
                out.push_str(",\"frame_serial\":");
                optional(&mut out, e.frame_serial);
                out.push_str(",\"render_end_ns\":");
                optional(&mut out, e.render_end_ns);
                out.push_str(",\"copied_ns\":");
                optional(&mut out, e.copied_ns);
                out.push_str(",\"visible_ns\":");
                optional(&mut out, e.visible_ns);
                out.push('}');
            } else {
                out.push_str("null");
            }
            out.push('}');
        }
        out.push_str("],\"records\":[");
        for (i, s) in self.samples.iter().enumerate() {
            if i != 0 {
                out.push(',')
            }
            write!(out,"{{\"generation\":{},\"completed_ns\":{},\"frame_wall_ns\":{},\"compose_wall_ns\":{},\"present_wall_ns\":{},\"damage_bounds\":",s.generation,s.completed_ns,s.frame_wall_ns,s.compose_wall_ns,s.present_wall_ns).unwrap();
            rectangle(&mut out, s.damage_bounds);
            write!(
                out,
                ",\"full_client_copy\":{},\"cursor_queued_to_copy_ticks\":",
                s.full_client_copy
            )
            .unwrap();
            if let Some(ticks) = s.cursor_queued_to_copy_ticks {
                write!(out, "{}", ticks).unwrap()
            } else {
                out.push_str("null")
            }
            out.push('}');
        }
        if self.managed_output == Some(true) {
            write!(out,"],\"published_generations_observed\":{},\"presentation_receipts\":null,\"note\":\"Managed output context and published window geometry only. Per-generation CPU copies, GPU completions, output timings and physical visibility are unavailable.\"}}",self.observed_generations).unwrap();
        } else {
            out.push_str("],\"note\":\"Window generations observed in successful software copies; full_client_copy needs one complete covering region. Generation cohorts may complete after the guest timer. Frame/compose/present wall spans reuse existing owner clocks; no input-to-scanout or physical frame receipt.\"}");
        }
        Ok(out)
    }
}
fn optional<T: core::fmt::Display>(out: &mut String, value: Option<T>) {
    if let Some(v) = value {
        write!(out, "{}", v).unwrap();
    } else {
        out.push_str("null");
    }
}
fn rectangle(out: &mut String, r: Rect) {
    write!(
        out,
        "{{\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}",
        r.x, r.y, r.w, r.h
    )
    .unwrap();
}
pub struct Report {
    pub maximized: bool,
    pub process: Identity,
    pub screen_w: u32,
    pub screen_h: u32,
    pub started_ns: u64,
    pub ended_ns: u64,
    pub presents: u64,
    pub cursor_moves: u64,
    pub tick_hz: u64,
}
#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> Report {
        Report {
            maximized: false,
            process: Identity {
                instance: 17,
                generation: 17,
            },
            screen_w: 1920,
            screen_h: 1080,
            started_ns: 1,
            ended_ns: 10,
            presents: 0,
            cursor_moves: 0,
            tick_hz: 1000,
        }
    }
    fn geometry() -> (Rect, Rect) {
        (Rect::new(62, 40, 430, 262), Rect::new(70, 70, 414, 224))
    }
    fn copy() -> Sample {
        Sample {
            generation: 3,
            completed_ns: 5,
            frame_wall_ns: 3,
            compose_wall_ns: 1,
            present_wall_ns: 2,
            damage_bounds: geometry().1,
            full_client_copy: true,
            cursor_queued_to_copy_ticks: None,
        }
    }
    #[test]
    fn software_copy_receipt_retains_original_time_and_geometry() {
        let (frame, client) = geometry();
        let mut trace = Trace::new(1).unwrap();
        trace.observe_context(false, frame, client, 3);
        assert!(trace.report(context()).is_err());
        trace.record(frame, client, copy());
        let report = trace.report(context()).unwrap();
        assert!(report.contains("\"receipt\":\"software-display-copy-present-success\""));
        assert!(report.contains("\"completed_ns\":5,\"frame_wall_ns\":3"));
        assert!(report.contains("\"physical_visibility\":null"));
        assert!(!report.contains("published_generations_observed"));
    }
    #[test]
    fn managed_context_never_invents_copy_or_gpu_completion() {
        let (frame, client) = geometry();
        let mut trace = Trace::new(1).unwrap();
        trace.observe_context(true, frame, client, 3);
        trace.observe_context(true, frame, client, 3);
        trace.observe_context(true, frame, client, 5);
        let report = trace.report(context()).unwrap();
        assert!(report.contains("\"receipt\":\"managed-output-context-only\""));
        assert!(report.contains(
            "\"records\":[],\"published_generations_observed\":2,\"presentation_receipts\":null"
        ));
        assert!(!report.contains("completed_ns"));
        assert!(!report.contains("software-display-copy-present-success"));
        assert!(report.contains("\"physical_visibility\":null"));
    }
    #[test]
    fn changed_output_geometry_and_generation_reject_the_context() {
        let (frame, client) = geometry();
        let mut empty = Trace::new(1).unwrap();
        empty.observe_context(true, frame, client, 0);
        assert!(empty.report(context()).is_err());
        for case in 0..5 {
            let mut trace = Trace::new(1).unwrap();
            trace.observe_context(true, frame, client, 3);
            match case {
                0 => trace.observe_context(false, frame, client, 4),
                1 => trace.observe_context(true, Rect::new(62, 40, 431, 262), client, 4),
                2 => trace.observe_context(true, frame, Rect::new(70, 70, 415, 224), 4),
                3 => trace.observe_context(true, frame, client, 2),
                _ => trace.record(frame, client, copy()),
            }
            assert!(trace.report(context()).is_err());
        }
        let mut trace = Trace::new(1).unwrap();
        trace.record(frame, client, copy());
        trace.observe_context(true, frame, client, 4);
        assert!(trace.report(context()).is_err());
    }
    #[test]
    fn source_binds_to_first_actual_observation_after_output_startup() {
        let (frame, client) = geometry();
        let mut trace = Trace::new(1).unwrap();
        trace.observe_context(false, frame, client, 3);
        trace.observe_context(true, frame, client, 0);
        assert!(trace.report(context()).is_err());
        trace.observe_context(true, frame, client, 3);
        assert!(
            trace
                .report(context())
                .unwrap()
                .contains("managed-output-context-only")
        );
        trace.record(frame, client, copy());
        assert!(trace.report(context()).is_err());
    }
    #[test]
    fn composition_choices_keep_exact_generations_and_original_admission_failure() {
        use crate::outputs::selection::{Decision, Target};
        let (frame, client) = geometry();
        let mut trace = Trace::new(1).unwrap();
        trace.observe_context(true, frame, client, 3);
        let mut choice = Choice {
            target: Some(Target {
                adapter_id: 8,
                connector_id: 2048,
                head_id: 0,
                device_generation: 2,
                connection_generation: 9,
                display_generation: 4,
            }),
            owner: "managed-cpu",
            active: true,
            decision: Decision::operations(7, 3),
            execution: None,
        };
        trace.observe_choice(choice, 3);
        trace.observe_choice(choice, 4);
        assert_eq!(trace.selections.len(), 1);
        assert_eq!(trace.selections[0].last_generation, 4);
        choice.target.as_mut().unwrap().display_generation += 1;
        trace.observe_choice(choice, 5);
        assert_eq!(trace.selections.len(), 2);
        let report = trace.report(context()).unwrap();
        assert!(report.contains("\"reason\":\"native-operations-missing\""));
        assert!(report.contains("\"required_operations\":7,\"available_operations\":3"));
        assert!(
            report.contains("\"display_generation\":4")
                && report.contains("\"display_generation\":5")
        );
        assert!(
            report.contains("\"records\":[]") && report.contains("\"presentation_receipts\":null")
        );
    }
    #[test]
    fn composition_history_retains_transitions_and_rejects_regressing_observations() {
        use crate::outputs::selection::{Decision, Execution, Reason};
        let (frame, client) = geometry();
        let mut trace = Trace::new(1).unwrap();
        trace.observe_context(true, frame, client, 3);
        let mut choice = Choice {
            target: None,
            owner: "primary-native-gr",
            active: true,
            decision: Decision::operations(7, 7),
            execution: None,
        };
        choice.execution = Some(Execution {
            device_address: 4096,
            device_handle_generation: 2,
            backend_device_generation: 3,
            reset_generation: 1,
            completed_frames: 2,
            visible_frames: 2,
            render_submit_jobs: Some(4),
            primitive_submit_jobs: Some(2),
            completed_frame: 2,
            result: Some(1),
            phase: Some(4),
            frame_generation: Some(2),
            frame_serial: Some(2),
            render_end_ns: Some(5),
            copied_ns: Some(6),
            visible_ns: Some(7),
        });
        trace.observe_choice(choice, 3);
        choice.execution.as_mut().unwrap().visible_frames = 3;
        trace.observe_choice(choice, 4);
        assert_eq!(
            trace.selections.len(),
            1,
            "changing owner counters is not another selection"
        );
        assert!(
            trace
                .report(context())
                .unwrap()
                .contains("\"visible_frames\":3")
        );
        choice.execution.as_mut().unwrap().device_handle_generation += 1;
        trace.observe_choice(choice, 5);
        assert_eq!(
            trace.selections.len(),
            2,
            "a reused device address has a distinct owner generation"
        );
        choice.owner = "primary-cpu";
        choice.execution = None;
        choice.decision = Decision::new(Reason::PriorNativeFailure);
        trace.observe_choice(choice, 6);
        choice.decision = Decision::device(
            Reason::NativeBinding,
            crate::render::gpu::device::Error::Native(-17),
        );
        trace.observe_choice(choice, 7);
        assert_eq!(trace.selections.len(), 4);
        assert!(
            trace
                .report(context())
                .unwrap()
                .contains("\"provider_status\":-17")
        );
        trace.observe_choice(choice, 6);
        assert!(trace.report(context()).is_err());
    }
}
#[derive(Default)]
pub struct Ledger {
    pub software_reserved: bool,
    pub snes_reserved: bool,
    pub child: Identity,
    pub options: SnesOptions,
    pub trace: Option<Trace>,
    pub started_ns: u64,
    pub presents_before: u64,
    pub cursor_before: u64,
    pub report_ready: bool,
}
pub fn exists(env: &Environment<'_>, name: &[u8]) -> Result<bool, Error> {
    let name = FilePath::parse(name).map_err(|_| Error::Length)?;
    Ok(env.system.files().info(&name)?.is_some())
}
pub fn remove(env: &Environment<'_>, name: &[u8]) -> bool {
    match exists(env, name) {
        Ok(false) => true,
        Ok(true) => {
            FilePath::parse(name).is_ok_and(|name| env.system.files().delete(&name).is_ok())
        }
        Err(_) => false,
    }
}
pub fn live(programs: &Programs<'_>, identity: Identity) -> bool {
    identity.valid()
        && matches!(programs.status(identity.handle()),Ok(Some(info)) if info.state!=a::ProgramInstanceState::done.0 && info.id==identity.instance)
}
pub fn gone(programs: &Programs<'_>, identity: Identity) -> bool {
    match programs.status(identity.handle()) {
        Ok(None) => true,
        Ok(Some(info)) => info.state == a::ProgramInstanceState::done.0,
        Err(Error::Raw(code)) => crate::hosted::process::gone(code),
        _ => false,
    }
}
impl Ledger {
    pub fn reserve_snes(&mut self, env: &Environment<'_>) -> bool {
        if self.snes_reserved
            || SNES_PATHS
                .into_iter()
                .any(|name| exists(env, name) != Ok(false))
        {
            return false;
        }
        self.snes_reserved = true;
        true
    }
    pub fn cleanup(&mut self, env: &Environment<'_>, caller: Identity) -> bool {
        let mut ok = true;
        if self.software_reserved {
            if super::wire::software_path(caller).is_some_and(|p| remove(env, p.as_bytes())) {
                self.software_reserved = false
            } else {
                ok = false
            }
        }
        if self.snes_reserved {
            let mut retired = true;
            for name in SNES_PATHS {
                retired &= remove(env, name)
            }
            if retired {
                self.snes_reserved = false
            } else {
                ok = false
            }
        }
        ok
    }
}
