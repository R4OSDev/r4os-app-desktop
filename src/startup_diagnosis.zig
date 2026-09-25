//! Temporary bounded startup diagnosis. Only the desktop event thread records
//! events; one deferred TEMP write after thirty seconds, outside motion traces.
const std = @import("std");
const r4os = @import("r4os");
var clock: ?r4os.r4sys.Context = null;
var start_ns: u64 = 0;
var bytes: [24576]u8 = undefined;
var used: usize = 0;
var count: usize = 0;
var dropped: usize = 0;
var flushed = false;
pub fn initialize(sys: r4os.r4sys.Context) void { if (clock == null) clock = sys; }
pub fn record(comptime fmt: []const u8, args: anytype) void {
    const sys = clock orelse return;
    if (flushed) return;
    if (count == 64) { dropped += 1; return; }
    const now = sys.monotonicNanoseconds() orelse return;
    if (start_ns == 0) start_ns = now;
    const prefix = std.fmt.bufPrint(bytes[used .. bytes.len - 128], "ns={d} ", .{now}) catch { dropped += 1; return; };
    const text = std.fmt.bufPrint(bytes[used + prefix.len .. bytes.len - 128], fmt ++ "\n", args) catch { dropped += 1; return; };
    used += prefix.len + text.len; count += 1;
}
pub fn flush() void {
    if (flushed or start_ns == 0 or @import("presentation_profile.zig").running()) return;
    const sys = clock orelse return;
    const now = sys.monotonicNanoseconds() orelse return;
    if (now < start_ns or now - start_ns < 30 * std.time.ns_per_s) return;
    flushed = true;
    const suffix = std.fmt.bufPrint(bytes[used..], "END R4DESK84 startup records={d} dropped={d}\n", .{count, dropped}) catch return;
    used += suffix.len;
    _ = sys.fileWrite("C:\\TEMP\\AMD221\\DSK84SEL.LOG", bytes[0..used]);
}
