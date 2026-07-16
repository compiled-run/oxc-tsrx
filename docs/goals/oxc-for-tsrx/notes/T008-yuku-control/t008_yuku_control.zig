const std = @import("std");
const parser = @import("parser");

const Input = struct {
    path: []const u8,
    source: []const u8,
};

const Sample = struct {
    elapsed_ns: u64,
    valid_files: usize,
    valid_bytes: usize,
    output_bytes: usize,
    failures: usize,
};

fn formatOwned(allocator: std.mem.Allocator, source: []const u8) !?[]u8 {
    var tree = try parser.parse(allocator, source, .{
        .lang = .tsrx,
        .comments = .both,
    });
    defer tree.deinit();
    if (tree.hasErrors()) return null;

    const result = try parser.codegen.print(allocator, &tree, .{
        .format = .pretty,
        .indent = 2,
        .quotes = .preserve,
        .comments = .all,
    });
    defer result.deinit(allocator);
    if (result.errors.len > 0) return null;
    return try allocator.dupe(u8, result.code);
}

fn runSample(io: std.Io, allocator: std.mem.Allocator, inputs: []const Input) !Sample {
    const started = std.Io.Clock.awake.now(io).nanoseconds;
    var valid_files: usize = 0;
    var valid_bytes: usize = 0;
    var output_bytes: usize = 0;
    var failures: usize = 0;

    for (inputs) |input| {
        const output = try formatOwned(allocator, input.source) orelse {
            failures += 1;
            continue;
        };
        defer allocator.free(output);
        valid_files += 1;
        valid_bytes += input.source.len;
        output_bytes += output.len;
    }

    const ended = std.Io.Clock.awake.now(io).nanoseconds;
    return .{
        .elapsed_ns = @intCast(ended - started),
        .valid_files = valid_files,
        .valid_bytes = valid_bytes,
        .output_bytes = output_bytes,
        .failures = failures,
    };
}

fn sort(values: []u64) void {
    for (1..values.len) |index| {
        const value = values[index];
        var cursor = index;
        while (cursor > 0 and values[cursor - 1] > value) : (cursor -= 1) {
            values[cursor] = values[cursor - 1];
        }
        values[cursor] = value;
    }
}

pub fn main(init: std.process.Init) !void {
    const arena = init.arena.allocator();
    const allocator = std.heap.smp_allocator;
    const args = try init.minimal.args.toSlice(arena);
    if (args.len < 2) return error.MissingInputFiles;

    if (args.len == 3 and std.mem.eql(u8, args[1], "--print")) {
        const source = try std.Io.Dir.cwd().readFileAlloc(
            init.io,
            args[2],
            arena,
            .limited(64 * 1024 * 1024),
        );
        const output = try formatOwned(allocator, source) orelse return error.FormatFailed;
        defer allocator.free(output);
        var print_buffer: [4096]u8 = undefined;
        var print_file_writer: std.Io.File.Writer = .init(.stdout(), init.io, &print_buffer);
        try print_file_writer.interface.writeAll(output);
        try print_file_writer.interface.flush();
        return;
    }

    var inputs: std.ArrayList(Input) = .empty;
    defer inputs.deinit(allocator);
    for (args[1..]) |path| {
        const source = try std.Io.Dir.cwd().readFileAlloc(
            init.io,
            path,
            arena,
            .limited(64 * 1024 * 1024),
        );
        try inputs.append(allocator, .{ .path = path, .source = source });
    }

    _ = try runSample(init.io, allocator, inputs.items);
    var samples: [5]Sample = undefined;
    var times: [5]u64 = undefined;
    for (&samples, 0..) |*sample, index| {
        sample.* = try runSample(init.io, allocator, inputs.items);
        times[index] = sample.elapsed_ns;
    }
    sort(&times);
    const median_ns = times[2];
    const representative = for (samples) |sample| {
        if (sample.elapsed_ns == median_ns) break sample;
    } else unreachable;

    var non_idempotent: usize = 0;
    var validation_failures: usize = 0;
    for (inputs.items) |input| {
        const first = try formatOwned(allocator, input.source) orelse {
            validation_failures += 1;
            std.debug.print("validation failure: {s}\n", .{input.path});
            continue;
        };
        defer allocator.free(first);
        const second = try formatOwned(allocator, first) orelse {
            non_idempotent += 1;
            std.debug.print("second-pass failure: {s}\n", .{input.path});
            continue;
        };
        defer allocator.free(second);
        if (!std.mem.eql(u8, first, second)) {
            non_idempotent += 1;
            std.debug.print("second-pass difference: {s}\n", .{input.path});
        }
    }

    var stdout_buffer: [4096]u8 = undefined;
    var stdout_file_writer: std.Io.File.Writer = .init(.stdout(), init.io, &stdout_buffer);
    const out = &stdout_file_writer.interface;
    try out.print(
        \\{{"files":{d},"validFiles":{d},"validBytes":{d},"outputBytes":{d},"failures":{d},"medianMs":{d:.6},"throughputMiBs":{d:.6},"nonIdempotent":{d},"validationFailures":{d},"samplesNs":[{d},{d},{d},{d},{d}]}}
        \\
    , .{
        inputs.items.len,
        representative.valid_files,
        representative.valid_bytes,
        representative.output_bytes,
        representative.failures,
        @as(f64, @floatFromInt(median_ns)) / 1_000_000.0,
        (@as(f64, @floatFromInt(representative.valid_bytes)) / 1024.0 / 1024.0) /
            (@as(f64, @floatFromInt(median_ns)) / 1_000_000_000.0),
        non_idempotent,
        validation_failures,
        samples[0].elapsed_ns,
        samples[1].elapsed_ns,
        samples[2].elapsed_ns,
        samples[3].elapsed_ns,
        samples[4].elapsed_ns,
    });
    try out.flush();
}
