//! The results JSON's schema, and the host facts recorded beside it so an archived run stays
//! comparable to a later one.

use std::{env, fs, path::Path, process::Command};

use oxc_adapter::OXC_REVISION;
use serde::Serialize;
use serde_json::Value;

use crate::{
    budgets::Budgets,
    stats::{Assertion, Distribution},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Host {
    pub(crate) os: &'static str,
    pub(crate) architecture: &'static str,
    pub(crate) rustc: String,
    pub(crate) system: String,
    pub(crate) build_profile: &'static str,
    pub(crate) oxc_revision: &'static str,
    pub(crate) stock_oxlint_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Corpus {
    pub(crate) kind: &'static str,
    pub(crate) bytes: usize,
    pub(crate) fnv1a64: String,
    pub(crate) structural_forms: [&'static str; 3],
    pub(crate) diagnostic_rule: &'static str,
    pub(crate) note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct P01Summary {
    pub(crate) control: Distribution,
    pub(crate) candidate_standard_path: Distribution,
    pub(crate) median_latency_ratio: f64,
    pub(crate) p95_latency_ratio: f64,
    pub(crate) stock_oxlint_cli_reference: Distribution,
    pub(crate) candidate_standard_cli: Distribution,
    pub(crate) cli_median_latency_ratio: f64,
    pub(crate) diagnostic_parity: bool,
    pub(crate) direct_bypass: bool,
    pub(crate) comparison_basis: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct P02Summary {
    pub(crate) projected_scan_copy_parse: Distribution,
    pub(crate) equivalent_tsx_parse: Distribution,
    pub(crate) equivalent_tsx_throughput_ratio: f64,
    pub(crate) warm_10k_scan_copy_parse: Distribution,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct P03Summary {
    pub(crate) in_process_projected_lint: Distribution,
    pub(crate) in_process_equivalent_tsx_lint: Distribution,
    pub(crate) in_process_latency_ratio: f64,
    pub(crate) cli_projected_lint: Distribution,
    pub(crate) cli_equivalent_tsx_lint: Distribution,
    pub(crate) cli_latency_ratio: f64,
    pub(crate) diagnostic_parity: bool,
    pub(crate) threads: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct P05Summary {
    pub(crate) candidate_tsrx_cold_process: Distribution,
    pub(crate) stock_oxlint_tsx_cold_process: Distribution,
    pub(crate) p95_latency_ratio: f64,
    pub(crate) fresh_processes: bool,
    pub(crate) rules: [&'static str; 2],
    pub(crate) diagnostic_parity: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct P07Summary {
    pub(crate) candidate_tsrx_rss_bytes: Vec<u64>,
    pub(crate) candidate_tsx_rss_bytes: Vec<u64>,
    pub(crate) candidate_tsrx_median_rss_bytes: u64,
    pub(crate) candidate_tsx_median_rss_bytes: u64,
    pub(crate) allowed_rss_bytes: u64,
    pub(crate) measurement: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigSessionSummary {
    pub(crate) config_loads: u32,
    pub(crate) config_load_ns: u64,
    pub(crate) files: u32,
    pub(crate) parse_count: u32,
    pub(crate) configured_rule_diagnostics: usize,
    pub(crate) configured_rule_applied: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Summaries {
    pub(crate) p01: P01Summary,
    pub(crate) p02: P02Summary,
    pub(crate) p03: P03Summary,
    pub(crate) p05: P05Summary,
    pub(crate) p07: P07Summary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawSamples {
    pub(crate) control_before_ns: Vec<u64>,
    pub(crate) control_after_ns: Vec<u64>,
    pub(crate) candidate_standard_total_ns: Vec<u64>,
    pub(crate) candidate_standard_parse_ns: Vec<u64>,
    pub(crate) candidate_tsrx_total_ns: Vec<u64>,
    pub(crate) candidate_tsrx_scan_ns: Vec<u64>,
    pub(crate) candidate_tsrx_projection_ns: Vec<u64>,
    pub(crate) candidate_tsrx_parse_ns: Vec<u64>,
    pub(crate) candidate_tsrx_semantic_ns: Vec<u64>,
    pub(crate) candidate_tsrx_lint_ns: Vec<u64>,
    pub(crate) warm_10k_scan_copy_parse_ns: Vec<u64>,
    pub(crate) stock_cli_before_ns: Vec<u64>,
    pub(crate) stock_cli_after_ns: Vec<u64>,
    pub(crate) candidate_standard_cli_ns: Vec<u64>,
    pub(crate) candidate_tsrx_cli_ns: Vec<u64>,
    pub(crate) stock_cold_cli_ns: Vec<u64>,
    pub(crate) candidate_cold_cli_ns: Vec<u64>,
}

/// An assertion as it appears in the results JSON: the raw comparison plus, for the one
/// threshold in this family that is not read straight from `budgets.json`, the rule that
/// produced it. The adjudicator compares that rule instead of the numeric limit, so a
/// page-granular difference in measured RSS between two reruns of the same build cannot be
/// mistaken for a budget change. The string is a pure function of frozen budget fields and
/// never carries a measured value.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReportedAssertion {
    #[serde(flatten)]
    pub(crate) assertion: Assertion,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) threshold_derivation: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Report {
    pub(crate) schema_version: u32,
    pub(crate) generated_at_unix_ms: u128,
    pub(crate) host: Host,
    pub(crate) budgets: Budgets,
    pub(crate) corpus: Corpus,
    pub(crate) config_session: ConfigSessionSummary,
    pub(crate) summaries: Summaries,
    pub(crate) raw_samples: RawSamples,
    pub(crate) assertions: Vec<ReportedAssertion>,
    pub(crate) passed: bool,
    pub(crate) limitations: [&'static str; 3],
}

pub(crate) fn host(budgets: &Budgets) -> Host {
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

pub(crate) fn command_text(program: &str, arguments: &[&str]) -> String {
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
