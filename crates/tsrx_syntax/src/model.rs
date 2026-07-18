use std::ops::Range;

pub(crate) const NONE: u32 = u32::MAX;

/// A byte range in the original UTF-8 source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ByteSpan {
    pub start: u32,
    pub end: u32,
}

impl ByteSpan {
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    #[must_use]
    pub const fn intersects(self, start: u32, end: u32) -> bool {
        if start == end {
            return self.start <= start && start <= self.end;
        }
        self.start < end && start < self.end
    }
}

/// Structural spellings retained by the compact overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralKind {
    FunctionBody,
    If,
    Else,
    For,
    Empty,
    Switch,
    Case,
    Default,
    Try,
    Pending,
    Catch,
}

impl StructuralKind {
    pub(crate) const fn projected_token(self) -> &'static str {
        match self {
            Self::FunctionBody => "{",
            Self::If | Self::Empty => "if",
            Self::Else => "else",
            Self::For => "for",
            Self::Switch => "switch",
            Self::Case => "case",
            Self::Default => "default",
            Self::Try | Self::Pending | Self::Catch => "",
        }
    }
}

/// One authored `@` byte. The payload stays in the original source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructuralToken {
    pub kind: StructuralKind,
    pub span: ByteSpan,
    pub(crate) owner: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlKind {
    If,
    For,
    Switch,
    Try,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlContext {
    Statement,
    Expression,
    JsxChild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClauseRole {
    If,
    ElseIf,
    Else,
    For,
    Empty,
    Case,
    Default,
    Try,
    Pending,
    Catch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmbeddedKind {
    DynamicOpen,
    DynamicClose,
    StyleContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EmbeddedToken {
    pub kind: EmbeddedKind,
    pub span: ByteSpan,
    pub owner: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DynamicTag {
    pub expression: ByteSpan,
    pub closing_expression: ByteSpan,
    pub first_closing_comment: u32,
    pub closing_comment_count: u32,
    pub self_closing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StyleBlock {
    pub content: ByteSpan,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ForHeader {
    pub left: ByteSpan,
    pub right: ByteSpan,
    pub index: ByteSpan,
    pub key: ByteSpan,
    pub annotated: bool,
    pub r#await: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Clause {
    pub role: ClauseRole,
    pub keyword: ByteSpan,
    pub header: ByteSpan,
    pub body: ByteSpan,
    pub for_header: ForHeader,
    pub bindings: u8,
    pub next: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SyntaxNode {
    pub kind: ControlKind,
    pub context: ControlContext,
    pub span: ByteSpan,
    pub parent: u32,
    pub first_child: u32,
    pub last_child: u32,
    pub next_sibling: u32,
    pub first_clause: u32,
    pub last_clause: u32,
}

/// Compact lossless overlay over the original source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    pub(crate) source_len: u32,
    pub(crate) source_fingerprint: u128,
    pub(crate) tokens: Vec<StructuralToken>,
    pub(crate) nodes: Vec<SyntaxNode>,
    pub(crate) clauses: Vec<Clause>,
    pub(crate) embedded_tokens: Vec<EmbeddedToken>,
    pub(crate) dynamic_tags: Vec<DynamicTag>,
    pub(crate) dynamic_comments: Vec<ByteSpan>,
    pub(crate) style_blocks: Vec<StyleBlock>,
    pub(crate) first_root: u32,
    pub(crate) last_root: u32,
}

impl Overlay {
    #[must_use]
    pub fn tokens(&self) -> &[StructuralToken] {
        &self.tokens
    }

    #[must_use]
    pub const fn source_len(&self) -> u32 {
        self.source_len
    }

    #[must_use]
    pub fn control_count(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn dynamic_tag_count(&self) -> usize {
        self.dynamic_tags.len()
    }

    #[must_use]
    pub fn style_block_count(&self) -> usize {
        self.style_blocks.len()
    }

    /// Returns true only when an edit stays wholly in unchanged authored syntax.
    #[must_use]
    pub fn is_identity_range(&self, range: Range<u32>) -> bool {
        range.start <= range.end
            && range.end <= self.source_len
            && self
                .tokens
                .iter()
                .all(|token| !token.span.intersects(range.start, range.end))
            && self
                .embedded_tokens
                .iter()
                .all(|token| !token.span.intersects(range.start, range.end))
    }
}

#[cfg(all(test, target_pointer_width = "64"))]
mod layout_tests {
    use std::mem::size_of;

    use super::{
        ByteSpan, Clause, DynamicTag, EmbeddedToken, ForHeader, StructuralToken, StyleBlock,
        SyntaxNode,
    };

    #[test]
    fn hot_record_layouts_remain_compact() {
        assert_eq!(size_of::<ByteSpan>(), 8);
        assert_eq!(size_of::<StructuralToken>(), 16);
        assert_eq!(size_of::<EmbeddedToken>(), 16);
        assert_eq!(size_of::<DynamicTag>(), 28);
        assert_eq!(size_of::<StyleBlock>(), 8);
        assert_eq!(size_of::<ForHeader>(), 36);
        assert_eq!(size_of::<Clause>(), 68);
        assert_eq!(size_of::<SyntaxNode>(), 36);
    }
}
