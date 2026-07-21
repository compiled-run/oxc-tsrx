use tsrx_tape_schema::{
    CommentRecord, CoordinateDomain, DiagnosticLabelRecord, DiagnosticPhase, DiagnosticRecord,
    DiagnosticSeverity, DiagnosticTable, DynamicImportRecord, ExportExportNameKind,
    ExportImportNameKind, ExportLocalNameKind, FieldRecord, ImportNameKind, ListRecord,
    ListValueRecord, ModuleNameRecord, ObjectRecord, OptionalStringRange, OptionalTapeSpan,
    OptionalValueSpanRecord, ParseCompleteness, StaticExportEntryRecord, StaticExportRecord,
    StaticImportEntryRecord, StaticImportRecord, TapeSpan, ValueRef, ValueSpanRecord,
};

const fn assert_copy<T: Copy>() {}

fn assert_layout<T>(size: usize, alignment: usize) {
    assert_eq!(size_of::<T>(), size);
    assert_eq!(align_of::<T>(), alignment);
}

#[test]
fn result_records_are_fixed_width_copy_values_with_stable_discriminants() {
    assert_copy::<TapeSpan>();
    assert_copy::<OptionalTapeSpan>();
    assert_copy::<OptionalStringRange>();
    assert_copy::<ValueSpanRecord>();
    assert_copy::<OptionalValueSpanRecord>();
    assert_copy::<ModuleNameRecord>();
    assert_copy::<StaticImportRecord>();
    assert_copy::<StaticImportEntryRecord>();
    assert_copy::<StaticExportRecord>();
    assert_copy::<StaticExportEntryRecord>();
    assert_copy::<DynamicImportRecord>();
    assert_copy::<CommentRecord>();
    assert_copy::<DiagnosticRecord>();
    assert_copy::<DiagnosticLabelRecord>();
    assert_copy::<ValueRef>();
    assert_copy::<ObjectRecord>();
    assert_copy::<FieldRecord>();
    assert_copy::<ListRecord>();
    assert_copy::<ListValueRecord>();

    assert_layout::<TapeSpan>(8, 4);
    assert_layout::<OptionalTapeSpan>(12, 4);
    assert_layout::<OptionalStringRange>(12, 4);
    assert_layout::<ValueSpanRecord>(16, 4);
    assert_layout::<OptionalValueSpanRecord>(20, 4);
    assert_layout::<ModuleNameRecord>(28, 4);
    assert_layout::<StaticImportRecord>(32, 4);
    assert_layout::<StaticImportEntryRecord>(48, 4);
    assert_layout::<StaticExportRecord>(16, 4);
    assert_layout::<StaticExportEntryRecord>(116, 4);
    assert_layout::<DynamicImportRecord>(16, 4);
    assert_layout::<CommentRecord>(20, 4);
    assert_layout::<DiagnosticRecord>(92, 4);
    assert_layout::<DiagnosticLabelRecord>(24, 4);
    assert_layout::<ValueRef>(8, 4);
    assert_layout::<ObjectRecord>(8, 4);
    assert_layout::<FieldRecord>(20, 4);
    assert_layout::<ListRecord>(8, 4);
    assert_layout::<ListValueRecord>(12, 4);

    assert_eq!(ParseCompleteness::Complete as u8, 1);
    assert_eq!(ParseCompleteness::Recovered as u8, 2);
    assert_eq!(ParseCompleteness::Failed as u8, 3);
    assert_eq!(CoordinateDomain::ProjectedUtf8Bytes as u8, 1);
    assert_eq!(CoordinateDomain::AuthoredUtf8Bytes as u8, 2);
    assert_eq!(CoordinateDomain::OriginalUtf16Units as u8, 3);
    assert_eq!(ImportNameKind::Default as u8, 3);
    assert_eq!(ExportImportNameKind::None as u8, 4);
    assert_eq!(ExportExportNameKind::None as u8, 3);
    assert_eq!(ExportLocalNameKind::None as u8, 3);
    assert_eq!(DiagnosticSeverity::Advice as u8, 3);
    assert_eq!(DiagnosticPhase::Semantic as u8, 2);
    assert_eq!(DiagnosticPhase::Recovery as u8, 3);
}

#[test]
fn nullable_ranges_distinguish_null_from_present_empty_values() {
    let absent = OptionalStringRange::NONE;
    let empty = OptionalStringRange::some(tsrx_tape_schema::StringRange::new(0, 0));
    assert!(absent.get().is_none());
    assert_eq!(empty.get(), Some(tsrx_tape_schema::StringRange::new(0, 0)));

    let absent = OptionalTapeSpan::NONE;
    let empty = OptionalTapeSpan::some(TapeSpan::new(7, 7));
    assert!(absent.get().is_none());
    assert_eq!(empty.get(), Some(TapeSpan::new(7, 7)));

    let mut diagnostics = DiagnosticTable::new();
    let labels = diagnostics
        .append_labels([
            (TapeSpan::new(1, 2), Some("primary"), true),
            (TapeSpan::new(3, 4), None, false),
        ])
        .expect("fixed diagnostic labels");
    let labels = diagnostics.labels(labels).expect("packed labels");
    assert!(labels[0].primary);
    assert!(!labels[1].primary);
    assert_eq!(
        diagnostics.optional_string(labels[0].message),
        Some("primary")
    );
}
use std::mem::{align_of, size_of};
