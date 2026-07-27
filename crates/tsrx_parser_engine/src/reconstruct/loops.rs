//! The three `for` forms, and the ordinals that tie each projected header helper back to the
//! loop clause it was generated for.

use tsrx_syntax::{
    ClauseRole, ControlContext, ControlKind, ForHeader, OverlayClause, OverlayView,
    ProjectionSegment,
};
use tsrx_tape_schema::{FlatTape, RecordIndex, ValueRef};

use crate::{
    TsrxParseError,
    projection::{map_endpoint, project_authored_start},
    tape_index::{ParentIndex, ParentSlot},
};

use super::{
    access::{
        exact_one_value, field_value, list_field, object_field, object_type, require_type,
        scalar_field, scalar_u32,
    },
    control::{find_wrapper_call, place_control, prepare_control_block},
    edits::{append_empty_metadata, replace_type},
    objects::find_unique_start,
    scaffold::{require_scaffold_callee, scaffold_tag_matches},
    spans::{AuthoredStart, require_authored_object_span},
};

pub(super) struct LoopReconstructor<'overlay, 'parse, 'starts> {
    pub(super) overlay: OverlayView<'overlay>,
    pub(super) segments: &'parse [ProjectionSegment],
    pub(super) prefix: &'parse str,
    pub(super) loop_objects: &'parse [(u32, RecordIndex)],
    pub(super) block_objects: &'parse [(u32, RecordIndex)],
    pub(super) header_ordinals: &'parse [u32],
    pub(super) parents: &'parse ParentIndex,
    pub(super) starts: &'starts mut Vec<AuthoredStart>,
    pub(super) body_lists: &'starts mut Vec<RecordIndex>,
}

#[derive(Debug, Clone, Copy)]
struct ProjectedEmpty {
    statement: RecordIndex,
    block: RecordIndex,
    list: RecordIndex,
    entry: RecordIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopStatementKind {
    Classic,
    In,
    Of,
}

impl LoopReconstructor<'_, '_, '_> {
    pub(super) fn reconstruct_all(&mut self, tape: &mut FlatTape) -> Result<(), TsrxParseError> {
        for node_index in (0..self.overlay.nodes.len()).rev() {
            if self.overlay.nodes[node_index].kind == ControlKind::For {
                self.reconstruct(tape, node_index)?;
            }
        }
        Ok(())
    }

    fn reconstruct(
        &mut self,
        tape: &mut FlatTape,
        node_index: usize,
    ) -> Result<(), TsrxParseError> {
        let node = self.overlay.nodes[node_index];
        let (clause_index, clause, empty_clause) = for_clauses(self.overlay, node_index)?;
        let authored_after_sigil = node
            .span
            .start
            .checked_add(1)
            .ok_or(TsrxParseError::Unsupported("for root span overflow"))?;
        let projected_start = project_authored_start(self.segments, authored_after_sigil)
            .ok_or(TsrxParseError::Unsupported("for root is outside affine source"))?;
        let loop_object = find_unique_start(self.loop_objects, projected_start, "for root")?;
        let kind = LoopStatementKind::from_object(tape, loop_object)?;
        let projected_end = scalar_u32(tape, loop_object, "end")?;
        if map_endpoint(self.segments, projected_end, false) != Some(clause.body.end) {
            return Err(TsrxParseError::Unsupported(
                "projected for does not match authored body span",
            ));
        }

        let empty = empty_clause
            .map(|clause| self.find_projected_empty(tape, loop_object, clause))
            .transpose()?;
        let wrapper = match node.context {
            ControlContext::Statement => None,
            ControlContext::Expression | ControlContext::JsxChild => Some(find_wrapper_call(
                tape,
                self.parents,
                loop_object,
                self.prefix,
                node_index,
                empty.map(|projected| projected.statement),
            )?),
        };
        if let Some(empty) = empty {
            let removed = tape.remove_list_value(empty.list, empty.entry)?;
            if removed != ValueRef::object(empty.statement) {
                return Err(TsrxParseError::Unsupported(
                    "removed empty statement does not match projection",
                ));
            }
        }

        let body = object_field(tape, loop_object, "body")?;
        prepare_control_block(tape, body, self.body_lists)?;
        if let Some(empty) = empty {
            prepare_control_block(tape, empty.block, self.body_lists)?;
        }
        let (index, key) =
            self.reconstruct_header(tape, loop_object, kind, clause_index, clause.for_header)?;
        if kind == LoopStatementKind::Of {
            let body_field = tape
                .field_index(loop_object, "body")
                .ok_or(TsrxParseError::Unsupported("for-of has no body field"))?;
            let index = if let Some(index) = index { index } else { tape.push_scalar("null")? };
            tape.insert_field_before(loop_object, body_field, "index", index)?;
            if let Some(key) = key {
                tape.insert_field_before(loop_object, body_field, "key", key)?;
            }
        } else if index.is_some() || key.is_some() {
            return Err(TsrxParseError::Unsupported(
                "non-for-of loop carries index or key metadata",
            ));
        }

        replace_type(tape, loop_object, r#""JSXForExpression""#)?;
        append_empty_metadata(tape, loop_object)?;
        let statement_type = tape.push_scalar(kind.statement_type())?;
        tape.append_field(loop_object, "statementType", statement_type)?;
        let empty_value = if let Some(empty) = empty {
            ValueRef::object(empty.block)
        } else {
            tape.push_scalar("null")?
        };
        tape.append_field(loop_object, "empty", empty_value)?;

        place_control(
            tape,
            self.parents,
            loop_object,
            node.context,
            wrapper,
            node.span,
            self.starts,
        )?;
        self.starts.push(AuthoredStart { object: loop_object, start: node.span.start, end: None });
        Ok(())
    }

    fn find_projected_empty(
        &self,
        tape: &FlatTape,
        loop_object: RecordIndex,
        clause: OverlayClause,
    ) -> Result<ProjectedEmpty, TsrxParseError> {
        let projected_start = project_authored_start(self.segments, clause.body.start)
            .ok_or(TsrxParseError::Unsupported("empty block is outside affine source"))?;
        let block = find_unique_start(self.block_objects, projected_start, "empty block")?;
        require_type(tape, block, r#""BlockStatement""#)?;
        let statement = self
            .parents
            .parent_container(ValueRef::object(block))
            .and_then(ValueRef::as_object)
            .ok_or(TsrxParseError::Unsupported("empty block has no projected if"))?;
        require_type(tape, statement, r#""IfStatement""#)?;
        if field_value(tape, statement, "consequent")? != ValueRef::object(block)
            || tape.scalar(field_value(tape, statement, "alternate")?) != Some("null")
        {
            return Err(TsrxParseError::Unsupported("projected empty if has unexpected branches"));
        }
        let test = object_field(tape, statement, "test")?;
        require_type(tape, test, r#""Literal""#)?;
        if scalar_field(tape, test, "value")? != "false" {
            return Err(TsrxParseError::Unsupported("projected empty guard is not false"));
        }
        let ParentSlot::ListValue(entry) = self
            .parents
            .parent_slot(ValueRef::object(statement))
            .ok_or(TsrxParseError::Unsupported("projected empty if has no list entry"))?
        else {
            return Err(TsrxParseError::Unsupported("projected empty if is not a statement"));
        };
        let list = self
            .parents
            .parent_container(ValueRef::object(statement))
            .and_then(ValueRef::as_list)
            .ok_or(TsrxParseError::Unsupported("projected empty if has no statement list"))?;
        let ParentSlot::ListValue(loop_entry) = self
            .parents
            .parent_slot(ValueRef::object(loop_object))
            .ok_or(TsrxParseError::Unsupported("projected loop has no list entry"))?
        else {
            return Err(TsrxParseError::Unsupported("projected loop is not a statement"));
        };
        let loop_list = self
            .parents
            .parent_container(ValueRef::object(loop_object))
            .and_then(ValueRef::as_list)
            .ok_or(TsrxParseError::Unsupported("projected loop has no statement list"))?;
        if loop_list != list
            || tape.list_value(loop_entry) != Some(ValueRef::object(loop_object))
            || tape.list_value_next(loop_entry) != Some(entry)
            || tape.list_value(entry) != Some(ValueRef::object(statement))
        {
            return Err(TsrxParseError::Unsupported(
                "projected empty clause does not immediately follow its loop",
            ));
        }
        Ok(ProjectedEmpty { statement, block, list, entry })
    }

    fn reconstruct_header(
        &self,
        tape: &mut FlatTape,
        loop_object: RecordIndex,
        kind: LoopStatementKind,
        clause_index: usize,
        header: ForHeader,
    ) -> Result<(Option<ValueRef>, Option<ValueRef>), TsrxParseError> {
        if kind == LoopStatementKind::Of {
            let projected_await = scalar_field(tape, loop_object, "await")?;
            if projected_await != if header.r#await { "true" } else { "false" } {
                return Err(TsrxParseError::Unsupported(
                    "projected for-await flag does not match authored loop",
                ));
            }
        } else if header.r#await || header.annotated {
            return Err(TsrxParseError::Unsupported(
                "await or annotated metadata on a non-for-of loop",
            ));
        }
        if !header.annotated {
            return Ok((None, None));
        }
        let ordinal = self
            .header_ordinals
            .get(clause_index)
            .copied()
            .filter(|ordinal| *ordinal != tsrx_syntax::NONE_INDEX)
            .and_then(|ordinal| usize::try_from(ordinal).ok())
            .ok_or(TsrxParseError::Unsupported("annotated for clause has no header ordinal"))?;
        extract_annotated_header(tape, loop_object, self.segments, self.prefix, ordinal, header)
    }
}

impl LoopStatementKind {
    fn from_object(tape: &FlatTape, object: RecordIndex) -> Result<Self, TsrxParseError> {
        match object_type(tape, object) {
            Some(r#""ForStatement""#) => Ok(Self::Classic),
            Some(r#""ForInStatement""#) => Ok(Self::In),
            Some(r#""ForOfStatement""#) => Ok(Self::Of),
            _ => Err(TsrxParseError::Unsupported("projected for has an unexpected statement type")),
        }
    }

    const fn statement_type(self) -> &'static str {
        match self {
            Self::Classic => r#""ForStatement""#,
            Self::In => r#""ForInStatement""#,
            Self::Of => r#""ForOfStatement""#,
        }
    }
}

fn for_clauses(
    overlay: OverlayView<'_>,
    node_index: usize,
) -> Result<(usize, OverlayClause, Option<OverlayClause>), TsrxParseError> {
    let node = overlay.nodes[node_index];
    let index = usize::try_from(node.first_clause)
        .map_err(|_| TsrxParseError::Unsupported("invalid for clause index"))?;
    let clause =
        *overlay.clauses.get(index).ok_or(TsrxParseError::Unsupported("missing for clause"))?;
    if clause.role != ClauseRole::For {
        return Err(TsrxParseError::Unsupported("for node starts with non-for clause"));
    }
    let empty = if clause.next == tsrx_syntax::NONE_INDEX {
        None
    } else {
        let empty = usize::try_from(clause.next)
            .ok()
            .and_then(|index| overlay.clauses.get(index))
            .copied()
            .ok_or(TsrxParseError::Unsupported("invalid empty clause index"))?;
        if empty.role != ClauseRole::Empty {
            return Err(TsrxParseError::Unsupported("for node has an unexpected trailing clause"));
        }
        Some(empty)
    };
    Ok((index, clause, empty))
}

pub(super) fn build_header_ordinals(overlay: OverlayView<'_>) -> Result<Vec<u32>, TsrxParseError> {
    let mut ordinals = vec![tsrx_syntax::NONE_INDEX; overlay.clauses.len()];
    let mut ordinal = 0_usize;
    for node in overlay.nodes {
        let mut clause_index = node.first_clause;
        while clause_index != tsrx_syntax::NONE_INDEX {
            let index = usize::try_from(clause_index)
                .map_err(|_| TsrxParseError::Unsupported("invalid clause index"))?;
            let clause = overlay
                .clauses
                .get(index)
                .ok_or(TsrxParseError::Unsupported("invalid clause index"))?;
            if clause.for_header.annotated {
                ordinals[index] = u32::try_from(ordinal)
                    .map_err(|_| TsrxParseError::Unsupported("annotated header overflow"))?;
                ordinal = ordinal
                    .checked_add(1)
                    .ok_or(TsrxParseError::Unsupported("annotated header overflow"))?;
            }
            clause_index = clause.next;
        }
    }
    Ok(ordinals)
}

fn extract_annotated_header(
    tape: &mut FlatTape,
    loop_object: RecordIndex,
    segments: &[ProjectionSegment],
    prefix: &str,
    ordinal: usize,
    header: ForHeader,
) -> Result<(Option<ValueRef>, Option<ValueRef>), TsrxParseError> {
    let right_field = tape
        .field_index(loop_object, "right")
        .ok_or(TsrxParseError::Unsupported("annotated for-of has no right field"))?;
    let wrapper = tape
        .field_value(right_field)
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("annotated for right is not a wrapper call"))?;
    require_type(tape, wrapper, r#""CallExpression""#)?;
    require_scaffold_callee(tape, wrapper, prefix, "H", ordinal)?;
    let arguments = list_field(tape, wrapper, "arguments")?;
    let mut values = tape.values(arguments);
    let right = values
        .next()
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("annotated for right value is not an expression"))?;
    require_authored_object_span(tape, right, segments, header.right)?;

    let index = if header.index.is_empty() {
        None
    } else {
        let call = values
            .next()
            .and_then(ValueRef::as_object)
            .ok_or(TsrxParseError::Unsupported("for index wrapper missing"))?;
        Some(extract_header_value(tape, call, segments, prefix, "IH", ordinal, header.index)?)
    };
    let key = if header.key.is_empty() {
        None
    } else {
        let call = values
            .next()
            .and_then(ValueRef::as_object)
            .ok_or(TsrxParseError::Unsupported("for key wrapper missing"))?;
        Some(extract_header_value(tape, call, segments, prefix, "KH", ordinal, header.key)?)
    };
    let end = values
        .next()
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("for header end marker missing"))?;
    require_type(tape, end, r#""Identifier""#)?;
    if !scaffold_tag_matches(scalar_field(tape, end, "name")?, prefix, "HE", ordinal)
        || values.next().is_some()
    {
        return Err(TsrxParseError::Unsupported("for header scaffold has unexpected arguments"));
    }
    tape.set_field_value(right_field, ValueRef::object(right))?;
    Ok((index.map(ValueRef::object), key.map(ValueRef::object)))
}

fn extract_header_value(
    tape: &FlatTape,
    call: RecordIndex,
    segments: &[ProjectionSegment],
    prefix: &str,
    tag: &str,
    ordinal: usize,
    authored_span: tsrx_syntax::ByteSpan,
) -> Result<RecordIndex, TsrxParseError> {
    require_type(tape, call, r#""CallExpression""#)?;
    require_scaffold_callee(tape, call, prefix, tag, ordinal)?;
    let expression = exact_one_value(tape, list_field(tape, call, "arguments")?)?
        .as_object()
        .ok_or(TsrxParseError::Unsupported("header wrapper value is not an expression"))?;
    require_authored_object_span(tape, expression, segments, authored_span)?;
    Ok(expression)
}
