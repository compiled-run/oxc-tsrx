//! Nanosecond measurements every toolchain lane reports back to its caller.

use std::time::Instant;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FormatEngineTimings {
    pub parse_ns: u64,
    pub format_ns: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EngineTimings {
    pub parse_ns: u64,
    pub semantic_ns: u64,
    pub lint_ns: u64,
}

pub(super) fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
