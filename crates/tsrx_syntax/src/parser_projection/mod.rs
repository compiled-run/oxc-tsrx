use std::{fmt::Write as _, ops::Range};

use crate::{
    diagnostics::{ProjectionError, to_u32},
    model::{
        ByteSpan, ClauseRole, ControlContext, ControlKind, EmbeddedKind, NONE, Overlay,
        ParserDynamicKind, StructuralKind,
    },
    scanner::source_fingerprint,
};

use crate::projection_view::{ProjectionSegment, ProjectionView};

/// Legal TSX plus an affine map for ranges copied byte-for-byte from authored TSRX.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedProjection {
    projected: String,
    segments: Vec<ProjectionSegment>,
    dynamic_prefix: Option<String>,
    dynamic_count: u32,
    dynamic_offsets: Vec<u32>,
    synthetic_generator_spans: Vec<ByteSpan>,
    synthetic_callee_spans: Vec<(u32, u32)>,
}

impl MappedProjection {
    #[must_use]
    pub fn source(&self) -> &str {
        &self.projected
    }

    #[must_use]
    pub fn view(&self) -> ProjectionView<'_> {
        ProjectionView { source: &self.projected, segments: &self.segments }
    }

    /// Maps a projected range only when every byte belongs to one unchanged authored segment.
    #[must_use]
    pub fn map_range(&self, range: Range<u32>) -> Option<Range<u32>> {
        self.map_range_with(range, false)
    }

    /// Maps a projected fix only when it is affine and cannot invalidate a paired dynamic name.
    #[must_use]
    pub fn map_fix_range(&self, range: Range<u32>) -> Option<Range<u32>> {
        self.map_range_with(range, true)
    }

    fn map_range_with(&self, range: Range<u32>, require_fixable: bool) -> Option<Range<u32>> {
        map_single_segment(&self.segments, range, require_fixable)
    }

    /// Returns the collision-free synthetic dynamic-tag namespace and expected tag count.
    #[must_use]
    pub fn dynamic_contract(&self) -> Option<(&str, u32, &[u32])> {
        if self.dynamic_count == 0 {
            return None;
        }
        self.dynamic_prefix
            .as_deref()
            .map(|prefix| (prefix, self.dynamic_count, self.dynamic_offsets.as_slice()))
    }

    /// Collision-free marker namespace used by the parser-only reconstruction lane.
    ///
    /// Parser projections retain this even when their only implemented construct has no emitted
    /// marker (for example a self-closing raw style element), so validation never has to infer a
    /// namespace from untrusted projected comments.
    #[must_use]
    pub fn parser_marker_prefix(&self) -> Option<&str> {
        self.dynamic_prefix.as_deref()
    }

    /// Returns true when an authored range belongs to a generator introduced only as projection
    /// scaffolding. Generator-specific built-in diagnostics in these ranges are synthetic.
    #[must_use]
    pub fn is_synthetic_generator_range(&self, range: Range<u32>) -> bool {
        self.synthetic_generator_spans.iter().any(|span| span.intersects(range.start, range.end))
    }

    /// Projected byte spans of helper callees introduced by this projection.
    ///
    /// The parser's dynamic-expression validator uses these exact spans to distinguish generated
    /// control helpers from authored calls, including authored escaped identifiers that decode to
    /// the same collision-free prefix.
    #[must_use]
    pub fn synthetic_callee_spans(&self) -> &[(u32, u32)] {
        &self.synthetic_callee_spans
    }
}

fn map_single_segment(
    segments: &[ProjectionSegment],
    range: Range<u32>,
    require_fixable: bool,
) -> Option<Range<u32>> {
    if range.start > range.end {
        return None;
    }
    segments.iter().find_map(|segment| {
        if require_fixable && !segment.fixable {
            return None;
        }
        let inside = if range.is_empty() {
            segment.projected.start < range.start && range.start < segment.projected.end
        } else {
            segment.projected.start <= range.start && range.end <= segment.projected.end
        };
        inside.then(|| {
            let start = segment.original_start + (range.start - segment.projected.start);
            start..start + (range.end - range.start)
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WrapperManifest {
    node: u32,
    context: ControlContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeaderManifest {
    ordinal: u32,
    has_index: bool,
    has_key: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TryManifest {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
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
    fn key(self, overlay: &Overlay) -> (u32, u8) {
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

#[expect(
    dead_code,
    reason = "the manifests the shared action builders emit are consumed by the formatter lift lane in `projection`, not by the parser lane"
)]
struct BuiltProjection {
    mapped: MappedProjection,
    prefix: String,
    wrappers: Vec<WrapperManifest>,
    headers: Vec<HeaderManifest>,
    tries: Vec<TryManifest>,
}

struct Builder<'a> {
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
    fn new(
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

    fn finish(mut self) -> Result<MappedProjection, ProjectionError> {
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

    fn wrapper_start(&mut self, node_index: u32) -> Result<(), ProjectionError> {
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

    fn parser_code_block_end(&mut self, block_index: u32) -> Result<(), ProjectionError> {
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

    fn header(&mut self, clause_index: u32, ordinal: u32) -> Result<(), ProjectionError> {
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

    fn for_body(&mut self, clause_index: u32) -> Result<(), ProjectionError> {
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

    fn parser_dynamic(&mut self, token_index: u32) -> Result<(), ProjectionError> {
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

/// Builds the legal-TSX projection consumed by the canonical TSRX parser.
///
/// Unlike the lint projection, this parser-only lane retains each authored closing dynamic-tag
/// expression inside collision-free scaffold consumed after the same single OXC parse.
///
/// # Errors
///
/// Returns an error for a stale overlay or a projection scaffold collision.
pub fn project_for_parser(
    source: &str,
    overlay: &Overlay,
) -> Result<MappedProjection, ProjectionError> {
    Ok(build_projection_with_purpose(source, overlay, true, ProjectionPurpose::Parser)?.mapped)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectionPurpose {
    Types,
    Parser,
}

fn build_projection_with_purpose(
    source: &str,
    overlay: &Overlay,
    record_segments: bool,
    purpose: ProjectionPurpose,
) -> Result<BuiltProjection, ProjectionError> {
    validate_overlay_source(source, overlay)?;
    validate_projection_lane(overlay, purpose)?;
    let prefix = collision_free_prefix(source)?;
    let (wrapper_actions, wrappers) = build_wrapper_actions(overlay)?;

    let (try_end_actions, tries) = build_try_actions(source, overlay)?;
    let mut parser_code_block_end_actions = overlay
        .parser_code_blocks
        .iter()
        .enumerate()
        .map(|(index, _)| to_u32(index).map(Action::ParserCodeBlockEnd))
        .collect::<Result<Vec<_>, _>>()?;
    parser_code_block_end_actions.sort_unstable_by_key(|action| action.key(overlay));

    let (header_actions, headers) =
        build_header_actions(overlay, purpose == ProjectionPurpose::Types)?;

    let mut builder = Builder::new(source, overlay, &prefix, record_segments, purpose);
    project_actions(
        &mut builder,
        overlay,
        purpose,
        &wrapper_actions,
        &try_end_actions,
        &parser_code_block_end_actions,
        &header_actions,
    )?;
    let mut mapped = builder.finish()?;
    mapped.synthetic_generator_spans = overlay
        .nodes
        .iter()
        .filter(|node| node.context != ControlContext::Statement || node.kind == ControlKind::Try)
        .map(|node| node.span)
        .collect();
    if record_segments && (!overlay.dynamic_tags.is_empty() || purpose == ProjectionPurpose::Parser)
    {
        mapped.dynamic_prefix = Some(prefix.clone());
    }
    if record_segments && !overlay.dynamic_tags.is_empty() {
        mapped.dynamic_count = to_u32(overlay.dynamic_tags.len())?;
        mapped.dynamic_offsets =
            overlay.dynamic_tags.iter().map(|tag| tag.expression.start).collect();
    }
    Ok(BuiltProjection { mapped, prefix, wrappers, headers, tries })
}

fn project_actions(
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

fn validate_projection_lane(
    overlay: &Overlay,
    purpose: ProjectionPurpose,
) -> Result<(), ProjectionError> {
    if purpose == ProjectionPurpose::Parser {
        validate_parser_code_blocks(overlay)?;
        return validate_parser_dynamic_boundaries(overlay);
    }
    if overlay.parser_dynamic_tokens.is_empty()
        && overlay.style_blocks.iter().all(|style| !style.self_closing)
    {
        Ok(())
    } else {
        Err(ProjectionError::StructuralMismatch)
    }
}

fn validate_parser_code_blocks(overlay: &Overlay) -> Result<(), ProjectionError> {
    let mut previous_token = None;
    for block in &overlay.parser_code_blocks {
        let token =
            overlay.tokens.get(block.token as usize).ok_or(ProjectionError::StructuralMismatch)?;
        if token.kind != StructuralKind::FunctionBody
            || block.body.start != token.span.end
            || block.body.end <= block.body.start
            || block.body.end > overlay.source_len
            || previous_token.is_some_and(|previous| previous >= block.token)
        {
            return Err(ProjectionError::StructuralMismatch);
        }
        previous_token = Some(block.token);
    }
    Ok(())
}

fn validate_parser_dynamic_boundaries(overlay: &Overlay) -> Result<(), ProjectionError> {
    if overlay.dynamic_tags.is_empty() {
        return if overlay.parser_dynamic_tokens.is_empty() {
            Ok(())
        } else {
            Err(ProjectionError::StructuralMismatch)
        };
    }
    if overlay.parser_dynamic_tokens.is_empty() {
        return Err(ProjectionError::StructuralMismatch);
    }

    let tag_count = to_u32(overlay.dynamic_tags.len())?;
    let mut next_owner = 0_u32;
    let mut previous_offset = None;
    let mut stack = Vec::<(u32, u8)>::with_capacity(overlay.dynamic_tags.len().min(16));

    validate_dynamic_subtree_bounds(overlay, tag_count, &mut stack)?;

    for token in &overlay.parser_dynamic_tokens {
        if previous_offset.is_some_and(|offset| token.offset < offset) {
            return Err(ProjectionError::StructuralMismatch);
        }
        previous_offset = Some(token.offset);
        let tag = overlay
            .dynamic_tags
            .get(token.owner as usize)
            .ok_or(ProjectionError::StructuralMismatch)?;
        match token.kind {
            ParserDynamicKind::OpenStart => {
                if token.owner != next_owner
                    || token.offset != tag.opening.start
                    || tag.subtree_end <= token.owner
                    || tag.subtree_end > tag_count
                {
                    return Err(ProjectionError::StructuralMismatch);
                }
                stack.push((token.owner, 1));
                next_owner = next_owner.checked_add(1).ok_or(ProjectionError::SourceTooLarge)?;
            }
            ParserDynamicKind::OpenEnd => {
                if stack.last() != Some(&(token.owner, 1)) || token.offset != tag.expression.end {
                    return Err(ProjectionError::StructuralMismatch);
                }
                if tag.self_closing {
                    stack.pop();
                } else if let Some((_, phase)) = stack.last_mut() {
                    *phase = 2;
                }
            }
            ParserDynamicKind::CloseStart => {
                if tag.self_closing
                    || stack.last() != Some(&(token.owner, 2))
                    || token.offset != tag.closing.start
                {
                    return Err(ProjectionError::StructuralMismatch);
                }
                if let Some((_, phase)) = stack.last_mut() {
                    *phase = 3;
                }
            }
            ParserDynamicKind::CloseEnd => {
                if tag.self_closing
                    || stack.last() != Some(&(token.owner, 3))
                    || token.offset != tag.closing_expression.end
                {
                    return Err(ProjectionError::StructuralMismatch);
                }
                stack.pop();
            }
        }
    }
    if next_owner == tag_count && stack.is_empty() {
        Ok(())
    } else {
        Err(ProjectionError::StructuralMismatch)
    }
}

fn validate_dynamic_subtree_bounds(
    overlay: &Overlay,
    tag_count: u32,
    stack: &mut Vec<(u32, u8)>,
) -> Result<(), ProjectionError> {
    // Dynamic owners are assigned in opening-source preorder. Validate every exclusive subtree
    // bound from the authored element ranges before using those bounds as identity-scan jumps.
    // The caller reuses this stack allocation for boundary-event phases.
    let mut previous_opening = None;
    for (index, tag) in overlay.dynamic_tags.iter().enumerate() {
        let owner = to_u32(index)?;
        let full_end = tag.closing.end;
        if previous_opening.is_some_and(|start| tag.opening.start <= start)
            || tag.opening.start >= full_end
            || tag.self_closing != tag.closing.is_empty()
            || (tag.self_closing && tag.closing.end <= tag.opening.end)
            || tag.subtree_end <= owner
            || tag.subtree_end > tag_count
        {
            return Err(ProjectionError::StructuralMismatch);
        }
        previous_opening = Some(tag.opening.start);

        while stack.last().is_some_and(|&(active, _)| {
            let active = &overlay.dynamic_tags[active as usize];
            let active_end = active.closing.end;
            tag.opening.start >= active_end
        }) {
            let (completed, _) = stack.pop().ok_or(ProjectionError::StructuralMismatch)?;
            if overlay.dynamic_tags[completed as usize].subtree_end != owner {
                return Err(ProjectionError::StructuralMismatch);
            }
        }

        if stack.last().is_some_and(|&(parent, _)| {
            let parent = &overlay.dynamic_tags[parent as usize];
            let parent_end = parent.closing.end;
            full_end > parent_end
        }) {
            return Err(ProjectionError::StructuralMismatch);
        }
        stack.push((owner, 0));
    }
    while let Some((completed, _)) = stack.pop() {
        if overlay.dynamic_tags[completed as usize].subtree_end != tag_count {
            return Err(ProjectionError::StructuralMismatch);
        }
    }
    Ok(())
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

fn validate_overlay_source(source: &str, overlay: &Overlay) -> Result<(), ProjectionError> {
    if source.len() != overlay.source_len as usize
        || source_fingerprint(source.as_bytes()) != overlay.source_fingerprint
    {
        return Err(ProjectionError::SourceChanged { offset: 0 });
    }
    Ok(())
}

fn collision_free_prefix(source: &str) -> Result<String, ProjectionError> {
    for nonce in 0..=1024_u16 {
        let prefix = format!("_t{nonce:x}_");
        if !source.contains(&prefix) {
            return Ok(prefix);
        }
    }
    Err(ProjectionError::MarkerSpaceExhausted)
}
