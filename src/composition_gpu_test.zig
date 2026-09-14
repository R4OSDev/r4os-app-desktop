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
const Model = struct {
    const s = p.swapchain.lifecycle;
    const List = struct { requests: [c.render_list_capacity]c.R4GfxRenderRequest = undefined,
        grids: [c.render_list_capacity]c.R4GfxLogicalGrid = @splat(std.mem.zeroes(c.R4GfxLogicalGrid)), count: usize = 0, color_flags: ?u32 = null };
    const Operation = union(enum) { copy: c.R4GfxCopyRequestEx, draw: c.R4GfxRenderRequest, list: List, present: c.R4GfxImagePresentRequest };
    const Job = struct { handle: c.R4GfxJob, operation: Operation, dependency: u64, result: u32 = 0, terminal: bool = false, cancelled: bool = false, pins: u32 = 0 };
    var buffers: [c.device_resource_capacity]?[]align(4) u8 = @splat(null);
    var jobs: [c.device_job_capacity]?Job = @splat(null);
    var outcomes: [256]u32 = @splat(0);
    var serial: u64 = 0;
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
            @as(u32, if (color_enabled) c.device_gpu_color else 0);
        return rc;
    }
    fn create(device: *const c.R4GfxDevice, input: *const c.R4GfxResourceDesc, out: *c.R4GfxResource) callconv(.c) i32 {
        return createImage(device, input, null, out);
    }
    fn createColor(device: *const c.R4GfxDevice, input: *const c.R4GfxColorResourceDesc, out: *c.R4GfxResource) callconv(.c) i32 {
        return createImage(device, &input.resource, input.description, out);
    }
    fn createImage(device: *const c.R4GfxDevice, input: *const c.R4GfxResourceDesc, description: ?c.R4GfxColorDescription, out: *c.R4GfxResource) i32 {
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
        if (input.kind != c.resource_image or (input.source_kind != c.source_create_native and input.source_kind != c.source_create_system)) return p.createResource(device,input,out);
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
        if (rc == 0) if (buffers[resource.slot-1]) |memory| { t.allocator.free(memory); buffers[resource.slot-1] = null; };
        return rc;
    }
    fn submit(device: *const c.R4GfxDevice, operation: Operation, count: u32, address: u64, out: *c.R4GfxJob) i32 {
        if (busy_count != 0) { busy_count -= 1; return c.status_busy; }
        const index = for (&jobs,0..) |*slot,i| { if (slot.* == null) break i; } else return c.status_busy;
        std.debug.assert(count <= 1 and serial+1 < outcomes.len);
        const dependency = if (count == 0) 0 else @as(*const c.R4GfxCopyFence,@ptrFromInt(address)).point;
        serial += 1;
        const handle: c.R4GfxJob = .{ .slot=@intCast(index+1), .reserved=0, .generation=serial, .device_generation=device.generation, .device_address=device.address };
        jobs[index] = .{ .handle=handle, .operation=operation, .dependency=dependency }; out.*=handle;
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
        std.debug.assert(flags == c.color_transform_output | c.color_transform_relative_white | c.color_transform_dither);
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
    fn present(device: *const c.R4GfxDevice, input: *const c.R4GfxImagePresentRequest, out: *c.R4GfxJob) callconv(.c) i32 {
        return submit(device,.{.present=input.*},input.dependency_count,input.dependencies,out);
    }
    fn find(handle: *const c.R4GfxJob) *Job {
        const job = &jobs[handle.slot-1].?; std.debug.assert(std.meta.eql(handle.*,job.handle)); return job;
    }
    fn info(_: *const c.R4GfxDevice, handle: *const c.R4GfxJob, out: *c.R4GfxJobInfo) callconv(.c) i32 {
        const job = find(handle); out.* = std.mem.zeroes(c.R4GfxJobInfo);
        out.version=1; out.size=@sizeOf(c.R4GfxJobInfo); out.point=handle.generation; out.timeline=123;
        out.phase=if(job.terminal) a.gfx_queue_phase_terminal else a.gfx_queue_phase_running;
        out.result=job.result; out.flags=if(job.terminal) 0 else 3; return 0;
    }
    fn fence(_: *const c.R4GfxDevice, handle: *const c.R4GfxJob, out: *c.R4GfxCopyFence) callconv(.c) i32 {
        _=find(handle); out.*=.{ .slot=handle.slot, .adapter_id=9, .timeline=123, .point=handle.generation, .device_generation=1, .reset_generation=1 }; return 0;
    }
    fn cancel(_: *const c.R4GfxDevice, handle: *const c.R4GfxJob) callconv(.c) i32 { find(handle).cancelled=true; return 0; }
    fn releaseJob(_: *const c.R4GfxDevice, handle: *const c.R4GfxJob) callconv(.c) i32 {
        if (!find(handle).terminal or find(handle).pins != 0) return c.status_busy;
        jobs[handle.slot-1]=null; return 0;
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
                        std.debug.assert(grid.enabled == 0);
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
    try engine.close();
    for(&Model.jobs) |*job| try t.expect(job.*==null);
    for(&Model.buffers) |*buffer| try t.expect(buffer.*==null);
    try t.expect(engine.reserved_bytes==0);
    try checkPrimitives(graphics, device);
    try checkSwapchain(graphics, device);
    try checkHdrOutput(graphics, device);
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
    try engine.close();
    for (&Model.jobs) |*job| try t.expect(job.* == null);
    for (&Model.buffers) |*buffer| try t.expect(buffer.* == null);
    try t.expectEqual(@as(u64, 0), engine.reserved_bytes);
}

fn checkSwapchain(graphics: anytype, device: *const c.R4GfxDevice) !void {
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
    try primitiveCapture(&cache, .{ .x = 3, .y = 3, .w = 1, .h = 1 });
    try t.expect(engine.prepared(&cache)); try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
    try t.expect(engine.uploaded_bytes == uploads and frame.assets.converted_pixels == conversions and Model.native_allocations == allocations);
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
    try checkShapesAndLargeImage(graphics, device);
    try checkAssets();
    try checkOutputTransforms(graphics, device);
    std.debug.print("[desktop-primitives] fill/glyph/indexed/alpha/ARGB: bounded GPU capture; no CPU layer pixels; warm upload=0; fallback remains complete\n", .{});
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
                try cache.startOutput(view);
                const painter = (try cache.begin(1, bounds, bounds)).?;
                painter.blitXrgb32(bounds.x, bounds.y, @intCast(bounds.w), @intCast(bounds.h), source);
                try cache.end(1); _ = try cache.finish();
                try t.expect(cache.reserved == 0 and std.meta.eql(cache.commands[0].scissor, geometry.native(view)));
                try engine.prepare(&cache, 1000); try engine.begin(&cache, 1000); try pump(&engine, &cache, device, .copied);
                try t.expectEqualSlices(u32, &expected, &Model.visible);
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
