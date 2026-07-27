//! The documented tsgolint protocol-v2 wire shapes, its frame reader, and the headless child run.

use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use super::batch::{PreparedTypeBatch, protocol_diagnostic};
use crate::toolchain::TypeBatchDiagnostic;

pub(crate) fn run_type_protocol(
    executable: &Path,
    collect_fixes: bool,
    prepared: &PreparedTypeBatch<'_>,
) -> Result<Vec<TypeBatchDiagnostic>, String> {
    let mut command = Command::new(executable);
    command.arg("headless").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
    if collect_fixes {
        // Suggestions remain visible but are marked non-safe by `protocol_diagnostic`.
        command.args(["-fix", "-fix-suggestions"]);
    }
    let mut child = command.spawn().map_err(|error| {
        format!("unable to start supported tsgolint at {}: {error}", executable.display())
    })?;
    let encoded = serde_json::to_vec(&prepared.payload)
        .map_err(|error| format!("unable to encode tsgolint protocol v2 payload: {error}"))?;
    let mut stdin =
        child.stdin.take().ok_or_else(|| "tsgolint did not expose stdin".to_string())?;
    stdin
        .write_all(&encoded)
        .map_err(|error| format!("unable to transfer in-memory TSRX sources: {error}"))?;
    drop(stdin);

    let mut stdout =
        child.stdout.take().ok_or_else(|| "tsgolint did not expose stdout".to_string())?;
    let mut diagnostics = Vec::new();
    let mut protocol_error = None;
    while let Some(frame) = read_protocol_frame(&mut stdout)? {
        match frame.kind {
            0 => protocol_error = Some(parse_protocol_error(&frame.payload)?),
            1 => {
                let message: ProtocolDiagnostic = serde_json::from_slice(&frame.payload)
                    .map_err(|error| format!("invalid tsgolint diagnostic frame: {error}"))?;
                if let Some(diagnostic) =
                    protocol_diagnostic(message, &prepared.severities, &prepared.directives)
                {
                    diagnostics.push(diagnostic);
                }
            }
            2 => {}
            kind => return Err(format!("unsupported tsgolint protocol frame type {kind}")),
        }
    }
    let status = child.wait().map_err(|error| format!("unable to wait for tsgolint: {error}"))?;
    if let Some(error) = protocol_error {
        return Err(format!("tsgolint protocol error: {error}"));
    }
    if !status.success() {
        return Err(format!("tsgolint exited with {status}"));
    }
    Ok(diagnostics)
}

fn parse_protocol_error(payload: &[u8]) -> Result<String, String> {
    serde_json::from_slice::<ProtocolError>(payload)
        .map(|error| error.error)
        .map_err(|error| format!("invalid tsgolint error frame: {error}"))
}

/// `RuleEnum::name` returns `&'static str`, so the grouped payload borrows every rule name that
/// OXC already owns for the lifetime of the process.
#[derive(Debug, Clone, Serialize)]
pub(super) struct ProtocolRule {
    pub(super) name: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) options: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProtocolConfigGroup {
    pub(super) file_paths: Vec<String>,
    pub(super) rules: Vec<ProtocolRule>,
}

/// The payload borrows each projected source rather than copying it: `serde_json` writes the same
/// bytes from a `&str`, and the batch that owns those projections outlives this encode.
#[derive(Debug, Serialize)]
pub(crate) struct ProtocolPayload<'a> {
    pub(super) version: u8,
    pub(crate) configs: Vec<ProtocolConfigGroup>,
    pub(super) source_overrides: FxHashMap<String, &'a str>,
    pub(super) report_syntactic: bool,
    pub(super) report_semantic: bool,
}

#[derive(Debug, Deserialize)]
struct ProtocolError {
    error: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ProtocolRange {
    pub(super) pos: u32,
    pub(super) end: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProtocolRuleMessage {
    pub(super) id: String,
    pub(super) description: String,
    #[serde(rename = "help")]
    _help: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProtocolFix {
    pub(super) text: String,
    pub(super) range: ProtocolRange,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProtocolSuggestion {
    pub(super) fixes: Vec<ProtocolFix>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProtocolLabeledRange {
    pub(super) label: String,
    pub(super) range: ProtocolRange,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProtocolDiagnostic {
    pub(super) kind: u8,
    pub(super) range: Option<ProtocolRange>,
    pub(super) message: ProtocolRuleMessage,
    pub(super) file_path: Option<String>,
    pub(super) rule: Option<String>,
    #[serde(default)]
    pub(super) fixes: Vec<ProtocolFix>,
    #[serde(default)]
    pub(super) suggestions: Vec<ProtocolSuggestion>,
    #[serde(default)]
    pub(super) labeled_ranges: Vec<ProtocolLabeledRange>,
}

struct ProtocolFrame {
    kind: u8,
    payload: Vec<u8>,
}

fn read_protocol_frame(reader: &mut impl Read) -> Result<Option<ProtocolFrame>, String> {
    let mut first = [0_u8; 1];
    let read = reader
        .read(&mut first)
        .map_err(|error| format!("unable to read tsgolint protocol frame: {error}"))?;
    if read == 0 {
        return Ok(None);
    }
    let mut size_bytes = [0_u8; 4];
    size_bytes[0] = first[0];
    reader
        .read_exact(&mut size_bytes[1..])
        .map_err(|error| format!("truncated tsgolint frame size: {error}"))?;
    let size = usize::try_from(u32::from_le_bytes(size_bytes))
        .map_err(|_| "tsgolint frame exceeds addressable memory".to_string())?;
    let mut kind = [0_u8; 1];
    reader
        .read_exact(&mut kind)
        .map_err(|error| format!("truncated tsgolint frame kind: {error}"))?;
    let mut payload = vec![0_u8; size];
    reader
        .read_exact(&mut payload)
        .map_err(|error| format!("truncated tsgolint frame payload: {error}"))?;
    Ok(Some(ProtocolFrame { kind: kind[0], payload }))
}
