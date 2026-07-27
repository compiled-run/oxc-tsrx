use super::builder::Builder;
use super::entry::ProjectionPurpose;
use crate::{
    diagnostics::{ProjectionError, to_u32},
    model::{ClauseRole, ControlContext, ControlKind, EmbeddedKind, NONE, Overlay},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    TryEnd(u32),
    ParserCodeBlockEnd(u32),
    WrapperEnd(u32),
    WrapperStart(u32),
    Token(u32),
    Header { clause: u32, ordinal: u32 },
    ForBody(u32),
    Embedded(u32),
    ParserDynamic(u32),
}

impl Action {
    pub(super) fn key(self, overlay: &Overlay) -> (u32, u8) {
        match self {
            Self::TryEnd(node) => (overlay.nodes[node as usize].span.end, 0),
            Self::ParserCodeBlockEnd(block) => {
                (overlay.parser_code_blocks[block as usize].body.end.saturating_sub(1), 0)
            }
            Self::WrapperEnd(node) => (overlay.nodes[node as usize].span.end, 1),
            Self::WrapperStart(node) => (overlay.nodes[node as usize].span.start, 2),
            Self::Token(token) => (overlay.tokens[token as usize].span.start, 3),
            Self::Header { clause, .. } => (overlay.clauses[clause as usize].header.start, 3),
            Self::ForBody(clause) => {
                (overlay.clauses[clause as usize].body.start.saturating_add(1), 0)
            }
            Self::Embedded(token) => (overlay.embedded_tokens[token as usize].span.start, 3),
            Self::ParserDynamic(token) => (overlay.parser_dynamic_tokens[token as usize].offset, 2),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WrapperManifest {
    node: u32,
    context: ControlContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HeaderManifest {
    ordinal: u32,
    has_index: bool,
    has_key: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TryManifest {
    node: u32,
    context: ControlContext,
    flags: u8,
}

impl TryManifest {
    const HAS_PENDING: u8 = 1;
    const HAS_CATCH: u8 = 1 << 1;
    const CATCH_HAS_HEADER: u8 = 1 << 2;
    const AUTHORED_SEMICOLON: u8 = 1 << 3;
}

pub(super) fn build_wrapper_actions(
    overlay: &Overlay,
) -> Result<(Vec<Action>, Vec<WrapperManifest>), ProjectionError> {
    let mut actions = Vec::with_capacity(overlay.nodes.len().saturating_mul(2));
    let mut manifests = Vec::new();
    let mut active = Vec::with_capacity(8);
    for (index, node) in overlay.nodes.iter().enumerate() {
        while active
            .last()
            .is_some_and(|&active: &u32| overlay.nodes[active as usize].span.end <= node.span.start)
        {
            let active = active.pop().ok_or(ProjectionError::StructuralMismatch)?;
            actions.push(Action::WrapperEnd(active));
        }
        if node.context != ControlContext::Statement {
            let node_index = to_u32(index)?;
            if active
                .last()
                .is_some_and(|&active| node.span.end > overlay.nodes[active as usize].span.end)
            {
                return Err(ProjectionError::StructuralMismatch);
            }
            actions.push(Action::WrapperStart(node_index));
            active.push(node_index);
            manifests.push(WrapperManifest { node: node_index, context: node.context });
        }
    }
    while let Some(active) = active.pop() {
        actions.push(Action::WrapperEnd(active));
    }
    Ok((actions, manifests))
}

pub(super) fn build_header_actions(
    overlay: &Overlay,
    type_semantic: bool,
) -> Result<(Vec<Action>, Vec<HeaderManifest>), ProjectionError> {
    let mut actions = Vec::new();
    let mut manifests = Vec::new();
    for node in &overlay.nodes {
        let mut clause_index = node.first_clause;
        while clause_index != NONE {
            let clause = overlay.clauses[clause_index as usize];
            if clause.for_header.annotated || type_semantic && clause.role == ClauseRole::For {
                let ordinal = to_u32(manifests.len())?;
                actions.push(Action::Header { clause: clause_index, ordinal });
                if clause.for_header.annotated {
                    manifests.push(HeaderManifest {
                        ordinal,
                        has_index: !clause.for_header.index.is_empty(),
                        has_key: !clause.for_header.key.is_empty(),
                    });
                }
                if type_semantic
                    && (!clause.for_header.index.is_empty() || !clause.for_header.key.is_empty())
                {
                    actions.push(Action::ForBody(clause_index));
                }
            }
            clause_index = clause.next;
        }
    }
    Ok((actions, manifests))
}

pub(super) fn build_try_actions(
    source: &str,
    overlay: &Overlay,
) -> Result<(Vec<Action>, Vec<TryManifest>), ProjectionError> {
    let mut actions = Vec::new();
    let mut manifests = Vec::new();
    let mut active = Vec::with_capacity(8);
    for (index, node) in overlay.nodes.iter().enumerate() {
        while active.last().is_some_and(|&node_index: &u32| {
            overlay.nodes[node_index as usize].span.end <= node.span.start
        }) {
            actions.push(Action::TryEnd(active.pop().ok_or(ProjectionError::StructuralMismatch)?));
        }
        if node.kind != ControlKind::Try {
            continue;
        }
        let node_index = to_u32(index)?;
        let mut flags = 0;
        let mut clause_index = node.first_clause;
        while clause_index != NONE {
            let clause = overlay.clauses[clause_index as usize];
            match clause.role {
                ClauseRole::Pending => flags |= TryManifest::HAS_PENDING,
                ClauseRole::Catch => {
                    flags |= TryManifest::HAS_CATCH;
                    if !clause.header.is_empty() {
                        flags |= TryManifest::CATCH_HAS_HEADER;
                    }
                }
                _ => {}
            }
            clause_index = clause.next;
        }
        if flags & (TryManifest::HAS_PENDING | TryManifest::HAS_CATCH) == 0 {
            return Err(ProjectionError::StructuralMismatch);
        }
        if source.as_bytes()[node.span.end as usize..]
            .iter()
            .find(|byte| !byte.is_ascii_whitespace())
            == Some(&b';')
        {
            flags |= TryManifest::AUTHORED_SEMICOLON;
        }
        active.push(node_index);
        manifests.push(TryManifest { node: node_index, context: node.context, flags });
    }
    while let Some(node) = active.pop() {
        actions.push(Action::TryEnd(node));
    }
    Ok((actions, manifests))
}

pub(super) fn project_actions(
    builder: &mut Builder<'_>,
    overlay: &Overlay,
    purpose: ProjectionPurpose,
    wrapper_actions: &[Action],
    try_end_actions: &[Action],
    parser_code_block_end_actions: &[Action],
    header_actions: &[Action],
) -> Result<(), ProjectionError> {
    let mut wrapper_cursor = 0usize;
    let mut try_end_cursor = 0usize;
    let mut parser_code_block_end_cursor = 0usize;
    let mut token_cursor = 0usize;
    let mut header_cursor = 0usize;
    let mut embedded_cursor = 0usize;
    let mut parser_dynamic_cursor = 0usize;
    loop {
        let wrapper = wrapper_actions.get(wrapper_cursor).copied();
        let try_end = try_end_actions.get(try_end_cursor).copied();
        let parser_code_block_end =
            parser_code_block_end_actions.get(parser_code_block_end_cursor).copied();
        let token = (token_cursor < overlay.tokens.len())
            .then(|| to_u32(token_cursor).map(Action::Token))
            .transpose()?;
        let header = header_actions.get(header_cursor).copied();
        let embedded = (embedded_cursor < overlay.embedded_tokens.len())
            .then(|| to_u32(embedded_cursor).map(Action::Embedded))
            .transpose()?;
        let parser_dynamic = (purpose == ProjectionPurpose::Parser
            && parser_dynamic_cursor < overlay.parser_dynamic_tokens.len())
        .then(|| to_u32(parser_dynamic_cursor).map(Action::ParserDynamic))
        .transpose()?;
        let Some(action) =
            [wrapper, try_end, parser_code_block_end, token, header, embedded, parser_dynamic]
                .into_iter()
                .flatten()
                .min_by_key(|action| action.key(overlay))
        else {
            break;
        };
        match action {
            Action::TryEnd(node) => {
                try_end_cursor += 1;
                builder.try_end(node)?;
            }
            Action::ParserCodeBlockEnd(block) => {
                parser_code_block_end_cursor += 1;
                builder.parser_code_block_end(block)?;
            }
            Action::WrapperEnd(node) => {
                wrapper_cursor += 1;
                builder.wrapper_end(node)?;
            }
            Action::WrapperStart(node) => {
                wrapper_cursor += 1;
                builder.wrapper_start(node)?;
            }
            Action::Token(token) => {
                token_cursor += 1;
                builder.token(token)?;
            }
            Action::Header { clause, ordinal } => {
                header_cursor += 1;
                builder.header(clause, ordinal)?;
            }
            Action::ForBody(clause) => {
                header_cursor += 1;
                builder.for_body(clause)?;
            }
            Action::Embedded(token) => {
                embedded_cursor += 1;
                if purpose != ProjectionPurpose::Parser
                    || overlay.embedded_tokens[token as usize].kind == EmbeddedKind::StyleContent
                {
                    builder.embedded(token)?;
                }
            }
            Action::ParserDynamic(token) => {
                parser_dynamic_cursor += 1;
                builder.parser_dynamic(token)?;
            }
        }
    }
    Ok(())
}
