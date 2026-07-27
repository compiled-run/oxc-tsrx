// Benchmark math intentionally converts bounded byte/nanosecond counters to floating point for
// readable rates. The harness is release-only and retains every raw latency/RSS sample.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::struct_field_names,
    clippy::too_many_lines
)]

use std::{
    env,
    fmt::Write as _,
    fs,
    hint::black_box,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use memory_stats::memory_stats;
use oxc_adapter::{FormatRequest, OXC_REVISION, SourceKind};
use serde::{Deserialize, Serialize};
use tsrx_format::{FormatMode, FormatSession, format_text};
use tsrx_syntax::{project, scan};

const MEBIBYTE: f64 = 1_048_576.0;
const HISTORICAL_INCUMBENT_MIB_S: f64 = 1.66;

#[derive(Debug)]
struct Args {
    budget_path: PathBuf,
    output_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Budgets {
    schema_version: u32,
    warmups: usize,
    samples: usize,
    generalized_control_warmups: usize,
    generalized_control_samples: usize,
    batch_warmups: usize,
    batch_samples: usize,
    cold_process_samples: usize,
    rss_process_samples: usize,
    corpus_target_bytes: usize,
    generalized_control_target_bytes: usize,
    batch_corpus_target_bytes: usize,
    memory_corpus_target_bytes: usize,
    candidate_binary: PathBuf,
    stock_oxfmt_binary: PathBuf,
    p04: P04Budget,
    p05: P05Budget,
    p07: P07Budget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct P04Budget {
    direct_median_latency_ratio_max: f64,
    direct_p95_latency_ratio_max: f64,
    sequential_median_mib_per_second_min: f64,
    sequential_p95_mib_per_second_min: f64,
    historical_incumbent_derived_mib_per_second_min: f64,
    default_thread_mib_per_second_min: f64,
    generalized_control_median_mib_per_second_min: f64,
    generalized_control_p95_mib_per_second_min: f64,
    generalized_control_scaling_ratio_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct P05Budget {
    stdin_p95_ms_max: f64,
    upstream_latency_ratio_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct P07Budget {
    upstream_ratio_max: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Host {
    os: &'static str,
    architecture: &'static str,
    rustc: String,
    system: String,
    build_profile: &'static str,
    oxc_revision: &'static str,
    stock_oxfmt_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    bytes: usize,
    equivalent_tsx_bytes: usize,
    fnv1a64: String,
    structural_forms: [&'static str; 3],
    note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeneralizedControlCorpus {
    bytes: usize,
    half_bytes: usize,
    fnv1a64: String,
    structural_forms: [&'static str; 10],
    dynamic_tag_count: usize,
    style_payload_count: usize,
    note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Distribution {
    samples: usize,
    p50_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    median_mib_per_second: f64,
    p95_mib_per_second: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhaseDistribution {
    samples: usize,
    p50_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FormatPhaseTimings {
    scan: PhaseDistribution,
    projection: PhaseDistribution,
    parse: PhaseDistribution,
    format: PhaseDistribution,
    lift: PhaseDistribution,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct P04Summary {
    canonical_direct_control: Distribution,
    candidate_standard_direct: Distribution,
    direct_median_latency_ratio: f64,
    direct_p95_latency_ratio: f64,
    direct_output_parity: bool,
    direct_bypass: bool,
    candidate_tsrx_sequential: Distribution,
    candidate_tsrx_phase_timings: FormatPhaseTimings,
    historical_incumbent_baseline_mib_per_second: f64,
    historical_incumbent_derived_floor_mib_per_second: f64,
    candidate_default_thread_batch: Distribution,
    generalized_control: GeneralizedControlSummary,
    idempotent: bool,
    parse_count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeneralizedControlSummary {
    candidate_generalized_control: Distribution,
    candidate_generalized_control_half: Distribution,
    generalized_control_scaling_ratio: f64,
    generalized_control_idempotent: bool,
    generalized_control_parse_count: u32,
    generalized_control_embedded_parse_count: u32,
    generalized_control_embedded_format_ns: u64,
    generalized_control_style_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct P05Summary {
    candidate_tsrx_stdin: Distribution,
    stock_oxfmt_tsx_stdin: Distribution,
    p95_latency_ratio: f64,
    fresh_processes: bool,
    complete_output_produced: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct P07Summary {
    candidate_tsrx_rss_bytes: Vec<u64>,
    canonical_tsx_rss_bytes: Vec<u64>,
    candidate_tsrx_median_rss_bytes: u64,
    canonical_tsx_median_rss_bytes: u64,
    rss_ratio: f64,
    measurement: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigSessionSummary {
    config_loads: u32,
    config_load_ns: u64,
    files: u32,
    parse_count: u32,
    options_applied: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawSamples {
    direct_control_before_ns: Vec<u64>,
    direct_control_after_ns: Vec<u64>,
    candidate_standard_ns: Vec<u64>,
    candidate_tsrx_sequential_ns: Vec<u64>,
    candidate_tsrx_scan_ns: Vec<u64>,
    candidate_tsrx_projection_ns: Vec<u64>,
    candidate_tsrx_parse_ns: Vec<u64>,
    candidate_tsrx_format_ns: Vec<u64>,
    candidate_tsrx_lift_ns: Vec<u64>,
    candidate_generalized_control_ns: Vec<u64>,
    candidate_generalized_control_half_ns: Vec<u64>,
    candidate_batch_ns: Vec<u64>,
    candidate_stdin_ns: Vec<u64>,
    stock_stdin_ns: Vec<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Assertion {
    name: &'static str,
    comparison: &'static str,
    observed: f64,
    threshold: f64,
    pass: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema_version: u32,
    generated_at_unix_ms: u128,
    host: Host,
    budgets: Budgets,
    corpus: Corpus,
    generalized_control_corpus: GeneralizedControlCorpus,
    config_session: ConfigSessionSummary,
    p04: P04Summary,
    p05: P05Summary,
    p07: P07Summary,
    raw_samples: RawSamples,
    assertions: Vec<Assertion>,
    passed: bool,
    limitations: [&'static str; 3],
}

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.first().is_some_and(|value| value == "--memory-child") {
        let result = run_memory_child(&arguments);
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("oxc-tsrx format memory child: {error}");
                ExitCode::FAILURE
            }
        };
    }
    match run(arguments.into_iter()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("oxc-tsrx format benchmark: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: impl Iterator<Item = String>) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("performance gates require a release build".to_string());
    }
    let args = parse_args(arguments)?;
    let budget_source = fs::read_to_string(&args.budget_path).map_err(|error| {
        format!("unable to read budgets {}: {error}", args.budget_path.display())
    })?;
    let budgets: Budgets = serde_json::from_str(&budget_source)
        .map_err(|error| format!("invalid formatter benchmark budgets: {error}"))?;
    validate_budgets(&budgets)?;
    ensure_binary(&budgets.candidate_binary, "candidate formatter")?;
    ensure_binary(&budgets.stock_oxfmt_binary, "stock Oxfmt")?;

    let tsrx = build_corpus(budgets.corpus_target_bytes);
    let overlay = scan(&tsrx).map_err(|error| error.to_string())?;
    let tsx = project(&tsrx, &overlay).map_err(|error| error.to_string())?;
    let generalized_control =
        build_generalized_control_corpus(budgets.generalized_control_target_bytes);
    let generalized_control_half =
        build_generalized_control_corpus(budgets.generalized_control_target_bytes / 2);
    let generalized_overlay = scan(&generalized_control).map_err(|error| error.to_string())?;
    let generalized_dynamic_tag_count = generalized_overlay.dynamic_tag_count();
    let generalized_style_payload_count = generalized_overlay.style_block_count();
    let tsrx_path = Path::new("benchmark.tsrx");
    let tsx_path = Path::new("benchmark.tsx");

    for _ in 0..budgets.warmups {
        black_box(measure_control(&tsx)?);
        black_box(measure_product(tsx_path, &tsx)?);
        black_box(measure_product(tsrx_path, &tsrx)?);
        black_box(measure_control(&tsx)?);
    }

    let mut control_before = Vec::with_capacity(budgets.samples);
    let mut control_after = Vec::with_capacity(budgets.samples);
    let mut standard = Vec::with_capacity(budgets.samples);
    let mut projected = Vec::with_capacity(budgets.samples);
    for _ in 0..budgets.samples {
        control_before.push(measure_control(&tsx)?);
        standard.push(measure_product(tsx_path, &tsx)?);
        projected.push(measure_product(tsrx_path, &tsrx)?);
        control_after.push(measure_control(&tsx)?);
    }

    for _ in 0..budgets.generalized_control_warmups {
        black_box(measure_product(tsrx_path, &generalized_control_half)?);
        black_box(measure_product(tsrx_path, &generalized_control)?);
    }
    let mut generalized_control_samples = Vec::with_capacity(budgets.generalized_control_samples);
    let mut generalized_control_half_samples =
        Vec::with_capacity(budgets.generalized_control_samples);
    for sample in 0..budgets.generalized_control_samples {
        if sample % 2 == 0 {
            generalized_control_half_samples
                .push(measure_product(tsrx_path, &generalized_control_half)?);
            generalized_control_samples.push(measure_product(tsrx_path, &generalized_control)?);
        } else {
            generalized_control_samples.push(measure_product(tsrx_path, &generalized_control)?);
            generalized_control_half_samples
                .push(measure_product(tsrx_path, &generalized_control_half)?);
        }
    }

    let expected = &control_before[0].1;
    let direct_output_parity =
        control_before.iter().chain(&control_after).all(|sample| sample.1 == *expected)
            && standard.iter().all(|sample| sample.1 == *expected);
    let direct_bypass = standard.iter().all(|sample| {
        let metadata = &sample.2;
        metadata.mode == FormatMode::Direct
            && metadata.timings.scan_ns == 0
            && metadata.timings.projection_ns == 0
            && metadata.timings.lift_ns == 0
            && metadata.projection_bytes == 0
    });
    let parse_count = projected[0].2.parse_count;
    let first_code = &projected[0].1;
    let idempotent = format_text(tsrx_path, first_code)
        .is_ok_and(|second| second.code == *first_code && !second.changed);
    let generalized_control_parse_count = generalized_control_samples[0].2.parse_count;
    let generalized_control_embedded_parse_count =
        generalized_control_samples[0].2.embedded_parse_count;
    let generalized_control_embedded_format_ns =
        generalized_control_samples[0].2.timings.embedded_format_ns;
    let generalized_control_style_count = generalized_control_samples[0].2.style_count;
    let generalized_control_code = &generalized_control_samples[0].1;
    let generalized_control_idempotent = format_text(tsrx_path, generalized_control_code)
        .is_ok_and(|second| second.code == *generalized_control_code && !second.changed);

    let temporary = create_temp_directory()?;
    let config_session = measure_config_session(&temporary.join("config-session"))?;
    let process_result = run_process_measurements(&temporary, &tsrx, &tsx, &budgets);
    let cleanup_result = fs::remove_dir_all(&temporary)
        .map_err(|error| format!("unable to remove {}: {error}", temporary.display()));
    let processes = process_result?;
    cleanup_result?;

    let control_ns =
        control_before.iter().chain(&control_after).map(|sample| sample.0).collect::<Vec<_>>();
    let standard_ns = standard.iter().map(|sample| sample.0).collect::<Vec<_>>();
    let projected_ns = projected.iter().map(|sample| sample.0).collect::<Vec<_>>();
    let projected_scan_ns =
        projected.iter().map(|sample| sample.2.timings.scan_ns).collect::<Vec<_>>();
    let projected_projection_ns =
        projected.iter().map(|sample| sample.2.timings.projection_ns).collect::<Vec<_>>();
    let projected_parse_ns =
        projected.iter().map(|sample| sample.2.timings.parse_ns).collect::<Vec<_>>();
    let projected_format_ns =
        projected.iter().map(|sample| sample.2.timings.format_ns).collect::<Vec<_>>();
    let projected_lift_ns =
        projected.iter().map(|sample| sample.2.timings.lift_ns).collect::<Vec<_>>();
    let generalized_control_ns =
        generalized_control_samples.iter().map(|sample| sample.0).collect::<Vec<_>>();
    let generalized_control_half_ns =
        generalized_control_half_samples.iter().map(|sample| sample.0).collect::<Vec<_>>();
    let control_distribution = distribution(&control_ns, tsx.len());
    let standard_distribution = distribution(&standard_ns, tsx.len());
    let projected_distribution = distribution(&projected_ns, tsrx.len());
    let projected_phase_timings = FormatPhaseTimings {
        scan: phase_distribution(&projected_scan_ns),
        projection: phase_distribution(&projected_projection_ns),
        parse: phase_distribution(&projected_parse_ns),
        format: phase_distribution(&projected_format_ns),
        lift: phase_distribution(&projected_lift_ns),
    };
    let generalized_control_distribution =
        distribution(&generalized_control_ns, generalized_control.len());
    let generalized_control_half_distribution =
        distribution(&generalized_control_half_ns, generalized_control_half.len());
    let generalized_control_scaling_ratio = ratio(
        generalized_control_distribution.p50_ns,
        generalized_control_half_distribution.p50_ns,
    ) / (generalized_control.len() as f64
        / generalized_control_half.len() as f64);
    let batch_distribution = distribution(&processes.batch_ns, processes.batch_bytes);
    let candidate_stdin_distribution = distribution(&processes.candidate_stdin_ns, tsrx.len());
    let stock_stdin_distribution = distribution(&processes.stock_stdin_ns, tsx.len());
    let direct_median_latency_ratio =
        ratio(standard_distribution.p50_ns, control_distribution.p50_ns);
    let direct_p95_latency_ratio = ratio(standard_distribution.p95_ns, control_distribution.p95_ns);
    let historical_incumbent_derived_floor_mib_per_second = HISTORICAL_INCUMBENT_MIB_S * 10.0;
    let stdin_ratio = ratio(candidate_stdin_distribution.p95_ns, stock_stdin_distribution.p95_ns);
    let tsrx_rss = median_u64(&processes.tsrx_rss_bytes);
    let tsx_rss = median_u64(&processes.tsx_rss_bytes);
    let rss_ratio = ratio(tsrx_rss, tsx_rss);

    let mut assertions = Vec::new();
    assert_max(
        &mut assertions,
        "p04_direct_median_ratio",
        "candidate standard median / canonical formatter median",
        direct_median_latency_ratio,
        budgets.p04.direct_median_latency_ratio_max,
    );
    assert_max(
        &mut assertions,
        "p04_direct_p95_ratio",
        "candidate standard p95 / canonical formatter p95",
        direct_p95_latency_ratio,
        budgets.p04.direct_p95_latency_ratio_max,
    );
    assert_bool(&mut assertions, "p04_direct_output_parity", direct_output_parity);
    assert_bool(&mut assertions, "p04_direct_bypass", direct_bypass);
    assert_min(
        &mut assertions,
        "p04_sequential_median_mib_s",
        "complete TSRX format median throughput",
        projected_distribution.median_mib_per_second,
        budgets.p04.sequential_median_mib_per_second_min,
    );
    assert_min(
        &mut assertions,
        "p04_sequential_p95_mib_s",
        "complete TSRX format throughput at p95 latency",
        projected_distribution.p95_mib_per_second,
        budgets.p04.sequential_p95_mib_per_second_min,
    );
    assert_min(
        &mut assertions,
        "p04_historical_incumbent_derived_floor_mib_s",
        "candidate median throughput / absolute 16.6 MiB/s floor derived from 10 x a non-comparable historical 1.66 MiB/s incumbent result",
        projected_distribution.median_mib_per_second,
        budgets.p04.historical_incumbent_derived_mib_per_second_min,
    );
    assert_min(
        &mut assertions,
        "p04_default_thread_mib_s",
        "complete multi-file --check throughput at p95 latency",
        batch_distribution.p95_mib_per_second,
        budgets.p04.default_thread_mib_per_second_min,
    );
    assert_min(
        &mut assertions,
        "p04_generalized_control_median_mib_s",
        "complete nested/control/dynamic/style format median throughput",
        generalized_control_distribution.median_mib_per_second,
        budgets.p04.generalized_control_median_mib_per_second_min,
    );
    assert_min(
        &mut assertions,
        "p04_generalized_control_p95_mib_s",
        "complete nested/control/dynamic/style format throughput at p95 latency",
        generalized_control_distribution.p95_mib_per_second,
        budgets.p04.generalized_control_p95_mib_per_second_min,
    );
    assert_max(
        &mut assertions,
        "p04_generalized_control_linear_scaling",
        "full/half median latency normalized by corpus byte ratio",
        generalized_control_scaling_ratio,
        budgets.p04.generalized_control_scaling_ratio_max,
    );
    assert_bool(
        &mut assertions,
        "p04_generalized_control_idempotent",
        generalized_control_idempotent,
    );
    assert_bool(
        &mut assertions,
        "p04_generalized_control_one_parse",
        generalized_control_parse_count == 1,
    );
    assert_bool(
        &mut assertions,
        "p04_generalized_dynamic_style_coverage",
        generalized_dynamic_tag_count > 0 && generalized_style_payload_count > 0,
    );
    assert_bool(
        &mut assertions,
        "p04_generalized_style_metadata",
        generalized_control_style_count == generalized_style_payload_count,
    );
    assert_bool(
        &mut assertions,
        "p04_generalized_no_hidden_embedded_parse",
        generalized_control_embedded_parse_count == 0
            && generalized_control_embedded_format_ns == 0,
    );
    assert_bool(&mut assertions, "p04_idempotent", idempotent);
    assert_bool(&mut assertions, "p04_one_parse", parse_count == 1);
    assert_bool(
        &mut assertions,
        "p04_config_compiled_once",
        config_session.config_loads == 1 && config_session.config_load_ns > 0,
    );
    assert_bool(
        &mut assertions,
        "p04_config_one_parse_per_file",
        config_session.files == 2 && config_session.parse_count == config_session.files,
    );
    assert_bool(&mut assertions, "p04_config_options_applied", config_session.options_applied);
    assert_max(
        &mut assertions,
        "p05_stdin_p95_ms",
        "fresh candidate TSRX stdin p95 milliseconds",
        candidate_stdin_distribution.p95_ms,
        budgets.p05.stdin_p95_ms_max,
    );
    assert_max(
        &mut assertions,
        "p05_upstream_ratio",
        "candidate TSRX stdin p95 / stock Oxfmt TSX stdin p95",
        stdin_ratio,
        budgets.p05.upstream_latency_ratio_max,
    );
    assert_bool(&mut assertions, "p05_complete_output", processes.complete_output);
    assert_max(
        &mut assertions,
        "p07_rss_ratio",
        "candidate TSRX RSS / canonical TSX RSS",
        rss_ratio,
        budgets.p07.upstream_ratio_max,
    );

    let generated_at_unix_ms = now_millis()?;
    let passed = assertions.iter().all(|assertion| assertion.pass);
    let report = Report {
        schema_version: budgets.schema_version,
        generated_at_unix_ms,
        host: Host {
            os: env::consts::OS,
            architecture: env::consts::ARCH,
            rustc: command_text("rustc", &["--version"]),
            system: command_text("uname", &["-a"]),
            build_profile: "release (codegen-units=1, thin LTO, panic=abort, stripped)",
            oxc_revision: OXC_REVISION,
            stock_oxfmt_version: command_path_text(&budgets.stock_oxfmt_binary, &["--version"]),
        },
        budgets: budgets.clone(),
        corpus: Corpus {
            bytes: tsrx.len(),
            equivalent_tsx_bytes: tsx.len(),
            fnv1a64: format!("{:016x}", fnv1a64(tsrx.as_bytes())),
            structural_forms: ["@{", "@if", "@else"],
            note: "Retained statement-control corpus for comparison with earlier formatter reports.",
        },
        generalized_control_corpus: GeneralizedControlCorpus {
            bytes: generalized_control.len(),
            half_bytes: generalized_control_half.len(),
            fnv1a64: format!("{:016x}", fnv1a64(generalized_control.as_bytes())),
            structural_forms: [
                "@{",
                "@if",
                "@else",
                "@for",
                "@empty",
                "index/key",
                "@switch/@case/@default",
                "@try/@pending/@catch",
                "<{dynamic}>",
                "<style>",
            ],
            dynamic_tag_count: generalized_dynamic_tag_count,
            style_payload_count: generalized_style_payload_count,
            note: "Repeated direct JSX-child, nested, expression, annotated loop, switch, source-order try, nested dynamic-tag, and raw-style forms.",
        },
        config_session,
        p04: P04Summary {
            canonical_direct_control: control_distribution,
            candidate_standard_direct: standard_distribution,
            direct_median_latency_ratio,
            direct_p95_latency_ratio,
            direct_output_parity,
            direct_bypass,
            candidate_tsrx_sequential: projected_distribution,
            candidate_tsrx_phase_timings: projected_phase_timings,
            historical_incumbent_baseline_mib_per_second: HISTORICAL_INCUMBENT_MIB_S,
            historical_incumbent_derived_floor_mib_per_second,
            candidate_default_thread_batch: batch_distribution,
            generalized_control: GeneralizedControlSummary {
                candidate_generalized_control: generalized_control_distribution,
                candidate_generalized_control_half: generalized_control_half_distribution,
                generalized_control_scaling_ratio,
                generalized_control_idempotent,
                generalized_control_parse_count,
                generalized_control_embedded_parse_count,
                generalized_control_embedded_format_ns,
                generalized_control_style_count,
            },
            idempotent,
            parse_count,
        },
        p05: P05Summary {
            candidate_tsrx_stdin: candidate_stdin_distribution,
            stock_oxfmt_tsx_stdin: stock_stdin_distribution,
            p95_latency_ratio: stdin_ratio,
            fresh_processes: true,
            complete_output_produced: processes.complete_output,
        },
        p07: P07Summary {
            candidate_tsrx_rss_bytes: processes.tsrx_rss_bytes,
            canonical_tsx_rss_bytes: processes.tsx_rss_bytes,
            candidate_tsrx_median_rss_bytes: tsrx_rss,
            canonical_tsx_median_rss_bytes: tsx_rss,
            rss_ratio,
            measurement: "fresh benchmark child after complete in-memory formatted output production",
        },
        raw_samples: RawSamples {
            direct_control_before_ns: control_before.iter().map(|sample| sample.0).collect(),
            direct_control_after_ns: control_after.iter().map(|sample| sample.0).collect(),
            candidate_standard_ns: standard_ns,
            candidate_tsrx_sequential_ns: projected_ns,
            candidate_tsrx_scan_ns: projected_scan_ns,
            candidate_tsrx_projection_ns: projected_projection_ns,
            candidate_tsrx_parse_ns: projected_parse_ns,
            candidate_tsrx_format_ns: projected_format_ns,
            candidate_tsrx_lift_ns: projected_lift_ns,
            candidate_generalized_control_ns: generalized_control_ns,
            candidate_generalized_control_half_ns: generalized_control_half_ns,
            candidate_batch_ns: processes.batch_ns,
            candidate_stdin_ns: processes.candidate_stdin_ns,
            stock_stdin_ns: processes.stock_stdin_ns,
        },
        assertions,
        passed,
        limitations: [
            "The generalized corpus covers every supported control family plus repeated nested dynamic tags and byte-preserved raw style; CSS payload formatting and validation are intentionally outside this lane.",
            "Fresh stdin compares project TSRX behavior with stock Oxfmt on equivalent TSX, not stock .tsrx support.",
            "RSS uses same-binary canonical TSX as the engine control so Node launcher memory cannot distort P07.",
        ],
    };

    let output_path = args.output_path.unwrap_or_else(|| {
        PathBuf::from(format!("benchmarks/native-format/results-{generated_at_unix_ms}.json"))
    });
    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("unable to serialize report: {error}"))?;
    fs::write(&output_path, format!("{json}\n"))
        .map_err(|error| format!("unable to write {}: {error}", output_path.display()))?;
    print_summary(&report, &output_path);
    if !report.passed {
        return Err("one or more frozen formatter performance assertions failed".to_string());
    }
    Ok(())
}

type ProductSample = (u64, String, tsrx_format::FormatMetadata);

fn measure_control(source: &str) -> Result<(u64, String), String> {
    let started = Instant::now();
    let output = oxc_adapter::format(&FormatRequest {
        parse_source: source,
        source_kind: SourceKind::TypeScriptReact,
        dynamic_tags: None,
        options: None,
    })?;
    Ok((elapsed_ns(started), output.code))
}

fn measure_product(path: &Path, source: &str) -> Result<ProductSample, String> {
    let started = Instant::now();
    let output = format_text(path, source)?;
    Ok((elapsed_ns(started), output.code, output.metadata))
}

fn measure_config_session(root: &Path) -> Result<ConfigSessionSummary, String> {
    fs::create_dir(root)
        .map_err(|error| format!("unable to create {}: {error}", root.display()))?;
    let config_path = root.join(".oxfmtrc.json");
    fs::write(&config_path, r#"{"singleQuote":true,"semi":false}"#)
        .map_err(|error| format!("unable to write {}: {error}", config_path.display()))?;
    let session = FormatSession::new(root, None)?;
    let tsrx = session.format_text(
        &root.join("configured.tsrx"),
        "export function Configured() @{ const message = \"hello\"; }\n",
    )?;
    let tsx = session.format_text(
        &root.join("configured.tsx"),
        "export const Configured = () => <div title=\"hello\">hello</div>;\n",
    )?;
    let options_applied = tsrx.code.contains("'hello'")
        && !tsrx.code.contains("'hello';")
        && tsx.code.contains("title=\"hello\"")
        && !tsx.code.trim_end().ends_with(';');
    Ok(ConfigSessionSummary {
        config_loads: session.config_loads(),
        config_load_ns: session.config_load_ns(),
        files: 2,
        parse_count: tsrx.metadata.parse_count.saturating_add(tsx.metadata.parse_count),
        options_applied,
    })
}

#[derive(Debug)]
struct ProcessMeasurements {
    batch_bytes: usize,
    batch_ns: Vec<u64>,
    candidate_stdin_ns: Vec<u64>,
    stock_stdin_ns: Vec<u64>,
    tsrx_rss_bytes: Vec<u64>,
    tsx_rss_bytes: Vec<u64>,
    complete_output: bool,
}

fn run_process_measurements(
    temporary: &Path,
    tsrx: &str,
    tsx: &str,
    budgets: &Budgets,
) -> Result<ProcessMeasurements, String> {
    let batch_directory = temporary.join("batch");
    fs::create_dir(&batch_directory)
        .map_err(|error| format!("unable to create {}: {error}", batch_directory.display()))?;
    let per_file_target = 256 * 1024;
    let per_file = build_corpus(per_file_target);
    let file_count = budgets.batch_corpus_target_bytes.div_ceil(per_file.len()).max(2);
    let mut paths = Vec::with_capacity(file_count);
    for index in 0..file_count {
        let path = batch_directory.join(format!("batch-{index:04}.tsrx"));
        fs::write(&path, &per_file)
            .map_err(|error| format!("unable to write {}: {error}", path.display()))?;
        paths.push(path);
    }
    let batch_bytes = per_file.len() * paths.len();
    for _ in 0..budgets.batch_warmups {
        black_box(measure_batch(&budgets.candidate_binary, &paths)?);
    }
    let batch_ns = (0..budgets.batch_samples)
        .map(|_| measure_batch(&budgets.candidate_binary, &paths))
        .collect::<Result<Vec<_>, _>>()?;

    let stdin_tsrx = build_corpus(10 * 1024);
    let stdin_overlay = scan(&stdin_tsrx).map_err(|error| error.to_string())?;
    let stdin_tsx = project(&stdin_tsrx, &stdin_overlay).map_err(|error| error.to_string())?;
    let mut candidate_stdin_ns = Vec::with_capacity(budgets.cold_process_samples);
    let mut stock_stdin_ns = Vec::with_capacity(budgets.cold_process_samples);
    let mut complete_output = true;
    for _ in 0..budgets.cold_process_samples {
        let candidate = measure_stdin_process(&budgets.candidate_binary, "cold.tsrx", &stdin_tsrx)?;
        let stock = measure_stdin_process(&budgets.stock_oxfmt_binary, "cold.tsx", &stdin_tsx)?;
        complete_output &= !candidate.1.is_empty() && !stock.1.is_empty();
        candidate_stdin_ns.push(candidate.0);
        stock_stdin_ns.push(stock.0);
    }

    let memory_tsrx = build_corpus(budgets.memory_corpus_target_bytes);
    let memory_overlay = scan(&memory_tsrx).map_err(|error| error.to_string())?;
    let memory_tsx = project(&memory_tsrx, &memory_overlay).map_err(|error| error.to_string())?;
    let memory_tsrx_path = temporary.join("memory.tsrx");
    let memory_tsx_path = temporary.join("memory.tsx");
    fs::write(&memory_tsrx_path, memory_tsrx)
        .map_err(|error| format!("unable to write {}: {error}", memory_tsrx_path.display()))?;
    fs::write(&memory_tsx_path, memory_tsx)
        .map_err(|error| format!("unable to write {}: {error}", memory_tsx_path.display()))?;
    let current = env::current_exe().map_err(|error| error.to_string())?;
    let mut tsrx_rss_bytes = Vec::with_capacity(budgets.rss_process_samples);
    let mut tsx_rss_bytes = Vec::with_capacity(budgets.rss_process_samples);
    for _ in 0..budgets.rss_process_samples {
        tsrx_rss_bytes.push(measure_memory_child(&current, &memory_tsrx_path)?);
        tsx_rss_bytes.push(measure_memory_child(&current, &memory_tsx_path)?);
    }

    // Keep the primary corpus arguments used so the function's comparison boundary stays clear.
    black_box((tsrx.len(), tsx.len()));
    Ok(ProcessMeasurements {
        batch_bytes,
        batch_ns,
        candidate_stdin_ns,
        stock_stdin_ns,
        tsrx_rss_bytes,
        tsx_rss_bytes,
        complete_output,
    })
}

fn measure_batch(binary: &Path, paths: &[PathBuf]) -> Result<u64, String> {
    let started = Instant::now();
    let output = Command::new(binary)
        .arg("--check")
        .args(paths)
        .output()
        .map_err(|error| format!("unable to run {}: {error}", binary.display()))?;
    let elapsed = elapsed_ns(started);
    if output.status.code() != Some(1) {
        return Err(format!(
            "batch formatter exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(elapsed)
}

fn measure_stdin_process(
    binary: &Path,
    filepath: &str,
    source: &str,
) -> Result<(u64, Vec<u8>), String> {
    let started = Instant::now();
    let mut child = Command::new(binary)
        .arg(format!("--stdin-filepath={filepath}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("unable to spawn {}: {error}", binary.display()))?;
    child
        .stdin
        .take()
        .ok_or("formatter child has no stdin")?
        .write_all(source.as_bytes())
        .map_err(|error| format!("unable to write formatter stdin: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("unable to wait for {}: {error}", binary.display()))?;
    let elapsed = elapsed_ns(started);
    if !output.status.success() {
        return Err(format!(
            "stdin formatter {} failed: {}",
            binary.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok((elapsed, output.stdout))
}

fn run_memory_child(arguments: &[String]) -> Result<(), String> {
    if arguments.len() != 2 {
        return Err("--memory-child requires one source path".to_string());
    }
    let path = Path::new(&arguments[1]);
    let source = fs::read_to_string(path)
        .map_err(|error| format!("unable to read {}: {error}", path.display()))?;
    let output = format_text(path, &source)?;
    black_box(&output);
    let memory = memory_stats().ok_or("current-process RSS is unavailable")?;
    println!("{}", memory.physical_mem);
    black_box(output);
    Ok(())
}

fn measure_memory_child(binary: &Path, source: &Path) -> Result<u64, String> {
    let output = Command::new(binary)
        .arg("--memory-child")
        .arg(source)
        .output()
        .map_err(|error| format!("unable to run memory child: {error}"))?;
    if !output.status.success() {
        return Err(format!("memory child failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| error.to_string())?
        .trim()
        .parse::<u64>()
        .map_err(|error| format!("invalid memory child output: {error}"))
}

fn build_corpus(target_bytes: usize) -> String {
    let mut source = String::with_capacity(target_bytes + 2048);
    source.push_str("type Props = { ready: boolean; label: string };\n");
    let payload = "x".repeat(896);
    let mut index = 0usize;
    while source.len() < target_bytes {
        write!(
            source,
            "export function View{index}({{ ready, label }}: Props) @{{\n  const payload = \"{payload}\";\n  const pattern = /@if\\s+\\//gu;\n  <p title={{`@else ${{label}}`}}>@if is text {{payload}}</p>;\n  @if (ready) {{\n    <strong>{{label}}</strong>;\n  }} @else {{\n    <span>{{pattern.source}}</span>;\n  }}\n}}\n"
        )
        .expect("writing to a String cannot fail");
        index += 1;
    }
    source
}

fn build_generalized_control_corpus(target_bytes: usize) -> String {
    let mut source = String::with_capacity(target_bytes + 2048);
    source.push_str(
        "type ControlProps = { ready: boolean; label: string };\n\
         type Row = { id: number; active: boolean; label: string };\n",
    );
    let payload = "control".repeat(16);
    let mut index = 0usize;
    while source.len() < target_bytes {
        write!(
            source,
            "export async function Control{index}({{ ready, label }}: ControlProps) @{{\n\
               const rows: Row[] = [{{ id: {index}, active: ready, label }}];\n\
               const Dynamic = ready ? \"article\" : \"aside\";\n\
               const Child = \"span\";\n\
               const rendered = @if (ready) {{ <b>{{label}}</b> }} @else {{ <i>{payload}</i> }};\n\
               const recovered = @try {{ <mark>{{rendered}}</mark> }} @catch (error: Error, reset: () => void) {{ <button onClick={{reset}}>{{String(error)}}</button> }};\n\
               <section data-control={{\"{index}\"}}>\n\
                 @if (ready) {{\n\
                   <div>\n\
                     @for await (const row of rows; index rowIndex; key row.id) {{\n\
                       <article>\n\
                         @if (row.active) {{ <strong>{{rendered}}</strong> }} @else {{ <span>{{rowIndex}}</span> }}\n\
                       </article>\n\
                     }} @empty {{ <p>{{label}}</p> }}\n\
                   </div>\n\
                 }} @else {{ <aside>{{rendered}}</aside> }}\n\
                 @switch (rows[0].id % 3) {{\n\
                   @case 0: {{\n\
                     @try {{\n\
                       @if (ready) {{ <strong>{{recovered}}</strong> }} @else {{ <span>{{label}}</span> }}\n\
                     }} @pending {{ <i>pending</i> }} @catch (error, reset) {{ <button onClick={{reset}}>{{String(error)}}</button> }}\n\
                   }}\n\
                   @case 1: {{ <small>{{rendered}}</small> }}\n\
                   @default: {{ <u>{{label}}</u> }}\n\
                 }}\n\
                 <{{Dynamic}} data-dynamic={{\"{index}\"}}>\n\
                   <{{Child}} data-child={{label}} />\n\
                   <style data-scope={{\"{index}\"}}>/* raw {{label}} */ .control-{index}{{color:red}} @media (min-width:1px){{.control-{index}::before{{content:\"{{label}}\"}}}}</style>\n\
                 </{{Dynamic}}>\n\
               </section>;\n\
             }}\n"
        )
        .expect("writing to a String cannot fail");
        index += 1;
    }
    source
}

fn parse_args(mut arguments: impl Iterator<Item = String>) -> Result<Args, String> {
    if arguments.next().as_deref() != Some("--assert") {
        return Err("expected --assert <budgets.json> [--output <report.json>]".to_string());
    }
    let budget_path = PathBuf::from(arguments.next().ok_or("--assert requires a budget path")?);
    let mut output_path = None;
    while let Some(argument) = arguments.next() {
        if argument != "--output" {
            return Err(format!("unsupported benchmark option: {argument}"));
        }
        if output_path.is_some() {
            return Err("--output may be specified only once".to_string());
        }
        output_path = Some(PathBuf::from(arguments.next().ok_or("--output requires a path")?));
    }
    Ok(Args { budget_path, output_path })
}

fn validate_budgets(budgets: &Budgets) -> Result<(), String> {
    if budgets.schema_version != 2 {
        return Err("unsupported formatter budget schema".to_string());
    }
    if budgets.warmups < 5
        || budgets.samples < 30
        || budgets.generalized_control_warmups < 5
        || budgets.generalized_control_samples < 15
        || budgets.batch_warmups < 5
        || budgets.batch_samples < 15
        || budgets.cold_process_samples < 20
        || budgets.rss_process_samples < 5
        || budgets.generalized_control_target_bytes < 256 * 1024
    {
        return Err("formatter sample counts are below the frozen minima".to_string());
    }
    if budgets.p04.direct_median_latency_ratio_max > 1.05
        || budgets.p04.direct_p95_latency_ratio_max > 1.08
        || budgets.p04.sequential_median_mib_per_second_min < 15.0
        || budgets.p04.historical_incumbent_derived_mib_per_second_min < 16.6
        || budgets.p04.default_thread_mib_per_second_min < 100.0
        || budgets.p04.generalized_control_median_mib_per_second_min < 15.0
        || budgets.p04.generalized_control_p95_mib_per_second_min < 12.0
        || budgets.p04.generalized_control_scaling_ratio_max > 1.35
        || budgets.p05.stdin_p95_ms_max > 110.0
        || budgets.p05.upstream_latency_ratio_max > 1.25
        || budgets.p07.upstream_ratio_max > 1.15
    {
        return Err("formatter budgets weaken the frozen performance contract".to_string());
    }
    Ok(())
}

fn ensure_binary(path: &Path, label: &str) -> Result<(), String> {
    path.is_file()
        .then_some(())
        .ok_or_else(|| format!("{label} binary is missing: {}", path.display()))
}

fn distribution(samples: &[u64], bytes: usize) -> Distribution {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let p50_ns = percentile(&sorted, 50);
    let p95_ns = percentile(&sorted, 95);
    let p99_ns = percentile(&sorted, 99);
    Distribution {
        samples: samples.len(),
        p50_ns,
        p95_ns,
        p99_ns,
        p50_ms: ns_to_ms(p50_ns),
        p95_ms: ns_to_ms(p95_ns),
        p99_ms: ns_to_ms(p99_ns),
        median_mib_per_second: throughput(bytes, p50_ns),
        p95_mib_per_second: throughput(bytes, p95_ns),
    }
}

fn phase_distribution(samples: &[u64]) -> PhaseDistribution {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    PhaseDistribution {
        samples: samples.len(),
        p50_ns: percentile(&sorted, 50),
        p95_ns: percentile(&sorted, 95),
        p99_ns: percentile(&sorted, 99),
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    let rank = sorted.len().saturating_mul(percentile).div_ceil(100);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn throughput(bytes: usize, nanoseconds: u64) -> f64 {
    (bytes as f64 / MEBIBYTE) / (nanoseconds as f64 / 1_000_000_000.0)
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    numerator as f64 / denominator.max(1) as f64
}

fn ns_to_ms(value: u64) -> f64 {
    value as f64 / 1_000_000.0
}

fn median_u64(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn assert_min(
    assertions: &mut Vec<Assertion>,
    name: &'static str,
    comparison: &'static str,
    observed: f64,
    threshold: f64,
) {
    assertions.push(Assertion {
        name,
        comparison,
        observed,
        threshold,
        pass: observed >= threshold,
    });
}

fn assert_max(
    assertions: &mut Vec<Assertion>,
    name: &'static str,
    comparison: &'static str,
    observed: f64,
    threshold: f64,
) {
    assertions.push(Assertion {
        name,
        comparison,
        observed,
        threshold,
        pass: observed <= threshold,
    });
}

fn assert_bool(assertions: &mut Vec<Assertion>, name: &'static str, value: bool) {
    assertions.push(Assertion {
        name,
        comparison: "required boolean invariant",
        observed: f64::from(value),
        threshold: 1.0,
        pass: value,
    });
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn create_temp_directory() -> Result<PathBuf, String> {
    let path = env::temp_dir().join(format!(
        "oxc-tsrx-format-benchmark-{}-{}",
        std::process::id(),
        now_millis()?
    ));
    fs::create_dir(&path)
        .map_err(|error| format!("unable to create {}: {error}", path.display()))?;
    Ok(path)
}

fn now_millis() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| error.to_string())
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn command_text(program: &str, arguments: &[&str]) -> String {
    Command::new(program)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(
            || "unavailable".to_string(),
            |output| String::from_utf8_lossy(&output.stdout).trim().to_string(),
        )
}

fn command_path_text(program: &Path, arguments: &[&str]) -> String {
    Command::new(program)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(
            || "unavailable".to_string(),
            |output| String::from_utf8_lossy(&output.stdout).trim().to_string(),
        )
}

fn print_summary(report: &Report, path: &Path) {
    println!("formatter benchmark report: {}", path.display());
    for assertion in &report.assertions {
        println!(
            "{} {} observed={:.3} threshold={:.3}",
            if assertion.pass { "PASS" } else { "FAIL" },
            assertion.name,
            assertion.observed,
            assertion.threshold
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tsrx_format::format_text;

    use super::build_generalized_control_corpus;

    #[test]
    fn generalized_dynamic_style_corpus_formats_and_converges() {
        let source = build_generalized_control_corpus(4096);
        let first = format_text(Path::new("benchmark.tsrx"), &source)
            .unwrap_or_else(|error| panic!("{error}\n{source}"));
        let second = format_text(Path::new("benchmark.tsrx"), &first.code).unwrap();
        assert_eq!(second.code, first.code);
        assert_eq!(first.metadata.parse_count, 1);
        assert!(first.metadata.style_count > 0);
        assert_eq!(first.metadata.embedded_parse_count, 0);
    }
}
