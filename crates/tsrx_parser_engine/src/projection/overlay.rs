//! Proving the scanner overlay still describes this source: every structural token resolves to
//! an owner node of the kind it claims.

use tsrx_syntax::{ControlKind, NONE_INDEX, OverlayView, StructuralKind};

use crate::TsrxParseError;

use super::{
    clauses::{
        validate_for_clauses, validate_if_clauses, validate_switch_clauses, validate_try_clauses,
    },
    embedded::{validate_dynamic_overlay, validate_style_overlay},
};

pub(crate) fn validate_overlay(view: OverlayView<'_>) -> Result<(), TsrxParseError> {
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
