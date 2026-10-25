const std = @import("std");

pub fn build(b: *std.Build) void {
    // Rust product builds use the SDK's shared manifest/Cargo owner. The
    // independent original Zig oracles remain available as explicit targets.
    const sdk_build = b.lazyImport(@This(), "r4os_sdk") orelse return;
    const sdk_dep = b.dependencyFromBuildZig(sdk_build, .{});
    const libraries_build = b.lazyImport(@This(), "r4os_libraries") orelse return;
    const libraries_dep = b.dependencyFromBuildZig(libraries_build, .{});
    const sdk = sdk_build.sdk(b, sdk_dep, .{});
    if (!(b.option(bool, "host-tests", "Build only explicit original host oracles") orelse false))
        @panic("Build the Rust product through Build.sh/Build.bat. Original Zig oracles require -Dhost-tests=true.");

    const check_wire = b.createModule(.{ .root_source_file = b.path("Contract/desktop_check_v2.zig"), .target = b.graph.host });
    _ = sdk.addR4MFWithOptions(b.path("Tests/WindowImages/module.R4MF"), .{ .zig_module_roots = &.{b.path("Contract/desktop_check_v2.zig")} });
    b.step("window-images-fixture", "Build explicit real Desktop/WINSVC CPU image fixture").dependOn(b.getInstallStep());
    b.step("diagnostic-test", "Explicit bounded Desktop work byte contract and generation binding").dependOn(&b.addRunArtifact(b.addTest(.{ .root_module = check_wire })).step);

    const test_target = b.graph.host;
    const console_text = b.createModule(.{ .root_source_file = libraries_dep.namedLazyPath("r4std_console_text"), .target = test_target });
    const test_optimize: std.builtin.OptimizeMode = .Debug;
    const host_r4os = sdk.createR4osModule(test_target, test_optimize);
    const capture_root = b.createModule(.{ .root_source_file = b.path("Tests/Original/capture_file_test.zig"), .target = test_target, .optimize = test_optimize });
    capture_root.addImport("r4os", host_r4os);
    const capture_tests = b.addRunArtifact(b.addTest(.{ .root_module = capture_root }));
    b.step("capture-test", "Exact capture stage publication and acknowledgement-only cleanup").dependOn(&capture_tests.step);
    const input_root = b.createModule(.{ .root_source_file = b.path("Tests/Original/console_input.zig"), .target = test_target, .optimize = test_optimize });
    input_root.addImport("r4os", host_r4os);
    const input_tests = b.addRunArtifact(b.addTest(.{ .root_module = input_root }));
    const input_step = b.step("input-test", "Desktop typed input, exact Run UTF8/graphemes and current window generation");
    input_step.dependOn(&input_tests.step);
    const r4gfx = b.createModule(.{
        .root_source_file = libraries_dep.namedLazyPath("r4gfx_zig_binding"),
        .target = test_target,
        .optimize = test_optimize,
    });
    r4gfx.addImport("r4os", host_r4os);
    const readback = b.createModule(.{ .root_source_file = libraries_dep.namedLazyPath("r4gfx_readback"), .target = test_target });
    readback.addImport("r4os", host_r4os);
    readback.addImport("r4gfx", r4gfx);
    const topology = b.createModule(.{ .root_source_file = libraries_dep.namedLazyPath("r4gfx_desktop_outputs"), .target = test_target });
    topology.addImport("r4os", host_r4os);
    const r4nv_binding = b.createModule(.{ .root_source_file = libraries_dep.namedLazyPath("r4nv_zig_binding"), .target = test_target });
    r4nv_binding.addImport("r4os", host_r4os);
    const r4amd_binding = b.createModule(.{ .root_source_file = libraries_dep.namedLazyPath("r4amd_zig_binding"), .target = test_target });
    r4amd_binding.addImport("r4os", host_r4os);
    const r4gfx_implementation = b.createModule(.{ .root_source_file = libraries_dep.path("R4GFX/Contract/Generated/implementation_abi.zig"), .target = test_target });
    r4gfx_implementation.addImport("r4os", host_r4os);
    const r4gfx_provider = b.createModule(.{ .root_source_file = libraries_dep.path("R4GFX/Source/device.zig"), .target = test_target });
    r4gfx_provider.addImport("r4os", host_r4os);
    r4gfx_provider.addImport("r4l_contract", r4gfx_implementation);
    r4gfx_provider.addImport("r4nv_binding", r4nv_binding);
    r4gfx_provider.addImport("r4amd_binding", r4amd_binding);
    const color_provider = r4gfx_provider;
    libraries_build.addR4gfxHostColor(b, color_provider, libraries_dep.path("R4GFX"));
    const r4std_abi = b.createModule(.{
        .root_source_file = libraries_dep.path("R4STD/Bindings/Zig/r4std_abi.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    r4std_abi.addImport("r4os", host_r4os);
    const r4std = b.createModule(.{
        .root_source_file = libraries_dep.path("R4STD/Bindings/Zig/r4std.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    r4std.addImport("r4os", host_r4os);
    r4std.addImport("r4std_abi.zig", r4std_abi);
    const r4img_abi = b.createModule(.{
        .root_source_file = libraries_dep.path("R4IMG/Bindings/Zig/r4img_abi.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    r4img_abi.addImport("r4os", host_r4os);
    const r4img = b.createModule(.{
        .root_source_file = libraries_dep.path("R4IMG/Bindings/Zig/r4img.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    r4img.addImport("r4os", host_r4os);
    r4img.addImport("r4img_abi.zig", r4img_abi);
    const image_contract = b.createModule(.{ .root_source_file = libraries_dep.path("R4IMG/Contract/Generated/implementation_abi.zig"), .target = test_target });
    image_contract.addImport("r4os", host_r4os);
    const image_provider = b.createModule(.{ .root_source_file = libraries_dep.path("R4IMG/Source/main.zig"), .target = test_target });
    image_provider.addImport("r4os", host_r4os);
    image_provider.addImport("r4l_contract", image_contract);
    image_provider.addIncludePath(libraries_dep.path("R4IMG/ThirdParty/stb"));
    image_provider.addCSourceFile(.{ .file = libraries_dep.path("R4IMG/ThirdParty/stb/r4img_stb.c"), .flags = &.{ "-std=c11", "-fno-builtin" } });
    const png_fixture = b.createModule(.{ .root_source_file = libraries_dep.path("R4IMG/Tests/Decoder/png_fixture.zig"), .target = test_target });
    const raster_fixture = b.createModule(.{ .root_source_file = libraries_dep.path("R4IMG/Tests/Decoder/raster_fixture.zig"), .target = test_target });
    const r4std_implementation = b.createModule(.{
        .root_source_file = libraries_dep.path("R4STD/Contract/Generated/implementation_abi.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    r4std_implementation.addImport("r4os", host_r4os);
    const r4std_provider = b.createModule(.{
        .root_source_file = libraries_dep.path("R4STD/Source/main.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    r4std_provider.addImport("r4os", host_r4os);
    r4std_provider.addImport("r4l_contract", r4std_implementation);
    const r4std_test = b.createModule(.{
        .root_source_file = libraries_dep.path("R4STD/Tests/consumer_runtime.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    r4std_test.addImport("provider", r4std_provider);
    r4std_test.addImport("r4os", host_r4os);
    r4std_test.addImport("r4std", r4std);

    // This source-only Rust oracle shares the existing original R4STD
    // provider above. It never installs or builds a replacement Desktop.
    const rust_provider = b.createModule(.{ .root_source_file = b.path("Tests/RustNative/standard_provider.zig"), .target = test_target, .optimize = .ReleaseSafe, .pic = true });
    rust_provider.addImport("provider", r4std_provider);
    rust_provider.addImport("r4os", host_r4os);
    rust_provider.addImport("color_provider", color_provider);
    rust_provider.addImport("desktop_check",check_wire);
    const rust_gpu_model = b.createModule(.{ .root_source_file = b.path("Tests/Original/composition_gpu_test.zig"), .target = test_target, .optimize = .ReleaseSafe, .pic = true });
    rust_gpu_model.addImport("r4os",host_r4os);
    rust_gpu_model.addImport("r4img",r4img);
    // Native aggregate only: the canonical PS7 runner stages the original
    // six source files and removes exactly the otherwise colliding R4L entry.
    // Decoder/API exports and normal product/test modules stay untouched.
    const rust_image_root=b.option([]const u8,"rust-image-oracle-root","Temporary native image sources staged by RustNative.ps1");
    const rust_image_provider=if(rust_image_root) |path| blk:{
        const module=b.createModule(.{.root_source_file=.{.cwd_relative=b.pathJoin(&.{path,"main.zig"})},.target=test_target});
        module.addImport("r4os",host_r4os);module.addImport("r4l_contract",image_contract);
        module.addIncludePath(libraries_dep.path("R4IMG/ThirdParty/stb"));
        module.addCSourceFile(.{.file=libraries_dep.path("R4IMG/ThirdParty/stb/r4img_stb.c"),.flags=&.{"-std=c11","-fno-builtin"}});
        break :blk module;
    }else image_provider;
    rust_gpu_model.addImport("image_provider",rust_image_provider);
    rust_gpu_model.addImport("color_provider",color_provider);
    rust_gpu_model.addImport("png_fixture",png_fixture);
    rust_gpu_model.addImport("raster_fixture",raster_fixture);
    rust_gpu_model.addImport("profile_fixtures",b.createModule(.{.root_source_file=libraries_dep.path("R4GFX/Tests/Color/fixtures.zig"),.target=test_target,.optimize=test_optimize}));
    rust_gpu_model.addImport("r4gfx",r4gfx);
    rust_gpu_model.addImport("r4gfx_device_provider",r4gfx_provider);
    rust_gpu_model.addImport("r4gfx_desktop_outputs",topology);
    rust_gpu_model.addImport("r4gfx_readback",readback);
    rust_provider.addImport("gpu_owner_model",rust_gpu_model);
    const rust_provider_object = b.addObject(.{ .name = "desktop-rust-standard", .root_module = rust_provider });
    rust_provider_object.bundle_compiler_rt = true;
    b.step("rust-provider-oracle", "Build the existing R4STD provider for Rust configuration cases").dependOn(&b.addInstallFile(rust_provider_object.getEmittedBin(), "lib/desktop-rust-standard.o").step);

    const model_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/model.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const window_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/window.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const surface_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/surface.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const paint_module = b.createModule(.{
        .root_source_file = b.path("Tests/Original/paint.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    paint_module.addImport("r4os", sdk.createR4osModule(test_target, test_optimize));
    const paint_tests = b.addTest(.{ .root_module = paint_module });
    const draw_module = b.createModule(.{
        .root_source_file = b.path("Tests/Original/draw.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    draw_module.addImport("r4os", host_r4os);
    draw_module.addImport("r4std_console_text", console_text);
    draw_module.addImport("r4gfx", r4gfx);
    draw_module.addImport("r4gfx_device_provider", r4gfx_provider);
    const draw_tests = b.addTest(.{ .root_module = draw_module });
    const compositor_module = b.createModule(.{
        .root_source_file = b.path("Tests/Original/compositor.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    compositor_module.addImport("r4os", host_r4os);
    compositor_module.addImport("r4std_console_text", console_text);
    compositor_module.addImport("r4std", r4std);
    compositor_module.addImport("r4std_test", r4std_test);
    compositor_module.addImport("r4img", r4img);
    compositor_module.addImport("color_provider", color_provider);
    compositor_module.addImport("image_provider", image_provider);
    compositor_module.addImport("png_fixture", png_fixture);
    compositor_module.addImport("raster_fixture", raster_fixture);
    compositor_module.addImport("r4gfx", r4gfx);
    compositor_module.addImport("r4gfx_readback", readback);
    compositor_module.addImport("r4gfx_device_provider", r4gfx_provider);
    const compositor_tests = b.addTest(.{ .root_module = compositor_module });
    const gui_shape_renderer_module = b.createModule(.{
        .root_source_file = b.path("Tests/Original/gui_shape_renderer.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    gui_shape_renderer_module.addImport("r4os", sdk.createR4osModule(test_target, test_optimize));
    const gui_shape_renderer_tests = b.addTest(.{ .root_module = gui_shape_renderer_module });
    const scene_buffer_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/scene_buffer.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const start_menu_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/start_menu.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const desktop_config_module = b.createModule(.{
        .root_source_file = b.path("Tests/Original/desktop_config.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    desktop_config_module.addImport("r4os", host_r4os);
    desktop_config_module.addImport("r4std", r4std);
    desktop_config_module.addImport("r4std_test", r4std_test);
    const desktop_config_tests = b.addTest(.{
        .root_module = desktop_config_module,
    });
    const desktop_folder_module = b.createModule(.{
        .root_source_file = b.path("Tests/Original/desktop_folder.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    desktop_folder_module.addImport("r4os", host_r4os);
    desktop_folder_module.addImport("r4std", r4std);
    desktop_folder_module.addImport("r4std_test", r4std_test);
    const desktop_folder_tests = b.addTest(.{
        .root_module = desktop_folder_module,
    });
    const desktop_layout_module = b.createModule(.{
        .root_source_file = b.path("Tests/Original/desktop_layout.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    desktop_layout_module.addImport("r4os", host_r4os);
    desktop_layout_module.addImport("r4std", r4std);
    desktop_layout_module.addImport("r4std_test", r4std_test);
    const desktop_layout_tests = b.addTest(.{
        .root_module = desktop_layout_module,
    });
    const desktop_items_module = b.createModule(.{
        .root_source_file = b.path("Tests/Original/desktop_items.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    desktop_items_module.addImport("r4os", host_r4os);
    desktop_items_module.addImport("r4std", r4std);
    desktop_items_module.addImport("r4std_test", r4std_test);
    const desktop_items_tests = b.addTest(.{
        .root_module = desktop_items_module,
    });
    const icon_source_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/icon_source.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const quick_launch_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/quick_launch.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const text_field_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/text_field.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const run_command_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/run_command.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const console_scroll_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/console_scroll.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const appearance_signal_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/appearance_signal.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const appearance_colors_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/appearance_colors.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const wallpaper_module = b.createModule(.{
        .root_source_file = b.path("Tests/Original/wallpaper.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    wallpaper_module.addImport("r4os", host_r4os);
    wallpaper_module.addImport("r4img", r4img);
    wallpaper_module.addImport("r4gfx", r4gfx);
    wallpaper_module.addImport("color_provider", color_provider);
    wallpaper_module.addImport("image_provider", image_provider);
    wallpaper_module.addImport("png_fixture", png_fixture);
    wallpaper_module.addImport("raster_fixture", raster_fixture);
    wallpaper_module.addImport("profile_fixtures", b.createModule(.{
        .root_source_file = libraries_dep.path("R4GFX/Tests/Color/fixtures.zig"),
        .target = test_target,
        .optimize = test_optimize,
    }));
    const wallpaper_tests = b.addTest(.{
        .root_module = wallpaper_module,
    });
    const gui_frame_snapshot_module = b.createModule(.{
        .root_source_file = b.path("Tests/Original/gui_frame_snapshot.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    gui_frame_snapshot_module.addImport("r4os", sdk.createR4osModule(test_target, test_optimize));
    const gui_frame_snapshot_tests = b.addTest(.{
        .root_module = gui_frame_snapshot_module,
    });
    const window_service_gate_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("Tests/Original/window_service_gate.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const tray_module = b.createModule(.{
        .root_source_file = b.path("Tests/Original/tray.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    tray_module.addImport("r4os", sdk.createR4osModule(test_target, test_optimize));
    const tray_tests = b.addTest(.{ .root_module = tray_module });
    const physical_input_module = b.createModule(.{
        .root_source_file = b.path("Tests/Original/physical_input.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    physical_input_module.addImport("r4os", sdk.createR4osModule(test_target, test_optimize));
    for ([_]*std.Build.Module{ draw_module, compositor_module, paint_module, scene_buffer_tests.root_module, wallpaper_module, gui_shape_renderer_module }) |module| {
        module.addImport("r4gfx_desktop_outputs", topology);
        module.addImport("desktop_check", check_wire);
    }
    const physical_input_tests = b.addTest(.{ .root_module = physical_input_module });

    const run_model_tests = b.addRunArtifact(model_tests);
    const run_compositor_tests = b.addRunArtifact(compositor_tests);
    const run_window_tests = b.addRunArtifact(window_tests);
    const run_surface_tests = b.addRunArtifact(surface_tests);
    const run_paint_tests = b.addRunArtifact(paint_tests);
    const run_draw_tests = b.addRunArtifact(draw_tests);
    const run_gui_shape_renderer_tests = b.addRunArtifact(gui_shape_renderer_tests);
    const run_scene_buffer_tests = b.addRunArtifact(scene_buffer_tests);
    const run_start_menu_tests = b.addRunArtifact(start_menu_tests);
    const run_desktop_config_tests = b.addRunArtifact(desktop_config_tests);
    const run_desktop_folder_tests = b.addRunArtifact(desktop_folder_tests);
    const run_desktop_layout_tests = b.addRunArtifact(desktop_layout_tests);
    const run_desktop_items_tests = b.addRunArtifact(desktop_items_tests);
    const run_quick_launch_tests = b.addRunArtifact(quick_launch_tests);
    const run_icon_source_tests = b.addRunArtifact(icon_source_tests);
    const run_text_field_tests = b.addRunArtifact(text_field_tests);
    const run_run_command_tests = b.addRunArtifact(run_command_tests);
    text_field_tests.root_module.addImport("r4os", host_r4os);
    run_command_tests.root_module.addImport("r4os", host_r4os);
    input_step.dependOn(&run_text_field_tests.step);
    input_step.dependOn(&run_run_command_tests.step);
    console_scroll_tests.root_module.addImport("r4std_console_text", console_text);
    const run_console_scroll_tests = b.addRunArtifact(console_scroll_tests);
    const run_appearance_signal_tests = b.addRunArtifact(appearance_signal_tests);
    const run_appearance_colors_tests = b.addRunArtifact(appearance_colors_tests);
    const run_wallpaper_tests = b.addRunArtifact(wallpaper_tests);
    const run_gui_frame_snapshot_tests = b.addRunArtifact(gui_frame_snapshot_tests);
    const run_window_service_gate_tests = b.addRunArtifact(window_service_gate_tests);
    const run_tray_tests = b.addRunArtifact(tray_tests);
    const run_physical_input_tests = b.addRunArtifact(physical_input_tests);
    const render_step = b.step("render-test", "Run the existing compositor and its imported painter checks");
    render_step.dependOn(&run_compositor_tests.step);
    render_step.dependOn(&run_wallpaper_tests.step);
    const test_step = b.step("test", "Run Desktop Zig tests");
    test_step.dependOn(&capture_tests.step);
    test_step.dependOn(&input_tests.step);
    test_step.dependOn(&run_model_tests.step);
    test_step.dependOn(&run_window_tests.step);
    test_step.dependOn(&run_surface_tests.step);
    test_step.dependOn(&run_paint_tests.step);
    test_step.dependOn(&run_draw_tests.step);
    test_step.dependOn(&run_gui_shape_renderer_tests.step);
    test_step.dependOn(&run_scene_buffer_tests.step);
    test_step.dependOn(&run_compositor_tests.step);
    test_step.dependOn(&run_start_menu_tests.step);
    test_step.dependOn(&run_desktop_config_tests.step);
    test_step.dependOn(&run_desktop_folder_tests.step);
    test_step.dependOn(&run_desktop_layout_tests.step);
    test_step.dependOn(&run_desktop_items_tests.step);
    test_step.dependOn(&run_quick_launch_tests.step);
    test_step.dependOn(&run_icon_source_tests.step);
    test_step.dependOn(&run_text_field_tests.step);
    test_step.dependOn(&run_run_command_tests.step);
    test_step.dependOn(&run_console_scroll_tests.step);
    test_step.dependOn(&run_appearance_signal_tests.step);
    test_step.dependOn(&run_appearance_colors_tests.step);
    test_step.dependOn(&run_wallpaper_tests.step);
    test_step.dependOn(&run_gui_frame_snapshot_tests.step);
    test_step.dependOn(&run_window_service_gate_tests.step);
    test_step.dependOn(&run_tray_tests.step);
    test_step.dependOn(&run_physical_input_tests.step);
}
