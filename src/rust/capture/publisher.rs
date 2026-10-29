//! Demand-only synchronous publication into the kernel CPU snapshot owner.
//! A readback map remains immutable; separate cursor pixels use the existing
//! Desktop CPU scene storage rather than writing through a read-only map.
use crate::{
    outputs::{manager::Manager, native::Output},
    render::{
        cursor,
        geometry::{Dirty, MAX_DAMAGE_REGIONS, Rect},
        runtime::Frames,
        scene::{CpuScene, intersect},
    },
};
use r4os_sdk::{App, abi as a};
#[derive(Default)]
pub struct Publisher {
    pub consumers: u32,
    pub cursor_damage: Dirty,
    pub suppressed: bool,
}
fn consumers(app: &App<'_>) -> u32 {
    let Some(table) = app.desktop_api().filter(|t| t.remote_frame_consumers != 0) else {
        return 0;
    };
    let call: a::R4DeskFns::remote_frame_consumers =
        unsafe { core::mem::transmute(table.remote_frame_consumers) };
    unsafe { call() }
}
fn publish(
    app: &App<'_>,
    pixels: &[u32],
    width: i32,
    height: i32,
    regions: &[Rect],
    pointer: super::remote::Cursor,
) -> i32 {
    let Some(table) = app
        .desktop_api()
        .filter(|t| t.remote_frame_publish_regions != 0)
    else {
        return a::err_no_fn;
    };
    if width <= 0
        || height <= 0
        || regions.is_empty()
        || regions.len() > MAX_DAMAGE_REGIONS
        || pixels.len() != width as usize * height as usize
    {
        return a::remote_frame_error_invalid;
    }
    let mut wire = [a::DisplayDamageRect::default(); MAX_DAMAGE_REGIONS];
    let mut bounds = Rect::ZERO;
    for (i, region) in regions.iter().copied().enumerate() {
        if intersect(region, Rect::new(0, 0, width, height)) != Some(region) {
            return a::remote_frame_error_out_of_range;
        }
        wire[i] = a::DisplayDamageRect {
            x: region.x,
            y: region.y,
            w: region.w as u32,
            h: region.h as u32,
        };
        bounds = bounds.merged(region);
    }
    let Ok(count) = u32::try_from(pixels.len()) else {
        return a::remote_frame_error_invalid;
    };
    let Some(bytes) = count.checked_mul(4) else {
        return a::remote_frame_error_invalid;
    };
    let info = a::RemoteFrameInfo {
        flags: a::remote_frame_flag_ready
            | a::remote_frame_flag_dirty_valid
            | a::remote_frame_flag_cursor_valid,
        width: width as u32,
        height: height as u32,
        stride_pixels: width as u32,
        bytes_per_pixel: 4,
        frame_pixels: count,
        frame_bytes: bytes,
        dirty_x: bounds.x,
        dirty_y: bounds.y,
        dirty_w: bounds.w as u32,
        dirty_h: bounds.h as u32,
        cursor_x: pointer.x,
        cursor_y: pointer.y,
        cursor_flags: if pointer.visible {
            a::remote_frame_cursor_flag_visible
        } else {
            0
        },
        ..Default::default()
    };
    let call: a::R4DeskFns::remote_frame_publish_regions =
        unsafe { core::mem::transmute(table.remote_frame_publish_regions) };
    unsafe {
        call(
            &info,
            pixels.as_ptr(),
            count,
            wire.as_ptr(),
            regions.len() as u32,
        )
    }
}
impl Publisher {
    pub fn refresh(&mut self, app: &App<'_>) -> bool {
        let next = consumers(app);
        let became = self.consumers == 0 && next != 0;
        self.consumers = next;
        if next == 0 {
            self.cursor_damage = Dirty::default();
        }
        became
    }
    pub fn sync(&self, outputs: &mut Manager<'_>, legacy: Option<&mut Output<'_>>) {
        let wanted = self.consumers != 0 && !self.suppressed;
        if let Some(owner) = legacy {
            owner.capture_demand(wanted && !outputs.active());
        }
        let active = outputs.active();
        let primary = outputs.layout.primary();
        for slot in &mut outputs.slots {
            let selected = wanted
                && active
                && slot.logical_index == primary
                && !slot.disabled
                && !slot.sleeping
                && !slot.failed
                && !slot.reconfiguring;
            if let Some(owner) = slot.resource.gpu_mut() {
                owner.capture_demand(selected);
            }
            if let Some(owner) = slot.resource.software_mut() {
                owner.capture_demand(selected);
            }
        }
    }
    pub fn reset(
        &mut self,
        app: &App<'_>,
        outputs: &mut Manager<'_>,
        legacy: Option<&mut Output<'_>>,
    ) -> i32 {
        let code = app
            .desktop_api()
            .filter(|t| t.remote_frame_source_reset != 0)
            .map_or(a::err_no_fn, |table| {
                let call: a::R4DeskFns::remote_frame_source_reset =
                    unsafe { core::mem::transmute(table.remote_frame_source_reset) };
                unsafe { call() }
            });
        if let Some(owner) = legacy {
            owner.invalidate_capture();
        }
        for slot in &mut outputs.slots {
            if let Some(owner) = slot.resource.gpu_mut() {
                owner.invalidate_capture();
            }
            if let Some(owner) = slot.resource.software_mut() {
                owner.invalidate_capture();
            }
        }
        self.cursor_damage = Dirty::default();
        code
    }
    fn result(&mut self, code: i32) {
        if code >= 0 || code == a::remote_frame_error_unavailable {
            self.cursor_damage = Dirty::default();
        }
    }
    pub fn primary(
        &mut self,
        app: &App<'_>,
        frames: &mut Frames,
        regions: &[Rect],
        pointer: super::remote::Cursor,
    ) -> i32 {
        if self.suppressed || self.consumers == 0 {
            return a::remote_frame_error_unavailable;
        }
        let Some(mut canvas) = frames.canonical() else {
            return a::remote_frame_error_unavailable;
        };
        let bounds = canvas.full_rect();
        let mut dirty = Dirty::default();
        for region in regions.iter().chain(self.cursor_damage.regions()) {
            if let Some(clipped) = intersect(*region, bounds) {
                dirty.invalidate(clipped);
            }
        }
        if !dirty.active() {
            return a::remote_frame_error_unavailable;
        }
        let code = if pointer.separate && pointer.visible {
            let overlay = cursor::Overlay::apply(&mut canvas, pointer.x, pointer.y);
            publish(
                app,
                overlay.pixels(),
                bounds.w,
                bounds.h,
                dirty.regions(),
                pointer,
            )
        } else {
            publish(
                app,
                canvas.pixels(),
                bounds.w,
                bounds.h,
                dirty.regions(),
                pointer,
            )
        };
        self.result(code);
        code
    }
    pub fn image(
        &mut self,
        app: &App<'_>,
        frames: &mut Frames,
        image: super::remote::Image<'_>,
        width: i32,
        height: i32,
    ) -> bool {
        if self.suppressed || self.consumers == 0 || image.width != width || image.height != height
        {
            return false;
        }
        let code = if image.cursor.separate && image.cursor.visible {
            frames
                .capture_shadow(image.pixels, width, height, |canvas| {
                    let overlay = cursor::Overlay::apply(canvas, image.cursor.x, image.cursor.y);
                    publish(
                        app,
                        overlay.pixels(),
                        width,
                        height,
                        &[image.damage],
                        image.cursor,
                    )
                })
                .unwrap_or(a::remote_frame_error_unavailable)
        } else if CpuScene::required_bytes(width, height).map(|bytes| bytes / 4)
            != Some(image.pixels.len())
        {
            a::remote_frame_error_unavailable
        } else {
            // Readback publication still invalidates a prior CPU scene. The
            // immutable source loan lasts through the synchronous kernel copy;
            // no mutable shadow or resize is needed without an actual overlay.
            frames.current = false;
            publish(
                app,
                image.pixels,
                width,
                height,
                &[image.damage],
                image.cursor,
            )
        };
        self.result(code);
        code >= 0
    }
}
