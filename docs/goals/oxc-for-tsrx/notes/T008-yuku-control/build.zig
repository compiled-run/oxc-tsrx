const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{
        .preferred_optimize_mode = .ReleaseFast,
    });

    const util_module = b.createModule(.{
        .root_source_file = b.path("src/util/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    const codegen_options = b.addOptions();
    codegen_options.addOption(bool, "source_maps", false);

    const parser_module = b.createModule(.{
        .root_source_file = b.path("src/parser/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    parser_module.addImport("util", util_module);
    parser_module.addImport("codegen_options", codegen_options.createModule());

    const benchmark_module = b.createModule(.{
        .root_source_file = b.path("src/t008_yuku_control.zig"),
        .target = target,
        .optimize = optimize,
    });
    benchmark_module.addImport("parser", parser_module);

    const executable = b.addExecutable(.{
        .name = "t008-yuku-control",
        .root_module = benchmark_module,
    });
    b.installArtifact(executable);

    const run = b.addRunArtifact(executable);
    run.addArgs(b.args orelse &.{});
    const run_step = b.step("run", "Run direct TSRX parse-to-print control");
    run_step.dependOn(&run.step);
}
