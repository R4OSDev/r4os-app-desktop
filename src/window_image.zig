//! One stable Desktop-owned consumer lease. Its immutable BO metadata may be
//! captured by several outputs; counters and receipts change on the main task.
const std = @import("std");
const r4os = @import("r4os");
const gfx = @import("r4gfx");
const a = r4os.abi;
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

    /// The receiver owns an independent import before publishing a front.
    /// Partial failure retains any returned handle for the same close path.
    pub fn open(self: *Frame, memory: anytype, message: a.WindowGraphicsFrame) !void {
        if (self.reference.reference.id != 0 or self.readers != 0 or self.receipt != null) return error.Busy;
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
        if (self.readers != 0) return false;
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
