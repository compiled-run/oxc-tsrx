use std::{fmt, mem};

use crate::{ListRange, ProjectedCommentKind, RecordIndex, StringRange, TapeBuildError, TapeSpan};

use bounds::{
    append_records, begin_direct_range, finish_direct_range, index_usize, list_range, push_record,
    range_slice, string_range, table_has_room,
};
use packed_text::PackedTextStorage;
use spans::{try_map_optional_span, try_map_optional_value_span, try_map_span};

mod bounds;
mod kinds;
mod packed_text;
mod records;
mod spans;
#[cfg(test)]
mod tests;

pub use kinds::{
    DiagnosticPhase, DiagnosticSeverity, ExportExportNameKind, ExportImportNameKind,
    ExportLocalNameKind, ImportNameKind,
};
pub use packed_text::{OwnedPackedTextStorage, PackedTextRef};
pub use records::{
    CommentRecord, DiagnosticLabelRecord, DiagnosticRecord, DynamicImportRecord, ModuleNameRecord,
    PackedStringWriter, StaticExportEntryRecord, StaticExportRecord, StaticImportEntryRecord,
    StaticImportRecord,
};
pub use spans::{OptionalStringRange, OptionalTapeSpan, OptionalValueSpanRecord, ValueSpanRecord};

/// Whether a canonical parse produced a complete, recovered, or failed result.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseCompleteness {
    Complete = 1,
    Recovered = 2,
    Failed = 3,
}

/// Coordinate system used by every span in one result.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoordinateDomain {
    ProjectedUtf8Bytes = 1,
    AuthoredUtf8Bytes = 2,
    OriginalUtf16Units = 3,
}

/// Owned, OXC-independent flat module record.
#[derive(Debug, Default)]
pub struct ModuleTable {
    has_module_syntax: bool,
    static_imports: Vec<StaticImportRecord>,
    static_import_entries: Vec<StaticImportEntryRecord>,
    static_exports: Vec<StaticExportRecord>,
    static_export_entries: Vec<StaticExportEntryRecord>,
    dynamic_imports: Vec<DynamicImportRecord>,
    import_metas: Vec<TapeSpan>,
    strings: PackedTextStorage,
}

impl ModuleTable {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            has_module_syntax: false,
            static_imports: Vec::new(),
            static_import_entries: Vec::new(),
            static_exports: Vec::new(),
            static_export_entries: Vec::new(),
            dynamic_imports: Vec::new(),
            import_metas: Vec::new(),
            strings: PackedTextStorage::new(),
        }
    }

    pub fn set_has_module_syntax(&mut self, has_module_syntax: bool) {
        self.has_module_syntax = has_module_syntax;
    }

    /// Maps every source span stored by this module table in one allocation-free pass.
    ///
    /// Packed string and list ranges are storage coordinates and are never passed to `mapper`.
    /// Each present source span is passed exactly once. If `mapper` fails, its error is returned
    /// immediately; spans mapped before that error remain updated, so callers should discard the
    /// table on failure.
    ///
    /// # Errors
    ///
    /// Returns the first error produced by `mapper`.
    pub fn try_map_spans<E>(
        &mut self,
        mut mapper: impl FnMut(TapeSpan) -> Result<TapeSpan, E>,
    ) -> Result<(), E> {
        for record in &mut self.static_imports {
            try_map_span(&mut record.span, &mut mapper)?;
            try_map_span(&mut record.module_request.span, &mut mapper)?;
        }
        for entry in &mut self.static_import_entries {
            try_map_optional_span(&mut entry.import_name.span, &mut mapper)?;
            try_map_span(&mut entry.local_name.span, &mut mapper)?;
        }
        for record in &mut self.static_exports {
            try_map_span(&mut record.span, &mut mapper)?;
        }
        for entry in &mut self.static_export_entries {
            try_map_span(&mut entry.span, &mut mapper)?;
            try_map_optional_value_span(&mut entry.module_request, &mut mapper)?;
            try_map_optional_span(&mut entry.import_name.span, &mut mapper)?;
            try_map_optional_span(&mut entry.export_name.span, &mut mapper)?;
            try_map_optional_span(&mut entry.local_name.span, &mut mapper)?;
        }
        for record in &mut self.dynamic_imports {
            try_map_span(&mut record.span, &mut mapper)?;
            try_map_span(&mut record.module_request, &mut mapper)?;
        }
        for span in &mut self.import_metas {
            try_map_span(span, &mut mapper)?;
        }
        Ok(())
    }

    /// Destructively takes the static-import columns for authored reconstruction.
    #[must_use]
    pub fn take_static_imports(
        &mut self,
    ) -> (Vec<StaticImportRecord>, Vec<StaticImportEntryRecord>) {
        (mem::take(&mut self.static_imports), mem::take(&mut self.static_import_entries))
    }

    /// Destructively takes the static-export columns for authored reconstruction.
    #[must_use]
    pub fn take_static_exports(
        &mut self,
    ) -> (Vec<StaticExportRecord>, Vec<StaticExportEntryRecord>) {
        (mem::take(&mut self.static_exports), mem::take(&mut self.static_export_entries))
    }

    /// Destructively takes the dynamic-import column for authored reconstruction.
    #[must_use]
    pub fn take_dynamic_imports(&mut self) -> Vec<DynamicImportRecord> {
        mem::take(&mut self.dynamic_imports)
    }

    /// Destructively takes the `import.meta` column for authored reconstruction.
    #[must_use]
    pub fn take_import_metas(&mut self) -> Vec<TapeSpan> {
        mem::take(&mut self.import_metas)
    }

    /// Destructively takes the packed module string buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if lossless surrogate fixups remain; use
    /// [`Self::take_text_storage`] for arbitrary JavaScript strings.
    pub fn take_string_storage(&mut self) -> Result<String, TapeBuildError> {
        self.strings.take_utf8()
    }

    /// Destructively takes module text without losing unpaired UTF-16 surrogate fixups.
    #[must_use]
    pub fn take_text_storage(&mut self) -> OwnedPackedTextStorage {
        self.strings.take_owned()
    }

    /// Returns true after every owned record and string allocation has been mem-taken.
    #[must_use]
    pub fn is_storage_released(&self) -> bool {
        self.static_imports.capacity() == 0
            && self.static_import_entries.capacity() == 0
            && self.static_exports.capacity() == 0
            && self.static_export_entries.capacity() == 0
            && self.dynamic_imports.capacity() == 0
            && self.import_metas.capacity() == 0
            && self.strings.is_released()
    }

    #[must_use]
    pub const fn has_module_syntax(&self) -> bool {
        self.has_module_syntax
    }

    /// Appends a module-owned string to packed storage.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit range limit is exceeded.
    pub fn push_string(&mut self, value: &str) -> Result<StringRange, TapeBuildError> {
        self.strings.push_str(value)
    }

    /// Appends exact JavaScript UTF-16 code units to packed module storage.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit range limit is exceeded.
    pub fn push_utf16(&mut self, value: &[u16]) -> Result<StringRange, TapeBuildError> {
        self.strings.push_utf16(value)
    }

    /// Attaches collision-safe UTF-16 fixups to an existing same-width module value.
    ///
    /// # Errors
    ///
    /// Returns an error unless `value` encodes to the exact existing UTF-8/placeholder bytes.
    pub fn repair_utf16(
        &mut self,
        range: StringRange,
        value: &[u16],
    ) -> Result<(), TapeBuildError> {
        self.strings.repair_utf16(range, value)
    }

    /// Attaches collision-safe UTF-16 fixups to ordered, disjoint module values in one merge.
    ///
    /// # Errors
    ///
    /// Returns an error unless ranges are ordered and disjoint and every value encodes to its
    /// exact existing UTF-8/placeholder bytes.
    pub fn repair_utf16_batch<'a>(
        &mut self,
        repairs: impl IntoIterator<Item = (StringRange, &'a [u16])>,
    ) -> Result<(), TapeBuildError> {
        self.strings.repair_utf16_batch(repairs)
    }

    #[must_use]
    pub fn string(&self, range: StringRange) -> Option<&str> {
        self.text(range)?.as_str()
    }

    /// Returns one lossless packed JavaScript string, including unpaired UTF-16 units.
    #[must_use]
    pub fn text(&self, range: StringRange) -> Option<PackedTextRef<'_>> {
        self.strings.text(range)
    }

    /// Returns the complete packed buffer only when every value is valid UTF-8.
    #[must_use]
    pub fn string_storage(&self) -> Option<&str> {
        self.strings.as_str()
    }

    #[must_use]
    pub fn value(&self, value: ValueSpanRecord) -> Option<&str> {
        self.string(value.value)
    }

    /// Returns one lossless module value, including unpaired UTF-16 units.
    #[must_use]
    pub fn value_text(&self, value: ValueSpanRecord) -> Option<PackedTextRef<'_>> {
        self.text(value.value)
    }

    #[must_use]
    pub fn optional_value(&self, value: OptionalValueSpanRecord) -> Option<&str> {
        self.value(value.get()?)
    }

    #[must_use]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the record is a small copyable index pair; borrowing it would add an indirection to a field read"
    )]
    pub fn name<K>(&self, name: ModuleNameRecord<K>) -> Option<&str> {
        self.string(name.name.get()?)
    }

    /// Captures the checked start cursor for a directly emitted static-import entry group.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the cursor exceeds the 32-bit range domain.
    pub fn begin_static_import_entries(&self) -> Result<u32, TapeBuildError> {
        begin_direct_range(&self.static_import_entries)
    }

    /// Finishes a directly emitted static-import entry group.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the checked range exceeds the 32-bit
    /// table domain. Returns
    /// [`TapeBuildError::InvalidRecordIndex`] unless exactly `expected_length` entries were
    /// appended after `start`.
    pub fn finish_static_import_entries(
        &self,
        start: u32,
        expected_length: u32,
    ) -> Result<ListRange, TapeBuildError> {
        finish_direct_range(&self.static_import_entries, start, expected_length)
    }

    /// Appends an import entry.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_static_import_entry(
        &mut self,
        record: StaticImportEntryRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.static_import_entries, record)
    }

    /// Appends a contiguous import-entry group without an intermediate allocation.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn append_static_import_entries<I>(
        &mut self,
        records: I,
    ) -> Result<ListRange, TapeBuildError>
    where
        I: IntoIterator<Item = StaticImportEntryRecord>,
    {
        append_records(&mut self.static_import_entries, records)
    }

    /// Appends a static import statement.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_static_import(
        &mut self,
        record: StaticImportRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.static_imports, record)
    }

    /// Appends an export entry.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_static_export_entry(
        &mut self,
        record: StaticExportEntryRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.static_export_entries, record)
    }

    /// Captures the checked start cursor for a directly emitted static-export entry group.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the cursor exceeds the 32-bit range domain.
    pub fn begin_static_export_entries(&self) -> Result<u32, TapeBuildError> {
        begin_direct_range(&self.static_export_entries)
    }

    /// Finishes a directly emitted static-export entry group.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the checked range exceeds the 32-bit
    /// table domain. Returns
    /// [`TapeBuildError::InvalidRecordIndex`] unless exactly `expected_length` entries were
    /// appended after `start`.
    pub fn finish_static_export_entries(
        &self,
        start: u32,
        expected_length: u32,
    ) -> Result<ListRange, TapeBuildError> {
        finish_direct_range(&self.static_export_entries, start, expected_length)
    }

    /// Appends a contiguous export-entry group without an intermediate allocation.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn append_static_export_entries<I>(
        &mut self,
        records: I,
    ) -> Result<ListRange, TapeBuildError>
    where
        I: IntoIterator<Item = StaticExportEntryRecord>,
    {
        append_records(&mut self.static_export_entries, records)
    }

    /// Appends a static export statement.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_static_export(
        &mut self,
        record: StaticExportRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.static_exports, record)
    }

    /// Appends a dynamic import expression.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_dynamic_import(
        &mut self,
        record: DynamicImportRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.dynamic_imports, record)
    }

    /// Appends an `import.meta` span.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_import_meta(&mut self, span: TapeSpan) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.import_metas, span)
    }

    #[must_use]
    pub fn static_imports(&self) -> &[StaticImportRecord] {
        &self.static_imports
    }

    #[must_use]
    pub fn static_import_entries(&self, range: ListRange) -> Option<&[StaticImportEntryRecord]> {
        range_slice(&self.static_import_entries, range)
    }

    #[must_use]
    pub fn static_exports(&self) -> &[StaticExportRecord] {
        &self.static_exports
    }

    #[must_use]
    pub fn static_export_entries(&self, range: ListRange) -> Option<&[StaticExportEntryRecord]> {
        range_slice(&self.static_export_entries, range)
    }

    #[must_use]
    pub fn dynamic_imports(&self) -> &[DynamicImportRecord] {
        &self.dynamic_imports
    }

    #[must_use]
    pub fn import_metas(&self) -> &[TapeSpan] {
        &self.import_metas
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.static_imports.is_empty()
            && self.static_exports.is_empty()
            && self.dynamic_imports.is_empty()
            && self.import_metas.is_empty()
    }
}

/// Independent packed comment table.
#[derive(Debug, Default)]
pub struct CommentTable {
    records: Vec<CommentRecord>,
    strings: PackedTextStorage,
}

impl CommentTable {
    #[must_use]
    pub const fn new() -> Self {
        Self { records: Vec::new(), strings: PackedTextStorage::new() }
    }

    /// Maps every comment source span in one allocation-free pass.
    ///
    /// Comment values and their packed string ranges are left untouched. If `mapper` fails, its
    /// error is returned immediately; spans mapped before that error remain updated, so callers
    /// should discard the table on failure.
    ///
    /// # Errors
    ///
    /// Returns the first error produced by `mapper`.
    pub fn try_map_spans<E>(
        &mut self,
        mut mapper: impl FnMut(TapeSpan) -> Result<TapeSpan, E>,
    ) -> Result<(), E> {
        for record in &mut self.records {
            try_map_span(&mut record.span, &mut mapper)?;
        }
        Ok(())
    }

    /// Appends one comment and copies its delimiter-free value once.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at a 32-bit table or string limit.
    pub fn push(
        &mut self,
        kind: ProjectedCommentKind,
        span: TapeSpan,
        value: &str,
    ) -> Result<RecordIndex, TapeBuildError> {
        table_has_room(&self.records)?;
        let value = self.strings.push_str(value)?;
        push_record(&mut self.records, CommentRecord::new(kind, span, value))
    }

    /// Appends one comment from exact JavaScript UTF-16 code units.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at a 32-bit table or string limit.
    pub fn push_utf16(
        &mut self,
        kind: ProjectedCommentKind,
        span: TapeSpan,
        value: &[u16],
    ) -> Result<RecordIndex, TapeBuildError> {
        table_has_room(&self.records)?;
        let string_start = self.strings.len();
        let fixup_start = self.strings.fixups.as_ref().map_or(0, Vec::len);
        let value = self.strings.push_utf16(value)?;
        match push_record(&mut self.records, CommentRecord::new(kind, span, value)) {
            Ok(index) => Ok(index),
            Err(error) => {
                self.strings.truncate(string_start, fixup_start);
                Err(error)
            }
        }
    }

    /// Attaches collision-safe UTF-16 fixups to an existing same-width comment value.
    ///
    /// # Errors
    ///
    /// Returns an error unless `value` encodes to the exact existing UTF-8/placeholder bytes.
    pub fn repair_utf16(
        &mut self,
        range: StringRange,
        value: &[u16],
    ) -> Result<(), TapeBuildError> {
        self.strings.repair_utf16(range, value)
    }

    /// Attaches collision-safe UTF-16 fixups to ordered, disjoint comment values in one merge.
    ///
    /// # Errors
    ///
    /// Returns an error unless ranges are ordered and disjoint and every value encodes to its
    /// exact existing UTF-8/placeholder bytes.
    pub fn repair_utf16_batch<'a>(
        &mut self,
        repairs: impl IntoIterator<Item = (StringRange, &'a [u16])>,
    ) -> Result<(), TapeBuildError> {
        self.strings.repair_utf16_batch(repairs)
    }

    #[must_use]
    pub fn records(&self) -> &[CommentRecord] {
        &self.records
    }

    #[must_use]
    pub fn value(&self, comment: &CommentRecord) -> Option<&str> {
        self.value_text(comment)?.as_str()
    }

    /// Returns one lossless comment value, including unpaired UTF-16 units.
    #[must_use]
    pub fn value_text(&self, comment: &CommentRecord) -> Option<PackedTextRef<'_>> {
        self.strings.text(comment.value)
    }

    /// Returns the complete packed buffer only when every value is valid UTF-8.
    #[must_use]
    pub fn string_storage(&self) -> Option<&str> {
        self.strings.as_str()
    }

    /// Destructively takes comment records for authored reconstruction.
    #[must_use]
    pub fn take_records(&mut self) -> Vec<CommentRecord> {
        mem::take(&mut self.records)
    }

    /// Destructively takes the packed comment string buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if lossless surrogate fixups remain; use
    /// [`Self::take_text_storage`] for arbitrary JavaScript strings.
    pub fn take_string_storage(&mut self) -> Result<String, TapeBuildError> {
        self.strings.take_utf8()
    }

    /// Destructively takes comment text without losing unpaired UTF-16 surrogate fixups.
    #[must_use]
    pub fn take_text_storage(&mut self) -> OwnedPackedTextStorage {
        self.strings.take_owned()
    }

    /// Returns true after every owned record and string allocation has been mem-taken.
    #[must_use]
    pub fn is_storage_released(&self) -> bool {
        self.records.capacity() == 0 && self.strings.is_released()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Independent packed diagnostic and label tables.
#[derive(Debug, Default)]
pub struct DiagnosticTable {
    records: Vec<DiagnosticRecord>,
    labels: Vec<DiagnosticLabelRecord>,
    strings: PackedTextStorage,
}

impl DiagnosticTable {
    #[must_use]
    pub const fn new() -> Self {
        Self { records: Vec::new(), labels: Vec::new(), strings: PackedTextStorage::new() }
    }

    /// Maps every diagnostic-label source span in one allocation-free pass.
    ///
    /// Diagnostic records contain only packed table/string ranges, not source spans, and are left
    /// untouched. If `mapper` fails, its error is returned immediately; spans mapped before that
    /// error remain updated, so callers should discard the table on failure.
    ///
    /// # Errors
    ///
    /// Returns the first error produced by `mapper`.
    pub fn try_map_spans<E>(
        &mut self,
        mut mapper: impl FnMut(TapeSpan) -> Result<TapeSpan, E>,
    ) -> Result<(), E> {
        for label in &mut self.labels {
            try_map_span(&mut label.span, &mut mapper)?;
        }
        Ok(())
    }

    /// Appends a diagnostic-owned string to packed storage.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit range limit is exceeded.
    pub fn push_string(&mut self, value: &str) -> Result<StringRange, TapeBuildError> {
        self.strings.push_str(value)
    }

    /// Appends exact JavaScript UTF-16 code units to packed diagnostic storage.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit range limit is exceeded.
    pub fn push_utf16(&mut self, value: &[u16]) -> Result<StringRange, TapeBuildError> {
        self.strings.push_utf16(value)
    }

    /// Attaches collision-safe UTF-16 fixups to ordered, disjoint diagnostic strings in one merge.
    ///
    /// # Errors
    ///
    /// Returns an error unless ranges are ordered and disjoint and every value encodes to its
    /// exact existing UTF-8/placeholder bytes.
    pub fn repair_utf16_batch<'a>(
        &mut self,
        repairs: impl IntoIterator<Item = (StringRange, &'a [u16])>,
    ) -> Result<(), TapeBuildError> {
        self.strings.repair_utf16_batch(repairs)
    }

    /// Appends an optional diagnostic-owned string.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit range limit is exceeded.
    pub fn push_optional_string(
        &mut self,
        value: Option<&str>,
    ) -> Result<OptionalStringRange, TapeBuildError> {
        value.map_or(Ok(OptionalStringRange::NONE), |value| {
            self.push_string(value).map(OptionalStringRange::some)
        })
    }

    #[must_use]
    pub fn string(&self, range: StringRange) -> Option<&str> {
        self.text(range)?.as_str()
    }

    /// Returns one lossless diagnostic string, including unpaired UTF-16 units.
    #[must_use]
    pub fn text(&self, range: StringRange) -> Option<PackedTextRef<'_>> {
        self.strings.text(range)
    }

    #[must_use]
    pub fn optional_string(&self, range: OptionalStringRange) -> Option<&str> {
        self.string(range.get()?)
    }

    /// Returns one optional lossless diagnostic string.
    #[must_use]
    pub fn optional_text(&self, range: OptionalStringRange) -> Option<PackedTextRef<'_>> {
        self.text(range.get()?)
    }

    /// Returns the complete packed buffer only when every value is valid UTF-8.
    #[must_use]
    pub fn string_storage(&self) -> Option<&str> {
        self.strings.as_str()
    }

    /// Destructively takes diagnostic and label columns for authored reconstruction.
    #[must_use]
    pub fn take_records_and_labels(
        &mut self,
    ) -> (Vec<DiagnosticRecord>, Vec<DiagnosticLabelRecord>) {
        (mem::take(&mut self.records), mem::take(&mut self.labels))
    }

    /// Destructively takes the packed diagnostic string buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if lossless surrogate fixups remain; use
    /// [`Self::take_text_storage`] for arbitrary JavaScript strings.
    pub fn take_string_storage(&mut self) -> Result<String, TapeBuildError> {
        self.strings.take_utf8()
    }

    /// Destructively takes diagnostic text without losing unpaired UTF-16 surrogate fixups.
    #[must_use]
    pub fn take_text_storage(&mut self) -> OwnedPackedTextStorage {
        self.strings.take_owned()
    }

    /// Returns true after every owned record and string allocation has been mem-taken.
    #[must_use]
    pub fn is_storage_released(&self) -> bool {
        self.records.capacity() == 0 && self.labels.capacity() == 0 && self.strings.is_released()
    }

    /// Appends one diagnostic label.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at a 32-bit table or string limit.
    pub fn push_label(
        &mut self,
        span: TapeSpan,
        message: Option<&str>,
    ) -> Result<RecordIndex, TapeBuildError> {
        self.push_labeled(span, message, false)
    }

    /// Appends one diagnostic label with its codeframe-primary marker.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at a 32-bit table or string limit.
    pub fn push_labeled(
        &mut self,
        span: TapeSpan,
        message: Option<&str>,
        primary: bool,
    ) -> Result<RecordIndex, TapeBuildError> {
        table_has_room(&self.labels)?;
        let message = self.push_optional_string(message)?;
        push_record(&mut self.labels, DiagnosticLabelRecord::new(span, message, primary))
    }

    /// Captures the checked start cursor for a directly emitted diagnostic-label group.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the cursor exceeds the 32-bit range domain.
    pub fn begin_labels(&self) -> Result<u32, TapeBuildError> {
        begin_direct_range(&self.labels)
    }

    /// Finishes a directly emitted diagnostic-label group.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the checked range exceeds the 32-bit
    /// table domain. Returns
    /// [`TapeBuildError::InvalidRecordIndex`] unless exactly `expected_length` labels were
    /// appended after `start`.
    pub fn finish_labels(
        &self,
        start: u32,
        expected_length: u32,
    ) -> Result<ListRange, TapeBuildError> {
        finish_direct_range(&self.labels, start, expected_length)
    }

    /// Appends a contiguous diagnostic-label group without an intermediate allocation.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at a 32-bit table or string limit.
    pub fn append_labels<'a, I>(&mut self, labels: I) -> Result<ListRange, TapeBuildError>
    where
        I: IntoIterator<Item = (TapeSpan, Option<&'a str>, bool)>,
    {
        let start = self.labels.len();
        let string_start = self.strings.len();
        let fixup_start = self.strings.fixups.as_ref().map_or(0, Vec::len);
        for (span, message, primary) in labels {
            if let Err(error) = self.push_labeled(span, message, primary) {
                self.labels.truncate(start);
                self.strings.truncate(string_start, fixup_start);
                return Err(error);
            }
        }
        list_range(start, self.labels.len() - start)
    }

    /// Appends a fully packed diagnostic record.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_record(&mut self, record: DiagnosticRecord) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.records, record)
    }

    /// Packs and appends one diagnostic while retaining every public OXC metadata string.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at a 32-bit table or string limit.
    #[expect(clippy::too_many_arguments, reason = "one parameter per diagnostic record field")]
    pub fn push_diagnostic(
        &mut self,
        phase: DiagnosticPhase,
        severity: DiagnosticSeverity,
        message: &str,
        labels: ListRange,
        help: Option<&str>,
        note: Option<&str>,
        code_scope: Option<&str>,
        code_number: Option<&str>,
        url: Option<&str>,
        codeframe: Option<&str>,
    ) -> Result<RecordIndex, TapeBuildError> {
        table_has_room(&self.records)?;
        let string_start = self.strings.len();
        let fixup_start = self.strings.fixups.as_ref().map_or(0, Vec::len);
        let packed = (|| {
            Ok(DiagnosticRecord::new(
                phase,
                severity,
                self.push_string(message)?,
                labels,
                self.push_optional_string(help)?,
                self.push_optional_string(note)?,
                self.push_optional_string(code_scope)?,
                self.push_optional_string(code_number)?,
                self.push_optional_string(url)?,
                self.push_optional_string(codeframe)?,
            ))
        })();
        let record = match packed {
            Ok(record) => record,
            Err(error) => {
                self.strings.truncate(string_start, fixup_start);
                return Err(error);
            }
        };
        push_record(&mut self.records, record)
    }

    /// Replaces the optional codeframe string for a previously appended diagnostic.
    ///
    /// # Errors
    ///
    /// Returns an invalid-index or capacity error without changing the record.
    pub fn set_codeframe(
        &mut self,
        diagnostic: RecordIndex,
        codeframe: Option<&str>,
    ) -> Result<(), TapeBuildError> {
        let index = index_usize(diagnostic);
        self.records.get(index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        let codeframe = self.push_optional_string(codeframe)?;
        self.records[index].codeframe = codeframe;
        Ok(())
    }

    /// Renders a codeframe directly into packed storage without a temporary owned string.
    ///
    /// The inner result reports a formatter failure. Either failure path leaves the diagnostic and
    /// packed storage unchanged.
    ///
    /// # Errors
    ///
    /// Returns an invalid-index or capacity error without changing the record.
    pub fn write_codeframe<F>(
        &mut self,
        diagnostic: RecordIndex,
        render: F,
    ) -> Result<Result<(), fmt::Error>, TapeBuildError>
    where
        F: FnOnce(&mut PackedStringWriter<'_>) -> fmt::Result,
    {
        let index = index_usize(diagnostic);
        self.records.get(index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        let start = self.strings.len();
        let fixup_start = self.strings.fixups.as_ref().map_or(0, Vec::len);
        u32::try_from(start).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let rendered = {
            let mut writer = PackedStringWriter { storage: self.strings.utf8_storage_mut() };
            render(&mut writer)
        };
        if let Err(error) = rendered {
            self.strings.truncate(start, fixup_start);
            return Ok(Err(error));
        }
        let codeframe = match string_range(start, self.strings.len() - start) {
            Ok(range) => OptionalStringRange::some(range),
            Err(error) => {
                self.strings.truncate(start, fixup_start);
                return Err(error);
            }
        };
        self.records[index].codeframe = codeframe;
        Ok(Ok(()))
    }

    #[must_use]
    pub fn records(&self) -> &[DiagnosticRecord] {
        &self.records
    }

    #[must_use]
    pub fn labels(&self, range: ListRange) -> Option<&[DiagnosticLabelRecord]> {
        range_slice(&self.labels, range)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}
