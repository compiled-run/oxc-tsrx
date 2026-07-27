//! Native lint orchestration and identity-only TSRX fix mapping.

mod fixes;
mod pipeline;
mod report;
mod session;
mod translate;

pub use oxc_adapter::{RuleFilter as ConfigRuleFilter, RuleSeverity as ConfigRuleSeverity};
pub use report::{
    DiagnosticOutput, EditorFix, FileCounts, FixOutput, LabelOutput, Metadata, Output, SpanOutput,
    TimingOutput,
};
pub use session::{LintSession, Options, lint_file, lint_text};
