use oxc_ast::{CommentKind, ast::Program};
use oxc_diagnostics::{OxcDiagnostic, Severity};
use oxc_span::GetSpan;
use oxc_syntax::module_record::{
    self, ExportExportName, ExportImportName, ExportLocalName, ImportImportName, ModuleRecord,
};
use rustc_hash::FxHashMap;
use tsrx_tape_schema::{
    CommentTable, DiagnosticPhase, DiagnosticSeverity, DiagnosticTable, DynamicImportRecord,
    ExportExportNameKind, ExportImportNameKind, ExportLocalNameKind, ImportNameKind,
    ModuleNameRecord, ModuleTable, OptionalStringRange, OptionalTapeSpan, OptionalValueSpanRecord,
    ProjectedCommentKind, StaticExportEntryRecord, StaticExportRecord, StaticImportEntryRecord,
    StaticImportRecord, StringRange, TapeBuildError, TapeSpan, ValueSpanRecord,
};

use super::{ProjectedParseError, RejectionModuleNames};

type StatementKey = (u32, u32);

struct AssociationLink<T> {
    value: T,
    next: Option<usize>,
}

#[derive(Debug, Default)]
struct AssociationRange {
    head: Option<usize>,
    tail: Option<usize>,
    length: u32,
}

#[derive(Debug, Default)]
struct StatementAssociations {
    import_entries: AssociationRange,
    import_requests: AssociationRange,
    exports: AssociationRange,
}

fn append_association<T>(
    links: &mut Vec<AssociationLink<T>>,
    range: &mut AssociationRange,
    value: T,
) -> Result<(), TapeBuildError> {
    let length = range.length.checked_add(1).ok_or(TapeBuildError::CapacityOverflow)?;
    let index = links.len();
    links.push(AssociationLink { value, next: None });
    if let Some(tail) = range.tail {
        links[tail].next = Some(index);
    } else {
        range.head = Some(index);
    }
    range.tail = Some(index);
    range.length = length;
    Ok(())
}

struct ModuleAssociations<'record, 'ast> {
    statements: Vec<StatementAssociations>,
    import_entries: Vec<AssociationLink<&'record module_record::ImportEntry<'ast>>>,
    import_requests: Vec<AssociationLink<(oxc_span::Span, &'record str)>>,
    exports: Vec<AssociationLink<&'record module_record::ExportEntry<'ast>>>,
}

fn build_module_associations<'record, 'ast>(
    program: &Program<'ast>,
    record: &'record ModuleRecord<'ast>,
) -> Result<ModuleAssociations<'record, 'ast>, ProjectedParseError>
where
    'ast: 'record,
{
    let mut statement_indices = FxHashMap::<StatementKey, usize>::default();
    statement_indices.reserve(program.body.len());
    let mut statements = Vec::with_capacity(program.body.len());
    for (index, statement) in program.body.iter().enumerate() {
        if statement_indices.insert(span_key(statement.span()), index).is_some() {
            return Err(ProjectedParseError::Invariant(
                "Program statements have duplicate source spans".to_string(),
            ));
        }
        statements.push(StatementAssociations::default());
    }

    let mut import_entries = Vec::with_capacity(record.import_entries.len());
    for entry in &record.import_entries {
        let Some(index) = statement_indices.get(&span_key(entry.statement_span)).copied() else {
            return Err(ProjectedParseError::Invariant(
                "import entry does not correspond to a Program statement".to_string(),
            ));
        };
        append_association(&mut import_entries, &mut statements[index].import_entries, entry)?;
    }
    let mut import_requests = Vec::with_capacity(record.requested_modules.len());
    for (name, requests) in &record.requested_modules {
        for request in requests.iter().filter(|request| request.is_import) {
            let Some(index) = statement_indices.get(&span_key(request.statement_span)).copied()
            else {
                return Err(ProjectedParseError::Invariant(
                    "import request does not correspond to a Program statement".to_string(),
                ));
            };
            append_association(
                &mut import_requests,
                &mut statements[index].import_requests,
                (request.span, name.as_str()),
            )?;
        }
    }

    let export_capacity = record
        .local_export_entries
        .len()
        .checked_add(record.indirect_export_entries.len())
        .and_then(|length| length.checked_add(record.star_export_entries.len()))
        .ok_or(TapeBuildError::CapacityOverflow)?;
    let mut exports = Vec::with_capacity(export_capacity);
    for entry in record
        .local_export_entries
        .iter()
        .chain(record.indirect_export_entries.iter())
        .chain(record.star_export_entries.iter())
    {
        let Some(index) = statement_indices.get(&span_key(entry.statement_span)).copied() else {
            return Err(ProjectedParseError::Invariant(
                "export entry does not correspond to a Program statement".to_string(),
            ));
        };
        append_association(&mut exports, &mut statements[index].exports, entry)?;
    }

    Ok(ModuleAssociations { statements, import_entries, import_requests, exports })
}

pub(super) fn serialize_comments(
    program: &Program<'_>,
    source: &str,
) -> Result<CommentTable, TapeBuildError> {
    let mut comments = CommentTable::default();
    for comment in &program.comments {
        comments.push(
            match comment.kind {
                CommentKind::Line => ProjectedCommentKind::Line,
                CommentKind::SingleLineBlock | CommentKind::MultiLineBlock => {
                    ProjectedCommentKind::Block
                }
            },
            span(comment.span),
            comment.content_span().source_text(source),
        )?;
    }
    Ok(comments)
}

pub(super) fn append_diagnostics<'a>(
    output: &mut DiagnosticTable,
    diagnostics: impl IntoIterator<Item = &'a OxcDiagnostic>,
    phase: DiagnosticPhase,
) -> Result<(), TapeBuildError> {
    for diagnostic in diagnostics {
        let label_count =
            u32::try_from(diagnostic.labels.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let label_start = output.begin_labels()?;
        for label in &diagnostic.labels {
            let start = label.offset();
            let end = start.checked_add(label.len()).ok_or(TapeBuildError::CapacityOverflow)?;
            output.push_labeled(TapeSpan::new(start, end), label.label(), label.primary())?;
        }
        let labels = output.finish_labels(label_start, label_count)?;
        output.push_diagnostic(
            phase,
            match diagnostic.severity {
                Severity::Error => DiagnosticSeverity::Error,
                Severity::Warning => DiagnosticSeverity::Warning,
                Severity::Advice => DiagnosticSeverity::Advice,
            },
            diagnostic.message.as_ref(),
            labels,
            diagnostic.help.as_deref(),
            diagnostic.note.as_deref(),
            diagnostic.code.scope.as_deref(),
            diagnostic.code.number.as_deref(),
            diagnostic.url.as_deref(),
            None,
        )?;
    }
    Ok(())
}

pub(super) fn serialize_module(
    program: &Program<'_>,
    record: &ModuleRecord<'_>,
) -> Result<ModuleTable, ProjectedParseError> {
    let mut output = ModuleTable::default();
    output.set_has_module_syntax(record.has_module_syntax);
    let associations = build_module_associations(program, record)?;

    for (statement, association) in program.body.iter().zip(&associations.statements) {
        let statement_span = statement.span();
        if association.import_requests.length == 0 {
            if association.import_entries.length != 0 {
                return Err(ProjectedParseError::Invariant(
                    "import entries have no corresponding module request".to_string(),
                ));
            }
        } else {
            let mut request_link = association.import_requests.head;
            while let Some(request_index) = request_link {
                let request_node = &associations.import_requests[request_index];
                let (request_span, request) = request_node.value;
                let entry_start = output.begin_static_import_entries()?;
                let mut entry_link = association.import_entries.head;
                while let Some(entry_index) = entry_link {
                    let entry_node = &associations.import_entries[entry_index];
                    let entry = serialize_import_entry(&mut output, entry_node.value)?;
                    output.push_static_import_entry(entry)?;
                    entry_link = entry_node.next;
                }
                let packed_entries = output
                    .finish_static_import_entries(entry_start, association.import_entries.length)?;
                let request = output.push_string(request)?;
                output.push_static_import(StaticImportRecord::new(
                    span(statement_span),
                    ValueSpanRecord::new(request, span(request_span)),
                    packed_entries,
                ))?;
                request_link = request_node.next;
            }
        }
        if association.exports.length != 0 {
            let entry_start = output.begin_static_export_entries()?;
            let mut module_request_cache = None;
            let mut export_link = association.exports.head;
            while let Some(export_index) = export_link {
                let export_node = &associations.exports[export_index];
                let entry = serialize_export_entry(
                    &mut output,
                    export_node.value,
                    &mut module_request_cache,
                )?;
                output.push_static_export_entry(entry)?;
                export_link = export_node.next;
            }
            let entries =
                output.finish_static_export_entries(entry_start, association.exports.length)?;
            output.push_static_export(StaticExportRecord::new(span(statement_span), entries))?;
        }
    }

    for dynamic in &record.dynamic_imports {
        output.push_dynamic_import(DynamicImportRecord::new(
            span(dynamic.span),
            span(dynamic.module_request),
        ))?;
    }
    for import_meta in &record.import_metas {
        output.push_import_meta(span(*import_meta))?;
    }
    Ok(output)
}

pub(super) fn serialize_rejection_module_names(
    record: &ModuleRecord<'_>,
    source: &str,
) -> RejectionModuleNames {
    // This is a rare failed-parse carrier. Most module names are identifiers, so reserving for
    // every possible import/export component can retain tens of bytes per unrelated record.
    let mut output = RejectionModuleNames::default();
    for entry in &record.import_entries {
        if let ImportImportName::Name(name) = &entry.import_name {
            retain_quoted_module_name(&mut output, name.span, source);
        }
    }
    for entry in record
        .local_export_entries
        .iter()
        .chain(record.indirect_export_entries.iter())
        .chain(record.star_export_entries.iter())
    {
        if let ExportImportName::Name(name) = &entry.import_name {
            retain_quoted_module_name(&mut output, name.span, source);
        }
        if let ExportExportName::Name(name) = &entry.export_name {
            retain_quoted_module_name(&mut output, name.span, source);
        }
        if let ExportLocalName::Name(name) | ExportLocalName::Default(name) = &entry.local_name {
            retain_quoted_module_name(&mut output, name.span, source);
        }
    }
    output
}

fn retain_quoted_module_name(
    output: &mut RejectionModuleNames,
    span: oxc_span::Span,
    source: &str,
) {
    if usize::try_from(span.start)
        .ok()
        .and_then(|start| source.as_bytes().get(start))
        .is_some_and(|byte| matches!(byte, b'\'' | b'"'))
    {
        output.push(span);
    }
}

fn serialize_import_entry(
    output: &mut ModuleTable,
    entry: &module_record::ImportEntry<'_>,
) -> Result<StaticImportEntryRecord, TapeBuildError> {
    let import_name = match &entry.import_name {
        ImportImportName::Name(name) => ModuleNameRecord::new(
            ImportNameKind::Name,
            OptionalStringRange::some(output.push_string(name.name.as_str())?),
            OptionalTapeSpan::some(span(name.span)),
        ),
        ImportImportName::NamespaceObject => ModuleNameRecord::new(
            ImportNameKind::NamespaceObject,
            OptionalStringRange::NONE,
            OptionalTapeSpan::NONE,
        ),
        ImportImportName::Default(default_span) => ModuleNameRecord::new(
            ImportNameKind::Default,
            OptionalStringRange::NONE,
            OptionalTapeSpan::some(span(*default_span)),
        ),
    };
    let local = output.push_string(entry.local_name.name.as_str())?;
    Ok(StaticImportEntryRecord::new(
        import_name,
        ValueSpanRecord::new(local, span(entry.local_name.span)),
        entry.is_type,
    ))
}

fn serialize_export_entry(
    output: &mut ModuleTable,
    entry: &module_record::ExportEntry<'_>,
    module_request_cache: &mut Option<((u32, u32), StringRange)>,
) -> Result<StaticExportEntryRecord, TapeBuildError> {
    let module_request = entry.module_request.as_ref().map_or_else(
        || Ok(OptionalValueSpanRecord::NONE),
        |request| {
            let key = span_key(request.span);
            let value = match *module_request_cache {
                Some((cached_key, value)) if cached_key == key => value,
                _ => {
                    let value = output.push_string(request.name.as_str())?;
                    *module_request_cache = Some((key, value));
                    value
                }
            };
            Ok(OptionalValueSpanRecord::some(ValueSpanRecord::new(value, span(request.span))))
        },
    )?;
    Ok(StaticExportEntryRecord::new(
        span(entry.span),
        module_request,
        export_import_name(output, &entry.import_name)?,
        export_export_name(output, &entry.export_name)?,
        export_local_name(output, &entry.local_name)?,
        entry.is_type,
    ))
}

fn export_import_name(
    output: &mut ModuleTable,
    name: &ExportImportName<'_>,
) -> Result<ModuleNameRecord<ExportImportNameKind>, TapeBuildError> {
    Ok(match name {
        ExportImportName::Name(name) => ModuleNameRecord::new(
            ExportImportNameKind::Name,
            OptionalStringRange::some(output.push_string(name.name.as_str())?),
            OptionalTapeSpan::some(span(name.span)),
        ),
        ExportImportName::All => ModuleNameRecord::new(
            ExportImportNameKind::All,
            OptionalStringRange::NONE,
            OptionalTapeSpan::NONE,
        ),
        ExportImportName::AllButDefault => ModuleNameRecord::new(
            ExportImportNameKind::AllButDefault,
            OptionalStringRange::NONE,
            OptionalTapeSpan::NONE,
        ),
        ExportImportName::Null => ModuleNameRecord::new(
            ExportImportNameKind::None,
            OptionalStringRange::NONE,
            OptionalTapeSpan::NONE,
        ),
    })
}

fn export_export_name(
    output: &mut ModuleTable,
    name: &ExportExportName<'_>,
) -> Result<ModuleNameRecord<ExportExportNameKind>, TapeBuildError> {
    Ok(match name {
        ExportExportName::Name(name) => ModuleNameRecord::new(
            ExportExportNameKind::Name,
            OptionalStringRange::some(output.push_string(name.name.as_str())?),
            OptionalTapeSpan::some(span(name.span)),
        ),
        ExportExportName::Default(default_span) => ModuleNameRecord::new(
            ExportExportNameKind::Default,
            OptionalStringRange::NONE,
            OptionalTapeSpan::some(span(*default_span)),
        ),
        ExportExportName::Null => ModuleNameRecord::new(
            ExportExportNameKind::None,
            OptionalStringRange::NONE,
            OptionalTapeSpan::NONE,
        ),
    })
}

fn export_local_name(
    output: &mut ModuleTable,
    name: &ExportLocalName<'_>,
) -> Result<ModuleNameRecord<ExportLocalNameKind>, TapeBuildError> {
    Ok(match name {
        ExportLocalName::Name(name) => ModuleNameRecord::new(
            ExportLocalNameKind::Name,
            OptionalStringRange::some(output.push_string(name.name.as_str())?),
            OptionalTapeSpan::some(span(name.span)),
        ),
        ExportLocalName::Default(name) => ModuleNameRecord::new(
            ExportLocalNameKind::Default,
            OptionalStringRange::some(output.push_string(name.name.as_str())?),
            OptionalTapeSpan::some(span(name.span)),
        ),
        ExportLocalName::Null => ModuleNameRecord::new(
            ExportLocalNameKind::None,
            OptionalStringRange::NONE,
            OptionalTapeSpan::NONE,
        ),
    })
}

fn span(span: oxc_span::Span) -> TapeSpan {
    TapeSpan::new(span.start, span.end)
}

fn span_key(span: oxc_span::Span) -> (u32, u32) {
    (span.start, span.end)
}

#[cfg(test)]
mod tests {
    use oxc_diagnostics::OxcDiagnostic;
    use oxc_span::Span;
    use tsrx_tape_schema::{DiagnosticPhase, DiagnosticSeverity, DiagnosticTable};

    use super::append_diagnostics;

    #[test]
    fn oxc_primary_labels_and_metadata_survive_neutral_serialization() {
        let diagnostic = OxcDiagnostic::warn("message")
            .with_label(Span::new(2, 5).primary_label("primary"))
            .with_help("help")
            .with_note("note")
            .with_error_code("scope", "number")
            .with_url("https://example.invalid");
        let mut table = DiagnosticTable::new();
        append_diagnostics(&mut table, [&diagnostic], DiagnosticPhase::Semantic)
            .expect("serialize OXC diagnostic");

        let record = &table.records()[0];
        assert_eq!(record.phase, DiagnosticPhase::Semantic);
        assert_eq!(record.severity, DiagnosticSeverity::Warning);
        assert_eq!(table.string(record.message), Some("message"));
        assert_eq!(table.optional_string(record.help), Some("help"));
        assert_eq!(table.optional_string(record.note), Some("note"));
        assert_eq!(table.optional_string(record.code_scope), Some("scope"));
        assert_eq!(table.optional_string(record.code_number), Some("number"));
        assert_eq!(table.optional_string(record.url), Some("https://example.invalid"));
        let labels = table.labels(record.labels).expect("labels");
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].span, tsrx_tape_schema::TapeSpan::new(2, 5));
        assert!(labels[0].primary);
        assert_eq!(table.optional_string(labels[0].message), Some("primary"));
    }
}
