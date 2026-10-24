//! Explicit bounded observations for one benchmark window. CPU copy receipts
//! and managed output context stay separate; neither proves physical visibility.
use super::wire::{Identity, SNES_PATHS, SnesOptions};
use crate::{hosted::process::Programs, render::geometry::Rect, runtime::environment::Environment};
use alloc::{string::String, vec::Vec};
use core::fmt::Write;
use r4os_sdk::{abi as a, path::FilePath, storage::Error};
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
}
impl Trace {
    pub fn new(window_index: usize) -> Result<Self, ()> {
        let mut samples = Vec::new();
        samples.try_reserve_exact(4096).map_err(|_| ())?;
        Ok(Self {
            window_index,
            samples,
            overflow: false,
            frame: None,
            client: None,
            managed_output: None,
            observed_generation: 0,
            observed_generations: 0,
        })
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
        if self.frame.is_some_and(|old| old != frame || self.client != Some(client))
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
            || (if self.managed_output == Some(true) { self.observed_generations == 0 } else { self.samples.is_empty() })
            || context.ended_ns <= context.started_ns {
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
        let receipt = if self.managed_output == Some(true) { "managed-output-context-only" } else { "software-display-copy-present-success" };
        write!(out,"{{\"contract\":\"desktop-window-output-v1\",\"receipt\":\"{}\",\"window_case\":\"{}\",\"process\":{{\"instance_id\":{},\"reserved\":0,\"generation\":{}}},\"screen\":{{\"w\":{},\"h\":{}}},\"frame\":",receipt,case,context.process.instance,context.process.generation,context.screen_w,context.screen_h).unwrap();
        rectangle(&mut out, self.frame.unwrap());
        out.push_str(",\"client\":");
        rectangle(&mut out, self.client.unwrap());
        write!(out,",\"session_start_ns\":{},\"session_end_ns\":{},\"global_desktop_presents\":{},\"cursor_moves\":{},\"cursor_tick_hz\":{},\"physical_visibility\":null,\"records\":[",context.started_ns,context.ended_ns,context.presents,context.cursor_moves,context.tick_hz).unwrap();
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
            maximized: false, process: Identity { instance: 17, generation: 17 },
            screen_w: 1920, screen_h: 1080, started_ns: 1, ended_ns: 10,
            presents: 0, cursor_moves: 0, tick_hz: 1000,
        }
    }
    fn geometry() -> (Rect, Rect) {
        (Rect::new(62, 40, 430, 262), Rect::new(70, 70, 414, 224))
    }
    fn copy() -> Sample {
        Sample {
            generation: 3, completed_ns: 5, frame_wall_ns: 3,
            compose_wall_ns: 1, present_wall_ns: 2, damage_bounds: geometry().1,
            full_client_copy: true, cursor_queued_to_copy_ticks: None,
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
        assert!(report.contains("\"records\":[],\"published_generations_observed\":2,\"presentation_receipts\":null"));
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
        assert!(trace.report(context()).unwrap().contains("managed-output-context-only"));
        trace.record(frame, client, copy());
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
