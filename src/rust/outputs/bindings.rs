//! Select the existing CPU/GPU broker contract for a hosted window. Every
//! intersecting output must admit the same consumer; no adapter guessing.
use super::manager::Manager;
use crate::{
    hosted::window_set::{self, Binding, KindOfConsumer},
    render::{geometry::Rect, scene::intersect},
};
use r4os_sdk::{abi as a, graphics::Graphics};
pub fn select(
    manager: &Manager<'_>,
    graphics: &Graphics<'_, '_>,
    client: Rect,
    revision: u64,
    time: u64,
    primary_ready: bool,
) -> Option<Binding> {
    if !manager.active() {
        return window_set::primary_software(graphics, revision, primary_ready);
    }
    let required = r4gfx::device_gpu_render
        | r4gfx::device_gpu_render_list
        | r4gfx::device_gpu_grid
        | r4gfx::device_gpu_color
        | r4gfx::device_gpu_color_grid;
    let mut chosen: Option<usize> = None;
    let mut area = 0u64;
    let mut ready = true;
    let mut gpu = None;
    for (index, slot) in manager.slots.iter().enumerate() {
        if slot
            .resource
            .gpu()
            .is_some_and(|owner| owner.external_receipts() != 0)
            && (slot.logical_index.is_none() || slot.failed || slot.reconfiguring || slot.sleeping)
        {
            return None;
        }
        if slot.logical_index.is_none() || slot.disabled {
            continue;
        }
        let Some(overlap) = slot.bounds().and_then(|bounds| intersect(client, bounds)) else {
            continue;
        };
        if slot.failed || slot.reconfiguring || slot.sleeping || slot.paused {
            return None;
        }
        let is_gpu = slot.resource.gpu().is_some();
        if gpu.is_some_and(|old| old != is_gpu) {
            return None;
        }
        gpu = Some(is_gpu);
        if let Some(owner) = slot.resource.gpu() {
            let state = owner.consumer_state(time)?;
            if state.operations & required != required
                || state.device_generation != slot.target.device_generation
            {
                return None;
            }
            ready &= state.ready;
            if let Some(previous) = chosen {
                if manager.slots[previous].target.adapter_id != slot.target.adapter_id {
                    return None;
                }
            }
        } else {
            let owner = slot.resource.software()?;
            if !owner.ready() || owner.lost() {
                return None;
            }
        }
        let pixels = overlap.w as u64 * overlap.h as u64;
        if chosen.is_none() || pixels > area {
            chosen = Some(index);
            area = pixels;
        }
    }
    let slot = &manager.slots[chosen?];
    let is_gpu = gpu?;
    let backend = if is_gpu {
        let state = slot.resource.gpu()?.consumer_state(time)?;
        let mut found = None;
        for index in 0..32 {
            if let Ok(candidate) = graphics.queues().backend_info(index) {
                if candidate.binding.adapter_id == slot.target.adapter_id
                    && candidate.binding.device_generation == state.device_generation
                    && candidate.binding.reset_generation == state.reset_generation
                    && candidate.binding.milestone == a::gfx_queue_milestone_device_execution
                    && candidate.memory_generation != 0
                {
                    found = Some(candidate);
                    break;
                }
            }
        }
        found?
    } else {
        let candidate = graphics.queues().backend_info(0).ok()?;
        if candidate.binding.adapter_id != 0
            || candidate.binding.milestone != a::gfx_queue_milestone_cpu_stores
        {
            return None;
        }
        candidate
    };
    Some(Binding {
        backend,
        target: slot.target,
        ready,
        kind: if is_gpu {
            KindOfConsumer::Gpu
        } else {
            KindOfConsumer::Cpu
        },
        headless_revision: None,
        scale: slot.view.scale,
    })
}
