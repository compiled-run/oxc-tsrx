use tsrx_syntax::{
    ByteSpan, ClauseRole, ControlContext, ControlKind, OverlayClause, OverlayView,
    ProjectionSegment,
};
use tsrx_tape_schema::{FlatTape, RecordIndex, ValueRef};

use crate::{
    TsrxParseError,
    projection::map_endpoint,
    tape_index::{ParentIndex, ParentSlot},
};

use super::{
    access::{
        exact_two_values, field_value, has_type, list_field, object_field, object_type,
        require_type, scalar_field, scalar_u32,
    },
    control::{find_wrapper_call, place_control, prepare_control_block},
    edits::append_empty_metadata,
    scaffold::{require_scaffold_callee, scaffold_tag_index, scaffold_tag_matches},
    spans::{AuthoredStart, require_authored_object_span, require_object_span_within},
};

pub(super) struct TryReconstructor<'overlay, 'parse, 'starts> {
    pub(super) authored: &'parse str,
    pub(super) overlay: OverlayView<'overlay>,
    pub(super) segments: &'parse [ProjectionSegment],
    pub(super) prefix: &'parse str,
    pub(super) try_objects: &'parse [Option<RecordIndex>],
    pub(super) parents: &'parse ParentIndex,
    pub(super) starts: &'starts mut Vec<AuthoredStart>,
    pub(super) body_lists: &'starts mut Vec<RecordIndex>,
}

#[derive(Debug, Clone, Copy)]
struct ProjectedTry {
    block: RecordIndex,
    pending: Option<RecordIndex>,
    handler: Option<ProjectedCatch>,
}

#[derive(Debug, Clone, Copy)]
struct ProjectedCatch {
    function: RecordIndex,
    body: RecordIndex,
    param: Option<ValueRef>,
    reset_param: Option<ValueRef>,
}

impl TryReconstructor<'_, '_, '_> {
    pub(super) fn reconstruct_all(&mut self, tape: &mut FlatTape) -> Result<(), TsrxParseError> {
        for node_index in (0..self.overlay.nodes.len()).rev() {
            if self.overlay.nodes[node_index].kind == ControlKind::Try {
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
        let (try_clause, pending_clause, catch_clause) = try_clauses(self.overlay, node_index)?;
        let root = self
            .try_objects
            .get(node_index)
            .copied()
            .flatten()
            .ok_or(TsrxParseError::Unsupported("missing projected try helper"))?;
        let projected_statement = projected_try_statement(tape, self.parents, root)?;

        let wrapper = match node.context {
            ControlContext::Statement => None,
            ControlContext::Expression | ControlContext::JsxChild => Some(find_wrapper_call(
                tape,
                self.parents,
                projected_statement,
                self.prefix,
                node_index,
                None,
            )?),
        };
        let projected = validate_try_helper(
            tape,
            self.authored,
            root,
            self.segments,
            self.prefix,
            node_index,
            try_clause,
            pending_clause,
            catch_clause,
        )?;

        prepare_control_block(tape, projected.block, self.body_lists)?;
        if let Some(pending) = projected.pending {
            prepare_control_block(tape, pending, self.body_lists)?;
        }
        if let Some(handler) = projected.handler {
            let clause = catch_clause
                .ok_or(TsrxParseError::Unsupported("projected catch has no authored clause"))?;
            prepare_control_block(tape, handler.body, self.body_lists)?;
            rebuild_catch_clause(tape, handler, clause)?;
            self.starts.push(AuthoredStart {
                object: handler.function,
                start: clause.keyword.start,
                end: None,
            });
        }

        let root_start = field_value(tape, projected.block, "start")?;
        let last_block = projected
            .handler
            .map_or_else(|| projected.pending.unwrap_or(projected.block), |handler| handler.body);
        let root_end = field_value(tape, last_block, "end")?;
        tape.clear_fields(root)?;
        let kind = tape.push_scalar(r#""JSXTryExpression""#)?;
        tape.append_field(root, "type", kind)?;
        tape.append_field(root, "start", root_start)?;
        tape.append_field(root, "end", root_end)?;
        tape.append_field(root, "block", ValueRef::object(projected.block))?;
        let handler = if let Some(handler) = projected.handler {
            ValueRef::object(handler.function)
        } else {
            tape.push_scalar("null")?
        };
        tape.append_field(root, "handler", handler)?;
        let pending = if let Some(pending) = projected.pending {
            ValueRef::object(pending)
        } else {
            tape.push_scalar("null")?
        };
        tape.append_field(root, "pending", pending)?;
        let finalizer = tape.push_scalar("null")?;
        tape.append_field(root, "finalizer", finalizer)?;
        append_empty_metadata(tape, root)?;
        let statement_type = tape.push_scalar(r#""TryStatement""#)?;
        tape.append_field(root, "statementType", statement_type)?;

        place_try_control(
            tape,
            self.parents,
            root,
            projected_statement,
            node.context,
            wrapper,
            node.span.start,
            node.span.end,
            self.authored,
            self.segments,
            self.starts,
        )?;
        self.starts.push(AuthoredStart { object: root, start: node.span.start, end: None });
        Ok(())
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
fn validate_try_helper(
    tape: &FlatTape,
    authored: &str,
    root: RecordIndex,
    segments: &[ProjectionSegment],
    prefix: &str,
    node_index: usize,
    try_clause: OverlayClause,
    pending_clause: Option<OverlayClause>,
    catch_clause: Option<OverlayClause>,
) -> Result<ProjectedTry, TsrxParseError> {
    require_type(tape, root, r#""CallExpression""#)?;
    require_scaffold_callee(tape, root, prefix, "T", node_index)?;
    if scalar_field(tape, root, "optional")? != "false" {
        return Err(TsrxParseError::Unsupported("try helper is optional"));
    }
    let (manifest, end_marker) = exact_two_values(tape, list_field(tape, root, "arguments")?)?;
    let manifest =
        manifest.as_object().ok_or(TsrxParseError::Unsupported("try manifest is not an object"))?;
    require_type(tape, manifest, r#""ObjectExpression""#)?;
    let end_marker = end_marker
        .as_object()
        .ok_or(TsrxParseError::Unsupported("try end marker is not an object"))?;
    require_type(tape, end_marker, r#""Identifier""#)?;
    if !scaffold_tag_matches(scalar_field(tape, end_marker, "name")?, prefix, "TE", node_index) {
        return Err(TsrxParseError::Unsupported("unknown try end marker"));
    }

    let properties = list_field(tape, manifest, "properties")?;
    let mut values = tape.values(properties);
    let block = validate_try_method(
        tape,
        authored,
        values.next(),
        segments,
        prefix,
        "B",
        node_index,
        try_clause,
        0,
    )?
    .body;
    let pending = pending_clause
        .map(|clause| {
            validate_try_method(
                tape,
                authored,
                values.next(),
                segments,
                prefix,
                "P",
                node_index,
                clause,
                0,
            )
            .map(|method| method.body)
        })
        .transpose()?;
    let handler = catch_clause
        .map(|clause| {
            validate_try_method(
                tape,
                authored,
                values.next(),
                segments,
                prefix,
                "C",
                node_index,
                clause,
                usize::from(clause.bindings),
            )
        })
        .transpose()?;
    if values.next().is_some() {
        return Err(TsrxParseError::Unsupported("try manifest has unexpected methods"));
    }
    Ok(ProjectedTry { block, pending, handler })
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
fn validate_try_method(
    tape: &FlatTape,
    authored: &str,
    value: Option<ValueRef>,
    segments: &[ProjectionSegment],
    prefix: &str,
    tag: &str,
    node_index: usize,
    clause: OverlayClause,
    binding_count: usize,
) -> Result<ProjectedCatch, TsrxParseError> {
    let property = value
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("try method property missing"))?;
    require_type(tape, property, r#""Property""#)?;
    if scalar_field(tape, property, "kind")? != r#""init""#
        || scalar_field(tape, property, "method")? != "true"
        || scalar_field(tape, property, "shorthand")? != "false"
        || scalar_field(tape, property, "computed")? != "false"
    {
        return Err(TsrxParseError::Unsupported("try manifest method has unexpected flags"));
    }
    let key = object_field(tape, property, "key")?;
    require_type(tape, key, r#""Identifier""#)?;
    if !scaffold_tag_matches(scalar_field(tape, key, "name")?, prefix, tag, node_index) {
        return Err(TsrxParseError::Unsupported("unknown try method key"));
    }
    let function = object_field(tape, property, "value")?;
    require_type(tape, function, r#""FunctionExpression""#)?;
    if tape.scalar(field_value(tape, function, "id")?) != Some("null")
        || scalar_field(tape, function, "generator")? != "true"
        || scalar_field(tape, function, "async")? != "true"
        || scalar_field(tape, function, "expression")? != "false"
    {
        return Err(TsrxParseError::Unsupported("try method is not an async generator"));
    }
    let parameters = list_field(tape, function, "params")?;
    let mut values = tape.values(parameters);
    let param = values.next();
    let reset_param = values.next();
    if values.next().is_some()
        || usize::from(param.is_some()) + usize::from(reset_param.is_some()) != binding_count
    {
        return Err(TsrxParseError::Unsupported("try method has unexpected parameters"));
    }
    for (index, value) in [param, reset_param].into_iter().enumerate() {
        let Some(value) = value else {
            continue;
        };
        let parameter = value
            .as_object()
            .ok_or(TsrxParseError::Unsupported("catch binding is not an object"))?;
        let authored_span = require_object_span_within(tape, parameter, segments, clause.header)?;
        let valid_kind = matches!(
            (index, object_type(tape, parameter)),
            (0, Some(r#""Identifier""# | r#""ObjectPattern""# | r#""ArrayPattern""#))
                | (1, Some(r#""Identifier""#))
        );
        if !valid_kind || catch_binding_is_optional(authored, authored_span.end, clause.header.end)
        {
            return Err(TsrxParseError::AuthoredGrammar(
                "unsupported catch binding shape".to_string(),
            ));
        }
    }
    if let (Some(param), Some(reset)) = (param, reset_param) {
        let param = param
            .as_object()
            .ok_or(TsrxParseError::Unsupported("catch binding is not an object"))?;
        let reset = reset
            .as_object()
            .ok_or(TsrxParseError::Unsupported("catch binding is not an object"))?;
        if scalar_u32(tape, param, "end")? > scalar_u32(tape, reset, "start")? {
            return Err(TsrxParseError::Unsupported("catch bindings are not in authored order"));
        }
    }
    let body = object_field(tape, function, "body")?;
    require_type(tape, body, r#""BlockStatement""#)?;
    require_authored_object_span(tape, body, segments, clause.body)?;
    Ok(ProjectedCatch { function, body, param, reset_param })
}

fn projected_try_statement(
    tape: &FlatTape,
    parents: &ParentIndex,
    root: RecordIndex,
) -> Result<RecordIndex, TsrxParseError> {
    let statement = parents
        .parent_container(ValueRef::object(root))
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("projected try has no expression statement"))?;
    require_type(tape, statement, r#""ExpressionStatement""#)?;
    if field_value(tape, statement, "expression")? != ValueRef::object(root)
        || !matches!(
            parents.parent_slot(ValueRef::object(statement)),
            Some(ParentSlot::ListValue(_))
        )
    {
        return Err(TsrxParseError::Unsupported("projected try statement has unexpected topology"));
    }
    Ok(statement)
}

fn rebuild_catch_clause(
    tape: &mut FlatTape,
    handler: ProjectedCatch,
    clause: OverlayClause,
) -> Result<(), TsrxParseError> {
    let start = field_value(tape, handler.body, "start")?;
    let end = field_value(tape, handler.body, "end")?;
    tape.clear_fields(handler.function)?;
    let kind = tape.push_scalar(r#""CatchClause""#)?;
    tape.append_field(handler.function, "type", kind)?;
    tape.append_field(handler.function, "start", start)?;
    tape.append_field(handler.function, "end", end)?;
    let param = if let Some(param) = handler.param { param } else { tape.push_scalar("null")? };
    tape.append_field(handler.function, "param", param)?;
    let reset =
        if let Some(reset) = handler.reset_param { reset } else { tape.push_scalar("null")? };
    tape.append_field(handler.function, "resetParam", reset)?;
    tape.append_field(handler.function, "body", ValueRef::object(handler.body))?;
    if clause.role != ClauseRole::Catch {
        return Err(TsrxParseError::Unsupported("try handler does not match catch clause"));
    }
    Ok(())
}

fn try_clauses(
    overlay: OverlayView<'_>,
    node_index: usize,
) -> Result<(OverlayClause, Option<OverlayClause>, Option<OverlayClause>), TsrxParseError> {
    let node = overlay.nodes[node_index];
    let first_index = usize::try_from(node.first_clause)
        .map_err(|_| TsrxParseError::Unsupported("invalid try clause index"))?;
    let first = *overlay
        .clauses
        .get(first_index)
        .filter(|clause| clause.role == ClauseRole::Try)
        .ok_or(TsrxParseError::Unsupported("try node has no try clause"))?;
    let mut next = first.next;
    let pending = if usize::try_from(next)
        .ok()
        .and_then(|index| overlay.clauses.get(index))
        .is_some_and(|clause| clause.role == ClauseRole::Pending)
    {
        let clause = overlay.clauses[usize::try_from(next)
            .map_err(|_| TsrxParseError::Unsupported("invalid pending clause index"))?];
        next = clause.next;
        Some(clause)
    } else {
        None
    };
    let catch = if usize::try_from(next)
        .ok()
        .and_then(|index| overlay.clauses.get(index))
        .is_some_and(|clause| clause.role == ClauseRole::Catch)
    {
        let clause = overlay.clauses[usize::try_from(next)
            .map_err(|_| TsrxParseError::Unsupported("invalid catch clause index"))?];
        next = clause.next;
        Some(clause)
    } else {
        None
    };
    if next != tsrx_syntax::NONE_INDEX || pending.is_none() && catch.is_none() {
        return Err(TsrxParseError::Unsupported("malformed try clause chain"));
    }
    Ok((first, pending, catch))
}

fn catch_binding_is_optional(authored: &str, end: u32, header_end: u32) -> bool {
    let Ok(end) = usize::try_from(end) else {
        return true;
    };
    let Ok(header_end) = usize::try_from(header_end) else {
        return true;
    };
    authored
        .get(end..header_end)
        .and_then(|tail| tail.bytes().find(|byte| !byte.is_ascii_whitespace()))
        == Some(b'?')
}

pub(super) fn collect_try_helpers(
    tape: &FlatTape,
    calls: &[RecordIndex],
    overlay: OverlayView<'_>,
    prefix: &str,
) -> Result<Vec<Option<RecordIndex>>, TsrxParseError> {
    let mut helpers = vec![None; overlay.nodes.len()];
    for &call in calls {
        let Some(callee) = tape
            .field_index(call, "callee")
            .and_then(|field| tape.field_value(field))
            .and_then(ValueRef::as_object)
            .filter(|callee| has_type(tape, *callee, r#""Identifier""#))
        else {
            continue;
        };
        let Some(name) = tape
            .field_index(callee, "name")
            .and_then(|field| tape.field_value(field))
            .and_then(|value| tape.scalar(value))
        else {
            continue;
        };
        let Some(node_index) = scaffold_tag_index(name, prefix, "T") else {
            continue;
        };
        let node = overlay
            .nodes
            .get(node_index)
            .filter(|node| node.kind == ControlKind::Try)
            .ok_or(TsrxParseError::Unsupported("unknown projected try helper"))?;
        let slot = helpers
            .get_mut(node_index)
            .ok_or(TsrxParseError::Unsupported("unknown projected try helper"))?;
        if slot.replace(call).is_some() || node.span.is_empty() {
            return Err(TsrxParseError::Unsupported("duplicate or malformed projected try helper"));
        }
    }
    for (index, node) in overlay.nodes.iter().enumerate() {
        if (node.kind == ControlKind::Try) != helpers[index].is_some() {
            return Err(TsrxParseError::Unsupported("projected try helper set is incomplete"));
        }
    }
    Ok(helpers)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
pub(super) fn place_try_control(
    tape: &mut FlatTape,
    parents: &ParentIndex,
    control: RecordIndex,
    statement: RecordIndex,
    context: ControlContext,
    wrapper: Option<RecordIndex>,
    authored_start: u32,
    authored_end: u32,
    authored: &str,
    segments: &[ProjectionSegment],
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    if context != ControlContext::Statement {
        return place_control(
            tape,
            parents,
            control,
            context,
            wrapper,
            ByteSpan::new(authored_start, authored_end),
            starts,
        );
    }
    if wrapper.is_some() || field_value(tape, statement, "expression")? != ValueRef::object(control)
    {
        return Err(TsrxParseError::Unsupported("statement try has unexpected wrapper topology"));
    }
    let statement_slot = parents
        .parent_slot(ValueRef::object(statement))
        .ok_or(TsrxParseError::Unsupported("statement try has no parent slot"))?;
    if !matches!(statement_slot, ParentSlot::ListValue(_)) {
        return Err(TsrxParseError::Unsupported("statement try is not in a statement list"));
    }

    let control_start = field_value(tape, control, "start")?;
    let control_end = field_value(tape, control, "end")?;
    let start_field = tape
        .field_index(statement, "start")
        .ok_or(TsrxParseError::Unsupported("try statement has no start"))?;
    tape.set_field_value(start_field, control_start)?;
    let end_field = tape
        .field_index(statement, "end")
        .ok_or(TsrxParseError::Unsupported("try statement has no end"))?;
    let projected_end = tape
        .field_value(end_field)
        .and_then(|value| tape.scalar_u32(value))
        .ok_or(TsrxParseError::Unsupported("try statement end is not an integer"))?;
    match map_endpoint(segments, projected_end, false) {
        Some(mapped) if mapped == authored_end => {}
        Some(mapped) if authored_semicolon_tail(authored, authored_end, mapped) => {}
        Some(_) => return Err(TsrxParseError::Unsupported("invalid try statement tail")),
        None => tape.set_field_value(end_field, control_end)?,
    }
    starts.push(AuthoredStart { object: statement, start: authored_start, end: None });
    Ok(())
}

fn authored_semicolon_tail(authored: &str, start: u32, end: u32) -> bool {
    let Ok(start) = usize::try_from(start) else {
        return false;
    };
    let Ok(end) = usize::try_from(end) else {
        return false;
    };
    authored
        .get(start..end)
        .and_then(|tail| tail.strip_suffix(';'))
        .is_some_and(|trivia| trivia.bytes().all(|byte| byte.is_ascii_whitespace()))
}
