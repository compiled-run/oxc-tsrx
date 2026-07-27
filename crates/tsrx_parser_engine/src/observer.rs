use std::mem::size_of;

use tsrx_tape_schema::{
    FieldRecord, FlatTape, ListRecord, ListValueRecord, ObjectRecord, RecordIndex, ValueRef,
};

use crate::{source_bridge::PreparedSource, utf16_result, utf16_result::Utf16WorkObserver};

/// Route-owned work observed by the nonshipping Stage 4 qualification sibling.
///
/// The feature that exposes this type is disabled in production. Each field is accumulated at the
/// operation that owns the work; the Node-API sibling only publishes the resulting totals.
#[cfg(feature = "stage4-observer")]
#[doc(hidden)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stage4WorkCounters {
    pub scans: usize,
    pub copied_bytes: usize,
    pub projection_bytes: usize,
    pub map_bytes: usize,
    pub surrogate_bytes: usize,
    pub tape_bytes: usize,
}

#[cfg(feature = "stage4-observer")]
impl Utf16WorkObserver for Stage4WorkCounters {
    fn record_scan(&mut self) {
        self.scans = self.scans.saturating_add(1);
    }

    fn record_bridge(&mut self, source: &PreparedSource<'_>) {
        let work = source.work();
        self.copied_bytes = self.copied_bytes.saturating_add(work.utf8_bytes);
        self.map_bytes = self.map_bytes.saturating_add(work.boundary_bytes);
        self.surrogate_bytes = self.surrogate_bytes.saturating_add(work.fixup_bytes);
    }

    fn record_projection(&mut self, projected_bytes: usize, map_bytes: usize) {
        self.projection_bytes = self.projection_bytes.saturating_add(projected_bytes);
        self.map_bytes = self.map_bytes.saturating_add(map_bytes);
    }

    fn record_tape(&mut self, tape: &FlatTape) {
        self.tape_bytes = self.tape_bytes.saturating_add(logical_tape_bytes(tape));
    }

    fn record_copy(&mut self, _lane: utf16_result::RepairCopyLane, utf16_units: usize) {
        self.copied_bytes =
            self.copied_bytes.saturating_add(utf16_units.saturating_mul(size_of::<u16>()));
    }
}

#[cfg(feature = "stage4-observer")]
fn logical_tape_bytes(tape: &FlatTape) -> usize {
    let key_bytes = (0..tape.object_count()).fold(0_usize, |total, index| {
        let Ok(index) = u32::try_from(index) else {
            return usize::MAX;
        };
        tape.fields(RecordIndex::new(index))
            .fold(total, |total, field| total.saturating_add(tape.key(field).len()))
    });
    size_of::<u16>()
        .saturating_add(size_of::<ValueRef>())
        .saturating_add(tape.object_count().saturating_mul(size_of::<ObjectRecord>()))
        .saturating_add(tape.field_count().saturating_mul(size_of::<FieldRecord>()))
        .saturating_add(tape.list_count().saturating_mul(size_of::<ListRecord>()))
        .saturating_add(tape.list_value_count().saturating_mul(size_of::<ListValueRecord>()))
        .saturating_add(key_bytes)
        .saturating_add(tape.scalar_storage().len())
}
