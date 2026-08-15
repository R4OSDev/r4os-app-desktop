const std = @import("std");
const model = @import("model.zig");
const surface = @import("surface.zig");
const theme = @import("theme.zig");

pub const max_entries: usize = 10;
pub const max_submenus: usize = 3;
pub const max_submenu_entries: usize = 8;
pub const submenu_w: i32 = 196;
const max_title: usize = 31;
const max_policy: usize = 10;
const max_submenu_id: usize = 15;
const max_path: usize = 63;
const max_args: usize = 127;

pub const EntryKind = enum(u8) {
    item,
    action,
    submenu,
};

pub const SubmenuId = enum(u8) {
    none,
    programs,
    internet,
    settings,
};

pub const LaunchPolicy = enum(u8) {
    console,
    gui,
    auto,
    action,

    pub fn label(self: LaunchPolicy) []const u8 {
        return switch (self) {
            .console => "console",
            .gui => "gui",
            .auto => "auto",
            .action => "action",
        };
    }
};

pub const Group = enum(u8) {
    programs,
    system,
    power,
};

pub const Launch = struct {
    target: model.UiTarget,
    kind: EntryKind,
    policy: LaunchPolicy,
    title: [*:0]const u8,
    path: [*:0]const u8,
    args: [*:0]const u8,
};

pub const Entry = struct {
    target: model.UiTarget = .none,
    kind: EntryKind = .item,
    launch_policy: LaunchPolicy = .auto,
    submenu_id: SubmenuId = .none,
    title: [max_title + 1]u8 = .{0} ** (max_title + 1),
    policy: [max_policy + 1]u8 = .{0} ** (max_policy + 1),
    path: [max_path + 1]u8 = .{0} ** (max_path + 1),
    args: [max_args + 1]u8 = .{0} ** (max_args + 1),

    pub fn label(self: *const Entry) [*:0]const u8 {
        return @ptrCast(&self.title);
    }

    pub fn policyLabel(self: *const Entry) [*:0]const u8 {
        return @ptrCast(&self.policy);
    }

    pub fn pathZ(self: *const Entry) [*:0]const u8 {
        return @ptrCast(&self.path);
    }

    pub fn argsZ(self: *const Entry) [*:0]const u8 {
        return @ptrCast(&self.args);
    }

    pub fn launch(self: *const Entry) Launch {
        return .{
            .target = self.target,
            .kind = self.kind,
            .policy = self.launch_policy,
            .title = self.label(),
            .path = self.pathZ(),
            .args = self.argsZ(),
        };
    }
};

pub const Submenu = struct {
    id: SubmenuId = .none,
    entries: [max_submenu_entries]Entry = .{Entry{}} ** max_submenu_entries,
    count: usize = 0,

    pub fn label(self: *const Submenu, index: usize) [*:0]const u8 {
        if (index >= self.count) return "";
        return self.entries[index].label();
    }

    pub fn policy(self: *const Submenu, index: usize) [*:0]const u8 {
        if (index >= self.count) return "";
        return self.entries[index].policyLabel();
    }

    pub fn target(self: *const Submenu, index: usize) model.UiTarget {
        if (index >= self.count) return .none;
        return self.entries[index].target;
    }

    pub fn launch(self: *const Submenu, index: usize) ?Launch {
        if (index >= self.count) return null;
        return self.entries[index].launch();
    }

    fn addKnown(self: *Submenu, item_target: model.UiTarget, kind: EntryKind, title: []const u8, launch_policy: LaunchPolicy, path: []const u8) void {
        self.addKnownArgs(item_target, kind, title, launch_policy, path, "");
    }

    fn addKnownArgs(self: *Submenu, item_target: model.UiTarget, kind: EntryKind, title: []const u8, launch_policy: LaunchPolicy, path: []const u8, args: []const u8) void {
        if (self.count >= self.entries.len or item_target == .none) return;
        var entry = Entry{ .target = item_target, .kind = kind, .launch_policy = launch_policy };
        copyZ(entry.title[0..], title);
        copyZ(entry.policy[0..], launch_policy.label());
        copyZ(entry.path[0..], path);
        copyZ(entry.args[0..], args);
        self.entries[self.count] = entry;
        self.count += 1;
    }

    fn addSubmenu(self: *Submenu, item_target: model.UiTarget, title: []const u8, id: SubmenuId) void {
        if (self.count >= self.entries.len or item_target == .none or id == .none) return;
        var entry = Entry{ .target = item_target, .kind = .submenu, .launch_policy = .action, .submenu_id = id };
        copyZ(entry.title[0..], title);
        self.entries[self.count] = entry;
        self.count += 1;
    }
};

pub const Menu = struct {
    entries: [max_entries]Entry = .{Entry{}} ** max_entries,
    submenus: [max_submenus]Submenu = .{Submenu{}} ** max_submenus,
    count: usize = 0,
    submenu_count: usize = 0,
    loaded_from_config: bool = false,

    pub fn initDefault() Menu {
        var menu = Menu{};
        menu.addKnown(.menu_update, .item, "R4OS Update", .gui, "/R4OS/SOFTWARE/DESKTOP/UPDATE.R4X");
        menu.addSubmenu(.menu_programs, "Programs", .programs);
        if (menu.findOrCreateSubmenu(.programs)) |programs| {
            programs.addKnown(.menu_terminal, .item, "Terminal", .console, "/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X");
            programs.addKnown(.menu_notepad, .item, "Notepad", .gui, "/R4OS/SOFTWARE/DESKTOP/NOTEPAD.R4X");
            programs.addKnown(.menu_paint, .item, "Paint", .gui, "/R4OS/SOFTWARE/DESKTOP/PAINT.R4X");
            programs.addKnown(.menu_calc, .item, "Calculator", .gui, "/R4OS/SOFTWARE/DESKTOP/CALC.R4X");
            programs.addKnownArgs(.menu_synth, .item, "R4Synth", .console, "/R4OS/SOFTWARE/TERMINAL/SYNTH.R4X", "C:\\TEMP\\TADA.WAV");
            programs.addKnown(.menu_devmgr, .item, "Device Manager", .gui, "/R4OS/SOFTWARE/DESKTOP/DEVMGR.R4X");
            programs.addKnown(.menu_r4code, .item, "R4Code", .gui, "/SOFTWARE/R4CODE/R4CODE.R4X");
            programs.addSubmenu(.menu_programs_internet, "Internet", .internet);
        }
        if (menu.findOrCreateSubmenu(.internet)) |internet| {
            internet.addKnown(.menu_klickifax, .item, "Klickifax", .gui, "/R4OS/SOFTWARE/INTERNET/KLICKIFAX.R4X");
        }
        menu.addKnown(.menu_terminal_mode, .action, "Terminal Mode", .action, "");
        menu.addKnown(.menu_run, .action, "Run...", .action, "");
        menu.addSubmenu(.menu_settings, "Settings", .settings);
        if (menu.findOrCreateSubmenu(.settings)) |settings| {
            settings.addKnown(.menu_settings_appearance, .item, "Appearance", .gui, "/R4OS/SOFTWARE/DESKTOP/APPEARANCE.R4X");
            settings.addKnown(.menu_settings_default_apps, .item, "Default Apps", .gui, "/R4OS/SOFTWARE/DESKTOP/APPDEF.R4X");
            settings.addKnown(.menu_settings_registry, .item, "Registry Editor", .gui, "/R4OS/SOFTWARE/DESKTOP/REGEDIT.R4X");
            settings.addKnown(.menu_settings_network, .item, "Network", .gui, "/R4OS/SOFTWARE/DESKTOP/NETCFG.R4X");
            settings.addKnown(.menu_settings_services, .item, "Services", .gui, "/R4OS/SOFTWARE/DESKTOP/SERVICES.R4X");
            settings.addKnown(.menu_settings_log_center, .item, "Log Center", .gui, "/R4OS/SOFTWARE/DESKTOP/LOGCENTER.R4X");
            settings.addKnown(.menu_settings_time, .item, "Time Settings", .gui, "/R4OS/SOFTWARE/DESKTOP/TIMESET.R4X");
        }
        menu.addKnown(.menu_tasks, .action, "Tasks", .action, "");
        menu.addKnown(.menu_restart, .action, "Restart", .action, "");
        menu.addKnown(.menu_poweroff, .action, "Poweroff", .action, "");
        menu.addKnown(.menu_halt, .action, "Halt", .action, "");
        return menu;
    }

    pub fn loadFromBytes(self: *Menu, bytes: []const u8) bool {
        var parsed = Menu{};
        var rest = bytes;
        while (rest.len > 0) {
            const split = findByte(rest, '\n') orelse rest.len;
            const line = trim(rest[0..split]);
            if (split < rest.len) {
                rest = rest[split + 1 ..];
            } else {
                rest = rest[split..];
            }
            if (line.len == 0 or line[0] == '#') continue;
            parsed.addLine(line);
        }

        if (parsed.count == 0) return false;
        parsed.loaded_from_config = true;
        self.* = parsed;
        return true;
    }

    pub fn label(self: *const Menu, index: usize) [*:0]const u8 {
        if (index >= self.count) return "";
        return self.entries[index].label();
    }

    pub fn policy(self: *const Menu, index: usize) [*:0]const u8 {
        if (index >= self.count) return "";
        return self.entries[index].policyLabel();
    }

    pub fn target(self: *const Menu, index: usize) model.UiTarget {
        if (index >= self.count) return .none;
        return self.entries[index].target;
    }

    pub fn launch(self: *const Menu, index: usize) ?Launch {
        if (index >= self.count) return null;
        return self.entries[index].launch();
    }

    pub fn hasSubmenu(self: *const Menu, index: usize) bool {
        if (self.submenu(index)) |child| return child.count > 0;
        return false;
    }

    pub fn submenu(self: *const Menu, index: usize) ?*const Submenu {
        if (index >= self.count) return null;
        const entry = &self.entries[index];
        if (entry.kind != .submenu) return null;
        return self.submenuById(entry.submenu_id);
    }

    pub fn submenuTarget(self: *const Menu, parent: usize, index: usize) model.UiTarget {
        if (self.submenu(parent)) |child| return child.target(index);
        return .none;
    }

    pub fn submenuLaunch(self: *const Menu, parent: usize, index: usize) ?Launch {
        if (self.submenu(parent)) |child| return child.launch(index);
        return null;
    }

    pub fn submenuHasSubmenu(self: *const Menu, parent: usize, index: usize) bool {
        const child = self.submenu(parent) orelse return false;
        if (index >= child.count) return false;
        const entry = &child.entries[index];
        if (entry.kind != .submenu) return false;
        const nested = self.submenuById(entry.submenu_id) orelse return false;
        return nested.count > 0;
    }

    pub fn nestedSubmenu(self: *const Menu, parent: usize, index: usize) ?*const Submenu {
        const child = self.submenu(parent) orelse return null;
        if (index >= child.count) return null;
        const entry = &child.entries[index];
        if (entry.kind != .submenu) return null;
        return self.submenuById(entry.submenu_id);
    }

    pub fn nestedTarget(self: *const Menu, parent: usize, index: usize, nested_index: usize) model.UiTarget {
        if (self.nestedSubmenu(parent, index)) |nested| return nested.target(nested_index);
        return .none;
    }

    pub fn nestedLaunch(self: *const Menu, parent: usize, index: usize, nested_index: usize) ?Launch {
        if (self.nestedSubmenu(parent, index)) |nested| return nested.launch(nested_index);
        return null;
    }

    pub fn submenuHit(self: *const Menu, screen_w: i32, screen_h: i32, parent: usize, x: i32, y: i32) ?usize {
        const child = self.submenu(parent) orelse return null;
        if (child.count == 0) return null;
        const rect = self.submenuRect(screen_w, screen_h, parent) orelse return null;
        if (!rect.contains(x, y)) return null;
        var index: usize = 0;
        while (index < child.count) : (index += 1) {
            const item_y = submenuItemY(rect, index);
            if (y >= item_y and y < item_y + theme.menu_item_h) return index;
        }
        return null;
    }

    pub fn submenuRect(self: *const Menu, screen_w: i32, screen_h: i32, parent: usize) ?surface.Rect {
        const child = self.submenu(parent) orelse return null;
        if (child.count == 0) return null;
        const work = surface.workArea(screen_w, screen_h, theme.taskbar_h);
        const desired = surface.Rect{
            .x = theme.menu_w - 4,
            .y = self.itemY(screen_h, parent) - 4,
            .w = submenu_w,
            .h = @as(i32, @intCast(child.count)) * theme.menu_item_h + 8,
        };
        return desired.clampInside(work);
    }

    pub fn nestedRect(self: *const Menu, screen_w: i32, screen_h: i32, parent: usize, index: usize) ?surface.Rect {
        const nested = self.nestedSubmenu(parent, index) orelse return null;
        if (nested.count == 0) return null;
        const first = self.submenuRect(screen_w, screen_h, parent) orelse return null;
        const work = surface.workArea(screen_w, screen_h, theme.taskbar_h);
        const desired = surface.Rect{
            .x = first.x + first.w - 4,
            .y = submenuItemY(first, index) - 4,
            .w = submenu_w,
            .h = @as(i32, @intCast(nested.count)) * theme.menu_item_h + 8,
        };
        return desired.clampInside(work);
    }

    pub fn nestedHit(self: *const Menu, screen_w: i32, screen_h: i32, parent: usize, index: usize, x: i32, y: i32) ?usize {
        const nested = self.nestedSubmenu(parent, index) orelse return null;
        const rect = self.nestedRect(screen_w, screen_h, parent, index) orelse return null;
        if (!rect.contains(x, y)) return null;
        var nested_index: usize = 0;
        while (nested_index < nested.count) : (nested_index += 1) {
            const item_y = submenuItemY(rect, nested_index);
            if (y >= item_y and y < item_y + theme.menu_item_h) return nested_index;
        }
        return null;
    }

    pub fn hit(self: *const Menu, screen_h: i32, x: i32, y: i32) ?usize {
        const top = screen_h - theme.taskbar_h - theme.menu_h;
        if (x < 0 or x >= theme.menu_w or y < top or y >= top + theme.menu_h) return null;
        var index: usize = 0;
        while (index < self.count) : (index += 1) {
            const item_y = self.itemY(screen_h, index);
            if (y >= item_y and y < item_y + theme.menu_item_h) return index;
        }
        return null;
    }

    pub fn itemY(self: *const Menu, screen_h: i32, index: usize) i32 {
        const top = screen_h - theme.taskbar_h - theme.menu_h;
        var y = top + theme.menu_top_pad;
        var prev_group: ?Group = null;
        var i: usize = 0;
        while (i <= index and i < self.count) : (i += 1) {
            const current_group = self.group(i);
            if (prev_group == null or prev_group.? != current_group) y += theme.menu_group_h;
            if (i == index) return y;
            y += theme.menu_item_h;
            prev_group = current_group;
        }
        return y;
    }

    pub fn startsGroup(self: *const Menu, index: usize) bool {
        if (index >= self.count) return false;
        if (index == 0) return true;
        return self.group(index - 1) != self.group(index);
    }

    pub fn groupLabel(self: *const Menu, index: usize) [*:0]const u8 {
        if (index >= self.count) return "";
        return switch (self.group(index)) {
            .programs => "Programs",
            .system => "System",
            .power => "Power",
        };
    }

    fn group(self: *const Menu, index: usize) Group {
        if (index >= self.count) return .system;
        return groupForTarget(self.entries[index].target);
    }

    fn submenuById(self: *const Menu, id: SubmenuId) ?*const Submenu {
        if (id == .none) return null;
        var i: usize = 0;
        while (i < self.submenu_count) : (i += 1) {
            if (self.submenus[i].id == id) return &self.submenus[i];
        }
        return null;
    }

    fn addLine(self: *Menu, line: []const u8) void {
        if (startsWith(line, "ITEM;")) {
            const title = valueOf(line, "title=") orelse return;
            const path = valueOf(line, "path=") orelse "";
            const args = valueOf(line, "args=") orelse "";
            const launch_policy = valueOf(line, "policy=") orelse valueOf(line, "class=") orelse "auto";
            self.addKnownArgs(targetForItem(title, path), .item, title, parsePolicy(launch_policy), path, args);
        } else if (startsWith(line, "ACTION;")) {
            const title = valueOf(line, "title=") orelse return;
            const action = valueOf(line, "action=") orelse return;
            self.addKnown(targetForAction(action), .action, title, .action, "");
        } else if (startsWith(line, "SUBMENU;")) {
            const title = valueOf(line, "title=") orelse return;
            const id = parseSubmenuId(valueOf(line, "id=") orelse valueOf(line, "menu=") orelse "");
            self.addSubmenu(targetForSubmenu(id), title, id);
        } else if (startsWith(line, "CHILDMENU;")) {
            const parent_id = parseSubmenuId(valueOf(line, "menu=") orelse return);
            const child_id = parseSubmenuId(valueOf(line, "id=") orelse return);
            const title = valueOf(line, "title=") orelse return;
            if (self.findOrCreateSubmenu(parent_id)) |parent| {
                parent.addSubmenu(targetForSubmenu(child_id), title, child_id);
                _ = self.findOrCreateSubmenu(child_id);
            }
        } else if (startsWith(line, "SUBACTION;")) {
            const id = parseSubmenuId(valueOf(line, "menu=") orelse return);
            const title = valueOf(line, "title=") orelse return;
            const action = valueOf(line, "action=") orelse return;
            if (self.findOrCreateSubmenu(id)) |submenu_entry| {
                submenu_entry.addKnown(targetForSubAction(id, action), .action, title, .action, "");
            }
        } else if (startsWith(line, "SUBITEM;")) {
            const id = parseSubmenuId(valueOf(line, "menu=") orelse return);
            const title = valueOf(line, "title=") orelse return;
            const path = valueOf(line, "path=") orelse "";
            const args = valueOf(line, "args=") orelse "";
            const launch_policy = valueOf(line, "policy=") orelse valueOf(line, "class=") orelse "auto";
            if (self.findOrCreateSubmenu(id)) |submenu_entry| {
                submenu_entry.addKnownArgs(targetForSubItem(id, title, path), .item, title, parsePolicy(launch_policy), path, args);
            }
        }
    }

    fn addKnown(self: *Menu, item_target: model.UiTarget, kind: EntryKind, title: []const u8, launch_policy: LaunchPolicy, path: []const u8) void {
        self.addKnownArgs(item_target, kind, title, launch_policy, path, "");
    }

    fn addKnownArgs(self: *Menu, item_target: model.UiTarget, kind: EntryKind, title: []const u8, launch_policy: LaunchPolicy, path: []const u8, args: []const u8) void {
        if (self.count >= self.entries.len or item_target == .none) return;
        var entry = Entry{ .target = item_target, .kind = kind, .launch_policy = launch_policy };
        copyZ(entry.title[0..], title);
        copyZ(entry.policy[0..], launch_policy.label());
        copyZ(entry.path[0..], path);
        copyZ(entry.args[0..], args);
        self.entries[self.count] = entry;
        self.count += 1;
    }

    fn addSubmenu(self: *Menu, item_target: model.UiTarget, title: []const u8, id: SubmenuId) void {
        if (self.count >= self.entries.len or item_target == .none or id == .none) return;
        var entry = Entry{ .target = item_target, .kind = .submenu, .launch_policy = .action, .submenu_id = id };
        copyZ(entry.title[0..], title);
        copyZ(entry.policy[0..], "");
        self.entries[self.count] = entry;
        self.count += 1;
        _ = self.findOrCreateSubmenu(id);
    }

    fn findOrCreateSubmenu(self: *Menu, id: SubmenuId) ?*Submenu {
        if (id == .none) return null;
        var i: usize = 0;
        while (i < self.submenu_count) : (i += 1) {
            if (self.submenus[i].id == id) return &self.submenus[i];
        }
        if (self.submenu_count >= self.submenus.len) return null;
        self.submenus[self.submenu_count] = .{ .id = id };
        self.submenu_count += 1;
        return &self.submenus[self.submenu_count - 1];
    }
};

fn groupForTarget(target: model.UiTarget) Group {
    return switch (target) {
        .menu_restart, .menu_poweroff, .menu_halt => .power,
        .menu_terminal_mode, .menu_run, .menu_settings, .menu_tasks => .system,
        else => .programs,
    };
}

fn targetForItem(title: []const u8, path: []const u8) model.UiTarget {
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/UPDATE.R4X") or equalsIgnoreCase(title, "R4OS Update")) return .menu_update;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X") or equalsIgnoreCase(title, "Terminal")) return .menu_terminal;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/NOTEPAD.R4X") or equalsIgnoreCase(title, "Notepad")) return .menu_notepad;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/PAINT.R4X") or equalsIgnoreCase(title, "Paint")) return .menu_paint;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/CALC.R4X") or equalsIgnoreCase(title, "Calculator")) return .menu_calc;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/TERMINAL/SYNTH.R4X") or equalsIgnoreCase(title, "R4Synth")) return .menu_synth;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/DEVMGR.R4X") or equalsIgnoreCase(title, "Device Manager")) return .menu_devmgr;
    if (equalsIgnoreCase(path, "/SOFTWARE/R4CODE/R4CODE.R4X") or equalsIgnoreCase(title, "R4Code")) return .menu_r4code;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/INTERNET/KLICKIFAX.R4X") or equalsIgnoreCase(title, "Klickifax")) return .menu_klickifax;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/APPDEF.R4X") or equalsIgnoreCase(title, "Default Apps")) return .menu_settings_default_apps;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/REGEDIT.R4X") or equalsIgnoreCase(title, "Registry Editor")) return .menu_settings_registry;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/NETCFG.R4X") or equalsIgnoreCase(title, "Network")) return .menu_settings_network;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/SERVICES.R4X") or equalsIgnoreCase(title, "Services")) return .menu_settings_services;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/LOGCENTER.R4X") or equalsIgnoreCase(title, "Log Center")) return .menu_settings_log_center;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/TIMESET.R4X") or equalsIgnoreCase(title, "Time Settings")) return .menu_settings_time;
    if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/APPEARANCE.R4X") or equalsIgnoreCase(title, "Appearance")) return .menu_settings_appearance;
    return .none;
}

fn targetForAction(action: []const u8) model.UiTarget {
    if (equalsIgnoreCase(action, "run")) return .menu_run;
    if (equalsIgnoreCase(action, "terminal_mode")) return .menu_terminal_mode;
    if (equalsIgnoreCase(action, "tasks")) return .menu_tasks;
    if (equalsIgnoreCase(action, "restart")) return .menu_restart;
    if (equalsIgnoreCase(action, "poweroff")) return .menu_poweroff;
    if (equalsIgnoreCase(action, "halt")) return .menu_halt;
    return .none;
}

fn targetForSubmenu(id: SubmenuId) model.UiTarget {
    return switch (id) {
        .programs => .menu_programs,
        .internet => .menu_programs_internet,
        .settings => .menu_settings,
        .none => .none,
    };
}

fn targetForSubAction(id: SubmenuId, action: []const u8) model.UiTarget {
    _ = action;
    return switch (id) {
        .programs, .internet => .none,
        .settings => .none,
        .none => .none,
    };
}

fn targetForSubItem(id: SubmenuId, title: []const u8, path: []const u8) model.UiTarget {
    return switch (id) {
        .programs => targetForItem(title, path),
        .internet => if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/INTERNET/KLICKIFAX.R4X") or equalsIgnoreCase(title, "Klickifax"))
            .menu_klickifax
        else
            .none,
        .settings => if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/APPEARANCE.R4X") or equalsIgnoreCase(title, "Appearance"))
            .menu_settings_appearance
        else if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/APPDEF.R4X") or equalsIgnoreCase(title, "Default Apps"))
            .menu_settings_default_apps
        else if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/REGEDIT.R4X") or equalsIgnoreCase(title, "Registry Editor"))
            .menu_settings_registry
        else if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/NETCFG.R4X") or equalsIgnoreCase(title, "Network"))
            .menu_settings_network
        else if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/SERVICES.R4X") or equalsIgnoreCase(title, "Services"))
            .menu_settings_services
        else if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/LOGCENTER.R4X") or equalsIgnoreCase(title, "Log Center"))
            .menu_settings_log_center
        else if (equalsIgnoreCase(path, "/R4OS/SOFTWARE/DESKTOP/TIMESET.R4X") or equalsIgnoreCase(title, "Time Settings"))
            .menu_settings_time
        else
            .none,
        .none => .none,
    };
}

fn parseSubmenuId(value: []const u8) SubmenuId {
    if (equalsIgnoreCase(value, "programs")) return .programs;
    if (equalsIgnoreCase(value, "internet")) return .internet;
    if (equalsIgnoreCase(value, "settings")) return .settings;
    return .none;
}

fn parsePolicy(value: []const u8) LaunchPolicy {
    if (equalsIgnoreCase(value, "console")) return .console;
    if (equalsIgnoreCase(value, "gui")) return .gui;
    if (equalsIgnoreCase(value, "action")) return .action;
    return .auto;
}

fn valueOf(line: []const u8, key: []const u8) ?[]const u8 {
    var rest = line;
    while (rest.len > 0) {
        const split = findByte(rest, ';') orelse rest.len;
        const part = trim(rest[0..split]);
        if (startsWith(part, key)) return part[key.len..];
        if (split < rest.len) {
            rest = rest[split + 1 ..];
        } else {
            rest = rest[split..];
        }
    }
    return null;
}

fn copyZ(out: []u8, value: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const count = @min(value.len, out.len - 1);
    if (count > 0) @memcpy(out[0..count], value[0..count]);
    out[count] = 0;
}

fn trim(value: []const u8) []const u8 {
    var start: usize = 0;
    var end = value.len;
    while (start < end and isSpace(value[start])) : (start += 1) {}
    while (end > start and isSpace(value[end - 1])) : (end -= 1) {}
    return value[start..end];
}

fn startsWith(value: []const u8, prefix: []const u8) bool {
    return value.len >= prefix.len and bytesEqual(value[0..prefix.len], prefix);
}

fn equalsIgnoreCase(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (asciiLower(a[i]) != asciiLower(b[i])) return false;
    }
    return true;
}

fn bytesEqual(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

fn findByte(value: []const u8, needle: u8) ?usize {
    var i: usize = 0;
    while (i < value.len) : (i += 1) {
        if (value[i] == needle) return i;
    }
    return null;
}

fn asciiLower(ch: u8) u8 {
    if (ch >= 'A' and ch <= 'Z') return ch + ('a' - 'A');
    return ch;
}

fn isSpace(ch: u8) bool {
    return ch == ' ' or ch == '\t' or ch == '\r' or ch == '\n';
}

pub fn submenuItemY(rect: surface.Rect, index: usize) i32 {
    return rect.y + 4 + @as(i32, @intCast(index)) * theme.menu_item_h;
}

test "default menu keeps stable Desktop entries" {
    const menu = Menu.initDefault();

    try std.testing.expectEqual(@as(usize, 9), menu.count);
    try std.testing.expectEqual(@as(usize, 3), menu.submenu_count);
    try std.testing.expectEqual(model.UiTarget.menu_update, menu.target(0));
    try std.testing.expectEqualStrings("/R4OS/SOFTWARE/DESKTOP/UPDATE.R4X", std.mem.span(menu.launch(0).?.path));
    try std.testing.expectEqual(model.UiTarget.menu_programs, menu.target(1));
    try std.testing.expectEqual(EntryKind.submenu, menu.launch(1).?.kind);
    try std.testing.expect(menu.hasSubmenu(1));
    try std.testing.expectEqual(model.UiTarget.menu_terminal, menu.submenuTarget(1, 0));
    try std.testing.expectEqual(LaunchPolicy.console, menu.submenuLaunch(1, 0).?.policy);
    try std.testing.expectEqual(model.UiTarget.menu_paint, menu.submenuTarget(1, 2));
    try std.testing.expectEqual(model.UiTarget.menu_r4code, menu.submenuTarget(1, 6));
    try std.testing.expectEqual(model.UiTarget.menu_programs_internet, menu.submenuTarget(1, 7));
    try std.testing.expect(menu.submenuHasSubmenu(1, 7));
    try std.testing.expectEqual(model.UiTarget.menu_klickifax, menu.nestedTarget(1, 7, 0));
    try std.testing.expectEqualStrings("/R4OS/SOFTWARE/INTERNET/KLICKIFAX.R4X", std.mem.span(menu.nestedLaunch(1, 7, 0).?.path));
    try std.testing.expectEqual(model.UiTarget.menu_terminal_mode, menu.target(2));
    try std.testing.expectEqual(model.UiTarget.menu_settings, menu.target(4));
    try std.testing.expectEqual(model.UiTarget.menu_restart, menu.target(6));
    try std.testing.expectEqual(model.UiTarget.menu_halt, menu.target(8));
    try std.testing.expectEqual(model.UiTarget.menu_settings_appearance, menu.submenuTarget(4, 0));
    try std.testing.expectEqualStrings("/R4OS/SOFTWARE/DESKTOP/APPEARANCE.R4X", std.mem.span(menu.submenuLaunch(4, 0).?.path));
}

test "menu parser loads MENU.R4S item and action lines" {
    var menu = Menu.initDefault();
    const ok = menu.loadFromBytes(
        \\# comment
        \\SUBMENU;title=Programs;id=programs
        \\SUBITEM;menu=programs;title=Terminal;path=/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X;class=console;policy=console
        \\SUBITEM;menu=programs;title=Notepad;path=/R4OS/SOFTWARE/DESKTOP/NOTEPAD.R4X;class=gui;policy=gui
        \\CHILDMENU;menu=programs;title=Internet;id=internet
        \\SUBITEM;menu=internet;title=Klickifax;path=/R4OS/SOFTWARE/INTERNET/KLICKIFAX.R4X;class=gui;policy=gui
        \\ACTION;title=Terminal Mode;action=terminal_mode
        \\ACTION;title=Run...;action=run
        \\SUBMENU;title=Settings;id=settings
        \\SUBITEM;menu=settings;title=Appearance;path=/R4OS/SOFTWARE/DESKTOP/APPEARANCE.R4X;class=gui;policy=gui
        \\ACTION;title=Tasks;action=tasks
        \\ACTION;title=Restart;action=restart
        \\ACTION;title=Poweroff;action=poweroff
        \\ACTION;title=Halt;action=halt
    );

    try std.testing.expect(ok);
    try std.testing.expect(menu.loaded_from_config);
    try std.testing.expectEqual(@as(usize, 8), menu.count);
    try std.testing.expectEqual(@as(usize, 3), menu.submenu_count);
    try std.testing.expectEqual(model.UiTarget.menu_programs, menu.target(0));
    try std.testing.expectEqual(model.UiTarget.menu_terminal, menu.submenuTarget(0, 0));
    try std.testing.expectEqual(model.UiTarget.menu_notepad, menu.submenuTarget(0, 1));
    try std.testing.expectEqual(model.UiTarget.menu_programs_internet, menu.submenuTarget(0, 2));
    try std.testing.expectEqual(model.UiTarget.menu_klickifax, menu.nestedTarget(0, 2, 0));
    try std.testing.expectEqual(LaunchPolicy.gui, menu.nestedLaunch(0, 2, 0).?.policy);
    try std.testing.expectEqualStrings("/R4OS/SOFTWARE/INTERNET/KLICKIFAX.R4X", std.mem.span(menu.nestedLaunch(0, 2, 0).?.path));
    try std.testing.expectEqual(model.UiTarget.menu_settings, menu.target(3));
    try std.testing.expectEqual(model.UiTarget.menu_settings_appearance, menu.submenuTarget(3, 0));
    try std.testing.expectEqual(model.UiTarget.menu_poweroff, menu.target(6));
}

test "menu parser keeps defaults when no known entries are found" {
    var menu = Menu.initDefault();
    const ok = menu.loadFromBytes(
        \\ITEM;title=Unknown;path=/R4OS/SOFTWARE/DESKTOP/UNKNOWN.R4X;policy=gui
        \\ACTION;title=Settings;action=settings
        \\ACTION;title=Nope;action=unknown
    );

    try std.testing.expect(!ok);
    try std.testing.expectEqual(@as(usize, 9), menu.count);
    try std.testing.expectEqual(model.UiTarget.menu_update, menu.target(0));
}

test "menu layout groups programs system and power actions" {
    const menu = Menu.initDefault();
    const screen_h: i32 = 720;
    const top = screen_h - theme.taskbar_h - theme.menu_h;

    try std.testing.expect(menu.startsGroup(0));
    try std.testing.expect(!menu.startsGroup(1));
    try std.testing.expect(menu.startsGroup(2));
    try std.testing.expect(menu.startsGroup(6));
    try std.testing.expectEqualStrings("Programs", std.mem.span(menu.groupLabel(0)));
    try std.testing.expectEqualStrings("System", std.mem.span(menu.groupLabel(2)));
    try std.testing.expectEqualStrings("Power", std.mem.span(menu.groupLabel(6)));
    try std.testing.expectEqual(top + theme.menu_top_pad + theme.menu_group_h, menu.itemY(screen_h, 0));
    try std.testing.expectEqual(@as(?usize, 1), menu.hit(screen_h, 40, menu.itemY(screen_h, 1) + 3));
    try std.testing.expectEqual(@as(?usize, 0), menu.hit(screen_h, 40, menu.itemY(screen_h, 1) - 3));
    const rect = menu.submenuRect(1280, screen_h, 1).?;
    try std.testing.expectEqual(@as(i32, theme.menu_w - 4), rect.x);
    try std.testing.expectEqual(@as(?usize, 7), menu.submenuHit(1280, screen_h, 1, rect.x + 8, submenuItemY(rect, 7) + 3));
    const nested_rect = menu.nestedRect(1280, screen_h, 1, 7).?;
    try std.testing.expectEqual(@as(?usize, 0), menu.nestedHit(1280, screen_h, 1, 7, nested_rect.x + 8, submenuItemY(nested_rect, 0) + 3));
}
