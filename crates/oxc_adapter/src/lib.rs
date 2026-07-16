//! The only revision-specific boundary between OXC for TSRX and canonical OXC crates.

pub mod editor;

use std::{
    collections::BTreeMap,
    ffi::OsStr,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    sync::{Arc, Mutex},
    time::Instant,
};

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrayExpression, BinaryExpression, CallExpression, ChainElement, Expression, JSXAttributeValue,
    JSXOpeningElement, NewExpression, ObjectExpression, SpreadElement, TaggedTemplateExpression,
    TemplateLiteral,
};
use oxc_ast_visit::{Visit, walk};
use oxc_config::{ConfigDiscovery, ConfigFileNames, DiscoveredConfigFile, is_js_config_path};
use oxc_formatter::{
    ArrowParentheses, AttributePosition, BracketSameLine, BracketSpacing,
    EmbeddedLanguageFormatting, Expand, JsFormatOptions, QuoteProperties, QuoteStyle, Semicolons,
    TrailingCommas, format_program, parse_for_format,
};
use oxc_formatter_core::{IndentStyle, IndentWidth, LineEnding, LineWidth};
use oxc_linter::{
    AllowWarnDeny, ConfigBuilderError, ConfigStore, ConfigStoreBuilder, ContextSubHost,
    ContextSubHostOptions, DisableDirectives, ExternalPluginStore, FixKind,
    LintFilter as OxcLintFilter, LintIgnoreMatcher, LintOptions, Linter, Message, ModuleRecord,
    Oxlintrc, PossibleFixes, RuntimeFileSystem, TsGoLintState,
};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::operator::{BinaryOperator, UnaryOperator};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

pub const OXC_REVISION: &str = "8e0ed2ebb96137fb1611cdbd5742d5cb46037d40";
pub const SUPPORTED_TSGOLINT_VERSION: &str = "0.24.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    JavaScript,
    JavaScriptReact,
    TypeScript,
    TypeScriptReact,
}

impl SourceKind {
    /// Infers the canonical OXC source type from a standard JavaScript/TypeScript path.
    ///
    /// # Errors
    ///
    /// Returns an error for extensions outside `.js`, `.jsx`, `.ts`, and `.tsx` families.
    pub fn from_path(path: &Path) -> Result<Self, String> {
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("js" | "mjs" | "cjs") => Ok(Self::JavaScript),
            Some("jsx") => Ok(Self::JavaScriptReact),
            Some("ts" | "mts" | "cts") => Ok(Self::TypeScript),
            Some("tsx") => Ok(Self::TypeScriptReact),
            extension => Err(format!("Unsupported source extension: {extension:?}")),
        }
    }

    fn source_type(self) -> SourceType {
        match self {
            Self::JavaScript => SourceType::unambiguous(),
            Self::JavaScriptReact => SourceType::jsx(),
            Self::TypeScript => SourceType::ts(),
            Self::TypeScriptReact => SourceType::tsx(),
        }
    }
}

#[derive(Debug)]
pub struct FormatRequest<'a> {
    pub parse_source: &'a str,
    pub source_kind: SourceKind,
    pub dynamic_tags: Option<DynamicTagContract<'a>>,
    pub options: Option<&'a FormatOptions>,
}

/// Oxfmt-compatible options that affect JavaScript, TypeScript, JSX, and TSRX output.
///
/// This project-owned representation keeps revision-specific OXC option types inside this adapter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormatOptions {
    pub use_tabs: Option<bool>,
    pub tab_width: Option<u8>,
    pub end_of_line: Option<String>,
    pub print_width: Option<u16>,
    pub single_quote: Option<bool>,
    pub jsx_single_quote: Option<bool>,
    pub quote_props: Option<String>,
    pub trailing_comma: Option<String>,
    pub semi: Option<bool>,
    pub arrow_parens: Option<String>,
    pub bracket_spacing: Option<bool>,
    pub bracket_same_line: Option<bool>,
    pub object_wrap: Option<String>,
    pub single_attribute_per_line: Option<bool>,
    pub embedded_language_formatting: Option<String>,
    pub html_whitespace_sensitivity: Option<String>,
}

/// Collision-free scaffold contract for validating TSRX dynamic names in an existing OXC AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynamicTagContract<'a> {
    pub prefix: &'a str,
    pub count: u32,
    pub original_offsets: &'a [u32],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FormatEngineTimings {
    pub parse_ns: u64,
    pub format_ns: u64,
}

#[derive(Debug)]
pub struct EngineFormatResult {
    pub code: String,
    pub timings: FormatEngineTimings,
    pub parse_count: u32,
}

/// Formats one legal JavaScript/TypeScript projection with canonical Oxfmt.
///
/// This deliberately calls [`parse_for_format`] once and [`format_program`] once. Keeping this
/// sequence here prevents revision-specific OXC APIs from leaking into the TSRX language crates
/// and makes the one-parse invariant directly inspectable.
///
/// # Errors
///
/// Returns an error when canonical OXC parsing or document printing fails.
pub fn format(request: &FormatRequest<'_>) -> Result<EngineFormatResult, String> {
    let allocator = Allocator::default();
    let source_type = request.source_kind.source_type();

    let started = Instant::now();
    let parsed = parse_for_format(&allocator, request.parse_source, source_type);
    if !parsed.diagnostics.is_empty() {
        let errors = parsed
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("OXC formatter parse failed: {errors}"));
    }
    validate_dynamic_tags(&parsed.program, request.dynamic_tags)?;
    let parse_ns = elapsed_ns(started);

    let started = Instant::now();
    let options = request
        .options
        .map_or_else(|| Ok(JsFormatOptions::default()), js_format_options)?;
    let code = format_program(&allocator, &parsed.program, options, None)
        .print()
        .map_err(|error| format!("OXC formatter print failed: {error}"))?
        .into_code();
    let format_ns = elapsed_ns(started);

    Ok(EngineFormatResult {
        code,
        timings: FormatEngineTimings {
            parse_ns,
            format_ns,
        },
        parse_count: 1,
    })
}

fn js_format_options(options: &FormatOptions) -> Result<JsFormatOptions, String> {
    let mut resolved = JsFormatOptions::default();
    if let Some(use_tabs) = options.use_tabs {
        resolved.indent_style = if use_tabs {
            IndentStyle::Tab
        } else {
            IndentStyle::Space
        };
    }
    if let Some(width) = options.tab_width {
        resolved.indent_width = IndentWidth::try_from(width)
            .map_err(|error| format!("invalid Oxfmt tabWidth {width}: {error}"))?;
    }
    if let Some(value) = &options.end_of_line {
        resolved.line_ending = LineEnding::from_str(value)
            .map_err(|error| format!("invalid Oxfmt endOfLine `{value}`: {error}"))?;
    }
    if let Some(width) = options.print_width {
        resolved.line_width = LineWidth::try_from(width)
            .map_err(|error| format!("invalid Oxfmt printWidth {width}: {error}"))?;
    }
    if let Some(single) = options.single_quote {
        resolved.quote_style = if single {
            QuoteStyle::Single
        } else {
            QuoteStyle::Double
        };
    }
    if let Some(single) = options.jsx_single_quote {
        resolved.jsx_quote_style = if single {
            QuoteStyle::Single
        } else {
            QuoteStyle::Double
        };
    }
    if let Some(value) = &options.quote_props {
        resolved.quote_properties = QuoteProperties::from_str(value)
            .map_err(|error| format!("invalid Oxfmt quoteProps `{value}`: {error}"))?;
    }
    if let Some(value) = &options.trailing_comma {
        resolved.trailing_commas = TrailingCommas::from_str(value)
            .map_err(|error| format!("invalid Oxfmt trailingComma `{value}`: {error}"))?;
    }
    if let Some(semi) = options.semi {
        resolved.semicolons = if semi {
            Semicolons::Always
        } else {
            Semicolons::AsNeeded
        };
    }
    if let Some(value) = &options.arrow_parens {
        resolved.arrow_parentheses = match value.as_str() {
            "avoid" => ArrowParentheses::AsNeeded,
            "always" => ArrowParentheses::Always,
            _ => {
                return Err(format!(
                    "invalid Oxfmt arrowParens `{value}`: expected `always` or `avoid`"
                ));
            }
        };
    }
    if let Some(spacing) = options.bracket_spacing {
        resolved.bracket_spacing = BracketSpacing::from(spacing);
    }
    if let Some(same_line) = options.bracket_same_line {
        resolved.bracket_same_line = BracketSameLine::from(same_line);
    }
    if let Some(value) = &options.object_wrap {
        resolved.expand = match value.as_str() {
            "preserve" => Expand::Auto,
            "collapse" => Expand::Never,
            _ => return Err(format!("invalid Oxfmt objectWrap `{value}`")),
        };
    }
    if let Some(single_attribute) = options.single_attribute_per_line {
        resolved.attribute_position = if single_attribute {
            AttributePosition::Multiline
        } else {
            AttributePosition::Auto
        };
    }
    if let Some(value) = &options.embedded_language_formatting {
        resolved.embedded_language_formatting = EmbeddedLanguageFormatting::from_str(value)
            .map_err(|error| {
                format!("invalid Oxfmt embeddedLanguageFormatting `{value}`: {error}")
            })?;
    }
    if let Some(value) = &options.html_whitespace_sensitivity {
        resolved.html_whitespace_sensitivity_ignore = match value.as_str() {
            "ignore" => true,
            "css" | "strict" => false,
            _ => return Err(format!("invalid Oxfmt htmlWhitespaceSensitivity `{value}`")),
        };
    }
    Ok(resolved)
}

#[derive(Debug)]
pub struct LintRequest<'a> {
    pub path: &'a Path,
    pub original_source: &'a str,
    pub parse_source: &'a str,
    pub source_kind: SourceKind,
    pub rules: &'a [String],
    pub collect_fixes: bool,
    pub dynamic_tags: Option<DynamicTagContract<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSeverity {
    Allow,
    Warn,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFilter {
    pub severity: RuleSeverity,
    pub name: String,
}

#[derive(Debug)]
pub struct LintEngineOptions<'a> {
    pub cwd: &'a Path,
    pub config_path: Option<&'a Path>,
    /// Directory against which a materialized configuration's relative paths are resolved.
    ///
    /// The thin Vite+ host uses this when the JSON payload lives in a disposable directory but
    /// was authored in the consumer project. Ordinary JSON/JSONC loading leaves it unset.
    pub config_base: Option<&'a Path>,
    pub filters: &'a [RuleFilter],
    pub collect_fixes: bool,
}

/// One compiled Oxlint configuration reused across a native lint batch.
pub struct LintEngine {
    linter: Linter,
    config_store: ConfigStore,
    ignore_matcher: LintIgnoreMatcher,
    config_path: Option<PathBuf>,
    config_load_ns: u64,
    number_of_rules: usize,
    collect_fixes: bool,
    deny_warnings: bool,
    max_warnings: Option<usize>,
    cwd: PathBuf,
    type_mode: TypeMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeMode {
    Disabled,
    Aware,
    Check,
}

impl LintEngine {
    /// Discover and compile one JSON/JSONC Oxlint configuration.
    ///
    /// # Errors
    ///
    /// Returns an actionable error for invalid, conflicting, JavaScript/TypeScript, external-plugin,
    /// or type-aware configuration before any source file is parsed or changed.
    pub fn new(options: &LintEngineOptions<'_>) -> Result<Self, String> {
        Self::new_with_capabilities(options, false, false)
    }

    /// Compile the same configuration with the explicit TypeScript-Go opt-in.
    ///
    /// # Errors
    ///
    /// Returns the same configuration errors as [`Self::new`]. Executable discovery happens only
    /// when a type-aware source is linted.
    pub fn new_type_aware(
        options: &LintEngineOptions<'_>,
        type_check: bool,
    ) -> Result<Self, String> {
        Self::new_with_capabilities(options, true, type_check)
    }

    fn new_with_capabilities(
        options: &LintEngineOptions<'_>,
        type_aware: bool,
        requested_type_check: bool,
    ) -> Result<Self, String> {
        let started = Instant::now();
        let (config, config_path) =
            load_oxlintrc(options.cwd, options.config_path, options.config_base)?;
        reject_unavailable_lint_capabilities(&config, type_aware)?;
        let type_check = requested_type_check || config.options.type_check == Some(true);

        let base_root = config.dir().unwrap_or(options.cwd).to_path_buf();
        let ignore_patterns = config.ignore_patterns.clone();
        let mut external_plugin_store = ExternalPluginStore::new(false);
        let filters = options
            .filters
            .iter()
            .map(|filter| {
                OxcLintFilter::new(
                    match filter.severity {
                        RuleSeverity::Allow => oxc_linter::AllowWarnDeny::Allow,
                        RuleSeverity::Warn => oxc_linter::AllowWarnDeny::Warn,
                        RuleSeverity::Deny => oxc_linter::AllowWarnDeny::Deny,
                    },
                    filter.name.clone(),
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        let built = ConfigStoreBuilder::from_oxlintrc(
            false,
            config,
            None,
            &mut external_plugin_store,
            None,
        )
        .map_err(config_builder_error)?
        .with_filters(filters.iter())
        .build(&mut external_plugin_store)
        .map_err(config_builder_error)?;
        let config_store = ConfigStore::new(built, FxHashMap::default(), external_plugin_store);
        let number_of_rules = config_store.number_of_rules(type_aware).unwrap_or(0);
        let deny_warnings = config_store.deny_warnings();
        let max_warnings = config_store.max_warnings();
        let lint_options = LintOptions {
            fix: if options.collect_fixes {
                FixKind::SafeFix
            } else {
                FixKind::None
            },
            ..LintOptions::default()
        };
        let linter = Linter::new(lint_options, config_store.clone(), None);
        Ok(Self {
            linter,
            config_store,
            ignore_matcher: LintIgnoreMatcher::new(&ignore_patterns, &base_root, Vec::new()),
            config_path,
            config_load_ns: elapsed_ns(started),
            number_of_rules,
            collect_fixes: options.collect_fixes,
            deny_warnings,
            max_warnings,
            cwd: options.cwd.to_path_buf(),
            type_mode: if !type_aware {
                TypeMode::Disabled
            } else if type_check {
                TypeMode::Check
            } else {
                TypeMode::Aware
            },
        })
    }

    #[must_use]
    pub fn should_ignore(&self, path: &Path) -> bool {
        let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.ignore_matcher.should_ignore(&normalized)
    }

    #[must_use]
    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    #[must_use]
    pub fn config_load_ns(&self) -> u64 {
        self.config_load_ns
    }

    #[must_use]
    pub fn config_loads(&self) -> u32 {
        u32::from(self.config_path.is_some())
    }

    #[must_use]
    pub fn number_of_rules(&self) -> usize {
        self.number_of_rules
    }

    #[must_use]
    pub fn deny_warnings(&self) -> bool {
        self.deny_warnings
    }

    #[must_use]
    pub fn max_warnings(&self) -> Option<usize> {
        self.max_warnings
    }

    #[must_use]
    pub const fn type_aware_enabled(&self) -> bool {
        !matches!(self.type_mode, TypeMode::Disabled)
    }

    #[must_use]
    pub const fn type_check_enabled(&self) -> bool {
        matches!(self.type_mode, TypeMode::Check)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSpan {
    pub offset: u32,
    pub length: u32,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineFix {
    pub offset: u32,
    pub length: u32,
    pub replacement: String,
    pub safe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineDiagnostic {
    pub rule: Option<String>,
    pub plugin: Option<String>,
    pub code: String,
    pub severity: String,
    pub message: String,
    pub labels: Vec<EngineSpan>,
    pub fixes: Vec<EngineFix>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EngineTimings {
    pub parse_ns: u64,
    pub semantic_ns: u64,
    pub lint_ns: u64,
}

#[derive(Debug)]
pub struct LintResult {
    pub diagnostics: Vec<EngineDiagnostic>,
    pub timings: EngineTimings,
    pub parse_count: u32,
    pub disable_directives: Option<DisableDirectives>,
}

#[derive(Debug)]
pub struct TypeLintRequest<'a> {
    pub virtual_path: &'a Path,
    pub projected_source: &'a str,
    pub collect_fixes: bool,
    pub disable_directives: Option<DisableDirectives>,
}

#[derive(Debug)]
pub struct TypeLintResult {
    pub diagnostics: Vec<EngineDiagnostic>,
    pub elapsed_ns: u64,
    pub process_count: u32,
}

#[derive(Debug)]
pub struct TypeBatchFile<'a> {
    pub authored_path: &'a Path,
    pub virtual_path: &'a Path,
    pub projected_source: &'a str,
    pub disable_directives: Option<&'a DisableDirectives>,
}

#[derive(Debug)]
pub struct TypeBatchDiagnostic {
    pub virtual_path: Option<PathBuf>,
    pub diagnostic: EngineDiagnostic,
}

#[derive(Debug)]
pub struct TypeBatchResult {
    pub diagnostics: Vec<TypeBatchDiagnostic>,
    pub elapsed_ns: u64,
    pub process_count: u32,
}

/// Runs canonical OXC parsing, semantic analysis, and selected built-in lint rules.
///
/// # Errors
///
/// Returns an error when parsing, semantic construction, rule selection, or linter configuration
/// fails. No partial diagnostic set or edit is returned on those failures.
pub fn lint(request: &LintRequest<'_>) -> Result<LintResult, String> {
    let filters = request
        .rules
        .iter()
        .map(|name| RuleFilter {
            severity: RuleSeverity::Deny,
            name: name.clone(),
        })
        .collect::<Vec<_>>();
    let engine = LintEngine::new(&LintEngineOptions {
        cwd: request.path.parent().unwrap_or_else(|| Path::new(".")),
        config_path: None,
        config_base: None,
        filters: &filters,
        collect_fixes: request.collect_fixes,
    })?;
    engine.lint(request)
}

impl LintEngine {
    /// Run the already-compiled configuration over one legal JS/TSX source buffer.
    ///
    /// # Errors
    ///
    /// Returns an error for parser, semantic, dynamic-tag, or unavailable type-aware behavior.
    pub fn lint(&self, request: &LintRequest<'_>) -> Result<LintResult, String> {
        if request.collect_fixes != self.collect_fixes {
            return Err("lint request fix mode differs from the compiled lint session".to_string());
        }
        let allocator = Allocator::default();
        let source_type = request.source_kind.source_type();

        let started = Instant::now();
        let mut parsed = Parser::new(&allocator, request.parse_source, source_type).parse();
        if !parsed.diagnostics.is_empty() {
            let errors = parsed
                .diagnostics
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!("OXC parse failed: {errors}"));
        }
        validate_dynamic_tags(&parsed.program, request.dynamic_tags)?;
        let parse_ns = elapsed_ns(started);

        // AST spans address the legal TSX projection. Expanded framework projections are mapped back
        // by the TSRX adapter after linting, so source-sensitive rules must inspect this same buffer.
        parsed.program.source_text = request.parse_source;

        let started = Instant::now();
        let semantic_return = SemanticBuilder::new_linter().build(&parsed.program);
        let semantic_ns = elapsed_ns(started);
        if !semantic_return.diagnostics.is_empty() {
            let errors = semantic_return
                .diagnostics
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!("OXC semantic analysis failed: {errors}"));
        }
        let semantic = semantic_return.semantic;

        let started = Instant::now();
        let module_record = Arc::new(ModuleRecord::new(
            request.path,
            &parsed.module_record,
            &semantic,
        ));
        let mut context_options = ContextSubHostOptions::default();
        context_options.respect_eslint_disable_directives =
            self.config_store.respect_eslint_disable_directives();
        let context_sub_hosts = vec![ContextSubHost::new(
            semantic,
            module_record,
            0,
            context_options,
        )];
        let (messages, disable_directives) = if self.type_aware_enabled() {
            self.linter.run_with_disable_directives::<false>(
                request.path,
                context_sub_hosts,
                &allocator,
                None,
                None,
            )
        } else {
            (
                self.linter.run(request.path, context_sub_hosts, &allocator),
                None,
            )
        };
        let lint_ns = elapsed_ns(started);

        let diagnostics = messages.iter().map(map_message).collect();

        Ok(LintResult {
            diagnostics,
            timings: EngineTimings {
                parse_ns,
                semantic_ns,
                lint_ns,
            },
            parse_count: 1,
            disable_directives,
        })
    }

    /// Runs the official public TypeScript-Go source-override seam for one in-memory projection.
    ///
    /// # Errors
    ///
    /// Returns an actionable error when the opt-in is disabled, the supported executable is
    /// missing, projected source cannot be read, or tsgolint fails. No generated file is written.
    pub fn lint_types(&self, request: TypeLintRequest<'_>) -> Result<TypeLintResult, String> {
        if !self.type_aware_enabled() {
            return Err("type-aware linting requires the explicit opt-in".to_string());
        }
        if request.collect_fixes != self.collect_fixes {
            return Err(
                "type-aware lint request fix mode differs from the compiled lint session"
                    .to_string(),
            );
        }
        let started = Instant::now();
        let state = TsGoLintState::try_new(
            &self.cwd,
            self.config_store.clone(),
            if request.collect_fixes {
                FixKind::SafeFix
            } else {
                FixKind::None
            },
        )?
        .with_silent(true)
        .with_type_check(self.type_check_enabled());
        let file_system = ProjectedFileSystem {
            path: request.virtual_path,
            source: request.projected_source,
        };
        let mut directives = FxHashMap::default();
        if let Some(disable_directives) = request.disable_directives {
            directives.insert(request.virtual_path.to_path_buf(), disable_directives);
        }
        let paths: Vec<Arc<OsStr>> = vec![Arc::from(request.virtual_path.as_os_str())];
        let messages = state.lint_source(&paths, &file_system, Arc::new(Mutex::new(directives)))?;
        Ok(TypeLintResult {
            diagnostics: messages.iter().map(map_message).collect(),
            elapsed_ns: elapsed_ns(started),
            process_count: 1,
        })
    }

    /// Runs one documented tsgolint protocol-v2 batch while preserving each virtual file path.
    ///
    /// OXC's public single-source helper converts diagnostics to `Message` and drops `file_path`.
    /// The documented headless stream is therefore required for a correct multi-file project
    /// batch. Rules are resolved against each authored path before its `.tsrx.tsx` virtual name is
    /// sent, so user overrides retain their original meaning.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing executable, invalid rule serialization, protocol corruption,
    /// or a failed tsgolint process. Sources are transferred only through stdin.
    pub fn lint_type_batch(
        &self,
        files: &[TypeBatchFile<'_>],
        collect_fixes: bool,
    ) -> Result<TypeBatchResult, String> {
        if !self.type_aware_enabled() {
            return Err("type-aware linting requires the explicit opt-in".to_string());
        }
        if collect_fixes != self.collect_fixes {
            return Err(
                "type-aware lint request fix mode differs from the compiled lint session"
                    .to_string(),
            );
        }
        let started = Instant::now();
        let prepared = prepare_type_batch(self, files)?;
        if prepared.payload.configs.is_empty() {
            return Ok(TypeBatchResult {
                diagnostics: Vec::new(),
                elapsed_ns: elapsed_ns(started),
                process_count: 0,
            });
        }
        let executable = find_tsgolint_executable(&self.cwd)?;
        verify_tsgolint_version(&executable)?;
        let diagnostics = run_type_protocol(&executable, collect_fixes, &prepared)?;
        Ok(TypeBatchResult {
            diagnostics,
            elapsed_ns: elapsed_ns(started),
            process_count: 1,
        })
    }
}

struct ProjectedFileSystem<'a> {
    path: &'a Path,
    source: &'a str,
}

impl RuntimeFileSystem for ProjectedFileSystem<'_> {
    fn read_to_arena_str<'a>(
        &self,
        path: &Path,
        allocator: &'a Allocator,
    ) -> Result<&'a str, io::Error> {
        if path == self.path {
            Ok(allocator.alloc_str(self.source))
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no in-memory TSRX projection for {}", path.display()),
            ))
        }
    }

    fn write_file(&self, path: &Path, _content: &str) -> Result<(), io::Error> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "type-aware source overrides are read-only: {}",
                path.display()
            ),
        ))
    }
}

struct PreparedTypeBatch<'a> {
    payload: ProtocolPayload,
    severities: FxHashMap<PathBuf, FxHashMap<String, AllowWarnDeny>>,
    directives: FxHashMap<PathBuf, &'a DisableDirectives>,
}

fn prepare_type_batch<'a>(
    engine: &LintEngine,
    files: &'a [TypeBatchFile<'a>],
) -> Result<PreparedTypeBatch<'a>, String> {
    let mut groups = BTreeMap::<String, ProtocolConfigGroup>::new();
    let mut source_overrides = FxHashMap::default();
    let mut severities = FxHashMap::default();
    let mut directives = FxHashMap::default();
    for file in files {
        let virtual_path = file.virtual_path.to_string_lossy().into_owned();
        source_overrides.insert(virtual_path.clone(), file.projected_source.to_string());
        let (rules, file_severities) = resolved_protocol_rules(engine, file.authored_path)?;
        if !rules.is_empty() || engine.type_check_enabled() {
            let signature = serde_json::to_string(&rules)
                .map_err(|error| format!("unable to group type-aware rules: {error}"))?;
            groups
                .entry(signature)
                .or_insert_with(|| ProtocolConfigGroup {
                    rules: rules.clone(),
                    file_paths: Vec::new(),
                })
                .file_paths
                .push(virtual_path);
        }
        severities.insert(file.virtual_path.to_path_buf(), file_severities);
        if let Some(disable_directives) = file.disable_directives {
            directives.insert(file.virtual_path.to_path_buf(), disable_directives);
        }
    }
    Ok(PreparedTypeBatch {
        payload: ProtocolPayload {
            version: 2,
            configs: groups.into_values().collect(),
            source_overrides,
            report_syntactic: engine.type_check_enabled(),
            report_semantic: engine.type_check_enabled(),
        },
        severities,
        directives,
    })
}

fn resolved_protocol_rules(
    engine: &LintEngine,
    authored_path: &Path,
) -> Result<(Vec<ProtocolRule>, FxHashMap<String, AllowWarnDeny>), String> {
    let resolved = engine.config_store.resolve(authored_path);
    let mut rules = Vec::new();
    let mut severities = FxHashMap::default();
    for (rule, severity) in resolved.rules.iter() {
        if !severity.is_warn_deny() || !rule.is_tsgolint_rule() {
            continue;
        }
        let options = match rule.to_configuration() {
            Some(Ok(options)) => Some(options),
            Some(Err(error)) => {
                return Err(format!(
                    "unable to serialize type-aware rule {}: {error}",
                    rule.name()
                ));
            }
            None => None,
        };
        rules.push(ProtocolRule {
            name: rule.name().to_string(),
            options,
        });
        severities.insert(rule.name().to_string(), *severity);
    }
    rules.sort_by(|left, right| {
        left.name.cmp(&right.name).then_with(|| {
            serde_json::to_string(&left.options)
                .unwrap_or_default()
                .cmp(&serde_json::to_string(&right.options).unwrap_or_default())
        })
    });
    Ok((rules, severities))
}

fn run_type_protocol(
    executable: &Path,
    collect_fixes: bool,
    prepared: &PreparedTypeBatch<'_>,
) -> Result<Vec<TypeBatchDiagnostic>, String> {
    let mut command = Command::new(executable);
    command
        .arg("headless")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if collect_fixes {
        // Suggestions remain visible but are marked non-safe by `protocol_diagnostic`.
        command.args(["-fix", "-fix-suggestions"]);
    }
    let mut child = command.spawn().map_err(|error| {
        format!(
            "unable to start supported tsgolint at {}: {error}",
            executable.display()
        )
    })?;
    let encoded = serde_json::to_vec(&prepared.payload)
        .map_err(|error| format!("unable to encode tsgolint protocol v2 payload: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "tsgolint did not expose stdin".to_string())?;
    stdin
        .write_all(&encoded)
        .map_err(|error| format!("unable to transfer in-memory TSRX sources: {error}"))?;
    drop(stdin);

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "tsgolint did not expose stdout".to_string())?;
    let mut diagnostics = Vec::new();
    let mut protocol_error = None;
    while let Some(frame) = read_protocol_frame(&mut stdout)? {
        match frame.kind {
            0 => protocol_error = Some(parse_protocol_error(&frame.payload)?),
            1 => {
                let message: ProtocolDiagnostic = serde_json::from_slice(&frame.payload)
                    .map_err(|error| format!("invalid tsgolint diagnostic frame: {error}"))?;
                if let Some(diagnostic) =
                    protocol_diagnostic(message, &prepared.severities, &prepared.directives)
                {
                    diagnostics.push(diagnostic);
                }
            }
            2 => {}
            kind => return Err(format!("unsupported tsgolint protocol frame type {kind}")),
        }
    }
    let status = child
        .wait()
        .map_err(|error| format!("unable to wait for tsgolint: {error}"))?;
    if let Some(error) = protocol_error {
        return Err(format!("tsgolint protocol error: {error}"));
    }
    if !status.success() {
        return Err(format!("tsgolint exited with {status}"));
    }
    Ok(diagnostics)
}

fn parse_protocol_error(payload: &[u8]) -> Result<String, String> {
    serde_json::from_slice::<ProtocolError>(payload)
        .map(|error| error.error)
        .map_err(|error| format!("invalid tsgolint error frame: {error}"))
}

#[derive(Debug, Clone, Serialize)]
struct ProtocolRule {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct ProtocolConfigGroup {
    file_paths: Vec<String>,
    rules: Vec<ProtocolRule>,
}

#[derive(Debug, Serialize)]
struct ProtocolPayload {
    version: u8,
    configs: Vec<ProtocolConfigGroup>,
    source_overrides: FxHashMap<String, String>,
    report_syntactic: bool,
    report_semantic: bool,
}

#[derive(Debug, Deserialize)]
struct ProtocolError {
    error: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ProtocolRange {
    pos: u32,
    end: u32,
}

#[derive(Debug, Deserialize)]
struct ProtocolRuleMessage {
    id: String,
    description: String,
    #[serde(rename = "help")]
    _help: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProtocolFix {
    text: String,
    range: ProtocolRange,
}

#[derive(Debug, Deserialize)]
struct ProtocolSuggestion {
    fixes: Vec<ProtocolFix>,
}

#[derive(Debug, Deserialize)]
struct ProtocolLabeledRange {
    label: String,
    range: ProtocolRange,
}

#[derive(Debug, Deserialize)]
struct ProtocolDiagnostic {
    kind: u8,
    range: Option<ProtocolRange>,
    message: ProtocolRuleMessage,
    file_path: Option<String>,
    rule: Option<String>,
    #[serde(default)]
    fixes: Vec<ProtocolFix>,
    #[serde(default)]
    suggestions: Vec<ProtocolSuggestion>,
    #[serde(default)]
    labeled_ranges: Vec<ProtocolLabeledRange>,
}

struct ProtocolFrame {
    kind: u8,
    payload: Vec<u8>,
}

fn read_protocol_frame(reader: &mut impl Read) -> Result<Option<ProtocolFrame>, String> {
    let mut first = [0_u8; 1];
    let read = reader
        .read(&mut first)
        .map_err(|error| format!("unable to read tsgolint protocol frame: {error}"))?;
    if read == 0 {
        return Ok(None);
    }
    let mut size_bytes = [0_u8; 4];
    size_bytes[0] = first[0];
    reader
        .read_exact(&mut size_bytes[1..])
        .map_err(|error| format!("truncated tsgolint frame size: {error}"))?;
    let size = usize::try_from(u32::from_le_bytes(size_bytes))
        .map_err(|_| "tsgolint frame exceeds addressable memory".to_string())?;
    let mut kind = [0_u8; 1];
    reader
        .read_exact(&mut kind)
        .map_err(|error| format!("truncated tsgolint frame kind: {error}"))?;
    let mut payload = vec![0_u8; size];
    reader
        .read_exact(&mut payload)
        .map_err(|error| format!("truncated tsgolint frame payload: {error}"))?;
    Ok(Some(ProtocolFrame {
        kind: kind[0],
        payload,
    }))
}

fn protocol_diagnostic(
    message: ProtocolDiagnostic,
    severities: &FxHashMap<PathBuf, FxHashMap<String, AllowWarnDeny>>,
    directives: &FxHashMap<PathBuf, &DisableDirectives>,
) -> Option<TypeBatchDiagnostic> {
    let virtual_path = message.file_path.map(PathBuf::from);
    if message.kind == 0 {
        let rule = message.rule?;
        let severity = virtual_path
            .as_ref()
            .and_then(|path| severities.get(path))
            .and_then(|rules| rules.get(&rule))?;
        if let (Some(path), Some(range)) = (virtual_path.as_ref(), message.range.as_ref())
            && directives.get(path).is_some_and(|directives| {
                directives.contains(&rule, Span::new(range.pos, range.end))
            })
        {
            return None;
        }
        let mut labels = message
            .labeled_ranges
            .into_iter()
            .map(|label| EngineSpan {
                offset: label.range.pos,
                length: label.range.end.saturating_sub(label.range.pos),
                message: Some(label.label),
            })
            .collect::<Vec<_>>();
        if labels.is_empty() {
            if let Some(range) = message.range {
                labels.push(EngineSpan {
                    offset: range.pos,
                    length: range.end.saturating_sub(range.pos),
                    message: None,
                });
            }
        } else if let Some(range) = message.range
            && range.end > range.pos
        {
            labels.push(EngineSpan {
                offset: range.pos,
                length: range.end - range.pos,
                message: None,
            });
        }
        let mut fixes = message
            .fixes
            .into_iter()
            .map(|fix| EngineFix {
                offset: fix.range.pos,
                length: fix.range.end.saturating_sub(fix.range.pos),
                replacement: fix.text,
                safe: true,
            })
            .collect::<Vec<_>>();
        fixes.extend(
            message
                .suggestions
                .into_iter()
                .flat_map(|suggestion| suggestion.fixes)
                .map(|fix| EngineFix {
                    offset: fix.range.pos,
                    length: fix.range.end.saturating_sub(fix.range.pos),
                    replacement: fix.text,
                    safe: false,
                }),
        );
        Some(TypeBatchDiagnostic {
            virtual_path,
            diagnostic: EngineDiagnostic {
                rule: Some(rule.clone()),
                plugin: Some("typescript".to_string()),
                code: format!("typescript({rule})"),
                severity: if *severity == AllowWarnDeny::Deny {
                    "error".to_string()
                } else {
                    "warning".to_string()
                },
                message: message.message.description,
                labels,
                fixes,
            },
        })
    } else {
        let labels = message.range.map_or_else(Vec::new, |range| {
            vec![EngineSpan {
                offset: range.pos,
                length: range.end.saturating_sub(range.pos),
                message: None,
            }]
        });
        Some(TypeBatchDiagnostic {
            virtual_path,
            diagnostic: EngineDiagnostic {
                rule: None,
                plugin: Some("typescript".to_string()),
                code: format!("typescript({})", message.message.id),
                severity: "error".to_string(),
                message: message.message.description,
                labels,
                fixes: Vec::new(),
            },
        })
    }
}

fn find_tsgolint_executable(cwd: &Path) -> Result<PathBuf, String> {
    #[cfg(windows)]
    const FILES: &[&str] = &["tsgolint.CMD", "tsgolint.exe"];
    #[cfg(not(windows))]
    const FILES: &[&str] = &["tsgolint"];

    if let Ok(configured) = std::env::var("OXLINT_TSGOLINT_PATH") {
        let path = PathBuf::from(&configured);
        if path.is_file() {
            return Ok(path);
        }
        if path.is_dir()
            && let Some(candidate) = FILES
                .iter()
                .map(|name| path.join(name))
                .find(|candidate| candidate.is_file())
        {
            return Ok(candidate);
        }
        return Err(format!(
            "OXLINT_TSGOLINT_PATH does not identify a tsgolint executable: {configured}"
        ));
    }
    let mut directory = cwd.to_path_buf();
    loop {
        let node_modules = directory.join("node_modules");
        if let Some(package) = tsgolint_platform_package() {
            let native =
                node_modules
                    .join("@oxlint-tsgolint")
                    .join(package)
                    .join(if cfg!(windows) {
                        "tsgolint.exe"
                    } else {
                        "tsgolint"
                    });
            if native.is_file() {
                return Ok(native);
            }
        }
        if let Some(candidate) = FILES
            .iter()
            .map(|name| node_modules.join(".bin").join(name))
            .find(|candidate| candidate.is_file())
        {
            return Ok(candidate);
        }
        if !directory.pop() {
            break;
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            if let Some(candidate) = FILES
                .iter()
                .map(|name| directory.join(name))
                .find(|candidate| candidate.is_file())
            {
                return Ok(candidate);
            }
        }
    }
    Err(
        "type-aware linting requires oxlint-tsgolint 0.24.0; install it in this project or set OXLINT_TSGOLINT_PATH"
            .to_string(),
    )
}

fn tsgolint_platform_package() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("darwin-arm64"),
        ("macos", "x86_64") => Some("darwin-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("linux", "x86_64") => Some("linux-x64"),
        ("windows", "aarch64") => Some("win32-arm64"),
        ("windows", "x86_64") => Some("win32-x64"),
        _ => None,
    }
}

fn verify_tsgolint_version(executable: &Path) -> Result<(), String> {
    let canonical = executable
        .canonicalize()
        .unwrap_or_else(|_| executable.to_path_buf());
    for directory in canonical.ancestors().skip(1).take(6) {
        let manifest_path = directory.join("package.json");
        let Ok(source) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&source) else {
            continue;
        };
        let name = manifest.get("name").and_then(serde_json::Value::as_str);
        if !name
            .is_some_and(|name| name == "oxlint-tsgolint" || name.starts_with("@oxlint-tsgolint/"))
        {
            continue;
        }
        let version = manifest
            .get("version")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                format!(
                    "tsgolint package metadata at {} has no version",
                    manifest_path.display()
                )
            })?;
        if version == SUPPORTED_TSGOLINT_VERSION {
            return Ok(());
        }
        return Err(format!(
            "unsupported tsgolint version {version}; OXC for TSRX requires oxlint-tsgolint {SUPPORTED_TSGOLINT_VERSION} for protocol v2"
        ));
    }
    if std::env::var("OXC_TSRX_TSGOLINT_VERSION")
        .is_ok_and(|version| version == SUPPORTED_TSGOLINT_VERSION)
    {
        return Ok(());
    }
    Err(format!(
        "unable to verify tsgolint version for {}; use the oxlint-tsgolint {SUPPORTED_TSGOLINT_VERSION} npm package or set OXC_TSRX_TSGOLINT_VERSION={SUPPORTED_TSGOLINT_VERSION} for a verified standalone binary",
        executable.display()
    ))
}

const OXLINT_CONFIG_FILE_NAMES: ConfigFileNames = ConfigFileNames {
    json: ".oxlintrc.json",
    jsonc: ".oxlintrc.jsonc",
    js: &["oxlint.config.ts", "oxlint.config.mts"],
    vite: "vite.config.ts",
};

fn load_oxlintrc(
    cwd: &Path,
    explicit_path: Option<&Path>,
    config_base: Option<&Path>,
) -> Result<(Oxlintrc, Option<PathBuf>), String> {
    if config_base.is_some() && explicit_path.is_none() {
        return Err("a config base requires an explicit materialized Oxlint config".to_string());
    }
    let path = if let Some(path) = explicit_path {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        if is_js_config_path(&path) {
            return Err(
                "JavaScript/TypeScript Oxlint config modules require the future thin npm host; use JSON or JSONC for the native CLI"
                    .to_string(),
            );
        }
        Some(path)
    } else {
        discover_oxlintrc(cwd)?
    };

    let Some(path) = path else {
        return Ok((Oxlintrc::default(), None));
    };
    let path = path.canonicalize().unwrap_or(path);
    let mut config = if config_base.is_some() {
        load_materialized_oxlintrc(&path)?
    } else {
        Oxlintrc::from_file(&path).map_err(|error| error.to_string())?
    };
    if let Some(base) = config_base {
        let base = resolve_existing_config_base(cwd, base, "Oxlint")?;
        // ConfigStoreBuilder and LintIgnoreMatcher intentionally derive relative extends,
        // overrides, and ignorePatterns from Oxlintrc::path. The materialized JSON remains the
        // file we loaded, while this synthetic path restores the authored Vite config directory.
        config.path = base.join(".oxc-tsrx-vite-plus.oxlintrc.json");
        config.set_config_dir(&base);
    }
    Ok((config, Some(path)))
}

fn load_materialized_oxlintrc(path: &Path) -> Result<Oxlintrc, String> {
    let source = std::fs::read_to_string(path).map_err(|error| {
        format!(
            "unable to read materialized Oxlint config {}: {error}",
            path.display()
        )
    })?;
    let value: serde_json::Value = serde_json::from_str(&source).map_err(|error| {
        format!(
            "invalid materialized Oxlint config {}: {error}",
            path.display()
        )
    })?;
    oxlintrc_from_materialized_value(value, "<root>")
}

fn oxlintrc_from_materialized_value(
    mut value: serde_json::Value,
    context: &str,
) -> Result<Oxlintrc, String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| format!("materialized Oxlint config {context} must be an object"))?;
    let mut path_extends = Vec::new();
    let mut object_extends = Vec::new();
    if let Some(extends) = object.remove("extends") {
        let extends = extends.as_array().ok_or_else(|| {
            format!("materialized Oxlint config {context}.extends must be an array")
        })?;
        for (index, item) in extends.iter().enumerate() {
            match item {
                serde_json::Value::String(_) => path_extends.push(item.clone()),
                serde_json::Value::Object(_) => {
                    object_extends.push(oxlintrc_from_materialized_value(
                        item.clone(),
                        &format!("{context}.extends[{index}]"),
                    )?);
                }
                _ => {
                    return Err(format!(
                        "materialized Oxlint config {context}.extends[{index}] must be a path string or config object"
                    ));
                }
            }
        }
    }
    if !path_extends.is_empty() {
        object.insert(
            "extends".to_string(),
            serde_json::Value::Array(path_extends),
        );
    }
    let source = serde_json::to_string(&value).map_err(|error| {
        format!("unable to serialize materialized Oxlint config {context}: {error}")
    })?;
    let mut config = Oxlintrc::from_string(&source).map_err(|error| error.to_string())?;
    config.extends_configs = object_extends;
    Ok(config)
}

fn resolve_existing_config_base(cwd: &Path, base: &Path, tool: &str) -> Result<PathBuf, String> {
    let base = if base.is_absolute() {
        base.to_path_buf()
    } else {
        cwd.join(base)
    };
    let base = base.canonicalize().map_err(|error| {
        format!(
            "unable to resolve {tool} config base {}: {error}",
            base.display()
        )
    })?;
    if !base.is_dir() {
        return Err(format!(
            "{tool} config base is not a directory: {}",
            base.display()
        ));
    }
    Ok(base)
}

fn discover_oxlintrc(cwd: &Path) -> Result<Option<PathBuf>, String> {
    let discovery = ConfigDiscovery::new(OXLINT_CONFIG_FILE_NAMES, false);
    let mut directory = cwd.to_path_buf();
    loop {
        let discovered = discovery
            .find_unique_config_by_readdir(&directory, true)
            .map_err(|error| {
                format!(
                    "conflicting Oxlint configuration files in {}: {error:?}",
                    directory.display()
                )
            })?;
        if let Some(discovered) = discovered {
            return match discovered {
                DiscoveredConfigFile::Json(path) | DiscoveredConfigFile::Jsonc(path) => {
                    Ok(Some(path))
                }
                DiscoveredConfigFile::Js(_) | DiscoveredConfigFile::Vite(_) => Err(
                    "JavaScript/TypeScript Oxlint config modules require the future thin npm host; use .oxlintrc.json or .oxlintrc.jsonc for the native CLI"
                        .to_string(),
                ),
            };
        }
        if !directory.pop() {
            return Ok(None);
        }
    }
}

fn reject_unavailable_lint_capabilities(
    config: &Oxlintrc,
    type_aware_opt_in: bool,
) -> Result<(), String> {
    if config
        .external_plugins
        .as_ref()
        .is_some_and(|plugins| !plugins.is_empty())
    {
        return Err(
            "JavaScript plugins are not supported by the native TSRX path yet: OXC's public package does not expose its zero-copy plugin host, and OXC for TSRX will not silently add a second parse"
                .to_string(),
        );
    }
    if !type_aware_opt_in
        && (config.options.type_aware == Some(true) || config.options.type_check == Some(true))
    {
        return Err(
            "type-aware tsgolint/type-check mode requires the explicit --type-aware or --type-check opt-in; it is never started or silently disabled by config alone"
                .to_string(),
        );
    }
    Ok(())
}

fn config_builder_error(error: ConfigBuilderError) -> String {
    let message = error.to_string();
    drop(error);
    if message.to_ascii_lowercase().contains("plugin") {
        format!(
            "JavaScript plugins are unavailable on the native TSRX path without OXC's public zero-copy host: {message}"
        )
    } else {
        format!("invalid Oxlint configuration: {message}")
    }
}

fn map_message(message: &Message) -> EngineDiagnostic {
    let rule = message.rule.as_ref().map(|rule| rule.rule_name.to_string());
    let plugin = message
        .rule
        .as_ref()
        .map(|rule| rule.plugin_name.to_string());
    let labels = message
        .error
        .labels
        .iter()
        .map(|label| EngineSpan {
            offset: label.offset(),
            length: label.len(),
            message: label.label().map(ToString::to_string),
        })
        .collect();
    EngineDiagnostic {
        rule,
        plugin,
        code: message.error.code.to_string(),
        severity: format!("{:?}", message.error.severity).to_ascii_lowercase(),
        message: message.error.message.to_string(),
        labels,
        fixes: fixes(&message.fixes),
    }
}

fn fixes(possible: &PossibleFixes) -> Vec<EngineFix> {
    let list = match possible {
        PossibleFixes::None => return Vec::new(),
        PossibleFixes::Single(fix) => std::slice::from_ref(fix),
        PossibleFixes::Multiple(fixes) => fixes.as_slice(),
    };
    list.iter()
        .map(|fix| EngineFix {
            offset: fix.span.start,
            length: fix.span.size(),
            replacement: fix.content.to_string(),
            safe: FixKind::SafeFix.can_apply(fix.kind),
        })
        .collect()
}

fn validate_dynamic_tags(
    program: &oxc_ast::ast::Program<'_>,
    contract: Option<DynamicTagContract<'_>>,
) -> Result<(), String> {
    let Some(contract) = contract else {
        return Ok(());
    };
    let count = usize::try_from(contract.count)
        .map_err(|_| "TSRX dynamic-tag count exceeds addressable memory".to_string())?;
    if count == 0 || contract.prefix.is_empty() || contract.original_offsets.len() != count {
        return Err("invalid empty TSRX dynamic-tag scaffold contract".to_string());
    }
    let mut validator = DynamicTagValidator {
        prefix: contract.prefix,
        original_offsets: contract.original_offsets,
        seen: vec![false; count],
        error: None,
    };
    validator.visit_program(program);
    if let Some(error) = validator.error {
        return Err(error);
    }
    if let Some(index) = validator.seen.iter().position(|seen| !seen) {
        return Err(format!("OXC parse lost TSRX dynamic-tag scaffold {index}"));
    }
    Ok(())
}

struct DynamicTagValidator<'c> {
    prefix: &'c str,
    original_offsets: &'c [u32],
    seen: Vec<bool>,
    error: Option<String>,
}

impl<'a> Visit<'a> for DynamicTagValidator<'_> {
    fn visit_jsx_opening_element(&mut self, element: &JSXOpeningElement<'a>) {
        if self.error.is_some() {
            return;
        }
        let Some(name) = element.name.get_identifier_name() else {
            walk::walk_jsx_opening_element(self, element);
            return;
        };
        let Some(index) = scaffold_ordinal(name.as_str(), self.prefix, 'D', false) else {
            if name.as_str().starts_with(self.prefix) {
                self.error = Some("malformed TSRX dynamic-tag scaffold".to_string());
                return;
            }
            walk::walk_jsx_opening_element(self, element);
            return;
        };
        let index = index as usize;
        if index >= self.seen.len() || self.seen[index] {
            self.error = Some(format!("invalid TSRX dynamic-tag scaffold {index}"));
            return;
        }

        let mut expression = None;
        let mut end_sentinel = false;
        for item in &element.attributes {
            let Some(attribute) = item.as_attribute() else {
                continue;
            };
            let Some(identifier) = attribute.name.as_identifier() else {
                continue;
            };
            let name = identifier.name.as_str();
            if let Some(attribute_index) = scaffold_ordinal(name, self.prefix, 'Z', true) {
                let valid_value = attribute.value.as_ref().is_some_and(|value| {
                    matches!(
                        value,
                        JSXAttributeValue::ExpressionContainer(container)
                            if matches!(container.expression.as_expression(), Some(Expression::NullLiteral(_)))
                    )
                });
                if attribute_index as usize != index || end_sentinel || !valid_value {
                    self.error = Some(format!("mismatched TSRX dynamic-tag end scaffold {index}"));
                    return;
                }
                end_sentinel = true;
                continue;
            }
            let Some(attribute_index) = scaffold_ordinal(name, self.prefix, 'A', true) else {
                if name.starts_with(self.prefix) {
                    self.error = Some(format!("malformed TSRX dynamic-tag attribute {index}"));
                    return;
                }
                continue;
            };
            if attribute_index as usize != index || expression.is_some() {
                self.error = Some(format!("mismatched TSRX dynamic-tag attribute {index}"));
                return;
            }
            expression = attribute.value.as_ref().and_then(|value| match value {
                JSXAttributeValue::ExpressionContainer(container) => {
                    container.expression.as_expression()
                }
                _ => None,
            });
        }
        let Some(expression) = expression else {
            self.error = Some(format!("missing TSRX dynamic-tag expression {index}"));
            return;
        };
        if !end_sentinel {
            self.error = Some(format!("missing TSRX dynamic-tag end scaffold {index}"));
            return;
        }
        if !is_valid_dynamic_expression(expression) {
            let original_offset = self.original_offsets[index];
            self.error = Some(format!(
                "TSRX dynamic tag {index} at source byte {original_offset} (projected byte {}) must be an identifier, member, static string, or runtime expression without calls, construction, spreads, concatenation, interpolation, objects, or arrays",
                expression.span().start,
            ));
            return;
        }
        self.seen[index] = true;
        walk::walk_jsx_opening_element(self, element);
    }
}

fn scaffold_ordinal(name: &str, prefix: &str, kind: char, suffix: bool) -> Option<u32> {
    let rest = name.strip_prefix(prefix)?.strip_prefix(kind)?;
    let digits = if suffix {
        rest.strip_suffix('_')?
    } else {
        rest
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn is_valid_dynamic_expression(expression: &Expression<'_>) -> bool {
    if !is_valid_dynamic_root(expression) {
        return false;
    }
    let mut validator = DisallowedDynamicSyntax::default();
    validator.visit_expression(expression);
    !validator.found
}

fn is_valid_dynamic_root(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::ParenthesizedExpression(wrapper) => is_valid_dynamic_root(&wrapper.expression),
        Expression::TSAsExpression(wrapper) => is_valid_dynamic_root(&wrapper.expression),
        Expression::TSTypeAssertion(wrapper) => is_valid_dynamic_root(&wrapper.expression),
        Expression::TSNonNullExpression(wrapper) => is_valid_dynamic_root(&wrapper.expression),
        Expression::ChainExpression(wrapper) => match &wrapper.expression {
            ChainElement::CallExpression(_) => false,
            ChainElement::TSNonNullExpression(inner) => is_valid_dynamic_root(&inner.expression),
            _ => true,
        },
        Expression::Identifier(identifier) => identifier.name != "undefined",
        Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::JSXElement(_)
        | Expression::JSXFragment(_) => false,
        Expression::UnaryExpression(unary) => unary.operator != UnaryOperator::Void,
        _ => true,
    }
}

#[derive(Default)]
struct DisallowedDynamicSyntax {
    found: bool,
}

impl<'a> Visit<'a> for DisallowedDynamicSyntax {
    fn visit_array_expression(&mut self, _expression: &ArrayExpression<'a>) {
        self.found = true;
    }

    fn visit_object_expression(&mut self, _expression: &ObjectExpression<'a>) {
        self.found = true;
    }

    fn visit_call_expression(&mut self, _expression: &CallExpression<'a>) {
        self.found = true;
    }

    fn visit_new_expression(&mut self, _expression: &NewExpression<'a>) {
        self.found = true;
    }

    fn visit_spread_element(&mut self, _spread: &SpreadElement<'a>) {
        self.found = true;
    }

    fn visit_tagged_template_expression(&mut self, _expression: &TaggedTemplateExpression<'a>) {
        self.found = true;
    }

    fn visit_template_literal(&mut self, template: &TemplateLiteral<'a>) {
        if template.expressions.is_empty() {
            walk::walk_template_literal(self, template);
        } else {
            self.found = true;
        }
    }

    fn visit_binary_expression(&mut self, expression: &BinaryExpression<'a>) {
        if expression.operator == BinaryOperator::Addition {
            self.found = true;
        } else {
            walk::walk_binary_expression(self, expression);
        }
    }
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{DynamicTagContract, FormatRequest, SourceKind, format};

    fn format_dynamic(expression: &str) -> Result<String, String> {
        let source = format!("const value = <_t0_D0 _t0_A0_={{{expression}}} _t0_Z0_={{null}} />;");
        let original_offsets = [0];
        format(&FormatRequest {
            parse_source: &source,
            source_kind: SourceKind::TypeScriptReact,
            dynamic_tags: Some(DynamicTagContract {
                prefix: "_t0_",
                count: 1,
                original_offsets: &original_offsets,
            }),
            options: None,
        })
        .map(|result| result.code)
    }

    #[test]
    fn dynamic_tag_validator_matches_authoritative_allowed_ast_shapes() {
        for expression in [
            "tag",
            "obj.new",
            "obj?.[key]",
            "(obj)[key]",
            "obj![key]",
            "-1",
            "() => Tag",
            "x = Tag",
            "x += Tag",
            "x++",
            "++x",
            "`d\\${kind}`",
        ] {
            assert!(format_dynamic(expression).is_ok(), "{expression}");
        }
    }

    #[test]
    fn dynamic_tag_validator_rejects_authoritative_disallowed_ast_shapes() {
        for expression in [
            "/x/",
            "null as any",
            "undefined as any",
            "true as any",
            "tag()",
            "condition ? tagName() : Tag",
            "new TagName()",
            "({ tag }).tag",
            "[Tag][0]",
            "'hello' + 'bye'",
            "`d${kind}`",
            "tag`div`",
            "fn!()",
            "fn<string>()",
            "key in [Tag]",
        ] {
            let error = format_dynamic(expression).unwrap_err();
            assert!(error.contains("dynamic tag"), "{expression}: {error}");
            assert!(error.contains("source byte 0"), "{expression}: {error}");
        }
    }
}
