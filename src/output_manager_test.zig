const std = @import("std");
const r4os = @import("r4os");
const a = r4os.abi;
const Manager = @import("output_manager.zig").Manager;
const catalog = @import("r4gfx_desktop_outputs");
const t = std.testing;

const Source = struct {
    var revision: u64 = 1;
    var present: u32 = 1;
    var stale = false;
    var native_output = false;
    var bpc: u32 = 8;
    var buffers: u32 = 2;
    var connection: u64 = 4;
    var display: u64 = 5;
    var width: u32 = 1920;
    var height: u32 = 1080;
    fn identity() a.GfxOutputId {
        return .{ .adapter_id = 1, .connector_id = 2, .device_generation = 3, .connection_generation = connection };
    }
    fn clock(out: *a.MonotonicClockInfo) callconv(.c) i32 {
        out.* = .{ .flags = a.monotonic_clock_flag_valid, .instant_ns = 2 * std.time.ns_per_s };
        return 1;
    }
    fn catalog(out: *a.GfxDisplayRevision) callconv(.c) i32 {
        out.* = .{ .revision = revision, .present = present };
        return a.gfx_output_ok;
    }
    fn output(index: u32, out: *a.GfxOutputInfo) callconv(.c) i32 {
        std.debug.assert(index == 0);
        out.* = .{ .identity = identity(),
            .topology_revision = revision + @as(u64, @intFromBool(stale)),
            .flags = a.gfx_output_flag_connected | @as(u32, if (native_output) a.gfx_output_flag_active else 0) };
        return a.gfx_output_ok;
    }
    fn color(_: *const a.GfxOutputId, out: *a.GfxOutputColorState) callconv(.c) i32 {
        if (!native_output) return a.gfx_output_error_unsupported;
        out.* = .{ .identity = identity(), .revision = revision, .flags = 7,
            .format = a.gfx_buffer_format_xrgb8888, .bpc = bpc, .primaries = 1, .transfer = 1, .range = 1,
            .reference_white = 1_000_000, .peak = 1_000_000 };
        return a.gfx_output_ok;
    }
    fn target(_: u32, head: u32, out: *a.GfxOutputTarget) callconv(.c) i32 {
        if (!native_output or head != 0) return a.gfx_output_error_unsupported;
        out.* = .{ .adapter_id = 1, .connector_id = 2, .head_id = 0,
            .device_generation = 3, .connection_generation = connection, .display_generation = display };
        return a.gfx_output_ok;
    }
    fn presentation(_: *const a.GfxOutputTarget, out: *a.DisplayPresentationInfo) callconv(.c) i32 {
        out.* = .{ .flags = a.display_presentation_info_native, .width = width, .height = height,
            .format = a.gfx_buffer_format_xrgb8888, .buffer_count = buffers, .interval_ns = 16_666_666 };
        return a.gfx_output_ok;
    }
};

pub fn check() !void {
    Source.revision = 1; Source.present = 1; Source.stale = false; Source.native_output = false;
    Source.bpc = 8; Source.buffers = 2; Source.connection = 4;
    Source.display = 5;
    Source.width = 1920; Source.height = 1080;
    const raw: a.R4XStartContext = .{};
    const sys: a.R4XStartR4Sys = .{ .monotonic_clock = @intFromPtr(&Source.clock) };
    const draw: a.R4XStartR4Draw = .{ .gfx_output_revision = @intFromPtr(&Source.catalog),
        .gfx_output_info = @intFromPtr(&Source.output), .gfx_output_color = @intFromPtr(&Source.color),
        .display_output_target = @intFromPtr(&Source.target), .display_output_presentation_info = @intFromPtr(&Source.presentation) };
    const bundle: r4os.program.Bundle = .{ .raw = &raw, .sys = &sys, .draw = &draw };
    var manager: Manager = .{ .allocator = t.allocator, .raw = &raw,
        .sys = r4os.r4sys.Context.init(&bundle), .draw = r4os.r4draw.Context.init(&bundle) };
    try t.expect(manager.refresh(1));
    try t.expectEqual(@as(usize, 1), manager.snapshot.count);
    const complete = manager.snapshot;

    // A retired owner is actually collected and reconciled. It must not
    // announce a new output or force the App to withdraw its capture source.
    manager.slots[0] = .{ .target = .{ .adapter_id = 1, .connector_id = 7 },
        .failed = true, .retry_after_ns = std.time.ns_per_s };
    try t.expect(!manager.refresh(1));
    try t.expect(!manager.slots[0].occupied() and !manager.reconcile);
    try t.expect(manager.snapshot.sameOutputs(&complete));
    manager.configure(null);
    try t.expect(!manager.refresh(1));

    // Failed discovery retains the last complete output set even when
    // maintenance is pending; a real catalog change still reaches the App.
    Source.stale = true;
    manager.configure(null);
    try t.expect(!manager.refresh(1));
    try t.expect(manager.reconcile and manager.snapshot.sameOutputs(&complete));
    Source.stale = false; Source.revision = 2; Source.present = 0;
    try t.expect(manager.refresh(2));
    try t.expectEqual(@as(usize, 0), manager.snapshot.count);
    try t.expectEqual(@as(u64, 2), manager.revision);
    manager.configure(null);
    try t.expect(!manager.refresh(2));

    // The real Store.colorAt tags each output with the global catalog
    // revision. A new catalog revision alone must preserve its exact owner;
    // changed wire encoding, frame pool or receiver still needs a new one.
    Source.present = 1; Source.native_output = true;
    for (0..5) |scenario| {
        Source.revision = 10; Source.bpc = 8; Source.buffers = 2; Source.connection = 4;
        manager.slots = @splat(.{});
        manager.snapshot = try catalog.Snapshot.read(&manager.draw);
        const entry = manager.snapshot.entries[0];
        const view: catalog.topology.Viewport = .{ .pixel_w = 1920, .pixel_h = 1080 };
        manager.slots[0] = .{ .target = entry.target, .color_revision = 10, .view = view, .logical_index = 0 };
        manager.layout = try catalog.topology.Layout.init(&.{.{ .key = entry.key, .view = view, .primary = true }}, 10);
        manager.revision = 10; manager.reconcile = false;
        // Retirement has already requested preparation cancellation. Even if
        // its binding reappears, that owner must finish closing before reuse.
        if (scenario == 4) {
            manager.slots[0].logical_index = null;
            manager.slots[0].retry_after_ns = 3 * std.time.ns_per_s;
        }
        Source.revision = 11;
        switch (scenario) { 0, 4 => {}, 1 => Source.bpc = 10, 2 => Source.buffers = 3, 3 => Source.connection = 5, else => unreachable }
        try t.expect(manager.refresh(11));
        if (scenario == 0) {
            try t.expectEqual(@as(usize, 1), manager.layout.count);
            try t.expect(manager.slots[0].logical_index == 0 and !manager.slots[0].failed);
            try t.expectEqual(@as(u64, 11), manager.slots[0].color_revision);
        } else {
            // No hardware provider is supplied. If reuse is forbidden the
            // new owner cannot be admitted, while the original slot retires.
            try t.expectEqual(@as(usize, 0), manager.layout.count);
            if (scenario == 4) {
                try t.expect(manager.slots[0].occupied() and manager.slots[0].logical_index == null);
                try t.expectEqual(@as(u64, 10), manager.slots[0].color_revision);
            } else if (manager.slots[0].occupied()) {
                // Early retirement may free this slot for the rejected new
                // admission. Only its empty retry metadata may remain; the
                // original encoding and all runtime owners must be gone.
                const retry = manager.slots[0];
                try t.expect(retry.failed and retry.logical_index == null and retry.gpu == null and retry.software == null);
                try t.expect(std.meta.eql(retry.target, manager.snapshot.entries[0].target));
                try t.expectEqual(@as(u64, 11), retry.color_revision);
            }
        }
    }
    Source.native_output = false;
    const cursor_slot: @import("output_manager.zig").Slot = .{ .target=.{.adapter_id=1,.connector_id=2,
        .device_generation=3,.connection_generation=4,.display_generation=5},
        .logical_index=0,.view=.{.pixel_w=1920,.pixel_h=1080} };
    const cursor_info: a.DisplayCursorInfo = .{ .flags=15,.display_generation=5,
        .backend=.{.adapter_id=1,.device_generation=3,.reset_generation=1} };
    try t.expect(cursor_slot.cursorCompatible(cursor_info, 1920, 1080));
    for (0..9) |change| {
        var wrong = cursor_slot;
        switch (change) {
            0 => wrong.view.scale = 180,
            1 => wrong.view.rotation = .clockwise90,
            2 => wrong.view.origin.x = 1,
            3 => wrong.target.display_generation += 1,
            4 => wrong.target.head_id += 1,
            5 => wrong.target.device_generation += 1,
            6 => wrong.target.adapter_id += 1,
            7 => wrong.sleeping = true,
            8 => wrong.reconfiguring = true,
            else => unreachable,
        }
        try t.expect(!wrong.cursorCompatible(cursor_info, 1920, 1080));
    }
    try t.expect(!cursor_slot.cursorCompatible(cursor_info, 1280, 720));

    // Native mode geometry and encoding can change while the exact output
    // target is retained. A complete changed catalog still retires the old
    // owner before its swapchain is polled; mixed discovery owns no release.
    Source.native_output = true; Source.present = 1; Source.connection = 4; Source.display = 5;
    for (0..3) |scenario| {
        Source.stale = false; Source.revision = 20; Source.bpc = 8; Source.buffers = 2;
        Source.width = 1920; Source.height = 1080;
        manager.snapshot = try catalog.Snapshot.read(&manager.draw);
        manager.slots = @splat(.{}); manager.reconcile = false;
        const original = manager.snapshot.entries[0];
        manager.slots[0] = .{ .target = original.target, .color_revision = 20,
            .view = .{ .pixel_w = 1920, .pixel_h = 1080 }, .logical_index = 0 };
        switch (scenario) {
            0 => { Source.width = 1280; Source.height = 720; }, // No catalog stamp change.
            1 => Source.buffers = 3,
            2 => { Source.revision = 21; Source.bpc = 10; },
            else => unreachable,
        }
        Source.stale = true;
        manager.poll();
        try t.expect(manager.slots[0].occupied() and manager.slots[0].logical_index == 0);
        Source.stale = false;
        manager.poll();
        try t.expect(!manager.slots[0].occupied() and manager.reconcile);
        for (manager.software_targets) |failed| try t.expect(failed.connector_id == 0);
    }
    Source.width = 1920; Source.height = 1080;

    // A complete new output generation must retire the old owner before
    // polling its revoked swapchain. Mixed discovery must retain that owner;
    // neither path may fabricate a GPU fault or forget a real quarantine.
    Source.native_output = true; Source.stale = false; Source.revision = 20;
    Source.present = 1; Source.bpc = 8; Source.buffers = 2; Source.connection = 4; Source.display = 5;
    manager.snapshot = try catalog.Snapshot.read(&manager.draw);
    manager.slots = @splat(.{}); manager.reconcile = false;
    const previous = manager.snapshot.entries[0];
    manager.slots[0] = .{ .target = previous.target, .color_revision = 20,
        .view = .{ .pixel_w = 1920, .pixel_h = 1080 }, .logical_index = 0 };
    Source.revision = 21; Source.display = 6; Source.stale = true;
    manager.poll();
    try t.expect(manager.slots[0].occupied() and manager.slots[0].logical_index == 0);
    Source.stale = false;
    manager.poll();
    try t.expect(!manager.slots[0].occupied() and manager.reconcile);
    for (manager.software_targets) |failed| try t.expect(failed.connector_id == 0);
    const current = try catalog.Snapshot.read(&manager.draw);
    manager.software_targets[0] = previous.target;
    try t.expect(manager.softwareOnly(current.entries[0].target));
}
