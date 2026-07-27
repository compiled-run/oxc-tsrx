use super::access::{
    exact_one_value, field_value, list_field, object_field, require_type, scalar_u32,
};
use super::control::{find_wrapper_call, place_control};
use super::edits::{append_empty_metadata, order_span_fields_before, replace_type};
use super::objects::find_unique_start;
use super::spans::{AuthoredStart, require_authored_object_span};
use crate::{
    TsrxParseError, projection::map_endpoint, projection::project_authored_start,
    tape_index::ParentIndex,
};
use tsrx_syntax::{
    ClauseRole, ControlContext, ControlKind, OverlayClause, OverlayView, ProjectionSegment,
};
use tsrx_tape_schema::{FlatTape, RecordIndex, ValueRef};

pub(super) struct SwitchReconstructor<'overlay, 'parse, 'starts> {
    pub(super) overlay: OverlayView<'overlay>,
    pub(super) segments: &'parse [ProjectionSegment],
    pub(super) prefix: &'parse str,
    pub(super) switch_objects: &'parse [(u32, RecordIndex)],
    pub(super) parents: &'parse ParentIndex,
    pub(super) starts: &'starts mut Vec<AuthoredStart>,
    pub(super) body_lists: &'starts mut Vec<RecordIndex>,
}

impl SwitchReconstructor<'_, '_, '_> {
    pub(super) fn reconstruct_all(&mut self, tape: &mut FlatTape) -> Result<(), TsrxParseError> {
        for node_index in (0..self.overlay.nodes.len()).rev() {
            if self.overlay.nodes[node_index].kind == ControlKind::Switch {
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
        let authored_after_sigil = node
            .span
            .start
            .checked_add(1)
            .ok_or(TsrxParseError::Unsupported("switch root span overflow"))?;
        let projected_start = project_authored_start(self.segments, authored_after_sigil)
            .ok_or(TsrxParseError::Unsupported("switch root is outside affine source"))?;
        let switch = find_unique_start(self.switch_objects, projected_start, "switch root")?;
        require_type(tape, switch, r#""SwitchStatement""#)?;
        let projected_end = scalar_u32(tape, switch, "end")?;
        if map_endpoint(self.segments, projected_end, false) != Some(node.span.end) {
            return Err(TsrxParseError::Unsupported(
                "projected switch does not match authored control span",
            ));
        }

        let cases = list_field(tape, switch, "cases")?;
        let mut entry = tape
            .list_first_value(cases)
            .ok_or(TsrxParseError::Unsupported("invalid switch cases list"))?;
        let mut clause_index = node.first_clause;
        while clause_index != tsrx_syntax::NONE_INDEX {
            if entry.is_none() {
                return Err(TsrxParseError::Unsupported("projected switch has too few cases"));
            }
            let clause = usize::try_from(clause_index)
                .ok()
                .and_then(|index| self.overlay.clauses.get(index))
                .copied()
                .ok_or(TsrxParseError::Unsupported("invalid switch clause index"))?;
            let case = tape
                .list_value(entry)
                .and_then(ValueRef::as_object)
                .ok_or(TsrxParseError::Unsupported("projected switch case is not an object"))?;
            let next = tape
                .list_value_next(entry)
                .ok_or(TsrxParseError::Unsupported("invalid switch case entry"))?;
            self.reconstruct_case(tape, case, clause)?;
            clause_index = clause.next;
            entry = next;
        }
        if !entry.is_none() {
            return Err(TsrxParseError::Unsupported("projected switch has too many cases"));
        }

        order_span_fields_before(tape, switch, "discriminant")?;
        replace_type(tape, switch, r#""JSXSwitchExpression""#)?;
        append_empty_metadata(tape, switch)?;
        let statement_type = tape.push_scalar(r#""SwitchStatement""#)?;
        tape.append_field(switch, "statementType", statement_type)?;

        let wrapper = match node.context {
            ControlContext::Statement => None,
            ControlContext::Expression | ControlContext::JsxChild => {
                Some(find_wrapper_call(tape, self.parents, switch, self.prefix, node_index, None)?)
            }
        };
        place_control(tape, self.parents, switch, node.context, wrapper, node.span, self.starts)?;
        self.starts.push(AuthoredStart { object: switch, start: node.span.start, end: None });
        Ok(())
    }

    fn reconstruct_case(
        &mut self,
        tape: &mut FlatTape,
        case: RecordIndex,
        clause: OverlayClause,
    ) -> Result<(), TsrxParseError> {
        require_type(tape, case, r#""SwitchCase""#)?;
        let start = scalar_u32(tape, case, "start")?;
        let end = scalar_u32(tape, case, "end")?;
        if map_endpoint(self.segments, start, true) != Some(clause.keyword.end)
            || map_endpoint(self.segments, end, false) != Some(clause.body.end)
        {
            return Err(TsrxParseError::Unsupported(
                "projected case does not match authored clause span",
            ));
        }
        match clause.role {
            ClauseRole::Case => {
                let test = object_field(tape, case, "test")?;
                require_authored_object_span(tape, test, self.segments, clause.header)?;
            }
            ClauseRole::Default => {
                if tape.scalar(field_value(tape, case, "test")?) != Some("null") {
                    return Err(TsrxParseError::Unsupported("projected default case has a test"));
                }
            }
            _ => {
                return Err(TsrxParseError::Unsupported("switch has a non-switch clause"));
            }
        }

        let consequent_field = tape
            .field_index(case, "consequent")
            .ok_or(TsrxParseError::Unsupported("switch case has no consequent"))?;
        let consequent = tape
            .field_value(consequent_field)
            .and_then(ValueRef::as_list)
            .ok_or(TsrxParseError::Unsupported("switch case consequent is not a list"))?;
        let block = exact_one_value(tape, consequent)?
            .as_object()
            .ok_or(TsrxParseError::Unsupported("projected case body is not a block"))?;
        require_type(tape, block, r#""BlockStatement""#)?;
        require_authored_object_span(tape, block, self.segments, clause.body)?;
        let body = list_field(tape, block, "body")?;
        tape.set_field_value(consequent_field, ValueRef::list(body))?;
        self.body_lists.push(body);
        order_switch_case_fields(tape, case)?;
        self.starts.push(AuthoredStart { object: case, start: clause.keyword.start, end: None });
        Ok(())
    }
}

fn order_switch_case_fields(tape: &mut FlatTape, case: RecordIndex) -> Result<(), TsrxParseError> {
    order_span_fields_before(tape, case, "test")?;
    let consequent = tape
        .field_index(case, "consequent")
        .ok_or(TsrxParseError::Unsupported("switch case has no consequent field"))?;
    let test = tape
        .field_index(case, "test")
        .ok_or(TsrxParseError::Unsupported("switch case has no test field"))?;
    tape.move_field_before(case, consequent, test)?;
    Ok(())
}
