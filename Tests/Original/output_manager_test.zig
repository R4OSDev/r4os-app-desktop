const std = @import("std");
const r4os = @import("r4os");
const a = r4os.abi;
const Manager = @import("output_manager.zig").Manager;
const catalog = @import("r4gfx_desktop_outputs");
const topology = catalog.topology;
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

// The Rust catalog adapter consumes the same Source functions as this test.
var native_sys: a.R4XStartR4Sys = .{};
var native_draw: a.R4XStartR4Draw = .{};
var native_imports: [2]a.R4XStartImport = undefined;
var native_raw: a.R4XStartContext = .{};
pub fn nativeContext() *const a.R4XStartContext {
    Source.revision=1;Source.present=1;Source.stale=false;Source.native_output=false;
    Source.bpc=8;Source.buffers=2;Source.connection=4;Source.display=5;Source.width=1920;Source.height=1080;
    native_sys=.{.monotonic_clock=@intFromPtr(&Source.clock)};
    native_draw=.{.gfx_output_revision=@intFromPtr(&Source.catalog),.gfx_output_info=@intFromPtr(&Source.output),
        .gfx_output_color=@intFromPtr(&Source.color),.display_output_target=@intFromPtr(&Source.target),
        .display_output_presentation_info=@intFromPtr(&Source.presentation)};
    native_imports=.{
        .{.group_id=@intFromEnum(a.R4LGroup.r4sys),.min_version=1,.resolved_version=1,.flags=a.r4xstart_import_flag_group_interface,.module_name=@intFromPtr("R4SYS"),.symbol_name=@intFromPtr("Query"),.table=@intFromPtr(&native_sys)},
        .{.group_id=@intFromEnum(a.R4LGroup.r4draw),.min_version=1,.resolved_version=1,.flags=a.r4xstart_import_flag_group_interface,.module_name=@intFromPtr("R4DRAW"),.symbol_name=@intFromPtr("Query"),.table=@intFromPtr(&native_draw)},
    };
    native_raw=.{.app_class=@intFromEnum(a.R4XStartAppClass.console),.instance_id=86,.flags=a.r4xstart_flag_imports_valid,.imports=@intFromPtr(&native_imports),.import_count=native_imports.len};
    return &native_raw;
}
pub fn nativeControl(op:u32,value:u64) i32 {
    switch(op){0=>Source.revision=value,1=>Source.present=std.math.cast(u32,value) orelse return -1,
        2=>Source.stale=value!=0,3=>Source.native_output=value!=0,4=>Source.bpc=std.math.cast(u32,value) orelse return -1,
        5=>Source.buffers=std.math.cast(u32,value) orelse return -1,6=>Source.connection=value,7=>Source.display=value,
        8=>Source.width=std.math.cast(u32,value) orelse return -1,9=>Source.height=std.math.cast(u32,value) orelse return -1,else=>return -1}
    return 0;
}

pub fn check() !void {
    const geometry = @import("output_geometry.zig");
    const layout = try catalog.topology.Layout.init(&.{.{ .key = .{ .adapter = 1, .connector = 1 }, .primary = true,
        .view = .{ .pixel_w = 640, .pixel_h = 480 } }}, 1);
    const area = @import("surface.zig").Rect{ .x = 0, .y = 0, .w = 640, .h = 448 };
    const prior_rect = @import("surface.zig").Rect{ .x = 40, .y = 60, .w = 300, .h = 160 };
    try t.expectEqual(prior_rect, geometry.rebaseWindow(&layout, prior_rect, .{ .x = std.math.minInt(i32) }, area, 24));
    try t.expectEqual(@import("surface.zig").Rect{ .x = 10, .y = 20, .w = 300, .h = 160 },
        geometry.rebaseWindow(&layout, prior_rect, .{ .x = 30, .y = 40 }, area, 24));
    const spanning = [_]topology.Output{
        .{ .key = .{ .adapter = 1, .connector = 1 }, .primary = true, .view = .{ .pixel_w = 640, .pixel_h = 480 } },
        .{ .key = .{ .adapter = 1, .connector = 2 },
            .view = .{ .origin = .{ .x = 640 - std.math.maxInt(i32) }, .pixel_w = 640, .pixel_h = 480 } },
    };
    const wide = try topology.Layout.init(&spanning, 2);
    // Generic topology mathematics can represent this span. The desktop
    // refuses its unsupported normalized workspace before publishing it.
    try t.expect(geometry.desktopBounds(&wide) == null);
    const wide_bounds: @import("surface.zig").Rect = .{ .x = spanning[1].view.origin.x, .y = 0, .w = std.math.maxInt(i32), .h = 480 };
    const edge_cursor = geometry.cursorRect(wide_bounds, .{ .x = 639, .y = 479 });
    try t.expectEqual(@import("surface.zig").Rect{ .x = 639, .y = 479, .w = 1, .h = 1 }, edge_cursor);
    const far_output = try geometry.logical(spanning[1].view);
    try t.expectEqual(@as(i32, std.math.maxInt(i32)), far_output.merged(edge_cursor).w);
    // Adjacent output coverage remains complete; only the outer desktop edge
    // clips the cursor, not the boundary of whichever output owns its hotspot.
    try t.expectEqual(@as(i32, 12), geometry.cursorRect(geometry.desktopBounds(&layout).?, .{ .x = 600 }).w);
    var tiny = try topology.Layout.init(&.{.{ .key = .{ .connector = 1 }, .primary = true, .view = .{ .pixel_w = 1, .pixel_h = 1 } }}, 3);
    try t.expect(geometry.desktopBounds(&tiny) != null);
    tiny.outputs[0].view.origin.x = std.math.maxInt(i32) - 1;
    try t.expect(geometry.desktopBounds(&tiny) == null);
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
