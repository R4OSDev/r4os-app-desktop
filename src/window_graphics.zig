//! Main-task owner of WINSVC GPU surfaces. Stable frames outlive every
//! captured output; one immutable pending request per surface handles retries.
const std = @import("std");
const a = @import("r4os").abi;
const image = @import("window_image.zig");
pub const Spec = struct { owner: a.ProgramProcessHandle, config: a.WindowGraphicsConfig, consumer_ready: bool = true };
const State = enum { empty, taking, live, returning, acknowledged };
const Slot = struct { frame: image.Frame = .{}, state: State = .empty };
pub const Window = struct {
    owner: a.ProgramProcessHandle = .{},
    surface: a.WindowGraphicsSurface = .{},
    config: a.WindowGraphicsConfig = .{},
    closing: bool = false,
    removed: bool = false,
    ended: bool = false,
    retry_take: bool = false,
    serial: u64 = 0,
    publication: ?a.WindowGraphicsPublication = null,
    request: ?a.WindowGraphicsConsumer = null,
    request_slot: usize = 0,
    slots: [3]Slot = @splat(.{}),
    current: ?usize = null,

    pub fn front(self: *Window) ?*image.Frame {
        const index = self.current orelse return null;
        const frame = &self.slots[index].frame;
        return if (self.closing or frame.retired or frame.failed) null else frame;
    }
    pub fn retire(self: *Window) void {
        self.retry_take = false;
        self.current = null;
        for (&self.slots) |*slot| if (slot.state != .empty) { slot.frame.retired = true; };
    }
    /// A failed RPC proves no release. Only successful restart-cleanup, or
    /// confirmed death of this exact service generation, ends metadata loans.
    pub fn acknowledgeReset(self: *Window) void {
        self.retire(); self.closing = true; self.ended = true;
        self.publication = null; self.request = null;
        for (&self.slots) |*slot| if (slot.state != .empty) { slot.state = .acknowledged; };
    }
    pub fn disconnect(self: *Window) void { self.retire(); self.closing = true; }
    fn empty(self: *const Window) bool {
        for (&self.slots) |*slot| if (slot.state != .empty) return false;
        return true;
    }
    fn acknowledge(self: *Window, index: usize) void {
        self.slots[index].frame.retired = true; self.slots[index].state = .acknowledged;
        if (self.current == index) self.current = null;
    }
    fn nextRequest(self: *Window, action: u32, index: usize, desktop: a.ProgramProcessHandle) bool {
        self.serial = std.math.add(u64, self.serial, 1) catch { self.disconnect(); return false; };
        self.request_slot = index;
        self.request = .{ .desktop = desktop, .surface = self.surface, .action = action, .request_serial = self.serial };
        if (action == a.window_graphics_take) { self.slots[index].state = .taking; return true; }
        const frame = &self.slots[index].frame;
        self.request.?.chain = frame.message.chain;
        self.request.?.image_slot = frame.message.image_slot;
        self.request.?.acquire_token = frame.message.acquire_token;
        self.request.?.fence = frame.consumerFence();
        self.request.?.result = if (frame.failed) a.window_graphics_failed else a.window_graphics_ok;
        return true;
    }
    fn consume(self: *Window, transport: anytype, memory: anytype) void {
        const request = self.request orelse return;
        var reply: a.WindowGraphicsReply = .{};
        if (!transport.consumer(&request, &reply)) return;
        const index = self.request_slot;
        const slot = &self.slots[index];
        if (reply.result == a.window_graphics_ok) {
            if (!std.meta.eql(reply.surface, self.surface)) { self.disconnect(); transport.invalidate(); return; }
            if (request.action == a.window_graphics_take) {
                // Retain the lease even if metadata/import validation fails:
                // its exact chain/slot/token is needed for an explicit return.
                slot.frame.message = reply.frame;
                const valid = std.meta.eql(reply.frame.surface, self.surface) and reply.chain == reply.frame.chain and
                    reply.image_slot == reply.frame.image_slot and reply.acquire_token == reply.frame.acquire_token and
                    reply.frame.config_revision == self.config.revision and reply.frame.flags == 0;
                slot.state = .live;
                if (!valid) { slot.frame.failed = true; slot.frame.retired = true; self.disconnect(); }
                else slot.frame.open(memory, reply.frame) catch { slot.frame.failed = true; slot.frame.retired = true; };
                if (self.closing or self.removed or self.config.flags & a.window_graphics_visible == 0) slot.frame.retired = true;
                if (!slot.frame.failed and !slot.frame.retired) {
                    if (self.current) |old| self.slots[old].frame.retired = true;
                    self.current = index;
                }
            } else {
                if (reply.chain != request.chain or reply.image_slot != request.image_slot or reply.acquire_token != request.acquire_token) {
                    self.disconnect(); transport.invalidate(); return;
                }
                if (reply.flags & a.window_graphics_fence_released != 0) self.acknowledge(index)
                else slot.state = .returning;
            }
            self.request = null;
        } else if (reply.result == a.window_graphics_not_ready or reply.result == a.window_graphics_busy or reply.result == a.window_graphics_capacity) {
            // Negative replies did not mutate the broker. Preserve the exact
            // request for retry, except an empty poll which needs no pin.
            if (request.action == a.window_graphics_take) { slot.* = .{}; self.request = null; }
        } else if (reply.result == a.window_graphics_closed or
            (reply.result == a.window_graphics_stale and reply.surface.serial == 0)) {
            // Closed chain / absent exact surface cannot borrow this fence.
            // A stale serial on an existing surface is NOT this proof.
            if (request.action == a.window_graphics_take) { slot.* = .{}; self.disconnect(); }
            else self.acknowledge(index);
            self.request = null;
        } else if (request.action == a.window_graphics_take) {
            slot.* = .{}; self.request = null; self.disconnect();
        } else if (reply.result == a.window_graphics_device_lost and request.action == a.window_graphics_return) {
            // A rejected successful fence has not released the lease. Retry
            // as failure under a NEW serial, invalidating rather than recycling.
            slot.frame.failed = true; self.request = null;
        } else if (reply.result == a.window_graphics_device_lost and request.action == a.window_graphics_release_fence) {
            // retireConsumer invalidates the chain and ends its metadata loan.
            self.acknowledge(index); self.request = null;
        } else { self.disconnect(); transport.invalidate(); }
    }
    fn publish(self: *Window, transport: anytype) void {
        const request = self.publication orelse return;
        var reply: a.WindowGraphicsReply = .{};
        if (!transport.publish(&request, &reply)) return;
        if (reply.result == a.window_graphics_ok) {
            if (reply.surface.serial == 0 or reply.surface.window_id != request.window_id or
                !std.meta.eql(reply.surface.owner, request.owner) or !std.meta.eql(reply.surface.desktop, request.desktop) or
                (request.surface.serial != 0 and !std.meta.eql(reply.surface, request.surface))) { self.disconnect(); transport.invalidate(); return; }
            self.surface = reply.surface;
            if (request.action == a.window_graphics_remove) self.removed = true
            else self.config = request.config;
            self.publication = null;
        } else if (reply.result == a.window_graphics_closed or
            (reply.result == a.window_graphics_stale and reply.surface.serial == 0)) {
            // A removed surface may still own leased frames. Return them;
            // only an absent surface can end those leases immediately.
            if (reply.surface.serial == 0) self.acknowledgeReset()
            else { self.publication = null; self.removed = true; self.disconnect(); }
        } else if (reply.result == a.window_graphics_not_owner and self.surface.serial == 0) {
            // Initial publication failed before creating a surface (e.g. app
            // exited between enumeration and publication).
            self.publication = null; self.acknowledgeReset();
        } else if (reply.result != a.window_graphics_busy and reply.result != a.window_graphics_capacity and reply.result != a.window_graphics_not_ready) {
            self.disconnect(); transport.invalidate();
        }
    }
    pub fn poll(self: *Window, desktop: a.ProgramProcessHandle, window_id: u32, wanted: ?Spec, transport: anytype, memory: anytype) void {
        defer {
            var matches = false;
            if (wanted) |spec| {
                var expected = spec.config; var actual = self.config;
                expected.revision = 0; actual.revision = 0;
                matches = std.meta.eql(self.owner, spec.owner) and std.meta.eql(expected, actual);
            }
            if (!matches) self.retire();
        }
        if (self.surface.serial != 0 and transport.serviceDead(self.surface.service)) self.acknowledgeReset();
        if (self.current) |index| if (self.slots[index].frame.failed) self.retire();
        if (self.owner.instance_id != 0 and (wanted == null or !std.meta.eql(self.owner, wanted.?.owner))) self.disconnect();
        if (wanted) |spec| {
            var previous = self.config; var next = spec.config;
            previous.revision = 0; next.revision = 0;
            if (!std.meta.eql(previous, next)) self.retire();
        }
        for (&self.slots) |*slot| if (slot.state == .acknowledged and slot.frame.closeAcknowledged(memory)) { slot.* = .{}; };
        if (self.ended and self.empty()) self.* = .{};
        if (self.ended) return;
        // Never overtake an RPC whose reply may have been lost.
        if (self.request != null) { self.consume(transport, memory); return; }
        if (self.publication != null) { self.publish(transport); return; }
        // Return retired images before accepting more. The last real GPU
        // receipt stays resident until the broker's fence-release ack.
        for (&self.slots, 0..) |*slot, index| {
            if ((slot.state == .live and slot.frame.retired and slot.frame.readers == 0) or slot.state == .returning) {
                if (self.nextRequest(if (slot.state == .returning) a.window_graphics_release_fence else a.window_graphics_return, index, desktop))
                    self.consume(transport, memory);
                return;
            }
        }
        if (self.closing) {
            if (self.surface.serial != 0 and !self.removed) {
                self.publication = .{ .desktop = desktop, .owner = self.owner, .window_id = window_id,
                    .action = a.window_graphics_remove, .surface = self.surface };
                self.publish(transport);
            } else if (self.empty()) self.* = .{};
            return;
        }
        const spec = wanted orelse return;
        if (spec.owner.instance_id == 0 or spec.owner.generation == 0) return;
        var old = self.config; var next = spec.config;
        old.revision = 0; next.revision = 0;
        if (self.surface.serial == 0 or !std.meta.eql(old, next)) {
            self.owner = spec.owner;
            var old_geometry = old; var new_geometry = next;
            old_geometry.flags = 0; new_geometry.flags = 0;
            next.revision = if (self.surface.serial != 0 and std.meta.eql(old_geometry, new_geometry)) self.config.revision
                else std.math.add(u64, self.config.revision, 1) catch { self.disconnect(); return; };
            self.publication = .{ .desktop = desktop, .owner = spec.owner, .window_id = window_id,
                .action = a.window_graphics_publish, .surface = self.surface, .config = next };
            self.publish(transport); return;
        }
        if (self.config.flags & a.window_graphics_visible == 0) return;
        // FIFO frames cannot be replaced before the compositor has sampled
        // them. A fully occluded window naturally applies bounded backpressure.
        if (self.current) |current| if (self.slots[current].frame.receipt == null) return;
        // A queued image may have woken us before the output's next frame
        // deadline. Retry admission when it opens, even without another IPC
        // revision; the following empty Take returns to ordinary idle waits.
        self.retry_take = !spec.consumer_ready;
        if (self.retry_take) return;
        const index = for (&self.slots, 0..) |*slot, i| { if (slot.state == .empty) break i; } else return;
        if (self.nextRequest(a.window_graphics_take, index, desktop)) self.consume(transport, memory);
    }
    pub fn needsPolling(self: *const Window) bool {
        if (self.publication != null or self.request != null or self.closing or self.ended or self.retry_take) return true;
        for (&self.slots) |*slot| if (slot.state == .returning or slot.state == .acknowledged or
            (slot.state == .live and slot.frame.retired)) return true;
        return false;
    }
};
