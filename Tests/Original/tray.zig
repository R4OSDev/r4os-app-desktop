const std = @import("std");
const r4os = @import("r4os");
const surface = @import("surface.zig");

pub const max_items: usize = r4os.abi.tray_max_items;
pub const max_owners: usize = r4os.abi.tray_max_owners;
pub const event_capacity: usize = r4os.abi.tray_event_queue_capacity;
pub const slot_width: i32 = 24;
pub const system_gap: i32 = 4;
pub const window_gap: i32 = 6;
pub const preferred_window_width: i32 = 150;
pub const minimum_window_width: i32 = 40;
pub const tooltip_height: i32 = 22;
pub const tooltip_gap: i32 = 4;

const valid_item_flags = r4os.abi.tray_item_flag_visible |
    r4os.abi.tray_item_flag_enabled |
    r4os.abi.tray_item_flag_attention;

pub const Identity = struct {
    owner: r4os.abi.ProgramProcessHandle = .{},
    item_id: u64 = 0,
    serial: u64 = 0,

    pub fn valid(self: Identity) bool {
        return ownerValid(self.owner) and self.item_id != 0 and self.serial != 0;
    }

    pub fn eql(self: Identity, other: Identity) bool {
        return sameOwner(self.owner, other.owner) and self.item_id == other.item_id and self.serial == other.serial;
    }
};

pub const Entry = struct {
    used: bool = false,
    owner: r4os.abi.ProgramProcessHandle = .{},
    item_id: u64 = 0,
    revision: u64 = 0,
    flags: u32 = 0,
    status_flags: u32 = 0,
    tooltip_len: u16 = 0,
    tooltip: [r4os.abi.tray_tooltip_bytes]u8 = .{0} ** r4os.abi.tray_tooltip_bytes,
    icon: [r4os.abi.tray_icon_pixel_count]u32 = .{0} ** r4os.abi.tray_icon_pixel_count,
    order: u64 = 0,
    serial: u64 = 0,
    layout_visible: bool = false,
    rect: surface.Rect = .{ .x = 0, .y = 0, .w = 0, .h = 0 },

    pub fn identity(self: *const Entry) Identity {
        if (!self.used) return .{};
        return .{ .owner = self.owner, .item_id = self.item_id, .serial = self.serial };
    }

    pub fn providerVisible(self: *const Entry) bool {
        return self.used and (self.flags & r4os.abi.tray_item_flag_visible) != 0;
    }

    pub fn enabled(self: *const Entry) bool {
        return self.used and (self.flags & r4os.abi.tray_item_flag_enabled) != 0;
    }

    pub fn tooltipSlice(self: *const Entry) []const u8 {
        return self.tooltip[0..@min(@as(usize, self.tooltip_len), self.tooltip.len)];
    }
};

pub const LayoutSpec = struct {
    screen_w: i32,
    screen_h: i32,
    taskbar_h: i32,
    item_y: i32,
    system_left: i32,
    window_start: i32,
    visible_windows: usize,
};

pub const Mutation = struct {
    result: i32,
    changed: bool = false,
    cancelled_waiter: ?Waiter = null,
};

pub const Waiter = struct {
    owner: r4os.abi.ProgramProcessHandle,
    request_id: u32,
    deadline_tick: u64,
    after_sequence: u64,
};

pub const WaitBegin = union(enum) {
    parked,
    ready,
    timed_out,
    busy,
    not_found,
};

pub const WaitReply = struct {
    waiter: Waiter,
    result: i32,
    event: ?r4os.abi.TrayEvent = null,
};

const OwnerState = struct {
    used: bool = false,
    owner: r4os.abi.ProgramProcessHandle = .{},
    next_sequence: u64 = 1,
    events: [event_capacity]r4os.abi.TrayEvent = .{r4os.abi.TrayEvent{}} ** event_capacity,
    head: usize = 0,
    count: usize = 0,
    dropped: u32 = 0,
    waiter: ?Waiter = null,
};

pub const Registry = struct {
    desktop_epoch: u64 = 0,
    revision: u64 = 1,
    next_order: u64 = 1,
    next_serial: u64 = 1,
    entries: [max_items]Entry = .{Entry{}} ** max_items,
    owners: [max_owners]OwnerState = .{OwnerState{}} ** max_owners,
    registered_count: usize = 0,
    layout_visible_count: usize = 0,
    window_right: i32 = 0,

    pub fn init(desktop_epoch: u64) Registry {
        return .{ .desktop_epoch = if (desktop_epoch == 0) 1 else desktop_epoch };
    }

    pub fn upsert(self: *Registry, request: *const r4os.abi.TrayServiceRequest) Mutation {
        if (!validItemRequest(request)) return .{ .result = r4os.abi.tray_result_bad_request };
        if (self.findEntryIndex(request.owner, request.item_id)) |index| {
            const current = &self.entries[index];
            if (request.item_revision < current.revision) return .{ .result = r4os.abi.tray_result_stale };
            if (request.item_revision == current.revision) {
                return if (entryMatchesRequest(current, request))
                    .{ .result = r4os.abi.tray_result_ok }
                else
                    .{ .result = r4os.abi.tray_result_stale };
            }

            var candidate = entryFromRequest(request, current.order, current.serial);
            candidate.layout_visible = current.layout_visible;
            candidate.rect = current.rect;
            self.entries[index] = candidate;
            self.bumpRevision();
            return .{ .result = r4os.abi.tray_result_ok, .changed = true };
        }

        if (self.registered_count >= max_items) return .{ .result = r4os.abi.tray_result_full };
        const entry_index = self.freeEntryIndex() orelse return .{ .result = r4os.abi.tray_result_full };
        var owner_index = self.findOwnerIndex(request.owner);
        if (owner_index == null) {
            owner_index = self.freeOwnerIndex() orelse return .{ .result = r4os.abi.tray_result_full };
        }

        const order = self.takeCounter(&self.next_order);
        const serial = self.takeCounter(&self.next_serial);
        const candidate = entryFromRequest(request, order, serial);
        if (!self.owners[owner_index.?].used) {
            self.owners[owner_index.?] = .{ .used = true, .owner = request.owner };
        }
        self.entries[entry_index] = candidate;
        self.registered_count += 1;
        self.bumpRevision();
        return .{ .result = r4os.abi.tray_result_ok, .changed = true };
    }

    pub fn remove(self: *Registry, owner: r4os.abi.ProgramProcessHandle, item_id: u64) Mutation {
        if (!ownerValid(owner) or item_id == 0) return .{ .result = r4os.abi.tray_result_bad_request };
        const index = self.findEntryIndex(owner, item_id) orelse return .{ .result = r4os.abi.tray_result_ok };
        const was_visible = self.entries[index].layout_visible;
        self.entries[index] = .{};
        self.registered_count -|= 1;
        if (was_visible) self.layout_visible_count -|= 1;
        self.purgeItemEvents(owner, item_id);
        var cancelled: ?Waiter = null;
        if (!self.ownerHasItems(owner)) {
            if (self.findOwnerIndex(owner)) |owner_index| {
                cancelled = self.owners[owner_index].waiter;
                self.owners[owner_index] = .{};
            }
        }
        self.bumpRevision();
        return .{ .result = r4os.abi.tray_result_ok, .changed = true, .cancelled_waiter = cancelled };
    }

    pub fn removeOwner(self: *Registry, owner: r4os.abi.ProgramProcessHandle) Mutation {
        if (!ownerValid(owner)) return .{ .result = r4os.abi.tray_result_bad_request };
        var changed = false;
        var index: usize = 0;
        while (index < self.entries.len) : (index += 1) {
            if (!self.entries[index].used or !sameOwner(self.entries[index].owner, owner)) continue;
            if (self.entries[index].layout_visible) self.layout_visible_count -|= 1;
            self.entries[index] = .{};
            self.registered_count -|= 1;
            changed = true;
        }
        var cancelled: ?Waiter = null;
        if (self.findOwnerIndex(owner)) |owner_index| {
            cancelled = self.owners[owner_index].waiter;
            self.owners[owner_index] = .{};
            changed = true;
        }
        if (changed) self.bumpRevision();
        return .{ .result = r4os.abi.tray_result_ok, .changed = changed, .cancelled_waiter = cancelled };
    }

    pub fn statusResult(self: *const Registry, owner: r4os.abi.ProgramProcessHandle, item_id: u64) i32 {
        if (!ownerValid(owner)) return r4os.abi.tray_result_bad_request;
        if (item_id == 0) return r4os.abi.tray_result_ok;
        return if (self.findEntryIndex(owner, item_id) != null) r4os.abi.tray_result_ok else r4os.abi.tray_result_not_found;
    }

    pub fn response(self: *const Registry, owner: r4os.abi.ProgramProcessHandle, item_id: u64, result: i32, extra_flags: u32) r4os.abi.TrayServiceResponse {
        var response_value: r4os.abi.TrayServiceResponse = .{
            .result = result,
            .flags = extra_flags,
            .desktop_epoch = self.desktop_epoch,
            .registry_revision = self.revision,
            .owner = owner,
            .item_id = item_id,
            .registered_count = @intCast(self.registered_count),
            .visible_count = @intCast(self.layout_visible_count),
            .capacity = @intCast(max_items),
        };
        if (self.findOwnerIndex(owner)) |owner_index| {
            response_value.queued_events = @intCast(self.owners[owner_index].count);
            response_value.dropped_events = self.owners[owner_index].dropped;
        }
        if (item_id != 0) {
            if (self.findEntryIndex(owner, item_id)) |index| {
                const entry = &self.entries[index];
                response_value.flags |= r4os.abi.tray_response_flag_exists;
                if (entry.layout_visible) response_value.flags |= r4os.abi.tray_response_flag_layout_visible;
                response_value.item_revision = entry.revision;
                response_value.item_flags = entry.flags;
                response_value.status_flags = entry.status_flags;
            }
        }
        return response_value;
    }

    pub fn beginWait(self: *Registry, owner: r4os.abi.ProgramProcessHandle, request_id: u32, after_sequence: u64, deadline_tick: u64, now: u64) WaitBegin {
        const owner_index = self.findOwnerIndex(owner) orelse return .not_found;
        const state = &self.owners[owner_index];
        self.discardConsumedEvents(state, after_sequence);
        if (state.count != 0) return .ready;
        if (deadline_tick <= now) return .timed_out;
        if (state.waiter != null) return .busy;
        state.waiter = .{
            .owner = owner,
            .request_id = request_id,
            .deadline_tick = deadline_tick,
            .after_sequence = after_sequence,
        };
        return .parked;
    }

    pub fn immediateWaitReply(self: *Registry, owner: r4os.abi.ProgramProcessHandle, request_id: u32, after_sequence: u64) ?WaitReply {
        const owner_index = self.findOwnerIndex(owner) orelse return null;
        const state = &self.owners[owner_index];
        self.discardConsumedEvents(state, after_sequence);
        const event = self.popEvent(state) orelse return null;
        return .{
            .waiter = .{ .owner = owner, .request_id = request_id, .deadline_tick = 0, .after_sequence = after_sequence },
            .result = r4os.abi.tray_result_ok,
            .event = event,
        };
    }

    pub fn takeWaitReply(self: *Registry, now: u64) ?WaitReply {
        var index: usize = 0;
        while (index < self.owners.len) : (index += 1) {
            const state = &self.owners[index];
            const waiter = state.waiter orelse continue;
            self.discardConsumedEvents(state, waiter.after_sequence);
            if (self.popEvent(state)) |event| {
                state.waiter = null;
                return .{ .waiter = waiter, .result = r4os.abi.tray_result_ok, .event = event };
            }
            if (waiter.deadline_tick <= now) {
                state.waiter = null;
                return .{ .waiter = waiter, .result = r4os.abi.tray_result_timeout };
            }
        }
        return null;
    }

    pub fn restoreEvent(self: *Registry, owner: r4os.abi.ProgramProcessHandle, event: r4os.abi.TrayEvent) void {
        const owner_index = self.findOwnerIndex(owner) orelse return;
        const state = &self.owners[owner_index];
        if (state.count >= event_capacity) {
            state.head = (state.head + 1) % event_capacity;
            state.count -= 1;
            state.dropped +|= 1;
        }
        state.head = (state.head + event_capacity - 1) % event_capacity;
        state.events[state.head] = event;
        state.count += 1;
    }

    pub fn enqueueActivation(self: *Registry, identity: Identity, kind: u16, wheel_delta: i32, x: i32, y: i32, tick: u64) bool {
        const entry = self.findByIdentity(identity) orelse return false;
        if (!entry.enabled() or !entry.layout_visible or !validEventKind(kind)) return false;
        const owner_index = self.findOwnerIndex(entry.owner) orelse return false;
        const state = &self.owners[owner_index];
        var sequence = state.next_sequence;
        state.next_sequence +%= 1;
        if (state.next_sequence == 0) state.next_sequence = 1;
        if (sequence == 0) sequence = 1;
        const event: r4os.abi.TrayEvent = .{
            .sequence = sequence,
            .owner = entry.owner,
            .item_id = entry.item_id,
            .item_revision = entry.revision,
            .kind = kind,
            .wheel_delta = wheel_delta,
            .x = x,
            .y = y,
            .tick = tick,
        };
        if (state.count == event_capacity) {
            state.head = (state.head + 1) % event_capacity;
            state.count -= 1;
            state.dropped +|= 1;
        }
        const tail = (state.head + state.count) % event_capacity;
        state.events[tail] = event;
        state.count += 1;
        return true;
    }

    pub fn layout(self: *Registry, spec: LayoutSpec) bool {
        const external_right = @max(0, spec.system_left - system_gap);
        const window_reserve = windowReservation(spec.visible_windows);
        const external_left_limit = @min(external_right, @max(0, spec.window_start + window_reserve + window_gap));
        const capacity: usize = if (external_right <= external_left_limit)
            0
        else
            @intCast(@divTrunc(external_right - external_left_limit, slot_width));
        const visible_capacity = @min(capacity, self.providerVisibleCount());
        const external_left = external_right - @as(i32, @intCast(visible_capacity)) * slot_width;
        const next_window_right = @max(spec.window_start, external_left - window_gap);

        var changed = next_window_right != self.window_right;
        self.window_right = next_window_right;
        var next_visible_count: usize = 0;
        for (&self.entries) |*entry| {
            if (!entry.used) continue;
            const rank = if (entry.providerVisible()) self.visibleRank(entry.order) else max_items;
            const visible = entry.providerVisible() and rank < visible_capacity;
            const next_rect = if (visible) surface.Rect{
                .x = external_right - @as(i32, @intCast(rank + 1)) * slot_width,
                .y = spec.screen_h - spec.taskbar_h + spec.item_y,
                .w = slot_width,
                .h = slot_width,
            } else surface.Rect{ .x = 0, .y = 0, .w = 0, .h = 0 };
            if (entry.layout_visible != visible or !rectEql(entry.rect, next_rect)) changed = true;
            entry.layout_visible = visible;
            entry.rect = next_rect;
            if (visible) next_visible_count += 1;
        }
        if (next_visible_count != self.layout_visible_count) changed = true;
        self.layout_visible_count = next_visible_count;
        if (changed) self.bumpRevision();
        return changed;
    }

    pub fn hit(self: *const Registry, x: i32, y: i32) ?Identity {
        for (&self.entries) |*entry| {
            if (entry.used and entry.layout_visible and entry.rect.contains(x, y)) return entry.identity();
        }
        return null;
    }

    pub fn findByIdentity(self: *const Registry, identity: Identity) ?*const Entry {
        if (!identity.valid()) return null;
        const index = self.findEntryIndex(identity.owner, identity.item_id) orelse return null;
        const candidate = &self.entries[index];
        return if (candidate.serial == identity.serial) candidate else null;
    }

    pub fn entryAt(self: *const Registry, index: usize) ?*const Entry {
        if (index >= self.entries.len or !self.entries[index].used) return null;
        return &self.entries[index];
    }

    pub fn ownerAt(self: *const Registry, start: usize) ?struct { index: usize, owner: r4os.abi.ProgramProcessHandle } {
        var index = start;
        while (index < self.owners.len) : (index += 1) {
            if (self.owners[index].used) return .{ .index = index, .owner = self.owners[index].owner };
        }
        return null;
    }

    pub fn tooltipRect(self: *const Registry, identity: Identity, screen_w: i32, screen_h: i32, taskbar_h: i32) ?surface.Rect {
        const item = self.findByIdentity(identity) orelse return null;
        if (!item.layout_visible or item.tooltip_len == 0) return null;
        const desired_w = @min(@as(i32, @intCast(item.tooltipSlice().len)) * 8 + 12, @max(0, screen_w - 8));
        if (desired_w <= 0) return null;
        const x = clamp(item.rect.x + @divTrunc(item.rect.w - desired_w, 2), 4, @max(4, screen_w - desired_w - 4));
        return .{
            .x = x,
            .y = @max(0, screen_h - taskbar_h - tooltip_gap - tooltip_height),
            .w = desired_w,
            .h = tooltip_height,
        };
    }

    fn providerVisibleCount(self: *const Registry) usize {
        var count: usize = 0;
        for (self.entries) |entry| if (entry.providerVisible()) {
            count += 1;
        };
        return count;
    }

    fn visibleRank(self: *const Registry, order: u64) usize {
        var rank: usize = 0;
        for (self.entries) |entry| {
            if (entry.providerVisible() and entry.order < order) rank += 1;
        }
        return rank;
    }

    fn freeEntryIndex(self: *const Registry) ?usize {
        for (self.entries, 0..) |entry, index| if (!entry.used) return index;
        return null;
    }

    fn findEntryIndex(self: *const Registry, owner: r4os.abi.ProgramProcessHandle, item_id: u64) ?usize {
        for (self.entries, 0..) |entry, index| {
            if (entry.used and entry.item_id == item_id and sameOwner(entry.owner, owner)) return index;
        }
        return null;
    }

    fn ownerHasItems(self: *const Registry, owner: r4os.abi.ProgramProcessHandle) bool {
        for (self.entries) |entry| if (entry.used and sameOwner(entry.owner, owner)) return true;
        return false;
    }

    fn freeOwnerIndex(self: *const Registry) ?usize {
        for (self.owners, 0..) |owner, index| if (!owner.used) return index;
        return null;
    }

    fn findOwnerIndex(self: *const Registry, owner: r4os.abi.ProgramProcessHandle) ?usize {
        for (self.owners, 0..) |state, index| {
            if (state.used and sameOwner(state.owner, owner)) return index;
        }
        return null;
    }

    fn purgeItemEvents(self: *Registry, owner: r4os.abi.ProgramProcessHandle, item_id: u64) void {
        const owner_index = self.findOwnerIndex(owner) orelse return;
        const state = &self.owners[owner_index];
        var kept: [event_capacity]r4os.abi.TrayEvent = .{r4os.abi.TrayEvent{}} ** event_capacity;
        var kept_count: usize = 0;
        var offset: usize = 0;
        while (offset < state.count) : (offset += 1) {
            const event = state.events[(state.head + offset) % event_capacity];
            if (event.item_id == item_id) continue;
            kept[kept_count] = event;
            kept_count += 1;
        }
        state.events = kept;
        state.head = 0;
        state.count = kept_count;
    }

    fn discardConsumedEvents(self: *Registry, state: *OwnerState, after_sequence: u64) void {
        _ = self;
        while (state.count != 0) {
            const event = state.events[state.head];
            if (event.sequence > after_sequence) break;
            state.head = (state.head + 1) % event_capacity;
            state.count -= 1;
        }
    }

    fn popEvent(self: *Registry, state: *OwnerState) ?r4os.abi.TrayEvent {
        _ = self;
        if (state.count == 0) return null;
        var event = state.events[state.head];
        state.head = (state.head + 1) % event_capacity;
        state.count -= 1;
        if (state.dropped != 0) {
            event.flags |= r4os.abi.tray_event_flag_overflow;
            event.dropped_before = state.dropped;
            state.dropped = 0;
        }
        return event;
    }

    fn takeCounter(self: *Registry, counter: *u64) u64 {
        _ = self;
        var value = counter.*;
        counter.* +%= 1;
        if (counter.* == 0) counter.* = 1;
        if (value == 0) value = 1;
        return value;
    }

    fn bumpRevision(self: *Registry) void {
        self.revision +%= 1;
        if (self.revision == 0) self.revision = 1;
    }
};

pub fn windowButtonWidth(window_start: i32, window_right: i32, visible_count: usize) i32 {
    if (visible_count == 0 or window_right <= window_start) return 0;
    const gaps = @as(i32, @intCast(visible_count - 1)) * window_gap;
    const available = window_right - window_start - gaps;
    if (available <= 0) return 0;
    return @min(preferred_window_width, @max(1, @divTrunc(available, @as(i32, @intCast(visible_count)))));
}

pub fn windowButtonRect(window_start: i32, window_right: i32, visible_count: usize, ordinal: usize, y: i32, height: i32) surface.Rect {
    const width = windowButtonWidth(window_start, window_right, visible_count);
    if (width <= 0 or ordinal >= visible_count) return .{ .x = 0, .y = 0, .w = 0, .h = 0 };
    return .{
        .x = window_start + @as(i32, @intCast(ordinal)) * (width + window_gap),
        .y = y,
        .w = width,
        .h = height,
    };
}

fn windowReservation(visible_count: usize) i32 {
    if (visible_count == 0) return 0;
    return @as(i32, @intCast(visible_count)) * minimum_window_width +
        @as(i32, @intCast(visible_count - 1)) * window_gap;
}

fn entryFromRequest(request: *const r4os.abi.TrayServiceRequest, order: u64, serial: u64) Entry {
    return .{
        .used = true,
        .owner = request.owner,
        .item_id = request.item_id,
        .revision = request.item_revision,
        .flags = request.item_flags,
        .status_flags = request.status_flags,
        .tooltip_len = request.tooltip_length,
        .tooltip = request.tooltip,
        .icon = request.icon,
        .order = order,
        .serial = serial,
    };
}

fn validItemRequest(request: *const r4os.abi.TrayServiceRequest) bool {
    if (!validBaseRequest(request) or request.item_id == 0 or request.item_revision == 0 or
        request.tooltip_length > r4os.abi.tray_tooltip_bytes or
        request.icon_width != r4os.abi.tray_icon_width or
        request.icon_height != r4os.abi.tray_icon_height or
        request.icon_format != r4os.abi.tray_icon_format_argb32 or
        (request.item_flags & ~valid_item_flags) != 0)
    {
        return false;
    }
    return std.unicode.utf8ValidateSlice(request.tooltip[0..request.tooltip_length]) and
        allZero(request.tooltip[request.tooltip_length..]);
}

pub fn validBaseRequest(request: *const r4os.abi.TrayServiceRequest) bool {
    return request.magic == r4os.abi.tray_service_request_magic and
        request.version == r4os.abi.tray_service_request_version and
        request.size == @sizeOf(r4os.abi.TrayServiceRequest) and
        ownerValid(request.owner) and
        allZero(request.reserved0[0..]);
}

fn entryMatchesRequest(entry: *const Entry, request: *const r4os.abi.TrayServiceRequest) bool {
    return entry.flags == request.item_flags and
        entry.status_flags == request.status_flags and
        entry.tooltip_len == request.tooltip_length and
        std.mem.eql(u8, entry.tooltip[0..], request.tooltip[0..]) and
        std.mem.eql(u8, std.mem.sliceAsBytes(entry.icon[0..]), std.mem.sliceAsBytes(request.icon[0..]));
}

fn validEventKind(kind: u16) bool {
    return kind == r4os.abi.tray_event_kind_primary or
        kind == r4os.abi.tray_event_kind_double or
        kind == r4os.abi.tray_event_kind_context or
        kind == r4os.abi.tray_event_kind_wheel;
}

pub fn ownerValid(owner: r4os.abi.ProgramProcessHandle) bool {
    return owner.instance_id != 0 and owner.reserved == 0 and owner.generation != 0;
}

pub fn sameOwner(left: r4os.abi.ProgramProcessHandle, right: r4os.abi.ProgramProcessHandle) bool {
    return left.instance_id == right.instance_id and left.reserved == right.reserved and left.generation == right.generation;
}

fn allZero(bytes: []const u8) bool {
    for (bytes) |byte| if (byte != 0) return false;
    return true;
}

fn rectEql(left: surface.Rect, right: surface.Rect) bool {
    return left.x == right.x and left.y == right.y and left.w == right.w and left.h == right.h;
}

fn clamp(value: i32, min_value: i32, max_value: i32) i32 {
    return @max(min_value, @min(max_value, value));
}

fn makeRequest(owner: r4os.abi.ProgramProcessHandle, id: u64, revision: u64, color: u32) r4os.abi.TrayServiceRequest {
    var value: r4os.abi.TrayServiceRequest = .{
        .owner = owner,
        .item_id = id,
        .item_revision = revision,
        .item_flags = r4os.abi.tray_item_flag_visible | r4os.abi.tray_item_flag_enabled,
        .tooltip_length = 4,
        .icon_width = 16,
        .icon_height = 16,
        .icon_format = r4os.abi.tray_icon_format_argb32,
    };
    @memcpy(value.tooltip[0..4], "test");
    value.icon[0] = color;
    return value;
}

test "registry upsert is atomic idempotent generation-bound and bounded" {
    const owner = r4os.abi.ProgramProcessHandle{ .instance_id = 4, .generation = 10 };
    var registry = Registry.init(55);
    var first = makeRequest(owner, 7, 1, 0xff11_2233);
    try std.testing.expect(registry.upsert(&first).changed);
    try std.testing.expect(!registry.upsert(&first).changed);
    var conflicting = first;
    conflicting.icon[0] = 0xff44_5566;
    try std.testing.expectEqual(r4os.abi.tray_result_stale, registry.upsert(&conflicting).result);
    conflicting.item_revision = 2;
    try std.testing.expect(registry.upsert(&conflicting).changed);
    try std.testing.expectEqual(@as(usize, 1), registry.registered_count);
    try std.testing.expect(registry.findEntryIndex(owner, 7) != null);
    try std.testing.expect(registry.findEntryIndex(.{ .instance_id = 4, .generation = 11 }, 7) == null);

    var id: u64 = 8;
    while (id <= max_items + 6) : (id += 1) {
        var next = makeRequest(owner, id, 1, 0xff00_0000 | @as(u32, @intCast(id)));
        try std.testing.expect(registry.upsert(&next).changed);
    }
    var overflow = makeRequest(owner, 99, 1, 0xffff_ffff);
    try std.testing.expectEqual(r4os.abi.tray_result_full, registry.upsert(&overflow).result);
    try std.testing.expectEqual(@as(usize, max_items), registry.registered_count);
}

test "layout anchors oldest external item beside system entries and preserves windows" {
    const owner = r4os.abi.ProgramProcessHandle{ .instance_id = 2, .generation = 8 };
    var registry = Registry.init(8);
    var id: u64 = 1;
    while (id <= 6) : (id += 1) {
        var next = makeRequest(owner, id, 1, 0xff00_0000);
        _ = registry.upsert(&next);
    }
    try std.testing.expect(registry.layout(.{
        .screen_w = 640,
        .screen_h = 480,
        .taskbar_h = 32,
        .item_y = 4,
        .system_left = 500,
        .window_start = 180,
        .visible_windows = 4,
    }));
    const first = registry.entries[registry.findEntryIndex(owner, 1).?];
    const second = registry.entries[registry.findEntryIndex(owner, 2).?];
    try std.testing.expect(first.layout_visible);
    try std.testing.expectEqual(@as(i32, 472), first.rect.x);
    try std.testing.expectEqual(@as(i32, 448), second.rect.x);
    try std.testing.expect(registry.window_right <= second.rect.x);
    try std.testing.expect(windowButtonWidth(180, registry.window_right, 4) >= minimum_window_width);
}

test "removed and re-registered item gets a new click identity" {
    const owner = r4os.abi.ProgramProcessHandle{ .instance_id = 9, .generation = 30 };
    var registry = Registry.init(30);
    var item = makeRequest(owner, 1, 1, 0xffff_ffff);
    _ = registry.upsert(&item);
    const old_identity = registry.entries[registry.findEntryIndex(owner, 1).?].identity();
    _ = registry.remove(owner, 1);
    item.item_revision = 2;
    _ = registry.upsert(&item);
    const new_identity = registry.entries[registry.findEntryIndex(owner, 1).?].identity();
    try std.testing.expect(!old_identity.eql(new_identity));
    try std.testing.expect(registry.findByIdentity(old_identity) == null);
    try std.testing.expect(registry.findByIdentity(new_identity) != null);
}

test "event queue is sequenced bounded and reports overflow to one waiter" {
    const owner = r4os.abi.ProgramProcessHandle{ .instance_id = 5, .generation = 70 };
    var registry = Registry.init(70);
    var item = makeRequest(owner, 1, 1, 0xffff_ffff);
    _ = registry.upsert(&item);
    _ = registry.layout(.{
        .screen_w = 800,
        .screen_h = 600,
        .taskbar_h = 32,
        .item_y = 4,
        .system_left = 700,
        .window_start = 200,
        .visible_windows = 1,
    });
    const identity = registry.entries[registry.findEntryIndex(owner, 1).?].identity();
    var count: usize = 0;
    while (count < event_capacity + 2) : (count += 1) {
        try std.testing.expect(registry.enqueueActivation(identity, r4os.abi.tray_event_kind_primary, 0, 1, 2, @intCast(count + 1)));
    }
    try std.testing.expectEqual(WaitBegin.ready, registry.beginWait(owner, 3, 0, 100, 1));
    const reply = registry.immediateWaitReply(owner, 3, 0).?;
    try std.testing.expect(reply.event.?.sequence > 1);
    try std.testing.expect((reply.event.?.flags & r4os.abi.tray_event_flag_overflow) != 0);
    try std.testing.expectEqual(@as(u32, 2), reply.event.?.dropped_before);
    try std.testing.expectEqual(WaitBegin.parked, registry.beginWait(owner, 4, 100, 200, 1));
    try std.testing.expectEqual(WaitBegin.busy, registry.beginWait(owner, 5, 100, 200, 1));
}
