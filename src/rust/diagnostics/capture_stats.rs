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
    native_failures: &(u64, [Option<crate::outputs::selection::NativeFailure>; 4]),
    host: &crate::runtime::service::Host<'_>,
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
            write!(report, "{{\"frame\":{},\"reused_captures\":{},\"atlas_reused_layers\":{},\"asset_misses\":{},\"captured_reads\":{},\"completed_reads\":{},\"failed_reads\":{},\"reads_empty\":{},\"external_receipts\":{},\"prepared_while_busy\":{},\"coalesced_scenes\":{},\"prepared_with_gpu_jobs\":{},\"prepared_during_worker\":{},\"pending_scene\":{}}}",s.frame,s.reused_captures,s.atlas_reused_layers,s.asset_misses,s.captured_reads,s.completed_reads,s.failed_reads,s.reads_empty,s.external_receipts,s.prepared_while_busy,s.coalesced_scenes,s.prepared_with_gpu_jobs,s.prepared_during_worker,s.pending_scene).unwrap();
        } else {
            report.push_str("null");
        }
        report.push('}');
    }
    append_failure_details(&mut report, native_failures, host.failures());
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

fn append_failure_details(
    report: &mut String,
    native_failures: &(u64, [Option<crate::outputs::selection::NativeFailure>; 4]),
    service_failures: (u64, &[Option<crate::runtime::service::Failure>; 4]),
) {
    write!(report, "],\"native_failures\":{{\"total\":{},\"retained_capacity\":4,\"scope\":\"Copied managed native failures before teardown; no clock sampler or cause inferred\",\"records\":[", native_failures.0).unwrap();
    for (index, failure) in native_failures.1.iter().flatten().enumerate() {
        if index != 0 {
            report.push(',');
        }
        let t = failure.target;
        write!(report, "{{\"target\":{{\"adapter_id\":{},\"connector_id\":{},\"head_id\":{},\"device_generation\":{},\"connection_generation\":{},\"display_generation\":{}}},\"gpu_error\":",t.adapter_id,t.connector_id,t.head_id,t.device_generation,t.connection_generation,t.display_generation).unwrap();
        match failure.gpu_error {
            Some(e) => write!(report, "\"{e:?}\"").unwrap(),
            None => report.push_str("null"),
        }
        write!(report, ",\"stage\":\"{}\"", failure.stage).unwrap();
        report.push_str(",\"capture_error\":");
        match failure.capture_error {
            Some(e) => write!(report, "\"{e:?}\"").unwrap(),
            None => report.push_str("null"),
        }
        report.push_str(",\"frame\":");
        match failure.frame {
            Some(n) => write!(report, "{n}").unwrap(),
            None => report.push_str("null"),
        }
        report.push_str(",\"latest_frame\":");
        match failure.latest_frame {
            Some(n) => write!(report, "{n}").unwrap(),
            None => report.push_str("null"),
        }
        write!(report,",\"worker_pending\":{},\"awaiting_visible\":{},\"latest_captured\":{},\"old_reads_empty\":",failure.worker_pending,failure.awaiting_visible,failure.latest_captured).unwrap();
        match failure.old_reads_empty {
            Some(n) => write!(report, "{n}").unwrap(),
            None => report.push_str("null"),
        }
        write!(
            report,
            ",\"latest_reads_empty\":{},\"gpu_physically_retired\":",
            failure.latest_reads_empty
        )
        .unwrap();
        match failure.gpu_physically_retired {
            Some(n) => write!(report, "{n}").unwrap(),
            None => report.push_str("null"),
        }
        report.push('}');
    }
    let (count, failures) = service_failures;
    write!(report, "]}},\"service_failures\":{{\"total\":{},\"retained_capacity\":4,\"scope\":\"Failure-only values; no timestamp, poller or elapsed cause inferred\",\"records\":[", count).unwrap();
    for (index, failure) in failures.iter().flatten().enumerate() {
        if index != 0 {
            report.push(',');
        }
        write!(
            report,
            "{{\"sequence\":{},\"operation\":{},\"kind\":\"{}\",\"raw\":",
            failure.sequence, failure.operation, failure.kind
        )
        .unwrap();
        match failure.raw {
            Some(raw) => write!(report, "{}", raw).unwrap(),
            None => report.push_str("null"),
        }
        write!(
            report,
            ",\"site_line\":{},\"had_claim\":{},\"endpoint\":{},\"work_ticket\":{},\"invalidated\":{}}}",
            failure.site_line, failure.had_claim, failure.endpoint, failure.work_ticket, failure.invalidated
        )
        .unwrap();
    }
    report.push_str("]}}");
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" {
        fn r4desk_json_valid(bytes: *const u8, count: usize) -> i32;
    }
    #[test]
    fn copied_failure_report_is_independent_valid_json_with_unknown_worker_state() {
        use crate::{
            outputs::selection::{NativeFailure, Target},
            render::{gpu, primitive_assets},
        };
        let service = [None; 4];
        let failure = NativeFailure {
            target: Target::from(r4os_sdk::abi::GfxOutputTarget {
                adapter_id: 7,
                connector_id: 2048,
                device_generation: u64::MAX,
                ..Default::default()
            }),
            stage: "capture-newest",
            gpu_error: Some(gpu::Error::Graphics),
            capture_error: Some(primitive_assets::Error::State),
            frame: None,
            latest_frame: Some(u64::MAX),
            worker_pending: true,
            awaiting_visible: false,
            latest_captured: false,
            old_reads_empty: None,
            latest_reads_empty: false,
            gpu_physically_retired: None,
        };
        for records in [[None; 4], [Some(failure), None, None, None]] {
            let total = records.iter().flatten().count() as u64;
            let mut report = String::from("{\"outputs\":[");
            append_failure_details(&mut report, &(total, records), (0, &service));
            assert_eq!(
                unsafe { r4desk_json_valid(report.as_ptr(), report.len()) },
                1,
                "independent std.json parser rejected {report}"
            );
            assert!(
                report.contains("\"native_failures\":{")
                    && report.contains("\"service_failures\":{")
            );
            if total != 0 {
                assert!(
                    report.contains("\"frame\":null")
                        && report.contains("\"old_reads_empty\":null")
                );
                assert!(report.contains("\"latest_frame\":18446744073709551615"));
                assert!(report.contains("\"stage\":\"capture-newest\""));
            }
        }
    }
}
