//! Cold preparation and reuse are methods of the single native transaction.
//! Jobs/scanout/borrowed consumer receipts are drained by its coordinator.
use super::*;
impl Engine {
    pub fn prepared(
        &mut self,
        s: &Session<'_, '_>,
        cache: &Cache,
        recording: Option<&Frame>,
    ) -> bool {
        if self.chain_status.is_some_and(|status| status.life >= 2) {
            return false;
        }
        let resources = &mut self.resources;
        let screen = cache.screen();
        if resources.over.slot == 0
            || resources.blit.slot == 0
            || resources.fill.slot == 0
            || resources.sampler.slot == 0
            || !resources.workings[self.output_index].fits(
                s,
                screen.w,
                screen.h,
                g::format_abgr16161616f,
            )
            || !resources.outputs[self.output_index].fits(s, screen.w, screen.h, self.output_format)
            || !resources.staging.fits(
                s,
                if recording.is_some() { 512 } else { screen.w },
                if recording.is_some() { 512 } else { screen.h },
                g::format_argb8888,
            )
        {
            return false;
        }
        if self.has_hdr(cache) != self.hdr_frame {
            return false;
        }
        for (i, image) in self.resources.layers.iter().enumerate() {
            if image.resource.slot != 0
                && (cache.complete_scene || image.external_reference.id != 0)
                && cache.entries()[i].frame != cache.frame()
            {
                return false;
            }
        }
        let mut visited = [false; layers::CAPACITY];
        for command in cache.commands() {
            let i = command.entry as usize;
            if visited[i] {
                continue;
            }
            visited[i] = true;
            let entry = &cache.entries()[i];
            if let Some(source) = self.external_source(i) {
                let image = &mut self.resources.layers[i];
                if !same_external(image, source)
                    || !image.fits(
                        s,
                        source.descriptor.width as i32,
                        source.descriptor.height as i32,
                        source.descriptor.format,
                    )
                {
                    return false;
                }
                continue;
            }
            if entry.external {
                return false;
            }
            if !self.resources.layers[i].fits(s, entry.bounds.w, entry.bounds.h, g::format_argb8888)
                || self.resources.layers[i].color_view.slot == 0
            {
                return false;
            }
        }
        if let Some(frame) = recording {
            if self.snapshot.capacity() < frame.commands().len() {
                return false;
            }
            for i in 0..primitive_assets::TEXTURE_CAPACITY {
                let texture = frame.assets.texture(i);
                if texture.pinned == cache.frame()
                    && !self.resources.assets[i].fits(
                        s,
                        texture.width as i32,
                        texture.height as i32,
                        g::format_argb8888,
                    )
                {
                    return false;
                }
            }
        }
        true
    }
    pub fn prepare(
        &mut self,
        s: &Session<'_, '_>,
        cache: &Cache,
        recording: Option<&Frame>,
        deadline: u64,
        mut clock: impl FnMut() -> u64,
        cancelled: impl Fn() -> bool,
    ) -> Result<()> {
        let mut check = || {
            if cancelled() {
                Err(Error::State)
            } else if clock() >= deadline {
                Err(Error::Deadline)
            } else {
                Ok(())
            }
        };
        check()?;
        if self.active() || !self.jobs.drained() {
            return Err(Error::Busy);
        }
        if !cache.captured() || cache.commands().is_empty() {
            return Err(Error::State);
        }
        let info = s.refresh().map_err(|_| Error::Graphics)?;
        let required = self.required_operations();
        if info.gpu_operations & required != required {
            return Err(Error::Unsupported);
        }
        self.batch_enabled = info.gpu_operations & g::device_gpu_render_list != 0;
        self.grid_enabled = info.gpu_operations & g::device_gpu_grid != 0;
        for (i, image) in self.resources.layers.iter_mut().enumerate() {
            if image.resource.slot != 0
                && (cache.complete_scene || image.external_reference.id != 0)
                && cache.entries()[i].frame != cache.frame()
            {
                image.release(s, &mut self.resources.charged)?;
            }
        }
        if recording.is_some_and(|frame| frame.view().is_some()) && !self.grid_enabled {
            return Err(Error::Unsupported);
        }
        check()?;
        self.resources.prepare_state(s, &mut check)?;
        check()?;
        let screen = cache.screen();
        let mut presentation = None;
        for head in 0..if self.destination == Destination::Display {
            8
        } else {
            0
        } {
            if self.head.is_some_and(|selected| selected != head) {
                continue;
            }
            let mut value = g::R4GfxPresentationInfo::default();
            let rc = unsafe { s.client.presentation_info(s.raw(), head, &mut value) };
            if rc == g::status_busy {
                return Err(Error::Busy);
            }
            if rc != g::status_ok
                || value.flags & g::present_native == 0
                || value.adapter_id != info.adapter_id
                || value.device_generation != info.device_generation
                || value.reset_generation != info.reset_generation
            {
                continue;
            }
            if value.width != screen.w as u32
                || value.height != screen.h as u32
                || value.format != self.output_format
            {
                if self.head.is_some() {
                    return Err(Error::Busy);
                }
                continue;
            }
            presentation = Some(value);
            break;
        }
        if (self.head.is_some() || self.color_output) && presentation.is_none() {
            return Err(Error::Stale);
        }
        if self.chain.slot != 0
            && (presentation.is_none()
                || self.presentation.is_none_or(|old| {
                    presentation.is_some_and(|new| {
                        old.display_generation != new.display_generation
                            || old.width != new.width
                            || old.height != new.height
                    })
                }))
        {
            self.close_chain(s)?;
        }
        let count = presentation.map_or(1, |v| v.buffer_count) as usize;
        if count == 0 || count > self.resources.outputs.len() {
            return Err(Error::Unsupported);
        }
        let direct = presentation.is_some_and(|v| v.flags & g::present_direct != 0)
            && info.gpu_operations & g::device_gpu_direct != 0;
        for image in &mut self.resources.outputs[..count] {
            check()?;
            match image.prepare(
                s,
                &mut self.resources.charged,
                self.resources.budget,
                screen.w,
                screen.h,
                self.output_format,
                true,
                direct,
                deadline,
                self.output_color,
            ) {
                Err(Error::Unsupported | Error::Limit) if direct => image.prepare(
                    s,
                    &mut self.resources.charged,
                    self.resources.budget,
                    screen.w,
                    screen.h,
                    self.output_format,
                    true,
                    false,
                    deadline,
                    self.output_color,
                )?,
                value => value?,
            }
        }
        let hdr = self.has_hdr(cache);
        if hdr != self.hdr_frame {
            for image in &mut self.resources.workings {
                image.release(s, &mut self.resources.charged)?;
            }
            self.hdr_frame = hdr;
            self.invalidate();
        }
        let working = if self.hdr_frame {
            color::working(self.output_color)
        } else {
            software::description(true, false)
        };
        for image in &mut self.resources.workings[..count] {
            check()?;
            image.prepare(
                s,
                &mut self.resources.charged,
                self.resources.budget,
                screen.w,
                screen.h,
                g::format_abgr16161616f,
                true,
                false,
                deadline,
                working,
            )?;
        }
        if let Some(value) = presentation {
            if self.chain.slot == 0 {
                check()?;
                let handles = [
                    self.resources.outputs[0].resource,
                    self.resources.outputs[1].resource,
                    self.resources.outputs[2].resource,
                ];
                let desc = g::R4GfxSwapchainDesc {
                    version: 1,
                    size: core::mem::size_of::<g::R4GfxSwapchainDesc>() as u32,
                    head_id: value.head_id,
                    policy: g::present_policy_fifo,
                    flags: g::present_require_vsync,
                    count: count as u32,
                    display_generation: value.display_generation,
                    images: handles.as_ptr() as u64,
                };
                accepted(unsafe { s.client.swapchain_open(s.raw(), &desc, &mut self.chain) })?;
                self.presentation = Some(value);
            }
        }
        check()?;
        self.resources.staging.prepare(
            s,
            &mut self.resources.charged,
            self.resources.budget,
            if recording.is_some() { 512 } else { screen.w },
            if recording.is_some() { 512 } else { screen.h },
            g::format_argb8888,
            false,
            false,
            deadline,
            software::description(false, false),
        )?;
        let mut visited = [false; layers::CAPACITY];
        for command in cache.commands() {
            let i = command.entry as usize;
            if visited[i] {
                continue;
            }
            visited[i] = true;
            let entry = &cache.entries()[i];
            check()?;
            if !entry.initialized || entry.generation == 0 {
                return Err(Error::State);
            }
            if let Some(source) = self.external_source(i) {
                let required = g::device_gpu_color_grid
                    | g::device_gpu_color
                    | g::device_gpu_grid
                    | g::device_gpu_render_list;
                if info.gpu_operations & required != required {
                    return Err(Error::Unsupported);
                }
                self.resources.layers[i].external(
                    s,
                    &mut self.resources.charged,
                    self.resources.budget,
                    source.reference,
                    source.descriptor,
                    source.color,
                )?;
                continue;
            }
            if entry.external {
                return Err(Error::State);
            }
            self.resources.layers[i].prepare(
                s,
                &mut self.resources.charged,
                self.resources.budget,
                entry.bounds.w,
                entry.bounds.h,
                g::format_argb8888,
                true,
                false,
                deadline,
                software::description(false, false),
            )?;
            check()?;
            self.resources.layers[i].color_view(s)?;
        }
        if let Some(frame) = recording {
            check()?;
            if frame.commands().len() > crate::render::primitive_frame::CAPACITY {
                return Err(Error::Limit);
            }
            if self.snapshot.capacity() < frame.commands().len() {
                let mut replacement = Vec::new();
                replacement
                    .try_reserve_exact(frame.commands().len())
                    .map_err(|_| Error::Limit)?;
                self.snapshot = replacement;
            }
            for i in 0..primitive_assets::TEXTURE_CAPACITY {
                let texture = frame.assets.texture(i);
                if texture.pinned != cache.frame() {
                    continue;
                }
                check()?;
                self.resources.assets[i].prepare(
                    s,
                    &mut self.resources.charged,
                    self.resources.budget,
                    texture.width as i32,
                    texture.height as i32,
                    g::format_argb8888,
                    true,
                    false,
                    deadline,
                    software::description(false, false),
                )?;
            }
        }
        check()
    }
    pub(super) fn plan_reuse(&mut self, cache: &Cache, recording: Option<&Frame>) {
        self.reuse_layers = [false; layers::CAPACITY];
        let Some(frame) = recording else {
            return;
        };
        if frame.mirror() || self.snapshot.is_empty() {
            return;
        }
        for (i, entry) in cache.entries().iter().enumerate() {
            let image = &self.resources.layers[i];
            if entry.frame != cache.frame()
                || entry.external
                || image.generation == 0
                || image.resource.slot == 0
                || !same_resource(image.resource, self.snapshot_images[i])
            {
                continue;
            }
            let mut previous = self.snapshot.iter().filter(|c| c.layer as usize == i);
            let mut matched = 0;
            let mut identical = true;
            for command in frame.commands().iter().filter(|c| c.layer as usize == i) {
                let Some(old) = previous.next() else {
                    identical = false;
                    break;
                };
                if !same_command(command, old) {
                    identical = false;
                    break;
                }
                if let Some(texture) = command.texture {
                    let generation = frame.assets.texture(texture as usize).generation;
                    if generation == 0
                        || generation != self.snapshot_assets[texture as usize]
                        || generation != self.resources.assets[texture as usize].generation
                    {
                        identical = false;
                        break;
                    }
                }
                matched += 1;
            }
            self.reuse_layers[i] = identical && matched != 0 && previous.next().is_none();
        }
    }
    pub fn reuse_capture(
        &mut self,
        cache: &Cache,
        frame: &mut Frame,
        damage: Option<Rect>,
    ) -> Result<()> {
        self.reuse_layers = [false; layers::CAPACITY];
        let Some(changed) = damage else {
            return Ok(());
        };
        let Some(view) = cache.view() else {
            return Ok(());
        };
        if !cache.collecting()
            || !cache.commands().is_empty()
            || frame.mirror()
            || self.active()
            || self.fault.is_some()
            || self.snapshot.is_empty()
            || self.frame != cache.frame() - 1
            || self.capture_view != Some(view)
        {
            return Ok(());
        }
        for (i, entry) in cache.entries().iter().enumerate() {
            let image = &self.resources.layers[i];
            self.reuse_layers[i] = entry.initialized
                && !entry.external
                && entry.frame == self.frame
                && intersect(entry.capture_bounds, changed).is_none()
                && image.generation == entry.generation
                && image.resource.slot != 0
                && same_resource(image.resource, self.snapshot_images[i]);
        }
        for command in &self.snapshot {
            if let Some(texture) = command.texture {
                let generation = frame.assets.texture(texture as usize).generation;
                if generation == 0 || generation != self.snapshot_assets[texture as usize] {
                    self.reuse_layers[command.layer as usize] = false;
                }
            }
        }
        for command in &self.snapshot {
            if self.reuse_layers[command.layer as usize] {
                if let Some(texture) = command.texture {
                    frame
                        .assets
                        .reuse_texture(texture as usize)
                        .map_err(|_| Error::State)?;
                }
            }
        }
        Ok(())
    }
    pub fn replay(&self) -> layers::Replay<'_> {
        layers::Replay {
            commands: &self.snapshot,
            reusable: &self.reuse_layers,
        }
    }
    pub(super) fn remember(&mut self, cache: &Cache, recording: Option<&Frame>) {
        self.snapshot.clear();
        let Some(frame) = recording else {
            return;
        };
        if frame.mirror() {
            return;
        }
        self.snapshot.extend_from_slice(frame.commands());
        for (i, entry) in cache.entries().iter().enumerate() {
            self.snapshot_images[i] = if entry.frame == cache.frame() && !entry.external {
                self.resources.layers[i].resource
            } else {
                Default::default()
            };
        }
        for i in 0..primitive_assets::TEXTURE_CAPACITY {
            self.snapshot_assets[i] = self.resources.assets[i].generation;
        }
    }
}
fn same_external(image: &Image, source: External) -> bool {
    image.external_reference.id == source.reference.reference.id
        && image.external_reference.generation == source.reference.reference.generation
        && image.external_reference.reserved0 == source.reference.reference.reserved0
        && image
            .external_color
            .is_some_and(|v| color::same_description(v, source.color))
}
fn same_command(x: &Command, y: &Command) -> bool {
    let grid = |v: a::GfxSampleGrid| {
        [
            v.enabled as i64,
            v.rotation as i64,
            v.scale as i64,
            v.pixel_width as i64,
            v.pixel_height as i64,
            v.target_x as i64,
            v.target_y as i64,
            v.reserved as i64,
            v.viewport_x as i64,
            v.viewport_y as i64,
            v.viewport_width as i64,
            v.viewport_height as i64,
            v.guest_width as i64,
            v.guest_height as i64,
            v.source_x as i64,
            v.source_y as i64,
        ]
    };
    x.layer == y.layer
        && x.texture == y.texture
        && x.source == y.source
        && x.target == y.target
        && x.scissor == y.scissor
        && x.color == y.color
        && x.over == y.over
        && grid(x.grid) == grid(y.grid)
}
