//! Optional hardware plane. The ordinary compositor remains responsible
//! for the clean-frame barrier and the software arrow. No timers busy-wait.
const std = @import("std");
const a = @import("r4os").abi;
const asset = @import("cursor_asset.zig");
pub const Controller = struct {
    software: bool = true,
    disabled: bool = false,
    info: a.DisplayCursorInfo = .{},
    acquired: bool = false,
    image_sequence: u64 = 0,
    pending_sequence: u64 = 0,
    pending_operation: u32 = 0,
    clean_pending: bool = false,
    clean_ready: bool = false,
    retry_at: u64 = 0,
    reference: a.GfxBufferReference = .{},
    mapping: a.GfxBufferMap = .{},
    source_ready: bool = false,

    pub fn poll(self: *Controller, api: anytype, now: u64, x: i32, y: i32, wanted: bool) bool {
        const before = self.software;
        self.advance(api, now, x, y, wanted);
        return before != self.software;
    }
    pub fn presented(self: *Controller, success: bool) void {
        if (self.clean_pending and success) self.clean_ready = true;
    }
    pub fn retryPending(self: *const Controller) bool {
        return self.pending_sequence != 0 or self.clean_pending or self.mapping.lease.id != 0;
    }
    fn advance(self: *Controller, api: anytype, now: u64, x: i32, y: i32, wanted: bool) void {
        if (!api.supportsDisplayCursor()) { self.disabled = true; return; }
        var status: a.DisplayCursorStatus = .{};
        const rc = api.displayCursorStatus(&status);
        if (rc == a.gfx_output_error_busy) return;
        const known = rc == a.gfx_output_ok;
        if (known and (status.phase == a.display_cursor_phase_lost or status.flags & a.display_cursor_state_unknown != 0)) {
            // A timeout never proves that the old plane has disappeared.
            self.software = false; return;
        }
        const shown = known and status.flags & a.display_cursor_state_visible != 0;
        if (self.pending_sequence != 0) {
            if (!known or status.display_generation != self.info.display_generation or status.completed < self.pending_sequence) return;
            self.pending_sequence = 0;
            self.image_sequence = status.image_sequence;
            if (status.error_code != 0) self.disabled = true;
            if (self.pending_operation == a.display_cursor_operation_release and status.error_code == 0) self.acquired = false;
            self.discardSource(api);
            self.software = !shown;
        }
        if (self.acquired and (!known or status.display_generation != self.info.display_generation)) {
            self.software = false; return;
        }
        if (self.acquired and status.flags & a.display_cursor_state_claimed == 0) {
            self.acquired = false; self.image_sequence = 0;
        }
        const allowed = wanted and (!known or status.flags & a.display_cursor_state_suspended == 0) and
            (self.info.display_generation == 0 or (x >= self.info.min_x and x <= self.info.max_x and y >= self.info.min_y and y <= self.info.max_y));
        if (self.disabled or !allowed) {
            self.clean_pending = false; self.clean_ready = false;
            self.software = !shown;
            if (self.acquired and (shown or self.disabled)) {
                _ = self.send(api, if (self.disabled) a.display_cursor_operation_release else a.display_cursor_operation_hide, 0, 0);
            } else if (!self.acquired) self.discardSource(api);
            return;
        }
        if (!self.acquired) {
            self.software = !shown;
            if (shown or now < self.retry_at) return;
            self.retry_at = now +| std.time.ns_per_s;
            var info: a.DisplayCursorInfo = .{};
            if (api.displayCursorInfo(&info) != a.gfx_output_ok or info.version != 1 or info.size < @sizeOf(a.DisplayCursorInfo) or
                info.flags & 15 != 15 or info.max_width < asset.width or info.max_height < asset.height) return;
            self.info = info;
            if (!self.prepareSource(api)) { self.retry_at = now; return; }
            if (self.send(api, a.display_cursor_operation_prepare, 0, 0)) self.acquired = true else self.retry_at = now;
            return;
        }
        self.discardSource(api);
        if (shown) {
            self.software = false; self.clean_pending = false; self.clean_ready = false;
            if (status.x != x or status.y != y) _ = self.send(api, a.display_cursor_operation_move, x, y);
            return;
        }
        // Auto-hide for mode changes is also visible here. A new clean
        // scene must pass through Present before every subsequent show.
        if (!self.clean_pending) {
            self.software = false; self.clean_pending = true; self.clean_ready = false;
            return;
        }
        if (self.clean_ready and self.send(api, a.display_cursor_operation_show, x, y)) {
            self.clean_pending = false; self.clean_ready = false;
        }
    }
    fn send(self: *Controller, api: anytype, operation: u32, x: i32, y: i32) bool {
        var request: a.DisplayCursorRequest = .{ .operation = operation, .display_generation = self.info.display_generation, .head_id = self.info.head_id };
        if (operation == a.display_cursor_operation_prepare) {
            request.reference = self.reference.reference; request.width = asset.width; request.height = asset.height;
            request.pitch = asset.width * 4; request.byte_length = asset.width * asset.height * 4;
        } else if (operation == a.display_cursor_operation_show or operation == a.display_cursor_operation_move) {
            request.image_sequence = self.image_sequence; request.x = x; request.y = y;
        }
        var status: a.DisplayCursorStatus = .{};
        const rc = api.displayCursorSubmit(&request, &status);
        if (rc != a.gfx_output_ok) {
            if (rc != a.gfx_output_error_busy) self.disabled = true;
            return false;
        }
        self.pending_sequence = status.sequence; self.pending_operation = operation;
        return true;
    }
    fn prepareSource(self: *Controller, api: anytype) bool {
        const bytes = asset.width * asset.height * 4;
        if (self.source_ready) return true;
        if (self.reference.reference.id == 0) {
            const descriptor: a.GfxBufferDescriptor = .{ .byte_length = bytes, .width = asset.width, .height = asset.height,
                .format = a.gfx_buffer_format_argb8888, .plane_count = 1, .plane_pitches = .{asset.width * 4,0,0,0},
                .location = a.gfx_buffer_location_system, .usage = a.gfx_buffer_usage_cpu_read | a.gfx_buffer_usage_cpu_write };
            const rc = api.gfxBufferCreate(&descriptor, &self.reference);
            if (rc != a.gfx_buffer_result_ok) { if (rc != a.gfx_buffer_error_busy) self.disabled = true; return false; }
        }
        if (self.mapping.lease.id == 0) {
            const rc = api.gfxBufferMap(&self.reference.reference, a.gfx_buffer_map_write, 0, bytes, &self.mapping);
            if (rc != a.gfx_buffer_result_ok) { if (rc != a.gfx_buffer_error_busy) self.disabled = true; return false; }
            if (self.mapping.cpu_address == 0 or self.mapping.cpu_address > std.math.maxInt(usize) - bytes or
                self.mapping.cpu_address & 3 != 0 or self.mapping.byte_length != bytes or self.mapping.cache_policy != a.gfx_buffer_cache_write_back) {
                self.disabled = true; return false;
            }
            const pixels: [*]u32 = @ptrFromInt(self.mapping.cpu_address);
            for (0..asset.height) |row| for (0..asset.width) |col| { pixels[row * asset.width + col] = asset.pixel(col, row); };
        }
        if (api.gfxBufferUnmap(&self.mapping.lease) != a.gfx_buffer_result_ok) return false;
        self.mapping = .{}; self.source_ready = true;
        return true;
    }
    fn discardSource(self: *Controller, api: anytype) void {
        if (self.mapping.lease.id != 0) {
            if (api.gfxBufferUnmap(&self.mapping.lease) != a.gfx_buffer_result_ok) return;
            self.mapping = .{};
        }
        if (self.reference.reference.id != 0) {
            if (api.gfxBufferRelease(&self.reference.reference) != a.gfx_buffer_result_ok) return;
            self.reference = .{}; self.source_ready = false;
        }
    }
};
