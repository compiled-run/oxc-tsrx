mod support;

use support::{
    assert_empty_path, assert_failed, assert_no_scaffold, field, list_field, object_field,
    one_object, program_body, require_type, scalar_field, span,
};
use tsrx_parser_engine::{TsrxParseRequest, parse_tsrx};
use tsrx_tape_schema::{FlatTape, RecordIndex};

fn field_names(tape: &FlatTape, object: RecordIndex) -> Vec<&str> {
    tape.fields(object).map(|record| tape.key(record)).collect()
}

fn nullable_object(tape: &FlatTape, object: RecordIndex, name: &str) -> Option<RecordIndex> {
    let value = field(tape, object, name);
    if tape.scalar(value) == Some("null") {
        None
    } else {
        Some(value.as_object().expect("nullable object field"))
    }
}

fn assert_control_block(
    tape: &FlatTape,
    block: RecordIndex,
    expected_span: (u32, u32),
) -> Vec<RecordIndex> {
    require_type(tape, block, "BlockStatement");
    assert_eq!(span(tape, block), expected_span);
    assert_eq!(field_names(tape, block), ["type", "start", "end", "body", "metadata"]);
    assert_empty_path(tape, block);
    list_field(tape, block, "body")
        .into_iter()
        .map(|value| value.as_object().expect("block body object"))
        .collect()
}

fn assert_try_head(
    tape: &FlatTape,
    node: RecordIndex,
    expected_span: (u32, u32),
    block_span: (u32, u32),
) -> RecordIndex {
    require_type(tape, node, "JSXTryExpression");
    assert_eq!(span(tape, node), expected_span);
    assert_eq!(scalar_field(tape, node, "statementType"), r#""TryStatement""#);
    assert_eq!(
        field_names(tape, node),
        [
            "type",
            "start",
            "end",
            "block",
            "handler",
            "pending",
            "finalizer",
            "metadata",
            "statementType",
        ]
    );
    assert_empty_path(tape, node);
    let block = object_field(tape, node, "block");
    assert_control_block(tape, block, block_span);
    assert_eq!(tape.scalar(field(tape, node, "finalizer")), Some("null"));
    block
}

fn assert_handler(
    tape: &FlatTape,
    handler: RecordIndex,
    expected_span: (u32, u32),
    body_span: (u32, u32),
) -> RecordIndex {
    require_type(tape, handler, "CatchClause");
    assert_eq!(span(tape, handler), expected_span);
    assert_eq!(field_names(tape, handler), ["type", "start", "end", "param", "resetParam", "body"]);
    let body = object_field(tape, handler, "body");
    assert_control_block(tape, body, body_span);
    body
}

#[test]
fn reconstructs_statement_try_as_an_expression_statement() {
    let source = "function run(){@try{one}@catch(error){two}}";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("statement @try");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let function_body = object_field(tape, function, "body");
    let statement = one_object(&list_field(tape, function_body, "body"));
    require_type(tape, statement, "ExpressionStatement");
    assert_eq!(span(tape, statement), (15, 42));
    let node = object_field(tape, statement, "expression");
    assert_try_head(tape, node, (15, 42), (19, 24));
    assert_eq!(tape.scalar(field(tape, node, "pending")), Some("null"));
    let handler = nullable_object(tape, node, "handler").expect("catch handler");
    assert_handler(tape, handler, (24, 42), (37, 42));
    let param = object_field(tape, handler, "param");
    require_type(tape, param, "Identifier");
    assert_eq!(span(tape, param), (31, 36));
    assert_eq!(tape.scalar(field(tape, handler, "resetParam")), Some("null"));
    assert_no_scaffold(tape);
}

#[test]
fn reconstructs_pending_only_expression_try_directly() {
    let source = "const value=@try{<b/>}@pending{<i/>};";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("pending-only @try");
    let tape = result.program();
    let declaration = one_object(&program_body(tape));
    let declarator = one_object(&list_field(tape, declaration, "declarations"));
    let node = object_field(tape, declarator, "init");
    assert_try_head(tape, node, (12, 36), (16, 22));
    assert_eq!(tape.scalar(field(tape, node, "handler")), Some("null"));
    let pending = nullable_object(tape, node, "pending").expect("pending block");
    let body = assert_control_block(tape, pending, (30, 36));
    require_type(tape, one_record(&body), "JSXElement");
    assert_no_scaffold(tape);
}

fn one_record(values: &[RecordIndex]) -> RecordIndex {
    assert_eq!(values.len(), 1);
    values[0]
}

#[test]
fn preserves_headerless_catch_with_explicit_null_bindings() {
    let source = "const value=@try{<b/>}@catch{<i/>};";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("headerless @catch");
    let tape = result.program();
    let declaration = one_object(&program_body(tape));
    let declarator = one_object(&list_field(tape, declaration, "declarations"));
    let node = object_field(tape, declarator, "init");
    assert_try_head(tape, node, (12, 34), (16, 22));
    assert_eq!(tape.scalar(field(tape, node, "pending")), Some("null"));
    let handler = nullable_object(tape, node, "handler").expect("handler");
    assert_handler(tape, handler, (22, 34), (28, 34));
    assert_eq!(tape.scalar(field(tape, handler, "param")), Some("null"));
    assert_eq!(tape.scalar(field(tape, handler, "resetParam")), Some("null"));
    assert_no_scaffold(tape);
}

#[test]
fn preserves_one_and_two_catch_bindings_in_authored_order() {
    let one = parse_tsrx(&TsrxParseRequest {
        source: "const value=@try{<b/>}@catch(error){<i>{error}</i>};",
    })
    .expect("one catch binding");
    let declaration = one_object(&program_body(one.program()));
    let declarator = one_object(&list_field(one.program(), declaration, "declarations"));
    let node = object_field(one.program(), declarator, "init");
    let handler = nullable_object(one.program(), node, "handler").expect("handler");
    let param = object_field(one.program(), handler, "param");
    require_type(one.program(), param, "Identifier");
    assert_eq!(span(one.program(), param), (29, 34));
    assert_eq!(one.program().scalar(field(one.program(), handler, "resetParam")), Some("null"));
    assert_no_scaffold(one.program());

    let two = parse_tsrx(&TsrxParseRequest {
        source: "const value=@try{<b/>}@catch(error, reset){reset();<i/>};",
    })
    .expect("two catch bindings");
    let declaration = one_object(&program_body(two.program()));
    let declarator = one_object(&list_field(two.program(), declaration, "declarations"));
    let node = object_field(two.program(), declarator, "init");
    assert_try_head(two.program(), node, (12, 56), (16, 22));
    let handler = nullable_object(two.program(), node, "handler").expect("handler");
    assert_handler(two.program(), handler, (22, 56), (42, 56));
    let param = object_field(two.program(), handler, "param");
    let reset = object_field(two.program(), handler, "resetParam");
    require_type(two.program(), param, "Identifier");
    require_type(two.program(), reset, "Identifier");
    assert_eq!(span(two.program(), param), (29, 34));
    assert_eq!(span(two.program(), reset), (36, 41));
    let body = object_field(two.program(), handler, "body");
    let values = list_field(two.program(), body, "body");
    assert_eq!(values.len(), 2);
    require_type(
        two.program(),
        values[0].as_object().expect("reset statement"),
        "ExpressionStatement",
    );
    require_type(two.program(), values[1].as_object().expect("catch JSX"), "JSXElement");
    assert_no_scaffold(two.program());
}

#[test]
fn accepts_destructured_and_typed_catch_bindings_in_the_canonical_js_view() {
    let source = concat!(
        "const value=@try{ok}@catch({message}: ErrorInfo, ",
        "reset: () => void){reset();message};"
    );
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("typed catch bindings");
    let tape = result.program();
    let declaration = one_object(&program_body(tape));
    let declarator = one_object(&list_field(tape, declaration, "declarations"));
    let node = object_field(tape, declarator, "init");
    let handler = nullable_object(tape, node, "handler").expect("handler");
    let param = object_field(tape, handler, "param");
    let reset = object_field(tape, handler, "resetParam");
    require_type(tape, param, "ObjectPattern");
    require_type(tape, reset, "Identifier");
    assert_eq!(
        span(tape, param),
        (
            u32::try_from(source.find("{message}").expect("pattern start")).unwrap(),
            u32::try_from(source.find("{message}").expect("pattern start") + "{message}".len())
                .unwrap(),
        )
    );
    assert_eq!(
        span(tape, reset),
        (
            u32::try_from(source.find("reset:").expect("reset start")).unwrap(),
            u32::try_from(source.find("reset:").expect("reset start") + "reset".len()).unwrap(),
        )
    );
    assert_no_scaffold(tape);

    let array =
        parse_tsrx(&TsrxParseRequest { source: "const value=@try{ok}@catch([error]){error};" })
            .expect("array catch binding");
    let declaration = one_object(&program_body(array.program()));
    let declarator = one_object(&list_field(array.program(), declaration, "declarations"));
    let node = object_field(array.program(), declarator, "init");
    let handler = nullable_object(array.program(), node, "handler").expect("handler");
    require_type(array.program(), object_field(array.program(), handler, "param"), "ArrayPattern");
    assert_no_scaffold(array.program());
}

#[test]
fn permits_returns_only_below_nested_function_or_loop_boundaries() {
    for source in [
        "const value=@try{for(;;){return 1}}@catch{};",
        "const value=@try{for(const x in xs){return x}}@catch{};",
        "const value=@try{for(const x of xs){return x}}@catch{};",
        "const value=@try{while(ok){return 1}}@catch{};",
        "const value=@try{do{return 1}while(ok)}@catch{};",
        "const value=@try{function nested(){return 1}}@catch{};",
        "const value=@try{(()=>{return 1})()}@catch{};",
    ] {
        let result = parse_tsrx(&TsrxParseRequest { source }).expect("nested return boundary");
        assert_no_scaffold(result.program());
    }
}

#[test]
fn preserves_empty_try_family_blocks() {
    for source in [
        "const value=@try{}@pending{};",
        "const value=@try{}@catch{};",
        "const value=@try{}@pending{}@catch{};",
    ] {
        let result = parse_tsrx(&TsrxParseRequest { source }).expect("empty try blocks");
        let tape = result.program();
        let declaration = one_object(&program_body(tape));
        let declarator = one_object(&list_field(tape, declaration, "declarations"));
        let node = object_field(tape, declarator, "init");
        assert!(list_field(tape, object_field(tape, node, "block"), "body").is_empty());
        if let Some(pending) = nullable_object(tape, node, "pending") {
            assert!(list_field(tape, pending, "body").is_empty());
        }
        if let Some(handler) = nullable_object(tape, node, "handler") {
            let body = object_field(tape, handler, "body");
            assert!(list_field(tape, body, "body").is_empty());
        }
        assert_no_scaffold(tape);
    }
}

#[test]
fn preserves_an_authored_statement_semicolon_outside_the_try_span() {
    for source in [
        "function run(){@try{one}@catch{two};next()}",
        "function run(){@try{one}@catch{two} ;next()}",
        "function run(){@try{one}@catch{two}\n  ;next()}",
    ] {
        let result = parse_tsrx(&TsrxParseRequest { source }).expect("statement semicolon");
        let tape = result.program();
        let function = one_object(&program_body(tape));
        let function_body = object_field(tape, function, "body");
        let statements = list_field(tape, function_body, "body");
        assert_eq!(statements.len(), 2);
        let statement = statements[0].as_object().expect("try statement");
        let node = object_field(tape, statement, "expression");
        assert_eq!(
            span(tape, statement).1,
            u32::try_from(source.find(';').expect("authored semicolon") + 1).unwrap()
        );
        assert!(span(tape, node).1 < span(tape, statement).1);
        require_type(
            tape,
            statements[1].as_object().expect("following statement"),
            "ExpressionStatement",
        );
        assert_no_scaffold(tape);
    }
}

#[test]
fn preserves_pending_then_catch_and_all_explicit_fields() {
    let source = "const value=@try{<b/>}@pending{<u/>}@catch(error, reset){<i/>};";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("pending and catch");
    let tape = result.program();
    let declaration = one_object(&program_body(tape));
    let declarator = one_object(&list_field(tape, declaration, "declarations"));
    let node = object_field(tape, declarator, "init");
    assert_try_head(tape, node, (12, 62), (16, 22));
    let pending = nullable_object(tape, node, "pending").expect("pending");
    assert_control_block(tape, pending, (30, 36));
    let handler = nullable_object(tape, node, "handler").expect("handler");
    assert_handler(tape, handler, (36, 62), (56, 62));
    assert_no_scaffold(tape);
}

#[test]
fn reconstructs_jsx_child_try_without_an_expression_container() {
    let source =
        concat!("function View() @{<main>@try{<b/>}@pending{<u/>}", "@catch(error){<i/>}</main>}");
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("JSX-child @try");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    let main = object_field(tape, code_block, "render");
    let node = one_object(&list_field(tape, main, "children"));
    assert_try_head(tape, node, (24, 67), (28, 34));
    let pending = nullable_object(tape, node, "pending").expect("pending");
    assert_control_block(tape, pending, (42, 48));
    let handler = nullable_object(tape, node, "handler").expect("handler");
    assert_handler(tape, handler, (48, 67), (61, 67));
    assert_no_scaffold(tape);
}

#[test]
fn promotes_a_terminal_statement_try_to_code_block_render() {
    let source =
        concat!("function View() @{ @try{<b/>}@pending{<u/>}", "@catch(error, reset){<i/>} }");
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("terminal @try");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    assert!(list_field(tape, code_block, "body").is_empty());
    let node = object_field(tape, code_block, "render");
    assert_try_head(tape, node, (19, 69), (23, 29));
    assert_no_scaffold(tape);
}

#[test]
fn composes_switch_if_and_for_inside_try_clauses() {
    let source = concat!(
        "function View() @{<main>@try{",
        "@switch(x){@default:{<b/>}}}",
        "@catch(error){@if(ok){<i/>}@else{@for(const x of xs){<u/>}}}",
        "</main>}"
    );
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("nested @try controls");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    let main = object_field(tape, code_block, "render");
    let node = one_object(&list_field(tape, main, "children"));
    assert_try_head(tape, node, (24, 117), (28, 57));
    let block = object_field(tape, node, "block");
    let switch = one_record(&assert_control_block(tape, block, (28, 57)));
    require_type(tape, switch, "JSXSwitchExpression");
    let handler = nullable_object(tape, node, "handler").expect("handler");
    let handler_body = assert_handler(tape, handler, (57, 117), (70, 117));
    let outer_if = one_record(&assert_control_block(tape, handler_body, (70, 117)));
    require_type(tape, outer_if, "JSXIfExpression");
    let alternate = object_field(tape, outer_if, "alternate");
    let loop_node = one_object(&list_field(tape, alternate, "body"));
    require_type(tape, loop_node, "JSXForExpression");
    assert_no_scaffold(tape);
}

#[test]
fn reconstructs_try_inside_switch_and_nested_try_inside_out() {
    let inside_switch = parse_tsrx(&TsrxParseRequest {
        source: concat!("function View() @{ @switch(x){@case 1:{", "@try{<b/>}@catch{<i/>}}} }"),
    })
    .expect("try inside switch");
    let function = one_object(&program_body(inside_switch.program()));
    let code_block = object_field(inside_switch.program(), function, "body");
    let switch = object_field(inside_switch.program(), code_block, "render");
    let case = one_object(&list_field(inside_switch.program(), switch, "cases"));
    let node = one_object(&list_field(inside_switch.program(), case, "consequent"));
    require_type(inside_switch.program(), node, "JSXTryExpression");
    assert_no_scaffold(inside_switch.program());

    let nested = parse_tsrx(&TsrxParseRequest {
        source: concat!("function View() @{ @try{", "@try{<b/>}@catch{<i/>}}@catch{<u/>} }"),
    })
    .expect("nested tries");
    let function = one_object(&program_body(nested.program()));
    let code_block = object_field(nested.program(), function, "body");
    let outer = object_field(nested.program(), code_block, "render");
    let block = object_field(nested.program(), outer, "block");
    let inner = one_object(&list_field(nested.program(), block, "body"));
    require_type(nested.program(), inner, "JSXTryExpression");
    assert_no_scaffold(nested.program());
}

#[test]
fn leaves_an_ordinary_javascript_try_unchanged() {
    let source = concat!("function View() @{ try { <b/>; } catch(error) { <i/>; } ", "<main/> }");
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("ordinary try");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    let ordinary = one_object(&list_field(tape, code_block, "body"));
    require_type(tape, ordinary, "TryStatement");
    let block = object_field(tape, ordinary, "block");
    let statement = one_object(&list_field(tape, block, "body"));
    require_type(tape, statement, "ExpressionStatement");
    let handler = object_field(tape, ordinary, "handler");
    let handler_body = object_field(tape, handler, "body");
    let statement = one_object(&list_field(tape, handler_body, "body"));
    require_type(tape, statement, "ExpressionStatement");
    assert_no_scaffold(tape);
}

#[test]
fn malformed_try_clause_orders_return_no_program() {
    for source in [
        "const value=@try{};",
        "const value=@try{}@catch{}@pending{};",
        "const value=@try{}@pending{}@pending{};",
        "const value=@try{}@catch{}@catch{};",
        "const value=@try{}@catch();",
        "const value=@try{}@catch(error,);",
        "const value=@try{}@catch(error, reset, extra){};",
        "const value=@try{}@pending(value){};",
        "const value=@try{return 1}@catch{};",
        "const value=@try{}@pending{return 1};",
        "const value=@try{}@catch{return 1};",
        "const value=@try{if(ok){return 1}}@catch{};",
        "const value=@try{try{return 1}catch{}}@catch{};",
        "const value=@try{switch(x){case 1:return 1}}@catch{};",
        "const value=@try{}@catch(error = fallback){};",
        "const value=@try{}@catch({x} = fallback){};",
        "const value=@try{}@catch(...error){};",
        "const value=@try{}@catch(error?: Error){};",
        "const value=@try{}@catch(error, reset = fallback){};",
    ] {
        assert_failed(source);
    }
}
