//! Bounded queue scheduling model for the actual desktop Engine. Resource
//! validation, CPU staging and pixel arithmetic use the real R4GFX provider.
//! NVIDIA packet execution and common BO retention have their owner tests.
const std = @import("std");
const t = std.testing;
const a = @import("r4os").abi;
const p = @import("r4gfx_device_provider");
const c = p.c;
const fixture = @import("gfx_renderer_test.zig");
const layers = @import("composition_layers.zig");
const scene = @import("scene_buffer.zig");
const gpu = @import("composition_gpu.zig");
const surface = @import("surface.zig");
const Model = struct {
    const Operation = union(enum) { copy: c.R4GfxCopyRequestEx, draw: c.R4GfxRenderRequest, present: c.R4GfxImagePresentRequest };
    const Job = struct { handle: c.R4GfxJob, operation: Operation, dependency: u64, result: u32 = 0, terminal: bool = false, cancelled: bool = false };
    var buffers: [c.device_resource_capacity]?[]align(4) u8 = @splat(null);
    var jobs: [c.device_job_capacity]?Job = @splat(null);
    var outcomes: [256]u32 = @splat(0);
    var serial: u64 = 0;
    var native_allocations: u32 = 0;
    var presents: u32 = 0;
    var busy_count: u32 = 0;
    var reject_draw = false;
    var visible: [64]u32 = @splat(0);
    const table: c.DeviceV1 = blk: {
        var value = fixture.table;
        value.device_refresh = refresh; value.resource_create = create; value.resource_release = release;
        value.copy_submit_ex = copy; value.render_submit = render; value.image_present = present;
        value.job_info = info; value.job_fence = fence; value.job_cancel = cancel; value.job_release = releaseJob;
        break :blk value;
    };
    fn refresh(device: *const c.R4GfxDevice, out: *c.R4GfxDeviceInfo) callconv(.c) i32 {
        const rc = p.refresh(device, out);
        if (rc == 0) out.gpu_operations = c.device_gpu_copy_rows | c.device_gpu_render | c.device_gpu_present;
        return rc;
    }
    fn create(device: *const c.R4GfxDevice, input: *const c.R4GfxResourceDesc, out: *c.R4GfxResource) callconv(.c) i32 {
        if (input.kind != c.resource_image or (input.source_kind != c.source_create_native and input.source_kind != c.source_create_system)) return p.createResource(device,input,out);
        var desc = input.*;
        var bytes: u64 = 0;
        if (input.source_kind == c.source_create_native) {
            const request: *const c.R4GfxNativeImage = @ptrFromInt(input.source_address);
            const pitch = (@as(u64,request.width)*4 + 255) & ~@as(u64,255);
            bytes = pitch * request.height;
            desc.image = .{ .cpu_address=0, .byte_length=bytes, .pitch=pitch, .width=request.width, .height=request.height, .format=request.format, .reserved=0 };
        } else bytes = desc.image.byte_length;
        const memory = t.allocator.alignedAlloc(u8,.fromByteUnits(4),@intCast(bytes)) catch return c.status_limit;
        @memset(memory,0);
        desc.source_kind = c.source_borrow_cpu; desc.source_address = 0; desc.source_generation = 1;
        desc.image.cpu_address = @intFromPtr(memory.ptr);
        const rc = p.createResource(device,&desc,out);
        if (rc == 0) {
            buffers[out.slot-1] = memory;
            if (input.source_kind == c.source_create_native) native_allocations += 1;
        } else t.allocator.free(memory);
        return rc;
    }
    fn release(device: *const c.R4GfxDevice, resource: *const c.R4GfxResource) callconv(.c) i32 {
        for (&jobs) |*slot| if (slot.*) |job| if (!job.terminal) {
            const source = switch (job.operation) { .copy => |v| v.copy.source, .draw => |v| v.source, .present => |v| v.source };
            const target = switch (job.operation) { .copy => |v| v.copy.target, .draw => |v| v.target, .present => std.mem.zeroes(c.R4GfxResource) };
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
        if (!find(handle).terminal) return c.status_busy;
        jobs[handle.slot-1]=null; return 0;
    }
    fn image(device: *const c.R4GfxDevice, resource: *const c.R4GfxResource) c.R4GfxCpuImage {
        var value: c.R4GfxResourceInfo=undefined; std.debug.assert(p.resourceInfo(device,resource,&value)==0); return value.image;
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
            .draw => |value| {
                std.debug.assert(value.scissor.x==value.target_rect.x and value.scissor.y==value.target_rect.y and value.scissor.width==value.target_rect.width and value.scissor.height==value.target_rect.height);
                const command:c.R4GfxDraw=.{ .source=value.source,.target=value.target,.pipeline=value.pipeline,.sampler=value.sampler,
                    .source_rect=.{.x=@intCast(value.source_rect.x),.y=@intCast(value.source_rect.y),.width=value.source_rect.width,.height=value.source_rect.height},
                    .target_rect=.{.x=@intCast(value.target_rect.x),.y=@intCast(value.target_rect.y),.width=value.target_rect.width,.height=value.target_rect.height},.color=value.color,.opacity=value.opacity };
                var stats:c.R4GfxRenderStats=undefined;
                std.debug.assert(p.render(device,&.{.commands=@intFromPtr(&command),.command_count=1,.flags=0,.pixel_budget=64},&stats)==0);
            },
            .present => |value| {
                const src=image(device,&value.source); const from:[*]const u8=@ptrFromInt(src.cpu_address);
                std.debug.assert(src.width==8 and src.height==8);
                for(0..8) |row| @memcpy(std.mem.sliceAsBytes(visible[row*8..][0..8]),from[row*src.pitch..][0..32]);
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
    var source:fixture.Fixture=.{.table_override=&Model.table};
    const graphics=try source.open(); defer graphics.destroy();
    var cache=layers.Cache.init(t.allocator,1024*1024); defer cache.deinit();
    var engine=gpu.Engine.init(&graphics.client,&graphics.device);
    const device:*const c.R4GfxDevice=@ptrCast(&graphics.device);
    const full:surface.Rect=.{.x=0,.y=0,.w=8,.h=8}; const pixel:surface.Rect=.{.x=3,.y=3,.w=1,.h=1};
    try capture(&cache,full,0x882244);
    try engine.prepare(&cache,1000); try engine.begin(&cache,1000);
    try pump(&engine,&cache,device,.copied);
    try t.expect(Model.presents==1 and Model.native_allocations==3 and engine.uploaded_bytes==320);
    var expected:[64]u32=undefined; var target:scene.SceneBuffer=.{};
    try t.expect(target.attach(std.mem.sliceAsBytes(&expected),8,8));
    _=try @import("composition_software.zig").paint(&graphics.client,&graphics.device,&cache,&target);
    try t.expectEqualSlices(u32,&expected,&Model.visible);
    const serial=Model.serial;
    for(0..8) |_| try t.expectEqual(gpu.Progress.copied,engine.advance(&cache,1));
    try t.expect(Model.serial==serial);
    try capture(&cache,pixel,0x445566);
    try t.expect(engine.prepared(&cache));
    try engine.begin(&cache,1000); try pump(&engine,&cache,device,.copied);
    _=try @import("composition_software.zig").paint(&graphics.client,&graphics.device,&cache,&target);
    try t.expectEqualSlices(u32,&expected,&Model.visible);
    try t.expect(Model.presents==2 and Model.native_allocations==3 and engine.uploaded_bytes==324);
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
    _=try @import("composition_software.zig").paint(&graphics.client,&graphics.device,&cache,&target);
    try t.expectEqualSlices(u32,&expected,&Model.visible);
    try engine.close();
    for(&Model.jobs) |*job| try t.expect(job.*==null);
    for(&Model.buffers) |*buffer| try t.expect(buffer.*==null);
    try t.expect(engine.reserved_bytes==0);
}
