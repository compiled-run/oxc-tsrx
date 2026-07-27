//! Per-construct clause checks, so a later pass can index clauses by role without re-deriving
//! their bounds from the source.

use tsrx_syntax::{ByteSpan, ClauseRole, ForHeader, NONE_INDEX, OverlayView};

use crate::TsrxParseError;

pub(super) fn validate_try_clauses(
    view: OverlayView<'_>,
    node_index: usize,
) -> Result<(), TsrxParseError> {
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

pub(super) fn validate_switch_clauses(
    view: OverlayView<'_>,
    node_index: usize,
) -> Result<(), TsrxParseError> {
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

pub(super) fn validate_for_clauses(
    view: OverlayView<'_>,
    node_index: usize,
) -> Result<(), TsrxParseError> {
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

pub(super) fn validate_if_clauses(
    view: OverlayView<'_>,
    node_index: usize,
) -> Result<(), TsrxParseError> {
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
