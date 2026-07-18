//! Lossless, allocation-light TSRX recognition and legal-TSX projection.

mod diagnostics;
mod model;
mod projection;
mod scanner;

pub use diagnostics::ProjectionError;
pub use model::{ByteSpan, Overlay, StructuralKind, StructuralToken};
pub use projection::{
    FormatProjection, MappedProjection, TypeProjection, lift_formatted, project,
    project_for_format, project_for_lint, project_for_types,
};

use scanner::Scanner;

/// Performs one byte-oriented structural scan and returns a compact overlay over `source`.
///
/// # Errors
///
/// Returns an error for malformed or unsupported TSRX, unterminated lexical constructs, and
/// sources beyond OXC's 32-bit span limit.
pub fn scan(source: &str) -> Result<Overlay, ProjectionError> {
    Scanner::new(source).finish()
}
