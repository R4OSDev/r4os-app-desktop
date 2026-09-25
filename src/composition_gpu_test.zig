//! Bounded queue scheduling model for the actual desktop Engine. Resource
//! validation, CPU staging and pixel arithmetic use the real R4GFX provider.
//! NVIDIA packet execution and common BO retention have their owner tests.
const std = @import("std");
const t = std.testing;
const r4os = @import("r4os");
const a = r4os.abi;
const p = @import("r4gfx_device_provider");
const c = p.c;
const fixture = @import("gfx_renderer_test.zig");
const layers = @import("composition_layers.zig");
const scene = @import("scene_buffer.zig");
const gpu = @import("composition_gpu.zig");
const surface = @import("surface.zig");
const window_image = @import("window_image.zig");
const WindowMemory = struct {
    var descriptor: a.GfxBufferDescriptor = .{ .byte_length = 256, .width = 8, .height = 8, .format = c.format_argb8888,
        .plane_count = 1, .plane_pitches = .{32,0,0,0}, .usage = 31 };
    var refs: [32]bool = @splat(false);
    var serial: u64 = 10;
    var generations: [32]u64 = @splat(0);
    var pixels: [128]u32 = @splat(0);
    var cpu_ready = false;
    var cpu_failed = false;
    var map_busy = false;
    var unmap_busy = false;
    var mapped: usize = 0;
    pub fn query(_: @This(), fence: *const a.GfxFence, out: *a.GfxFenceStatus) i32 {
        out.* = .{ .fence = fence.*, .milestone = a.gfx_queue_milestone_cpu_stores,
            .result = if (cpu_failed) a.gfx_queue_result_failed else if (cpu_ready) a.gfx_queue_result_complete else a.gfx_queue_result_pending };
        return a.gfx_queue_ok;
    }
    pub fn map(_: @This(), _: *const a.GfxBufferHandle, bytes: u64, out: *a.GfxBufferMap) i32 {
        if (map_busy) return a.gfx_buffer_error_busy;
        std.debug.assert(bytes <= @sizeOf(@TypeOf(pixels)));
        mapped += 1;
        out.* = .{ .lease = .{ .id = 1, .generation = 1 }, .cpu_address = @intFromPtr(&pixels), .byte_length = bytes };
        return a.gfx_buffer_result_ok;
    }
    pub fn unmap(_: @This(), _: *const a.GfxBufferHandle) i32 {
        if (unmap_busy) return a.gfx_buffer_error_busy;
        std.debug.assert(mapped != 0); mapped -= 1;
        return a.gfx_buffer_result_ok;
    }
    fn source() a.GfxBufferReference { return .{ .reference = .{ .id = 1, .generation = 1 }, .buffer = .{ .id = 44, .generation = 7 } }; }
    pub fn import(_: @This(), input: *const a.GfxBufferHandle, out: *a.GfxBufferReference) i32 {
        if (input.id == 0 or input.id > refs.len or !refs[input.id-1] or generations[input.id-1] != input.generation) return -1;
        const slot = for (refs, 0..) |live, i| { if (!live) break i; } else return -1;
        serial += 1; refs[slot] = true; generations[slot] = serial;
        out.* = .{ .reference = .{ .id = @intCast(slot+1), .generation = serial }, .buffer = source().buffer };
        return 1;
    }
    pub fn describe(_: @This(), input: *const a.GfxBufferHandle, out: *a.GfxBufferDescriptor) i32 {
        if (input.id == 0 or input.id > refs.len or !refs[input.id-1] or generations[input.id-1] != input.generation) return -1;
        out.* = descriptor; return 1;
    }
    pub fn release(_: @This(), input: *const a.GfxBufferHandle) i32 {
        if (input.id == 0 or input.id > refs.len or !refs[input.id-1] or generations[input.id-1] != input.generation) return -1;
        refs[input.id-1] = false; return 1;
    }
    fn count() usize { var n: usize = 0; for (refs) |live| { n += @intFromBool(live); } return n; }
};
const Model = struct {
    const s = p.swapchain.lifecycle;
    const List = struct { requests: [c.render_list_capacity]c.R4GfxRenderRequest = undefined,
        grids: [c.render_list_capacity]c.R4GfxLogicalGrid = @splat(std.mem.zeroes(c.R4GfxLogicalGrid)), count: usize = 0, color_flags: ?u32 = null };
    const Operation = union(enum) { copy: c.R4GfxCopyRequestEx, draw: c.R4GfxRenderRequest, list: List, present: c.R4GfxImagePresentRequest };
    const Job = struct { handle: c.R4GfxJob, operation: Operation, dependency: u64, external_wait: bool = false, result: u32 = 0, terminal: bool = false, cancelled: bool = false, pins: u32 = 0 };
    var buffers: [c.device_resource_capacity]?[]align(4) u8 = @splat(null);
    var imported: [c.device_resource_capacity]a.GfxBufferReference = @splat(.{});
    var producer_ready = true;
    var hold_terminal = false;
    var combined_enabled = true;
    var jobs: [c.device_job_capacity]?Job = @splat(null);
    var outcomes: [2048]u32 = @splat(0);
    var serial: u64 = 0;
    // Retired client handles do not free common receipts while descendants
    // still reference them. Exercise a smaller finite pool than the kernel.
    const Receipt = struct { live: bool = false, client: bool = false, parent: u64 = 0, children: u32 = 0 };
    var receipts: [2048]Receipt = @splat(.{});
    var retained_limit: usize = 0;
    var retained_count: usize = 0;
    var retained_peak: usize = 0;
    var native_allocations: u32 = 0;
    var presents: u32 = 0;
    var busy_count: u32 = 0;
    var reject_draw = false;
    var batches: usize = 0;
    var max_batch: usize = 0;
    var visible: [64]u32 = @splat(0);
    var chain_enabled = false;
    var chain_format: u32 = c.format_xrgb8888;
    var color_enabled = true;
    var color_batches: usize = 0;
    var allow_visible = false;
    var staged: [64]u32 = @splat(0);
    var clock: u64 = 1;
    var chain: s.Chain = .{};
    var chain_images: [3]c.R4GfxResource = undefined;
    var chain_render: [3]?c.R4GfxJob = @splat(null);
    var chain_present: [3]?c.R4GfxJob = @splat(null);
    const table: c.DeviceV1 = blk: {
        var value = fixture.table;
        value.device_refresh = refresh; value.resource_create = create; value.resource_release = release;
        value.copy_submit_ex = copy; value.render_submit = render; value.image_present = present;
        value.resource_info = resourceInfo;
        value.render_submit_list = renderList;
        value.render_submit_grid_list = renderGridList;
        value.job_info = info; value.job_fence = fence; value.job_cancel = cancel; value.job_release = releaseJob;
        value.presentation_info = presentationInfo; value.swapchain_open = chainOpen; value.swapchain_acquire = chainAcquire;
        value.swapchain_present = chainPresent; value.swapchain_poll = chainPoll; value.swapchain_release = chainRelease; value.swapchain_close = chainClose;
        break :blk value;
    };
    const colors: c.ColorV1 = blk: {
        var value = p.color_api.table;
        value.color_resource_create = createColor;
        value.color_render_submit = renderColorList;
        value.color_render_submit_grid = renderColorGridList;
        break :blk value;
    };
    fn presentationInfo(_: *const c.R4GfxDevice, head: u32, out: *c.R4GfxPresentationInfo) callconv(.c) i32 {
        if (!chain_enabled or head != 0) return c.status_unsupported;
        out.* = std.mem.zeroes(c.R4GfxPresentationInfo);
        out.version = 1; out.size = @sizeOf(c.R4GfxPresentationInfo); out.width = 8; out.height = 8;
        out.format = chain_format;
        out.flags = c.present_native | c.present_synchronized | c.present_visibility | c.present_active;
        out.device_generation = 1; out.reset_generation = 1;
        out.display_generation = 1; out.sequence = 1; out.policies = 3; out.buffer_count = 2; out.plane_count = 1; out.path = 1;
        return c.status_ok;
    }
    fn chainOpen(device: *const c.R4GfxDevice, request: *const c.R4GfxSwapchainDesc, out: *c.R4GfxSwapchain) callconv(.c) i32 {
        std.debug.assert(chain_enabled and request.count == 2 and chain_render[0] == null and request.policy == c.present_policy_fifo);
        chain.configure(.{ .count = request.count, .require_vsync = true }, .{ .generation = 1, .width = 8, .height = 8,
            .policies = 3, .synchronized = true, .visibility = true }) catch return c.status_busy;
        @memcpy(chain_images[0..request.count], @as([*]const c.R4GfxResource, @ptrFromInt(request.images))[0..request.count]);
        out.* = .{ .slot = 1, .reserved = 0, .generation = chain.generation, .device_generation = device.generation, .device_address = device.address };
        return 0;
    }
    fn key(frame: c.R4GfxSwapchainFrame) s.Token { return .{ .slot = frame.slot, .generation = frame.generation, .serial = frame.serial }; }
    fn chainFrameValue(token: s.Token) c.R4GfxSwapchainFrame {
        return .{ .slot = token.slot, .reserved = 0, .generation = token.generation, .serial = token.serial,
            .image = if (token.slot == 0) std.mem.zeroes(c.R4GfxResource) else chain_images[token.slot - 1] };
    }
    fn chainAcquire(_: *const c.R4GfxDevice, _: *const c.R4GfxSwapchain, input: u64, out: *c.R4GfxSwapchainFrame) callconv(.c) i32 {
        clock += 1; out.* = chainFrameValue(chain.acquire(clock, input) catch return c.status_busy); return 0;
    }
    fn chainPresent(_: *const c.R4GfxDevice, _: *const c.R4GfxSwapchain, request: *const c.R4GfxSwapchainPresent) callconv(.c) i32 {
        clock += 1;
        chain.present(key(request.frame), clock, .composition, request.render_job.slot != 0) catch return c.status_invalid;
        if (request.render_job.slot != 0) {
            jobs[request.render_job.slot - 1].?.pins += 1;
            chain_render[request.frame.slot - 1] = request.render_job;
        }
        return 0;
    }
    fn chainFrameStatus(index: usize) c.R4GfxSwapchainFrameStatus {
        const frame = &chain.frames[index]; const stamp = frame.times;
        return .{ .frame = chainFrameValue(frame.token), .phase = @intFromEnum(frame.phase), .result = @intFromEnum(frame.result), .path = @intFromEnum(frame.path),
            .held_flags = @as(u32, @intFromBool(frame.render_held)) | (@as(u32, @intFromBool(frame.consumer_held)) << 1),
            .input_ns = stamp.input_ns, .acquired_ns = stamp.acquired_ns, .queued_ns = stamp.queued_ns, .render_end_ns = stamp.render_end_ns,
            .selected_ns = stamp.selected_ns, .submitted_ns = stamp.submitted_ns, .copied_ns = stamp.copied_ns,
            .visible_ns = stamp.visible_ns, .released_ns = stamp.released_ns };
    }
    fn chainPoll(device: *const c.R4GfxDevice, _: *const c.R4GfxSwapchain, out: *c.R4GfxSwapchainStatus) callconv(.c) i32 {
        clock += 1;
        for (&chain.frames, 0..) |*frame, i| {
            if (chain_render[i]) |handle| {
                if (chain.life != .active) _ = cancel(device, &handle);
                const job = &jobs[handle.slot - 1].?;
                if (job.terminal) {
                    chain.rendered(frame.token, clock, job.result == a.gfx_queue_result_complete) catch return c.status_invalid;
                    job.pins -= 1; chain_render[i] = null;
                }
            }
            if (chain_present[i]) |handle| {
                if (chain.life != .active) _ = cancel(device, &handle);
                const job = jobs[handle.slot - 1].?;
                if (job.terminal) {
                    chain.retired(frame.token, clock, job.result == a.gfx_queue_result_complete) catch return c.status_invalid;
                    std.debug.assert(releaseJob(device, &handle) == 0); chain_present[i] = null;
                }
            }
            if (allow_visible and frame.phase == .submitted and !frame.consumer_held) {
                visible = staged;
                chain.visible(frame.token, clock) catch return c.status_invalid;
            }
        }
        if (chain.candidate()) |token| {
            var job: c.R4GfxJob = undefined;
            const rc = present(device, &.{ .version = 1, .size = @sizeOf(c.R4GfxImagePresentRequest), .source = chain_images[token.slot - 1],
                .frame_key = token.serial, .deadline_ns = 1000, .dependency_count = 0, .dependencies = 0, .reserved = 0 }, &job);
            if (rc == 0) { chain_present[token.slot - 1] = job; chain.submitted(token, clock) catch unreachable; }
        }
        out.* = .{ .version = 1, .size = @sizeOf(c.R4GfxSwapchainStatus), .life = @intFromEnum(chain.life), .policy = 0, .count = 2,
            .queued_count = 0, .held_count = 0, .path = 1, .generation = chain.generation, .next_start_ns = chain.next_start_ns,
            .frame0 = chainFrameStatus(0), .frame1 = chainFrameStatus(1), .frame2 = chainFrameStatus(2) };
        for (&chain.frames) |*frame| { out.queued_count += @intFromBool(frame.phase == .queued); out.held_count += @intFromBool(frame.consumer_held or frame.render_held); }
        return 0;
    }
    fn chainRelease(_: *const c.R4GfxDevice, _: *const c.R4GfxSwapchain, frame: *const c.R4GfxSwapchainFrame) callconv(.c) i32 {
        chain.release(key(frame.*)) catch return c.status_busy; return 0;
    }
    fn chainClose(device: *const c.R4GfxDevice, handle: *const c.R4GfxSwapchain) callconv(.c) i32 {
        chain.change(.closing);
        var snapshot: c.R4GfxSwapchainStatus = undefined;
        const rc = chainPoll(device, handle, &snapshot); if (rc != 0) return rc;
        for (&chain.frames) |*frame| if (frame.phase != .free) { chain.release(frame.token) catch return c.status_busy; };
        return 0;
    }
    fn refresh(device: *const c.R4GfxDevice, out: *c.R4GfxDeviceInfo) callconv(.c) i32 {
        const rc = p.refresh(device, out);
        if (rc == 0) out.gpu_operations = c.device_gpu_copy_rows | c.device_gpu_render | c.device_gpu_present | c.device_gpu_render_list | c.device_gpu_grid |
            @as(u32, if (color_enabled) c.device_gpu_color else 0) | @as(u32, if (combined_enabled) c.device_gpu_color_grid else 0);
        return rc;
    }
    fn create(device: *const c.R4GfxDevice, input: *const c.R4GfxResourceDesc, out: *c.R4GfxResource) callconv(.c) i32 {
        return createImage(device, input, null, out);
    }
    fn createColor(device: *const c.R4GfxDevice, input: *const c.R4GfxColorResourceDesc, out: *c.R4GfxResource) callconv(.c) i32 {
        return createImage(device, &input.resource, input.description, out);
    }
    fn createImage(device: *const c.R4GfxDevice, input: *const c.R4GfxResourceDesc, description: ?c.R4GfxColorDescription, out: *c.R4GfxResource) i32 {
        if (input.source_kind == c.source_import_buffer) {
            var ref: a.GfxBufferReference = .{};
            if (WindowMemory.import(.{}, @ptrFromInt(input.source_address), &ref) != 1) return c.status_stale;
            var desc = input.*;
            desc.source_kind = c.source_borrow_cpu; desc.source_address = 0; desc.source_generation = ref.reference.generation;
            const source = WindowMemory.descriptor;
            desc.image = .{ .cpu_address = @intFromPtr(&WindowMemory.pixels), .byte_length = source.byte_length,
                .pitch = source.plane_pitches[0], .width = source.width, .height = source.height, .format = source.format, .reserved = 0 };
            const rc = p.color_api.table.color_resource_create(device, &.{ .version = 1, .size = @sizeOf(c.R4GfxColorResourceDesc),
                .resource = desc, .description = description.? }, out);
            if (rc == c.status_ok) imported[out.slot-1] = ref else _ = WindowMemory.release(.{}, &ref.reference);
            return rc;
        }
        if (input.source_kind == c.source_color_view) {
            // This queue model uses borrowed host arrays for device storage.
            // BO import/retention of the productive view has its owner check.
            const source: *const c.R4GfxResource = @ptrFromInt(input.source_address);
            var desc = input.*;
            desc.source_kind = c.source_borrow_cpu; desc.source_address = 0; desc.source_generation = source.generation;
            desc.image = image(device, source);
            return p.color_api.table.color_resource_create(device, &.{ .version = 1, .size = @sizeOf(c.R4GfxColorResourceDesc),
                .resource = desc, .description = description.? }, out);
        }
        if (input.kind != c.resource_image or (input.source_kind != c.source_create_native and input.source_kind != c.source_create_system))
            return if (description) |value| p.color_api.table.color_resource_create(device,
                &.{ .version = 1, .size = @sizeOf(c.R4GfxColorResourceDesc), .resource = input.*, .description = value }, out)
                else p.createResource(device,input,out);
        var desc = input.*;
        var bytes: u64 = 0;
        if (input.source_kind == c.source_create_native) {
            const request: *const c.R4GfxNativeImage = @ptrFromInt(input.source_address);
            const pitch = (@as(u64,request.width) * @as(u64, if (request.format == c.format_abgr16161616f) 8 else 4) + 255) & ~@as(u64,255);
            bytes = pitch * request.height;
            desc.image = .{ .cpu_address=0, .byte_length=bytes, .pitch=pitch, .width=request.width, .height=request.height, .format=request.format, .reserved=0 };
        } else bytes = desc.image.byte_length;
        const memory = t.allocator.alignedAlloc(u8,.fromByteUnits(4),@intCast(bytes)) catch return c.status_limit;
        @memset(memory,0);
        desc.source_kind = c.source_borrow_cpu; desc.source_address = 0; desc.source_generation = 1;
        desc.image.cpu_address = @intFromPtr(memory.ptr);
        const rc = if (description) |value| p.color_api.table.color_resource_create(device,
            &.{ .version = 1, .size = @sizeOf(c.R4GfxColorResourceDesc), .resource = desc, .description = value }, out)
            else p.createResource(device,&desc,out);
        if (rc == 0) {
            buffers[out.slot-1] = memory;
            if (input.source_kind == c.source_create_native) native_allocations += 1;
        } else t.allocator.free(memory);
        return rc;
    }
    fn release(device: *const c.R4GfxDevice, resource: *const c.R4GfxResource) callconv(.c) i32 {
        for (&jobs) |*slot| if (slot.*) |job| if (!job.terminal) {
            const source = switch (job.operation) { .copy => |v| v.copy.source, .draw => |v| v.source, .list => |v| v.requests[0].source, .present => |v| v.source };
            const target = switch (job.operation) { .copy => |v| v.copy.target, .draw => |v| v.target, .list => |v| v.requests[0].target, .present => std.mem.zeroes(c.R4GfxResource) };
            std.debug.assert(!std.meta.eql(resource.*,source) and !std.meta.eql(resource.*,target));
        };
        const rc = p.releaseResource(device,resource);
        var remaining: c.R4GfxResourceInfo = undefined;
        if (rc == 0 and p.resourceInfo(device, resource, &remaining) != c.status_ok) {
            if (buffers[resource.slot-1]) |memory| { t.allocator.free(memory); buffers[resource.slot-1] = null; }
            if (imported[resource.slot-1].reference.id != 0) {
                std.debug.assert(WindowMemory.release(.{}, &imported[resource.slot-1].reference) == 1);
                imported[resource.slot-1] = .{};
            }
        }
        return rc;
    }
    fn resourceInfo(device: *const c.R4GfxDevice, resource: *const c.R4GfxResource, out: *c.R4GfxResourceInfo) callconv(.c) i32 {
        const rc = p.resourceInfo(device, resource, out);
        if (rc == 0 and imported[resource.slot-1].reference.id != 0) {
            out.buffer_id = imported[resource.slot-1].buffer.id; out.buffer_generation = imported[resource.slot-1].buffer.generation;
        }
        return rc;
    }
    fn submit(device: *const c.R4GfxDevice, operation: Operation, count: u32, address: u64, out: *c.R4GfxJob) i32 {
        if (busy_count != 0) { busy_count -= 1; return c.status_busy; }
        if (retained_limit != 0 and retained_count >= retained_limit) return c.status_busy;
        const index = for (&jobs,0..) |*slot,i| { if (slot.* == null) break i; } else return c.status_busy;
        std.debug.assert(count <= 2 and serial+1 < outcomes.len);
        var dependency: u64 = 0; var external_wait = false;
        if (count != 0) for (@as([*]const c.R4GfxCopyFence, @ptrFromInt(address))[0..count]) |value| {
            if (value.timeline == 456) { std.debug.assert(value.point == 1); external_wait = true; }
            else { std.debug.assert(value.timeline == 123); dependency = value.point; }
        };
        serial += 1;
        if (retained_limit != 0) {
            if (dependency != 0) {
                std.debug.assert(receipts[dependency].live);
                receipts[dependency].children += 1;
            }
            receipts[serial] = .{ .live = true, .client = true, .parent = dependency };
            retained_count += 1;
            retained_peak = @max(retained_peak, retained_count);
        }
        const handle: c.R4GfxJob = .{ .slot=@intCast(index+1), .reserved=0, .generation=serial, .device_generation=device.generation, .device_address=device.address };
        jobs[index] = .{ .handle=handle, .operation=operation, .dependency=dependency, .external_wait=external_wait }; out.*=handle;
        return 0;
    }
    fn copy(device: *const c.R4GfxDevice, input: *const c.R4GfxCopyRequestEx, out: *c.R4GfxJob) callconv(.c) i32 {
        return submit(device,.{.copy=input.*},input.dependency_count,input.dependencies,out);
    }
    fn render(device: *const c.R4GfxDevice, input: *const c.R4GfxRenderRequest, out: *c.R4GfxJob) callconv(.c) i32 {
        if (reject_draw) return c.status_unsupported;
        return submit(device,.{.draw=input.*},input.dependency_count,input.dependencies,out);
    }
    fn renderList(device: *const c.R4GfxDevice, input: *const c.R4GfxRenderListRequest, out: *c.R4GfxJob) callconv(.c) i32 {
        if (reject_draw) return c.status_unsupported;
        std.debug.assert(input.version == 1 and input.size == @sizeOf(c.R4GfxRenderListRequest) and input.reserved == 0 and input.count > 0 and input.count <= c.render_list_capacity);
        const requests: [*]const c.R4GfxRenderRequest = @ptrFromInt(input.commands);
        var list: List = .{ .count = input.count };
        @memcpy(list.requests[0..input.count], requests[0..input.count]);
        for (list.requests[1..input.count]) |request| std.debug.assert(std.meta.eql(request.source, requests[0].source) and
            std.meta.eql(request.target, requests[0].target) and std.meta.eql(request.pipeline, requests[0].pipeline) and
            std.meta.eql(request.sampler, requests[0].sampler) and request.transfer == requests[0].transfer and
            request.deadline_ns == requests[0].deadline_ns and request.dependencies == 0 and request.dependency_count == 0);
        const rc = submit(device, .{ .list = list }, requests[0].dependency_count, requests[0].dependencies, out);
        if (rc == 0) { batches += 1; max_batch = @max(max_batch, input.count); }
        return rc;
    }
    fn renderColorList(device: *const c.R4GfxDevice, input: *const c.R4GfxRenderListRequest, flags: u32, out: *c.R4GfxJob) callconv(.c) i32 {
        if (!color_enabled) return c.status_unsupported;
        std.debug.assert(flags == c.color_transform_relative_white or flags == c.color_transform_output | c.color_transform_dither or
            flags == c.color_transform_output | c.color_transform_relative_white | c.color_transform_dither);
        const rc = renderList(device, input, out);
        if (rc == 0) { jobs[out.slot - 1].?.operation.list.color_flags = flags; color_batches += 1; }
        return rc;
    }
    fn renderGridList(device: *const c.R4GfxDevice, input: *const c.R4GfxRenderGridListRequest, out: *c.R4GfxJob) callconv(.c) i32 {
        if (reject_draw) return c.status_unsupported;
        std.debug.assert(input.version == 1 and input.size == @sizeOf(c.R4GfxRenderGridListRequest) and input.count > 0 and input.count <= c.render_list_capacity);
        const requests: [*]const c.R4GfxRenderRequest = @ptrFromInt(input.commands);
        const grids: [*]const c.R4GfxLogicalGrid = @ptrFromInt(input.grids);
        var list: List = .{ .count = input.count };
        @memcpy(list.requests[0..input.count], requests[0..input.count]);
        @memcpy(list.grids[0..input.count], grids[0..input.count]);
        const rc = submit(device, .{ .list = list }, requests[0].dependency_count, requests[0].dependencies, out);
        if (rc == 0) { batches += 1; max_batch = @max(max_batch, input.count); }
        return rc;
    }
    fn renderColorGridList(device: *const c.R4GfxDevice, input: *const c.R4GfxRenderGridListRequest, flags: u32, out: *c.R4GfxJob) callconv(.c) i32 {
        if (!combined_enabled) return c.status_unsupported;
        std.debug.assert(flags == 0 or flags == c.color_transform_relative_white);
        const rc = renderGridList(device, input, out);
        if (rc == 0) { jobs[out.slot-1].?.operation.list.color_flags = flags; color_batches += 1; }
        return rc;
    }
    fn present(device: *const c.R4GfxDevice, input: *const c.R4GfxImagePresentRequest, out: *c.R4GfxJob) callconv(.c) i32 {
        return submit(device,.{.present=input.*},input.dependency_count,input.dependencies,out);
    }
    fn find(handle: *const c.R4GfxJob) *Job {
        const job = &jobs[handle.slot-1].?; std.debug.assert(std.meta.eql(handle.*,job.handle)); return job;
    }
    fn info(_: *const c.R4GfxDevice, handle: *const c.R4GfxJob, out: *c.R4GfxJobInfo) callconv(.c) i32 {
        const job = find(handle); out.* = std.mem.zeroes(c.R4GfxJobInfo);
        out.version=1; out.size=@sizeOf(c.R4GfxJobInfo); out.point=handle.generation; out.timeline=123;
        out.device_generation=1; out.reset_generation=1;
        out.phase=if(job.terminal) a.gfx_queue_phase_terminal else a.gfx_queue_phase_running;
        out.result=job.result; out.flags=if(job.terminal and !hold_terminal) 0 else 3; return 0;
    }
    fn fence(_: *const c.R4GfxDevice, handle: *const c.R4GfxJob, out: *c.R4GfxCopyFence) callconv(.c) i32 {
        _=find(handle); out.*=.{ .slot=handle.slot, .adapter_id=9, .timeline=123, .point=handle.generation, .device_generation=1, .reset_generation=1 }; return 0;
    }
    fn cancel(_: *const c.R4GfxDevice, handle: *const c.R4GfxJob) callconv(.c) i32 { find(handle).cancelled=true; return 0; }
    fn releaseJob(_: *const c.R4GfxDevice, handle: *const c.R4GfxJob) callconv(.c) i32 {
        if (!find(handle).terminal or find(handle).pins != 0 or hold_terminal) return c.status_busy;
        jobs[handle.slot-1]=null;
        if (retained_limit != 0) {
            std.debug.assert(receipts[handle.generation].live and receipts[handle.generation].client);
            receipts[handle.generation].client = false;
            var changed = true;
            while (changed) {
                changed = false;
                for (&receipts) |*item| if (item.live and !item.client and item.children == 0) {
                    if (item.parent != 0) { std.debug.assert(receipts[item.parent].children != 0); receipts[item.parent].children -= 1; }
                    item.* = .{}; retained_count -= 1; changed = true;
                };
            }
        }
        return 0;
    }
    fn image(device: *const c.R4GfxDevice, resource: *const c.R4GfxResource) c.R4GfxCpuImage {
        var value: c.R4GfxResourceInfo=undefined; std.debug.assert(p.resourceInfo(device,resource,&value)==0); return value.image;
    }
    fn executeDraw(device: *const c.R4GfxDevice, value: c.R4GfxRenderRequest, grid: c.R4GfxLogicalGrid, color_flags: ?u32) void {
                const state = p.get(device, false) catch unreachable;
                const target_resource = state.resource(value.target, true) catch unreachable;
                const source_resource = if (value.source.slot != 0) state.resource(value.source, true) catch unreachable else null;
                const pipeline = state.resource(value.pipeline, true) catch unreachable;
                if (color_flags == null) p.color_resource.nativeTransition(source_resource, target_resource, value.transfer, pipeline.operation, value.color) catch unreachable;
                // The model executes each clipped sample through the real CPU
                // provider. Target clipping never changes the source transform.
                for (0..value.scissor.height) |row| for (0..value.scissor.width) |column| {
                    const x = @as(i64, value.scissor.x) + @as(i64, @intCast(column));
                    const y = @as(i64, value.scissor.y) + @as(i64, @intCast(row));
                    const target = image(device, &value.target);
                    if (x < 0 or y < 0 or x >= target.width or y >= target.height or x < value.target_rect.x or y < value.target_rect.y or
                        x >= @as(i64, value.target_rect.x) + value.target_rect.width or y >= @as(i64, value.target_rect.y) + value.target_rect.height) continue;
                    var command: c.R4GfxDraw = .{ .source = value.source, .target = value.target, .pipeline = value.pipeline, .sampler = value.sampler,
                        .source_rect = std.mem.zeroes(c.R4GfxRect), .target_rect = .{ .x = @intCast(x), .y = @intCast(y), .width = 1, .height = 1 },
                        .color = value.color, .opacity = if (value.source.slot == 0) 0 else value.opacity };
                    if (value.source.slot != 0) command.source_rect = .{
                        .x = @intCast(@as(i64, value.source_rect.x) + @divTrunc((2 * (x - value.target_rect.x) + 1) * value.source_rect.width, 2 * @as(i64, value.target_rect.width))),
                        .y = @intCast(@as(i64, value.source_rect.y) + @divTrunc((2 * (y - value.target_rect.y) + 1) * value.source_rect.height, 2 * @as(i64, value.target_rect.height))),
                        .width = 1, .height = 1 };
                    if (grid.enabled != 0) {
                        const px = x + grid.target_x; const py = y + grid.target_y;
                        const oriented: [2]i64 = switch (grid.rotation) {
                            0 => .{ px, py }, 1 => .{ grid.pixel_height - 1 - py, px },
                            2 => .{ grid.pixel_width - 1 - px, grid.pixel_height - 1 - py },
                            3 => .{ py, grid.pixel_width - 1 - px }, else => unreachable,
                        };
                        const lx = @divFloor((2 * oriented[0] + 1) * 120, 2 * @as(i64, grid.scale)) - grid.viewport_x;
                        const ly = @divFloor((2 * oriented[1] + 1) * 120, 2 * @as(i64, grid.scale)) - grid.viewport_y;
                        command.source_rect.x = @intCast(value.source_rect.x + @divFloor(lx * grid.guest_width, grid.viewport_width) - grid.source_x);
                        command.source_rect.y = @intCast(value.source_rect.y + @divFloor(ly * grid.guest_height, grid.viewport_height) - grid.source_y);
                    }
                    if (target_resource.color) |description| {
                        std.debug.assert(grid.enabled == 0 or color_flags != null);
                        if (source_resource) |source| {
                            const from: c.R4GfxColorImage = .{ .version = 1, .size = @sizeOf(c.R4GfxColorImage), .image = source.image,
                                .description = source.color.?, .profile = std.mem.zeroes(c.R4GfxColorProfile) };
                            const to: c.R4GfxColorImage = .{ .version = 1, .size = @sizeOf(c.R4GfxColorImage), .image = target,
                                .description = description, .profile = std.mem.zeroes(c.R4GfxColorProfile) };
                            var stats: c.R4GfxCpuStats = undefined;
                            std.debug.assert(p.color_api.table.color_image_transform(&from, &to, &.{
                                .version = 1, .size = @sizeOf(c.R4GfxColorTransform), .source_rect = command.source_rect, .target_rect = command.target_rect,
                                .sampler = c.render_sampler_nearest, .operation = pipeline.operation, .opacity = value.opacity * 257,
                                .flags = color_flags orelse 0, .pixel_budget = 1 }, &stats) == 0);
                        } else {
                            std.debug.assert(target.format == c.format_abgr16161616f and value.color == 0);
                            const bytes: [*]u8 = @ptrFromInt(target.cpu_address);
                            @memset(bytes[@as(usize, @intCast(y)) * target.pitch + @as(usize, @intCast(x)) * 8..][0..8], 0);
                        }
                    } else {
                        var stats: c.R4GfxRenderStats = undefined;
                        std.debug.assert(p.render(device, &.{ .commands = @intFromPtr(&command), .command_count = 1, .flags = 0, .pixel_budget = 1 }, &stats) == 0);
                    }
                };
    }
    fn complete(device: *const c.R4GfxDevice) void {
        var selected: ?*Job = null;
        for (&jobs) |*slot| if (slot.*) |*job| if (!job.terminal) {
            if (selected == null or job.handle.generation < selected.?.handle.generation) selected=job;
        };
        const job = selected orelse return;
        if (job.external_wait and !producer_ready and !job.cancelled) return;
        if (job.dependency != 0 and outcomes[job.dependency]==0) return;
        job.result = if (job.cancelled or (job.dependency!=0 and outcomes[job.dependency]!=a.gfx_queue_result_complete)) a.gfx_queue_result_cancelled else a.gfx_queue_result_complete;
        if (job.result==a.gfx_queue_result_complete) switch(job.operation) {
            .copy => |value| {
                const src=image(device,&value.copy.source); const dst=image(device,&value.copy.target);
                const from: [*]const u8=@ptrFromInt(src.cpu_address); const to: [*]u8=@ptrFromInt(dst.cpu_address);
                for(0..value.row_count) |row| @memcpy(to[value.copy.target_offset+row*value.target_pitch..][0..value.copy.byte_length],from[value.copy.source_offset+row*value.source_pitch..][0..value.copy.byte_length]);
            },
            .draw => |value| executeDraw(device, value, std.mem.zeroes(c.R4GfxLogicalGrid), null),
            .list => |*value| for (value.requests[0..value.count], value.grids[0..value.count]) |request, grid| executeDraw(device, request, grid, value.color_flags),
            .present => |value| {
                const src=image(device,&value.source); const from:[*]const u8=@ptrFromInt(src.cpu_address);
                std.debug.assert(src.width==8 and src.height==8);
                const target_pixels = if (chain_enabled) &staged else &visible;
                for(0..8) |row| @memcpy(std.mem.sliceAsBytes(target_pixels[row*8..][0..8]),from[row*src.pitch..][0..32]);
                presents+=1;
            },
        };
        job.terminal=true; outcomes[job.handle.generation]=job.result;
    }
};

fn capture(cache:*layers.Cache, damage:surface.Rect, color:u32) !void {
    const screen:surface.Rect=.{.x=0,.y=0,.w=8,.h=8};
    try cache.start(screen);
    const background=(try cache.begin(1,screen,damage)).?; background.fillRect(background.fullRect(),0x203040); try cache.end(1);
    const area:surface.Rect=.{.x=2,.y=2,.w=4,.h=4};
    if(try cache.begin(2,area,damage)) |overlay| { overlay.fillRect(overlay.fullRect(),color); try cache.end(2); }
    _=try cache.finish();
}
fn pump(engine:*gpu.Engine, cache:*layers.Cache, device:*const c.R4GfxDevice, expected:gpu.Progress) !void {
    for(0..256) |_| {
        const progress=engine.advance(cache,1);
        if(progress!=.pending) {try t.expectEqual(expected,progress); return;}
        Model.complete(device);
    }
    return error.Stalled;
}
pub fn check() !void {
    Model.serial=0; Model.native_allocations=0; Model.presents=0; Model.busy_count=2; Model.reject_draw=false; Model.outcomes=@splat(0); Model.visible=@splat(0);
    var source:fixture.Fixture=.{.table_override=&Model.table,.color_override=&Model.colors};
    const graphics=try source.open(); defer graphics.destroy();
    var cache=layers.Cache.init(t.allocator,1024*1024); defer cache.deinit();
    var engine=gpu.Engine.init(&graphics.client,&graphics.colors,&graphics.device);
    const device:*const c.R4GfxDevice=@ptrCast(&graphics.device);
    const full:surface.Rect=.{.x=0,.y=0,.w=8,.h=8}; const pixel:surface.Rect=.{.x=3,.y=3,.w=1,.h=1};
    try capture(&cache,full,0x882244);
    try engine.prepare(&cache,1000); try engine.begin(&cache,1000);
    try pump(&engine,&cache,device,.copied);
    try t.expect(Model.presents==1 and Model.native_allocations==4 and engine.uploaded_bytes==320);
    var expected:[64]u32=undefined; var target:scene.SceneBuffer=.{};
    try t.expect(target.attach(std.mem.sliceAsBytes(&expected),8,8));
    _=try @import("composition_software.zig").paint(&graphics.colors,&cache,&target);
    try t.expectEqualSlices(u32,&expected,&Model.visible);
    const serial=Model.serial;
    for(0..8) |_| try t.expectEqual(gpu.Progress.copied,engine.advance(&cache,1));
    try t.expect(Model.serial==serial);
    try capture(&cache,pixel,0x445566);
    try t.expect(engine.prepared(&cache));
    try engine.begin(&cache,1000); try pump(&engine,&cache,device,.copied);
    _=try @import("composition_software.zig").paint(&graphics.colors,&cache,&target);
    try t.expectEqualSlices(u32,&expected,&Model.visible);
    try t.expect(Model.presents==2 and Model.native_allocations==4 and engine.uploaded_bytes==324);
    const last=Model.visible;
    try capture(&cache,pixel,0x998877); Model.reject_draw=true;
    try engine.begin(&cache,1000); try pump(&engine,&cache,device,.failed);
    try t.expectEqualSlices(u32,&last,&Model.visible);
    try t.expect(engine.needsFull(8,8));
    try t.expectError(error.Incomplete,engine.begin(&cache,1000));
    Model.reject_draw=false;
    try capture(&cache,full,0x998877); try engine.begin(&cache,1000);
    _=engine.advance(&cache,1); engine.cancel(error.Deadline);
    try t.expectError(error.Busy,engine.close());
    try pump(&engine,&cache,device,.failed);
    try t.expect(Model.presents==2);
    try capture(&cache,full,0x998877); try engine.begin(&cache,1000); try pump(&engine,&cache,device,.copied);
    _=try @import("composition_software.zig").paint(&graphics.colors,&cache,&target);
    try t.expectEqualSlices(u32,&expected,&Model.visible);
    try cache.start(full);
    const black = (try cache.begin(1, full, full)).?; black.fillRect(full, 0); try cache.end(1);
    const overlay = (try cache.begin(2, pixel, pixel)).?;
    const white = [_]u32{0x80ffffff};
    try t.expect(overlay.blendArgb32(pixel, pixel.x, pixel.y, 1, 1, 1, std.mem.sliceAsBytes(&white))); try cache.end(2);
    _ = try cache.finish();
    try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
    try t.expectEqual(@as(u32, 0xbcbcbc), Model.visible[3 * 8 + 3]);
    try t.expectEqual(@as(u32, 0), Model.visible[0]);
    try checkReadback(graphics, device, &engine, &cache);
    try engine.close();
    for(&Model.jobs) |*job| try t.expect(job.*==null);
    for(&Model.buffers) |*buffer| try t.expect(buffer.*==null);
    try t.expect(engine.reserved_bytes==0);
    try checkDependencyPressure(graphics, device);
    try checkPrimitives(graphics, device);
    try checkSwapchain(graphics, device);
    try checkHdrOutput(graphics, device);
    try checkWindowImages(graphics, device);
    try checkHdrWindows(graphics, device);
    try checkWindowTransport(graphics, device);
    try checkCpuWindow(graphics);
}

const WindowTransport = struct {
    surface: a.WindowGraphicsSurface = .{},
    config: a.WindowGraphicsConfig = .{},
    last: ?a.WindowGraphicsConsumer = null,
    reply: a.WindowGraphicsReply = .{},
    lose_publish: bool = true,
    lose_take: bool = true,
    lose_return: bool = true,
    available: bool = true,
    release_ready: bool = false,
    dead: bool = false,
    invalidated: bool = false,
    publishes: u32 = 0,
    takes: u32 = 0,
    returns: u32 = 0,
    borrowed: a.GfxFence = .{},
    cpu: bool = false,
    chain_closed: bool = false,
    lose_inspect: bool = false,
    revision: u64 = 1,
    pub fn invalidate(self: *@This()) void { self.invalidated = true; }
    pub fn serviceDead(self: *@This(), _: a.ProgramProcessHandle) bool { return self.dead; }
    pub fn publish(self: *@This(), request: *const a.WindowGraphicsPublication, out: *a.WindowGraphicsReply) bool {
        if (request.action == a.window_graphics_publish) {
            if (self.surface.serial == 0) {
                self.surface = .{ .service = .{ .instance_id = 31, .generation = 9 }, .desktop = request.desktop,
                    .owner = request.owner, .window_id = request.window_id, .serial = 42 };
                self.publishes += 1;
            }
            self.config = request.config;
        }
        out.* = .{ .result = a.window_graphics_ok, .surface = self.surface, .config = self.config };
        if (self.lose_publish) { self.lose_publish = false; return false; }
        return true;
    }
    pub fn consumer(self: *@This(), request: *const a.WindowGraphicsConsumer, out: *a.WindowGraphicsReply) bool {
        if (request.action == a.window_graphics_inspect) {
            std.debug.assert(request.request_serial == 0 and request.result == 0 and std.meta.eql(request.fence, a.GfxFence{}));
            out.* = .{ .result = if (self.chain_closed) a.window_graphics_closed else a.window_graphics_ok,
                .surface = self.surface, .chain = request.chain, .image_slot = request.image_slot,
                .acquire_token = request.acquire_token, .revision = self.revision };
            if (self.lose_inspect) { self.lose_inspect = false; return false; }
            return true;
        }
        if (self.last) |last| if (last.request_serial == request.request_serial) {
            std.debug.assert(std.meta.eql(last, request.*)); out.* = self.reply; return true;
        };
        out.* = .{ .result = a.window_graphics_ok, .surface = self.surface, .config = self.config, .revision = self.revision,
            .chain = request.chain, .image_slot = request.image_slot, .acquire_token = request.acquire_token };
        switch (request.action) {
            a.window_graphics_take => {
                if (!self.available) { out.result = a.window_graphics_not_ready; return true; }
                self.available = false; self.takes += 1;
                out.frame = windowMessage(); out.frame.surface = self.surface; out.frame.config_revision = self.config.revision;
                if (self.cpu) out.frame.ready.adapter_id = 0;
                out.chain = out.frame.chain; out.image_slot = out.frame.image_slot; out.acquire_token = out.frame.acquire_token;
            },
            a.window_graphics_return => {
                if (self.cpu) std.debug.assert(WindowMemory.mapped == 0);
                self.returns += 1; self.borrowed = request.fence;
                if (request.result != a.window_graphics_ok or self.chain_closed) { out.flags = a.window_graphics_fence_released; self.borrowed = .{}; }
            },
            a.window_graphics_release_fence => {
                std.debug.assert(std.meta.eql(self.borrowed, request.fence));
                if (!self.release_ready) { out.result = a.window_graphics_not_ready; return true; }
                self.borrowed = .{}; out.flags = a.window_graphics_fence_released;
            },
            else => unreachable,
        }
        self.last = request.*; self.reply = out.*;
        if (request.action == a.window_graphics_take and self.lose_take) { self.lose_take = false; return false; }
        if (request.action == a.window_graphics_return and self.lose_return) { self.lose_return = false; return false; }
        return true;
    }
};
fn checkCpuWindow(graphics: anytype) !void {
    const transport = @import("window_graphics.zig");
    var owner: transport.Window = .{};
    var server: WindowTransport = .{ .cpu = true, .lose_publish = false, .lose_take = false, .lose_return = true };
    const desktop: a.ProgramProcessHandle = .{ .instance_id = 5, .generation = 7 };
    var spec: transport.Spec = .{ .owner = .{ .instance_id = 17, .generation = 3 },
        .config = .{ .width = 8, .height = 8, .flags = a.window_graphics_visible } };
    WindowMemory.descriptor = .{ .byte_length = 256, .width = 8, .height = 8, .format = c.format_argb8888,
        .plane_count = 1, .plane_pitches = .{32, 0, 0, 0}, .usage = 31 };
    WindowMemory.refs = @splat(false); WindowMemory.refs[0] = true; WindowMemory.generations[0] = 1;
    WindowMemory.cpu_ready = false; WindowMemory.cpu_failed = false;
    @memset(&WindowMemory.pixels, 0x80808080);
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(owner.front() == null and owner.needsPolling() and WindowMemory.mapped == 0);
    WindowMemory.cpu_ready = true; WindowMemory.map_busy = true;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(owner.front() == null and WindowMemory.mapped == 0);
    WindowMemory.map_busy = false;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    const front = owner.front().?;
    try t.expect(WindowMemory.mapped == 1 and !owner.needsPolling());
    var cpu: @import("composition_software.zig").Owner = .{}; defer cpu.deinit();
    var pixels: [64]u32 = @splat(0x123456);
    var canvas: scene.SceneBuffer = .{};
    try t.expect(canvas.attach(std.mem.sliceAsBytes(&pixels), 8, 8));
    canvas.origin_x = -3; canvas.origin_y = 5;
    try cpu.begin(t.allocator, &canvas);
    const damage: surface.Rect = .{ .x = -2, .y = 6, .w = 2, .h = 2 };
    const base = (try cpu.cache.?.begin(1, canvas.fullRect(), damage)).?;
    base.fillRect(canvas.fullRect(), 0); try cpu.cache.?.end(1);
    try cpu.cache.?.external(2, canvas.fullRect(), damage, front);
    try t.expect(front.readers == 1 and !front.cpu_consumed);
    try cpu.finish(&graphics.colors, &canvas);
    for (pixels, 0..) |value, i| try t.expectEqual(@as(u32, if (i % 8 >= 1 and i % 8 <= 2 and i / 8 >= 1 and i / 8 <= 2) 0xbcbcbc else 0x123456), value);
    try t.expect(front.readers == 0 and front.cpu_consumed and WindowMemory.mapped == 1);
    // A pending replacement keeps the old front. A resize cancels preparation
    // and releases the CPU map before sending even a retried Return.
    server.available = true; WindowMemory.cpu_ready = false;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(owner.front() == front and WindowMemory.mapped == 1 and owner.needsPolling());
    spec.config.width = 9;
    WindowMemory.unmap_busy = true;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(owner.front() == null and server.returns == 0 and WindowMemory.mapped == 1);
    WindowMemory.unmap_busy = false;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(server.returns == 1 and WindowMemory.mapped == 0 and owner.request != null);
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(server.returns == 1 and owner.request == null);
    // Exact service death ends remaining metadata loans; all local imports
    // and mappings still run through the normal close owner.
    server.dead = true;
    owner.poll(desktop, 1, null, &server, WindowMemory{});
    try t.expect(WindowMemory.count() == 1 and WindowMemory.mapped == 0);
    try t.expectEqual(a.gfx_buffer_result_ok, WindowMemory.release(.{}, &WindowMemory.source().reference));
    // A producer may close its chain while its window remains visible. Even
    // an occluded, never-composited front must retire, without ending readers
    // early or dropping an uncertain Return reply.
    owner = .{};
    server = .{ .cpu = true, .lose_publish = false, .lose_take = false, .lose_return = true };
    spec.config.width = 8;
    WindowMemory.refs = @splat(false); WindowMemory.refs[0] = true; WindowMemory.generations[0] = 1;
    WindowMemory.cpu_ready = true;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    const retained = owner.front().?;
    retained.readers = 1;
    server.chain_closed = true; server.revision += 1; server.lose_inspect = true;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(owner.front() == retained and owner.needsPolling() and server.returns == 0);
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(owner.front() == null and retained.retired and !retained.failed and WindowMemory.mapped == 1);
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(server.returns == 0);
    retained.readers = 0;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(server.returns == 1 and owner.request != null and WindowMemory.mapped == 0);
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(server.returns == 1 and WindowMemory.count() == 1 and !owner.needsPolling());
    try t.expectEqual(a.gfx_buffer_result_ok, WindowMemory.release(.{}, &WindowMemory.source().reference));
    std.debug.print("[desktop-cpu-window] producer fence, read lease, linear alpha, sparse damage, pending replacement, resize and lost Return: OK\n", .{});
}
fn checkWindowTransport(graphics: anytype, device: *const c.R4GfxDevice) !void {
    const transport = @import("window_graphics.zig");
    var owner: transport.Window = .{};
    var server: WindowTransport = .{};
    const desktop: a.ProgramProcessHandle = .{ .instance_id = 5, .generation = 7 };
    var spec: transport.Spec = .{ .owner = .{ .instance_id = 17, .generation = 3 },
        .config = .{ .width = 8, .height = 8, .flags = a.window_graphics_visible } };
    WindowMemory.refs[0] = true; WindowMemory.generations[0] = 1;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(owner.publication != null and owner.surface.serial == 0 and server.publishes == 1);
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(owner.publication == null and owner.surface.serial == 42 and server.publishes == 1);
    spec.consumer_ready = false;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(owner.needsPolling() and server.takes == 0 and owner.request == null);
    spec.consumer_ready = true;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    const take = owner.request.?;
    try t.expect(owner.front() == null and server.takes == 1 and WindowMemory.count() == 1);
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    const front = owner.front().?;
    try t.expect(owner.request == null and server.takes == 1 and WindowMemory.count() == 2);
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(owner.serial == take.request_serial); // no replacement before composition
    try t.expect(!owner.needsPolling()); // fully occluded front does not spin
    try t.expectEqual(@as(i32, 1), WindowMemory.release(.{}, &WindowMemory.source().reference));
    Model.serial = 0; Model.outcomes = @splat(0); Model.producer_ready = true;
    var recording = try @import("primitive_frame.zig").Frame.init(t.allocator); defer recording.deinit(); recording.mirror = false;
    var cache = layers.Cache.init(t.allocator, 1024); defer cache.deinit(); cache.recording = &recording;
    var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
    try windowCapture(&cache, front, .{ .pixel_w = 8, .pixel_h = 8 });
    try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000);
    // Hide during an active GPU read. Return must wait for physical retirement.
    spec.config.flags = 0;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(owner.front() == null and front.retired and front.readers == 1 and server.returns == 0);
    try pump(&engine, &cache, device, .copied);
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    const returned = owner.request.?;
    try t.expect(server.returns == 1 and returned.fence.point != 0 and engine.external_receipts == 1);
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(server.returns == 1 and owner.request == null and engine.external_receipts == 1);
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    const releasing = owner.request.?;
    try t.expect(releasing.action == a.window_graphics_release_fence and releasing.request_serial > returned.request_serial);
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(std.meta.eql(releasing, owner.request.?) and engine.external_receipts == 1);
    server.release_ready = true;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(engine.external_receipts == 0 and !server.invalidated and server.borrowed.point == 0);
    try engine.close(); try t.expect(WindowMemory.count() == 0);

    // A transport loss is not a service-generation death. Keep an already
    // borrowed front through both, and clean it only after the read ends.
    owner = .{}; server = .{ .lose_publish = false, .lose_take = false, .lose_return = false };
    spec.config.flags = a.window_graphics_visible;
    WindowMemory.refs[0] = true; WindowMemory.generations[0] = 1;
    owner.poll(desktop, 1, spec, &server, WindowMemory{}); owner.poll(desktop, 1, spec, &server, WindowMemory{});
    const held = owner.front().?; try held.borrow();
    try t.expectEqual(@as(i32, 1), WindowMemory.release(.{}, &WindowMemory.source().reference));
    const old_owner = owner.owner;
    spec.owner.generation += 1;
    owner.poll(desktop, 1, spec, &server, WindowMemory{});
    try t.expect(std.meta.eql(owner.owner, old_owner) and owner.front() == null and held.retired and held.readers == 1);
    owner.disconnect();
    try t.expect(WindowMemory.count() == 1 and held.readers == 1);
    server.dead = true;
    owner.poll(desktop, 1, null, &server, WindowMemory{});
    try t.expect(WindowMemory.count() == 1 and owner.ended);
    try t.expect(held.finish(null, true));
    owner.poll(desktop, 1, null, &server, WindowMemory{});
    try t.expect(WindowMemory.count() == 0 and owner.surface.serial == 0 and !owner.needsPolling());
    std.debug.print("[desktop-window-transport] lost publish/Take/Return replies, FIFO admission, hide during draw, exact fence ack and service-death retirement: OK\n", .{});
}

fn windowCapture(cache: *layers.Cache, frame: *window_image.Frame, view: @import("output_geometry.zig").topology.Viewport) !void {
    const geometry = @import("output_geometry.zig");
    const bounds = try geometry.logical(view);
    try cache.startOutput(view);
    const background = (try cache.begin(1, bounds, bounds)).?;
    background.fillRect(bounds, 0); try cache.end(1);
    // Two disjoint damage pieces retain this front once, preserving sampling
    // against the full client bounds rather than stretching each clipped quad.
    const left: surface.Rect = .{ .x = bounds.x, .y = bounds.y, .w = 2, .h = bounds.h };
    try cache.external(80, bounds, left, frame);
    try cache.external(80, bounds, .{ .x = bounds.x+2, .y = bounds.y, .w = bounds.w-2, .h = bounds.h }, frame);
    const corner: surface.Rect = .{ .x = bounds.x, .y = bounds.y, .w = 1, .h = 1 };
    const overlay = (try cache.begin(2, corner, corner)).?;
    overlay.fillRect(corner, 0x007f00); try cache.end(2);
    _ = try cache.finish();
}
fn windowMessage() a.WindowGraphicsFrame {
    return .{ .surface = .{ .serial = 1, .window_id = 1, .owner = .{ .instance_id = 17, .generation = 3 } },
        .chain = 1, .acquire_token = 1, .present_serial = 1, .source = WindowMemory.source(), .descriptor = WindowMemory.descriptor,
        .format = .{ .format = WindowMemory.descriptor.format, .color = @bitCast(@import("composition_software.zig").description(false, false)) },
        .ready = .{ .slot = 1, .adapter_id = 9, .timeline = 456, .point = 1, .device_generation = 1, .reset_generation = 1 } };
}
fn checkWindowImages(graphics: anytype, device: *const c.R4GfxDevice) !void {
    const geometry = @import("output_geometry.zig");
    for (0..4) |rotation| {
        Model.serial = 0; Model.outcomes = @splat(0); Model.producer_ready = false;
        WindowMemory.refs = @splat(false); WindowMemory.refs[0] = true; WindowMemory.generations[0] = 1;
        for (&WindowMemory.pixels, 0..) |*pixel, i| pixel.* = if (i % 2 == 0) 0x80808080 else 0xff800000;
        var front: window_image.Frame = .{};
        try front.open(WindowMemory{}, windowMessage());
        try t.expectEqual(@as(i32,1), WindowMemory.release(.{}, &WindowMemory.source().reference));
        var recording = try @import("primitive_frame.zig").Frame.init(t.allocator); defer recording.deinit(); recording.mirror = false;
        var recording2 = try @import("primitive_frame.zig").Frame.init(t.allocator); defer recording2.deinit(); recording2.mirror = false;
        var cache = layers.Cache.init(t.allocator, 1024); defer cache.deinit(); cache.recording = &recording;
        var cache2 = layers.Cache.init(t.allocator, 1024); defer cache2.deinit(); cache2.recording = &recording2;
        var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
        var second = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
        const view: geometry.topology.Viewport = .{ .pixel_w = 8, .pixel_h = 8, .scale = 150,
            .origin = .{ .x = -19, .y = 7 }, .rotation = @enumFromInt(rotation) };
        try windowCapture(&cache, &front, view); try windowCapture(&cache2, &front, view);
        try t.expect(front.readers == 2 and cache.reserved == 0 and cache2.reserved == 0);
        Model.combined_enabled = false;
        try t.expectError(error.Unsupported, engine.prepare(&cache, 1000));
        Model.combined_enabled = true;
        try engine.prepare(&cache, 1000); try second.prepare(&cache2, 1000);
        try t.expect(WindowMemory.count() == 3);
        try engine.begin(&cache, 1000);
        for (0..32) |_| { _ = engine.advance(&cache, 1); Model.complete(device); }
        try t.expect(engine.active() and front.readers == 2 and front.receipt == null);
        Model.producer_ready = true;
        try pump(&engine, &cache, device, .copied);
        try t.expect(front.readers == 1 and engine.external_receipts == 1);
        try t.expectError(error.Busy, engine.close());
        try t.expect(!front.closeAcknowledged(WindowMemory{}));
        for (0..8) |y| for (0..8) |x| {
            const oriented: [2]usize = switch (rotation) { 0 => .{x,y}, 1 => .{7-y,x}, 2 => .{7-x,7-y}, 3 => .{y,7-x}, else => unreachable };
            const lx = ((2 * oriented[0] + 1) * 120) / 300;
            const ly = ((2 * oriented[1] + 1) * 120) / 300;
            const sx = lx * 8 / 7;
            const expected: u32 = if (lx == 0 and ly == 0) 0x007f00 else if (sx % 2 == 0) 0xbcbcbc else 0x800000;
            try t.expectEqual(expected, Model.visible[y*8+x]);
        };
        try second.begin(&cache2, 1000); try pump(&second, &cache2, device, .copied);
        try t.expect(front.readers == 0 and engine.external_receipts == 0 and second.external_receipts == 1);
        const allocated = Model.native_allocations; const imported = WindowMemory.count();
        try windowCapture(&cache2, &front, view);
        try t.expect(second.prepared(&cache2));
        try second.begin(&cache2, 1000); try pump(&second, &cache2, device, .copied);
        try t.expect(WindowMemory.count() == imported and Model.native_allocations == allocated and second.external_receipts == 1);
        try t.expect(!front.failed and front.consumerFence().point != 0);
        front.retired = true;
        try t.expectError(error.Stale, front.borrow());
        try t.expect(front.closeAcknowledged(WindowMemory{}));
        try engine.close(); try second.close();
        try t.expect(WindowMemory.count() == 0 and engine.reserved_bytes == 0 and second.reserved_bytes == 0);
        for (&Model.jobs) |*job| try t.expect(job.* == null);
    }
    // Cancellation remains logically terminal while physical resources are
    // held; neither the service lease nor its origin worker may disappear.
    Model.serial = 0; Model.outcomes = @splat(0); Model.producer_ready = false;
    WindowMemory.refs[0] = true; WindowMemory.generations[0] = 1;
    var front: window_image.Frame = .{}; try front.open(WindowMemory{}, windowMessage());
    try t.expectEqual(@as(i32,1), WindowMemory.release(.{}, &WindowMemory.source().reference));
    var recording = try @import("primitive_frame.zig").Frame.init(t.allocator); defer recording.deinit(); recording.mirror = false;
    var cache = layers.Cache.init(t.allocator, 1024); defer cache.deinit(); cache.recording = &recording;
    var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
    try windowCapture(&cache, &front, .{ .pixel_w = 8, .pixel_h = 8 });
    try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000);
    for (0..24) |_| { _ = engine.advance(&cache, 1); Model.complete(device); }
    Model.hold_terminal = true; engine.cancel(error.Stale);
    for (0..24) |_| { _ = engine.advance(&cache, 1); Model.complete(device); }
    try t.expect(engine.active() and front.readers == 1 and !front.closeAcknowledged(WindowMemory{}));
    Model.hold_terminal = false; Model.producer_ready = true;
    try pump(&engine, &cache, device, .failed);
    try t.expect(front.failed and front.readers == 0 and front.consumerFence().point != 0 and engine.external_receipts == 1);
    try t.expect(front.closeAcknowledged(WindowMemory{})); try engine.close();
    try t.expect(WindowMemory.count() == 0);
    {
        // Extended linear window colors must survive composition before the
        // output transform. Any intermediate 8-bit image would lose both ends.
        const original = WindowMemory.descriptor; defer WindowMemory.descriptor = original;
        WindowMemory.descriptor.byte_length = 512; WindowMemory.descriptor.plane_pitches[0] = 64;
        WindowMemory.descriptor.format = c.format_abgr16161616f;
        const halves: *[256]f16 = @ptrCast(&WindowMemory.pixels);
        for (0..64) |i| @memcpy(halves[i*4..][0..4], &[_]f16{ -0.25, 2, 0, 1 });
        WindowMemory.refs[0] = true; WindowMemory.generations[0] = 1;
        var message = windowMessage();
        message.format.color = @bitCast(@import("composition_software.zig").description(true, false));
        try front.open(WindowMemory{}, message);
        try t.expectEqual(@as(i32, 1), WindowMemory.release(.{}, &WindowMemory.source().reference));
        try windowCapture(&cache, &front, .{ .pixel_w = 8, .pixel_h = 8 });
        try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000);
        try pump(&engine, &cache, device, .copied);
        const work = Model.image(device, @ptrCast(&engine.workings[engine.output_index].resource));
        const row: [*]const f16 = @ptrFromInt(work.cpu_address + work.pitch);
        try t.expectEqual(@as(f16, -0.25), row[4]);
        try t.expectEqual(@as(f16, 2), row[5]);
        try t.expectEqual(@as(f16, 1), row[7]);
        try t.expect(front.closeAcknowledged(WindowMemory{})); try engine.close();
        try t.expect(WindowMemory.count() == 0);
        var fallback = layers.Cache.init(t.allocator, 1024); defer fallback.deinit();
        try t.expect(fallback.hook().external != null and fallback.hook().external_cpu);
        fallback.recording = &recording; recording.mirror = true;
        try t.expect(fallback.hook().external == null);
        recording.mirror = false;
        try t.expect(fallback.hook().external != null);
    }
    std.debug.print("[desktop-window-images] direct FP16 extended range, alpha/grid/clips/order, producer dependency, two outputs, warm reuse, held cancellation and CPU fallback: OK\n", .{});
}

fn checkHdrOutput(graphics: anytype, device: *const c.R4GfxDevice) !void {
    Model.chain_enabled = true; defer Model.chain_enabled = false;
    Model.chain_format = c.format_xrgb2101010; defer Model.chain_format = c.format_xrgb8888;
    Model.allow_visible = true; Model.clock = 1; Model.chain = .{};
    Model.serial = 0; Model.presents = 0; Model.outcomes = @splat(0); Model.color_batches = 0;
    Model.chain_render = @splat(null); Model.chain_present = @splat(null);
    var cache = layers.Cache.init(t.allocator, 1024 * 1024); defer cache.deinit();
    var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
    const encoding: a.GfxOutputColorState = .{ .flags = 7, .format = c.format_xrgb2101010,
        .bpc = 10, .primaries = 3, .transfer = 3, .range = 2, .reference_white = 2030000, .peak = 10000000 };
    try t.expectError(error.Unsupported, engine.configureOutput(null, c.format_xrgb2101010));
    try t.expectError(error.Unsupported, engine.configureOutput(encoding, c.format_xrgb8888));
    try engine.configureOutput(encoding, c.format_xrgb2101010);
    const full: surface.Rect = .{ .x = 0, .y = 0, .w = 8, .h = 8 };
    const pixel: surface.Rect = .{ .x = 3, .y = 3, .w = 1, .h = 1 };
    try cache.start(full);
    const background = (try cache.begin(1, full, full)).?;
    background.fillRect(full, 0); background.fillRect(.{ .x = 0, .y = 0, .w = 1, .h = 1 }, 0xffffff); try cache.end(1);
    const overlay = (try cache.begin(2, pixel, pixel)).?;
    const white = [_]u32{0x80ffffff};
    try t.expect(overlay.blendArgb32(pixel, pixel.x, pixel.y, 1, 1, 1, std.mem.sliceAsBytes(&white))); try cache.end(2);
    _ = try cache.finish();
    Model.color_enabled = false;
    try t.expectError(error.Unsupported, engine.prepare(&cache, 1000));
    try t.expectEqual(@as(u64, 0), engine.reserved_bytes);
    Model.color_enabled = true;
    try engine.prepare(&cache, 1000);
    try t.expectError(error.Busy, engine.configureOutput(null, c.format_xrgb8888));
    try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
    var receipts: usize = 0;
    for (0..32) |_| {
        Model.complete(device); try engine.pollPresentation();
        if (engine.completion()) |done| { try t.expect(done.status.result == 1); receipts += 1; }
        if (!engine.pending()) break;
    }
    try t.expect(receipts == 1 and Model.color_batches == 1);
    // Independent ST2084 anchors: SDR reference white maps to203cd/m2;
    // half-alpha white to203*128/255cd/m2. Limited RGB10 uses64..940.
    for ([_]u32{ 0, 10, 20 }) |shift| {
        const white_code: i32 = @intCast((Model.visible[0] >> @intCast(shift)) & 1023);
        const half_code: i32 = @intCast((Model.visible[3 * 8 + 3] >> @intCast(shift)) & 1023);
        try t.expect(@abs(white_code - 573) <= 1);
        try t.expect(@abs(half_code - 511) <= 1);
        try t.expectEqual(@as(u32, 64), (Model.visible[7] >> @intCast(shift)) & 1023);
    }
    var capture_pixels: [64]u32 = undefined; var capture_target: scene.SceneBuffer = .{};
    try t.expect(capture_target.attach(std.mem.sliceAsBytes(&capture_pixels), 8, 8));
    _ = try @import("composition_software.zig").paint(&graphics.colors, &cache, &capture_target);
    try t.expectEqual(@as(u32, 0xffffff), capture_pixels[0]);
    try t.expectEqual(@as(u32, 0xbcbcbc), capture_pixels[3 * 8 + 3]);
    try t.expectEqual(@as(u32, 0), capture_pixels[7]);
    var readback = @import("r4gfx_readback").Owner.init(t.allocator, &graphics.client, &graphics.colors, &graphics.device);
    try readback.prepare(8, 8, engine.output_format, engine.output_color);
    try readback.begin(.{ .source = engine.outputs[engine.output_index].resource, .epoch = 8, .frame = 1,
        .regions = &.{.{ .x = 0, .y = 0, .width = 8, .height = 8 }}, .now_ns = 1, .deadline_ns = 1000 });
    try pumpReadback(&readback, device);
    // Capture applies the existing rational HDR shoulder: 1000-nit peak,
    // 203-nit white ->100-nit SDR, knee75 nits. White maps to about88 nits
    // (sRGB241); half-alpha white stays below the knee (sRGB188).
    for ([_]usize{ 0, 3 * 8 + 3, 7 }, [_]i32{ 241, 188, 0 }) |index, expected| for ([_]u5{ 0, 8, 16 }) |shift| {
        const actual: i32 = @intCast((readback.pixels[index] >> shift) & 255);
        try t.expect(@abs(expected - actual) <= 2);
    };
    try readback.close();
    try engine.close();
    for (&Model.jobs) |*job| try t.expect(job.* == null);
    for (&Model.buffers) |*buffer| try t.expect(buffer.* == null);
    try t.expectEqual(@as(u64, 0), engine.reserved_bytes);
}

fn pumpReadback(owner: *@import("r4gfx_readback").Owner, device: *const c.R4GfxDevice) !void {
    for (0..32) |_| {
        Model.complete(device); owner.poll(2);
        if (owner.phase == .ready) return;
        if (owner.phase == .failed) return error.ReadbackFailed;
    }
    return error.ReadbackStalled;
}
fn checkReadback(graphics: anytype, device: *const c.R4GfxDevice, engine: *gpu.Engine, cache: *layers.Cache) !void {
    const readback = @import("r4gfx_readback");
    var owner = readback.Owner.init(t.allocator, &graphics.client, &graphics.colors, &graphics.device);
    const initial_serial = Model.serial;
    owner.poll(1); try t.expect(Model.serial == initial_serial and owner.phase == .empty);
    try owner.prepare(8, 8, c.format_xrgb8888, readback.sdr());
    const staging = owner.staging;
    const pixel = [_]@import("r4gfx").R4GfxRect{.{ .x = 3, .y = 3, .width = 1, .height = 1 }};
    try owner.begin(.{ .source = engine.outputs[0].resource, .epoch = 1, .frame = 1, .regions = &pixel, .now_ns = 1, .deadline_ns = 1000 });
    owner.poll(1);
    try t.expect(owner.phase == .copying and owner.sourceHeld() and owner.stats.copy_bytes == 0 and owner.region_count == 1 and owner.regions[0].width == 8);
    try t.expectError(error.Busy, owner.prepare(4, 4, c.format_xrgb8888, readback.sdr()));
    try pumpReadback(&owner, device);
    try t.expect(!owner.sourceHeld() and owner.stats.copy_bytes == 256);
    try t.expectEqualSlices(u32, &Model.visible, owner.pixels);
    try owner.acknowledge(true);
    const point: surface.Rect = .{ .x = 3, .y = 3, .w = 1, .h = 1 };
    // A real second composition changes one pixel; the staging image is reused.
    try capture(cache, point, 0x445566); try engine.prepare(cache, 1000);
    try engine.begin(cache, 1000); try pump(engine, cache, device, .copied);
    try owner.begin(.{ .source = engine.outputs[0].resource, .epoch = 1, .frame = 2, .base_frame = 1,
        .regions = &pixel, .now_ns = 1, .deadline_ns = 1000 });
    try pumpReadback(&owner, device);
    try t.expect(std.meta.eql(staging, owner.staging) and owner.stats.copy_bytes == 260);
    try t.expectEqualSlices(u32, &Model.visible, owner.pixels);
    try owner.acknowledge(true);
    try owner.begin(.{ .source = engine.outputs[0].resource, .epoch = 1, .frame = 3, .base_frame = 2,
        .regions = &pixel, .now_ns = 2, .deadline_ns = 3 });
    owner.poll(3);
    try t.expect(owner.phase == .draining and owner.sourceHeld());
    try t.expectError(error.Busy, owner.close());
    Model.complete(device); owner.poll(3);
    try t.expect(owner.phase == .failed and !owner.sourceHeld() and !owner.valid);
    try owner.acknowledge(false);
    // Failure/epoch replacement requires a full new image despite tiny damage.
    try owner.begin(.{ .source = engine.outputs[0].resource, .epoch = 2, .frame = 1,
        .regions = &pixel, .now_ns = 1, .deadline_ns = 1000 });
    try pumpReadback(&owner, device);
    try t.expect(owner.regions[0].width == 8 and owner.stats.copy_bytes == 516);
    try t.expectEqualSlices(u32, &Model.visible, owner.pixels);
    try owner.close();
    var desktop = @import("remote_capture.zig").Capture.init(t.allocator, &graphics.client, &graphics.colors, &graphics.device);
    desktop.setDemand(true);
    const view: @import("output_geometry.zig").topology.Viewport = .{ .pixel_w = 8, .pixel_h = 8 };
    try desktop.prepare(view, c.format_xrgb8888, readback.sdr());
    desktop.record(0, 1, point, view, .{ .x = 2, .y = 3, .visible = true, .separate = true });
    desktop.complete(0, engine.outputs[0].resource, 1);
    engine.readback_pin = desktop.reader.source;
    try t.expect(engine.captureBlocked(1));
    try t.expectError(error.Busy, engine.close());
    // A later completed frame cannot overwrite the snapshot being copied.
    desktop.record(1, 2, point, view, .{});
    desktop.complete(1, engine.outputs[0].resource, 1);
    try t.expect(desktop.skipped == 1 and desktop.redraw);
    Model.complete(device); desktop.poll(2); engine.readback_pin = desktop.reader.source;
    try t.expect(!engine.captureBlocked(2) and desktop.image() != null);
    try t.expectEqualSlices(u32, &Model.visible, desktop.image().?.pixels.?);
    try t.expect(desktop.cursor().x == 2 and desktop.cursor().visible and desktop.cursor().separate);
    desktop.acknowledge(true);
    try t.expect(desktop.takeRedraw(2));
    // Last reader cancellation preserves a source until the real receipt.
    desktop.record(0, 3, point, view, .{});
    desktop.complete(0, engine.outputs[0].resource, 3);
    desktop.setDemand(false); desktop.poll(3);
    try t.expect(desktop.reader.sourceHeld());
    Model.complete(device); desktop.poll(4);
    try t.expect(desktop.reader.phase == .empty and desktop.logical_pixels.len == 0 and desktop.image() == null);
    try desktop.close();
    // Monitor ICC/calibration sees a mutable output, while remote capture
    // retains the canonical completed sRGB scene before that wire transform.
    desktop.setDemand(true); try desktop.prepareProfile(view);
    var pre_profile = Model.visible;
    desktop.record(0, 4, .{ .x = 0, .y = 0, .w = 8, .h = 8 }, view, .{});
    desktop.stageProfile(0, &pre_profile, 4);
    @memset(&pre_profile, 0x373737);
    try t.expect(desktop.image() == null and !desktop.reader.sourceHeld());
    desktop.complete(0, engine.outputs[0].resource, 5);
    try t.expectEqualSlices(u32, &Model.visible, desktop.image().?.pixels.?);
    desktop.acknowledge(true); desktop.setDemand(false); desktop.poll(6);
    try desktop.close();
}

fn checkSwapchain(graphics: anytype, device: *const c.R4GfxDevice) !void {
    // A dirty, otherwise idle output must wake at the producer deadline;
    // an unused third ABI slot cannot stand in for a free configured image.
    {
        var paced = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
        paced.chain = std.mem.zeroes(@TypeOf(paced.chain));
        paced.chain.slot = 1; paced.chain.generation = 1;
        var status = std.mem.zeroes(@TypeOf(paced.chain_status.?));
        status.count = 2; status.next_start_ns = 5 * std.time.ns_per_ms + 1;
        paced.chain_status = status;
        try t.expect(paced.captureBlocked(4 * std.time.ns_per_ms));
        try t.expectEqual(@as(u64, 2), paced.captureWaitTicks(4 * std.time.ns_per_ms, 1000, 500));
        try t.expectEqual(@as(u64, 1), paced.captureWaitTicks(6 * std.time.ns_per_ms, 1000, 500));
        try t.expect(!paced.captureBlocked(6 * std.time.ns_per_ms));
        status.frame0.phase = 3; status.frame1.phase = 3; paced.chain_status = status;
        try t.expect(paced.captureBlocked(6 * std.time.ns_per_ms));
        try t.expectEqual(@as(u64, 500), paced.captureWaitTicks(6 * std.time.ns_per_ms, 1000, 500));
        status.count = 3; paced.chain_status = status;
        try t.expect(!paced.captureBlocked(6 * std.time.ns_per_ms));
        try t.expectEqual(@as(u64, 1), paced.captureWaitTicks(6 * std.time.ns_per_ms, 1000, 500));
        status.life = 1; paced.chain_status = status;
        try t.expectEqual(@as(u64, 500), paced.captureWaitTicks(6 * std.time.ns_per_ms, 1000, 500));
    }
    try checkTargetDamage(graphics, device);
    Model.chain_enabled = true; defer Model.chain_enabled = false;
    Model.allow_visible = false; Model.clock = 1; Model.chain = .{};
    Model.serial = 0; Model.presents = 0; Model.outcomes = @splat(0);
    Model.chain_render = @splat(null); Model.chain_present = @splat(null);
    Model.visible = @splat(0xabcdef);
    var cache = layers.Cache.init(t.allocator, 1024 * 1024); defer cache.deinit();
    var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
    const full: surface.Rect = .{ .x = 0, .y = 0, .w = 8, .h = 8 };
    try capture(&cache, full, 0x882244);
    try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000);
    try pump(&engine, &cache, device, .copied);
    for (0..4) |_| { Model.complete(device); try engine.pollPresentation(); }
    try t.expect(engine.chain.slot != 0 and engine.pending() and !engine.active() and engine.completion() == null);
    try t.expect(Model.chain.frames[0].times.copied_ns != 0 and Model.chain.frames[0].times.visible_ns == 0);
    try t.expect(std.mem.allEqual(u32, &Model.visible, 0xabcdef));
    // Render a second complete desktop while the first awaits scanout. The
    // bounded pool rejects further acquisition without serializing input.
    try capture(&cache, full, 0x445566);
    try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
    try t.expectError(error.Busy, engine.acquire(0));
    try t.expect(Model.presents == 1 and Model.chain.frames[1].phase == .queued);
    var expected: [64]u32 = undefined; var target: scene.SceneBuffer = .{};
    try t.expect(target.attach(std.mem.sliceAsBytes(&expected), 8, 8));
    _ = try @import("composition_software.zig").paint(&graphics.colors, &cache, &target);
    Model.allow_visible = true;
    var receipts: usize = 0;
    for (0..32) |_| {
        Model.complete(device); try engine.pollPresentation();
        if (engine.completion()) |done| {
            receipts += 1;
            try t.expect(done.frame == receipts and done.status.result == 1 and done.status.path == c.present_path_composition and
                done.status.visible_ns >= done.status.copied_ns and done.status.copied_ns >= done.status.submitted_ns);
        }
        if (!engine.pending()) break;
    }
    try t.expect(receipts == 2 and Model.presents == 2 and !engine.pending());
    try t.expectEqualSlices(u32, &expected, &Model.visible);
    try capture(&cache, full, 0x998877); try engine.begin(&cache, 1000);
    _ = engine.advance(&cache, 1); engine.cancel(error.Deadline);
    try pump(&engine, &cache, device, .failed);
    try engine.close();
    try t.expectEqualSlices(u32, &expected, &Model.visible);
    for (&Model.jobs) |*job| try t.expect(job.* == null);
    for (&Model.buffers) |*buffer| try t.expect(buffer.* == null);
    try t.expect(engine.reserved_bytes == 0);
}

fn targetDamageCapture(cache: *layers.Cache, x: i32, color: u32) !void {
    const full: surface.Rect = .{ .x = 0, .y = 0, .w = 8, .h = 8 };
    try cache.start(full);
    const background = (try cache.begin(1, full, full)).?;
    // The first layer need not be opaque: replacing it must erase old
    // cursor pixels even under fully transparent or translucent texels.
    var backdrop: [64]u32 = undefined;
    for (&backdrop, 0..) |*pixel, i| pixel.* = (@as(u32, @intCast(i % 4)) * 85 << 24) | 0x203040;
    try t.expect(background.blendArgb32(full, 0, 0, 8, 8, 1, std.mem.sliceAsBytes(&backdrop)));
    try cache.end(1);
    const area: surface.Rect = .{ .x = 2, .y = 2, .w = 4, .h = 3 };
    const overlay = (try cache.begin(2, area, full)).?;
    overlay.fillRect(overlay.fullRect(), color); try cache.end(2);
    const cursor: surface.Rect = .{ .x = x, .y = 6, .w = 1, .h = 1 };
    const arrow = (try cache.begin(3, cursor, full)).?;
    arrow.fillRect(arrow.fullRect(), 0xffffff); try cache.end(3);
    _ = try cache.finish();
}

fn checkTargetDamage(graphics: anytype, device: *const c.R4GfxDevice) !void {
    Model.chain_enabled = true; defer Model.chain_enabled = false;
    Model.allow_visible = false; Model.clock = 1; Model.chain = .{};
    Model.serial = 0; Model.presents = 0; Model.outcomes = @splat(0);
    Model.chain_render = @splat(null); Model.chain_present = @splat(null);
    var cache = layers.Cache.init(t.allocator, 1024 * 1024); defer cache.deinit();
    var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
    const full: surface.Rect = .{ .x = 0, .y = 0, .w = 8, .h = 8 };
    const view: @import("output_geometry.zig").topology.Viewport = .{ .pixel_w = 8, .pixel_h = 8 };
    // Keep the first image awaiting visibility so the next capture must use
    // the other target. Subsequent pairs exercise two different buffer ages.
    var previous_x: i32 = 0;
    var previous_color: u32 = 0x882244;
    for (0..4) |pair| {
        Model.allow_visible = false;
        for (0..2) |member| {
            const step = pair * 2 + member;
            const x: i32 = @intCast(step % 7);
            const color: u32 = if (step >= 4) 0x445566 else 0x882244;
            var damage: surface.Rect = .{ .x = @min(previous_x, x), .y = 6, .w = @intCast(@abs(x - previous_x) + 1), .h = 1 };
            if (color != previous_color) damage = damage.merged(.{ .x = 2, .y = 2, .w = 4, .h = 3 });
            try targetDamageCapture(&cache, x, color);
            engine.compositionDamage(view, damage);
            try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000);
            try t.expectEqual(member, engine.output_index);
            if (step < 2) try t.expectEqualDeep(full, engine.encode_area)
            else if (step < 4) try t.expect(engine.encode_area.h == 1 and engine.encode_area.y == 6 and engine.encode_area.w >= 2);
            const before = engine.render_jobs;
            try pump(&engine, &cache, device, .copied);
            if (step < 2) try t.expectEqual(@as(u64, 4), engine.render_jobs - before)
            else if (step < 4) try t.expectEqual(@as(u64, 3), engine.render_jobs - before);
            var expected: [64]u32 = @splat(0); var target: scene.SceneBuffer = .{};
            try t.expect(target.attach(std.mem.sliceAsBytes(&expected), 8, 8));
            _ = try @import("composition_software.zig").paint(&graphics.colors, &cache, &target);
            const output_image = &engine.outputs[engine.output_index];
            const image = Model.buffers[output_image.resource.slot - 1].?;
            const pixels: []const u32 = std.mem.bytesAsSlice(u32, image);
            const stride: usize = @intCast(output_image.info.image.pitch / 4);
            for (0..8) |y| try t.expectEqualSlices(u32, expected[y * 8..][0..8], pixels[y * stride..][0..8]);
            previous_x = x; previous_color = color;
        }
        Model.allow_visible = true;
        for (0..32) |_| {
            Model.complete(device); try engine.pollPresentation();
            _ = engine.completion();
            if (!engine.pending()) break;
        }
        try t.expect(!engine.pending());
    }
    // Cancellation invalidates every output certificate. A succeeding frame
    // cannot reuse even the unaffected target's untracked older content.
    try targetDamageCapture(&cache, 3, previous_color);
    engine.compositionDamage(view, .{ .x = 0, .y = 6, .w = 4, .h = 1 });
    try engine.begin(&cache, 1000); _ = engine.advance(&cache, 1); Model.complete(device);
    engine.cancel(error.Deadline); try pump(&engine, &cache, device, .failed);
    for (&engine.outputs) |*output| try t.expectEqual(@as(u64, 0), output.generation);
    try targetDamageCapture(&cache, 4, previous_color);
    engine.compositionDamage(view, .{ .x = 3, .y = 6, .w = 2, .h = 1 });
    try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000);
    try t.expectEqualDeep(full, engine.encode_area);
    try pump(&engine, &cache, device, .copied);
    for (0..32) |_| { Model.complete(device); try engine.pollPresentation(); _ = engine.completion(); if (!engine.pending()) break; }
    try t.expect(!engine.pending());
    var transformed = view; transformed.scale = 180;
    engine.compositionDamage(transformed, .{ .x = 0, .y = 0, .w = 1, .h = 1 });
    try t.expect(engine.capture_damage == null);
    for (&engine.outputs) |*output| try t.expectEqual(@as(u64, 0), output.generation);
    try engine.close();
    for (&Model.jobs) |*job| try t.expect(job.* == null);
    for (&Model.buffers) |*buffer| try t.expect(buffer.* == null);
    std.debug.print("[desktop-swapchain] damage follows both target ages; all pixels match full composition; unrelated layer skipped; cancel/view change invalidate: OK\n", .{});
}

fn primitiveScene(painter: *scene.SceneBuffer) !void {
    const paint = @import("paint.zig");
    painter.fillRect(.{ .x = 0, .y = 0, .w = 8, .h = 8 }, 0x203040);
    paint.textScene(painter, undefined, 0, 0, "A", 0xffffff, 0x203040);
    const indices = [_]u8{ 0, 1, 1, 0 };
    var palette: [256]u32 = @splat(0); palette[0] = 0x773311; palette[1] = 0x229955;
    try t.expect(painter.blitIndexed8Nearest(.{ .x = 0, .y = 4, .w = 4, .h = 4 }, .{ .indices = &indices, .palette = &palette,
        .source_x = 0, .source_y = 0, .source_w = 2, .source_h = 2, .source_stride = 2, .guest_w = 2, .guest_h = 2,
        .viewport = .{ .x = 0, .y = 4, .w = 4, .h = 4 } }));
    try t.expect(painter.blendAlpha8(4, 4, 2, 2, 2, 0x669933, &.{ 0, 128, 255, 64 }));
    const argb = [_]u32{ 0x80ff0000, 0xff445566 };
    try t.expect(painter.blendArgb32(painter.fullRect(), 5, 6, 2, 1, 1, std.mem.sliceAsBytes(&argb)));
}
fn primitiveCapture(cache: *layers.Cache, damage: surface.Rect) !void {
    const full: surface.Rect = .{ .x = 0, .y = 0, .w = 8, .h = 8 };
    try cache.start(full);
    const painter = (try cache.begin(1, full, damage)).?;
    try primitiveScene(painter);
    try cache.end(1); _ = try cache.finish();
}
fn checkPrimitives(graphics: *@import("gfx_renderer.zig").Renderer, device: *const c.R4GfxDevice) !void {
    var frame = try @import("primitive_frame.zig").Frame.init(t.allocator); defer frame.deinit();
    frame.mirror = false;
    var cache = layers.Cache.init(t.allocator, 1024 * 1024); defer cache.deinit(); cache.recording = &frame;
    var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
    const full: surface.Rect = .{ .x = 0, .y = 0, .w = 8, .h = 8 };
    var expected: [64]u32 = undefined; var target: scene.SceneBuffer = .{};
    try t.expect(target.attach(std.mem.sliceAsBytes(&expected), 8, 8)); try primitiveScene(&target);
    try primitiveCapture(&cache, full);
    try t.expect(cache.reserved == 0 and frame.count >= 5 and frame.assets.misses == 4 and frame.merged_fills != 0);
    try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
    for (expected, Model.visible) |reference, actual| inline for (.{ 0, 8, 16 }) |shift| {
        const difference = @as(i32, @intCast((reference >> shift) & 255)) - @as(i32, @intCast((actual >> shift) & 255));
        try t.expect(@abs(difference) <= 1);
    };
    try t.expect(Model.batches > 0 and Model.max_batch >= 2 and engine.primitive_jobs < engine.primitive_draws);
    const uploads = engine.uploaded_bytes; const conversions = frame.assets.converted_pixels; const allocations = Model.native_allocations;
    const visible_warm = Model.visible;
    const draws_warm = engine.primitive_draws; const jobs_warm = engine.primitive_jobs;
    try primitiveCapture(&cache, full);
    try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
    try t.expect(engine.primitive_draws == draws_warm and engine.primitive_jobs == jobs_warm);
    try t.expectEqualSlices(u32, &visible_warm, &Model.visible);
    // An unchanged command stream still depends on the exact texture-page
    // generation. Even an atlas append outside this layer must invalidate it.
    try primitiveCapture(&cache, full);
    const texture = for (frame.commands[0..frame.count]) |command| { if (command.texture) |index| break index; } else return error.MissingTexture;
    frame.assets.generation += 1;
    frame.assets.textures[texture].generation = frame.assets.generation;
    frame.assets.textures[texture].dirty = frame.assets.textures[texture].rect();
    try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
    try t.expect(engine.primitive_draws == draws_warm + frame.count and engine.uploaded_bytes > uploads);
    try t.expectEqualSlices(u32, &visible_warm, &Model.visible);
    const uploads_refreshed = engine.uploaded_bytes;
    try primitiveCapture(&cache, .{ .x = 3, .y = 3, .w = 1, .h = 1 });
    try t.expect(engine.prepared(&cache)); try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
    try t.expect(engine.uploaded_bytes == uploads_refreshed and frame.assets.converted_pixels == conversions and Model.native_allocations == allocations);
    frame.mirror = true; for (&cache.entries) |*entry| entry.initialized = false;
    try primitiveCapture(&cache, full);
    _ = try @import("composition_software.zig").paint(&graphics.colors, &cache, &target);
    try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
    const visible = Model.visible;
    try primitiveCapture(&cache, full); Model.reject_draw = true;
    try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .failed); Model.reject_draw = false;
    try t.expectEqualSlices(u32, &visible, &Model.visible);
    try engine.close();
    for (&Model.jobs) |*job| try t.expect(job.* == null);
    for (&Model.buffers) |*buffer| try t.expect(buffer.* == null);
    try checkLayerReuse(graphics, device);
    try checkShapesAndLargeImage(graphics, device);
    try checkAssets();
    try checkOutputTransforms(graphics, device);
    std.debug.print("[desktop-primitives] fill/glyph/indexed/alpha/ARGB: bounded GPU capture; no CPU layer pixels; warm upload=0; fallback remains complete\n", .{});
}

fn checkLayerReuse(graphics: *@import("gfx_renderer.zig").Renderer, device: *const c.R4GfxDevice) !void {
    var frame = try @import("primitive_frame.zig").Frame.init(t.allocator); defer frame.deinit(); frame.mirror = false;
    var cache = layers.Cache.init(t.allocator, 1024); defer cache.deinit(); cache.recording = &frame;
    var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
    const full: surface.Rect = .{ .x = 0, .y = 0, .w = 8, .h = 8 };
    try capture(&cache, full, 0x882244);
    try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
    const draws = engine.primitive_draws;
    const first = Model.visible;
    try capture(&cache, full, 0x882244);
    try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
    try t.expectEqual(draws, engine.primitive_draws);
    try t.expectEqualSlices(u32, &first, &Model.visible);
    // A changed overlay redraws only itself; the ordered final composition
    // still reads both images and must expose the new pixels.
    try capture(&cache, full, 0x445566);
    var overlay_draws: usize = 0;
    for (frame.commands[0..frame.count]) |command| { if (command.layer == 1) overlay_draws += 1; }
    try t.expect(overlay_draws != 0);
    try engine.begin(&cache, 1000);
    try t.expect(engine.reuse_layers[0] and !engine.reuse_layers[1]);
    try pump(&engine, &cache, device, .copied);
    try t.expectEqual(draws + overlay_draws, engine.primitive_draws);
    try t.expectEqual(@as(u32, 0x203040), Model.visible[0]);
    try t.expectEqual(@as(u32, 0x445566), Model.visible[3 * 8 + 3]);
    const changed = Model.visible;
    // Let an admitted draw alter its private layer before cancellation.
    // Returning to the old scene must repaint instead of using its snapshot.
    try capture(&cache, full, 0x998877); try engine.begin(&cache, 1000);
    _ = engine.advance(&cache, 1); Model.complete(device); engine.cancel(error.Deadline);
    try t.expectError(error.Busy, engine.close());
    try pump(&engine, &cache, device, .failed);
    try capture(&cache, full, 0x445566); try engine.begin(&cache, 1000);
    try t.expect(!engine.reuse_layers[0] and !engine.reuse_layers[1]);
    try pump(&engine, &cache, device, .copied);
    try t.expectEqualSlices(u32, &changed, &Model.visible);
    engine.invalidate();
    try capture(&cache, full, 0x445566); try engine.begin(&cache, 1000);
    try t.expect(!engine.reuse_layers[0] and !engine.reuse_layers[1]);
    try pump(&engine, &cache, device, .copied);
    try t.expectEqualSlices(u32, &changed, &Model.visible);
    try engine.close();
    try t.expect(engine.snapshot.len == 0 and engine.reserved_bytes == 0);
    for (&Model.jobs) |*job| try t.expect(job.* == null);
    for (&Model.buffers) |*buffer| try t.expect(buffer.* == null);
    std.debug.print("[desktop-primitives] exact layer reuse, selective repaint, texture invalidation and cancelled partial draw: OK\n", .{});
    try checkCaptureReuse(graphics, device);
}

fn checkCaptureReuse(graphics: *@import("gfx_renderer.zig").Renderer, device: *const c.R4GfxDevice) !void {
    var frame = try @import("primitive_frame.zig").Frame.init(t.allocator); defer frame.deinit(); frame.mirror = false;
    var cache = layers.Cache.init(t.allocator, 1024); defer cache.deinit(); cache.recording = &frame;
    var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
    const full: surface.Rect = .{ .x = 0, .y = 0, .w = 8, .h = 8 };
    const view: @import("output_geometry.zig").topology.Viewport = .{ .pixel_w = 8, .pixel_h = 8 };
    var prior_cursor: i32 = 0;
    for (0..9) |step| {
        const cursor_x: i32 = @intCast(step % 7);
        const area: surface.Rect = .{ .x = if (step < 5) 2 else 4, .y = 2, .w = 2, .h = 2 };
        const color: u32 = if (step < 2) 0x882244 else 0x445566;
        const cursor: surface.Rect = .{ .x = cursor_x, .y = 6, .w = 1, .h = 1 };
        var damage: surface.Rect = .{ .x = @min(prior_cursor, cursor_x), .y = 6, .w = @intCast(@abs(cursor_x - prior_cursor) + 1), .h = 1 };
        if (step == 0) damage = full;
        if (step >= 2 and step <= 5) damage = damage.merged(.{ .x = 2, .y = 2, .w = 4, .h = 2 });
        if (step == 7) engine.invalidate();
        if (step == 8) {
            const texture = for (frame.commands[0..frame.count]) |command| { if (command.texture) |value| break value; } else return error.MissingTexture;
            frame.assets.generation += 1;
            frame.assets.textures[texture].generation = frame.assets.generation;
            frame.assets.textures[texture].dirty = frame.assets.textures[texture].rect();
        }
        try cache.startOutput(view);
        engine.reuseCapture(&cache, damage);
        const background = (try cache.begin(1, full, full)).?;
        background.fillRect(full, 0x203040); try cache.end(1);
        const reused = cache.reused_captures;
        const hits = frame.assets.hits;
        if (step != 3) {
            const painter = try cache.begin(2, area, full);
            const expect_reuse = step == 1 or step == 6;
            try t.expectEqual(expect_reuse, painter == null);
            if (painter) |overlay| {
                const pixels: [4]u32 = @splat(0xff000000 | color);
                try t.expect(overlay.blendArgb32(area, area.x, area.y, 2, 2, 1, std.mem.sliceAsBytes(&pixels)));
                try cache.end(2);
            }
            if (expect_reuse) {
                try t.expectEqual(reused + 1, cache.reused_captures);
                try t.expectEqual(hits, frame.assets.hits); // No painter or asset-key rehash.
                for (frame.commands[0..frame.count]) |command| if (command.texture) |texture| {
                    try t.expectEqual(frame.assets.frame, frame.assets.textures[texture].pinned);
                };
            }
        }
        const arrow = (try cache.begin(3, cursor, full)).?;
        arrow.fillRect(cursor, 0xffffff); try cache.end(3);
        _ = try cache.finish();
        engine.compositionDamage(view, damage);
        try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
        var expected: [64]u32 = @splat(0x203040);
        if (step != 3) for (0..2) |y| {
            for (0..2) |x| expected[(y + 2) * 8 + x + @as(usize, @intCast(area.x))] = color;
        };
        expected[6 * 8 + @as(usize, @intCast(cursor_x))] = 0xffffff;
        try t.expectEqualSlices(u32, &expected, &Model.visible);
        prior_cursor = cursor_x;
    }
    // A changed viewport invalidates CPU capture even if its native size is unchanged.
    var moved = view; moved.origin.x = 1;
    try cache.startOutput(moved);
    engine.reuseCapture(&cache, .{ .x = 0, .y = 6, .w = 8, .h = 1 });
    for (cache.replay_layers) |retained| try t.expect(!retained);
    _ = try cache.finish();
    try engine.close();
    for (&Model.jobs) |*job| try t.expect(job.* == null);
    for (&Model.buffers) |*buffer| try t.expect(buffer.* == null);
    std.debug.print("[desktop-primitives] CPU capture reuse skips unchanged painters/asset hashing; pixels match through content change, removal, return, move, texture generation and invalidation: OK\n", .{});
}

fn checkOutputTransforms(graphics: *@import("gfx_renderer.zig").Renderer, device: *const c.R4GfxDevice) !void {
    const geometry = @import("output_geometry.zig");
    const software = @import("output_cpu.zig");
    var frame = try @import("primitive_frame.zig").Frame.init(t.allocator); defer frame.deinit(); frame.mirror = false;
    var cache = layers.Cache.init(t.allocator, 1024); defer cache.deinit(); cache.recording = &frame;
    var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
    for ([_]u32{ 120, 240, 180 }) |scale| {
        for (0..4) |rotation| {
            const view: geometry.topology.Viewport = .{ .pixel_w = 8, .pixel_h = 8, .scale = scale,
                .origin = .{ .x = -31, .y = 12 }, .rotation = @enumFromInt(rotation) };
            const bounds = try geometry.logical(view);
            var pixels: [64]u32 = undefined;
            for (0..@intCast(bounds.h)) |y| for (0..@intCast(bounds.w)) |x| {
                pixels[y * @as(usize, @intCast(bounds.w)) + x] = @intCast(0x102030 + y * 0x010700 + x * 0x0b01);
            };
            const source = pixels[0..@intCast(bounds.w * bounds.h)];
            var expected: [64]u32 = undefined;
            software.transform(view, source, @intCast(bounds.w), &expected);
            for (0..2) |iteration| {
                const converted = frame.assets.converted_pixels;
                const draws = engine.primitive_draws;
                try cache.startOutput(view);
                const painter = (try cache.begin(1, bounds, bounds)).?;
                painter.blitXrgb32(bounds.x, bounds.y, @intCast(bounds.w), @intCast(bounds.h), source);
                try cache.end(1); _ = try cache.finish();
                try t.expect(cache.reserved == 0 and std.meta.eql(cache.commands[0].scissor, geometry.native(view)));
                try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
                try t.expectEqualSlices(u32, &expected, &Model.visible);
                if (iteration == 1) {
                    try t.expectEqual(draws, engine.primitive_draws);
                    var remote = @import("remote_capture.zig").Capture.init(t.allocator, &graphics.client, &graphics.colors, &graphics.device);
                    remote.setDemand(true); try remote.prepare(view, c.format_xrgb8888, @import("r4gfx_readback").sdr());
                    remote.record(0, 1, bounds, view, .{ .x = bounds.x + 1, .y = bounds.y + 2, .visible = true });
                    remote.complete(0, engine.outputs[0].resource, 1);
                    for (0..8) |_| { Model.complete(device); remote.poll(2); if (remote.ready) break; }
                    const captured = remote.image() orelse return error.CaptureNotReady;
                    try t.expectEqualSlices(u32, source, captured.pixels.?);
                    try t.expect(remote.cursor().x == 1 and remote.cursor().y == 2);
                    try remote.close();
                }
                if (iteration == 1) try t.expectEqual(converted, frame.assets.converted_pixels);
            }
        }
    }
    try engine.close();
    for (&Model.jobs) |*job| try t.expect(job.* == null);
    for (&Model.buffers) |*buffer| try t.expect(buffer.* == null);
    std.debug.print("[desktop-outputs] signed origin, native composition, four rotations, 150/200 percent and warm assets: OK\n", .{});
}

fn shapesAndLargeImage(painter: *scene.SceneBuffer, pixels: []const u32) !void {
    const shapes = @import("gui_shape_renderer.zig");
    try t.expect(painter.blitXrgb32Nearest(painter.fullRect(), .{ .pixels = pixels, .source_x = 0, .source_y = 0,
        .source_w = 1024, .source_h = 512, .source_stride = 1024, .guest_w = 1024, .guest_h = 512,
        .viewport = .{ .x = -300, .y = -200, .w = 1024, .h = 512 } }));
    for (0..16) |index| painter.fillRect(.{ .x = @intCast(index % 8), .y = @intCast(index / 8), .w = 1, .h = 1 },
        0x102030 + @as(u32, @intCast(index)) * 0x010101);
    var bytes: [@sizeOf(a.GuiShapeResource) + 4 * @sizeOf(a.GuiPathSegment)]u8 = undefined;
    const rounded = try r4os.gui_shapes.roundedRect(&bytes, .{ .x = 2, .y = 2, .w = 4, .h = 4,
        .radii = .{ .top_left_x = 2, .top_left_y = 2, .bottom_right_x = 2, .bottom_right_y = 2 },
        .fill_argb = 0x8070b010, .shadow = .{ .argb = 0x90000000, .offset_x = 1, .offset_y = 1, .blur = 1 } });
    for ([_]u32{ a.gui_frame_command_kind_shadow, a.gui_frame_command_kind_rounded_rect }) |kind| {
        const command = try r4os.gui_shapes.command(kind, 0, 0, 8, 8, 0, rounded.len);
        try t.expect(shapes.replay(t.allocator, painter, painter.fullRect(), command, rounded) == .drawn);
    }
    var path = try r4os.gui_shapes.PathBuilder.init(&bytes, .{ .stroke_argb = 0xc0e030a0, .stroke_width = 1.5, .line_cap = .round });
    try path.moveTo(.{ .x = 0, .y = 7 });
    try path.cubicTo(.{ .x = 1, .y = 2 }, .{ .x = 6, .y = 2 }, .{ .x = 7, .y = 7 });
    const curve = try path.finish();
    const command = try r4os.gui_shapes.command(a.gui_frame_command_kind_path_stroke, 0, 0, 8, 8, 0, curve.len);
    try t.expect(shapes.replay(t.allocator, painter, painter.fullRect(), command, curve) == .drawn);
}
fn checkShapesAndLargeImage(graphics: *@import("gfx_renderer.zig").Renderer, device: *const c.R4GfxDevice) !void {
    const pixels = try t.allocator.alloc(u32, 1024 * 512); defer t.allocator.free(pixels);
    for (pixels, 0..) |*pixel, index| pixel.* = @as(u32, @intCast(index)) & 0xffffff;
    var expected: [64]u32 = undefined; var reference: scene.SceneBuffer = .{};
    try t.expect(reference.attach(std.mem.sliceAsBytes(&expected), 8, 8));
    try shapesAndLargeImage(&reference, pixels);
    var frame = try @import("primitive_frame.zig").Frame.init(t.allocator); defer frame.deinit(); frame.mirror = false;
    var cache = layers.Cache.init(t.allocator, 1024); defer cache.deinit(); cache.recording = &frame;
    const full = reference.fullRect();
    try cache.start(full); const painter = (try cache.begin(1, full, full)).?;
    try shapesAndLargeImage(painter, pixels);
    try cache.end(1); _ = try cache.finish();
    try t.expect(cache.reserved == 0);
    var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
    try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
    for (expected, Model.visible) |want, actual| inline for (.{ 0, 8, 16 }) |shift| {
        const difference = @as(i32,@intCast((want >> shift) & 255)) - @as(i32,@intCast((actual >> shift) & 255));
        try t.expect(@abs(difference) <= 3); // Three separately rounded mask blends.
    };
    try t.expect(Model.max_batch == 16 and engine.staging.info.image.byte_length == 1024 * 1024 and
        engine.uploaded_bytes == (1024 * 512 + 512 * 512) * 4 and frame.assets.reserved <= frame.assets.budget);
    try engine.close();
    std.debug.print("[desktop-primitives] 16 draws/batch; large source tiled through 1 MB staging; curves/AA/shadow max=3 LSB\n", .{});
}

fn checkAssets() !void {
    const assets = @import("primitive_assets.zig"); const images = @import("primitive_image.zig");
    const metadata = assets.entry_capacity * @sizeOf(assets.Entry);
    var atlas = try assets.Cache.init(t.allocator, metadata + 512 * 512 * 4); defer atlas.deinit();
    try atlas.start(1);
    const rows = [_]u64{ 1, 2 };
    var view: images.View = .{ .format = .glyph, .width = 2, .height = 2, .stride = 0, .rows = &rows,
        .foreground = 0x123456, .background = 0x112233, .identity = .{ .font = 8, .revision = 1, .glyph = 65 } };
    const first = try atlas.intern(view); const generation = atlas.textures[first.texture].generation;
    _ = try atlas.intern(view); try t.expect(atlas.hits == 1 and atlas.converted_pixels == 4);
    view.identity.revision = 2; _ = try atlas.intern(view);
    view.identity.dpi_x = 144; view.identity.dpi_y = 144; _ = try atlas.intern(view);
    try t.expect(atlas.misses == 3 and atlas.textures[first.texture].generation > generation);
    atlas.uploaded(first.texture, generation); try t.expect(atlas.textures[first.texture].dirty != null);
    var large: [129 * 129]u32 = @splat(0x123456);
    const large_view: images.View = .{ .format = .xrgb, .width = 129, .height = 129, .stride = 129 * 4, .bytes = std.mem.sliceAsBytes(&large) };
    try t.expectError(error.OutOfMemory, atlas.intern(large_view));
    try t.expect(atlas.reserved <= atlas.budget and atlas.textures[first.texture].generation > generation);
    try atlas.start(2); _ = try atlas.intern(large_view); try t.expect(atlas.evictions != 0 and atlas.reserved <= atlas.budget);
    const conversions = atlas.converted_pixels; large[7] ^= 1; _ = try atlas.intern(large_view);
    try t.expect(atlas.converted_pixels > conversions);

    var frame = try @import("primitive_frame.zig").Frame.init(t.allocator); defer frame.deinit(); frame.mirror = false;
    var cache = layers.Cache.init(t.allocator, 4); defer cache.deinit(); cache.recording = &frame;
    const full: surface.Rect = .{ .x = 0, .y = 0, .w = 8, .h = 8 };
    try cache.start(full); const painter = (try cache.begin(1, full, full)).?;
    painter.reject(error.OutOfMemory);
    try t.expectError(error.OutOfMemory, cache.end(1));
    try t.expectError(error.OutOfMemory, cache.finish());
    try t.expect(cache.command_count == 0 and cache.reserved == 0);
}

fn checkHdrWindows(graphics: anytype, device: *const c.R4GfxDevice) !void {
    const policy = @import("window_color.zig");
    const original = WindowMemory.descriptor;
    defer WindowMemory.descriptor = original;
    Model.chain_enabled = true; defer Model.chain_enabled = false;
    defer Model.chain_format = c.format_xrgb8888;
    for ([_]bool{ false, true }) |hdr_output| for ([_]bool{ false, true }) |pq_input| {
        Model.chain_format = if (hdr_output) c.format_xrgb2101010 else c.format_xrgb8888;
        Model.allow_visible = true; Model.clock = 1; Model.chain = .{};
        Model.serial = 0; Model.outcomes = @splat(0); Model.producer_ready = true;
        Model.chain_render = @splat(null); Model.chain_present = @splat(null);
        var cache = layers.Cache.init(t.allocator, 4096); defer cache.deinit();
        var recording = try @import("primitive_frame.zig").Frame.init(t.allocator); defer recording.deinit();
        recording.mirror = false; cache.recording = &recording;
        var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
        if (hdr_output) try engine.configureOutput(.{ .flags = 7, .format = c.format_xrgb2101010,
            .bpc = 10, .primaries = 3, .transfer = 3, .range = 2, .reference_white = 2030000, .peak = 10000000 }, c.format_xrgb2101010);
        WindowMemory.descriptor = original;
        WindowMemory.descriptor.format = if (pq_input) c.format_xrgb2101010 else c.format_abgr16161616f;
        WindowMemory.descriptor.byte_length = if (pq_input) 256 else 512;
        WindowMemory.descriptor.plane_pitches[0] = if (pq_input) 32 else 64;
        for (0..64) |i| {
            if (pq_input) {
                // Independent ST2084 code anchors for 80 and 1000 cd/m2.
                const code: u32 = if (i % 2 == 0) 497 else 769;
                WindowMemory.pixels[i] = code | (code << 10) | (code << 20);
            } else {
                const halves: *[256]f16 = @ptrCast(&WindowMemory.pixels);
                const value: f16 = if (i % 2 == 0) 1 else 12.5;
                @memcpy(halves[i*4..][0..4], &[_]f16{value, value, value, 1});
            }
        }
        WindowMemory.refs[0] = true; WindowMemory.generations[0] = 1;
        var front: window_image.Frame = .{};
        var message = windowMessage(); message.format.color = @bitCast(if (pq_input) policy.pq(true) else policy.scrgb(true));
        try front.open(WindowMemory{}, message);
        try t.expectEqual(@as(i32, 1), WindowMemory.release(.{}, &WindowMemory.source().reference));
        const view: @import("output_geometry.zig").topology.Viewport = .{ .pixel_w = 8, .pixel_h = 8 };
        try windowCapture(&cache, &front, view);
        try engine.prepare(&cache, 1000); try t.expect(engine.hdr_frame);
        try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
        for (0..32) |_| { Model.complete(device); try engine.pollPresentation(); _ = engine.completion(); if (!engine.pending()) break; }
        const work = Model.image(device, @ptrCast(&engine.workings[engine.output_index].resource));
        const row: [*]const f16 = @ptrFromInt(work.cpu_address + work.pitch);
        const white: f32 = if (hdr_output) 203 else 100;
        try t.expectApproxEqAbs(@as(f32, 80), @as(f32, row[0]) * white, 1);
        try t.expectApproxEqAbs(@as(f32, 1000), @as(f32, row[4]) * white, 3);
        // The shared output shoulder maps 10000 to the monitor peak. It must
        // retain highlights above SDR white and keep 80-nit scRGB absolute.
        for ([_]usize{8, 9}, [_]i32{if (hdr_output) 490 else 230, if (hdr_output) 710 else 254}) |index, expected| {
            const shifts = if (hdr_output) [_]u5{0, 10, 20} else [_]u5{0, 8, 16};
            for (shifts) |shift| {
                const actual: i32 = @intCast((Model.visible[index] >> shift) & @as(u32, if (hdr_output) 1023 else 255));
                try t.expect(@abs(actual - expected) <= 2);
            }
        }
        var readback = @import("r4gfx_readback").Owner.init(t.allocator, &graphics.client, &graphics.colors, &graphics.device);
        try readback.prepare(8, 8, engine.output_format, engine.output_color);
        try readback.begin(.{ .source = engine.outputs[engine.output_index].resource, .epoch = 1, .frame = 1,
            .regions = &.{.{ .x = 0, .y = 0, .width = 8, .height = 8 }}, .now_ns = 1, .deadline_ns = 1000 });
        try pumpReadback(&readback, device);
        try t.expect((readback.pixels[9] & 255) > (readback.pixels[8] & 255));
        if (!hdr_output) try t.expectEqualSlices(u32, &Model.visible, readback.pixels);
        try readback.close();
        const allocated = Model.native_allocations;
        try windowCapture(&cache, &front, view);
        try t.expect(engine.prepared(&cache));
        try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
        for (0..32) |_| { Model.complete(device); try engine.pollPresentation(); _ = engine.completion(); if (!engine.pending()) break; }
        try t.expectEqual(allocated, Model.native_allocations);
        try t.expect(front.closeAcknowledged(WindowMemory{}));
        // Removing the HDR source rebuilds the private working images once,
        // restoring the regular SDR policy on this same output owner.
        try cache.start(.{.x=0,.y=0,.w=8,.h=8});
        const background = (try cache.begin(1, cache.screen, cache.screen)).?;
        background.fillRect(cache.screen, 0xffffff); try cache.end(1); _ = try cache.finish();
        try t.expect(!engine.prepared(&cache));
        try engine.prepare(&cache, 1000); try t.expect(!engine.hdr_frame);
        try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
        for (0..32) |_| { Model.complete(device); try engine.pollPresentation(); _ = engine.completion(); if (!engine.pending()) break; }
        if (!hdr_output) try t.expectEqual(@as(u32, 0xffffff), Model.visible[9]);
        try engine.close();
        try t.expect(WindowMemory.count() == 0 and engine.reserved_bytes == 0);
        for (&Model.jobs) |*job| try t.expect(job.* == null);
        for (&Model.buffers) |*buffer| try t.expect(buffer.* == null);
    };
    std.debug.print("[desktop-window-hdr] absolute scRGB/PQ, 80/1000-nit anchors, SDR/HDR output, capture, warm reuse and HDR removal: OK\n", .{});
}

fn checkDependencyPressure(graphics: *@import("gfx_renderer.zig").Renderer, device: *const c.R4GfxDevice) !void {
    Model.receipts = @splat(.{}); Model.retained_count = 0; Model.retained_peak = 0; Model.retained_limit = 24;
    defer Model.retained_limit = 0;
    var frame = try @import("primitive_frame.zig").Frame.init(t.allocator); defer frame.deinit(); frame.mirror = false;
    var cache = layers.Cache.init(t.allocator, 1024); defer cache.deinit(); cache.recording = &frame;
    const full: surface.Rect = .{ .x = 0, .y = 0, .w = 8, .h = 8 };
    try cache.start(full);
    const painter = (try cache.begin(1, full, full)).?;
    painter.fillRect(full, 0);
    const translucent: [64]u32 = @splat(0x8000ff00);
    for (0..768) |_| try t.expect(painter.blendArgb32(full, 0, 0, 8, 8, 8, std.mem.sliceAsBytes(&translucent)));
    try cache.end(1); _ = try cache.finish();
    var engine = gpu.Engine.init(&graphics.client, &graphics.colors, &graphics.device);
    try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000);
    var result: gpu.Progress = .pending;
    for (0..512) |_| {
        result = engine.advance(&cache, 1);
        if (result != .pending) break;
        Model.complete(device);
    }
    if (result == .pending) {
        // Preserve allocator/ownership diagnostics even for the old failing
        // implementation; cancellation must eventually retire its graph.
        engine.cancel(error.Deadline);
        try pump(&engine, &cache, device, .failed);
    }
    try engine.close();
    try t.expectEqual(gpu.Progress.copied, result);
    try t.expect(engine.primitive_jobs > Model.retained_limit);
    try t.expect(Model.retained_count == 0 and Model.retained_peak <= c.device_job_capacity);
    for (Model.visible) |pixel| try t.expectEqual(@as(u32, 0x00ff00), pixel);
    std.debug.print("[desktop-dependencies] 768 draws exceed finite receipt pool; ordered pixels match; all receipts retired; peak={d}\n", .{Model.retained_peak});
}
