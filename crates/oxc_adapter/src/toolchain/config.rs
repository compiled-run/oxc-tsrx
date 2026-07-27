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
    /// The config declares external JavaScript plugins, which the native path cannot host.
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
            Self::UnsupportedJsPlugins => formatter.write_str(
                "JavaScript plugins are not supported by the native TSRX path yet: OXC's public package does not expose its zero-copy plugin host, and OXC for TSRX will not silently add a second parse",
            ),
            Self::TypeAwareWithoutOptIn => formatter.write_str(
                "type-aware tsgolint/type-check mode requires the explicit --type-aware or --type-check opt-in; it is never started or silently disabled by config alone",
            ),
            Self::JsPluginsUnavailable { detail } => write!(
                formatter,
                "JavaScript plugins are unavailable on the native TSRX path without OXC's public zero-copy host: {detail}"
            ),
            Self::Invalid { detail } => {
                write!(formatter, "invalid Oxlint configuration: {detail}")
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

pub(super) fn config_builder_error(error: ConfigBuilderError) -> ConfigError {
    let detail = error.to_string();
    drop(error);
    if detail.to_ascii_lowercase().contains("plugin") {
        ConfigError::JsPluginsUnavailable { detail }
    } else {
        ConfigError::Invalid { detail }
    }
}
