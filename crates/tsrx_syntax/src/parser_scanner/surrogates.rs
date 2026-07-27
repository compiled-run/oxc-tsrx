//! Classifying which opaque lexical context each lone UTF-16 surrogate falls in. This is the one
//! job the scanner does for the UTF-16 bridge rather than for projection.

use crate::diagnostics::ProjectionError;

use super::Scanner;

/// Opaque lexical context in which an actual lone UTF-16 surrogate is reference-valid.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpaqueSurrogateContext {
    QuotedString = 1,
    TemplateRaw = 2,
    RegexBody = 3,
    Comment = 4,
    JsxText = 5,
    RawStyle = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProbeState {
    Unseen,
    Seen(OpaqueSurrogateContext),
    Conflict,
}

pub(super) struct SurrogateProbes {
    offsets: Vec<u32>,
    states: Vec<ProbeState>,
    pub(super) changes: Vec<(usize, ProbeState)>,
}

impl SurrogateProbes {
    pub(super) fn new(offsets: &[u32]) -> Self {
        Self {
            offsets: offsets.to_vec(),
            states: vec![ProbeState::Unseen; offsets.len()],
            changes: Vec::new(),
        }
    }

    fn mark(&mut self, start: usize, end: usize, context: OpaqueSurrogateContext) {
        let Ok(start) = u32::try_from(start) else {
            return;
        };
        let Ok(end) = u32::try_from(end) else {
            return;
        };
        let first = self.offsets.partition_point(|offset| *offset < start);
        let last = self.offsets.partition_point(|offset| *offset < end);
        for index in first..last {
            let previous = self.states[index];
            let next = match previous {
                ProbeState::Unseen => ProbeState::Seen(context),
                ProbeState::Seen(previous) if previous == context => continue,
                ProbeState::Seen(_) | ProbeState::Conflict => ProbeState::Conflict,
            };
            self.changes.push((index, previous));
            self.states[index] = next;
        }
    }

    pub(super) fn rollback(&mut self, change_count: usize) {
        while self.changes.len() > change_count {
            let (index, previous) = self.changes.pop().expect("change length was checked");
            self.states[index] = previous;
        }
    }

    fn contexts(&self) -> Vec<Option<OpaqueSurrogateContext>> {
        self.states
            .iter()
            .map(|state| match state {
                ProbeState::Seen(context) => Some(*context),
                ProbeState::Unseen | ProbeState::Conflict => None,
            })
            .collect()
    }
}
impl Scanner<'_> {
    pub(crate) fn classify_surrogates(self) -> Vec<Option<OpaqueSurrogateContext>> {
        self.classify_surrogates_detailed().0
    }

    pub(crate) fn classify_surrogates_detailed(
        mut self,
    ) -> (Vec<Option<OpaqueSurrogateContext>>, Option<ProjectionError>) {
        if self.surrogate_probes.is_none() {
            return (Vec::new(), None);
        }
        let error = self.scan_region(0, None).err();
        let contexts = self
            .surrogate_probes
            .as_deref()
            .expect("classification scanner has probes")
            .borrow()
            .contexts();
        (contexts, error)
    }

    pub(super) fn mark_surrogates(
        &self,
        start: usize,
        end: usize,
        context: OpaqueSurrogateContext,
    ) {
        if let Some(probes) = self.surrogate_probes.as_deref() {
            probes.borrow_mut().mark(start, end, context);
        }
    }
}
