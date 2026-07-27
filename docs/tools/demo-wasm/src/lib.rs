//! Docs-only helper. Exposes the real lint, format, and projection engines to
//! the playground as WebAssembly. Every function takes and returns JSON
//! strings so the browser worker can reuse the exact payload shapes that
//! docs/serve.mjs produces from the native binaries.

use std::path::Path;

use napi_derive::napi;
use tsrx_lint::{ConfigRuleFilter, ConfigRuleSeverity, LintSession};

const DEMO_DIR: &str = "/demo";
const DEMO_FILE: &str = "/demo/demo.tsrx";

fn error_json(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

/// Lint one in-memory TSRX source with the real engine.
///
/// `options_json`: `{ "config": string?, "filters": [{ "rule": string,
/// "severity": "allow"|"warn"|"deny" }]? }` — the same request shape the demo
/// server accepts. Returns the CLI's `--format=json` report, or `{ "error" }`.
#[napi]
pub fn lint(source: String, options_json: String) -> String {
    let options: serde_json::Value = match serde_json::from_str(&options_json) {
        Ok(value) => value,
        Err(_) => serde_json::Value::Null,
    };
    if options["typeAware"].as_bool() == Some(true) || options["typeCheck"].as_bool() == Some(true)
    {
        return error_json("type-aware lint is unavailable: it needs the local development server");
    }
    let mut filters = Vec::new();
    if let Some(entries) = options["filters"].as_array() {
        for entry in entries {
            let (Some(rule), Some(severity)) = (entry["rule"].as_str(), entry["severity"].as_str())
            else {
                continue;
            };
            let severity = match severity {
                "allow" => ConfigRuleSeverity::Allow,
                "warn" => ConfigRuleSeverity::Warn,
                "deny" => ConfigRuleSeverity::Deny,
                _ => continue,
            };
            filters.push(ConfigRuleFilter {
                severity,
                name: rule.to_string(),
            });
        }
    }
    let config_source = options["config"]
        .as_str()
        .filter(|text| !text.trim().is_empty());
    let session = match LintSession::new_with_config_source(
        Path::new(DEMO_DIR),
        config_source,
        &filters,
        false,
    ) {
        Ok(session) => session,
        Err(error) => return error_json(&error.to_string()),
    };
    let output = match session.lint_text(Path::new(DEMO_FILE), &source) {
        Ok(output) => output,
        Err(error) => return error_json(&error.to_string()),
    };
    let report = session.aggregate(vec![output]);
    serde_json::to_string(&report).unwrap_or_else(|error| error_json(&error.to_string()))
}

/// Format one in-memory TSRX source with the real engine.
/// Returns `{ "formatted": string }` or `{ "error": string }`.
#[napi]
pub fn format(source: String) -> String {
    match tsrx_format::format_text(Path::new(DEMO_FILE), &source) {
        Ok(output) => serde_json::json!({ "formatted": output.code }).to_string(),
        Err(error) => error_json(&error.to_string()),
    }
}

/// Project one in-memory TSRX source. Mirrors docs/tools/projection-dump:
/// `{ "projected", "tokens", "counts" }` (or the type-semantic projection
/// with `types = true`), `{ "error" }` on failure.
#[napi]
pub fn project(source: String, types: bool) -> String {
    let overlay = match tsrx_syntax::scan(&source) {
        Ok(overlay) => overlay,
        Err(error) => return error_json(&error.to_string()),
    };
    if types {
        return match tsrx_syntax::project_for_types(&source, &overlay) {
            Ok(projection) => serde_json::json!({ "projected": projection.source() }).to_string(),
            Err(error) => error_json(&error.to_string()),
        };
    }
    let tokens = overlay
        .tokens()
        .iter()
        .map(|token| {
            serde_json::json!({
                "kind": format!("{:?}", token.kind),
                "start": token.span.start,
                "end": token.span.end,
            })
        })
        .collect::<Vec<_>>();
    match tsrx_syntax::project_for_lint(&source, &overlay) {
        Ok(projection) => serde_json::json!({
            "projected": projection.source(),
            "tokens": tokens,
            "counts": {
                "controls": overlay.control_count(),
                "dynamicTags": overlay.dynamic_tag_count(),
                "styleBlocks": overlay.style_block_count(),
            },
        })
        .to_string(),
        Err(error) => error_json(&error.to_string()),
    }
}
