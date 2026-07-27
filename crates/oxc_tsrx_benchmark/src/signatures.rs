use serde::Serialize;
use serde_json::Value;

use crate::RULE;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticSignature {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) offset: u32,
    pub(crate) length: u32,
}

pub(crate) fn control_signatures(
    diagnostics: &[oxc_adapter::EngineDiagnostic],
) -> Vec<DiagnosticSignature> {
    let mut signatures = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule.as_deref() == Some(RULE))
        .flat_map(|diagnostic| {
            diagnostic.labels.iter().map(|label| DiagnosticSignature {
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
                offset: label.offset,
                length: label.length,
            })
        })
        .collect::<Vec<_>>();
    signatures.sort_unstable();
    signatures
}

pub(crate) fn product_signatures(result: &tsrx_lint::Output) -> Vec<DiagnosticSignature> {
    let mut signatures = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == RULE)
        .flat_map(|diagnostic| {
            diagnostic.labels.iter().map(|label| DiagnosticSignature {
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
                offset: label.span.offset,
                length: label.span.length,
            })
        })
        .collect::<Vec<_>>();
    signatures.sort_unstable();
    signatures
}

pub(crate) fn json_signatures(value: &Value) -> Result<Vec<DiagnosticSignature>, String> {
    let diagnostics = value
        .get("diagnostics")
        .and_then(Value::as_array)
        .ok_or("JSON report has no diagnostics array")?;
    let mut signatures = Vec::new();
    for diagnostic in diagnostics {
        let code = diagnostic.get("code").and_then(Value::as_str).unwrap_or_default();
        let rule = diagnostic.get("rule").and_then(Value::as_str).unwrap_or_default();
        if !code.contains(RULE) && rule != RULE {
            continue;
        }
        let message = diagnostic.get("message").and_then(Value::as_str).unwrap_or_default();
        let labels = diagnostic
            .get("labels")
            .and_then(Value::as_array)
            .ok_or("diagnostic has no labels array")?;
        for label in labels {
            let span = label.get("span").ok_or("diagnostic label has no span")?;
            let offset = span
                .get("offset")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or("diagnostic label has invalid offset")?;
            let length = span
                .get("length")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or("diagnostic label has invalid length")?;
            signatures.push(DiagnosticSignature {
                code: code.to_string(),
                message: message.to_string(),
                offset,
                length,
            });
        }
    }
    signatures.sort_unstable();
    Ok(signatures)
}
