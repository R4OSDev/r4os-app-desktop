//! Explicit private guest check using public input/clipboard/process APIs.
//! Run only in an otherwise quiet owned QEMU guest; never on a user desktop.
const std = @import("std");
const r = @import("r4os");
const target = @import("target.zig");
const unicode_path = "C:\\R4OS\\SOFTWARE\\DESKTOP\\00084 \u{c4}\u{4e2d}A\u{308}.R4X";
const quoted = "\"" ++ unicode_path ++ "\"";
// This bounded binary copy is test-owned storage, outside the small App stack.
var artifact_storage: [64 * 1024]u8 = undefined;
const Control = struct {
    sys: r.r4sys.Context,
    desk: r.r4desk.Context,
    phase: []const u8 = "begin",
    fn key(self: *@This(), value: u32) !void {
        for ([_]u32{ r.abi.remote_input_kind_key_down, r.abi.remote_input_kind_key_up }) |kind| {
            const event: r.abi.RemoteInputEvent = .{ .kind = kind, .key = value };
            if (self.desk.remoteInputPush(&event) != 1) return error.InputAdmission;
        }
    }
    fn text(self: *@This(), bytes: []const u8) !void {
        const view = try std.unicode.Utf8View.init(bytes);
        var it = view.iterator();
        while (it.nextCodepoint()) |cp| try self.key(cp);
    }
    fn revision(self: *@This()) !u32 {
        var info: r.abi.ClipboardInfo = .{};
        if (self.desk.clipboardInfo(&info) != 0) return error.ClipboardInfo;
        return info.revision;
    }
    fn waitRevision(self: *@This(), before: u32) !void {
        const deadline = self.sys.ticks() +| self.sys.ticksFromMilliseconds(5000);
        while (try self.revision() == before) {
            if (self.sys.ticks() >= deadline) return error.ClipboardAcknowledgement;
            self.sys.sleepTicks(1);
        }
    }
    fn copied(self: *@This(), expected: []const u8) !void {
        const before = try self.revision();
        try self.key(r.gui.Key.ctrl_c);
        try self.waitRevision(before);
        var bytes: [256]u8 = undefined;
        const got = self.desk.clipboardRead(&bytes);
        if (got < 0 or got >= bytes.len) return error.ClipboardRead;
        if (!std.mem.eql(u8, expected, bytes[0..@intCast(got)])) {
            self.sys.write("RUNINPUT actual="); self.sys.write(bytes[0..@intCast(got)]); self.sys.println("");
            return error.ExactInputBytes;
        }
    }
    fn cut(self: *@This()) !void {
        const before = try self.revision();
        try self.key(r.gui.Key.ctrl_x);
        try self.waitRevision(before);
    }
    fn paste(self: *@This(), bytes: []const u8) !void {
        if (self.desk.clipboardWrite(bytes) != bytes.len) return error.ClipboardWrite;
        try self.key(r.gui.Key.ctrl_v);
    }
    fn rejectAndResume(self: *@This()) !void {
        try self.key(r.gui.Key.escape);
        try self.key('r');
    }
    fn run(self: *@This()) !void {
        if (self.sys.exists(target.started) or self.sys.exists(target.stopped)) return error.OwnMarkerPresent;
        // The stock host image writer admits ASCII payload names. Materialize
        // this owned Unicode alias through the current guest filesystem,
        // independently of SSHD's deliberate protected-system-path policy.
        self.phase = "owned-unicode-alias";
        if (self.sys.exists(unicode_path)) return error.OwnAliasPresent;
        const artifact = &artifact_storage;
        const count = self.sys.fileRead("C:\\TEMP\\C84TARGET.R4X", artifact);
        if (count <= 0 or count >= artifact.len) return error.TargetArtifactRead;
        var expected_hash: [32]u8 = undefined;
        std.crypto.hash.sha2.Sha256.hash(artifact[0..@intCast(count)], &expected_hash, .{});
        if (self.sys.fileWrite(unicode_path, artifact[0..@intCast(count)]) != count) return error.UnicodeAliasWrite;
        if (self.sys.fileRead(unicode_path, artifact) != count) return error.UnicodeAliasRead;
        var actual_hash: [32]u8 = undefined;
        std.crypto.hash.sha2.Sha256.hash(artifact[0..@intCast(count)], &actual_hash, .{});
        if (!std.mem.eql(u8, &expected_hash, &actual_hash)) return error.UnicodeAliasBytes;
        try self.key('r');
        try self.cut();
        self.phase = "typed-scalars";
        const typed = "A\u{e4}\u{4e2d}B\u{308}";
        try self.text(typed); try self.copied(typed);
        self.phase = "grapheme-backspace";
        try self.key(r.gui.Key.backspace); try self.copied("A\u{e4}\u{4e2d}");
        try self.cut();
        const full = "a" ** 125 ++ "\u{e4}";
        self.phase = "capacity-paste";
        try self.paste(full); try self.copied(full);
        self.phase = "scalar-overflow";
        try self.key(0x1f680); try self.rejectAndResume(); try self.copied(full);
        try self.cut(); try self.text("a"); try self.copied("a");
        self.phase = "invalid-utf8-paste";
        try self.paste("\xff"); try self.rejectAndResume(); try self.copied("a");
        self.phase = "paste-overflow";
        try self.paste(full); try self.rejectAndResume(); try self.copied("a");
        try self.cut();
        self.phase = "browse";
        try self.key(r.gui.Key.tab); try self.key(r.gui.Key.enter);
        try self.copied(quoted);
        self.phase = "browse-backspace";
        for (0..6) |_| try self.key(r.gui.Key.backspace);
        try self.copied("\"C:\\R4OS\\SOFTWARE\\DESKTOP\\00084 \u{c4}\u{4e2d}");
        try self.cut(); try self.paste(quoted); try self.copied(quoted);
        self.phase = "unicode-launch";
        try self.key(r.gui.Key.enter);
        const deadline = self.sys.ticks() +| self.sys.ticksFromMilliseconds(7000);
        while (!self.sys.exists(target.started)) {
            if (self.sys.ticks() >= deadline) return error.UnicodeLaunch;
            self.sys.sleepTicks(1);
        }
        var handle: r.abi.ProgramProcessHandle = .{};
        if (self.sys.fileRead(target.started, std.mem.asBytes(&handle)) != @sizeOf(r.abi.ProgramProcessHandle) or handle.instance_id == 0 or handle.generation == 0) return error.TargetIdentity;
        var info: r.abi.ProgramInstanceInfo = .{};
        while (true) {
            if (self.sys.programHandleStatus(&handle, &info) != 0 or info.app_class != @intFromEnum(r.abi.ProgramInstanceClass.gui)) return error.TargetWindow;
            if (info.window_id >= 0) break;
            if (self.sys.ticks() >= deadline) return error.TargetWindow;
            self.sys.sleepTicks(1);
        }
        self.phase = "target-close-retirement";
        if (self.sys.programHandleRequestClose(&handle) != 0) return error.TargetClose;
        while (!self.sys.exists(target.stopped)) {
            if (self.sys.ticks() >= deadline) return error.TargetCloseAcknowledgement;
            self.sys.sleepTicks(1);
        }
        var close_bytes: [64]u8 = undefined;
        const close_count = self.sys.fileRead(target.stopped, &close_bytes);
        if (close_count != target.close_text.len or !std.mem.eql(u8, close_bytes[0..target.close_text.len], target.close_text)) return error.TargetCloseBytes;
        // The Desktop owns this Run child and its reap. Do not consume that
        // completion; verify disappearance of the exact current handle.
        while (self.sys.programHandleStatus(&handle, &info) == 0) {
            if (self.sys.ticks() >= deadline) return error.DesktopTargetRetirement;
            self.sys.sleepTicks(1);
        }
        self.sys.println("RUNINPUT: exact UTF8/scalars/graphemes,127-byte admission, invalid/overflow refusal, quoted Browse, real Unicode GUI launch/Close/owner retirement OK");
    }
};
pub fn r4_app_main(app: *r.App) i32 {
    var control: Control = .{ .sys = app.system(), .desk = app.desktop() orelse return 90 };
    control.run() catch |err| {
        control.sys.write("RUNINPUT FAILED phase="); control.sys.write(control.phase);
        control.sys.write(": "); control.sys.println(@errorName(err));
        return 91;
    };
    return 0;
}
