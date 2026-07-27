//! Repairing the tape's own string values, where the correct decoding depends on which lexical
//! context the value came from: JSX text, quoted string, template, or raw style.

use tsrx_syntax::OpaqueSurrogateContext;
use tsrx_tape_schema::{FlatTape, RecordIndex, TapeSpan};

use crate::{TsrxParseError, source_bridge::PreparedSource};

use super::{
    ledger::FixupLedger,
    observer::{RepairCopyLane, Utf16WorkObserver},
    pua_markers::{
        javascript_pua_markers, javascript_quoted_pua_markers, jsx_pua_markers,
        jsx_quoted_pua_markers, literal_pua_markers,
    },
    tape_fields::{
        object_field, object_span, object_type, patch_json_field, record_index, replace_json_field,
        required_object_field, style_payload_span,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceValueKind {
    JavaScriptLiteral,
    JsxAttributeLiteral,
    TemplateElement,
    JsxText,
    RawStyle,
}

#[derive(Debug, Clone, Copy)]
struct SourceValue {
    object: RecordIndex,
    kind: SourceValueKind,
    context: OpaqueSurrogateContext,
    span: TapeSpan,
}

pub(super) fn repair_program_values<W: Utf16WorkObserver>(
    tape: &mut FlatTape,
    source: &PreparedSource<'_>,
    reachable_objects: &[bool],
    ledger: &mut FixupLedger<'_, '_>,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    let (jsx_attribute_literals, directive_parents) =
        classify_program_value_parents(tape, reachable_objects)?;
    let values = collect_program_values(tape, source, reachable_objects, &jsx_attribute_literals)?;
    for value in values {
        repair_program_value(tape, source, &directive_parents, value, observer)?;
        ledger.claim(value.span, value.context)?;
    }
    Ok(())
}

fn classify_program_value_parents(
    tape: &FlatTape,
    reachable_objects: &[bool],
) -> Result<(Vec<bool>, Vec<Option<RecordIndex>>), TsrxParseError> {
    let mut jsx_attribute_literals = vec![false; tape.object_count()];
    let mut directive_parents = vec![None; tape.object_count()];
    for raw in 0..tape.object_count() {
        if !reachable_objects.get(raw).copied().unwrap_or(false) {
            continue;
        }
        let object = record_index(raw)?;
        match object_type(tape, object) {
            Some(r#""JSXAttribute""#) => {
                if let Some(value) = object_field(tape, object, "value") {
                    let index = usize::try_from(value.into_raw()).map_err(|_| {
                        TsrxParseError::Unsupported("JSX attribute value index exceeds usize")
                    })?;
                    *jsx_attribute_literals.get_mut(index).ok_or_else(|| {
                        TsrxParseError::Adapter(
                            "JSX attribute value is outside object table".to_string(),
                        )
                    })? = true;
                }
            }
            Some(r#""ExpressionStatement""#) if tape.field_index(object, "directive").is_some() => {
                let expression = required_object_field(tape, object, "expression")?;
                let index = usize::try_from(expression.into_raw()).map_err(|_| {
                    TsrxParseError::Unsupported("directive expression index exceeds usize")
                })?;
                let parent = directive_parents.get_mut(index).ok_or_else(|| {
                    TsrxParseError::Adapter(
                        "directive expression is outside object table".to_string(),
                    )
                })?;
                if parent.replace(object).is_some() {
                    return Err(TsrxParseError::Adapter(
                        "directive expression has multiple parents".to_string(),
                    ));
                }
            }
            _ => {}
        }
    }
    Ok((jsx_attribute_literals, directive_parents))
}

fn collect_program_values(
    tape: &FlatTape,
    source: &PreparedSource<'_>,
    reachable_objects: &[bool],
    jsx_attribute_literals: &[bool],
) -> Result<Vec<SourceValue>, TsrxParseError> {
    let mut values = Vec::new();
    for raw in 0..tape.object_count() {
        if !reachable_objects.get(raw).copied().unwrap_or(false) {
            continue;
        }
        let object = record_index(raw)?;
        let Some(kind) = object_type(tape, object) else {
            continue;
        };
        let kind = match kind {
            r#""Literal""# if jsx_attribute_literals.get(raw).copied().unwrap_or(false) => {
                SourceValueKind::JsxAttributeLiteral
            }
            r#""Literal""# => SourceValueKind::JavaScriptLiteral,
            r#""TemplateElement""# => SourceValueKind::TemplateElement,
            r#""JSXText""# => SourceValueKind::JsxText,
            r#""JSXStyleElement""# => SourceValueKind::RawStyle,
            _ => continue,
        };
        let span = if kind == SourceValueKind::RawStyle {
            let Some(span) = style_payload_span(tape, object)? else {
                continue;
            };
            span
        } else {
            object_span(tape, object)?
        };
        let context = match kind {
            SourceValueKind::JavaScriptLiteral => match source
                .original_span(span.start, span.end)
                .and_then(|value| value.first().copied())
            {
                Some(unit) if unit == u16::from(b'/') => OpaqueSurrogateContext::RegexBody,
                _ => OpaqueSurrogateContext::QuotedString,
            },
            SourceValueKind::JsxAttributeLiteral => OpaqueSurrogateContext::QuotedString,
            SourceValueKind::TemplateElement => OpaqueSurrogateContext::TemplateRaw,
            SourceValueKind::JsxText => OpaqueSurrogateContext::JsxText,
            SourceValueKind::RawStyle => OpaqueSurrogateContext::RawStyle,
        };
        if source.has_fixup_context_in(span.start, span.end, context) {
            values.push(SourceValue { object, kind, context, span });
        }
    }
    Ok(values)
}

fn repair_program_value<W: Utf16WorkObserver>(
    tape: &mut FlatTape,
    source: &PreparedSource<'_>,
    directive_parents: &[Option<RecordIndex>],
    value: SourceValue,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    let authored = source
        .original_span(value.span.start, value.span.end)
        .ok_or_else(|| TsrxParseError::Adapter("source value span is not exact".to_string()))?;
    match value.kind {
        SourceValueKind::JavaScriptLiteral => {
            repair_literal(tape, value.object, authored, false, observer)?;
            let index = usize::try_from(value.object.into_raw())
                .map_err(|_| TsrxParseError::Unsupported("literal object index exceeds usize"))?;
            if let Some(parent) = directive_parents.get(index).copied().flatten() {
                replace_json_field(
                    tape,
                    parent,
                    "directive",
                    quoted_interior(authored)?,
                    RepairCopyLane::ProgramSemantic,
                    observer,
                )?;
            }
        }
        SourceValueKind::JsxAttributeLiteral => {
            repair_literal(tape, value.object, authored, true, observer)?;
        }
        SourceValueKind::TemplateElement => {
            let value_object = required_object_field(tape, value.object, "value")?;
            replace_json_field(
                tape,
                value_object,
                "raw",
                authored,
                RepairCopyLane::ProgramRaw,
                observer,
            )?;
            let markers = javascript_pua_markers(authored)?;
            patch_json_field(
                tape,
                value_object,
                "cooked",
                &markers,
                true,
                RepairCopyLane::ProgramSemantic,
                observer,
            )?;
        }
        SourceValueKind::JsxText => {
            replace_json_field(
                tape,
                value.object,
                "raw",
                authored,
                RepairCopyLane::ProgramRaw,
                observer,
            )?;
            let markers = jsx_pua_markers(authored);
            patch_json_field(
                tape,
                value.object,
                "value",
                &markers,
                false,
                RepairCopyLane::ProgramSemantic,
                observer,
            )?;
        }
        SourceValueKind::RawStyle => {
            replace_json_field(
                tape,
                value.object,
                "css",
                authored,
                RepairCopyLane::ProgramRaw,
                observer,
            )?;
        }
    }
    Ok(())
}

fn quoted_interior(authored: &[u16]) -> Result<&[u16], TsrxParseError> {
    let Some((&first, rest)) = authored.split_first() else {
        return Err(TsrxParseError::Adapter("directive literal has no opening quote".to_string()));
    };
    if first != u16::from(b'\'') && first != u16::from(b'"') {
        return Err(TsrxParseError::Adapter("directive literal is not quoted".to_string()));
    }
    let Some((&last, interior)) = rest.split_last() else {
        return Err(TsrxParseError::Adapter("directive literal has no closing quote".to_string()));
    };
    if last != first {
        return Err(TsrxParseError::Adapter("directive literal quotes do not match".to_string()));
    }
    Ok(interior)
}

fn repair_literal<W: Utf16WorkObserver>(
    tape: &mut FlatTape,
    object: RecordIndex,
    authored: &[u16],
    jsx_attribute: bool,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    replace_json_field(tape, object, "raw", authored, RepairCopyLane::ProgramRaw, observer)?;
    match authored.first().copied() {
        Some(unit) if unit == u16::from(b'\'') || unit == u16::from(b'"') => {
            let markers = if jsx_attribute {
                jsx_quoted_pua_markers(authored)?
            } else {
                javascript_quoted_pua_markers(authored)?
            };
            patch_json_field(
                tape,
                object,
                "value",
                &markers,
                false,
                RepairCopyLane::ProgramSemantic,
                observer,
            )?;
        }
        Some(unit) if unit == u16::from(b'/') => {
            let pattern = regex_pattern(authored)?;
            let regex = required_object_field(tape, object, "regex")?;
            let markers = literal_pua_markers(pattern);
            patch_json_field(
                tape,
                regex,
                "pattern",
                &markers,
                false,
                RepairCopyLane::ProgramSemantic,
                observer,
            )?;
        }
        _ => {
            return Err(TsrxParseError::Adapter(
                "surrogate-bearing Literal is neither string nor RegExp".to_string(),
            ));
        }
    }
    Ok(())
}

fn regex_pattern(value: &[u16]) -> Result<&[u16], TsrxParseError> {
    if value.first().copied() != Some(u16::from(b'/')) {
        return Err(TsrxParseError::Adapter("RegExp does not start with slash".to_string()));
    }
    let mut escaped = false;
    let mut in_class = false;
    for index in 1..value.len() {
        match value[index] {
            _ if escaped => escaped = false,
            unit if unit == u16::from(b'\\') => escaped = true,
            unit if unit == u16::from(b'[') => in_class = true,
            unit if unit == u16::from(b']') => in_class = false,
            unit if unit == u16::from(b'/') && !in_class => return Ok(&value[1..index]),
            _ => {}
        }
    }
    Err(TsrxParseError::Adapter("RegExp has no closing slash".to_string()))
}
