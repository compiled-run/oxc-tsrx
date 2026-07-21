use tsrx_parser_engine::{
    TsrxParseOptions, TsrxParseRequest, parse_tsrx, parse_tsrx_with_options,
    parse_tsrx_with_options_for_compat_transfer,
};
use tsrx_tape_schema::{
    Completeness, CoordinateDomain, DiagnosticPhase, FlatTape, ParseCompleteness,
    ProjectedCommentKind, RecordIndex, ValueKind,
};

fn scalar_u32(tape: &FlatTape, object: RecordIndex, name: &str) -> u32 {
    tape.field_index(object, name)
        .and_then(|field| tape.field_value(field))
        .and_then(|value| tape.scalar_u32(value))
        .unwrap_or_else(|| panic!("missing numeric `{name}`"))
}

fn assert_reachable_ranges_match_spans(tape: &FlatTape) {
    let mut objects = vec![false; tape.object_count()];
    let mut lists = vec![false; tape.list_count()];
    let mut pending = vec![tape.root()];
    while let Some(value) = pending.pop() {
        match value.kind() {
            ValueKind::Missing | ValueKind::Scalar => {}
            ValueKind::Object => {
                let object = value.as_object().expect("object index");
                let index = object.get().expect("object value") as usize;
                if std::mem::replace(&mut objects[index], true) {
                    continue;
                }
                if tape.field_index(object, "start").is_some() {
                    let range = tape
                        .field_index(object, "range")
                        .and_then(|field| tape.field_value(field))
                        .and_then(tsrx_tape_schema::ValueRef::as_list)
                        .expect("spanned object range");
                    let values = tape.values(range).collect::<Vec<_>>();
                    assert_eq!(values.len(), 2);
                    assert_eq!(
                        tape.scalar_u32(values[0]),
                        Some(scalar_u32(tape, object, "start"))
                    );
                    assert_eq!(
                        tape.scalar_u32(values[1]),
                        Some(scalar_u32(tape, object, "end"))
                    );
                }
                pending.extend(tape.fields(object).map(|field| field.value));
            }
            ValueKind::List => {
                let list = value.as_list().expect("list index");
                let index = list.get().expect("list value") as usize;
                if !std::mem::replace(&mut lists[index], true) {
                    pending.extend(tape.values(list));
                }
            }
        }
    }
}

fn complete(source: &str) -> tsrx_parser_engine::TsrxParseResult {
    let result = parse_tsrx(&TsrxParseRequest { source }).expect("canonical parse result");
    assert_eq!(result.status, ParseCompleteness::Complete);
    assert_eq!(
        result.coordinate_domain,
        CoordinateDomain::AuthoredUtf8Bytes
    );
    assert!(result.program.is_some());
    assert!(result.module.is_some());
    assert!(result.completeness.contains(Completeness::COMPLETE));
    assert!(result.completeness.contains(Completeness::HAS_PROGRAM));
    assert!(result.completeness.contains(Completeness::HAS_MODULE));
    result
}

#[test]
fn authored_module_and_comments_survive_while_projection_records_do_not() {
    let source = "/*top*/ import type { T } from 'types'; export function View() @{ @if(ok){/*yes*/<main/>}@else{<aside/>} } export { View }; const d=import('dyn'); const m=import.meta; //tail\n";
    let result = complete(source);
    let module = result.module.as_ref().expect("module table");
    assert!(module.has_module_syntax());
    assert_eq!(module.static_imports().len(), 1);
    assert_eq!(module.static_exports().len(), 2);
    assert_eq!(module.dynamic_imports().len(), 1);
    assert_eq!(module.import_metas().len(), 1);
    assert!(
        !module
            .string_storage()
            .expect("UTF-8 module storage")
            .contains("_t")
    );

    let comments = result.comments.records();
    assert_eq!(comments.len(), 3);
    assert_eq!(comments[0].kind, ProjectedCommentKind::Block);
    assert_eq!(result.comments.value(&comments[0]), Some("top"));
    assert_eq!(result.comments.value(&comments[1]), Some("yes"));
    assert_eq!(result.comments.value(&comments[2]), Some("tail"));
    for comment in comments {
        let start = usize::try_from(comment.span.start).expect("start");
        let end = usize::try_from(comment.span.end).expect("end");
        let spelling = &source[start..end];
        assert!(spelling.starts_with("//") || spelling.starts_with("/*"));
    }
    assert!(
        !result
            .comments
            .string_storage()
            .expect("UTF-8 comment storage")
            .contains("_t")
    );
}

#[test]
fn compatibility_transfer_omits_only_the_unobserved_module_table() {
    let source =
        "/*top*/ import { value } from 'dep'; export function View() @{ @if(value){<main/>} }";
    let options = TsrxParseOptions {
        source_type: Some("module"),
        preserve_parens: Some(false),
        ..TsrxParseOptions::default()
    };
    let full = parse_tsrx_with_options(&TsrxParseRequest { source }, options).expect("full parse");
    let lean = parse_tsrx_with_options_for_compat_transfer(&TsrxParseRequest { source }, options)
        .expect("compatibility parse");

    assert!(full.module.is_some());
    assert!(full.completeness.contains(Completeness::HAS_MODULE));
    assert!(lean.module.is_none());
    assert!(!lean.completeness.contains(Completeness::HAS_MODULE));
    assert_eq!(lean.comments.records(), full.comments.records());
    assert_eq!(
        lean.program
            .expect("lean Program")
            .program_transfer_engine_owned()
            .expect("lean transfer"),
        full.program
            .expect("full Program")
            .program_transfer_owned()
            .expect("full transfer"),
    );
}

#[test]
fn recordless_typescript_module_syntax_preserves_the_pinned_oxc_flag() {
    for source in [
        "const value=1; export = value; function View() @{ <main/> }",
        "export as namespace App; function View() @{ <main/> }",
    ] {
        let result = complete(source);
        let module = result.module.as_ref().expect("module table");
        assert!(
            module.has_module_syntax(),
            "recordless module syntax: {source}"
        );
        assert!(module.static_imports().is_empty());
        assert!(module.static_exports().is_empty());
        assert!(module.import_metas().is_empty());
    }
}

#[test]
fn wide_reexports_share_one_packed_module_request() {
    use std::fmt::Write as _;

    const ENTRIES: usize = 128;
    let request = "module-".repeat(512);
    let mut source = String::from("export {");
    for index in 0..ENTRIES {
        write!(&mut source, " value as exported{index},").expect("re-export entry");
    }
    write!(&mut source, "}} from '{request}';").expect("re-export request");
    source.push_str(" function View() @{ <main/> }");

    let result = complete(&source);
    let module = result.module.as_ref().expect("module table");
    assert_eq!(module.static_exports().len(), 1);
    let entries = module
        .static_export_entries(module.static_exports()[0].entries)
        .expect("re-export entries");
    assert_eq!(entries.len(), ENTRIES);
    let first = entries[0]
        .module_request
        .get()
        .expect("first module request")
        .value;
    assert!(entries.iter().all(|entry| {
        entry
            .module_request
            .get()
            .is_some_and(|value| value.value == first)
    }));
    assert_eq!(module.string(first), Some(request.as_str()));
    assert_eq!(
        module
            .string_storage()
            .expect("UTF-8 module storage")
            .matches(&request)
            .count(),
        1
    );
}

#[test]
fn css_comments_and_every_projection_marker_are_suppressed() {
    let source =
        "const x=<{tag}>{/*body*/}x</{/*close*/tag}>;const y=<style>/*css*/a{b:c}</style>;//tail\n";
    let result = complete(source);
    let values = result
        .comments
        .records()
        .iter()
        .map(|comment| result.comments.value(comment).expect("comment value"))
        .collect::<Vec<_>>();
    assert_eq!(values, ["body", "close", "tail"]);
    let comment_storage = result
        .comments
        .string_storage()
        .expect("UTF-8 comment storage");
    assert!(!comment_storage.contains("css"));
    assert!(!comment_storage.contains("_t"));
}

#[test]
fn fatal_javascript_and_tsrx_grammar_are_returned_as_failed_results() {
    for (source, expected_comments) in [
        ("/*js*/ export const broken = ; //after", &["js"][..]),
        (
            "/*before*/ function View() @{ @if(ok){<main/>} //tail\n",
            &["before", "tail"][..],
        ),
    ] {
        let result = parse_tsrx(&TsrxParseRequest { source }).expect("grammar result data");
        assert_eq!(result.status, ParseCompleteness::Failed);
        assert!(result.program.is_none());
        assert!(result.module.is_none());
        assert!(!result.errors.is_empty());
        assert!(!result.completeness.contains(Completeness::COMPLETE));
        assert!(!result.completeness.contains(Completeness::HAS_PROGRAM));
        assert!(!result.completeness.contains(Completeness::HAS_MODULE));
        assert!(result.completeness.contains(Completeness::HAS_COMMENTS));
        assert!(result.completeness.contains(Completeness::HAS_ERRORS));
        let comments = result
            .comments
            .records()
            .iter()
            .map(|comment| result.comments.value(comment).expect("grammar comment"))
            .collect::<Vec<_>>();
        assert_eq!(comments, expected_comments);
        for error in result.errors.records() {
            assert_eq!(error.phase, DiagnosticPhase::Grammar);
            let codeframe = result
                .errors
                .optional_string(error.codeframe)
                .expect("authored grammar codeframe");
            assert!(codeframe.contains("input.tsrx"));
            assert!(codeframe.contains(source));
            assert!(
                !result
                    .errors
                    .labels(error.labels)
                    .expect("grammar labels")
                    .is_empty()
            );
        }
    }
}

#[test]
fn optional_semantic_diagnostics_remain_complete_and_use_authored_spans() {
    let source = "function View() @{ let x; let x; <main/> }";
    let result = parse_tsrx_with_options(
        &TsrxParseRequest { source },
        TsrxParseOptions {
            filename: "Semantic.tsrx",
            show_semantic_errors: true,
            ..TsrxParseOptions::default()
        },
    )
    .expect("semantic result");
    assert_eq!(result.status, ParseCompleteness::Complete);
    assert!(result.program.is_some());
    assert!(result.module.is_some());
    let error = result
        .errors
        .records()
        .iter()
        .find(|error| error.phase == DiagnosticPhase::Semantic)
        .expect("duplicate binding diagnostic");
    assert_eq!(
        result.errors.string(error.message),
        Some("Identifier `x` has already been declared")
    );
    for label in result.errors.labels(error.labels).expect("labels") {
        let slice = &source[label.span.start as usize..label.span.end as usize];
        assert_eq!(slice, "x");
    }
    let codeframe = result
        .errors
        .optional_string(error.codeframe)
        .expect("authored semantic codeframe");
    assert!(codeframe.contains("Semantic.tsrx"));
    assert!(codeframe.contains(source));
}

#[test]
fn parser_options_reach_custom_and_ordinary_tsrx_nodes() {
    let source = "const value: number = (1); function View() @{ @if(ok){<main/>} }";
    let defaults = parse_tsrx(&TsrxParseRequest { source }).expect("default options");
    assert!(
        defaults
            .program()
            .scalar_storage()
            .contains(r#""ParenthesizedExpression""#)
    );

    let configured = parse_tsrx_with_options(
        &TsrxParseRequest { source },
        TsrxParseOptions {
            filename: "Options.tsrx",
            include_ts_fields: true,
            ranges: true,
            preserve_parens: Some(false),
            ..TsrxParseOptions::default()
        },
    )
    .expect("configured options");
    assert!(
        !configured
            .program()
            .scalar_storage()
            .contains(r#""ParenthesizedExpression""#)
    );
    assert!(configured.program().field_count() > defaults.program().field_count());
    assert_reachable_ranges_match_spans(configured.program());
}

#[test]
fn wide_result_tables_are_source_ordered_and_scaffold_free() {
    const COUNT: usize = 512;
    let mut source = String::new();
    for index in 0..COUNT {
        use std::fmt::Write as _;
        writeln!(
            &mut source,
            "import {{ v{index} }} from 'm{index}'; /*c{index}*/ export const e{index}=v{index};"
        )
        .expect("wide fixture");
    }
    source.push_str("function View() @{ @if(ok){<main/>}@else{<aside/>} }");
    let result = complete(&source);
    let module = result.module.as_ref().expect("wide module");
    assert_eq!(module.static_imports().len(), COUNT);
    assert_eq!(module.static_exports().len(), COUNT);
    assert_eq!(result.comments.len(), COUNT);
    assert!(
        module
            .static_imports()
            .windows(2)
            .all(|pair| pair[0].span.start < pair[1].span.start)
    );
    assert!(
        result
            .comments
            .records()
            .windows(2)
            .all(|pair| pair[0].span.start < pair[1].span.start)
    );
    assert!(
        !module
            .string_storage()
            .expect("UTF-8 module storage")
            .contains("_t")
    );
}
