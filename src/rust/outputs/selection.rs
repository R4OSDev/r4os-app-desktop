//! Read-only explanation of the actual output owner's composition admission.
//! This is neither another output catalog nor a GPU execution receipt.
use crate::render::gpu::{self, device::Error as DeviceError};
use r4os_sdk::abi as a;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    NotPrepared, NativeAdmitted, PriorNativeFailure, ColorProfile,
    NativeOpen, NativeState, NativeBinding, NativeRefresh, NativeOperations,
    PrimaryCpu, PrimaryNative, PrimaryPending,
}
impl Reason {
    pub fn name(self) -> &'static str {
        match self {
            Self::NotPrepared => "not-prepared", Self::NativeAdmitted => "native-operations-admitted",
            Self::PriorNativeFailure => "prior-native-failure", Self::ColorProfile => "color-profile",
            Self::NativeOpen => "native-open-failed", Self::NativeState => "native-state-unavailable",
            Self::NativeBinding => "native-device-bind-failed", Self::NativeRefresh => "native-device-refresh-failed",
            Self::NativeOperations => "native-operations-missing",
            Self::PrimaryCpu => "primary-cpu-admission", Self::PrimaryNative => "primary-native-admission",
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
impl Default for Decision { fn default() -> Self { Self::new(Reason::NotPrepared) } }
impl Decision {
    pub const fn new(reason: Reason) -> Self {
        Self { reason, error: None, status: None, required_operations: None, available_operations: None }
    }
    pub fn admitted(self) -> bool { self.reason == Reason::NativeAdmitted }
    pub fn device(reason: Reason, error: DeviceError) -> Self {
        let (name, code) = match error {
            DeviceError::Unavailable => ("unavailable", None), DeviceError::Memory => ("memory", None),
            DeviceError::WrongOwner => ("wrong-owner", None), DeviceError::Closed => ("closed", None),
            DeviceError::Invalid => ("invalid", None), DeviceError::Native(code) => ("provider-status", Some(code)),
        };
        Self { error: Some(name), status: code, ..Self::new(reason) }
    }
    pub fn open(error: super::native::Error) -> Self {
        let name = match error {
            super::native::Error::Memory => "memory",
            super::native::Error::Device(e) => return Self::device(Reason::NativeOpen, e),
            super::native::Error::Gpu(error) => match error {
                gpu::Error::Busy => "gpu-busy", gpu::Error::Unsupported => "gpu-unsupported",
                gpu::Error::Stale => "gpu-stale", gpu::Error::Graphics => "graphics",
                gpu::Error::Limit => "limit", gpu::Error::State => "state",
                gpu::Error::Deadline => "deadline", gpu::Error::Incomplete => "incomplete",
            },
        };
        Self { error: Some(name), ..Self::new(Reason::NativeOpen) }
    }
    pub fn operations(required: u32, available: u32) -> Self {
        Self { required_operations: Some(required), available_operations: Some(available),
            ..Self::new(if available & required == required { Reason::NativeAdmitted } else { Reason::NativeOperations }) }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub adapter_id: u32, pub connector_id: u32, pub head_id: u32,
    pub device_generation: u64, pub connection_generation: u64, pub display_generation: u64,
}
impl From<a::GfxOutputTarget> for Target {
    fn from(v: a::GfxOutputTarget) -> Self {
        Self { adapter_id: v.adapter_id, connector_id: v.connector_id, head_id: v.head_id,
            device_generation: v.device_generation, connection_generation: v.connection_generation,
            display_generation: v.display_generation }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
    pub target: Option<Target>, pub owner: &'static str, pub active: bool, pub decision: Decision,
}
impl Choice {
    pub fn primary(admission: super::primary::Admission) -> Self {
        let (owner, active, reason) = match admission {
            super::primary::Admission::Cpu => ("primary-cpu", true, Reason::PrimaryCpu),
            super::primary::Admission::Native => ("primary-native-gr", true, Reason::PrimaryNative),
            super::primary::Admission::Pending => ("primary-pending", false, Reason::PrimaryPending),
        };
        Self { target: None, owner, active, decision: Decision::new(reason) }
    }
}
