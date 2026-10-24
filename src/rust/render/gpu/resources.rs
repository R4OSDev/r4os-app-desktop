//! Native image/state resources of one composition transaction. The original
//! pitches, color views and aggregate charging survive every refused release.
use super::{Error, Result, accepted, device::Session};
use crate::render::{layers, primitive_assets, software};
use alloc::vec::Vec;
use r4gfx as g;
use r4os_contract as a;
#[derive(Default)]
pub struct Image {
    pub resource: g::R4GfxResource,
    pub color_view: g::R4GfxResource,
    pub opaque_view: g::R4GfxResource,
    pub info: g::R4GfxResourceInfo,
    pub generation: u64,
    pub charge: u64,
    pub external_reference: a::GfxBufferHandle,
    pub external_color: Option<g::R4GfxColorDescription>,
}
pub struct Store {
    pub layers: Vec<Image>,
    pub assets: Vec<Image>,
    pub outputs: Vec<Image>,
    pub workings: Vec<Image>,
    pub staging: Image,
    pub over: g::R4GfxResource,
    pub blit: g::R4GfxResource,
    pub fill: g::R4GfxResource,
    pub sampler: g::R4GfxResource,
    pub charged: u64,
    pub budget: u64,
}
pub fn descriptor(kind: u32) -> g::R4GfxResourceDesc {
    g::R4GfxResourceDesc {
        version: 1,
        size: core::mem::size_of::<g::R4GfxResourceDesc>() as u32,
        kind,
        ..Default::default()
    }
}
fn images(count: usize) -> Result<Vec<Image>> {
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| Error::Limit)?;
    values.resize_with(count, Image::default);
    Ok(values)
}
impl Store {
    pub fn new() -> Result<Self> {
        Ok(Self {
            layers: images(layers::CAPACITY)?,
            assets: images(primitive_assets::TEXTURE_CAPACITY)?,
            outputs: images(g::swapchain_image_capacity as usize)?,
            workings: images(g::swapchain_image_capacity as usize)?,
            staging: Image::default(),
            over: Default::default(),
            blit: Default::default(),
            fill: Default::default(),
            sampler: Default::default(),
            charged: 0,
            budget: 256 * 1024 * 1024,
        })
    }
    pub fn prepare_state(
        &mut self,
        s: &Session<'_, '_>,
        mut check: impl FnMut() -> Result<()>,
    ) -> Result<()> {
        check()?;
        state(
            &mut self.over,
            s,
            g::resource_pipeline,
            g::render_operation_over,
        )?;
        check()?;
        state(
            &mut self.blit,
            s,
            g::resource_pipeline,
            g::render_operation_blit,
        )?;
        check()?;
        state(
            &mut self.fill,
            s,
            g::resource_pipeline,
            g::render_operation_fill,
        )?;
        check()?;
        state(
            &mut self.sampler,
            s,
            g::resource_sampler,
            g::render_sampler_nearest,
        )
    }
    pub fn close(&mut self, s: &Session<'_, '_>) -> Result<()> {
        // Caller first drains jobs, scanout/readback and external receipts.
        for image in self
            .layers
            .iter_mut()
            .chain(self.assets.iter_mut())
            .chain(self.outputs.iter_mut())
            .chain(self.workings.iter_mut())
            .chain(core::iter::once(&mut self.staging))
        {
            image.release(s, &mut self.charged)?;
        }
        for handle in [
            &mut self.over,
            &mut self.blit,
            &mut self.fill,
            &mut self.sampler,
        ] {
            if handle.slot != 0 {
                accepted(unsafe { s.client.resource_release(s.raw(), handle) })?;
                *handle = Default::default();
            }
        }
        Ok(())
    }
}
fn state(out: &mut g::R4GfxResource, s: &Session<'_, '_>, kind: u32, operation: u32) -> Result<()> {
    if out.slot != 0 {
        return Ok(());
    }
    let mut d = descriptor(kind);
    if kind == g::resource_sampler {
        d.sampler = operation;
    } else {
        d.operation = operation;
    }
    accepted(unsafe { s.client.resource_create(s.raw(), &d, out) })
}
impl Image {
    pub fn fits(&mut self, s: &Session<'_, '_>, width: i32, height: i32, format: u32) -> bool {
        if self.resource.slot == 0 {
            return false;
        }
        let mut info = g::R4GfxResourceInfo::default();
        if unsafe { s.client.resource_info(s.raw(), &self.resource, &mut info) } != g::status_ok
            || info.flags & g::resource_invalidated != 0
            || info.image.width != width as u32
            || info.image.height != height as u32
            || info.image.format != format
        {
            return false;
        }
        self.info = info;
        true
    }
    pub fn color_view(&mut self, s: &Session<'_, '_>, opaque: bool) -> Result<()> {
        let output = if opaque {
            &mut self.opaque_view
        } else {
            &mut self.color_view
        };
        if output.slot != 0 {
            return Ok(());
        }
        let mut d = descriptor(g::resource_image);
        d.source_kind = g::source_color_view;
        d.source_address = &self.resource as *const _ as u64;
        let color = g::R4GfxColorResourceDesc {
            version: 1,
            size: core::mem::size_of::<g::R4GfxColorResourceDesc>() as u32,
            resource: d,
            description: software::description(false, opaque),
            ..Default::default()
        };
        accepted(unsafe { s.colors.color_resource_create(s.raw(), &color, output) })
    }
    pub fn prepare(
        &mut self,
        s: &Session<'_, '_>,
        charged: &mut u64,
        budget: u64,
        width: i32,
        height: i32,
        format: u32,
        native: bool,
        scanout: bool,
        deadline: u64,
        description: g::R4GfxColorDescription,
    ) -> Result<()> {
        if width <= 0 || height <= 0 {
            return Err(Error::State);
        }
        if self.resource.slot != 0 {
            if self.fits(s, width, height, format) {
                return Ok(());
            }
            self.release(s, charged)?;
        }
        let row = (width as u64)
            .checked_mul(if format == g::format_abgr16161616f {
                8
            } else {
                4
            })
            .ok_or(Error::Limit)?;
        let pitch = if native {
            row.checked_add(255).ok_or(Error::Limit)? & !255
        } else {
            row
        };
        let bytes = pitch.checked_mul(height as u64).ok_or(Error::Limit)?;
        let mask = if native { 65535 } else { 4095 };
        let charge = bytes.checked_add(mask).ok_or(Error::Limit)? & !mask;
        if charge > budget || *charged > budget - charge {
            return Err(Error::Limit);
        }
        let request = g::R4GfxNativeImage {
            version: 1,
            size: core::mem::size_of::<g::R4GfxNativeImage>() as u32,
            deadline_ns: deadline,
            width: width as u32,
            height: height as u32,
            format,
            layout: 0,
        };
        let mut d = descriptor(g::resource_image);
        d.flags = g::image_target;
        if native {
            d.source_kind = if scanout {
                g::source_create_native_scanout
            } else {
                g::source_create_native
            };
            d.source_address = &request as *const _ as u64;
        } else {
            d.source_kind = g::source_create_system;
            d.image = g::R4GfxCpuImage {
                cpu_address: 0,
                byte_length: bytes,
                pitch,
                width: width as u32,
                height: height as u32,
                format,
                reserved: 0,
            };
        }
        let rc = if [
            g::format_xrgb8888,
            g::format_xrgb2101010,
            g::format_abgr16161616f,
        ]
        .contains(&format)
        {
            let request = g::R4GfxColorResourceDesc {
                version: 1,
                size: core::mem::size_of::<g::R4GfxColorResourceDesc>() as u32,
                resource: d,
                description,
                ..Default::default()
            };
            unsafe {
                s.colors
                    .color_resource_create(s.raw(), &request, &mut self.resource)
            }
        } else {
            unsafe { s.client.resource_create(s.raw(), &d, &mut self.resource) }
        };
        // Track any returned native identity even on a refused metadata read.
        if self.resource.slot != 0 {
            self.charge = charge;
            *charged = charged.checked_add(charge).ok_or(Error::Limit)?;
        }
        accepted(rc)?;
        self.info = Default::default();
        accepted(unsafe {
            s.client
                .resource_info(s.raw(), &self.resource, &mut self.info)
        })?;
        if self.info.image.byte_length > charge {
            let extra = self.info.image.byte_length - charge;
            *charged = charged.checked_add(extra).ok_or(Error::Limit)?;
            self.charge = self.info.image.byte_length;
            self.release(s, charged)?;
            return Err(Error::Limit);
        }
        self.generation = 0;
        Ok(())
    }
    pub fn external(
        &mut self,
        s: &Session<'_, '_>,
        charged: &mut u64,
        budget: u64,
        reference: a::GfxBufferReference,
        description: a::GfxBufferDescriptor,
        color: g::R4GfxColorDescription,
    ) -> Result<()> {
        if same_handle(self.external_reference, reference.reference)
            && self
                .external_color
                .is_some_and(|previous| crate::render::color::same_description(previous, color))
            && self.fits(
                s,
                description.width as i32,
                description.height as i32,
                description.format,
            )
        {
            return Ok(());
        }
        self.release(s, charged)?;
        let bytes = description.byte_length;
        if bytes > budget || *charged > budget - bytes {
            return Err(Error::Limit);
        }
        let mut d = descriptor(g::resource_image);
        d.source_kind = g::source_import_buffer;
        d.source_address = &reference.reference as *const _ as u64;
        let request = g::R4GfxColorResourceDesc {
            version: 1,
            size: core::mem::size_of::<g::R4GfxColorResourceDesc>() as u32,
            resource: d,
            description: color,
            ..Default::default()
        };
        let rc = unsafe {
            s.colors
                .color_resource_create(s.raw(), &request, &mut self.resource)
        };
        if self.resource.slot != 0 {
            self.charge = bytes;
            *charged = charged.checked_add(bytes).ok_or(Error::Limit)?;
        }
        accepted(rc)?;
        accepted(unsafe {
            s.client
                .resource_info(s.raw(), &self.resource, &mut self.info)
        })?;
        if self.info.buffer_id != reference.buffer.id
            || self.info.buffer_generation != reference.buffer.generation
        {
            return Err(Error::Stale);
        }
        self.external_reference = reference.reference;
        self.external_color = Some(color);
        Ok(())
    }
    pub fn release(&mut self, s: &Session<'_, '_>, charged: &mut u64) -> Result<()> {
        if self.opaque_view.slot != 0 {
            accepted(unsafe { s.client.resource_release(s.raw(), &self.opaque_view) })?;
            self.opaque_view = Default::default();
        }
        if self.color_view.slot != 0 {
            accepted(unsafe { s.client.resource_release(s.raw(), &self.color_view) })?;
            self.color_view = Default::default();
        }
        if self.resource.slot == 0 {
            return Ok(());
        }
        accepted(unsafe { s.client.resource_release(s.raw(), &self.resource) })?;
        *charged = charged.checked_sub(self.charge).ok_or(Error::State)?;
        *self = Default::default();
        Ok(())
    }
}
fn same_handle(x: a::GfxBufferHandle, y: a::GfxBufferHandle) -> bool {
    x.id == y.id && x.generation == y.generation && x.reserved0 == y.reserved0
}
