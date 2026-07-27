//! The budget file's schema, and the argument parsing that locates it. Every threshold this
//! harness asserts is declared there rather than in code.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) budget_path: PathBuf,
    pub(crate) output_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Budgets {
    pub(crate) schema_version: u32,
    pub(crate) warmups: usize,
    pub(crate) samples: usize,
    pub(crate) warm_10k_samples: usize,
    pub(crate) cold_process_samples: usize,
    pub(crate) rss_process_samples: usize,
    pub(crate) corpus_target_bytes: usize,
    pub(crate) memory_corpus_target_bytes: usize,
    pub(crate) candidate_binary: PathBuf,
    pub(crate) stock_oxlint_binary: PathBuf,
    pub(crate) p01: P01Budget,
    pub(crate) p02: P02Budget,
    pub(crate) p03: P03Budget,
    pub(crate) p05: P05Budget,
    pub(crate) p07: P07Budget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct P01Budget {
    pub(crate) median_latency_ratio_max: f64,
    pub(crate) p95_latency_ratio_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct P02Budget {
    pub(crate) median_mib_per_second_min: f64,
    pub(crate) p95_mib_per_second_min: f64,
    pub(crate) equivalent_tsx_ratio_min: f64,
    pub(crate) warm_10k_p95_ms_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct P03Budget {
    pub(crate) one_thread_median_mib_per_second_min: f64,
    pub(crate) end_to_end_latency_ratio_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct P05Budget {
    pub(crate) cold_p95_ms_max: f64,
    pub(crate) upstream_latency_ratio_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct P07Budget {
    pub(crate) upstream_ratio_max: f64,
    pub(crate) additive_bytes_max: u64,
}

pub(crate) fn parse_args(mut arguments: impl Iterator<Item = String>) -> Result<Args, String> {
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

pub(crate) fn validate_budgets(budgets: &Budgets) -> Result<(), String> {
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

pub(crate) fn ensure_binary(path: &Path, label: &str) -> Result<(), String> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("{label} binary does not exist: {}", path.display()))
    }
}
