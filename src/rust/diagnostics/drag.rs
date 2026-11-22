//! Bounded, explicit observations of the primary CPU renderer. Reporting and
//! formatting happen after the sampling window. A copy receipt ends at the
//! graphics API, independently of when the monitor scans out the pixels.
use super::{snes, wire::Identity};
use crate::{
    render::{geometry::Rect, runtime::Timing},
    runtime::environment::Environment,
};
use alloc::{format, string::String, vec::Vec};
use core::fmt::Write;
use r4os_sdk::path::FilePath;

const LIMIT: usize = 2048;

#[derive(Clone, Copy, Default)]
struct Input {
    serial: u64,
    ns: u64,
    x: i32,
    y: i32,
    edge: u8, // 1 down, 2 motion, 3 up
}
struct Receipt {
    input: Input,
    rect: Rect,
    frame: u64,
    timing: Timing,
    accepted: bool,
    painted: bool,
}
pub struct Trace {
    pub caller: Identity,
    pub target: Identity,
    pub slot: usize,
    until: u64,
    input: Input,
    up: Input,
    rows: Vec<Receipt>,
    overflow: u64,
}
impl Trace {
    pub fn new(caller: Identity, target: Identity, slot: usize, until: u64) -> Option<Self> {
        let mut rows = Vec::new();
        rows.try_reserve_exact(LIMIT).ok()?;
        Some(Self {
            caller, target, slot, until, rows, overflow: 0,
            input: Input::default(), up: Input::default(),
        })
    }
    pub fn active(&self, tick: u64) -> bool {
        tick < self.until
    }
    pub fn input(&mut self, serial: u64, ns: u64, rect: Rect, down: bool, up: bool) {
        self.input = Input { serial, ns, x: rect.x, y: rect.y, edge: if up { 3 } else if down { 1 } else { 2 } };
        if up {
            self.up = self.input;
        }
    }
    pub fn frame(&mut self, rect: Rect, frame: u64, timing: Timing, accepted: bool, painted: bool) {
        if self.rows.len() == LIMIT {
            self.overflow = self.overflow.saturating_add(1);
            return;
        }
        self.rows.push(Receipt { input: self.input, rect, frame, timing, accepted, painted });
    }
    pub fn export(&self, env: &Environment<'_>) -> Result<(), ()> {
        let name = format!("C:\\TEMP\\DR{:08X}-{:016X}.CSV", self.caller.instance, self.caller.generation);
        if snes::exists(env, name.as_bytes()) != Ok(false) {
            return Err(());
        }
        let mut out = String::new();
        // Max decimal digits of the fixed set of integer fields plus header.
        out.try_reserve_exact(240 * self.rows.len() + 512).map_err(|_| ())?;
        let u = self.up;
        writeln!(out,"# drag-v1 target={},{} overflow={} up={},{},{},{}",self.target.instance,self.target.generation,self.overflow,u.serial,u.ns,u.x,u.y).unwrap();
        out.push_str("seq,input_ns,input_x,input_y,edge,x,y,frame,start_ns,raster_end_ns,composed_ns,completed_ns,accepted,painted\n");
        for row in &self.rows {
            let i = row.input;
            let t = row.timing;
            writeln!(out,"{},{},{},{},{},{},{},{},{},{},{},{},{},{}",i.serial,i.ns,i.x,i.y,i.edge,row.rect.x,row.rect.y,row.frame,t.started_ns,t.raster_end_ns.unwrap_or(0),t.started_ns.saturating_add(t.composition_ns),t.completed_ns,u8::from(row.accepted),u8::from(row.painted)).unwrap();
        }
        let path = FilePath::parse(name.as_bytes()).map_err(|_| ())?;
        let written = env.system.files().write(&path, out.as_bytes()).map_err(|_| ())?;
        (written == out.len()).then_some(()).ok_or(())
    }
}
