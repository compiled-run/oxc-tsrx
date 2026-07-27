use std::fmt::Write as _;

use crate::{
    diagnostics::{ProjectionError, to_u32},
    model::{
        ByteSpan, ClauseRole, ControlContext, ControlKind, EmbeddedKind, NONE, Overlay,
        StructuralKind,
    },
};

use super::{
    format::{HeaderManifest, TryManifest, WrapperManifest},
    mapping::MappedProjection,
    marker::{collision_free_prefix, validate_overlay_source},
};
use crate::projection_view::ProjectionSegment;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    TryEnd(u32),
    WrapperEnd(u32),
    WrapperStart(u32),
    Token(u32),
    Header { clause: u32, ordinal: u32 },
    ForBody(u32),
    Embedded(u32),
}

impl Action {
    fn key(self, overlay: &Overlay) -> (u32, u8) {
        match self {
            Self::TryEnd(node) => (overlay.nodes[node as usize].span.end, 0),
            Self::WrapperEnd(node) => (overlay.nodes[node as usize].span.end, 1),
            Self::WrapperStart(node) => (overlay.nodes[node as usize].span.start, 2),
            Self::Token(token) => (overlay.tokens[token as usize].span.start, 3),
            Self::Header { clause, .. } => (overlay.clauses[clause as usize].header.start, 3),
            Self::ForBody(clause) => {
                (overlay.clauses[clause as usize].body.start.saturating_add(1), 0)
            }
            Self::Embedded(token) => (overlay.embedded_tokens[token as usize].span.start, 3),
        }
    }
}

pub(super) struct BuiltProjection {
    pub(super) mapped: MappedProjection,
    pub(super) prefix: String,
    pub(super) wrappers: Vec<WrapperManifest>,
    pub(super) headers: Vec<HeaderManifest>,
    pub(super) tries: Vec<TryManifest>,
}

struct Builder<'a> {
    source: &'a str,
    overlay: &'a Overlay,
    prefix: &'a str,
    output: String,
    segments: Vec<ProjectionSegment>,
    record_segments: bool,
    type_semantic: bool,
    cursor: usize,
}

impl<'a> Builder<'a> {
    fn new(
        source: &'a str,
        overlay: &'a Overlay,
        prefix: &'a str,
        record_segments: bool,
        type_semantic: bool,
    ) -> Self {
        Self {
            source,
            overlay,
            prefix,
            output: String::with_capacity(
                source
                    .len()
                    .saturating_add(overlay.tokens.len().saturating_mul(64))
                    .saturating_add(overlay.embedded_tokens.len().saturating_mul(32)),
            ),
            segments: if record_segments {
                Vec::with_capacity(
                    overlay
                        .tokens
                        .len()
                        .saturating_mul(2)
                        .saturating_add(overlay.dynamic_tags.len())
                        .saturating_add(1),
                )
            } else {
                Vec::new()
            },
            record_segments,
            type_semantic,
            cursor: 0,
        }
    }

    fn finish(mut self) -> Result<MappedProjection, ProjectionError> {
        self.copy_to(self.source.len())?;
        Ok(MappedProjection {
            projected: self.output,
            segments: self.segments,
            dynamic_prefix: None,
            dynamic_count: 0,
            dynamic_offsets: Vec::new(),
            synthetic_generator_spans: Vec::new(),
            synthetic_callee_spans: Vec::new(),
        })
    }

    fn copy_to(&mut self, end: usize) -> Result<(), ProjectionError> {
        if end < self.cursor || end > self.source.len() {
            return Err(ProjectionError::SourceChanged {
                offset: to_u32(end.min(self.source.len()))?,
            });
        }
        if end > self.cursor {
            let span = ByteSpan::new(to_u32(self.cursor)?, to_u32(end)?);
            self.copy_original(span)?;
            self.cursor = end;
        }
        Ok(())
    }

    fn copy_original(&mut self, span: ByteSpan) -> Result<(), ProjectionError> {
        self.copy_original_with_fixability(span, true)
    }

    fn copy_original_with_fixability(
        &mut self,
        span: ByteSpan,
        fixable: bool,
    ) -> Result<(), ProjectionError> {
        let start = span.start as usize;
        let end = span.end as usize;
        let Some(value) = self.source.get(start..end) else {
            return Err(ProjectionError::SourceChanged { offset: span.start });
        };
        let projected_start =
            self.record_segments.then(|| to_u32(self.output.len())).transpose()?;
        self.output.push_str(value);
        let Some(projected_start) = projected_start else {
            return Ok(());
        };
        let projected_end = to_u32(self.output.len())?;
        if let Some(previous) = self.segments.last_mut()
            && previous.projected.end == projected_start
            && previous.fixable == fixable
            && previous.original_start + (previous.projected.end - previous.projected.start)
                == span.start
        {
            previous.projected.end = projected_end;
        } else {
            self.segments.push(ProjectionSegment {
                projected: ByteSpan::new(projected_start, projected_end),
                original_start: span.start,
                fixable,
            });
        }
        Ok(())
    }

    fn wrapper_start(&mut self, node_index: u32) -> Result<(), ProjectionError> {
        let node = self.overlay.nodes[node_index as usize];
        self.copy_to(node.span.start as usize)?;
        if node.context == ControlContext::JsxChild {
            self.output.push('{');
        }
        write!(
            self.output,
            "{}W{node_index}_({{async *{}M{node_index}_(){{/*{}N{node_index}S__*/",
            self.prefix, self.prefix, self.prefix
        )
        .expect("writing to a String cannot fail");
        Ok(())
    }

    fn wrapper_end(&mut self, node_index: u32) -> Result<(), ProjectionError> {
        let node = self.overlay.nodes[node_index as usize];
        self.copy_to(node.span.end as usize)?;
        write!(
            self.output,
            "/*{}N{node_index}E__*/}}}},{}E{node_index}_)",
            self.prefix, self.prefix
        )
        .expect("writing to a String cannot fail");
        if node.context == ControlContext::JsxChild {
            self.output.push('}');
        }
        Ok(())
    }

    fn try_end(&mut self, node_index: u32) -> Result<(), ProjectionError> {
        let node = self.overlay.nodes[node_index as usize];
        if node.kind != ControlKind::Try {
            return Err(ProjectionError::StructuralMismatch);
        }
        self.copy_to(node.span.end as usize)?;
        write!(self.output, "}},{}TE{node_index}_)", self.prefix)
            .expect("writing to a String cannot fail");
        Ok(())
    }

    fn token(&mut self, token_index: u32) -> Result<(), ProjectionError> {
        let token = self.overlay.tokens[token_index as usize];
        let start = token.span.start as usize;
        self.copy_to(start)?;
        let spelling = match token.kind {
            StructuralKind::FunctionBody => b"@{".as_slice(),
            StructuralKind::If => b"@if",
            StructuralKind::Else => b"@else",
            StructuralKind::For => b"@for",
            StructuralKind::Empty => b"@empty",
            StructuralKind::Switch => b"@switch",
            StructuralKind::Case => b"@case",
            StructuralKind::Default => b"@default",
            StructuralKind::Try => b"@try",
            StructuralKind::Pending => b"@pending",
            StructuralKind::Catch => b"@catch",
        };
        if self.source.as_bytes().get(start..start + spelling.len()) != Some(spelling) {
            return Err(ProjectionError::SourceChanged { offset: token.span.start });
        }
        match token.kind {
            StructuralKind::FunctionBody if self.type_semantic => {
                write!(self.output, "/*{}{token_index}*/", self.prefix)
                    .expect("writing to a String cannot fail");
                self.cursor = start + 1;
                if self.source.as_bytes().get(self.cursor) != Some(&b'{') {
                    return Err(ProjectionError::SourceChanged { offset: token.span.start });
                }
                self.copy_to(self.cursor + 1)?;
                self.output.push_str("\nif (false) return null as any;\n");
            }
            StructuralKind::Try => {
                if token.owner == NONE {
                    return Err(ProjectionError::StructuralMismatch);
                }
                write!(
                    self.output,
                    "/*{}{token_index}*/{}T{}_({{async *{}B{}_()",
                    self.prefix, self.prefix, token.owner, self.prefix, token.owner
                )
                .expect("writing to a String cannot fail");
                self.cursor = start + spelling.len();
            }
            StructuralKind::Pending => {
                if token.owner == NONE {
                    return Err(ProjectionError::StructuralMismatch);
                }
                write!(
                    self.output,
                    ",/*{}{token_index}*/async *{}P{}_()",
                    self.prefix, self.prefix, token.owner
                )
                .expect("writing to a String cannot fail");
                self.cursor = start + spelling.len();
            }
            StructuralKind::Catch => {
                if token.owner == NONE {
                    return Err(ProjectionError::StructuralMismatch);
                }
                write!(
                    self.output,
                    ",/*{}{token_index}*/async *{}C{}_",
                    self.prefix, self.prefix, token.owner
                )
                .expect("writing to a String cannot fail");
                if !self.catch_has_header(token.owner)? {
                    self.output.push_str("()");
                }
                self.cursor = start + spelling.len();
            }
            kind => {
                write!(self.output, "/*{}{token_index}*/", self.prefix)
                    .expect("writing to a String cannot fail");
                if kind == StructuralKind::Empty {
                    self.output.push_str("if (false)");
                    self.cursor = start + spelling.len();
                } else {
                    self.cursor = start + 1;
                }
            }
        }
        Ok(())
    }

    fn catch_has_header(&self, node: u32) -> Result<bool, ProjectionError> {
        let mut clause = self.overlay.nodes[node as usize].first_clause;
        while clause != NONE {
            let current = self.overlay.clauses[clause as usize];
            if current.role == ClauseRole::Catch {
                return Ok(!current.header.is_empty());
            }
            clause = current.next;
        }
        Err(ProjectionError::StructuralMismatch)
    }

    fn header(&mut self, clause_index: u32, ordinal: u32) -> Result<(), ProjectionError> {
        let clause = self.overlay.clauses[clause_index as usize];
        if self.type_semantic {
            return self.type_header(clause);
        }
        let header = clause.for_header;
        if !header.annotated {
            return Err(ProjectionError::ScaffoldMismatch { index: ordinal as usize });
        }
        self.copy_to(clause.header.start as usize)?;
        self.output.push('(');
        self.copy_original(header.left)?;
        write!(self.output, " of {}H{ordinal}_(/*{}R{ordinal}S__*/", self.prefix, self.prefix)
            .expect("writing to a String cannot fail");
        self.copy_original(header.right)?;
        write!(self.output, "/*{}R{ordinal}E__*/", self.prefix)
            .expect("writing to a String cannot fail");
        if !header.index.is_empty() {
            write!(self.output, ",{}IH{ordinal}_(/*{}I{ordinal}S__*/", self.prefix, self.prefix)
                .expect("writing to a String cannot fail");
            self.copy_original(header.index)?;
            write!(self.output, "/*{}I{ordinal}E__*/)", self.prefix)
                .expect("writing to a String cannot fail");
        }
        if !header.key.is_empty() {
            write!(self.output, ",{}KH{ordinal}_(/*{}K{ordinal}S__*/", self.prefix, self.prefix)
                .expect("writing to a String cannot fail");
            self.copy_original(header.key)?;
            write!(self.output, "/*{}K{ordinal}E__*/)", self.prefix)
                .expect("writing to a String cannot fail");
        }
        write!(self.output, ",{}HE{ordinal}_))", self.prefix)
            .expect("writing to a String cannot fail");
        self.cursor = clause.header.end as usize;
        Ok(())
    }

    fn type_header(&mut self, clause: crate::model::Clause) -> Result<(), ProjectionError> {
        if clause.role != ClauseRole::For {
            return Err(ProjectionError::StructuralMismatch);
        }
        let header = clause.for_header;
        if header.left.is_empty() || header.right.is_empty() {
            return Err(ProjectionError::StructuralMismatch);
        }
        self.copy_to(clause.header.start as usize)?;
        self.output.push('(');
        let left = self
            .source
            .get(header.left.start as usize..header.left.end as usize)
            .ok_or(ProjectionError::SourceChanged { offset: header.left.start })?;
        let trimmed = left.trim_start();
        if !trimmed.starts_with("const ")
            && !trimmed.starts_with("let ")
            && !trimmed.starts_with("var ")
            && !trimmed.starts_with("using ")
            && !trimmed.starts_with("await using ")
        {
            self.output.push_str("const ");
        }
        self.copy_original(header.left)?;
        self.output.push_str(" of ");
        self.copy_original(header.right)?;
        self.output.push(')');
        self.cursor = clause.header.end as usize;
        Ok(())
    }

    fn for_body(&mut self, clause_index: u32) -> Result<(), ProjectionError> {
        if !self.type_semantic {
            return Err(ProjectionError::StructuralMismatch);
        }
        let clause = self.overlay.clauses[clause_index as usize];
        if clause.role != ClauseRole::For
            || self.source.as_bytes().get(clause.body.start as usize) != Some(&b'{')
        {
            return Err(ProjectionError::StructuralMismatch);
        }
        self.copy_to(clause.body.start.saturating_add(1) as usize)?;
        let header = clause.for_header;
        if !header.index.is_empty() {
            self.output.push_str("\nlet ");
            self.copy_original(header.index)?;
            self.output.push_str(" = 0;\n");
        }
        if !header.key.is_empty() {
            self.output.push_str("\nvoid (");
            self.copy_original(header.key)?;
            self.output.push_str(");\n");
        }
        Ok(())
    }

    fn embedded(&mut self, token_index: u32) -> Result<(), ProjectionError> {
        let token = self.overlay.embedded_tokens[token_index as usize];
        let span_start = token.span.start as usize;
        let span_end = token.span.end as usize;
        self.copy_to(span_start)?;
        match token.kind {
            EmbeddedKind::DynamicOpen => {
                let tag = self
                    .overlay
                    .dynamic_tags
                    .get(token.owner as usize)
                    .ok_or(ProjectionError::StructuralMismatch)?;
                if self.source.as_bytes().get(span_start..span_start + 2) != Some(b"<{")
                    || tag.expression.start < token.span.start + 2
                    || tag.expression.end + 1 != token.span.end
                    || self.source.as_bytes().get(tag.expression.end as usize) != Some(&b'}')
                {
                    return Err(ProjectionError::SourceChanged { offset: token.span.start });
                }
                write!(
                    self.output,
                    "<{}D{} {}A{}_={{",
                    self.prefix, token.owner, self.prefix, token.owner
                )
                .expect("writing to a String cannot fail");
                self.cursor = tag.expression.start as usize;
                self.copy_original_with_fixability(tag.expression, tag.self_closing)?;
                self.cursor = tag.expression.end as usize;
                write!(self.output, "}} {}Z{}_={{null}}", self.prefix, token.owner)
                    .expect("writing to a String cannot fail");
                self.cursor = span_end;
            }
            EmbeddedKind::DynamicClose => {
                if self.source.as_bytes().get(span_start..span_start + 3) != Some(b"</{")
                    || self.source.as_bytes().get(span_end.saturating_sub(1)) != Some(&b'>')
                {
                    return Err(ProjectionError::SourceChanged { offset: token.span.start });
                }
                let tag = self
                    .overlay
                    .dynamic_tags
                    .get(token.owner as usize)
                    .ok_or(ProjectionError::StructuralMismatch)?;
                let first = tag.first_closing_comment as usize;
                let end = first
                    .checked_add(tag.closing_comment_count as usize)
                    .ok_or(ProjectionError::SourceTooLarge)?;
                let comments = self
                    .overlay
                    .dynamic_comments
                    .get(first..end)
                    .ok_or(ProjectionError::StructuralMismatch)?;
                for (offset, comment) in comments.iter().enumerate() {
                    let comment_source = self
                        .source
                        .as_bytes()
                        .get(comment.start as usize..comment.end as usize)
                        .ok_or(ProjectionError::SourceChanged { offset: comment.start })?;
                    if comment.start < tag.closing_expression.start
                        || comment.end > tag.closing_expression.end
                        || (!comment_source.starts_with(b"//")
                            && !comment_source.starts_with(b"/*"))
                    {
                        return Err(ProjectionError::StructuralMismatch);
                    }
                    let ordinal = first + offset;
                    write!(self.output, "{{/*{}Q{ordinal}__*/ null}}", self.prefix)
                        .expect("writing to a String cannot fail");
                }
                write!(self.output, "</{}D{}>", self.prefix, token.owner)
                    .expect("writing to a String cannot fail");
                self.cursor = span_end;
            }
            EmbeddedKind::StyleContent => {
                let style = self
                    .overlay
                    .style_blocks
                    .get(token.owner as usize)
                    .ok_or(ProjectionError::StructuralMismatch)?;
                if style.content != token.span {
                    return Err(ProjectionError::StructuralMismatch);
                }
                write!(self.output, "{{/*{}S{}__*/ null}}", self.prefix, token.owner)
                    .expect("writing to a String cannot fail");
                self.cursor = span_end;
            }
        }
        Ok(())
    }
}

pub(super) fn build_projection(
    source: &str,
    overlay: &Overlay,
    record_segments: bool,
) -> Result<BuiltProjection, ProjectionError> {
    build_projection_with_purpose(source, overlay, record_segments, ProjectionPurpose::Syntax)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectionPurpose {
    Syntax,
    Types,
}

pub(super) fn build_projection_with_purpose(
    source: &str,
    overlay: &Overlay,
    record_segments: bool,
    purpose: ProjectionPurpose,
) -> Result<BuiltProjection, ProjectionError> {
    validate_overlay_source(source, overlay)?;
    let prefix = collision_free_prefix(source)?;
    let (wrapper_actions, wrappers) = build_wrapper_actions(overlay)?;

    let (try_end_actions, tries) = build_try_actions(source, overlay)?;

    let (header_actions, headers) =
        build_header_actions(overlay, purpose == ProjectionPurpose::Types)?;

    let mut builder = Builder::new(
        source,
        overlay,
        &prefix,
        record_segments,
        purpose == ProjectionPurpose::Types,
    );
    let mut wrapper_cursor = 0usize;
    let mut try_end_cursor = 0usize;
    let mut token_cursor = 0usize;
    let mut header_cursor = 0usize;
    let mut embedded_cursor = 0usize;
    loop {
        let wrapper = wrapper_actions.get(wrapper_cursor).copied();
        let try_end = try_end_actions.get(try_end_cursor).copied();
        let token = (token_cursor < overlay.tokens.len())
            .then(|| to_u32(token_cursor).map(Action::Token))
            .transpose()?;
        let header = header_actions.get(header_cursor).copied();
        let embedded = (embedded_cursor < overlay.embedded_tokens.len())
            .then(|| to_u32(embedded_cursor).map(Action::Embedded))
            .transpose()?;
        let Some(action) = [wrapper, try_end, token, header, embedded]
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
                builder.embedded(token)?;
            }
        }
    }
    let mut mapped = builder.finish()?;
    mapped.synthetic_generator_spans = overlay
        .nodes
        .iter()
        .filter(|node| node.context != ControlContext::Statement || node.kind == ControlKind::Try)
        .map(|node| node.span)
        .collect();
    if record_segments && !overlay.dynamic_tags.is_empty() {
        mapped.dynamic_prefix = Some(prefix.clone());
        mapped.dynamic_count = to_u32(overlay.dynamic_tags.len())?;
        mapped.dynamic_offsets =
            overlay.dynamic_tags.iter().map(|tag| tag.expression.start).collect();
    }
    Ok(BuiltProjection { mapped, prefix, wrappers, headers, tries })
}

fn build_wrapper_actions(
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

fn build_header_actions(
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

fn build_try_actions(
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

#[cfg(all(test, target_pointer_width = "64"))]
mod layout_tests {
    use std::mem::size_of;

    use super::Action;

    #[test]
    fn action_layout_remains_compact() {
        assert_eq!(size_of::<Action>(), 12);
    }
}
