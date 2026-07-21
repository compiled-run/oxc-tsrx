use crate::{diagnostics::ProjectionError, model::StructuralKind};

use super::{
    super::format::{FormatProjection, TokenManifest},
    text::{previous_non_whitespace, skip_ascii_whitespace, token_at},
};

pub(super) fn lift_tokens(
    lifted: &str,
    projection: &FormatProjection,
) -> Result<String, ProjectionError> {
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
