//! Explicit, bounded qualification of the productive desktop painter and GPU
//! engine. It owns private offscreen images, never a Window session or display.
const std = @import("std");
const r4os = @import("r4os");
const gfx = @import("r4gfx");
const api = @import("api.zig");
const renderer = @import("gfx_renderer.zig");
const gpu = @import("composition_gpu.zig");
const layers = @import("composition_layers.zig");
const primitive = @import("primitive_frame.zig");
const software = @import("composition_software.zig");
const compositor = @import("compositor.zig");
const scene_buffer = @import("scene_buffer.zig");
const surface = @import("surface.zig");
const readback = @import("r4gfx_readback");
const reference = @import("primitive_reference.zig");
const Reference = enum { basic, shapes, centers, chunks };
const width = 640;
const height = 400;
const full: surface.Rect = .{ .x = 0, .y = 0, .w = width, .h = height };
const sentinel = 0x13579b;
const timeout = 15 * std.time.ns_per_s;

pub fn requested(args: [*:0]const u8) bool {
    return argument(args, "/COMPOSITIONVERIFY") or argument(args, "/COMPOSITIONREFERENCES");
}
fn argument(args: [*:0]const u8, wanted: []const u8) bool {
    var it = std.mem.tokenizeAny(u8, std.mem.span(args), " \t");
    while (it.next()) |arg| if (std.ascii.eqlIgnoreCase(arg, wanted)) return true;
    return false;
}

pub fn run(ctx: *api.Context, raw: *const r4os.abi.R4XStartContext) i32 {
    if (argument(ctx.argsRaw(), "/CAPTURE")) {
        const owner = Owner.create(ctx, raw, true) catch |err| {
            ctx.write("DESKTOP capture-verify: FAIL create="); ctx.println(@errorName(err)); return 1;
        };
        var passed = true;
        owner.checkPublishedCapture(argument(ctx.argsRaw(), "/CLIENT")) catch |err| {
            owner.log("FAIL capture stage={s} error={s}", .{ owner.stage, @errorName(err) }); passed = false;
        };
        if (!owner.destroy()) { ctx.println("DESKTOP capture-verify: FAIL retirement-held"); return 1; }
        if (!passed) return 1;
        ctx.println("DESKTOP capture-verify: PASS native-pixels screenshot publication retirement present=none");
        return 0;
    }
    if (argument(ctx.argsRaw(), "/COMPOSITIONREFERENCES")) {
        for (std.enums.values(Reference)) |kind| {
            const owner = Owner.create(ctx, raw, true) catch |err| {
                ctx.write("DESKTOP references: FAIL create="); ctx.println(@errorName(err)); return 1;
            };
            var passed = true;
            owner.checkReference(kind) catch |err| {
                owner.log("FAIL reference={s} stage={s} error={s}", .{ @tagName(kind), owner.stage, @errorName(err) }); passed = false;
            };
            if (!owner.destroy()) { ctx.println("DESKTOP references: FAIL retirement-held"); return 1; }
            if (!passed) return 1;
        }
        ctx.println("DESKTOP references: PASS original-basic+shapes center-sampling chunk-boundaries remote-demand present=none");
        return 0;
    }
    for ([_]bool{ false, true }) |recording| {
        const owner = Owner.create(ctx, raw, recording) catch |err| {
            ctx.write("DESKTOP offscreen: FAIL create="); ctx.println(@errorName(err)); return 1;
        };
        var passed = true;
        const checked = if (argument(ctx.argsRaw(), "/TRANSITIONS")) owner.checkTransitions() else owner.check();
        checked catch |err| {
            owner.log("FAIL stage={s} error={s}", .{ owner.stage, @errorName(err) }); passed = false;
        };
        if (!owner.destroy()) { ctx.println("DESKTOP offscreen: FAIL retirement-held"); return 1; }
        if (!passed) return 1;
    }
    ctx.println(if (argument(ctx.argsRaw(), "/TRANSITIONS"))
        "DESKTOP transitions: PASS producer-commands window/fullscreen/resize/occlusion queues=bounded present=none"
    else "DESKTOP offscreen: PASS productive-painter layers+primitives readback present=none");
    return 0;
}

const Fixture = struct {
    windows: [4]@import("window.zig").Window = [_]@import("window.zig").Window{.{ .kind = .app, .x = 24, .y = 32, .w = 280, .h = 200,
        .normal_x = 24, .normal_y = 32, .normal_w = 280, .normal_h = 200 }} ** 4,
    commands: [4][2]r4os.abi.GuiFrameCommand = undefined,
    frames: [4]@import("gui_frame_snapshot.zig").View = undefined,
    items: @import("desktop_items.zig").Items = .{},
    quick: @import("quick_launch.zig").Bar = .{},
    tray: @import("tray.zig").Registry = .{},
    menu: @import("start_menu.zig").Menu = undefined,
    wallpaper: [192 * 132]u32 = undefined,
    active: usize = 2,
    menu_open: bool = false,
    cursor_visible: bool = true,
    pixel: u32 = 0x55aa88,
    fn init(self: *Fixture) void {
        self.menu = @import("start_menu.zig").Menu.initDefault();
        for (&self.windows, 0..) |*win, i| {
            win.x += @as(i32, @intCast(i)) * 90; win.y += @as(i32, @intCast(i)) * 28;
            @memcpy(win.title_buf[0..16], "GPU desktop test");
            self.commands[i] = .{
                .{ .kind = r4os.abi.gui_frame_command_kind_clear, .rgb = 0x183048 + @as(u32, @intCast(i)) * 0x183018 },
                .{ .kind = r4os.abi.gui_frame_command_kind_rect, .x = 12, .y = 18, .w = 94, .h = 38, .rgb = 0x88bbcc },
            };
            self.frames[i] = .{ .valid = true, .commands = &self.commands[i] };
        }
        self.windows[3].visible = false;
        for (&self.wallpaper, 0..) |*pixel, i| pixel.* = @as(u32, @intCast(i % 192)) * 0x010000 +
            @as(u32, @intCast(i / 192)) * 0x100 + 0x40;
    }
    fn paint(self: *Fixture, ctx: *api.Context, damage: surface.Rect) compositor.CullStats {
        const result = compositor.compose(ctx, width, height, &self.windows, &self.frames, self.active, "12:34", "DE",
            .{}, &self.tray, .{}, .{}, &self.items, &self.quick, @import("desktop_items.zig").no_selection,
            @import("desktop_items.zig").no_selection, self.menu_open, &self.menu, 0, false, 0, 0, false, 0, 0,
            .none, .{}, false, "Terminal", "", "", &.{}, &.{},
            .{ .width = 192, .height = 132, .pixels = &self.wallpaper }, .{}, 8, 437, false, false,
            600, 300, self.cursor_visible, .none, .none, damage);
        // A tiny independent desktop overlay supplies alpha/mask/clip cases
        // and a one-pixel update outside windows, menus and the taskbar.
        if (ctx.beginLayer(31, .{ .x = 552, .y = 32, .w = 48, .h = 48 })) |target| {
            defer target.end();
            target.context.paintRect(552, 32, 48, 48, 0x203040);
            const alpha = [_]u8{ 0, 64, 128, 255, 255, 128, 64, 0 };
            _ = target.context.paintAlpha8(556, 36, 4, 2, 4, 0xffeedd, &alpha);
            target.context.paintRect(580, 60, 1, 1, self.pixel);
        }
        return result;
    }
};

const Owner = struct {
    ctx: *api.Context,
    graphics: *renderer.Renderer,
    cache: layers.Cache,
    cpu_cache: layers.Cache,
    engine: gpu.Engine,
    reader: readback.Owner,
    remote: @import("remote_capture.zig").Capture,
    primitives: primitive.Frame,
    fixture: Fixture = .{},
    scene: scene_buffer.SceneBuffer = .{},
    reference: scene_buffer.SceneBuffer = .{},
    untouched: []u32,
    expected: []u32,
    recording: bool,
    stage: []const u8 = "create",
    polls: u64 = 0,
    frames: u64 = 0,
    compared: u64 = 0,
    max_error: u32 = 0,
    queue_peak: usize = 0,
    reference_case: ?Reference = null,
    large_source: []u32 = &.{},

    fn create(ctx: *api.Context, raw: *const r4os.abi.R4XStartContext, recording: bool) !*Owner {
        const allocator = ctx.allocator();
        const graphics = renderer.Renderer.create(allocator, raw) orelse return error.Graphics;
        errdefer graphics.destroy();
        const info = graphics.info() orelse return error.Graphics;
        const operations = gfx.device_gpu_render | gfx.device_gpu_copy_rows;
        if (info.gpu_operations & operations != operations) return error.Unsupported;
        const self = try allocator.create(Owner);
        errdefer allocator.destroy(self);
        const untouched = try allocator.alloc(u32, width * height);
        errdefer allocator.free(untouched);
        const expected = try allocator.alloc(u32, width * height);
        errdefer allocator.free(expected);
        self.* = .{ .ctx = ctx, .graphics = graphics, .cache = layers.Cache.init(allocator, 128 * 1024 * 1024),
            .cpu_cache = layers.Cache.init(allocator, 128 * 1024 * 1024),
            .engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device),
            .reader = readback.Owner.init(allocator, &graphics.client, &graphics.colors, &graphics.device),
            .remote = @import("remote_capture.zig").Capture.init(allocator, &graphics.client, &graphics.colors, &graphics.device),
            .primitives = try primitive.Frame.init(allocator), .untouched = untouched, .expected = expected, .recording = recording };
        self.engine.destination = .readback;
        self.engine.clock = .{ .context = @intFromPtr(ctx), .read = clock };
        self.primitives.mirror = false;
        if (recording) self.cache.recording = &self.primitives;
        @memset(untouched, sentinel); @memset(expected, 0);
        std.debug.assert(self.scene.attach(std.mem.sliceAsBytes(untouched), width, height));
        std.debug.assert(self.reference.attach(std.mem.sliceAsBytes(expected), width, height));
        self.fixture.init();
        self.log("begin backend={d} adapter={d} operations={x}", .{ info.backend, info.adapter_id, info.gpu_operations });
        return self;
    }
    fn clock(raw: usize) u64 {
        const ctx: *const api.Context = @ptrFromInt(raw);
        return ctx.sys.monotonicNanoseconds() orelse 0;
    }
    fn now(self: *Owner) u64 { return clock(@intFromPtr(self.ctx)); }
    fn log(self: *Owner, comptime format: []const u8, args: anytype) void {
        var buffer: [1024]u8 = undefined;
        self.ctx.write(if (self.recording) "DESKTOP offscreen primitives: " else "DESKTOP offscreen layers: ");
        self.ctx.println(std.fmt.bufPrint(&buffer, format, args) catch "log-overflow");
    }
    fn capture(self: *Owner, cpu: bool, damage: surface.Rect) !void {
        if (self.reference_case != null) {
            if (cpu) {
                @memset(self.reference.pixels.?, 0);
                try self.paintReference(&self.reference);
                if (self.reference_case == .centers or self.reference_case == .chunks) {
                    // Independent integer anchors: whole-image center samples
                    // versus the historical guest-edge mapping of chunks.
                    for (self.reference.pixels.?, 0..) |*pixel, index| {
                        const x = index % 8; const y = index / 8;
                        pixel.* = if (self.reference_case == .centers)
                            sampleColor(((2 * x + 1) * 3) / 16, ((2 * y + 1) * 3) / 16)
                        else sampleColor((x * 257) / 8, (y * 2) / 8);
                    }
                }
                return;
            }
            try self.cache.start(self.scene.fullRect());
            const painter = (try self.cache.begin(1, self.scene.fullRect(), damage)) orelse return error.ReferencePaint;
            try self.paintReference(painter);
            try self.cache.end(1); _ = try self.cache.finish();
            return;
        }
        const cache = if (cpu) &self.cpu_cache else &self.cache;
        const scene = if (cpu) &self.reference else &self.scene;
        try cache.start(full);
        scene.layer_hook = cache.hook();
        self.ctx.beginSceneClipped(scene, damage);
        defer { self.ctx.endScene(); scene.clearPaintClip(); scene.layer_hook = null; }
        _ = self.fixture.paint(self.ctx, damage);
        _ = try cache.finish();
        if (cpu) _ = try software.paint(&self.graphics.colors, cache, scene);
    }
    fn advance(self: *Owner, expected: gpu.Progress) !void {
        const limit = self.now() + timeout + timeout;
        while (true) {
            const value = self.engine.advance(&self.cache, self.now());
            self.polls += 1;
            var held: usize = 0;
            for (self.engine.jobs) |job| if (job != null) { held += 1; };
            self.queue_peak = @max(self.queue_peak, held);
            if (value != .pending) {
                if (value != expected) {
                    self.log("engine result={s} reason={s}", .{ @tagName(value), if (self.engine.fault) |err| @errorName(err) else "none" });
                    return error.FrameFailed;
                }
                return;
            }
            if (self.now() >= limit) return error.Retained;
            self.ctx.taskYield();
        }
    }
    fn compare(self: *Owner) !void {
        const w: u32 = @intCast(self.scene.width); const h: u32 = @intCast(self.scene.height);
        try self.reader.prepare(w, h, self.engine.output_format, self.engine.output_color);
        const started = self.now();
        try self.reader.begin(.{ .source = self.engine.outputs[self.engine.output_index].resource, .epoch = 1,
            .frame = self.cache.frame, .regions = &.{.{ .x = 0, .y = 0, .width = w, .height = h }},
            .now_ns = started, .deadline_ns = started + timeout });
        while (self.reader.pending()) {
            self.reader.poll(self.now()); self.ctx.taskYield();
            if (self.now() > started + timeout + timeout) return error.Retained;
        }
        // `valid` certifies a previously acknowledged publication for delta
        // capture. This diagnostic reads a fresh complete frame before any
        // publication; the ready phase is its retirement/conversion receipt.
        if (self.reader.phase != .ready) {
            self.log("readback phase={s} reason={s}", .{ @tagName(self.reader.phase),
                if (self.reader.failure) |err| @errorName(err) else "none" });
            return error.ReadbackFailed;
        }
        // Same current scene, independently painted into CPU layers and
        // composed through the productive linear-light software owner.
        try self.capture(true, full);
        var maximum: u32 = 0;
        var mismatches: usize = 0;
        const tolerance: u32 = if (self.reference_case) |kind| switch (kind) {
            .basic => reference.basic_tolerance, .shapes => reference.shapes_tolerance, .centers, .chunks => 0,
        } else 1;
        for (self.reference.pixels.?, self.reader.pixels, 0..) |want, actual, index| {
            var delta: u32 = 0;
            for ([_]u5{ 0, 8, 16 }) |shift| delta = @max(delta, @abs(@as(i32, @intCast((want >> shift) & 255)) - @as(i32, @intCast((actual >> shift) & 255))));
            maximum = @max(maximum, delta);
            if (delta > tolerance) {
                if (mismatches < 8) self.log("pixel x={d} y={d} expected={x} actual={x} delta={d}", .{ index % w, index / w, want, actual, delta });
                mismatches += 1;
            }
        }
        self.max_error = @max(self.max_error, maximum); self.compared += self.reader.pixels.len;
        self.log("pixels={d} max-LSB={d} mismatches={d} tolerance={d}", .{ self.reader.pixels.len, maximum, mismatches, tolerance });
        if (mismatches != 0) return error.Pixels;
        for (self.untouched) |pixel| if (pixel != sentinel) return error.UnexpectedCpuComposition;
        try self.reader.acknowledge(false);
    }
    fn frame(self: *Owner, name: []const u8, damage: surface.Rect) !void {
        self.stage = name;
        const start = self.now();
        const uploads = self.engine.uploaded_bytes;
        const draws = self.engine.render_jobs;
        try self.capture(false, damage);
        const warm = self.engine.prepared(&self.cache);
        if (!warm) try self.engine.prepare(&self.cache, self.now() + timeout);
        try self.engine.begin(&self.cache, self.now() + timeout);
        try self.advance(.copied);
        if (self.engine.present_fence != null or self.engine.chain.slot != 0) return error.UnexpectedPresent;
        self.frames += 1;
        self.log("frame={s} warm={any} render-ns={d} uploads={d} GR={d} reserved={d} primitive-draws={d} primitive-jobs={d}",
            .{ name, warm, self.now() - start, self.engine.uploaded_bytes - uploads, self.engine.render_jobs - draws,
                self.engine.reserved_bytes, self.engine.primitive_draws, self.engine.primitive_jobs });
        try self.compare();
    }
    fn check(self: *Owner) !void {
        try self.frame("cold", full);
        const uploads = self.engine.uploaded_bytes;
        try self.frame("warm", full);
        if (self.engine.uploaded_bytes != uploads) return error.ResidentReupload;
        const draws = self.engine.render_jobs;
        for (0..16) |_| if (self.engine.advance(&self.cache, self.now()) != .copied) return error.Idle;
        if (self.engine.render_jobs != draws or self.engine.uploaded_bytes != uploads) return error.Idle;
        self.fixture.pixel = 0x8855cc;
        try self.frame("one-pixel", .{ .x = 580, .y = 60, .w = 1, .h = 1 });
        if (!self.recording and self.engine.uploaded_bytes - uploads != 4) return error.DamageUpload;
        self.fixture.menu_open = true;
        try self.frame("menu", full);
        self.fixture.menu_open = false;
        self.fixture.windows[2].x += 56;
        try self.frame("move", full);
        self.fixture.windows[2].visible = false;
        try self.frame("close", full);
        self.fixture.windows[1].minimized = true;
        self.fixture.active = 0;
        try self.frame("occlusion", full);
        self.stage = "cancel";
        try self.capture(false, full);
        if (!self.engine.prepared(&self.cache)) try self.engine.prepare(&self.cache, self.now() + timeout);
        try self.engine.begin(&self.cache, self.now() + timeout);
        _ = self.engine.advance(&self.cache, self.now());
        self.engine.cancel(error.Deadline);
        try self.advance(.failed);
        if (!self.engine.needsFull(width, height)) return error.PartialFrame;
        try self.frame("reconstructed", full);
        self.log("PASS frames={d} pixels={d} max-LSB={d} polls={d} CPU-scene=untouched idle-jobs=0 present=none", .{
            self.frames, self.compared, self.max_error, self.polls });
    }
    fn checkTransitions(self: *Owner) !void {
        // Generic GUI commands use the ordinary frame-snapshot/painter path.
        // These are private frames, not a Window-registered application or a
        // display-capability override. Window geometry uses its real owner.
        const window = &self.fixture.windows[2];
        window.normal_x = window.x; window.normal_y = window.y;
        window.normal_w = window.w; window.normal_h = window.h;
        const original = window.geometry();
        const work = surface.workArea(width, height, @import("theme.zig").taskbar_h);
        self.fixture.commands[2][0].rgb = 0x804020;
        try self.frame("producer-window-a", full);
        self.fixture.commands[2][0].rgb = 0x2060a0;
        self.fixture.commands[2][1].rgb = 0xcc8040;
        try self.frame("producer-window-b", full);
        if (!window.setFullscreen(true, full, work) or !std.meta.eql(window.geometry(), full)) return error.Geometry;
        try self.frame("producer-fullscreen", full);
        self.fixture.menu_open = true;
        try self.frame("producer-fullscreen-menu", full);
        self.fixture.menu_open = false;
        if (!window.setFullscreen(false, full, work) or !std.meta.eql(window.geometry(), original)) return error.Geometry;
        try self.frame("producer-restored", full);
        window.w += 48; window.h += 32;
        try self.frame("producer-resized", full);
        const cover = &self.fixture.windows[3];
        cover.visible = true;
        if (!cover.setFullscreen(true, full, work)) return error.Geometry;
        self.fixture.active = 3;
        try self.frame("producer-occluded", full);
        cover.visible = false; self.fixture.active = 2;
        try self.frame("producer-revealed", full);
        window.visible = false; self.fixture.active = 1;
        try self.frame("producer-closed", full);
        const reserved = self.engine.reserved_bytes;
        const draws = self.engine.render_jobs;
        const uploads = self.engine.uploaded_bytes;
        for (0..16) |_| if (self.engine.advance(&self.cache, self.now()) != .copied) return error.Idle;
        if (self.queue_peak == 0 or self.queue_peak > self.engine.jobs.len or
            self.engine.reserved_bytes != reserved or self.engine.render_jobs != draws or self.engine.uploaded_bytes != uploads)
            return error.Idle;
        self.log("PASS transitions frames={d} pixels={d} max-LSB={d} queue-peak={d}/{d} idle-allocation=0 idle-jobs=0 present=none", .{
            self.frames, self.compared, self.max_error, self.queue_peak, self.engine.jobs.len });
    }
    fn sampleColor(x: usize, y: usize) u32 {
        return (@as(u32, @intCast(x % 256)) << 16) | (@as(u32, @intCast(y)) * 64 << 8) | 0x33;
    }
    fn paintReference(self: *Owner, painter: *scene_buffer.SceneBuffer) !void {
        switch (self.reference_case.?) {
            .basic => try reference.basic(painter, &self.ctx.draw),
            .shapes => try reference.shapes(painter, self.large_source, self.ctx.allocator()),
            .centers => {
                var pixels: [9]u32 = undefined;
                for (&pixels, 0..) |*pixel, index| pixel.* = sampleColor(index % 3, index / 3);
                if (!painter.blitXrgb32Nearest(painter.fullRect(), .{ .pixels = &pixels,
                    .source_x = 0, .source_y = 0, .source_w = 3, .source_h = 3, .source_stride = 3,
                    .guest_w = 3, .guest_h = 3, .viewport = .{ .x = 0, .y = 0, .w = 8, .h = 8 } })) return error.ReferencePaint;
            },
            .chunks => {
                // Two128-pixel transport blocks and a final partial block;
                // their common guest geometry is257x2 ->8x8, not integral.
                var pixels: [257 * 2]u32 = undefined;
                for (&pixels, 0..) |*pixel, index| pixel.* = sampleColor(index % 257, index / 257);
                var start: u32 = 0;
                while (start < 257) : (start += 128) {
                    const count: u32 = @min(128, 257 - start);
                    const x0 = (start * 8 + 256) / 257;
                    const x1 = ((start + count) * 8 + 256) / 257;
                    if (x0 == x1) continue;
                    if (!painter.blitXrgb32Nearest(.{ .x = @intCast(x0), .y = 0, .w = @intCast(x1 - x0), .h = 8 },
                        .{ .pixels = pixels[start..], .source_x = start, .source_y = 0, .source_w = count, .source_h = 2,
                            .source_stride = 257, .guest_w = 257, .guest_h = 2, .viewport = .{ .x = 0, .y = 0, .w = 8, .h = 8 } })) return error.ReferencePaint;
                }
            },
        }
    }
    fn checkReference(self: *Owner, kind: Reference) !void {
        self.reference_case = kind;
        if (!self.scene.attach(std.mem.sliceAsBytes(self.untouched), 8, 8) or
            !self.reference.attach(std.mem.sliceAsBytes(self.expected), 8, 8)) return error.State;
        if (kind == .shapes) {
            self.large_source = try self.ctx.allocator().alloc(u32, 1024 * 512);
            for (self.large_source, 0..) |*pixel, index| pixel.* = @as(u32, @intCast(index)) & 0xffffff;
        }
        const bounds = self.scene.fullRect();
        self.log("reference={s} original-geometry=8x8", .{@tagName(kind)});
        if (kind == .basic) {
            self.stage = "zero-budget";
            try self.capture(false, bounds);
            self.engine.budget_bytes = 0;
            if (self.engine.prepare(&self.cache, self.now() + timeout)) |_| return error.BudgetAccepted
            else |err| if (err != error.Limit) return err;
            if (self.engine.reserved_bytes != 0) return error.BudgetCharge;
            self.engine.budget_bytes = 256 * 1024 * 1024;
        }
        try self.frame(@tagName(kind), bounds);
        const uploads = self.engine.uploaded_bytes;
        const converted = self.primitives.assets.converted_pixels;
        const draws = self.engine.primitive_draws;
        try self.frame("reference-warm", bounds);
        if (self.engine.uploaded_bytes != uploads or self.primitives.assets.converted_pixels != converted or self.engine.primitive_draws != draws)
            return error.ResidentReupload;
        if (self.cache.reserved != 0 or self.primitives.assets.reserved > self.primitives.assets.budget or
            self.engine.reserved_bytes > self.engine.budget_bytes or self.engine.staging.info.image.byte_length != 1024 * 1024)
            return error.BudgetCharge;
        if (kind == .shapes and (self.engine.primitive_batch_peak != 16 or uploads != (1024 * 512 + 512 * 512) * 4)) return error.Batch;
        if (kind == .chunks and self.primitives.fractional_pixels != 128) return error.ChunkMapping;
        if (kind == .basic) try self.remoteDemand();
        // Public compositor Busy is checked while a real frame owns its
        // captured sources; no extra commands are admitted on the retry.
        try self.capture(false, bounds);
        try self.engine.begin(&self.cache, self.now() + timeout);
        if (self.engine.begin(&self.cache, self.now() + timeout)) |_| return error.BusyAccepted
        else |err| if (err != error.Busy) return err;
        if (self.engine.prepare(&self.cache, self.now() + timeout)) |_| return error.BusyAccepted
        else |err| if (err != error.Busy) return err;
        try self.advance(.copied);
        self.log("PASS reference={s} pixels={d} max-LSB={d} batch-peak={d} uploaded={d} source-cache={d}/{d} staging={d} warm-uploads=0 warm-conversions=0 busy=2",
            .{ @tagName(kind), self.compared, self.max_error, self.engine.primitive_batch_peak, uploads,
                self.primitives.assets.reserved, self.primitives.assets.budget, self.engine.staging.info.image.byte_length });
    }
    fn remoteDemand(self: *Owner) !void {
        self.stage = "remote-demand";
        const capture_owner = &self.remote;
        const view: @import("output_geometry.zig").topology.Viewport = .{ .pixel_w = 8, .pixel_h = 8 };
        const source = self.engine.outputs[0].resource;
        const bounds = self.scene.fullRect();
        capture_owner.record(0, 1, bounds, view, .{});
        capture_owner.complete(0, source, self.now());
        if (capture_owner.reader.stats.frames != 0 or capture_owner.reader.staging.slot != 0 or capture_owner.reader.pixels.len != 0)
            return error.UnwantedCapture;
        capture_owner.setDemand(true);
        try capture_owner.prepare(view, self.engine.output_format, self.engine.output_color);
        capture_owner.record(0, 2, bounds, view, .{});
        // This private consumer is allowed after retired rendering. No Window
        // or RemoteFrame publication and no display visibility is asserted.
        capture_owner.complete(0, source, self.now());
        const until = self.now() + timeout;
        while (!capture_owner.ready) {
            capture_owner.poll(self.now()); self.ctx.taskYield();
            if (self.now() > until) return error.CaptureFailed;
        }
        const image = capture_owner.image() orelse return error.CaptureFailed;
        if (!std.mem.eql(u32, self.reader.pixels, image.pixels.?)) return error.CapturePixels;
        if (capture_owner.reader.sourceHeld() or capture_owner.reader.stats.frames != 1 or capture_owner.reader.stats.copy_bytes != 256)
            return error.CaptureRetention;
        capture_owner.acknowledge(true);
        capture_owner.setDemand(false);
        while (capture_owner.reader.pending()) {
            capture_owner.poll(self.now()); self.ctx.taskYield();
            if (self.now() > until) return error.CaptureFailed;
        }
        try capture_owner.close();
        self.log("remote-demand off=no-storage on=64-matching-pixels CE=256B off=retired publication=none", .{});
    }
    fn checkPublishedCapture(self: *Owner, client: bool) !void {
        const verification = @import("capture_verify.zig");
        var session = try verification.Session.init(self.ctx, client);
        defer session.close();
        self.fixture.cursor_visible = false;
        const capture_owner = &self.remote;
        const count: usize = if (client) 3 else 7;
        for (0..count) |phase| {
            self.stage = "capture-scene";
            if (phase == 1) { self.fixture.windows[2].x += 47; self.fixture.windows[2].y -= 19; }
            if (phase == 2) {
                self.fixture.menu_open = true;
                if (client) self.fixture.windows[2].w -= 53;
            }
            if (phase == 3) { self.fixture.menu_open = false; self.fixture.windows[2].w -= 53; }
            const detached = phase == 6 or (client and phase == 2);
            if (detached) {
                // Inject a source detach through the real capture owner, not
                // a physical connector event. Old CPU leases stay valid.
                capture_owner.invalidate();
                if (self.ctx.desk.remoteFrameSourceReset() != 0) return error.SourceReset;
                self.fixture.cursor_visible = true;
            }
            try self.frame("capture", full);
            var view: @import("output_geometry.zig").topology.Viewport = .{ .pixel_w = width, .pixel_h = height };
            if (phase == 4) view.scale = 150;
            if (phase == 5) view.rotation = .clockwise90;
            const bounds = try @import("output_geometry.zig").logical(view);
            const pointer: @import("remote_capture.zig").Cursor = .{
                .x = if (detached) 600 else if (phase == 1) -3 else if (phase == 2) bounds.w - 5 else 320,
                .y = if (detached) 300 else if (phase == 1) -2 else if (phase == 2) bounds.h - 6 else 160,
                .visible = true, .separate = !detached,
            };
            const source = self.engine.outputs[self.engine.output_index].resource;
            if (phase == 0) {
                capture_owner.record(0, self.cache.frame, bounds, view, pointer);
                capture_owner.complete(0, source, self.now());
                if (capture_owner.reader.stats.frames != 0 or capture_owner.reader.staging.slot != 0) return error.UnwantedCapture;
                try session.acquire();
            }
            capture_owner.setDemand(self.ctx.remoteFrameConsumers() != 0);
            try capture_owner.prepare(view, self.engine.output_format, self.engine.output_color);
            capture_owner.record(0, self.cache.frame, bounds, view, pointer);
            capture_owner.complete(0, source, self.now());
            for (capture_owner.reader.jobs) |job| if (job) |value| {
                self.log("capture CE timeline={d} point={d} device={d} reset={d} bytes={d}", .{
                    value.fence.timeline, value.fence.point, value.fence.device_generation, value.fence.reset_generation, value.bytes });
            };
            const deadline = self.now() + timeout;
            while (!capture_owner.ready) {
                capture_owner.poll(self.now()); self.ctx.taskYield();
                if (self.now() >= deadline) return error.CaptureFailed;
            }
            if (capture_owner.reader.sourceHeld() or capture_owner.reader.pending()) return error.CaptureRetention;
            var image = capture_owner.image() orelse return error.CaptureFailed;
            // Inverse orientation reference, independent of nativeIndex():
            // normal, 125% center samples, or a quarter-turn permutation.
            var maximum: u32 = 0;
            for (image.pixels.?, 0..) |actual, i| {
                const x = i % @as(usize, @intCast(image.width));
                const y = i / @as(usize, @intCast(image.width));
                const index = if (phase == 5) (height - 1 - x) * width + y else if (phase == 4)
                    ((2 * y + 1) * 5 / 8) * width + (2 * x + 1) * 5 / 8 else i;
                const expected = self.reference.pixels.?[index];
                for ([_]u5{ 0, 8, 16 }) |shift| maximum = @max(maximum,
                    @abs(@as(i32, @intCast((actual >> shift) & 255)) - @as(i32, @intCast((expected >> shift) & 255))));
            }
            if (maximum > 1) return error.OrientedPixels;
            self.stage = "capture-publication";
            try session.publish(&image, pointer, phase);
            capture_owner.acknowledge(true);
            self.log("capture phase={d} geometry={d}x{d} pixels={d} max-LSB={d} copy-bytes={d} cpu-read={d} oriented={d} source-held=false", .{
                phase, image.width, image.height, image.pixels.?.len, maximum,
                capture_owner.reader.stats.copy_bytes, capture_owner.reader.stats.cpu_read_bytes, capture_owner.oriented_bytes });
            try session.waitClient(phase);
        }
        try session.finish();
        const frames = capture_owner.reader.stats.frames;
        const bytes = capture_owner.reader.stats.copy_bytes;
        capture_owner.setDemand(false);
        capture_owner.poll(self.now());
        try capture_owner.close();
        capture_owner.record(0, self.cache.frame + 1, full, .{ .pixel_w = width, .pixel_h = height }, .{});
        capture_owner.complete(0, self.engine.outputs[self.engine.output_index].resource, self.now());
        if (capture_owner.reader.stats.frames != frames or capture_owner.reader.stats.copy_bytes != bytes or
            capture_owner.reader.staging.slot != 0 or capture_owner.reader.pixels.len != 0) return error.UnwantedCapture;
        self.log("capture last-reader=0 off-jobs=0 off-storage=0 frames={d} copy-bytes={d}", .{ frames, bytes });
    }
    fn destroy(self: *Owner) bool {
        self.remote.setDemand(false);
        self.reader.cancel(error.Stale);
        if (self.engine.active()) self.engine.cancel(error.Graphics);
        const deadline = self.now() + timeout + timeout;
        while (self.reader.pending() or self.engine.active() or self.remote.reader.pending()) {
            self.remote.poll(self.now());
            self.reader.poll(self.now());
            _ = self.engine.advance(&self.cache, self.now());
            if (self.now() >= deadline) return false;
            self.ctx.taskYield();
        }
        self.remote.close() catch return false;
        self.reader.close() catch return false;
        self.engine.close() catch return false;
        if (self.engine.reserved_bytes != 0) return false;
        self.log("retirement=complete GPU-reserved=0", .{});
        const allocator = self.ctx.allocator();
        self.cache.deinit(); self.cpu_cache.deinit(); self.primitives.deinit();
        allocator.free(self.untouched); allocator.free(self.expected);
        allocator.free(self.large_source);
        self.graphics.destroy(); allocator.destroy(self);
        return true;
    }
};
