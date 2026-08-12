//! Overlay checks for the two embedded languages, dynamic tags and raw `<style>`, whose
//! interiors OXC never sees and therefore never validates.

use tsrx_syntax::{OverlayView, ParserDynamicKind};

use crate::TsrxParseError;

#[expect(
    clippy::too_many_lines,
    reason = "one flat validation pass; each check reads the cursor the previous one left"
)]
pub(super) fn validate_dynamic_overlay(view: OverlayView<'_>) -> Result<(), TsrxParseError> {
    let mut opened = vec![false; view.dynamic_tags.len()];
    let mut closed = vec![false; view.dynamic_tags.len()];
    let mut active = Vec::with_capacity(8);
    let mut previous_style_end = 0_u32;
    let mut previous_script_end = 0_u32;
    for token in view.embedded {
        match token.kind {
            tsrx_syntax::EmbeddedKind::DynamicOpen => {
                if token.span.is_empty() {
                    return Err(TsrxParseError::Unsupported(
                        "dynamic embedded tokens are unordered",
                    ));
                }
                let index = usize::try_from(token.owner)
                    .map_err(|_| TsrxParseError::Unsupported("dynamic owner index overflow"))?;
                let tag = view
                    .dynamic_tags
                    .get(index)
                    .ok_or(TsrxParseError::Unsupported("unknown dynamic tag owner"))?;
                if std::mem::replace(
                    opened
                        .get_mut(index)
                        .ok_or(TsrxParseError::Unsupported("unknown dynamic opening"))?,
                    true,
                ) || tag.expression.is_empty()
                    || tag.opening != token.span
                    || token.span.start.checked_add(2) != Some(tag.expression.start)
                    || tag.expression.end.checked_add(1) != Some(token.span.end)
                {
                    return Err(TsrxParseError::Unsupported("malformed dynamic opening token"));
                }
                if !tag.self_closing {
                    active.push(token.owner);
                }
            }
            tsrx_syntax::EmbeddedKind::DynamicClose => {
                if token.span.is_empty() {
                    return Err(TsrxParseError::Unsupported(
                        "dynamic embedded tokens are unordered",
                    ));
                }
                let index = usize::try_from(token.owner)
                    .map_err(|_| TsrxParseError::Unsupported("dynamic owner index overflow"))?;
                let tag = view
                    .dynamic_tags
                    .get(index)
                    .ok_or(TsrxParseError::Unsupported("unknown dynamic tag owner"))?;
                let closing_min_end = tag
                    .closing_expression
                    .end
                    .checked_add(2)
                    .ok_or(TsrxParseError::Unsupported("dynamic closing span overflow"))?;
                if tag.self_closing
                    || active.pop() != Some(token.owner)
                    || std::mem::replace(
                        closed
                            .get_mut(index)
                            .ok_or(TsrxParseError::Unsupported("unknown dynamic closing"))?,
                        true,
                    )
                    || tag.closing_expression.is_empty()
                    || tag.closing != token.span
                    || token.span.start.checked_add(3) != Some(tag.closing_expression.start)
                    || closing_min_end > token.span.end
                {
                    return Err(TsrxParseError::Unsupported("malformed dynamic closing token"));
                }
            }
            tsrx_syntax::EmbeddedKind::StyleContent => {
                if token.span.start < previous_style_end {
                    return Err(TsrxParseError::Unsupported("style payload tokens are unordered"));
                }
                previous_style_end = token.span.end;
            }
            tsrx_syntax::EmbeddedKind::ScriptContent => {
                if token.span.start < previous_script_end {
                    return Err(TsrxParseError::Unsupported("script payload tokens are unordered"));
                }
                previous_script_end = token.span.end;
            }
        }
    }
    if !active.is_empty() {
        return Err(TsrxParseError::Unsupported("dynamic projection nesting is incomplete"));
    }
    validate_parser_dynamic_boundaries(view)?;
    let mut comments_seen = vec![false; view.dynamic_comments.len()];
    for tag in view.dynamic_tags {
        if tag.opening.is_empty()
            || tag.opening.start.checked_add(2) != Some(tag.expression.start)
            || tag.expression.is_empty()
            || tag.expression.end.checked_add(1) != Some(tag.opening.end)
            || tag.self_closing != tag.closing.is_empty()
            || (tag.self_closing && tag.closing.end <= tag.opening.end)
            || (!tag.self_closing
                && (tag.closing.start.checked_add(3) != Some(tag.closing_expression.start)
                    || tag.closing_expression.is_empty()
                    || tag.closing_expression.end.saturating_add(2) > tag.closing.end))
        {
            return Err(TsrxParseError::Unsupported("malformed dynamic authored spans"));
        }
        if tag.self_closing {
            if !tag.closing_expression.is_empty() || tag.closing_comment_count != 0 {
                return Err(TsrxParseError::Unsupported(
                    "self-closing dynamic tag has closing metadata",
                ));
            }
            continue;
        }
        let first = usize::try_from(tag.first_closing_comment)
            .map_err(|_| TsrxParseError::Unsupported("dynamic comment index overflow"))?;
        let end = first
            .checked_add(tag.closing_comment_count as usize)
            .ok_or(TsrxParseError::Unsupported("dynamic comment range overflow"))?;
        for (offset, comment) in view
            .dynamic_comments
            .get(first..end)
            .ok_or(TsrxParseError::Unsupported("dynamic comment range outside overlay"))?
            .iter()
            .enumerate()
        {
            let seen = comments_seen
                .get_mut(first + offset)
                .ok_or(TsrxParseError::Unsupported("dynamic comment index is unknown"))?;
            if comment.is_empty()
                || comment.start < tag.closing_expression.start
                || comment.end > tag.closing_expression.end
                || std::mem::replace(seen, true)
            {
                return Err(TsrxParseError::Unsupported(
                    "dynamic comment lies outside closing expression",
                ));
            }
        }
    }
    if comments_seen.iter().any(|seen| !seen) {
        return Err(TsrxParseError::Unsupported("unowned dynamic closing comment"));
    }
    Ok(())
}

pub(super) fn validate_script_overlay(view: OverlayView<'_>) -> Result<(), TsrxParseError> {
    let mut payloads = vec![false; view.script_blocks.len()];
    for token in
        view.embedded.iter().filter(|token| token.kind == tsrx_syntax::EmbeddedKind::ScriptContent)
    {
        let index = usize::try_from(token.owner)
            .map_err(|_| TsrxParseError::Unsupported("script owner index overflow"))?;
        let script = view
            .script_blocks
            .get(index)
            .ok_or(TsrxParseError::Unsupported("unknown script owner"))?;
        let content = script.content;
        let element = script.element;
        if content != token.span
            || element.start >= content.start
            || content.end >= element.end
            || std::mem::replace(
                payloads
                    .get_mut(index)
                    .ok_or(TsrxParseError::Unsupported("unknown script payload"))?,
                true,
            )
        {
            return Err(TsrxParseError::Unsupported(
                "malformed or duplicated script payload token",
            ));
        }
    }
    if payloads.iter().any(|payload| !payload) {
        return Err(TsrxParseError::Unsupported("script payload token is missing"));
    }
    Ok(())
}

pub(super) fn validate_style_overlay(view: OverlayView<'_>) -> Result<(), TsrxParseError> {
    let mut payloads = vec![false; view.style_blocks.len()];
    for token in
        view.embedded.iter().filter(|token| token.kind == tsrx_syntax::EmbeddedKind::StyleContent)
    {
        let index = usize::try_from(token.owner)
            .map_err(|_| TsrxParseError::Unsupported("style owner index overflow"))?;
        let style = view
            .style_blocks
            .get(index)
            .ok_or(TsrxParseError::Unsupported("unknown style owner"))?;
        if style.self_closing
            || style.content != token.span
            || std::mem::replace(
                payloads
                    .get_mut(index)
                    .ok_or(TsrxParseError::Unsupported("unknown style payload"))?,
                true,
            )
        {
            return Err(TsrxParseError::Unsupported("malformed or duplicated style payload token"));
        }
    }

    let mut previous_start = None;
    let mut ancestors: Vec<tsrx_syntax::OverlayStyleBlock> = Vec::with_capacity(4);
    for (index, style) in view.style_blocks.iter().enumerate() {
        while ancestors.last().is_some_and(|ancestor| ancestor.element.end <= style.element.start) {
            ancestors.pop();
        }
        let nested_in_opening =
            ancestors.last().is_none_or(|ancestor| style.element.end <= ancestor.content.start);
        let spans_are_valid = previous_start.is_none_or(|start| style.element.start > start)
            && nested_in_opening
            && !style.element.is_empty()
            && style.element.start < style.content.start
            && style.content.end <= style.element.end
            && if style.self_closing {
                style.content.is_empty()
                    && style.content.start == style.element.end
                    && !payloads[index]
            } else {
                style.content.end < style.element.end && payloads[index]
            };
        if !spans_are_valid {
            return Err(TsrxParseError::Unsupported("malformed authored style spans"));
        }
        previous_start = Some(style.element.start);
        ancestors.push(*style);
    }
    Ok(())
}

fn validate_parser_dynamic_boundaries(view: OverlayView<'_>) -> Result<(), TsrxParseError> {
    let mut phases = vec![0_u8; view.dynamic_tags.len()];
    let mut active = Vec::with_capacity(8);
    let mut previous = 0_u32;
    for (ordinal, token) in view.parser_dynamic.iter().enumerate() {
        if ordinal != 0 && token.offset < previous {
            return Err(TsrxParseError::Unsupported("parser dynamic boundaries are unordered"));
        }
        previous = token.offset;
        let index = usize::try_from(token.owner)
            .map_err(|_| TsrxParseError::Unsupported("parser dynamic owner overflow"))?;
        let tag = view
            .dynamic_tags
            .get(index)
            .ok_or(TsrxParseError::Unsupported("unknown parser dynamic owner"))?;
        let phase = phases
            .get_mut(index)
            .ok_or(TsrxParseError::Unsupported("unknown parser dynamic owner"))?;
        match token.kind {
            ParserDynamicKind::OpenStart if *phase == 0 && token.offset == tag.opening.start => {
                *phase = 1;
                active.push(token.owner);
            }
            ParserDynamicKind::OpenEnd
                if *phase == 1
                    && token.offset == tag.expression.end
                    && active.pop() == Some(token.owner) =>
            {
                *phase = 2;
            }
            ParserDynamicKind::CloseStart
                if *phase == 2 && !tag.self_closing && token.offset == tag.closing.start =>
            {
                *phase = 3;
                active.push(token.owner);
            }
            ParserDynamicKind::CloseEnd
                if *phase == 3
                    && token.offset == tag.closing_expression.end
                    && active.pop() == Some(token.owner) =>
            {
                *phase = 4;
            }
            _ => {
                return Err(TsrxParseError::Unsupported("malformed parser dynamic boundary"));
            }
        }
    }
    if !active.is_empty()
        || phases
            .iter()
            .zip(view.dynamic_tags)
            .any(|(&phase, tag)| phase != if tag.self_closing { 2 } else { 4 })
    {
        return Err(TsrxParseError::Unsupported("incomplete parser dynamic boundary set"));
    }
    Ok(())
}
