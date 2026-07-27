//! Discovering, loading, and validating the one Oxlint configuration a lint session compiles.

use std::path::{Path, PathBuf};

use oxc_config::{ConfigDiscovery, ConfigFileNames, DiscoveredConfigFile, is_js_config_path};
use oxc_linter::{ConfigBuilderError, Oxlintrc};

const OXLINT_CONFIG_FILE_NAMES: ConfigFileNames = ConfigFileNames {
    json: ".oxlintrc.json",
    jsonc: ".oxlintrc.jsonc",
    js: &["oxlint.config.ts", "oxlint.config.mts"],
    vite: "vite.config.ts",
};

pub(super) fn load_oxlintrc(
    cwd: &Path,
    explicit_path: Option<&Path>,
    config_base: Option<&Path>,
) -> Result<(Oxlintrc, Option<PathBuf>), String> {
    if config_base.is_some() && explicit_path.is_none() {
        return Err("a config base requires an explicit materialized Oxlint config".to_string());
    }
    let path = if let Some(path) = explicit_path {
        let path = if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) };
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
        format!("unable to read materialized Oxlint config {}: {error}", path.display())
    })?;
    let value: serde_json::Value = serde_json::from_str(&source).map_err(|error| {
        format!("invalid materialized Oxlint config {}: {error}", path.display())
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
        object.insert("extends".to_string(), serde_json::Value::Array(path_extends));
    }
    let source = serde_json::to_string(&value).map_err(|error| {
        format!("unable to serialize materialized Oxlint config {context}: {error}")
    })?;
    let mut config = Oxlintrc::from_string(&source).map_err(|error| error.to_string())?;
    config.extends_configs = object_extends;
    Ok(config)
}

fn resolve_existing_config_base(cwd: &Path, base: &Path, tool: &str) -> Result<PathBuf, String> {
    let base = if base.is_absolute() { base.to_path_buf() } else { cwd.join(base) };
    let base = base.canonicalize().map_err(|error| {
        format!("unable to resolve {tool} config base {}: {error}", base.display())
    })?;
    if !base.is_dir() {
        return Err(format!("{tool} config base is not a directory: {}", base.display()));
    }
    Ok(base)
}

fn discover_oxlintrc(cwd: &Path) -> Result<Option<PathBuf>, String> {
    let discovery = ConfigDiscovery::new(OXLINT_CONFIG_FILE_NAMES, false);
    let mut directory = cwd.to_path_buf();
    loop {
        let discovered =
            discovery.find_unique_config_by_readdir(&directory, true).map_err(|error| {
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

pub(super) fn reject_unavailable_lint_capabilities(
    config: &Oxlintrc,
    type_aware_opt_in: bool,
) -> Result<(), String> {
    if config.external_plugins.as_ref().is_some_and(|plugins| !plugins.is_empty()) {
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

pub(super) fn config_builder_error(error: ConfigBuilderError) -> String {
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
