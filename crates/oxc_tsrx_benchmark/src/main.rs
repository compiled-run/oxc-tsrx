//! The lint performance harness: it measures this project against stock Oxlint on equivalent
//! sources and fails the run when a declared budget is missed.

// Benchmark math intentionally converts bounded byte/nanosecond counters to floating point for
// human-readable rates. TSX/TSRX names and `_ns` fields are units, not accidental similarities.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::struct_field_names,
    clippy::too_many_lines,
    reason = "benchmark math converts bounded counters to floating point and the report structs name their units"
)]

mod budgets;
mod fixtures;
mod in_process;
mod process;
mod report;
mod signatures;
mod stats;

use std::{
    env, fs,
    hint::black_box,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

use oxc_adapter::{LintEngine, LintEngineOptions};
use tsrx_lint::{ConfigRuleFilter, ConfigRuleSeverity, LintSession};
use tsrx_syntax::{project, scan};

use crate::{
    budgets::{Budgets, ensure_binary, parse_args, resolve_incumbent_binary, validate_budgets},
    fixtures::{build_corpus, create_temp_directory, fnv1a64},
    in_process::{measure_config_session, measure_control, measure_product},
    process::{run_memory_child, run_process_measurements},
    report::{
        Corpus, P01Summary, P02Summary, P03Summary, P05Summary, P07Summary, RawSamples, Report,
        ReportedAssertion, Summaries, host,
    },
    stats::{boolean, distribution, maximum, minimum, percentile, ratio},
};

pub(crate) const RULE: &str = "no-debugger";
pub(crate) const STARTUP_RULES: [&str; 2] = ["no-debugger", "no-unused-vars"];

#[expect(
    clippy::print_stderr,
    reason = "the benchmark binary reports failures on stderr and exits non-zero"
)]
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

#[expect(
    clippy::print_stdout,
    reason = "the benchmark binary's report on stdout is its contract with the docs site"
)]
fn run() -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("performance gates require a release build".to_string());
    }

    let args = parse_args(env::args().skip(1))?;
    let budget_source = fs::read_to_string(&args.budget_path).map_err(|error| {
        format!("unable to read budgets {}: {error}", args.budget_path.display())
    })?;
    let mut budgets: Budgets = serde_json::from_str(&budget_source)
        .map_err(|error| format!("invalid benchmark budgets: {error}"))?;
    validate_budgets(&budgets)?;
    ensure_binary(&budgets.candidate_binary, "candidate")?;
    // The report must publish the budget file's own declared paths, so the declared copy is taken
    // before the working copy's incumbent path is resolved against the installed layout.
    let declared_budgets = budgets.clone();
    budgets.stock_oxlint_binary =
        resolve_incumbent_binary(&budgets.stock_oxlint_binary, "stock Oxlint")?;

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

    // Every threshold above reads straight from `budgets.json` except P07's, which is
    // `min(ratio, additive)` over the measured equivalent-TSX RSS and therefore moves by a
    // page between two runs of the same build. Publish the rule that produced it, stated
    // purely in frozen budget numbers, so the adjudicator can prove a rerun shares the same
    // budget without demanding a byte-identical measured limit.
    let p07_derivation = format!(
        "min(candidateTsxMedianRssBytes * {}, candidateTsxMedianRssBytes + {})",
        declared_budgets.p07.upstream_ratio_max, declared_budgets.p07.additive_bytes_max
    );
    let assertions = assertions
        .into_iter()
        .map(|assertion| {
            let threshold_derivation = (assertion.name == "P07 TSRX peak RSS")
                .then(|| p07_derivation.clone());
            ReportedAssertion { assertion, threshold_derivation }
        })
        .collect::<Vec<_>>();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    let report = Report {
        schema_version: 1,
        generated_at_unix_ms: timestamp,
        host: host(&budgets),
        budgets: declared_budgets,
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
            .filter(|reported| !reported.assertion.pass)
            .map(|reported| reported.assertion.name)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("performance assertions failed: {failures}"));
    }
    Ok(())
}
