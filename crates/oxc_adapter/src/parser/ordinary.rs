//! Direct ordinary JavaScript/TypeScript parsing through public OXC primitives.
//!
//! This module deliberately owns the revision-sensitive OXC conversion seam. Its result is made
//! only of project-owned Rust values, so no OXC arena borrow or Node-API type escapes the adapter.

use std::sync::Arc;

use oxc_allocator::Allocator;
use oxc_ast::ast::CommentKind;
use oxc_ast_visit::utf8_to_utf16::Utf8ToUtf16;
use oxc_diagnostics::{NamedSource, OxcDiagnostic, Severity};
use oxc_parser::{ParseOptions, Parser};
use oxc_semantic::SemanticBuilder;
use oxc_span::{SourceType, Span};
use oxc_syntax::module_record::{
    ExportEntry, ExportExportName, ExportImportName, ExportLocalName, ImportEntry,
    ImportImportName, ModuleRecord, NameSpan,
};
use rustc_hash::FxHashMap;

/// Pinned-`oxc-parser` options for the direct ordinary-language lane.
#[derive(Debug, Clone, Copy)]
pub struct OrdinaryParseRequest<'a> {
    pub filename: &'a str,
    pub source: &'a str,
    /// An explicit `lang` value. `None` preserves filename inference, including `.d.ts`, `.mjs`,
    /// `.cjs`, `.mts`, `.cts`, and OXC's extensionless default.
    pub lang: Option<&'a str>,
    pub source_type: Option<&'a str>,
    pub ast_type: Option<&'a str>,
    pub ranges: bool,
    pub preserve_parens: Option<bool>,
    pub show_semantic_errors: bool,
}

#[derive(Debug)]
pub struct OrdinaryParseResult {
    pub program_and_fixes: String,
    pub module: OrdinaryModule,
    pub comments: Vec<OrdinaryComment>,
    pub errors: Vec<OrdinaryDiagnostic>,
}

#[derive(Debug)]
pub struct OrdinaryComment {
    pub kind: &'static str,
    pub value: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug)]
pub struct OrdinaryDiagnosticLabel {
    pub message: Option<String>,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug)]
pub struct OrdinaryDiagnostic {
    pub severity: &'static str,
    pub message: String,
    pub labels: Vec<OrdinaryDiagnosticLabel>,
    pub help_message: Option<String>,
    pub codeframe: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdinaryNameKind {
    Name,
    NamespaceObject,
    Default,
    All,
    AllButDefault,
    None,
}

impl OrdinaryNameKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::NamespaceObject => "NamespaceObject",
            Self::Default => "Default",
            Self::All => "All",
            Self::AllButDefault => "AllButDefault",
            Self::None => "None",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OrdinarySpan {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug)]
pub struct OrdinaryValueSpan {
    pub value: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug)]
pub struct OrdinaryImportName {
    pub kind: OrdinaryNameKind,
    pub name: Option<String>,
    pub start: Option<u32>,
    pub end: Option<u32>,
}

#[derive(Debug)]
pub struct OrdinaryStaticImportEntry {
    pub import_name: OrdinaryImportName,
    pub local_name: OrdinaryValueSpan,
    pub is_type: bool,
}

#[derive(Debug)]
pub struct OrdinaryStaticImport {
    pub start: u32,
    pub end: u32,
    pub module_request: OrdinaryValueSpan,
    pub entries: Vec<OrdinaryStaticImportEntry>,
}

#[derive(Debug)]
pub struct OrdinaryExportImportName {
    pub kind: OrdinaryNameKind,
    pub name: Option<String>,
    pub start: Option<u32>,
    pub end: Option<u32>,
}

#[derive(Debug)]
pub struct OrdinaryExportExportName {
    pub kind: OrdinaryNameKind,
    pub name: Option<String>,
    pub start: Option<u32>,
    pub end: Option<u32>,
}

#[derive(Debug)]
pub struct OrdinaryExportLocalName {
    pub kind: OrdinaryNameKind,
    pub name: Option<String>,
    pub start: Option<u32>,
    pub end: Option<u32>,
}

#[derive(Debug)]
pub struct OrdinaryStaticExportEntry {
    pub start: u32,
    pub end: u32,
    pub module_request: Option<OrdinaryValueSpan>,
    pub import_name: OrdinaryExportImportName,
    pub export_name: OrdinaryExportExportName,
    pub local_name: OrdinaryExportLocalName,
    pub is_type: bool,
}

#[derive(Debug)]
pub struct OrdinaryStaticExport {
    pub start: u32,
    pub end: u32,
    pub entries: Vec<OrdinaryStaticExportEntry>,
}

#[derive(Debug, Clone, Copy)]
pub struct OrdinaryDynamicImport {
    pub start: u32,
    pub end: u32,
    pub module_request: OrdinarySpan,
}

#[derive(Debug, Default)]
pub struct OrdinaryModule {
    pub has_module_syntax: bool,
    pub static_imports: Vec<OrdinaryStaticImport>,
    pub static_exports: Vec<OrdinaryStaticExport>,
    pub dynamic_imports: Vec<OrdinaryDynamicImport>,
    pub import_metas: Vec<OrdinarySpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AstType {
    JavaScript,
    TypeScript,
}

fn source_type(request: OrdinaryParseRequest<'_>) -> SourceType {
    let inferred = match request.lang {
        Some("js") => SourceType::unambiguous(),
        Some("jsx") => SourceType::unambiguous().with_jsx(true),
        Some("ts") => SourceType::unambiguous().with_typescript(true),
        Some("tsx") => SourceType::unambiguous()
            .with_typescript(true)
            .with_jsx(true),
        Some("dts") => SourceType::d_ts(),
        _ => SourceType::from_path(request.filename).unwrap_or_default(),
    };
    match request.source_type {
        Some("script") => inferred.with_script(true),
        Some("module") => inferred.with_module(true),
        Some("commonjs") => inferred.with_commonjs(true),
        Some("unambiguous") => inferred.with_unambiguous(true),
        _ => inferred,
    }
}

fn ast_type(source_type: SourceType, requested: Option<&str>) -> AstType {
    match requested {
        Some("js") => AstType::JavaScript,
        Some("ts") => AstType::TypeScript,
        _ if source_type.is_javascript() => AstType::JavaScript,
        _ => AstType::TypeScript,
    }
}

fn span(value: Span) -> OrdinarySpan {
    OrdinarySpan {
        start: value.start,
        end: value.end,
    }
}

fn value_span(value: &NameSpan<'_>) -> OrdinaryValueSpan {
    OrdinaryValueSpan {
        value: value.name.to_string(),
        start: value.span.start,
        end: value.span.end,
    }
}

fn import_name(value: &ImportImportName<'_>) -> OrdinaryImportName {
    let (kind, name, start, end) = match value {
        ImportImportName::Name(name) => (
            OrdinaryNameKind::Name,
            Some(name.name.to_string()),
            Some(name.span.start),
            Some(name.span.end),
        ),
        ImportImportName::NamespaceObject => (OrdinaryNameKind::NamespaceObject, None, None, None),
        ImportImportName::Default(span) => (
            OrdinaryNameKind::Default,
            None,
            Some(span.start),
            Some(span.end),
        ),
    };
    OrdinaryImportName {
        kind,
        name,
        start,
        end,
    }
}

fn import_entry(value: &ImportEntry<'_>) -> OrdinaryStaticImportEntry {
    OrdinaryStaticImportEntry {
        import_name: import_name(&value.import_name),
        local_name: value_span(&value.local_name),
        is_type: value.is_type,
    }
}

fn export_import_name(value: &ExportImportName<'_>) -> OrdinaryExportImportName {
    let (kind, name, start, end) = match value {
        ExportImportName::Name(name) => (
            OrdinaryNameKind::Name,
            Some(name.name.to_string()),
            Some(name.span.start),
            Some(name.span.end),
        ),
        ExportImportName::All => (OrdinaryNameKind::All, None, None, None),
        ExportImportName::AllButDefault => (OrdinaryNameKind::AllButDefault, None, None, None),
        ExportImportName::Null => (OrdinaryNameKind::None, None, None, None),
    };
    OrdinaryExportImportName {
        kind,
        name,
        start,
        end,
    }
}

fn export_export_name(value: &ExportExportName<'_>) -> OrdinaryExportExportName {
    let (kind, name, start, end) = match value {
        ExportExportName::Name(name) => (
            OrdinaryNameKind::Name,
            Some(name.name.to_string()),
            Some(name.span.start),
            Some(name.span.end),
        ),
        ExportExportName::Default(span) => (
            OrdinaryNameKind::Default,
            None,
            Some(span.start),
            Some(span.end),
        ),
        ExportExportName::Null => (OrdinaryNameKind::None, None, None, None),
    };
    OrdinaryExportExportName {
        kind,
        name,
        start,
        end,
    }
}

fn export_local_name(value: &ExportLocalName<'_>) -> OrdinaryExportLocalName {
    let (kind, name, start, end) = match value {
        ExportLocalName::Name(name) => (
            OrdinaryNameKind::Name,
            Some(name.name.to_string()),
            Some(name.span.start),
            Some(name.span.end),
        ),
        ExportLocalName::Default(name) => (
            OrdinaryNameKind::Default,
            Some(name.name.to_string()),
            Some(name.span.start),
            Some(name.span.end),
        ),
        ExportLocalName::Null => (OrdinaryNameKind::None, None, None, None),
    };
    OrdinaryExportLocalName {
        kind,
        name,
        start,
        end,
    }
}

fn export_entry(value: &ExportEntry<'_>) -> OrdinaryStaticExportEntry {
    OrdinaryStaticExportEntry {
        start: value.span.start,
        end: value.span.end,
        module_request: value.module_request.as_ref().map(value_span),
        import_name: export_import_name(&value.import_name),
        export_name: export_export_name(&value.export_name),
        local_name: export_local_name(&value.local_name),
        is_type: value.is_type,
    }
}

fn module(record: &ModuleRecord<'_>) -> OrdinaryModule {
    let mut static_imports = record
        .requested_modules
        .iter()
        .flat_map(|(name, requests)| {
            requests
                .iter()
                .filter(|request| request.is_import)
                .map(|request| {
                    let entries = record
                        .import_entries
                        .iter()
                        .filter(|entry| entry.statement_span == request.statement_span)
                        .map(import_entry)
                        .collect();
                    OrdinaryStaticImport {
                        start: request.statement_span.start,
                        end: request.statement_span.end,
                        module_request: OrdinaryValueSpan {
                            value: name.to_string(),
                            start: request.span.start,
                            end: request.span.end,
                        },
                        entries,
                    }
                })
        })
        .collect::<Vec<_>>();
    static_imports.sort_unstable_by_key(|entry| entry.start);

    let mut grouped = FxHashMap::<(u32, u32), Vec<OrdinaryStaticExportEntry>>::default();
    for entry in record
        .local_export_entries
        .iter()
        .chain(record.indirect_export_entries.iter())
        .chain(record.star_export_entries.iter())
    {
        grouped
            .entry((entry.statement_span.start, entry.statement_span.end))
            .or_default()
            .push(export_entry(entry));
    }
    let mut static_exports = grouped
        .into_iter()
        .map(|((start, end), entries)| OrdinaryStaticExport {
            start,
            end,
            entries,
        })
        .collect::<Vec<_>>();
    static_exports.sort_unstable_by_key(|entry| (entry.start, entry.end));
    let dynamic_imports = record
        .dynamic_imports
        .iter()
        .map(|entry| OrdinaryDynamicImport {
            start: entry.span.start,
            end: entry.span.end,
            module_request: span(entry.module_request),
        })
        .collect();
    let import_metas = record.import_metas.iter().copied().map(span).collect();
    OrdinaryModule {
        has_module_syntax: record.has_module_syntax,
        static_imports,
        static_exports,
        dynamic_imports,
        import_metas,
    }
}

fn convert_diagnostics(
    filename: &str,
    source_text: &str,
    diagnostics: impl IntoIterator<Item = OxcDiagnostic>,
) -> Vec<OrdinaryDiagnostic> {
    let diagnostics = diagnostics.into_iter().collect::<Vec<_>>();
    if diagnostics.is_empty() {
        return Vec::new();
    }
    let source = Arc::new(NamedSource::new(filename, source_text.to_string()));
    diagnostics
        .into_iter()
        .map(|diagnostic| {
            let severity = match diagnostic.severity {
                Severity::Error => "Error",
                Severity::Warning => "Warning",
                Severity::Advice => "Advice",
            };
            let message = diagnostic.message.to_string();
            let labels = diagnostic
                .labels
                .iter()
                .map(|label| OrdinaryDiagnosticLabel {
                    message: label.label().map(ToString::to_string),
                    start: label.offset(),
                    end: label.offset() + label.len(),
                })
                .collect();
            let help_message = diagnostic.help.as_ref().map(ToString::to_string);
            let codeframe = format!("{:?}", diagnostic.with_source_code(Arc::clone(&source)));
            OrdinaryDiagnostic {
                severity,
                message,
                labels,
                help_message,
                codeframe,
            }
        })
        .collect()
}

/// Parses an ordinary language directly through the pinned public OXC crates.
///
/// This path never constructs a TSRX source bridge or tape and never calls the TSRX syntax or
/// parser-engine crates. All OXC-owned values die before this function returns.
#[must_use]
pub fn parse_ordinary(request: OrdinaryParseRequest<'_>) -> OrdinaryParseResult {
    let allocator = Allocator::default();
    let source_type = source_type(request);
    let ast_type = ast_type(source_type, request.ast_type);
    let parsed = Parser::new(&allocator, request.source, source_type)
        .with_options(ParseOptions {
            preserve_parens: request.preserve_parens.unwrap_or(true),
            ..ParseOptions::default()
        })
        .parse();
    let mut program = parsed.program;
    let mut module_record = parsed.module_record;
    let mut diagnostics = parsed.diagnostics;
    if request.show_semantic_errors {
        diagnostics.extend(SemanticBuilder::new_compiler().build(&program).diagnostics);
    }

    // Codeframes must be rendered against original UTF-8 offsets before the public spans move to
    // JavaScript's UTF-16 coordinate domain.
    let mut errors = convert_diagnostics(request.filename, request.source, diagnostics);
    let mut comments = program
        .comments
        .iter()
        .map(|comment| OrdinaryComment {
            kind: match comment.kind {
                CommentKind::Line => "Line",
                CommentKind::SingleLineBlock | CommentKind::MultiLineBlock => "Block",
            },
            value: comment
                .content_span()
                .source_text(request.source)
                .to_string(),
            start: comment.span.start,
            end: comment.span.end,
        })
        .collect::<Vec<_>>();

    let converter = Utf8ToUtf16::new(request.source);
    converter.convert_program(&mut program);
    converter.convert_module_record(&mut module_record);
    if let Some(mut offsets) = converter.converter() {
        for comment in &mut comments {
            offsets.convert_offset(&mut comment.start);
            offsets.convert_offset(&mut comment.end);
        }
        for error in &mut errors {
            for label in &mut error.labels {
                offsets.convert_offset(&mut label.start);
                offsets.convert_offset(&mut label.end);
            }
        }
    }
    if ast_type == AstType::JavaScript
        && let Some(hashbang) = &program.hashbang
    {
        comments.insert(
            0,
            OrdinaryComment {
                kind: "Line",
                value: hashbang.value.to_string(),
                start: hashbang.span.start,
                end: hashbang.span.end,
            },
        );
    }

    OrdinaryParseResult {
        program_and_fixes: program
            .to_estree_json_with_fixes(ast_type == AstType::TypeScript, request.ranges),
        module: module(&module_record),
        comments,
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::{AstType, OrdinaryParseRequest, ast_type, parse_ordinary, source_type};

    fn request<'a>(filename: &'a str, source: &'a str) -> OrdinaryParseRequest<'a> {
        OrdinaryParseRequest {
            filename,
            source,
            lang: None,
            source_type: None,
            ast_type: None,
            ranges: false,
            preserve_parens: None,
            show_semantic_errors: false,
        }
    }

    #[test]
    fn preserves_filename_inference_and_explicit_language_distinction() {
        assert!(source_type(request("x.cjs", "")).is_commonjs());
        assert!(source_type(request("x.mjs", "")).is_module());
        assert!(source_type(request("x.d.ts", "")).is_typescript_definition());
        let mut explicit = request("x.cjs", "");
        explicit.lang = Some("js");
        assert!(source_type(explicit).is_unambiguous());
    }

    #[test]
    fn ast_type_override_is_independent_of_parse_language() {
        let source_type = source_type(request("x.ts", ""));
        assert_eq!(ast_type(source_type, None), AstType::TypeScript);
        assert_eq!(ast_type(source_type, Some("js")), AstType::JavaScript);
    }

    #[test]
    fn maps_astral_program_module_comment_and_error_spans_to_utf16() {
        let source = "/*😀*/ import x from '😀'; const x = 1;";
        let mut input = request("x.js", source);
        input.show_semantic_errors = true;
        let result = parse_ordinary(input);
        assert_eq!((result.comments[0].start, result.comments[0].end), (0, 6));
        assert_eq!(result.module.static_imports[0].module_request.start, 21);
        assert!(
            result
                .errors
                .iter()
                .flat_map(|error| &error.labels)
                .all(|label| {
                    usize::try_from(label.end).expect("label end") <= source.encode_utf16().count()
                })
        );
    }
}
