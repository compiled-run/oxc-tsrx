//! The only revision-specific boundary between OXC for TSRX and canonical OXC crates.

use std::path::Path;

use oxc_span::SourceType;

#[cfg(feature = "parser")]
pub mod parser;

#[cfg(feature = "editor")]
pub mod editor;

#[cfg(any(feature = "parser", feature = "toolchain"))]
mod dynamic_tags;

#[cfg(feature = "parser")]
pub(crate) use dynamic_tags::{
    DynamicTagValidationError, validate_dynamic_tags_with_synthetic_calls,
};

#[cfg(feature = "toolchain")]
pub(crate) use dynamic_tags::validate_dynamic_tags;

#[cfg(feature = "toolchain")]
mod toolchain;

#[cfg(feature = "toolchain")]
pub use toolchain::*;

pub const OXC_REVISION: &str = "8e0ed2ebb96137fb1611cdbd5742d5cb46037d40";

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

    pub(crate) fn source_type(self) -> SourceType {
        match self {
            Self::JavaScript => SourceType::unambiguous(),
            Self::JavaScriptReact => SourceType::jsx(),
            Self::TypeScript => SourceType::ts(),
            Self::TypeScriptReact => SourceType::tsx(),
        }
    }
}

/// Collision-free scaffold contract for validating TSRX dynamic names in an existing OXC AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynamicTagContract<'a> {
    pub prefix: &'a str,
    pub count: u32,
    pub original_offsets: &'a [u32],
}
