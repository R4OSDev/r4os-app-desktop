//! One stable Desktop-owned consumer lease. Its immutable BO metadata may be
//! captured by several outputs; counters and receipts change on the main task.
const std = @import("std");
const r4os = @import("r4os");
const gfx = @import("r4gfx");
const a = r4os.abi;
/// The compositor uses the same BO and fence owners for CPU and GPU images.
pub const Memory = struct {
    draw: r4os.r4draw.Context,
    pub fn import(self: @This(), source: *const a.GfxBufferHandle, out: *a.GfxBufferReference) i32 { return self.draw.buffers().import(source, out); }
    pub fn describe(self: @This(), reference: *const a.GfxBufferHandle, out: *a.GfxBufferDescriptor) i32 { return self.draw.buffers().describe(reference, out); }
    pub fn release(self: @This(), reference: *const a.GfxBufferHandle) i32 { return self.draw.buffers().release(reference); }
    pub fn map(self: @This(), reference: *const a.GfxBufferHandle, bytes: u64, out: *a.GfxBufferMap) i32 { return self.draw.buffers().map(reference, a.gfx_buffer_map_read, 0, bytes, out); }
    pub fn unmap(self: @This(), lease: *const a.GfxBufferHandle) i32 { return self.draw.buffers().unmap(lease); }
    pub fn query(self: @This(), fence: *const a.GfxFence, out: *a.GfxFenceStatus) i32 { return self.draw.queues().query(fence, out); }
};
pub const Receipt = struct {
    client: *const gfx.DeviceV1Client,
    device: *const gfx.R4GfxDevice,
    owner_holds: *usize,
    job: gfx.R4GfxJob,
    fence: gfx.R4GfxCopyFence,
    result: u32,
    fn release(self: *const Receipt) bool {
        if (self.client.job_release(self.device, &self.job) != gfx.status_ok) return false;
        std.debug.assert(self.owner_holds.* != 0);
        self.owner_holds.* -= 1;
        return true;
    }
};
pub const Frame = struct {
    message: a.WindowGraphicsFrame = .{},
    reference: a.GfxBufferReference = .{},
    readers: usize = 0,
    retired: bool = false,
    failed: bool = false,
    receipt: ?Receipt = null,
    mapping: a.GfxBufferMap = .{},
    cpu_consumed: bool = false,

    /// The receiver owns an independent import before publishing a front.
    /// Partial failure retains any returned handle for the same close path.
    pub fn open(self: *Frame, memory: anytype, message: a.WindowGraphicsFrame) !void {
        if (self.reference.reference.id != 0 or self.readers != 0 or self.receipt != null or self.mapping.lease.id != 0) return error.Busy;
        if (message.version != 1 or message.size != @sizeOf(a.WindowGraphicsFrame) or message.surface.serial == 0 or
            message.chain == 0 or message.acquire_token == 0 or message.descriptor.width == 0 or message.descriptor.height == 0 or
            message.descriptor.width > 32768 or message.descriptor.height > 32768 or
            message.format.format != message.descriptor.format or message.format.reserved != 0 or
            message.ready.timeline == 0 or message.ready.point == 0) return error.Invalid;
        self.* = .{ .message = message };
        if (memory.import(&message.source.reference, &self.reference) != a.gfx_buffer_result_ok) return error.Graphics;
        var actual: a.GfxBufferDescriptor = .{};
        if (self.reference.flags != 0 or !std.meta.eql(self.reference.buffer, message.source.buffer) or
            std.meta.eql(self.reference.reference, message.source.reference) or
            memory.describe(&self.reference.reference, &actual) != a.gfx_buffer_result_ok or
            !std.meta.eql(actual, message.descriptor)) return error.Stale;
    }
    pub fn isCpu(self: *const Frame) bool { return self.message.ready.adapter_id == 0; }
    /// Never block the desktop on a producer. A pending frame keeps the
    /// previous front visible; ordinary read mapping then excludes writers.
    pub fn prepareCpu(self: *Frame, memory: anytype) !bool {
        if (!self.isCpu()) return true;
        if (self.mapping.lease.id != 0) return true;
        const desc = self.message.descriptor;
        if (desc.location != a.gfx_buffer_location_system or desc.modifier != 0 or desc.plane_count != 1 or
            desc.usage & a.gfx_buffer_usage_cpu_read == 0 or desc.plane_offsets[0] != 0 or
            (desc.format != gfx.format_xrgb8888 and desc.format != gfx.format_argb8888) or
            desc.plane_pitches[0] < @as(u64, desc.width) * 4 or
            desc.byte_length < try std.math.mul(u64, desc.plane_pitches[0], desc.height)) return error.Invalid;
        var status: a.GfxFenceStatus = .{};
        if (memory.query(&self.message.ready, &status) != a.gfx_queue_ok or status.version != 1 or
            status.size != @sizeOf(a.GfxFenceStatus) or !std.meta.eql(status.fence, self.message.ready) or
            status.milestone != a.gfx_queue_milestone_cpu_stores) return error.Graphics;
        if (status.result == a.gfx_queue_result_pending) return false;
        if (status.result != a.gfx_queue_result_complete) return error.Graphics;
        const rc = memory.map(&self.reference.reference, desc.byte_length, &self.mapping);
        if (rc == a.gfx_buffer_error_busy) return false;
        if (rc != a.gfx_buffer_result_ok) return error.Graphics;
        if (self.mapping.version != 1 or self.mapping.size != @sizeOf(a.GfxBufferMap) or self.mapping.reserved0 != 0 or
            self.mapping.lease.id == 0 or self.mapping.lease.generation == 0 or self.mapping.lease.reserved0 != 0 or
            self.mapping.cpu_address == 0 or self.mapping.byte_length < desc.byte_length or
            self.mapping.cpu_address > std.math.maxInt(u64) - desc.byte_length) return error.Invalid;
        return true;
    }
    pub fn cpuImage(self: *const Frame) ?gfx.R4GfxColorImage {
        if (!self.isCpu() or self.mapping.lease.id == 0) return null;
        const desc = self.message.descriptor;
        return .{ .version = 1, .size = @sizeOf(gfx.R4GfxColorImage),
            .image = .{ .cpu_address = self.mapping.cpu_address, .byte_length = self.mapping.byte_length,
                .pitch = desc.plane_pitches[0], .width = desc.width, .height = desc.height, .format = desc.format, .reserved = 0 },
            .description = self.description(), .profile = std.mem.zeroes(gfx.R4GfxColorProfile) };
    }
    pub fn releaseCpuMapping(self: *Frame, memory: anytype) bool {
        if (self.readers != 0) return false;
        if (self.mapping.lease.id != 0) {
            if (memory.unmap(&self.mapping.lease) != a.gfx_buffer_result_ok) return false;
            self.mapping = .{};
        }
        return true;
    }
    pub fn borrow(self: *Frame) !void {
        if (self.retired or self.failed or self.reference.reference.id == 0) return error.Stale;
        self.readers = try std.math.add(usize, self.readers, 1);
    }
    /// A successful return transfers the completed job to this frame. The
    /// originating worker remains alive until the eventual service ack.
    pub fn finish(self: *Frame, receipt: ?Receipt, failed: bool) bool {
        std.debug.assert(self.readers != 0);
        if (receipt) |next| {
            if (self.receipt) |old| if (!old.release()) return false;
            next.owner_holds.* += 1;
            self.receipt = next;
            if (next.result != a.gfx_queue_result_complete) self.failed = true;
        }
        self.failed = self.failed or failed;
        self.readers -= 1;
        return true;
    }
    pub fn consumerFence(self: *const Frame) a.GfxFence {
        const value = if (self.receipt) |receipt| receipt.fence else return .{};
        return .{ .slot = value.slot, .adapter_id = value.adapter_id, .timeline = value.timeline,
            .point = value.point, .device_generation = value.device_generation, .reset_generation = value.reset_generation };
    }
    /// Only the transport owner calls this after WINSVC ended its lease and
    /// borrowed fence metadata (or the service generation is confirmed dead).
    pub fn closeAcknowledged(self: *Frame, memory: anytype) bool {
        if (!self.releaseCpuMapping(memory)) return false;
        if (self.receipt) |receipt| {
            if (!receipt.release()) return false;
            self.receipt = null;
        }
        if (self.reference.reference.id != 0 and memory.release(&self.reference.reference) != a.gfx_buffer_result_ok) return false;
        self.* = .{};
        return true;
    }
    pub fn description(self: *const Frame) gfx.R4GfxColorDescription { return @bitCast(self.message.format.color); }
};
