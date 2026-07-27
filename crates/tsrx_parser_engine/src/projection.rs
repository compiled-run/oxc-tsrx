use tsrx_syntax::{
    ByteSpan, ClauseRole, ControlContext, ControlKind, ForHeader, NONE_INDEX, OverlayView,
    ParserDynamicKind, ProjectionSegment, ProjectionView, StructuralKind,
};
use tsrx_tape_schema::{CommentRecord, CommentTable, ProjectedCommentKind, StringRange};

use crate::TsrxParseError;

#[derive(Debug, Clone, Copy)]
enum MarkerKind {
    Token(u32),
    Style(u32),
    WrapperStart(u32),
    WrapperEnd(u32),
    Header { ordinal: u32, part: HeaderPart, boundary: MarkerBoundary },
}

#[derive(Debug, Clone, Copy)]
enum HeaderPart {
    Right,
    Index,
    Key,
}

#[derive(Debug, Clone, Copy)]
enum MarkerBoundary {
    Start,
    End,
}

pub(super) fn validate_overlay(view: OverlayView<'_>) -> Result<(), TsrxParseError> {
    validate_dynamic_overlay(view)?;
    validate_style_overlay(view)?;
    let mut root_tokens = vec![false; view.nodes.len()];
    for token in view.tokens {
        let valid = match token.kind {
            StructuralKind::FunctionBody => code_block_owner_matches(view, *token),
            StructuralKind::If => mark_control_root(
                view,
                &mut root_tokens,
                token.owner,
                token.span.start,
                ControlKind::If,
            ),
            StructuralKind::For => mark_control_root(
                view,
                &mut root_tokens,
                token.owner,
                token.span.start,
                ControlKind::For,
            ),
            StructuralKind::Switch => mark_control_root(
                view,
                &mut root_tokens,
                token.owner,
                token.span.start,
                ControlKind::Switch,
            ),
            StructuralKind::Try => mark_control_root(
                view,
                &mut root_tokens,
                token.owner,
                token.span.start,
                ControlKind::Try,
            ),
            StructuralKind::Else => control_owner_matches(view, token.owner, ControlKind::If),
            StructuralKind::Empty => control_owner_matches(view, token.owner, ControlKind::For),
            StructuralKind::Case | StructuralKind::Default => {
                control_owner_matches(view, token.owner, ControlKind::Switch)
            }
            StructuralKind::Pending | StructuralKind::Catch => {
                control_owner_matches(view, token.owner, ControlKind::Try)
            }
        };
        if !valid || token.span.end != token.span.start.saturating_add(1) {
            return Err(TsrxParseError::Unsupported(
                "control family outside implemented parser syntax",
            ));
        }
    }
    if root_tokens.iter().any(|seen| !seen) {
        return Err(TsrxParseError::Unsupported("control node has no unique root token"));
    }
    for (node_index, node) in view.nodes.iter().enumerate() {
        match node.kind {
            ControlKind::If => validate_if_clauses(view, node_index)?,
            ControlKind::For => validate_for_clauses(view, node_index)?,
            ControlKind::Switch => validate_switch_clauses(view, node_index)?,
            ControlKind::Try => validate_try_clauses(view, node_index)?,
        }
    }
    if view.nodes.is_empty() && view.first_root != NONE_INDEX {
        return Err(TsrxParseError::Unsupported("invalid empty control topology"));
    }
    Ok(())
}

fn code_block_owner_matches(view: OverlayView<'_>, token: tsrx_syntax::StructuralToken) -> bool {
    if token.owner == NONE_INDEX {
        return true;
    }
    let Some(node) = usize::try_from(token.owner).ok().and_then(|owner| view.nodes.get(owner))
    else {
        return false;
    };
    let mut clause = node.first_clause;
    while clause != NONE_INDEX {
        let Some(current) = usize::try_from(clause).ok().and_then(|index| view.clauses.get(index))
        else {
            return false;
        };
        if current.body.start < token.span.start && token.span.end <= current.body.end {
            return true;
        }
        clause = current.next;
    }
    false
}

#[allow(clippy::too_many_lines)]
fn validate_dynamic_overlay(view: OverlayView<'_>) -> Result<(), TsrxParseError> {
    let mut opened = vec![false; view.dynamic_tags.len()];
    let mut closed = vec![false; view.dynamic_tags.len()];
    let mut active = Vec::with_capacity(8);
    let mut previous_dynamic_end = 0_u32;
    let mut previous_style_end = 0_u32;
    for token in view.embedded {
        match token.kind {
            tsrx_syntax::EmbeddedKind::DynamicOpen => {
                if token.span.is_empty()
                    || (view.parser_dynamic.is_empty() && token.span.start < previous_dynamic_end)
                {
                    return Err(TsrxParseError::Unsupported(
                        "dynamic embedded tokens are unordered",
                    ));
                }
                previous_dynamic_end = token.span.end;
                let index = usize::try_from(token.owner)
                    .map_err(|_| TsrxParseError::Unsupported("dynamic owner index overflow"))?;
                let tag = view
                    .dynamic_tags
                    .get(index)
                    .ok_or(TsrxParseError::Unsupported("unknown dynamic tag owner"))?;
                if std::mem::replace(
                    opened
                        .get_mut(index)
                        .ok_or(TsrxParseError::Unsupported("unknown dynamic opening"))?,
                    true,
                ) || tag.expression.is_empty()
                    || tag.opening != token.span
                    || token.span.start.checked_add(2) != Some(tag.expression.start)
                    || tag.expression.end.checked_add(1) != Some(token.span.end)
                {
                    return Err(TsrxParseError::Unsupported("malformed dynamic opening token"));
                }
                if !tag.self_closing {
                    active.push(token.owner);
                }
            }
            tsrx_syntax::EmbeddedKind::DynamicClose => {
                if token.span.is_empty()
                    || (view.parser_dynamic.is_empty() && token.span.start < previous_dynamic_end)
                {
                    return Err(TsrxParseError::Unsupported(
                        "dynamic embedded tokens are unordered",
                    ));
                }
                previous_dynamic_end = token.span.end;
                let index = usize::try_from(token.owner)
                    .map_err(|_| TsrxParseError::Unsupported("dynamic owner index overflow"))?;
                let tag = view
                    .dynamic_tags
                    .get(index)
                    .ok_or(TsrxParseError::Unsupported("unknown dynamic tag owner"))?;
                let closing_min_end = tag
                    .closing_expression
                    .end
                    .checked_add(2)
                    .ok_or(TsrxParseError::Unsupported("dynamic closing span overflow"))?;
                if tag.self_closing
                    || active.pop() != Some(token.owner)
                    || std::mem::replace(
                        closed
                            .get_mut(index)
                            .ok_or(TsrxParseError::Unsupported("unknown dynamic closing"))?,
                        true,
                    )
                    || tag.closing_expression.is_empty()
                    || tag.closing != token.span
                    || token.span.start.checked_add(3) != Some(tag.closing_expression.start)
                    || closing_min_end > token.span.end
                {
                    return Err(TsrxParseError::Unsupported("malformed dynamic closing token"));
                }
            }
            tsrx_syntax::EmbeddedKind::StyleContent => {
                if token.span.start < previous_style_end {
                    return Err(TsrxParseError::Unsupported("style payload tokens are unordered"));
                }
                previous_style_end = token.span.end;
            }
        }
    }
    if !active.is_empty() {
        return Err(TsrxParseError::Unsupported("dynamic projection nesting is incomplete"));
    }
    if view.parser_dynamic.is_empty() {
        if view
            .dynamic_tags
            .iter()
            .enumerate()
            .any(|(index, tag)| !opened[index] || closed[index] == tag.self_closing)
        {
            return Err(TsrxParseError::Unsupported("incomplete dynamic embedded token set"));
        }
    } else {
        validate_parser_dynamic_boundaries(view)?;
    }
    let mut comments_seen = vec![false; view.dynamic_comments.len()];
    for tag in view.dynamic_tags {
        if tag.opening.is_empty()
            || tag.opening.start.checked_add(2) != Some(tag.expression.start)
            || tag.expression.is_empty()
            || tag.expression.end.checked_add(1) != Some(tag.opening.end)
            || tag.self_closing != tag.closing.is_empty()
            || (tag.self_closing && tag.closing.end <= tag.opening.end)
            || (!tag.self_closing
                && (tag.closing.start.checked_add(3) != Some(tag.closing_expression.start)
                    || tag.closing_expression.is_empty()
                    || tag.closing_expression.end.saturating_add(2) > tag.closing.end))
        {
            return Err(TsrxParseError::Unsupported("malformed dynamic authored spans"));
        }
        if tag.self_closing {
            if !tag.closing_expression.is_empty() || tag.closing_comment_count != 0 {
                return Err(TsrxParseError::Unsupported(
                    "self-closing dynamic tag has closing metadata",
                ));
            }
            continue;
        }
        let first = usize::try_from(tag.first_closing_comment)
            .map_err(|_| TsrxParseError::Unsupported("dynamic comment index overflow"))?;
        let end = first
            .checked_add(tag.closing_comment_count as usize)
            .ok_or(TsrxParseError::Unsupported("dynamic comment range overflow"))?;
        for (offset, comment) in view
            .dynamic_comments
            .get(first..end)
            .ok_or(TsrxParseError::Unsupported("dynamic comment range outside overlay"))?
            .iter()
            .enumerate()
        {
            let seen = comments_seen
                .get_mut(first + offset)
                .ok_or(TsrxParseError::Unsupported("dynamic comment index is unknown"))?;
            if comment.is_empty()
                || comment.start < tag.closing_expression.start
                || comment.end > tag.closing_expression.end
                || std::mem::replace(seen, true)
            {
                return Err(TsrxParseError::Unsupported(
                    "dynamic comment lies outside closing expression",
                ));
            }
        }
    }
    if comments_seen.iter().any(|seen| !seen) {
        return Err(TsrxParseError::Unsupported("unowned dynamic closing comment"));
    }
    Ok(())
}

fn validate_style_overlay(view: OverlayView<'_>) -> Result<(), TsrxParseError> {
    let mut payloads = vec![false; view.style_blocks.len()];
    for token in
        view.embedded.iter().filter(|token| token.kind == tsrx_syntax::EmbeddedKind::StyleContent)
    {
        let index = usize::try_from(token.owner)
            .map_err(|_| TsrxParseError::Unsupported("style owner index overflow"))?;
        let style = view
            .style_blocks
            .get(index)
            .ok_or(TsrxParseError::Unsupported("unknown style owner"))?;
        if style.self_closing
            || style.content != token.span
            || std::mem::replace(
                payloads
                    .get_mut(index)
                    .ok_or(TsrxParseError::Unsupported("unknown style payload"))?,
                true,
            )
        {
            return Err(TsrxParseError::Unsupported("malformed or duplicated style payload token"));
        }
    }

    let mut previous_start = None;
    let mut ancestors: Vec<tsrx_syntax::OverlayStyleBlock> = Vec::with_capacity(4);
    for (index, style) in view.style_blocks.iter().enumerate() {
        while ancestors.last().is_some_and(|ancestor| ancestor.element.end <= style.element.start) {
            ancestors.pop();
        }
        let nested_in_opening =
            ancestors.last().is_none_or(|ancestor| style.element.end <= ancestor.content.start);
        let spans_are_valid = previous_start.is_none_or(|start| style.element.start > start)
            && nested_in_opening
            && !style.element.is_empty()
            && style.element.start < style.content.start
            && style.content.end <= style.element.end
            && if style.self_closing {
                style.content.is_empty()
                    && style.content.start == style.element.end
                    && !payloads[index]
            } else {
                style.content.end < style.element.end && payloads[index]
            };
        if !spans_are_valid {
            return Err(TsrxParseError::Unsupported("malformed authored style spans"));
        }
        previous_start = Some(style.element.start);
        ancestors.push(*style);
    }
    Ok(())
}

fn validate_parser_dynamic_boundaries(view: OverlayView<'_>) -> Result<(), TsrxParseError> {
    let mut phases = vec![0_u8; view.dynamic_tags.len()];
    let mut active = Vec::with_capacity(8);
    let mut previous = 0_u32;
    for (ordinal, token) in view.parser_dynamic.iter().enumerate() {
        if ordinal != 0 && token.offset < previous {
            return Err(TsrxParseError::Unsupported("parser dynamic boundaries are unordered"));
        }
        previous = token.offset;
        let index = usize::try_from(token.owner)
            .map_err(|_| TsrxParseError::Unsupported("parser dynamic owner overflow"))?;
        let tag = view
            .dynamic_tags
            .get(index)
            .ok_or(TsrxParseError::Unsupported("unknown parser dynamic owner"))?;
        let phase = phases
            .get_mut(index)
            .ok_or(TsrxParseError::Unsupported("unknown parser dynamic owner"))?;
        match token.kind {
            ParserDynamicKind::OpenStart if *phase == 0 && token.offset == tag.opening.start => {
                *phase = 1;
                active.push(token.owner);
            }
            ParserDynamicKind::OpenEnd
                if *phase == 1
                    && token.offset == tag.expression.end
                    && active.pop() == Some(token.owner) =>
            {
                *phase = 2;
            }
            ParserDynamicKind::CloseStart
                if *phase == 2 && !tag.self_closing && token.offset == tag.closing.start =>
            {
                *phase = 3;
                active.push(token.owner);
            }
            ParserDynamicKind::CloseEnd
                if *phase == 3
                    && token.offset == tag.closing_expression.end
                    && active.pop() == Some(token.owner) =>
            {
                *phase = 4;
            }
            _ => {
                return Err(TsrxParseError::Unsupported("malformed parser dynamic boundary"));
            }
        }
    }
    if !active.is_empty()
        || phases
            .iter()
            .zip(view.dynamic_tags)
            .any(|(&phase, tag)| phase != if tag.self_closing { 2 } else { 4 })
    {
        return Err(TsrxParseError::Unsupported("incomplete parser dynamic boundary set"));
    }
    Ok(())
}

fn validate_try_clauses(view: OverlayView<'_>, node_index: usize) -> Result<(), TsrxParseError> {
    let node = view.nodes[node_index];
    let first_index = usize::try_from(node.first_clause)
        .map_err(|_| TsrxParseError::Unsupported("invalid try clause index"))?;
    let first =
        view.clauses.get(first_index).ok_or(TsrxParseError::Unsupported("missing try clause"))?;
    if first.role != ClauseRole::Try
        || first.keyword.start != node.span.start
        || !valid_plain_clause(*first, node.span)
    {
        return Err(TsrxParseError::Unsupported("malformed try clause"));
    }

    let mut clause_index = first.next;
    let mut last = node.first_clause;
    let mut saw_pending = false;
    let mut saw_catch = false;
    let mut previous_body_end = first.body.end;
    while clause_index != NONE_INDEX {
        let index = usize::try_from(clause_index)
            .map_err(|_| TsrxParseError::Unsupported("invalid try clause index"))?;
        let clause = view
            .clauses
            .get(index)
            .ok_or(TsrxParseError::Unsupported("invalid try clause index"))?;
        let valid = match clause.role {
            ClauseRole::Pending => {
                let unique_and_ordered = !saw_pending && !saw_catch;
                saw_pending = true;
                unique_and_ordered && valid_plain_clause(*clause, node.span)
            }
            ClauseRole::Catch => {
                let unique = !saw_catch;
                saw_catch = true;
                unique && valid_catch_clause(*clause, node.span)
            }
            _ => false,
        };
        if !valid || clause.keyword.start < previous_body_end {
            return Err(TsrxParseError::Unsupported("malformed try clause chain"));
        }
        last = clause_index;
        previous_body_end = clause.body.end;
        clause_index = clause.next;
    }
    if !saw_pending && !saw_catch || last != node.last_clause {
        return Err(TsrxParseError::Unsupported("incomplete try clause chain"));
    }
    let last_clause = view.clauses[usize::try_from(last)
        .map_err(|_| TsrxParseError::Unsupported("invalid try clause index"))?];
    if last_clause.body.end != node.span.end {
        return Err(TsrxParseError::Unsupported("try span does not end with its last clause"));
    }
    Ok(())
}

fn valid_plain_clause(clause: tsrx_syntax::OverlayClause, node_span: ByteSpan) -> bool {
    clause.header.is_empty()
        && !clause.body.is_empty()
        && clause.for_header == ForHeader::default()
        && clause.bindings == 0
        && clause.keyword.end == clause.keyword.start.saturating_add(1)
        && span_contains(node_span, clause.keyword)
        && span_contains(node_span, clause.body)
        && clause.keyword.end <= clause.body.start
}

fn valid_catch_clause(clause: tsrx_syntax::OverlayClause, node_span: ByteSpan) -> bool {
    let bindings_match_header = match clause.bindings {
        0 => clause.header.is_empty(),
        1 | 2 => {
            !clause.header.is_empty()
                && span_contains(node_span, clause.header)
                && clause.keyword.end <= clause.header.start
                && clause.header.end <= clause.body.start
        }
        _ => false,
    };
    bindings_match_header
        && !clause.body.is_empty()
        && clause.for_header == ForHeader::default()
        && clause.keyword.end == clause.keyword.start.saturating_add(1)
        && span_contains(node_span, clause.keyword)
        && span_contains(node_span, clause.body)
        && clause.keyword.end <= clause.body.start
}

fn validate_switch_clauses(view: OverlayView<'_>, node_index: usize) -> Result<(), TsrxParseError> {
    let node = view.nodes[node_index];
    let mut clause_index = node.first_clause;
    let mut last = NONE_INDEX;
    let mut saw_default = false;
    while clause_index != NONE_INDEX {
        let index = usize::try_from(clause_index)
            .map_err(|_| TsrxParseError::Unsupported("invalid switch clause index"))?;
        let clause = view
            .clauses
            .get(index)
            .ok_or(TsrxParseError::Unsupported("invalid switch clause index"))?;
        let valid = match clause.role {
            ClauseRole::Case => !clause.header.is_empty(),
            ClauseRole::Default => {
                let unique = !saw_default && clause.header.is_empty();
                saw_default = true;
                unique
            }
            _ => false,
        };
        if !valid
            || clause.body.is_empty()
            || clause.for_header != ForHeader::default()
            || clause.bindings != 0
        {
            return Err(TsrxParseError::Unsupported("malformed switch clause chain"));
        }
        last = clause_index;
        clause_index = clause.next;
    }
    if last != node.last_clause {
        return Err(TsrxParseError::Unsupported("incomplete switch clause chain"));
    }
    Ok(())
}

fn control_owner_matches(view: OverlayView<'_>, owner: u32, expected: ControlKind) -> bool {
    usize::try_from(owner)
        .ok()
        .and_then(|index| view.nodes.get(index))
        .is_some_and(|node| node.kind == expected)
}

fn mark_control_root(
    view: OverlayView<'_>,
    roots: &mut [bool],
    owner: u32,
    start: u32,
    expected: ControlKind,
) -> bool {
    let Some((node, seen)) = usize::try_from(owner)
        .ok()
        .and_then(|index| view.nodes.get(index).zip(roots.get_mut(index)))
    else {
        return false;
    };
    node.kind == expected && node.span.start == start && !std::mem::replace(seen, true)
}

fn validate_for_clauses(view: OverlayView<'_>, node_index: usize) -> Result<(), TsrxParseError> {
    let node = view.nodes[node_index];
    let first_index = usize::try_from(node.first_clause)
        .map_err(|_| TsrxParseError::Unsupported("invalid for clause index"))?;
    let first =
        view.clauses.get(first_index).ok_or(TsrxParseError::Unsupported("missing for clause"))?;
    if first.role != ClauseRole::For || first.body.is_empty() {
        return Err(TsrxParseError::Unsupported("malformed for clause"));
    }
    let header = first.for_header;
    if header.annotated {
        if header.left.is_empty()
            || header.right.is_empty()
            || !span_contains(first.header, header.left)
            || !span_contains(first.header, header.right)
            || !header.index.is_empty() && !span_contains(first.header, header.index)
            || !header.key.is_empty() && !span_contains(first.header, header.key)
        {
            return Err(TsrxParseError::Unsupported("malformed annotated for header"));
        }
    } else if !header.left.is_empty()
        || !header.right.is_empty()
        || !header.index.is_empty()
        || !header.key.is_empty()
    {
        return Err(TsrxParseError::Unsupported("unannotated for header carries annotations"));
    }

    let mut last = node.first_clause;
    if first.next != NONE_INDEX {
        let empty_index = usize::try_from(first.next)
            .map_err(|_| TsrxParseError::Unsupported("invalid empty clause index"))?;
        let empty = view
            .clauses
            .get(empty_index)
            .ok_or(TsrxParseError::Unsupported("missing empty clause"))?;
        if empty.role != ClauseRole::Empty
            || empty.next != NONE_INDEX
            || !empty.header.is_empty()
            || empty.body.is_empty()
            || empty.for_header != ForHeader::default()
        {
            return Err(TsrxParseError::Unsupported("malformed empty clause"));
        }
        last = first.next;
    }
    if last != node.last_clause {
        return Err(TsrxParseError::Unsupported("incomplete for clause chain"));
    }
    Ok(())
}

fn span_contains(outer: ByteSpan, inner: ByteSpan) -> bool {
    outer.start <= inner.start && inner.start <= inner.end && inner.end <= outer.end
}

fn validate_if_clauses(view: OverlayView<'_>, node_index: usize) -> Result<(), TsrxParseError> {
    let node = view.nodes[node_index];
    let mut clause_index = node.first_clause;
    let mut ordinal = 0_usize;
    let mut saw_else = false;
    let mut last = NONE_INDEX;
    while clause_index != NONE_INDEX {
        let index = usize::try_from(clause_index)
            .map_err(|_| TsrxParseError::Unsupported("invalid if clause index"))?;
        let clause = view
            .clauses
            .get(index)
            .ok_or(TsrxParseError::Unsupported("invalid if clause index"))?;
        let valid_role = match clause.role {
            ClauseRole::If => ordinal == 0,
            ClauseRole::ElseIf => ordinal != 0 && !saw_else,
            ClauseRole::Else => {
                let valid = ordinal != 0 && !saw_else;
                saw_else = true;
                valid
            }
            _ => false,
        };
        if !valid_role || saw_else && clause.next != NONE_INDEX {
            return Err(TsrxParseError::Unsupported("malformed if clause chain"));
        }
        last = clause_index;
        clause_index = clause.next;
        ordinal += 1;
    }
    if ordinal == 0 || last != node.last_clause {
        return Err(TsrxParseError::Unsupported("incomplete if clause chain"));
    }
    Ok(())
}

pub(super) fn validate_projection(
    source: &str,
    view: ProjectionView<'_>,
    overlay: OverlayView<'_>,
) -> Result<(), TsrxParseError> {
    if view.segments.is_empty() {
        return Err(TsrxParseError::Unsupported("projection has no affine source"));
    }
    let source_len = u32::try_from(source.len())
        .map_err(|_| TsrxParseError::Unsupported("source above 4 GiB"))?;
    let projected_len = u32::try_from(view.source.len())
        .map_err(|_| TsrxParseError::Unsupported("projection above 4 GiB"))?;
    let allowed_gaps = build_allowed_gaps(source, overlay, source_len)?;
    let mut gap_index = 0_usize;
    let mut original_cursor = 0_u32;
    let mut projected_cursor = 0_u32;
    for segment in view.segments {
        let length = segment
            .projected
            .end
            .checked_sub(segment.projected.start)
            .ok_or(TsrxParseError::Unsupported("reversed projection segment"))?;
        let original_end = segment
            .original_start
            .checked_add(length)
            .ok_or(TsrxParseError::Unsupported("projection span overflow"))?;
        if segment.projected.start < projected_cursor
            || segment.projected.end > projected_len
            || segment.original_start < original_cursor
            || original_end > source_len
            || !segment.fixable
            || !consume_allowed_gap(
                original_cursor,
                segment.original_start,
                &allowed_gaps,
                &mut gap_index,
            )
        {
            return Err(TsrxParseError::Unsupported("non-canonical affine projection map"));
        }
        let projected = slice(view.source, segment.projected.start, segment.projected.end)?;
        let authored = slice(source, segment.original_start, original_end)?;
        if projected != authored {
            return Err(TsrxParseError::Unsupported(
                "affine projection bytes differ from authored source",
            ));
        }
        projected_cursor = segment.projected.end;
        original_cursor = original_end;
    }
    if !consume_allowed_gap(original_cursor, source_len, &allowed_gaps, &mut gap_index)
        || gap_index != allowed_gaps.len()
    {
        return Err(TsrxParseError::Unsupported(
            "projection omitted non-structural authored bytes",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn build_allowed_gaps(
    source: &str,
    overlay: OverlayView<'_>,
    source_len: u32,
) -> Result<Vec<ByteSpan>, TsrxParseError> {
    let mut token_gaps = Vec::with_capacity(overlay.tokens.len());
    for token in overlay.tokens {
        let omitted_length = match token.kind {
            StructuralKind::Try => 4,
            StructuralKind::Empty | StructuralKind::Catch => 6,
            StructuralKind::Pending => 8,
            _ => 1,
        };
        let end = token
            .span
            .start
            .checked_add(omitted_length)
            .ok_or(TsrxParseError::Unsupported("structural token span overflow"))?;
        if token.span.start >= end || end > source_len {
            return Err(TsrxParseError::Unsupported("invalid structural token gap"));
        }
        token_gaps.push(ByteSpan::new(token.span.start, end));
    }
    let mut header_gaps = Vec::with_capacity(overlay.clauses.len().saturating_mul(2));
    for node in overlay.nodes {
        let mut clause_index = node.first_clause;
        while clause_index != NONE_INDEX {
            let clause = usize::try_from(clause_index)
                .ok()
                .and_then(|index| overlay.clauses.get(index))
                .ok_or(TsrxParseError::Unsupported("header gap has no owning clause"))?;
            let header = clause.for_header;
            if header.annotated {
                let mut cursor = clause.header.start;
                for span in [header.left, header.right, header.index, header.key] {
                    if span.is_empty() {
                        continue;
                    }
                    if span.start < cursor || clause.header.end < span.end {
                        return Err(TsrxParseError::Unsupported(
                            "annotated for values are out of source order",
                        ));
                    }
                    if cursor < span.start {
                        header_gaps.push(ByteSpan::new(cursor, span.start));
                    }
                    cursor = span.end;
                }
                if cursor < clause.header.end {
                    header_gaps.push(ByteSpan::new(cursor, clause.header.end));
                }
            }
            clause_index = clause.next;
        }
    }
    let mut dynamic_gaps = Vec::with_capacity(if overlay.parser_dynamic.is_empty() {
        overlay.embedded.len().saturating_mul(2)
    } else {
        overlay.parser_dynamic.len()
    });
    if overlay.parser_dynamic.is_empty() {
        for token in overlay.embedded {
            let expression = match token.kind {
                tsrx_syntax::EmbeddedKind::DynamicOpen
                | tsrx_syntax::EmbeddedKind::DynamicClose => {
                    let tag = usize::try_from(token.owner)
                        .ok()
                        .and_then(|index| overlay.dynamic_tags.get(index))
                        .ok_or(TsrxParseError::Unsupported("dynamic gap has no owner"))?;
                    if token.kind == tsrx_syntax::EmbeddedKind::DynamicOpen {
                        tag.expression
                    } else {
                        tag.closing_expression
                    }
                }
                tsrx_syntax::EmbeddedKind::StyleContent => continue,
            };
            add_dynamic_gaps(source, source_len, token.span, expression, &mut dynamic_gaps)?;
        }
    } else {
        for token in overlay.parser_dynamic {
            add_parser_dynamic_gap(source, source_len, overlay, *token, &mut dynamic_gaps)?;
        }
    }

    // Style owners are preorder identities, while payload tokens are naturally emitted in source
    // order (including styles nested in an opening attribute). Keep a separate stream so the
    // fixed-way merge remains linear without sorting either table.
    let mut style_gaps = Vec::with_capacity(overlay.style_blocks.len());
    for token in overlay
        .embedded
        .iter()
        .filter(|token| token.kind == tsrx_syntax::EmbeddedKind::StyleContent)
    {
        let style = usize::try_from(token.owner)
            .ok()
            .and_then(|index| overlay.style_blocks.get(index))
            .ok_or(TsrxParseError::Unsupported("style gap has no owner"))?;
        validate_style_source(source, source_len, *style)?;
        if style.content != token.span {
            return Err(TsrxParseError::Unsupported("style gap differs from its payload token"));
        }
        if !style.content.is_empty() {
            style_gaps.push(style.content);
        }
    }
    for style in overlay.style_blocks.iter().filter(|style| style.self_closing) {
        validate_style_source(source, source_len, *style)?;
    }

    let streams = [
        token_gaps.as_slice(),
        header_gaps.as_slice(),
        dynamic_gaps.as_slice(),
        style_gaps.as_slice(),
    ];
    let mut cursors = [0_usize; 4];
    let total = streams.iter().map(|stream| stream.len()).sum();
    let mut merged = Vec::with_capacity(total);
    loop {
        let mut selected = None;
        for (stream_index, stream) in streams.iter().enumerate() {
            let Some(gap) = stream.get(cursors[stream_index]) else {
                continue;
            };
            if selected.is_none_or(|(_, current): (usize, ByteSpan)| gap.start < current.start) {
                selected = Some((stream_index, *gap));
            }
        }
        let Some((stream_index, gap)) = selected else {
            break;
        };
        cursors[stream_index] += 1;
        push_merged_gap(&mut merged, gap)?;
    }
    Ok(merged)
}

fn validate_style_source(
    source: &str,
    source_len: u32,
    style: tsrx_syntax::OverlayStyleBlock,
) -> Result<(), TsrxParseError> {
    if style.element.end > source_len || style.content.end > source_len {
        return Err(TsrxParseError::Unsupported("style span lies outside authored source"));
    }
    let opening_end = if style.self_closing { style.element.end } else { style.content.start };
    let opening = slice(source, style.element.start, opening_end)?;
    let boundary = opening.as_bytes().get("<style".len()).copied();
    if !opening.starts_with("<style")
        || !boundary.is_some_and(|byte| byte.is_ascii_whitespace() || matches!(byte, b'>' | b'/'))
        || if style.self_closing {
            !opening.ends_with("/>")
        } else {
            !opening.ends_with('>')
                || slice(source, style.content.end, style.element.end)? != "</style>"
        }
    {
        return Err(TsrxParseError::Unsupported("style source boundary is not canonical"));
    }
    Ok(())
}

fn push_merged_gap(merged: &mut Vec<ByteSpan>, gap: ByteSpan) -> Result<(), TsrxParseError> {
    if let Some(previous) = merged.last_mut() {
        if gap.start < previous.end {
            return Err(TsrxParseError::Unsupported("overlapping structural projection gaps"));
        }
        if gap.start == previous.end {
            previous.end = gap.end;
            return Ok(());
        }
    }
    merged.push(gap);
    Ok(())
}

fn add_parser_dynamic_gap(
    source: &str,
    source_len: u32,
    overlay: OverlayView<'_>,
    token: tsrx_syntax::ParserDynamicToken,
    gaps: &mut Vec<ByteSpan>,
) -> Result<(), TsrxParseError> {
    let tag = usize::try_from(token.owner)
        .ok()
        .and_then(|index| overlay.dynamic_tags.get(index))
        .ok_or(TsrxParseError::Unsupported("parser dynamic gap has no owner"))?;
    let (gap, expected) = match token.kind {
        ParserDynamicKind::OpenStart => {
            (ByteSpan::new(tag.opening.start, tag.expression.start), b"<{".as_slice())
        }
        ParserDynamicKind::OpenEnd => {
            (ByteSpan::new(tag.expression.end, tag.opening.end), b"}".as_slice())
        }
        ParserDynamicKind::CloseStart => {
            (ByteSpan::new(tag.closing.start, tag.closing_expression.start), b"</{".as_slice())
        }
        ParserDynamicKind::CloseEnd => {
            let gap = ByteSpan::new(tag.closing_expression.end, tag.closing.end);
            if gap.start >= gap.end || gap.end > source_len {
                return Err(TsrxParseError::Unsupported("invalid parser dynamic closing gap"));
            }
            let suffix = source.as_bytes().get(gap.start as usize..gap.end as usize).ok_or(
                TsrxParseError::Unsupported("parser dynamic closing gap lies outside source"),
            )?;
            if suffix.first() != Some(&b'}')
                || suffix.last() != Some(&b'>')
                || !suffix[1..suffix.len() - 1].iter().all(u8::is_ascii_whitespace)
            {
                return Err(TsrxParseError::AuthoredGrammar(
                    "parser dynamic closing suffix is malformed".to_string(),
                ));
            }
            gaps.push(gap);
            return Ok(());
        }
    };
    if gap.start >= gap.end
        || gap.end > source_len
        || source.as_bytes().get(gap.start as usize..gap.end as usize) != Some(expected)
    {
        return Err(TsrxParseError::Unsupported("parser dynamic boundary gap is malformed"));
    }
    gaps.push(gap);
    Ok(())
}

fn add_dynamic_gaps(
    source: &str,
    source_len: u32,
    syntax: ByteSpan,
    expression: ByteSpan,
    gaps: &mut Vec<ByteSpan>,
) -> Result<(), TsrxParseError> {
    if syntax.start >= expression.start
        || expression.start >= expression.end
        || expression.end >= syntax.end
        || syntax.end > source_len
    {
        return Err(TsrxParseError::Unsupported("invalid dynamic projection gap"));
    }
    let suffix_start = usize::try_from(expression.end)
        .map_err(|_| TsrxParseError::Unsupported("dynamic suffix overflow"))?;
    let suffix_end = usize::try_from(syntax.end)
        .map_err(|_| TsrxParseError::Unsupported("dynamic suffix overflow"))?;
    let suffix = source
        .as_bytes()
        .get(suffix_start..suffix_end)
        .ok_or(TsrxParseError::Unsupported("dynamic suffix lies outside source"))?;
    if suffix.first() != Some(&b'}') {
        return Err(TsrxParseError::Unsupported("dynamic suffix has no closing brace"));
    }
    if source.as_bytes().get(syntax.start as usize..expression.start as usize)
        != Some(if syntax.start.saturating_add(2) == expression.start {
            b"<{".as_slice()
        } else {
            b"</{".as_slice()
        })
        || suffix.len() > 1
            && (suffix.last() != Some(&b'>')
                || !suffix[1..suffix.len() - 1].iter().all(u8::is_ascii_whitespace))
    {
        return Err(TsrxParseError::Unsupported("dynamic syntax contains unsupported trivia"));
    }
    gaps.push(ByteSpan::new(syntax.start, expression.start));
    gaps.push(ByteSpan::new(expression.end, syntax.end));
    Ok(())
}

fn consume_allowed_gap(start: u32, end: u32, allowed: &[ByteSpan], index: &mut usize) -> bool {
    let mut cursor = start;
    while cursor < end {
        let Some(gap) = allowed.get(*index) else {
            return false;
        };
        if gap.start != cursor || gap.end <= cursor || gap.end > end {
            return false;
        }
        cursor = gap.end;
        *index += 1;
    }
    cursor == end
}

struct MarkerValidation {
    token_markers: Vec<bool>,
    style_markers: Vec<bool>,
    wrapper_starts: Vec<bool>,
    wrapper_ends: Vec<bool>,
    header_markers: Vec<u8>,
    annotated_clauses: Vec<usize>,
}

impl MarkerValidation {
    fn new(overlay: OverlayView<'_>) -> Result<Self, TsrxParseError> {
        let annotated_clauses = ordered_annotated_clauses(overlay)?;
        Ok(Self {
            token_markers: vec![false; overlay.tokens.len()],
            style_markers: vec![false; overlay.style_blocks.len()],
            wrapper_starts: vec![false; overlay.nodes.len()],
            wrapper_ends: vec![false; overlay.nodes.len()],
            header_markers: vec![0_u8; annotated_clauses.len()],
            annotated_clauses,
        })
    }

    fn record(
        &mut self,
        marker: MarkerKind,
        comment: &CommentRecord,
        authored: &str,
        projected: &str,
        segments: &[ProjectionSegment],
        overlay: OverlayView<'_>,
    ) -> Result<(), TsrxParseError> {
        match marker {
            MarkerKind::Token(index) => {
                self.record_token(index, comment, authored, projected, segments, overlay)
            }
            MarkerKind::Style(index) => {
                self.record_style(index, comment, projected, segments, overlay)
            }
            MarkerKind::WrapperStart(index) => self.record_wrapper(index, true, overlay),
            MarkerKind::WrapperEnd(index) => self.record_wrapper(index, false, overlay),
            MarkerKind::Header { ordinal, part, boundary } => {
                self.record_header(ordinal, part, boundary, comment, segments, overlay)
            }
        }
    }

    fn record_style(
        &mut self,
        raw: u32,
        comment: &CommentRecord,
        projected: &str,
        segments: &[ProjectionSegment],
        overlay: OverlayView<'_>,
    ) -> Result<(), TsrxParseError> {
        let index = usize::try_from(raw)
            .map_err(|_| TsrxParseError::Unsupported("style marker index overflow"))?;
        let style = overlay
            .style_blocks
            .get(index)
            .ok_or(TsrxParseError::Unsupported("unknown style marker"))?;
        if style.self_closing {
            return Err(TsrxParseError::Unsupported("self-closing style has a payload marker"));
        }
        let scaffold_start = project_authored_end(segments, style.content.start)
            .ok_or(TsrxParseError::Unsupported("unmapped style marker start"))?;
        let scaffold_end = project_authored_start(segments, style.content.end)
            .ok_or(TsrxParseError::Unsupported("unmapped style marker end"))?;
        let positioned = comment.span.start == scaffold_start.saturating_add(1)
            && slice(projected, scaffold_start, comment.span.start)? == "{"
            && slice(projected, comment.span.end, scaffold_end)? == " null}";
        let seen = self
            .style_markers
            .get_mut(index)
            .ok_or(TsrxParseError::Unsupported("unknown style marker"))?;
        if std::mem::replace(seen, true) || !positioned {
            return Err(TsrxParseError::Unsupported("duplicated or displaced style marker"));
        }
        Ok(())
    }

    fn record_token(
        &mut self,
        raw: u32,
        comment: &CommentRecord,
        authored: &str,
        projected: &str,
        segments: &[ProjectionSegment],
        overlay: OverlayView<'_>,
    ) -> Result<(), TsrxParseError> {
        let index = usize::try_from(raw)
            .map_err(|_| TsrxParseError::Unsupported("marker index overflow"))?;
        let token =
            overlay.tokens.get(index).ok_or(TsrxParseError::Unsupported("unknown token marker"))?;
        let positioned = match token.kind {
            StructuralKind::Empty => {
                let body_start = empty_clause_for_owner(overlay, token.owner)?.body.start;
                let projected_body = project_authored_start(segments, body_start)
                    .ok_or(TsrxParseError::Unsupported("unmapped empty marker"))?;
                let trivia_start = token.span.start.saturating_add(6);
                let trivia = slice(authored, trivia_start, body_start)?;
                comment.span.end <= projected_body
                    && slice(projected, comment.span.end, projected_body)?
                        .strip_prefix("if (false)")
                        == Some(trivia)
            }
            StructuralKind::Try | StructuralKind::Pending | StructuralKind::Catch => {
                try_marker_positioned(*token, comment, authored, projected, segments, overlay)?
            }
            StructuralKind::FunctionBody => {
                let projected_start = project_authored_start(segments, token.span.end)
                    .ok_or(TsrxParseError::Unsupported("unmapped code-block marker"))?;
                if overlay
                    .parser_code_blocks
                    .binary_search_by_key(&(raw), |block| block.token)
                    .is_ok()
                {
                    let text = slice(projected, comment.span.start, comment.span.end)?;
                    let (marker_prefix, _) = parse_marker(text)
                        .ok_or(TsrxParseError::Unsupported("invalid JSX code-block marker"))?;
                    let expected = format!("{{(async function*{marker_prefix}J{raw}_(){{");
                    slice(projected, projected_start, comment.span.start)? == expected
                } else {
                    comment.span.start == projected_start.saturating_add(1)
                        && slice(projected, projected_start, comment.span.start)? == "{"
                }
            }
            _ => {
                let projected_start = project_authored_start(segments, token.span.end)
                    .ok_or(TsrxParseError::Unsupported("unmapped token marker"))?;
                comment.span.end == projected_start
            }
        };
        if self.token_markers[index] || !positioned {
            return Err(TsrxParseError::Unsupported("duplicated or displaced token marker"));
        }
        self.token_markers[index] = true;
        Ok(())
    }

    fn record_wrapper(
        &mut self,
        raw: u32,
        start: bool,
        overlay: OverlayView<'_>,
    ) -> Result<(), TsrxParseError> {
        let index = usize::try_from(raw)
            .map_err(|_| TsrxParseError::Unsupported("wrapper index overflow"))?;
        let node = overlay
            .nodes
            .get(index)
            .ok_or(TsrxParseError::Unsupported("unknown wrapper marker"))?;
        if node.context == ControlContext::Statement {
            return Err(TsrxParseError::Unsupported("statement control has a synthetic wrapper"));
        }
        let seen =
            if start { &mut self.wrapper_starts[index] } else { &mut self.wrapper_ends[index] };
        if *seen {
            return Err(TsrxParseError::Unsupported("duplicated wrapper marker"));
        }
        *seen = true;
        Ok(())
    }

    fn record_header(
        &mut self,
        ordinal: u32,
        part: HeaderPart,
        boundary: MarkerBoundary,
        comment: &CommentRecord,
        segments: &[ProjectionSegment],
        overlay: OverlayView<'_>,
    ) -> Result<(), TsrxParseError> {
        let index = usize::try_from(ordinal)
            .map_err(|_| TsrxParseError::Unsupported("header index overflow"))?;
        let clause = self
            .annotated_clauses
            .get(index)
            .and_then(|clause| overlay.clauses.get(*clause))
            .ok_or(TsrxParseError::Unsupported("unknown header marker"))?;
        let authored_span = match part {
            HeaderPart::Right => clause.for_header.right,
            HeaderPart::Index => clause.for_header.index,
            HeaderPart::Key => clause.for_header.key,
        };
        if authored_span.is_empty() {
            return Err(TsrxParseError::Unsupported("marker for absent header value"));
        }
        let projected_start = project_authored_start(segments, authored_span.start)
            .ok_or(TsrxParseError::Unsupported("unmapped header marker"))?;
        let projected_end = project_authored_end(segments, authored_span.end)
            .ok_or(TsrxParseError::Unsupported("unmapped header marker"))?;
        let positioned = match boundary {
            MarkerBoundary::Start => comment.span.end == projected_start,
            MarkerBoundary::End => comment.span.start == projected_end,
        };
        let bit = header_marker_bit(part, boundary);
        let seen = self
            .header_markers
            .get_mut(index)
            .ok_or(TsrxParseError::Unsupported("unknown header marker"))?;
        if !positioned || *seen & bit != 0 {
            return Err(TsrxParseError::Unsupported("duplicated or displaced header marker"));
        }
        *seen |= bit;
        Ok(())
    }

    fn is_complete(&self, overlay: OverlayView<'_>) -> bool {
        self.token_markers.iter().all(|seen| *seen)
            && self
                .style_markers
                .iter()
                .zip(overlay.style_blocks)
                .all(|(seen, style)| *seen != style.self_closing)
            && overlay.nodes.iter().enumerate().all(|(index, node)| {
                let expected = node.context != ControlContext::Statement;
                self.wrapper_starts[index] == expected && self.wrapper_ends[index] == expected
            })
            && self
                .annotated_clauses
                .iter()
                .filter_map(|index| overlay.clauses.get(*index))
                .zip(self.header_markers.iter().copied())
                .all(|(clause, seen)| seen == expected_header_markers(clause.for_header))
    }
}

fn try_marker_positioned(
    token: tsrx_syntax::OverlayToken,
    comment: &CommentRecord,
    authored: &str,
    projected: &str,
    segments: &[ProjectionSegment],
    overlay: OverlayView<'_>,
) -> Result<bool, TsrxParseError> {
    let clause = try_clause_for_token(overlay, token)?;
    let projected_body = project_authored_start(segments, clause.body.start)
        .ok_or(TsrxParseError::Unsupported("unmapped try-family marker"))?;
    if comment.span.end > projected_body {
        return Ok(false);
    }
    let marker = slice(projected, comment.span.start, comment.span.end)?;
    let prefix = parse_marker(marker)
        .map(|(prefix, _)| prefix)
        .ok_or(TsrxParseError::Unsupported("invalid try-family marker"))?;
    let keyword_length = match token.kind {
        StructuralKind::Try => 4,
        StructuralKind::Pending => 8,
        StructuralKind::Catch => 6,
        _ => return Ok(false),
    };
    let trivia_start = token
        .span
        .start
        .checked_add(keyword_length)
        .ok_or(TsrxParseError::Unsupported("try-family token overflow"))?;
    let authored_tail = slice(authored, trivia_start, clause.body.start)?;
    let projected_tail = slice(projected, comment.span.end, projected_body)?;
    let scaffold_matches = match token.kind {
        StructuralKind::Try => {
            strip_scaffold_name(projected_tail, prefix, 'T', token.owner)
                .and_then(|tail| tail.strip_prefix("({async *"))
                .and_then(|tail| strip_scaffold_name(tail, prefix, 'B', token.owner))
                .and_then(|tail| tail.strip_prefix("()"))
                == Some(authored_tail)
        }
        StructuralKind::Pending => {
            synthetic_comma_precedes(projected, comment.span.start)?
                && projected_tail
                    .strip_prefix("async *")
                    .and_then(|tail| strip_scaffold_name(tail, prefix, 'P', token.owner))
                    .and_then(|tail| tail.strip_prefix("()"))
                    == Some(authored_tail)
        }
        StructuralKind::Catch => {
            let tail = projected_tail
                .strip_prefix("async *")
                .and_then(|tail| strip_scaffold_name(tail, prefix, 'C', token.owner));
            let tail = if clause.header.is_empty() {
                tail.and_then(|tail| tail.strip_prefix("()"))
            } else {
                tail
            };
            synthetic_comma_precedes(projected, comment.span.start)? && tail == Some(authored_tail)
        }
        _ => false,
    };
    Ok(scaffold_matches)
}

fn strip_scaffold_name<'a>(
    value: &'a str,
    prefix: &str,
    marker: char,
    owner: u32,
) -> Option<&'a str> {
    let tail = value.strip_prefix(prefix)?.strip_prefix(marker)?;
    let digit_end = tail.bytes().take_while(u8::is_ascii_digit).count();
    if parse_decimal(tail.get(..digit_end)?) != Some(owner) {
        return None;
    }
    tail.get(digit_end..)?.strip_prefix('_')
}

fn synthetic_comma_precedes(projected: &str, point: u32) -> Result<bool, TsrxParseError> {
    let start = point
        .checked_sub(1)
        .ok_or(TsrxParseError::Unsupported("try-family marker at projection start"))?;
    Ok(slice(projected, start, point)? == ",")
}

fn try_clause_for_token(
    overlay: OverlayView<'_>,
    token: tsrx_syntax::OverlayToken,
) -> Result<tsrx_syntax::OverlayClause, TsrxParseError> {
    let expected_role = match token.kind {
        StructuralKind::Try => ClauseRole::Try,
        StructuralKind::Pending => ClauseRole::Pending,
        StructuralKind::Catch => ClauseRole::Catch,
        _ => {
            return Err(TsrxParseError::Unsupported("non-try token requested a try clause"));
        }
    };
    let node = usize::try_from(token.owner)
        .ok()
        .and_then(|index| overlay.nodes.get(index))
        .filter(|node| node.kind == ControlKind::Try)
        .ok_or(TsrxParseError::Unsupported("try-family token has no try owner"))?;
    let mut clause_index = node.first_clause;
    let mut found = None;
    while clause_index != NONE_INDEX {
        let clause = usize::try_from(clause_index)
            .ok()
            .and_then(|index| overlay.clauses.get(index))
            .ok_or(TsrxParseError::Unsupported("invalid try clause index"))?;
        if clause.role == expected_role && found.replace(*clause).is_some() {
            return Err(TsrxParseError::Unsupported("duplicated try-family clause role"));
        }
        clause_index = clause.next;
    }
    found
        .filter(|clause| clause.keyword == token.span)
        .ok_or(TsrxParseError::Unsupported("try-family token does not match its clause"))
}

pub(super) fn reconstruct_comments<'a>(
    authored: &str,
    projected: &'a str,
    segments: &[ProjectionSegment],
    mut comments: CommentTable,
    overlay: OverlayView<'_>,
    expected_prefix: Option<&'a str>,
    require_complete_markers: bool,
) -> Result<(Option<&'a str>, CommentTable), TsrxParseError> {
    let mut markers = MarkerValidation::new(overlay)?;
    let mut prefix = expected_prefix;
    let mut authored_comments = CommentTable::default();
    let projected_records = comments.take_records();
    let projected_strings = comments.take_string_storage()?;
    debug_assert!(comments.is_storage_released());
    drop(comments);
    for comment in projected_records {
        if let Some(mapped_span) = map_affine_span(segments, comment.span) {
            let source = slice(authored, mapped_span.start, mapped_span.end)?;
            let projected_source = slice(projected, comment.span.start, comment.span.end)?;
            let kind_matches = match comment.kind {
                ProjectedCommentKind::Line => source.starts_with("//"),
                ProjectedCommentKind::Block => source.starts_with("/*"),
            };
            let value = match comment.kind {
                ProjectedCommentKind::Line => source.strip_prefix("//"),
                ProjectedCommentKind::Block => {
                    source.strip_prefix("/*").and_then(|value| value.strip_suffix("*/"))
                }
            };
            if source != projected_source
                || !kind_matches
                || value != packed_string(&projected_strings, comment.value)
            {
                return Err(TsrxParseError::Unsupported(
                    "authored comment differs from its affine projection",
                ));
            }
            authored_comments.push(
                comment.kind,
                mapped_span,
                value.ok_or(TsrxParseError::Unsupported(
                    "authored comment delimiters are malformed",
                ))?,
            )?;
            continue;
        }
        if !require_complete_markers {
            if comment.kind != ProjectedCommentKind::Block {
                return Err(TsrxParseError::Unsupported("unknown non-block projection comment"));
            }
            continue;
        }
        if comment.kind != ProjectedCommentKind::Block {
            return Err(TsrxParseError::Unsupported("unknown non-block projection comment"));
        }
        let text = slice(projected, comment.span.start, comment.span.end)?;
        let (comment_prefix, marker) =
            parse_marker(text).ok_or(TsrxParseError::Unsupported("unknown projected comment"))?;
        if prefix.replace(comment_prefix).is_some_and(|seen| seen != comment_prefix) {
            return Err(TsrxParseError::Unsupported("mixed projection marker namespaces"));
        }
        markers.record(marker, &comment, authored, projected, segments, overlay)?;
    }
    drop(projected_strings);
    if require_complete_markers && !markers.is_complete(overlay) {
        return Err(TsrxParseError::Unsupported("incomplete projection marker set"));
    }
    if require_complete_markers {
        let marker_prefix =
            prefix.ok_or(TsrxParseError::Unsupported("missing marker namespace"))?;
        if authored.contains(marker_prefix) {
            return Err(TsrxParseError::Unsupported(
                "projection namespace collides with authored source",
            ));
        }
    }
    Ok((prefix, authored_comments))
}

fn ordered_annotated_clauses(overlay: OverlayView<'_>) -> Result<Vec<usize>, TsrxParseError> {
    let mut ordered = Vec::new();
    for node in overlay.nodes {
        let mut clause_index = node.first_clause;
        while clause_index != NONE_INDEX {
            let index = usize::try_from(clause_index)
                .map_err(|_| TsrxParseError::Unsupported("invalid clause index"))?;
            let clause = overlay
                .clauses
                .get(index)
                .ok_or(TsrxParseError::Unsupported("invalid clause index"))?;
            if clause.for_header.annotated {
                ordered.push(index);
            }
            clause_index = clause.next;
        }
    }
    Ok(ordered)
}

fn empty_clause_for_owner(
    overlay: OverlayView<'_>,
    owner: u32,
) -> Result<tsrx_syntax::OverlayClause, TsrxParseError> {
    let node = usize::try_from(owner)
        .ok()
        .and_then(|index| overlay.nodes.get(index))
        .ok_or(TsrxParseError::Unsupported("empty token has no owner"))?;
    let first = usize::try_from(node.first_clause)
        .ok()
        .and_then(|index| overlay.clauses.get(index))
        .ok_or(TsrxParseError::Unsupported("empty token owner has no clause"))?;
    usize::try_from(first.next)
        .ok()
        .and_then(|index| overlay.clauses.get(index))
        .copied()
        .filter(|clause| clause.role == ClauseRole::Empty)
        .ok_or(TsrxParseError::Unsupported("empty token has no empty clause"))
}

fn parse_marker(comment: &str) -> Option<(&str, MarkerKind)> {
    let body = comment.strip_prefix("/*")?.strip_suffix("*/")?;
    let nonce_tail = body.strip_prefix("_t")?;
    let nonce_length = nonce_tail.bytes().take_while(u8::is_ascii_hexdigit).count();
    if nonce_length == 0 || nonce_tail.as_bytes().get(nonce_length) != Some(&b'_') {
        return None;
    }
    let prefix_length = 2 + nonce_length + 1;
    let prefix = body.get(..prefix_length)?;
    let marker = body.get(prefix_length..)?;
    if let Some(wrapper) = marker.strip_prefix('N') {
        if let Some(index) = wrapper.strip_suffix("S__").and_then(parse_decimal) {
            return Some((prefix, MarkerKind::WrapperStart(index)));
        }
        if let Some(index) = wrapper.strip_suffix("E__").and_then(parse_decimal) {
            return Some((prefix, MarkerKind::WrapperEnd(index)));
        }
        return None;
    }
    if let Some(index) =
        marker.strip_prefix('S').and_then(|tail| tail.strip_suffix("__")).and_then(parse_decimal)
    {
        return Some((prefix, MarkerKind::Style(index)));
    }
    if let Some((&part, tail)) = marker.as_bytes().split_first()
        && matches!(part, b'R' | b'I' | b'K')
    {
        let (part, tail) = (
            match part {
                b'R' => HeaderPart::Right,
                b'I' => HeaderPart::Index,
                b'K' => HeaderPart::Key,
                _ => unreachable!(),
            },
            std::str::from_utf8(tail).ok()?,
        );
        if let Some(index) = tail.strip_suffix("S__").and_then(parse_decimal) {
            return Some((
                prefix,
                MarkerKind::Header { ordinal: index, part, boundary: MarkerBoundary::Start },
            ));
        }
        if let Some(index) = tail.strip_suffix("E__").and_then(parse_decimal) {
            return Some((
                prefix,
                MarkerKind::Header { ordinal: index, part, boundary: MarkerBoundary::End },
            ));
        }
        return None;
    }
    parse_decimal(marker).map(|index| (prefix, MarkerKind::Token(index)))
}

fn header_marker_bit(part: HeaderPart, boundary: MarkerBoundary) -> u8 {
    let offset = match part {
        HeaderPart::Right => 0,
        HeaderPart::Index => 2,
        HeaderPart::Key => 4,
    };
    1 << (offset
        + match boundary {
            MarkerBoundary::Start => 0,
            MarkerBoundary::End => 1,
        })
}

fn expected_header_markers(header: tsrx_syntax::ForHeader) -> u8 {
    let mut expected = header_marker_bit(HeaderPart::Right, MarkerBoundary::Start)
        | header_marker_bit(HeaderPart::Right, MarkerBoundary::End);
    if !header.index.is_empty() {
        expected |= header_marker_bit(HeaderPart::Index, MarkerBoundary::Start)
            | header_marker_bit(HeaderPart::Index, MarkerBoundary::End);
    }
    if !header.key.is_empty() {
        expected |= header_marker_bit(HeaderPart::Key, MarkerBoundary::Start)
            | header_marker_bit(HeaderPart::Key, MarkerBoundary::End);
    }
    expected
}

fn parse_decimal(value: &str) -> Option<u32> {
    if value.is_empty() {
        return None;
    }
    value.bytes().try_fold(0_u32, |number, byte| {
        byte.is_ascii_digit()
            .then_some(u32::from(byte - b'0'))
            .and_then(|digit| number.checked_mul(10)?.checked_add(digit))
    })
}

fn slice(source: &str, start: u32, end: u32) -> Result<&str, TsrxParseError> {
    let start = usize::try_from(start)
        .map_err(|_| TsrxParseError::Unsupported("span start exceeds host usize"))?;
    let end = usize::try_from(end)
        .map_err(|_| TsrxParseError::Unsupported("span end exceeds host usize"))?;
    source.get(start..end).ok_or(TsrxParseError::Unsupported("span is not a source boundary"))
}

fn packed_string(source: &str, range: StringRange) -> Option<&str> {
    let start = usize::try_from(range.start).ok()?;
    let length = usize::try_from(range.length).ok()?;
    source.get(start..start.checked_add(length)?)
}

pub(super) fn map_endpoint(
    segments: &[ProjectionSegment],
    point: u32,
    is_start: bool,
) -> Option<u32> {
    let index = segments.partition_point(|segment| {
        if is_start { segment.projected.start <= point } else { segment.projected.start < point }
    });
    let segment = segments.get(index.checked_sub(1)?)?;
    let contains = if is_start {
        segment.projected.start <= point && point < segment.projected.end
    } else {
        segment.projected.start < point && point <= segment.projected.end
    };
    contains.then(|| segment.original_start + (point - segment.projected.start))
}

/// Maps a complete span only when every byte belongs to one unchanged authored segment.
///
/// Empty spans map inside a segment, at the unambiguous outer boundaries, or where two abutting
/// segments agree exactly. A generated gap or discontinuous boundary is never assigned
/// approximately to either side.
pub(super) fn map_affine_span(
    segments: &[ProjectionSegment],
    span: tsrx_tape_schema::TapeSpan,
) -> Option<tsrx_tape_schema::TapeSpan> {
    if span.start > span.end {
        return None;
    }
    if span.start == span.end {
        let right = map_endpoint(segments, span.start, true);
        let left = map_endpoint(segments, span.end, false);
        let boundary = match (left, right) {
            (Some(left), Some(right)) if left == right => Some(left),
            (None, Some(right))
                if segments
                    .first()
                    .is_some_and(|segment| segment.projected.start == span.start) =>
            {
                Some(right)
            }
            (Some(left), None)
                if segments.last().is_some_and(|segment| segment.projected.end == span.end) =>
            {
                Some(left)
            }
            _ => None,
        }?;
        return Some(tsrx_tape_schema::TapeSpan::new(boundary, boundary));
    }
    let index = segments.partition_point(|segment| segment.projected.start <= span.start);
    let segment = segments.get(index.checked_sub(1)?)?;
    let inside = segment.projected.start <= span.start && span.end <= segment.projected.end;
    inside.then(|| {
        let start = segment.original_start + (span.start - segment.projected.start);
        tsrx_tape_schema::TapeSpan::new(start, start + (span.end - span.start))
    })
}

pub(super) fn project_authored_start(segments: &[ProjectionSegment], point: u32) -> Option<u32> {
    let index = segments.partition_point(|segment| segment.original_start <= point);
    let segment = segments.get(index.checked_sub(1)?)?;
    let original_end = segment.original_start + (segment.projected.end - segment.projected.start);
    (point < original_end).then(|| segment.projected.start + (point - segment.original_start))
}

pub(super) fn project_authored_end(segments: &[ProjectionSegment], point: u32) -> Option<u32> {
    let index = segments.partition_point(|segment| segment.original_start < point);
    let segment = segments.get(index.checked_sub(1)?)?;
    let original_end = segment.original_start + (segment.projected.end - segment.projected.start);
    (point <= original_end).then(|| segment.projected.start + (point - segment.original_start))
}

#[cfg(test)]
mod affine_mapping_tests {
    use tsrx_syntax::{ByteSpan, ProjectionSegment};
    use tsrx_tape_schema::TapeSpan;

    use super::{map_affine_span, map_endpoint};

    fn segment(projected_start: u32, projected_end: u32, original_start: u32) -> ProjectionSegment {
        ProjectionSegment {
            projected: ByteSpan::new(projected_start, projected_end),
            original_start,
            fixable: true,
        }
    }

    #[test]
    fn empty_spans_map_only_at_exact_outer_or_continuous_boundaries() {
        let gapped = [segment(0, 5, 100), segment(10, 15, 200)];
        assert_eq!(map_endpoint(&gapped, 0, true), Some(100));
        assert_eq!(map_endpoint(&gapped, 0, false), None);
        assert_eq!(map_endpoint(&gapped, 5, false), Some(105));
        assert_eq!(map_endpoint(&gapped, 5, true), None);
        assert_eq!(map_endpoint(&gapped, 10, false), None);
        assert_eq!(map_endpoint(&gapped, 10, true), Some(200));
        assert_eq!(map_endpoint(&gapped, 15, false), Some(205));
        assert_eq!(map_endpoint(&gapped, 15, true), None);
        assert_eq!(map_affine_span(&gapped, TapeSpan::new(0, 0)), Some(TapeSpan::new(100, 100)));
        assert_eq!(map_affine_span(&gapped, TapeSpan::new(15, 15)), Some(TapeSpan::new(205, 205)));
        assert_eq!(map_affine_span(&gapped, TapeSpan::new(5, 5)), None);
        assert_eq!(map_affine_span(&gapped, TapeSpan::new(10, 10)), None);
        assert_eq!(map_affine_span(&gapped, TapeSpan::new(4, 11)), None);
        assert_eq!(map_affine_span(&gapped, TapeSpan::new(11, 4)), None);

        let discontinuous = [segment(0, 5, 100), segment(5, 10, 200)];
        assert_eq!(map_endpoint(&discontinuous, 5, false), Some(105));
        assert_eq!(map_endpoint(&discontinuous, 5, true), Some(200));
        assert_eq!(map_affine_span(&discontinuous, TapeSpan::new(5, 5)), None);
        assert_eq!(map_affine_span(&discontinuous, TapeSpan::new(4, 6)), None);

        let continuous = [segment(0, 5, 100), segment(5, 10, 105)];
        assert_eq!(map_endpoint(&continuous, 5, false), Some(105));
        assert_eq!(map_endpoint(&continuous, 5, true), Some(105));
        assert_eq!(
            map_affine_span(&continuous, TapeSpan::new(5, 5)),
            Some(TapeSpan::new(105, 105))
        );
        assert_eq!(map_affine_span(&continuous, TapeSpan::new(4, 6)), None);
    }
}
