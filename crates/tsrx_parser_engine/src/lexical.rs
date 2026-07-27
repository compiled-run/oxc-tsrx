use std::collections::HashMap;

use tsrx_tape_schema::{FlatTape, RecordIndex, ValueKind, ValueRef};

use crate::TsrxParseError;

#[derive(Clone, Copy, PartialEq, Eq)]
enum EdgeRole {
    Normal,
    FunctionBody,
    NameOnly,
    SuperCall,
    SuperProperty,
    MethodFunction { super_call: bool },
    ObjectMethodFunction,
}

// A compact copyable traversal state is faster and clearer here than repeatedly
// materializing nested state-machine wrappers for independent lexical capabilities.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy)]
struct Context {
    validate: bool,
    async_ok: bool,
    generator_ok: bool,
    top_level_await: bool,
    new_target_ok: bool,
    super_property_ok: bool,
    super_call_ok: bool,
    break_depth: u32,
    continue_depth: u32,
    flow_scope: u32,
    class_outer_id: u32,
    return_ok: bool,
    class_derived: bool,
}

impl Context {
    const fn program() -> Self {
        Self {
            validate: false,
            async_ok: false,
            generator_ok: false,
            top_level_await: true,
            new_target_ok: false,
            super_property_ok: false,
            super_call_ok: false,
            break_depth: 0,
            continue_depth: 0,
            flow_scope: 0,
            class_outer_id: 0,
            return_ok: false,
            class_derived: false,
        }
    }

    const fn activated(mut self) -> Self {
        self.validate = true;
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    Program,
    Function,
    ArrowFunction,
    Class,
    StaticBlock,
    MethodDefinition,
    PropertyDefinition,
    Property,
    Loop { for_of: bool },
    SwitchStatement,
    LabeledStatement,
    BreakStatement { is_continue: bool },
    ReturnStatement,
    AwaitExpression,
    YieldExpression,
    Super,
    CallExpression,
    MemberExpression,
    MetaProperty,
    Identifier,
    VariableDeclaration,
    JsxCodeBlock,
    JsxIfExpression,
    JsxForExpression,
    JsxSwitchExpression,
    JsxTryExpression,
    Other,
}

#[derive(Clone, Copy)]
enum Work<'tape> {
    Value(ValueRef, Context, EdgeRole),
    ExitObject(usize),
    ExitList(usize),
    ExitLabel(LabelKey<'tape>),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct LabelKey<'tape> {
    scope: u32,
    name: &'tape str,
}

struct Validator<'tape> {
    tape: &'tape FlatTape,
    work: Vec<Work<'tape>>,
    object_states: Vec<u8>,
    span_fields: Vec<SpanFields>,
    list_states: Vec<u8>,
    labels: HashMap<LabelKey<'tape>, bool>,
    class_outers: Vec<Context>,
    next_scope: u32,
    promote_module: bool,
}

pub(super) struct FinalizationIndex {
    object_states: Vec<u8>,
    span_fields: Vec<SpanFields>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct SpanFields {
    pub(super) start: Option<RecordIndex>,
    pub(super) end: Option<RecordIndex>,
    pub(super) range: Option<RecordIndex>,
}

impl SpanFields {
    fn record(&mut self, name: &str, field: RecordIndex) {
        match name {
            "start" => self.start = Some(field),
            "end" => self.end = Some(field),
            "range" => self.range = Some(field),
            _ => {}
        }
    }
}

impl FinalizationIndex {
    pub(super) fn reachable_span_fields(&self) -> impl Iterator<Item = (usize, SpanFields)> + '_ {
        self.object_states
            .iter()
            .zip(&self.span_fields)
            .enumerate()
            .filter_map(|(index, (&state, &fields))| (state != 0).then_some((index, fields)))
    }
}

pub(super) fn validate_authored_contexts(
    tape: &mut FlatTape,
) -> Result<FinalizationIndex, TsrxParseError> {
    let (promote, index) = Validator::new(tape).run()?;
    if promote {
        let program = tape
            .root()
            .as_object()
            .ok_or(TsrxParseError::Unsupported("lexical validation root is not a Program"))?;
        let field = tape
            .field_index(program, "sourceType")
            .ok_or(TsrxParseError::Unsupported("Program has no sourceType"))?;
        let current = tape
            .field_value(field)
            .ok_or(TsrxParseError::Unsupported("Program sourceType is invalid"))?;
        if tape.scalar(current) != Some(r#""module""#) {
            let module = tape.push_scalar(r#""module""#)?;
            tape.set_field_value(field, module)?;
        }
    }
    Ok(index)
}

impl<'tape> Validator<'tape> {
    fn new(tape: &'tape FlatTape) -> Self {
        Self {
            tape,
            work: Vec::with_capacity(tape.object_count().saturating_add(tape.list_count())),
            object_states: vec![0; tape.object_count()],
            span_fields: vec![SpanFields::default(); tape.object_count()],
            list_states: vec![0; tape.list_count()],
            labels: HashMap::new(),
            class_outers: Vec::new(),
            next_scope: 1,
            promote_module: false,
        }
    }

    fn run(mut self) -> Result<(bool, FinalizationIndex), TsrxParseError> {
        self.push(self.tape.root(), Context::program(), EdgeRole::Normal);
        while let Some(work) = self.work.pop() {
            match work {
                Work::Value(value, context, role) => self.visit_value(value, context, role)?,
                Work::ExitObject(index) => self.object_states[index] = 2,
                Work::ExitList(index) => self.list_states[index] = 2,
                Work::ExitLabel(key) => self.exit_label(key)?,
            }
        }
        Ok((
            self.promote_module,
            FinalizationIndex { object_states: self.object_states, span_fields: self.span_fields },
        ))
    }

    fn visit_value(
        &mut self,
        value: ValueRef,
        context: Context,
        role: EdgeRole,
    ) -> Result<(), TsrxParseError> {
        match value.kind() {
            ValueKind::Missing | ValueKind::Scalar => Ok(()),
            ValueKind::Object => {
                let object = value.as_object().ok_or(TsrxParseError::Unsupported(
                    "invalid object reference during lexical validation",
                ))?;
                let index = self.enter_object(object)?;
                self.work.push(Work::ExitObject(index));
                self.visit_object(object, context, role)
            }
            ValueKind::List => {
                let list = value.as_list().ok_or(TsrxParseError::Unsupported(
                    "invalid list reference during lexical validation",
                ))?;
                let index = self.enter_list(list)?;
                self.work.push(Work::ExitList(index));
                for child in self.tape.values(list) {
                    self.push(child, context, EdgeRole::Normal);
                }
                Ok(())
            }
        }
    }

    fn visit_object(
        &mut self,
        object: RecordIndex,
        context: Context,
        role: EdgeRole,
    ) -> Result<(), TsrxParseError> {
        let (kind, fields, generic_visited) = self.classify_object(object, context);
        self.span_fields[index_of(object)?] = fields;
        match kind {
            NodeKind::Program | NodeKind::Other => {
                if !generic_visited {
                    self.visit_generic(object, context);
                }
                Ok(())
            }
            NodeKind::Function => self.visit_function(object, context, role),
            NodeKind::ArrowFunction => self.visit_arrow(object, context),
            NodeKind::Class => self.visit_class(object, context),
            NodeKind::StaticBlock => self.visit_static_block(object, context),
            NodeKind::MethodDefinition => self.visit_method(object, context),
            NodeKind::PropertyDefinition => self.visit_property_definition(object, context),
            NodeKind::Property => {
                self.visit_property(object, context);
                Ok(())
            }
            NodeKind::Loop { for_of } => self.visit_loop(object, context, for_of),
            NodeKind::SwitchStatement => self.visit_switch(object, context),
            NodeKind::LabeledStatement => self.visit_label_chain(object, context),
            NodeKind::BreakStatement { is_continue } => {
                self.visit_break(object, context, is_continue)
            }
            NodeKind::ReturnStatement => self.visit_return(object, context),
            NodeKind::AwaitExpression => self.visit_await(object, context),
            NodeKind::YieldExpression => self.visit_yield(object, context),
            NodeKind::Super => Self::visit_super(context, role),
            NodeKind::CallExpression => {
                self.visit_call(object, context);
                Ok(())
            }
            NodeKind::MemberExpression => {
                self.visit_member(object, context);
                Ok(())
            }
            NodeKind::MetaProperty => self.visit_meta(object, context),
            NodeKind::Identifier => self.visit_identifier(object, context, role),
            NodeKind::VariableDeclaration => self.visit_variable(object, context),
            NodeKind::JsxCodeBlock => {
                self.visit_code_block(object, context, role);
                Ok(())
            }
            NodeKind::JsxIfExpression => {
                self.visit_jsx_if(object, context);
                Ok(())
            }
            NodeKind::JsxForExpression => self.visit_jsx_for(object, context),
            NodeKind::JsxSwitchExpression => self.visit_jsx_switch(object, context),
            NodeKind::JsxTryExpression => {
                self.visit_jsx_try(object, context);
                Ok(())
            }
        }
    }

    fn visit_function(
        &mut self,
        object: RecordIndex,
        outer: Context,
        role: EdgeRole,
    ) -> Result<(), TsrxParseError> {
        let (super_property_ok, super_call_ok) = match role {
            EdgeRole::MethodFunction { super_call } => (true, super_call),
            EdgeRole::ObjectMethodFunction => (true, false),
            _ => (false, false),
        };
        let function = Context {
            validate: false,
            async_ok: scalar_field_is(self.tape, object, "async", "true"),
            generator_ok: scalar_field_is(self.tape, object, "generator", "true"),
            top_level_await: false,
            new_target_ok: true,
            super_property_ok,
            super_call_ok,
            break_depth: 0,
            continue_depth: 0,
            flow_scope: self.fresh_scope()?,
            class_outer_id: 0,
            return_ok: true,
            class_derived: false,
        };
        for field in self.tape.fields(object) {
            let (child, child_role) = match self.tape.key(field) {
                "body" => (function, EdgeRole::FunctionBody),
                "id" | "decorators" => (outer, EdgeRole::Normal),
                _ => (function, EdgeRole::Normal),
            };
            self.push(field.value, child, child_role);
        }
        Ok(())
    }

    fn visit_arrow(&mut self, object: RecordIndex, outer: Context) -> Result<(), TsrxParseError> {
        let arrow = Context {
            validate: false,
            async_ok: scalar_field_is(self.tape, object, "async", "true"),
            generator_ok: false,
            top_level_await: false,
            new_target_ok: outer.new_target_ok,
            super_property_ok: outer.super_property_ok,
            super_call_ok: outer.super_call_ok,
            break_depth: 0,
            continue_depth: 0,
            flow_scope: self.fresh_scope()?,
            class_outer_id: 0,
            return_ok: true,
            class_derived: false,
        };
        for field in self.tape.fields(object) {
            let role = if self.tape.key(field) == "body" {
                EdgeRole::FunctionBody
            } else {
                EdgeRole::Normal
            };
            self.push(field.value, arrow, role);
        }
        Ok(())
    }

    fn visit_class(&mut self, object: RecordIndex, outer: Context) -> Result<(), TsrxParseError> {
        let super_class = field_value(self.tape, object, "superClass")?;
        let class_outer_id = self.register_class_outer(outer)?;
        let flow_scope = self.fresh_scope()?;
        let class = Context {
            validate: false,
            async_ok: false,
            generator_ok: false,
            top_level_await: false,
            new_target_ok: true,
            super_property_ok: true,
            super_call_ok: false,
            break_depth: 0,
            continue_depth: 0,
            flow_scope,
            class_outer_id,
            return_ok: false,
            class_derived: !is_null(self.tape, super_class),
        };
        for field in self.tape.fields(object) {
            let child = if self.tape.key(field) == "body" {
                self.validate_class_body(field.value)?;
                class
            } else {
                outer
            };
            self.push(field.value, child, EdgeRole::Normal);
        }
        Ok(())
    }

    fn visit_static_block(
        &mut self,
        object: RecordIndex,
        class: Context,
    ) -> Result<(), TsrxParseError> {
        let block = Context {
            validate: false,
            async_ok: false,
            generator_ok: false,
            top_level_await: false,
            break_depth: 0,
            continue_depth: 0,
            flow_scope: self.fresh_scope()?,
            return_ok: false,
            ..class
        };
        self.visit_generic(object, block);
        Ok(())
    }

    fn visit_method(&mut self, object: RecordIndex, class: Context) -> Result<(), TsrxParseError> {
        let outer = self.class_outer(class)?;
        let computed = scalar_field_is(self.tape, object, "computed", "true");
        let super_call = class.class_derived
            && scalar_field(self.tape, object, "kind") == Some(r#""constructor""#);
        for field in self.tape.fields(object) {
            let (context, role) = match self.tape.key(field) {
                "key" if !computed => (outer, EdgeRole::NameOnly),
                "key" | "decorators" => (outer, EdgeRole::Normal),
                "value" => (class, EdgeRole::MethodFunction { super_call }),
                _ => (class, EdgeRole::Normal),
            };
            self.push(field.value, context, role);
        }
        Ok(())
    }

    fn visit_property_definition(
        &mut self,
        object: RecordIndex,
        class: Context,
    ) -> Result<(), TsrxParseError> {
        let outer = self.class_outer(class)?;
        let computed = scalar_field_is(self.tape, object, "computed", "true");
        for field in self.tape.fields(object) {
            let (context, role) = match self.tape.key(field) {
                "key" if !computed => (outer, EdgeRole::NameOnly),
                "key" | "decorators" => (outer, EdgeRole::Normal),
                _ => (class, EdgeRole::Normal),
            };
            self.push(field.value, context, role);
        }
        Ok(())
    }

    fn visit_property(&mut self, object: RecordIndex, context: Context) {
        let computed = scalar_field_is(self.tape, object, "computed", "true");
        let method = scalar_field_is(self.tape, object, "method", "true")
            || matches!(scalar_field(self.tape, object, "kind"), Some(r#""get""# | r#""set""#));
        for field in self.tape.fields(object) {
            let role = match self.tape.key(field) {
                "key" if !computed => EdgeRole::NameOnly,
                "value" if method => EdgeRole::ObjectMethodFunction,
                _ => EdgeRole::Normal,
            };
            self.push(field.value, context, role);
        }
    }

    fn visit_loop(
        &mut self,
        object: RecordIndex,
        context: Context,
        for_of: bool,
    ) -> Result<(), TsrxParseError> {
        if for_of && context.validate && scalar_field_is(self.tape, object, "await", "true") {
            self.require_await(context)?;
        }
        let mut body = context;
        body.break_depth = increment(body.break_depth)?;
        body.continue_depth = increment(body.continue_depth)?;
        if body.validate {
            body.return_ok = true;
        }
        for field in self.tape.fields(object) {
            let child = if self.tape.key(field) == "body" { body } else { context };
            self.push(field.value, child, EdgeRole::Normal);
        }
        Ok(())
    }

    fn visit_switch(
        &mut self,
        object: RecordIndex,
        context: Context,
    ) -> Result<(), TsrxParseError> {
        let mut cases = context;
        cases.break_depth = increment(cases.break_depth)?;
        for field in self.tape.fields(object) {
            let child = if self.tape.key(field) == "cases" { cases } else { context };
            self.push(field.value, child, EdgeRole::Normal);
        }
        Ok(())
    }

    fn visit_label_chain(
        &mut self,
        object: RecordIndex,
        context: Context,
    ) -> Result<(), TsrxParseError> {
        let mut labels = Vec::new();
        let mut current = object;
        let mut remaining = self.tape.object_count();
        let body = loop {
            if remaining == 0 {
                return Err(TsrxParseError::Unsupported("cycle in authored label chain"));
            }
            remaining -= 1;
            let label = object_field(self.tape, current, "label")?;
            labels.push((
                LabelKey { scope: context.flow_scope, name: identifier_name(self.tape, label)? },
                label,
            ));
            let body = field_value(self.tape, current, "body")?;
            let Some(next) = body.as_object() else {
                break body;
            };
            if node_kind(self.tape, next) != NodeKind::LabeledStatement {
                break body;
            }
            let index = self.enter_object(next)?;
            self.work.push(Work::ExitObject(index));
            current = next;
        };
        let continuable = body
            .as_object()
            .is_some_and(|object| matches!(node_kind(self.tape, object), NodeKind::Loop { .. }));
        for (key, label) in labels {
            if self.labels.insert(key, continuable).is_some() {
                return Err(TsrxParseError::AuthoredGrammar(
                    "duplicate authored label in one lexical scope".to_string(),
                ));
            }
            self.work.push(Work::ExitLabel(key));
            self.push(ValueRef::object(label), context, EdgeRole::NameOnly);
        }
        self.push(body, context, EdgeRole::Normal);
        Ok(())
    }

    fn visit_break(
        &self,
        object: RecordIndex,
        context: Context,
        is_continue: bool,
    ) -> Result<(), TsrxParseError> {
        if !context.validate {
            return Ok(());
        }
        let label = field_value(self.tape, object, "label")?;
        if is_null(self.tape, label) {
            let depth = if is_continue { context.continue_depth } else { context.break_depth };
            if depth == 0 {
                return Err(TsrxParseError::AuthoredGrammar(
                    if is_continue {
                        "continue has no authored loop target"
                    } else {
                        "break has no authored loop or switch target"
                    }
                    .to_string(),
                ));
            }
            return Ok(());
        }
        let label = label
            .as_object()
            .ok_or(TsrxParseError::Unsupported("break or continue label is not an Identifier"))?;
        let key = LabelKey { scope: context.flow_scope, name: identifier_name(self.tape, label)? };
        let continuable = self.labels.get(&key).copied().ok_or_else(|| {
            TsrxParseError::AuthoredGrammar(
                "break or continue has no authored label target".to_string(),
            )
        })?;
        if is_continue && !continuable {
            return Err(TsrxParseError::AuthoredGrammar(
                "continue label does not target a standard loop".to_string(),
            ));
        }
        Ok(())
    }

    fn visit_return(
        &mut self,
        object: RecordIndex,
        context: Context,
    ) -> Result<(), TsrxParseError> {
        if context.validate && !context.return_ok {
            return Err(TsrxParseError::AuthoredGrammar(
                "return is outside an authored return boundary".to_string(),
            ));
        }
        self.visit_generic(object, context);
        Ok(())
    }

    fn visit_await(&mut self, object: RecordIndex, context: Context) -> Result<(), TsrxParseError> {
        if context.validate {
            self.require_await(context)?;
            let argument = field_value(self.tape, object, "argument")?;
            if is_null(self.tape, argument) || argument.kind() == ValueKind::Missing {
                return Err(TsrxParseError::Unsupported(
                    "await expression has no authored argument",
                ));
            }
        }
        self.visit_generic(object, context);
        Ok(())
    }

    fn visit_yield(&mut self, object: RecordIndex, context: Context) -> Result<(), TsrxParseError> {
        if context.validate && !context.generator_ok {
            return Err(TsrxParseError::AuthoredGrammar(
                "yield is outside an authored generator".to_string(),
            ));
        }
        self.visit_generic(object, context);
        Ok(())
    }

    fn visit_super(context: Context, role: EdgeRole) -> Result<(), TsrxParseError> {
        if !context.validate {
            return Ok(());
        }
        let allowed = match role {
            EdgeRole::SuperCall => context.super_call_ok,
            EdgeRole::SuperProperty => context.super_property_ok,
            _ => false,
        };
        if allowed {
            Ok(())
        } else {
            Err(TsrxParseError::AuthoredGrammar(
                "super is outside an authored method capability".to_string(),
            ))
        }
    }

    fn visit_call(&mut self, object: RecordIndex, context: Context) {
        for field in self.tape.fields(object) {
            let role = if self.tape.key(field) == "callee" {
                EdgeRole::SuperCall
            } else {
                EdgeRole::Normal
            };
            self.push(field.value, context, role);
        }
    }

    fn visit_member(&mut self, object: RecordIndex, context: Context) {
        let computed = scalar_field_is(self.tape, object, "computed", "true");
        for field in self.tape.fields(object) {
            let role = match self.tape.key(field) {
                "object" => EdgeRole::SuperProperty,
                "property" if !computed => EdgeRole::NameOnly,
                _ => EdgeRole::Normal,
            };
            self.push(field.value, context, role);
        }
    }

    fn visit_meta(&mut self, object: RecordIndex, context: Context) -> Result<(), TsrxParseError> {
        let meta = object_field(self.tape, object, "meta")?;
        let property = object_field(self.tape, object, "property")?;
        if context.validate
            && identifier_name(self.tape, meta)? == r#""new""#
            && identifier_name(self.tape, property)? == r#""target""#
            && !context.new_target_ok
        {
            return Err(TsrxParseError::AuthoredGrammar(
                "new.target is outside an authored function or class capability".to_string(),
            ));
        }
        self.push(ValueRef::object(meta), context, EdgeRole::NameOnly);
        self.push(ValueRef::object(property), context, EdgeRole::NameOnly);
        Ok(())
    }

    fn visit_identifier(
        &self,
        object: RecordIndex,
        context: Context,
        role: EdgeRole,
    ) -> Result<(), TsrxParseError> {
        if !context.validate || role == EdgeRole::NameOnly {
            return Ok(());
        }
        match scalar_field(self.tape, object, "name") {
            Some(r#""await""#) => Err(TsrxParseError::AuthoredGrammar(
                "await identifier is outside a name-only position".to_string(),
            )),
            Some(r#""yield""#) => Err(TsrxParseError::AuthoredGrammar(
                "yield identifier is outside a name-only position".to_string(),
            )),
            _ => Ok(()),
        }
    }

    fn visit_variable(
        &mut self,
        object: RecordIndex,
        context: Context,
    ) -> Result<(), TsrxParseError> {
        if context.validate && scalar_field(self.tape, object, "kind") == Some(r#""await using""#) {
            self.require_await(context)?;
        }
        self.visit_generic(object, context);
        Ok(())
    }

    fn visit_code_block(&mut self, object: RecordIndex, context: Context, role: EdgeRole) {
        let mut block = context.activated();
        block.return_ok = role == EdgeRole::FunctionBody;
        self.visit_generic(object, block);
    }

    fn visit_jsx_if(&mut self, object: RecordIndex, context: Context) {
        let active = context.activated();
        let mut branch = active;
        branch.return_ok = true;
        for field in self.tape.fields(object) {
            let child = match self.tape.key(field) {
                "consequent" | "alternate" => branch,
                _ => active,
            };
            self.push(field.value, child, EdgeRole::Normal);
        }
    }

    fn visit_jsx_for(
        &mut self,
        object: RecordIndex,
        context: Context,
    ) -> Result<(), TsrxParseError> {
        let active = context.activated();
        let mut body = active;
        body.return_ok = true;
        body.break_depth = increment(body.break_depth)?;
        body.continue_depth = increment(body.continue_depth)?;
        let mut empty = active;
        empty.return_ok = true;
        for field in self.tape.fields(object) {
            let child = match self.tape.key(field) {
                "body" => body,
                "empty" => empty,
                _ => active,
            };
            self.push(field.value, child, EdgeRole::Normal);
        }
        Ok(())
    }

    fn visit_jsx_switch(
        &mut self,
        object: RecordIndex,
        context: Context,
    ) -> Result<(), TsrxParseError> {
        let active = context.activated();
        let mut cases = active;
        cases.return_ok = false;
        cases.break_depth = increment(cases.break_depth)?;
        for field in self.tape.fields(object) {
            let child = if self.tape.key(field) == "cases" { cases } else { active };
            self.push(field.value, child, EdgeRole::Normal);
        }
        Ok(())
    }

    fn visit_jsx_try(&mut self, object: RecordIndex, context: Context) {
        let mut active = context.activated();
        active.return_ok = false;
        self.visit_generic(object, active);
    }

    fn visit_generic(&mut self, object: RecordIndex, context: Context) {
        for field in self.tape.fields(object) {
            self.push(field.value, context, EdgeRole::Normal);
        }
    }

    fn classify_object(
        &mut self,
        object: RecordIndex,
        context: Context,
    ) -> (NodeKind, SpanFields, bool) {
        let mut kind = None;
        let mut span_fields = SpanFields::default();
        let mut generic_visited = false;
        for (offset, (field_index, field)) in self.tape.fields_indexed(object).enumerate() {
            let name = self.tape.key(field);
            if name == "type" {
                kind = self.tape.scalar(field.value);
                if offset == 0 && matches!(classify_kind(kind), NodeKind::Program | NodeKind::Other)
                {
                    generic_visited = true;
                }
            }
            span_fields.record(name, field_index);
            if generic_visited {
                self.push(field.value, context, EdgeRole::Normal);
            }
        }
        (classify_kind(kind), span_fields, generic_visited)
    }

    fn require_await(&mut self, context: Context) -> Result<(), TsrxParseError> {
        if context.async_ok {
            return Ok(());
        }
        if context.top_level_await {
            self.promote_module = true;
            return Ok(());
        }
        Err(TsrxParseError::AuthoredGrammar(
            "await is outside an authored async or module context".to_string(),
        ))
    }

    fn fresh_scope(&mut self) -> Result<u32, TsrxParseError> {
        let scope = self.next_scope;
        self.next_scope = scope
            .checked_add(1)
            .ok_or(TsrxParseError::Unsupported("lexical flow-scope index overflow"))?;
        Ok(scope)
    }

    fn register_class_outer(&mut self, outer: Context) -> Result<u32, TsrxParseError> {
        self.class_outers.push(outer);
        u32::try_from(self.class_outers.len())
            .map_err(|_| TsrxParseError::Unsupported("class outer-context index above 4 GiB"))
    }

    fn class_outer(&self, class: Context) -> Result<Context, TsrxParseError> {
        let index = class
            .class_outer_id
            .checked_sub(1)
            .ok_or(TsrxParseError::Unsupported("class element has no outer lexical context"))?;
        self.class_outers.get(index as usize).copied().ok_or(TsrxParseError::Unsupported(
            "class outer-context index is outside the lexical table",
        ))
    }

    fn validate_class_body(&self, value: ValueRef) -> Result<(), TsrxParseError> {
        let body =
            value.as_object().ok_or(TsrxParseError::Unsupported("class body is not an object"))?;
        if object_type(self.tape, body) != Some(r#""ClassBody""#) {
            return Err(TsrxParseError::Unsupported("class body has an unexpected node type"));
        }
        let elements = field_value(self.tape, body, "body")?
            .as_list()
            .ok_or(TsrxParseError::Unsupported("class body elements are not a list"))?;
        for element in self.tape.values(elements) {
            let element = element
                .as_object()
                .ok_or(TsrxParseError::Unsupported("class body element is not an object"))?;
            if !matches!(
                object_type(self.tape, element),
                Some(
                    r#""StaticBlock""#
                        | r#""MethodDefinition""#
                        | r#""TSAbstractMethodDefinition""#
                        | r#""PropertyDefinition""#
                        | r#""TSAbstractPropertyDefinition""#
                        | r#""AccessorProperty""#
                        | r#""TSAbstractAccessorProperty""#
                        | r#""TSIndexSignature""#
                )
            ) {
                return Err(TsrxParseError::Unsupported("unknown lexical class-body element"));
            }
        }
        Ok(())
    }

    fn exit_label(&mut self, key: LabelKey<'tape>) -> Result<(), TsrxParseError> {
        if self.labels.remove(&key).is_none() {
            return Err(TsrxParseError::Unsupported("lexical label stack is inconsistent"));
        }
        Ok(())
    }

    fn push(&mut self, value: ValueRef, context: Context, role: EdgeRole) {
        self.work.push(Work::Value(value, context, role));
    }

    fn enter_object(&mut self, object: RecordIndex) -> Result<usize, TsrxParseError> {
        let index = index_of(object)?;
        let state = self
            .object_states
            .get_mut(index)
            .ok_or(TsrxParseError::Unsupported("object reference outside lexical tape"))?;
        match *state {
            0 => *state = 1,
            1 => {
                return Err(TsrxParseError::Unsupported("cycle in reconstructed object graph"));
            }
            _ => {
                return Err(TsrxParseError::Unsupported(
                    "shared object in reconstructed reachable graph",
                ));
            }
        }
        Ok(index)
    }

    fn enter_list(&mut self, list: RecordIndex) -> Result<usize, TsrxParseError> {
        let index = index_of(list)?;
        let state = self
            .list_states
            .get_mut(index)
            .ok_or(TsrxParseError::Unsupported("list reference outside lexical tape"))?;
        match *state {
            0 => *state = 1,
            1 => {
                return Err(TsrxParseError::Unsupported("cycle in reconstructed list graph"));
            }
            _ => {
                return Err(TsrxParseError::Unsupported(
                    "shared list in reconstructed reachable graph",
                ));
            }
        }
        Ok(index)
    }
}

fn node_kind(tape: &FlatTape, object: RecordIndex) -> NodeKind {
    classify_kind(object_type(tape, object))
}

fn classify_kind(kind: Option<&str>) -> NodeKind {
    match kind {
        None => NodeKind::Other,
        Some(kind) => match kind {
            r#""Program""# => NodeKind::Program,
            r#""FunctionDeclaration""#
            | r#""FunctionExpression""#
            | r#""TSDeclareFunction""#
            | r#""TSEmptyBodyFunctionExpression""# => NodeKind::Function,
            r#""ArrowFunctionExpression""# => NodeKind::ArrowFunction,
            r#""ClassDeclaration""# | r#""ClassExpression""# => NodeKind::Class,
            r#""StaticBlock""# => NodeKind::StaticBlock,
            r#""MethodDefinition""# | r#""TSAbstractMethodDefinition""# => {
                NodeKind::MethodDefinition
            }
            r#""PropertyDefinition""#
            | r#""TSAbstractPropertyDefinition""#
            | r#""AccessorProperty""#
            | r#""TSAbstractAccessorProperty""# => NodeKind::PropertyDefinition,
            r#""Property""# => NodeKind::Property,
            r#""ForStatement""#
            | r#""ForInStatement""#
            | r#""WhileStatement""#
            | r#""DoWhileStatement""# => NodeKind::Loop { for_of: false },
            r#""ForOfStatement""# => NodeKind::Loop { for_of: true },
            r#""SwitchStatement""# => NodeKind::SwitchStatement,
            r#""LabeledStatement""# => NodeKind::LabeledStatement,
            r#""BreakStatement""# => NodeKind::BreakStatement { is_continue: false },
            r#""ContinueStatement""# => NodeKind::BreakStatement { is_continue: true },
            r#""ReturnStatement""# => NodeKind::ReturnStatement,
            r#""AwaitExpression""# => NodeKind::AwaitExpression,
            r#""YieldExpression""# => NodeKind::YieldExpression,
            r#""Super""# => NodeKind::Super,
            r#""CallExpression""# => NodeKind::CallExpression,
            r#""MemberExpression""# => NodeKind::MemberExpression,
            r#""MetaProperty""# => NodeKind::MetaProperty,
            r#""Identifier""# => NodeKind::Identifier,
            r#""VariableDeclaration""# => NodeKind::VariableDeclaration,
            r#""JSXCodeBlock""# => NodeKind::JsxCodeBlock,
            r#""JSXIfExpression""# => NodeKind::JsxIfExpression,
            r#""JSXForExpression""# => NodeKind::JsxForExpression,
            r#""JSXSwitchExpression""# => NodeKind::JsxSwitchExpression,
            r#""JSXTryExpression""# => NodeKind::JsxTryExpression,
            _ => NodeKind::Other,
        },
    }
}

fn object_type(tape: &FlatTape, object: RecordIndex) -> Option<&str> {
    scalar_field(tape, object, "type")
}

fn scalar_field<'tape>(
    tape: &'tape FlatTape,
    object: RecordIndex,
    name: &str,
) -> Option<&'tape str> {
    tape.field_index(object, name)
        .and_then(|field| tape.field_value(field))
        .and_then(|value| tape.scalar(value))
}

fn scalar_field_is(tape: &FlatTape, object: RecordIndex, name: &str, value: &str) -> bool {
    scalar_field(tape, object, name) == Some(value)
}

fn field_value(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<ValueRef, TsrxParseError> {
    tape.field_index(object, name).and_then(|field| tape.field_value(field)).ok_or(
        TsrxParseError::Unsupported("reconstructed object is missing a required lexical field"),
    )
}

fn object_field(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<RecordIndex, TsrxParseError> {
    field_value(tape, object, name)?
        .as_object()
        .ok_or(TsrxParseError::Unsupported("required lexical field is not an object"))
}

fn identifier_name(tape: &FlatTape, identifier: RecordIndex) -> Result<&str, TsrxParseError> {
    if object_type(tape, identifier) != Some(r#""Identifier""#) {
        return Err(TsrxParseError::Unsupported("lexical name component is not an Identifier"));
    }
    scalar_field(tape, identifier, "name")
        .ok_or(TsrxParseError::Unsupported("Identifier has no encoded name"))
}

fn is_null(tape: &FlatTape, value: ValueRef) -> bool {
    tape.scalar(value) == Some("null")
}

fn increment(depth: u32) -> Result<u32, TsrxParseError> {
    depth.checked_add(1).ok_or(TsrxParseError::Unsupported("lexical control-depth overflow"))
}

fn index_of(index: RecordIndex) -> Result<usize, TsrxParseError> {
    index
        .get()
        .map(|raw| raw as usize)
        .ok_or(TsrxParseError::Unsupported("missing record index during lexical validation"))
}
