const std = @import("std");
const t = std.testing;
const a = @import("r4os").abi;
const asset = @import("cursor_asset.zig");
const controller = @import("cursor_controller.zig");
const Model = struct {
    supported: bool = true,
    status: a.DisplayCursorStatus = .{ .display_generation = 7 },
    request: a.DisplayCursorRequest = .{},
    calls: usize = 0,
    creates: usize = 0,
    mapped: bool = false,
    released: bool = false,
    busy: bool = false,
    pixels: [asset.width * asset.height]u32 = undefined,
    pub fn supportsDisplayCursor(self: *Model) bool { return self.supported; }
    pub fn displayCursorInfo(_: *Model, out: *a.DisplayCursorInfo) i32 {
        out.* = .{ .display_generation=7,.flags=15,.max_width=256,.max_height=256,.min_x=-32768,.min_y=-32768,.max_x=32767,.max_y=32767 }; return 1;
    }
    pub fn displayCursorStatus(self: *Model, out: *a.DisplayCursorStatus) i32 { out.* = self.status; return 1; }
    pub fn displayCursorSubmit(self: *Model, request: *const a.DisplayCursorRequest, out: *a.DisplayCursorStatus) i32 {
        if (self.busy) return a.gfx_output_error_busy;
        std.debug.assert(!self.mapped and request.display_generation == 7 and self.status.sequence == self.status.completed);
        self.request = request.*; self.calls += 1; self.status.sequence += 1;
        self.status.phase = a.display_cursor_phase_queued; self.status.flags |= a.display_cursor_state_claimed;
        out.* = self.status; return 1;
    }
    fn finish(self: *Model) void {
        self.status.completed = self.status.sequence; self.status.phase = a.display_cursor_phase_complete;
        switch (self.request.operation) {
            a.display_cursor_operation_prepare => { self.status.image_sequence = self.status.sequence; self.status.flags |= a.display_cursor_state_image_ready; },
            a.display_cursor_operation_show, a.display_cursor_operation_move => {
                self.status.flags |= a.display_cursor_state_visible; self.status.x = self.request.x; self.status.y = self.request.y;
            },
            a.display_cursor_operation_hide => self.status.flags &= ~a.display_cursor_state_visible,
            a.display_cursor_operation_release => { self.status.flags = 0; self.status.image_sequence = 0; },
            else => unreachable,
        }
    }
    pub fn gfxBufferCreate(self: *Model, d: *const a.GfxBufferDescriptor, out: *a.GfxBufferReference) i32 {
        std.debug.assert(self.creates == 0 and d.byte_length == @sizeOf(@TypeOf(self.pixels)) and d.format == a.gfx_buffer_format_argb8888);
        self.creates += 1; out.* = .{ .reference=.{.id=4,.generation=9} }; return 1;
    }
    pub fn gfxBufferMap(self: *Model, reference: *const a.GfxBufferHandle, access: u32, offset: u64, bytes: u64, out: *a.GfxBufferMap) i32 {
        std.debug.assert(reference.id == 4 and access == a.gfx_buffer_map_write and offset == 0 and bytes == @sizeOf(@TypeOf(self.pixels)));
        self.mapped = true; out.* = .{ .lease=.{.id=5,.generation=9},.cpu_address=@intFromPtr(&self.pixels),.byte_length=bytes,.cache_policy=a.gfx_buffer_cache_write_back }; return 1;
    }
    pub fn gfxBufferUnmap(self: *Model, _: *const a.GfxBufferHandle) i32 { std.debug.assert(self.mapped); self.mapped = false; return 1; }
    pub fn gfxBufferRelease(self: *Model, _: *const a.GfxBufferHandle) i32 {
        std.debug.assert(!self.mapped and self.status.completed >= 1); self.released = true; return 1;
    }
};
pub fn check() !void {
    var old: Model = .{ .supported=false }; var fallback: controller.Controller = .{};
    try t.expect(!fallback.poll(&old, 1, 10, 20, true) and fallback.software and old.calls == 0 and old.creates == 0);
    var model: Model = .{}; var cursor: controller.Controller = .{};
    try t.expect(!cursor.poll(&model, 1, 10, 20, true) and cursor.software and model.request.operation == a.display_cursor_operation_prepare);
    try t.expect(model.pixels[0] == 0xff000000 and model.pixels[2*asset.width+1] == 0xffffffff and model.pixels[asset.width-1] == 0);
    _ = cursor.poll(&model, 2, 12, 21, true);
    try t.expect(model.calls == 1 and !model.released and cursor.software);
    model.finish();
    try t.expect(cursor.poll(&model, 3, 12, 21, true) and !cursor.software and cursor.clean_pending and model.released);
    cursor.presented(false); _ = cursor.poll(&model, 4, 12, 21, true);
    try t.expect(model.calls == 1); // Failed Present cannot erase the old software image.
    cursor.presented(true); model.busy = true; _ = cursor.poll(&model, 5, 12, 21, true);
    try t.expect(cursor.clean_ready and model.calls == 1);
    model.busy = false; _ = cursor.poll(&model, 6, 12, 21, true);
    try t.expect(model.request.operation == a.display_cursor_operation_show and model.request.image_sequence == 1 and !cursor.software);
    _ = cursor.poll(&model, 7, 15, 24, true); try t.expect(model.calls == 2);
    model.finish(); _ = cursor.poll(&model, 8, 17, 27, true);
    try t.expect(model.calls == 3 and model.request.operation == a.display_cursor_operation_move and model.request.x == 17 and !cursor.software);
    _ = cursor.poll(&model, 9, 22, 29, true); try t.expect(model.calls == 3);
    model.finish(); _ = cursor.poll(&model, 10, 22, 29, true);
    try t.expect(model.calls == 4 and model.request.x == 22 and model.creates == 1);
    model.finish(); _ = cursor.poll(&model, 11, 22, 29, false);
    try t.expect(model.request.operation == a.display_cursor_operation_hide and !cursor.software);
    _ = cursor.poll(&model, 12, 22, 29, false); try t.expect(!cursor.software);
    model.finish(); try t.expect(cursor.poll(&model, 13, 22, 29, false) and cursor.software);
    model.status.flags |= a.display_cursor_state_suspended;
    _ = cursor.poll(&model, 14, 22, 29, true); try t.expect(cursor.software and model.calls == 5);
    model.status.flags &= ~a.display_cursor_state_suspended;
    try t.expect(cursor.poll(&model, 15, 22, 29, true) and cursor.clean_pending);
    _ = cursor.poll(&model, 16, 22, 29, true); try t.expect(model.calls == 5);
    cursor.presented(true); _ = cursor.poll(&model, 17, 22, 29, true);
    model.status.phase = a.display_cursor_phase_lost; model.status.flags |= a.display_cursor_state_unknown;
    _ = cursor.poll(&model, 18, 22, 29, false); try t.expect(!cursor.software and model.calls == 6);
    // A real late receipt can settle the exact operation; a timeout alone cannot.
    model.status.flags &= ~a.display_cursor_state_unknown; model.finish(); _ = cursor.poll(&model, 19, 22, 29, false);
    try t.expect(model.calls == 7 and model.request.operation == a.display_cursor_operation_hide and !cursor.software);
    model.finish(); _ = cursor.poll(&model, 20, 22, 29, false); try t.expect(cursor.software);
}
