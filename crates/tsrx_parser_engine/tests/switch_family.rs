#[expect(
    dead_code,
    reason = "the shared test-support module is compiled into every integration binary and each one uses a different part of it"
)]
mod support;

use support::{
    assert_empty_path, assert_no_scaffold, field, list_field, object_field, one_object,
    program_body, require_type, scalar_field, span,
};
use tsrx_parser_engine::{TsrxParseRequest, parse_tsrx};
use tsrx_tape_schema::{FlatTape, RecordIndex};

fn field_names(tape: &FlatTape, object: RecordIndex) -> Vec<&str> {
    tape.fields(object).map(|record| tape.key(record)).collect()
}

fn assert_switch_head(
    tape: &FlatTape,
    node: RecordIndex,
    expected_span: (u32, u32),
    discriminant_span: (u32, u32),
) {
    require_type(tape, node, "JSXSwitchExpression");
    assert_eq!(span(tape, node), expected_span);
    assert_eq!(scalar_field(tape, node, "statementType"), r#""SwitchStatement""#);
    assert_eq!(span(tape, object_field(tape, node, "discriminant")), discriminant_span);
    assert_empty_path(tape, node);
    assert_eq!(
        field_names(tape, node),
        ["type", "start", "end", "discriminant", "cases", "metadata", "statementType",]
    );
}

fn cases(tape: &FlatTape, switch: RecordIndex) -> Vec<RecordIndex> {
    list_field(tape, switch, "cases")
        .into_iter()
        .map(|value| value.as_object().expect("SwitchCase object"))
        .collect()
}

fn assert_case(
    tape: &FlatTape,
    case: RecordIndex,
    expected_span: (u32, u32),
    test_span: Option<(u32, u32)>,
) -> Vec<RecordIndex> {
    require_type(tape, case, "SwitchCase");
    assert_eq!(span(tape, case), expected_span);
    assert_eq!(field_names(tape, case), ["type", "start", "end", "consequent", "test"]);
    match test_span {
        Some(expected) => assert_eq!(span(tape, object_field(tape, case, "test")), expected),
        None => assert_eq!(tape.scalar(field(tape, case, "test")), Some("null")),
    }
    list_field(tape, case, "consequent")
        .into_iter()
        .map(|value| value.as_object().expect("case consequent object"))
        .collect()
}

#[test]
fn reconstructs_statement_switch_as_an_expression_statement() {
    let source = "function run(){@switch(kind){@case 1:{one}@default:{two}}}";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("statement @switch");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let function_body = object_field(tape, function, "body");
    let statement = one_object(&list_field(tape, function_body, "body"));
    require_type(tape, statement, "ExpressionStatement");
    assert_eq!(span(tape, statement), (15, 57));
    let switch = object_field(tape, statement, "expression");
    assert_switch_head(tape, switch, (15, 57), (23, 27));

    let cases = cases(tape, switch);
    assert_eq!(cases.len(), 2);
    let first = assert_case(tape, cases[0], (29, 42), Some((35, 36)));
    let default = assert_case(tape, cases[1], (42, 56), None);
    assert_eq!(span(tape, one_object_value(&first)), (38, 41));
    assert_eq!(span(tape, one_object_value(&default)), (52, 55));
    assert_no_scaffold(tape);
}

fn one_object_value(values: &[RecordIndex]) -> RecordIndex {
    assert_eq!(values.len(), 1);
    values[0]
}

#[test]
fn reconstructs_expression_switch_directly_as_the_initializer() {
    let source = "const value=@switch(kind){@case 1:{one}@default:{two}};";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("expression @switch");
    let tape = result.program();
    let declaration = one_object(&program_body(tape));
    let declarator = one_object(&list_field(tape, declaration, "declarations"));
    let switch = object_field(tape, declarator, "init");
    assert_switch_head(tape, switch, (12, 54), (20, 24));
    let cases = cases(tape, switch);
    assert_case(tape, cases[0], (26, 39), Some((32, 33)));
    assert_case(tape, cases[1], (39, 53), None);
    assert_no_scaffold(tape);
}

#[test]
fn reconstructs_jsx_child_switch_without_an_expression_container() {
    let source =
        concat!("function View() @{<main>@switch(kind){", "@case 1:{<b/>}@default:{<i/>}}</main>}");
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("JSX-child @switch");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    let main = object_field(tape, code_block, "render");
    let switch = one_object(&list_field(tape, main, "children"));
    assert_switch_head(tape, switch, (24, 68), (32, 36));
    let cases = cases(tape, switch);
    let first = assert_case(tape, cases[0], (38, 52), Some((44, 45)));
    let default = assert_case(tape, cases[1], (52, 67), None);
    require_type(tape, one_object_value(&first), "JSXElement");
    require_type(tape, one_object_value(&default), "JSXElement");
    assert_no_scaffold(tape);
}

#[test]
fn preserves_empty_switches_and_empty_case_consequents() {
    let empty = parse_tsrx(&TsrxParseRequest { source: "const x=@switch(kind){};" })
        .expect("empty @switch");
    let declaration = one_object(&program_body(empty.program()));
    let declarator = one_object(&list_field(empty.program(), declaration, "declarations"));
    let switch = object_field(empty.program(), declarator, "init");
    assert_switch_head(empty.program(), switch, (8, 23), (16, 20));
    assert!(cases(empty.program(), switch).is_empty());
    assert_no_scaffold(empty.program());

    let empty_case = parse_tsrx(&TsrxParseRequest { source: "const x=@switch(kind){@case 1:{}};" })
        .expect("empty @case");
    let declaration = one_object(&program_body(empty_case.program()));
    let declarator = one_object(&list_field(empty_case.program(), declaration, "declarations"));
    let switch = object_field(empty_case.program(), declarator, "init");
    assert_switch_head(empty_case.program(), switch, (8, 33), (16, 20));
    let case = one_object_value(&cases(empty_case.program(), switch));
    assert!(assert_case(empty_case.program(), case, (22, 32), Some((28, 29))).is_empty());
    assert_no_scaffold(empty_case.program());
}

#[test]
fn preserves_default_first_source_order_and_flattens_clause_braces() {
    let source =
        concat!("const x=@switch(kind){@default:{zero}", "@case 1:{const y=1;<b/>}@case 2:{two}};");
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("ordered @switch");
    let tape = result.program();
    let declaration = one_object(&program_body(tape));
    let declarator = one_object(&list_field(tape, declaration, "declarations"));
    let switch = object_field(tape, declarator, "init");
    let cases = cases(tape, switch);
    assert_eq!(cases.len(), 3);
    let default = assert_case(tape, cases[0], (22, 37), None);
    let first = assert_case(tape, cases[1], (37, 61), Some((43, 44)));
    let second = assert_case(tape, cases[2], (61, 74), Some((67, 68)));
    require_type(tape, one_object_value(&default), "ExpressionStatement");
    assert_eq!(first.len(), 2);
    require_type(tape, first[0], "VariableDeclaration");
    require_type(tape, first[1], "JSXElement");
    require_type(tape, one_object_value(&second), "ExpressionStatement");
    assert_no_scaffold(tape);
}

#[test]
fn promotes_a_terminal_statement_switch_to_code_block_render() {
    let source = "function View() @{ @switch(kind){@case 1:{<b/>}@default:{<i/>}} }";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("terminal @switch");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    require_type(tape, code_block, "JSXCodeBlock");
    assert!(list_field(tape, code_block, "body").is_empty());
    let switch = object_field(tape, code_block, "render");
    assert_switch_head(tape, switch, (19, 63), (27, 31));
    let cases = cases(tape, switch);
    assert_case(tape, cases[0], (33, 47), Some((39, 40)));
    assert_case(tape, cases[1], (47, 62), None);
    assert_no_scaffold(tape);
}

#[test]
fn composes_nested_if_and_for_controls_as_flat_case_consequents() {
    let source = concat!(
        "function View() @{<main>@switch(kind){",
        "@case 1:{@if(ok){<b/>}@else{<i/>}}",
        "@case 2:{@for(const x of xs){<u>{x}</u>}}",
        "@default:{<em/>}}</main>}"
    );
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("nested @switch controls");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    let main = object_field(tape, code_block, "render");
    let switch = one_object(&list_field(tape, main, "children"));
    assert_switch_head(tape, switch, (24, 130), (32, 36));
    let cases = cases(tape, switch);
    let first = assert_case(tape, cases[0], (38, 72), Some((44, 45)));
    let second = assert_case(tape, cases[1], (72, 113), Some((78, 79)));
    assert_case(tape, cases[2], (113, 129), None);
    require_type(tape, one_object_value(&first), "JSXIfExpression");
    require_type(tape, one_object_value(&second), "JSXForExpression");
    assert_no_scaffold(tape);
}

#[test]
fn reconstructs_an_inner_switch_before_its_outer_if_block() {
    let source =
        concat!("function View() @{ @if(ok){", "@switch(kind){@case 1:{<b/>}@default:{<i/>}}} }");
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("switch inside if");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    let outer = object_field(tape, code_block, "render");
    require_type(tape, outer, "JSXIfExpression");
    let consequent = object_field(tape, outer, "consequent");
    let switch = one_object(&list_field(tape, consequent, "body"));
    require_type(tape, switch, "JSXSwitchExpression");
    assert_no_scaffold(tape);
}

#[test]
fn reconstructs_an_inner_switch_before_its_outer_for_block() {
    let source =
        concat!("function View() @{ @for(const x of xs){", "@switch(x.kind){@default:{<i/>}}} }");
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("switch inside for");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    let outer = object_field(tape, code_block, "render");
    require_type(tape, outer, "JSXForExpression");
    let body = object_field(tape, outer, "body");
    let switch = one_object(&list_field(tape, body, "body"));
    require_type(tape, switch, "JSXSwitchExpression");
    assert_no_scaffold(tape);
}

#[test]
fn leaves_an_ordinary_javascript_switch_case_block_unchanged() {
    let source = "function View() @{ switch(kind){case 1:<b/>;break;} <main/> }";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("ordinary switch");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    let ordinary = one_object(&list_field(tape, code_block, "body"));
    require_type(tape, ordinary, "SwitchStatement");
    let case = one_object(&list_field(tape, ordinary, "cases"));
    let consequent = list_field(tape, case, "consequent");
    assert_eq!(consequent.len(), 2);
    require_type(
        tape,
        consequent[0].as_object().expect("expression statement"),
        "ExpressionStatement",
    );
    require_type(tape, consequent[1].as_object().expect("break statement"), "BreakStatement");
    require_type(tape, object_field(tape, code_block, "render"), "JSXElement");
    assert_no_scaffold(tape);
}

#[test]
fn reconstructs_nested_switches_inside_out() {
    let source = concat!(
        "function View() @{<main>@switch(a){@case 1:{",
        "@switch(b){@default:{<i/>}}}@default:{<b/>}}</main>}"
    );
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("nested switches");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    let main = object_field(tape, code_block, "render");
    let outer = one_object(&list_field(tape, main, "children"));
    require_type(tape, outer, "JSXSwitchExpression");
    let outer_cases = cases(tape, outer);
    let first = list_field(tape, outer_cases[0], "consequent");
    let inner = one_object(&first);
    require_type(tape, inner, "JSXSwitchExpression");
    let inner_default = one_object_value(&cases(tape, inner));
    let child = one_object(&list_field(tape, inner_default, "consequent"));
    require_type(tape, child, "JSXElement");
    assert_no_scaffold(tape);
}
