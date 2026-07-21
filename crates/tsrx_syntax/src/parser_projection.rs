#![allow(dead_code)]

use std::{fmt::Write as _, ops::Range};

use crate::{
    diagnostics::{ProjectionError, to_u32},
    model::{
        ByteSpan, ClauseRole, ControlContext, ControlKind, EmbeddedKind, NONE, Overlay,
        ParserDynamicKind, StructuralKind,
    },
    scanner::{Scanner, source_fingerprint},
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

/// Legal TSX for TypeScript-Go plus an authored-byte map.
///
/// This is deliberately distinct from [`MappedProjection`]. The syntax-lint projection only has
/// to satisfy OXC's parser and built-in rules; the type projection also declares its synthetic
/// helpers so they cannot erase surrounding TypeScript types or create false missing-name errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeProjection {
    projected: String,
    segments: Vec<ProjectionSegment>,
}

impl TypeProjection {
    #[must_use]
    pub fn source(&self) -> &str {
        &self.projected
    }

    #[must_use]
    pub fn view(&self) -> ProjectionView<'_> {
        ProjectionView {
            source: &self.projected,
            segments: &self.segments,
        }
    }

    /// Maps a diagnostic whose first and last bytes are both anchored in authored source.
    ///
    /// TypeScript diagnostics can span synthetic control scaffolding between two authored tokens.
    /// The endpoints must still map monotonically; diagnostics wholly inside generated helpers are
    /// rejected.
    #[must_use]
    pub fn map_range(&self, range: Range<u32>) -> Option<Range<u32>> {
        if range.start > range.end {
            return None;
        }
        if range.is_empty() {
            return self
                .map_endpoint(range.start, true)
                .map(|point| point..point);
        }
        let start = self.map_endpoint(range.start, true)?;
        let end = self.map_endpoint(range.end, false)?;
        (start <= end).then_some(start..end)
    }

    /// Maps a fix only when the complete edit is one unchanged affine authored segment.
    #[must_use]
    pub fn map_fix_range(&self, range: Range<u32>) -> Option<Range<u32>> {
        map_single_segment(&self.segments, range, true)
    }

    fn map_endpoint(&self, point: u32, start: bool) -> Option<u32> {
        self.segments.iter().find_map(|segment| {
            let contains = if start {
                segment.projected.start <= point && point < segment.projected.end
            } else {
                segment.projected.start < point && point <= segment.projected.end
            };
            contains.then(|| segment.original_start + (point - segment.projected.start))
        })
    }
}

impl MappedProjection {
    #[must_use]
    pub fn source(&self) -> &str {
        &self.projected
    }

    #[must_use]
    pub fn view(&self) -> ProjectionView<'_> {
        ProjectionView {
            source: &self.projected,
            segments: &self.segments,
        }
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
        self.synthetic_generator_spans
            .iter()
            .any(|span| span.intersects(range.start, range.end))
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
struct TokenManifest {
    kind: StructuralKind,
    owner: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TryManifest {
    node: u32,
    context: ControlContext,
    flags: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DynamicManifest {
    self_closing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StyleManifest {
    payload: ByteSpan,
}

impl TryManifest {
    const HAS_PENDING: u8 = 1;
    const HAS_CATCH: u8 = 1 << 1;
    const CATCH_HAS_HEADER: u8 = 1 << 2;
    const AUTHORED_SEMICOLON: u8 = 1 << 3;

    const fn has_pending(self) -> bool {
        self.flags & Self::HAS_PENDING != 0
    }

    const fn has_catch(self) -> bool {
        self.flags & Self::HAS_CATCH != 0
    }

    const fn catch_has_header(self) -> bool {
        self.flags & Self::CATCH_HAS_HEADER != 0
    }

    const fn authored_semicolon(self) -> bool {
        self.flags & Self::AUTHORED_SEMICOLON != 0
    }
}

/// Legal TSX plus the compact manifest required to lift canonical Oxfmt output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatProjection {
    projected: String,
    prefix: String,
    tokens: Vec<TokenManifest>,
    wrappers: Vec<WrapperManifest>,
    headers: Vec<HeaderManifest>,
    tries: Vec<TryManifest>,
    try_slots: Vec<u32>,
    dynamics: Vec<DynamicManifest>,
    dynamic_count: u32,
    dynamic_offsets: Vec<u32>,
    dynamic_comments: Vec<ByteSpan>,
    styles: Vec<StyleManifest>,
    shape_fingerprint: u128,
}

impl FormatProjection {
    #[must_use]
    pub fn source(&self) -> &str {
        &self.projected
    }

    #[must_use]
    pub fn marker_count(&self) -> usize {
        self.tokens.len() + self.dynamics.len() + self.dynamic_comments.len() + self.styles.len()
    }

    #[must_use]
    pub fn style_count(&self) -> usize {
        self.styles.len()
    }

    /// Returns the collision-free synthetic dynamic-tag namespace and expected tag count.
    #[must_use]
    pub fn dynamic_contract(&self) -> Option<(&str, u32, &[u32])> {
        (!self.dynamics.is_empty()).then_some((
            self.prefix.as_str(),
            self.dynamic_count,
            self.dynamic_offsets.as_slice(),
        ))
    }
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
            Self::ParserCodeBlockEnd(block) => (
                overlay.parser_code_blocks[block as usize]
                    .body
                    .end
                    .saturating_sub(1),
                0,
            ),
            Self::WrapperEnd(node) => (overlay.nodes[node as usize].span.end, 1),
            Self::WrapperStart(node) => (overlay.nodes[node as usize].span.start, 2),
            Self::Token(token) => (overlay.tokens[token as usize].span.start, 3),
            Self::Header { clause, .. } => (overlay.clauses[clause as usize].header.start, 3),
            Self::ForBody(clause) => (
                overlay.clauses[clause as usize]
                    .body
                    .start
                    .saturating_add(1),
                0,
            ),
            Self::Embedded(token) => (overlay.embedded_tokens[token as usize].span.start, 3),
            Self::ParserDynamic(token) => (overlay.parser_dynamic_tokens[token as usize].offset, 2),
        }
    }
}

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
        self.synthetic_callee_spans
            .push((to_u32(start)?, to_u32(self.output.len())?));
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
        let projected_start = self
            .record_segments
            .then(|| to_u32(self.output.len()))
            .transpose()?;
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
            return Err(ProjectionError::SourceChanged {
                offset: token.span.start,
            });
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
                    return Err(ProjectionError::SourceChanged {
                        offset: token.span.start,
                    });
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
        self.overlay
            .parser_code_blocks
            .binary_search_by_key(&token, |block| block.token)
            .ok()
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
        let closing = block
            .body
            .end
            .checked_sub(1)
            .ok_or(ProjectionError::StructuralMismatch)? as usize;
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
            return Err(ProjectionError::ScaffoldMismatch {
                index: ordinal as usize,
            });
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
            .ok_or(ProjectionError::SourceChanged {
                offset: header.left.start,
            })?;
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

    #[allow(clippy::too_many_lines)]
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
                    return Err(ProjectionError::SourceChanged {
                        offset: token.span.start,
                    });
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
                    return Err(ProjectionError::SourceChanged {
                        offset: token.span.start,
                    });
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
                            .ok_or(ProjectionError::SourceChanged {
                                offset: comment.start,
                            })?;
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
                write!(
                    self.output,
                    "{{/*{}S{}__*/ null}}",
                    self.prefix, token.owner
                )
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

/// Performs the legacy equal-width projection used by standard-syntax control baselines.
///
/// Expanded JSX-child and expression controls require [`project_for_lint`] or
/// [`project_for_format`].
///
/// # Errors
///
/// Returns an error when `overlay` was produced from different source bytes.
pub fn project(source: &str, overlay: &Overlay) -> Result<String, ProjectionError> {
    validate_overlay_source(source, overlay)?;
    if let Some(token) = overlay.embedded_tokens.first() {
        return Err(ProjectionError::UnsupportedSyntax {
            offset: token.span.start,
            construct: "embedded syntax in the legacy equal-width projection",
        });
    }
    let mut bytes = source.as_bytes().to_vec();
    for token in &overlay.tokens {
        bytes[token.span.start as usize] = b' ';
    }
    String::from_utf8(bytes).map_err(|_| ProjectionError::SourceChanged { offset: 0 })
}

/// Builds a legal-TSX projection with explicit affine source-map segments.
///
/// # Errors
///
/// Returns an error for a stale overlay or a projection scaffold collision.
pub fn project_for_lint(
    source: &str,
    overlay: &Overlay,
) -> Result<MappedProjection, ProjectionError> {
    Ok(build_projection(source, overlay, true)?.mapped)
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

/// Builds the Rust-native TypeScript-Go projection.
///
/// Synthetic helpers are declared after the projected module. Keeping the declarations at the end
/// preserves every authored/scaffold offset shared with the normal syntax-lint projection, which
/// lets one OXC parse supply disable-directive spans without a second parser pass.
///
/// # Errors
///
/// Returns an error for a stale overlay or a projection scaffold collision.
pub fn project_for_types(
    source: &str,
    overlay: &Overlay,
) -> Result<TypeProjection, ProjectionError> {
    let built = build_projection_with_purpose(source, overlay, true, ProjectionPurpose::Types)?;
    let mut projected = built.mapped.projected;
    append_type_helper_declarations(&mut projected, overlay, &built.prefix);
    Ok(TypeProjection {
        projected,
        segments: built.mapped.segments,
    })
}

fn append_type_helper_declarations(output: &mut String, overlay: &Overlay, prefix: &str) {
    if overlay.nodes.is_empty()
        && overlay.dynamic_tags.is_empty()
        && overlay
            .clauses
            .iter()
            .all(|clause| !clause.for_header.annotated)
    {
        return;
    }
    output.push_str("\n/* OXC for TSRX type-only projection helpers. */\n");
    for (index, node) in overlay.nodes.iter().enumerate() {
        if node.context != ControlContext::Statement {
            writeln!(
                output,
                "declare function {prefix}W{index}_<T>(value: T, end: unknown): any;"
            )
            .expect("writing to a String cannot fail");
            writeln!(output, "declare const {prefix}E{index}_: unique symbol;")
                .expect("writing to a String cannot fail");
        }
        if node.kind == ControlKind::Try {
            writeln!(
                output,
                "declare function {prefix}T{index}_(value: {{ {prefix}B{index}_(): AsyncGenerator<unknown>; {prefix}P{index}_?(): AsyncGenerator<unknown>; {prefix}C{index}_?(error: unknown, reset: () => void): AsyncGenerator<unknown>; }}, end: unknown): any;"
            )
            .expect("writing to a String cannot fail");
            writeln!(output, "declare const {prefix}TE{index}_: unique symbol;")
                .expect("writing to a String cannot fail");
        }
    }
    for (ordinal, clause) in overlay
        .clauses
        .iter()
        .filter(|clause| clause.for_header.annotated)
        .enumerate()
    {
        writeln!(
            output,
            "declare function {prefix}H{ordinal}_<T>(value: T, ...metadata: unknown[]): T;"
        )
        .expect("writing to a String cannot fail");
        if !clause.for_header.index.is_empty() {
            writeln!(
                output,
                "declare function {prefix}IH{ordinal}_<T>(value: T): T;"
            )
            .expect("writing to a String cannot fail");
        }
        if !clause.for_header.key.is_empty() {
            writeln!(
                output,
                "declare function {prefix}KH{ordinal}_<T>(value: T): T;"
            )
            .expect("writing to a String cannot fail");
        }
        writeln!(output, "declare const {prefix}HE{ordinal}_: unique symbol;")
            .expect("writing to a String cannot fail");
    }
    for index in 0..overlay.dynamic_tags.len() {
        writeln!(output, "declare const {prefix}D{index}: any;")
            .expect("writing to a String cannot fail");
    }
}

/// Builds a legal-TSX formatter projection and checked lift manifest.
///
/// # Errors
///
/// Returns an error for a stale overlay or a projection scaffold collision.
pub fn project_for_format(
    source: &str,
    overlay: &Overlay,
) -> Result<FormatProjection, ProjectionError> {
    let built = build_projection(source, overlay, false)?;
    let mut try_slots = vec![NONE; overlay.nodes.len()];
    for (slot, manifest) in built.tries.iter().enumerate() {
        try_slots[manifest.node as usize] = to_u32(slot)?;
    }
    let styles = overlay
        .style_blocks
        .iter()
        .map(|style| StyleManifest {
            payload: style.content,
        })
        .collect();
    let dynamic_count = to_u32(overlay.dynamic_tags.len())?;
    Ok(FormatProjection {
        projected: built.mapped.projected,
        prefix: built.prefix,
        tokens: overlay
            .tokens
            .iter()
            .map(|token| TokenManifest {
                kind: token.kind,
                owner: token.owner,
            })
            .collect(),
        wrappers: built.wrappers,
        headers: built.headers,
        tries: built.tries,
        try_slots,
        dynamics: overlay
            .dynamic_tags
            .iter()
            .map(|tag| DynamicManifest {
                self_closing: tag.self_closing,
            })
            .collect(),
        dynamic_count,
        dynamic_offsets: overlay
            .dynamic_tags
            .iter()
            .map(|tag| tag.expression.start)
            .collect(),
        dynamic_comments: overlay.dynamic_comments.clone(),
        styles,
        shape_fingerprint: structural_fingerprint(overlay),
    })
}

fn build_projection(
    source: &str,
    overlay: &Overlay,
    record_segments: bool,
) -> Result<BuiltProjection, ProjectionError> {
    build_projection_with_purpose(source, overlay, record_segments, ProjectionPurpose::Syntax)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectionPurpose {
    Syntax,
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
        mapped.dynamic_offsets = overlay
            .dynamic_tags
            .iter()
            .map(|tag| tag.expression.start)
            .collect();
    }
    Ok(BuiltProjection {
        mapped,
        prefix,
        wrappers,
        headers,
        tries,
    })
}

#[allow(clippy::too_many_arguments)]
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
        let parser_code_block_end = parser_code_block_end_actions
            .get(parser_code_block_end_cursor)
            .copied();
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
        let Some(action) = [
            wrapper,
            try_end,
            parser_code_block_end,
            token,
            header,
            embedded,
            parser_dynamic,
        ]
        .into_iter()
        .flatten()
        .min_by_key(|action| action.key(overlay)) else {
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
        let token = overlay
            .tokens
            .get(block.token as usize)
            .ok_or(ProjectionError::StructuralMismatch)?;
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
                next_owner = next_owner
                    .checked_add(1)
                    .ok_or(ProjectionError::SourceTooLarge)?;
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
            manifests.push(WrapperManifest {
                node: node_index,
                context: node.context,
            });
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
                actions.push(Action::Header {
                    clause: clause_index,
                    ordinal,
                });
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
            actions.push(Action::TryEnd(
                active.pop().ok_or(ProjectionError::StructuralMismatch)?,
            ));
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
        manifests.push(TryManifest {
            node: node_index,
            context: node.context,
            flags,
        });
    }
    while let Some(node) = active.pop() {
        actions.push(Action::TryEnd(node));
    }
    Ok((actions, manifests))
}

/// Lifts canonical Oxfmt output back into TSRX after validating every synthetic scaffold.
///
/// # Errors
///
/// Returns an error if Oxfmt changed or duplicated scaffolding, or if the lifted structure no
/// longer matches the source overlay.
pub fn lift_formatted(
    formatted: &str,
    original_source: &str,
    projection: &FormatProjection,
) -> Result<String, ProjectionError> {
    let lifted = lift_scaffolds(formatted, projection)?;
    let lifted = if projection.dynamics.is_empty()
        && projection.dynamic_comments.is_empty()
        && projection.styles.is_empty()
    {
        lifted
    } else {
        lift_embedded(&lifted, original_source, projection)?
    };
    let lifted = lift_tokens(&lifted, projection)?;
    if lifted.contains(&projection.prefix) {
        return Err(ProjectionError::MarkerResidual);
    }
    let rescanned = Scanner::new(&lifted).finish()?;
    if structural_fingerprint(&rescanned) != projection.shape_fingerprint {
        return Err(ProjectionError::StructuralMismatch);
    }
    Ok(lifted)
}

fn lift_embedded(
    source: &str,
    original_source: &str,
    projection: &FormatProjection,
) -> Result<String, ProjectionError> {
    let bytes = source.as_bytes();
    let dynamic_open = format!("<{}D", projection.prefix);
    let dynamic_close = format!("</{}D", projection.prefix);
    let comment_marker = format!("{{/*{}Q", projection.prefix);
    let style_marker = format!("{{/*{}S", projection.prefix);
    let mut expressions = vec![ScaffoldSpan::MISSING; projection.dynamics.len()];
    let mut opened = vec![false; projection.dynamics.len()];
    let mut closed = vec![false; projection.dynamics.len()];
    let mut comments = vec![false; projection.dynamic_comments.len()];
    let mut styles = vec![false; projection.styles.len()];
    let restored_bytes = projection
        .styles
        .iter()
        .map(|manifest| (manifest.payload.end - manifest.payload.start) as usize)
        .chain(
            projection
                .dynamic_comments
                .iter()
                .map(|span| (span.end - span.start) as usize),
        )
        .fold(0usize, usize::saturating_add);
    let mut output = String::with_capacity(source.len().saturating_add(restored_bytes));
    let mut copied = 0usize;
    let mut cursor = 0usize;
    while cursor < source.len() {
        if source[cursor..].starts_with(&dynamic_close) {
            let digits_start = cursor + dynamic_close.len();
            let (ordinal, digits_end) =
                parse_decimal(bytes, digits_start).ok_or(ProjectionError::MarkerResidual)?;
            let index = ordinal as usize;
            let manifest = projection
                .dynamics
                .get(index)
                .ok_or(ProjectionError::MarkerResidual)?;
            if manifest.self_closing || closed[index] || expressions[index].is_missing() {
                return Err(ProjectionError::ScaffoldMismatch { index });
            }
            let end = expect_byte_after_whitespace(source, digits_end, b'>', index)?;
            output.push_str(&source[copied..cursor]);
            output.push_str("</{");
            let expression = expressions[index];
            output.push_str(&source[expression.start..expression.end]);
            output.push_str("}>");
            copied = end;
            cursor = end;
            closed[index] = true;
            continue;
        }

        if source[cursor..].starts_with(&dynamic_open) {
            let digits_start = cursor + dynamic_open.len();
            let (ordinal, digits_end) =
                parse_decimal(bytes, digits_start).ok_or(ProjectionError::MarkerResidual)?;
            let index = ordinal as usize;
            if projection.dynamics.get(index).is_none() || opened[index] {
                return Err(ProjectionError::ScaffoldMismatch { index });
            }
            let attribute_start = skip_ascii_whitespace(source, digits_end);
            let attribute = format!("{}A{ordinal}_", projection.prefix);
            let attribute_end = attribute_start.saturating_add(attribute.len());
            if source.as_bytes().get(attribute_start..attribute_end) != Some(attribute.as_bytes()) {
                return Err(ProjectionError::ScaffoldMismatch { index });
            }
            let mut expression_open =
                expect_byte_after_whitespace(source, attribute_end, b'=', index)?;
            expression_open = skip_ascii_whitespace(source, expression_open);
            if source.as_bytes().get(expression_open) != Some(&b'{') {
                return Err(ProjectionError::ScaffoldMismatch { index });
            }
            let sentinel = format!("{}Z{ordinal}_", projection.prefix);
            let sentinel_start = source[expression_open + 1..]
                .find(&sentinel)
                .map(|relative| expression_open + 1 + relative)
                .ok_or(ProjectionError::ScaffoldMismatch { index })?;
            let expression_close = source
                .as_bytes()
                .get(..sentinel_start)
                .ok_or(ProjectionError::ScaffoldMismatch { index })?
                .iter()
                .rposition(|byte| !byte.is_ascii_whitespace())
                .filter(|position| source.as_bytes()[*position] == b'}')
                .ok_or(ProjectionError::ScaffoldMismatch { index })?;
            let expression = trimmed_content_range(source, expression_open + 1, expression_close)?;
            let mut sentinel_end =
                expect_byte_after_whitespace(source, sentinel_start + sentinel.len(), b'=', index)?;
            sentinel_end = expect_byte_after_whitespace(source, sentinel_end, b'{', index)?;
            sentinel_end = expect_word_after_whitespace(source, sentinel_end, b"null", index)?;
            sentinel_end = expect_byte_after_whitespace(source, sentinel_end, b'}', index)?;
            output.push_str(&source[copied..cursor]);
            output.push_str("<{");
            output.push_str(&source[expression.clone()]);
            output.push('}');
            copied = sentinel_end;
            cursor = copied;
            expressions[index] = ScaffoldSpan {
                start: expression.start,
                end: expression.end,
            };
            opened[index] = true;
            continue;
        }

        if source[cursor..].starts_with(&comment_marker) {
            let digits_start = cursor + comment_marker.len();
            let (ordinal, digits_end) =
                parse_decimal(bytes, digits_start).ok_or(ProjectionError::MarkerResidual)?;
            let index = ordinal as usize;
            let span = *projection
                .dynamic_comments
                .get(index)
                .ok_or(ProjectionError::MarkerResidual)?;
            if comments[index] || source.as_bytes().get(digits_end..digits_end + 4) != Some(b"__*/")
            {
                return Err(ProjectionError::ScaffoldMismatch { index });
            }
            let mut end = expect_word_after_whitespace(source, digits_end + 4, b"null", index)?;
            end = expect_byte_after_whitespace(source, end, b'}', index)?;
            let comment = original_source
                .get(span.start as usize..span.end as usize)
                .ok_or(ProjectionError::StructuralMismatch)?;
            output.push_str(&source[copied..cursor]);
            output.push_str(comment);
            copied = end;
            cursor = end;
            comments[index] = true;
            continue;
        }

        if source[cursor..].starts_with(&style_marker) {
            let digits_start = cursor + style_marker.len();
            let (ordinal, digits_end) =
                parse_decimal(bytes, digits_start).ok_or(ProjectionError::MarkerResidual)?;
            let index = ordinal as usize;
            let manifest = projection
                .styles
                .get(index)
                .ok_or(ProjectionError::MarkerResidual)?;
            if styles[index] || source.as_bytes().get(digits_end..digits_end + 4) != Some(b"__*/") {
                return Err(ProjectionError::ScaffoldMismatch { index });
            }
            let mut end = expect_word_after_whitespace(source, digits_end + 4, b"null", index)?;
            end = expect_byte_after_whitespace(source, end, b'}', index)?;
            let payload = original_source
                .get(manifest.payload.start as usize..manifest.payload.end as usize)
                .ok_or(ProjectionError::StructuralMismatch)?;
            output.push_str(&source[copied..cursor]);
            output.push_str(payload);
            copied = end;
            cursor = end;
            styles[index] = true;
            continue;
        }
        cursor += source[cursor..].chars().next().map_or(1, char::len_utf8);
    }
    output.push_str(&source[copied..]);

    for (index, manifest) in projection.dynamics.iter().enumerate() {
        if !opened[index] || (!manifest.self_closing && !closed[index]) {
            return Err(ProjectionError::ScaffoldMismatch { index });
        }
    }
    if styles.iter().any(|seen| !seen) {
        let index = styles.iter().position(|seen| !seen).unwrap_or(0);
        return Err(ProjectionError::ScaffoldMismatch { index });
    }
    if comments.iter().any(|seen| !seen) {
        let index = comments.iter().position(|seen| !seen).unwrap_or(0);
        return Err(ProjectionError::ScaffoldMismatch { index });
    }
    Ok(output)
}

const MISSING_POSITION: usize = usize::MAX;

#[derive(Clone, Copy, PartialEq, Eq)]
struct ScaffoldSpan {
    start: usize,
    end: usize,
}

impl ScaffoldSpan {
    const MISSING: Self = Self {
        start: MISSING_POSITION,
        end: MISSING_POSITION,
    };

    const fn is_missing(self) -> bool {
        self.start == MISSING_POSITION
    }
}

#[derive(Clone, Copy)]
struct IndexedWrapper {
    wrapper: ScaffoldSpan,
    method: ScaffoldSpan,
    start_marker: ScaffoldSpan,
    end_marker: ScaffoldSpan,
    end_sentinel: ScaffoldSpan,
}

impl Default for IndexedWrapper {
    fn default() -> Self {
        Self {
            wrapper: ScaffoldSpan::MISSING,
            method: ScaffoldSpan::MISSING,
            start_marker: ScaffoldSpan::MISSING,
            end_marker: ScaffoldSpan::MISSING,
            end_sentinel: ScaffoldSpan::MISSING,
        }
    }
}

#[derive(Clone, Copy)]
struct IndexedHeader {
    helper: ScaffoldSpan,
    right_start: ScaffoldSpan,
    right_end: ScaffoldSpan,
    index_helper: ScaffoldSpan,
    index_start: ScaffoldSpan,
    index_end: ScaffoldSpan,
    key_helper: ScaffoldSpan,
    key_start: ScaffoldSpan,
    key_end: ScaffoldSpan,
    end_sentinel: ScaffoldSpan,
}

#[derive(Clone, Copy)]
struct IndexedTry {
    call: ScaffoldSpan,
    body_method: ScaffoldSpan,
    pending_method: ScaffoldSpan,
    catch_method: ScaffoldSpan,
    end_sentinel: ScaffoldSpan,
    try_marker: ScaffoldSpan,
    pending_marker: ScaffoldSpan,
    catch_marker: ScaffoldSpan,
}

struct IndexedScaffolds {
    wrappers: Vec<IndexedWrapper>,
    headers: Vec<IndexedHeader>,
    tries: Vec<IndexedTry>,
}

impl Default for IndexedTry {
    fn default() -> Self {
        Self {
            call: ScaffoldSpan::MISSING,
            body_method: ScaffoldSpan::MISSING,
            pending_method: ScaffoldSpan::MISSING,
            catch_method: ScaffoldSpan::MISSING,
            end_sentinel: ScaffoldSpan::MISSING,
            try_marker: ScaffoldSpan::MISSING,
            pending_marker: ScaffoldSpan::MISSING,
            catch_marker: ScaffoldSpan::MISSING,
        }
    }
}

#[derive(Clone, Copy)]
enum WrapperReplacement {
    Empty,
    Try,
}

impl WrapperReplacement {
    const fn text(self) -> &'static str {
        match self {
            Self::Empty => "",
            Self::Try => "@try ",
        }
    }
}

impl Default for IndexedHeader {
    fn default() -> Self {
        Self {
            helper: ScaffoldSpan::MISSING,
            right_start: ScaffoldSpan::MISSING,
            right_end: ScaffoldSpan::MISSING,
            index_helper: ScaffoldSpan::MISSING,
            index_start: ScaffoldSpan::MISSING,
            index_end: ScaffoldSpan::MISSING,
            key_helper: ScaffoldSpan::MISSING,
            key_start: ScaffoldSpan::MISSING,
            key_end: ScaffoldSpan::MISSING,
            end_sentinel: ScaffoldSpan::MISSING,
        }
    }
}

#[derive(Clone, Copy)]
struct WrapperEdit {
    index: usize,
    replace_start: usize,
    content_start: usize,
    content_end: usize,
    replace_end: usize,
    dedent: usize,
    replacement: WrapperReplacement,
}

#[derive(Clone, Copy)]
enum EditReplacement {
    Empty,
    Index,
    Key,
    Pending,
    Catch,
}

impl EditReplacement {
    const fn text(self) -> &'static str {
        match self {
            Self::Empty => "",
            Self::Index => "; index ",
            Self::Key => "; key ",
            Self::Pending => " @pending ",
            Self::Catch => " @catch ",
        }
    }
}

#[derive(Clone, Copy)]
struct ScaffoldEdit {
    index: usize,
    start: usize,
    end: usize,
    replacement: EditReplacement,
}

fn lift_scaffolds(
    formatted: &str,
    projection: &FormatProjection,
) -> Result<String, ProjectionError> {
    if projection.wrappers.is_empty()
        && projection.headers.is_empty()
        && projection.tries.is_empty()
    {
        return Ok(formatted.to_string());
    }
    let indexed = index_scaffolds(formatted, projection)?;
    let regular_wrappers = projection
        .wrappers
        .iter()
        .copied()
        .zip(indexed.wrappers)
        .map(|(manifest, positions)| wrapper_edit(formatted, manifest, positions))
        .collect::<Result<Vec<_>, _>>()?;
    let try_wrappers = projection
        .tries
        .iter()
        .copied()
        .zip(indexed.tries.iter().copied())
        .map(|(manifest, positions)| try_wrapper_edit(formatted, manifest, positions))
        .collect::<Result<Vec<_>, _>>()?;
    let wrappers = merge_wrappers(regular_wrappers, try_wrappers);

    let mut header_edits = Vec::with_capacity(projection.headers.len().saturating_mul(4));
    for (manifest, positions) in projection.headers.iter().copied().zip(indexed.headers) {
        append_header_edits(formatted, manifest, positions, &mut header_edits)?;
    }
    let mut try_edits = Vec::with_capacity(projection.tries.len().saturating_mul(2));
    for (token_index, token) in projection.tokens.iter().copied().enumerate() {
        if !matches!(token.kind, StructuralKind::Pending | StructuralKind::Catch) {
            continue;
        }
        let slot = try_slot(projection, token.owner)?;
        let manifest = projection.tries[slot];
        let positions = indexed.tries[slot];
        try_edits.push(try_clause_edit(
            formatted,
            token_index,
            token.kind,
            manifest,
            positions,
        )?);
    }
    let edits = merge_edits(header_edits, try_edits);
    render_scaffolds(formatted, &wrappers, &edits)
}

#[allow(clippy::too_many_lines)]
fn index_scaffolds(
    source: &str,
    projection: &FormatProjection,
) -> Result<IndexedScaffolds, ProjectionError> {
    let mut wrappers = vec![IndexedWrapper::default(); projection.wrappers.len()];
    let mut headers = vec![IndexedHeader::default(); projection.headers.len()];
    let mut tries = vec![IndexedTry::default(); projection.tries.len()];
    let bytes = source.as_bytes();
    let mut cursor = 0usize;
    while let Some(relative) = source[cursor..].find(&projection.prefix) {
        let prefix_start = cursor + relative;
        let suffix_start = prefix_start + projection.prefix.len();
        let Some(&kind) = bytes.get(suffix_start) else {
            return Err(ProjectionError::MarkerResidual);
        };
        match kind {
            b'0'..=b'9' => {
                let (token_index, span) =
                    parse_token_marker_occurrence(bytes, prefix_start, suffix_start)
                        .ok_or(ProjectionError::MarkerResidual)?;
                let token = *projection
                    .tokens
                    .get(token_index as usize)
                    .ok_or(ProjectionError::MarkerResidual)?;
                if matches!(
                    token.kind,
                    StructuralKind::Try | StructuralKind::Pending | StructuralKind::Catch
                ) {
                    let slot = try_slot(projection, token.owner)?;
                    let target = match token.kind {
                        StructuralKind::Try => &mut tries[slot].try_marker,
                        StructuralKind::Pending => &mut tries[slot].pending_marker,
                        StructuralKind::Catch => &mut tries[slot].catch_marker,
                        _ => unreachable!(),
                    };
                    set_scaffold_span(target, span, token_index as usize)?;
                }
            }
            b'W' | b'M' | b'E' => {
                let (node, span) =
                    parse_identifier_occurrence(bytes, prefix_start, suffix_start + 1)
                        .ok_or(ProjectionError::MarkerResidual)?;
                let slot = wrapper_slot(projection, node)?;
                let target = match kind {
                    b'W' => &mut wrappers[slot].wrapper,
                    b'M' => &mut wrappers[slot].method,
                    b'E' => &mut wrappers[slot].end_sentinel,
                    _ => unreachable!(),
                };
                set_scaffold_span(target, span, node as usize)?;
            }
            b'N' => {
                let (node, side, span) =
                    parse_marker_occurrence(bytes, prefix_start, suffix_start + 1)
                        .ok_or(ProjectionError::MarkerResidual)?;
                let slot = wrapper_slot(projection, node)?;
                let target = match side {
                    b'S' => &mut wrappers[slot].start_marker,
                    b'E' => &mut wrappers[slot].end_marker,
                    _ => return Err(ProjectionError::MarkerResidual),
                };
                set_scaffold_span(target, span, node as usize)?;
            }
            b'H' if bytes.get(suffix_start + 1) == Some(&b'E') => {
                let (ordinal, span) =
                    parse_identifier_occurrence(bytes, prefix_start, suffix_start + 2)
                        .ok_or(ProjectionError::MarkerResidual)?;
                let positions = header_positions_mut(&mut headers, ordinal)?;
                set_scaffold_span(&mut positions.end_sentinel, span, ordinal as usize)?;
            }
            b'H' => {
                let (ordinal, span) =
                    parse_identifier_occurrence(bytes, prefix_start, suffix_start + 1)
                        .ok_or(ProjectionError::MarkerResidual)?;
                set_scaffold_span(
                    &mut header_positions_mut(&mut headers, ordinal)?.helper,
                    span,
                    ordinal as usize,
                )?;
            }
            b'I' if bytes.get(suffix_start + 1) == Some(&b'H') => {
                let (ordinal, span) =
                    parse_identifier_occurrence(bytes, prefix_start, suffix_start + 2)
                        .ok_or(ProjectionError::MarkerResidual)?;
                set_scaffold_span(
                    &mut header_positions_mut(&mut headers, ordinal)?.index_helper,
                    span,
                    ordinal as usize,
                )?;
            }
            b'K' if bytes.get(suffix_start + 1) == Some(&b'H') => {
                let (ordinal, span) =
                    parse_identifier_occurrence(bytes, prefix_start, suffix_start + 2)
                        .ok_or(ProjectionError::MarkerResidual)?;
                set_scaffold_span(
                    &mut header_positions_mut(&mut headers, ordinal)?.key_helper,
                    span,
                    ordinal as usize,
                )?;
            }
            b'R' | b'I' | b'K' => {
                let (ordinal, side, span) =
                    parse_marker_occurrence(bytes, prefix_start, suffix_start + 1)
                        .ok_or(ProjectionError::MarkerResidual)?;
                let positions = header_positions_mut(&mut headers, ordinal)?;
                let target = match (kind, side) {
                    (b'R', b'S') => &mut positions.right_start,
                    (b'R', b'E') => &mut positions.right_end,
                    (b'I', b'S') => &mut positions.index_start,
                    (b'I', b'E') => &mut positions.index_end,
                    (b'K', b'S') => &mut positions.key_start,
                    (b'K', b'E') => &mut positions.key_end,
                    _ => return Err(ProjectionError::MarkerResidual),
                };
                set_scaffold_span(target, span, ordinal as usize)?;
            }
            b'T' if bytes.get(suffix_start + 1) == Some(&b'E') => {
                let (node, span) =
                    parse_identifier_occurrence(bytes, prefix_start, suffix_start + 2)
                        .ok_or(ProjectionError::MarkerResidual)?;
                let slot = try_slot(projection, node)?;
                set_scaffold_span(&mut tries[slot].end_sentinel, span, node as usize)?;
            }
            b'T' | b'B' | b'P' | b'C' => {
                let (node, span) =
                    parse_identifier_occurrence(bytes, prefix_start, suffix_start + 1)
                        .ok_or(ProjectionError::MarkerResidual)?;
                let slot = try_slot(projection, node)?;
                let target = match kind {
                    b'T' => &mut tries[slot].call,
                    b'B' => &mut tries[slot].body_method,
                    b'P' => &mut tries[slot].pending_method,
                    b'C' => &mut tries[slot].catch_method,
                    _ => unreachable!(),
                };
                set_scaffold_span(target, span, node as usize)?;
            }
            b'D' | b'A' | b'Z' | b'Q' | b'S' => {}
            _ => return Err(ProjectionError::MarkerResidual),
        }
        cursor = suffix_start + 1;
    }

    for (manifest, positions) in projection.wrappers.iter().copied().zip(&wrappers) {
        if positions.wrapper.is_missing()
            || positions.method.is_missing()
            || positions.start_marker.is_missing()
            || positions.end_marker.is_missing()
            || positions.end_sentinel.is_missing()
        {
            return Err(ProjectionError::ScaffoldMismatch {
                index: manifest.node as usize,
            });
        }
    }
    for (manifest, positions) in projection.headers.iter().copied().zip(&headers) {
        let index_positions = [
            positions.index_helper,
            positions.index_start,
            positions.index_end,
        ];
        let key_positions = [positions.key_helper, positions.key_start, positions.key_end];
        let index_positions_valid = if manifest.has_index {
            index_positions.iter().all(|span| !span.is_missing())
        } else {
            index_positions.iter().all(|span| span.is_missing())
        };
        let key_positions_valid = if manifest.has_key {
            key_positions.iter().all(|span| !span.is_missing())
        } else {
            key_positions.iter().all(|span| span.is_missing())
        };
        if positions.helper.is_missing()
            || positions.right_start.is_missing()
            || positions.right_end.is_missing()
            || positions.end_sentinel.is_missing()
            || !index_positions_valid
            || !key_positions_valid
        {
            return Err(ProjectionError::ScaffoldMismatch {
                index: manifest.ordinal as usize,
            });
        }
    }
    for (manifest, positions) in projection.tries.iter().copied().zip(&tries) {
        let pending_valid = if manifest.has_pending() {
            !positions.pending_method.is_missing() && !positions.pending_marker.is_missing()
        } else {
            positions.pending_method.is_missing() && positions.pending_marker.is_missing()
        };
        let catch_valid = if manifest.has_catch() {
            !positions.catch_method.is_missing() && !positions.catch_marker.is_missing()
        } else {
            positions.catch_method.is_missing() && positions.catch_marker.is_missing()
        };
        if positions.call.is_missing()
            || positions.body_method.is_missing()
            || positions.end_sentinel.is_missing()
            || positions.try_marker.is_missing()
            || !pending_valid
            || !catch_valid
        {
            return Err(ProjectionError::ScaffoldMismatch {
                index: manifest.node as usize,
            });
        }
    }
    Ok(IndexedScaffolds {
        wrappers,
        headers,
        tries,
    })
}

fn wrapper_slot(projection: &FormatProjection, node: u32) -> Result<usize, ProjectionError> {
    projection
        .wrappers
        .binary_search_by_key(&node, |wrapper| wrapper.node)
        .map_err(|_| ProjectionError::ScaffoldMismatch {
            index: node as usize,
        })
}

fn try_slot(projection: &FormatProjection, node: u32) -> Result<usize, ProjectionError> {
    let slot =
        *projection
            .try_slots
            .get(node as usize)
            .ok_or(ProjectionError::ScaffoldMismatch {
                index: node as usize,
            })?;
    if slot == NONE {
        return Err(ProjectionError::ScaffoldMismatch {
            index: node as usize,
        });
    }
    Ok(slot as usize)
}

fn header_positions_mut(
    headers: &mut [IndexedHeader],
    ordinal: u32,
) -> Result<&mut IndexedHeader, ProjectionError> {
    headers
        .get_mut(ordinal as usize)
        .ok_or(ProjectionError::ScaffoldMismatch {
            index: ordinal as usize,
        })
}

fn set_scaffold_span(
    target: &mut ScaffoldSpan,
    value: ScaffoldSpan,
    index: usize,
) -> Result<(), ProjectionError> {
    if !target.is_missing() {
        return Err(ProjectionError::ScaffoldMismatch { index });
    }
    *target = value;
    Ok(())
}

fn parse_identifier_occurrence(
    bytes: &[u8],
    prefix_start: usize,
    digits_start: usize,
) -> Option<(u32, ScaffoldSpan)> {
    let (ordinal, digits_end) = parse_decimal(bytes, digits_start)?;
    (bytes.get(digits_end) == Some(&b'_')).then_some((
        ordinal,
        ScaffoldSpan {
            start: prefix_start,
            end: digits_end + 1,
        },
    ))
}

fn parse_token_marker_occurrence(
    bytes: &[u8],
    prefix_start: usize,
    digits_start: usize,
) -> Option<(u32, ScaffoldSpan)> {
    let (ordinal, digits_end) = parse_decimal(bytes, digits_start)?;
    if prefix_start < 2
        || bytes.get(prefix_start - 2..prefix_start) != Some(b"/*")
        || bytes.get(digits_end..digits_end + 2) != Some(b"*/")
    {
        return None;
    }
    Some((
        ordinal,
        ScaffoldSpan {
            start: prefix_start - 2,
            end: digits_end + 2,
        },
    ))
}

fn parse_marker_occurrence(
    bytes: &[u8],
    prefix_start: usize,
    digits_start: usize,
) -> Option<(u32, u8, ScaffoldSpan)> {
    let (ordinal, digits_end) = parse_decimal(bytes, digits_start)?;
    let side = *bytes.get(digits_end)?;
    if !matches!(side, b'S' | b'E')
        || bytes.get(digits_end + 1..digits_end + 5) != Some(b"__*/")
        || prefix_start < 2
        || bytes.get(prefix_start - 2..prefix_start) != Some(b"/*")
    {
        return None;
    }
    Some((
        ordinal,
        side,
        ScaffoldSpan {
            start: prefix_start - 2,
            end: digits_end + 5,
        },
    ))
}

fn parse_decimal(bytes: &[u8], mut index: usize) -> Option<(u32, usize)> {
    let start = index;
    let mut value = 0u32;
    while let Some(byte @ b'0'..=b'9') = bytes.get(index) {
        value = value
            .checked_mul(10)?
            .checked_add(u32::from(*byte - b'0'))?;
        index += 1;
    }
    (index > start).then_some((value, index))
}

fn append_header_edits(
    source: &str,
    manifest: HeaderManifest,
    positions: IndexedHeader,
    edits: &mut Vec<ScaffoldEdit>,
) -> Result<(), ProjectionError> {
    let index = manifest.ordinal as usize;
    let mut cursor = expect_byte_after_whitespace(source, positions.helper.end, b'(', index)?;
    let _ = expect_span_after_whitespace(source, cursor, positions.right_start, index)?;
    let right =
        trimmed_content_range(source, positions.right_start.end, positions.right_end.start)?;
    cursor = positions.right_end.end;

    let annotation_index = if manifest.has_index {
        cursor = expect_byte_after_whitespace(source, cursor, b',', index)?;
        cursor = expect_span_after_whitespace(source, cursor, positions.index_helper, index)?;
        cursor = expect_byte_after_whitespace(source, cursor, b'(', index)?;
        let _ = expect_span_after_whitespace(source, cursor, positions.index_start, index)?;
        let content =
            trimmed_content_range(source, positions.index_start.end, positions.index_end.start)?;
        cursor = positions.index_end.end;
        cursor = expect_byte_after_whitespace(source, cursor, b')', index)?;
        Some(content)
    } else {
        None
    };
    let key = if manifest.has_key {
        cursor = expect_byte_after_whitespace(source, cursor, b',', index)?;
        cursor = expect_span_after_whitespace(source, cursor, positions.key_helper, index)?;
        cursor = expect_byte_after_whitespace(source, cursor, b'(', index)?;
        let _ = expect_span_after_whitespace(source, cursor, positions.key_start, index)?;
        let content =
            trimmed_content_range(source, positions.key_start.end, positions.key_end.start)?;
        cursor = positions.key_end.end;
        cursor = expect_byte_after_whitespace(source, cursor, b')', index)?;
        Some(content)
    } else {
        None
    };
    cursor = expect_byte_after_whitespace(source, cursor, b',', index)?;
    let _ = expect_span_after_whitespace(source, cursor, positions.end_sentinel, index)?;
    let call_end = scaffold_call_end(source, positions.end_sentinel.end, index)?;

    edits.push(ScaffoldEdit {
        index,
        start: positions.helper.start,
        end: right.start,
        replacement: EditReplacement::Empty,
    });
    let mut previous_end = right.end;
    if let Some(annotation_index) = annotation_index {
        edits.push(ScaffoldEdit {
            index,
            start: previous_end,
            end: annotation_index.start,
            replacement: EditReplacement::Index,
        });
        previous_end = annotation_index.end;
    }
    if let Some(key) = key {
        edits.push(ScaffoldEdit {
            index,
            start: previous_end,
            end: key.start,
            replacement: EditReplacement::Key,
        });
        previous_end = key.end;
    }
    edits.push(ScaffoldEdit {
        index,
        start: previous_end,
        end: call_end,
        replacement: EditReplacement::Empty,
    });
    Ok(())
}

fn try_wrapper_edit(
    source: &str,
    manifest: TryManifest,
    positions: IndexedTry,
) -> Result<WrapperEdit, ProjectionError> {
    let index = manifest.node as usize;
    let mut cursor =
        expect_span_after_whitespace(source, positions.try_marker.end, positions.call, index)?;
    cursor = expect_byte_after_whitespace(source, cursor, b'(', index)?;
    cursor = expect_byte_after_whitespace(source, cursor, b'{', index)?;
    cursor = expect_word_after_whitespace(source, cursor, b"async", index)?;
    cursor = expect_byte_after_whitespace(source, cursor, b'*', index)?;
    cursor = expect_span_after_whitespace(source, cursor, positions.body_method, index)?;
    cursor = expect_byte_after_whitespace(source, cursor, b'(', index)?;
    cursor = expect_byte_after_whitespace(source, cursor, b')', index)?;
    let content_start = skip_ascii_whitespace(source, cursor);
    if source.as_bytes().get(content_start) != Some(&b'{') {
        return Err(ProjectionError::ScaffoldMismatch { index });
    }

    let separator = previous_non_whitespace(source, positions.end_sentinel.start)
        .filter(|position| source.as_bytes()[*position] == b',')
        .ok_or(ProjectionError::ScaffoldMismatch { index })?;
    let object_close = previous_non_whitespace(source, separator)
        .filter(|position| source.as_bytes()[*position] == b'}')
        .ok_or(ProjectionError::ScaffoldMismatch { index })?;
    let before_object = previous_non_whitespace(source, object_close)
        .ok_or(ProjectionError::ScaffoldMismatch { index })?;
    let body_close = if source.as_bytes()[before_object] == b',' {
        previous_non_whitespace(source, before_object)
            .filter(|position| source.as_bytes()[*position] == b'}')
            .ok_or(ProjectionError::ScaffoldMismatch { index })?
    } else if source.as_bytes()[before_object] == b'}' {
        before_object
    } else {
        return Err(ProjectionError::ScaffoldMismatch { index });
    };
    let content_end = body_close + 1;
    let call_end = scaffold_call_end(source, positions.end_sentinel.end, index)?;
    let semicolon = skip_ascii_whitespace(source, call_end);
    let replace_end = if !(manifest.context == ControlContext::Statement
        && manifest.authored_semicolon())
        && source.as_bytes().get(semicolon) == Some(&b';')
    {
        semicolon + 1
    } else {
        call_end
    };
    if !(positions.try_marker.start < content_start
        && content_start <= content_end
        && content_end < replace_end)
    {
        return Err(ProjectionError::ScaffoldMismatch { index });
    }
    Ok(WrapperEdit {
        index,
        replace_start: positions.try_marker.start,
        content_start,
        content_end,
        replace_end,
        dedent: line_indent(source, content_start)
            .saturating_sub(line_indent(source, positions.call.start)),
        replacement: WrapperReplacement::Try,
    })
}

fn try_clause_edit(
    source: &str,
    token_index: usize,
    kind: StructuralKind,
    manifest: TryManifest,
    positions: IndexedTry,
) -> Result<ScaffoldEdit, ProjectionError> {
    let (marker, method, replacement, has_header) = match kind {
        StructuralKind::Pending => (
            positions.pending_marker,
            positions.pending_method,
            EditReplacement::Pending,
            false,
        ),
        StructuralKind::Catch => (
            positions.catch_marker,
            positions.catch_method,
            EditReplacement::Catch,
            manifest.catch_has_header(),
        ),
        _ => return Err(ProjectionError::StructuralMismatch),
    };
    let comma = previous_non_whitespace(source, marker.start)
        .filter(|position| source.as_bytes()[*position] == b',')
        .ok_or(ProjectionError::ScaffoldMismatch { index: token_index })?;
    let previous_body_close = previous_non_whitespace(source, comma)
        .filter(|position| source.as_bytes()[*position] == b'}')
        .ok_or(ProjectionError::ScaffoldMismatch { index: token_index })?;
    let mut cursor = expect_word_after_whitespace(source, marker.end, b"async", token_index)?;
    cursor = expect_byte_after_whitespace(source, cursor, b'*', token_index)?;
    cursor = expect_span_after_whitespace(source, cursor, method, token_index)?;
    let end = if has_header {
        if source.as_bytes().get(skip_ascii_whitespace(source, cursor)) != Some(&b'(') {
            return Err(ProjectionError::ScaffoldMismatch { index: token_index });
        }
        cursor
    } else {
        cursor = expect_byte_after_whitespace(source, cursor, b'(', token_index)?;
        cursor = expect_byte_after_whitespace(source, cursor, b')', token_index)?;
        let body_start = skip_ascii_whitespace(source, cursor);
        if source.as_bytes().get(body_start) != Some(&b'{') {
            return Err(ProjectionError::ScaffoldMismatch { index: token_index });
        }
        body_start
    };
    Ok(ScaffoldEdit {
        index: token_index,
        start: previous_body_close + 1,
        end,
        replacement,
    })
}

fn merge_wrappers(left: Vec<WrapperEdit>, right: Vec<WrapperEdit>) -> Vec<WrapperEdit> {
    let mut output = Vec::with_capacity(left.len() + right.len());
    let mut left = left.into_iter().peekable();
    let mut right = right.into_iter().peekable();
    while left.peek().is_some() || right.peek().is_some() {
        let take_left = match (left.peek(), right.peek()) {
            (Some(left), Some(right)) => left.replace_start <= right.replace_start,
            (Some(_), None) => true,
            _ => false,
        };
        output.push(if take_left {
            left.next().expect("peeked wrapper exists")
        } else {
            right.next().expect("peeked wrapper exists")
        });
    }
    output
}

fn merge_edits(left: Vec<ScaffoldEdit>, right: Vec<ScaffoldEdit>) -> Vec<ScaffoldEdit> {
    let mut output = Vec::with_capacity(left.len() + right.len());
    let mut left = left.into_iter().peekable();
    let mut right = right.into_iter().peekable();
    while left.peek().is_some() || right.peek().is_some() {
        let take_left = match (left.peek(), right.peek()) {
            (Some(left), Some(right)) => left.start <= right.start,
            (Some(_), None) => true,
            _ => false,
        };
        output.push(if take_left {
            left.next().expect("peeked edit exists")
        } else {
            right.next().expect("peeked edit exists")
        });
    }
    output
}

fn wrapper_edit(
    source: &str,
    manifest: WrapperManifest,
    positions: IndexedWrapper,
) -> Result<WrapperEdit, ProjectionError> {
    let index = manifest.node as usize;
    let mut cursor = expect_byte_after_whitespace(source, positions.wrapper.end, b'(', index)?;
    cursor = expect_byte_after_whitespace(source, cursor, b'{', index)?;
    cursor = expect_word_after_whitespace(source, cursor, b"async", index)?;
    cursor = expect_byte_after_whitespace(source, cursor, b'*', index)?;
    cursor = expect_span_after_whitespace(source, cursor, positions.method, index)?;
    cursor = expect_byte_after_whitespace(source, cursor, b'(', index)?;
    cursor = expect_byte_after_whitespace(source, cursor, b')', index)?;
    cursor = expect_byte_after_whitespace(source, cursor, b'{', index)?;
    let _ = expect_span_after_whitespace(source, cursor, positions.start_marker, index)?;
    let content_start = skip_ascii_whitespace(source, positions.start_marker.end);
    let content_end =
        trim_ascii_whitespace_end(source.as_bytes(), content_start, positions.end_marker.start);
    if content_start > content_end {
        return Err(ProjectionError::ScaffoldMismatch { index });
    }

    cursor = positions.end_marker.end;
    cursor = expect_byte_after_whitespace(source, cursor, b'}', index)?;
    let trailing_method_comma = skip_ascii_whitespace(source, cursor);
    if source.as_bytes().get(trailing_method_comma) == Some(&b',') {
        cursor = trailing_method_comma + 1;
    }
    cursor = expect_byte_after_whitespace(source, cursor, b'}', index)?;
    cursor = expect_byte_after_whitespace(source, cursor, b',', index)?;
    let _ = expect_span_after_whitespace(source, cursor, positions.end_sentinel, index)?;
    let call_end = scaffold_call_end(source, positions.end_sentinel.end, index)?;
    let (replace_start, replace_end) = if manifest.context == ControlContext::JsxChild {
        let open = previous_non_whitespace(source, positions.wrapper.start)
            .filter(|position| source.as_bytes()[*position] == b'{')
            .ok_or(ProjectionError::ScaffoldMismatch { index })?;
        let close = skip_ascii_whitespace(source, call_end);
        if source.as_bytes().get(close) != Some(&b'}') {
            return Err(ProjectionError::ScaffoldMismatch { index });
        }
        (open, close + 1)
    } else {
        (positions.wrapper.start, call_end)
    };
    if !(replace_start <= positions.wrapper.start
        && positions.wrapper.start < content_start
        && content_start <= content_end
        && content_end < replace_end)
    {
        return Err(ProjectionError::ScaffoldMismatch { index });
    }
    Ok(WrapperEdit {
        index,
        replace_start,
        content_start,
        content_end,
        replace_end,
        dedent: line_indent(source, content_start)
            .saturating_sub(line_indent(source, positions.wrapper.start)),
        replacement: WrapperReplacement::Empty,
    })
}

fn expect_byte_after_whitespace(
    source: &str,
    cursor: usize,
    expected: u8,
    index: usize,
) -> Result<usize, ProjectionError> {
    let position = skip_ascii_whitespace(source, cursor);
    if source.as_bytes().get(position) != Some(&expected) {
        return Err(ProjectionError::ScaffoldMismatch { index });
    }
    Ok(position + 1)
}

fn expect_word_after_whitespace(
    source: &str,
    cursor: usize,
    expected: &[u8],
    index: usize,
) -> Result<usize, ProjectionError> {
    let position = skip_ascii_whitespace(source, cursor);
    let end = position.saturating_add(expected.len());
    if source.as_bytes().get(position..end) != Some(expected)
        || source
            .as_bytes()
            .get(end)
            .is_some_and(|byte| super::scanner::is_identifier_continue(*byte))
    {
        return Err(ProjectionError::ScaffoldMismatch { index });
    }
    Ok(end)
}

fn expect_span_after_whitespace(
    source: &str,
    cursor: usize,
    expected: ScaffoldSpan,
    index: usize,
) -> Result<usize, ProjectionError> {
    if skip_ascii_whitespace(source, cursor) != expected.start {
        return Err(ProjectionError::ScaffoldMismatch { index });
    }
    Ok(expected.end)
}

fn trimmed_content_range(
    source: &str,
    mut start: usize,
    mut end: usize,
) -> Result<Range<usize>, ProjectionError> {
    while start < end && source.as_bytes()[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && source.as_bytes()[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if start == end {
        return Err(ProjectionError::StructuralMismatch);
    }
    Ok(start..end)
}

fn trim_ascii_whitespace_end(bytes: &[u8], start: usize, mut end: usize) -> usize {
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    end
}

fn render_scaffolds(
    source: &str,
    wrappers: &[WrapperEdit],
    edits: &[ScaffoldEdit],
) -> Result<String, ProjectionError> {
    let mut writer = LiftWriter::new(source.len());
    let mut active: Vec<usize> = Vec::with_capacity(8);
    let mut active_dedent = 0usize;
    let mut wrapper_cursor = 0usize;
    let mut edit_cursor = 0usize;
    let mut source_cursor = 0usize;

    loop {
        let next_wrapper = wrappers.get(wrapper_cursor);
        let next_edit = edits.get(edit_cursor);
        if next_wrapper.is_some_and(|wrapper| wrapper.replace_start < source_cursor) {
            return Err(ProjectionError::ScaffoldMismatch {
                index: next_wrapper.map_or(0, |wrapper| wrapper.index),
            });
        }
        if next_edit.is_some_and(|edit| edit.start < source_cursor) {
            return Err(ProjectionError::ScaffoldMismatch {
                index: next_edit.map_or(0, |edit| edit.index),
            });
        }

        let next_wrapper_start = next_wrapper.map_or(usize::MAX, |wrapper| wrapper.replace_start);
        let next_edit_start = next_edit.map_or(usize::MAX, |edit| edit.start);
        let next_start = next_wrapper_start.min(next_edit_start);
        let next_end = active
            .last()
            .map_or(usize::MAX, |&index| wrappers[index].content_end);

        if !active.is_empty() && next_end <= next_start {
            if source_cursor > next_end {
                return Err(ProjectionError::StructuralMismatch);
            }
            writer.write(&source[source_cursor..next_end], active_dedent);
            let wrapper_index = active.pop().ok_or(ProjectionError::StructuralMismatch)?;
            let wrapper = wrappers[wrapper_index];
            source_cursor = wrapper.replace_end;
            active_dedent = active_dedent
                .checked_sub(wrapper.dedent)
                .ok_or(ProjectionError::StructuralMismatch)?;
            continue;
        }

        if next_start == usize::MAX {
            if !active.is_empty() {
                return Err(ProjectionError::StructuralMismatch);
            }
            writer.write(&source[source_cursor..], active_dedent);
            break;
        }

        if next_edit_start <= next_wrapper_start {
            let edit = *next_edit.ok_or(ProjectionError::StructuralMismatch)?;
            if edit.start > edit.end
                || active
                    .last()
                    .is_some_and(|&index| edit.end > wrappers[index].content_end)
            {
                return Err(ProjectionError::ScaffoldMismatch { index: edit.index });
            }
            writer.write(&source[source_cursor..edit.start], active_dedent);
            writer.write(edit.replacement.text(), active_dedent);
            source_cursor = edit.end;
            edit_cursor += 1;
        } else {
            let wrapper = *next_wrapper.ok_or(ProjectionError::StructuralMismatch)?;
            if wrapper.replace_start > wrapper.content_start
                || wrapper.content_start > wrapper.content_end
                || wrapper.content_end > wrapper.replace_end
                || active
                    .last()
                    .is_some_and(|&index| wrapper.replace_end > wrappers[index].content_end)
            {
                return Err(ProjectionError::ScaffoldMismatch {
                    index: wrapper.index,
                });
            }
            writer.write(&source[source_cursor..wrapper.replace_start], active_dedent);
            writer.write(wrapper.replacement.text(), active_dedent);
            source_cursor = wrapper.content_start;
            active_dedent = active_dedent
                .checked_add(wrapper.dedent)
                .ok_or(ProjectionError::SourceTooLarge)?;
            active.push(wrapper_cursor);
            wrapper_cursor += 1;
        }
    }
    writer.finish()
}

struct LiftWriter {
    output: Vec<u8>,
    state: TextState,
    escaped: bool,
    template_interpolations: Vec<usize>,
    line_start: bool,
    previous_byte: Option<u8>,
}

impl LiftWriter {
    fn new(capacity: usize) -> Self {
        Self {
            output: Vec::with_capacity(capacity),
            state: TextState::Code,
            escaped: false,
            template_interpolations: Vec::with_capacity(4),
            line_start: true,
            previous_byte: None,
        }
    }

    fn write(&mut self, source: &str, dedent: usize) {
        let bytes = source.as_bytes();
        let mut index = 0usize;
        while index < bytes.len() {
            if self.line_start && self.state != TextState::Template {
                let mut removed = 0usize;
                while removed < dedent
                    && bytes
                        .get(index)
                        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
                {
                    index += 1;
                    removed += 1;
                }
                if index == bytes.len() {
                    return;
                }
            }
            self.line_start = false;
            let byte = bytes[index];
            self.output.push(byte);
            self.update_text_state(byte, bytes.get(index + 1).copied());
            self.line_start = byte == b'\n';
            self.previous_byte = Some(byte);
            index += 1;
        }
    }

    fn update_text_state(&mut self, byte: u8, next: Option<u8>) {
        match self.state {
            TextState::Code => match byte {
                b'\'' => self.state = TextState::Single,
                b'"' => self.state = TextState::Double,
                b'`' => self.state = TextState::Template,
                b'/' if next == Some(b'/') => self.state = TextState::LineComment,
                b'/' if next == Some(b'*') => self.state = TextState::BlockComment,
                b'{' => {
                    if let Some(depth) = self.template_interpolations.last_mut() {
                        if *depth == usize::MAX {
                            *depth = 0;
                        } else {
                            *depth = depth.saturating_add(1);
                        }
                    }
                }
                b'}' => {
                    if let Some(depth) = self.template_interpolations.last_mut() {
                        if *depth == 0 {
                            self.template_interpolations.pop();
                            self.state = TextState::Template;
                        } else if *depth != usize::MAX {
                            *depth -= 1;
                        }
                    }
                }
                _ => {}
            },
            TextState::Single => {
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                } else if byte == b'\'' {
                    self.state = TextState::Code;
                }
            }
            TextState::Double => {
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                } else if byte == b'"' {
                    self.state = TextState::Code;
                }
            }
            TextState::Template => {
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                } else if byte == b'`' {
                    self.state = TextState::Code;
                } else if byte == b'$' && next == Some(b'{') {
                    self.template_interpolations.push(usize::MAX);
                    self.state = TextState::Code;
                }
            }
            TextState::LineComment => {
                if byte == b'\n' {
                    self.state = TextState::Code;
                }
            }
            TextState::BlockComment => {
                if byte == b'/' && self.previous_byte == Some(b'*') {
                    self.state = TextState::Code;
                }
            }
        }
    }

    fn finish(self) -> Result<String, ProjectionError> {
        String::from_utf8(self.output).map_err(|_| ProjectionError::StructuralMismatch)
    }
}

fn lift_tokens(lifted: &str, projection: &FormatProjection) -> Result<String, ProjectionError> {
    let marker_prefix = format!("/*{}", projection.prefix);
    let mut output = String::with_capacity(lifted.len());
    let mut source_cursor = 0usize;
    let mut search_cursor = 0usize;
    let mut expected_index = next_lifted_token(&projection.tokens, 0);
    while let Some(relative) = lifted[search_cursor..].find(&marker_prefix) {
        let marker_start = search_cursor + relative;
        let digits_start = marker_start + marker_prefix.len();
        let digits_end = lifted.as_bytes()[digits_start..]
            .iter()
            .position(|byte| !byte.is_ascii_digit())
            .map_or(lifted.len(), |offset| digits_start + offset);
        if digits_start == digits_end || !lifted[digits_end..].starts_with("*/") {
            return Err(ProjectionError::MarkerResidual);
        }
        let actual_index = lifted[digits_start..digits_end]
            .parse::<usize>()
            .map_err(|_| ProjectionError::MarkerResidual)?;
        if actual_index < expected_index {
            return Err(ProjectionError::MarkerDuplicated {
                index: actual_index,
            });
        }
        if actual_index > expected_index {
            return Err(ProjectionError::MarkerReordered {
                index: expected_index,
            });
        }
        let Some(token) = projection.tokens.get(expected_index) else {
            return Err(ProjectionError::MarkerDuplicated {
                index: actual_index,
            });
        };
        let kind = token.kind;
        let marker_end = digits_end + 2;
        let target_start = skip_ascii_whitespace(lifted, marker_end);
        let expected = kind.projected_token();
        if !token_at(lifted, target_start, expected) {
            return Err(ProjectionError::MarkerTargetChanged {
                index: expected_index,
                expected,
            });
        }
        let (replace_start, replace_end, replacement) = if kind == StructuralKind::Empty {
            let condition_start = skip_ascii_whitespace(lifted, target_start + expected.len());
            if !lifted[condition_start..].starts_with("(false)") {
                return Err(ProjectionError::ScaffoldMismatch {
                    index: expected_index,
                });
            }
            let whitespace_start = previous_non_whitespace(lifted, marker_start)
                .filter(|position| lifted.as_bytes()[*position] == b'}')
                .map_or(marker_start, |position| position + 1);
            let replace_start = if whitespace_start >= source_cursor
                && lifted.as_bytes()[whitespace_start..marker_start]
                    .iter()
                    .all(u8::is_ascii_whitespace)
            {
                whitespace_start
            } else {
                marker_start
            };
            (replace_start, condition_start + "(false)".len(), " @empty")
        } else {
            (marker_start, target_start, "@")
        };
        if replace_start < source_cursor {
            return Err(ProjectionError::StructuralMismatch);
        }
        output.push_str(&lifted[source_cursor..replace_start]);
        output.push_str(replacement);
        source_cursor = replace_end;
        search_cursor = marker_end;
        expected_index = next_lifted_token(&projection.tokens, expected_index + 1);
    }
    if expected_index != projection.tokens.len() {
        return Err(ProjectionError::MarkerMissing {
            index: expected_index,
        });
    }
    output.push_str(&lifted[source_cursor..]);
    Ok(output)
}

fn next_lifted_token(tokens: &[TokenManifest], mut index: usize) -> usize {
    while tokens.get(index).is_some_and(|token| {
        matches!(
            token.kind,
            StructuralKind::Try | StructuralKind::Pending | StructuralKind::Catch
        )
    }) {
        index += 1;
    }
    index
}

fn scaffold_call_end(
    source: &str,
    after_end_name: usize,
    index: usize,
) -> Result<usize, ProjectionError> {
    let mut cursor = skip_ascii_whitespace(source, after_end_name);
    if source.as_bytes().get(cursor) == Some(&b',') {
        cursor = skip_ascii_whitespace(source, cursor + 1);
    }
    if source.as_bytes().get(cursor) != Some(&b')') {
        return Err(ProjectionError::ScaffoldMismatch { index });
    }
    Ok(cursor + 1)
}

fn previous_non_whitespace(source: &str, before: usize) -> Option<usize> {
    source.as_bytes()[..before]
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
}

fn skip_ascii_whitespace(source: &str, mut index: usize) -> usize {
    while source
        .as_bytes()
        .get(index)
        .is_some_and(u8::is_ascii_whitespace)
    {
        index += 1;
    }
    index
}

fn line_indent(source: &str, position: usize) -> usize {
    let line_start = source.as_bytes()[..position]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    source.as_bytes()[line_start..position]
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace() && **byte != b'\n' && **byte != b'\r')
        .count()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextState {
    Code,
    Single,
    Double,
    Template,
    LineComment,
    BlockComment,
}

fn token_at(source: &str, start: usize, token: &str) -> bool {
    source.as_bytes().get(start..start + token.len()) == Some(token.as_bytes())
        && (token == "{"
            || source
                .as_bytes()
                .get(start + token.len())
                .is_none_or(|byte| !super::scanner::is_identifier_continue(*byte)))
}

fn validate_overlay_source(source: &str, overlay: &Overlay) -> Result<(), ProjectionError> {
    if source.len() != overlay.source_len as usize
        || source_fingerprint(source.as_bytes()) != overlay.source_fingerprint
    {
        return Err(ProjectionError::SourceChanged { offset: 0 });
    }
    Ok(())
}

fn structural_fingerprint(overlay: &Overlay) -> u128 {
    let mut first = 0x517c_c1b7_2722_0a95_u64;
    let mut second = 0x6eed_0e9d_a4d9_4a4f_u64;
    let mut mix = |value: u64| {
        first = (first ^ value)
            .wrapping_mul(0x9e37_79b1_85eb_ca87)
            .rotate_left(23);
        second = (second ^ value.rotate_left(29))
            .wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
            .rotate_left(31);
    };
    mix(overlay.tokens.len() as u64);
    for token in &overlay.tokens {
        mix(u64::from(token.owner) << 8 | token.kind as u64);
    }
    mix(overlay.nodes.len() as u64);
    for node in &overlay.nodes {
        mix(u64::from(node.parent) << 16 | (node.kind as u64) << 8 | node.context as u64);
    }
    mix(overlay.clauses.len() as u64);
    for clause in &overlay.clauses {
        let flags = u64::from(clause.for_header.annotated)
            | (u64::from(clause.for_header.r#await) << 1)
            | (u64::from(!clause.for_header.index.is_empty()) << 2)
            | (u64::from(!clause.for_header.key.is_empty()) << 3)
            | (u64::from(!clause.header.is_empty()) << 4)
            | (u64::from(clause.bindings) << 5);
        mix((clause.role as u64) << 8 | flags);
    }
    mix(overlay.embedded_tokens.len() as u64);
    for token in &overlay.embedded_tokens {
        mix(u64::from(token.owner) << 8 | token.kind as u64);
    }
    mix(overlay.parser_code_blocks.len() as u64);
    for block in &overlay.parser_code_blocks {
        mix(u64::from(block.token) << 32 | u64::from(block.body.end));
    }
    mix(overlay.dynamic_tags.len() as u64);
    for tag in &overlay.dynamic_tags {
        let flags =
            u64::from(tag.self_closing) | (u64::from(!tag.closing_expression.is_empty()) << 1);
        mix(flags);
    }
    mix(overlay.style_blocks.len() as u64);
    u128::from(first) << 64 | u128::from(second)
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
