const r4os = @import("r4os");
const r4img = @import("r4img");
const r4std = @import("r4std");
const desktop_api = @import("api.zig");
const app = @import("app.zig");
const gfx_renderer = @import("gfx_renderer.zig");
const composition_worker = @import("composition_worker.zig");
const output_manager = @import("output_manager.zig");

pub fn r4_app_main(r4_app: *r4os.App) i32 {
    if (!r4std.init(r4_app.startContext())) return r4os.abi.err_no_group;
    var images = r4img.Context.init(r4_app.startContext()) orelse return r4os.abi.err_no_group;
    var ctx = desktop_api.Context.init(r4_app) orelse return r4os.abi.err_no_group;
    defer ctx.closeWindowService();
    ctx.graphics = gfx_renderer.Renderer.create(ctx.allocator(), r4_app.startContext());
    defer if (ctx.graphics) |graphics| {
        ctx.write("R4DESK gfx: batches="); ctx.printU64(graphics.batches);
        ctx.write(" fills="); ctx.printU64(graphics.fills);
        ctx.write(" rejected="); ctx.printU64(graphics.rejected_batches); ctx.write("\n");
        graphics.destroy(); ctx.graphics = null;
    };
    if (ctx.graphics) |graphics| {
        if (graphics.info()) |info| {
            ctx.write("R4DESK gfx: DEVICE_V1 backend="); ctx.printU64(info.backend);
            ctx.write(" adapter="); ctx.printU64(info.adapter_id); ctx.write("\n");
        }
    } else ctx.println("R4DESK gfx: scene fallback");
    const composition = composition_worker.Worker.create(ctx.allocator(), r4_app.startContext(), ctx.sys);
    defer if (composition) |worker| {
        ctx.write("R4DESK composition: visible="); ctx.printU64(worker.frames_visible);
        ctx.write(" uploads="); ctx.printU64(worker.engine.uploaded_bytes);
        ctx.write(" draws="); ctx.printU64(worker.engine.render_jobs);
        ctx.write(" preparations="); ctx.printU64(worker.prepared_threads); ctx.write("\n");
        worker.destroy();
    };
    const outputs = output_manager.Manager.create(ctx.allocator(), r4_app.startContext(), ctx.sys, ctx.draw);
    defer if (outputs) |manager| manager.destroy();
    var desktop = app.App{ .ctx = &ctx, .images = &images, .composition = composition, .outputs = outputs };
    return desktop.run();
}
