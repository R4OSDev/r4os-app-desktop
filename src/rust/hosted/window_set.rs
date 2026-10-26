//! Main-task connection between WINSVC publication, hosted lifecycle and CPU
//! capture. Four independent surface owners never borrow the window model.
use super::{
    client::Clients,
    model::GraphicsRetirement,
    process::{same, valid},
    window_graphics::{Spec, Transport, Window},
    window_image::{Identity, Memory, NoReceipt, Receipt},
};
use crate::{
    render::{color, geometry::Dirty, software::ExternalFrame},
    runtime::{service::Host, wire::Wire},
    windows::{
        controller::Controller,
        service_mirror::ResetAcknowledged,
        window::{Kind, Window as Geometry},
    },
};
use r4os_contract as a;
use r4os_sdk::{c_facades as c, graphics::Graphics};
#[derive(Clone, Copy)]
pub enum KindOfConsumer {
    Cpu,
    Gpu,
}
#[derive(Clone, Copy)]
pub struct Binding {
    pub backend: a::GfxBackendInfo,
    pub target: a::GfxOutputTarget,
    pub ready: bool,
    pub kind: KindOfConsumer,
    pub headless_revision: Option<u64>,
    pub scale: u32,
}
/// Only call for the currently selected primary software output. Native and
/// managed outputs supply their own exact binding and admission deadline.
pub fn primary_software(
    graphics: &Graphics<'_, '_>,
    revision: u64,
    ready: bool,
) -> Option<Binding> {
    let backend = graphics.queues().backend_info(0).ok()?;
    if backend.binding.adapter_id != 0
        || backend.binding.milestone != a::gfx_queue_milestone_cpu_stores
    {
        return None;
    }
    let mut target = a::GfxOutputTarget::default();
    let slot = unsafe {
        c::R4SdkGuiDrawContext_displayOutputTarget(0, graphics.native_draw(), 0, 0, &mut target)
    };
    if slot.status < 0 || slot.flags & 2 != 0 {
        return None;
    }
    let result = slot.bits as i32;
    let headless_revision = if result == a::gfx_output_ok {
        None
    } else {
        if revision == 0
            || ![
                a::gfx_output_error_stale,
                a::gfx_output_error_unavailable,
                a::gfx_output_error_unsupported,
            ]
            .contains(&result)
        {
            return None;
        }
        target = a::GfxOutputTarget::default();
        Some(revision)
    };
    Some(Binding {
        backend,
        target,
        ready,
        kind: KindOfConsumer::Cpu,
        headless_revision,
        scale: 120,
    })
}
pub fn specification(
    window: &Geometry,
    owner: a::ProgramProcessHandle,
    index: usize,
    mirrored: bool,
    service_available: bool,
    terminal_mode: bool,
    binding: Binding,
) -> Option<Spec> {
    if index == 0
        || !mirrored
        || !service_available
        || window.kind() != Kind::App
        || window.close_requested().is_some()
        || !valid(owner)
        || owner.instance_id != window.instance_id()
    {
        return None;
    }
    let client = window.client_surface().rect;
    if client.is_empty() || client.w > 32768 || client.h > 32768 {
        return None;
    }
    let (width, height) = if matches!(binding.kind, KindOfConsumer::Gpu) {
        let scale = u64::from(binding.scale);
        let width = (client.w as u64 * scale).div_ceil(120);
        let height = (client.h as u64 * scale).div_ceil(120);
        if width == 0 || height == 0 || width > 32768 || height > 32768 {
            return None;
        }
        (width as u32, height as u32)
    } else {
        (client.w as u32, client.h as u32)
    };
    let mut config = a::WindowGraphicsConfig {
        width,
        height,
        flags: if window.visible() && !window.minimized() && !terminal_mode {
            a::window_graphics_visible
        } else {
            0
        },
        present_modes: a::window_graphics_fifo | a::window_graphics_mailbox,
        min_images: 2,
        max_images: 3,
        backend: binding.backend,
        output: a::GfxOutputId {
            adapter_id: binding.target.adapter_id,
            connector_id: binding.target.connector_id,
            device_generation: binding.target.device_generation,
            connection_generation: binding.target.connection_generation,
        },
        display_generation: binding
            .headless_revision
            .unwrap_or(binding.target.display_generation),
        ..a::WindowGraphicsConfig::default()
    };
    if binding.headless_revision.is_some() {
        config.flags |= a::window_graphics_headless;
    }
    match binding.kind {
        KindOfConsumer::Cpu => color::publish_cpu(&mut config),
        KindOfConsumer::Gpu => color::publish(&mut config),
    }
    Some(Spec {
        owner,
        config,
        consumer_ready: binding.ready,
    })
}
pub struct Set<M: Memory, R: Receipt = NoReceipt> {
    windows: [Window<M, R>; 4],
}
impl<M: Memory, R: Receipt> Set<M, R> {
    pub fn new(memory: M) -> Self
    where
        M: Clone,
    {
        Self {
            windows: core::array::from_fn(|_| Window::new(memory.clone())),
        }
    }
    pub fn specifications(
        windows: &Controller,
        clients: &Clients,
        mirrored: [bool; 4],
        available: bool,
        terminal_mode: bool,
        bindings: [Option<Binding>; 4],
    ) -> [Option<Spec>; 4] {
        core::array::from_fn(|i| {
            if clients.slots()[i].retiring() {
                None
            } else {
                specification(
                    &windows.windows()[i],
                    clients.slots()[i].handle(),
                    i,
                    mirrored[i],
                    available,
                    terminal_mode,
                    bindings[i]?,
                )
            }
        })
    }
    pub fn poll(
        &mut self,
        desktop: a::ProgramProcessHandle,
        wanted: [Option<Spec>; 4],
        transport: &mut impl Transport,
        windows: &Controller,
        dirty: &mut Dirty,
    ) -> bool {
        let mut changed = false;
        for (i, owner) in self.windows.iter_mut().enumerate() {
            let before = owner.front_message();
            owner.poll(desktop, i as u32, wanted[i], transport);
            let after = owner.front_message();
            let equal = match (before, after) {
                (None, None) => true,
                (Some(a), Some(b)) => a.same(&b),
                _ => false,
            };
            if !equal {
                dirty.invalidate(windows.windows()[i].frame_surface().rect);
                changed = true;
            }
        }
        changed
    }
    pub fn acknowledge_reset(&mut self, proof: &ResetAcknowledged, host: &Host<'_>) -> bool {
        if !host.claimed()
            || proof.endpoint() == 0
            || proof.endpoint() != host.endpoint()
            || !same(proof.owner(), host.owner())
        {
            return false;
        }
        for owner in &mut self.windows {
            unsafe {
                owner.acknowledge_reset();
            }
        }
        true
    }
    pub fn disconnect(&mut self) {
        for owner in &mut self.windows {
            owner.disconnect();
        }
    }
    pub fn refresh_markers(&self, host: &mut Host<'_>, activity: u64) {
        if !host.claimed() || !self.windows.iter().any(Window::observes_images) {
            return;
        }
        let missing = self
            .windows
            .iter()
            .any(|w| w.observes_images() && host.image_marker(w.surface()).is_none());
        let _ = host.refresh_images(activity, missing);
    }
    pub fn detached(&self) -> bool {
        self.windows.iter().all(Window::detached)
    }
    pub fn needs_polling(&self) -> bool {
        self.windows.iter().any(Window::needs_polling)
    }
    pub fn front(&mut self, index: usize) -> Option<&mut super::window_image::Frame<M, R>> {
        self.windows.get_mut(index)?.front()
    }
    pub fn finish_read(
        &mut self,
        index: usize,
        identity: Identity,
        receipt: Option<R>,
        failed: bool,
    ) -> core::result::Result<(), Option<R>> {
        let Some(window) = self.windows.get_mut(index) else {
            return Err(receipt);
        };
        window.finish_read(identity, receipt, failed)
    }
    /// The returned short exclusive loans prevent polling/reset while CPU
    /// capture consumes them. Capture ends every reader before returning.
    pub fn cpu_fronts(&mut self) -> [Option<&mut dyn ExternalFrame>; 4] {
        self.windows.each_mut().map(|window| {
            window.front().and_then(|frame| {
                if frame.cpu_image().is_some() {
                    Some(frame as &mut dyn ExternalFrame)
                } else {
                    None
                }
            })
        })
    }
}
impl<M: Memory, R: Receipt> GraphicsRetirement for Set<M, R> {
    fn retire(&mut self, index: usize, handle: a::ProgramProcessHandle) -> bool {
        let Some(owner) = self.windows.get_mut(index) else {
            return false;
        };
        if owner.detached() {
            return true;
        }
        if !same(owner.owner(), handle) {
            return false;
        }
        owner.disconnect();
        owner.detached()
    }
}
