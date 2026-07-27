//! The subprocess measurements: cold start, peak resident memory, and CLI diagnostic parity, none
//! of which are observable from inside this process.

use std::{
    env, fs,
    hint::black_box,
    path::Path,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Instant,
};

use memory_stats::memory_stats;
use serde_json::Value;
use tsrx_lint::{ConfigRuleFilter, ConfigRuleSeverity, LintSession};
use tsrx_syntax::{project, scan};

use crate::{
    RULE, STARTUP_RULES,
    budgets::Budgets,
    fixtures::build_corpus,
    signatures::{DiagnosticSignature, json_signatures},
    stats::elapsed_ns,
};

#[derive(Debug)]
pub(crate) struct CliSample {
    pub(crate) total_ns: u64,
    pub(crate) signatures: Vec<DiagnosticSignature>,
}

#[derive(Debug)]
pub(crate) struct ProcessMeasurements {
    pub(crate) stock_cli_before: Vec<CliSample>,
    pub(crate) stock_cli_after: Vec<CliSample>,
    pub(crate) candidate_standard_cli: Vec<CliSample>,
    pub(crate) candidate_tsrx_cli: Vec<CliSample>,
    pub(crate) stock_cold_cli_ns: Vec<u64>,
    pub(crate) candidate_cold_cli_ns: Vec<u64>,
    pub(crate) candidate_tsx_rss: Vec<u64>,
    pub(crate) candidate_tsrx_rss: Vec<u64>,
    pub(crate) diagnostic_parity: bool,
    pub(crate) startup_diagnostic_parity: bool,
}

pub(crate) fn run_process_measurements(
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

pub(crate) fn run_cli(binary: &Path, source: &Path) -> Result<CliSample, String> {
    run_cli_with_rules(binary, source, &[RULE])
}

pub(crate) fn run_cli_with_rules(
    binary: &Path,
    source: &Path,
    rules: &[&str],
) -> Result<CliSample, String> {
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

pub(crate) fn sample_peak_rss(source: &Path) -> Result<u64, String> {
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

#[expect(
    clippy::print_stdout,
    reason = "the memory child process hands its measurement back to the parent on stdout"
)]
pub(crate) fn run_memory_child(path: &Path) -> Result<(), String> {
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

pub(crate) fn validate_exit(binary: &Path, status: std::process::ExitStatus) -> Result<(), String> {
    if matches!(status.code(), Some(0 | 1)) {
        Ok(())
    } else {
        Err(format!("{} exited with {status}", binary.display()))
    }
}
