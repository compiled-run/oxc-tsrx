use std::fmt;

use crate::{ListRange, ProjectedCommentKind, StringRange, TapeSpan};

use super::kinds::{
    DiagnosticPhase, DiagnosticSeverity, ExportExportNameKind, ExportImportNameKind,
    ExportLocalNameKind, ImportNameKind,
};
use super::spans::{
    OptionalStringRange, OptionalTapeSpan, OptionalValueSpanRecord, ValueSpanRecord,
};

/// One optional module name and span paired with its context-specific kind.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleNameRecord<K = ImportNameKind> {
    pub kind: K,
    pub name: OptionalStringRange,
    pub span: OptionalTapeSpan,
}

impl<K> ModuleNameRecord<K> {
    #[must_use]
    pub const fn new(kind: K, name: OptionalStringRange, span: OptionalTapeSpan) -> Self {
        Self { kind, name, span }
    }
}

/// One entry belonging to a static import statement.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticImportEntryRecord {
    pub import_name: ModuleNameRecord<ImportNameKind>,
    pub local_name: ValueSpanRecord,
    pub is_type: bool,
}

impl StaticImportEntryRecord {
    #[must_use]
    pub const fn new(
        import_name: ModuleNameRecord<ImportNameKind>,
        local_name: ValueSpanRecord,
        is_type: bool,
    ) -> Self {
        Self { import_name, local_name, is_type }
    }
}

/// One source-ordered static import statement.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticImportRecord {
    pub span: TapeSpan,
    pub module_request: ValueSpanRecord,
    pub entries: ListRange,
}

impl StaticImportRecord {
    #[must_use]
    pub const fn new(span: TapeSpan, module_request: ValueSpanRecord, entries: ListRange) -> Self {
        Self { span, module_request, entries }
    }
}

/// One entry belonging to a static export statement.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticExportEntryRecord {
    pub span: TapeSpan,
    pub module_request: OptionalValueSpanRecord,
    pub import_name: ModuleNameRecord<ExportImportNameKind>,
    pub export_name: ModuleNameRecord<ExportExportNameKind>,
    pub local_name: ModuleNameRecord<ExportLocalNameKind>,
    pub is_type: bool,
}

impl StaticExportEntryRecord {
    #[must_use]
    pub const fn new(
        span: TapeSpan,
        module_request: OptionalValueSpanRecord,
        import_name: ModuleNameRecord<ExportImportNameKind>,
        export_name: ModuleNameRecord<ExportExportNameKind>,
        local_name: ModuleNameRecord<ExportLocalNameKind>,
        is_type: bool,
    ) -> Self {
        Self { span, module_request, import_name, export_name, local_name, is_type }
    }
}

/// One source-ordered static export statement.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticExportRecord {
    pub span: TapeSpan,
    pub entries: ListRange,
}

impl StaticExportRecord {
    #[must_use]
    pub const fn new(span: TapeSpan, entries: ListRange) -> Self {
        Self { span, entries }
    }
}

/// One dynamic import expression.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DynamicImportRecord {
    pub span: TapeSpan,
    pub module_request: TapeSpan,
}

impl DynamicImportRecord {
    #[must_use]
    pub const fn new(span: TapeSpan, module_request: TapeSpan) -> Self {
        Self { span, module_request }
    }
}

/// One source-ordered comment with its delimiter-free value.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommentRecord {
    pub kind: ProjectedCommentKind,
    pub span: TapeSpan,
    pub value: StringRange,
}

impl CommentRecord {
    #[must_use]
    pub const fn new(kind: ProjectedCommentKind, span: TapeSpan, value: StringRange) -> Self {
        Self { kind, span, value }
    }
}

/// One source label attached to a diagnostic.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagnosticLabelRecord {
    pub span: TapeSpan,
    pub message: OptionalStringRange,
    pub primary: bool,
}

impl DiagnosticLabelRecord {
    #[must_use]
    pub const fn new(span: TapeSpan, message: OptionalStringRange, primary: bool) -> Self {
        Self { span, message, primary }
    }
}

/// One structured grammar or semantic diagnostic.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagnosticRecord {
    pub phase: DiagnosticPhase,
    pub severity: DiagnosticSeverity,
    pub message: StringRange,
    pub labels: ListRange,
    pub help: OptionalStringRange,
    pub note: OptionalStringRange,
    pub code_scope: OptionalStringRange,
    pub code_number: OptionalStringRange,
    pub url: OptionalStringRange,
    pub codeframe: OptionalStringRange,
}

impl DiagnosticRecord {
    #[expect(
        clippy::too_many_arguments,
        reason = "a const field-by-field constructor: one parameter per record field"
    )]
    #[must_use]
    pub const fn new(
        phase: DiagnosticPhase,
        severity: DiagnosticSeverity,
        message: StringRange,
        labels: ListRange,
        help: OptionalStringRange,
        note: OptionalStringRange,
        code_scope: OptionalStringRange,
        code_number: OptionalStringRange,
        url: OptionalStringRange,
        codeframe: OptionalStringRange,
    ) -> Self {
        Self {
            phase,
            severity,
            message,
            labels,
            help,
            note,
            code_scope,
            code_number,
            url,
            codeframe,
        }
    }
}

/// Append-only packed-string writer supplied to diagnostic render callbacks.
///
/// Callers cannot construct it or mutate previously packed bytes.
pub struct PackedStringWriter<'a> {
    pub(super) storage: &'a mut String,
}

impl fmt::Write for PackedStringWriter<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.storage.push_str(value);
        Ok(())
    }
}
