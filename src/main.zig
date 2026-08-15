const r4os = @import("r4os");
const r4img = @import("r4img");
const r4std = @import("r4std");
const desktop_api = @import("api.zig");
const app = @import("app.zig");

pub fn r4_app_main(r4_app: *r4os.App) i32 {
    if (!r4std.init(r4_app.startContext())) return r4os.abi.err_no_group;
    var images = r4img.Context.init(r4_app.startContext()) orelse return r4os.abi.err_no_group;
    var ctx = desktop_api.Context.init(r4_app) orelse return r4os.abi.err_no_group;
    var desktop = app.App{ .ctx = &ctx, .images = &images };
    return desktop.run();
}
