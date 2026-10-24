pub mod cpu;
pub mod cursor;
pub mod intents;
pub mod manager;
pub mod modes;
pub mod native;
pub mod power;
pub mod primary;
pub mod preferences;
pub mod profile;
pub mod topology_wire;
use r4os_contract as abi;
pub type NativeResult<T> = core::result::Result<T, i32>;
pub trait OutputCatalog {
    fn revision(&mut self) -> NativeResult<abi::GfxDisplayRevision>;
    fn info(&mut self, index: u32) -> NativeResult<abi::GfxOutputInfo>;
    fn power(&mut self, id: &abi::GfxOutputId) -> NativeResult<abi::GfxOutputPower>;
}
pub(super) fn same_id(a: r4os_contract::GfxOutputId, b: r4os_contract::GfxOutputId) -> bool {
    a.adapter_id == b.adapter_id
        && a.connector_id == b.connector_id
        && a.device_generation == b.device_generation
        && a.connection_generation == b.connection_generation
}
pub mod api;
pub mod bindings;
pub mod catalog;
pub mod color_policy;
pub mod control;
