//! Boundary faults exercise the actual SDK writer and Desktop publisher.
//! Filesystem identity/atomicity is qualified separately in the real guest.
const std = @import("std");
const r = @import("r4os");
const file = @import("capture_file.zig");
const t = std.testing;
var stage_storage: [64:0]u8 = @splat(0);
fn stage() []const u8 { return std.mem.sliceTo(&stage_storage, 0); }
var model: struct {
    begin_code: i32 = 0, write_code: i32 = 0, finish_code: i32 = 0,
    publish_code: i32 = 0, abort_code: i32 = 0,
    count: u64 = 0, finishes: u32 = 0, publishes: u32 = 0, aborts: u32 = 0,
} = .{};
fn begin(path: [*:0]const u8, flags: u32) callconv(.c) i32 {
    const bytes = std.mem.span(path);
    std.debug.assert(bytes.len == 20 and std.mem.startsWith(u8, bytes, "C:\\TEMP\\") and std.mem.endsWith(u8, bytes, ".TMP"));
    for (bytes[8..16]) |digit| std.debug.assert(std.ascii.isHex(digit));
    @memset(&stage_storage, 0); @memcpy(stage_storage[0..bytes.len], bytes);
    std.debug.assert(flags == r.abi.file_stream_open_create);
    return model.begin_code;
}
fn write(path: [*:0]const u8, offset: u64, _: [*]const u8, len: u32, flags: u32) callconv(.c) i32 {
    std.debug.assert(std.mem.eql(u8, std.mem.span(path), stage()) and flags == 0 and offset == model.count);
    if (model.write_code != 0) return model.write_code;
    model.count += len; return @intCast(len);
}
fn finish(path: [*:0]const u8, expected: u64, flags: u32) callconv(.c) i32 {
    std.debug.assert(std.mem.eql(u8, std.mem.span(path), stage()) and expected == model.count);
    std.debug.assert(flags == r.r4sys.file_stream_finish_keep_ownership);
    model.finishes += 1; return model.finish_code;
}
fn publish(target: [*:0]const u8, source: [*:0]const u8, backup: [*:0]const u8, flags: u32) callconv(.c) i32 {
    std.debug.assert(std.mem.eql(u8, std.mem.span(target), "C:\\TEMP\\CAPTURE.BMP"));
    std.debug.assert(std.mem.eql(u8, std.mem.span(source), stage()));
    const backup_bytes = std.mem.span(backup);
    std.debug.assert(backup_bytes.len == 20 and std.mem.eql(u8, backup_bytes[0..16], stage()[0..16]) and std.mem.endsWith(u8, backup_bytes, ".BAK"));
    std.debug.assert(flags == r.r4sys.file_replace_atomic_flag_consume_stage |
        r.r4sys.file_replace_atomic_flag_require_owned_stage | r.r4sys.file_replace_atomic_flag_require_target_absent);
    std.debug.assert(model.finishes == 1);
    model.publishes += 1; return model.publish_code;
}
fn abort(path: [*:0]const u8) callconv(.c) i32 {
    std.debug.assert(std.mem.eql(u8, std.mem.span(path), stage()));
    model.aborts += 1; return model.abort_code;
}
const Harness = struct {
    table: r.abi.R4XStartR4Sys = .{},
    imports: [1]r.abi.R4XStartImport = undefined,
    start: r.abi.R4XStartContext = undefined,
    app: r.App = undefined,
    fn init(self: *Harness) !r.r4sys.Context {
        model = .{};
        self.table.file_stream_begin = @intFromPtr(&begin);
        self.table.file_stream_write = @intFromPtr(&write);
        self.table.file_stream_finish = @intFromPtr(&finish);
        self.table.file_stream_abort = @intFromPtr(&abort);
        self.table.file_replace_atomic = @intFromPtr(&publish);
        self.imports = .{.{ .group_id = @intFromEnum(r.abi.R4LGroup.r4sys),
            .flags = r.abi.r4xstart_import_flag_group_interface, .table = @intFromPtr(&self.table) }};
        self.start = .{ .flags = r.abi.r4xstart_flag_imports_valid, .imports = @intFromPtr(&self.imports), .import_count = 1 };
        self.app = switch (r.App.init(&self.start, .console)) { .value => |app| app, .failure => return error.App };
        return self.app.system();
    }
};
test "successful moved stage keeps exact lease through flush and create-only publish" {
    var h: Harness = .{};
    const sys = try h.init();
    const initial = try file.Output.open(sys, "C:\\TEMP\\CAPTURE.BMP");
    var out = initial;
    try t.expect(out.write("BMdata")); try out.finish();
    try t.expectEqual(file.Phase.published, out.phase);
    try t.expect(out.bytes() == 6 and !out.retained() and out.abort());
    try t.expect(model.finishes == 1 and model.publishes == 1 and model.aborts == 0);
}
test "publication failure after flush aborts retained stage, never a released stream" {
    var h: Harness = .{}; const sys = try h.init();
    var out = try file.Output.open(sys, "C:\\TEMP\\CAPTURE.BMP");
    model.publish_code = r.r4sys.file_replace_atomic_error_io;
    try t.expect(out.write("BMdata")); try t.expectError(error.Publish, out.finish());
    try t.expect(out.phase == .finished and out.retained() and out.writer.owned);
    try t.expect(out.abort() and out.phase == .aborted);
    try t.expect(out.failure.? == .publish and out.error_code == model.publish_code and model.aborts == 1);
}
test "target conflict retains primary failure and only exact abort acknowledgement retires owner" {
    var h: Harness = .{}; const sys = try h.init();
    var out = try file.Output.open(sys, "C:\\TEMP\\CAPTURE.BMP");
    model.publish_code = r.r4sys.file_replace_atomic_error_conflict;
    model.abort_code = r.abi.file_stream_error_io;
    try t.expect(out.write("BMdata")); try t.expectError(error.Publish, out.finish());
    try t.expect(!out.abort() and out.retained() and out.phase == .finished);
    try t.expect(out.error_code == model.publish_code and out.cleanup_code == model.abort_code);
    // SDK convenience flags were reset; the Desktop still owns the identity.
    try t.expect(!out.writer.owned and std.mem.eql(u8, out.writer.path.bytes(), stage()));
    model.abort_code = r.abi.file_stream_error_not_found;
    try t.expect(!out.abort() and out.retained());
    model.abort_code = 0; try t.expect(out.abort() and !out.retained());
    try t.expect(out.error_code == model.publish_code and model.aborts == 3);
}
test "foreign stage refusal has no abort or publication side effect" {
    var h: Harness = .{}; const sys = try h.init();
    model.begin_code = r.abi.file_stream_error_io;
    try t.expectError(error.Open, file.Output.open(sys, "C:\\TEMP\\CAPTURE.BMP"));
    try t.expect(model.aborts == 0 and model.publishes == 0 and model.finishes == 0);
}
test "partial write or failed flush keeps exact owner for abort" {
    var h: Harness = .{}; const sys = try h.init();
    var out = try file.Output.open(sys, "C:\\TEMP\\CAPTURE.BMP");
    model.write_code = r.abi.file_stream_error_io;
    try t.expect(!out.write("partial") and out.retained());
    try t.expect(out.abort() and out.failure.? == .write and out.error_code == model.write_code);
    out = try file.Output.open(sys, "C:\\TEMP\\CAPTURE.BMP");
    model.write_code = 0; model.finish_code = r.abi.file_stream_error_io;
    try t.expect(out.write("BMdata")); try t.expectError(error.Finish, out.finish());
    try t.expect(out.retained() and out.abort() and out.failure.? == .finish and model.publishes == 0);
}
