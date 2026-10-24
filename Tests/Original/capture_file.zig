//! One create-only capture file. The SDK retains the exact stage identity
//! through flush, atomic publication and abort; filenames never imply ownership.
const std = @import("std");
const r = @import("r4os");
const storage = r.app_storage;
pub const Phase = enum { writing, finished, published, aborted };
pub const Failure = enum { write, finish, publish };
pub const Names = struct {
    target: storage.AbsoluteFilePath,
    stage: storage.AbsoluteFilePath,
    backup: storage.AbsoluteFilePath,
};
/// The current atomic provider requires short stage/backup names even on
/// NTFS. Create-only admission handles any hash collision without overwrite.
pub fn names(path: []const u8) !Names {
    const target = try storage.AbsoluteFilePath.parse(path);
    const parent = target.bytes()[0..(std.mem.lastIndexOfScalar(u8, target.bytes(), '\\') orelse return error.Path)];
    const key: u32 = @truncate(std.hash.Wyhash.hash(0, target.bytes()));
    var scratch: [r.path.file_path_max + 1:0]u8 = undefined;
    const stage = try storage.AbsoluteFilePath.parse(try std.fmt.bufPrint(&scratch, "{s}\\{x:0>8}.TMP", .{parent, key}));
    const backup = try storage.AbsoluteFilePath.parse(try std.fmt.bufPrint(&scratch, "{s}\\{x:0>8}.BAK", .{parent, key}));
    if (std.ascii.eqlIgnoreCase(target.bytes(), stage.bytes()) or std.ascii.eqlIgnoreCase(target.bytes(), backup.bytes())) return error.Path;
    return .{ .target = target, .stage = stage, .backup = backup };
}
pub const Output = struct {
    target: storage.AbsoluteFilePath,
    backup: storage.AbsoluteFilePath,
    writer: storage.OwnedStageWriter,
    phase: Phase = .writing,
    failure: ?Failure = null,
    error_code: i32 = 0,
    cleanup_code: i32 = 0,

    pub fn open(sys: r.r4sys.Context, path: []const u8) !Output {
        const paths = try names(path);
        const files: storage.Files = .{ .sys = sys };
        // The SDK lease-open entry may replace a pre-existing private stage.
        // Capture has no prior identity: ordinary CREATE alone atomically
        // refuses an existing name. Both streams support retained finish;
        // use the SDK's path-owning writer for all subsequent operations.
        return switch (files.streamWriter(paths.stage.asZ(), r.abi.file_stream_open_create)) {
            .writer => |writer| .{ .target = paths.target, .backup = paths.backup,
                .writer = .{ .files = files, .path = paths.stage, .state = writer.state } },
            .failure => error.Open,
        };
    }
    pub fn bytes(self: *const Output) u64 { return self.writer.state.offset; }
    pub fn retained(self: *const Output) bool { return self.phase == .writing or self.phase == .finished; }
    fn fail(self: *Output, kind: Failure, code: i32) void {
        if (self.failure == null) { self.failure = kind; self.error_code = code; }
    }
    pub fn write(self: *Output, data: []const u8) bool {
        if (self.phase != .writing or self.failure != null) return false;
        switch (self.writer.write(data)) {
            .ok => return true,
            .failure => |code| self.fail(.write, code),
            .missing => self.fail(.write, r.abi.file_stream_error_not_found),
        }
        return false;
    }
    pub fn finish(self: *Output) !void {
        if (self.phase != .writing or self.failure != null) return error.Finish;
        switch (self.writer.finishKeepOwnership()) {
            .ok => self.phase = .finished,
            .failure => |code| { self.fail(.finish, code); return error.Finish; },
            .missing => { self.fail(.finish, r.abi.file_stream_error_not_found); return error.Finish; },
        }
        const result = self.writer.files.replaceAtomic(self.target.asZ(), self.writer.path.asZ(), self.backup.asZ(),
            .{ .consume_stage = true, .require_target_absent = true, .require_owned_stage = true });
        switch (result) {
            .ok => { self.phase = .published; self.writer.owned = false; return; },
            .conflict => self.fail(.publish, r.r4sys.file_replace_atomic_error_conflict),
            .missing => self.fail(.publish, r.r4sys.file_replace_atomic_error_not_found),
            .unsupported => self.fail(.publish, r.r4sys.file_replace_atomic_error_unsupported),
            .bad_path => self.fail(.publish, r.r4sys.file_replace_atomic_error_bad_path),
            .failure => |code| self.fail(.publish, code),
        }
        return error.Publish;
    }
    /// Only an actual abort receipt clears our owner. The SDK may reset its
    /// convenience flags after failure; our phase and exact path stay retained.
    pub fn abort(self: *Output) bool {
        if (!self.retained()) return true;
        switch (self.writer.abort()) {
            .ok => { self.phase = .aborted; self.cleanup_code = 0; return true; },
            .failure => |code| self.cleanup_code = code,
            .missing => self.cleanup_code = r.abi.file_stream_error_not_found,
        }
        return false;
    }
    pub fn reportRetained(self: *const Output, sys: r.r4sys.Context) void {
        var message: [256]u8 = undefined;
        const text = std.fmt.bufPrint(&message, "Capture cleanup incomplete: {s}, primary {s}/{d}, cleanup {d}; owner retained until process teardown.",
            .{ self.writer.path.bytes(), if (self.failure) |f| @tagName(f) else "cancel", self.error_code, self.cleanup_code }) catch "Capture cleanup incomplete; exact file owner retained until process teardown.";
        sys.println(text);
    }
};
