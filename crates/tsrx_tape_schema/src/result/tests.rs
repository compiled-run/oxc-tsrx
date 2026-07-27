use std::fmt::Write as _;

use super::bounds::{checked_direct_range, checked_range_cursor, checked_record_index};
use super::{
    CommentTable, DiagnosticPhase, DiagnosticSeverity, DiagnosticTable, DynamicImportRecord,
    ExportExportNameKind, ExportImportNameKind, ExportLocalNameKind, ImportNameKind,
    ModuleNameRecord, ModuleTable, OptionalStringRange, OptionalTapeSpan, OptionalValueSpanRecord,
    StaticExportEntryRecord, StaticExportRecord, StaticImportEntryRecord, StaticImportRecord,
    ValueSpanRecord,
};
use crate::{ListRange, ProjectedCommentKind, RecordIndex, StringRange, TapeBuildError, TapeSpan};

fn direct_import_entry(offset: u32) -> StaticImportEntryRecord {
    StaticImportEntryRecord::new(
        ModuleNameRecord::new(
            ImportNameKind::Name,
            OptionalStringRange::NONE,
            OptionalTapeSpan::some(TapeSpan::new(offset, offset + 1)),
        ),
        ValueSpanRecord::new(StringRange::new(0, 0), TapeSpan::new(offset + 2, offset + 3)),
        false,
    )
}

fn direct_export_entry(offset: u32) -> StaticExportEntryRecord {
    StaticExportEntryRecord::new(
        TapeSpan::new(offset, offset + 1),
        OptionalValueSpanRecord::NONE,
        ModuleNameRecord::new(
            ExportImportNameKind::None,
            OptionalStringRange::NONE,
            OptionalTapeSpan::NONE,
        ),
        ModuleNameRecord::new(
            ExportExportNameKind::None,
            OptionalStringRange::NONE,
            OptionalTapeSpan::NONE,
        ),
        ModuleNameRecord::new(
            ExportLocalNameKind::None,
            OptionalStringRange::NONE,
            OptionalTapeSpan::NONE,
        ),
        false,
    )
}

#[test]
fn module_groups_are_contiguous_and_strings_are_owned() {
    let mut table = ModuleTable::new();
    table.set_has_module_syntax(true);
    let request = table.push_string("pkg").expect("module request");
    let local = table.push_string("local").expect("local name");
    let imported = table.push_string("value").expect("imported name");
    let entries = table
        .append_static_import_entries([StaticImportEntryRecord::new(
            ModuleNameRecord::new(
                ImportNameKind::Name,
                OptionalStringRange::some(imported),
                OptionalTapeSpan::some(TapeSpan::new(9, 14)),
            ),
            ValueSpanRecord::new(local, TapeSpan::new(18, 23)),
            false,
        )])
        .expect("entry range");
    table
        .push_static_import(StaticImportRecord::new(
            TapeSpan::new(0, 35),
            ValueSpanRecord::new(request, TapeSpan::new(30, 33)),
            entries,
        ))
        .expect("static import");

    assert!(table.has_module_syntax());
    let import = table.static_imports()[0];
    assert_eq!(table.value(import.module_request), Some("pkg"));
    let entry = table.static_import_entries(import.entries).expect("entries")[0];
    assert_eq!(table.name(entry.import_name), Some("value"));
    assert_eq!(table.value(entry.local_name), Some("local"));
}

#[test]
fn independent_comment_and_diagnostic_strings_are_packed() {
    let mut comments = CommentTable::new();
    comments.push(ProjectedCommentKind::Block, TapeSpan::new(0, 7), "note").expect("comment");
    assert_eq!(comments.value(&comments.records()[0]), Some("note"));

    let mut diagnostics = DiagnosticTable::new();
    let labels =
        diagnostics.append_labels([(TapeSpan::new(3, 4), Some("here"), true)]).expect("labels");
    diagnostics
        .push_diagnostic(
            DiagnosticPhase::Grammar,
            DiagnosticSeverity::Error,
            "Unexpected token",
            labels,
            None,
            None,
            Some("parser"),
            Some("1001"),
            None,
            None,
        )
        .expect("diagnostic");
    let record = diagnostics.records()[0];
    assert_eq!(diagnostics.string(record.message), Some("Unexpected token"));
    assert_eq!(diagnostics.optional_string(record.code_scope), Some("parser"));
    assert_eq!(
        diagnostics.optional_string(diagnostics.labels(record.labels).expect("labels")[0].message),
        Some("here")
    );
    assert!(diagnostics.labels(record.labels).expect("labels")[0].primary);

    diagnostics
        .write_codeframe(RecordIndex::new(0), |writer| writer.write_str("rendered frame"))
        .expect("codeframe capacity")
        .expect("codeframe render");
    let record = diagnostics.records()[0];
    assert_eq!(diagnostics.optional_string(record.codeframe), Some("rendered frame"));
    let storage_before_failure =
        diagnostics.string_storage().expect("fixup-free diagnostic storage").to_owned();
    let codeframe_before_failure = record.codeframe;
    assert!(matches!(
        diagnostics.write_codeframe(RecordIndex::new(0), |writer| {
            writer.write_str("discarded partial frame")?;
            Err(std::fmt::Error)
        }),
        Ok(Err(_))
    ));
    assert_eq!(diagnostics.string_storage(), Some(storage_before_failure.as_str()));
    assert_eq!(diagnostics.records()[0].codeframe, codeframe_before_failure);
}

#[test]
#[expect(clippy::too_many_lines, reason = "one exhaustive table test over every result-table span")]
fn every_result_table_source_span_is_mapped_once_without_touching_strings() {
    let mut module = ModuleTable::new();
    let text_units = [u16::from(b'n'), 0xd800, u16::from(b'm')];
    let text = module.push_utf16(&text_units).expect("module text");

    let import_entries = module
        .append_static_import_entries([StaticImportEntryRecord::new(
            ModuleNameRecord::new(
                ImportNameKind::Name,
                OptionalStringRange::some(text),
                OptionalTapeSpan::some(TapeSpan::new(5, 6)),
            ),
            ValueSpanRecord::new(text, TapeSpan::new(7, 8)),
            false,
        )])
        .expect("import entries");
    module
        .push_static_import(StaticImportRecord::new(
            TapeSpan::new(1, 2),
            ValueSpanRecord::new(text, TapeSpan::new(3, 4)),
            import_entries,
        ))
        .expect("static import");

    let export_entries = module
        .append_static_export_entries([StaticExportEntryRecord::new(
            TapeSpan::new(11, 12),
            OptionalValueSpanRecord::some(ValueSpanRecord::new(text, TapeSpan::new(13, 14))),
            ModuleNameRecord::new(
                ExportImportNameKind::Name,
                OptionalStringRange::some(text),
                OptionalTapeSpan::some(TapeSpan::new(15, 16)),
            ),
            ModuleNameRecord::new(
                ExportExportNameKind::Name,
                OptionalStringRange::some(text),
                OptionalTapeSpan::some(TapeSpan::new(17, 18)),
            ),
            ModuleNameRecord::new(
                ExportLocalNameKind::Name,
                OptionalStringRange::some(text),
                OptionalTapeSpan::some(TapeSpan::new(19, 20)),
            ),
            false,
        )])
        .expect("export entries");
    module
        .push_static_export(StaticExportRecord::new(TapeSpan::new(9, 10), export_entries))
        .expect("static export");
    module
        .push_dynamic_import(DynamicImportRecord::new(TapeSpan::new(21, 22), TapeSpan::new(23, 24)))
        .expect("dynamic import");
    module.push_import_meta(TapeSpan::new(25, 26)).expect("import meta");

    assert_eq!(module.string_storage(), None);
    let module_storage_before = module.strings.utf8.clone();
    let module_capacities_before = [
        module.static_imports.capacity(),
        module.static_import_entries.capacity(),
        module.static_exports.capacity(),
        module.static_export_entries.capacity(),
        module.dynamic_imports.capacity(),
        module.import_metas.capacity(),
    ];
    let module_string_capacity_before = module.strings.utf8.capacity();
    let mut mapped = Vec::new();
    module
        .try_map_spans(|span| {
            mapped.push(span);
            Ok::<_, ()>(TapeSpan::new(span.start + 100, span.end + 100))
        })
        .expect("module spans");

    assert_eq!(
        mapped,
        (1..=25).step_by(2).map(|start| TapeSpan::new(start, start + 1)).collect::<Vec<_>>()
    );
    assert_eq!(module.strings.utf8, module_storage_before);
    assert_eq!(
        [
            module.static_imports.capacity(),
            module.static_import_entries.capacity(),
            module.static_exports.capacity(),
            module.static_export_entries.capacity(),
            module.dynamic_imports.capacity(),
            module.import_metas.capacity(),
        ],
        module_capacities_before
    );
    assert_eq!(module.strings.utf8.capacity(), module_string_capacity_before);
    assert_eq!(module.text(text).expect("module text").to_utf16(), text_units);
    let import = module.static_imports()[0];
    assert_eq!(import.span, TapeSpan::new(101, 102));
    assert_eq!(import.module_request.span, TapeSpan::new(103, 104));
    let import_entry = module.static_import_entries(import.entries).expect("import entries")[0];
    assert_eq!(import_entry.import_name.span.get(), Some(TapeSpan::new(105, 106)));
    assert_eq!(import_entry.local_name.span, TapeSpan::new(107, 108));
    let export = module.static_exports()[0];
    assert_eq!(export.span, TapeSpan::new(109, 110));
    let export_entry = module.static_export_entries(export.entries).expect("export entries")[0];
    assert_eq!(export_entry.span, TapeSpan::new(111, 112));
    assert_eq!(
        export_entry.module_request.get().expect("module request").span,
        TapeSpan::new(113, 114)
    );
    assert_eq!(export_entry.import_name.span.get(), Some(TapeSpan::new(115, 116)));
    assert_eq!(export_entry.export_name.span.get(), Some(TapeSpan::new(117, 118)));
    assert_eq!(export_entry.local_name.span.get(), Some(TapeSpan::new(119, 120)));
    assert_eq!(
        module.dynamic_imports()[0],
        DynamicImportRecord::new(TapeSpan::new(121, 122), TapeSpan::new(123, 124))
    );
    assert_eq!(module.import_metas(), &[TapeSpan::new(125, 126)]);

    let mut comments = CommentTable::new();
    comments.push(ProjectedCommentKind::Line, TapeSpan::new(31, 32), "keep").expect("comment");
    let comment_storage_before =
        comments.string_storage().expect("fixup-free comment storage").to_owned();
    let comment_capacities_before = (comments.records.capacity(), comments.strings.utf8.capacity());
    comments
        .try_map_spans(|span| Ok::<_, ()>(TapeSpan::new(span.start + 100, span.end + 100)))
        .expect("comment spans");
    assert_eq!(comments.records()[0].span, TapeSpan::new(131, 132));
    assert_eq!(comments.string_storage(), Some(comment_storage_before.as_str()));
    assert_eq!(
        (comments.records.capacity(), comments.strings.utf8.capacity()),
        comment_capacities_before
    );
    assert_eq!(comments.value(&comments.records()[0]), Some("keep"));

    let mut diagnostics = DiagnosticTable::new();
    let labels = diagnostics
        .append_labels([
            (TapeSpan::new(41, 42), Some("first"), true),
            (TapeSpan::new(43, 44), None, false),
        ])
        .expect("labels");
    diagnostics
        .push_diagnostic(
            DiagnosticPhase::Grammar,
            DiagnosticSeverity::Error,
            "keep diagnostic",
            labels,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("diagnostic");
    let diagnostic_storage_before =
        diagnostics.string_storage().expect("fixup-free diagnostic storage").to_owned();
    let diagnostic_capacities_before = (
        diagnostics.records.capacity(),
        diagnostics.labels.capacity(),
        diagnostics.strings.utf8.capacity(),
    );
    diagnostics
        .try_map_spans(|span| Ok::<_, ()>(TapeSpan::new(span.start + 100, span.end + 100)))
        .expect("diagnostic spans");
    let labels = diagnostics.labels(diagnostics.records()[0].labels).expect("labels");
    assert_eq!(labels[0].span, TapeSpan::new(141, 142));
    assert_eq!(labels[1].span, TapeSpan::new(143, 144));
    assert_eq!(diagnostics.string_storage(), Some(diagnostic_storage_before.as_str()));
    assert_eq!(
        (
            diagnostics.records.capacity(),
            diagnostics.labels.capacity(),
            diagnostics.strings.utf8.capacity(),
        ),
        diagnostic_capacities_before
    );
    assert_eq!(diagnostics.string(diagnostics.records()[0].message), Some("keep diagnostic"));
}

#[test]
fn result_table_span_mappers_return_the_exact_mapper_error() {
    let mut module = ModuleTable::new();
    module
        .push_dynamic_import(DynamicImportRecord::new(TapeSpan::new(1, 2), TapeSpan::new(3, 4)))
        .expect("dynamic import");
    assert_eq!(
        module.try_map_spans(|span| {
            if span == TapeSpan::new(3, 4) { Err("module endpoint is unmapped") } else { Ok(span) }
        }),
        Err("module endpoint is unmapped")
    );

    let mut comments = CommentTable::new();
    comments.push(ProjectedCommentKind::Line, TapeSpan::new(5, 6), "comment").expect("comment");
    assert_eq!(
        comments.try_map_spans(|_| Err::<TapeSpan, _>("comment endpoint is unmapped")),
        Err("comment endpoint is unmapped")
    );

    let mut diagnostics = DiagnosticTable::new();
    diagnostics.append_labels([(TapeSpan::new(7, 8), None, true)]).expect("label");
    assert_eq!(
        diagnostics.try_map_spans(|_| Err::<TapeSpan, _>("label endpoint is unmapped")),
        Err("label endpoint is unmapped")
    );
}

#[test]
fn packed_text_fixups_are_sparse_position_keyed_and_lossless() {
    let mut comments = CommentTable::new();
    comments.push(ProjectedCommentKind::Line, TapeSpan::new(0, 4), "plain").expect("plain comment");
    let units = [0xe000, 0xd800, 0xd83d, 0xde00, 0xdc00, 0xe000];
    comments
        .push_utf16(ProjectedCommentKind::Block, TapeSpan::new(5, 13), &units)
        .expect("lossless comment");

    let plain = comments.value_text(&comments.records()[0]).expect("plain text");
    assert_eq!(plain.as_str(), Some("plain"));
    assert_eq!(plain.to_utf16(), "plain".encode_utf16().collect::<Vec<_>>());

    let repaired = comments.value_text(&comments.records()[1]).expect("repaired text");
    assert_eq!(repaired.as_str(), None);
    assert_eq!(repaired.to_utf16(), units);
    assert_eq!(comments.string_storage(), None);
    let table_debug = format!("{comments:?}");
    let text_debug = format!("{repaired:?}");
    assert!(table_debug.contains("<lossless UTF-16 text>"));
    assert!(text_debug.contains("<lossless UTF-16 text>"));
    assert!(!table_debug.contains('\u{e000}'));
    assert!(!text_debug.contains('\u{e000}'));
}

#[test]
fn existing_packed_values_accept_only_collision_safe_same_width_repairs() {
    let mut module = ModuleTable::new();
    let range = module
        .push_string(&format!("{}x{}", '\u{e000}', '\u{e000}'))
        .expect("placeholder-width module value");
    let units = [0xe000, u16::from(b'x'), 0xd800];
    module.repair_utf16(range, &units).expect("position-keyed repair");
    assert_eq!(module.text(range).expect("module text").to_utf16(), units);
    assert_eq!(
        module.repair_utf16(range, &units),
        Err(TapeBuildError::InvalidRecordIndex),
        "the same packed range cannot consume one fixup twice"
    );
    assert_eq!(
        module.repair_utf16(range, &[0xe000, u16::from(b'y'), 0xd800]),
        Err(TapeBuildError::InvalidRecordIndex)
    );
    assert_eq!(
        module.repair_utf16(range, &[0xe000, u16::from(b'x'), 0xe000]),
        Err(TapeBuildError::InvalidRecordIndex),
        "a later repair cannot silently remove an existing position-keyed fixup"
    );

    let mut batch = ModuleTable::new();
    let ranges = std::iter::repeat_with(|| {
        batch.push_string(&format!("{}x", '\u{e000}')).expect("placeholder-width batch value")
    })
    .take(4_096)
    .collect::<Vec<_>>();
    let repaired = [0xd800, u16::from(b'x')];
    batch
        .repair_utf16_batch(ranges.iter().copied().map(|range| (range, repaired.as_slice())))
        .expect("one ordered linear batch");
    assert!(
        ranges
            .iter()
            .all(|range| { batch.text(*range).expect("batch text").to_utf16() == repaired })
    );
    assert_eq!(batch.strings.fixups.as_ref().map(Vec::len), Some(4_096));

    let mut diagnostics = DiagnosticTable::new();
    let labels = diagnostics.append_labels(std::iter::empty()).expect("empty labels");
    diagnostics
        .push_diagnostic(
            DiagnosticPhase::Grammar,
            DiagnosticSeverity::Error,
            "message",
            labels,
            None,
            None,
            None,
            None,
            None,
            Some(&format!("{}x{}", '\u{e000}', '\u{e000}')),
        )
        .expect("diagnostic");
    let record = diagnostics.records()[0];
    let codeframe = record.codeframe.get().expect("codeframe range");
    diagnostics
        .repair_utf16_batch(std::iter::once((codeframe, units.as_slice())))
        .expect("lossless codeframe");
    let record = diagnostics.records()[0];
    assert_eq!(
        diagnostics.optional_text(record.codeframe).expect("codeframe text").to_utf16(),
        units
    );
}

#[test]
fn rejection_placeholder_repairs_are_position_keyed_and_preserve_authored_noncharacters() {
    let mut module = ModuleTable::new();
    let packed = format!("{}|{}|{}", '\u{ffff}', '\u{ffff}', '\u{e000}');
    let range = module.push_string(&packed).expect("same-width rejection fixture");
    let units = [0xffff, u16::from(b'|'), 0xd800, u16::from(b'|'), 0xe000];

    module.repair_utf16(range, &units).expect("position-keyed U+FFFF repair");

    assert_eq!(module.text(range).expect("lossless text").to_utf16(), units);
    assert_eq!(
        module.strings.utf8,
        format!("{}|{}|{}", '\u{ffff}', '\u{e000}', '\u{e000}'),
        "only the tracked rejection position is normalized to packed U+E000 storage"
    );
    assert_eq!(module.strings.fixups.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        module.strings.fixups.as_ref().expect("one fixup")[0].byte_start,
        range.start + 4,
        "the authored leading U+FFFF remains an ordinary scalar"
    );
}

#[test]
fn direct_module_entry_ranges_are_exact_checked_and_keep_empty_cursors() {
    let mut table = ModuleTable::new();

    let imports = table.begin_static_import_entries().expect("import start");
    table.push_static_import_entry(direct_import_entry(1)).expect("first import entry");
    table.push_static_import_entry(direct_import_entry(5)).expect("second import entry");
    assert_eq!(
        table.finish_static_import_entries(imports, 2).expect("import range"),
        ListRange::new(0, 2)
    );
    let empty_imports = table.begin_static_import_entries().expect("empty import start");
    assert_eq!(
        table.finish_static_import_entries(empty_imports, 0).expect("empty import range"),
        ListRange::new(2, 0)
    );
    assert_eq!(
        table.finish_static_import_entries(imports, 1),
        Err(TapeBuildError::InvalidRecordIndex)
    );

    let exports = table.begin_static_export_entries().expect("export start");
    table.push_static_export_entry(direct_export_entry(10)).expect("export entry");
    assert_eq!(
        table.finish_static_export_entries(exports, 1).expect("export range"),
        ListRange::new(0, 1)
    );
    let empty_exports = table.begin_static_export_entries().expect("empty export start");
    assert_eq!(
        table.finish_static_export_entries(empty_exports, 0).expect("empty export range"),
        ListRange::new(1, 0)
    );
}

#[test]
fn direct_label_ranges_and_cursor_arithmetic_fail_closed() {
    let mut table = DiagnosticTable::new();
    let labels = table.begin_labels().expect("label start");
    table.push_labeled(TapeSpan::new(1, 2), Some("one"), true).expect("first label");
    table.push_labeled(TapeSpan::new(3, 4), None, false).expect("second label");
    assert_eq!(table.finish_labels(labels, 2).expect("label range"), ListRange::new(0, 2));
    let empty = table.begin_labels().expect("empty label start");
    assert_eq!(table.finish_labels(empty, 0).expect("empty label range"), ListRange::new(2, 0));
    assert_eq!(table.finish_labels(labels, 1), Err(TapeBuildError::InvalidRecordIndex));
    assert_eq!(table.finish_labels(u32::MAX, 0), Err(TapeBuildError::InvalidRecordIndex));
    assert_eq!(table.finish_labels(u32::MAX - 1, 2), Err(TapeBuildError::CapacityOverflow));
    assert_eq!(table.finish_labels(u32::MAX - 1, 1), Err(TapeBuildError::InvalidRecordIndex));
    assert_eq!(
        checked_range_cursor(usize::try_from(u32::MAX).expect("u32 fits usize")),
        Ok(u32::MAX)
    );
    assert_eq!(
        checked_record_index(usize::try_from(u32::MAX).expect("u32 fits usize")),
        Err(TapeBuildError::CapacityOverflow)
    );
    assert_eq!(
        checked_record_index(usize::try_from(u32::MAX - 1).expect("u32 fits usize"))
            .map(RecordIndex::into_raw),
        Ok(u32::MAX - 1)
    );
    assert_eq!(checked_direct_range(u32::MAX, 0, u32::MAX), Ok(ListRange::new(u32::MAX, 0)));
    assert_eq!(
        checked_direct_range(u32::MAX - 1, 1, u32::MAX),
        Ok(ListRange::new(u32::MAX - 1, 1))
    );
    assert_eq!(
        checked_direct_range(u32::MAX - 1, 2, u32::MAX),
        Err(TapeBuildError::CapacityOverflow)
    );
}

#[test]
fn destructive_parts_release_every_source_allocation() {
    let mut module = ModuleTable::new();
    module.static_imports.reserve(1);
    module.static_import_entries.reserve(1);
    module.static_exports.reserve(1);
    module.static_export_entries.reserve(1);
    module.dynamic_imports.reserve(1);
    module.import_metas.reserve(1);
    module.strings.push_str("module").expect("module string");
    let imports = module.take_static_imports();
    let exports = module.take_static_exports();
    let dynamics = module.take_dynamic_imports();
    let metas = module.take_import_metas();
    let module_strings = module.take_string_storage().expect("UTF-8 module strings");
    assert!(!module_strings.is_empty());
    assert!(imports.0.capacity() > 0 && imports.1.capacity() > 0);
    assert!(exports.0.capacity() > 0 && exports.1.capacity() > 0);
    assert!(dynamics.capacity() > 0 && metas.capacity() > 0);
    assert!(module.is_storage_released());

    let mut comments = CommentTable::new();
    comments.records.reserve(1);
    comments.strings.push_str("comment").expect("comment string");
    let comment_records = comments.take_records();
    let comment_strings = comments.take_string_storage().expect("UTF-8 comment strings");
    assert!(comment_records.capacity() > 0);
    assert_eq!(comment_strings, "comment");
    assert!(comments.is_storage_released());

    let mut diagnostics = DiagnosticTable::new();
    diagnostics.records.reserve(1);
    diagnostics.labels.reserve(1);
    diagnostics.strings.push_str("diagnostic").expect("diagnostic string");
    let diagnostic_parts = diagnostics.take_records_and_labels();
    let diagnostic_strings = diagnostics.take_string_storage().expect("UTF-8 diagnostic strings");
    assert!(diagnostic_parts.0.capacity() > 0 && diagnostic_parts.1.capacity() > 0);
    assert_eq!(diagnostic_strings, "diagnostic");
    assert!(diagnostics.is_storage_released());

    let mut lossless = CommentTable::new();
    let record = lossless
        .push_utf16(ProjectedCommentKind::Line, TapeSpan::new(0, 3), &[0xd800])
        .expect("lossless comment");
    assert_eq!(
        lossless.take_string_storage(),
        Err(TapeBuildError::InvalidRecordIndex),
        "the UTF-8-only destructive API must not drop sparse fixups"
    );
    let records = lossless.take_records();
    let text = lossless.take_text_storage();
    assert_eq!(
        text.text(
            records[usize::try_from(record.into_raw()).expect("record index fits usize")].value,
        )
        .expect("released lossless text")
        .to_utf16(),
        [0xd800]
    );
    assert!(lossless.is_storage_released());
}
