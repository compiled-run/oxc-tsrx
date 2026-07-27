//! Percentiles, throughput, and the pass/fail assertion records the report carries.

use std::time::Instant;

use serde::Serialize;

pub(crate) const MEBIBYTE: f64 = 1_048_576.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Distribution {
    pub(crate) samples: usize,
    pub(crate) p50_ns: u64,
    pub(crate) p95_ns: u64,
    pub(crate) p99_ns: u64,
    pub(crate) p50_ms: f64,
    pub(crate) p95_ms: f64,
    pub(crate) p99_ms: f64,
    pub(crate) median_mib_per_second: f64,
    pub(crate) p95_mib_per_second: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Assertion {
    pub(crate) name: &'static str,
    pub(crate) comparison: &'static str,
    pub(crate) observed: f64,
    pub(crate) threshold: f64,
    pub(crate) pass: bool,
}

pub(crate) fn distribution(values: &[u64], bytes: usize) -> Result<Distribution, String> {
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

pub(crate) fn percentile(values: &[u64], quantile: f64) -> Result<u64, String> {
    if values.is_empty() {
        return Err("cannot summarize an empty sample set".to_string());
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = (quantile * sorted.len() as f64).ceil() as usize;
    Ok(sorted[rank.saturating_sub(1).min(sorted.len() - 1)])
}

pub(crate) fn throughput(bytes: usize, elapsed_ns: u64) -> f64 {
    if elapsed_ns == 0 {
        return f64::INFINITY;
    }
    (bytes as f64 / MEBIBYTE) / (elapsed_ns as f64 / 1_000_000_000.0)
}

pub(crate) fn ratio(numerator: u64, denominator: u64) -> f64 {
    numerator as f64 / denominator.max(1) as f64
}

pub(crate) fn ns_to_ms(value: u64) -> f64 {
    value as f64 / 1_000_000.0
}

pub(crate) fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

pub(crate) fn minimum(name: &'static str, observed: f64, threshold: f64) -> Assertion {
    Assertion { name, comparison: ">=", observed, threshold, pass: observed >= threshold }
}

pub(crate) fn maximum(name: &'static str, observed: f64, threshold: f64) -> Assertion {
    Assertion { name, comparison: "<=", observed, threshold, pass: observed <= threshold }
}

pub(crate) fn boolean(name: &'static str, observed: bool) -> Assertion {
    Assertion {
        name,
        comparison: "==",
        observed: if observed { 1.0 } else { 0.0 },
        threshold: 1.0,
        pass: observed,
    }
}
