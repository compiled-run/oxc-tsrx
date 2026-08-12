//! Lazy destructuring patterns, projected without `&` and restored with `lazy: true`.

use tsrx_syntax::{OverlayView, ProjectionSegment};
use tsrx_tape_schema::{FlatTape, RecordIndex};

use crate::{
    TsrxParseError,
    projection::{map_endpoint, project_authored_start},
    tape_index::ParentIndex,
};

use super::{
    access::{has_type, object_field, scalar_u32},
    objects::find_unique_start,
    spans::{AuthoredStart, record_authored_span},
};

pub(super) fn reconstruct_lazy_patterns(
    tape: &mut FlatTape,
    overlay: OverlayView<'_>,
    segments: &[ProjectionSegment],
    patterns: &[(u32, RecordIndex)],
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    for lazy in overlay.parser_lazy_patterns {
        let pattern = find_pattern(tape, lazy.pattern_start, segments, patterns)?;
        let declarator = declarator_for_pattern(tape, pattern, parents)?;

        if tape.field_index(pattern, "lazy").is_some() {
            return Err(TsrxParseError::Unsupported("projected lazy pattern already has metadata"));
        }
        let lazy_value = tape.push_scalar("true")?;
        tape.append_field(pattern, "lazy", lazy_value)?;
        let pattern_end = scalar_u32(tape, pattern, "end")?;
        let authored_end = map_endpoint(segments, pattern_end, false)
            .ok_or(TsrxParseError::Unsupported("lazy pattern end is unmapped"))?;
        record_authored_span(
            starts,
            pattern,
            tsrx_syntax::ByteSpan::new(lazy.pattern_start, authored_end),
        );
        if let Some(declarator) = declarator {
            let declarator_end = scalar_u32(tape, declarator, "end")?;
            let declarator_end = map_endpoint(segments, declarator_end, false)
                .ok_or(TsrxParseError::Unsupported("lazy declarator end is unmapped"))?;
            record_authored_span(
                starts,
                declarator,
                tsrx_syntax::ByteSpan::new(lazy.ampersand, declarator_end),
            );
        }
    }
    Ok(())
}

fn find_pattern(
    tape: &FlatTape,
    authored_start: u32,
    segments: &[ProjectionSegment],
    patterns: &[(u32, RecordIndex)],
) -> Result<RecordIndex, TsrxParseError> {
    let projected_start = project_authored_start(segments, authored_start)
        .ok_or(TsrxParseError::Unsupported("lazy pattern start is unmapped"))?;
    let pattern =
        find_unique_start(patterns, projected_start, "lazy pattern is missing or duplicated")?;
    if map_endpoint(segments, scalar_u32(tape, pattern, "start")?, true) != Some(authored_start) {
        return Err(TsrxParseError::Unsupported("lazy pattern start is displaced"));
    }
    Ok(pattern)
}

fn declarator_for_pattern(
    tape: &FlatTape,
    pattern: RecordIndex,
    parents: &ParentIndex,
) -> Result<Option<RecordIndex>, TsrxParseError> {
    let parent = parents
        .parent_container(tsrx_tape_schema::ValueRef::object(pattern))
        .ok_or(TsrxParseError::Unsupported("lazy pattern has no parent"))?;
    if let Some(declarator) = parent.as_object() {
        if has_type(tape, declarator, r#""VariableDeclarator""#)
            && object_field(tape, declarator, "id")? == pattern
        {
            return Ok(Some(declarator));
        }
        return Err(TsrxParseError::Unsupported("lazy pattern has an unsupported object parent"));
    }
    let list =
        parent.as_list().ok_or(TsrxParseError::Unsupported("lazy pattern parent is not a list"))?;
    let function = parents
        .parent_container(tsrx_tape_schema::ValueRef::list(list))
        .and_then(tsrx_tape_schema::ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("lazy parameter has no function parent"))?;
    if !matches!(
        super::access::object_type(tape, function),
        Some(
            r#""FunctionDeclaration""# | r#""FunctionExpression""# | r#""ArrowFunctionExpression""#
        )
    ) {
        return Err(TsrxParseError::Unsupported(
            "lazy pattern list is not a function parameter list",
        ));
    }
    Ok(None)
}
