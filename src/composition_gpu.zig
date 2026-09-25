//! Desktop GPU resources and a bounded, nonblocking composition transaction.
//! prepare() may allocate and belongs on the preparation thread. advance()
//! only stages changed rectangles and submits/polls ordinary R4GFX jobs.
const std = @import("std");
const r4os = @import("r4os");
const gfx = @import("r4gfx");
const layers = @import("composition_layers.zig");
const surface = @import("surface.zig");
const primitive_assets = @import("primitive_assets.zig");
const primitives = @import("primitive_frame.zig");
const color = @import("composition_software.zig");
const window_image = @import("window_image.zig");
const geometry = @import("output_geometry.zig");
const window_color = @import("window_color.zig");
const empty = std.mem.zeroes(gfx.R4GfxResource);
const Image = struct { resource: gfx.R4GfxResource = empty, color_view: gfx.R4GfxResource = empty, info: gfx.R4GfxResourceInfo = undefined, generation: u64 = 0, charge: u64 = 0,
    external_reference: r4os.abi.GfxBufferHandle = .{}, external_color: ?gfx.R4GfxColorDescription = null };
const Job = struct { handle: gfx.R4GfxJob, fence: gfx.R4GfxCopyFence, upload: ?usize = null, asset_upload: ?usize = null, generation: u64 = 0, bytes: u64 = 0, complete: bool = false, result: u32 = 0 };
pub const Error = error{ Busy, Unsupported, Stale, Graphics, Limit, State, Deadline, Incomplete };
pub const Progress = enum { pending, copied, failed };
pub const Completion = struct { frame: u64, status: gfx.R4GfxSwapchainFrameStatus };
pub const Engine = struct {
    client: *const gfx.DeviceV1Client,
    colors: *const gfx.ColorV1Client,
    device: *const gfx.R4GfxDevice,
    // Null preserves the legacy primary negotiation. Explicit outputs never
    // borrow a different head when their receiver disappears.
    head: ?u32 = null,
    output_format: u32 = gfx.format_xrgb8888,
    output_color: gfx.R4GfxColorDescription = color.description(false, true),
    color_output: bool = false,
    hdr_frame: bool = false,
    images: [layers.capacity]Image = @splat(.{}),
    assets: [primitive_assets.texture_capacity]Image = @splat(.{}),
    outputs: [gfx.swapchain_image_capacity]Image = @splat(.{}),
    workings: [gfx.swapchain_image_capacity]Image = @splat(.{}),
    output_index: usize = 0,
    chain: gfx.R4GfxSwapchain = std.mem.zeroes(gfx.R4GfxSwapchain),
    presentation: ?gfx.R4GfxPresentationInfo = null,
    chain_status: ?gfx.R4GfxSwapchainStatus = null,
    acquired: ?gfx.R4GfxSwapchainFrame = null,
    chain_frames: [gfx.swapchain_image_capacity]u64 = @splat(0),
    reported: [gfx.swapchain_image_capacity]bool = @splat(false),
    // A completed readback copy releases this pin before CPU conversion.
    // Resource retain alone does not prevent a swapchain image being reused.
    readback_pin: gfx.R4GfxResource = empty,
    chain_closing: bool = false,
    input_ns: u64 = 0,
    present_intent: u32 = 0,
    present_blockers: u32 = 0,
    staging: Image = .{},
    over: gfx.R4GfxResource = empty,
    blit: gfx.R4GfxResource = empty,
    fill: gfx.R4GfxResource = empty,
    sampler: gfx.R4GfxResource = empty,
    jobs: [gfx.device_job_capacity]?Job = @splat(null),
    external_last: [layers.capacity]?usize = @splat(null),
    external_receipts: usize = 0,
    last: ?usize = null,
    // Common receipts retain their dependency ancestors. Limit each chain
    // independently of reusable userland job slots and wait for its tail's
    // physical retirement before admitting another chain.
    dependency_depth: usize = 0,
    stage_job: ?usize = null,
    phase: enum { idle, upload, asset_upload, primitives, linear_clear, draw, encode, present, drain } = .idle,
    clear_working: bool = true,
    replace_background: bool = false,
    encode_area: surface.Rect = .{ .x = 0, .y = 0, .w = 0, .h = 0 },
    // Each rotating target contains its own earlier scene. Damage accumulates
    // until that exact target has completed composition. Layer captures stay
    // complete so primitive reuse never mistakes a clipped stream for a layer.
    capture_damage: ?surface.Rect = null,
    capture_view: ?geometry.topology.Viewport = null,
    target_damage: [gfx.swapchain_image_capacity]?surface.Rect = @splat(null),
    frame: u64 = 0,
    next_image: usize = 0,
    next_command: usize = 0,
    next_asset: usize = 0,
    asset_x: i32 = 0,
    asset_y: i32 = 0,
    next_primitive: usize = 0,
    deadline: u64 = 0,
    fault: ?Error = null,
    present_fence: ?gfx.R4GfxCopyFence = null,
    uploaded_bytes: u64 = 0,
    render_jobs: u64 = 0,
    primitive_jobs: u64 = 0,
    primitive_draws: u64 = 0,
    batch_enabled: bool = false,
    grid_enabled: bool = false,
    // A bounded CPU snapshot belongs to this output, not to the shared asset
    // cache. Allocate it only on the preparation thread; Engine itself must
    // remain small enough to create on a normal worker stack.
    snapshot_allocator: ?std.mem.Allocator = null,
    snapshot: []primitives.Command = &.{},
    snapshot_count: usize = 0,
    snapshot_images: [layers.capacity]gfx.R4GfxResource = @splat(empty),
    snapshot_assets: [primitive_assets.texture_capacity]u64 = @splat(0),
    reuse_layers: [layers.capacity]bool = @splat(false),
    reserved_bytes: u64 = 0,
    budget_bytes: u64 = 256 * 1024 * 1024,

    pub fn init(client: *const gfx.DeviceV1Client, colors: *const gfx.ColorV1Client, device: *const gfx.R4GfxDevice) Engine { return .{ .client = client, .colors = colors, .device = device }; }
    /// Configure the confirmed wire encoding before any resources or worker
    /// thread exist. A changed output gets a new owner after the old one drains.
    pub fn configureOutput(self: *Engine, state: ?r4os.abi.GfxOutputColorState, format: u32) Error!void {
        if (self.active() or self.reserved_bytes != 0 or self.chain.slot != 0) return error.Busy;
        var description = color.description(false, true);
        if (state) |value| {
            if (value.version != 1 or value.size < 128 or value.flags & 7 != 7 or
                value.format != format or value.reference_white == 0 or value.peak < value.reference_white or value.black >= value.reference_white or
                (value.primaries != gfx.color_primaries_srgb and value.primaries != gfx.color_primaries_bt2020) or
                (value.transfer != gfx.color_transfer_srgb and value.transfer != gfx.color_transfer_pq and value.transfer != gfx.color_transfer_hlg) or
                (value.range != gfx.color_range_full and value.range != gfx.color_range_limited)) return error.Unsupported;
            description.primaries = value.primaries; description.transfer = value.transfer; description.range = value.range;
            description.reference_white = value.reference_white; description.peak = value.peak; description.black = value.black;
            description.precision = switch (format) {
                gfx.format_xrgb8888 => if (value.bpc == 8) gfx.color_precision_unorm8 else return error.Unsupported,
                gfx.format_xrgb2101010 => if (value.bpc == 10) gfx.color_precision_unorm10 else return error.Unsupported,
                else => return error.Unsupported,
            };
        } else if (format != gfx.format_xrgb8888) return error.Unsupported;
        try accepted(self.colors.color_description_validate(&description));
        self.output_format = format; self.output_color = description;
        self.color_output = !std.meta.eql(description, color.description(false, true));
    }
    pub fn requiredOperations(self: *const Engine) u32 {
        return gfx.device_gpu_render | gfx.device_gpu_present | gfx.device_gpu_copy_rows |
            @as(u32, if (self.color_output) gfx.device_gpu_color else 0);
    }
    pub fn active(self: *const Engine) bool { return self.phase != .idle; }
    fn output(self: *Engine) *Image { return &self.outputs[self.output_index]; }
    fn working(self: *Engine) *Image { return &self.workings[self.output_index]; }
    fn hasHdr(cache: *const layers.Cache) bool {
        for (cache.commands[0..cache.command_count]) |command|
            if (cache.entries[command.entry].external) |frame|
                if (window_color.absolute(frame.description())) return true;
        return false;
    }
    pub fn invalidate(self: *Engine) void {
        for (&self.outputs) |*value| value.generation = 0;
        self.target_damage = @splat(null);
        self.snapshot_count = 0;
    }
    pub fn compositionDamage(self: *Engine, view: geometry.topology.Viewport, damage: ?surface.Rect) void {
        std.debug.assert(!self.active());
        if (self.capture_view == null or !std.meta.eql(self.capture_view.?, view)) self.invalidate();
        self.capture_view = view;
        self.capture_damage = null;
        // Fractional/rotated output sampling still uses full reconstruction.
        // Normal 1:1 outputs have an exact logical-to-native translation.
        if (view.scale != 120 or view.rotation != .normal) return;
        const bounds = geometry.logical(view) catch return;
        const clipped = geometry.intersect(bounds, damage orelse return) orelse return;
        self.capture_damage = .{ .x = clipped.x - bounds.x, .y = clipped.y - bounds.y, .w = clipped.w, .h = clipped.h };
    }
    pub fn pending(self: *const Engine) bool {
        for (self.chain_frames) |key| if (key != 0) return true;
        return false;
    }
    pub fn needsPolling(self: *const Engine) bool {
        if (self.active() or self.chain_closing) return true;
        const status = self.chain_status orelse return self.pending();
        var fronts: usize = 0;
        for ([_]gfx.R4GfxSwapchainFrameStatus{ status.frame0, status.frame1, status.frame2 }, 0..) |value, i| {
            if (self.chain_frames[i] == 0) continue;
            if (!self.reported[i] or value.phase != 4 or value.result != 1 or value.path != gfx.present_path_direct) return true;
            fronts += 1;
        }
        // One static visible front remains owned but needs no short polling
        // loop. New damage and ordinary desktop events resume its owner.
        return fronts > 1;
    }
    pub fn captureBlocked(self: *const Engine, now: u64) bool {
        if (self.active()) return true;
        if (self.chain.slot == 0 and self.readback_pin.slot != 0) return true;
        if (self.chain.slot == 0 or self.acquired != null) return false;
        const status = self.chain_status orelse return false;
        if (status.life >= 2) return false; // Let the owner rebuild/fall back.
        if (status.life == 1 or now < status.next_start_ns) return true;
        for ([_]gfx.R4GfxSwapchainFrameStatus{ status.frame0, status.frame1, status.frame2 }, 0..) |value, index|
            if (index < status.count and value.phase == 0) return false;
        return true;
    }
    /// Dirty output may be waiting only for its next producer start. Include
    /// that deadline in the normal event wait after all GPU work has drained.
    pub fn captureWaitTicks(self: *const Engine, now_ns: u64, hz: u32, limit: u64) u64 {
        if (self.active() or self.fault != null or self.chain.slot == 0 or self.acquired != null) return limit;
        const status = self.chain_status orelse return limit;
        if (status.life != 0) return limit;
        for ([_]gfx.R4GfxSwapchainFrameStatus{ status.frame0, status.frame1, status.frame2 }, 0..) |value, index| {
            if (index >= status.count or value.phase != 0) continue;
            const ticks = r4os.time_contract.durationToTicks(.{ .nanoseconds = status.next_start_ns -| now_ns }, hz) catch return limit;
            return @min(limit, @max(1, ticks));
        }
        return limit;
    }
    pub fn acquire(self: *Engine, input_ns: u64) Error!void {
        if (self.chain.slot == 0 or self.acquired != null) return;
        var frame: gfx.R4GfxSwapchainFrame = undefined;
        try accepted(self.client.swapchain_acquire(self.device, &self.chain, input_ns, &frame));
        if (frame.slot == 0 or frame.slot > self.outputs.len or !std.meta.eql(frame.image, self.outputs[frame.slot - 1].resource)) return error.Stale;
        self.acquired = frame; self.output_index = frame.slot - 1;
    }
    pub fn pollPresentation(self: *Engine) Error!void {
        const profile_stamp = @import("presentation_profile.zig").stamp();
        defer @import("presentation_profile.zig").end(.engine_poll, profile_stamp);
        if (self.chain.slot == 0) return;
        if (self.chain_closing) {
            self.closeChain() catch |err| { if (err != error.Busy) return err; };
            return;
        }
        var status: gfx.R4GfxSwapchainStatus = undefined;
        try accepted(self.client.swapchain_poll(self.device, &self.chain, &status));
        self.chain_status = status;
        if (status.life >= 2) return error.Stale;
        // Reporting visibility and releasing storage are independent. A
        // front buffer can already be visible while scanout still reads it.
        for ([_]gfx.R4GfxSwapchainFrameStatus{ status.frame0, status.frame1, status.frame2 }, 0..) |value, i| {
            if (value.phase != 4 or value.held_flags != 0 or !self.reported[i] or std.meta.eql(value.frame.image, self.readback_pin)) continue;
            const rc = self.client.swapchain_release(self.device, &self.chain, &value.frame);
            if (rc == gfx.status_busy) continue;
            try accepted(rc);
            self.chain_frames[i] = 0; self.reported[i] = false;
            switch (i) { 0 => self.chain_status.?.frame0.phase = 0, 1 => self.chain_status.?.frame1.phase = 0, 2 => self.chain_status.?.frame2.phase = 0, else => unreachable }
        }
    }
    pub fn completion(self: *Engine) ?Completion {
        const status = self.chain_status orelse return null;
        for ([_]gfx.R4GfxSwapchainFrameStatus{ status.frame0, status.frame1, status.frame2 }, 0..) |value, i| {
            if (value.phase != 4 or self.chain_frames[i] == 0 or self.reported[i]) continue;
            self.reported[i] = true;
            return .{ .frame = self.chain_frames[i], .status = value };
        }
        return null;
    }
    pub fn needsFull(self: *const Engine, width: i32, height: i32) bool {
        const target = &self.outputs[self.output_index];
        // Rotating targets still need a complete source-layer capture. Final
        // composition may be clipped using each target's accumulated damage.
        return self.chain.slot != 0 or target.resource.slot == 0 or target.generation == 0 or
            target.info.image.width != width or target.info.image.height != height;
    }
    pub fn prepared(self: *Engine, cache: *const layers.Cache) bool {
        if (self.chain_status) |status| if (status.life >= 2) return false;
        if (self.over.slot == 0 or self.blit.slot == 0 or self.fill.slot == 0 or self.sampler.slot == 0 or
            !self.imageFits(self.working(), cache.screen.w, cache.screen.h, gfx.format_abgr16161616f) or
            !self.imageFits(self.output(), cache.screen.w, cache.screen.h, self.output_format) or
            !self.imageFits(&self.staging, if (cache.recording != null) 512 else cache.screen.w,
                if (cache.recording != null) 512 else cache.screen.h, gfx.format_argb8888)) return false;
        if (hasHdr(cache) != self.hdr_frame) return false;
        var visited: [layers.capacity]bool = @splat(false);
        for (&self.images, 0..) |*image_value, i| if (image_value.external_reference.id != 0 and
            cache.entries[i].frame != cache.frame) return false;
        for (cache.commands[0..cache.command_count]) |command| {
            if (visited[command.entry]) continue;
            visited[command.entry] = true;
            const entry = &cache.entries[command.entry];
            if (entry.external) |frame| {
                if (!self.externalFits(&self.images[command.entry], frame)) return false;
                continue;
            }
            if (!self.imageFits(&self.images[command.entry], entry.bounds.w, entry.bounds.h, gfx.format_argb8888) or self.images[command.entry].color_view.slot == 0) return false;
        }
        if (cache.recording) |recording| {
            if (self.fill.slot == 0 or self.snapshot.len < recording.commands.len) return false;
            for (&recording.assets.textures, 0..) |*texture, index| {
                if (texture.pinned != cache.frame) continue;
                if (!self.imageFits(&self.assets[index], @intCast(texture.width), @intCast(texture.height), gfx.format_argb8888)) return false;
            }
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
        const operations = self.requiredOperations();
        if (device_info.gpu_operations & operations != operations) return error.Unsupported;
        self.batch_enabled = device_info.gpu_operations & gfx.device_gpu_render_list != 0;
        self.grid_enabled = device_info.gpu_operations & gfx.device_gpu_grid != 0;
        for (&self.images, 0..) |*image_value, i| if (image_value.external_reference.id != 0 and
            cache.entries[i].frame != cache.frame) try self.releaseImage(image_value);
        if (cache.recording) |recording| if (recording.view != null and !self.grid_enabled) return error.Unsupported;
        try self.stateResource(&self.over, gfx.resource_pipeline, gfx.render_operation_over);
        try self.stateResource(&self.fill, gfx.resource_pipeline, gfx.render_operation_fill);
        try self.stateResource(&self.blit, gfx.resource_pipeline, gfx.render_operation_blit);
        try self.stateResource(&self.sampler, gfx.resource_sampler, gfx.render_sampler_nearest);
        var presentation: ?gfx.R4GfxPresentationInfo = null;
        for (0..8) |head| {
            if (self.head) |selected| if (selected != head) continue;
            var value: gfx.R4GfxPresentationInfo = undefined;
            if (self.client.presentation_info(self.device, @intCast(head), &value) != gfx.status_ok or
                value.flags & gfx.present_native == 0 or value.adapter_id != device_info.adapter_id or
                value.device_generation != device_info.device_generation or value.reset_generation != device_info.reset_generation or
                value.width != cache.screen.w or value.height != cache.screen.h or value.format != self.output_format) continue;
            presentation = value; break;
        }
        if ((self.head != null or self.color_output) and presentation == null) return error.Stale;
        if (self.chain.slot != 0) {
            if (presentation == null or self.presentation.?.display_generation != presentation.?.display_generation or
                self.presentation.?.width != presentation.?.width or self.presentation.?.height != presentation.?.height) try self.closeChain();
        }
        const count: u32 = if (presentation) |value| value.buffer_count else 1;
        if (count < 1 or count > self.outputs.len) return error.Unsupported;
        const direct = presentation != null and presentation.?.flags & gfx.present_direct != 0 and device_info.gpu_operations & gfx.device_gpu_direct != 0;
        for (self.outputs[0..count]) |*value| {
            self.imageKind(value, cache.screen.w, cache.screen.h, self.output_format, true, direct, deadline) catch |err| {
                // Contiguous scanout storage can be unavailable under VRAM
                // pressure. The regular render/copy pool remains usable.
                if (!direct or (err != error.Unsupported and err != error.Limit)) return err;
                try self.image(value, cache.screen.w, cache.screen.h, self.output_format, true, deadline);
            };
        }
        const hdr = hasHdr(cache);
        if (hdr != self.hdr_frame) {
            // The old workers are drained. Recreate their private FP16
            // storage with its new absolute scale; never relabel a live BO.
            for (&self.workings) |*value| try self.releaseImage(value);
            self.hdr_frame = hdr;
            self.invalidate();
        }
        for (self.workings[0..count]) |*value| try self.image(value, cache.screen.w, cache.screen.h, gfx.format_abgr16161616f, true, deadline);
        if (presentation) |value| if (self.chain.slot == 0) {
            var handles: [gfx.swapchain_image_capacity]gfx.R4GfxResource = undefined;
            for (0..count) |i| handles[i] = self.outputs[i].resource;
            try accepted(self.client.swapchain_open(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxSwapchainDesc),
                .head_id = value.head_id, .policy = gfx.present_policy_fifo, .flags = gfx.present_require_vsync,
                .count = count, .display_generation = value.display_generation, .images = @intFromPtr(&handles) }, &self.chain));
            self.presentation = value;
        };
        try self.image(&self.staging, if (cache.recording != null) 512 else cache.screen.w,
            if (cache.recording != null) 512 else cache.screen.h, gfx.format_argb8888, false, deadline);
        var visited: [layers.capacity]bool = @splat(false);
        for (cache.commands[0..cache.command_count]) |command| {
            const index = command.entry;
            if (visited[index]) continue;
            visited[index] = true;
            const entry = &cache.entries[index];
            if (!entry.initialized or entry.generation == 0) return error.State;
            if (entry.external) |frame| {
                const needed = gfx.device_gpu_color_grid | gfx.device_gpu_color | gfx.device_gpu_grid | gfx.device_gpu_render_list;
                if (device_info.gpu_operations & needed != needed) return error.Unsupported;
                try self.externalImage(&self.images[index], frame);
                continue;
            }
            try self.image(&self.images[index], entry.bounds.w, entry.bounds.h, gfx.format_argb8888, true, deadline);
            try self.colorView(&self.images[index]);
        }
        if (cache.recording) |recording| {
            if (recording.commands.len > primitives.capacity) return error.Limit;
            if (self.snapshot.len < recording.commands.len) {
                const replacement = cache.allocator.alloc(primitives.Command, recording.commands.len) catch return error.Limit;
                if (self.snapshot_allocator) |allocator| allocator.free(self.snapshot);
                self.snapshot = replacement; self.snapshot_allocator = cache.allocator; self.snapshot_count = 0;
            }
            try self.stateResource(&self.fill, gfx.resource_pipeline, gfx.render_operation_fill);
            for (&recording.assets.textures, 0..) |*texture, index| {
                if (texture.pinned != cache.frame) continue;
                try self.image(&self.assets[index], @intCast(texture.width), @intCast(texture.height), gfx.format_argb8888, true, deadline);
            }
        }
    }
    fn stateResource(self: *Engine, handle: *gfx.R4GfxResource, kind: u32, operation: u32) Error!void {
        if (handle.slot != 0) return;
        var desc = descriptor(kind);
        if (kind == gfx.resource_sampler) desc.sampler = operation else desc.operation = operation;
        try accepted(self.client.resource_create(self.device, &desc, handle));
    }
    fn externalFits(self: *Engine, image_value: *Image, frame: *const window_image.Frame) bool {
        return std.meta.eql(image_value.external_reference, frame.reference.reference) and
            image_value.external_color != null and std.meta.eql(image_value.external_color.?, frame.description()) and
            self.imageFits(image_value, @intCast(frame.message.descriptor.width), @intCast(frame.message.descriptor.height), frame.message.descriptor.format);
    }
    fn externalImage(self: *Engine, image_value: *Image, frame: *const window_image.Frame) Error!void {
        if (self.externalFits(image_value, frame)) return;
        try self.releaseImage(image_value);
        const bytes = frame.message.descriptor.byte_length;
        if (bytes > self.budget_bytes or self.reserved_bytes > self.budget_bytes - bytes) return error.Limit;
        var desc = descriptor(gfx.resource_image);
        desc.source_kind = gfx.source_import_buffer;
        desc.source_address = @intFromPtr(&frame.reference.reference);
        try accepted(self.colors.color_resource_create(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxColorResourceDesc),
            .resource = desc, .description = frame.description() }, &image_value.resource));
        image_value.charge = bytes; self.reserved_bytes += bytes;
        try accepted(self.client.resource_info(self.device, &image_value.resource, &image_value.info));
        if (image_value.info.buffer_id != frame.reference.buffer.id or image_value.info.buffer_generation != frame.reference.buffer.generation) return error.Stale;
        image_value.external_reference = frame.reference.reference; image_value.external_color = frame.description();
    }
    fn image(self: *Engine, target: *Image, width: i32, height: i32, format: u32, native: bool, deadline: u64) Error!void {
        return self.imageKind(target, width, height, format, native, false, deadline);
    }
    fn colorView(self: *Engine, source: *Image) Error!void {
        if (source.color_view.slot != 0) return;
        var desc = descriptor(gfx.resource_image);
        desc.source_kind = gfx.source_color_view;
        desc.source_address = @intFromPtr(&source.resource);
        try accepted(self.colors.color_resource_create(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxColorResourceDesc),
            .resource = desc, .description = color.description(false, false) }, &source.color_view));
    }
    fn imageKind(self: *Engine, target: *Image, width: i32, height: i32, format: u32, native: bool, scanout: bool, deadline: u64) Error!void {
        if (width <= 0 or height <= 0) return error.State;
        if (target.resource.slot != 0) {
            if (self.imageFits(target, width, height, format)) return;
            try self.releaseImage(target);
        }
        const row: u64 = @as(u64, @intCast(width)) * @as(u64, if (format == gfx.format_abgr16161616f) 8 else 4);
        const pitch = if (native) (row + 255) & ~@as(u64, 255) else row;
        const size = pitch * @as(u64, @intCast(height));
        const charge = (size + (if (native) @as(u64, 65535) else 4095)) & ~(if (native) @as(u64, 65535) else 4095);
        if (charge > self.budget_bytes or self.reserved_bytes > self.budget_bytes - charge) return error.Limit;
        var desc = descriptor(gfx.resource_image); desc.flags = gfx.image_target;
        const request: gfx.R4GfxNativeImage = .{ .version = 1, .size = @sizeOf(gfx.R4GfxNativeImage), .deadline_ns = deadline,
            .width = @intCast(width), .height = @intCast(height), .format = format, .layout = 0 };
        if (native) { desc.source_kind = if (scanout) gfx.source_create_native_scanout else gfx.source_create_native; desc.source_address = @intFromPtr(&request); } else {
            desc.source_kind = gfx.source_create_system;
            desc.image = .{ .cpu_address = 0, .byte_length = size, .pitch = pitch, .width = @intCast(width), .height = @intCast(height), .format = format, .reserved = 0 };
        }
        if (format == gfx.format_xrgb8888 or format == gfx.format_xrgb2101010 or format == gfx.format_abgr16161616f) {
            try accepted(self.colors.color_resource_create(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxColorResourceDesc),
                .resource = desc, .description = if (format == gfx.format_abgr16161616f) (if (self.hdr_frame) window_color.working(self.output_color) else color.description(true, false)) else self.output_color }, &target.resource));
        } else try accepted(self.client.resource_create(self.device, &desc, &target.resource));
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
        if (image_value.color_view.slot != 0) {
            try accepted(self.client.resource_release(self.device, &image_value.color_view));
            image_value.color_view = empty;
        }
        if (image_value.resource.slot == 0) return;
        try accepted(self.client.resource_release(self.device, &image_value.resource));
        self.reserved_bytes -= image_value.charge;
        image_value.* = .{};
    }
    fn planReuse(self: *Engine, cache: *const layers.Cache) void {
        self.reuse_layers = @splat(false);
        const recording = cache.recording orelse return;
        if (recording.mirror or self.snapshot_count == 0) return;
        for (&cache.entries, 0..) |*entry, index| {
            const image_value = &self.images[index];
            if (entry.frame != cache.frame or entry.external != null or image_value.generation == 0 or
                image_value.resource.slot == 0 or !std.meta.eql(image_value.resource, self.snapshot_images[index])) continue;
            // Compare fields, never padding bytes or hashes. Commands remain
            // ordered within each layer; the final layer composition is still
            // executed in full, including changed positions and scissors.
            var previous: usize = 0;
            var matched: usize = 0;
            const identical = for (recording.commands[0..recording.count]) |command| {
                if (command.layer != index) continue;
                while (previous < self.snapshot_count and self.snapshot[previous].layer != index) previous += 1;
                if (previous == self.snapshot_count or !std.meta.eql(command, self.snapshot[previous])) break false;
                if (command.texture) |texture| {
                    const generation = recording.assets.textures[texture].generation;
                    if (generation == 0 or generation != self.snapshot_assets[texture] or generation != self.assets[texture].generation) break false;
                }
                previous += 1; matched += 1;
            } else true;
            if (!identical or matched == 0) continue;
            while (previous < self.snapshot_count and self.snapshot[previous].layer != index) previous += 1;
            self.reuse_layers[index] = previous == self.snapshot_count;
        }
    }
    /// Reuse CPU commands only outside the caller's accumulated scene damage.
    /// The usual full layer walk still determines presence, geometry and order.
    pub fn reuseCapture(self: *const Engine, cache: *layers.Cache, damage: ?surface.Rect) void {
        const changed = damage orelse return;
        const recording = cache.recording orelse return;
        const view = cache.view orelse return;
        if (!cache.collecting or cache.command_count != 0 or recording.mirror or self.active() or
            self.fault != null or self.snapshot_count == 0 or self.frame != cache.frame - 1 or
            self.capture_view == null or !std.meta.eql(self.capture_view.?, view)) return;
        for (&cache.entries, 0..) |*entry, index| {
            const image_value = &self.images[index];
            cache.replay_layers[index] = entry.initialized and entry.external == null and
                entry.frame == self.frame and geometry.intersect(entry.capture_bounds, changed) == null and
                image_value.generation == entry.generation and image_value.resource.slot != 0 and
                std.meta.eql(image_value.resource, self.snapshot_images[index]);
        }
        const commands = self.snapshot[0..self.snapshot_count];
        for (commands) |command| if (command.texture) |texture| {
            const generation = recording.assets.textures[texture].generation;
            if (generation == 0 or generation != self.snapshot_assets[texture]) cache.replay_layers[command.layer] = false;
        };
        // Do this before any new intern(): a painter earlier in layer order
        // must not evict a texture needed by a later retained layer. Appending
        // new atlas content is safe; normal GPU generation checks still apply.
        for (commands) |command| if (cache.replay_layers[command.layer]) {
            if (command.texture) |texture| {
                recording.assets.textures[texture].pinned = recording.assets.frame;
                recording.assets.textures[texture].touched = recording.assets.frame;
            }
        };
        cache.replay_commands = commands;
    }
    fn rememberPrimitives(self: *Engine, cache: *const layers.Cache) void {
        self.snapshot_count = 0;
        const recording = cache.recording orelse return;
        if (recording.mirror) return;
        // Called only after every write/read has physically retired without
        // a fault. A cancelled partial redraw must never certify old pixels.
        std.debug.assert(recording.count <= self.snapshot.len);
        @memcpy(self.snapshot[0..recording.count], recording.commands[0..recording.count]);
        self.snapshot_count = recording.count;
        for (&cache.entries, 0..) |*entry, index| self.snapshot_images[index] =
            if (entry.frame == cache.frame and entry.external == null) self.images[index].resource else empty;
        for (&self.assets, 0..) |*asset, index| self.snapshot_assets[index] = asset.generation;
    }
    pub fn begin(self: *Engine, cache: *const layers.Cache, deadline: u64) Error!void {
        if (self.active() or !self.drained()) return error.Busy;
        if (self.chain.slot == 0 and self.readback_pin.slot != 0) return error.Busy;
        if (cache.collecting or cache.failure != null or cache.command_count == 0 or self.output().resource.slot == 0 or self.staging.resource.slot == 0)
            return error.State;
        if (cache.recording) |recording| if (recording.count > self.snapshot.len) return error.State;
        // The first command of a complete desktop capture is its opaque
        // background (or fullscreen terminal). A new target must start there.
        if (self.needsFull(cache.screen.w, cache.screen.h) and !std.meta.eql(cache.commands[0].scissor, cache.screen)) return error.Incomplete;
        try self.acquire(self.input_ns);
        if (self.hdr_frame != hasHdr(cache)) return error.State;
        self.clear_working = self.needsFull(cache.screen.w, cache.screen.h);
        self.encode_area = cache.commands[0].scissor;
        for (cache.commands[1..cache.command_count]) |command| self.encode_area = self.encode_area.merged(command.scissor);
        if (self.capture_damage) |damage| {
            // Only complete captures can reconstruct older targets. The
            // caller's damage refers to changes since the last admitted frame.
            if (!std.meta.eql(cache.commands[0].scissor, cache.screen)) return error.Incomplete;
            for (&self.target_damage) |*area| area.* = if (area.*) |old| old.merged(damage) else damage;
            if (self.output().generation != 0) self.encode_area = self.target_damage[self.output_index].?;
            self.clear_working = true;
        } else {
            // An untracked capture may change any pixel; other target ages
            // cannot carry forward a narrower damage certificate.
            for (&self.target_damage) |*area| area.* = cache.screen;
        }
        // Over a cleared transparent target, the first layer is exactly its
        // converted source, including alpha. Replace it in one draw only if
        // it covers the entire reconstruction area. External/HDR conversion
        // and partial coverage keep the explicit clear and ordinary blend.
        const first = cache.commands[0];
        self.replace_background = self.clear_working and !self.hdr_frame and
            cache.entries[first.entry].external == null and
            (if (geometry.intersect(first.scissor, self.encode_area)) |covered|
                std.meta.eql(covered, self.encode_area) else false);
        if (self.acquired) |value| { self.chain_frames[value.slot - 1] = cache.frame; self.reported[value.slot - 1] = false; }
        self.frame = cache.frame; self.deadline = deadline; self.phase = if (cache.recording != null) .asset_upload else .upload;
        self.next_image = 0; self.next_command = 0; self.fault = null; self.present_fence = null;
        self.last = null; self.stage_job = null; self.dependency_depth = 0;
        self.external_last = @splat(null);
        self.next_asset = 0; self.asset_x = 0; self.asset_y = 0; self.next_primitive = 0;
        self.planReuse(cache);
    }
    pub fn cancel(self: *Engine, reason: Error) void {
        if (self.fault == null) {
            @import("startup_diagnosis.zig").record("engine-cancel reason={s} phase={s} frame={d} command={d} primitive={d} asset={d} jobs={d} draws={d} uploads={d} resources={d}",
                .{@errorName(reason), @tagName(self.phase), self.frame, self.next_command, self.next_primitive, self.next_asset,
                    self.render_jobs, self.primitive_draws, self.uploaded_bytes, self.reserved_bytes});
            self.fault = reason;
        }
        self.phase = .drain;
        self.invalidate();
        if (self.acquired) |value| {
            if (self.client.swapchain_release(self.device, &self.chain, &value) == gfx.status_ok) {
                self.chain_frames[value.slot - 1] = 0; self.acquired = null;
            }
        }
        for (&self.jobs) |*slot| if (slot.*) |*job| { _ = self.client.job_cancel(self.device, &job.handle); };
        if (self.chain.slot != 0) {
            self.chain_closing = true;
            self.closeChain() catch {};
        }
    }
    pub fn advance(self: *Engine, cache: *layers.Cache, now: u64) Progress {
        const profile_stamp = @import("presentation_profile.zig").stamp();
        defer @import("presentation_profile.zig").end(.engine_advance, profile_stamp);
        self.pollPresentation() catch |err| { if (self.active()) self.cancel(err); };
        if (!self.active()) return if (self.fault == null) .copied else .failed;
        if (cache.frame != self.frame or cache.collecting) self.cancel(error.State);
        if (now >= self.deadline and self.fault == null) self.cancel(error.Deadline);
        self.collect(cache) catch |err| self.cancel(err);
        if (self.phase == .drain) {
            if (!self.finishExternal(cache, self.fault != null)) return .pending;
            if (!self.drained()) return .pending;
            if (self.fault != null) if (self.acquired) |value| {
                const rc = self.client.swapchain_release(self.device, &self.chain, &value);
                if (rc == gfx.status_busy) return .pending;
                self.acquired = null; self.chain_frames[value.slot - 1] = 0;
            };
            self.phase = .idle;
            if (self.fault == null) {
                self.output().generation = self.frame;
                self.target_damage[self.output_index] = null;
                if (cache.recording != null) for (&cache.entries, 0..) |*entry, index| {
                    if (entry.frame == cache.frame) self.images[index].generation = entry.generation;
                };
                self.rememberPrimitives(cache);
            }
            return if (self.fault == null) .copied else .failed;
        }
        // Admission is bounded independently of scene size. Returning Busy
        // leaves this exact operation for a subsequent desktop event cycle.
        for (0..4) |_| {
            if (self.dependency_depth >= self.jobs.len) break;
            self.step(cache) catch |err| {
                if (err != error.Busy) self.cancel(err);
                return .pending;
            };
            if (self.phase == .drain) break;
        }
        return .pending;
    }
    fn collect(self: *Engine, cache: *layers.Cache) Error!void {
        const profile_stamp = @import("presentation_profile.zig").stamp();
        defer @import("presentation_profile.zig").end(.engine_collect, profile_stamp);
        for (&self.jobs, 0..) |*slot, index| if (slot.*) |*job| {
            var info: gfx.R4GfxJobInfo = undefined;
            try accepted(self.client.job_info(self.device, &job.handle, &info));
            if (info.phase != r4os.abi.gfx_queue_phase_terminal or info.flags != 0) continue;
            if (info.result != r4os.abi.gfx_queue_result_complete and self.fault == null) {
                @import("startup_diagnosis.zig").record("job-failed index={d} phase={d} result={d} flags={x}", .{index, info.phase, info.result, info.flags});
                self.cancel(error.Graphics);
            }
            if (!job.complete) {
                if (job.upload) |image_index| if (info.result == r4os.abi.gfx_queue_result_complete) {
                    self.images[image_index].generation = job.generation;
                    cache.uploaded(image_index, job.generation);
                };
                if (info.result == r4os.abi.gfx_queue_result_complete) {
                    self.uploaded_bytes +|= job.bytes;
                    if (job.asset_upload) |asset_index| {
                        self.assets[asset_index].generation = job.generation;
                        if (cache.recording) |recording| recording.assets.uploaded(asset_index, job.generation);
                    }
                }
                job.complete = true;
                job.result = info.result;
            }
            if (self.stage_job == index) self.stage_job = null;
            // Result and physical retirement were checked above. A later
            // submit is now ordered by this observation and need not pin the
            // completed dependency graph. External window receipts keep
            // their own exact handle until the consumer returns it.
            if (self.last == index) { self.last = null; self.dependency_depth = 0; }
            var retained = false;
            for (self.external_last) |last| if (last == index) { retained = true; break; };
            if (retained) continue;
            const released = self.client.job_release(self.device, &job.handle);
            if (released == gfx.status_busy) continue;
            try accepted(released);
            if (self.last == index) self.last = null;
            slot.* = null;
        };
    }
    fn drained(self: *const Engine) bool { for (&self.jobs) |*job| if (job.* != null) return false; return true; }
    fn finishExternal(self: *Engine, cache: *layers.Cache, failed: bool) bool {
        // Every output read must have physically retired, including an older
        // cancelled draw. A terminal result with held resources is insufficient.
        for (&self.jobs) |*slot| if (slot.*) |job| if (!job.complete) return false;
        for (&cache.entries, 0..) |*entry, i| if (entry.borrowed) {
            var receipt: ?window_image.Receipt = null;
            if (self.external_last[i]) |index| {
                const job = &self.jobs[index].?;
                if (self.client.job_fence(self.device, &job.handle, &job.fence) != gfx.status_ok) return false;
                receipt = .{ .client = self.client, .device = self.device, .owner_holds = &self.external_receipts,
                    .job = job.handle, .fence = job.fence, .result = job.result };
            }
            if (!entry.external.?.finish(receipt, failed)) return false;
            if (self.external_last[i]) |index| { self.jobs[index] = null; if (self.last == index) self.last = null; }
            self.external_last[i] = null; entry.borrowed = false;
        };
        return true;
    }
    pub fn discardCapture(self: *Engine, cache: *layers.Cache) bool {
        if (self.active() or !self.drained()) return false;
        return self.finishExternal(cache, true);
    }
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
        self.dependency_depth += 1;
        if (bytes != 0) self.stage_job = index;
    }
    fn step(self: *Engine, cache: *layers.Cache) Error!void {
        const profile_stamp = @import("presentation_profile.zig").stamp();
        const profile_stage: @import("presentation_profile.zig").Stage = switch (self.phase) {
            .idle => .step_idle, .upload => .step_upload, .asset_upload => .step_asset_upload,
            .primitives => .step_primitives, .linear_clear => .step_linear_clear, .draw => .step_draw,
            .encode => .step_encode, .present => .step_present, .drain => .step_drain,
        };
        defer @import("presentation_profile.zig").end(profile_stage, profile_stamp);
        const dependencies = self.dependency();
        const pointer: u64 = if (dependencies.len == 0) 0 else @intFromPtr(dependencies.ptr);
        switch (self.phase) {
            .asset_upload => {
                const recording = cache.recording orelse return error.State;
                if (self.stage_job != null) return error.Busy;
                while (self.next_asset < self.assets.len) : (self.next_asset += 1) {
                    const index = self.next_asset; const texture = &recording.assets.textures[index]; const image_value = &self.assets[index];
                    if (texture.pinned != cache.frame or image_value.generation == texture.generation) continue;
                    if (image_value.resource.slot == 0) return error.State;
                    const area = if (image_value.generation == 0) texture.rect() else texture.dirty orelse texture.rect();
                    const tile: surface.Rect = .{ .x = area.x + self.asset_x, .y = area.y + self.asset_y,
                        .w = @min(@as(i32, @intCast(self.staging.info.image.width)), area.w - self.asset_x),
                        .h = @min(@as(i32, @intCast(self.staging.info.image.height)), area.h - self.asset_y) };
                    const slot = try self.reserve();
                    try self.stagePixels(texture.pixels, texture.width, texture.height, texture.generation, tile);
                    var handle: gfx.R4GfxJob = undefined;
                    try accepted(self.client.copy_submit_ex(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxCopyRequestEx),
                        .copy = .{ .source = self.staging.resource, .target = image_value.resource, .source_offset = 0,
                            .target_offset = @as(u64, @intCast(tile.y)) * image_value.info.image.pitch + @as(u64, @intCast(tile.x)) * 4,
                            .byte_length = @as(u64, @intCast(tile.w)) * 4, .deadline_ns = self.deadline },
                        .row_count = @intCast(tile.h), .source_pitch = self.staging.info.image.pitch, .target_pitch = image_value.info.image.pitch,
                        .dependency_count = @intCast(dependencies.len), .dependencies = pointer }, &handle));
                    try self.track(slot, handle, null, texture.generation, @as(u64, @intCast(tile.w)) * @as(u64, @intCast(tile.h)) * 4);
                    self.asset_x += tile.w;
                    if (self.asset_x == area.w) { self.asset_x = 0; self.asset_y += tile.h; }
                    if (self.asset_y == area.h) {
                        self.jobs[slot].?.asset_upload = index;
                        self.asset_y = 0; self.next_asset += 1;
                    }
                    return;
                }
                self.phase = .primitives;
            },
            .primitives => {
                const recording = cache.recording orelse return error.State;
                while (self.next_primitive < recording.count and self.reuse_layers[recording.commands[self.next_primitive].layer]) self.next_primitive += 1;
                if (self.next_primitive == recording.count) { self.phase = .linear_clear; return; }
                const first = recording.commands[self.next_primitive];
                var requests: [gfx.render_list_capacity]gfx.R4GfxRenderRequest = undefined;
                var grids: [gfx.render_list_capacity]gfx.R4GfxLogicalGrid = undefined;
                var mapped = false;
                var count: usize = 0;
                const limit: usize = if (self.batch_enabled) requests.len else 1;
                // Preserve painter order. Only adjacent commands with the same
                // resource/state owners share one all-or-nothing queue job.
                while (count < limit and self.next_primitive + count < recording.count) : (count += 1) {
                    const command = recording.commands[self.next_primitive + count];
                    if (command.layer != first.layer or command.texture != first.texture or command.over != first.over) break;
                    grids[count] = @bitCast(command.grid); mapped = mapped or command.grid.enabled != 0;
                    requests[count] = .{ .version = 1, .size = @sizeOf(gfx.R4GfxRenderRequest),
                        .source = if (command.texture) |texture| self.assets[texture].resource else empty,
                        .target = self.images[command.layer].resource,
                        .pipeline = if (command.texture == null) self.fill else if (command.over) self.over else self.blit,
                        .sampler = if (command.texture == null) empty else self.sampler,
                        .source_rect = rect(command.source), .target_rect = rect(command.target), .scissor = rect(command.scissor),
                        .color = command.color, .opacity = 255, .transfer = 0,
                        .dependency_count = if (count == 0) @intCast(dependencies.len) else 0,
                        .deadline_ns = self.deadline, .dependencies = if (count == 0) pointer else 0 };
                }
                const slot = try self.reserve();
                var handle: gfx.R4GfxJob = undefined;
                if (mapped) {
                    if (!self.grid_enabled) return error.Unsupported;
                    try accepted(self.client.render_submit_grid_list(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxRenderGridListRequest),
                        .commands = @intFromPtr(&requests), .grids = @intFromPtr(&grids), .count = @intCast(count), .reserved = 0 }, &handle));
                } else if (count == 1) try accepted(self.client.render_submit(self.device, &requests[0], &handle))
                else try accepted(self.client.render_submit_list(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxRenderListRequest),
                    .commands = @intFromPtr(&requests), .count = @intCast(count), .reserved = 0 }, &handle));
                try self.track(slot, handle, null, 0, 0);
                self.next_primitive += count; self.render_jobs +|= 1;
                self.primitive_jobs +|= 1; self.primitive_draws +|= count;
            },
            .upload => {
                if (self.stage_job != null) return error.Busy;
                while (self.next_image < cache.entries.len) : (self.next_image += 1) {
                    const index = self.next_image; const entry = &cache.entries[index]; const image_value = &self.images[index];
                    if (entry.external != null) continue;
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
                self.phase = .linear_clear;
            },
            .linear_clear => {
                if (self.clear_working and !self.replace_background) {
                    const slot = try self.reserve();
                    var handle: gfx.R4GfxJob = undefined;
                    try accepted(self.client.render_submit(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxRenderRequest),
                        .source = empty, .target = self.working().resource, .pipeline = self.fill, .sampler = empty,
                        .source_rect = std.mem.zeroes(gfx.R4GfxSignedRect), .target_rect = rect(self.encode_area), .scissor = rect(self.encode_area),
                        .color = 0, .opacity = 255, .transfer = 0, .dependency_count = @intCast(dependencies.len), .deadline_ns = self.deadline, .dependencies = pointer }, &handle));
                    try self.track(slot, handle, null, 0, 0);
                    self.render_jobs +|= 1;
                }
                self.phase = .draw;
            },
            .draw => {
                while (self.next_command < cache.command_count and
                    geometry.intersect(cache.commands[self.next_command].scissor, self.encode_area) == null) self.next_command += 1;
                if (self.next_command == cache.command_count) { self.phase = .encode; return; }
                const slot = try self.reserve();
                const command = cache.commands[self.next_command]; const entry = &cache.entries[command.entry];
                const bounds = entry.bounds; const clip = geometry.intersect(command.scissor, self.encode_area) orelse return error.State;
                if (entry.external) |frame| {
                    try self.drawExternal(cache, command.entry, frame, clip, slot);
                    self.next_command += 1; self.render_jobs +|= 1;
                    return;
                }
                var handle: gfx.R4GfxJob = undefined;
                const request: gfx.R4GfxRenderRequest = .{ .version = 1, .size = @sizeOf(gfx.R4GfxRenderRequest),
                    .source = self.images[command.entry].color_view, .target = self.working().resource,
                    .pipeline = if (self.replace_background and self.next_command == 0) self.blit else self.over, .sampler = self.sampler,
                    .source_rect = .{ .x = clip.x - bounds.x, .y = clip.y - bounds.y, .width = @intCast(clip.w), .height = @intCast(clip.h) },
                    .target_rect = .{ .x = clip.x, .y = clip.y, .width = @intCast(clip.w), .height = @intCast(clip.h) },
                    .scissor = .{ .x = clip.x, .y = clip.y, .width = @intCast(clip.w), .height = @intCast(clip.h) },
                    .color = 0, .opacity = 255, .transfer = if (self.hdr_frame) gfx.render_transfer_identity else gfx.render_transfer_srgb_decode,
                    .dependency_count = @intCast(dependencies.len), .deadline_ns = self.deadline, .dependencies = pointer };
                if (self.hdr_frame) {
                    try accepted(self.colors.color_render_submit(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxRenderListRequest),
                        .commands = @intFromPtr(&request), .count = 1, .reserved = 0 }, gfx.color_transform_relative_white, &handle));
                } else try accepted(self.client.render_submit(self.device, &request, &handle));
                try self.track(slot, handle, null, 0, 0);
                self.next_command += 1; self.render_jobs +|= 1;
            },
            .encode => {
                const slot = try self.reserve();
                var handle: gfx.R4GfxJob = undefined;
                const area = rect(self.encode_area);
                const request: gfx.R4GfxRenderRequest = .{ .version = 1, .size = @sizeOf(gfx.R4GfxRenderRequest),
                    .source = self.working().resource, .target = self.output().resource, .pipeline = self.blit, .sampler = self.sampler,
                    .source_rect = area, .target_rect = area, .scissor = area, .color = 0, .opacity = 255,
                    .transfer = if (self.color_output or self.hdr_frame) gfx.render_transfer_identity else gfx.render_transfer_srgb_encode,
                    .dependency_count = @intCast(dependencies.len), .deadline_ns = self.deadline, .dependencies = pointer };
                if (self.color_output or self.hdr_frame) {
                    try accepted(self.colors.color_render_submit(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxRenderListRequest),
                        .commands = @intFromPtr(&request), .count = 1, .reserved = 0 },
                        gfx.color_transform_output | gfx.color_transform_dither |
                            @as(u32, if (self.hdr_frame) 0 else gfx.color_transform_relative_white), &handle));
                } else try accepted(self.client.render_submit(self.device, &request, &handle));
                try self.track(slot, handle, null, 0, 0);
                self.render_jobs +|= 1;
                self.phase = .present;
            },
            .present => {
                if (self.chain.slot != 0) {
                    const acquired = self.acquired orelse return error.State;
                    try accepted(self.client.swapchain_present(self.device, &self.chain, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxSwapchainPresent),
                        .frame = acquired, .render_job = if (self.last) |index| self.jobs[index].?.handle else std.mem.zeroes(gfx.R4GfxJob),
                        .deadline_ns = self.deadline, .intent = self.present_intent, .blockers = self.present_blockers }));
                    self.acquired = null; self.phase = .drain;
                    return;
                }
                const slot = try self.reserve();
                var handle: gfx.R4GfxJob = undefined;
                try accepted(self.client.image_present(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxImagePresentRequest),
                    .source = self.output().resource, .frame_key = self.frame, .deadline_ns = self.deadline,
                    .dependency_count = @intCast(dependencies.len), .dependencies = pointer, .reserved = 0 }, &handle));
                try self.track(slot, handle, null, 0, 0);
                self.present_fence = self.jobs[slot].?.fence;
                self.phase = .drain;
            },
            else => return error.State,
        }
    }
    fn drawExternal(self: *Engine, cache: *const layers.Cache, index: usize, frame: *const window_image.Frame, clip: surface.Rect, slot: usize) Error!void {
        const entry = &cache.entries[index];
        const view = cache.view orelse geometry.topology.Viewport{ .pixel_w = @intCast(cache.screen.w), .pixel_h = @intCast(cache.screen.h) };
        const bounds = entry.capture_bounds;
        const grid: gfx.R4GfxLogicalGrid = .{ .enabled = 1, .rotation = @intFromEnum(view.rotation), .scale = view.scale,
            .pixel_width = view.pixel_w, .pixel_height = view.pixel_h, .target_x = 0, .target_y = 0, .reserved = 0,
            .viewport_x = std.math.cast(i32, @as(i64, bounds.x) - view.origin.x) orelse return error.Limit,
            .viewport_y = std.math.cast(i32, @as(i64, bounds.y) - view.origin.y) orelse return error.Limit,
            .viewport_width = @intCast(bounds.w), .viewport_height = @intCast(bounds.h),
            .guest_width = frame.message.descriptor.width, .guest_height = frame.message.descriptor.height, .source_x = 0, .source_y = 0 };
        var dependencies: [2]gfx.R4GfxCopyFence = undefined;
        var count: u32 = 0;
        for (self.dependency()) |value| { dependencies[count] = value; count += 1; }
        const ready = frame.message.ready;
        const producer: gfx.R4GfxCopyFence = .{ .slot = ready.slot, .adapter_id = ready.adapter_id, .timeline = ready.timeline,
            .point = ready.point, .device_generation = ready.device_generation, .reset_generation = ready.reset_generation };
        if (count == 0 or !std.meta.eql(dependencies[0], producer)) { dependencies[count] = producer; count += 1; }
        const request: gfx.R4GfxRenderRequest = .{ .version = 1, .size = @sizeOf(gfx.R4GfxRenderRequest),
            .source = self.images[index].resource, .target = self.working().resource, .pipeline = self.over, .sampler = self.sampler,
            .source_rect = .{ .x = 0, .y = 0, .width = frame.message.descriptor.width, .height = frame.message.descriptor.height },
            .target_rect = rect(entry.bounds), .scissor = rect(clip), .color = 0, .opacity = 255, .transfer = gfx.render_transfer_identity,
            .dependency_count = count, .dependencies = @intFromPtr(&dependencies), .deadline_ns = self.deadline };
        var handle: gfx.R4GfxJob = undefined;
        try accepted(self.colors.color_render_submit_grid(self.device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxRenderGridListRequest),
            .commands = @intFromPtr(&request), .grids = @intFromPtr(&grid), .count = 1, .reserved = 0 }, if (window_color.absolute(frame.description())) 0 else gfx.color_transform_relative_white, &handle));
        self.external_last[index] = slot;
        try self.track(slot, handle, null, 0, 0);
    }
    fn stage(self: *Engine, entry: *const layers.Entry, area: surface.Rect) Error!void {
        return self.stagePixels(entry.pixels, @intCast(entry.bounds.w), @intCast(entry.bounds.h), entry.generation,
            .{ .x = area.x - entry.bounds.x, .y = area.y - entry.bounds.y, .w = area.w, .h = area.h });
    }
    fn stagePixels(self: *Engine, pixels_value: []const u32, width: u32, height: u32, generation: u64, area: surface.Rect) Error!void {
        var desc = descriptor(gfx.resource_image); desc.source_kind = gfx.source_borrow_cpu; desc.source_generation = generation;
        desc.image = .{ .cpu_address = @intFromPtr(pixels_value.ptr), .byte_length = pixels_value.len * 4,
            .pitch = @as(u64, width) * 4, .width = width, .height = height, .format = gfx.format_argb8888, .reserved = 0 };
        var source: gfx.R4GfxResource = undefined;
        try accepted(self.client.resource_create(self.device, &desc, &source));
        defer _ = self.client.resource_release(self.device, &source);
        const command: gfx.R4GfxDraw = .{ .source = source, .target = self.staging.resource, .pipeline = self.blit, .sampler = self.sampler,
            .source_rect = .{ .x = @intCast(area.x), .y = @intCast(area.y), .width = @intCast(area.w), .height = @intCast(area.h) },
            .target_rect = .{ .x = 0, .y = 0, .width = @intCast(area.w), .height = @intCast(area.h) }, .color = 0, .opacity = 255 };
        var stats: gfx.R4GfxRenderStats = undefined;
        const pixels = @as(u64, @intCast(area.w)) * @as(u64, @intCast(area.h));
        if (pixels > gfx.render_max_pixels) return error.Limit;
        try accepted(self.client.render(self.device, &.{ .commands = @intFromPtr(&command), .command_count = 1, .flags = 0, .pixel_budget = pixels }, &stats));
    }
    pub fn close(self: *Engine) Error!void {
        if (self.active() or !self.drained() or self.readback_pin.slot != 0 or self.external_receipts != 0) return error.Busy;
        try self.closeChain();
        for (&self.images) |*value| try self.releaseImage(value);
        for (&self.assets) |*value| try self.releaseImage(value);
        for (&self.outputs) |*value| try self.releaseImage(value);
        for (&self.workings) |*value| try self.releaseImage(value);
        try self.releaseImage(&self.staging);
        for ([_]*gfx.R4GfxResource{ &self.over, &self.blit, &self.fill, &self.sampler }) |handle| if (handle.slot != 0) {
            try accepted(self.client.resource_release(self.device, handle)); handle.* = empty;
        };
        if (self.snapshot_allocator) |allocator| allocator.free(self.snapshot);
        self.snapshot = &.{}; self.snapshot_count = 0; self.snapshot_allocator = null;
    }
    fn closeChain(self: *Engine) Error!void {
        if (self.readback_pin.slot != 0) return error.Busy;
        if (self.chain.slot == 0) return;
        try accepted(self.client.swapchain_close(self.device, &self.chain));
        self.chain = std.mem.zeroes(gfx.R4GfxSwapchain); self.presentation = null; self.chain_status = null;
        self.acquired = null; self.chain_frames = @splat(0); self.reported = @splat(false); self.output_index = 0;
        self.chain_closing = false;
        self.invalidate();
    }
};
fn rect(value: surface.Rect) gfx.R4GfxSignedRect { return .{ .x = value.x, .y = value.y, .width = @intCast(value.w), .height = @intCast(value.h) }; }
fn descriptor(kind: u32) gfx.R4GfxResourceDesc {
    var desc = std.mem.zeroes(gfx.R4GfxResourceDesc);
    desc.version = 1; desc.size = @sizeOf(gfx.R4GfxResourceDesc); desc.kind = kind;
    return desc;
}
fn accepted(status: i32) Error!void {
    return switch (status) { gfx.status_ok => {}, gfx.status_busy, gfx.status_occluded => error.Busy,
        gfx.status_stale, gfx.status_suboptimal, gfx.status_lost => error.Stale,
        gfx.status_unsupported, gfx.status_unavailable => error.Unsupported, gfx.status_limit => error.Limit, else => error.Graphics };
}
