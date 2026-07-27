//! Discovering, loading, and validating the one Oxlint configuration a lint session compiles.

use std::{
    error::Error,
    fmt, io,
    path::{Path, PathBuf},
};

use oxc_config::{ConfigDiscovery, ConfigFileNames, DiscoveredConfigFile, is_js_config_path};
use oxc_linter::{ConfigBuilderError, Oxlintrc};

const OXLINT_CONFIG_FILE_NAMES: ConfigFileNames = ConfigFileNames {
    json: ".oxlintrc.json",
    jsonc: ".oxlintrc.jsonc",
    js: &["oxlint.config.ts", "oxlint.config.mts"],
    vite: "vite.config.ts",
};

/// Why the one Oxlint configuration a session compiles could not be discovered, loaded, or used.
///
/// Variants that quote canonical OXC or `serde_json` keep the upstream wording in a `detail`
/// string rather than the upstream error type. That is deliberate: this crate exists to keep
/// revision-specific OXC types off its own surface (see the crate-level docs), and an
/// `oxc_linter::ConfigBuilderError` in a public variant would put the pinned revision back into
/// every consumer's signature. Filesystem failures keep their [`io::Error`] because `std` is not
/// revision-pinned and callers legitimately match on [`io::ErrorKind`].
#[derive(Debug)]
pub enum ConfigError {
    /// A config base was supplied without the materialized config it resolves paths for.
    BaseWithoutMaterializedConfig,
    /// An explicit `--config` names a JavaScript/TypeScript config module.
    ExplicitJsConfigModule,
    /// Directory discovery found a JavaScript/TypeScript config module.
    DiscoveredJsConfigModule,
    /// The materialized JSON config could not be read.
    UnreadableMaterialized { path: PathBuf, error: io::Error },
    /// The materialized JSON config is not valid JSON.
    InvalidMaterialized { path: PathBuf, detail: String },
    /// A materialized config node is not a JSON object.
    MaterializedNotObject { context: String },
    /// A materialized config node's `extends` is not a JSON array.
    MaterializedExtendsNotArray { context: String },
    /// A materialized `extends` entry is neither a path string nor a nested config object.
    MaterializedExtendsEntry { context: String, index: usize },
    /// A materialized config node could not be re-encoded for canonical OXC.
    UnserializableMaterialized { context: String, detail: String },
    /// The config base directory could not be canonicalized.
    UnresolvableBase { path: PathBuf, error: io::Error },
    /// The config base exists but is not a directory.
    BaseNotDirectory { path: PathBuf },
    /// One directory holds more than one Oxlint configuration file.
    ConflictingConfigFiles { directory: PathBuf, detail: String },
    /// The config declares external JavaScript plugins, which this Rust process cannot host
    /// itself; the `oxlint` command runs them over the TSX projection instead.
    UnsupportedJsPlugins,
    /// The config turns on type-aware linting without the explicit command-line opt-in.
    TypeAwareWithoutOptIn,
    /// Canonical OXC rejected the config for a plugin-related reason.
    JsPluginsUnavailable { detail: String },
    /// Canonical OXC rejected the config for any other reason.
    Invalid { detail: String },
    /// Canonical OXC could not parse the `.oxlintrc` document itself.
    Oxlintrc { detail: String },
    /// A caller-supplied rule filter is not a rule canonical OXC recognizes.
    Filter { detail: String },
    /// A discovered config could not be re-emitted with its `jsPlugins` stripped.
    UnserializableStrippedConfig { path: PathBuf, detail: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BaseWithoutMaterializedConfig => {
                formatter.write_str("a config base requires an explicit materialized Oxlint config")
            }
            Self::ExplicitJsConfigModule => formatter.write_str(
                "JavaScript/TypeScript Oxlint config modules require the future thin npm host; use JSON or JSONC for the native CLI",
            ),
            Self::DiscoveredJsConfigModule => formatter.write_str(
                "JavaScript/TypeScript Oxlint config modules require the future thin npm host; use .oxlintrc.json or .oxlintrc.jsonc for the native CLI",
            ),
            Self::UnreadableMaterialized { path, error } => write!(
                formatter,
                "unable to read materialized Oxlint config {}: {error}",
                path.display()
            ),
            Self::InvalidMaterialized { path, detail } => write!(
                formatter,
                "invalid materialized Oxlint config {}: {detail}",
                path.display()
            ),
            Self::MaterializedNotObject { context } => {
                write!(formatter, "materialized Oxlint config {context} must be an object")
            }
            Self::MaterializedExtendsNotArray { context } => {
                write!(formatter, "materialized Oxlint config {context}.extends must be an array")
            }
            Self::MaterializedExtendsEntry { context, index } => write!(
                formatter,
                "materialized Oxlint config {context}.extends[{index}] must be a path string or config object"
            ),
            Self::UnserializableMaterialized { context, detail } => write!(
                formatter,
                "unable to serialize materialized Oxlint config {context}: {detail}"
            ),
            Self::UnresolvableBase { path, error } => {
                write!(formatter, "unable to resolve Oxlint config base {}: {error}", path.display())
            }
            Self::BaseNotDirectory { path } => {
                write!(formatter, "Oxlint config base is not a directory: {}", path.display())
            }
            Self::ConflictingConfigFiles { directory, detail } => write!(
                formatter,
                "conflicting Oxlint configuration files in {}: {detail}",
                directory.display()
            ),
            // The first clause of this message used to claim OXC's public package exposes no plugin
            // host. That was false, and it is why JavaScript rules were refused on `.tsrx` for so
            // long: the published `oxlint` binary hosts them perfectly well, over legal TSX. What
            // this process cannot do is host them itself, because it is Rust with no Node runtime in
            // it. The `oxlint` command OXC for TSRX installs closes that gap by linting each `.tsrx`
            // file's TSX projection with the published binary and mapping every diagnostic back to
            // authored bytes, and it strips `jsPlugins` from the config it hands here. So reaching
            // this branch means one of two things: this target was run directly instead of through
            // `oxlint`, or the projection lane was switched off in the config.
            Self::UnsupportedJsPlugins => formatter.write_str(
                "JavaScript plugins are not hosted by the native TSRX lint target itself: it is a Rust process with no Node runtime. The `oxlint` command OXC for TSRX installs runs them on .tsrx for you, by linting the TSX projection with the published Oxlint binary and mapping every diagnostic back to your authored source. Run `oxlint` instead of this target, or remove the settings.oxcTsrx.jsPluginsOnTsrx false opt-out that turned that lane off",
            ),
            Self::TypeAwareWithoutOptIn => formatter.write_str(
                "type-aware tsgolint/type-check mode requires the explicit --type-aware or --type-check opt-in; it is never started or silently disabled by config alone",
            ),
            Self::JsPluginsUnavailable { detail } => write!(
                formatter,
                "JavaScript plugins are not hosted by the native TSRX lint target itself; the `oxlint` command OXC for TSRX installs runs them over the TSX projection instead: {detail}"
            ),
            Self::Invalid { detail } => {
                write!(formatter, "invalid Oxlint configuration: {detail}")
            }
            Self::UnserializableStrippedConfig { path, detail } => {
                write!(formatter, "unable to re-emit {}: {detail}", path.display())
            }
            Self::Oxlintrc { detail } | Self::Filter { detail } => formatter.write_str(detail),
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::UnreadableMaterialized { error, .. } | Self::UnresolvableBase { error, .. } => {
                Some(error)
            }
            _ => None,
        }
    }
}

impl ConfigError {
    /// Wraps a canonical OXC parse failure, keeping its wording verbatim.
    pub(super) fn oxlintrc(error: impl fmt::Display) -> Self {
        Self::Oxlintrc { detail: error.to_string() }
    }
}

pub(super) fn load_oxlintrc(
    cwd: &Path,
    explicit_path: Option<&Path>,
    config_base: Option<&Path>,
) -> Result<(Oxlintrc, Option<PathBuf>), ConfigError> {
    if config_base.is_some() && explicit_path.is_none() {
        return Err(ConfigError::BaseWithoutMaterializedConfig);
    }
    let path = if let Some(path) = explicit_path {
        let path = if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) };
        if is_js_config_path(&path) {
            return Err(ConfigError::ExplicitJsConfigModule);
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
        Oxlintrc::from_file(&path).map_err(ConfigError::oxlintrc)?
    };
    if let Some(base) = config_base {
        let base = resolve_existing_config_base(cwd, base)?;
        // ConfigStoreBuilder and LintIgnoreMatcher intentionally derive relative extends,
        // overrides, and ignorePatterns from Oxlintrc::path. The materialized JSON remains the
        // file we loaded, while this synthetic path restores the authored Vite config directory.
        config.path = base.join(".oxc-tsrx-vite-plus.oxlintrc.json");
        config.set_config_dir(&base);
    }
    Ok((config, Some(path)))
}

fn load_materialized_oxlintrc(path: &Path) -> Result<Oxlintrc, ConfigError> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| ConfigError::UnreadableMaterialized { path: path.to_path_buf(), error })?;
    let value: serde_json::Value = serde_json::from_str(&source).map_err(|error| {
        ConfigError::InvalidMaterialized { path: path.to_path_buf(), detail: error.to_string() }
    })?;
    oxlintrc_from_materialized_value(value, "<root>")
}

fn oxlintrc_from_materialized_value(
    mut value: serde_json::Value,
    context: &str,
) -> Result<Oxlintrc, ConfigError> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| ConfigError::MaterializedNotObject { context: context.to_string() })?;
    let mut path_extends = Vec::new();
    let mut object_extends = Vec::new();
    if let Some(extends) = object.remove("extends") {
        let extends = extends.as_array().ok_or_else(|| {
            ConfigError::MaterializedExtendsNotArray { context: context.to_string() }
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
                    return Err(ConfigError::MaterializedExtendsEntry {
                        context: context.to_string(),
                        index,
                    });
                }
            }
        }
    }
    if !path_extends.is_empty() {
        object.insert("extends".to_string(), serde_json::Value::Array(path_extends));
    }
    let source =
        serde_json::to_string(&value).map_err(|error| ConfigError::UnserializableMaterialized {
            context: context.to_string(),
            detail: error.to_string(),
        })?;
    let mut config = Oxlintrc::from_string(&source).map_err(ConfigError::oxlintrc)?;
    config.extends_configs = object_extends;
    Ok(config)
}

fn resolve_existing_config_base(cwd: &Path, base: &Path) -> Result<PathBuf, ConfigError> {
    let base = if base.is_absolute() { base.to_path_buf() } else { cwd.join(base) };
    let base = base
        .canonicalize()
        .map_err(|error| ConfigError::UnresolvableBase { path: base.clone(), error })?;
    if !base.is_dir() {
        return Err(ConfigError::BaseNotDirectory { path: base });
    }
    Ok(base)
}

fn discover_oxlintrc(cwd: &Path) -> Result<Option<PathBuf>, ConfigError> {
    let discovery = ConfigDiscovery::new(OXLINT_CONFIG_FILE_NAMES, false);
    let mut directory = cwd.to_path_buf();
    loop {
        let discovered =
            discovery.find_unique_config_by_readdir(&directory, true).map_err(|error| {
                ConfigError::ConflictingConfigFiles {
                    directory: directory.clone(),
                    detail: format!("{error:?}"),
                }
            })?;
        if let Some(discovered) = discovered {
            return match discovered {
                DiscoveredConfigFile::Json(path) | DiscoveredConfigFile::Jsonc(path) => {
                    Ok(Some(path))
                }
                DiscoveredConfigFile::Js(_) | DiscoveredConfigFile::Vite(_) => {
                    Err(ConfigError::DiscoveredJsConfigModule)
                }
            };
        }
        if !directory.pop() {
            return Ok(None);
        }
    }
}

/// One Oxlint configuration re-emitted with every `jsPlugins` declaration removed.
///
/// The `oxlint` command runs a project's JavaScript plugins over each `.tsrx` file's
/// TSX projection and hands this stripped configuration to the native lint target, so
/// `reject_unavailable_lint_capabilities` is never reached and the plugins are hosted
/// exactly once. Any other caller that hosts the plugins itself — the language server
/// does — needs the same treatment, and this is where that stripping lives so both
/// paths cannot drift apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsPluginFreeLintConfig {
    /// The configuration file the native lint engine would have loaded on its own.
    pub source_path: PathBuf,
    /// The directory that configuration was authored in.
    ///
    /// Relative `extends`, `overrides` globs and `ignorePatterns` are all resolved
    /// against it, so it has to be handed back to the engine as the config base or a
    /// stripped copy in a temporary directory would silently change what they match.
    pub base: PathBuf,
    /// The same configuration as JSON, minus `jsPlugins`.
    pub json: String,
}

/// The Oxlint configuration the native lint engine would load for `cwd`, re-emitted
/// without its JavaScript plugins.
///
/// Returns `Ok(None)` when there is nothing to strip: no configuration file, a
/// JavaScript config module, a file this cannot read or parse, or a configuration that
/// declares no `jsPlugins`. In every one of those cases the caller keeps whatever
/// configuration it already had, so a broken config is reported once, by the engine, in
/// the engine's own words.
///
/// Only the top level and `overrides` are stripped, which is exactly what the `oxlint`
/// wrapper strips. A `jsPlugins` declared by an `extends` target still reaches the
/// engine, and the engine still refuses it — callers must surface that refusal rather
/// than swallow it.
///
/// # Errors
///
/// Returns an error only when config discovery itself fails, which is the same error
/// the engine would have produced, or when the stripped copy cannot be re-encoded.
pub fn lint_config_without_js_plugins(
    cwd: &Path,
    explicit_path: Option<&Path>,
) -> Result<Option<JsPluginFreeLintConfig>, ConfigError> {
    let path = match explicit_path {
        Some(path) if path.is_absolute() => Some(path.to_path_buf()),
        Some(path) => Some(cwd.join(path)),
        None => discover_oxlintrc(cwd)?,
    };
    let Some(path) = path else {
        return Ok(None);
    };
    if is_js_config_path(&path) {
        return Ok(None);
    }
    let path = path.canonicalize().unwrap_or(path);
    let Ok(source) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&strip_jsonc(&source)) else {
        return Ok(None);
    };
    // The same opt-out the `oxlint` command reads. A project that has switched the
    // projection lane off has asked for the refusal, so nothing is stripped and the
    // engine answers in its own words — which the caller then has to show, because the
    // point of the opt-out is a stated position, not a blank editor.
    if opted_out_of_js_plugin_projection(&value) {
        return Ok(None);
    }
    if !remove_js_plugins(&mut value) {
        return Ok(None);
    }
    let base = path.parent().map_or_else(|| cwd.to_path_buf(), Path::to_path_buf);
    let json = serde_json::to_string(&value).map_err(|error| {
        ConfigError::UnserializableStrippedConfig { path: path.clone(), detail: error.to_string() }
    })?;
    Ok(Some(JsPluginFreeLintConfig { source_path: path, base, json }))
}

/// Whether one parsed configuration sets `settings.oxcTsrx.jsPluginsOnTsrx` to `false`.
///
/// `settings` is the only place a key Oxlint does not know can live: canonical Oxlint
/// rejects an unknown top-level key outright and ignores unknown `settings` subkeys.
fn opted_out_of_js_plugin_projection(value: &serde_json::Value) -> bool {
    value
        .get("settings")
        .and_then(|settings| settings.get("oxcTsrx"))
        .and_then(|section| section.get("jsPluginsOnTsrx"))
        .and_then(serde_json::Value::as_bool)
        == Some(false)
}

/// Delete `jsPlugins` from one parsed configuration and from each of its `overrides`,
/// reporting whether anything was there. Nothing else is touched: every other key is
/// still the user's, and Oxlint is still the thing that decides what it means.
fn remove_js_plugins(value: &mut serde_json::Value) -> bool {
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    let mut removed = object.remove("jsPlugins").is_some();
    if let Some(overrides) = object.get_mut("overrides").and_then(|value| value.as_array_mut()) {
        for entry in overrides {
            if let Some(entry) = entry.as_object_mut() {
                removed |= entry.remove("jsPlugins").is_some();
            }
        }
    }
    removed
}

/// JSONC as plain JSON: `//` and `/* */` comments dropped, trailing commas dropped,
/// string contents left exactly as written.
///
/// `Oxlintrc::from_file` accepts JSONC, so a configuration this has to re-emit may be
/// JSONC, and `serde_json` is not. Only comments and trailing commas are removed, so
/// nothing a configuration means can change on the way through.
///
/// Comments go first and trailing commas second, in that order and not together: a
/// comma is trailing only when the next thing that survives is `}` or `]`, and
/// `"rules": { "a": "error", // note` puts a comment between the two.
fn strip_jsonc(source: &str) -> String {
    strip_trailing_commas(&strip_json_comments(source))
}

fn strip_json_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut stripped = String::with_capacity(source.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let start = index;
                index = end_of_json_string(source, index);
                stripped.push_str(&source[start..index]);
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index =
                    source[index + 2..].find("*/").map_or(bytes.len(), |end| index + 2 + end + 2);
            }
            _ => {
                let width = char_width(source, index);
                stripped.push_str(&source[index..index + width]);
                index += width;
            }
        }
    }
    stripped
}

fn strip_trailing_commas(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut stripped = String::with_capacity(source.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let start = index;
                index = end_of_json_string(source, index);
                stripped.push_str(&source[start..index]);
            }
            b',' => {
                let trailing = source[index + 1..]
                    .chars()
                    .find(|character| !character.is_whitespace())
                    .is_some_and(|character| character == '}' || character == ']');
                if !trailing {
                    stripped.push(',');
                }
                index += 1;
            }
            _ => {
                let width = char_width(source, index);
                stripped.push_str(&source[index..index + width]);
                index += width;
            }
        }
    }
    stripped
}

/// One past the closing quote of the JSON string starting at `index`.
fn end_of_json_string(source: &str, index: usize) -> usize {
    let bytes = source.as_bytes();
    let mut cursor = index + 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"' {
            return cursor + 1;
        }
        if bytes[cursor] == b'\\' {
            cursor += 1;
        }
        cursor += char_width(source, cursor);
    }
    cursor.min(bytes.len())
}

/// The UTF-8 length of the character starting at `index`, or 1 past the end.
fn char_width(source: &str, index: usize) -> usize {
    source[index..].chars().next().map_or(1, char::len_utf8)
}

pub(super) fn reject_unavailable_lint_capabilities(
    config: &Oxlintrc,
    type_aware_opt_in: bool,
) -> Result<(), ConfigError> {
    if config.external_plugins.as_ref().is_some_and(|plugins| !plugins.is_empty()) {
        return Err(ConfigError::UnsupportedJsPlugins);
    }
    if !type_aware_opt_in
        && (config.options.type_aware == Some(true) || config.options.type_check == Some(true))
    {
        return Err(ConfigError::TypeAwareWithoutOptIn);
    }
    Ok(())
}

/// Classifies an OXC config-build failure by variant, keeping its rendered wording verbatim.
///
/// The four plugin variants become [`ConfigError::JsPluginsUnavailable`], because this Rust
/// process has no Node runtime to execute a JavaScript rule in; the `oxlint` command OXC for TSRX
/// installs hosts them over the TSX projection. The other five are ordinary configuration
/// defects and become [`ConfigError::Invalid`]. Matching the variant rather than searching the
/// rendered text matters: `UnsupportedNamedConfig` and `InvalidConfigFile` echo an authored
/// `extends` entry or file path back, so a substring test blamed the missing plugin host for
/// `extends: ["eslint-plugin-react"]` or a config named `plugin-overrides.json`.
///
/// `ConfigBuilderError` is not `#[non_exhaustive]`, so this match is exhaustive and a new upstream
/// variant fails the build here instead of being silently misfiled.
#[expect(
    clippy::needless_pass_by_value,
    reason = "both call sites are `map_err`, which hands the error over by value; the exhaustive match reads every variant without binding one, and only the rendered text outlives it"
)]
pub(super) fn config_builder_error(error: ConfigBuilderError) -> ConfigError {
    let detail = error.to_string();
    match error {
        ConfigBuilderError::PluginLoadFailed { .. }
        | ConfigBuilderError::NoExternalLinterConfigured { .. }
        | ConfigBuilderError::ReservedExternalPluginName { .. }
        | ConfigBuilderError::RelativeExternalPluginSpecifierInExtends { .. } => {
            ConfigError::JsPluginsUnavailable { detail }
        }
        ConfigBuilderError::UnknownRules { .. }
        | ConfigBuilderError::InvalidConfigFile { .. }
        | ConfigBuilderError::RuleConfigurationErrors { .. }
        | ConfigBuilderError::UnsupportedNamedConfig { .. }
        | ConfigBuilderError::CircularExtends { .. } => ConfigError::Invalid { detail },
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use oxc_linter::ConfigBuilderError;

    use super::{
        ConfigError, config_builder_error, opted_out_of_js_plugin_projection, remove_js_plugins,
        strip_jsonc,
    };

    /// A JSONC config the language server has to be able to re-emit. The comma before
    /// the line comment is the one this used to get wrong: it is trailing, but only
    /// after the comment it is followed by has gone.
    #[test]
    fn jsonc_configs_survive_the_trip_through_plain_json() {
        let stripped = strip_jsonc(
            r#"{
  // the project's own rules
  "jsPlugins": ["./plugin.mjs"], /* hosted by the oxlint wrapper */
  "rules": {
    "no-debugger": "error", // a comma, then a comment, then the brace
  },
  "settings": { "url": "https://oxc.rs", "note": "a } and a , inside a string" },
  "ignorePatterns": ["dist/**"],
}
"#,
        );
        let mut value: serde_json::Value = match serde_json::from_str(&stripped) {
            Ok(value) => value,
            Err(error) => panic!("stripped JSONC is not JSON ({error}):\n{stripped}"),
        };
        assert_eq!(value["settings"]["url"], "https://oxc.rs");
        assert_eq!(value["settings"]["note"], "a } and a , inside a string");
        assert_eq!(value["rules"]["no-debugger"], "error");

        assert!(remove_js_plugins(&mut value));
        assert!(value.get("jsPlugins").is_none());
        // Nothing else may move: the native engine still has to see the user's own
        // rules, ignore patterns and settings.
        assert_eq!(value["rules"]["no-debugger"], "error");
        assert_eq!(value["ignorePatterns"][0], "dist/**");
        // And a config with no plugins reports that there was nothing to strip, so the
        // caller keeps loading the file the user actually wrote.
        assert!(!remove_js_plugins(&mut value));
    }

    #[test]
    fn overrides_declare_js_plugins_too_and_the_opt_out_is_read_where_oxlint_allows_it() {
        let mut value: serde_json::Value = serde_json::from_str(
            r#"{"overrides":[{"files":["**/*.tsrx"],"jsPlugins":["./p.mjs"],"rules":{"a":"warn"}}]}"#,
        )
        .unwrap();
        assert!(remove_js_plugins(&mut value));
        assert!(value["overrides"][0].get("jsPlugins").is_none());
        assert_eq!(value["overrides"][0]["rules"]["a"], "warn");

        let opted_out: serde_json::Value =
            serde_json::from_str(r#"{"settings":{"oxcTsrx":{"jsPluginsOnTsrx":false}}}"#).unwrap();
        assert!(opted_out_of_js_plugin_projection(&opted_out));
        let opted_in: serde_json::Value =
            serde_json::from_str(r#"{"settings":{"oxcTsrx":{"jsPluginsOnTsrx":true}}}"#).unwrap();
        assert!(!opted_out_of_js_plugin_projection(&opted_in));
        assert!(!opted_out_of_js_plugin_projection(&serde_json::json!({ "settings": {} })));
    }

    #[test]
    fn config_builder_errors_classify_by_variant_not_by_rendered_text() {
        let plugin_related = [
            ConfigBuilderError::PluginLoadFailed {
                plugin_specifier: "./local-plugin.js".to_string(),
                error: "boom".to_string(),
            },
            ConfigBuilderError::NoExternalLinterConfigured {
                plugin_specifier: "./local-plugin.js".to_string(),
            },
            ConfigBuilderError::ReservedExternalPluginName { plugin_name: "eslint".to_string() },
            ConfigBuilderError::RelativeExternalPluginSpecifierInExtends {
                plugin_specifier: "./local-plugin.js".to_string(),
            },
        ];
        for error in plugin_related {
            let rendered = error.to_string();
            let classified = config_builder_error(error);
            assert!(
                matches!(&classified, ConfigError::JsPluginsUnavailable { detail } if *detail == rendered),
                "{classified:?}"
            );
        }

        // Every one of these five can echo an authored string containing "plugin" back at the
        // user, which is what the old substring test tripped over.
        let not_plugin_related = [
            ConfigBuilderError::UnknownRules { rules: Vec::new() },
            ConfigBuilderError::InvalidConfigFile {
                file: "configs/plugin-overrides.json".to_string(),
                reason: "unreadable".to_string(),
            },
            ConfigBuilderError::RuleConfigurationErrors { errors: Vec::new() },
            ConfigBuilderError::UnsupportedNamedConfig { name: "eslint-plugin-react".to_string() },
            ConfigBuilderError::CircularExtends {
                cycle: vec![PathBuf::from("plugin.oxlintrc.json")],
                referenced_from: vec![PathBuf::from(".oxlintrc.json")],
            },
        ];
        for error in not_plugin_related {
            let rendered = error.to_string();
            let classified = config_builder_error(error);
            assert!(
                matches!(&classified, ConfigError::Invalid { detail } if *detail == rendered),
                "{classified:?}"
            );
        }
    }

    #[test]
    fn an_extends_entry_named_after_a_plugin_is_not_blamed_on_the_missing_plugin_host() {
        // The exact user-visible symptom: `extends: ["eslint-plugin-react"]` used to be refused
        // with "JavaScript plugins are unavailable..." because the config name it echoes back
        // contains "plugin".
        let error =
            ConfigBuilderError::UnsupportedNamedConfig { name: "eslint-plugin-react".to_string() };
        let classified = config_builder_error(error);
        assert!(matches!(classified, ConfigError::Invalid { .. }), "{classified:?}");
        let message = classified.to_string();
        assert!(message.starts_with("invalid Oxlint configuration: "), "{message}");
        assert!(message.contains("eslint-plugin-react"), "{message}");
        assert!(!message.contains("JavaScript plugins are unavailable"), "{message}");
    }
}
