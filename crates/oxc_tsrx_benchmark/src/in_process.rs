use std::{fs, hint::black_box, path::Path, time::Instant};

use oxc_adapter::{LintEngine, LintRequest, SourceKind};
use tsrx_lint::LintSession;

use crate::{
    RULE,
    report::ConfigSessionSummary,
    signatures::{DiagnosticSignature, control_signatures, product_signatures},
    stats::elapsed_ns,
};

#[derive(Debug)]
pub(crate) struct ControlSample {
    pub(crate) total_ns: u64,
    pub(crate) signatures: Vec<DiagnosticSignature>,
}

#[derive(Debug)]
pub(crate) struct ProductSample {
    pub(crate) total_ns: u64,
    pub(crate) scan_ns: u64,
    pub(crate) projection_ns: u64,
    pub(crate) parse_ns: u64,
    pub(crate) semantic_ns: u64,
    pub(crate) lint_ns: u64,
    pub(crate) projection_bytes: usize,
    pub(crate) signatures: Vec<DiagnosticSignature>,
}

pub(crate) fn measure_control(
    engine: &LintEngine,
    path: &Path,
    source: &str,
) -> Result<ControlSample, String> {
    let started = Instant::now();
    let result = engine.lint(&LintRequest {
        path,
        original_source: source,
        parse_source: source,
        source_kind: SourceKind::TypeScriptReact,
        rules: &[],
        collect_fixes: false,
        dynamic_tags: None,
    })?;
    let total_ns = elapsed_ns(started);
    let signatures = control_signatures(&result.diagnostics);
    black_box(&result);
    Ok(ControlSample { total_ns, signatures })
}

pub(crate) fn measure_product(
    session: &LintSession,
    path: &Path,
    source: &str,
) -> Result<ProductSample, String> {
    let started = Instant::now();
    let result = session.lint_text(path, source)?;
    let total_ns = elapsed_ns(started);
    let signatures = product_signatures(&result);
    let sample = ProductSample {
        total_ns,
        scan_ns: result.metadata.timings.scan_ns,
        projection_ns: result.metadata.timings.projection_ns,
        parse_ns: result.metadata.timings.parse_ns,
        semantic_ns: result.metadata.timings.semantic_ns,
        lint_ns: result.metadata.timings.lint_ns,
        projection_bytes: result.metadata.projection_bytes,
        signatures,
    };
    black_box(&result);
    Ok(sample)
}

pub(crate) fn measure_config_session(root: &Path) -> Result<ConfigSessionSummary, String> {
    fs::create_dir(root)
        .map_err(|error| format!("unable to create {}: {error}", root.display()))?;
    let config_path = root.join(".oxlintrc.json");
    fs::write(&config_path, r#"{"rules":{"no-debugger":"error"}}"#)
        .map_err(|error| format!("unable to write {}: {error}", config_path.display()))?;
    let session = LintSession::new(root, None, &[], false)?;
    let output = session.lint_text(
        &root.join("configured.tsrx"),
        "export function Configured() @{ debugger; }\n",
    )?;
    let aggregate = session.aggregate(vec![output]);
    let configured_rule_diagnostics =
        aggregate.diagnostics.iter().filter(|diagnostic| diagnostic.rule == RULE).count();
    Ok(ConfigSessionSummary {
        config_loads: aggregate.metadata.config_loads,
        config_load_ns: aggregate.metadata.timings.config_ns,
        files: aggregate.number_of_files,
        parse_count: aggregate.metadata.parse_count,
        configured_rule_diagnostics,
        configured_rule_applied: configured_rule_diagnostics == 1,
    })
}
