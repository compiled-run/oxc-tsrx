use super::access::{field_value, has_type, object_field, require_type, scalar_u32};
use super::control::{find_wrapper_call, place_control, prepare_control_block};
use super::edits::{append_empty_metadata, replace_type};
use super::objects::find_unique_start;
use super::spans::AuthoredStart;
use crate::{
    TsrxParseError, projection::map_endpoint, projection::project_authored_start,
    tape_index::ParentIndex,
};
use tsrx_syntax::{ControlContext, ControlKind, OverlayView, ProjectionSegment};
use tsrx_tape_schema::{FlatTape, RecordIndex};

pub(super) struct IfReconstructor<'overlay, 'parse> {
    pub(super) overlay: OverlayView<'overlay>,
    pub(super) segments: &'parse [ProjectionSegment],
    pub(super) prefix: &'parse str,
    pub(super) if_objects: &'parse [(u32, RecordIndex)],
    pub(super) parents: &'parse ParentIndex,
    pub(super) starts: Vec<AuthoredStart>,
    pub(super) body_lists: Vec<RecordIndex>,
}

impl IfReconstructor<'_, '_> {
    pub(super) fn reconstruct_all(&mut self, tape: &mut FlatTape) -> Result<(), TsrxParseError> {
        for node_index in (0..self.overlay.nodes.len()).rev() {
            if self.overlay.nodes[node_index].kind == ControlKind::If {
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
            .ok_or(TsrxParseError::Unsupported("if root span overflow"))?;
        let projected_start = project_authored_start(self.segments, authored_after_sigil)
            .ok_or(TsrxParseError::Unsupported("if root is outside affine source"))?;
        let if_object = find_unique_start(self.if_objects, projected_start, "if root")?;
        require_type(tape, if_object, r#""IfStatement""#)?;
        let projected_end = scalar_u32(tape, if_object, "end")?;
        if map_endpoint(self.segments, projected_end, false) != Some(node.span.end) {
            return Err(TsrxParseError::Unsupported(
                "projected if does not match authored control span",
            ));
        }

        prepare_if_chain(tape, if_object, &mut self.body_lists)?;
        replace_type(tape, if_object, r#""JSXIfExpression""#)?;
        append_empty_metadata(tape, if_object)?;
        let statement_type = tape.push_scalar(r#""IfStatement""#)?;
        tape.append_field(if_object, "statementType", statement_type)?;

        let wrapper = match node.context {
            ControlContext::Statement => None,
            ControlContext::Expression | ControlContext::JsxChild => Some(find_wrapper_call(
                tape,
                self.parents,
                if_object,
                self.prefix,
                node_index,
                None,
            )?),
        };
        place_control(
            tape,
            self.parents,
            if_object,
            node.context,
            wrapper,
            node.span,
            &mut self.starts,
        )?;
        self.starts.push(AuthoredStart { object: if_object, start: node.span.start, end: None });
        Ok(())
    }
}

fn prepare_if_chain(
    tape: &mut FlatTape,
    root: RecordIndex,
    body_lists: &mut Vec<RecordIndex>,
) -> Result<(), TsrxParseError> {
    let mut current = root;
    loop {
        require_type(tape, current, r#""IfStatement""#)?;
        let consequent = object_field(tape, current, "consequent")?;
        prepare_control_block(tape, consequent, body_lists)?;
        let alternate = field_value(tape, current, "alternate")?;
        if tape.scalar(alternate) == Some("null") {
            return Ok(());
        }
        let alternate = alternate
            .as_object()
            .ok_or(TsrxParseError::Unsupported("if alternate is not an object"))?;
        if has_type(tape, alternate, r#""IfStatement""#) {
            current = alternate;
            continue;
        }
        require_type(tape, alternate, r#""BlockStatement""#)?;
        prepare_control_block(tape, alternate, body_lists)?;
        return Ok(());
    }
}
