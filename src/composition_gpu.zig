//! Desktop GPU resources and a bounded, nonblocking composition transaction.
//! prepare() may allocate and belongs on the preparation thread. advance()
//! only stages changed rectangles and submits/polls ordinary R4GFX jobs.
const std = @import("std");
const r4os = @import("r4os");
const gfx = @import("r4gfx");
const layers = @import("composition_layers.zig");
const surface = @import("surface.zig");
const empty = std.mem.zeroes(gfx.R4GfxResource);
const Image = struct { resource: gfx.R4GfxResource = empty, info: gfx.R4GfxResourceInfo = undefined, generation: u64 = 0, charge: u64 = 0 };
const Job = struct { handle: gfx.R4GfxJob, fence: gfx.R4GfxCopyFence, upload: ?usize = null, generation: u64 = 0, bytes: u64 = 0, complete: bool = false };
pub const Error = error{ Busy, Unsupported, Stale, Graphics, Limit, State, Deadline, Incomplete };
pub const Progress = enum { pending, copied, failed };
pub const Engine = struct {
    client: *const gfx.DeviceV1Client,
    device: *const gfx.R4GfxDevice,
    images: [layers.capacity]Image = @splat(.{}),
    output: Image = .{},
    staging: Image = .{},
    over: gfx.R4GfxResource = empty,
    blit: gfx.R4GfxResource = empty,
    sampler: gfx.R4GfxResource = empty,
    jobs: [gfx.device_job_capacity]?Job = @splat(null),
    last: ?usize = null,
    stage_job: ?usize = null,
    phase: enum { idle, upload, draw, present, drain } = .idle,
    frame: u64 = 0,
    next_image: usize = 0,
    next_command: usize = 0,
    deadline: u64 = 0,
    fault: ?Error = null,
    present_fence: ?gfx.R4GfxCopyFence = null,
    uploaded_bytes: u64 = 0,
    render_jobs: u64 = 0,
    reserved_bytes: u64 = 0,
    budget_bytes: u64 = 256 * 1024 * 1024,

    pub fn init(client: *const gfx.DeviceV1Client, device: *const gfx.R4GfxDevice) Engine { return .{ .client = client, .device = device }; }
    pub fn active(self: *const Engine) bool { return self.phase != .idle; }
    pub fn needsFull(self: *const Engine, width: i32, height: i32) bool {
        return self.output.resource.slot == 0 or self.output.generation == 0 or
            self.output.info.image.width != width or self.output.info.image.height != height;
    }
    pub fn prepared(self: *Engine, cache: *const layers.Cache) bool {
        if (self.over.slot == 0 or self.blit.slot == 0 or self.sampler.slot == 0 or
            !self.imageFits(&self.output, cache.screen.w, cache.screen.h, gfx.format_xrgb8888) or
            !self.imageFits(&self.staging, cache.screen.w, cache.screen.h, gfx.format_argb8888)) return false;
        var visited: [layers.capacity]bool = @splat(false);
        for (cache.commands[0..cache.command_count]) |command| {
            if (visited[command.entry]) continue;
            visited[command.entry] = true;
            const entry = &cache.entries[command.entry];
            if (!self.imageFits(&self.images[command.entry], entry.bounds.w, entry.bounds.h, gfx.format_argb8888)) return false;
        }
        return true;
    }
    fn imageFits(self: *Engine, target: *Image, width: i32, height: i32, format: u32) bool {
        if (target.resource.slot == 0) return false;
        var info: gfx.R4GfxResourceInfo = undefined;
        if (self.client.resource_info(self.device, &target.resource, &info) != gfx.status_ok or
            info.flags & gfx.resource_invalidated != 0 or info.image.width != width or
            info.image.height != height or info.image.format != format) return false;
        target.info = info;
        return true;
    }
    pub fn prepare(self: *Engine, cache: *const layers.Cache, deadline: u64) Error!void {
        if (self.active() or !self.drained()) return error.Busy;
        if (cache.collecting or cache.failure != null or cache.command_count == 0) return error.State;
        var device_info: gfx.R4GfxDeviceInfo = undefined;
        try accepted(self.client.device_refresh(self.device, &device_info));
        const operations = gfx.device_gpu_render | gfx.device_gpu_present | gfx.device_gpu_copy_rows;
        if (device_info.gpu_operations & operations != operations) return error.Unsupported;
        try self.stateResource(&self.over, gfx.resource_pipeline, gfx.render_operation_over);
        try self.stateResource(&self.blit, gfx.resource_pipeline, gfx.render_operation_blit);
        try self.stateResource(&self.sampler, gfx.resource_sampler, gfx.render_sampler_nearest);
        try self.image(&self.output, cache.screen.w, cache.screen.h, gfx.format_xrgb8888, true, deadline);
        try self.image(&self.staging, cache.screen.w, cache.screen.h, gfx.format_argb8888, false, deadline);
        var visited: [layers.capacity]bool = @splat(false);
        for (cache.commands[0..cache.command_count]) |command| {
            const index = command.entry;
            if (visited[index]) continue;
            visited[index] = true;
            const entry = &cache.entries[index];
            if (!entry.initialized or entry.generation == 0) return error.State;
            try self.image(&self.images[index], entry.bounds.w, entry.bounds.h, gfx.format_argb8888, true, deadline);
        }
    }
    fn stateResource(self: *Engine, handle: *gfx.R4GfxResource, kind: u32, operation: u32) Error!void {
        if (handle.slot != 0) return;
        var desc = descriptor(kind);
        if (kind == gfx.resource_sampler) desc.sampler = operation else desc.operation = operation;
        try accepted(self.client.resource_create(self.device, &desc, handle));
    }
    fn image(self: *Engine, target: *Image, width: i32, height: i32, format: u32, native: bool, deadline: u64) Error!void {
        if (width <= 0 or height <= 0) return error.State;
        if (target.resource.slot != 0) {
            if (self.imageFits(target, width, height, format)) return;
            try self.releaseImage(target);
        }
        const row: u64 = @as(u64, @intCast(width)) * 4;
        const pitch = if (native) (row + 255) & ~@as(u64, 255) else row;
        const size = pitch * @as(u64, @intCast(height));
        const charge = (size + (if (native) @as(u64, 65535) else 4095)) & ~(if (native) @as(u64, 65535) else 4095);
        if (charge > self.budget_bytes or self.reserved_bytes > self.budget_bytes - charge) return error.Limit;
        var desc = descriptor(gfx.resource_image); desc.flags = gfx.image_target;
        const request: gfx.R4GfxNativeImage = .{ .version = 1, .size = @sizeOf(gfx.R4GfxNativeImage), .deadline_ns = deadline,
            .width = @intCast(width), .height = @intCast(height), .format = format, .layout = 0 };
        if (native) { desc.source_kind = gfx.source_create_native; desc.source_address = @intFromPtr(&request); } else {
            desc.source_kind = gfx.source_create_system;
            desc.image = .{ .cpu_address = 0, .byte_length = size, .pitch = pitch, .width = @intCast(width), .height = @intCast(height), .format = format, .reserved = 0 };
        }
        try accepted(self.client.resource_create(self.device, &desc, &target.resource));
        // The resource remains tracked even if its metadata cannot be read.
        target.info = std.mem.zeroes(gfx.R4GfxResourceInfo);
        target.charge = charge; self.reserved_bytes += charge;
        try accepted(self.client.resource_info(self.device, &target.resource, &target.info));
        if (target.info.image.byte_length > charge) {
            self.reserved_bytes += target.info.image.byte_length - charge;
            target.charge = target.info.image.byte_length;
            try self.releaseImage(target);
            return error.Limit;
        }
        target.generation = 0;
    }
    fn releaseImage(self: *Engine, image_value: *Image) Error!void {
        if (image_value.resource.slot == 0) return;
        try accepted(self.client.resource_release(self.device, &image_value.resource));
        self.reserved_bytes -= image_value.charge;
        image_value.* = .{};
    }
    pub fn begin(self: *Engine, cache: *const layers.Cache, deadline: u64) Error!void {
        if (self.active() or !self.drained()) return error.Busy;
        if (cache.collecting or cache.failure != null or cache.command_count == 0 or self.output.resource.slot == 0 or self.staging.resource.slot == 0)
            return error.State;
        // The first command of a complete desktop capture is its opaque
        // background (or fullscreen terminal). A new target must start there.
        if (self.output.generation == 0 and !std.meta.eql(cache.commands[0].scissor, cache.screen)) return error.Incomplete;
        self.frame = cache.frame; self.deadline = deadline; self.phase = .upload;
        self.next_image = 0; self.next_command = 0; self.fault = null; self.present_fence = null;
        self.last = null; self.stage_job = null;
    }
    pub fn cancel(self: *Engine, reason: Error) void {
        if (self.fault == null) self.fault = reason;
        self.phase = .drain;
        self.output.generation = 0;
        for (&self.jobs) |*slot| if (slot.*) |*job| { _ = self.client.job_cancel(self.device, &job.handle); };
    }
    pub fn advance(self: *Engine, cache: *layers.Cache, now: u64) Progress {
        if (!self.active()) return if (self.fault == null) .copied else .failed;
        if (cache.frame != self.frame or cache.collecting) self.cancel(error.State);
        if (now >= self.deadline and self.fault == null) self.cancel(error.Deadline);
        self.collect(cache) catch |err| self.cancel(err);
        if (self.phase == .drain) {
            if (!self.drained()) return .pending;
            self.phase = .idle;
            if (self.fault == null) self.output.generation = self.frame;
            return if (self.fault == null) .copied else .failed;
        }
        // Admission is bounded independently of scene size. Returning Busy
        // leaves this exact operation for a subsequent desktop event cycle.
        for (0..4) |_| {
            self.step(cache) catch |err| {
                if (err != error.Busy) self.cancel(err);
                return .pending;
            };
            if (self.phase == .drain) break;
        }
        return .pending;
    }
    fn collect(self: *Engine, cache: *layers.Cache) Error!void {
        for (&self.jobs, 0..) |*slot, index| if (slot.*) |*job| {
            var info: gfx.R4GfxJobInfo = undefined;
            try accepted(self.client.job_info(self.device, &job.handle, &info));
            if (info.phase != r4os.abi.gfx_queue_phase_terminal or info.flags != 0) continue;
            if (info.result != r4os.abi.gfx_queue_result_complete and self.fault == null) self.cancel(error.Graphics);
            if (!job.complete) {
                if (job.upload) |image_index| if (info.result == r4os.abi.gfx_queue_result_complete) {
                    self.images[image_index].generation = job.generation;
                    cache.uploaded(image_index, job.generation);
                    self.uploaded_bytes +|= job.bytes;
                };
                job.complete = true;
            }
            if (self.stage_job == index) self.stage_job = null;
            // Keep the most recent receipt until the next job has copied its
            // explicit dependency; a failed predecessor must veto Present.
            if (self.last == index and self.phase != .drain) continue;
            try accepted(self.client.job_release(self.device, &job.handle));
            if (self.last == index) self.last = null;
            slot.* = null;
        };
    }
    fn drained(self: *const Engine) bool { for (&self.jobs) |*job| if (job.* != null) return false; return true; }
    fn reserve(self: *Engine) Error!usize { for (&self.jobs, 0..) |*job, i| if (job.* == null) return i; return error.Busy; }
    fn dependency(self: *const Engine) []const gfx.R4GfxCopyFence {
        return if (self.last) |index| @as([*]const gfx.R4GfxCopyFence, @ptrCast(&self.jobs[index].?.fence))[0..1] else &.{};
    }
    fn track(self: *Engine, index: usize, handle: gfx.R4GfxJob, upload: ?usize, generation: u64, bytes: u64) Error!void {
        // Store the handle before reading its fence so a failed query never
        // loses an admitted operation or its physical lifetime.
        self.jobs[index] = .{ .handle = handle, .fence = undefined, .upload = upload, .generation = generation, .bytes = bytes };
        try accepted(self.client.job_fence(self.device, &handle, &self.jobs[index].?.fence));
        self.last = index;
        if (upload != null) self.stage_job = index;
    }
    fn step(self: *Engine, cache: *layers.Cache) Error!void {
        const dependencies = self.dependency();
        const pointer: u64 = if (dependencies.len == 0) 0 else @intFromPtr(dependencies.ptr);
        switch (self.phase) {
            .upload => {
                if (self.stage_job != null) return error.Busy;
                while (self.next_image < cache.entries.len) : (self.next_image += 1) {
                    const index = self.next_image; const entry = &cache.entries[index]; const image_value = &self.images[index];
                    if (entry.frame != cache.frame or entry.generation == image_value.generation) continue;
                    if (image_value.resource.slot == 0) return error.State;
                    const area = if (image_value.generation == 0) entry.bounds else entry.dirty orelse entry.bounds;
                    const slot = try self.reserve();
                    try self.stage(entry, area);
                    const local_x: u64 = @intCast(area.x - entry.bounds.x); const local_y: u64 = @intCast(area.y - entry.bounds.y);
                    var handle: gfx.R4GfxJob = undefined;
                    try accepted(self.client.copy_submit_ex(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxCopyRequestEx),
                        .copy = .{ .source = self.staging.resource, .target = image_value.resource, .source_offset = 0,
                            .target_offset = local_y * image_value.info.image.pitch + local_x * 4, .byte_length = @as(u64, @intCast(area.w)) * 4, .deadline_ns = self.deadline },
                        .row_count = @intCast(area.h), .source_pitch = self.staging.info.image.pitch, .target_pitch = image_value.info.image.pitch,
                        .dependency_count = @intCast(dependencies.len), .dependencies = pointer }, &handle));
                    try self.track(slot, handle, index, entry.generation, @as(u64, @intCast(area.w)) * @as(u64, @intCast(area.h)) * 4);
                    self.next_image += 1;
                    return;
                }
                self.phase = .draw;
            },
            .draw => {
                if (self.next_command == cache.command_count) { self.phase = .present; return; }
                const slot = try self.reserve();
                const command = cache.commands[self.next_command]; const entry = &cache.entries[command.entry];
                const bounds = entry.bounds; const clip = command.scissor;
                var handle: gfx.R4GfxJob = undefined;
                try accepted(self.client.render_submit(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxRenderRequest),
                    .source = self.images[command.entry].resource, .target = self.output.resource, .pipeline = self.over, .sampler = self.sampler,
                    .source_rect = .{ .x = clip.x - bounds.x, .y = clip.y - bounds.y, .width = @intCast(clip.w), .height = @intCast(clip.h) },
                    .target_rect = .{ .x = clip.x, .y = clip.y, .width = @intCast(clip.w), .height = @intCast(clip.h) },
                    .scissor = .{ .x = clip.x, .y = clip.y, .width = @intCast(clip.w), .height = @intCast(clip.h) },
                    .color = 0, .opacity = 255, .transfer = 0, .dependency_count = @intCast(dependencies.len), .deadline_ns = self.deadline, .dependencies = pointer }, &handle));
                try self.track(slot, handle, null, 0, 0);
                self.next_command += 1; self.render_jobs +|= 1;
            },
            .present => {
                const slot = try self.reserve();
                var handle: gfx.R4GfxJob = undefined;
                try accepted(self.client.image_present(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxImagePresentRequest),
                    .source = self.output.resource, .frame_key = self.frame, .deadline_ns = self.deadline,
                    .dependency_count = @intCast(dependencies.len), .dependencies = pointer, .reserved = 0 }, &handle));
                try self.track(slot, handle, null, 0, 0);
                self.present_fence = self.jobs[slot].?.fence;
                self.phase = .drain;
            },
            else => return error.State,
        }
    }
    fn stage(self: *Engine, entry: *const layers.Entry, area: surface.Rect) Error!void {
        var desc = descriptor(gfx.resource_image); desc.source_kind = gfx.source_borrow_cpu; desc.source_generation = entry.generation;
        desc.image = .{ .cpu_address = @intFromPtr(entry.pixels.ptr), .byte_length = entry.pixels.len * 4,
            .pitch = @as(u64, @intCast(entry.bounds.w)) * 4, .width = @intCast(entry.bounds.w), .height = @intCast(entry.bounds.h), .format = gfx.format_argb8888, .reserved = 0 };
        var source: gfx.R4GfxResource = undefined;
        try accepted(self.client.resource_create(self.device, &desc, &source));
        defer _ = self.client.resource_release(self.device, &source);
        const command: gfx.R4GfxDraw = .{ .source = source, .target = self.staging.resource, .pipeline = self.blit, .sampler = self.sampler,
            .source_rect = .{ .x = @intCast(area.x - entry.bounds.x), .y = @intCast(area.y - entry.bounds.y), .width = @intCast(area.w), .height = @intCast(area.h) },
            .target_rect = .{ .x = 0, .y = 0, .width = @intCast(area.w), .height = @intCast(area.h) }, .color = 0, .opacity = 255 };
        var stats: gfx.R4GfxRenderStats = undefined;
        const pixels = @as(u64, @intCast(area.w)) * @as(u64, @intCast(area.h));
        if (pixels > gfx.render_max_pixels) return error.Limit;
        try accepted(self.client.render(self.device, &.{ .commands = @intFromPtr(&command), .command_count = 1, .flags = 0, .pixel_budget = pixels }, &stats));
    }
    pub fn close(self: *Engine) Error!void {
        if (self.active() or !self.drained()) return error.Busy;
        for (&self.images) |*value| try self.releaseImage(value);
        try self.releaseImage(&self.output); try self.releaseImage(&self.staging);
        for ([_]*gfx.R4GfxResource{ &self.over, &self.blit, &self.sampler }) |handle| if (handle.slot != 0) {
            try accepted(self.client.resource_release(self.device, handle)); handle.* = empty;
        };
    }
};
fn descriptor(kind: u32) gfx.R4GfxResourceDesc {
    var desc = std.mem.zeroes(gfx.R4GfxResourceDesc);
    desc.version = 1; desc.size = @sizeOf(gfx.R4GfxResourceDesc); desc.kind = kind;
    return desc;
}
fn accepted(status: i32) Error!void {
    return switch (status) { gfx.status_ok => {}, gfx.status_busy => error.Busy, gfx.status_stale => error.Stale,
        gfx.status_unsupported, gfx.status_unavailable => error.Unsupported, gfx.status_limit => error.Limit, else => error.Graphics };
}
