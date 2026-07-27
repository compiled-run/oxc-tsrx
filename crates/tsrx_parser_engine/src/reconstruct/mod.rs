use tsrx_syntax::{
    ByteSpan, ClauseRole, ControlContext, ControlKind, ForHeader, OverlayClause, OverlayView,
    ProjectionSegment, StructuralKind,
};
use tsrx_tape_schema::{
    FlatTape, ListRecord, ListValueInsertion, ListValueRecord, ObjectRecord, RecordIndex,
    ValueKind, ValueRef,
};

use crate::{
    TsrxParseError,
    lexical::{FinalizationIndex, SpanFields},
    projection::{map_endpoint, project_authored_end, project_authored_start},
    tape_index::{ParentIndex, ParentSlot},
};

#[derive(Debug, Clone, Copy)]
pub(super) struct AuthoredStart {
    object: RecordIndex,
    start: u32,
    end: Option<u32>,
}

struct IfReconstructor<'overlay, 'parse> {
    overlay: OverlayView<'overlay>,
    segments: &'parse [ProjectionSegment],
    prefix: &'parse str,
    if_objects: &'parse [(u32, RecordIndex)],
    parents: &'parse ParentIndex,
    starts: Vec<AuthoredStart>,
    body_lists: Vec<RecordIndex>,
}

struct LoopReconstructor<'overlay, 'parse, 'starts> {
    overlay: OverlayView<'overlay>,
    segments: &'parse [ProjectionSegment],
    prefix: &'parse str,
    loop_objects: &'parse [(u32, RecordIndex)],
    block_objects: &'parse [(u32, RecordIndex)],
    header_ordinals: &'parse [u32],
    parents: &'parse ParentIndex,
    starts: &'starts mut Vec<AuthoredStart>,
    body_lists: &'starts mut Vec<RecordIndex>,
}

struct SwitchReconstructor<'overlay, 'parse, 'starts> {
    overlay: OverlayView<'overlay>,
    segments: &'parse [ProjectionSegment],
    prefix: &'parse str,
    switch_objects: &'parse [(u32, RecordIndex)],
    parents: &'parse ParentIndex,
    starts: &'starts mut Vec<AuthoredStart>,
    body_lists: &'starts mut Vec<RecordIndex>,
}

struct TryReconstructor<'overlay, 'parse, 'starts> {
    authored: &'parse str,
    overlay: OverlayView<'overlay>,
    segments: &'parse [ProjectionSegment],
    prefix: &'parse str,
    try_objects: &'parse [Option<RecordIndex>],
    parents: &'parse ParentIndex,
    starts: &'starts mut Vec<AuthoredStart>,
    body_lists: &'starts mut Vec<RecordIndex>,
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

#[derive(Debug, Clone, Copy)]
struct ProjectedEmpty {
    statement: RecordIndex,
    block: RecordIndex,
    list: RecordIndex,
    entry: RecordIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectedCodeBlockKind {
    Block,
    JsxContainer,
}

#[derive(Debug, Clone, Copy)]
enum CodeBlockPlacement {
    DirectField,
    DirectList { slot: ParentSlot, policy: DirectListPolicy },
    Wrap(ParentSlot),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectListPolicy {
    None,
    CodeBlockBody,
    TemplateClause,
}

#[derive(Debug, Clone, Copy)]
struct ProjectedCodeBlock {
    object: RecordIndex,
    body_owner: RecordIndex,
    kind: ProjectedCodeBlockKind,
    authored_start: u32,
}

struct CodeBlockPlans {
    blocks: Vec<ProjectedCodeBlock>,
    direct_list_policies: Vec<DirectListPolicy>,
}

#[derive(Debug, Clone, Copy)]
struct ListEntryRemoval {
    list: RecordIndex,
    entry: RecordIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopStatementKind {
    Classic,
    In,
    Of,
}

pub(super) fn reconstruct_projected(
    tape: &mut FlatTape,
    authored: &str,
    overlay: OverlayView<'_>,
    segments: &[ProjectionSegment],
    prefix: &str,
) -> Result<Vec<AuthoredStart>, TsrxParseError> {
    let program = validate_program_shape(tape)?;
    let mut object_index = ProjectedObjectIndex::new();
    let parents =
        ParentIndex::build(tape, |object, kind, start| object_index.record(object, kind, start))?;
    object_index.sort();
    validate_module_declaration_placement(tape, &parents, program, &object_index.module_objects)?;
    let mut code_blocks = collect_code_block_plans(
        tape,
        overlay,
        segments,
        &object_index.block_objects,
        &object_index.jsx_containers,
        &parents,
        prefix,
    )?;
    mark_direct_custom_clause_blocks(
        &mut code_blocks.direct_list_policies,
        overlay,
        segments,
        &object_index.block_objects,
    )?;
    let header_ordinals = build_header_ordinals(overlay)?;
    let try_objects = collect_try_helpers(tape, &object_index.call_objects, overlay, prefix)?;
    let starts = initial_authored_starts(program, authored, overlay)?;
    let mut reconstructor = IfReconstructor {
        overlay,
        segments,
        prefix,
        if_objects: &object_index.if_objects,
        parents: &parents,
        starts,
        body_lists: Vec::with_capacity(overlay.clauses.len()),
    };

    reconstructor.reconstruct_all(tape)?;
    let mut starts = reconstructor.starts;
    let mut body_lists = reconstructor.body_lists;
    {
        let mut loops = LoopReconstructor {
            overlay,
            segments,
            prefix,
            loop_objects: &object_index.loop_objects,
            block_objects: &object_index.block_objects,
            header_ordinals: &header_ordinals,
            parents: &parents,
            starts: &mut starts,
            body_lists: &mut body_lists,
        };
        loops.reconstruct_all(tape)?;
    }
    {
        let mut switches = SwitchReconstructor {
            overlay,
            segments,
            prefix,
            switch_objects: &object_index.switch_objects,
            parents: &parents,
            starts: &mut starts,
            body_lists: &mut body_lists,
        };
        switches.reconstruct_all(tape)?;
    }
    {
        let mut tries = TryReconstructor {
            authored,
            overlay,
            segments,
            prefix,
            try_objects: &try_objects,
            parents: &parents,
            starts: &mut starts,
            body_lists: &mut body_lists,
        };
        tries.reconstruct_all(tape)?;
    }
    reconstruct_style_elements(tape, authored, overlay, segments, &parents, &mut starts)?;
    reconstruct_dynamic_tags(tape, authored, overlay, segments, prefix, &parents, &mut starts)?;
    normalize_control_body_lists(tape, &body_lists)?;
    let mut list_removals = Vec::new();
    reconstruct_code_blocks(
        tape,
        authored,
        segments,
        &code_blocks,
        &parents,
        &mut starts,
        &mut list_removals,
    )?;
    normalize_template_layout_text(tape, &object_index.layout_containers, &mut list_removals)?;
    tape.remove_list_values(
        &list_removals.iter().map(|removal| (removal.list, removal.entry)).collect::<Vec<_>>(),
    )?;
    Ok(starts)
}

fn validate_program_shape(tape: &FlatTape) -> Result<RecordIndex, TsrxParseError> {
    let program = tape
        .root()
        .as_object()
        .ok_or(TsrxParseError::Unsupported("projected root is not a Program"))?;
    require_type(tape, program, r#""Program""#)?;
    let _ = list_field(tape, program, "body")?;
    Ok(program)
}

fn validate_module_declaration_placement(
    tape: &FlatTape,
    parents: &ParentIndex,
    program: RecordIndex,
    module_objects: &[RecordIndex],
) -> Result<(), TsrxParseError> {
    let program_body = list_field(tape, program, "body")?;
    for &object in module_objects {
        let direct_program_member =
            matches!(parents.parent_slot(ValueRef::object(object)), Some(ParentSlot::ListValue(_)))
                && parents.parent_container(ValueRef::object(object))
                    == Some(ValueRef::list(program_body));
        let typescript_module_member = parents
            .parent_container(ValueRef::object(object))
            .and_then(ValueRef::as_list)
            .and_then(|list| parents.parent_container(ValueRef::list(list)))
            .and_then(ValueRef::as_object)
            .is_some_and(|owner| has_type(tape, owner, r#""TSModuleBlock""#));
        if !direct_program_member && !typescript_module_member {
            return Err(TsrxParseError::AuthoredGrammar(
                "module declaration is nested inside authored TSRX".to_string(),
            ));
        }
    }
    Ok(())
}

fn is_module_declaration_type(kind: &str) -> bool {
    matches!(
        kind,
        r#""ImportDeclaration""#
            | r#""ExportNamedDeclaration""#
            | r#""ExportDefaultDeclaration""#
            | r#""ExportAllDeclaration""#
            | r#""TSExportAssignment""#
            | r#""TSNamespaceExportDeclaration""#
    )
}

fn initial_authored_starts(
    program: RecordIndex,
    authored: &str,
    overlay: OverlayView<'_>,
) -> Result<Vec<AuthoredStart>, TsrxParseError> {
    let code_blocks =
        overlay.tokens.iter().filter(|token| token.kind == StructuralKind::FunctionBody).count();
    let capacity = overlay
        .nodes
        .len()
        .saturating_mul(2)
        .saturating_add(overlay.style_blocks.len().saturating_mul(3))
        .saturating_add(code_blocks.saturating_mul(2))
        .saturating_add(1);
    let mut starts = Vec::with_capacity(capacity);
    starts.push(AuthoredStart {
        object: program,
        start: 0,
        end: Some(
            u32::try_from(authored.len())
                .map_err(|_| TsrxParseError::Unsupported("authored Program exceeds 4 GiB"))?,
        ),
    });
    Ok(starts)
}

impl IfReconstructor<'_, '_> {
    fn reconstruct_all(&mut self, tape: &mut FlatTape) -> Result<(), TsrxParseError> {
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

impl LoopReconstructor<'_, '_, '_> {
    fn reconstruct_all(&mut self, tape: &mut FlatTape) -> Result<(), TsrxParseError> {
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

impl SwitchReconstructor<'_, '_, '_> {
    fn reconstruct_all(&mut self, tape: &mut FlatTape) -> Result<(), TsrxParseError> {
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

impl TryReconstructor<'_, '_, '_> {
    fn reconstruct_all(&mut self, tape: &mut FlatTape) -> Result<(), TsrxParseError> {
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

fn build_header_ordinals(overlay: OverlayView<'_>) -> Result<Vec<u32>, TsrxParseError> {
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
    let value = exact_one_value(tape, list_field(tape, call, "arguments")?)?
        .as_object()
        .ok_or(TsrxParseError::Unsupported("header wrapper value is not an expression"))?;
    require_authored_object_span(tape, value, segments, authored_span)?;
    Ok(value)
}

fn require_scaffold_callee(
    tape: &FlatTape,
    call: RecordIndex,
    prefix: &str,
    tag: &str,
    ordinal: usize,
) -> Result<(), TsrxParseError> {
    let callee = object_field(tape, call, "callee")?;
    require_type(tape, callee, r#""Identifier""#)?;
    if scaffold_tag_matches(scalar_field(tape, callee, "name")?, prefix, tag, ordinal) {
        Ok(())
    } else {
        Err(TsrxParseError::Unsupported("unknown annotated header helper"))
    }
}

fn scaffold_tag_matches(encoded: &str, prefix: &str, tag: &str, expected_index: usize) -> bool {
    scaffold_tag_index(encoded, prefix, tag) == Some(expected_index)
}

fn scaffold_tag_index(encoded: &str, prefix: &str, tag: &str) -> Option<usize> {
    encoded
        .strip_prefix('"')?
        .strip_suffix('"')?
        .strip_prefix(prefix)?
        .strip_prefix(tag)?
        .strip_suffix('_')?
        .parse()
        .ok()
}

fn require_authored_object_span(
    tape: &FlatTape,
    object: RecordIndex,
    segments: &[ProjectionSegment],
    authored: tsrx_syntax::ByteSpan,
) -> Result<(), TsrxParseError> {
    let start = scalar_u32(tape, object, "start")?;
    let end = scalar_u32(tape, object, "end")?;
    if map_endpoint(segments, start, true) == Some(authored.start)
        && map_endpoint(segments, end, false) == Some(authored.end)
    {
        Ok(())
    } else {
        Err(TsrxParseError::Unsupported("annotated header value span is synthetic"))
    }
}

fn require_object_span_within(
    tape: &FlatTape,
    object: RecordIndex,
    segments: &[ProjectionSegment],
    authored: tsrx_syntax::ByteSpan,
) -> Result<tsrx_syntax::ByteSpan, TsrxParseError> {
    if authored.is_empty() {
        return Err(TsrxParseError::Unsupported("catch binding has no authored header"));
    }
    let start = scalar_u32(tape, object, "start")?;
    let end = scalar_u32(tape, object, "end")?;
    let start = map_endpoint(segments, start, true)
        .ok_or(TsrxParseError::Unsupported("catch binding start is synthetic"))?;
    let end = map_endpoint(segments, end, false)
        .ok_or(TsrxParseError::Unsupported("catch binding end is synthetic"))?;
    if authored.start < start && start < end && end < authored.end {
        Ok(tsrx_syntax::ByteSpan::new(start, end))
    } else {
        Err(TsrxParseError::Unsupported("catch binding lies outside its authored header"))
    }
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

fn prepare_control_block(
    tape: &mut FlatTape,
    block: RecordIndex,
    body_lists: &mut Vec<RecordIndex>,
) -> Result<(), TsrxParseError> {
    require_type(tape, block, r#""BlockStatement""#)?;
    let body = list_field(tape, block, "body")?;
    order_span_fields_before(tape, block, "body")?;
    append_empty_metadata(tape, block)?;
    body_lists.push(body);
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct ProjectedStyle {
    element: RecordIndex,
    opening: RecordIndex,
}

fn reconstruct_style_elements(
    tape: &mut FlatTape,
    authored: &str,
    overlay: OverlayView<'_>,
    segments: &[ProjectionSegment],
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    if overlay.style_blocks.is_empty() {
        return Ok(());
    }
    let styles = collect_projected_styles(tape, overlay, segments, parents)?;
    let mut semicolons = Vec::new();
    for index in (0..overlay.style_blocks.len()).rev() {
        reconstruct_style_element(
            tape,
            authored,
            overlay.style_blocks[index],
            styles[index],
            segments,
            parents,
            starts,
            &mut semicolons,
        )?;
    }
    tape.insert_list_values_after(&semicolons)?;
    Ok(())
}

/// Matches ordinary OXC JSX style elements to scanner owners in one flat object-table pass.
/// The compact span stack accounts for nested attribute expressions without a sort, binary
/// search, source rescan, or per-style AST walk.
fn collect_projected_styles(
    tape: &FlatTape,
    overlay: OverlayView<'_>,
    segments: &[ProjectionSegment],
    parents: &ParentIndex,
) -> Result<Vec<ProjectedStyle>, TsrxParseError> {
    // OXC finishes an opening record after serializing its attributes. A nested style used inside
    // an attribute is therefore observed before its owning style, while ordinary siblings remain
    // in source order. Derive that exact postorder from scanner preorder in one flat pass.
    let expected_order = style_opening_postorder(overlay)?;
    let mut styles = vec![None; overlay.style_blocks.len()];
    let mut next = 0_usize;
    for raw in 0..tape.object_count() {
        let raw = u32::try_from(raw)
            .map_err(|_| TsrxParseError::Unsupported("object table above 4 GiB"))?;
        let opening = RecordIndex::new(raw);
        if !has_type(tape, opening, r#""JSXOpeningElement""#) {
            continue;
        }
        let Some(name) = tape
            .field_index(opening, "name")
            .and_then(|field| tape.field_value(field))
            .and_then(ValueRef::as_object)
            .filter(|name| has_type(tape, *name, r#""JSXIdentifier""#))
        else {
            continue;
        };
        if scalar_field(tape, name, "name")? != r#""style""# {
            continue;
        }
        let owner = *expected_order
            .get(next)
            .ok_or(TsrxParseError::Unsupported("projected style has no authored owner"))?;
        let expected = overlay
            .style_blocks
            .get(owner)
            .ok_or(TsrxParseError::Unsupported("unknown authored style owner"))?;
        let start = map_endpoint(segments, scalar_u32(tape, opening, "start")?, true).ok_or(
            TsrxParseError::Unsupported("projected style start is outside authored source"),
        )?;
        if start != expected.element.start {
            return Err(TsrxParseError::Unsupported(
                "projected styles are not in canonical opening order",
            ));
        }
        let element = parents
            .parent_container(ValueRef::object(opening))
            .and_then(ValueRef::as_object)
            .ok_or(TsrxParseError::Unsupported("style opening has no JSX element parent"))?;
        require_type(tape, element, r#""JSXElement""#)?;
        if field_value(tape, element, "openingElement")? != ValueRef::object(opening) {
            return Err(TsrxParseError::Unsupported(
                "style opening is not owned by its JSX element",
            ));
        }
        if styles[owner].replace(ProjectedStyle { element, opening }).is_some() {
            return Err(TsrxParseError::Unsupported("projected style owner is duplicated"));
        }
        next += 1;
    }
    if next != overlay.style_blocks.len() {
        return Err(TsrxParseError::Unsupported("projected style element set is incomplete"));
    }
    styles
        .into_iter()
        .map(|style| style.ok_or(TsrxParseError::Unsupported("projected style owner is missing")))
        .collect()
}

fn style_opening_postorder(overlay: OverlayView<'_>) -> Result<Vec<usize>, TsrxParseError> {
    let mut order = Vec::with_capacity(overlay.style_blocks.len());
    let mut stack = Vec::<usize>::with_capacity(4);
    for (index, style) in overlay.style_blocks.iter().enumerate() {
        while stack
            .last()
            .is_some_and(|owner| overlay.style_blocks[*owner].element.end <= style.element.start)
        {
            order.push(stack.pop().expect("the style stack has a last owner"));
        }
        if stack
            .last()
            .is_some_and(|owner| style.element.end > overlay.style_blocks[*owner].content.start)
        {
            return Err(TsrxParseError::Unsupported("style opening preorder has crossing spans"));
        }
        stack.push(index);
    }
    while let Some(owner) = stack.pop() {
        order.push(owner);
    }
    Ok(order)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
fn reconstruct_style_element(
    tape: &mut FlatTape,
    authored: &str,
    style: tsrx_syntax::OverlayStyleBlock,
    projected: ProjectedStyle,
    segments: &[ProjectionSegment],
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
    semicolons: &mut Vec<ListValueInsertion>,
) -> Result<(), TsrxParseError> {
    let opening_span = ByteSpan::new(
        style.element.start,
        if style.self_closing { style.element.end } else { style.content.start },
    );
    require_mapped_object_span(tape, projected.element, style.element, segments)?;
    require_mapped_object_span(tape, projected.opening, opening_span, segments)?;

    let attributes = list_field(tape, projected.opening, "attributes")?;
    let opening_name = object_field(tape, projected.opening, "name")?;
    require_static_style_name(tape, opening_name)?;
    let projected_self_closing = scalar_field(tape, projected.opening, "selfClosing")?;
    if projected_self_closing != if style.self_closing { "true" } else { "false" } {
        return Err(TsrxParseError::Unsupported(
            "projected style self-closing flag disagrees with overlay",
        ));
    }
    let opening_name_end = style
        .element
        .start
        .checked_add(6)
        .ok_or(TsrxParseError::Unsupported("style name span overflow"))?;
    require_mapped_object_span(
        tape,
        opening_name,
        ByteSpan::new(style.element.start.saturating_add(1), opening_name_end),
        segments,
    )?;

    append_empty_metadata(tape, projected.element)?;
    let metadata = object_field(tape, projected.element, "metadata")?;
    require_empty_metadata(tape, metadata)?;
    let children = list_field(tape, projected.element, "children")?;
    let closing_value = field_value(tape, projected.element, "closingElement")?;

    let (closing, css) = if style.self_closing {
        if tape.scalar(closing_value) != Some("null") || tape.values(children).next().is_some() {
            return Err(TsrxParseError::Unsupported(
                "self-closing style has projected closing content",
            ));
        }
        (None, None)
    } else {
        let (closing, closing_name, closing_span, css) = consume_paired_style_scaffold(
            tape,
            authored,
            style,
            children,
            closing_value,
            segments,
        )?;
        (Some((closing, closing_name, closing_span)), Some(css))
    };
    let children =
        if let Some(css) = css { build_style_children(tape, css, starts)? } else { children };

    rebuild_style_opening(
        tape,
        projected.opening,
        opening_span,
        attributes,
        opening_name,
        style.self_closing,
        starts,
    )?;
    let closing = if let Some((closing, name, span)) = closing {
        rebuild_style_closing(tape, closing, span, name, starts)?;
        Some(closing)
    } else {
        None
    };
    rebuild_style_element_node(
        tape,
        projected.element,
        style.element,
        metadata,
        children,
        projected.opening,
        closing,
        css,
        starts,
    )?;
    normalize_custom_jsx_statement(
        tape,
        authored,
        projected.element,
        style.element,
        segments,
        parents,
        starts,
        semicolons,
        style.self_closing,
    )?;
    Ok(())
}

fn consume_paired_style_scaffold<'a>(
    tape: &mut FlatTape,
    authored: &'a str,
    style: tsrx_syntax::OverlayStyleBlock,
    children: RecordIndex,
    closing_value: ValueRef,
    segments: &[ProjectionSegment],
) -> Result<(RecordIndex, RecordIndex, ByteSpan, &'a str), TsrxParseError> {
    let closing = closing_value
        .as_object()
        .ok_or(TsrxParseError::Unsupported("paired style has no projected closing element"))?;
    require_type(tape, closing, r#""JSXClosingElement""#)?;
    let closing_span = ByteSpan::new(style.content.end, style.element.end);
    require_mapped_object_span(tape, closing, closing_span, segments)?;
    let closing_name = object_field(tape, closing, "name")?;
    require_static_style_name(tape, closing_name)?;
    let closing_name_start = style
        .content
        .end
        .checked_add(2)
        .ok_or(TsrxParseError::Unsupported("style closing name overflow"))?;
    let closing_name_end = style
        .element
        .end
        .checked_sub(1)
        .ok_or(TsrxParseError::Unsupported("style closing name underflow"))?;
    require_mapped_object_span(
        tape,
        closing_name,
        ByteSpan::new(closing_name_start, closing_name_end),
        segments,
    )?;

    let helper = exact_one_value(tape, children)?
        .as_object()
        .ok_or(TsrxParseError::Unsupported("style payload scaffold is not an object"))?;
    require_type(tape, helper, r#""JSXExpressionContainer""#)?;
    let scaffold_start = project_authored_end(segments, style.content.start)
        .ok_or(TsrxParseError::Unsupported("style scaffold start is unmapped"))?;
    let scaffold_end = project_authored_start(segments, style.content.end)
        .ok_or(TsrxParseError::Unsupported("style scaffold end is unmapped"))?;
    if scalar_u32(tape, helper, "start")? != scaffold_start
        || scalar_u32(tape, helper, "end")? != scaffold_end
    {
        return Err(TsrxParseError::Unsupported("style payload scaffold span is displaced"));
    }
    let sentinel = object_field(tape, helper, "expression")?;
    require_type(tape, sentinel, r#""Literal""#)?;
    if tape.scalar(field_value(tape, sentinel, "value")?) != Some("null") {
        return Err(TsrxParseError::Unsupported("style payload sentinel is not null"));
    }
    if tape.pop_list_value(children)? != ValueRef::object(helper) {
        return Err(TsrxParseError::Unsupported("style payload scaffold is not the sole child"));
    }
    Ok((closing, closing_name, closing_span, slice_authored(authored, style.content)?))
}

fn require_static_style_name(tape: &FlatTape, name: RecordIndex) -> Result<(), TsrxParseError> {
    require_type(tape, name, r#""JSXIdentifier""#)?;
    if scalar_field(tape, name, "name")? != r#""style""# {
        return Err(TsrxParseError::Unsupported("projected style name is not lowercase style"));
    }
    Ok(())
}

fn require_mapped_object_span(
    tape: &FlatTape,
    object: RecordIndex,
    authored: ByteSpan,
    segments: &[ProjectionSegment],
) -> Result<(), TsrxParseError> {
    let start = map_endpoint(segments, scalar_u32(tape, object, "start")?, true);
    let end = map_endpoint(segments, scalar_u32(tape, object, "end")?, false);
    if start != Some(authored.start) || end != Some(authored.end) {
        return Err(TsrxParseError::Unsupported(
            "projected style span differs from authored source",
        ));
    }
    Ok(())
}

fn slice_authored(authored: &str, span: ByteSpan) -> Result<&str, TsrxParseError> {
    let start = usize::try_from(span.start)
        .map_err(|_| TsrxParseError::Unsupported("style span exceeds host usize"))?;
    let end = usize::try_from(span.end)
        .map_err(|_| TsrxParseError::Unsupported("style span exceeds host usize"))?;
    authored
        .get(start..end)
        .ok_or(TsrxParseError::Unsupported("style span is not a source boundary"))
}

fn rebuild_style_opening(
    tape: &mut FlatTape,
    opening: RecordIndex,
    span: ByteSpan,
    attributes: RecordIndex,
    name: RecordIndex,
    self_closing: bool,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    tape.clear_fields(opening)?;
    append_node_head(tape, opening, r#""JSXOpeningElement""#, span)?;
    tape.append_field(opening, "attributes", ValueRef::list(attributes))?;
    tape.append_field(opening, "name", ValueRef::object(name))?;
    let self_closing = tape.push_scalar(if self_closing { "true" } else { "false" })?;
    tape.append_field(opening, "selfClosing", self_closing)?;
    record_authored_span(starts, opening, span);
    Ok(())
}

fn rebuild_style_closing(
    tape: &mut FlatTape,
    closing: RecordIndex,
    span: ByteSpan,
    name: RecordIndex,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    tape.clear_fields(closing)?;
    append_node_head(tape, closing, r#""JSXClosingElement""#, span)?;
    tape.append_field(closing, "name", ValueRef::object(name))?;
    record_authored_span(starts, closing, span);
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
fn rebuild_style_element_node(
    tape: &mut FlatTape,
    element: RecordIndex,
    span: ByteSpan,
    metadata: RecordIndex,
    children: RecordIndex,
    opening: RecordIndex,
    closing: Option<RecordIndex>,
    css: Option<&str>,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    tape.clear_fields(element)?;
    append_node_head(tape, element, r#""JSXStyleElement""#, span)?;
    tape.append_field(element, "metadata", ValueRef::object(metadata))?;
    tape.append_field(element, "children", ValueRef::list(children))?;
    tape.append_field(element, "openingElement", ValueRef::object(opening))?;
    let closing = if let Some(closing) = closing {
        ValueRef::object(closing)
    } else {
        tape.push_scalar("null")?
    };
    tape.append_field(element, "closingElement", closing)?;
    if let Some(css) = css {
        let css = tape.push_json_string_scalar(css)?;
        tape.append_field(element, "css", css)?;
    }
    record_authored_span(starts, element, span);
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
struct CssListBuilder {
    first: RecordIndex,
    last: RecordIndex,
    length: u32,
}

impl CssListBuilder {
    fn push(&mut self, tape: &mut FlatTape, value: ValueRef) -> Result<(), TsrxParseError> {
        let entry =
            tape.push_list_value_record(ListValueRecord { value, next: RecordIndex::NONE })?;
        if self.first.is_none() {
            self.first = entry;
        } else {
            tape.set_list_value_next(self.last, entry)?;
        }
        self.last = entry;
        self.length = self
            .length
            .checked_add(1)
            .ok_or(TsrxParseError::Unsupported("CSS list length overflow"))?;
        Ok(())
    }

    fn finish(self, tape: &mut FlatTape) -> Result<RecordIndex, TsrxParseError> {
        tape.push_list_record(ListRecord { first_value: self.first, length: self.length })
            .map_err(Into::into)
    }
}

struct CssTapeBuilder<'tape, 'source, 'starts> {
    tape: &'tape mut FlatTape,
    source: &'source str,
    coordinates: CssCoordinates,
    starts: &'starts mut Vec<AuthoredStart>,
}

impl CssTapeBuilder<'_, '_, '_> {
    fn stylesheet(&mut self) -> Result<RecordIndex, TsrxParseError> {
        let children = self.rule_children(0, self.source.len())?;
        let sheet = self.node(r#""StyleSheet""#, 0, self.source.len())?;
        self.tape.append_field(sheet, "children", ValueRef::list(children))?;
        let source = self.tape.push_json_string_scalar(self.source)?;
        self.tape.append_field(sheet, "source", source)?;
        Ok(sheet)
    }

    fn rule_children(&mut self, start: usize, end: usize) -> Result<RecordIndex, TsrxParseError> {
        let mut list = CssListBuilder::default();
        let mut cursor = start;
        while cursor < end {
            skip_css_trivia(self.source.as_bytes(), &mut cursor, end);
            if cursor >= end {
                break;
            }
            let before = cursor;
            let node = if self.source.as_bytes()[cursor] == b'@' {
                self.at_rule(&mut cursor, end)?
            } else {
                self.rule(&mut cursor, end)?
            };
            if let Some(node) = node {
                list.push(self.tape, ValueRef::object(node))?;
            }
            if cursor <= before {
                cursor = before + 1;
            }
        }
        list.finish(self.tape)
    }

    fn at_rule(
        &mut self,
        cursor: &mut usize,
        end: usize,
    ) -> Result<Option<RecordIndex>, TsrxParseError> {
        let start = *cursor;
        let bytes = self.source.as_bytes();
        let mut name_end = start + 1;
        while name_end < end
            && matches!(bytes[name_end], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_')
        {
            name_end += 1;
        }
        let Some((delimiter, kind)) = scan_css_delimiter(bytes, name_end, end) else {
            *cursor = end;
            return Ok(None);
        };
        if kind == b'}' {
            *cursor = delimiter;
            return Ok(None);
        }

        let (prelude_start, prelude_end) = trim_css_range(bytes, name_end, delimiter);
        let (block, node_end) = if kind == b'{' {
            let closing = find_css_block_end(bytes, delimiter, end);
            let content_end = closing.unwrap_or(end);
            let children = self.rule_children(delimiter + 1, content_end)?;
            let block_end = closing.map_or(end, |closing| closing + 1);
            let block = self.block(delimiter, block_end, children)?;
            (Some(block), block_end)
        } else {
            (None, delimiter + 1)
        };
        *cursor = node_end;

        let node = self.node(r#""Atrule""#, start, node_end)?;
        let name = self
            .tape
            .push_json_string_scalar(self.source.get(start + 1..name_end).unwrap_or(""))?;
        self.tape.append_field(node, "name", name)?;
        let prelude = self
            .tape
            .push_json_string_scalar(self.source.get(prelude_start..prelude_end).unwrap_or(""))?;
        self.tape.append_field(node, "prelude", prelude)?;
        let block = if let Some(block) = block {
            ValueRef::object(block)
        } else {
            self.tape.push_scalar("null")?
        };
        self.tape.append_field(node, "block", block)?;
        Ok(Some(node))
    }

    fn rule(
        &mut self,
        cursor: &mut usize,
        end: usize,
    ) -> Result<Option<RecordIndex>, TsrxParseError> {
        let start = *cursor;
        let bytes = self.source.as_bytes();
        let Some((delimiter, kind)) = scan_css_delimiter(bytes, start, end) else {
            *cursor = end;
            return Ok(None);
        };
        if kind != b'{' {
            *cursor = if kind == b';' { delimiter + 1 } else { delimiter };
            return Ok(None);
        }
        let (selector_start, selector_end) = trim_css_range(bytes, start, delimiter);
        let closing = find_css_block_end(bytes, delimiter, end);
        let node_end = closing.map_or(end, |closing| closing + 1);
        *cursor = node_end;
        if selector_start == selector_end {
            return Ok(None);
        }

        let prelude = self.selector_list(selector_start, selector_end)?;
        let empty = CssListBuilder::default().finish(self.tape)?;
        let block = self.block(delimiter, node_end, empty)?;
        let rule = self.node(r#""Rule""#, selector_start, node_end)?;
        self.tape.append_field(rule, "prelude", ValueRef::object(prelude))?;
        self.tape.append_field(rule, "block", ValueRef::object(block))?;
        Ok(Some(rule))
    }

    fn selector_list(&mut self, start: usize, end: usize) -> Result<RecordIndex, TsrxParseError> {
        let bytes = self.source.as_bytes();
        let mut selectors = CssListBuilder::default();
        let mut segment_start = start;
        let mut cursor = start;
        let mut quote = None;
        let mut escaped = false;
        let mut parentheses = 0_u32;
        let mut brackets = 0_u32;
        while cursor <= end {
            let at_end = cursor == end;
            let byte = (!at_end).then(|| bytes[cursor]);
            if quote.is_some() {
                if escaped {
                    escaped = false;
                } else if byte == Some(b'\\') {
                    escaped = true;
                } else if byte == quote {
                    quote = None;
                }
                cursor += 1;
                continue;
            }
            match byte {
                Some(b'\'' | b'"') => quote = byte,
                Some(b'(') => parentheses = parentheses.saturating_add(1),
                Some(b')') => parentheses = parentheses.saturating_sub(1),
                Some(b'[') => brackets = brackets.saturating_add(1),
                Some(b']') => brackets = brackets.saturating_sub(1),
                Some(b'/') if bytes.get(cursor + 1) == Some(&b'*') => {
                    cursor = skip_css_comment(bytes, cursor + 2, end);
                    continue;
                }
                Some(b',') if parentheses == 0 && brackets == 0 => {
                    self.push_complex_selector(&mut selectors, segment_start, cursor)?;
                    segment_start = cursor + 1;
                }
                None => self.push_complex_selector(&mut selectors, segment_start, end)?,
                _ => {}
            }
            cursor += 1;
        }
        let selector_list = self.node(r#""SelectorList""#, start, end)?;
        let selectors = selectors.finish(self.tape)?;
        self.tape.append_field(selector_list, "children", ValueRef::list(selectors))?;
        Ok(selector_list)
    }

    fn push_complex_selector(
        &mut self,
        selectors: &mut CssListBuilder,
        start: usize,
        end: usize,
    ) -> Result<(), TsrxParseError> {
        let bytes = self.source.as_bytes();
        let (mut start, end) = trim_css_range(bytes, start, end);
        skip_css_trivia(bytes, &mut start, end);
        if start >= end {
            return Ok(());
        }
        let selector = self.node(r#""ComplexSelector""#, start, end)?;
        let children = CssListBuilder::default().finish(self.tape)?;
        self.tape.append_field(selector, "children", ValueRef::list(children))?;
        selectors.push(self.tape, ValueRef::object(selector))
    }

    fn block(
        &mut self,
        start: usize,
        end: usize,
        children: RecordIndex,
    ) -> Result<RecordIndex, TsrxParseError> {
        let block = self.node(r#""Block""#, start, end)?;
        self.tape.append_field(block, "children", ValueRef::list(children))?;
        Ok(block)
    }

    fn node(
        &mut self,
        kind: &str,
        start: usize,
        end: usize,
    ) -> Result<RecordIndex, TsrxParseError> {
        let object = self.tape.push_object_record(ObjectRecord::default())?;
        let span = ByteSpan::new(
            self.coordinates.utf16_offset(start)?,
            self.coordinates.utf16_offset(end)?,
        );
        append_node_head(self.tape, object, kind, span)?;
        record_authored_span(self.starts, object, span);
        Ok(object)
    }
}

fn build_style_children(
    tape: &mut FlatTape,
    css: &str,
    starts: &mut Vec<AuthoredStart>,
) -> Result<RecordIndex, TsrxParseError> {
    let stylesheet =
        CssTapeBuilder { tape, source: css, coordinates: CssCoordinates::new(css), starts }
            .stylesheet()?;
    let mut children = CssListBuilder::default();
    children.push(tape, ValueRef::object(stylesheet))?;
    children.finish(tape)
}

struct CssCoordinates {
    adjustments: Vec<(usize, usize)>,
}

impl CssCoordinates {
    fn new(source: &str) -> Self {
        let mut adjustments = Vec::new();
        let mut reduction = 0_usize;
        for (start, character) in source.char_indices() {
            let utf8 = character.len_utf8();
            let utf16 = character.len_utf16();
            if utf8 != utf16 {
                reduction += utf8 - utf16;
                adjustments.push((start + utf8, reduction));
            }
        }
        Self { adjustments }
    }

    fn utf16_offset(&self, utf8_offset: usize) -> Result<u32, TsrxParseError> {
        let completed = self.adjustments.partition_point(|(end, _)| *end <= utf8_offset);
        let reduction = completed
            .checked_sub(1)
            .and_then(|index| self.adjustments.get(index))
            .map_or(0, |(_, reduction)| *reduction);
        let offset = utf8_offset
            .checked_sub(reduction)
            .ok_or(TsrxParseError::Unsupported("invalid CSS UTF-16 offset"))?;
        u32::try_from(offset).map_err(|_| TsrxParseError::Unsupported("CSS offset exceeds 4 GiB"))
    }
}

fn trim_css_range(bytes: &[u8], mut start: usize, mut end: usize) -> (usize, usize) {
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    (start, end)
}

fn skip_css_trivia(bytes: &[u8], cursor: &mut usize, end: usize) {
    loop {
        while *cursor < end && bytes[*cursor].is_ascii_whitespace() {
            *cursor += 1;
        }
        if *cursor + 1 < end && bytes[*cursor..].starts_with(b"/*") {
            *cursor = skip_css_comment(bytes, *cursor + 2, end);
        } else {
            return;
        }
    }
}

fn skip_css_comment(bytes: &[u8], mut cursor: usize, end: usize) -> usize {
    while cursor + 1 < end {
        if bytes[cursor] == b'*' && bytes[cursor + 1] == b'/' {
            return cursor + 2;
        }
        cursor += 1;
    }
    end
}

fn scan_css_delimiter(bytes: &[u8], mut cursor: usize, end: usize) -> Option<(usize, u8)> {
    let mut quote = None;
    let mut escaped = false;
    let mut parentheses = 0_u32;
    let mut brackets = 0_u32;
    while cursor < end {
        let byte = bytes[cursor];
        if quote.is_some() {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if Some(byte) == quote {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'/' if bytes.get(cursor + 1) == Some(&b'*') => {
                cursor = skip_css_comment(bytes, cursor + 2, end);
                continue;
            }
            b'(' => parentheses = parentheses.saturating_add(1),
            b')' => parentheses = parentheses.saturating_sub(1),
            b'[' => brackets = brackets.saturating_add(1),
            b']' => brackets = brackets.saturating_sub(1),
            b'{' | b';' | b'}' if parentheses == 0 && brackets == 0 => {
                return Some((cursor, byte));
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn find_css_block_end(bytes: &[u8], opening: usize, end: usize) -> Option<usize> {
    let mut cursor = opening + 1;
    let mut depth = 1_u32;
    let mut quote = None;
    let mut escaped = false;
    while cursor < end {
        let byte = bytes[cursor];
        if quote.is_some() {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if Some(byte) == quote {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'/' if bytes.get(cursor + 1) == Some(&b'*') => {
                cursor = skip_css_comment(bytes, cursor + 2, end);
                continue;
            }
            b'{' => depth = depth.saturating_add(1),
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(cursor);
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

#[derive(Debug, Clone, Copy, Default)]
struct DynamicTokenSpans {
    opening: Option<ByteSpan>,
    closing: Option<ByteSpan>,
}

fn reconstruct_dynamic_tags(
    tape: &mut FlatTape,
    authored: &str,
    overlay: OverlayView<'_>,
    segments: &[ProjectionSegment],
    prefix: &str,
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    if overlay.dynamic_tags.is_empty() {
        return Ok(());
    }
    let spans = collect_dynamic_token_spans(overlay)?;
    let openings = collect_dynamic_openings(tape, overlay.dynamic_tags.len(), prefix)?;
    let mut semicolons = Vec::new();
    for index in (0..overlay.dynamic_tags.len()).rev() {
        let opening = openings[index]
            .ok_or(TsrxParseError::Unsupported("projected dynamic opening is missing"))?;
        reconstruct_dynamic_tag(
            tape,
            authored,
            overlay.dynamic_tags[index],
            spans[index],
            segments,
            prefix,
            index,
            opening,
            parents,
            starts,
            &mut semicolons,
        )?;
    }
    tape.insert_list_values_after(&semicolons)?;
    Ok(())
}

fn collect_dynamic_token_spans(
    overlay: OverlayView<'_>,
) -> Result<Vec<DynamicTokenSpans>, TsrxParseError> {
    overlay
        .dynamic_tags
        .iter()
        .map(|tag| {
            if tag.opening.is_empty() || tag.self_closing != tag.closing.is_empty() {
                return Err(TsrxParseError::Unsupported("incomplete dynamic projection span"));
            }
            Ok(DynamicTokenSpans {
                opening: Some(tag.opening),
                closing: (!tag.self_closing).then_some(tag.closing),
            })
        })
        .collect()
}

fn collect_dynamic_openings(
    tape: &FlatTape,
    count: usize,
    prefix: &str,
) -> Result<Vec<Option<RecordIndex>>, TsrxParseError> {
    let mut openings = vec![None; count];
    for raw in 0..tape.object_count() {
        let raw = u32::try_from(raw)
            .map_err(|_| TsrxParseError::Unsupported("object table above 4 GiB"))?;
        let opening = RecordIndex::new(raw);
        if !has_type(tape, opening, r#""JSXOpeningElement""#) {
            continue;
        }
        let Some(name) = tape
            .field_index(opening, "name")
            .and_then(|field| tape.field_value(field))
            .and_then(ValueRef::as_object)
            .filter(|name| has_type(tape, *name, r#""JSXIdentifier""#))
        else {
            continue;
        };
        let Some(index) = tape
            .field_index(name, "name")
            .and_then(|field| tape.field_value(field))
            .and_then(|value| tape.scalar(value))
            .and_then(|name| dynamic_scaffold_index(name, prefix, 'D', false))
        else {
            continue;
        };
        let slot = openings
            .get_mut(index)
            .ok_or(TsrxParseError::Unsupported("unknown dynamic opening ordinal"))?;
        if slot.replace(opening).is_some() {
            return Err(TsrxParseError::Unsupported("duplicate dynamic opening ordinal"));
        }
    }
    Ok(openings)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
#[expect(
    clippy::too_many_lines,
    reason = "one flat match over every dynamic-tag shape the projection can carry"
)]
fn reconstruct_dynamic_tag(
    tape: &mut FlatTape,
    authored: &str,
    tag: tsrx_syntax::OverlayDynamicTag,
    spans: DynamicTokenSpans,
    segments: &[ProjectionSegment],
    prefix: &str,
    index: usize,
    opening: RecordIndex,
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
    semicolons: &mut Vec<ListValueInsertion>,
) -> Result<(), TsrxParseError> {
    let opening_span =
        spans.opening.ok_or(TsrxParseError::Unsupported("dynamic tag has no opening token"))?;
    let closing_span = spans.closing;
    if tag.self_closing != closing_span.is_none() {
        return Err(TsrxParseError::Unsupported("dynamic closing topology disagrees with overlay"));
    }
    let element = parents
        .parent_container(ValueRef::object(opening))
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("dynamic opening has no JSX element parent"))?;
    require_type(tape, element, r#""JSXElement""#)?;
    if field_value(tape, element, "openingElement")? != ValueRef::object(opening) {
        return Err(TsrxParseError::Unsupported("dynamic opening is not owned by its JSX element"));
    }
    append_empty_metadata(tape, element)?;
    let metadata = object_field(tape, element, "metadata")?;
    require_empty_metadata(tape, metadata)?;
    let children = list_field(tape, element, "children")?;
    let attributes = list_field(tape, opening, "attributes")?;
    let projected_name = object_field(tape, opening, "name")?;
    require_dynamic_identifier(tape, projected_name, prefix, 'D', index, false)?;
    let projected_self_closing = scalar_field(tape, opening, "selfClosing")?;
    if projected_self_closing != if tag.self_closing { "true" } else { "false" } {
        return Err(TsrxParseError::Unsupported(
            "projected dynamic self-closing flag disagrees with overlay",
        ));
    }
    let opening_end = map_endpoint(segments, scalar_u32(tape, opening, "end")?, false)
        .ok_or(TsrxParseError::Unsupported("dynamic opening end is outside authored source"))?;
    if opening_end <= opening_span.end {
        return Err(TsrxParseError::Unsupported(
            "dynamic opening element does not include its terminator",
        ));
    }

    let (attribute_expression_container, opening_expression, first_attribute, end_attribute) =
        dynamic_opening_expression(tape, attributes, tag.expression, segments, prefix, index)?;
    let opening_expression = unwrap_parenthesized_expression(tape, opening_expression)?;
    require_expression_within(tape, opening_expression, tag.expression, segments)?;

    let closing_value = field_value(tape, element, "closingElement")?;
    let closing = if let Some(closing_span) = closing_span {
        let closing = closing_value
            .as_object()
            .ok_or(TsrxParseError::Unsupported("paired dynamic element has no closing element"))?;
        require_type(tape, closing, r#""JSXClosingElement""#)?;
        let projected_closing_name = object_field(tape, closing, "name")?;
        require_dynamic_identifier(tape, projected_closing_name, prefix, 'D', index, false)?;
        let (container, expression) = dynamic_closing_expression(
            tape,
            children,
            tag.closing_expression,
            segments,
            prefix,
            index,
        )?;
        if expression == opening_expression || container == attribute_expression_container {
            return Err(TsrxParseError::Unsupported(
                "dynamic opening and closing expressions share projected identity",
            ));
        }
        let removed = tape.pop_list_value(children)?;
        if removed != ValueRef::object(container) {
            return Err(TsrxParseError::Unsupported(
                "dynamic closing helper is not the final child",
            ));
        }
        rebuild_dynamic_name(
            tape,
            container,
            ByteSpan::new(
                closing_span.start.saturating_add(2),
                tag.closing_expression.end.saturating_add(1),
            ),
            expression,
            starts,
        )?;
        rebuild_dynamic_closing(tape, closing, closing_span, container, starts)?;
        Some(closing)
    } else {
        if tape.scalar(closing_value) != Some("null") {
            return Err(TsrxParseError::Unsupported(
                "self-closing dynamic element has a closing object",
            ));
        }
        None
    };

    let removed_first = tape.remove_list_value(attributes, first_attribute)?;
    let removed_end = tape.remove_list_value(attributes, end_attribute)?;
    if removed_first.kind() != ValueKind::Object || removed_end.kind() != ValueKind::Object {
        return Err(TsrxParseError::Unsupported("dynamic attributes are not object entries"));
    }
    rebuild_dynamic_name(
        tape,
        attribute_expression_container,
        ByteSpan::new(opening_span.start.saturating_add(1), opening_span.end),
        opening_expression,
        starts,
    )?;
    rebuild_dynamic_opening(
        tape,
        opening,
        ByteSpan::new(opening_span.start, opening_end),
        attributes,
        attribute_expression_container,
        tag.self_closing,
        starts,
    )?;
    let element_end = closing_span.map_or(opening_end, |span| span.end);
    rebuild_dynamic_element(
        tape,
        element,
        ByteSpan::new(opening_span.start, element_end),
        metadata,
        children,
        opening,
        closing,
        starts,
    )?;
    normalize_custom_jsx_statement(
        tape,
        authored,
        element,
        ByteSpan::new(opening_span.start, element_end),
        segments,
        parents,
        starts,
        semicolons,
        true,
    )?;
    Ok(())
}

fn dynamic_opening_expression(
    tape: &FlatTape,
    attributes: RecordIndex,
    authored: ByteSpan,
    segments: &[ProjectionSegment],
    prefix: &str,
    index: usize,
) -> Result<(RecordIndex, RecordIndex, RecordIndex, RecordIndex), TsrxParseError> {
    let first_entry = tape
        .list_first_value(attributes)
        .filter(|entry| !entry.is_none())
        .ok_or(TsrxParseError::Unsupported("dynamic opening has no expression attribute"))?;
    let end_entry = tape
        .list_value_next(first_entry)
        .filter(|entry| !entry.is_none())
        .ok_or(TsrxParseError::Unsupported("dynamic opening has no end sentinel attribute"))?;
    let expression_attribute = tape
        .list_value(first_entry)
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("dynamic expression attribute is not an object"))?;
    let end_attribute = tape
        .list_value(end_entry)
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("dynamic end attribute is not an object"))?;
    require_type(tape, expression_attribute, r#""JSXAttribute""#)?;
    require_type(tape, end_attribute, r#""JSXAttribute""#)?;
    let expression_name = object_field(tape, expression_attribute, "name")?;
    let end_name = object_field(tape, end_attribute, "name")?;
    require_dynamic_identifier(tape, expression_name, prefix, 'A', index, true)?;
    require_dynamic_identifier(tape, end_name, prefix, 'Z', index, true)?;

    let container = object_field(tape, expression_attribute, "value")?;
    require_type(tape, container, r#""JSXExpressionContainer""#)?;
    let expression = object_field(tape, container, "expression")?;
    require_expression_within(
        tape,
        unwrap_parenthesized_expression(tape, expression)?,
        authored,
        segments,
    )?;

    let end_container = object_field(tape, end_attribute, "value")?;
    require_type(tape, end_container, r#""JSXExpressionContainer""#)?;
    let sentinel = object_field(tape, end_container, "expression")?;
    require_type(tape, sentinel, r#""Literal""#)?;
    if tape.scalar(field_value(tape, sentinel, "value")?) != Some("null") {
        return Err(TsrxParseError::Unsupported("dynamic end sentinel is not null"));
    }
    Ok((container, expression, first_entry, end_entry))
}

fn dynamic_closing_expression(
    tape: &FlatTape,
    children: RecordIndex,
    authored: ByteSpan,
    segments: &[ProjectionSegment],
    prefix: &str,
    index: usize,
) -> Result<(RecordIndex, RecordIndex), TsrxParseError> {
    let helper = tape
        .values(children)
        .last()
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("dynamic closing helper child is missing"))?;
    require_type(tape, helper, r#""JSXExpressionContainer""#)?;
    let call = object_field(tape, helper, "expression")?;
    require_type(tape, call, r#""CallExpression""#)?;
    let callee = object_field(tape, call, "callee")?;
    require_dynamic_identifier(tape, callee, prefix, 'C', index, true)?;
    if tape.field_index(call, "optional").is_some_and(|field| {
        tape.field_value(field).and_then(|value| tape.scalar(value)) != Some("false")
    }) {
        return Err(TsrxParseError::Unsupported("dynamic closing helper call is optional"));
    }
    let grouped = exact_one_value(tape, list_field(tape, call, "arguments")?)?.as_object().ok_or(
        TsrxParseError::Unsupported("dynamic closing helper argument is not an expression"),
    )?;
    let expression = unwrap_parenthesized_expression(tape, grouped)?;
    require_expression_within(tape, expression, authored, segments)?;
    Ok((helper, expression))
}

fn unwrap_parenthesized_expression(
    tape: &FlatTape,
    mut expression: RecordIndex,
) -> Result<RecordIndex, TsrxParseError> {
    let mut remaining = tape.object_count();
    while has_type(tape, expression, r#""ParenthesizedExpression""#) {
        if remaining == 0 {
            return Err(TsrxParseError::Unsupported("cyclic parenthesized dynamic expression"));
        }
        expression = object_field(tape, expression, "expression")?;
        remaining -= 1;
    }
    Ok(expression)
}

fn require_expression_within(
    tape: &FlatTape,
    expression: RecordIndex,
    authored: ByteSpan,
    segments: &[ProjectionSegment],
) -> Result<(), TsrxParseError> {
    let start = map_endpoint(segments, scalar_u32(tape, expression, "start")?, true).ok_or(
        TsrxParseError::Unsupported("dynamic expression start is outside authored source"),
    )?;
    let end = map_endpoint(segments, scalar_u32(tape, expression, "end")?, false)
        .ok_or(TsrxParseError::Unsupported("dynamic expression end is outside authored source"))?;
    if authored.start <= start && start < end && end <= authored.end {
        Ok(())
    } else {
        Err(TsrxParseError::Unsupported("dynamic expression lies outside authored name"))
    }
}

fn require_dynamic_identifier(
    tape: &FlatTape,
    object: RecordIndex,
    prefix: &str,
    kind: char,
    index: usize,
    suffix: bool,
) -> Result<(), TsrxParseError> {
    require_type(tape, object, r#""Identifier""#)
        .or_else(|_| require_type(tape, object, r#""JSXIdentifier""#))?;
    if dynamic_scaffold_index(scalar_field(tape, object, "name")?, prefix, kind, suffix)
        == Some(index)
    {
        Ok(())
    } else {
        Err(TsrxParseError::Unsupported("dynamic scaffold identifier does not match owner"))
    }
}

fn dynamic_scaffold_index(encoded: &str, prefix: &str, kind: char, suffix: bool) -> Option<usize> {
    let value = encoded.strip_prefix('"')?.strip_suffix('"')?;
    let digits = value.strip_prefix(prefix)?.strip_prefix(kind)?;
    let digits = if suffix { digits.strip_suffix('_')? } else { digits };
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

fn require_empty_metadata(tape: &FlatTape, metadata: RecordIndex) -> Result<(), TsrxParseError> {
    let mut fields = tape.fields(metadata);
    let path = fields.next().ok_or(TsrxParseError::Unsupported("dynamic metadata has no path"))?;
    if tape.key(path) != "path" || fields.next().is_some() {
        return Err(TsrxParseError::Unsupported("dynamic metadata is not canonical"));
    }
    let path = path
        .value
        .as_list()
        .ok_or(TsrxParseError::Unsupported("dynamic metadata path is not a list"))?;
    if tape.values(path).next().is_some() {
        return Err(TsrxParseError::Unsupported("dynamic metadata path is not empty"));
    }
    Ok(())
}

fn append_node_head(
    tape: &mut FlatTape,
    object: RecordIndex,
    kind: &str,
    span: ByteSpan,
) -> Result<(), TsrxParseError> {
    let kind = tape.push_scalar(kind)?;
    let start = tape.push_u32_scalar(span.start)?;
    let end = tape.push_u32_scalar(span.end)?;
    tape.append_field(object, "type", kind)?;
    tape.append_field(object, "start", start)?;
    tape.append_field(object, "end", end)?;
    Ok(())
}

fn record_authored_span(starts: &mut Vec<AuthoredStart>, object: RecordIndex, span: ByteSpan) {
    starts.push(AuthoredStart { object, start: span.start, end: Some(span.end) });
}

fn rebuild_dynamic_name(
    tape: &mut FlatTape,
    name: RecordIndex,
    span: ByteSpan,
    expression: RecordIndex,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    tape.clear_fields(name)?;
    append_node_head(tape, name, r#""JSXExpressionContainer""#, span)?;
    tape.append_field(name, "expression", ValueRef::object(expression))?;
    let dynamic = tape.push_scalar("true")?;
    tape.append_field(name, "isDynamic", dynamic)?;
    record_authored_span(starts, name, span);
    Ok(())
}

fn rebuild_dynamic_opening(
    tape: &mut FlatTape,
    opening: RecordIndex,
    span: ByteSpan,
    attributes: RecordIndex,
    name: RecordIndex,
    self_closing: bool,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    tape.clear_fields(opening)?;
    append_node_head(tape, opening, r#""JSXOpeningElement""#, span)?;
    tape.append_field(opening, "attributes", ValueRef::list(attributes))?;
    tape.append_field(opening, "name", ValueRef::object(name))?;
    let dynamic = tape.push_scalar("true")?;
    tape.append_field(opening, "isDynamic", dynamic)?;
    let self_closing = tape.push_scalar(if self_closing { "true" } else { "false" })?;
    tape.append_field(opening, "selfClosing", self_closing)?;
    record_authored_span(starts, opening, span);
    Ok(())
}

fn rebuild_dynamic_closing(
    tape: &mut FlatTape,
    closing: RecordIndex,
    span: ByteSpan,
    name: RecordIndex,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    tape.clear_fields(closing)?;
    append_node_head(tape, closing, r#""JSXClosingElement""#, span)?;
    tape.append_field(closing, "name", ValueRef::object(name))?;
    let dynamic = tape.push_scalar("true")?;
    tape.append_field(closing, "isDynamic", dynamic)?;
    record_authored_span(starts, closing, span);
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
fn rebuild_dynamic_element(
    tape: &mut FlatTape,
    element: RecordIndex,
    span: ByteSpan,
    metadata: RecordIndex,
    children: RecordIndex,
    opening: RecordIndex,
    closing: Option<RecordIndex>,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    tape.clear_fields(element)?;
    append_node_head(tape, element, r#""JSXElement""#, span)?;
    tape.append_field(element, "metadata", ValueRef::object(metadata))?;
    tape.append_field(element, "children", ValueRef::list(children))?;
    tape.append_field(element, "openingElement", ValueRef::object(opening))?;
    let closing = if let Some(closing) = closing {
        ValueRef::object(closing)
    } else {
        tape.push_scalar("null")?
    };
    tape.append_field(element, "closingElement", closing)?;
    let dynamic = tape.push_scalar("true")?;
    tape.append_field(element, "isDynamic", dynamic)?;
    record_authored_span(starts, element, span);
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
fn normalize_custom_jsx_statement(
    tape: &mut FlatTape,
    authored: &str,
    element: RecordIndex,
    element_span: ByteSpan,
    segments: &[ProjectionSegment],
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
    semicolons: &mut Vec<ListValueInsertion>,
    separated_semicolon_is_jsx_text: bool,
) -> Result<(), TsrxParseError> {
    let mut expression_root = element;
    let mut parenthesized = false;
    let statement = loop {
        let Some(parent) = parents
            .parent_container(ValueRef::object(expression_root))
            .and_then(ValueRef::as_object)
        else {
            return Ok(());
        };
        if has_type(tape, parent, r#""ParenthesizedExpression""#) {
            if field_value(tape, parent, "expression")? != ValueRef::object(expression_root) {
                return Err(TsrxParseError::Unsupported(
                    "custom JSX parenthesis owns another expression",
                ));
            }
            expression_root = parent;
            parenthesized = true;
            continue;
        }
        break parent;
    };
    if !has_type(tape, statement, r#""ExpressionStatement""#) {
        return Ok(());
    }
    if field_value(tape, statement, "expression")? != ValueRef::object(expression_root) {
        return Err(TsrxParseError::Unsupported(
            "custom JSX expression statement owns another expression",
        ));
    }
    if parenthesized {
        let expression_slot = parents.parent_slot(ValueRef::object(expression_root)).ok_or(
            TsrxParseError::Unsupported(
                "parenthesized custom JSX statement has no expression slot",
            ),
        )?;
        ParentIndex::replace(tape, expression_slot, ValueRef::object(element))?;
        return Ok(());
    }
    let element_end = usize::try_from(element_span.end)
        .map_err(|_| TsrxParseError::Unsupported("custom JSX end exceeds host usize"))?;
    let semicolon_start = skip_custom_jsx_statement_trivia(authored, element_end)?;
    let has_semicolon = authored.as_bytes().get(semicolon_start) == Some(&b';');
    let authored_end = if has_semicolon {
        u32::try_from(semicolon_start)
            .ok()
            .and_then(|start| start.checked_add(1))
            .ok_or(TsrxParseError::Unsupported("custom JSX statement span overflow"))?
    } else {
        element_span.end
    };
    if map_endpoint(segments, scalar_u32(tape, statement, "end")?, false)
        .is_some_and(|mapped| mapped != authored_end)
    {
        return Err(TsrxParseError::Unsupported(
            "custom JSX statement has unsupported trailing syntax",
        ));
    }
    let statement_slot = parents
        .parent_slot(ValueRef::object(statement))
        .ok_or(TsrxParseError::Unsupported("custom JSX statement has no parent slot"))?;
    ParentIndex::replace(tape, statement_slot, ValueRef::object(element))?;
    let (list, entry) = custom_jsx_statement_list_anchor(tape, parents, statement, statement_slot)?;
    if has_semicolon {
        let start = u32::try_from(semicolon_start)
            .map_err(|_| TsrxParseError::Unsupported("custom JSX semicolon exceeds 4 GiB"))?;
        let span = ByteSpan::new(start, authored_end);
        let jsx_text = separated_semicolon_is_jsx_text && start != element_span.end;
        let value = build_custom_jsx_semicolon(tape, span, jsx_text, starts)?;
        semicolons.push(ListValueInsertion { list, after: entry, value });
    }
    Ok(())
}

fn skip_custom_jsx_statement_trivia(
    authored: &str,
    mut index: usize,
) -> Result<usize, TsrxParseError> {
    let bytes = authored.as_bytes();
    loop {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index..index + 2) == Some(b"//") {
            index += 2;
            while bytes.get(index).is_some_and(|byte| !matches!(byte, b'\n' | b'\r')) {
                index += 1;
            }
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            index += 2;
            while bytes.get(index..index + 2) != Some(b"*/") {
                if bytes.get(index).is_none() {
                    return Err(TsrxParseError::Unsupported(
                        "unterminated comment after custom JSX statement",
                    ));
                }
                index += 1;
            }
            index += 2;
        } else {
            return Ok(index);
        }
    }
}

fn custom_jsx_statement_list_anchor(
    tape: &FlatTape,
    parents: &ParentIndex,
    statement: RecordIndex,
    statement_slot: ParentSlot,
) -> Result<(RecordIndex, RecordIndex), TsrxParseError> {
    let mut current = statement;
    let mut slot = statement_slot;
    loop {
        match slot {
            ParentSlot::ListValue(entry) => {
                let list = parents
                    .parent_container(ValueRef::object(current))
                    .and_then(ValueRef::as_list)
                    .ok_or(TsrxParseError::Unsupported(
                        "custom JSX statement has no parent list",
                    ))?;
                return Ok((list, entry));
            }
            ParentSlot::Field(field) => {
                let label = parents
                    .parent_container(ValueRef::object(current))
                    .and_then(ValueRef::as_object)
                    .filter(|owner| has_type(tape, *owner, r#""LabeledStatement""#))
                    .ok_or(TsrxParseError::Unsupported(
                        "custom JSX statement is in an unsupported field",
                    ))?;
                if tape.field_index(label, "body") != Some(field) {
                    return Err(TsrxParseError::Unsupported(
                        "custom JSX statement is not a label body",
                    ));
                }
                current = label;
                slot = parents.parent_slot(ValueRef::object(label)).ok_or(
                    TsrxParseError::Unsupported("labeled custom JSX statement has no parent slot"),
                )?;
            }
        }
    }
}

fn build_custom_jsx_semicolon(
    tape: &mut FlatTape,
    span: ByteSpan,
    jsx_text: bool,
    starts: &mut Vec<AuthoredStart>,
) -> Result<ValueRef, TsrxParseError> {
    if !jsx_text {
        let empty = tape.push_object_record(ObjectRecord::default())?;
        append_node_head(tape, empty, r#""EmptyStatement""#, span)?;
        record_authored_span(starts, empty, span);
        return Ok(ValueRef::object(empty));
    }

    let text = tape.push_object_record(ObjectRecord::default())?;
    append_node_head(tape, text, r#""JSXText""#, span)?;
    let value = tape.push_scalar(r#"";""#)?;
    tape.append_field(text, "value", value)?;
    let raw = tape.push_scalar(r#"";""#)?;
    tape.append_field(text, "raw", raw)?;
    record_authored_span(starts, text, span);

    let statement = tape.push_object_record(ObjectRecord::default())?;
    append_node_head(tape, statement, r#""ExpressionStatement""#, span)?;
    tape.append_field(statement, "expression", ValueRef::object(text))?;
    record_authored_span(starts, statement, span);
    Ok(ValueRef::object(statement))
}

fn normalize_control_body_lists(
    tape: &mut FlatTape,
    bodies: &[RecordIndex],
) -> Result<(), TsrxParseError> {
    let mut replacements = Vec::new();
    for &body in bodies {
        prepare_body_list(tape, body, &mut replacements)?;
    }
    Ok(())
}

fn prepare_body_list(
    tape: &mut FlatTape,
    body: RecordIndex,
    replacements: &mut Vec<(RecordIndex, ValueRef)>,
) -> Result<(), TsrxParseError> {
    let scratch_start = replacements.len();
    for (entry, value) in tape.values_indexed(body) {
        let Some(statement) = value.as_object() else {
            continue;
        };
        if !has_type(tape, statement, r#""ExpressionStatement""#) {
            continue;
        }
        let expression = field_value(tape, statement, "expression")?;
        let Some(expression_object) = expression.as_object() else {
            continue;
        };
        if is_jsx_child_type(tape, expression_object) {
            replacements.push((entry, expression));
        }
    }
    for &(entry, value) in &replacements[scratch_start..] {
        tape.set_list_value(entry, value)?;
    }
    replacements.truncate(scratch_start);
    Ok(())
}

fn order_span_fields_before(
    tape: &mut FlatTape,
    object: RecordIndex,
    before: &str,
) -> Result<(), TsrxParseError> {
    let start = tape
        .field_index(object, "start")
        .ok_or(TsrxParseError::Unsupported("object has no start field"))?;
    let end = tape
        .field_index(object, "end")
        .ok_or(TsrxParseError::Unsupported("object has no end field"))?;
    let before = tape
        .field_index(object, before)
        .ok_or(TsrxParseError::Unsupported("object has no ordering anchor"))?;
    tape.move_field_before(object, start, before)?;
    tape.move_field_before(object, end, before)?;
    Ok(())
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

fn is_jsx_child_type(tape: &FlatTape, object: RecordIndex) -> bool {
    object_type(tape, object).is_some_and(|kind| {
        matches!(
            kind,
            r#""JSXElement""#
                | r#""JSXCodeBlock""#
                | r#""JSXStyleElement""#
                | r#""JSXFragment""#
                | r#""JSXIfExpression""#
                | r#""JSXForExpression""#
                | r#""JSXSwitchExpression""#
                | r#""JSXTryExpression""#
        )
    })
}

fn collect_code_block_plans(
    tape: &FlatTape,
    overlay: OverlayView<'_>,
    segments: &[ProjectionSegment],
    blocks: &[(u32, RecordIndex)],
    jsx_containers: &[(u32, RecordIndex)],
    parents: &ParentIndex,
    prefix: &str,
) -> Result<CodeBlockPlans, TsrxParseError> {
    let count =
        overlay.tokens.iter().filter(|token| token.kind == StructuralKind::FunctionBody).count();
    let mut plans = Vec::with_capacity(count);
    let mut seen = vec![false; tape.object_count()];
    let mut direct_list_policies = vec![DirectListPolicy::None; tape.object_count()];
    for (token_index, token) in overlay
        .tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.kind == StructuralKind::FunctionBody)
    {
        let token_index = u32::try_from(token_index)
            .map_err(|_| TsrxParseError::Unsupported("code-block token index overflow"))?;
        let projected_start = project_authored_start(segments, token.span.end)
            .ok_or(TsrxParseError::Unsupported("code block is outside affine source"))?;
        let block = find_optional_start(blocks, projected_start, "code block")?;
        let container =
            find_optional_start(jsx_containers, projected_start, "JSX-child code block")?;
        let (object, kind) = match (block, container) {
            (Some(object), None) => (object, ProjectedCodeBlockKind::Block),
            (None, Some(object)) => (object, ProjectedCodeBlockKind::JsxContainer),
            (Some(_), Some(_)) => {
                return Err(TsrxParseError::Unsupported("ambiguous projected code block"));
            }
            (None, None) => {
                return Err(TsrxParseError::Unsupported("code block has no projected owner"));
            }
        };
        let body_owner = if kind == ProjectedCodeBlockKind::JsxContainer {
            validate_jsx_child_container(tape, parents, object)?;
            validate_jsx_code_block_wrapper(tape, object, prefix, token_index)?
        } else {
            object
        };
        let index = index_of(object)?;
        let duplicate = seen
            .get_mut(index)
            .ok_or(TsrxParseError::Unsupported("code block owner is outside object table"))?;
        if std::mem::replace(duplicate, true) {
            return Err(TsrxParseError::Unsupported("duplicate projected code block owner"));
        }
        let body_owner_index = index_of(body_owner)?;
        if direct_list_policies[body_owner_index] != DirectListPolicy::None {
            return Err(TsrxParseError::Unsupported("duplicate projected code-block body owner"));
        }
        direct_list_policies[body_owner_index] = DirectListPolicy::CodeBlockBody;
        plans.push(ProjectedCodeBlock {
            object,
            body_owner,
            kind,
            authored_start: token.span.start,
        });
    }
    Ok(CodeBlockPlans { blocks: plans, direct_list_policies })
}

fn mark_direct_custom_clause_blocks(
    direct_list_policies: &mut [DirectListPolicy],
    overlay: OverlayView<'_>,
    segments: &[ProjectionSegment],
    blocks: &[(u32, RecordIndex)],
) -> Result<(), TsrxParseError> {
    for node in
        overlay.nodes.iter().filter(|node| matches!(node.kind, ControlKind::If | ControlKind::Try))
    {
        let mut clause = node.first_clause;
        while clause != tsrx_syntax::NONE_INDEX {
            let current = overlay
                .clauses
                .get(index_of_overlay(clause)?)
                .ok_or(TsrxParseError::Unsupported("custom clause is outside overlay table"))?;
            let projected_start = project_authored_start(segments, current.body.start).ok_or(
                TsrxParseError::Unsupported("custom clause body is outside affine source"),
            )?;
            let block = find_unique_start(blocks, projected_start, "custom clause block")?;
            let marker = direct_list_policies.get_mut(index_of(block)?).ok_or(
                TsrxParseError::Unsupported("custom clause block is outside object table"),
            )?;
            if *marker != DirectListPolicy::None {
                return Err(TsrxParseError::Unsupported(
                    "custom clause has an ambiguous code-block owner policy",
                ));
            }
            *marker = DirectListPolicy::TemplateClause;
            clause = current.next;
        }
    }
    Ok(())
}

fn find_wrapper_call(
    tape: &FlatTape,
    parents: &ParentIndex,
    control: RecordIndex,
    prefix: &str,
    node_index: usize,
    trailing: Option<RecordIndex>,
) -> Result<RecordIndex, TsrxParseError> {
    let mut value = ValueRef::object(control);
    let max_steps = tape.object_count().saturating_add(tape.list_count());
    for _ in 0..max_steps {
        value = parents
            .parent_container(value)
            .ok_or(TsrxParseError::Unsupported("control wrapper chain ended early"))?;
        let Some(object) = value.as_object() else {
            continue;
        };
        if has_type(tape, object, r#""CallExpression""#) {
            validate_wrapper_call(tape, object, control, prefix, node_index, trailing)?;
            return Ok(object);
        }
    }
    Err(TsrxParseError::Unsupported("control wrapper chain is cyclic or missing"))
}

fn validate_wrapper_call(
    tape: &FlatTape,
    call: RecordIndex,
    control: RecordIndex,
    prefix: &str,
    node_index: usize,
    trailing: Option<RecordIndex>,
) -> Result<(), TsrxParseError> {
    let callee = object_field(tape, call, "callee")?;
    require_type(tape, callee, r#""Identifier""#)?;
    if !scaffold_name_matches(scalar_field(tape, callee, "name")?, prefix, 'W', node_index) {
        return Err(TsrxParseError::Unsupported("unknown control wrapper callee"));
    }
    let (manifest, end_marker) = exact_two_values(tape, list_field(tape, call, "arguments")?)?;
    let object = manifest
        .as_object()
        .ok_or(TsrxParseError::Unsupported("wrapper manifest is not an object"))?;
    require_type(tape, object, r#""ObjectExpression""#)?;
    let property = exact_one_value(tape, list_field(tape, object, "properties")?)?
        .as_object()
        .ok_or(TsrxParseError::Unsupported("wrapper manifest property missing"))?;
    require_type(tape, property, r#""Property""#)?;
    let key = object_field(tape, property, "key")?;
    require_type(tape, key, r#""Identifier""#)?;
    if !scaffold_name_matches(scalar_field(tape, key, "name")?, prefix, 'M', node_index) {
        return Err(TsrxParseError::Unsupported("unknown wrapper method key"));
    }
    let function = object_field(tape, property, "value")?;
    require_type(tape, function, r#""FunctionExpression""#)?;
    if scalar_field(tape, function, "generator")? != "true"
        || scalar_field(tape, function, "async")? != "true"
    {
        return Err(TsrxParseError::Unsupported("control wrapper is not an async generator"));
    }
    let function_body = object_field(tape, function, "body")?;
    let body_list = list_field(tape, function_body, "body")?;
    let mut body = tape.values(body_list);
    if body.next() != Some(ValueRef::object(control))
        || trailing.is_some_and(|object| body.next() != Some(ValueRef::object(object)))
        || trailing.is_none() && body.next().is_some()
        || body.next().is_some()
    {
        return Err(TsrxParseError::Unsupported("control wrapper has unexpected statements"));
    }
    let end =
        end_marker.as_object().ok_or(TsrxParseError::Unsupported("wrapper end marker missing"))?;
    require_type(tape, end, r#""Identifier""#)?;
    if !scaffold_name_matches(scalar_field(tape, end, "name")?, prefix, 'E', node_index) {
        return Err(TsrxParseError::Unsupported("unknown wrapper end marker"));
    }
    Ok(())
}

fn scaffold_name_matches(encoded: &str, prefix: &str, marker: char, expected_index: usize) -> bool {
    let Some(name) = encoded.strip_prefix('"').and_then(|value| value.strip_suffix('"')) else {
        return false;
    };
    let Some(suffix) = name.strip_prefix(prefix).and_then(|value| value.strip_prefix(marker))
    else {
        return false;
    };
    suffix.strip_suffix('_').and_then(|value| value.parse::<usize>().ok()) == Some(expected_index)
}

fn reconstruct_code_blocks(
    tape: &mut FlatTape,
    authored: &str,
    segments: &[ProjectionSegment],
    plans: &CodeBlockPlans,
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
    removals: &mut Vec<ListEntryRemoval>,
) -> Result<(), TsrxParseError> {
    for code_block in plans.blocks.iter().rev().copied() {
        match code_block.kind {
            ProjectedCodeBlockKind::Block => reconstruct_block_code_block(
                tape,
                authored,
                segments,
                code_block,
                &plans.direct_list_policies,
                parents,
                starts,
                removals,
            )?,
            ProjectedCodeBlockKind::JsxContainer => {
                reconstruct_jsx_child_code_block(tape, segments, code_block, starts)?;
            }
        }
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
fn reconstruct_block_code_block(
    tape: &mut FlatTape,
    authored: &str,
    segments: &[ProjectionSegment],
    code_block: ProjectedCodeBlock,
    direct_list_policies: &[DirectListPolicy],
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
    removals: &mut Vec<ListEntryRemoval>,
) -> Result<(), TsrxParseError> {
    let block = code_block.object;
    require_type(tape, block, r#""BlockStatement""#)?;
    let placement = block_code_block_placement(tape, parents, block, direct_list_policies)?;
    let projected_end = scalar_u32(tape, block, "end")?;
    let authored_end = map_endpoint(segments, projected_end, false)
        .ok_or(TsrxParseError::Unsupported("code block end is outside affine source"))?;
    let body = list_field(tape, block, "body")?;
    let semicolon_end = prepare_code_block_placement(
        tape,
        authored,
        segments,
        authored_end,
        placement,
        parents,
        removals,
    )?;
    let render = take_code_block_render(tape, body)?;
    replace_type(tape, block, r#""JSXCodeBlock""#)?;
    order_span_fields_before(tape, block, "body")?;
    tape.append_field(block, "render", render)?;
    append_empty_metadata(tape, block)?;
    starts.push(AuthoredStart { object: block, start: code_block.authored_start, end: None });
    if let CodeBlockPlacement::Wrap(slot) = placement {
        let statement = create_expression_statement(tape, block)?;
        ParentIndex::replace(tape, slot, ValueRef::object(statement))?;
        starts.push(AuthoredStart {
            object: statement,
            start: code_block.authored_start,
            end: semicolon_end,
        });
    }
    Ok(())
}

fn reconstruct_jsx_child_code_block(
    tape: &mut FlatTape,
    segments: &[ProjectionSegment],
    code_block: ProjectedCodeBlock,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    let container = code_block.object;
    require_type(tape, container, r#""JSXExpressionContainer""#)?;
    let body = list_field(tape, code_block.body_owner, "body")?;
    let render = take_code_block_render(tape, body)?;
    let projected_end = scalar_u32(tape, container, "end")?;
    let authored_end = map_endpoint(segments, projected_end, false)
        .ok_or(TsrxParseError::Unsupported("JSX-child code block end is outside affine source"))?;
    let span = ByteSpan::new(code_block.authored_start, authored_end);
    tape.clear_fields(container)?;
    append_node_head(tape, container, r#""JSXCodeBlock""#, span)?;
    tape.append_field(container, "body", ValueRef::list(body))?;
    tape.append_field(container, "render", render)?;
    append_empty_metadata(tape, container)?;
    record_authored_span(starts, container, span);
    Ok(())
}

fn validate_jsx_code_block_wrapper(
    tape: &FlatTape,
    container: RecordIndex,
    prefix: &str,
    token: u32,
) -> Result<RecordIndex, TsrxParseError> {
    let grouped = object_field(tape, container, "expression")?;
    let function = unwrap_parenthesized_expression(tape, grouped)?;
    require_type(tape, function, r#""FunctionExpression""#)?;
    if scalar_field(tape, function, "generator")? != "true"
        || scalar_field(tape, function, "async")? != "true"
        || tape.values(list_field(tape, function, "params")?).next().is_some()
    {
        return Err(TsrxParseError::Unsupported(
            "JSX code-block wrapper is not a parameterless async generator",
        ));
    }
    let id = object_field(tape, function, "id")?;
    require_type(tape, id, r#""Identifier""#)?;
    let token = usize::try_from(token)
        .map_err(|_| TsrxParseError::Unsupported("JSX code-block token index overflow"))?;
    if !scaffold_name_matches(scalar_field(tape, id, "name")?, prefix, 'J', token) {
        return Err(TsrxParseError::Unsupported("unknown JSX code-block wrapper identity"));
    }
    let body = object_field(tape, function, "body")?;
    require_type(tape, body, r#""BlockStatement""#)?;
    Ok(body)
}

fn take_code_block_render(
    tape: &mut FlatTape,
    body: RecordIndex,
) -> Result<ValueRef, TsrxParseError> {
    let mut render = None;
    let mut trailing_semicolon = false;
    for value in tape.values(body) {
        if render.is_some() {
            if !trailing_semicolon && is_dynamic_semicolon(tape, value) {
                trailing_semicolon = true;
                continue;
            }
            return Err(TsrxParseError::AuthoredGrammar(
                "render expression precedes another statement".to_string(),
            ));
        }
        render = render_expression(tape, value)?;
    }
    let Some(render) = render else {
        return tape.push_scalar("null").map_err(Into::into);
    };
    if trailing_semicolon {
        tape.pop_list_value(body)?;
    }
    tape.pop_list_value(body)?;
    Ok(render)
}

fn block_code_block_placement(
    tape: &FlatTape,
    parents: &ParentIndex,
    block: RecordIndex,
    direct_list_policies: &[DirectListPolicy],
) -> Result<CodeBlockPlacement, TsrxParseError> {
    let slot = parents
        .parent_slot(ValueRef::object(block))
        .ok_or(TsrxParseError::Unsupported("projected code block has no parent"))?;
    let ParentSlot::Field(_) = slot else {
        let list = parents
            .parent_container(ValueRef::object(block))
            .and_then(ValueRef::as_list)
            .ok_or(TsrxParseError::Unsupported("projected code block has no parent list"))?;
        let owner = parents
            .parent_container(ValueRef::list(list))
            .and_then(ValueRef::as_object)
            .ok_or(TsrxParseError::Unsupported("projected code block list has no owner"))?;
        let policy =
            direct_list_policies.get(index_of(owner)?).copied().unwrap_or(DirectListPolicy::None);
        if policy != DirectListPolicy::None {
            return Ok(CodeBlockPlacement::DirectList { slot, policy });
        }
        if matches!(object_type(tape, owner), Some(r#""BlockStatement""# | r#""SwitchCase""#)) {
            return Ok(CodeBlockPlacement::Wrap(slot));
        }
        return Err(TsrxParseError::AuthoredGrammar(
            "code block is outside an implemented statement-list placement".to_string(),
        ));
    };
    let parent = parents
        .parent_container(ValueRef::object(block))
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("projected code block has no object parent"))?;
    let body_owns_block =
        tape.field_index(parent, "body").and_then(|field| tape.field_value(field))
            == Some(ValueRef::object(block));
    if body_owns_block
        && matches!(
            object_type(tape, parent),
            Some(
                r#""FunctionDeclaration""#
                    | r#""FunctionExpression""#
                    | r#""ArrowFunctionExpression""#
            )
        )
    {
        return Ok(CodeBlockPlacement::DirectField);
    }
    Err(TsrxParseError::AuthoredGrammar(
        "code block is outside an implemented expression placement".to_string(),
    ))
}

fn validate_jsx_child_container(
    tape: &FlatTape,
    parents: &ParentIndex,
    container: RecordIndex,
) -> Result<(), TsrxParseError> {
    let Some(ParentSlot::ListValue(_)) = parents.parent_slot(ValueRef::object(container)) else {
        return Err(TsrxParseError::Unsupported("code block JSX container is not a child"));
    };
    let list = parents
        .parent_container(ValueRef::object(container))
        .and_then(ValueRef::as_list)
        .ok_or(TsrxParseError::Unsupported("code block JSX container has no child list"))?;
    let owner = parents
        .parent_container(ValueRef::list(list))
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("code block JSX child list has no owner"))?;
    if !matches!(object_type(tape, owner), Some(r#""JSXElement""# | r#""JSXFragment""#))
        || tape.field_index(owner, "children").and_then(|field| tape.field_value(field))
            != Some(ValueRef::list(list))
    {
        return Err(TsrxParseError::Unsupported(
            "code block JSX container is outside authored children",
        ));
    }
    Ok(())
}

fn prepare_code_block_placement(
    tape: &FlatTape,
    authored: &str,
    segments: &[ProjectionSegment],
    authored_end: u32,
    placement: CodeBlockPlacement,
    parents: &ParentIndex,
    removals: &mut Vec<ListEntryRemoval>,
) -> Result<Option<u32>, TsrxParseError> {
    let (slot, policy) = match placement {
        CodeBlockPlacement::DirectField
        | CodeBlockPlacement::DirectList { policy: DirectListPolicy::CodeBlockBody, .. } => {
            return Ok(None);
        }
        CodeBlockPlacement::DirectList { slot, policy } => (slot, Some(policy)),
        CodeBlockPlacement::Wrap(slot) => (slot, None),
    };
    let semicolon = code_block_statement_boundary(authored, authored_end)?;
    let ParentSlot::ListValue(block_entry) = slot else {
        return Err(TsrxParseError::Unsupported(
            "code-block statement placement is not a list entry",
        ));
    };
    let mut after = tape
        .list_value_next(block_entry)
        .ok_or(TsrxParseError::Unsupported("code-block list entry is invalid"))?;
    if let Some(span) = semicolon {
        let (removal, next) = validate_semicolon_entry(tape, segments, parents, block_entry, span)?;
        removals.push(removal);
        after = next;
    }
    if policy == Some(DirectListPolicy::TemplateClause) && !after.is_none() {
        return Err(TsrxParseError::Unsupported(
            "direct code-block render precedes another clause statement",
        ));
    }
    Ok((policy.is_none()).then(|| semicolon.map(|span| span.end)).flatten())
}

fn validate_semicolon_entry(
    tape: &FlatTape,
    segments: &[ProjectionSegment],
    parents: &ParentIndex,
    block_entry: RecordIndex,
    authored: ByteSpan,
) -> Result<(ListEntryRemoval, RecordIndex), TsrxParseError> {
    let block = tape
        .list_value(block_entry)
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("code-block list entry is not an object"))?;
    let list = parents
        .parent_container(ValueRef::object(block))
        .and_then(ValueRef::as_list)
        .ok_or(TsrxParseError::Unsupported("code-block has no parent statement list"))?;
    let entry = tape.list_value_next(block_entry).filter(|entry| !entry.is_none()).ok_or(
        TsrxParseError::Unsupported("authored code-block semicolon has no projected statement"),
    )?;
    let statement = tape
        .list_value(entry)
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("authored code-block semicolon is not a statement"))?;
    require_type(tape, statement, r#""EmptyStatement""#)?;
    let projected_start = scalar_u32(tape, statement, "start")?;
    let projected_end = scalar_u32(tape, statement, "end")?;
    if map_endpoint(segments, projected_start, true) != Some(authored.start)
        || map_endpoint(segments, projected_end, false) != Some(authored.end)
    {
        return Err(TsrxParseError::Unsupported(
            "projected code-block semicolon differs from authored source",
        ));
    }
    let next = tape
        .list_value_next(entry)
        .ok_or(TsrxParseError::Unsupported("semicolon list entry is invalid"))?;
    Ok((ListEntryRemoval { list, entry }, next))
}

fn code_block_statement_boundary(
    authored: &str,
    authored_end: u32,
) -> Result<Option<ByteSpan>, TsrxParseError> {
    let mut index = usize::try_from(authored_end)
        .map_err(|_| TsrxParseError::Unsupported("code block end exceeds host usize"))?;
    let bytes = authored.as_bytes();
    let mut line_break = false;
    while let Some(byte) = bytes.get(index).copied() {
        match byte {
            b' ' | b'\t' | 0x0b | 0x0c => index += 1,
            b'\r' | b'\n' => {
                line_break = true;
                index += 1;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while let Some(byte) = bytes.get(index).copied() {
                    if matches!(byte, b'\r' | b'\n') {
                        break;
                    }
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let Some(relative_end) = authored[index + 2..].find("*/") else {
                    return Err(TsrxParseError::Unsupported(
                        "unterminated trivia after code block",
                    ));
                };
                let end = index + 2 + relative_end + 2;
                line_break |=
                    authored[index..end].bytes().any(|byte| matches!(byte, b'\r' | b'\n'));
                index = end;
            }
            _ => break,
        }
    }
    if bytes.get(index) == Some(&b';') {
        let start = u32::try_from(index)
            .map_err(|_| TsrxParseError::Unsupported("semicolon start exceeds u32"))?;
        return Ok(Some(ByteSpan::new(start, start + 1)));
    }
    if index == authored.len() || bytes.get(index) == Some(&b'}') || line_break {
        Ok(None)
    } else {
        Err(TsrxParseError::AuthoredGrammar(
            "code block expression requires an authored statement boundary".to_string(),
        ))
    }
}

/// Applies TSRX's JSX significant-whitespace rule over the serialized child lists in one flat
/// tape pass. Inline whitespace remains observable text; indentation-only text containing a line
/// break is scheduled for the shared validated in-place removal batch.
fn normalize_template_layout_text(
    tape: &mut FlatTape,
    layout_containers: &[RecordIndex],
    removals: &mut Vec<ListEntryRemoval>,
) -> Result<(), TsrxParseError> {
    let mut value_updates = Vec::new();
    for &object in layout_containers {
        let Some(children) = tape
            .field_index(object, "children")
            .and_then(|field| tape.field_value(field))
            .and_then(ValueRef::as_list)
        else {
            continue;
        };
        for (entry, value) in tape.values_indexed(children) {
            let Some(text) = value.as_object().filter(|text| has_type(tape, *text, r#""JSXText""#))
            else {
                continue;
            };
            let value_field = tape
                .field_index(text, "value")
                .ok_or(TsrxParseError::Unsupported("JSXText has no value field"))?;
            let raw = scalar_field(tape, text, "raw")?;
            let normalized = strip_template_block_comments_json(raw)?;
            let value = normalized.as_deref().unwrap_or(scalar_field(tape, text, "value")?);
            if value == r#""""# || is_layout_only_text_json(value) {
                removals.push(ListEntryRemoval { list: children, entry });
            } else if let Some(value) = normalized {
                value_updates.push((value_field, value));
            }
        }
    }
    for (field, encoded) in value_updates {
        let value = tape.push_scalar(&encoded)?;
        tape.set_field_value(field, value)?;
    }
    Ok(())
}

/// Matches `@tsrx/core`'s template raw-text semantics without another source or AST pass. OXC
/// correctly preserves the authored JSX text in `raw`; TSRX additionally treats `/* ... */` as a
/// template comment anywhere in that run, so only `value` drops those ranges.
fn strip_template_block_comments_json(encoded: &str) -> Result<Option<String>, TsrxParseError> {
    let inner = encoded
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or(TsrxParseError::Unsupported("JSXText raw field is not a JSON string"))?;
    let Some(first) = inner.find("/*") else {
        return Ok(None);
    };

    let mut output = String::with_capacity(encoded.len());
    output.push('"');
    output.push_str(&inner[..first]);
    let mut cursor = first;
    loop {
        let content_start = cursor + 2;
        let Some(close_offset) = inner[content_start..].find("*/") else {
            cursor = inner.len();
            break;
        };
        cursor = content_start + close_offset + 2;
        let Some(next_offset) = inner[cursor..].find("/*") else {
            break;
        };
        output.push_str(&inner[cursor..cursor + next_offset]);
        cursor += next_offset;
    }
    output.push_str(&inner[cursor..]);
    output.push('"');
    Ok(Some(output))
}

/// Classifies one JSON string scalar without allocating it. TSRX drops indentation-only text and
/// template line comments when the span contains CR/LF layout; a plain inline space remains a
/// real child.
fn is_layout_only_text_json(encoded: &str) -> bool {
    let Some(inner) = encoded.strip_prefix('"').and_then(|value| value.strip_suffix('"')) else {
        return false;
    };
    let mut chars = inner.chars();
    let mut has_newline = false;
    loop {
        let decoded = match next_json_string_character(&mut chars) {
            Ok(Some(character)) => character,
            Ok(None) => return has_newline,
            Err(()) => return false,
        };
        if decoded.is_whitespace() {
            has_newline |= matches!(decoded, '\n' | '\r');
            continue;
        }
        if decoded != '/' || !matches!(next_json_string_character(&mut chars), Ok(Some('/'))) {
            return false;
        }
        loop {
            match next_json_string_character(&mut chars) {
                Ok(Some('\n' | '\r')) => {
                    has_newline = true;
                    break;
                }
                Ok(Some(_)) => {}
                Ok(None) => return has_newline,
                Err(()) => return false,
            }
        }
    }
}

fn next_json_string_character(chars: &mut std::str::Chars<'_>) -> Result<Option<char>, ()> {
    let Some(character) = chars.next() else {
        return Ok(None);
    };
    if character != '\\' {
        return Ok(Some(character));
    }
    let decoded = match chars.next() {
        Some('"') => '"',
        Some('\\') => '\\',
        Some('/') => '/',
        Some('b') => '\u{0008}',
        Some('f') => '\u{000c}',
        Some('n') => '\n',
        Some('r') => '\r',
        Some('t') => '\t',
        Some('u') => {
            let mut value = 0_u32;
            for _ in 0..4 {
                let Some(digit) = chars.next().and_then(|digit| digit.to_digit(16)) else {
                    return Err(());
                };
                value = (value << 4) | digit;
            }
            char::from_u32(value).ok_or(())?
        }
        _ => return Err(()),
    };
    Ok(Some(decoded))
}

fn render_expression(
    tape: &FlatTape,
    statement: ValueRef,
) -> Result<Option<ValueRef>, TsrxParseError> {
    let Some(statement) = statement.as_object() else {
        return Ok(None);
    };
    if is_jsx_child_type(tape, statement) {
        return Ok(Some(ValueRef::object(statement)));
    }
    if !has_type(tape, statement, r#""ExpressionStatement""#) {
        return Ok(None);
    }
    let expression = field_value(tape, statement, "expression")?;
    let Some(object) = expression.as_object() else {
        return Ok(None);
    };
    Ok(is_jsx_child_type(tape, object).then_some(expression))
}

fn is_dynamic_semicolon(tape: &FlatTape, statement: ValueRef) -> bool {
    let Some(statement) = statement.as_object() else {
        return false;
    };
    if has_type(tape, statement, r#""EmptyStatement""#) {
        return true;
    }
    if !has_type(tape, statement, r#""ExpressionStatement""#) {
        return false;
    }
    let Some(text) = tape
        .field_index(statement, "expression")
        .and_then(|field| tape.field_value(field))
        .and_then(ValueRef::as_object)
        .filter(|text| has_type(tape, *text, r#""JSXText""#))
    else {
        return false;
    };
    scalar_field(tape, text, "value") == Ok(r#"";""#)
        && scalar_field(tape, text, "raw") == Ok(r#"";""#)
}

struct ProjectedObjectIndex {
    if_objects: Vec<(u32, RecordIndex)>,
    loop_objects: Vec<(u32, RecordIndex)>,
    switch_objects: Vec<(u32, RecordIndex)>,
    block_objects: Vec<(u32, RecordIndex)>,
    jsx_containers: Vec<(u32, RecordIndex)>,
    layout_containers: Vec<RecordIndex>,
    call_objects: Vec<RecordIndex>,
    module_objects: Vec<RecordIndex>,
}

impl ProjectedObjectIndex {
    fn new() -> Self {
        Self {
            if_objects: Vec::new(),
            loop_objects: Vec::new(),
            switch_objects: Vec::new(),
            block_objects: Vec::new(),
            jsx_containers: Vec::new(),
            layout_containers: Vec::new(),
            call_objects: Vec::new(),
            module_objects: Vec::new(),
        }
    }

    fn record(
        &mut self,
        object: RecordIndex,
        kind: Option<&str>,
        start: Option<u32>,
    ) -> Result<(), TsrxParseError> {
        if matches!(kind, Some(r#""JSXElement""# | r#""JSXFragment""#)) {
            self.layout_containers.push(object);
        }
        let target = match kind {
            Some(r#""IfStatement""#) => Some(&mut self.if_objects),
            Some(r#""ForStatement""# | r#""ForInStatement""# | r#""ForOfStatement""#) => {
                Some(&mut self.loop_objects)
            }
            Some(r#""SwitchStatement""#) => Some(&mut self.switch_objects),
            Some(r#""BlockStatement""#) => Some(&mut self.block_objects),
            Some(r#""JSXExpressionContainer""#) => Some(&mut self.jsx_containers),
            Some(r#""CallExpression""#) => {
                self.call_objects.push(object);
                None
            }
            _ => None,
        };
        if kind.is_some_and(is_module_declaration_type) {
            self.module_objects.push(object);
        }
        if let Some(target) = target {
            let start =
                start.ok_or(TsrxParseError::Unsupported("required ESTree field is not scalar"))?;
            target.push((start, object));
        }
        Ok(())
    }

    fn sort(&mut self) {
        for objects in [
            &mut self.if_objects,
            &mut self.loop_objects,
            &mut self.switch_objects,
            &mut self.block_objects,
            &mut self.jsx_containers,
        ] {
            objects.sort_unstable_by_key(|(start, _)| *start);
        }
    }
}

fn collect_try_helpers(
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

fn find_unique_start(
    objects: &[(u32, RecordIndex)],
    start: u32,
    shape: &'static str,
) -> Result<RecordIndex, TsrxParseError> {
    let first = objects.partition_point(|(candidate, _)| *candidate < start);
    let last = objects.partition_point(|(candidate, _)| *candidate <= start);
    if last - first != 1 {
        return Err(TsrxParseError::Unsupported(shape));
    }
    Ok(objects[first].1)
}

fn find_optional_start(
    objects: &[(u32, RecordIndex)],
    start: u32,
    shape: &'static str,
) -> Result<Option<RecordIndex>, TsrxParseError> {
    let first = objects.partition_point(|(candidate, _)| *candidate < start);
    let last = objects.partition_point(|(candidate, _)| *candidate <= start);
    match last - first {
        0 => Ok(None),
        1 => Ok(Some(objects[first].1)),
        _ => Err(TsrxParseError::Unsupported(shape)),
    }
}

fn index_of_overlay(index: u32) -> Result<usize, TsrxParseError> {
    usize::try_from(index)
        .map_err(|_| TsrxParseError::Unsupported("overlay index exceeds host usize"))
}

fn place_control(
    tape: &mut FlatTape,
    parents: &ParentIndex,
    control: RecordIndex,
    context: ControlContext,
    wrapper: Option<RecordIndex>,
    authored: ByteSpan,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    let value = ValueRef::object(control);
    match context {
        ControlContext::Statement => {
            if wrapper.is_some() {
                return Err(TsrxParseError::Unsupported(
                    "statement control unexpectedly has a wrapper",
                ));
            }
            let slot = parents
                .parent_slot(value)
                .ok_or(TsrxParseError::Unsupported("statement control has no parent slot"))?;
            if !matches!(slot, ParentSlot::ListValue(_)) {
                return Err(TsrxParseError::Unsupported(
                    "statement control is not in a statement list",
                ));
            }
            let statement = create_expression_statement(tape, control)?;
            ParentIndex::replace(tape, slot, ValueRef::object(statement))?;
            starts.push(AuthoredStart { object: statement, start: authored.start, end: None });
        }
        ControlContext::Expression => {
            let wrapper =
                wrapper.ok_or(TsrxParseError::Unsupported("expression control has no wrapper"))?;
            let slot = parents
                .parent_slot(ValueRef::object(wrapper))
                .ok_or(TsrxParseError::Unsupported("expression wrapper has no parent"))?;
            record_labeled_control_statement(tape, parents, wrapper, authored, starts)?;
            ParentIndex::replace(tape, slot, value)?;
        }
        ControlContext::JsxChild => {
            let wrapper =
                wrapper.ok_or(TsrxParseError::Unsupported("JSX-child control has no wrapper"))?;
            let container = parents
                .parent_container(ValueRef::object(wrapper))
                .and_then(ValueRef::as_object)
                .ok_or(TsrxParseError::Unsupported(
                    "JSX-child wrapper has no expression container",
                ))?;
            require_type(tape, container, r#""JSXExpressionContainer""#)?;
            if field_value(tape, container, "expression")? != ValueRef::object(wrapper) {
                return Err(TsrxParseError::Unsupported(
                    "wrapper is not the JSX container expression",
                ));
            }
            let slot = parents
                .parent_slot(ValueRef::object(container))
                .ok_or(TsrxParseError::Unsupported("JSX expression container has no child slot"))?;
            if !matches!(slot, ParentSlot::ListValue(_)) {
                return Err(TsrxParseError::Unsupported("JSX expression container is not a child"));
            }
            ParentIndex::replace(tape, slot, value)?;
        }
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
fn place_try_control(
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

fn record_labeled_control_statement(
    tape: &FlatTape,
    parents: &ParentIndex,
    wrapper: RecordIndex,
    authored: ByteSpan,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    let Some(statement) = parents
        .parent_container(ValueRef::object(wrapper))
        .and_then(ValueRef::as_object)
        .filter(|statement| has_type(tape, *statement, r#""ExpressionStatement""#))
    else {
        return Ok(());
    };
    if field_value(tape, statement, "expression")? != ValueRef::object(wrapper) {
        return Err(TsrxParseError::Unsupported(
            "control wrapper statement has an unexpected expression",
        ));
    }
    let Some(label) = parents
        .parent_container(ValueRef::object(statement))
        .and_then(ValueRef::as_object)
        .filter(|label| has_type(tape, *label, r#""LabeledStatement""#))
    else {
        return Ok(());
    };
    if field_value(tape, label, "body")? != ValueRef::object(statement) {
        return Err(TsrxParseError::Unsupported("labeled control wrapper is not the label body"));
    }
    starts.push(AuthoredStart {
        object: statement,
        start: authored.start,
        end: Some(authored.end),
    });
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

fn create_expression_statement(
    tape: &mut FlatTape,
    expression: RecordIndex,
) -> Result<RecordIndex, TsrxParseError> {
    let start = field_value(tape, expression, "start")?;
    let end = field_value(tape, expression, "end")?;
    let statement = tape.push_object_record(ObjectRecord::default())?;
    let kind = tape.push_scalar(r#""ExpressionStatement""#)?;
    tape.append_field(statement, "type", kind)?;
    tape.append_field(statement, "start", start)?;
    tape.append_field(statement, "end", end)?;
    if let Some(range) =
        tape.field_index(expression, "range").and_then(|field| tape.field_value(field))
    {
        tape.append_field(statement, "range", range)?;
    }
    tape.append_field(statement, "expression", ValueRef::object(expression))?;
    Ok(statement)
}

fn append_empty_metadata(tape: &mut FlatTape, object: RecordIndex) -> Result<(), TsrxParseError> {
    if let Some(field) = tape.field_index(object, "metadata") {
        let metadata = tape
            .field_value(field)
            .and_then(ValueRef::as_object)
            .ok_or(TsrxParseError::Unsupported("metadata is not an object"))?;
        let path = list_field(tape, metadata, "path")?;
        if tape.values(path).next().is_some() {
            return Err(TsrxParseError::Unsupported("metadata path is not empty"));
        }
        return Ok(());
    }
    let path = tape.push_list_record(ListRecord::default())?;
    let metadata = tape.push_object_record(ObjectRecord::default())?;
    tape.append_field(metadata, "path", ValueRef::list(path))?;
    tape.append_field(object, "metadata", ValueRef::object(metadata))?;
    Ok(())
}

pub(super) fn finalize_reachable_spans(
    tape: &mut FlatTape,
    segments: &[ProjectionSegment],
    authored_positions: &[AuthoredStart],
    finalization_index: &FinalizationIndex,
) -> Result<(), TsrxParseError> {
    let mut overrides = vec![None; tape.object_count()];
    for &position in authored_positions {
        let slot = overrides
            .get_mut(index_of(position.object)?)
            .ok_or(TsrxParseError::Unsupported("authored span override outside object table"))?;
        if slot.replace(position).is_some() {
            return Err(TsrxParseError::Unsupported("duplicate authored span override"));
        }
    }
    for (index, mut span_fields) in finalization_index.reachable_span_fields() {
        let authored = overrides[index].take();
        if authored.is_some() && span_fields.start.is_none() {
            let raw = u32::try_from(index).map_err(|_| {
                TsrxParseError::ResourceExhausted("object index exceeds the 32-bit tape limit")
            })?;
            span_fields = object_span_fields(tape, RecordIndex::new(raw));
        }
        finalize_object_span(tape, span_fields, segments, authored)?;
    }
    Ok(())
}

fn finalize_object_span(
    tape: &mut FlatTape,
    fields: SpanFields,
    segments: &[ProjectionSegment],
    authored: Option<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    let start = finalize_span_endpoint(
        tape,
        fields.start,
        segments,
        true,
        authored.map(|position| position.start),
    )?;
    let end = finalize_span_endpoint(
        tape,
        fields.end,
        segments,
        false,
        authored.and_then(|position| position.end),
    )?;
    if let Some(range_field) = fields.range {
        sync_range_values(
            tape,
            range_field,
            start.ok_or(TsrxParseError::Unsupported("ESTree range has no start field"))?,
            end.ok_or(TsrxParseError::Unsupported("ESTree range has no end field"))?,
        )?;
    }
    Ok(())
}

fn finalize_span_endpoint(
    tape: &mut FlatTape,
    field: Option<RecordIndex>,
    segments: &[ProjectionSegment],
    is_start: bool,
    authored: Option<u32>,
) -> Result<Option<ValueRef>, TsrxParseError> {
    let Some(field) = field else {
        if authored.is_some() {
            return Err(TsrxParseError::Unsupported(if is_start {
                "authored node has no start"
            } else {
                "authored node has no end"
            }));
        }
        return Ok(None);
    };
    let authored = if let Some(authored) = authored {
        authored
    } else {
        let projected = tape
            .field_value(field)
            .and_then(|value| tape.scalar_u32(value))
            .ok_or(TsrxParseError::Unsupported("non-numeric ESTree span"))?;
        map_reachable_endpoint(segments, projected, is_start)
            .ok_or(TsrxParseError::Unsupported("reachable synthetic ESTree span"))?
    };
    let value = tape.push_u32_scalar(authored)?;
    tape.set_field_value(field, value)?;
    Ok(Some(value))
}

fn object_span_fields(tape: &FlatTape, object: RecordIndex) -> SpanFields {
    let mut span_fields = SpanFields::default();
    for (field_index, field) in tape.fields_indexed(object) {
        match tape.key(field) {
            "start" => span_fields.start = Some(field_index),
            "end" => span_fields.end = Some(field_index),
            "range" => span_fields.range = Some(field_index),
            _ => {}
        }
    }
    span_fields
}

fn sync_range_values(
    tape: &mut FlatTape,
    range_field: RecordIndex,
    start: ValueRef,
    end: ValueRef,
) -> Result<(), TsrxParseError> {
    let range = tape
        .field_value(range_field)
        .and_then(ValueRef::as_list)
        .ok_or(TsrxParseError::Unsupported("ESTree range is not a list"))?;
    let (start_entry, end_entry) = {
        let mut entries = tape.values_indexed(range);
        let start_entry = entries
            .next()
            .map(|(entry, _)| entry)
            .ok_or(TsrxParseError::Unsupported("ESTree range has no start"))?;
        let end_entry = entries
            .next()
            .map(|(entry, _)| entry)
            .ok_or(TsrxParseError::Unsupported("ESTree range has no end"))?;
        if entries.next().is_some() {
            return Err(TsrxParseError::Unsupported("ESTree range has more than two entries"));
        }
        (start_entry, end_entry)
    };
    tape.set_list_value(start_entry, start)?;
    tape.set_list_value(end_entry, end)?;
    Ok(())
}

fn map_reachable_endpoint(
    segments: &[ProjectionSegment],
    projected: u32,
    is_start: bool,
) -> Option<u32> {
    map_endpoint(segments, projected, is_start).or_else(|| {
        (!is_start).then_some(())?;
        let index = segments.partition_point(|segment| segment.projected.start < projected);
        segments
            .get(index)
            .filter(|segment| segment.projected.start == projected)
            .map(|segment| segment.original_start)
    })
}

fn replace_type(
    tape: &mut FlatTape,
    object: RecordIndex,
    kind: &str,
) -> Result<(), TsrxParseError> {
    let field =
        tape.field_index(object, "type").ok_or(TsrxParseError::Unsupported("node has no type"))?;
    let kind = tape.push_scalar(kind)?;
    tape.set_field_value(field, kind)?;
    Ok(())
}

fn exact_one_value(tape: &FlatTape, list: RecordIndex) -> Result<ValueRef, TsrxParseError> {
    let mut values = tape.values(list);
    let value = values
        .next()
        .ok_or(TsrxParseError::Unsupported("scaffold list has an unexpected length"))?;
    if values.next().is_some() {
        return Err(TsrxParseError::Unsupported("scaffold list has an unexpected length"));
    }
    Ok(value)
}

fn exact_two_values(
    tape: &FlatTape,
    list: RecordIndex,
) -> Result<(ValueRef, ValueRef), TsrxParseError> {
    let mut values = tape.values(list);
    let first = values
        .next()
        .ok_or(TsrxParseError::Unsupported("scaffold list has an unexpected length"))?;
    let second = values
        .next()
        .ok_or(TsrxParseError::Unsupported("scaffold list has an unexpected length"))?;
    if values.next().is_some() {
        return Err(TsrxParseError::Unsupported("scaffold list has an unexpected length"));
    }
    Ok((first, second))
}

fn field_value(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<ValueRef, TsrxParseError> {
    tape.field_index(object, name)
        .and_then(|field| tape.field_value(field))
        .ok_or(TsrxParseError::Unsupported("missing required ESTree field"))
}

fn scalar_field<'a>(
    tape: &'a FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<&'a str, TsrxParseError> {
    tape.scalar(field_value(tape, object, name)?)
        .ok_or(TsrxParseError::Unsupported("required ESTree field is not scalar"))
}

fn scalar_u32(tape: &FlatTape, object: RecordIndex, name: &str) -> Result<u32, TsrxParseError> {
    tape.scalar_u32(field_value(tape, object, name)?)
        .ok_or(TsrxParseError::Unsupported("required ESTree field is not u32"))
}

fn object_field(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<RecordIndex, TsrxParseError> {
    field_value(tape, object, name)?
        .as_object()
        .ok_or(TsrxParseError::Unsupported("required ESTree field is not an object"))
}

fn list_field(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<RecordIndex, TsrxParseError> {
    field_value(tape, object, name)?
        .as_list()
        .ok_or(TsrxParseError::Unsupported("required ESTree field is not a list"))
}

fn object_type(tape: &FlatTape, object: RecordIndex) -> Option<&str> {
    tape.field_index(object, "type")
        .and_then(|field| tape.field_value(field))
        .and_then(|value| tape.scalar(value))
}

fn has_type(tape: &FlatTape, object: RecordIndex, expected: &str) -> bool {
    object_type(tape, object) == Some(expected)
}

fn require_type(
    tape: &FlatTape,
    object: RecordIndex,
    expected: &'static str,
) -> Result<(), TsrxParseError> {
    if has_type(tape, object, expected) {
        Ok(())
    } else {
        Err(TsrxParseError::Unsupported("unexpected ESTree node type"))
    }
}

fn index_of(index: RecordIndex) -> Result<usize, TsrxParseError> {
    let raw = index.get().ok_or(TsrxParseError::Unsupported("missing tape index"))?;
    usize::try_from(raw).map_err(|_| TsrxParseError::Unsupported("tape index exceeds host usize"))
}
