const std = @import("std");
const r4os = @import("r4os");

const command_alignment = @alignOf(r4os.abi.GuiFrameCommand);
const resource_alignment = @alignOf(u32);

pub const RefreshResult = enum {
    unchanged,
    updated,
    no_frame,
    retry,
};

pub const View = struct {
    supported: bool = false,
    valid: bool = false,
    full_damage: bool = true,
    info: r4os.abi.GuiFrameInfo = .{},
    commands: []const r4os.abi.GuiFrameCommand = &.{},
    resources: []const u8 = &.{},
    shared_rasters: []const r4os.abi.GuiSharedRasterMap = &.{},
    damage_regions: []const r4os.abi.DisplayDamageRect = &.{},
};

const Buffer = struct {
    info: r4os.abi.GuiFrameInfo = .{},
    command_memory: ?[]r4os.abi.GuiFrameCommand = null,
    resource_memory: ?[]align(resource_alignment) u8 = null,
    command_len: usize = 0,
    resource_len: usize = 0,
    shared_rasters: [r4os.abi.gui_shared_raster_max_frame_resources]r4os.abi.GuiSharedRasterMap =
        .{r4os.abi.GuiSharedRasterMap{}} ** r4os.abi.gui_shared_raster_max_frame_resources,
    shared_raster_count: usize = 0,
    damage_regions: [r4os.abi.gui_frame_max_damage_regions]r4os.abi.DisplayDamageRect = .{r4os.abi.DisplayDamageRect{}} ** r4os.abi.gui_frame_max_damage_regions,
    damage_count: usize = 0,
    full_damage: bool = true,
    valid: bool = false,

    fn commands(self: *Buffer) []r4os.abi.GuiFrameCommand {
        const memory = self.command_memory orelse return &.{};
        return memory[0..self.command_len];
    }

    fn resources(self: *Buffer) []u8 {
        const memory = self.resource_memory orelse return &.{};
        return memory[0..self.resource_len];
    }

    fn constCommands(self: *const Buffer) []const r4os.abi.GuiFrameCommand {
        const memory = self.command_memory orelse return &.{};
        return memory[0..self.command_len];
    }

    fn constResources(self: *const Buffer) []const u8 {
        const memory = self.resource_memory orelse return &.{};
        return memory[0..self.resource_len];
    }

    fn ensure(self: *Buffer, allocator: std.mem.Allocator, command_count: usize, resource_bytes: usize) bool {
        if (!self.ensureCommands(allocator, command_count)) return false;
        if (!self.ensureResources(allocator, resource_bytes)) return false;
        self.command_len = command_count;
        self.resource_len = resource_bytes;
        return true;
    }

    fn ensureCommands(self: *Buffer, allocator: std.mem.Allocator, wanted: usize) bool {
        const current = if (self.command_memory) |memory| memory.len else 0;
        if (current >= wanted) return true;
        const grown = @max(wanted, @max(@as(usize, 16), current *| 2));
        const replacement = allocator.alloc(r4os.abi.GuiFrameCommand, grown) catch return false;
        if (self.command_memory) |memory| {
            @memcpy(replacement[0..self.command_len], memory[0..self.command_len]);
            allocator.free(memory);
        }
        self.command_memory = replacement;
        return true;
    }

    fn ensureResources(self: *Buffer, allocator: std.mem.Allocator, wanted: usize) bool {
        const current = if (self.resource_memory) |memory| memory.len else 0;
        if (current >= wanted) return true;
        const grown = @max(wanted, @max(@as(usize, 4096), current *| 2));
        const replacement = allocator.alignedAlloc(u8, .fromByteUnits(resource_alignment), grown) catch return false;
        if (self.resource_memory) |memory| {
            @memcpy(replacement[0..self.resource_len], memory[0..self.resource_len]);
            allocator.free(memory);
        }
        self.resource_memory = replacement;
        return true;
    }

    fn deinit(self: *Buffer, allocator: std.mem.Allocator) void {
        std.debug.assert(self.shared_raster_count == 0);
        if (self.command_memory) |memory| allocator.free(memory);
        if (self.resource_memory) |memory| allocator.free(memory);
        self.* = .{};
    }
};

pub const Cache = struct {
    owner: r4os.abi.ProgramProcessHandle = .{},
    active: Buffer = .{},
    staging: Buffer = .{},
    pending: bool = false,
    out_of_memory: bool = false,
    delta_refreshes: u64 = 0,
    replacement_refreshes: u64 = 0,
    full_refreshes: u64 = 0,
    delta_fallbacks: u64 = 0,
    replacement_fallbacks: u64 = 0,
    appended_commands: u64 = 0,
    appended_resource_bytes: u64 = 0,

    pub fn bind(self: *Cache, allocator: std.mem.Allocator, owner: r4os.abi.ProgramProcessHandle) void {
        if (sameHandle(self.owner, owner)) {
            self.pending = true;
            return;
        }
        self.deinit(allocator);
        self.owner = owner;
        self.pending = true;
    }

    pub fn deinit(self: *Cache, allocator: std.mem.Allocator) void {
        self.active.deinit(allocator);
        self.staging.deinit(allocator);
        self.* = .{};
    }

    pub fn releaseSharedRasters(self: *Cache, reader: anytype) void {
        releaseBufferSharedRasters(&self.active, reader);
        releaseBufferSharedRasters(&self.staging, reader);
    }

    pub fn markPending(self: *Cache) void {
        self.pending = true;
    }

    pub fn view(self: *const Cache) View {
        if (!self.active.valid) return .{};
        return .{
            .supported = true,
            .valid = true,
            .full_damage = self.active.full_damage,
            .info = self.active.info,
            .commands = self.active.constCommands(),
            .resources = self.active.constResources(),
            .shared_rasters = self.active.shared_rasters[0..self.active.shared_raster_count],
            .damage_regions = self.active.damage_regions[0..self.active.damage_count],
        };
    }

    pub fn refresh(self: *Cache, allocator: std.mem.Allocator, reader: anytype) RefreshResult {
        if (!validHandle(self.owner)) return .no_frame;

        var info: r4os.abi.GuiFrameInfo = .{};
        if (reader.guiFrameInfo(&self.owner, &info) != r4os.abi.gui_frame_result_ok) {
            self.pending = true;
            return .retry;
        }
        if (!validInfo(info, self.owner)) {
            self.pending = true;
            return .retry;
        }
        if ((info.flags & r4os.abi.gui_frame_flag_committed) == 0 or info.committed_generation == 0) {
            self.pending = !self.active.valid;
            return .no_frame;
        }
        if (self.active.valid and
            self.active.info.committed_generation == info.committed_generation and
            sameHandle(self.active.info.owner, info.owner))
        {
            self.active.info = info;
            self.pending = false;
            self.out_of_memory = false;
            return .unchanged;
        }

        var generation_info: r4os.abi.GuiFrameGenerationInfo = .{};
        const generation_rc = reader.guiFrameGenerationInfo(&self.owner, info.committed_generation, &generation_info);
        const generation_valid = generation_rc == r4os.abi.gui_frame_result_ok and
            validGenerationInfo(generation_info, self.owner, info);
        if (generation_valid and
            (generation_info.flags & r4os.abi.gui_frame_generation_flag_replacement) != 0 and
            generation_info.base_generation == 0)
        {
            const replacement_result = self.replaceGeneration(allocator, reader, info, generation_info);
            if (replacement_result != .retry) return replacement_result;
            self.replacement_fallbacks +%= 1;
        } else if (generation_valid and self.active.valid and
            (generation_info.flags & r4os.abi.gui_frame_generation_flag_delta) != 0 and
            generation_info.base_generation == self.active.info.committed_generation)
        {
            const delta_result = self.appendDelta(allocator, reader, info, generation_info);
            if (delta_result != .retry) return delta_result;
            self.delta_fallbacks +%= 1;
        } else if (self.active.valid and info.committed_generation != self.active.info.committed_generation) {
            self.delta_fallbacks +%= 1;
        }

        const command_count = std.math.cast(usize, info.committed_command_count) orelse {
            self.pending = true;
            return .retry;
        };
        const resource_bytes = std.math.cast(usize, info.committed_resource_bytes) orelse {
            self.pending = true;
            return .retry;
        };
        _ = std.math.mul(usize, command_count, @sizeOf(r4os.abi.GuiFrameCommand)) catch {
            self.pending = true;
            return .retry;
        };
        if (!self.staging.ensure(allocator, command_count, resource_bytes)) {
            self.pending = true;
            self.out_of_memory = true;
            return .retry;
        }

        var read_info: r4os.abi.GuiFrameInfo = .{};
        const rc = reader.guiFrameRead(
            &self.owner,
            info.committed_generation,
            self.staging.commands(),
            self.staging.resources(),
            &read_info,
        );
        if (rc != r4os.abi.gui_frame_result_ok or
            !validInfo(read_info, self.owner) or
            read_info.committed_generation != info.committed_generation or
            read_info.committed_command_count != info.committed_command_count or
            read_info.committed_resource_bytes != info.committed_resource_bytes)
        {
            self.pending = true;
            return .retry;
        }

        if (!acquireBufferSharedRasters(&self.staging, reader, self.owner, read_info.committed_generation)) {
            self.pending = true;
            return .retry;
        }
        self.staging.info = read_info;
        self.staging.full_damage = true;
        self.staging.damage_count = 0;
        self.staging.valid = true;
        std.mem.swap(Buffer, &self.active, &self.staging);
        releaseBufferSharedRasters(&self.staging, reader);
        self.staging.valid = false;
        self.staging.command_len = 0;
        self.staging.resource_len = 0;
        self.releaseOversizedStaging(allocator);
        self.pending = false;
        self.out_of_memory = false;
        self.full_refreshes +%= 1;
        return .updated;
    }

    fn replaceGeneration(self: *Cache, allocator: std.mem.Allocator, reader: anytype, info: r4os.abi.GuiFrameInfo, generation_info: r4os.abi.GuiFrameGenerationInfo) RefreshResult {
        const command_count = std.math.cast(usize, generation_info.command_count) orelse return .retry;
        const resource_bytes = std.math.cast(usize, generation_info.resource_bytes) orelse return .retry;
        if (generation_info.total_command_count != generation_info.command_count or
            generation_info.total_resource_bytes != generation_info.resource_bytes or
            info.committed_command_count != generation_info.command_count or
            info.committed_resource_bytes != generation_info.resource_bytes or
            generation_info.chain_depth != 1 or
            generation_info.damage_count == 0 or generation_info.damage_count > r4os.abi.gui_frame_max_damage_regions)
        {
            return .retry;
        }
        if (!self.staging.ensure(allocator, command_count, resource_bytes)) {
            self.pending = true;
            self.out_of_memory = true;
            return .retry;
        }

        var read_info: r4os.abi.GuiFrameGenerationInfo = .{};
        var damage_regions: [r4os.abi.gui_frame_max_damage_regions]r4os.abi.DisplayDamageRect = undefined;
        const rc = reader.guiFrameGenerationRead(
            &self.owner,
            generation_info.generation,
            self.staging.commands(),
            self.staging.resources(),
            damage_regions[0..],
            &read_info,
        );
        if (rc != r4os.abi.gui_frame_result_ok or !validGenerationInfo(read_info, self.owner, info) or
            read_info.flags != generation_info.flags or read_info.base_generation != 0 or
            read_info.command_count != generation_info.command_count or read_info.resource_bytes != generation_info.resource_bytes or
            read_info.damage_count != generation_info.damage_count or read_info.chain_depth != 1)
        {
            self.pending = true;
            return .retry;
        }

        if (!acquireBufferSharedRasters(&self.staging, reader, self.owner, read_info.generation)) {
            self.pending = true;
            return .retry;
        }
        self.staging.info = info;
        self.staging.full_damage = !self.active.valid;
        self.staging.damage_count = if (self.active.valid) read_info.damage_count else 0;
        if (self.staging.damage_count != 0) {
            @memcpy(self.staging.damage_regions[0..self.staging.damage_count], damage_regions[0..self.staging.damage_count]);
        }
        self.staging.valid = true;
        std.mem.swap(Buffer, &self.active, &self.staging);
        releaseBufferSharedRasters(&self.staging, reader);
        self.staging.valid = false;
        self.staging.command_len = 0;
        self.staging.resource_len = 0;
        self.releaseOversizedStaging(allocator);
        self.pending = false;
        self.out_of_memory = false;
        self.replacement_refreshes +%= 1;
        return .updated;
    }

    fn appendDelta(self: *Cache, allocator: std.mem.Allocator, reader: anytype, info: r4os.abi.GuiFrameInfo, generation_info: r4os.abi.GuiFrameGenerationInfo) RefreshResult {
        const command_count = std.math.cast(usize, generation_info.command_count) orelse return .retry;
        const resource_bytes = std.math.cast(usize, generation_info.resource_bytes) orelse return .retry;
        const total_command_count = std.math.cast(usize, generation_info.total_command_count) orelse return .retry;
        const total_resource_bytes = std.math.cast(usize, generation_info.total_resource_bytes) orelse return .retry;
        const expected_commands = std.math.add(usize, self.active.command_len, command_count) catch return .retry;
        const expected_resources = std.math.add(usize, self.active.resource_len, resource_bytes) catch return .retry;
        if (expected_commands != total_command_count or expected_resources != total_resource_bytes or
            info.committed_command_count != total_command_count or info.committed_resource_bytes != total_resource_bytes)
        {
            return .retry;
        }
        if (!self.active.ensureCommands(allocator, total_command_count) or !self.active.ensureResources(allocator, total_resource_bytes)) {
            self.pending = true;
            self.out_of_memory = true;
            return .retry;
        }

        const command_base = self.active.command_len;
        const resource_base = self.active.resource_len;
        var read_info: r4os.abi.GuiFrameGenerationInfo = .{};
        var damage_regions: [r4os.abi.gui_frame_max_damage_regions]r4os.abi.DisplayDamageRect = undefined;
        const command_memory = self.active.command_memory orelse return .retry;
        var empty_resource: [0]u8 = .{};
        const resource_target: []u8 = if (self.active.resource_memory) |memory|
            memory[resource_base..total_resource_bytes]
        else if (resource_bytes == 0)
            empty_resource[0..]
        else
            return .retry;
        const rc = reader.guiFrameGenerationRead(
            &self.owner,
            generation_info.generation,
            command_memory[command_base..total_command_count],
            resource_target,
            damage_regions[0..],
            &read_info,
        );
        if (rc != r4os.abi.gui_frame_result_ok or !validGenerationInfo(read_info, self.owner, info) or
            read_info.generation != generation_info.generation or read_info.base_generation != generation_info.base_generation or
            read_info.command_count != generation_info.command_count or read_info.resource_bytes != generation_info.resource_bytes or
            read_info.damage_count > damage_regions.len)
        {
            self.pending = true;
            return .retry;
        }

        if (commandsContainSharedRaster(command_memory[command_base..total_command_count])) {
            self.pending = true;
            return .retry;
        }

        for (command_memory[command_base..total_command_count]) |*command| {
            if (command.resource_bytes == 0) continue;
            command.resource_offset = std.math.add(u64, command.resource_offset, resource_base) catch {
                self.pending = true;
                return .retry;
            };
        }
        self.active.command_len = total_command_count;
        self.active.resource_len = total_resource_bytes;
        self.active.info = info;
        self.active.full_damage = false;
        self.active.damage_count = read_info.damage_count;
        @memcpy(self.active.damage_regions[0..self.active.damage_count], damage_regions[0..self.active.damage_count]);
        self.active.valid = true;
        self.pending = false;
        self.out_of_memory = false;
        self.delta_refreshes +%= 1;
        self.appended_commands +%= command_count;
        self.appended_resource_bytes +%= resource_bytes;
        return .updated;
    }

    fn releaseOversizedStaging(self: *Cache, allocator: std.mem.Allocator) void {
        const active_commands = self.active.command_len;
        const active_resources = self.active.resource_len;
        const staging_commands = if (self.staging.command_memory) |memory| memory.len else 0;
        const staging_resources = if (self.staging.resource_memory) |memory| memory.len else 0;
        const command_oversized = staging_commands > 64 and staging_commands / 4 > active_commands;
        const resource_oversized = staging_resources > 64 * 1024 and staging_resources / 4 > active_resources;
        if (command_oversized or resource_oversized) self.staging.deinit(allocator);
    }
};

fn sameSharedRasterHandle(a: r4os.abi.GuiSharedRasterHandle, b: r4os.abi.GuiSharedRasterHandle) bool {
    return a.id == b.id and a.generation == b.generation;
}

fn releaseBufferSharedRasters(buffer: *Buffer, reader: anytype) void {
    for (buffer.shared_rasters[0..buffer.shared_raster_count]) |*map| _ = reader.guiSharedRasterRelease(&map.lease);
    buffer.shared_raster_count = 0;
}

fn frameCommandResource(buffer: *const Buffer, command: r4os.abi.GuiFrameCommand) ?[]const u8 {
    const offset = std.math.cast(usize, command.resource_offset) orelse return null;
    const length = std.math.cast(usize, command.resource_bytes) orelse return null;
    const end = std.math.add(usize, offset, length) catch return null;
    const resources = buffer.constResources();
    if (end > resources.len) return null;
    return resources[offset..end];
}

fn sharedRasterDescriptor(buffer: *const Buffer, command: r4os.abi.GuiFrameCommand) ?r4os.abi.GuiSharedRasterResource {
    if (command.kind != r4os.abi.gui_frame_command_kind_shared_raster or
        command.resource_bytes != r4os.abi.gui_shared_raster_resource_size)
    {
        return null;
    }
    const bytes = frameCommandResource(buffer, command) orelse return null;
    if (bytes.len != @sizeOf(r4os.abi.GuiSharedRasterResource)) return null;
    var descriptor: r4os.abi.GuiSharedRasterResource = .{};
    @memcpy(std.mem.asBytes(&descriptor), bytes);
    if (descriptor.version != r4os.abi.gui_shared_raster_resource_version or
        descriptor.size != r4os.abi.gui_shared_raster_resource_size or descriptor.handle.id == 0 or
        descriptor.handle.generation == 0 or descriptor.raster_generation == 0 or descriptor.flags != 0)
    {
        return null;
    }
    return descriptor;
}

fn validSharedRasterMap(
    map: r4os.abi.GuiSharedRasterMap,
    owner: r4os.abi.ProgramProcessHandle,
    frame_generation: u64,
    descriptor: r4os.abi.GuiSharedRasterResource,
) bool {
    if (map.version < r4os.abi.gui_shared_raster_map_version or map.size < r4os.abi.gui_shared_raster_map_size or
        !sameHandle(map.frame_owner, owner) or map.frame_generation != frame_generation or map.flags != 0 or
        !sameSharedRasterHandle(map.lease.handle, descriptor.handle) or
        map.lease.raster_generation != descriptor.raster_generation or map.lease.lease_token == 0 or
        map.lease.reserved0 != 0 or map.data_address == 0 or map.byte_length == 0 or
        map.byte_length > r4os.abi.gui_shared_raster_max_bytes or map.format != descriptor.format or
        map.width != descriptor.guest_w or map.height != descriptor.guest_h or map.width == 0 or map.height == 0)
    {
        return false;
    }
    const minimum_stride: u64 = switch (map.format) {
        r4os.abi.gui_shared_raster_format_xrgb32 => blk: {
            if (map.data_offset != 0 or map.stride_bytes % @sizeOf(u32) != 0) return false;
            break :blk std.math.mul(u64, map.width, @sizeOf(u32)) catch return false;
        },
        r4os.abi.gui_shared_raster_format_indexed8 => blk: {
            if (map.data_offset != r4os.abi.gui_indexed8_pixels_offset) return false;
            break :blk map.width;
        },
        r4os.abi.gui_shared_raster_format_alpha8 => blk: {
            if (map.data_offset != 0) return false;
            break :blk map.width;
        },
        else => return false,
    };
    if (map.stride_bytes < minimum_stride) return false;
    const rows = std.math.mul(u64, map.stride_bytes, map.height) catch return false;
    const required = std.math.add(u64, map.data_offset, rows) catch return false;
    return required == map.byte_length;
}

fn acquireBufferSharedRasters(
    buffer: *Buffer,
    reader: anytype,
    owner: r4os.abi.ProgramProcessHandle,
    frame_generation: u64,
) bool {
    if (buffer.shared_raster_count != 0) releaseBufferSharedRasters(buffer, reader);
    for (buffer.constCommands()) |command| {
        if (command.kind != r4os.abi.gui_frame_command_kind_shared_raster) continue;
        const descriptor = sharedRasterDescriptor(buffer, command) orelse {
            releaseBufferSharedRasters(buffer, reader);
            return false;
        };
        var duplicate = false;
        for (buffer.shared_rasters[0..buffer.shared_raster_count]) |map| {
            if (sameSharedRasterHandle(map.lease.handle, descriptor.handle) and
                map.lease.raster_generation == descriptor.raster_generation)
            {
                duplicate = true;
                break;
            }
        }
        if (duplicate) continue;
        if (buffer.shared_raster_count >= buffer.shared_rasters.len) {
            releaseBufferSharedRasters(buffer, reader);
            return false;
        }
        var map: r4os.abi.GuiSharedRasterMap = .{};
        const result = reader.guiSharedRasterAcquire(&owner, frame_generation, &descriptor.handle, descriptor.raster_generation, &map);
        if (result != r4os.abi.gui_frame_result_ok or !validSharedRasterMap(map, owner, frame_generation, descriptor)) {
            if (result == r4os.abi.gui_frame_result_ok) _ = reader.guiSharedRasterRelease(&map.lease);
            releaseBufferSharedRasters(buffer, reader);
            return false;
        }
        buffer.shared_rasters[buffer.shared_raster_count] = map;
        buffer.shared_raster_count += 1;
    }
    return true;
}

fn commandsContainSharedRaster(commands: []const r4os.abi.GuiFrameCommand) bool {
    for (commands) |command| if (command.kind == r4os.abi.gui_frame_command_kind_shared_raster) return true;
    return false;
}

fn validHandle(handle: r4os.abi.ProgramProcessHandle) bool {
    return handle.instance_id != 0 and handle.reserved == 0 and handle.generation != 0;
}

fn sameHandle(a: r4os.abi.ProgramProcessHandle, b: r4os.abi.ProgramProcessHandle) bool {
    return a.instance_id == b.instance_id and a.reserved == b.reserved and a.generation == b.generation;
}

fn validInfo(info: r4os.abi.GuiFrameInfo, owner: r4os.abi.ProgramProcessHandle) bool {
    return info.version >= r4os.abi.gui_frame_info_version and
        info.size >= r4os.abi.gui_frame_info_size and
        info.command_version == r4os.abi.gui_frame_command_version and
        info.command_size == r4os.abi.gui_frame_command_size and
        sameHandle(info.owner, owner);
}

fn validGenerationInfo(info: r4os.abi.GuiFrameGenerationInfo, owner: r4os.abi.ProgramProcessHandle, frame: r4os.abi.GuiFrameInfo) bool {
    return info.version >= r4os.abi.gui_frame_generation_info_version and
        info.size >= r4os.abi.gui_frame_generation_info_size and
        info.command_version == r4os.abi.gui_frame_command_version and
        info.command_size == r4os.abi.gui_frame_command_size and
        info.region_size == @sizeOf(r4os.abi.DisplayDamageRect) and
        info.generation == frame.committed_generation and
        info.total_command_count == frame.committed_command_count and
        info.total_resource_bytes == frame.committed_resource_bytes and
        sameHandle(info.owner, owner);
}

const FakeReader = struct {
    info: r4os.abi.GuiFrameInfo,
    commands: []const r4os.abi.GuiFrameCommand,
    resources: []const u8,
    stale_once: bool = false,
    reads: usize = 0,
    generation_supported: bool = false,
    generation_info: r4os.abi.GuiFrameGenerationInfo = .{},
    generation_commands: []const r4os.abi.GuiFrameCommand = &.{},
    generation_resources: []const u8 = &.{},
    generation_regions: []const r4os.abi.DisplayDamageRect = &.{},
    shared_map: r4os.abi.GuiSharedRasterMap = .{},
    shared_acquire_result: i32 = r4os.abi.gui_frame_result_ok,
    shared_acquires: u32 = 0,
    shared_releases: u32 = 0,

    fn guiFrameInfo(self: *FakeReader, handle: *const r4os.abi.ProgramProcessHandle, out: *r4os.abi.GuiFrameInfo) i32 {
        if (!sameHandle(handle.*, self.info.owner)) return r4os.abi.gui_frame_error_invalid;
        out.* = self.info;
        return r4os.abi.gui_frame_result_ok;
    }

    fn guiFrameRead(
        self: *FakeReader,
        handle: *const r4os.abi.ProgramProcessHandle,
        generation: u64,
        commands: []r4os.abi.GuiFrameCommand,
        resources: []u8,
        out: *r4os.abi.GuiFrameInfo,
    ) i32 {
        self.reads += 1;
        out.* = self.info;
        if (!sameHandle(handle.*, self.info.owner)) return r4os.abi.gui_frame_error_invalid;
        if (self.stale_once) {
            self.stale_once = false;
            return r4os.abi.gui_frame_error_stale;
        }
        if (generation != self.info.committed_generation) return r4os.abi.gui_frame_error_stale;
        if (commands.len < self.commands.len or resources.len < self.resources.len) return r4os.abi.gui_frame_error_buffer_too_small;
        @memcpy(commands[0..self.commands.len], self.commands);
        @memcpy(resources[0..self.resources.len], self.resources);
        return r4os.abi.gui_frame_result_ok;
    }

    fn guiFrameGenerationInfo(self: *FakeReader, handle: *const r4os.abi.ProgramProcessHandle, generation: u64, out: *r4os.abi.GuiFrameGenerationInfo) i32 {
        if (!self.generation_supported) return r4os.abi.err_no_fn;
        if (!sameHandle(handle.*, self.info.owner) or generation != self.generation_info.generation) return r4os.abi.gui_frame_error_stale;
        out.* = self.generation_info;
        return r4os.abi.gui_frame_result_ok;
    }

    fn guiFrameGenerationRead(
        self: *FakeReader,
        handle: *const r4os.abi.ProgramProcessHandle,
        generation: u64,
        commands: []r4os.abi.GuiFrameCommand,
        resources: []u8,
        regions: []r4os.abi.DisplayDamageRect,
        out: *r4os.abi.GuiFrameGenerationInfo,
    ) i32 {
        if (!self.generation_supported or !sameHandle(handle.*, self.info.owner) or generation != self.generation_info.generation) return r4os.abi.gui_frame_error_stale;
        if (commands.len < self.generation_commands.len or resources.len < self.generation_resources.len or regions.len < self.generation_regions.len) return r4os.abi.gui_frame_error_buffer_too_small;
        @memcpy(commands[0..self.generation_commands.len], self.generation_commands);
        @memcpy(resources[0..self.generation_resources.len], self.generation_resources);
        @memcpy(regions[0..self.generation_regions.len], self.generation_regions);
        out.* = self.generation_info;
        return r4os.abi.gui_frame_result_ok;
    }

    fn guiSharedRasterAcquire(
        self: *FakeReader,
        owner: *const r4os.abi.ProgramProcessHandle,
        frame_generation: u64,
        handle: *const r4os.abi.GuiSharedRasterHandle,
        raster_generation: u64,
        out: *r4os.abi.GuiSharedRasterMap,
    ) i32 {
        if (self.shared_acquire_result != r4os.abi.gui_frame_result_ok) return self.shared_acquire_result;
        if (!sameHandle(owner.*, self.shared_map.frame_owner) or frame_generation != self.shared_map.frame_generation or
            !sameSharedRasterHandle(handle.*, self.shared_map.lease.handle) or raster_generation != self.shared_map.lease.raster_generation)
        {
            return r4os.abi.gui_frame_error_stale;
        }
        self.shared_acquires += 1;
        out.* = self.shared_map;
        out.lease.lease_token +%= self.shared_acquires - 1;
        return r4os.abi.gui_frame_result_ok;
    }

    fn guiSharedRasterRelease(self: *FakeReader, lease: *const r4os.abi.GuiSharedRasterLease) i32 {
        if (lease.lease_token == 0) return r4os.abi.gui_frame_error_invalid;
        self.shared_releases += 1;
        return r4os.abi.gui_frame_result_ok;
    }
};

fn fakeInfo(owner: r4os.abi.ProgramProcessHandle, generation: u64, command_count: usize, resource_bytes: usize) r4os.abi.GuiFrameInfo {
    return .{
        .flags = r4os.abi.gui_frame_flag_committed,
        .owner = owner,
        .committed_generation = generation,
        .committed_command_count = command_count,
        .committed_resource_bytes = resource_bytes,
    };
}

fn fakeGenerationInfo(owner: r4os.abi.ProgramProcessHandle, generation: u64, base_generation: u64, command_count: usize, resource_bytes: usize, total_command_count: usize, total_resource_bytes: usize, damage_count: usize) r4os.abi.GuiFrameGenerationInfo {
    return .{
        .flags = if (base_generation == 0) r4os.abi.gui_frame_generation_flag_full else r4os.abi.gui_frame_generation_flag_delta,
        .damage_count = @intCast(damage_count),
        .owner = owner,
        .generation = generation,
        .base_generation = base_generation,
        .command_count = command_count,
        .resource_bytes = resource_bytes,
        .total_command_count = total_command_count,
        .total_resource_bytes = total_resource_bytes,
    };
}

test "snapshot refresh publishes only a complete generation" {
    const allocator = std.testing.allocator;
    const owner = r4os.abi.ProgramProcessHandle{ .instance_id = 7, .generation = 11 };
    const first_commands = [_]r4os.abi.GuiFrameCommand{.{ .kind = r4os.abi.gui_frame_command_kind_rect, .rgb = 0x112233 }};
    const second_commands = [_]r4os.abi.GuiFrameCommand{
        .{ .kind = r4os.abi.gui_frame_command_kind_clear, .rgb = 0x010203 },
        .{ .kind = r4os.abi.gui_frame_command_kind_text, .resource_bytes = 3 },
    };
    var reader = FakeReader{
        .info = fakeInfo(owner, 1, first_commands.len, 0),
        .commands = first_commands[0..],
        .resources = &.{},
    };
    var cache = Cache{};
    defer cache.deinit(allocator);
    cache.bind(allocator, owner);
    try std.testing.expectEqual(RefreshResult.updated, cache.refresh(allocator, &reader));
    try std.testing.expectEqual(@as(u64, 1), cache.view().info.committed_generation);

    reader.info = fakeInfo(owner, 2, second_commands.len, 3);
    reader.commands = second_commands[0..];
    reader.resources = "new";
    reader.stale_once = true;
    try std.testing.expectEqual(RefreshResult.retry, cache.refresh(allocator, &reader));
    try std.testing.expectEqual(@as(u64, 1), cache.view().info.committed_generation);
    try std.testing.expectEqual(@as(u32, 0x112233), cache.view().commands[0].rgb);

    try std.testing.expectEqual(RefreshResult.updated, cache.refresh(allocator, &reader));
    const view = cache.view();
    try std.testing.expectEqual(@as(u64, 2), view.info.committed_generation);
    try std.testing.expectEqual(@as(usize, 2), view.commands.len);
    try std.testing.expectEqualStrings("new", view.resources);
}

test "snapshot publishes one combined generation from buffered Canvas chunks" {
    const allocator = std.testing.allocator;
    const owner = r4os.abi.ProgramProcessHandle{ .instance_id = 8, .generation = 12 };
    const commands = [_]r4os.abi.GuiFrameCommand{
        .{ .kind = r4os.abi.gui_frame_command_kind_clear, .rgb = 0x010203 },
        .{ .kind = r4os.abi.gui_frame_command_kind_rect, .x = 1, .y = 2, .w = 3, .h = 4, .rgb = 0xA0B0C0 },
        .{ .kind = r4os.abi.gui_frame_command_kind_text, .resource_bytes = 2 },
        .{ .kind = r4os.abi.gui_frame_command_kind_raster, .w = 1, .h = 1, .resource_offset = 2, .resource_bytes = 4, .parameter0 = 1 },
    };
    const resources = [_]u8{ 'O', 'K', 0x33, 0x22, 0x11, 0 };
    var reader = FakeReader{
        .info = fakeInfo(owner, 9, commands.len, resources.len),
        .commands = commands[0..],
        .resources = resources[0..],
    };
    var cache = Cache{};
    defer cache.deinit(allocator);
    cache.bind(allocator, owner);

    try std.testing.expectEqual(RefreshResult.updated, cache.refresh(allocator, &reader));
    const view = cache.view();
    try std.testing.expect(view.valid and view.full_damage);
    try std.testing.expectEqual(@as(u64, 9), view.info.committed_generation);
    try std.testing.expectEqual(@as(usize, commands.len), view.commands.len);
    try std.testing.expectEqual(@as(u64, 0), view.commands[2].resource_offset);
    try std.testing.expectEqual(@as(u64, 2), view.commands[3].resource_offset);
    try std.testing.expectEqualSlices(u8, resources[0..], view.resources);

    reader.stale_once = true;
    reader.info.committed_generation = 10;
    try std.testing.expectEqual(RefreshResult.retry, cache.refresh(allocator, &reader));
    try std.testing.expectEqual(@as(u64, 9), cache.view().info.committed_generation);
    try std.testing.expectEqualSlices(u8, resources[0..], cache.view().resources);
}

test "snapshot acquires one shared lease per generation and retains the old frame on backpressure" {
    const allocator = std.testing.allocator;
    const owner = r4os.abi.ProgramProcessHandle{ .instance_id = 27, .generation = 31 };
    const handle = r4os.abi.GuiSharedRasterHandle{ .id = 3, .generation = 5 };
    var pixels = [_]u32{ 0xAA112233, 0xBB445566, 0xCC778899, 0xDDAABBCC };
    var descriptor = r4os.abi.GuiSharedRasterResource{
        .handle = handle,
        .raster_generation = 41,
        .format = r4os.abi.gui_shared_raster_format_xrgb32,
        .source_w = 2,
        .source_h = 2,
        .guest_w = 2,
        .guest_h = 2,
        .viewport_w = 2,
        .viewport_h = 2,
    };
    var resources: [2 * @sizeOf(r4os.abi.GuiSharedRasterResource)]u8 = undefined;
    @memcpy(resources[0..@sizeOf(r4os.abi.GuiSharedRasterResource)], std.mem.asBytes(&descriptor));
    @memcpy(resources[@sizeOf(r4os.abi.GuiSharedRasterResource)..], std.mem.asBytes(&descriptor));
    const commands = [_]r4os.abi.GuiFrameCommand{
        .{ .kind = r4os.abi.gui_frame_command_kind_shared_raster, .w = 1, .h = 2, .resource_bytes = @sizeOf(r4os.abi.GuiSharedRasterResource) },
        .{ .kind = r4os.abi.gui_frame_command_kind_shared_raster, .x = 1, .w = 1, .h = 2, .resource_offset = @sizeOf(r4os.abi.GuiSharedRasterResource), .resource_bytes = @sizeOf(r4os.abi.GuiSharedRasterResource) },
    };
    var reader = FakeReader{
        .info = fakeInfo(owner, 71, commands.len, resources.len),
        .commands = commands[0..],
        .resources = resources[0..],
        .shared_map = .{
            .frame_owner = owner,
            .frame_generation = 71,
            .lease = .{ .handle = handle, .raster_generation = descriptor.raster_generation, .lease_token = 91 },
            .data_address = @intFromPtr(&pixels),
            .byte_length = @sizeOf(@TypeOf(pixels)),
            .format = r4os.abi.gui_shared_raster_format_xrgb32,
            .width = 2,
            .height = 2,
            .stride_bytes = 2 * @sizeOf(u32),
        },
    };
    var cache = Cache{};
    defer cache.deinit(allocator);
    defer cache.releaseSharedRasters(&reader);
    cache.bind(allocator, owner);

    try std.testing.expectEqual(RefreshResult.updated, cache.refresh(allocator, &reader));
    try std.testing.expectEqual(@as(usize, 1), cache.view().shared_rasters.len);
    try std.testing.expectEqual(@as(u32, 1), reader.shared_acquires);
    try std.testing.expectEqual(@as(u32, 0), reader.shared_releases);

    descriptor.raster_generation = 42;
    @memcpy(resources[0..@sizeOf(r4os.abi.GuiSharedRasterResource)], std.mem.asBytes(&descriptor));
    @memcpy(resources[@sizeOf(r4os.abi.GuiSharedRasterResource)..], std.mem.asBytes(&descriptor));
    reader.info = fakeInfo(owner, 72, commands.len, resources.len);
    reader.shared_map.frame_generation = 72;
    reader.shared_map.lease.raster_generation = descriptor.raster_generation;
    reader.shared_acquire_result = r4os.abi.gui_frame_error_state;
    try std.testing.expectEqual(RefreshResult.retry, cache.refresh(allocator, &reader));
    try std.testing.expectEqual(@as(u64, 71), cache.view().info.committed_generation);
    try std.testing.expectEqual(@as(u32, 0), reader.shared_releases);

    reader.shared_acquire_result = r4os.abi.gui_frame_result_ok;
    try std.testing.expectEqual(RefreshResult.updated, cache.refresh(allocator, &reader));
    try std.testing.expectEqual(@as(u64, 72), cache.view().info.committed_generation);
    try std.testing.expectEqual(@as(u32, 2), reader.shared_acquires);
    try std.testing.expectEqual(@as(u32, 1), reader.shared_releases);
    cache.releaseSharedRasters(&reader);
    try std.testing.expectEqual(@as(u32, 2), reader.shared_releases);
}

test "snapshot cache accepts a committed empty frame and rejects handle reuse" {
    const allocator = std.testing.allocator;
    const owner = r4os.abi.ProgramProcessHandle{ .instance_id = 9, .generation = 31 };
    var reader = FakeReader{
        .info = fakeInfo(owner, 4, 0, 0),
        .commands = &.{},
        .resources = &.{},
    };
    var cache = Cache{};
    defer cache.deinit(allocator);
    cache.bind(allocator, owner);
    try std.testing.expectEqual(RefreshResult.updated, cache.refresh(allocator, &reader));
    try std.testing.expect(cache.view().valid);
    try std.testing.expectEqual(@as(usize, 0), cache.view().commands.len);

    reader.info.version = r4os.abi.gui_frame_info_version + 1;
    try std.testing.expectEqual(RefreshResult.unchanged, cache.refresh(allocator, &reader));
    try std.testing.expectEqual(r4os.abi.gui_frame_info_version + 1, cache.view().info.version);

    reader.info.owner.generation += 1;
    try std.testing.expectEqual(RefreshResult.retry, cache.refresh(allocator, &reader));
    try std.testing.expectEqual(@as(u64, 4), cache.view().info.committed_generation);
}

test "snapshot allocation failure retains the old frame" {
    var backing: [4096]u8 = undefined;
    var fixed = std.heap.FixedBufferAllocator.init(backing[0..]);
    const allocator = fixed.allocator();
    const owner = r4os.abi.ProgramProcessHandle{ .instance_id = 12, .generation = 8 };
    const first = [_]r4os.abi.GuiFrameCommand{.{ .kind = r4os.abi.gui_frame_command_kind_clear, .rgb = 0xABCDEF }};
    var reader = FakeReader{
        .info = fakeInfo(owner, 1, first.len, 0),
        .commands = first[0..],
        .resources = &.{},
    };
    var cache = Cache{};
    defer cache.deinit(allocator);
    cache.bind(allocator, owner);
    try std.testing.expectEqual(RefreshResult.updated, cache.refresh(allocator, &reader));

    reader.info = fakeInfo(owner, 2, 1024, 64 * 1024);
    reader.commands = &.{};
    reader.resources = &.{};
    try std.testing.expectEqual(RefreshResult.retry, cache.refresh(allocator, &reader));
    try std.testing.expect(cache.out_of_memory);
    try std.testing.expectEqual(@as(u64, 1), cache.view().info.committed_generation);
    try std.testing.expectEqual(@as(u32, 0xABCDEF), cache.view().commands[0].rgb);
}

test "snapshot cache has no legacy command-count ceiling" {
    const allocator = std.testing.allocator;
    const owner = r4os.abi.ProgramProcessHandle{ .instance_id = 15, .generation = 3 };
    const commands = try allocator.alloc(r4os.abi.GuiFrameCommand, 2048);
    defer allocator.free(commands);
    for (commands, 0..) |*command, index| {
        command.* = .{
            .kind = r4os.abi.gui_frame_command_kind_rect,
            .x = @intCast(index),
            .w = 1,
            .h = 1,
            .rgb = @intCast(index),
        };
    }
    var reader = FakeReader{
        .info = fakeInfo(owner, 77, commands.len, 0),
        .commands = commands,
        .resources = &.{},
    };
    var cache = Cache{};
    defer cache.deinit(allocator);
    cache.bind(allocator, owner);
    try std.testing.expectEqual(RefreshResult.updated, cache.refresh(allocator, &reader));
    try std.testing.expectEqual(@as(usize, 2048), cache.view().commands.len);
    try std.testing.expectEqual(@as(u32, 2047), cache.view().commands[2047].rgb);
}

test "snapshot cache appends one delta generation and preserves regional damage" {
    const allocator = std.testing.allocator;
    const owner = r4os.abi.ProgramProcessHandle{ .instance_id = 21, .generation = 5 };
    const base_commands = [_]r4os.abi.GuiFrameCommand{.{
        .kind = r4os.abi.gui_frame_command_kind_text,
        .resource_bytes = 1,
    }};
    const delta_commands = [_]r4os.abi.GuiFrameCommand{.{
        .kind = r4os.abi.gui_frame_command_kind_text,
        .resource_bytes = 2,
    }};
    const damage = [_]r4os.abi.DisplayDamageRect{
        .{ .x = 1, .y = 2, .w = 3, .h = 4 },
        .{ .x = 40, .y = 50, .w = 2, .h = 2 },
    };
    var reader = FakeReader{
        .info = fakeInfo(owner, 1, 1, 1),
        .commands = base_commands[0..],
        .resources = "A",
    };
    var cache = Cache{};
    defer cache.deinit(allocator);
    cache.bind(allocator, owner);
    try std.testing.expectEqual(RefreshResult.updated, cache.refresh(allocator, &reader));

    reader.info = fakeInfo(owner, 2, 2, 3);
    reader.commands = &.{};
    reader.resources = &.{};
    reader.generation_supported = true;
    reader.generation_info = fakeGenerationInfo(owner, 2, 1, 1, 2, 2, 3, damage.len);
    reader.generation_commands = delta_commands[0..];
    reader.generation_resources = "BC";
    reader.generation_regions = damage[0..];
    try std.testing.expectEqual(RefreshResult.updated, cache.refresh(allocator, &reader));

    const view = cache.view();
    try std.testing.expect(!view.full_damage);
    try std.testing.expectEqual(@as(usize, 2), view.commands.len);
    try std.testing.expectEqual(@as(u64, 1), view.commands[1].resource_offset);
    try std.testing.expectEqualStrings("ABC", view.resources);
    try std.testing.expectEqualSlices(r4os.abi.DisplayDamageRect, damage[0..], view.damage_regions);
    try std.testing.expectEqual(@as(u64, 1), cache.delta_refreshes);
    try std.testing.expectEqual(@as(u64, 1), cache.appended_commands);
    try std.testing.expectEqual(@as(u64, 2), cache.appended_resource_bytes);
}

test "snapshot cache swaps a standalone replacement without replaying history" {
    const allocator = std.testing.allocator;
    const owner = r4os.abi.ProgramProcessHandle{ .instance_id = 22, .generation = 6 };
    const old_commands = [_]r4os.abi.GuiFrameCommand{
        .{ .kind = r4os.abi.gui_frame_command_kind_clear, .rgb = 0x101010 },
        .{ .kind = r4os.abi.gui_frame_command_kind_text, .resource_bytes = 3 },
    };
    const replacement_commands = [_]r4os.abi.GuiFrameCommand{.{
        .kind = r4os.abi.gui_frame_command_kind_text,
        .resource_bytes = 2,
    }};
    const damage = [_]r4os.abi.DisplayDamageRect{.{ .x = 4, .y = 5, .w = 6, .h = 7 }};
    var reader = FakeReader{
        .info = fakeInfo(owner, 1, old_commands.len, 3),
        .commands = old_commands[0..],
        .resources = "old",
    };
    var cache = Cache{};
    defer cache.deinit(allocator);
    cache.bind(allocator, owner);
    try std.testing.expectEqual(RefreshResult.updated, cache.refresh(allocator, &reader));

    reader.info = fakeInfo(owner, 64, replacement_commands.len, 2);
    reader.commands = replacement_commands[0..];
    reader.resources = "ok";
    reader.generation_supported = true;
    reader.generation_info = fakeGenerationInfo(owner, 64, 0, 1, 2, 1, 2, damage.len);
    reader.generation_info.flags |= r4os.abi.gui_frame_generation_flag_replacement;
    reader.generation_info.chain_depth = 1;
    reader.generation_commands = replacement_commands[0..];
    reader.generation_resources = "ok";
    reader.generation_regions = damage[0..];
    try std.testing.expectEqual(RefreshResult.updated, cache.refresh(allocator, &reader));

    const view = cache.view();
    try std.testing.expect(!view.full_damage);
    try std.testing.expectEqual(@as(u64, 64), view.info.committed_generation);
    try std.testing.expectEqual(@as(usize, 1), view.commands.len);
    try std.testing.expectEqualStrings("ok", view.resources);
    try std.testing.expectEqualSlices(r4os.abi.DisplayDamageRect, damage[0..], view.damage_regions);
    try std.testing.expectEqual(@as(u64, 1), cache.replacement_refreshes);
    try std.testing.expectEqual(@as(u64, 0), cache.delta_refreshes);
}

comptime {
    if (command_alignment != 8) @compileError("GuiFrameCommand alignment drift");
}
