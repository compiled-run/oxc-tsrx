mod support;

use std::fmt::Write as _;

use support::{
    assert_empty_path, assert_failed, assert_no_scaffold, field, list_field, object_field,
    one_object, optional_field, program_body, require_type, scalar_field, span,
};
use tsrx_parser_engine::{TsrxParseOptions, TsrxParseRequest, parse_tsrx, parse_tsrx_with_options};
use tsrx_tape_schema::{FlatTape, RecordIndex};

fn field_names(tape: &FlatTape, object: RecordIndex) -> Vec<&str> {
    tape.fields(object).map(|record| tape.key(record)).collect()
}

fn initializer(tape: &FlatTape) -> RecordIndex {
    let declaration = one_object(&program_body(tape));
    let declarator = one_object(&list_field(tape, declaration, "declarations"));
    object_field(tape, declarator, "init")
}

fn dynamic_parts(
    tape: &FlatTape,
    element: RecordIndex,
) -> (RecordIndex, RecordIndex, Option<(RecordIndex, RecordIndex)>) {
    require_type(tape, element, "JSXElement");
    let opening = object_field(tape, element, "openingElement");
    let opening_name = object_field(tape, opening, "name");
    let closing = field(tape, element, "closingElement");
    let closing = if tape.scalar(closing) == Some("null") {
        None
    } else {
        let closing = closing.as_object().expect("closing element object");
        Some((closing, object_field(tape, closing, "name")))
    };
    (opening, opening_name, closing)
}

fn assert_dynamic_name(
    tape: &FlatTape,
    name: RecordIndex,
    expected_span: (u32, u32),
    expression_type: &str,
    expression_span: (u32, u32),
) -> RecordIndex {
    require_type(tape, name, "JSXExpressionContainer");
    assert_eq!(span(tape, name), expected_span);
    assert_eq!(field_names(tape, name), ["type", "start", "end", "expression", "isDynamic"]);
    assert_eq!(scalar_field(tape, name, "isDynamic"), "true");
    let expression = object_field(tape, name, "expression");
    require_type(tape, expression, expression_type);
    assert_eq!(span(tape, expression), expression_span);
    expression
}

#[test]
fn reconstructs_self_closing_dynamic_tag_with_exact_canonical_shape() {
    let source = "const value=<{tag}/>;";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("self-closing dynamic JSX");
    let tape = result.program();
    let element = initializer(tape);
    assert_eq!(span(tape, element), (12, 20));
    assert_eq!(
        field_names(tape, element),
        [
            "type",
            "start",
            "end",
            "metadata",
            "children",
            "openingElement",
            "closingElement",
            "isDynamic",
        ]
    );
    assert_empty_path(tape, element);
    assert!(list_field(tape, element, "children").is_empty());
    assert_eq!(scalar_field(tape, element, "isDynamic"), "true");
    assert_eq!(tape.scalar(field(tape, element, "closingElement")), Some("null"));

    let (opening, name, closing) = dynamic_parts(tape, element);
    assert!(closing.is_none());
    assert_eq!(span(tape, opening), (12, 20));
    assert_eq!(
        field_names(tape, opening),
        ["type", "start", "end", "attributes", "name", "isDynamic", "selfClosing",]
    );
    assert!(list_field(tape, opening, "attributes").is_empty());
    assert_eq!(scalar_field(tape, opening, "isDynamic"), "true");
    assert_eq!(scalar_field(tape, opening, "selfClosing"), "true");
    let expression = assert_dynamic_name(tape, name, (13, 18), "Identifier", (14, 17));
    assert_eq!(scalar_field(tape, expression, "name"), r#""tag""#);
    assert_no_scaffold(tape);
}

#[test]
fn reconstructs_paired_dynamic_tag_with_distinct_authored_name_expressions() {
    let source = "const value=<{tag}>Hi</{tag}>;";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("paired dynamic JSX");
    let tape = result.program();
    let element = initializer(tape);
    assert_eq!(span(tape, element), (12, 29));
    let children = list_field(tape, element, "children");
    let text = one_object(&children);
    require_type(tape, text, "JSXText");
    assert_eq!(span(tape, text), (19, 21));

    let (opening, opening_name, closing) = dynamic_parts(tape, element);
    assert_eq!(span(tape, opening), (12, 19));
    let (closing, closing_name) = closing.expect("paired closing element");
    assert_eq!(span(tape, closing), (21, 29));
    assert_eq!(field_names(tape, closing), ["type", "start", "end", "name", "isDynamic"]);
    assert_eq!(scalar_field(tape, closing, "isDynamic"), "true");
    let opening_expression =
        assert_dynamic_name(tape, opening_name, (13, 18), "Identifier", (14, 17));
    let closing_expression =
        assert_dynamic_name(tape, closing_name, (23, 28), "Identifier", (24, 27));
    assert_ne!(opening_name, closing_name);
    assert_ne!(opening_expression, closing_expression);
    assert_no_scaffold(tape);

    let source = "const value=<{a, b}></{a, b}>;";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("paired sequence name");
    let tape = result.program();
    let (_, opening_name, closing) = dynamic_parts(tape, initializer(tape));
    let (_, closing_name) = closing.expect("paired sequence closing element");
    assert_dynamic_name(tape, opening_name, (13, 19), "SequenceExpression", (14, 18));
    assert_dynamic_name(tape, closing_name, (22, 28), "SequenceExpression", (23, 27));
    assert_no_scaffold(tape);
}

#[test]
fn reconstructs_dynamic_tags_when_parenthesis_nodes_are_disabled() {
    let source = "const value=<{tag}>Hi</{tag}>;";
    let result = parse_tsrx_with_options(
        &TsrxParseRequest { source },
        TsrxParseOptions { preserve_parens: Some(false), ..TsrxParseOptions::default() },
    )
    .expect("paired dynamic JSX without ParenthesizedExpression nodes");
    let tape = result.program();
    let (_, opening_name, closing) = dynamic_parts(tape, initializer(tape));
    let (_, closing_name) = closing.expect("paired closing element");
    assert_dynamic_name(tape, opening_name, (13, 18), "Identifier", (14, 17));
    assert_dynamic_name(tape, closing_name, (23, 28), "Identifier", (24, 27));
    assert_no_scaffold(tape);
}

#[test]
fn preserves_authored_attributes_and_children_around_dynamic_scaffolding() {
    let source =
        r#"const value=<{tag} id="a" count={n} disabled {...props}>Hi{value}<b/></{tag}>;"#;
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("rich dynamic JSX");
    let tape = result.program();
    let element = initializer(tape);
    assert_eq!(span(tape, element), (12, 77));
    let opening = object_field(tape, element, "openingElement");
    assert_eq!(span(tape, opening), (12, 56));
    let attributes = list_field(tape, opening, "attributes");
    assert_eq!(attributes.len(), 4);
    for (value, expected) in attributes.iter().zip([(19, 25), (26, 35), (36, 44), (45, 55)]) {
        assert_eq!(span(tape, value.as_object().expect("attribute object")), expected);
    }
    let id = attributes[0].as_object().expect("id attribute");
    let count = attributes[1].as_object().expect("count attribute");
    let disabled = attributes[2].as_object().expect("disabled attribute");
    let spread = attributes[3].as_object().expect("spread attribute");
    for (attribute, name) in [(id, "id"), (count, "count"), (disabled, "disabled")] {
        require_type(tape, attribute, "JSXAttribute");
        assert_eq!(
            scalar_field(tape, object_field(tape, attribute, "name"), "name"),
            format!(r#""{name}""#)
        );
    }
    require_type(tape, spread, "JSXSpreadAttribute");
    assert_eq!(scalar_field(tape, object_field(tape, spread, "argument"), "name"), r#""props""#);
    assert_eq!(tape.scalar(field(tape, disabled, "value")), Some("null"));
    require_type(tape, object_field(tape, count, "value"), "JSXExpressionContainer");
    let children = list_field(tape, element, "children");
    assert_eq!(children.len(), 3);
    for (value, kind, expected) in [
        (children[0], "JSXText", (56, 58)),
        (children[1], "JSXExpressionContainer", (58, 65)),
        (children[2], "JSXElement", (65, 69)),
    ] {
        let object = value.as_object().expect("child object");
        require_type(tape, object, kind);
        assert_eq!(span(tape, object), expected);
    }
    assert_no_scaffold(tape);

    let source = "const value=<{Outer} child={<{Inner} />} />;";
    let result = parse_tsrx(&TsrxParseRequest { source })
        .expect("self-closing dynamic tag with a nested dynamic attribute");
    let tape = result.program();
    let outer = initializer(tape);
    let opening = object_field(tape, outer, "openingElement");
    let attributes = list_field(tape, opening, "attributes");
    assert_eq!(attributes.len(), 1);
    let attribute = attributes[0].as_object().expect("nested attribute");
    let container = object_field(tape, attribute, "value");
    let inner = object_field(tape, container, "expression");
    require_type(tape, inner, "JSXElement");
    assert_eq!(scalar_field(tape, inner, "isDynamic"), "true");
    assert_no_scaffold(tape);
}

#[test]
fn accepts_the_complete_dynamic_expression_family_and_unwraps_outer_parens() {
    for (expression, kind) in [
        ("tag", "Identifier"),
        ("obj.new", "MemberExpression"),
        ("obj.Tag", "MemberExpression"),
        ("obj[key]", "MemberExpression"),
        ("obj?.Tag", "ChainExpression"),
        ("obj?.[key]", "ChainExpression"),
        ("(obj)[key]", "MemberExpression"),
        ("obj![key]", "MemberExpression"),
        ("ok ? A : B", "ConditionalExpression"),
        (r#""div""#, "Literal"),
        ("`div`", "TemplateLiteral"),
        ("-1", "UnaryExpression"),
        ("x = Tag", "AssignmentExpression"),
        ("x += Tag", "AssignmentExpression"),
        ("x++", "UpdateExpression"),
        ("++x", "UpdateExpression"),
        ("() => Tag", "ArrowFunctionExpression"),
        ("tag as any", "TSAsExpression"),
        ("a, b", "SequenceExpression"),
    ] {
        let source = format!("const x=<{{{expression}}}/>;");
        let result = parse_tsrx(&TsrxParseRequest { source: &source })
            .unwrap_or_else(|error| panic!("allowed `{expression}` failed: {error}"));
        let tape = result.program();
        let element = initializer(tape);
        let (_, name, _) = dynamic_parts(tape, element);
        let expression = object_field(tape, name, "expression");
        require_type(tape, expression, kind);
        assert_no_scaffold(tape);
    }

    let source = "const x=<{((tag))}/>;";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("outer parens");
    let tape = result.program();
    let (_, name, _) = dynamic_parts(tape, initializer(tape));
    assert_dynamic_name(tape, name, (9, 18), "Identifier", (12, 15));
    assert_no_scaffold(tape);
}

#[test]
fn preserves_distinct_equivalent_closing_spelling_and_closing_whitespace() {
    let source = "const x=<{((Tag))}></{Tag}>;";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("equivalent close spelling");
    let tape = result.program();
    let element = initializer(tape);
    assert_eq!(span(tape, element), (8, 27));
    let (_, opening_name, closing) = dynamic_parts(tape, element);
    let (_, closing_name) = closing.expect("closing element");
    let opening_expression =
        assert_dynamic_name(tape, opening_name, (9, 18), "Identifier", (12, 15));
    let closing_expression =
        assert_dynamic_name(tape, closing_name, (21, 26), "Identifier", (22, 25));
    assert_ne!(opening_name, closing_name);
    assert_ne!(opening_expression, closing_expression);
    assert_no_scaffold(tape);

    let source = "const x=<{tag}>x</{tag} >;";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("closing whitespace");
    let tape = result.program();
    let element = initializer(tape);
    let (_, _, closing) = dynamic_parts(tape, element);
    let (closing, name) = closing.expect("closing element");
    assert_eq!(span(tape, closing), (16, 25));
    assert_eq!(span(tape, name), (18, 23));
    assert_no_scaffold(tape);
}

#[test]
fn recursively_rejects_disallowed_dynamic_expression_shapes() {
    for expression in [
        "tag()",
        "ok ? tag() : Tag",
        "new Tag()",
        "({tag}).tag",
        "[Tag][0]",
        r#""a"+"b""#,
        "`a${tag}`",
        "tag`x`",
        "null as any",
        "/x/",
        "undefined",
        "undefined as any",
        "true as any",
        "void 0",
        "fn!()",
        "fn<string>()",
        "key in [Tag]",
        r"ok ? \u005ft0_W0_(Tag) : Fallback",
    ] {
        let source = format!("const x=<{{{expression}}}/>;");
        assert_failed(&source);
    }
}

#[test]
fn direct_control_roots_fail_closed_as_dynamic_tag_names() {
    for source in [
        "const x=<{@if(ok){Tag}@else{Fallback}}/>;",
        "const x=<{@for(item of items){item.Tag}@empty{Fallback}}/>;",
        "const x=<{@switch(kind){@case 0:{A}@default:{B}}}/>;",
        "const x=<{@try{A}@pending{B}@catch{C}}/>;",
    ] {
        assert_failed(source);
    }
}

#[test]
fn preserves_nested_dynamic_topology_and_composes_with_controls() {
    let source = "const x=<{outer}><{inner}>Hi</{inner}></{outer}>;";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("nested dynamics");
    let tape = result.program();
    let outer = initializer(tape);
    assert_eq!(span(tape, outer), (8, 48));
    let inner = one_object(&list_field(tape, outer, "children"));
    require_type(tape, inner, "JSXElement");
    assert_eq!(span(tape, inner), (17, 38));
    assert_eq!(scalar_field(tape, outer, "isDynamic"), "true");
    assert_eq!(scalar_field(tape, inner, "isDynamic"), "true");
    assert_no_scaffold(tape);

    let source = "function View() @{<{tag}>@if(ok){<{other}/>}@else{<i/>}</{tag}>}";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("dynamic/control composition");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let code_block = object_field(tape, function, "body");
    let outer = object_field(tape, code_block, "render");
    require_type(tape, outer, "JSXElement");
    assert_eq!(span(tape, outer), (18, 63));
    let control = one_object(&list_field(tape, outer, "children"));
    require_type(tape, control, "JSXIfExpression");
    let consequent = object_field(tape, control, "consequent");
    let inner = one_object(&list_field(tape, consequent, "body"));
    require_type(tape, inner, "JSXElement");
    assert_eq!(span(tape, inner), (33, 43));
    assert_no_scaffold(tape);
}

#[test]
fn accepts_only_the_supported_dynamic_closing_comment_protocol() {
    let source = "const x=<{tag}>x</{/*lead*/ tag /*tail*/}>;";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("edge closing comments");
    let tape = result.program();
    let element = initializer(tape);
    let (_, _, closing) = dynamic_parts(tape, element);
    let (_, name) = closing.expect("closing element");
    assert_dynamic_name(tape, name, (18, 41), "Identifier", (28, 31));
    assert_no_scaffold(tape);

    let source = "const x=<{tag}>x</{\n//lead\ntag\n//tail\n}>;";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("line edge comments");
    let tape = result.program();
    let element = initializer(tape);
    let (_, _, closing) = dynamic_parts(tape, element);
    let (_, name) = closing.expect("closing element");
    let expression = object_field(tape, name, "expression");
    require_type(tape, expression, "Identifier");
    assert_no_scaffold(tape);

    let source = "const x=<{Outer}><{Inner}>x</{/*i*/ Inner}></{/*o*/ Outer}>;";
    let result = parse_tsrx(&TsrxParseRequest { source })
        .expect("nested dynamic elements with closing-edge comments");
    assert_no_scaffold(result.program());

    for malformed in [
        "const x=<{tag}>x</{other}>;",
        "const x=<{obj.Tag}>x</{obj . Tag}>;",
        "const x=<{tag}>x;",
        "const x=<{tag/>;",
        "const x=<{tag}/>x</{tag}>;",
        "const x=<{tag}>x</{/*only*/}>;",
        "const x=<{tag}>x</{tag} /*outside*/>;",
    ] {
        assert_failed(malformed);
    }
}

#[test]
fn supports_dynamic_parent_placements_and_keeps_static_jsx_ordinary() {
    for source in [
        "function run(){return <{tag}/>;}",
        "render(<{tag}/>);",
        "const x=[<{tag}/>];",
        "const x=<main><{tag}/></main>;",
        "const x=<main>{<{tag}/>}</main>;",
        "function View() @{ <{tag}/> }",
        "function run(){<{tag}/>;}",
    ] {
        let result = parse_tsrx(&TsrxParseRequest { source })
            .unwrap_or_else(|error| panic!("dynamic placement failed for `{source}`: {error}"));
        assert_no_scaffold(result.program());
    }

    let source = "function run(){<{tag}/>;}";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("statement semicolon topology");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let body = object_field(tape, function, "body");
    let statements = list_field(tape, body, "body");
    assert_eq!(statements.len(), 2);
    let dynamic = statements[0].as_object().expect("dynamic statement child");
    let semicolon = statements[1].as_object().expect("semicolon empty statement");
    require_type(tape, dynamic, "JSXElement");
    require_type(tape, semicolon, "EmptyStatement");
    assert_eq!(span(tape, dynamic), (15, 23));
    assert_eq!(span(tape, semicolon), (23, 24));
    assert_no_scaffold(tape);

    let source = "const x=<{tag}><div/></{tag}>;";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("static child isolation");
    let tape = result.program();
    let dynamic = initializer(tape);
    let static_child = one_object(&list_field(tape, dynamic, "children"));
    require_type(tape, static_child, "JSXElement");
    assert!(optional_field(tape, static_child, "isDynamic").is_none());
    let static_opening = object_field(tape, static_child, "openingElement");
    assert!(optional_field(tape, static_opening, "isDynamic").is_none());
    assert_no_scaffold(tape);
}

#[test]
fn composes_dynamic_tags_with_all_control_families_and_wide_siblings() {
    let source = concat!(
        "function View() @{<main>@switch(kind){@case 1:{<{A}/>}@default:{",
        "@try{<{B}/>}@catch{<{C}/>}}}</main>}"
    );
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("switch/try dynamics");
    assert_no_scaffold(result.program());

    let source = concat!(
        "function View() @{<main>@for(item of items){<{item.Tag}/>}@empty{",
        "<{fallback}/>}</main>}"
    );
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("for dynamics");
    assert_no_scaffold(result.program());

    let mut source = String::from("const x=<main>");
    for index in 0..128 {
        write!(source, "<{{tags[{index}]}}/>").expect("writing to a String cannot fail");
    }
    source.push_str("</main>;");
    let result = parse_tsrx(&TsrxParseRequest { source: &source }).expect("wide dynamics");
    assert_no_scaffold(result.program());
}

#[test]
fn composes_all_control_families_inside_jsx_expression_containers() {
    let controls = [
        "@if(ok){A}@else{B}",
        "@for(item of items){item.Tag}@empty{Fallback}",
        "@switch(kind){@case 0:{A}@default:{B}}",
        "@try{A}@pending{B}@catch{C}",
    ];

    for control in controls {
        for source in [
            format!("const x=<main child={{{control}}}/>;"),
            format!("const x=<{{Outer}} child={{{control}}}/>;"),
            format!("const x=<main>{{{control}}}</main>;"),
        ] {
            let result = parse_tsrx(&TsrxParseRequest { source: &source })
                .unwrap_or_else(|error| panic!("container control failed for `{source}`: {error}"));
            assert_no_scaffold(result.program());
        }
    }
}

#[test]
fn collision_free_parser_scaffold_never_escapes() {
    let source = r#"const marker="_t0_"; const x=<{tag}/>;"#;
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("prefix collision");
    assert!(result.program().scalar_storage().contains(r#""_t0_""#));
    assert!(!result.program().scalar_storage().contains("_t1_D0"));
    assert!(!result.program().scalar_storage().contains("_t1_A0_"));
    assert!(!result.program().scalar_storage().contains("_t1_Z0_"));
}

#[test]
fn decoded_authored_scalars_cannot_be_mistaken_for_parser_scaffolding() {
    let source = r#"const x=<{"\u005ft0_"}/>;"#;
    let result = parse_tsrx(&TsrxParseRequest { source })
        .expect("decoded authored string equal to the first scaffold prefix");
    let tape = result.program();
    let (_, name, _) = dynamic_parts(tape, initializer(tape));
    let expression = object_field(tape, name, "expression");
    require_type(tape, expression, "Literal");
    assert_eq!(scalar_field(tape, expression, "value"), r#""_t0_""#);
}

#[test]
fn preserves_canonical_semicolon_topology_after_bare_dynamic_statements() {
    for (source, semicolon_span) in
        [("function run(){<{tag}/> ;}", (24, 25)), ("function run(){<{tag}/>\n  ;}", (26, 27))]
    {
        let result = parse_tsrx(&TsrxParseRequest { source })
            .unwrap_or_else(|error| panic!("spaced semicolon failed for `{source}`: {error}"));
        let tape = result.program();
        let function = one_object(&program_body(tape));
        let block = object_field(tape, function, "body");
        let body = list_field(tape, block, "body");
        assert_eq!(body.len(), 2);
        let dynamic = body[0].as_object().expect("dynamic statement");
        require_type(tape, dynamic, "JSXElement");
        assert_eq!(span(tape, dynamic), (15, 23));
        let statement = body[1].as_object().expect("semicolon statement");
        require_type(tape, statement, "ExpressionStatement");
        assert_eq!(span(tape, statement), semicolon_span);
        let text = object_field(tape, statement, "expression");
        require_type(tape, text, "JSXText");
        assert_eq!(span(tape, text), semicolon_span);
        assert_eq!(scalar_field(tape, text, "value"), r#"";""#);
        assert_eq!(scalar_field(tape, text, "raw"), r#"";""#);
    }
}

#[test]
fn preserves_parenthesized_and_commented_dynamic_statement_topology() {
    let source = "function run(){(<{tag}/>);}";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("parenthesized dynamic statement");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let block = object_field(tape, function, "body");
    let body = list_field(tape, block, "body");
    assert_eq!(body.len(), 1);
    let statement = body[0].as_object().expect("expression statement");
    require_type(tape, statement, "ExpressionStatement");
    let expression = object_field(tape, statement, "expression");
    require_type(tape, expression, "JSXElement");
    assert_eq!(span(tape, expression), (16, 24));
    assert_no_scaffold(tape);

    let source = "function run(){<{tag}/> /*comment*/ ;}";
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("commented dynamic semicolon");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let block = object_field(tape, function, "body");
    let body = list_field(tape, block, "body");
    assert_eq!(body.len(), 2);
    require_type(tape, body[0].as_object().expect("dynamic statement"), "JSXElement");
    let statement = body[1].as_object().expect("semicolon statement");
    require_type(tape, statement, "ExpressionStatement");
    let text = object_field(tape, statement, "expression");
    require_type(tape, text, "JSXText");
    assert_eq!(span(tape, text), (36, 37));
    assert_eq!(scalar_field(tape, text, "value"), r#"";""#);
    assert_no_scaffold(tape);
}

#[test]
fn terminal_function_render_discards_its_optional_semicolon() {
    for source in [
        "function View() @{ <{tag}/>; }",
        "function View() @{ <{tag}/> ; }",
        "function View() @{ <{tag}/>\n  ; }",
    ] {
        let result = parse_tsrx(&TsrxParseRequest { source })
            .unwrap_or_else(|error| panic!("terminal render failed for `{source}`: {error}"));
        let tape = result.program();
        let function = one_object(&program_body(tape));
        let block = object_field(tape, function, "body");
        require_type(tape, block, "JSXCodeBlock");
        assert!(list_field(tape, block, "body").is_empty());
        let render = object_field(tape, block, "render");
        require_type(tape, render, "JSXElement");
        assert_eq!(span(tape, render), (19, 27));
    }
}

#[test]
fn supports_the_authoritative_labeled_dynamic_statement_placement() {
    for source in ["label:<{tag}/>;", "label:<{tag}/> \n ;"] {
        let result = parse_tsrx(&TsrxParseRequest { source })
            .unwrap_or_else(|error| panic!("labeled dynamic failed for `{source}`: {error}"));
        let tape = result.program();
        let body = program_body(tape);
        assert_eq!(body.len(), 2);
        let label = body[0].as_object().expect("label statement");
        require_type(tape, label, "LabeledStatement");
        let dynamic = object_field(tape, label, "body");
        require_type(tape, dynamic, "JSXElement");
        assert_eq!(span(tape, dynamic), (6, 14));
    }
}

#[test]
fn recursively_reconstructs_nested_tsrx_inside_dynamic_name_expressions() {
    let source = "const x=<{ok ? <{Tag}/> : Fallback}/>;";
    let result = parse_tsrx(&TsrxParseRequest { source })
        .expect("nested dynamic inside conditional name expression");
    let tape = result.program();
    let outer = initializer(tape);
    let (_, name, _) = dynamic_parts(tape, outer);
    let conditional = object_field(tape, name, "expression");
    require_type(tape, conditional, "ConditionalExpression");
    assert_eq!(span(tape, conditional), (10, 34));
    let nested = object_field(tape, conditional, "consequent");
    require_type(tape, nested, "JSXElement");
    assert_eq!(span(tape, nested), (15, 23));
    assert_eq!(scalar_field(tape, nested, "isDynamic"), "true");

    let source = "const x=<{() => <{Tag}/>}/>;";
    let result = parse_tsrx(&TsrxParseRequest { source })
        .expect("nested dynamic inside arrow name expression");
    let tape = result.program();
    let (_, name, _) = dynamic_parts(tape, initializer(tape));
    let arrow = object_field(tape, name, "expression");
    require_type(tape, arrow, "ArrowFunctionExpression");
    let nested = object_field(tape, arrow, "body");
    require_type(tape, nested, "JSXElement");
    assert_eq!(span(tape, nested), (16, 24));

    let source = "const x=<{() => <{Inner}/>}>body</{() => <{Inner}/>}>;";
    let result = parse_tsrx(&TsrxParseRequest { source })
        .expect("nested self-closing dynamics in both paired name expressions");
    let tape = result.program();
    let (_, opening_name, closing) = dynamic_parts(tape, initializer(tape));
    let (_, closing_name) = closing.expect("paired dynamic closing element");
    for name in [opening_name, closing_name] {
        let arrow = object_field(tape, name, "expression");
        require_type(tape, arrow, "ArrowFunctionExpression");
        let nested = object_field(tape, arrow, "body");
        require_type(tape, nested, "JSXElement");
        assert_eq!(scalar_field(tape, nested, "isDynamic"), "true");
    }
    assert_no_scaffold(tape);

    for (source, kind) in [
        ("const x=<{ok ? @if(foo){A}@else{B} : Fallback}/>;", "JSXIfExpression"),
        ("const x=<{ok ? @for(x of xs){x} : Fallback}/>;", "JSXForExpression"),
        ("const x=<{ok ? @try{A}@catch{B} : Fallback}/>;", "JSXTryExpression"),
    ] {
        let result = parse_tsrx(&TsrxParseRequest { source })
            .unwrap_or_else(|error| panic!("nested control failed for `{source}`: {error}"));
        let tape = result.program();
        let (_, name, _) = dynamic_parts(tape, initializer(tape));
        let conditional = object_field(tape, name, "expression");
        let consequent = object_field(tape, conditional, "consequent");
        require_type(tape, consequent, kind);
    }
}

#[test]
fn bare_dynamic_statement_siblings_remain_linear_and_ordered() {
    let mut source = String::from("function run(){");
    for index in 0..256 {
        write!(source, "<{{tags[{index}]}}/>;").expect("writing to a String cannot fail");
    }
    source.push('}');
    let result = parse_tsrx(&TsrxParseRequest { source: &source })
        .expect("wide bare dynamic statement list");
    let tape = result.program();
    let function = one_object(&program_body(tape));
    let block = object_field(tape, function, "body");
    let body = list_field(tape, block, "body");
    assert_eq!(body.len(), 512);
    for pair in body.chunks_exact(2) {
        require_type(tape, pair[0].as_object().expect("dynamic"), "JSXElement");
        require_type(tape, pair[1].as_object().expect("semicolon"), "EmptyStatement");
    }
}
