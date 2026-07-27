// Benchmark math intentionally converts bounded byte/nanosecond counters to floating point for
// human-readable rates. TSX/TSRX names and `_ns` fields are units, not accidental similarities.
#![allow(
    clippy::cast_possible_truncation,
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
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use memory_stats::memory_stats;
use oxc_adapter::{LintEngine, LintEngineOptions, LintRequest, OXC_REVISION, SourceKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tsrx_lint::{ConfigRuleFilter, ConfigRuleSeverity, LintSession};
use tsrx_syntax::{project, scan};

const MEBIBYTE: f64 = 1_048_576.0;
const RULE: &str = "no-debugger";
const STARTUP_RULES: [&str; 2] = ["no-debugger", "no-unused-vars"];

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
    warm_10k_samples: usize,
    cold_process_samples: usize,
    rss_process_samples: usize,
    corpus_target_bytes: usize,
    memory_corpus_target_bytes: usize,
    candidate_binary: PathBuf,
    stock_oxlint_binary: PathBuf,
    p01: P01Budget,
    p02: P02Budget,
    p03: P03Budget,
    p05: P05Budget,
    p07: P07Budget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct P01Budget {
    median_latency_ratio_max: f64,
    p95_latency_ratio_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct P02Budget {
    median_mib_per_second_min: f64,
    p95_mib_per_second_min: f64,
    equivalent_tsx_ratio_min: f64,
    warm_10k_p95_ms_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct P03Budget {
    one_thread_median_mib_per_second_min: f64,
    end_to_end_latency_ratio_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct P05Budget {
    cold_p95_ms_max: f64,
    upstream_latency_ratio_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct P07Budget {
    upstream_ratio_max: f64,
    additive_bytes_max: u64,
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
    stock_oxlint_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    kind: &'static str,
    bytes: usize,
    fnv1a64: String,
    structural_forms: [&'static str; 3],
    diagnostic_rule: &'static str,
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticSignature {
    code: String,
    message: String,
    offset: u32,
    length: u32,
}

#[derive(Debug)]
struct ControlSample {
    total_ns: u64,
    signatures: Vec<DiagnosticSignature>,
}

#[derive(Debug)]
struct ProductSample {
    total_ns: u64,
    scan_ns: u64,
    projection_ns: u64,
    parse_ns: u64,
    semantic_ns: u64,
    lint_ns: u64,
    projection_bytes: usize,
    signatures: Vec<DiagnosticSignature>,
}

#[derive(Debug)]
struct CliSample {
    total_ns: u64,
    signatures: Vec<DiagnosticSignature>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct P01Summary {
    control: Distribution,
    candidate_standard_path: Distribution,
    median_latency_ratio: f64,
    p95_latency_ratio: f64,
    stock_oxlint_cli_reference: Distribution,
    candidate_standard_cli: Distribution,
    cli_median_latency_ratio: f64,
    diagnostic_parity: bool,
    direct_bypass: bool,
    comparison_basis: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct P02Summary {
    projected_scan_copy_parse: Distribution,
    equivalent_tsx_parse: Distribution,
    equivalent_tsx_throughput_ratio: f64,
    warm_10k_scan_copy_parse: Distribution,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct P03Summary {
    in_process_projected_lint: Distribution,
    in_process_equivalent_tsx_lint: Distribution,
    in_process_latency_ratio: f64,
    cli_projected_lint: Distribution,
    cli_equivalent_tsx_lint: Distribution,
    cli_latency_ratio: f64,
    diagnostic_parity: bool,
    threads: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct P05Summary {
    candidate_tsrx_cold_process: Distribution,
    stock_oxlint_tsx_cold_process: Distribution,
    p95_latency_ratio: f64,
    fresh_processes: bool,
    rules: [&'static str; 2],
    diagnostic_parity: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct P07Summary {
    candidate_tsrx_rss_bytes: Vec<u64>,
    candidate_tsx_rss_bytes: Vec<u64>,
    candidate_tsrx_median_rss_bytes: u64,
    candidate_tsx_median_rss_bytes: u64,
    allowed_rss_bytes: u64,
    measurement: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigSessionSummary {
    config_loads: u32,
    config_load_ns: u64,
    files: u32,
    parse_count: u32,
    configured_rule_diagnostics: usize,
    configured_rule_applied: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Summaries {
    p01: P01Summary,
    p02: P02Summary,
    p03: P03Summary,
    p05: P05Summary,
    p07: P07Summary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawSamples {
    control_before_ns: Vec<u64>,
    control_after_ns: Vec<u64>,
    candidate_standard_total_ns: Vec<u64>,
    candidate_standard_parse_ns: Vec<u64>,
    candidate_tsrx_total_ns: Vec<u64>,
    candidate_tsrx_scan_ns: Vec<u64>,
    candidate_tsrx_projection_ns: Vec<u64>,
    candidate_tsrx_parse_ns: Vec<u64>,
    candidate_tsrx_semantic_ns: Vec<u64>,
    candidate_tsrx_lint_ns: Vec<u64>,
    warm_10k_scan_copy_parse_ns: Vec<u64>,
    stock_cli_before_ns: Vec<u64>,
    stock_cli_after_ns: Vec<u64>,
    candidate_standard_cli_ns: Vec<u64>,
    candidate_tsrx_cli_ns: Vec<u64>,
    stock_cold_cli_ns: Vec<u64>,
    candidate_cold_cli_ns: Vec<u64>,
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
    config_session: ConfigSessionSummary,
    summaries: Summaries,
    raw_samples: RawSamples,
    assertions: Vec<Assertion>,
    passed: bool,
    limitations: [&'static str; 3],
}

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    if arguments.next().as_deref() == Some("--memory-child") {
        let result = arguments
            .next()
            .ok_or("--memory-child requires a source path".to_string())
            .and_then(|path| run_memory_child(Path::new(&path)));
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("oxc-tsrx memory child: {error}");
                ExitCode::FAILURE
            }
        };
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("oxc-tsrx benchmark: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("performance gates require a release build".to_string());
    }

    let args = parse_args(env::args().skip(1))?;
    let budget_source = fs::read_to_string(&args.budget_path).map_err(|error| {
        format!("unable to read budgets {}: {error}", args.budget_path.display())
    })?;
    let budgets: Budgets = serde_json::from_str(&budget_source)
        .map_err(|error| format!("invalid benchmark budgets: {error}"))?;
    validate_budgets(&budgets)?;
    ensure_binary(&budgets.candidate_binary, "candidate")?;
    ensure_binary(&budgets.stock_oxlint_binary, "stock Oxlint")?;

    let filters =
        vec![ConfigRuleFilter { severity: ConfigRuleSeverity::Deny, name: RULE.to_string() }];
    let source = build_corpus(budgets.corpus_target_bytes);
    let overlay = scan(&source).map_err(|error| error.to_string())?;
    let equivalent_tsx = project(&source, &overlay).map_err(|error| error.to_string())?;
    let warm_source = build_corpus(10 * 1024);

    let temporary = create_temp_directory()?;
    let control_engine = LintEngine::new(&LintEngineOptions {
        cwd: &temporary,
        config_path: None,
        config_base: None,
        filters: &filters,
        collect_fixes: false,
    })?;
    let product_session = LintSession::new(&temporary, None, &filters, false)?;
    let tsrx_path = temporary.join("benchmark.tsrx");
    let tsx_path = temporary.join("benchmark.tsx");
    let warm_path = temporary.join("warm-10k.tsrx");
    for _ in 0..budgets.warmups {
        black_box(measure_control(&control_engine, &tsx_path, &equivalent_tsx)?);
        black_box(measure_product(&product_session, &tsx_path, &equivalent_tsx)?);
        black_box(measure_product(&product_session, &tsrx_path, &source)?);
        black_box(measure_control(&control_engine, &tsx_path, &equivalent_tsx)?);
    }

    let mut control_before = Vec::with_capacity(budgets.samples);
    let mut control_after = Vec::with_capacity(budgets.samples);
    let mut standard = Vec::with_capacity(budgets.samples);
    let mut projected = Vec::with_capacity(budgets.samples);
    for _ in 0..budgets.samples {
        control_before.push(measure_control(&control_engine, &tsx_path, &equivalent_tsx)?);
        standard.push(measure_product(&product_session, &tsx_path, &equivalent_tsx)?);
        projected.push(measure_product(&product_session, &tsrx_path, &source)?);
        control_after.push(measure_control(&control_engine, &tsx_path, &equivalent_tsx)?);
    }

    let expected = &control_before[0].signatures;
    let diagnostic_parity =
        control_before.iter().chain(&control_after).all(|sample| sample.signatures == *expected)
            && standard.iter().all(|sample| sample.signatures == *expected);
    let projected_parity = projected.iter().all(|sample| sample.signatures == *expected);
    if !diagnostic_parity || !projected_parity {
        return Err(
            "diagnostic output changed between control, standard, and TSRX paths".to_string()
        );
    }
    let direct_bypass = standard.iter().all(|sample| {
        sample.scan_ns == 0 && sample.projection_ns == 0 && sample.projection_bytes == 0
    });

    for _ in 0..budgets.warmups {
        black_box(measure_product(&product_session, &warm_path, &warm_source)?);
    }
    let mut warm_10k = Vec::with_capacity(budgets.warm_10k_samples);
    for _ in 0..budgets.warm_10k_samples {
        let sample = measure_product(&product_session, &warm_path, &warm_source)?;
        warm_10k.push(
            sample.scan_ns.saturating_add(sample.projection_ns).saturating_add(sample.parse_ns),
        );
    }

    let config_session = measure_config_session(&temporary.join("config-session"))?;
    let benchmark_result =
        run_process_measurements(&temporary, &source, &equivalent_tsx, &warm_source, &budgets);
    let cleanup_result = fs::remove_dir_all(&temporary)
        .map_err(|error| format!("unable to remove {}: {error}", temporary.display()));
    let processes = benchmark_result?;
    cleanup_result?;

    let control_ns = control_before
        .iter()
        .chain(&control_after)
        .map(|sample| sample.total_ns)
        .collect::<Vec<_>>();
    let standard_ns = standard.iter().map(|sample| sample.total_ns).collect::<Vec<_>>();
    let projected_ns = projected.iter().map(|sample| sample.total_ns).collect::<Vec<_>>();
    let standard_parse_ns = standard.iter().map(|sample| sample.parse_ns).collect::<Vec<_>>();
    let projected_parse_path_ns = projected
        .iter()
        .map(|sample| {
            sample.scan_ns.saturating_add(sample.projection_ns).saturating_add(sample.parse_ns)
        })
        .collect::<Vec<_>>();
    let control_distribution = distribution(&control_ns, source.len())?;
    let standard_distribution = distribution(&standard_ns, source.len())?;
    let projected_distribution = distribution(&projected_ns, source.len())?;
    let standard_parse_distribution = distribution(&standard_parse_ns, source.len())?;
    let projected_parse_distribution = distribution(&projected_parse_path_ns, source.len())?;
    let warm_distribution = distribution(&warm_10k, warm_source.len())?;

    let stock_cli_ns = processes
        .stock_cli_before
        .iter()
        .chain(&processes.stock_cli_after)
        .map(|sample| sample.total_ns)
        .collect::<Vec<_>>();
    let candidate_standard_cli_ns =
        processes.candidate_standard_cli.iter().map(|sample| sample.total_ns).collect::<Vec<_>>();
    let candidate_tsrx_cli_ns =
        processes.candidate_tsrx_cli.iter().map(|sample| sample.total_ns).collect::<Vec<_>>();
    let stock_cli_distribution = distribution(&stock_cli_ns, source.len())?;
    let candidate_standard_cli_distribution =
        distribution(&candidate_standard_cli_ns, source.len())?;
    let candidate_tsrx_cli_distribution = distribution(&candidate_tsrx_cli_ns, source.len())?;
    let stock_cold_distribution = distribution(&processes.stock_cold_cli_ns, warm_source.len())?;
    let candidate_cold_distribution =
        distribution(&processes.candidate_cold_cli_ns, warm_source.len())?;

    let p01_median_ratio = ratio(standard_distribution.p50_ns, control_distribution.p50_ns);
    let p01_p95_ratio = ratio(standard_distribution.p95_ns, control_distribution.p95_ns);
    let p01_cli_ratio =
        ratio(candidate_standard_cli_distribution.p50_ns, stock_cli_distribution.p50_ns);
    let p02_equivalent_ratio = projected_parse_distribution.median_mib_per_second
        / standard_parse_distribution.median_mib_per_second;
    let p03_hot_ratio = ratio(projected_distribution.p50_ns, standard_distribution.p50_ns);
    let p03_cli_ratio =
        ratio(candidate_tsrx_cli_distribution.p50_ns, candidate_standard_cli_distribution.p50_ns);
    let p05_ratio = ratio(candidate_cold_distribution.p95_ns, stock_cold_distribution.p95_ns);

    let candidate_tsrx_rss = processes.candidate_tsrx_rss;
    let candidate_tsx_rss = processes.candidate_tsx_rss;
    let tsrx_median_rss = percentile(&candidate_tsrx_rss, 0.50)?;
    let tsx_median_rss = percentile(&candidate_tsx_rss, 0.50)?;
    let ratio_limit = (tsx_median_rss as f64 * budgets.p07.upstream_ratio_max) as u64;
    let additive_limit = tsx_median_rss.saturating_add(budgets.p07.additive_bytes_max);
    let rss_limit = ratio_limit.min(additive_limit);

    let p01 = P01Summary {
        control: control_distribution,
        candidate_standard_path: standard_distribution.clone(),
        median_latency_ratio: p01_median_ratio,
        p95_latency_ratio: p01_p95_ratio,
        stock_oxlint_cli_reference: stock_cli_distribution,
        candidate_standard_cli: candidate_standard_cli_distribution.clone(),
        cli_median_latency_ratio: p01_cli_ratio,
        diagnostic_parity,
        direct_bypass,
        comparison_basis: "same-build oxc_adapter control sandwiched around the product standard path",
    };
    let p02 = P02Summary {
        projected_scan_copy_parse: projected_parse_distribution,
        equivalent_tsx_parse: standard_parse_distribution,
        equivalent_tsx_throughput_ratio: p02_equivalent_ratio,
        warm_10k_scan_copy_parse: warm_distribution,
    };
    let p03 = P03Summary {
        in_process_projected_lint: projected_distribution,
        in_process_equivalent_tsx_lint: standard_distribution,
        in_process_latency_ratio: p03_hot_ratio,
        cli_projected_lint: candidate_tsrx_cli_distribution,
        cli_equivalent_tsx_lint: candidate_standard_cli_distribution,
        cli_latency_ratio: p03_cli_ratio,
        diagnostic_parity: projected_parity && processes.diagnostic_parity,
        threads: 1,
    };
    let p05 = P05Summary {
        candidate_tsrx_cold_process: candidate_cold_distribution,
        stock_oxlint_tsx_cold_process: stock_cold_distribution,
        p95_latency_ratio: p05_ratio,
        fresh_processes: true,
        rules: STARTUP_RULES,
        diagnostic_parity: processes.startup_diagnostic_parity,
    };
    let p07 = P07Summary {
        candidate_tsrx_rss_bytes: candidate_tsrx_rss,
        candidate_tsx_rss_bytes: candidate_tsx_rss,
        candidate_tsrx_median_rss_bytes: tsrx_median_rss,
        candidate_tsx_median_rss_bytes: tsx_median_rss,
        allowed_rss_bytes: rss_limit,
        measurement: "peak current-process RSS sampled by memory-stats in fresh benchmark children while linting and serializing an equal-byte large corpus",
    };

    let mut assertions = vec![
        maximum(
            "P01 median standard-path latency ratio",
            p01.median_latency_ratio,
            budgets.p01.median_latency_ratio_max,
        ),
        maximum(
            "P01 p95 standard-path latency ratio",
            p01.p95_latency_ratio,
            budgets.p01.p95_latency_ratio_max,
        ),
        boolean("P01 diagnostic parity", p01.diagnostic_parity),
        boolean("P01 direct standard bypass", p01.direct_bypass),
        minimum(
            "P02 median scan+copy+parse throughput",
            p02.projected_scan_copy_parse.median_mib_per_second,
            budgets.p02.median_mib_per_second_min,
        ),
        minimum(
            "P02 p95 scan+copy+parse throughput",
            p02.projected_scan_copy_parse.p95_mib_per_second,
            budgets.p02.p95_mib_per_second_min,
        ),
        minimum(
            "P02 equivalent-TSX throughput ratio",
            p02.equivalent_tsx_throughput_ratio,
            budgets.p02.equivalent_tsx_ratio_min,
        ),
        maximum(
            "P02 warm 10 KiB p95 scan+copy+parse latency",
            p02.warm_10k_scan_copy_parse.p95_ms,
            budgets.p02.warm_10k_p95_ms_max,
        ),
        minimum(
            "P03 in-process one-thread lint throughput",
            p03.in_process_projected_lint.median_mib_per_second,
            budgets.p03.one_thread_median_mib_per_second_min,
        ),
        minimum(
            "P03 CLI one-thread lint throughput",
            p03.cli_projected_lint.median_mib_per_second,
            budgets.p03.one_thread_median_mib_per_second_min,
        ),
        maximum(
            "P03 end-to-end CLI latency ratio",
            p03.cli_latency_ratio,
            budgets.p03.end_to_end_latency_ratio_max,
        ),
        boolean("P03 diagnostic parity", p03.diagnostic_parity),
        boolean(
            "P03 config compiled once",
            config_session.config_loads == 1 && config_session.config_load_ns > 0,
        ),
        boolean(
            "P03 config one parse per file",
            config_session.files == 1 && config_session.parse_count == config_session.files,
        ),
        boolean("P03 configured rule applied", config_session.configured_rule_applied),
        maximum(
            "P05 fresh-process TSRX p95 latency",
            p05.candidate_tsrx_cold_process.p95_ms,
            budgets.p05.cold_p95_ms_max,
        ),
        maximum(
            "P05 fresh-process upstream latency ratio",
            p05.p95_latency_ratio,
            budgets.p05.upstream_latency_ratio_max,
        ),
        boolean("P05 startup-rule diagnostic parity", p05.diagnostic_parity),
        maximum(
            "P07 TSRX peak RSS",
            p07.candidate_tsrx_median_rss_bytes as f64,
            p07.allowed_rss_bytes as f64,
        ),
    ];
    assertions.shrink_to_fit();
    let passed = assertions.iter().all(|assertion| assertion.pass);

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    let report = Report {
        schema_version: 1,
        generated_at_unix_ms: timestamp,
        host: host(&budgets),
        budgets: budgets.clone(),
        corpus: Corpus {
            kind: "generated retained statement-control TSRX corpus",
            bytes: source.len(),
            fnv1a64: format!("{:016x}", fnv1a64(source.as_bytes())),
            structural_forms: ["@{", "@if", "@else"],
            diagnostic_rule: RULE,
            note: "Retains the original statement-control workload for longitudinal lint comparisons; the supported TSRX grammar is broader than this corpus.",
        },
        config_session,
        summaries: Summaries { p01, p02, p03, p05, p07 },
        raw_samples: RawSamples {
            control_before_ns: control_before.iter().map(|sample| sample.total_ns).collect(),
            control_after_ns: control_after.iter().map(|sample| sample.total_ns).collect(),
            candidate_standard_total_ns: standard_ns,
            candidate_standard_parse_ns: standard_parse_ns,
            candidate_tsrx_total_ns: projected_ns,
            candidate_tsrx_scan_ns: projected.iter().map(|sample| sample.scan_ns).collect(),
            candidate_tsrx_projection_ns: projected
                .iter()
                .map(|sample| sample.projection_ns)
                .collect(),
            candidate_tsrx_parse_ns: projected.iter().map(|sample| sample.parse_ns).collect(),
            candidate_tsrx_semantic_ns: projected.iter().map(|sample| sample.semantic_ns).collect(),
            candidate_tsrx_lint_ns: projected.iter().map(|sample| sample.lint_ns).collect(),
            warm_10k_scan_copy_parse_ns: warm_10k,
            stock_cli_before_ns: processes
                .stock_cli_before
                .iter()
                .map(|sample| sample.total_ns)
                .collect(),
            stock_cli_after_ns: processes
                .stock_cli_after
                .iter()
                .map(|sample| sample.total_ns)
                .collect(),
            candidate_standard_cli_ns,
            candidate_tsrx_cli_ns,
            stock_cold_cli_ns: processes.stock_cold_cli_ns,
            candidate_cold_cli_ns: processes.candidate_cold_cli_ns,
        },
        assertions,
        passed,
        limitations: [
            "This retained lint workload measures @{, @if, and @else; supported control grammar also includes @for/@empty, @switch/@case/@default, and @try/@pending/@catch.",
            "P03 default-thread batching is deferred until the multi-file CLI slice.",
            "The read-only Markless gate accepts all 179/179 parser-valid files; raw style payloads remain byte-preserved outside the JavaScript AST and are not CSS-linted.",
        ],
    };

    let output_path = args.output_path.unwrap_or_else(|| {
        PathBuf::from(format!("benchmarks/native-lint/results-{timestamp}.json"))
    });
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("unable to create {}: {error}", parent.display()))?;
    }
    let encoded = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("unable to encode benchmark report: {error}"))?;
    fs::write(&output_path, format!("{encoded}\n"))
        .map_err(|error| format!("unable to write {}: {error}", output_path.display()))?;

    println!("OXC for TSRX native lint benchmark: {}", if passed { "PASS" } else { "FAIL" });
    println!("raw report: {}", output_path.display());
    println!(
        "P02 scan+copy+parse: {:.2} MiB/s median, {:.2} MiB/s at p95",
        report.summaries.p02.projected_scan_copy_parse.median_mib_per_second,
        report.summaries.p02.projected_scan_copy_parse.p95_mib_per_second
    );
    println!(
        "P03 CLI lint: {:.2} MiB/s median, {:.3}x equivalent TSX",
        report.summaries.p03.cli_projected_lint.median_mib_per_second,
        report.summaries.p03.cli_latency_ratio
    );
    println!(
        "P05 fresh-process p95: {:.2} ms",
        report.summaries.p05.candidate_tsrx_cold_process.p95_ms
    );

    if !passed {
        let failures = report
            .assertions
            .iter()
            .filter(|assertion| !assertion.pass)
            .map(|assertion| assertion.name)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("performance assertions failed: {failures}"));
    }
    Ok(())
}

#[derive(Debug)]
struct ProcessMeasurements {
    stock_cli_before: Vec<CliSample>,
    stock_cli_after: Vec<CliSample>,
    candidate_standard_cli: Vec<CliSample>,
    candidate_tsrx_cli: Vec<CliSample>,
    stock_cold_cli_ns: Vec<u64>,
    candidate_cold_cli_ns: Vec<u64>,
    candidate_tsx_rss: Vec<u64>,
    candidate_tsrx_rss: Vec<u64>,
    diagnostic_parity: bool,
    startup_diagnostic_parity: bool,
}

fn run_process_measurements(
    directory: &Path,
    source: &str,
    equivalent_tsx: &str,
    warm_source: &str,
    budgets: &Budgets,
) -> Result<ProcessMeasurements, String> {
    let large_tsrx = directory.join("benchmark.tsrx");
    let large_tsx = directory.join("benchmark.tsx");
    let warm_tsrx = directory.join("warm.tsrx");
    let warm_tsx_source = {
        let overlay = scan(warm_source).map_err(|error| error.to_string())?;
        project(warm_source, &overlay).map_err(|error| error.to_string())?
    };
    let warm_tsx = directory.join("warm.tsx");
    fs::write(&large_tsrx, source).map_err(|error| error.to_string())?;
    fs::write(&large_tsx, equivalent_tsx).map_err(|error| error.to_string())?;
    fs::write(&warm_tsrx, warm_source).map_err(|error| error.to_string())?;
    fs::write(&warm_tsx, warm_tsx_source).map_err(|error| error.to_string())?;

    for _ in 0..budgets.warmups {
        black_box(run_cli(&budgets.stock_oxlint_binary, &large_tsx)?);
        black_box(run_cli(&budgets.candidate_binary, &large_tsx)?);
        black_box(run_cli(&budgets.candidate_binary, &large_tsrx)?);
    }

    let mut stock_cli_before = Vec::with_capacity(budgets.samples);
    let mut stock_cli_after = Vec::with_capacity(budgets.samples);
    let mut candidate_standard_cli = Vec::with_capacity(budgets.samples);
    let mut candidate_tsrx_cli = Vec::with_capacity(budgets.samples);
    for _ in 0..budgets.samples {
        stock_cli_before.push(run_cli(&budgets.stock_oxlint_binary, &large_tsx)?);
        candidate_standard_cli.push(run_cli(&budgets.candidate_binary, &large_tsx)?);
        candidate_tsrx_cli.push(run_cli(&budgets.candidate_binary, &large_tsrx)?);
        stock_cli_after.push(run_cli(&budgets.stock_oxlint_binary, &large_tsx)?);
    }

    let expected = &candidate_standard_cli[0].signatures;
    let diagnostic_parity = stock_cli_before
        .iter()
        .chain(&stock_cli_after)
        .all(|sample| sample.signatures == *expected)
        && candidate_standard_cli.iter().all(|sample| sample.signatures == *expected)
        && candidate_tsrx_cli.iter().all(|sample| sample.signatures == *expected);
    if !diagnostic_parity {
        return Err("CLI diagnostic parity failed".to_string());
    }

    for _ in 0..2 {
        black_box(run_cli_with_rules(&budgets.stock_oxlint_binary, &warm_tsx, &STARTUP_RULES)?);
        black_box(run_cli_with_rules(&budgets.candidate_binary, &warm_tsrx, &STARTUP_RULES)?);
    }
    let mut stock_cold_cli_ns = Vec::with_capacity(budgets.cold_process_samples);
    let mut candidate_cold_cli_ns = Vec::with_capacity(budgets.cold_process_samples);
    let mut startup_diagnostic_parity = true;
    for _ in 0..budgets.cold_process_samples {
        let stock = run_cli_with_rules(&budgets.stock_oxlint_binary, &warm_tsx, &STARTUP_RULES)?;
        let candidate = run_cli_with_rules(&budgets.candidate_binary, &warm_tsrx, &STARTUP_RULES)?;
        startup_diagnostic_parity &= stock.signatures == candidate.signatures;
        stock_cold_cli_ns.push(stock.total_ns);
        candidate_cold_cli_ns.push(candidate.total_ns);
    }

    let memory_source = build_corpus(budgets.memory_corpus_target_bytes);
    let memory_overlay = scan(&memory_source).map_err(|error| error.to_string())?;
    let memory_tsx_source =
        project(&memory_source, &memory_overlay).map_err(|error| error.to_string())?;
    let memory_tsrx = directory.join("memory.tsrx");
    let memory_tsx = directory.join("memory.tsx");
    fs::write(&memory_tsrx, memory_source).map_err(|error| error.to_string())?;
    fs::write(&memory_tsx, memory_tsx_source).map_err(|error| error.to_string())?;
    let mut candidate_tsx_rss = Vec::with_capacity(budgets.rss_process_samples);
    let mut candidate_tsrx_rss = Vec::with_capacity(budgets.rss_process_samples);
    for _ in 0..budgets.rss_process_samples {
        candidate_tsx_rss.push(sample_peak_rss(&memory_tsx)?);
        candidate_tsrx_rss.push(sample_peak_rss(&memory_tsrx)?);
    }

    Ok(ProcessMeasurements {
        stock_cli_before,
        stock_cli_after,
        candidate_standard_cli,
        candidate_tsrx_cli,
        stock_cold_cli_ns,
        candidate_cold_cli_ns,
        candidate_tsx_rss,
        candidate_tsrx_rss,
        diagnostic_parity,
        startup_diagnostic_parity,
    })
}

fn measure_control(
    engine: &LintEngine,
    path: &Path,
    source: &str,
) -> Result<ControlSample, String> {
    let started = Instant::now();
    let result = engine.lint(&LintRequest {
        path,
        original_source: source,
        parse_source: source,
        source_kind: SourceKind::TypeScriptReact,
        rules: &[],
        collect_fixes: false,
        dynamic_tags: None,
    })?;
    let total_ns = elapsed_ns(started);
    let signatures = control_signatures(&result.diagnostics);
    black_box(&result);
    Ok(ControlSample { total_ns, signatures })
}

fn measure_product(
    session: &LintSession,
    path: &Path,
    source: &str,
) -> Result<ProductSample, String> {
    let started = Instant::now();
    let result = session.lint_text(path, source)?;
    let total_ns = elapsed_ns(started);
    let signatures = product_signatures(&result);
    let sample = ProductSample {
        total_ns,
        scan_ns: result.metadata.timings.scan_ns,
        projection_ns: result.metadata.timings.projection_ns,
        parse_ns: result.metadata.timings.parse_ns,
        semantic_ns: result.metadata.timings.semantic_ns,
        lint_ns: result.metadata.timings.lint_ns,
        projection_bytes: result.metadata.projection_bytes,
        signatures,
    };
    black_box(&result);
    Ok(sample)
}

fn measure_config_session(root: &Path) -> Result<ConfigSessionSummary, String> {
    fs::create_dir(root)
        .map_err(|error| format!("unable to create {}: {error}", root.display()))?;
    let config_path = root.join(".oxlintrc.json");
    fs::write(&config_path, r#"{"rules":{"no-debugger":"error"}}"#)
        .map_err(|error| format!("unable to write {}: {error}", config_path.display()))?;
    let session = LintSession::new(root, None, &[], false)?;
    let output = session.lint_text(
        &root.join("configured.tsrx"),
        "export function Configured() @{ debugger; }\n",
    )?;
    let aggregate = session.aggregate(vec![output]);
    let configured_rule_diagnostics =
        aggregate.diagnostics.iter().filter(|diagnostic| diagnostic.rule == RULE).count();
    Ok(ConfigSessionSummary {
        config_loads: aggregate.metadata.config_loads,
        config_load_ns: aggregate.metadata.timings.config_ns,
        files: aggregate.number_of_files,
        parse_count: aggregate.metadata.parse_count,
        configured_rule_diagnostics,
        configured_rule_applied: configured_rule_diagnostics == 1,
    })
}

fn run_cli(binary: &Path, source: &Path) -> Result<CliSample, String> {
    run_cli_with_rules(binary, source, &[RULE])
}

fn run_cli_with_rules(binary: &Path, source: &Path, rules: &[&str]) -> Result<CliSample, String> {
    let started = Instant::now();
    let mut command = Command::new(binary);
    command.arg("--format=json");
    for rule in rules {
        command.arg("--deny").arg(rule);
    }
    let output = command
        .arg(source)
        .output()
        .map_err(|error| format!("unable to run {}: {error}", binary.display()))?;
    let total_ns = elapsed_ns(started);
    validate_exit(binary, output.status)?;
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "{} returned invalid JSON: {error}; stderr: {}",
            binary.display(),
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    Ok(CliSample { total_ns, signatures: json_signatures(&value)? })
}

fn sample_peak_rss(source: &Path) -> Result<u64, String> {
    let binary = env::current_exe().map_err(|error| error.to_string())?;
    let output = Command::new(&binary)
        .arg("--memory-child")
        .arg(source)
        .output()
        .map_err(|error| format!("unable to run {}: {error}", binary.display()))?;
    if !output.status.success() {
        return Err(format!("memory child failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .map_err(|error| format!("memory child returned an invalid RSS value: {error}"))
}

fn run_memory_child(path: &Path) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("RSS samples require a release build".to_string());
    }
    let running = Arc::new(AtomicBool::new(true));
    let peak = Arc::new(AtomicUsize::new(0));
    let sampler_running = Arc::clone(&running);
    let sampler_peak = Arc::clone(&peak);
    let sampler = thread::spawn(move || {
        while sampler_running.load(Ordering::Relaxed) {
            if let Some(stats) = memory_stats() {
                sampler_peak.fetch_max(stats.physical_mem, Ordering::Relaxed);
            }
            thread::yield_now();
        }
        if let Some(stats) = memory_stats() {
            sampler_peak.fetch_max(stats.physical_mem, Ordering::Relaxed);
        }
    });

    let result = (|| {
        let source = fs::read_to_string(path)
            .map_err(|error| format!("unable to read {}: {error}", path.display()))?;
        let filters =
            [ConfigRuleFilter { severity: ConfigRuleSeverity::Deny, name: RULE.to_string() }];
        let session = LintSession::new(
            path.parent().unwrap_or_else(|| Path::new(".")),
            None,
            &filters,
            false,
        )?;
        let output = session.lint_text(path, &source)?;
        let json = serde_json::to_vec(&output).map_err(|error| error.to_string())?;
        black_box(json);
        Ok::<(), String>(())
    })();
    running.store(false, Ordering::Relaxed);
    sampler.join().map_err(|_| "RSS sampler thread panicked".to_string())?;
    result?;
    let peak = peak.load(Ordering::Relaxed);
    if peak == 0 {
        return Err("memory-stats could not read current-process RSS".to_string());
    }
    println!("{peak}");
    Ok(())
}

fn validate_exit(binary: &Path, status: std::process::ExitStatus) -> Result<(), String> {
    if matches!(status.code(), Some(0 | 1)) {
        Ok(())
    } else {
        Err(format!("{} exited with {status}", binary.display()))
    }
}

fn control_signatures(diagnostics: &[oxc_adapter::EngineDiagnostic]) -> Vec<DiagnosticSignature> {
    let mut signatures = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule.as_deref() == Some(RULE))
        .flat_map(|diagnostic| {
            diagnostic.labels.iter().map(|label| DiagnosticSignature {
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
                offset: label.offset,
                length: label.length,
            })
        })
        .collect::<Vec<_>>();
    signatures.sort_unstable();
    signatures
}

fn product_signatures(result: &tsrx_lint::Output) -> Vec<DiagnosticSignature> {
    let mut signatures = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == RULE)
        .flat_map(|diagnostic| {
            diagnostic.labels.iter().map(|label| DiagnosticSignature {
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
                offset: label.span.offset,
                length: label.span.length,
            })
        })
        .collect::<Vec<_>>();
    signatures.sort_unstable();
    signatures
}

fn json_signatures(value: &Value) -> Result<Vec<DiagnosticSignature>, String> {
    let diagnostics = value
        .get("diagnostics")
        .and_then(Value::as_array)
        .ok_or("JSON report has no diagnostics array")?;
    let mut signatures = Vec::new();
    for diagnostic in diagnostics {
        let code = diagnostic.get("code").and_then(Value::as_str).unwrap_or_default();
        let rule = diagnostic.get("rule").and_then(Value::as_str).unwrap_or_default();
        if !code.contains(RULE) && rule != RULE {
            continue;
        }
        let message = diagnostic.get("message").and_then(Value::as_str).unwrap_or_default();
        let labels = diagnostic
            .get("labels")
            .and_then(Value::as_array)
            .ok_or("diagnostic has no labels array")?;
        for label in labels {
            let span = label.get("span").ok_or("diagnostic label has no span")?;
            let offset = span
                .get("offset")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or("diagnostic label has invalid offset")?;
            let length = span
                .get("length")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or("diagnostic label has invalid length")?;
            signatures.push(DiagnosticSignature {
                code: code.to_string(),
                message: message.to_string(),
                offset,
                length,
            });
        }
    }
    signatures.sort_unstable();
    Ok(signatures)
}

fn distribution(values: &[u64], bytes: usize) -> Result<Distribution, String> {
    let p50_ns = percentile(values, 0.50)?;
    let p95_ns = percentile(values, 0.95)?;
    let p99_ns = percentile(values, 0.99)?;
    Ok(Distribution {
        samples: values.len(),
        p50_ns,
        p95_ns,
        p99_ns,
        p50_ms: ns_to_ms(p50_ns),
        p95_ms: ns_to_ms(p95_ns),
        p99_ms: ns_to_ms(p99_ns),
        median_mib_per_second: throughput(bytes, p50_ns),
        p95_mib_per_second: throughput(bytes, p95_ns),
    })
}

fn percentile(values: &[u64], quantile: f64) -> Result<u64, String> {
    if values.is_empty() {
        return Err("cannot summarize an empty sample set".to_string());
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = (quantile * sorted.len() as f64).ceil() as usize;
    Ok(sorted[rank.saturating_sub(1).min(sorted.len() - 1)])
}

fn throughput(bytes: usize, elapsed_ns: u64) -> f64 {
    if elapsed_ns == 0 {
        return f64::INFINITY;
    }
    (bytes as f64 / MEBIBYTE) / (elapsed_ns as f64 / 1_000_000_000.0)
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    numerator as f64 / denominator.max(1) as f64
}

fn ns_to_ms(value: u64) -> f64 {
    value as f64 / 1_000_000.0
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn minimum(name: &'static str, observed: f64, threshold: f64) -> Assertion {
    Assertion { name, comparison: ">=", observed, threshold, pass: observed >= threshold }
}

fn maximum(name: &'static str, observed: f64, threshold: f64) -> Assertion {
    Assertion { name, comparison: "<=", observed, threshold, pass: observed <= threshold }
}

fn boolean(name: &'static str, observed: bool) -> Assertion {
    Assertion {
        name,
        comparison: "==",
        observed: if observed { 1.0 } else { 0.0 },
        threshold: 1.0,
        pass: observed,
    }
}

fn build_corpus(target_bytes: usize) -> String {
    let mut source = String::with_capacity(target_bytes + 1024);
    source.push_str("type BenchmarkProps = { ready: boolean; value: number };\n\n");
    let mut index = 0usize;
    while source.len() < target_bytes {
        let debugger = if index.is_multiple_of(64) { "    debugger;\n" } else { "" };
        write!(
            source,
            "export function View{index:05}({{ ready, value }}: BenchmarkProps) @{{\n\
             \x20 const contact = \"@if@example.com\";\n\
             \x20 const label = `item-${{value}}-@else`;\n\
             \x20 // @if (false) {{ debugger; }}\n\
             \x20 @if (ready) {{\n\
             {debugger}\
             \x20   <main data-contact={{contact}} data-value={{value}}>{{label}}</main>;\n\
             \x20 }} @else {{\n\
             \x20   <aside>idle</aside>;\n\
             \x20 }}\n\
             }}\n\n"
        )
        .expect("writing to a String cannot fail");
        index += 1;
    }
    source
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn parse_args(arguments: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut arguments = arguments.peekable();
    let mut budget_path = None;
    let mut output_path = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--assert" => {
                budget_path =
                    Some(PathBuf::from(arguments.next().ok_or("--assert requires a budget file")?));
            }
            "--output" => {
                output_path =
                    Some(PathBuf::from(arguments.next().ok_or("--output requires a path")?));
            }
            "--help" | "-h" => {
                return Err(
                    "usage: oxc_tsrx_benchmark --assert <budgets.json> [--output <report.json>]"
                        .to_string(),
                );
            }
            value => return Err(format!("unknown benchmark option: {value}")),
        }
    }
    Ok(Args { budget_path: budget_path.ok_or("--assert <budgets.json> is required")?, output_path })
}

fn validate_budgets(budgets: &Budgets) -> Result<(), String> {
    if budgets.schema_version != 1 {
        return Err(format!("unsupported budget schema version {}", budgets.schema_version));
    }
    if budgets.warmups < 5
        || budgets.samples < 15
        || budgets.warm_10k_samples < 15
        || budgets.cold_process_samples < 20
        || budgets.rss_process_samples < 5
    {
        return Err(
            "budgets violate the frozen noise policy (5 warmups, 15 throughput, 20 cold, 5 RSS minimum)"
                .to_string(),
        );
    }
    if budgets.corpus_target_bytes < 10 * 1024
        || budgets.memory_corpus_target_bytes < budgets.corpus_target_bytes
    {
        return Err("benchmark corpora are too small or inconsistent".to_string());
    }
    Ok(())
}

fn ensure_binary(path: &Path, label: &str) -> Result<(), String> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("{label} binary does not exist: {}", path.display()))
    }
}

fn create_temp_directory() -> Result<PathBuf, String> {
    let nonce =
        SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_nanos();
    let directory = env::temp_dir()
        .join(format!("oxc-tsrx-native-lint-benchmark-{}-{nonce}", std::process::id()));
    fs::create_dir(&directory)
        .map_err(|error| format!("unable to create {}: {error}", directory.display()))?;
    Ok(directory)
}

fn host(budgets: &Budgets) -> Host {
    let stock_package = budgets
        .stock_oxlint_binary
        .parent()
        .and_then(Path::parent)
        .map(|path| path.join("package.json"));
    let stock_oxlint_version = stock_package
        .filter(|path| path.is_file())
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|source| serde_json::from_str::<Value>(&source).ok())
        .and_then(|value| value.get("version").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    Host {
        os: env::consts::OS,
        architecture: env::consts::ARCH,
        rustc: command_text("rustc", &["--version"]),
        system: command_text("uname", &["-a"]),
        build_profile: "release (codegen-units=1, thin LTO, panic=abort, stripped)",
        oxc_revision: OXC_REVISION,
        stock_oxlint_version,
    }
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
