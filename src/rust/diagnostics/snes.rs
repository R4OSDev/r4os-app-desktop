//! Explicit bounded receipts for one software benchmark window. A successful
//! CPU display copy is recorded; it is never promoted to physical visibility.
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
pub struct Trace {
    pub window_index: usize,
    samples: Vec<Sample>,
    overflow: bool,
    frame: Option<Rect>,
    client: Option<Rect>,
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
        })
    }
    pub fn record(&mut self, frame: Rect, client: Rect, sample: Sample) {
        if sample.generation == 0 {
            return;
        }
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
        if self.overflow || self.samples.is_empty() || context.ended_ns <= context.started_ns {
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
        write!(out,"{{\"contract\":\"desktop-window-output-v1\",\"receipt\":\"software-display-copy-present-success\",\"window_case\":\"{}\",\"process\":{{\"instance_id\":{},\"reserved\":0,\"generation\":{}}},\"screen\":{{\"w\":{},\"h\":{}}},\"frame\":",case,context.process.instance,context.process.generation,context.screen_w,context.screen_h).unwrap();
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
        out.push_str("],\"note\":\"Window generations observed in successful software copies; full_client_copy needs one complete covering region. Generation cohorts may complete after the guest timer. Frame/compose/present wall spans reuse existing owner clocks; no input-to-scanout or physical frame receipt.\"}");
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
