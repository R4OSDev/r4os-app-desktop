//! Ordered current GUI frame replay. Shared memory enters through an explicit
//! read-lease adapter; ordinary resources remain borrowed initialized bytes.
use super::{command_index::Bounds, frame_snapshot::View};
use crate::render::{
    geometry::Rect,
    painter::Painter,
    pixels::PixelBytes,
    primitive_assets::Result,
    primitive_image::{Format, Picture, View as Image},
    scene::intersect,
    text::cache::Fonts,
    ui::Ui,
};
use r4os_contract as a;

#[derive(Default, Debug)]
pub struct Stats {
    pub commands: u64,
    pub resource_bytes: u64,
}
pub trait Shapes {
    fn replay(
        &mut self,
        painter: &mut Painter<'_, '_>,
        client: Rect,
        command: &a::GuiFrameCommand,
        resource: &[u8],
    ) -> Result<()>;
}
pub struct ReadMap<'a> {
    pub metadata: &'a a::GuiSharedRasterMap,
    pub bytes: &'a [u8],
}
pub trait Maps {
    fn get(&self, descriptor: &a::GuiSharedRasterResource) -> Option<ReadMap<'_>>;
}
pub struct LeasedMaps<'a> {
    maps: &'a [a::GuiSharedRasterMap],
}
impl<'a> LeasedMaps<'a> {
    /// # Safety
    /// Every map is an exact admitted native read lease. Its initialized
    /// readable mapping stays resident and immutable until this borrow ends;
    /// no unmap, generation reset or writer may invalidate it in that scope.
    pub unsafe fn new(maps: &'a [a::GuiSharedRasterMap]) -> Self {
        Self { maps }
    }
}
impl Maps for LeasedMaps<'_> {
    fn get(&self, descriptor: &a::GuiSharedRasterResource) -> Option<ReadMap<'_>> {
        let map = self.maps.iter().find(|map| {
            map.lease.handle.id == descriptor.handle.id
                && map.lease.handle.generation == descriptor.handle.generation
                && map.lease.raster_generation == descriptor.raster_generation
        })?;
        let length = usize::try_from(map.byte_length).ok()?;
        if map.data_address == 0
            || length > isize::MAX as usize
            || map.data_address.checked_add(map.byte_length).is_none()
        {
            return None;
        }
        // The constructor's lease promise supplies pointer validity; the
        // metadata checks alone are deliberately not presented as that proof.
        let bytes = unsafe { core::slice::from_raw_parts(map.data_address as *const u8, length) };
        Some(ReadMap {
            metadata: map,
            bytes,
        })
    }
}
pub struct NoMaps;
impl Maps for NoMaps {
    fn get(&self, _: &a::GuiSharedRasterResource) -> Option<ReadMap<'_>> {
        None
    }
}

pub fn replay(
    ui: &mut Ui<'_, '_, impl Fonts>,
    client: Rect,
    frame: &View<'_>,
    maps: &impl Maps,
    shapes: &mut impl Shapes,
) -> Result<Stats> {
    let paint = ui.painter.clip();
    let selection = frame
        .index
        .select(frame.commands.len(), Bounds::local_clip(client, paint));
    let mut stats = Stats::default();
    for index in selection {
        let command = &frame.commands[index];
        stats.commands = stats.commands.wrapping_add(1);
        if command.version != a::gui_frame_command_version
            || command.size != a::gui_frame_command_size
        {
            continue;
        }
        if bounded(command.kind)
            && !paint_rect(client, command).is_some_and(|rect| rect.intersects(paint))
        {
            continue;
        }
        stats.resource_bytes = stats.resource_bytes.saturating_add(command.resource_bytes);
        match command.kind {
            a::gui_frame_command_kind_clear
                if command.resource_bytes == 0
                    && command.parameter0 == 0
                    && command.parameter1 == 0 =>
            {
                ui.fill(client, command.rgb)?
            }
            a::gui_frame_command_kind_rect
                if command.resource_bytes == 0
                    && command.parameter0 == 0
                    && command.parameter1 == 0 =>
            {
                if let Some(rect) = item_rect(client, command) {
                    ui.fill(rect, command.rgb)?;
                }
            }
            a::gui_frame_command_kind_text
                if command.parameter0 == 0 && command.parameter1 == 0 =>
            {
                if let (Some(text), Some(x), Some(y), Some(clip)) = (
                    resource(frame.resources, command),
                    client.x.checked_add(command.x),
                    client.y.checked_add(command.y),
                    intersect(client, paint),
                ) {
                    if x < clip.right() && y < clip.bottom() {
                        ui.text_slice(command.font_id, x, y, text, command.fg, command.bg, clip)?;
                    }
                }
            }
            a::gui_frame_command_kind_raster => {
                raster(&mut ui.painter, client, command, frame.resources)?
            }
            a::gui_frame_command_kind_indexed8 => {
                indexed(&mut ui.painter, client, command, frame.resources)?
            }
            a::gui_frame_command_kind_xrgb32_nearest => {
                nearest(&mut ui.painter, client, command, frame.resources)?
            }
            a::gui_frame_command_kind_shared_raster => {
                shared(&mut ui.painter, client, command, frame.resources, maps)?
            }
            a::gui_frame_command_kind_alpha8 => {
                alpha(&mut ui.painter, client, command, frame.resources)?
            }
            a::gui_frame_command_kind_argb32 => {
                argb(&mut ui.painter, client, command, frame.resources)?
            }
            a::gui_frame_command_kind_path_fill
            | a::gui_frame_command_kind_path_stroke
            | a::gui_frame_command_kind_rounded_rect
            | a::gui_frame_command_kind_shadow => {
                if item_rect(client, command).is_some() {
                    if let Some(bytes) = resource(frame.resources, command) {
                        shapes.replay(&mut ui.painter, client, command, bytes)?;
                    }
                }
            }
            _ => (),
        }
    }
    Ok(stats)
}
fn bounded(kind: u32) -> bool {
    matches!(
        kind,
        a::gui_frame_command_kind_rect
            | a::gui_frame_command_kind_raster
            | a::gui_frame_command_kind_indexed8
            | a::gui_frame_command_kind_xrgb32_nearest
            | a::gui_frame_command_kind_shared_raster
            | a::gui_frame_command_kind_alpha8
            | a::gui_frame_command_kind_argb32
            | a::gui_frame_command_kind_path_fill
            | a::gui_frame_command_kind_path_stroke
            | a::gui_frame_command_kind_rounded_rect
            | a::gui_frame_command_kind_shadow
    )
}
fn clipped(bounds: Rect, x: i64, y: i64, w: i64, h: i64) -> Option<Rect> {
    let (left, top) = (i64::from(bounds.x).max(x), i64::from(bounds.y).max(y));
    let (right, bottom) = (
        i64::from(bounds.right()).min(x.checked_add(w)?),
        i64::from(bounds.bottom()).min(y.checked_add(h)?),
    );
    if right <= left || bottom <= top {
        return None;
    }
    Some(Rect::new(
        left as i32,
        top as i32,
        (right - left) as i32,
        (bottom - top) as i32,
    ))
}
fn paint_rect(bounds: Rect, command: &a::GuiFrameCommand) -> Option<Rect> {
    let scale = if matches!(
        command.kind,
        a::gui_frame_command_kind_raster | a::gui_frame_command_kind_argb32
    ) {
        command.parameter0
    } else {
        1
    };
    if !(1..=16).contains(&scale) {
        return None;
    }
    clipped(
        bounds,
        i64::from(bounds.x) + i64::from(command.x),
        i64::from(bounds.y) + i64::from(command.y),
        i64::from(command.w).checked_mul(scale as i64)?,
        i64::from(command.h).checked_mul(scale as i64)?,
    )
}
fn item_rect(bounds: Rect, command: &a::GuiFrameCommand) -> Option<Rect> {
    clipped(
        bounds,
        i64::from(bounds.x) + i64::from(command.x),
        i64::from(bounds.y) + i64::from(command.y),
        i64::from(command.w),
        i64::from(command.h),
    )
}
fn resource<'a>(bytes: &'a [u8], command: &a::GuiFrameCommand) -> Option<&'a [u8]> {
    let start = usize::try_from(command.resource_offset).ok()?;
    bytes.get(start..start.checked_add(usize::try_from(command.resource_bytes).ok()?)?)
}
fn dimensions(command: &a::GuiFrameCommand, max_w: u32, max_h: u32, max_pixels: u64) -> bool {
    command.w > 0
        && command.h > 0
        && command.w <= max_w
        && command.h <= max_h
        && u64::from(command.w) * u64::from(command.h) <= max_pixels
}
fn no_text(command: &a::GuiFrameCommand) -> bool {
    command.fg == 0
        && command.bg == 0
        && command.font_id == 0
        && command.text_w == 0
        && command.text_h == 0
        && command.baseline == 0
        && command.line_height == 0
        && command.parameter0 == 0
        && command.parameter1 == 0
}
fn indexed_geometry(command: &a::GuiFrameCommand) -> bool {
    command.w > 0
        && command.h > 0
        && command.w <= a::gui_raster_max_width
        && command.h <= a::gui_raster_max_height
        && command.rgb == 0
        && no_text(command)
}
// These three compiler-projected wire records contain only integer scalars.
// Every bit pattern is valid, and read_unaligned avoids a transport alignment
// claim. Keep this private admission list closed to references/enums/pointers.
unsafe trait Wire: Copy {}
unsafe impl Wire for a::GuiIndexed8Resource {}
unsafe impl Wire for a::GuiXrgb32Resource {}
unsafe impl Wire for a::GuiSharedRasterResource {}
fn record<T: Wire>(bytes: &[u8]) -> Option<T> {
    if bytes.len() < core::mem::size_of::<T>() {
        return None;
    }
    Some(unsafe { core::ptr::read_unaligned(bytes.as_ptr().cast::<T>()) })
}
fn picture(
    painter: &mut Painter<'_, '_>,
    view: Image<'_>,
    viewport: Rect,
    clip: Rect,
    source_x: u32,
    source_y: u32,
    guest_w: u32,
    guest_h: u32,
) -> Result<()> {
    if !view.valid() {
        return Ok(());
    }
    painter.picture(Picture {
        view,
        viewport,
        clip,
        source_x,
        source_y,
        guest_w,
        guest_h,
    })
}
fn raster(
    painter: &mut Painter<'_, '_>,
    bounds: Rect,
    command: &a::GuiFrameCommand,
    resources: &[u8],
) -> Result<()> {
    if !dimensions(
        command,
        a::gui_raster_max_width,
        a::gui_raster_max_height,
        u64::from(a::gui_raster_max_pixels),
    ) || !(1..=16).contains(&command.parameter0)
        || command.parameter1 != 0
        || u64::from(command.w) * u64::from(command.h) * 4 != command.resource_bytes
    {
        return Ok(());
    }
    let Some(source) = resource(resources, command).and_then(PixelBytes::new) else {
        return Ok(());
    };
    let scale = command.parameter0 as i64;
    for row in 0..command.h as usize {
        let mut start = 0;
        while start < command.w as usize {
            let color = source.word(row * command.w as usize + start);
            let mut end = start + 1;
            while end < command.w as usize && source.word(row * command.w as usize + end) == color {
                end += 1;
            }
            if let Some(rect) = clipped(
                bounds,
                i64::from(bounds.x) + i64::from(command.x) + start as i64 * scale,
                i64::from(bounds.y) + i64::from(command.y) + row as i64 * scale,
                (end - start) as i64 * scale,
                scale,
            ) {
                painter.fill(rect, color & 0xffffff)?;
            }
            start = end;
        }
    }
    Ok(())
}
#[cfg(test)]
#[path = "../../../Tests/RustNative/frame_pixels.rs"]
mod tests;
fn indexed(
    painter: &mut Painter<'_, '_>,
    bounds: Rect,
    command: &a::GuiFrameCommand,
    resources: &[u8],
) -> Result<()> {
    if !indexed_geometry(command) {
        return Ok(());
    }
    let Some(bytes) = resource(resources, command) else {
        return Ok(());
    };
    let Some(h) = record::<a::GuiIndexed8Resource>(bytes) else {
        return Ok(());
    };
    if h.version != a::gui_indexed8_resource_version
        || h.size != a::gui_indexed8_resource_size
        || h.source_w == 0
        || h.source_h == 0
        || h.source_w > a::gui_raster_max_width
        || h.source_h > a::gui_raster_max_height
        || h.guest_w == 0
        || h.guest_h == 0
        || h.viewport_w == 0
        || h.viewport_h == 0
        || h.palette_entries != a::gui_indexed8_palette_entries
        || h.palette_offset != a::gui_indexed8_palette_offset
        || h.pixels_offset != a::gui_indexed8_pixels_offset
        || h.pixel_stride != h.source_w
        || u64::from(h.pixels_offset) + u64::from(h.source_w) * u64::from(h.source_h)
            != bytes.len() as u64
        || u64::from(h.source_x) + u64::from(h.source_w) > u64::from(h.guest_w)
        || u64::from(h.source_y) + u64::from(h.source_h) > u64::from(h.guest_h)
    {
        return Ok(());
    }
    let Some(clip) = item_rect(bounds, command) else {
        return Ok(());
    };
    let Some(viewport) = viewport(
        bounds,
        h.viewport_x,
        h.viewport_y,
        h.viewport_w,
        h.viewport_h,
    ) else {
        return Ok(());
    };
    let palette_start = h.palette_offset as usize;
    let Some(palette) = bytes
        .get(palette_start..palette_start + 1024)
        .and_then(PixelBytes::new)
    else {
        return Ok(());
    };
    let mut view = Image::new(
        Format::Indexed,
        h.source_w,
        h.source_h,
        h.pixel_stride as usize,
        &bytes[h.pixels_offset as usize..],
    );
    view.palette = palette;
    picture(
        painter, view, viewport, clip, h.source_x, h.source_y, h.guest_w, h.guest_h,
    )
}
fn nearest(
    painter: &mut Painter<'_, '_>,
    bounds: Rect,
    command: &a::GuiFrameCommand,
    resources: &[u8],
) -> Result<()> {
    if !indexed_geometry(command) {
        return Ok(());
    }
    let Some(bytes) = resource(resources, command) else {
        return Ok(());
    };
    let Some(h) = record::<a::GuiXrgb32Resource>(bytes) else {
        return Ok(());
    };
    if h.version != a::gui_xrgb32_resource_version
        || h.size != a::gui_xrgb32_resource_size
        || h.source_w == 0
        || h.source_h == 0
        || h.source_w > a::gui_raster_max_width
        || h.source_h > a::gui_raster_max_height
        || h.guest_w == 0
        || h.guest_h == 0
        || h.viewport_w == 0
        || h.viewport_h == 0
        || h.pixels_offset != a::gui_xrgb32_pixels_offset
        || h.pixel_stride < h.source_w
        || h.pixel_stride > a::gui_raster_max_width
        || h.flags != 0
        || h.reserved0 != 0
        || u64::from(h.pixels_offset)
            + ((u64::from(h.source_h) - 1) * u64::from(h.pixel_stride) + u64::from(h.source_w)) * 4
            != bytes.len() as u64
        || u64::from(h.source_x) + u64::from(h.source_w) > u64::from(h.guest_w)
        || u64::from(h.source_y) + u64::from(h.source_h) > u64::from(h.guest_h)
    {
        return Ok(());
    }
    let Some(clip) = item_rect(bounds, command) else {
        return Ok(());
    };
    let Some(viewport) = viewport(
        bounds,
        h.viewport_x,
        h.viewport_y,
        h.viewport_w,
        h.viewport_h,
    ) else {
        return Ok(());
    };
    picture(
        painter,
        Image::new(
            Format::Xrgb,
            h.source_w,
            h.source_h,
            h.pixel_stride as usize * 4,
            &bytes[h.pixels_offset as usize..],
        ),
        viewport,
        clip,
        h.source_x,
        h.source_y,
        h.guest_w,
        h.guest_h,
    )
}
fn viewport(bounds: Rect, x: i32, y: i32, w: u32, h: u32) -> Option<Rect> {
    if w == 0 || h == 0 {
        return None;
    }
    let rect = Rect::new(
        bounds.x.checked_add(x)?,
        bounds.y.checked_add(y)?,
        i32::try_from(w).ok()?,
        i32::try_from(h).ok()?,
    );
    rect.x.checked_add(rect.w)?;
    rect.y.checked_add(rect.h)?;
    Some(rect)
}
fn alpha_row(
    painter: &mut Painter<'_, '_>,
    bounds: Rect,
    x: i32,
    y: i32,
    rgb: u32,
    alpha: &[u8],
) -> Result<()> {
    let Some(clip) = clipped(bounds, i64::from(x), i64::from(y), alpha.len() as i64, 1) else {
        return Ok(());
    };
    let source_x = (i64::from(clip.x) - i64::from(x)) as usize;
    let mut view = Image::new(
        Format::Alpha,
        clip.w as u32,
        1,
        clip.w as usize,
        &alpha[source_x..source_x + clip.w as usize],
    );
    view.foreground = rgb;
    picture(painter, view, clip, clip, 0, 0, clip.w as u32, 1)
}
fn alpha(
    painter: &mut Painter<'_, '_>,
    bounds: Rect,
    command: &a::GuiFrameCommand,
    resources: &[u8],
) -> Result<()> {
    if !dimensions(
        command,
        a::gui_alpha8_max_width,
        a::gui_alpha8_max_height,
        u64::from(a::gui_alpha8_max_pixels),
    ) || command.parameter0 != 0
        || command.parameter1 != 0
        || u64::from(command.w) * u64::from(command.h) != command.resource_bytes
    {
        return Ok(());
    }
    let Some(bytes) = resource(resources, command) else {
        return Ok(());
    };
    let Some(x) = bounds.x.checked_add(command.x) else {
        return Ok(());
    };
    for row in 0..command.h {
        let y = i64::from(bounds.y) + i64::from(command.y) + i64::from(row);
        let Ok(y) = i32::try_from(y) else {
            continue;
        };
        let offset = row as usize * command.w as usize;
        alpha_row(
            painter,
            bounds,
            x,
            y,
            command.rgb,
            &bytes[offset..offset + command.w as usize],
        )?;
    }
    Ok(())
}
fn argb(
    painter: &mut Painter<'_, '_>,
    bounds: Rect,
    command: &a::GuiFrameCommand,
    resources: &[u8],
) -> Result<()> {
    if !dimensions(
        command,
        a::gui_argb32_max_width,
        a::gui_argb32_max_height,
        u64::from(a::gui_argb32_max_pixels),
    ) || !(1..=16).contains(&command.parameter0)
        || command.parameter1 != 0
        || u64::from(command.w) * u64::from(command.h) * 4 != command.resource_bytes
    {
        return Ok(());
    }
    let Some(bytes) = resource(resources, command) else {
        return Ok(());
    };
    let Some(viewport) = viewport(
        bounds,
        command.x,
        command.y,
        command.w * command.parameter0 as u32,
        command.h * command.parameter0 as u32,
    ) else {
        return Ok(());
    };
    picture(
        painter,
        Image::new(
            Format::Argb,
            command.w,
            command.h,
            command.w as usize * 4,
            bytes,
        ),
        viewport,
        bounds,
        0,
        0,
        command.w,
        command.h,
    )
}
fn shared(
    painter: &mut Painter<'_, '_>,
    bounds: Rect,
    command: &a::GuiFrameCommand,
    resources: &[u8],
    maps: &impl Maps,
) -> Result<()> {
    if !dimensions(
        command,
        a::gui_raster_max_width,
        a::gui_raster_max_height,
        u64::from(a::gui_raster_max_pixels),
    ) || !no_text(command)
    {
        return Ok(());
    }
    let Some(bytes) = resource(resources, command) else {
        return Ok(());
    };
    if bytes.len() != core::mem::size_of::<a::GuiSharedRasterResource>() {
        return Ok(());
    }
    let Some(d) = record::<a::GuiSharedRasterResource>(bytes) else {
        return Ok(());
    };
    if d.version != a::gui_shared_raster_resource_version
        || d.size != a::gui_shared_raster_resource_size
        || d.handle.id == 0
        || d.handle.generation == 0
        || d.raster_generation == 0
        || d.flags != 0
        || d.source_w == 0
        || d.source_h == 0
        || d.guest_w == 0
        || d.guest_h == 0
        || d.viewport_w == 0
        || d.viewport_h == 0
        || u64::from(d.source_x) + u64::from(d.source_w) > u64::from(d.guest_w)
        || u64::from(d.source_y) + u64::from(d.source_h) > u64::from(d.guest_h)
        || (d.format != a::gui_shared_raster_format_alpha8 && command.rgb != 0)
    {
        return Ok(());
    }
    let Some(map) = maps.get(&d) else {
        return Ok(());
    };
    let m = map.metadata;
    if m.format != d.format
        || m.lease.handle.id != d.handle.id
        || m.lease.handle.generation != d.handle.generation
        || m.lease.raster_generation != d.raster_generation
    {
        return Ok(());
    }
    let Some(memory) = usize::try_from(m.byte_length)
        .ok()
        .and_then(|length| map.bytes.get(..length))
    else {
        return Ok(());
    };
    let Some(clip) = item_rect(bounds, command) else {
        return Ok(());
    };
    let Some(viewport) = viewport(
        bounds,
        d.viewport_x,
        d.viewport_y,
        d.viewport_w,
        d.viewport_h,
    ) else {
        return Ok(());
    };
    match d.format {
        a::gui_shared_raster_format_xrgb32 if m.data_offset == 0 && m.stride_bytes % 4 == 0 => {
            picture(
                painter,
                Image::new(
                    Format::Xrgb,
                    d.source_w,
                    d.source_h,
                    m.stride_bytes as usize,
                    memory,
                ),
                viewport,
                clip,
                d.source_x,
                d.source_y,
                d.guest_w,
                d.guest_h,
            )?
        }
        a::gui_shared_raster_format_indexed8
            if m.data_offset == a::gui_indexed8_pixels_offset
                && m.data_offset as usize <= memory.len() =>
        {
            let Some(palette) = memory.get(..1024).and_then(PixelBytes::new) else {
                return Ok(());
            };
            let mut view = Image::new(
                Format::Indexed,
                d.source_w,
                d.source_h,
                m.stride_bytes as usize,
                &memory[m.data_offset as usize..],
            );
            view.palette = palette;
            picture(
                painter, view, viewport, clip, d.source_x, d.source_y, d.guest_w, d.guest_h,
            )?;
        }
        a::gui_shared_raster_format_alpha8
            if m.data_offset == 0
                && d.viewport_w == d.guest_w
                && d.viewport_h == d.guest_h
                && command.w == d.source_w
                && command.h == d.source_h =>
        {
            let Some(x) = bounds.x.checked_add(command.x) else {
                return Ok(());
            };
            let Some(y) = bounds.y.checked_add(command.y) else {
                return Ok(());
            };
            for row in 0..d.source_h {
                let Some(offset) = (d.source_y as usize + row as usize)
                    .checked_mul(m.stride_bytes as usize)
                    .and_then(|v| v.checked_add(d.source_x as usize))
                else {
                    return Ok(());
                };
                let Some(alpha) = offset
                    .checked_add(d.source_w as usize)
                    .and_then(|end| memory.get(offset..end))
                else {
                    return Ok(());
                };
                let Some(y) = y.checked_add(row as i32) else {
                    return Ok(());
                };
                alpha_row(painter, bounds, x, y, command.rgb, alpha)?;
            }
        }
        _ => (),
    }
    Ok(())
}
