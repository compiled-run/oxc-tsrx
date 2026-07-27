use std::fmt::Write as _;

use super::entry::ProjectionPurpose;
use super::mapping::MappedProjection;
use crate::{
    diagnostics::{ProjectionError, to_u32},
    model::{
        ByteSpan, ClauseRole, ControlContext, ControlKind, EmbeddedKind, NONE, Overlay,
        ParserDynamicKind, StructuralKind,
    },
    projection_view::ProjectionSegment,
};

pub(super) struct Builder<'a> {
    source: &'a str,
    overlay: &'a Overlay,
    prefix: &'a str,
    output: String,
    segments: Vec<ProjectionSegment>,
    record_segments: bool,
    purpose: ProjectionPurpose,
    cursor: usize,
    synthetic_callee_spans: Vec<(u32, u32)>,
}
impl<'a> Builder<'a> {
    pub(super) fn new(
        source: &'a str,
        overlay: &'a Overlay,
        prefix: &'a str,
        record_segments: bool,
        purpose: ProjectionPurpose,
    ) -> Self {
        let raw_style_bytes = overlay.style_blocks.iter().fold(0_usize, |bytes, style| {
            bytes.saturating_add(style.content.end.saturating_sub(style.content.start) as usize)
        });
        Self {
            source,
            overlay,
            prefix,
            output: String::with_capacity(
                source
                    .len()
                    .saturating_sub(raw_style_bytes)
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
                        .saturating_add(overlay.style_blocks.len())
                        .saturating_add(1),
                )
            } else {
                Vec::new()
            },
            record_segments,
            purpose,
            cursor: 0,
            synthetic_callee_spans: Vec::new(),
        }
    }

    pub(super) fn finish(mut self) -> Result<MappedProjection, ProjectionError> {
        self.copy_to(self.source.len())?;
        Ok(MappedProjection {
            projected: self.output,
            segments: self.segments,
            dynamic_prefix: None,
            dynamic_count: 0,
            dynamic_offsets: Vec::new(),
            synthetic_generator_spans: Vec::new(),
            synthetic_callee_spans: self.synthetic_callee_spans,
        })
    }

    fn record_synthetic_callee(&mut self, start: usize) -> Result<(), ProjectionError> {
        self.synthetic_callee_spans.push((to_u32(start)?, to_u32(self.output.len())?));
        Ok(())
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

    pub(super) fn wrapper_start(&mut self, node_index: u32) -> Result<(), ProjectionError> {
        let node = self.overlay.nodes[node_index as usize];
        self.copy_to(node.span.start as usize)?;
        if node.context == ControlContext::JsxChild {
            self.output.push('{');
        }
        let callee_start = self.output.len();
        write!(self.output, "{}W{node_index}_", self.prefix)
            .expect("writing to a String cannot fail");
        self.record_synthetic_callee(callee_start)?;
        write!(
            self.output,
            "({{async *{}M{node_index}_(){{/*{}N{node_index}S__*/",
            self.prefix, self.prefix
        )
        .expect("writing to a String cannot fail");
        Ok(())
    }

    pub(super) fn wrapper_end(&mut self, node_index: u32) -> Result<(), ProjectionError> {
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

    pub(super) fn try_end(&mut self, node_index: u32) -> Result<(), ProjectionError> {
        let node = self.overlay.nodes[node_index as usize];
        if node.kind != ControlKind::Try {
            return Err(ProjectionError::StructuralMismatch);
        }
        self.copy_to(node.span.end as usize)?;
        write!(self.output, "}},{}TE{node_index}_)", self.prefix)
            .expect("writing to a String cannot fail");
        Ok(())
    }

    pub(super) fn token(&mut self, token_index: u32) -> Result<(), ProjectionError> {
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
            StructuralKind::FunctionBody if self.purpose == ProjectionPurpose::Parser => {
                self.parser_function_body(token_index, start)?;
            }
            StructuralKind::FunctionBody if self.purpose == ProjectionPurpose::Types => {
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
                write!(self.output, "/*{}{token_index}*/", self.prefix)
                    .expect("writing to a String cannot fail");
                let callee_start = self.output.len();
                write!(self.output, "{}T{}_", self.prefix, token.owner)
                    .expect("writing to a String cannot fail");
                self.record_synthetic_callee(callee_start)?;
                write!(self.output, "({{async *{}B{}_()", self.prefix, token.owner)
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

    fn parser_function_body(
        &mut self,
        token_index: u32,
        start: usize,
    ) -> Result<(), ProjectionError> {
        // Keep the authored opening brace affine. A JSX-child code block then gets an
        // authenticated function wrapper so its body may contain statements; ordinary blocks
        // need only the marker after the brace.
        self.cursor = start + 1;
        self.copy_to(start + 2)?;
        if self.parser_code_block(token_index).is_some() {
            write!(
                self.output,
                "(async function*{}J{token_index}_(){{/*{}{token_index}*/",
                self.prefix, self.prefix
            )
            .expect("writing to a String cannot fail");
        } else {
            write!(self.output, "/*{}{token_index}*/", self.prefix)
                .expect("writing to a String cannot fail");
        }
        Ok(())
    }

    fn parser_code_block(&self, token: u32) -> Option<usize> {
        self.overlay.parser_code_blocks.binary_search_by_key(&token, |block| block.token).ok()
    }

    pub(super) fn parser_code_block_end(
        &mut self,
        block_index: u32,
    ) -> Result<(), ProjectionError> {
        if self.purpose != ProjectionPurpose::Parser {
            return Err(ProjectionError::StructuralMismatch);
        }
        let block = self
            .overlay
            .parser_code_blocks
            .get(block_index as usize)
            .ok_or(ProjectionError::StructuralMismatch)?;
        let closing =
            block.body.end.checked_sub(1).ok_or(ProjectionError::StructuralMismatch)? as usize;
        if self.source.as_bytes().get(closing) != Some(&b'}') {
            return Err(ProjectionError::SourceChanged {
                offset: block.body.end.saturating_sub(1),
            });
        }
        self.copy_to(closing)?;
        self.output.push_str("})");
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

    pub(super) fn header(
        &mut self,
        clause_index: u32,
        ordinal: u32,
    ) -> Result<(), ProjectionError> {
        let clause = self.overlay.clauses[clause_index as usize];
        if self.purpose == ProjectionPurpose::Types {
            return self.type_header(clause);
        }
        let header = clause.for_header;
        if !header.annotated {
            return Err(ProjectionError::ScaffoldMismatch { index: ordinal as usize });
        }
        self.copy_to(clause.header.start as usize)?;
        self.output.push('(');
        self.copy_original(header.left)?;
        self.output.push_str(" of ");
        let callee_start = self.output.len();
        write!(self.output, "{}H{ordinal}_", self.prefix).expect("writing to a String cannot fail");
        self.record_synthetic_callee(callee_start)?;
        write!(self.output, "(/*{}R{ordinal}S__*/", self.prefix)
            .expect("writing to a String cannot fail");
        self.copy_original(header.right)?;
        write!(self.output, "/*{}R{ordinal}E__*/", self.prefix)
            .expect("writing to a String cannot fail");
        if !header.index.is_empty() {
            self.output.push(',');
            let callee_start = self.output.len();
            write!(self.output, "{}IH{ordinal}_", self.prefix)
                .expect("writing to a String cannot fail");
            self.record_synthetic_callee(callee_start)?;
            write!(self.output, "(/*{}I{ordinal}S__*/", self.prefix)
                .expect("writing to a String cannot fail");
            self.copy_original(header.index)?;
            write!(self.output, "/*{}I{ordinal}E__*/)", self.prefix)
                .expect("writing to a String cannot fail");
        }
        if !header.key.is_empty() {
            self.output.push(',');
            let callee_start = self.output.len();
            write!(self.output, "{}KH{ordinal}_", self.prefix)
                .expect("writing to a String cannot fail");
            self.record_synthetic_callee(callee_start)?;
            write!(self.output, "(/*{}K{ordinal}S__*/", self.prefix)
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

    pub(super) fn for_body(&mut self, clause_index: u32) -> Result<(), ProjectionError> {
        if self.purpose != ProjectionPurpose::Types {
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

    #[expect(
        clippy::too_many_lines,
        reason = "one flat match over every embedded-language token shape"
    )]
    pub(super) fn embedded(&mut self, token_index: u32) -> Result<(), ProjectionError> {
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
                if self.purpose == ProjectionPurpose::Parser {
                    self.output.push('(');
                }
                self.cursor = tag.expression.start as usize;
                self.copy_original_with_fixability(
                    tag.expression,
                    tag.self_closing || self.purpose == ProjectionPurpose::Parser,
                )?;
                self.cursor = tag.expression.end as usize;
                if self.purpose == ProjectionPurpose::Parser {
                    self.output.push(')');
                }
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
                if self.purpose == ProjectionPurpose::Parser {
                    if tag.closing_expression.is_empty() {
                        return Err(ProjectionError::StructuralMismatch);
                    }
                    write!(self.output, "{{{}C{}_((", self.prefix, token.owner)
                        .expect("writing to a String cannot fail");
                    self.cursor = tag.closing_expression.start as usize;
                    self.copy_original_with_fixability(tag.closing_expression, true)?;
                    self.cursor = tag.closing_expression.end as usize;
                    self.output.push_str("))}");
                } else {
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

    pub(super) fn parser_dynamic(&mut self, token_index: u32) -> Result<(), ProjectionError> {
        if self.purpose != ProjectionPurpose::Parser {
            return Err(ProjectionError::StructuralMismatch);
        }
        let token = self.overlay.parser_dynamic_tokens[token_index as usize];
        let tag = self
            .overlay
            .dynamic_tags
            .get(token.owner as usize)
            .ok_or(ProjectionError::StructuralMismatch)?;
        match token.kind {
            ParserDynamicKind::OpenStart => {
                if token.offset != tag.opening.start
                    || self
                        .source
                        .as_bytes()
                        .get(tag.opening.start as usize..tag.expression.start as usize)
                        != Some(b"<{")
                {
                    return Err(ProjectionError::StructuralMismatch);
                }
                self.copy_to(tag.opening.start as usize)?;
                write!(
                    self.output,
                    "<{}D{} {}A{}_={{(",
                    self.prefix, token.owner, self.prefix, token.owner
                )
                .expect("writing to a String cannot fail");
                self.cursor = tag.expression.start as usize;
            }
            ParserDynamicKind::OpenEnd => {
                if token.offset != tag.expression.end
                    || tag.opening.end != tag.expression.end.saturating_add(1)
                    || self.source.as_bytes().get(tag.expression.end as usize) != Some(&b'}')
                {
                    return Err(ProjectionError::StructuralMismatch);
                }
                self.copy_to(tag.expression.end as usize)?;
                write!(self.output, ")}} {}Z{}_={{null}}", self.prefix, token.owner)
                    .expect("writing to a String cannot fail");
                self.cursor = tag.opening.end as usize;
            }
            ParserDynamicKind::CloseStart => {
                if token.offset != tag.closing.start
                    || self
                        .source
                        .as_bytes()
                        .get(tag.closing.start as usize..tag.closing_expression.start as usize)
                        != Some(b"</{")
                {
                    return Err(ProjectionError::StructuralMismatch);
                }
                self.copy_to(tag.closing.start as usize)?;
                self.output.push('{');
                let callee_start = self.output.len();
                write!(self.output, "{}C{}_", self.prefix, token.owner)
                    .expect("writing to a String cannot fail");
                self.record_synthetic_callee(callee_start)?;
                self.output.push_str("((");
                self.cursor = tag.closing_expression.start as usize;
            }
            ParserDynamicKind::CloseEnd => {
                if token.offset != tag.closing_expression.end
                    || tag.closing.end <= tag.closing_expression.end
                {
                    return Err(ProjectionError::StructuralMismatch);
                }
                self.copy_to(tag.closing_expression.end as usize)?;
                write!(self.output, "))}}</{}D{}>", self.prefix, token.owner)
                    .expect("writing to a String cannot fail");
                self.cursor = tag.closing.end as usize;
            }
        }
        Ok(())
    }
}
