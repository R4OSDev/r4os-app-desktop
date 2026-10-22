//! Resource ledger for explicit owner work. No normal sampling or painter.
const r = @import("r4os");
const wire = @import("desktop_check");
const Trace = @import("snes_output_trace.zig").Trace;
pub const State = struct {
    software_reserved: bool = false,
    snes_reserved: bool = false,
    child: r.abi.ProgramProcessHandle = .{},
    options: wire.SnesOptions = .{},
    trace: ?*Trace = null,
    started_ns: u64 = 0,
    presents_before: u64 = 0,
    cursor_before: u64 = 0,
    report_ready: bool = false,

    pub fn reserveSnes(self: *State, sys: *const r.r4sys.Context) bool {
        if (self.snes_reserved) return false;
        for (wire.snes_paths) |path| if (sys.exists(path)) return false;
        self.snes_reserved = true;
        return true;
    }
    pub fn cleanupFiles(self: *State, sys: *const r.r4sys.Context, caller: wire.Identity) bool {
        var ok = true;
        if (self.software_reserved) {
            var buffer: [128:0]u8 = undefined;
            const path = wire.softwareReportPath(&buffer, caller) catch return false;
            if (sys.exists(path) and sys.fileDelete(path) < 0) ok = false else self.software_reserved = false;
        }
        if (self.snes_reserved) {
            var retired = true;
            for (wire.snes_paths) |path| if (sys.exists(path) and sys.fileDelete(path) < 0) { retired = false; };
            if (retired) self.snes_reserved = false else ok = false;
        }
        return ok;
    }
};
