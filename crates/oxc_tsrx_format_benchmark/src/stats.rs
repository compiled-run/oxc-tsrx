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
pub(crate) struct PhaseDistribution {
    pub(crate) samples: usize,
    pub(crate) p50_ns: u64,
    pub(crate) p95_ns: u64,
    pub(crate) p99_ns: u64,
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

pub(crate) fn distribution(samples: &[u64], bytes: usize) -> Distribution {
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

pub(crate) fn phase_distribution(samples: &[u64]) -> PhaseDistribution {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    PhaseDistribution {
        samples: samples.len(),
        p50_ns: percentile(&sorted, 50),
        p95_ns: percentile(&sorted, 95),
        p99_ns: percentile(&sorted, 99),
    }
}

pub(crate) fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    let rank = sorted.len().saturating_mul(percentile).div_ceil(100);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

pub(crate) fn throughput(bytes: usize, nanoseconds: u64) -> f64 {
    (bytes as f64 / MEBIBYTE) / (nanoseconds as f64 / 1_000_000_000.0)
}

pub(crate) fn ratio(numerator: u64, denominator: u64) -> f64 {
    numerator as f64 / denominator.max(1) as f64
}

pub(crate) fn ns_to_ms(value: u64) -> f64 {
    value as f64 / 1_000_000.0
}

pub(crate) fn median_u64(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

pub(crate) fn assert_min(
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

pub(crate) fn assert_max(
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

pub(crate) fn assert_bool(assertions: &mut Vec<Assertion>, name: &'static str, value: bool) {
    assertions.push(Assertion {
        name,
        comparison: "required boolean invariant",
        observed: f64::from(value),
        threshold: 1.0,
        pass: value,
    });
}

pub(crate) fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
