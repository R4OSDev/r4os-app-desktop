//! Actual R4GFX provider for the existing desktop painter comparison.
const r4os = @import("r4os");
const provider = @import("r4gfx_device_provider");
const renderer = @import("gfx_renderer.zig");
const a = r4os.abi;
pub const table: provider.c.DeviceV1 = .{
    .header = provider.c.device_v1_header,
    .storage_size = provider.storageSize,
    .device_open = provider.open,
    .device_close = provider.close,
    .device_info = provider.info,
    .device_refresh = provider.refresh,
    .resource_create = provider.createResource,
    .resource_retain = provider.retainResource,
    .resource_release = provider.releaseResource,
    .resource_info = provider.resourceInfo,
    .render = provider.render,
    .copy_submit = provider.submitCopy,
    .job_info = provider.jobInfo,
    .job_cancel = provider.cancelJob,
    .job_release = provider.releaseJob,
    .copy_submit_ex = provider.submitCopyEx,
    .job_fence = provider.jobFence,
    .render_submit = provider.submitRender,
    .image_prepare = provider.prepareImage,
    .image_present = provider.presentImage,
    .render_submit_list = provider.submitRenderList,
};
pub const Fixture = struct {
    table_override: ?*const provider.c.DeviceV1 = null,
    sys: a.R4XStartR4Sys = .{},
    draw: a.R4XStartR4Draw = .{},
    imports: [4]a.R4XStartImport = undefined,
    raw: a.R4XStartContext = .{},
    pub fn open(self: *Fixture) !*renderer.Renderer {
        self.imports = .{
            .{ .group_id = @intFromEnum(a.R4LGroup.r4sys), .flags = a.r4xstart_import_flag_group_interface, .table = @intFromPtr(&self.sys) },
            .{ .group_id = @intFromEnum(a.R4LGroup.r4draw), .flags = a.r4xstart_import_flag_group_interface, .table = @intFromPtr(&self.draw) },
            .{ .module_name = @intFromPtr("R4GFX"), .symbol_name = @intFromPtr("DEVICE_V1"), .min_version = provider.c.device_v1_header.abi_minor, .resolved_version = provider.c.device_v1_header.abi_minor, .table = @intFromPtr(self.table_override orelse &table) },
            .{ .module_name = @intFromPtr("R4NV"), .symbol_name = @intFromPtr("BACKEND_V1"), .min_version = 3 },
        };
        self.raw = .{ .flags = a.r4xstart_flag_imports_valid, .imports = @intFromPtr(&self.imports), .import_count = self.imports.len, .instance_id = 9 };
        return renderer.Renderer.create(@import("std").testing.allocator, &self.raw) orelse error.RendererUnavailable;
    }
};
