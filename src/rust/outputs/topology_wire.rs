//! Generated native topology fields at the shared R4GFX boundary. All point,
//! layout validation and normalization stay with their existing owners.
use crate::{
    render::output_geometry::topology::{
        self as t, Key, Layout, Output, Point, Rotation, Viewport,
    },
    runtime::library::{required, scalar},
};
use core::mem::MaybeUninit;
use r4os_library_helpers::{Error, raw as c};
pub fn key(value: Key) -> Result<c::R4GfxTopologyKey, Error> {
    let mut out = MaybeUninit::uninit();
    required(unsafe {
        c::R4GfxTopologyKey_from_fields(
            out.as_mut_ptr(),
            value.adapter,
            value.connector,
            value.receiver.as_ptr().cast(),
        )
    })?;
    Ok(unsafe { out.assume_init() })
}
pub fn read_key(value: &c::R4GfxTopologyKey) -> Result<Key, Error> {
    let mut out = Key {
        adapter: scalar(unsafe { c::R4GfxTopologyKey_get_adapter(value) })?,
        connector: scalar(unsafe { c::R4GfxTopologyKey_get_connector(value) })? as u32,
        ..Default::default()
    };
    required(unsafe { c::R4GfxTopologyKey_get_receiver(value, out.receiver.as_mut_ptr().cast()) })?;
    Ok(out)
}
pub fn point(value: Point) -> Result<c::R4GfxTopologyPoint, Error> {
    let mut out = MaybeUninit::uninit();
    required(unsafe { c::R4GfxTopologyPoint_from_fields(out.as_mut_ptr(), value.x, value.y) })?;
    Ok(unsafe { out.assume_init() })
}
pub fn view(value: Viewport) -> Result<c::R4GfxTopologyViewport, Error> {
    let origin = point(value.origin)?;
    let mut out = MaybeUninit::uninit();
    required(unsafe {
        c::R4GfxTopologyViewport_from_fields(
            out.as_mut_ptr(),
            &origin,
            value.pixel_w,
            value.pixel_h,
            value.scale,
            value.rotation as u32,
        )
    })?;
    Ok(unsafe { out.assume_init() })
}
pub fn read_view(value: &c::R4GfxTopologyViewport) -> Result<Viewport, Error> {
    let mut origin = MaybeUninit::<c::R4GfxTopologyPoint>::uninit();
    required(unsafe { c::R4GfxTopologyViewport_get_origin(value, origin.as_mut_ptr()) })?;
    let origin = Point {
        x: scalar(unsafe { c::R4GfxTopologyPoint_get_x(origin.as_ptr()) })? as i32,
        y: scalar(unsafe { c::R4GfxTopologyPoint_get_y(origin.as_ptr()) })? as i32,
    };
    let rotation = match scalar(unsafe { c::R4GfxTopologyViewport_get_rotation(value) })? {
        0 => Rotation::Normal,
        1 => Rotation::Clockwise90,
        2 => Rotation::Clockwise180,
        3 => Rotation::Clockwise270,
        _ => return Err(Error::InvalidProvider),
    };
    Ok(Viewport {
        origin,
        pixel_w: scalar(unsafe { c::R4GfxTopologyViewport_get_pixel_w(value) })? as u32,
        pixel_h: scalar(unsafe { c::R4GfxTopologyViewport_get_pixel_h(value) })? as u32,
        scale: scalar(unsafe { c::R4GfxTopologyViewport_get_scale(value) })? as u32,
        rotation,
    })
}
pub fn output(value: Output) -> Result<c::R4GfxTopologyOutput, Error> {
    let key = key(value.key)?;
    let view = view(value.view)?;
    let mut out = MaybeUninit::uninit();
    required(unsafe {
        c::R4GfxTopologyOutput_from_fields(
            out.as_mut_ptr(),
            &key,
            &view,
            value.enabled as u8,
            value.primary as u8,
            value.clone_group,
            value.refresh_millihz,
        )
    })?;
    Ok(unsafe { out.assume_init() })
}
pub fn read_output(value: &c::R4GfxTopologyOutput) -> Result<Output, Error> {
    let mut key = MaybeUninit::<c::R4GfxTopologyKey>::uninit();
    let mut view = MaybeUninit::<c::R4GfxTopologyViewport>::uninit();
    required(unsafe { c::R4GfxTopologyOutput_get_key(value, key.as_mut_ptr()) })?;
    required(unsafe { c::R4GfxTopologyOutput_get_view(value, view.as_mut_ptr()) })?;
    Ok(Output {
        key: read_key(unsafe { key.assume_init_ref() })?,
        view: read_view(unsafe { view.assume_init_ref() })?,
        enabled: scalar(unsafe { c::R4GfxTopologyOutput_get_enabled(value) })? != 0,
        primary: scalar(unsafe { c::R4GfxTopologyOutput_get_primary(value) })? != 0,
        clone_group: scalar(unsafe { c::R4GfxTopologyOutput_get_clone_group(value) })? as u8,
        refresh_millihz: scalar(unsafe { c::R4GfxTopologyOutput_get_refresh_millihz(value) })?
            as u32,
    })
}
pub fn layout(value: &Layout) -> Result<c::R4GfxTopologyLayout, Error> {
    let dummy = output(Output {
        enabled: false,
        view: Viewport {
            pixel_w: 1,
            pixel_h: 1,
            ..Default::default()
        },
        ..Default::default()
    })?;
    let mut entries = [dummy; t::CAPACITY];
    for (i, value) in value.outputs().iter().enumerate() {
        entries[i] = output(*value)?;
    }
    let mut out = MaybeUninit::uninit();
    required(unsafe {
        c::R4GfxTopologyLayout_from_fields(
            out.as_mut_ptr(),
            entries.as_ptr().cast(),
            value.outputs().len() as u64,
            value.primary().unwrap_or(0) as u64,
            value.revision(),
        )
    })?;
    Ok(unsafe { out.assume_init() })
}
pub fn read_layout(value: &c::R4GfxTopologyLayout) -> Result<Layout, Error> {
    let count = scalar(unsafe { c::R4GfxTopologyLayout_get_count(value) })? as usize;
    if count > t::CAPACITY {
        return Err(Error::InvalidProvider);
    }
    if count == 0 {
        return Ok(Layout::default());
    }
    let revision = scalar(unsafe { c::R4GfxTopologyLayout_get_revision(value) })?;
    let mut source = [MaybeUninit::<c::R4GfxTopologyOutput>::uninit(); t::CAPACITY];
    required(unsafe { c::R4GfxTopologyLayout_get_outputs(value, source.as_mut_ptr().cast()) })?;
    let mut outputs = [Output::default(); t::CAPACITY];
    for i in 0..count {
        outputs[i] = read_output(unsafe { source[i].assume_init_ref() })?;
    }
    Layout::new(&outputs[..count], revision).map_err(|_| Error::InvalidProvider)
}
pub struct Normalized {
    pub layout: Layout,
    pub translation: Point,
}
pub fn normalize(
    values: &mut [Output],
    selected: usize,
    revision: u64,
) -> Result<Normalized, Error> {
    if values.len() > t::CAPACITY || selected >= values.len() {
        return Err(Error::Native("Bounds"));
    }
    let origin = values[selected].view.origin;
    let mut candidate = [Output::default(); t::CAPACITY];
    candidate[..values.len()].copy_from_slice(values);
    for (i, output) in candidate[..values.len()].iter_mut().enumerate() {
        output.primary = i == selected;
    }
    let admitted =
        Layout::new(&candidate[..values.len()], revision).map_err(|_| Error::Native("Bounds"))?;
    let native = layout(&admitted)?;
    let mut normalized = MaybeUninit::uninit();
    required(unsafe { c::R4GfxControl_normalized(0, &native, normalized.as_mut_ptr()) })?;
    let layout = read_layout(unsafe { normalized.assume_init_ref() })?;
    values.copy_from_slice(layout.outputs());
    Ok(Normalized {
        layout,
        translation: origin,
    })
}
