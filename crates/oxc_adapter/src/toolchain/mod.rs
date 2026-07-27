//! Lint, format, type-aware, and editor surfaces enabled by the default `toolchain` feature.

mod config;
mod diagnostics;
mod engine;
mod format;
mod session;
mod timings;
mod tsgolint;

pub use diagnostics::{EngineDiagnostic, EngineFix, EngineSpan};
pub use engine::{LintEngine, LintEngineOptions};
pub use format::{EngineFormatResult, FormatOptions, FormatRequest, format};
pub use session::{
    LintRequest, LintResult, TypeBatchDiagnostic, TypeBatchFile, TypeBatchResult, TypeLintRequest,
    TypeLintResult, lint,
};
pub use timings::{EngineTimings, FormatEngineTimings};
pub use tsgolint::SUPPORTED_TSGOLINT_VERSION;

// `RuleSeverity` and `RuleFilter` are defined here rather than in a submodule because
// `tsrx_lint`'s own public API names `oxc_adapter::toolchain::RuleFilter`, and `rustdoc` records
// the *defining* module of a foreign type. Moving either one into `toolchain::<submodule>` rewrites
// four lines of `tsrx_lint`'s frozen public surface without changing a single signature. The frozen
// surface outranks the "mod.rs carries no logic" layout rule; `tests/architecture.rs` pins this.
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
