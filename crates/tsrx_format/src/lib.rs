//! Native TSRX formatting orchestration over canonical Oxfmt.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use oxc_adapter::{
    DynamicTagContract, FormatOptions as EngineFormatOptions, FormatRequest, SourceKind,
};
use serde::Deserialize;
use serde_json::Value;
use tsrx_syntax::{lift_formatted, project_for_format, scan};

pub use oxc_adapter::OXC_REVISION;

/// Deliberate embedded-CSS boundary for this release. `<style>` payloads never leave the
/// current process and stay byte-exact until canonical OXC CSS can share the pinned allocator
/// graph without a downstream Cargo patch.
pub const EMBEDDED_CSS_MODE: &str = "KEEP_RAW";
pub const EMBEDDED_CSS_PARSE_COUNT: u32 = 0;
pub const EMBEDDED_CSS_FORMAT_NS: u64 = 0;
pub const EMBEDDED_CSS_USES_SUBPROCESS: bool = false;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatMode {
    Direct,
    Projected,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FormatTimings {
    pub scan_ns: u64,
    pub projection_ns: u64,
    pub parse_ns: u64,
    pub format_ns: u64,
    /// Canonical embedded-language formatting time; zero while raw CSS is preserved verbatim.
    pub embedded_format_ns: u64,
    pub lift_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatMetadata {
    pub native: bool,
    pub engine: &'static str,
    pub oxc_revision: &'static str,
    pub mode: FormatMode,
    pub parse_count: u32,
    /// Zero while style payloads use the checked raw-preservation path.
    pub embedded_parse_count: u32,
    pub is_tsrx: bool,
    pub projection_bytes: usize,
    pub marker_count: usize,
    pub style_count: usize,
    pub timings: FormatTimings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatOutput {
    pub code: String,
    pub changed: bool,
    pub metadata: FormatMetadata,
}

/// One discovered/compiled Oxfmt-compatible JSON configuration reused across a batch or editor.
pub struct FormatSession {
    cwd: PathBuf,
    config_root: PathBuf,
    config_path: Option<PathBuf>,
    config_load_ns: u64,
    base: FileFormatOptions,
    overrides: Vec<FormatOverride>,
    ignore: Option<Gitignore>,
}

#[derive(Debug, Clone, Default)]
struct FileFormatOptions {
    engine: EngineFormatOptions,
    insert_final_newline: Option<bool>,
}

struct FormatOverride {
    files: GlobSet,
    exclude_files: GlobSet,
    options: FileFormatOptions,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RawOxfmtrc {
    #[serde(flatten)]
    options: RawFormatOptions,
    overrides: Vec<RawFormatOverride>,
    ignore_patterns: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RawFormatOverride {
    files: Vec<String>,
    exclude_files: Vec<String>,
    options: RawFormatOptions,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RawFormatOptions {
    #[serde(rename = "$schema")]
    schema: Option<Value>,
    use_tabs: Option<bool>,
    tab_width: Option<u8>,
    end_of_line: Option<String>,
    print_width: Option<u16>,
    single_quote: Option<bool>,
    jsx_single_quote: Option<bool>,
    quote_props: Option<String>,
    trailing_comma: Option<String>,
    semi: Option<bool>,
    arrow_parens: Option<String>,
    bracket_spacing: Option<bool>,
    bracket_same_line: Option<bool>,
    object_wrap: Option<String>,
    single_attribute_per_line: Option<bool>,
    embedded_language_formatting: Option<String>,
    html_whitespace_sensitivity: Option<String>,
    insert_final_newline: Option<bool>,
    sort_imports: Option<Value>,
    sort_tailwindcss: Option<Value>,
    jsdoc: Option<Value>,
    experimental_operator_position: Option<Value>,
    experimental_ternaries: Option<Value>,
    prose_wrap: Option<Value>,
    sort_package_json: Option<Value>,
    svelte: Option<Value>,
    vue_indent_script_and_style: Option<Value>,
    #[serde(flatten)]
    unknown: BTreeMap<String, Value>,
}

impl FormatSession {
    /// Discover or explicitly load one JSON/JSONC Oxfmt configuration.
    ///
    /// # Errors
    ///
    /// Returns an error for conflicts, invalid config, JS/TS config modules, unsupported
    /// TSRX-affecting options, or malformed glob/ignore patterns.
    pub fn new(cwd: &Path, explicit_config: Option<&Path>) -> Result<Self, String> {
        Self::new_with_config_base(cwd, explicit_config, None)
    }

    /// Load a materialized config while resolving relative paths from its authored directory.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid base or any invalid/unsupported formatter configuration.
    pub fn new_with_config_base(
        cwd: &Path,
        explicit_config: Option<&Path>,
        config_base: Option<&Path>,
    ) -> Result<Self, String> {
        let started = Instant::now();
        let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
        if config_base.is_some() && explicit_config.is_none() {
            return Err("a config base requires an explicit materialized Oxfmt config".to_string());
        }
        reject_editorconfig(&cwd, explicit_config)?;
        let config_path = resolve_oxfmt_config(&cwd, explicit_config)?;
        let (base, overrides, ignore_patterns, config_root) = if let Some(path) = &config_path {
            let raw = read_oxfmt_config(path)?;
            let base = raw.options.resolve("root Oxfmt config")?;
            let overrides = raw
                .overrides
                .into_iter()
                .enumerate()
                .map(|(index, raw)| FormatOverride::new(raw, index))
                .collect::<Result<Vec<_>, _>>()?;
            let root = if let Some(config_base) = config_base {
                resolve_existing_config_base(&cwd, config_base)?
            } else {
                path.parent().unwrap_or(&cwd).to_path_buf()
            };
            (base, overrides, raw.ignore_patterns, root)
        } else {
            (
                FileFormatOptions::default(),
                Vec::new(),
                Vec::new(),
                cwd.clone(),
            )
        };
        let ignore = build_ignore(&config_root, &ignore_patterns)?;
        Ok(Self {
            cwd,
            config_root,
            config_path,
            config_load_ns: elapsed_ns(started),
            base,
            overrides,
            ignore,
        })
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
    pub fn should_ignore(&self, path: &Path) -> bool {
        let path = self.absolute_path(path);
        let path = path.canonicalize().unwrap_or(path);
        if !path.starts_with(&self.config_root) {
            return false;
        }
        self.ignore
            .as_ref()
            .is_some_and(|ignore| ignore.matched_path_or_any_parents(path, false).is_ignore())
    }

    /// Format one loaded source with the options resolved for its authored path.
    ///
    /// # Errors
    ///
    /// Returns no partial output for configuration, projection, parser, formatter, or lift errors.
    pub fn format_text(&self, path: &Path, source: &str) -> Result<FormatOutput, String> {
        let options = self.options_for(path);
        format_text_with_options(path, source, Some(&options))
    }

    fn absolute_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.cwd.join(path)
        }
    }

    fn options_for(&self, path: &Path) -> FileFormatOptions {
        let absolute = self.absolute_path(path);
        let relative = absolute
            .strip_prefix(&self.config_root)
            .unwrap_or(&absolute);
        let candidate = relative.to_string_lossy();
        let mut options = self.base.clone();
        for r#override in &self.overrides {
            if r#override.files.is_match(candidate.as_ref())
                && !r#override.exclude_files.is_match(candidate.as_ref())
            {
                options.merge(&r#override.options);
            }
        }
        options
    }
}

fn resolve_existing_config_base(cwd: &Path, base: &Path) -> Result<PathBuf, String> {
    let base = if base.is_absolute() {
        base.to_path_buf()
    } else {
        cwd.join(base)
    };
    let base = base.canonicalize().map_err(|error| {
        format!(
            "unable to resolve Oxfmt config base {}: {error}",
            base.display()
        )
    })?;
    if !base.is_dir() {
        return Err(format!(
            "Oxfmt config base is not a directory: {}",
            base.display()
        ));
    }
    Ok(base)
}

impl FileFormatOptions {
    fn merge(&mut self, other: &Self) {
        macro_rules! merge {
            ($($field:ident),+ $(,)?) => {
                $(if other.engine.$field.is_some() {
                    self.engine.$field.clone_from(&other.engine.$field);
                })+
            };
        }
        merge!(
            use_tabs,
            tab_width,
            end_of_line,
            print_width,
            single_quote,
            jsx_single_quote,
            quote_props,
            trailing_comma,
            semi,
            arrow_parens,
            bracket_spacing,
            bracket_same_line,
            object_wrap,
            single_attribute_per_line,
            embedded_language_formatting,
            html_whitespace_sensitivity,
        );
        if other.insert_final_newline.is_some() {
            self.insert_final_newline = other.insert_final_newline;
        }
    }
}

impl FormatOverride {
    fn new(raw: RawFormatOverride, index: usize) -> Result<Self, String> {
        if raw.files.is_empty() {
            return Err(format!(
                "Oxfmt override {index} requires at least one files pattern"
            ));
        }
        Ok(Self {
            files: build_globs(&raw.files, &format!("Oxfmt override {index} files"))?,
            exclude_files: build_globs(
                &raw.exclude_files,
                &format!("Oxfmt override {index} excludeFiles"),
            )?,
            options: raw.options.resolve(&format!("Oxfmt override {index}"))?,
        })
    }
}

impl RawFormatOptions {
    fn resolve(self, context: &str) -> Result<FileFormatOptions, String> {
        let Self {
            schema,
            use_tabs,
            tab_width,
            end_of_line,
            print_width,
            single_quote,
            jsx_single_quote,
            quote_props,
            trailing_comma,
            semi,
            arrow_parens,
            bracket_spacing,
            bracket_same_line,
            object_wrap,
            single_attribute_per_line,
            embedded_language_formatting,
            html_whitespace_sensitivity,
            insert_final_newline,
            sort_imports,
            sort_tailwindcss,
            jsdoc,
            experimental_operator_position,
            experimental_ternaries,
            prose_wrap,
            sort_package_json,
            svelte,
            vue_indent_script_and_style,
            unknown,
        } = self;
        let _language_irrelevant = (
            schema,
            prose_wrap,
            sort_package_json,
            svelte,
            vue_indent_script_and_style,
        );
        if let Some((name, _)) = unknown.into_iter().next() {
            return Err(format!(
                "unsupported Oxfmt option `{name}` in {context}; OXC for TSRX never silently ignores unknown TSRX-affecting options"
            ));
        }
        reject_enabled_value(context, "sortImports", sort_imports)?;
        reject_enabled_value(context, "sortTailwindcss", sort_tailwindcss)?;
        reject_enabled_value(context, "jsdoc", jsdoc)?;
        if embedded_language_formatting.is_some() {
            return Err(format!(
                "Oxfmt `embeddedLanguageFormatting` is not available for TSRX in {context}: canonical embedded-language callbacks are outside the public pinned formatter boundary"
            ));
        }
        if experimental_operator_position.is_some() || experimental_ternaries.is_some() {
            return Err(format!(
                "experimental Oxfmt options are not supported by the pinned formatter in {context}"
            ));
        }
        Ok(FileFormatOptions {
            engine: EngineFormatOptions {
                use_tabs,
                tab_width,
                end_of_line,
                print_width,
                single_quote,
                jsx_single_quote,
                quote_props,
                trailing_comma,
                semi,
                arrow_parens,
                bracket_spacing,
                bracket_same_line,
                object_wrap,
                single_attribute_per_line,
                embedded_language_formatting: None,
                html_whitespace_sensitivity,
            },
            insert_final_newline,
        })
    }
}

fn reject_enabled_value(context: &str, name: &str, value: Option<Value>) -> Result<(), String> {
    if value.is_some_and(|value| !value.is_null() && value != Value::Bool(false)) {
        return Err(format!(
            "Oxfmt `{name}` is not available for TSRX in {context}: it needs a callback/config surface outside the public pinned formatter boundary"
        ));
    }
    Ok(())
}

fn build_globs(patterns: &[String], context: &str) -> Result<GlobSet, String> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(
            Glob::new(pattern)
                .map_err(|error| format!("invalid {context} pattern `{pattern}`: {error}"))?,
        );
    }
    builder
        .build()
        .map_err(|error| format!("unable to build {context}: {error}"))
}

fn build_ignore(root: &Path, patterns: &[String]) -> Result<Option<Gitignore>, String> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GitignoreBuilder::new(root);
    for pattern in patterns {
        builder
            .add_line(None, pattern)
            .map_err(|error| format!("invalid Oxfmt ignorePatterns entry `{pattern}`: {error}"))?;
    }
    builder
        .build()
        .map(Some)
        .map_err(|error| format!("unable to build Oxfmt ignorePatterns: {error}"))
}

fn resolve_oxfmt_config(cwd: &Path, explicit: Option<&Path>) -> Result<Option<PathBuf>, String> {
    if let Some(path) = explicit {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        if is_js_config(&path) {
            return Err(
                "JavaScript/TypeScript Oxfmt config modules require the future thin npm host; use JSON or JSONC for the native CLI"
                    .to_string(),
            );
        }
        return Ok(Some(path.canonicalize().unwrap_or(path)));
    }

    let mut directory = cwd.to_path_buf();
    loop {
        let entries = fs::read_dir(&directory)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                let name = path.file_name()?.to_str()?;
                matches!(
                    name,
                    ".oxfmtrc.json"
                        | ".oxfmtrc.jsonc"
                        | "oxfmt.config.js"
                        | "oxfmt.config.mjs"
                        | "oxfmt.config.cjs"
                        | "oxfmt.config.ts"
                        | "oxfmt.config.mts"
                        | "oxfmt.config.cts"
                )
                .then_some(path)
            })
            .collect::<Vec<_>>();
        match entries.as_slice() {
            [] => {}
            [path] if is_js_config(path) => {
                return Err(
                    "JavaScript/TypeScript Oxfmt config modules require the future thin npm host; use .oxfmtrc.json or .oxfmtrc.jsonc for the native CLI"
                        .to_string(),
                );
            }
            [path] => return Ok(Some(path.canonicalize().unwrap_or_else(|_| path.clone()))),
            _ => {
                let names = entries
                    .iter()
                    .filter_map(|path| path.file_name())
                    .map(|name| name.to_string_lossy())
                    .collect::<Vec<_>>();
                return Err(format!(
                    "multiple Oxfmt configuration files found in {}: {}",
                    directory.display(),
                    names.join(", ")
                ));
            }
        }
        if !directory.pop() {
            return Ok(None);
        }
    }
}

fn reject_editorconfig(cwd: &Path, explicit: Option<&Path>) -> Result<(), String> {
    if explicit.is_some_and(|path| path.file_name().is_some_and(|name| name == ".editorconfig")) {
        return Err(
            ".editorconfig is not supported by the native TSRX formatter yet; its settings are never silently ignored"
                .to_string(),
        );
    }
    let mut directory = cwd.to_path_buf();
    loop {
        let path = directory.join(".editorconfig");
        if path.is_file() {
            return Err(format!(
                ".editorconfig is not supported by the native TSRX formatter yet: {}; its settings are never silently ignored",
                path.display()
            ));
        }
        if !directory.pop() {
            return Ok(());
        }
    }
}

fn is_js_config(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("js" | "mjs" | "cjs" | "ts" | "mts" | "cts")
    )
}

fn read_oxfmt_config(path: &Path) -> Result<RawOxfmtrc, String> {
    let mut source = fs::read_to_string(path)
        .map_err(|error| format!("unable to read Oxfmt config {}: {error}", path.display()))?;
    json_strip_comments::strip(&mut source)
        .map_err(|error| format!("invalid JSONC in {}: {error}", path.display()))?;
    serde_json::from_str(&source)
        .map_err(|error| format!("invalid Oxfmt config {}: {error}", path.display()))
}

/// Formats source already owned by a caller without filesystem I/O.
///
/// Ordinary JavaScript/TypeScript source goes directly to canonical Oxfmt. `.tsrx` source is
/// scanned once, projected once with checked structural markers, parsed/formatted once by Oxfmt,
/// and lifted once. Style payloads stay byte-identical behind checked opaque markers until OXC's
/// CSS formatter is consumable without a downstream Cargo patch. A failure returns no partial
/// output and cannot mutate the caller's source.
///
/// # Errors
///
/// Returns an error for unsupported extensions, unsupported TSRX grammar, invalid projected TSX,
/// canonical Oxfmt failures, or any marker/source-fidelity violation.
pub fn format_text(path: &Path, source: &str) -> Result<FormatOutput, String> {
    format_text_with_options(path, source, None)
}

fn format_text_with_options(
    path: &Path,
    source: &str,
    options: Option<&FileFormatOptions>,
) -> Result<FormatOutput, String> {
    let is_tsrx = path
        .extension()
        .is_some_and(|extension| extension == "tsrx");
    if !is_tsrx {
        return format_direct(path, source, options);
    }

    let mut timings = FormatTimings::default();
    let started = Instant::now();
    let overlay = scan(source).map_err(|error| error.to_string())?;
    timings.scan_ns = elapsed_ns(started);

    let started = Instant::now();
    let projection = project_for_format(source, &overlay).map_err(|error| error.to_string())?;
    timings.projection_ns = elapsed_ns(started);

    let engine = oxc_adapter::format(&FormatRequest {
        parse_source: projection.source(),
        source_kind: SourceKind::TypeScriptReact,
        dynamic_tags: projection
            .dynamic_contract()
            .map(|(prefix, count, original_offsets)| DynamicTagContract {
                prefix,
                count,
                original_offsets,
            }),
        options: options.map(|options| &options.engine),
    })?;
    timings.parse_ns = engine.timings.parse_ns;
    timings.format_ns = engine.timings.format_ns;

    let style_count = projection.style_count();

    let started = Instant::now();
    let code =
        lift_formatted(&engine.code, source, &projection).map_err(|error| error.to_string())?;
    let code = apply_final_newline(code, options);
    timings.lift_ns = elapsed_ns(started);

    Ok(FormatOutput {
        changed: code != source,
        code,
        metadata: FormatMetadata {
            native: true,
            engine: "oxc_formatter",
            oxc_revision: OXC_REVISION,
            mode: FormatMode::Projected,
            parse_count: engine.parse_count,
            embedded_parse_count: EMBEDDED_CSS_PARSE_COUNT,
            is_tsrx: true,
            projection_bytes: projection.source().len(),
            marker_count: projection.marker_count(),
            style_count,
            timings,
        },
    })
}

fn format_direct(
    path: &Path,
    source: &str,
    options: Option<&FileFormatOptions>,
) -> Result<FormatOutput, String> {
    let engine = oxc_adapter::format(&FormatRequest {
        parse_source: source,
        source_kind: SourceKind::from_path(path)?,
        dynamic_tags: None,
        options: options.map(|options| &options.engine),
    })?;
    let code = apply_final_newline(engine.code, options);
    Ok(FormatOutput {
        changed: code != source,
        code,
        metadata: FormatMetadata {
            native: true,
            engine: "oxc_formatter",
            oxc_revision: OXC_REVISION,
            mode: FormatMode::Direct,
            parse_count: engine.parse_count,
            embedded_parse_count: EMBEDDED_CSS_PARSE_COUNT,
            is_tsrx: false,
            projection_bytes: 0,
            marker_count: 0,
            style_count: 0,
            timings: FormatTimings {
                parse_ns: engine.timings.parse_ns,
                format_ns: engine.timings.format_ns,
                ..FormatTimings::default()
            },
        },
    })
}

fn apply_final_newline(mut code: String, options: Option<&FileFormatOptions>) -> String {
    if options.and_then(|options| options.insert_final_newline) == Some(false) {
        while code.ends_with('\n') || code.ends_with('\r') {
            code.pop();
        }
    }
    code
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        EMBEDDED_CSS_FORMAT_NS, EMBEDDED_CSS_MODE, EMBEDDED_CSS_PARSE_COUNT,
        EMBEDDED_CSS_USES_SUBPROCESS, FormatMode, format_text,
    };

    #[test]
    fn embedded_css_boundary_is_keep_raw_without_hidden_work() {
        let payload = "/* spacing is authored */ .card{color:oklch(62% .2 25);  margin:0  1rem}";
        let source =
            format!("export function View() @{{<main><style>{payload}</style><p>Hi</p></main>}}\n");
        let output = format_text(Path::new("View.tsrx"), &source).unwrap();

        assert_eq!(EMBEDDED_CSS_MODE, "KEEP_RAW");
        assert_eq!(EMBEDDED_CSS_PARSE_COUNT, 0);
        assert_eq!(EMBEDDED_CSS_FORMAT_NS, 0);
        const { assert!(!EMBEDDED_CSS_USES_SUBPROCESS) };
        assert!(output.code.contains(payload));
        assert_eq!(output.metadata.style_count, 1);
        assert_eq!(
            output.metadata.embedded_parse_count,
            EMBEDDED_CSS_PARSE_COUNT
        );
        assert_eq!(
            output.metadata.timings.embedded_format_ns,
            EMBEDDED_CSS_FORMAT_NS
        );
    }

    #[test]
    fn formats_tsrx_with_one_parse_and_converges() {
        let source = "export function View({ready}:{ready:boolean}) @{ @if(ready){<p>Crème 🚀</p>;}@else{<span>no</span>;} }\n";
        let first = format_text(Path::new("View.tsrx"), source).unwrap();
        assert_eq!(first.metadata.mode, FormatMode::Projected);
        assert_eq!(first.metadata.parse_count, 1);
        assert_eq!(first.metadata.embedded_parse_count, 0);
        assert_eq!(first.metadata.marker_count, 3);
        assert!(first.metadata.projection_bytes > source.len());
        assert!(
            first
                .code
                .contains("function View({ ready }: { ready: boolean }) @{")
        );
        assert!(first.code.contains("@if (ready) {"));
        assert!(first.code.contains("} @else {"));

        let second = format_text(Path::new("View.tsrx"), &first.code).unwrap();
        assert_eq!(second.code, first.code);
        assert!(!second.changed);
    }

    #[test]
    fn ordinary_tsx_has_zero_tsrx_work() {
        let source = "export const view=<main>hello</main>\n";
        let output = format_text(Path::new("View.tsx"), source).unwrap();
        assert_eq!(output.metadata.mode, FormatMode::Direct);
        assert_eq!(output.metadata.parse_count, 1);
        assert_eq!(output.metadata.embedded_parse_count, 0);
        assert_eq!(output.metadata.timings.scan_ns, 0);
        assert_eq!(output.metadata.timings.projection_ns, 0);
        assert_eq!(output.metadata.timings.lift_ns, 0);
        assert_eq!(output.metadata.projection_bytes, 0);
        assert_eq!(output.metadata.marker_count, 0);
    }

    #[test]
    fn direct_control_lift_preserves_utf8_bytes_and_converges() {
        let source = "export function View({ok}:{ok:boolean}) @{<main>@if(ok){<p>Crème 🚀 ❚</p>}@else{<i>否</i>}</main>}\n";
        let first = format_text(Path::new("View.tsrx"), source).unwrap();
        assert!(first.code.contains("Crème 🚀 ❚"));
        assert!(first.code.contains("否"));
        let second = format_text(Path::new("View.tsrx"), &first.code).unwrap();
        assert_eq!(second.code, first.code);
    }

    #[test]
    fn switch_and_source_order_try_format_with_one_parse_and_converge() {
        let source = concat!(
            "export function View({value}:{value:number}) @{<main>",
            "@switch(value){@case 0:{@try{<b/>}@pending{<i/>}",
            "@catch(error:Error,reset:()=>void){<button onClick={reset}>{error.message}</button>}}",
            "@default:{<em/>}}</main>}\n"
        );
        let first = format_text(Path::new("View.tsrx"), source).unwrap();
        assert_eq!(first.metadata.parse_count, 1);
        assert_eq!(first.metadata.embedded_parse_count, 0);
        assert_eq!(first.metadata.style_count, 0);
        assert!(first.code.contains("@switch (value)"));
        assert!(first.code.contains("} @pending {"));
        assert!(
            first
                .code
                .contains("} @catch (error: Error, reset: () => void) {")
        );
        let second = format_text(Path::new("View.tsrx"), &first.code).unwrap();
        assert_eq!(second.metadata.parse_count, 1);
        assert_eq!(second.code, first.code);
    }

    #[test]
    fn expression_control_preserves_template_raw_indentation() {
        let source = "export function View({ok}:{ok:boolean}) @{const value=@if(ok){`first\n  raw 🚀`}@else{`second\n    否`};<p>{value}</p>;}\n";
        let first = format_text(Path::new("View.tsrx"), source).unwrap();
        assert!(first.code.contains("`first\n  raw 🚀`"));
        assert!(first.code.contains("`second\n    否`"));
        let second = format_text(Path::new("View.tsrx"), &first.code).unwrap();
        assert_eq!(second.code, first.code);
    }

    #[test]
    fn dynamic_tags_and_raw_css_preservation_format_natively_and_converge() {
        let source = "export function View({tag}:{tag:string}) @{<main><{tag}>Hi</{tag}><style>.card{color:red}</style></main>}\n";
        let first = format_text(Path::new("List.tsrx"), source).unwrap();
        assert_eq!(first.metadata.parse_count, 1);
        assert_eq!(first.metadata.embedded_parse_count, 0);
        assert_eq!(first.metadata.style_count, 1);
        assert!(first.code.contains("<{tag}>"));
        assert!(first.code.contains("</{tag}>"));
        assert!(first.code.contains(".card{color:red}"));
        let second = format_text(Path::new("List.tsrx"), &first.code).unwrap();
        assert_eq!(second.code, first.code);
        assert!(!second.changed);
    }

    #[test]
    fn preserves_dynamic_closing_comments_and_converges() {
        let source = concat!(
            "export function View({Tag}:{Tag:string}) @{",
            "<main>",
            "@if (true) {<>",
            "<{Tag}>block</{Tag /* closing block */}>",
            "<{Tag}>line</{Tag // closing line\n}>",
            "</>}",
            "</main>",
            "}\n",
        );
        let first = format_text(Path::new("Comments.tsrx"), source).unwrap();
        assert_eq!(first.code.matches("/* closing block */").count(), 1);
        assert_eq!(first.code.matches("// closing line").count(), 1);
        assert!(first.code.contains("</{Tag}>"));
        let second = format_text(Path::new("Comments.tsrx"), &first.code).unwrap();
        assert_eq!(second.code, first.code);
    }

    #[test]
    fn dynamic_regex_braces_use_checked_end_sentinels_and_converge() {
        let source = concat!(
            "export function View({ok,Tag}:{ok:boolean;Tag:string}) @{",
            "<{ok ? /\\{/ : Tag} />",
            "}\n",
        );
        let first = format_text(Path::new("Regex.tsrx"), source).unwrap();
        assert!(first.code.contains("<{ok ? /\\{/ : Tag} />;"));
        assert!(first.code.contains("\n}\n"), "{}", first.code);
        let second = format_text(Path::new("Regex.tsrx"), &first.code).unwrap();
        assert_eq!(second.code, first.code);
    }

    #[test]
    fn repeated_dynamic_style_markers_converge_past_single_digit_ordinals() {
        let mut source = String::from("export function View({Tag}:{Tag:string}) @{<main>");
        for _ in 0..32 {
            source
                .push_str("<{Tag}><{Tag} /><style>/* raw {label} */ .x{color:red}</style></{Tag}>");
        }
        source.push_str("</main>}\n");
        let first = format_text(Path::new("Repeated.tsrx"), &source).unwrap();
        assert_eq!(first.metadata.style_count, 32);
        assert_eq!(first.metadata.embedded_parse_count, 0);
        assert_eq!(first.code.matches("<{Tag}>").count(), 32);
        assert_eq!(first.code.matches("<{Tag} />").count(), 32);
        assert_eq!(first.code.matches("/* raw {label} */").count(), 32);
        let second = format_text(Path::new("Repeated.tsrx"), &first.code).unwrap();
        assert_eq!(second.code, first.code);
    }
}
