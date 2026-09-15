//! Desktop-owned R4GFX resources and bounded batches. Present stays in R4DRAW.
//! Unsupported primitives flush first and use the existing scene painter.
const std = @import("std");
const r4os = @import("r4os");
const gfx = @import("r4gfx");
const scene_buffer = @import("scene_buffer.zig");
const surface = @import("surface.zig");
const empty_resource = std.mem.zeroes(gfx.R4GfxResource);
const Raster = struct {
    lease: r4os.abi.GuiSharedRasterLease = .{},
    resource: gfx.R4GfxResource = empty_resource,
};

pub const Renderer = struct {
    allocator: std.mem.Allocator,
    client: gfx.DeviceV1Client,
    colors: gfx.ColorV1Client,
    storage: []align(gfx.device_storage_alignment) u8,
    device: gfx.R4GfxDevice,
    fill_pipeline: gfx.R4GfxResource = empty_resource,
    target: gfx.R4GfxResource = empty_resource,
    scene: ?*scene_buffer.SceneBuffer = null,
    generation: u64 = 0,
    commands: [128]gfx.R4GfxDraw = undefined,
    count: u32 = 0,
    pixels: u64 = 0,
    batches: u64 = 0,
    fills: u64 = 0,
    rejected_batches: u64 = 0,
    // Exact leases distinguish overlapping active/staging frame snapshots.
    // R4GFX reuses their common immutable raster generation and BO import.
    rasters: [128]Raster = @splat(.{}),

    pub fn create(allocator: std.mem.Allocator, raw: *const r4os.abi.R4XStartContext) ?*Renderer {
        return createForAdapter(allocator, raw, 0);
    }
    pub fn createForAdapter(allocator: std.mem.Allocator, raw: *const r4os.abi.R4XStartContext, adapter: u32) ?*Renderer {
        const client = gfx.DeviceV1Client.init(raw) catch return failed(raw, "device-interface", 0);
        const colors = gfx.ColorV1Client.init(raw) catch return failed(raw, "color-interface", 0);
        const bytes = client.storage_size();
        if (bytes == 0 or bytes > std.math.maxInt(usize)) return failed(raw, "storage-size", 0);
        const storage = allocator.alignedAlloc(u8, .fromByteUnits(gfx.device_storage_alignment), @intCast(bytes)) catch return failed(raw, "storage-allocation", 0);
        @memset(storage, 0);
        const self = allocator.create(Renderer) catch { allocator.free(storage); return failed(raw, "renderer-allocation", 0); };
        var device: gfx.R4GfxDevice = undefined;
        const open_rc = client.device_open(&.{ .version = 1, .size = @sizeOf(gfx.R4GfxDeviceConfig), .storage_address = @intFromPtr(storage.ptr),
            .storage_bytes = storage.len, .start_context = @intFromPtr(raw), .preferred_adapter = adapter, .flags = 0 }, &device);
        if (open_rc != gfx.status_ok)
        { allocator.destroy(self); allocator.free(storage); return failed(raw, "device-open", open_rc); }
        self.* = .{ .allocator = allocator, .client = client, .colors = colors, .storage = storage, .device = device };
        var descriptor = std.mem.zeroes(gfx.R4GfxResourceDesc);
        descriptor.version = 1; descriptor.size = @sizeOf(gfx.R4GfxResourceDesc);
        descriptor.kind = gfx.resource_pipeline; descriptor.operation = gfx.render_operation_fill;
        const pipeline_rc = client.resource_create(&device, &descriptor, &self.fill_pipeline);
        if (pipeline_rc != gfx.status_ok) {
            self.destroy(); return failed(raw, "fill-pipeline", pipeline_rc);
        }
        return self;
    }
    fn failed(raw: *const r4os.abi.R4XStartContext, stage: []const u8, result: i32) ?*Renderer {
        const bundle = r4os.program.bundleValueFromR4XStart(raw) orelse return null;
        const sys = r4os.r4sys.Context.init(&bundle);
        sys.write("R4DESK gfx: initialization failed stage="); sys.write(stage);
        sys.write(" result="); sys.printI32(result); sys.println("");
        return null;
    }
    pub fn destroy(self: *Renderer) void {
        self.end();
        // The provider owns retryable cleanup. Never free its storage while
        // a queue receipt or a failed physical release remains there.
        if (self.client.device_close(&self.device) != gfx.status_ok) return;
        const allocator = self.allocator;
        allocator.free(self.storage);
        allocator.destroy(self);
    }
    pub fn begin(self: *Renderer, scene: *scene_buffer.SceneBuffer) void {
        self.end();
        const pixels = scene.pixels orelse return;
        if (scene.width <= 0 or scene.height <= 0 or scene.premultiplied or scene.origin_x != 0 or scene.origin_y != 0 or scene.layer_hook != null) return;
        const generation = std.math.add(u64, self.generation, 1) catch return;
        var descriptor = std.mem.zeroes(gfx.R4GfxResourceDesc);
        descriptor.version = 1; descriptor.size = @sizeOf(gfx.R4GfxResourceDesc);
        descriptor.kind = gfx.resource_image; descriptor.flags = gfx.image_target;
        descriptor.source_kind = gfx.source_borrow_cpu; descriptor.source_generation = generation;
        descriptor.image = .{ .cpu_address = @intFromPtr(pixels.ptr), .byte_length = pixels.len * @sizeOf(u32),
            .pitch = @as(u64, @intCast(scene.width)) * @sizeOf(u32), .width = @intCast(scene.width), .height = @intCast(scene.height),
            .format = gfx.format_xrgb8888, .reserved = 0 };
        if (self.client.resource_create(&self.device, &descriptor, &self.target) != gfx.status_ok) return;
        self.generation = generation;
        self.scene = scene;
        scene.render_hook = .{ .context = @intFromPtr(self), .fill = enqueueFill, .flush = flushHook };
    }
    pub fn end(self: *Renderer) void {
        self.flush();
        if (self.scene) |scene| scene.render_hook = null;
        self.scene = null;
        if (self.target.slot != 0) {
            _ = self.client.resource_release(&self.device, &self.target);
            self.target = empty_resource;
        }
    }
    fn enqueueFill(context: usize, rect: surface.Rect, color: u32) bool {
        const self: *Renderer = @ptrFromInt(context);
        const pixels = @as(u64, @intCast(rect.w)) * @as(u64, @intCast(rect.h));
        if (self.count == self.commands.len or pixels > gfx.render_max_pixels - self.pixels) self.flush();
        if (pixels > gfx.render_max_pixels) return false;
        var command = std.mem.zeroes(gfx.R4GfxDraw);
        command.target = self.target; command.pipeline = self.fill_pipeline;
        command.target_rect = .{ .x = @intCast(rect.x), .y = @intCast(rect.y), .width = @intCast(rect.w), .height = @intCast(rect.h) };
        command.color = color;
        self.commands[self.count] = command;
        self.count += 1; self.pixels += pixels;
        return true;
    }
    fn flushHook(context: usize) void { const self: *Renderer = @ptrFromInt(context); self.flush(); }
    pub fn flush(self: *Renderer) void {
        if (self.count == 0) return;
        const scene = self.scene orelse unreachable;
        var stats: gfx.R4GfxRenderStats = undefined;
        const result = self.client.render(&self.device, &.{ .commands = @intFromPtr(&self.commands), .command_count = self.count,
            .flags = 0, .pixel_budget = self.pixels }, &stats);
        if (result == gfx.status_ok) {
            self.batches +|= 1; self.fills +|= stats.cpu.commands;
        } else {
            // These commands only fill a borrowed CPU target. Replaying an
            // already written fill is idempotent and preserves painter order.
            self.rejected_batches +|= 1;
            const hook = scene.render_hook;
            const clip = scene.paint_clip;
            scene.render_hook = null; scene.paint_clip = null;
            defer { scene.render_hook = hook; scene.paint_clip = clip; }
            for (self.commands[0..self.count]) |command| {
                const rect = command.target_rect;
                scene.fillRect(.{ .x = @intCast(rect.x), .y = @intCast(rect.y), .w = @intCast(rect.width), .h = @intCast(rect.height) }, command.color);
            }
        }
        self.count = 0; self.pixels = 0;
    }
    pub fn retainRaster(self: *Renderer, lease: *const r4os.abi.GuiSharedRasterLease) void {
        for (&self.rasters) |*entry| if (entry.lease.lease_token != 0 and std.meta.eql(entry.lease, lease.*)) return;
        const entry = for (&self.rasters) |*item| { if (item.lease.lease_token == 0) break item; } else return;
        var descriptor = std.mem.zeroes(gfx.R4GfxResourceDesc);
        descriptor.version = 1; descriptor.size = @sizeOf(gfx.R4GfxResourceDesc);
        descriptor.kind = gfx.resource_image; descriptor.source_kind = gfx.source_shared_raster;
        descriptor.source_address = @intFromPtr(lease);
        if (self.client.resource_create(&self.device, &descriptor, &entry.resource) == gfx.status_ok) entry.lease = lease.*;
    }
    pub fn releaseRaster(self: *Renderer, lease: *const r4os.abi.GuiSharedRasterLease) void {
        for (&self.rasters) |*entry| if (entry.lease.lease_token != 0 and std.meta.eql(entry.lease, lease.*)) {
            self.flush();
            if (self.client.resource_release(&self.device, &entry.resource) == gfx.status_ok) entry.* = .{};
            return;
        };
    }
    pub fn info(self: *Renderer) ?gfx.R4GfxDeviceInfo {
        var value: gfx.R4GfxDeviceInfo = undefined;
        if (self.client.device_refresh(&self.device, &value) != gfx.status_ok) return null;
        return value;
    }
};
