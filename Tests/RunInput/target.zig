//! Small reusable SDK-only target for the Zig/Rust Desktop Run comparison.
//! The private runner installs these bytes only at the Unicode/spaced name.
const std = @import("std");
const r = @import("r4os");
pub const started = "C:\\TEMP\\RUN84-START.BIN";
pub const stopped = "C:\\TEMP\\RUN84-STOP.TXT";
pub const close_text = "Run Unicode target: Close acknowledged\n";
pub fn r4_app_main(app: *r.App) i32 {
    const sys = app.system();
    if (sys.exists(started) or sys.exists(stopped)) return 21;
    var handle: r.abi.ProgramProcessHandle = .{};
    if (sys.programOpenHandle(@intCast(app.startContext().instance_id), &handle) != 0) return 22;
    if (sys.fileWrite(started, std.mem.asBytes(&handle)) != @sizeOf(r.abi.ProgramProcessHandle)) return 23;
    const deadline = sys.ticks() +| sys.ticksFromMilliseconds(15000);
    while (!sys.programShouldClose()) {
        if (sys.ticks() >= deadline) return 24;
        sys.sleepTicks(1);
    }
    return if (sys.fileWrite(stopped, close_text) == close_text.len) 0 else 25;
}
