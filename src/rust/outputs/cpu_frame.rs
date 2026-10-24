//! The CPU output's one frame transaction. CPU maps and scene loans finish
//! before presentation; every refusal follows the parent's retained teardown.
use super::*;
use crate::render::output_geometry::topology::Rotation;
use crate::{
    hosted::replay::Shapes,
    render::{
        compositor::{self, Completed, Scene},
        coverage::Path,
        output_geometry as geometry,
        scene::CpuScene,
        software::{Colors, ExternalFrame},
        text::cache::{Cache as FontCache, Fonts},
    },
    shell::{desktop_view, taskbar_view},
};
impl Output {
    /// # Safety
    /// Hosted frame maps are immutable through synchronous painting. Graphics
    /// maps grant exclusive writable storage through exact Unmap, as required
    /// by the ordinary SDK. No scene/map slice survives this transaction.
    pub unsafe fn render<'f>(
        &mut self,
        app: &App<'_>,
        graphics: &Graphics<'_, '_>,
        colors: &impl Colors,
        scene: &Scene<'_>,
        external: [Option<&'f mut dyn ExternalFrame>; 4],
        damage: Rect,
        input_ns: u64,
        time: u64,
        deadline: u64,
        cache: &mut FontCache,
        fonts: &mut impl Fonts,
        icons: &mut (impl desktop_view::Icons + taskbar_view::Icons),
        shapes: &mut impl Shapes,
    ) -> Result<Completed> {
        if scene.path != Path::CpuOutput || damage.is_empty() {
            return Err(Error::State);
        }
        self.poll(app, time);
        let state = self.state.as_deref_mut().ok_or(Error::State)?;
        state.capture_damage = Some(damage);
        let result = unsafe {
            state.render_frame(
                app, graphics, colors, scene, external, input_ns, time, deadline, cache, fonts,
                icons, shapes,
            )
        };
        if result.is_err() {
            state.abandon(app, graphics);
        }
        result
    }
}
impl State {
    unsafe fn render_frame<'f>(
        &mut self,
        app: &App<'_>,
        graphics: &Graphics<'_, '_>,
        colors: &impl Colors,
        scene: &Scene<'_>,
        external: [Option<&'f mut dyn ExternalFrame>; 4],
        input_ns: u64,
        time: u64,
        deadline: u64,
        cache: &mut FontCache,
        fonts: &mut impl Fonts,
        icons: &mut (impl desktop_view::Icons + taskbar_view::Icons),
        shapes: &mut impl Shapes,
    ) -> Result<Completed> {
        let (bounds, scratch) = self.begin_frame(app, graphics, input_ns, time)?;
        let count = CpuScene::required_bytes(bounds.w, bounds.h).ok_or(Error::Limit)? / 4;
        let pixels = if scratch {
            &mut self.scratch[..count]
        } else {
            // Kernel gfx/buffers::create commits initialized zeroed system
            // pages (memory/vm_transactions::commit). begin_frame admitted the
            // exact full writable mapping. The source
            // and destination of orientation are disjoint whenever scratch=true.
            unsafe { core::slice::from_raw_parts_mut(self.mapping.cpu_address as *mut u32, count) }
        };
        let mut canvas = CpuScene::output(pixels, bounds).ok_or(Error::State)?;
        let completed = unsafe {
            compositor::compose(
                &mut self.composition,
                colors,
                &mut canvas,
                scene,
                external,
                &[self.repaint],
                cache,
                fonts,
                icons,
                shapes,
            )
        }
        .map_err(|_| Error::Graphics)?;
        drop(canvas);
        self.serial = self.serial.saturating_add(1);
        if self.serial == 0 {
            return Err(Error::Limit);
        }
        let frame = self.acquired.ok_or(Error::State)?;
        self.capture.record(
            frame.slot as usize - 1,
            self.serial,
            self.capture_damage.unwrap_or(bounds),
            self.view,
            self.capture_cursor,
        );
        self.capture
            .stage_profile(frame.slot as usize - 1, pixels, time);
        let native_damage = physical_damage(self.view, self.repaint)?;
        if scratch {
            let output = unsafe {
                core::slice::from_raw_parts_mut(
                    self.mapping.cpu_address as *mut u32,
                    self.view.pixel_w as usize * self.view.pixel_h as usize,
                )
            };
            transform_region(self.view, pixels, bounds.w as usize, output, native_damage)?;
        }
        if self.profile.is_some() || self.limited {
            // The scene loan has ended. Profiles/limited quantization operate
            // only on this unpublished native image, after canonical capture.
            let output = unsafe {
                core::slice::from_raw_parts_mut(
                    self.mapping.cpu_address as *mut u32,
                    self.view.pixel_w as usize * self.view.pixel_h as usize,
                )
            };
            for row in native_damage.y..native_damage.bottom() {
                let start = row as usize * self.view.pixel_w as usize + native_damage.x as usize;
                let line = &mut output[start..start + native_damage.w as usize];
                if let Some(profile) = &self.profile {
                    profile
                        .apply(line, line.len() as u32, 1)
                        .map_err(|_| Error::Graphics)?;
                }
                if self.limited {
                    for pixel in line {
                        *pixel = limited_rgb8(*pixel);
                    }
                }
            }
        }
        if unsafe {
            code(c::R4SdkGuiDrawContext_gfxBufferUnmap(
                0,
                graphics.native_draw(),
                &self.mapping.lease,
            ))
        } != a::gfx_buffer_result_ok
        {
            self.lost = true;
            return Err(Error::Graphics);
        }
        self.mapping = a::GfxBufferMap::default();
        let session = self.device.bind(app).map_err(|_| Error::Graphics)?;
        let request = g::R4GfxSwapchainPresent {
            version: 1,
            size: core::mem::size_of::<g::R4GfxSwapchainPresent>() as u32,
            frame,
            deadline_ns: deadline,
            blockers: g::present_block_cursor
                | if self.capture.wanted() {
                    g::present_block_readers
                } else {
                    0
                },
            ..Default::default()
        };
        let rc = unsafe {
            session
                .client
                .swapchain_present(session.raw(), &self.chain, &request)
        };
        if rc != g::status_ok {
            if rc != g::status_busy && rc != g::status_occluded {
                self.lost = true;
            }
            return Err(if rc == g::status_busy || rc == g::status_occluded {
                Error::Busy
            } else {
                Error::Graphics
            });
        }
        self.history.accepted(frame.slot as usize - 1);
        self.acquired = None;
        self.pending = true;
        self.reported[frame.slot as usize - 1] = false;
        self.stats.last_submitted = Some(frame.slot - 1);
        if let Ok(ended) = r4os_sdk::time::Clock::new(app).instant() {
            if time != 0 && ended.nanoseconds >= time {
                self.stats.frame_submit_last_ns = ended.nanoseconds - time;
                self.stats.frame_submit_max_ns = self
                    .stats
                    .frame_submit_max_ns
                    .max(self.stats.frame_submit_last_ns);
            }
        }
        Ok(completed)
    }
    fn begin_frame(
        &mut self,
        app: &App<'_>,
        graphics: &Graphics<'_, '_>,
        input_ns: u64,
        time: u64,
    ) -> Result<(Rect, bool)> {
        if !self.ready || self.lost || self.acquired.is_some() || self.mapping.lease.id != 0 {
            return Err(Error::Busy);
        }
        let bounds = geometry::logical(self.view).map_err(|_| Error::Limit)?;
        let session = self.device.bind(app).map_err(|_| Error::Graphics)?;
        if self.capture.wanted() && time >= self.capture.retry_ns {
            let prepared = if self.profile.is_some() || self.limited {
                self.capture.prepare_profile(&session, self.view)
            } else {
                unsafe {
                    self.capture.prepare(
                        app,
                        &session,
                        self.view,
                        g::format_xrgb8888,
                        software::description(false, true),
                    )
                }
            };
            if prepared.is_err() {
                self.capture.retry(time);
            }
        }
        let mut frame = g::R4GfxSwapchainFrame::default();
        let rc = unsafe {
            session
                .client
                .swapchain_acquire(session.raw(), &self.chain, input_ns, &mut frame)
        };
        if rc != g::status_ok {
            if rc != g::status_busy && rc != g::status_occluded {
                self.lost = true;
            }
            return Err(if rc == g::status_busy || rc == g::status_occluded {
                Error::Busy
            } else {
                Error::Graphics
            });
        }
        self.acquired = Some(frame);
        if frame.slot == 0
            || frame.slot > self.count
            || !same_resource(frame.image, self.images[frame.slot as usize - 1])
        {
            self.lost = true;
            return Err(Error::Stale);
        }
        let bytes = u64::from(self.view.pixel_w) * u64::from(self.view.pixel_h) * 4;
        let rc = unsafe {
            code(c::R4SdkGuiDrawContext_gfxBufferMap(
                0,
                graphics.native_draw(),
                &self.references[frame.slot as usize - 1].reference,
                a::gfx_buffer_map_write,
                0,
                bytes,
                &mut self.mapping,
            ))
        };
        if rc != a::gfx_buffer_result_ok {
            return Err(Error::Busy);
        }
        if self.mapping.version != 1
            || self.mapping.size as usize != core::mem::size_of::<a::GfxBufferMap>()
            || self.mapping.reserved0 != 0
            || self.mapping.lease.id == 0
            || self.mapping.lease.generation == 0
            || self.mapping.lease.reserved0 != 0
            || self.mapping.cpu_address == 0
            || !self.mapping.cpu_address.is_multiple_of(4)
            || self.mapping.byte_length < bytes
            || self.mapping.cpu_address.checked_add(bytes).is_none()
        {
            self.lost = true;
            return Err(Error::Graphics);
        }
        let profile = self
            .profile
            .as_ref()
            .map(Profile::handle)
            .transpose()
            .map_err(|_| Error::Graphics)?
            .unwrap_or_default();
        let same_profile = self.history_profile.address == profile.address
            && self.history_profile.generation == profile.generation;
        if self.history_view != Some(self.view)
            || !same_profile
            || self.limited != self.history_limited
        {
            self.history = History::default();
            self.history_view = Some(self.view);
            self.history_profile = profile;
            self.history_limited = self.limited;
        }
        self.history
            .invalidate(self.capture_damage.unwrap_or(bounds));
        let scratch = self.profile.is_some()
            || self.limited
            || self.view.rotation != Rotation::Normal
            || self.view.scale != 120;
        if scratch {
            let count = CpuScene::required_bytes(bounds.w, bounds.h)
                .filter(|v| *v <= 64 * 1024 * 1024)
                .ok_or(Error::Limit)?
                / 4;
            if self.scratch.len() < count {
                let mut replacement = Vec::new();
                replacement
                    .try_reserve_exact(count)
                    .map_err(|_| Error::Limit)?;
                replacement.resize(count, 0);
                self.scratch = replacement;
                self.history = History::default();
            }
        }
        self.repaint = self.history.required(frame.slot as usize - 1, bounds);
        Ok((bounds, scratch))
    }
}
pub(super) fn physical_damage(view: Viewport, damage: Rect) -> Result<Rect> {
    if damage.is_empty() {
        return Ok(Rect::ZERO);
    }
    let area = view
        .physical_damage(geometry::topology::Rect {
            x: damage.x,
            y: damage.y,
            w: damage.w as u32,
            h: damage.h as u32,
        })
        .map_err(|_| Error::State)?;
    Ok(area.map_or(Rect::ZERO, |r| Rect::new(r.x, r.y, r.w as i32, r.h as i32)))
}
pub(super) fn transform_region(
    view: Viewport,
    source: &[u32],
    stride: usize,
    output: &mut [u32],
    damage: Rect,
) -> Result<()> {
    let projection = view.projection().map_err(|_| Error::State)?;
    if damage.is_empty() {
        return Ok(());
    }
    if damage.x < 0
        || damage.y < 0
        || damage.right() > view.pixel_w as i32
        || damage.bottom() > view.pixel_h as i32
        || output.len() < view.pixel_w as usize * view.pixel_h as usize
    {
        return Err(Error::State);
    }
    for y in damage.y as usize..damage.bottom() as usize {
        for x in damage.x as usize..damage.right() as usize {
            let (sx, sy) = projection.logical_pixel(x, y);
            output[y * view.pixel_w as usize + x] =
                *source.get(sy * stride + sx).ok_or(Error::State)?;
        }
    }
    Ok(())
}
pub(super) fn limited_rgb8(pixel: u32) -> u32 {
    // Exact original R4GFX color_signal.limitedRgb8 integer quantization.
    let mut result = 0xff000000;
    for shift in [0, 8, 16] {
        result |= (16 + (((pixel >> shift) & 255) * 219 + 127) / 255) << shift;
    }
    result
}
