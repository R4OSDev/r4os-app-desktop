//! Read-only explanation of the actual output owner's composition admission.
//! This is neither another output catalog nor a GPU execution receipt.
use crate::render::gpu::{self, device::Error as DeviceError};
use r4os_sdk::abi as a;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    NotPrepared,
    NativeAdmitted,
    PriorNativeFailure,
    ColorProfile,
    NativeOpen,
    NativeState,
    NativeBinding,
    NativeRefresh,
    NativeOperations,
    PrimaryCpu,
    PrimaryNative,
    PrimaryPending,
}
impl Reason {
    pub fn name(self) -> &'static str {
        match self {
            Self::NotPrepared => "not-prepared",
            Self::NativeAdmitted => "native-operations-admitted",
            Self::PriorNativeFailure => "prior-native-failure",
            Self::ColorProfile => "color-profile",
            Self::NativeOpen => "native-open-failed",
            Self::NativeState => "native-state-unavailable",
            Self::NativeBinding => "native-device-bind-failed",
            Self::NativeRefresh => "native-device-refresh-failed",
            Self::NativeOperations => "native-operations-missing",
            Self::PrimaryCpu => "primary-cpu-admission",
            Self::PrimaryNative => "primary-native-admission",
            Self::PrimaryPending => "primary-admission-pending",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub reason: Reason,
    pub error: Option<&'static str>,
    pub status: Option<i32>,
    pub required_operations: Option<u32>,
    pub available_operations: Option<u32>,
}
impl Default for Decision {
    fn default() -> Self {
        Self::new(Reason::NotPrepared)
    }
}
impl Decision {
    pub const fn new(reason: Reason) -> Self {
        Self {
            reason,
            error: None,
            status: None,
            required_operations: None,
            available_operations: None,
        }
    }
    pub fn admitted(self) -> bool {
        self.reason == Reason::NativeAdmitted
    }
    pub fn device(reason: Reason, error: DeviceError) -> Self {
        let (name, code) = match error {
            DeviceError::Unavailable => ("unavailable", None),
            DeviceError::Memory => ("memory", None),
            DeviceError::WrongOwner => ("wrong-owner", None),
            DeviceError::Closed => ("closed", None),
            DeviceError::Invalid => ("invalid", None),
            DeviceError::Native(code) => ("provider-status", Some(code)),
        };
        Self {
            error: Some(name),
            status: code,
            ..Self::new(reason)
        }
    }
    pub fn open(error: super::native::Error) -> Self {
        let name = match error {
            super::native::Error::Memory => "memory",
            super::native::Error::Device(e) => return Self::device(Reason::NativeOpen, e),
            super::native::Error::Gpu(error) => match error {
                gpu::Error::Busy => "gpu-busy",
                gpu::Error::Unsupported => "gpu-unsupported",
                gpu::Error::Stale => "gpu-stale",
                gpu::Error::Graphics => "graphics",
                gpu::Error::Limit => "limit",
                gpu::Error::State => "state",
                gpu::Error::Deadline => "deadline",
                gpu::Error::Incomplete => "incomplete",
            },
        };
        Self {
            error: Some(name),
            ..Self::new(Reason::NativeOpen)
        }
    }
    pub fn operations(required: u32, available: u32) -> Self {
        Self {
            required_operations: Some(required),
            available_operations: Some(available),
            ..Self::new(if available & required == required {
                Reason::NativeAdmitted
            } else {
                Reason::NativeOperations
            })
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub adapter_id: u32,
    pub connector_id: u32,
    pub head_id: u32,
    pub device_generation: u64,
    pub connection_generation: u64,
    pub display_generation: u64,
}
impl From<a::GfxOutputTarget> for Target {
    fn from(v: a::GfxOutputTarget) -> Self {
        Self {
            adapter_id: v.adapter_id,
            connector_id: v.connector_id,
            head_id: v.head_id,
            device_generation: v.device_generation,
            connection_generation: v.connection_generation,
            display_generation: v.display_generation,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Execution {
    pub device_address: u64,
    pub device_handle_generation: u64,
    pub backend_device_generation: u64,
    pub reset_generation: u64,
    pub completed_frames: u64,
    pub visible_frames: u64,
    pub render_submit_jobs: Option<u64>,
    pub primitive_submit_jobs: Option<u64>,
    pub completed_frame: u64,
    pub result: Option<u32>,
    pub phase: Option<u32>,
    pub frame_generation: Option<u64>,
    pub frame_serial: Option<u64>,
    pub render_end_ns: Option<u64>,
    pub copied_ns: Option<u64>,
    pub visible_ns: Option<u64>,
}
/// Copied only for explicit owner diagnostics. None means the worker owns
/// the state; it must not be interpreted as zero work or a release receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureState {
    pub frame: u64,
    pub reused_captures: u64,
    pub atlas_reused_layers: u64,
    pub asset_misses: u64,
    pub captured_reads: u64,
    pub completed_reads: u64,
    pub failed_reads: u64,
    pub reads_empty: bool,
    pub external_receipts: usize,
    pub prepared_while_busy: u64,
    pub coalesced_scenes: u64,
    pub prepared_with_gpu_jobs: u64,
    pub prepared_during_worker: u64,
    pub pending_scene: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureObservation {
    pub choice: Choice,
    pub state: Option<CaptureState>,
}
/// Copied once at managed native failure, before teardown can erase its cause.
/// A missing old state means worker ownership, never a zero-work receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeFailure {
    pub target: Target,
    pub stage: &'static str,
    pub gpu_error: Option<gpu::Error>,
    pub capture_error: Option<crate::render::primitive_assets::Error>,
    pub frame: Option<u64>,
    pub latest_frame: Option<u64>,
    pub worker_pending: bool,
    pub awaiting_visible: bool,
    pub latest_captured: bool,
    pub old_reads_empty: Option<bool>,
    pub latest_reads_empty: bool,
    pub gpu_physically_retired: Option<bool>,
}
impl Execution {
    fn same_owner(self, other: Self) -> bool {
        (
            self.device_address,
            self.device_handle_generation,
            self.backend_device_generation,
            self.reset_generation,
        ) == (
            other.device_address,
            other.device_handle_generation,
            other.backend_device_generation,
            other.reset_generation,
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
    pub target: Option<Target>,
    pub owner: &'static str,
    pub active: bool,
    pub decision: Decision,
    pub execution: Option<Execution>,
}
impl Choice {
    pub fn same_selection(self, other: Self) -> bool {
        self.target == other.target
            && self.owner == other.owner
            && self.active == other.active
            && self.decision == other.decision
            && match (self.execution, other.execution) {
                (Some(a), Some(b)) => a.same_owner(b),
                (None, None) => true,
                _ => false,
            }
    }
    pub fn primary(admission: super::primary::Admission) -> Self {
        let (owner, active, reason) = match admission {
            super::primary::Admission::Cpu => ("primary-cpu", true, Reason::PrimaryCpu),
            super::primary::Admission::Native => ("primary-native-gr", true, Reason::PrimaryNative),
            super::primary::Admission::Pending => {
                ("primary-pending", false, Reason::PrimaryPending)
            }
        };
        Self {
            target: None,
            owner,
            active,
            decision: Decision::new(reason),
            execution: None,
        }
    }
}
