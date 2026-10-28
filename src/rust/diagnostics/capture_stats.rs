//! An explicit copied owner snapshot. No frame, poller, clock sampler or
//! rendering policy is introduced by observing the capture ledger.
use super::{
    snes,
    wire::{self, Identity},
};
use crate::{outputs::selection::CaptureObservation, runtime::environment::Environment};
use alloc::string::String;
use core::fmt::Write;
use r4os_sdk::path::FilePath;

pub fn run(
    env: &Environment<'_>,
    caller: Identity,
    sequence: u64,
    observations: &[Option<CaptureObservation>; 4],
) -> Result<(), ()> {
    let path = wire::capture_path(caller, sequence).ok_or(())?;
    // This caller/generation owns only its three report names. Preserve any
    // previous report after an ambiguous RPC instead of overwriting evidence.
    if snes::exists(env, path.as_bytes()) != Ok(false) {
        return Err(());
    }
    let mut report = String::new();
    report.try_reserve(8192).map_err(|_| ())?;
    write!(report, "{{\"contract\":\"desktop-capture-owner-v1\",\"caller_instance\":{},\"caller_generation\":{},\"sequence\":{},\"physical_visibility\":null,\"scope\":\"Native owner lifetime counters, including background work; no optical or per-window timing receipt. Null state is worker-owned, never zero.\",\"outputs\":[", caller.instance, caller.generation, sequence).unwrap();
    for (i, observation) in observations.iter().flatten().enumerate() {
        if i != 0 {
            report.push(',');
        }
        let c = observation.choice;
        write!(
            report,
            "{{\"owner\":\"{}\",\"active\":{},\"reason\":\"{}\",\"target\":",
            c.owner,
            c.active,
            c.decision.reason.name()
        )
        .unwrap();
        if let Some(t) = c.target {
            write!(report, "{{\"adapter_id\":{},\"connector_id\":{},\"head_id\":{},\"device_generation\":{},\"connection_generation\":{},\"display_generation\":{}}}", t.adapter_id,t.connector_id,t.head_id,t.device_generation,t.connection_generation,t.display_generation).unwrap();
        } else {
            report.push_str("null");
        }
        report.push_str(",\"execution\":");
        if let Some(e) = c.execution {
            write!(report, "{{\"device_address\":{},\"device_handle_generation\":{},\"backend_device_generation\":{},\"reset_generation\":{},\"completed_frames\":{},\"visible_frames\":{}}}", e.device_address,e.device_handle_generation,e.backend_device_generation,e.reset_generation,e.completed_frames,e.visible_frames).unwrap();
        } else {
            report.push_str("null");
        }
        report.push_str(",\"capture\":");
        if let Some(s) = observation.state {
            write!(report, "{{\"frame\":{},\"reused_captures\":{},\"captured_reads\":{},\"completed_reads\":{},\"failed_reads\":{},\"reads_empty\":{},\"external_receipts\":{}}}",s.frame,s.reused_captures,s.captured_reads,s.completed_reads,s.failed_reads,s.reads_empty,s.external_receipts).unwrap();
        } else {
            report.push_str("null");
        }
        report.push('}');
    }
    report.push_str("]}");
    let path = FilePath::parse(path.as_bytes()).map_err(|_| ())?;
    if env
        .system
        .files()
        .write(&path, report.as_bytes())
        .map_err(|_| ())?
        != report.len()
    {
        return Err(());
    }
    Ok(())
}
