mod support;

use support::{
    assert_empty_path, assert_no_scaffold, field, list_field, object_field, one_object,
    program_body, require_type, scalar_field, span,
};
use tsrx_parser_engine::{TsrxParseRequest, parse_tsrx};
use tsrx_tape_schema::{FlatTape, RecordIndex};

fn assert_if_head(
    tape: &FlatTape,
    if_node: RecordIndex,
    expected_span: (u32, u32),
    test_span: (u32, u32),
) {
    require_type(tape, if_node, "JSXIfExpression");
    assert_eq!(scalar_field(tape, if_node, "statementType"), r#""IfStatement""#);
    assert_eq!(span(tape, if_node), expected_span);
    let test = object_field(tape, if_node, "test");
    require_type(tape, test, "Identifier");
    assert_eq!(span(tape, test), test_span);
    assert_empty_path(tape, if_node);
}

#[test]
fn reconstructs_statement_context_if_as_an_expression_statement() {
    let source = "function run(){@if(ok){foo()}@else{bar()}}";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("statement @if");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let function_body = object_field(tape, function, "body");
    require_type(tape, function_body, "BlockStatement");
    assert_eq!(span(tape, function_body), (14, 42));

    let statement = one_object(&list_field(tape, function_body, "body"));
    require_type(tape, statement, "ExpressionStatement");
    assert_eq!(span(tape, statement), (15, 41));
    let if_node = object_field(tape, statement, "expression");
    assert_if_head(tape, if_node, (15, 41), (19, 21));

    let consequent = object_field(tape, if_node, "consequent");
    require_type(tape, consequent, "BlockStatement");
    assert_eq!(span(tape, consequent), (22, 29));
    assert_empty_path(tape, consequent);
    let alternate = object_field(tape, if_node, "alternate");
    require_type(tape, alternate, "BlockStatement");
    assert_eq!(span(tape, alternate), (34, 41));
    assert_empty_path(tape, alternate);
    assert_no_scaffold(tape);
}

#[test]
fn reconstructs_expression_context_if_directly_as_the_initializer() {
    let source = "const value=@if(ok){one}@else{two};";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("expression @if");
    let tape = result.program();
    let declaration = one_object(&program_body(tape));
    require_type(tape, declaration, "VariableDeclaration");
    let declarator = one_object(&list_field(tape, declaration, "declarations"));
    let if_node = object_field(tape, declarator, "init");
    assert_if_head(tape, if_node, (12, 34), (16, 18));

    let consequent = object_field(tape, if_node, "consequent");
    assert_eq!(span(tape, consequent), (19, 24));
    let expression = one_object(&list_field(tape, consequent, "body"));
    require_type(tape, expression, "ExpressionStatement");
    let alternate = object_field(tape, if_node, "alternate");
    assert_eq!(span(tape, alternate), (29, 34));
    assert_no_scaffold(tape);
}

#[test]
fn reconstructs_jsx_child_if_directly_in_children() {
    let source = "function View() @{<main>@if(ok){<b/>}@else{<i/>}</main>}";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("JSX-child @if");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    require_type(tape, code_block, "JSXCodeBlock");
    let render = object_field(tape, code_block, "render");
    let if_node = one_object(&list_field(tape, render, "children"));
    assert_if_head(tape, if_node, (24, 48), (28, 30));

    let consequent = object_field(tape, if_node, "consequent");
    let child = one_object(&list_field(tape, consequent, "body"));
    require_type(tape, child, "JSXElement");
    assert_eq!(span(tape, child), (32, 36));
    let alternate = object_field(tape, if_node, "alternate");
    let child = one_object(&list_field(tape, alternate, "body"));
    require_type(tape, child, "JSXElement");
    assert_eq!(span(tape, child), (43, 47));
    assert_no_scaffold(tape);
}

#[test]
fn preserves_else_if_as_a_nested_standard_if_statement() {
    let source = "function View() @{<main>@if(a){<a/>}@else if(b){<b/>}@else{<c/>}</main>}";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("chained @else if");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    let render = object_field(tape, code_block, "render");
    let if_node = one_object(&list_field(tape, render, "children"));
    assert_if_head(tape, if_node, (24, 64), (28, 29));

    let alternate = object_field(tape, if_node, "alternate");
    require_type(tape, alternate, "IfStatement");
    assert_eq!(span(tape, alternate), (42, 64));
    assert_eq!(span(tape, object_field(tape, alternate, "test")), (45, 46));
    assert_eq!(span(tape, object_field(tape, alternate, "consequent")), (47, 53));
    assert_eq!(span(tape, object_field(tape, alternate, "alternate")), (58, 64));
    assert_no_scaffold(tape);
}

#[test]
fn preserves_a_missing_alternate_as_explicit_null() {
    let source = "const value=@if(ok){one};";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("if without else");
    let tape = result.program();
    let declaration = one_object(&program_body(tape));
    let declarator = one_object(&list_field(tape, declaration, "declarations"));
    let if_node = object_field(tape, declarator, "init");
    assert_if_head(tape, if_node, (12, 24), (16, 18));
    assert_eq!(tape.scalar(field(tape, if_node, "alternate")), Some("null"));
    assert_no_scaffold(tape);
}

#[test]
fn reconstructs_nested_jsx_child_ifs_in_source_order() {
    let source = "function View() @{<main>@if(a){@if(b){<b/>}@else{<i/>}}@else{<u/>}</main>}";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("nested JSX-child ifs");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    let render = object_field(tape, code_block, "render");
    let outer_if = one_object(&list_field(tape, render, "children"));
    assert_if_head(tape, outer_if, (24, 66), (28, 29));
    let outer_consequent = object_field(tape, outer_if, "consequent");
    assert_eq!(span(tape, outer_consequent), (30, 55));

    let inner_if = one_object(&list_field(tape, outer_consequent, "body"));
    assert_if_head(tape, inner_if, (31, 54), (35, 36));
    let inner_consequent = object_field(tape, inner_if, "consequent");
    assert_eq!(span(tape, inner_consequent), (37, 43));
    let inner_alternate = object_field(tape, inner_if, "alternate");
    assert_eq!(span(tape, inner_alternate), (48, 54));
    let outer_alternate = object_field(tape, outer_if, "alternate");
    assert_eq!(span(tape, outer_alternate), (60, 66));
    assert_no_scaffold(tape);
}

#[test]
fn promotes_a_terminal_statement_if_to_code_block_render() {
    let source = "function View() @{ @if (ok) { <main /> } }";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("terminal statement if");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    require_type(tape, code_block, "JSXCodeBlock");
    assert!(list_field(tape, code_block, "body").is_empty());
    let if_node = object_field(tape, code_block, "render");
    assert_if_head(tape, if_node, (19, 40), (24, 26));
    assert_no_scaffold(tape);
}
