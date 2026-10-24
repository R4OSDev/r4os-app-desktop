//! Ordered uploads, primitive groups, linear composition and output encoding.
//! Every admitted job is tracked before metadata queries; no native wait here.
use super::*;
impl Engine {
    pub(super) fn step(
        &mut self,
        s: &Session<'_, '_>,
        cache: &Cache,
        recording: Option<&Frame>,
    ) -> Result<()> {
        let dependency = self.jobs.dependency()?.first().copied();
        let dependencies = dependency.as_ref().map_or(&[][..], core::slice::from_ref);
        let pointer = if dependencies.is_empty() {
            0
        } else {
            dependencies.as_ptr() as u64
        };
        match self.phase {
            Phase::AssetUpload => {
                let frame = recording.ok_or(Error::State)?;
                if self.jobs.stage.is_some() {
                    return Err(Error::Busy);
                }
                while self.next_asset < self.resources.assets.len() {
                    let i = self.next_asset;
                    let texture = frame.assets.texture(i);
                    let image = &self.resources.assets[i];
                    if texture.pinned != cache.frame() || image.generation == texture.generation {
                        self.next_asset += 1;
                        continue;
                    }
                    if image.resource.slot == 0 {
                        return Err(Error::State);
                    }
                    let area = if image.generation == 0 {
                        texture.rect()
                    } else {
                        texture.dirty.unwrap_or_else(|| texture.rect())
                    };
                    let tile = Rect::new(
                        area.x + self.asset_x,
                        area.y + self.asset_y,
                        (self.resources.staging.info.image.width as i32).min(area.w - self.asset_x),
                        (self.resources.staging.info.image.height as i32)
                            .min(area.h - self.asset_y),
                    );
                    let slot = self.jobs.reserve()?;
                    self.stage_pixels(
                        s,
                        texture.pixels(),
                        texture.width,
                        texture.height,
                        texture.generation,
                        tile,
                    )?;
                    let image = &self.resources.assets[i];
                    let staging = &self.resources.staging;
                    let request = g::R4GfxCopyRequestEx {
                        version: 1,
                        size: core::mem::size_of::<g::R4GfxCopyRequestEx>() as u32,
                        copy: g::R4GfxCopyRequest {
                            source: staging.resource,
                            target: image.resource,
                            source_offset: 0,
                            target_offset: tile.y as u64 * image.info.image.pitch
                                + tile.x as u64 * 4,
                            byte_length: tile.w as u64 * 4,
                            deadline_ns: self.deadline,
                        },
                        row_count: tile.h as u32,
                        source_pitch: staging.info.image.pitch,
                        target_pitch: image.info.image.pitch,
                        dependency_count: dependencies.len() as u32,
                        dependencies: pointer,
                    };
                    let mut handle = g::R4GfxJob::default();
                    accepted(unsafe { s.client.copy_submit_ex(s.raw(), &request, &mut handle) })?;
                    self.jobs.track(
                        s,
                        slot,
                        handle,
                        None,
                        texture.generation,
                        tile.w as u64 * tile.h as u64 * 4,
                    )?;
                    self.asset_x += tile.w;
                    if self.asset_x == area.w {
                        self.asset_x = 0;
                        self.asset_y += tile.h;
                    }
                    if self.asset_y == area.h {
                        self.jobs.job_mut(slot).unwrap().asset_upload = Some(i);
                        self.asset_y = 0;
                        self.next_asset += 1;
                    }
                    return Ok(());
                }
                self.phase = Phase::Primitives;
            }
            Phase::Primitives => {
                let frame = recording.ok_or(Error::State)?;
                while self.next_primitive < frame.commands().len()
                    && self.reuse_layers[frame.commands()[self.next_primitive].layer as usize]
                {
                    self.next_primitive += 1;
                }
                if self.next_primitive == frame.commands().len() {
                    self.phase = Phase::LinearClear;
                    return Ok(());
                }
                let first = frame.commands()[self.next_primitive];
                let mut requests =
                    [g::R4GfxRenderRequest::default(); g::render_list_capacity as usize];
                let mut grids = [g::R4GfxLogicalGrid::default(); g::render_list_capacity as usize];
                let mut mapped = false;
                let mut count = 0;
                let limit = if self.batch_enabled {
                    requests.len()
                } else {
                    1
                };
                while count < limit && self.next_primitive + count < frame.commands().len() {
                    let command = frame.commands()[self.next_primitive + count];
                    if command.layer != first.layer
                        || command.texture != first.texture
                        || command.over != first.over
                    {
                        break;
                    }
                    grids[count] = logical_grid(command.grid);
                    mapped |= command.grid.enabled != 0;
                    requests[count] = g::R4GfxRenderRequest {
                        version: 1,
                        size: core::mem::size_of::<g::R4GfxRenderRequest>() as u32,
                        source: command.texture.map_or_else(g::R4GfxResource::default, |i| {
                            self.resources.assets[i as usize].resource
                        }),
                        target: self.resources.layers[command.layer as usize].resource,
                        pipeline: if command.texture.is_none() {
                            self.resources.fill
                        } else if command.over {
                            self.resources.over
                        } else {
                            self.resources.blit
                        },
                        sampler: if command.texture.is_none() {
                            Default::default()
                        } else {
                            self.resources.sampler
                        },
                        source_rect: rect(command.source),
                        target_rect: rect(command.target),
                        scissor: rect(command.scissor),
                        color: command.color,
                        opacity: 255,
                        transfer: 0,
                        dependency_count: if count == 0 {
                            dependencies.len() as u32
                        } else {
                            0
                        },
                        deadline_ns: self.deadline,
                        dependencies: if count == 0 { pointer } else { 0 },
                    };
                    count += 1;
                }
                let slot = self.jobs.reserve()?;
                let mut handle = g::R4GfxJob::default();
                let rc = if mapped {
                    if !self.grid_enabled {
                        return Err(Error::Unsupported);
                    }
                    let list = g::R4GfxRenderGridListRequest {
                        version: 1,
                        size: core::mem::size_of::<g::R4GfxRenderGridListRequest>() as u32,
                        commands: requests.as_ptr() as u64,
                        grids: grids.as_ptr() as u64,
                        count: count as u32,
                        reserved: 0,
                    };
                    unsafe {
                        s.client
                            .render_submit_grid_list(s.raw(), &list, &mut handle)
                    }
                } else if count == 1 {
                    unsafe { s.client.render_submit(s.raw(), &requests[0], &mut handle) }
                } else {
                    let list = g::R4GfxRenderListRequest {
                        version: 1,
                        size: core::mem::size_of::<g::R4GfxRenderListRequest>() as u32,
                        commands: requests.as_ptr() as u64,
                        count: count as u32,
                        reserved: 0,
                    };
                    unsafe { s.client.render_submit_list(s.raw(), &list, &mut handle) }
                };
                accepted(rc)?;
                self.jobs.track(s, slot, handle, None, 0, 0)?;
                self.next_primitive += count;
                self.render_jobs = self.render_jobs.saturating_add(1);
                self.primitive_jobs = self.primitive_jobs.saturating_add(1);
                self.primitive_draws = self.primitive_draws.saturating_add(count as u64);
                self.primitive_batch_peak = self.primitive_batch_peak.max(count);
            }
            Phase::Upload => {
                if self.jobs.stage.is_some() {
                    return Err(Error::Busy);
                }
                while self.next_image < cache.entries().len() {
                    let i = self.next_image;
                    let entry = &cache.entries()[i];
                    let image = &self.resources.layers[i];
                    if entry.external
                        || entry.frame != cache.frame()
                        || entry.generation == image.generation
                    {
                        self.next_image += 1;
                        continue;
                    }
                    if image.resource.slot == 0 {
                        return Err(Error::State);
                    }
                    let area = if image.generation == 0 {
                        entry.bounds
                    } else {
                        entry.dirty.unwrap_or(entry.bounds)
                    };
                    let slot = self.jobs.reserve()?;
                    self.stage_pixels(
                        s,
                        entry.pixels(),
                        entry.bounds.w as u32,
                        entry.bounds.h as u32,
                        entry.generation,
                        Rect::new(
                            area.x - entry.bounds.x,
                            area.y - entry.bounds.y,
                            area.w,
                            area.h,
                        ),
                    )?;
                    let image = &self.resources.layers[i];
                    let staging = &self.resources.staging;
                    let request = g::R4GfxCopyRequestEx {
                        version: 1,
                        size: core::mem::size_of::<g::R4GfxCopyRequestEx>() as u32,
                        copy: g::R4GfxCopyRequest {
                            source: staging.resource,
                            target: image.resource,
                            source_offset: 0,
                            target_offset: (area.y - entry.bounds.y) as u64
                                * image.info.image.pitch
                                + (area.x - entry.bounds.x) as u64 * 4,
                            byte_length: area.w as u64 * 4,
                            deadline_ns: self.deadline,
                        },
                        row_count: area.h as u32,
                        source_pitch: staging.info.image.pitch,
                        target_pitch: image.info.image.pitch,
                        dependency_count: dependencies.len() as u32,
                        dependencies: pointer,
                    };
                    let mut handle = g::R4GfxJob::default();
                    accepted(unsafe { s.client.copy_submit_ex(s.raw(), &request, &mut handle) })?;
                    self.jobs.track(
                        s,
                        slot,
                        handle,
                        Some(i),
                        entry.generation,
                        area.w as u64 * area.h as u64 * 4,
                    )?;
                    self.next_image += 1;
                    return Ok(());
                }
                self.phase = Phase::LinearClear;
            }
            Phase::LinearClear => {
                if self.clear_working && !self.replace_background {
                    let slot = self.jobs.reserve()?;
                    let request = g::R4GfxRenderRequest {
                        version: 1,
                        size: core::mem::size_of::<g::R4GfxRenderRequest>() as u32,
                        target: self.resources.workings[self.output_index].resource,
                        pipeline: self.resources.fill,
                        target_rect: rect(self.encode_area),
                        scissor: rect(self.encode_area),
                        opacity: 255,
                        dependency_count: dependencies.len() as u32,
                        dependencies: pointer,
                        deadline_ns: self.deadline,
                        ..Default::default()
                    };
                    let mut handle = g::R4GfxJob::default();
                    accepted(unsafe { s.client.render_submit(s.raw(), &request, &mut handle) })?;
                    self.jobs.track(s, slot, handle, None, 0, 0)?;
                    self.render_jobs = self.render_jobs.saturating_add(1);
                }
                self.phase = Phase::Draw;
            }
            Phase::Draw => {
                while self.next_command < cache.commands().len()
                    && intersect(
                        cache.commands()[self.next_command].scissor,
                        self.encode_area,
                    )
                    .is_none()
                {
                    self.next_command += 1;
                }
                if self.next_command == cache.commands().len() {
                    self.phase = Phase::Encode;
                    return Ok(());
                }
                let slot = self.jobs.reserve()?;
                let command = cache.commands()[self.next_command];
                let entry = &cache.entries()[command.entry as usize];
                let bounds = entry.bounds;
                let clip = intersect(command.scissor, self.encode_area).ok_or(Error::State)?;
                if let Some(source) = self.external_source(command.entry as usize) {
                    self.draw_external(s, cache, command.entry as usize, source, clip, slot)?;
                    self.next_command += 1;
                    self.render_jobs = self.render_jobs.saturating_add(1);
                    return Ok(());
                }
                if entry.external {
                    return Err(Error::State);
                }
                let mut handle = g::R4GfxJob::default();
                let request = g::R4GfxRenderRequest {
                    version: 1,
                    size: core::mem::size_of::<g::R4GfxRenderRequest>() as u32,
                    source: self.resources.layers[command.entry as usize].color_view,
                    target: self.resources.workings[self.output_index].resource,
                    pipeline: if self.replace_background && self.next_command == 0 {
                        self.resources.blit
                    } else {
                        self.resources.over
                    },
                    sampler: self.resources.sampler,
                    source_rect: g::R4GfxSignedRect {
                        x: clip.x - bounds.x,
                        y: clip.y - bounds.y,
                        width: clip.w as u32,
                        height: clip.h as u32,
                    },
                    target_rect: rect(clip),
                    scissor: rect(clip),
                    color: 0,
                    opacity: 255,
                    transfer: if self.hdr_frame {
                        g::render_transfer_identity
                    } else {
                        g::render_transfer_srgb_decode
                    },
                    dependency_count: dependencies.len() as u32,
                    deadline_ns: self.deadline,
                    dependencies: pointer,
                };
                let rc = if self.hdr_frame {
                    let list = g::R4GfxRenderListRequest {
                        version: 1,
                        size: core::mem::size_of::<g::R4GfxRenderListRequest>() as u32,
                        commands: &request as *const _ as u64,
                        count: 1,
                        reserved: 0,
                    };
                    unsafe {
                        s.colors.color_render_submit(
                            s.raw(),
                            &list,
                            g::color_transform_relative_white,
                            &mut handle,
                        )
                    }
                } else {
                    unsafe { s.client.render_submit(s.raw(), &request, &mut handle) }
                };
                accepted(rc)?;
                self.jobs.track(s, slot, handle, None, 0, 0)?;
                self.next_command += 1;
                self.render_jobs = self.render_jobs.saturating_add(1);
            }
            Phase::Encode => {
                let slot = self.jobs.reserve()?;
                let area = rect(self.encode_area);
                let request = g::R4GfxRenderRequest {
                    version: 1,
                    size: core::mem::size_of::<g::R4GfxRenderRequest>() as u32,
                    source: self.resources.workings[self.output_index].resource,
                    target: self.resources.outputs[self.output_index].resource,
                    pipeline: self.resources.blit,
                    sampler: self.resources.sampler,
                    source_rect: area,
                    target_rect: area,
                    scissor: area,
                    color: 0,
                    opacity: 255,
                    transfer: if self.color_output || self.hdr_frame {
                        g::render_transfer_identity
                    } else {
                        g::render_transfer_srgb_encode
                    },
                    dependency_count: dependencies.len() as u32,
                    deadline_ns: self.deadline,
                    dependencies: pointer,
                };
                let mut handle = g::R4GfxJob::default();
                let rc = if self.color_output || self.hdr_frame {
                    let list = g::R4GfxRenderListRequest {
                        version: 1,
                        size: core::mem::size_of::<g::R4GfxRenderListRequest>() as u32,
                        commands: &request as *const _ as u64,
                        count: 1,
                        reserved: 0,
                    };
                    unsafe {
                        s.colors.color_render_submit(
                            s.raw(),
                            &list,
                            g::color_transform_output
                                | g::color_transform_dither
                                | if self.hdr_frame {
                                    0
                                } else {
                                    g::color_transform_relative_white
                                },
                            &mut handle,
                        )
                    }
                } else {
                    unsafe { s.client.render_submit(s.raw(), &request, &mut handle) }
                };
                accepted(rc)?;
                self.jobs.track(s, slot, handle, None, 0, 0)?;
                self.render_jobs = self.render_jobs.saturating_add(1);
                self.phase = if self.destination == Destination::Display {
                    Phase::Present
                } else {
                    Phase::Drain
                };
            }
            Phase::Present => {
                if self.chain.slot != 0 {
                    let acquired = self.acquired.ok_or(Error::State)?;
                    let request = g::R4GfxSwapchainPresent {
                        version: 1,
                        size: core::mem::size_of::<g::R4GfxSwapchainPresent>() as u32,
                        frame: acquired,
                        render_job: self
                            .jobs
                            .last
                            .and_then(|i| self.jobs.job(i))
                            .map_or_else(g::R4GfxJob::default, |j| j.handle),
                        deadline_ns: self.deadline,
                        intent: self.present_intent,
                        blockers: self.present_blockers,
                    };
                    accepted(unsafe {
                        s.client.swapchain_present(s.raw(), &self.chain, &request)
                    })?;
                    self.acquired = None;
                    self.phase = Phase::Drain;
                    return Ok(());
                }
                let slot = self.jobs.reserve()?;
                let request = g::R4GfxImagePresentRequest {
                    version: 1,
                    size: core::mem::size_of::<g::R4GfxImagePresentRequest>() as u32,
                    source: self.resources.outputs[self.output_index].resource,
                    frame_key: self.frame,
                    deadline_ns: self.deadline,
                    dependency_count: dependencies.len() as u32,
                    dependencies: pointer,
                    reserved: 0,
                };
                let mut handle = g::R4GfxJob::default();
                accepted(unsafe { s.client.image_present(s.raw(), &request, &mut handle) })?;
                self.jobs.track(s, slot, handle, None, 0, 0)?;
                self.present_fence = Some(self.jobs.job(slot).unwrap().fence);
                self.phase = Phase::Drain;
            }
            _ => return Err(Error::State),
        }
        Ok(())
    }
    fn draw_external(
        &mut self,
        s: &Session<'_, '_>,
        cache: &Cache,
        index: usize,
        source: External,
        clip: Rect,
        slot: usize,
    ) -> Result<()> {
        let entry = &cache.entries()[index];
        let view = cache.view().unwrap_or(geometry::topology::Viewport {
            pixel_w: cache.screen().w as u32,
            pixel_h: cache.screen().h as u32,
            ..Default::default()
        });
        let bounds = entry.capture_bounds;
        let grid = g::R4GfxLogicalGrid {
            enabled: 1,
            rotation: view.rotation as u32,
            scale: view.scale,
            pixel_width: view.pixel_w,
            pixel_height: view.pixel_h,
            target_x: 0,
            target_y: 0,
            reserved: 0,
            viewport_x: i32::try_from(bounds.x as i64 - view.origin.x as i64)
                .map_err(|_| Error::Limit)?,
            viewport_y: i32::try_from(bounds.y as i64 - view.origin.y as i64)
                .map_err(|_| Error::Limit)?,
            viewport_width: bounds.w as u32,
            viewport_height: bounds.h as u32,
            guest_width: source.descriptor.width,
            guest_height: source.descriptor.height,
            source_x: 0,
            source_y: 0,
        };
        let mut dependencies = [g::R4GfxCopyFence::default(); 2];
        let mut count = 0;
        if let Some(fence) = self.jobs.dependency()?.first() {
            dependencies[0] = *fence;
            count = 1;
        }
        let producer = g::R4GfxCopyFence {
            slot: source.ready.slot,
            adapter_id: source.ready.adapter_id,
            timeline: source.ready.timeline,
            point: source.ready.point,
            device_generation: source.ready.device_generation,
            reset_generation: source.ready.reset_generation,
        };
        if count == 0 || !same_fence(dependencies[0], producer) {
            dependencies[count] = producer;
            count += 1;
        }
        let request = g::R4GfxRenderRequest {
            version: 1,
            size: core::mem::size_of::<g::R4GfxRenderRequest>() as u32,
            source: self.resources.layers[index].resource,
            target: self.resources.workings[self.output_index].resource,
            pipeline: self.resources.over,
            sampler: self.resources.sampler,
            source_rect: g::R4GfxSignedRect {
                x: 0,
                y: 0,
                width: source.descriptor.width,
                height: source.descriptor.height,
            },
            target_rect: rect(entry.bounds),
            scissor: rect(clip),
            color: 0,
            opacity: 255,
            transfer: g::render_transfer_identity,
            dependency_count: count as u32,
            dependencies: dependencies.as_ptr() as u64,
            deadline_ns: self.deadline,
        };
        let list = g::R4GfxRenderGridListRequest {
            version: 1,
            size: core::mem::size_of::<g::R4GfxRenderGridListRequest>() as u32,
            commands: &request as *const _ as u64,
            grids: &grid as *const _ as u64,
            count: 1,
            reserved: 0,
        };
        let mut handle = g::R4GfxJob::default();
        accepted(unsafe {
            s.colors.color_render_submit_grid(
                s.raw(),
                &list,
                if color::absolute(source.color) {
                    0
                } else {
                    g::color_transform_relative_white
                },
                &mut handle,
            )
        })?;
        // Track before metadata queries, then mark this exact external tail.
        let tracked = self.jobs.track(s, slot, handle, None, 0, 0);
        self.jobs.mark_external(index, slot)?;
        tracked
    }
    fn stage_pixels(
        &mut self,
        s: &Session<'_, '_>,
        pixels: &[u32],
        width: u32,
        height: u32,
        generation: u64,
        area: Rect,
    ) -> Result<()> {
        self.release_stage_source(s)?;
        let mut d = descriptor(g::resource_image);
        d.source_kind = g::source_borrow_cpu;
        d.source_generation = generation;
        d.image = g::R4GfxCpuImage {
            cpu_address: pixels.as_ptr() as u64,
            byte_length: pixels.len() as u64 * 4,
            pitch: width as u64 * 4,
            width,
            height,
            format: g::format_argb8888,
            reserved: 0,
        };
        accepted(unsafe {
            s.client
                .resource_create(s.raw(), &d, &mut self.stage_source)
        })?;
        let draw = g::R4GfxDraw {
            source: self.stage_source,
            target: self.resources.staging.resource,
            pipeline: self.resources.blit,
            sampler: self.resources.sampler,
            source_rect: g::R4GfxRect {
                x: area.x as u32,
                y: area.y as u32,
                width: area.w as u32,
                height: area.h as u32,
            },
            target_rect: g::R4GfxRect {
                x: 0,
                y: 0,
                width: area.w as u32,
                height: area.h as u32,
            },
            color: 0,
            opacity: 255,
        };
        let pixels = area.w as u64 * area.h as u64;
        let result = if pixels > g::render_max_pixels {
            Err(Error::Limit)
        } else {
            let request = g::R4GfxRenderBatch {
                commands: &draw as *const _ as u64,
                command_count: 1,
                flags: 0,
                pixel_budget: pixels,
            };
            let mut stats = g::R4GfxRenderStats::default();
            accepted(unsafe { s.client.render(s.raw(), &request, &mut stats) })
        };
        let released = self.release_stage_source(s);
        result.and(released)
    }
    pub(super) fn release_stage_source(&mut self, s: &Session<'_, '_>) -> Result<()> {
        if self.stage_source.slot != 0 {
            accepted(unsafe { s.client.resource_release(s.raw(), &self.stage_source) })?;
            self.stage_source = Default::default();
        }
        Ok(())
    }
}
fn logical_grid(v: a::GfxSampleGrid) -> g::R4GfxLogicalGrid {
    g::R4GfxLogicalGrid {
        enabled: v.enabled,
        rotation: v.rotation,
        scale: v.scale,
        pixel_width: v.pixel_width,
        pixel_height: v.pixel_height,
        target_x: v.target_x,
        target_y: v.target_y,
        reserved: v.reserved,
        viewport_x: v.viewport_x,
        viewport_y: v.viewport_y,
        viewport_width: v.viewport_width,
        viewport_height: v.viewport_height,
        guest_width: v.guest_width,
        guest_height: v.guest_height,
        source_x: v.source_x,
        source_y: v.source_y,
    }
}
fn same_fence(x: g::R4GfxCopyFence, y: g::R4GfxCopyFence) -> bool {
    x.slot == y.slot
        && x.adapter_id == y.adapter_id
        && x.timeline == y.timeline
        && x.point == y.point
        && x.device_generation == y.device_generation
        && x.reset_generation == y.reset_generation
}
