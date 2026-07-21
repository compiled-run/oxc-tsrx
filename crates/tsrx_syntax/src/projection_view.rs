use crate::model::ByteSpan;

/// One unchanged affine segment between projected TSX and authored TSRX.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionSegment {
    pub projected: ByteSpan,
    pub original_start: u32,
    pub fixable: bool,
}

/// Allocation-free borrowed access to one legal-TSX projection and its source map.
#[derive(Debug, Clone, Copy)]
pub struct ProjectionView<'a> {
    pub source: &'a str,
    pub segments: &'a [ProjectionSegment],
}
