//! Portable output owner. Each output has its own common BO pool and native
//! presentation queue. A busy scanout never holds another output's image.
const std = @import("std");
const r4os = @import("r4os");
const gfx = @import("r4gfx");
const a = r4os.abi;
const geometry = @import("output_geometry.zig");
const scene_buffer = @import("scene_buffer.zig");
const renderer = @import("gfx_renderer.zig");
const composition = @import("composition_software.zig");
const catalog = @import("r4gfx_desktop_outputs");
const Profile = catalog.profiles.Owner(gfx);
const empty = std.mem.zeroes(gfx.R4GfxResource);
pub const Output = struct {
    graphics: *renderer.Renderer,
    draw: r4os.r4draw.Context,
    view: geometry.topology.Viewport,
    target: a.GfxOutputTarget,
    references: [3]a.GfxBufferReference = @splat(.{}),
    images: [3]gfx.R4GfxResource = @splat(empty),
    chain: gfx.R4GfxSwapchain = std.mem.zeroes(gfx.R4GfxSwapchain),
    acquired: ?gfx.R4GfxSwapchainFrame = null,
    mapping: a.GfxBufferMap = .{},
    scratch: []u8 = &.{},
    scene: scene_buffer.SceneBuffer = .{},
    color_composition: composition.Owner = .{},
    profile: ?Profile = null,
    count: u32 = 0,
    ready: bool = false,
    pending: bool = false,
    lost: bool = false,
    completed: u64 = 0,
    visible: u64 = 0,
    failed: u64 = 0,
    discarded: u64 = 0,
    prepare_stage: []const u8 = "presentation-info",
    prepare_error: ?anyerror = null,
    last_status: i32 = 0,
    last_submitted: ?u32 = null,
    pub fn setProfile(self: *Output, sys: anytype, choice: catalog.color_preferences.Choice) !void {
        if (self.profile != null or self.acquired != null or self.pending or self.completed != 0) return error.State;
        if (!choice.enabled()) return;
        self.profile = try Profile.openFile(self.graphics.allocator, self.graphics.colors, sys, choice.profilePath(), choice.intent, choice.flags);
    }

    pub fn create(allocator: std.mem.Allocator, raw: *const a.R4XStartContext, draw: r4os.r4draw.Context,
        view: geometry.topology.Viewport, target: a.GfxOutputTarget) ?*Output
    {
        const graphics = renderer.Renderer.createForAdapter(allocator, raw, target.adapter_id) orelse return null;
        const self = allocator.create(Output) catch { graphics.destroy(); return null; };
        self.* = .{ .graphics = graphics, .draw = draw, .view = view, .target = target };
        // Keep partial resources addressable until cleanup succeeds.
        self.prepare() catch |err| { self.lost = true; self.prepare_error = err; };
        return self;
    }
    fn prepare(self: *Output) !void {
        try self.view.validate();
        var info: gfx.R4GfxPresentationInfo = undefined;
        const client = &self.graphics.client; const device = &self.graphics.device;
        try self.accepted(client.presentation_info(device, self.target.head_id, &info), gfx.status_ok);
        if (info.display_generation != self.target.display_generation or info.width != self.view.pixel_w or info.height != self.view.pixel_h or
            info.flags & gfx.present_native == 0 or info.buffer_count < 2 or info.buffer_count > self.images.len) return error.Stale;
        const pitch = @as(u64, info.width) * 4;
        const bytes = pitch * info.height;
        if (bytes > 64 * 1024 * 1024) return error.Limit;
        self.count = info.buffer_count;
        for (0..self.count) |i| {
            self.prepare_stage = "image-allocation";
            try self.accepted(self.draw.gfxBufferCreate(&.{ .width = info.width, .height = info.height, .byte_length = bytes,
                .format = a.gfx_buffer_format_xrgb8888, .plane_count = 1, .plane_pitches = .{ pitch, 0, 0, 0 },
                .usage = a.gfx_buffer_usage_cpu_read | a.gfx_buffer_usage_cpu_write | a.gfx_buffer_usage_transfer_source | a.gfx_buffer_usage_render }, &self.references[i]), a.gfx_buffer_result_ok);
            var desc = std.mem.zeroes(gfx.R4GfxResourceDesc);
            desc.version = 1; desc.size = @sizeOf(gfx.R4GfxResourceDesc); desc.kind = gfx.resource_image;
            desc.flags = gfx.image_target; desc.source_kind = gfx.source_import_buffer;
            desc.source_address = @intFromPtr(&self.references[i].reference);
            self.prepare_stage = "image-import";
            try self.accepted(client.resource_create(device, &desc, &self.images[i]), gfx.status_ok);
        }
        self.prepare_stage = "swapchain-open";
        try self.accepted(client.swapchain_open(device, &.{ .version = 1, .size = @sizeOf(gfx.R4GfxSwapchainDesc),
            .head_id = self.target.head_id, .policy = gfx.present_policy_fifo, .flags = 0,
            .display_generation = info.display_generation, .count = self.count, .images = @intFromPtr(&self.images) }, &self.chain), gfx.status_ok);
        self.ready = true;
    }
    fn accepted(self: *Output, status: i32, success: i32) !void {
        self.last_status = status;
        if (status != success) return error.Graphics;
    }
    pub fn poll(self: *Output) void {
        if (self.chain.slot == 0) return;
        var status: gfx.R4GfxSwapchainStatus = undefined;
        const client = &self.graphics.client; const device = &self.graphics.device;
        const rc = client.swapchain_poll(device, &self.chain, &status);
        if (rc != gfx.status_ok) { if (rc != gfx.status_busy) self.lost = true; return; }
        if (status.life >= 2) self.lost = true;
        self.pending = false;
        for ([_]gfx.R4GfxSwapchainFrameStatus{ status.frame0, status.frame1, status.frame2 }) |frame| {
            if (frame.phase == 0 or frame.phase == 1) continue;
            if (frame.phase != 4 or frame.held_flags != 0) { self.pending = true; continue; }
            if (client.swapchain_release(device, &self.chain, &frame.frame) != gfx.status_ok) { self.pending = true; continue; }
            if (frame.result == 1 or frame.result == 2) self.completed +|= 1
            else if (frame.result == 3) self.discarded +|= 1
            else { self.failed +|= 1; self.lost = true; }
            if (frame.result == 1 and frame.visible_ns != 0) self.visible +|= 1;
        }
    }
    pub fn begin(self: *Output, input_ns: u64) ?*scene_buffer.SceneBuffer {
        if (!self.ready or self.lost or self.acquired != null or self.mapping.lease.id != 0) return null;
        self.poll();
        if (self.lost) return null;
        var frame: gfx.R4GfxSwapchainFrame = undefined;
        const rc = self.graphics.client.swapchain_acquire(&self.graphics.device, &self.chain, input_ns, &frame);
        if (rc != gfx.status_ok) {
            if (rc != gfx.status_busy and rc != gfx.status_occluded) self.lost = true;
            return null;
        }
        if (frame.slot == 0 or frame.slot > self.count or !std.meta.eql(frame.image, self.images[frame.slot - 1])) { self.lost = true; return null; }
        self.acquired = frame;
        const bytes = @as(u64, self.view.pixel_w) * self.view.pixel_h * 4;
        if (self.draw.gfxBufferMap(&self.references[frame.slot - 1].reference, a.gfx_buffer_map_write, 0, bytes, &self.mapping) != a.gfx_buffer_result_ok) {
            self.abandon(); return null;
        }
        const bounds = geometry.logical(self.view) catch { self.abandon(); return null; };
        var storage: []u8 = @as([*]u8, @ptrFromInt(self.mapping.cpu_address))[0..@intCast(bytes)];
        if (self.profile != null or self.view.rotation != .normal or self.view.scale != 120) {
            const length = scene_buffer.SceneBuffer.requiredBytes(bounds.w, bounds.h) orelse { self.abandon(); return null; };
            if (length > 64 * 1024 * 1024) { self.abandon(); self.lost = true; return null; }
            if (self.scratch.len < length) {
                const replacement = self.graphics.allocator.alloc(u8, length) catch { self.abandon(); return null; };
                self.graphics.allocator.free(self.scratch); self.scratch = replacement;
            }
            storage = self.scratch;
        }
        if (!self.scene.attach(storage, bounds.w, bounds.h)) { self.abandon(); return null; }
        self.scene.origin_x = bounds.x; self.scene.origin_y = bounds.y;
        self.color_composition.begin(self.graphics.allocator, &self.scene) catch { self.abandon(); return null; };
        return &self.scene;
    }
    pub fn submit(self: *Output, deadline: u64) bool {
        const frame = self.acquired orelse return false;
        if (self.scene.failure != null or self.mapping.lease.id == 0) { self.abandon(); return false; }
        self.color_composition.finish(&self.graphics.colors, &self.scene) catch { self.abandon(); return false; };
        if (self.profile != null or self.view.rotation != .normal or self.view.scale != 120) {
            const pixels: [*]u32 = @ptrFromInt(self.mapping.cpu_address);
            transform(self.view, self.scene.pixels.?, @intCast(self.scene.width), pixels[0..@as(usize, self.view.pixel_w) * self.view.pixel_h]);
        }
        if (self.profile) |*profile| {
            const pixels: [*]u32 = @ptrFromInt(self.mapping.cpu_address);
            profile.applySdr(pixels[0..@as(usize, self.view.pixel_w) * self.view.pixel_h], self.view.pixel_w, self.view.pixel_h) catch {
                self.abandon(); return false;
            };
        }
        if (self.draw.gfxBufferUnmap(&self.mapping.lease) != a.gfx_buffer_result_ok) {
            // Preserve the mapping and acquired image for retained teardown.
            // Leaving this live would prevent every subsequent begin().
            self.lost = true; return false;
        }
        self.mapping = .{}; self.scene.reset();
        const rc = self.graphics.client.swapchain_present(&self.graphics.device, &self.chain,
            &.{ .version = 1, .size = @sizeOf(gfx.R4GfxSwapchainPresent), .frame = frame,
                .render_job = std.mem.zeroes(gfx.R4GfxJob), .deadline_ns = deadline, .intent = 0, .blockers = gfx.present_block_cursor });
        if (rc != gfx.status_ok) {
            self.abandon();
            if (rc != gfx.status_busy and rc != gfx.status_occluded) self.lost = true;
            return false;
        }
        self.acquired = null; self.pending = true;
        self.last_submitted = frame.slot - 1;
        return true;
    }
    fn abandon(self: *Output) void {
        self.color_composition.cancel(&self.scene);
        if (self.mapping.lease.id != 0) {
            if (self.draw.gfxBufferUnmap(&self.mapping.lease) != a.gfx_buffer_result_ok) { self.lost = true; return; }
            self.mapping = .{};
        }
        self.scene.reset();
        if (self.acquired) |frame| {
            if (self.graphics.client.swapchain_release(&self.graphics.device, &self.chain, &frame) != gfx.status_ok) { self.lost = true; return; }
            self.acquired = null;
        }
    }
    /// False leaves every retained owner in place for the next desktop cycle.
    pub fn destroy(self: *Output) bool {
        self.abandon();
        if (self.mapping.lease.id != 0 or self.acquired != null) return false;
        if (self.chain.slot != 0) {
            if (self.graphics.client.swapchain_close(&self.graphics.device, &self.chain) != gfx.status_ok) return false;
            self.chain = std.mem.zeroes(gfx.R4GfxSwapchain);
        }
        for (&self.images, &self.references) |*image, *reference| {
            if (image.slot != 0) {
                if (self.graphics.client.resource_release(&self.graphics.device, image) != gfx.status_ok) return false;
                image.* = empty;
            }
            if (reference.reference.id != 0) {
                if (self.draw.gfxBufferRelease(&reference.reference) != a.gfx_buffer_result_ok) return false;
                reference.* = .{};
            }
        }
        const allocator = self.graphics.allocator;
        if (self.profile) |*profile| {
            if (!profile.close()) return false;
            self.profile = null;
        }
        self.color_composition.deinit();
        allocator.free(self.scratch); self.graphics.destroy(); allocator.destroy(self);
        return true;
    }
};
pub fn transform(view: geometry.topology.Viewport, source: []const u32, stride: usize, output: []u32) void {
    for (0..view.pixel_h) |y| for (0..view.pixel_w) |x| {
        const oriented: [2]usize = switch (view.rotation) {
            .normal => .{ x, y }, .clockwise90 => .{ view.pixel_h - 1 - y, x },
            .clockwise180 => .{ view.pixel_w - 1 - x, view.pixel_h - 1 - y }, .clockwise270 => .{ y, view.pixel_w - 1 - x },
        };
        const sx = ((2 * oriented[0] + 1) * 120) / (2 * view.scale);
        const sy = ((2 * oriented[1] + 1) * 120) / (2 * view.scale);
        output[y * view.pixel_w + x] = source[sy * stride + sx];
    };
}
