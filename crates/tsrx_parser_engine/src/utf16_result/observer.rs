//! The counting seam the repair lanes report through, so the nonshipping measurement sibling can
//! see every copy while production compiles the instrumentation away.

use tsrx_tape_schema::FlatTape;

use crate::source_bridge::PreparedSource;

#[derive(Debug, Clone, Copy)]
pub(crate) enum RepairCopyLane {
    ProgramRaw,
    ProgramSemantic,
    Module,
    Comment,
    Codeframe,
}

pub(crate) trait Utf16WorkObserver {
    #[inline(always)]
    fn record_scan(&mut self) {}

    #[inline(always)]
    fn record_bridge(&mut self, _source: &PreparedSource<'_>) {}

    #[inline(always)]
    fn record_projection(&mut self, _projected_bytes: usize, _map_bytes: usize) {}

    #[inline(always)]
    fn record_tape(&mut self, _tape: &FlatTape) {}

    #[inline(always)]
    fn record_copy(&mut self, _lane: RepairCopyLane, _utf16_units: usize) {}

    #[inline(always)]
    fn record_program_compaction(&mut self) {}
}

pub(crate) struct NoopUtf16WorkObserver;

impl Utf16WorkObserver for NoopUtf16WorkObserver {}

#[cfg(test)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Utf16Work {
    pub(crate) bridge_observations: usize,
    pub(crate) bridge: crate::source_bridge::BridgeWork,
    pub(crate) program_raw_units: usize,
    pub(crate) program_semantic_units: usize,
    pub(crate) module_units: usize,
    pub(crate) comment_units: usize,
    pub(crate) codeframe_units: usize,
    pub(crate) program_compactions: usize,
}

#[cfg(test)]
impl Utf16Work {
    pub(crate) fn restored_units(self) -> usize {
        self.program_raw_units
            .saturating_add(self.program_semantic_units)
            .saturating_add(self.module_units)
            .saturating_add(self.comment_units)
            .saturating_add(self.codeframe_units)
    }

    pub(crate) fn restored_bytes(self) -> usize {
        self.restored_units().saturating_mul(size_of::<u16>())
    }
}

#[cfg(test)]
impl Utf16WorkObserver for Utf16Work {
    fn record_bridge(&mut self, source: &PreparedSource<'_>) {
        self.bridge_observations = self.bridge_observations.saturating_add(1);
        self.bridge = source.work();
    }

    fn record_copy(&mut self, lane: RepairCopyLane, utf16_units: usize) {
        let counter = match lane {
            RepairCopyLane::ProgramRaw => &mut self.program_raw_units,
            RepairCopyLane::ProgramSemantic => &mut self.program_semantic_units,
            RepairCopyLane::Module => &mut self.module_units,
            RepairCopyLane::Comment => &mut self.comment_units,
            RepairCopyLane::Codeframe => &mut self.codeframe_units,
        };
        *counter = counter.saturating_add(utf16_units);
    }

    fn record_program_compaction(&mut self) {
        self.program_compactions = self.program_compactions.saturating_add(1);
    }
}
