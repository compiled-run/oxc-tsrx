//! Recovering the authored comment table from the projected one, consuming the projection's own
//! marker comments on the way instead of leaking them to callers.

use tsrx_syntax::{OverlayView, ProjectionSegment};
use tsrx_tape_schema::{CommentTable, ProjectedCommentKind};

use crate::TsrxParseError;

use super::{
    mapping::map_affine_span,
    marker::parse_marker,
    marker_validation::MarkerValidation,
    text::{packed_string, slice},
};

pub(crate) fn reconstruct_comments<'a>(
    authored: &str,
    projected: &'a str,
    segments: &[ProjectionSegment],
    mut comments: CommentTable,
    overlay: OverlayView<'_>,
    expected_prefix: Option<&'a str>,
    require_complete_markers: bool,
) -> Result<(Option<&'a str>, CommentTable), TsrxParseError> {
    let mut markers = MarkerValidation::new(overlay)?;
    let mut prefix = expected_prefix;
    let mut authored_comments = CommentTable::default();
    let projected_records = comments.take_records();
    let projected_strings = comments.take_string_storage()?;
    debug_assert!(comments.is_storage_released());
    drop(comments);
    for comment in projected_records {
        if let Some(mapped_span) = map_affine_span(segments, comment.span) {
            let source = slice(authored, mapped_span.start, mapped_span.end)?;
            let projected_source = slice(projected, comment.span.start, comment.span.end)?;
            let kind_matches = match comment.kind {
                ProjectedCommentKind::Line => source.starts_with("//"),
                ProjectedCommentKind::Block => source.starts_with("/*"),
            };
            let value = match comment.kind {
                ProjectedCommentKind::Line => source.strip_prefix("//"),
                ProjectedCommentKind::Block => {
                    source.strip_prefix("/*").and_then(|value| value.strip_suffix("*/"))
                }
            };
            if source != projected_source
                || !kind_matches
                || value != packed_string(&projected_strings, comment.value)
            {
                return Err(TsrxParseError::Unsupported(
                    "authored comment differs from its affine projection",
                ));
            }
            authored_comments.push(
                comment.kind,
                mapped_span,
                value.ok_or(TsrxParseError::Unsupported(
                    "authored comment delimiters are malformed",
                ))?,
            )?;
            continue;
        }
        if !require_complete_markers {
            if comment.kind != ProjectedCommentKind::Block {
                return Err(TsrxParseError::Unsupported("unknown non-block projection comment"));
            }
            continue;
        }
        if comment.kind != ProjectedCommentKind::Block {
            return Err(TsrxParseError::Unsupported("unknown non-block projection comment"));
        }
        let text = slice(projected, comment.span.start, comment.span.end)?;
        let (comment_prefix, marker) =
            parse_marker(text).ok_or(TsrxParseError::Unsupported("unknown projected comment"))?;
        if prefix.replace(comment_prefix).is_some_and(|seen| seen != comment_prefix) {
            return Err(TsrxParseError::Unsupported("mixed projection marker namespaces"));
        }
        markers.record(marker, &comment, authored, projected, segments, overlay)?;
    }
    drop(projected_strings);
    if require_complete_markers && !markers.is_complete(overlay) {
        return Err(TsrxParseError::Unsupported("incomplete projection marker set"));
    }
    if require_complete_markers {
        let marker_prefix =
            prefix.ok_or(TsrxParseError::Unsupported("missing marker namespace"))?;
        if authored.contains(marker_prefix) {
            return Err(TsrxParseError::Unsupported(
                "projection namespace collides with authored source",
            ));
        }
    }
    Ok((prefix, authored_comments))
}
