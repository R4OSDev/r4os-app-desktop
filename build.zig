const std = @import("std");

pub fn build(b: *std.Build) void {
    // Eigenstaendiger Bau aus dem Manifest, seit 0.61.8. Die Hosttests
    // darunter bleiben unveraendert - hier kommt etwas dazu, nichts weg.
    const sdk_build = b.lazyImport(@This(), "r4os_sdk") orelse return;
    const sdk_dep = b.dependencyFromBuildZig(sdk_build, .{});
    const libraries_build = b.lazyImport(@This(), "r4os_libraries") orelse return;
    const libraries_dep = b.dependencyFromBuildZig(libraries_build, .{});
    const sdk = sdk_build.sdk(b, sdk_dep, .{});
    _ = sdk.addR4MFWithOptions(b.path("module.R4MF"), .{
        .zig_module_roots = &.{
            libraries_dep.namedLazyPath("r4std_zig_binding"),
            libraries_dep.namedLazyPath("r4img_zig_binding"),
        },
    });

    const test_target = b.graph.host;
    const test_optimize: std.builtin.OptimizeMode = .Debug;
    const host_r4os = sdk.createR4osModule(test_target, test_optimize);
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

    const model_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/model.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const window_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/window.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const surface_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/surface.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const paint_module = b.createModule(.{
        .root_source_file = b.path("src/paint.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    paint_module.addImport("r4os", sdk.createR4osModule(test_target, test_optimize));
    const paint_tests = b.addTest(.{ .root_module = paint_module });
    const draw_module = b.createModule(.{
        .root_source_file = b.path("src/draw.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    draw_module.addImport("r4os", sdk.createR4osModule(test_target, test_optimize));
    const draw_tests = b.addTest(.{ .root_module = draw_module });
    const gui_shape_renderer_module = b.createModule(.{
        .root_source_file = b.path("src/gui_shape_renderer.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    gui_shape_renderer_module.addImport("r4os", sdk.createR4osModule(test_target, test_optimize));
    const gui_shape_renderer_tests = b.addTest(.{ .root_module = gui_shape_renderer_module });
    const scene_buffer_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/scene_buffer.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const start_menu_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/start_menu.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const desktop_config_module = b.createModule(.{
        .root_source_file = b.path("src/desktop_config.zig"),
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
        .root_source_file = b.path("src/desktop_folder.zig"),
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
        .root_source_file = b.path("src/desktop_layout.zig"),
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
        .root_source_file = b.path("src/desktop_items.zig"),
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
            .root_source_file = b.path("src/icon_source.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const quick_launch_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/quick_launch.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const text_field_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/text_field.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const run_command_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/run_command.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const console_scroll_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/console_scroll.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const appearance_signal_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/appearance_signal.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const appearance_colors_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/appearance_colors.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const wallpaper_module = b.createModule(.{
        .root_source_file = b.path("src/wallpaper.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    wallpaper_module.addImport("r4os", host_r4os);
    wallpaper_module.addImport("r4img", r4img);
    const wallpaper_tests = b.addTest(.{
        .root_module = wallpaper_module,
    });
    const gui_frame_snapshot_module = b.createModule(.{
        .root_source_file = b.path("src/gui_frame_snapshot.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    gui_frame_snapshot_module.addImport("r4os", sdk.createR4osModule(test_target, test_optimize));
    const gui_frame_snapshot_tests = b.addTest(.{
        .root_module = gui_frame_snapshot_module,
    });
    const window_service_gate_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/window_service_gate.zig"),
            .target = test_target,
            .optimize = test_optimize,
        }),
    });
    const tray_module = b.createModule(.{
        .root_source_file = b.path("src/tray.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    tray_module.addImport("r4os", sdk.createR4osModule(test_target, test_optimize));
    const tray_tests = b.addTest(.{ .root_module = tray_module });
    const physical_input_module = b.createModule(.{
        .root_source_file = b.path("src/physical_input.zig"),
        .target = test_target,
        .optimize = test_optimize,
    });
    physical_input_module.addImport("r4os", sdk.createR4osModule(test_target, test_optimize));
    const physical_input_tests = b.addTest(.{ .root_module = physical_input_module });

    const run_model_tests = b.addRunArtifact(model_tests);
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
    const run_console_scroll_tests = b.addRunArtifact(console_scroll_tests);
    const run_appearance_signal_tests = b.addRunArtifact(appearance_signal_tests);
    const run_appearance_colors_tests = b.addRunArtifact(appearance_colors_tests);
    const run_wallpaper_tests = b.addRunArtifact(wallpaper_tests);
    const run_gui_frame_snapshot_tests = b.addRunArtifact(gui_frame_snapshot_tests);
    const run_window_service_gate_tests = b.addRunArtifact(window_service_gate_tests);
    const run_tray_tests = b.addRunArtifact(tray_tests);
    const run_physical_input_tests = b.addRunArtifact(physical_input_tests);
    const test_step = b.step("test", "Run Desktop Zig tests");
    test_step.dependOn(&run_model_tests.step);
    test_step.dependOn(&run_window_tests.step);
    test_step.dependOn(&run_surface_tests.step);
    test_step.dependOn(&run_paint_tests.step);
    test_step.dependOn(&run_draw_tests.step);
    test_step.dependOn(&run_gui_shape_renderer_tests.step);
    test_step.dependOn(&run_scene_buffer_tests.step);
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
