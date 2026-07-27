//! Repairing comment values, which are re-derived from the original span rather than patched,
//! because a comment's delimiters are known and its interior is otherwise opaque.

use tsrx_syntax::OpaqueSurrogateContext;
use tsrx_tape_schema::{CommentTable, ProjectedCommentKind};

use crate::{TsrxParseError, source_bridge::PreparedSource};

use super::{
    ledger::FixupLedger,
    observer::{RepairCopyLane, Utf16WorkObserver},
};

pub(super) fn repair_comment_values<W: Utf16WorkObserver>(
    comments: &mut CommentTable,
    source: &PreparedSource<'_>,
    ledger: &mut FixupLedger<'_, '_>,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    let records = comments.records().to_vec();
    let mut repairs = Vec::new();
    for comment in records {
        if !source.has_fixup_in(comment.span.start, comment.span.end) {
            continue;
        }
        let authored = source
            .original_span(comment.span.start, comment.span.end)
            .ok_or_else(|| TsrxParseError::Adapter("comment span is not exact".to_string()))?;
        let value = match comment.kind {
            ProjectedCommentKind::Line => authored.get(2..).ok_or_else(|| {
                TsrxParseError::Adapter("line comment span is too short".to_string())
            })?,
            ProjectedCommentKind::Block => authored
                .get(
                    2..authored.len().checked_sub(2).ok_or_else(|| {
                        TsrxParseError::Adapter("block comment span is too short".to_string())
                    })?,
                )
                .ok_or_else(|| {
                    TsrxParseError::Adapter("block comment span is invalid".to_string())
                })?,
        };
        repairs.push((comment.value, value, comment.span));
    }
    comments.repair_utf16_batch(repairs.iter().map(|(range, value, _)| (*range, *value)))?;
    observer.record_copy(
        RepairCopyLane::Comment,
        repairs.iter().map(|(_, value, _)| value.len()).sum(),
    );
    for (_, _, span) in repairs {
        ledger.claim(span, OpaqueSurrogateContext::Comment)?;
    }
    Ok(())
}
