//! The one error every native formatting entry point returns.

use std::{
    error::Error,
    fmt, io,
    path::{Path, PathBuf},
};

use oxc_adapter::{FormatError as EngineFormatError, SourceKindError};
use tsrx_syntax::ProjectionError;

/// Which part of an Oxfmt configuration document a rejection came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigScope {
    /// The top-level options block.
    Root,
    /// One entry of the `overrides` array.
    Override { index: usize },
}

impl fmt::Display for ConfigScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Root => formatter.write_str("root Oxfmt config"),
            Self::Override { index } => write!(formatter, "Oxfmt override {index}"),
        }
    }
}

/// Which glob list of an override rejected a pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobField {
    Files,
    ExcludeFiles,
}

impl fmt::Display for GlobField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Files => "files",
            Self::ExcludeFiles => "excludeFiles",
        })
    }
}

/// Why a native TSRX formatting run produced no output.
///
/// Variants that quote `serde_json`, `globset`, or `ignore` keep the upstream wording in a
/// `detail` string rather than the upstream error type, so a dependency bump cannot ripple into
/// this crate's signatures.
#[derive(Debug)]
pub enum FormatError {
    /// A config base was supplied without the materialized config it resolves paths for.
    BaseWithoutMaterializedConfig,
    /// An explicit `--config` names `.editorconfig`, which this formatter never applies silently.
    EditorConfigRejected,
    /// Directory discovery found an `.editorconfig` this formatter never applies silently.
    EditorConfigDiscovered { path: PathBuf },
    /// An explicit `--config` names a JavaScript/TypeScript config module.
    ExplicitJsConfigModule,
    /// Directory discovery found a JavaScript/TypeScript config module.
    DiscoveredJsConfigModule,
    /// One directory holds more than one Oxfmt configuration file.
    ConflictingConfigFiles { directory: PathBuf, names: Vec<String> },
    /// The Oxfmt configuration could not be read.
    UnreadableConfig { path: PathBuf, error: io::Error },
    /// The Oxfmt configuration's comments could not be stripped.
    InvalidJsonc { path: PathBuf, detail: String },
    /// The Oxfmt configuration is not the documented shape.
    InvalidConfig { path: PathBuf, detail: String },
    /// An option name this formatter does not recognize, which it never ignores silently.
    UnknownOption { option: String, scope: ConfigScope },
    /// A known Oxfmt option that needs a surface outside the pinned formatter boundary.
    UnavailableOption { option: &'static str, scope: ConfigScope },
    /// `embeddedLanguageFormatting`, which needs canonical embedded-language callbacks.
    EmbeddedLanguageFormattingUnavailable { scope: ConfigScope },
    /// An `experimental*` option the pinned formatter does not implement.
    ExperimentalOptions { scope: ConfigScope },
    /// An override declares no `files` patterns, so it could never match.
    OverrideWithoutFiles { index: usize },
    /// One override glob is not a valid pattern.
    InvalidGlob { scope: ConfigScope, field: GlobField, pattern: String, detail: String },
    /// A set of valid override globs could not be compiled together.
    UnbuildableGlobSet { scope: ConfigScope, field: GlobField, detail: String },
    /// One `ignorePatterns` entry is not a valid gitignore line.
    InvalidIgnorePattern { pattern: String, detail: String },
    /// A set of valid `ignorePatterns` entries could not be compiled together.
    UnbuildableIgnore { detail: String },
    /// The config base directory could not be canonicalized.
    UnresolvableBase { path: PathBuf, error: io::Error },
    /// The config base exists but is not a directory.
    BaseNotDirectory { path: PathBuf },
    /// The authored path carries no extension this formatter can parse.
    SourceKind(SourceKindError),
    /// The TSRX source could not be scanned, projected, or lifted back.
    Projection(ProjectionError),
    /// Canonical Oxfmt could not parse, print, or accept its options.
    Engine(EngineFormatError),
}

impl FormatError {
    pub(crate) fn unreadable_config(path: &Path, error: io::Error) -> Self {
        Self::UnreadableConfig { path: path.to_path_buf(), error }
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BaseWithoutMaterializedConfig => {
                formatter.write_str("a config base requires an explicit materialized Oxfmt config")
            }
            Self::EditorConfigRejected => formatter.write_str(
                ".editorconfig is not supported by the native TSRX formatter yet; its settings are never silently ignored",
            ),
            Self::EditorConfigDiscovered { path } => write!(
                formatter,
                ".editorconfig is not supported by the native TSRX formatter yet: {}; its settings are never silently ignored",
                path.display()
            ),
            Self::ExplicitJsConfigModule => formatter.write_str(
                "JavaScript/TypeScript Oxfmt config modules require the future thin npm host; use JSON or JSONC for the native CLI",
            ),
            Self::DiscoveredJsConfigModule => formatter.write_str(
                "JavaScript/TypeScript Oxfmt config modules require the future thin npm host; use .oxfmtrc.json or .oxfmtrc.jsonc for the native CLI",
            ),
            Self::ConflictingConfigFiles { directory, names } => write!(
                formatter,
                "multiple Oxfmt configuration files found in {}: {}",
                directory.display(),
                names.join(", ")
            ),
            Self::UnreadableConfig { path, error } => {
                write!(formatter, "unable to read Oxfmt config {}: {error}", path.display())
            }
            Self::InvalidJsonc { path, detail } => {
                write!(formatter, "invalid JSONC in {}: {detail}", path.display())
            }
            Self::InvalidConfig { path, detail } => {
                write!(formatter, "invalid Oxfmt config {}: {detail}", path.display())
            }
            Self::UnknownOption { option, scope } => write!(
                formatter,
                "unsupported Oxfmt option `{option}` in {scope}; OXC for TSRX never silently ignores unknown TSRX-affecting options"
            ),
            Self::UnavailableOption { option, scope } => write!(
                formatter,
                "Oxfmt `{option}` is not available for TSRX in {scope}: it needs a callback/config surface outside the public pinned formatter boundary"
            ),
            Self::EmbeddedLanguageFormattingUnavailable { scope } => write!(
                formatter,
                "Oxfmt `embeddedLanguageFormatting` is not available for TSRX in {scope}: canonical embedded-language callbacks are outside the public pinned formatter boundary"
            ),
            Self::ExperimentalOptions { scope } => write!(
                formatter,
                "experimental Oxfmt options are not supported by the pinned formatter in {scope}"
            ),
            Self::OverrideWithoutFiles { index } => {
                write!(formatter, "Oxfmt override {index} requires at least one files pattern")
            }
            Self::InvalidGlob { scope, field, pattern, detail } => write!(
                formatter,
                "invalid {scope} {field} pattern `{pattern}`: {detail}"
            ),
            Self::UnbuildableGlobSet { scope, field, detail } => {
                write!(formatter, "unable to build {scope} {field}: {detail}")
            }
            Self::InvalidIgnorePattern { pattern, detail } => {
                write!(formatter, "invalid Oxfmt ignorePatterns entry `{pattern}`: {detail}")
            }
            Self::UnbuildableIgnore { detail } => {
                write!(formatter, "unable to build Oxfmt ignorePatterns: {detail}")
            }
            Self::UnresolvableBase { path, error } => {
                write!(formatter, "unable to resolve Oxfmt config base {}: {error}", path.display())
            }
            Self::BaseNotDirectory { path } => {
                write!(formatter, "Oxfmt config base is not a directory: {}", path.display())
            }
            Self::SourceKind(error) => error.fmt(formatter),
            Self::Projection(error) => error.fmt(formatter),
            Self::Engine(error) => error.fmt(formatter),
        }
    }
}

impl Error for FormatError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::UnreadableConfig { error, .. } | Self::UnresolvableBase { error, .. } => {
                Some(error)
            }
            Self::SourceKind(error) => Some(error),
            Self::Projection(error) => Some(error),
            Self::Engine(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SourceKindError> for FormatError {
    fn from(error: SourceKindError) -> Self {
        Self::SourceKind(error)
    }
}

impl From<ProjectionError> for FormatError {
    fn from(error: ProjectionError) -> Self {
        Self::Projection(error)
    }
}

impl From<EngineFormatError> for FormatError {
    fn from(error: EngineFormatError) -> Self {
        Self::Engine(error)
    }
}

// `oxc_tsrx_cli` and `oxc_tsrx_format_benchmark` still funnel every failure into
// `Result<_, String>`, because their contract is the exact text they print and their exit codes.
// `?` at `oxc_tsrx_cli/src/fmt.rs:157,375`, `oxc_tsrx_format_benchmark/src/in_process.rs:23,33`
// and `oxc_tsrx_format_benchmark/src/process.rs:160` needs this conversion, and it renders exactly
// `Display`, so the text those binaries print is unchanged.
impl From<FormatError> for String {
    fn from(error: FormatError) -> Self {
        error.to_string()
    }
}
