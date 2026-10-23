//! Materialize with the current original capture_file/screenshot/recording
//! sources in the same temporary source directory and their normal SDK imports.
const std = @import("std");
const r = @import("r4os");
const capture_file = @import("capture_file.zig");
const output = capture_file.Output;
const shot = @import("screenshot.zig");
const recording = @import("recording.zig");
const base = "C:\\TEMP\\C85B-";
const failed = base ++ "CONFLICT.BMP";
const partial = base ++ "PARTIAL.BMP";
const bmp = base ++ "SHOT.BMP";
fn absent(sys: r.r4sys.Context, path: [*:0]const u8) !void {
    if (sys.exists(path)) return error.UnexpectedFile;
}
fn exact(sys: r.r4sys.Context, path: [*:0]const u8, expected: []const u8) !void {
    var bytes: [64]u8 = undefined;
    const info = sys.fileInfo(path) orelse return error.Missing;
    if (info.size != expected.len or sys.fileRead(path, &bytes) != expected.len or !std.mem.eql(u8, bytes[0..expected.len], expected)) return error.Bytes;
}
fn check(app: *r.App) !void {
    const sys = app.system();
    const allocator = app.allocator() orelse return error.Allocator;
    const Paths = struct { failed: capture_file.Names, foreign: capture_file.Names, partial: capture_file.Names, bmp: capture_file.Names };
    const paths = try allocator.create(Paths);
    defer allocator.destroy(paths);
    paths.failed = try capture_file.names(failed);
    paths.foreign = try capture_file.names(base ++ "FOREIGN.BMP");
    paths.partial = try capture_file.names(partial);
    paths.bmp = try capture_file.names(bmp);
    const failed_names = &paths.failed;
    const foreign_names = &paths.foreign;
    const partial_names = &paths.partial;
    const bmp_names = &paths.bmp;
    const foreign = foreign_names.stage.asZ().ptr;
    for ([_][*:0]const u8{ failed, failed_names.stage.asZ().ptr, failed_names.backup.asZ().ptr, foreign, partial_names.stage.asZ().ptr, bmp }) |p| try absent(sys, p);
    // A real target appears after the exact stage was acquired and written.
    var stage = try output.open(sys, failed);
    if (!stage.write("BMstage")) return error.Write;
    if (sys.fileWrite(failed, "FOREIGN-TARGET") != 14) return error.Target;
    if (stage.finish()) return error.ConflictMissing else |err| if (err != error.Publish) return err;
    if (!stage.retained() or stage.phase != .finished or stage.error_code != r.r4sys.file_replace_atomic_error_conflict) {
        var diagnostic: [128]u8 = undefined;
        sys.println(try std.fmt.bufPrint(&diagnostic, "CAPTURE85: ownership phase={s}, raw={d}, owned={any}", .{ @tagName(stage.phase), stage.error_code, stage.writer.owned }));
        return error.Ownership;
    }
    if (!stage.abort() or stage.retained()) return error.Abort;
    try exact(sys, failed, "FOREIGN-TARGET");
    try absent(sys, failed_names.stage.asZ().ptr); try absent(sys, failed_names.backup.asZ().ptr);
    if (sys.fileWrite(foreign, "FOREIGN-STAGE") != 13) return error.Foreign;
    if (output.open(sys, base ++ "FOREIGN.BMP")) |_| return error.ForeignOpened else |err| if (err != error.Open) return err;
    try exact(sys, foreign, "FOREIGN-STAGE");
    stage = try output.open(sys, partial);
    if (!stage.write("partial") or !stage.abort()) return error.Partial;
    try absent(sys, partial); try absent(sys, partial_names.stage.asZ().ptr);
    if (sys.fileDelete(failed) <= 0 or sys.fileDelete(foreign) <= 0) return error.Cleanup;
    sys.println("CAPTURE85: real flush/conflict/exact abort, foreign target/stage and partial-write cleanup OK");
    const desk = app.desktop() orelse return error.Desktop;
    const job = shot.Job.startAt(allocator, sys, desk, bmp) orelse return error.Screenshot;
    const start = sys.ticks();
    while (true) {
        if (job.collect()) |result| {
            if (result.code != 0 or result.retained or result.bytes <= 54) return error.Screenshot;
            break;
        }
        if (sys.ticks() -% start > sys.ticksFromMilliseconds(20000)) { job.requestCancel(); return error.Timeout; }
        sys.sleepTicks(1);
    }
    var header: [54]u8 = undefined;
    const info = sys.fileInfo(bmp) orelse return error.ScreenshotMissing;
    if (sys.fileReadAt(bmp, 0, &header) != header.len or !std.mem.eql(u8, header[0..2], "BM") or std.mem.readInt(u32, header[2..6], .little) != info.size) return error.Bmp;
    try absent(sys, bmp_names.stage.asZ().ptr); try absent(sys, bmp_names.backup.asZ().ptr);
    sys.println("CAPTURE85: real immutable screenshot/worker join/BMP and no stage/backup OK");
    const manager = recording.Manager.create(allocator, app.startContext(), sys, desk) orelse return error.Recorder;
    defer manager.destroy();
    if (!manager.start(0)) return error.Start;
    // One short explicit software recording; no codec/GPU or full test matrix.
    sys.sleepTicks(sys.ticksFromMilliseconds(1600));
    manager.stop();
    const stop = sys.ticks();
    var final_path: [160:0]u8 = @splat(0);
    while (true) {
        // Acquire completion before reading the worker-owned path. Collection
        // below performs the actual join and may then destroy this exact job.
        if (manager.job) |owned| if (@atomicLoad(u32, &owned.done, .acquire) != 0) { final_path = owned.path; };
        if (manager.collect()) |result| {
            if (!result.ok or manager.active() or result.parts == 0 or result.frames == 0 or result.bytes == 0) return error.Recording;
            sys.println("CAPTURE85: real software recording/Stop/drain/worker join/publication OK");
            const completed = try capture_file.names(std.mem.sliceTo(&final_path, 0));
            try absent(sys, completed.stage.asZ().ptr); try absent(sys, completed.backup.asZ().ptr);
            sys.println("CAPTURE85: exact screenshot/recording short stage and backup absence OK");
            break;
        }
        if (sys.ticks() -% stop > sys.ticksFromMilliseconds(40000)) return error.RecordingTimeout;
        sys.sleepTicks(1);
    }
}
pub fn r4_app_main(app: *r.App) i32 {
    check(app) catch |err| { app.system().write("CAPTURE85: FAIL "); app.system().println(@errorName(err)); return -1; };
    app.system().println("CAPTURE85: OK"); return 0;
}
