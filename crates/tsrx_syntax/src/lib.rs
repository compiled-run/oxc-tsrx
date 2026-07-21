//! Lossless, allocation-light TSRX recognition and legal-TSX projection.

mod model;
mod projection;
mod scanner;

pub use model::{
    ByteSpan, ClauseRole, ControlContext, ControlKind, EmbeddedKind, ForHeader, NONE_INDEX,
    Overlay, OverlayClause, OverlayDynamicTag, OverlayEmbedded, OverlayNode, OverlayStyleBlock,
    OverlayToken, OverlayView, ParserCodeBlock, ParserDynamicKind, ParserDynamicToken,
    ProjectionError, StructuralKind, StructuralToken,
};
pub use projection::{
    FormatProjection, MappedProjection, ProjectionSegment, ProjectionView, TypeProjection,
    lift_formatted, project, project_for_format, project_for_lint, project_for_parser,
    project_for_types,
};

pub use scanner::OpaqueSurrogateContext;
use scanner::Scanner;

/// Full result of the WTF-8 lexical proof, including any earlier authored grammar failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wtf8SurrogateClassification {
    pub contexts: Vec<Option<OpaqueSurrogateContext>>,
    pub earlier_error: Option<ProjectionError>,
}

/// Classifies pre-recorded three-byte WTF-8 lone-surrogate positions without passing them to OXC.
///
/// A `None` entry is syntactically active, ambiguous, or could not be proven before an earlier
/// grammar stop. Production callers must fail closed for every such entry.
#[must_use]
pub fn classify_wtf8_surrogates(
    source: &[u8],
    byte_offsets: &[u32],
) -> Vec<Option<OpaqueSurrogateContext>> {
    Scanner::new_for_surrogate_classification(source, byte_offsets).classify_surrogates()
}

/// Classifies WTF-8 surrogate probes while retaining an earlier structural scanner failure.
#[must_use]
pub fn classify_wtf8_surrogates_detailed(
    source: &[u8],
    byte_offsets: &[u32],
) -> Wtf8SurrogateClassification {
    let (contexts, earlier_error) = Scanner::new_for_surrogate_classification(source, byte_offsets)
        .classify_surrogates_detailed();
    Wtf8SurrogateClassification {
        contexts,
        earlier_error,
    }
}

/// Performs one byte-oriented structural scan and returns a compact overlay over `source`.
///
/// # Errors
///
/// Returns an error for malformed or unsupported TSRX, unterminated lexical constructs, and
/// sources beyond OXC's 32-bit span limit.
pub fn scan(source: &str) -> Result<Overlay, ProjectionError> {
    Scanner::new(source).finish()
}

/// Performs the parser-specific structural scan, including TSRX nested inside dynamic tag names.
///
/// The normal [`scan`] route remains unchanged for lint, format, and type projections. This
/// parser-only route adds flat source-ordered dynamic-name boundaries so overlapping authored
/// syntax can still be projected in one linear pass.
///
/// # Errors
///
/// Returns the same malformed, unsupported, unterminated, and source-size failures as [`scan`].
pub fn scan_for_parser(source: &str) -> Result<Overlay, ProjectionError> {
    Scanner::new_for_parser(source).finish()
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::{
        ProjectionError, StructuralKind, lift_formatted, project, project_for_format,
        project_for_lint, project_for_parser, project_for_types, scan, scan_for_parser,
    };

    #[test]
    fn equal_width_projection_masks_only_structural_sigils() {
        let source = "function View() @{ @if (ready) {} @else {} }";
        let overlay = scan(source).unwrap();
        assert_eq!(
            overlay
                .tokens()
                .iter()
                .map(|token| token.kind)
                .collect::<Vec<_>>(),
            [
                StructuralKind::FunctionBody,
                StructuralKind::If,
                StructuralKind::Else
            ]
        );
        let projected = project(source, &overlay).unwrap();
        assert_eq!(projected, "function View()  {  if (ready) {}  else {} }");
        assert_eq!(projected.len(), source.len());
    }

    #[test]
    fn protects_lexical_at_text_and_scans_interpolated_code() {
        let source = concat!(
            "import x from '@scope/pkg';\n",
            "const a = /@if\\s+\\//gu;\n",
            "const b = `@else ${(() => { @if (ready) {} })()}`;\n",
            "// @if (comment) {}\n",
            "function View() @{\n",
            "  <p title={`@if ${value}`}>@if is literal text</p>;\n",
            "}\n",
        );
        let overlay = scan(source).unwrap();
        assert_eq!(overlay.tokens().len(), 2);
    }

    #[test]
    fn recognizes_direct_jsx_and_expression_control_families() {
        let source = concat!(
            "function View() @{<main>@if(ok){<b/>}@else{<i/>}",
            "@for(const x of xs;index i;key x.id){<p>{i}</p>}@empty{<em/>}</main>}\n",
            "const result=@for await(const x of xs){x};\n",
        );
        let overlay = scan(source).unwrap();
        assert_eq!(overlay.control_count(), 3);
        assert_eq!(
            overlay
                .tokens()
                .iter()
                .map(|token| token.kind)
                .collect::<Vec<_>>(),
            [
                StructuralKind::FunctionBody,
                StructuralKind::If,
                StructuralKind::Else,
                StructuralKind::For,
                StructuralKind::Empty,
                StructuralKind::For,
            ]
        );
    }

    #[test]
    fn recognizes_switch_and_source_order_try_in_every_control_context() {
        let source = concat!(
            "function View() @{<main>@switch(value){@case 0:{@try{<b/>}@pending{<i/>}",
            "@catch(error:Error,reset:()=>void){<button onClick={reset}>{error.message}</button>}}",
            "@default:{<em/>}}</main>}\n",
            "const assigned=@switch(value){@case 1:{<b/>}};\n",
            "function run(){@try{work()}@catch{recover()};return consume(@try{work()}@pending{wait()});}\n",
        );
        let overlay = scan(source).unwrap();
        assert_eq!(overlay.control_count(), 5);
    }

    #[test]
    fn recognizes_dynamic_tags_and_raw_style_without_scanning_css_as_jsx() {
        let source = concat!(
            "function View({tag}:{tag:string}) @{<main>",
            "<{tag} class=\"card\">@if(ok){<b/>}</{ tag }>",
            "<style>/* <Fake> @if(x) {} */ .card { color: red; }</style>",
            "</main>}"
        );
        let overlay = scan(source).unwrap();
        assert_eq!(overlay.control_count(), 1);
        let projection = project_for_format(source, &overlay).unwrap();
        assert!(projection.source().contains("D0"));
        assert!(!projection.source().contains("<Fake>"));
        assert!(projection.source().contains("Z0_={null}"));
        assert!(projection.source().contains("S0__*/ null"));
    }

    #[test]
    fn type_projection_preserves_loop_bindings_and_identity_fix_boundaries() {
        let source = concat!(
            "type Item={id:string;save():Promise<void>};",
            "declare const items:Item[];",
            "function View() @{<main>@for(const item of items;index i;key item.id){",
            "@if(i>=0){item.save();<span>{item.id}</span>}}@empty{<i/>}</main>}"
        );
        let overlay = scan(source).unwrap();
        let projection = project_for_types(source, &overlay).unwrap();
        assert!(projection.source().contains("for(const item of items)"));
        assert!(projection.source().contains("let i = 0;"));
        assert!(projection.source().contains("void (item.id);"));
        assert!(
            projection
                .source()
                .contains("if (false) return null as any;")
        );

        let projected_start =
            u32::try_from(projection.source().find("item.save()").unwrap()).unwrap();
        let authored_start = u32::try_from(source.find("item.save()").unwrap()).unwrap();
        assert_eq!(
            projection.map_range(projected_start..projected_start + 11),
            Some(authored_start..authored_start + 11)
        );
        assert_eq!(
            projection.map_fix_range(projected_start..projected_start + 11),
            Some(authored_start..authored_start + 11)
        );
        let wrapper = u32::try_from(projection.source().find("W0_").unwrap()).unwrap();
        assert!(projection.map_fix_range(wrapper..wrapper + 3).is_none());
    }

    #[test]
    fn dynamic_tag_expression_is_affine_but_style_payload_is_synthetic() {
        let source =
            "function View({tag}:{tag:string}) @{<{tag}><style>.x{color:red}</style></{tag}>}";
        let overlay = scan(source).unwrap();
        let projection = project_for_lint(source, &overlay).unwrap();
        assert!(!projection.source().contains(".x{color:red}"));
        let projected_tag = projection.source().rfind("tag").unwrap();
        let mapped = projection
            .map_range(
                u32::try_from(projected_tag).unwrap()
                    ..u32::try_from(projected_tag + "tag".len()).unwrap(),
            )
            .unwrap();
        assert_eq!(mapped.start as usize, source.find("<{tag}").unwrap() + 2);
    }

    #[test]
    fn paired_dynamic_names_map_labels_but_not_one_sided_fixes() {
        for (source, fixable) in [
            ("function View() @{ <{tag} /> }", true),
            ("function View() @{ <{tag}></{tag}> }", false),
        ] {
            let projection = project_for_lint(source, &scan(source).unwrap()).unwrap();
            let marker = projection.source().find("A0_={tag").unwrap();
            let start = u32::try_from(marker + "A0_={".len()).unwrap();
            let range = start..start + 3;
            assert!(projection.map_range(range.clone()).is_some(), "{source}");
            assert_eq!(
                projection.map_fix_range(range).is_some(),
                fixable,
                "{source}"
            );
        }
    }

    #[test]
    fn parser_dynamic_projection_is_isolated_from_existing_tool_projections() {
        let source = "const value=<{a, b}>x</{/*lead*/ a, b /*tail*/}>;";
        let overlay = scan(source).unwrap();
        let lint = project_for_lint(source, &overlay).unwrap();
        let format = project_for_format(source, &overlay).unwrap();
        let types = project_for_types(source, &overlay).unwrap();
        let parser_overlay = scan_for_parser(source).unwrap();
        let parser = project_for_parser(source, &parser_overlay).unwrap();

        assert_eq!(lint.source(), format.source());
        assert!(types.source().starts_with(lint.source()));
        assert!(!lint.source().contains("C0_(("));
        assert!(lint.source().contains("Q0__"));
        assert!(lint.source().contains("Q1__"));
        assert!(parser.source().contains("A0_={(a, b)}"));
        assert!(parser.source().contains("C0_((/*lead*/ a, b /*tail*/))"));
        assert!(!parser.source().contains("Q0__"));
        assert_eq!(
            project_for_lint(source, &overlay).unwrap().source(),
            lint.source()
        );
    }

    #[test]
    fn parser_dynamic_root_controls_use_expression_context() {
        for (source, expected) in [
            ("const x=<{/*root*/ @if(ok){Tag}@else{Fallback}}/>;", "W0_"),
            (
                "const x=<{@for(item of items){item.Tag}@empty{Fallback}}/>;",
                "W0_",
            ),
            (
                "const x=<{@switch(kind){@case 0:{A}@default:{B}}}/>;",
                "W0_",
            ),
            ("const x=<{@try{A}@pending{B}@catch{C}}/>;", "T0_"),
        ] {
            let overlay = scan_for_parser(source)
                .unwrap_or_else(|error| panic!("parser scan failed for `{source}`: {error}"));
            assert!(
                overlay
                    .view()
                    .nodes
                    .iter()
                    .all(|node| node.context == super::ControlContext::Expression),
                "root control was not classified as an expression for `{source}`"
            );
            let projected = project_for_parser(source, &overlay)
                .unwrap_or_else(|error| panic!("parser projection failed for `{source}`: {error}"));
            assert!(projected.source().contains(expected), "{source}");
        }
    }

    #[test]
    fn parser_dynamic_controls_inside_arrow_blocks_keep_statement_context() {
        let source = "const x=<{() => { @if(ok){return Tag} return Fallback }}/>;";
        let overlay = scan_for_parser(source).expect("arrow-block parser scan");
        assert_eq!(overlay.view().nodes.len(), 1);
        assert_eq!(
            overlay.view().nodes[0].context,
            super::ControlContext::Statement
        );
        project_for_parser(source, &overlay).expect("arrow-block parser projection");

        let source = "const x=<{() => <{@if(ok){Tag}@else{Fallback}}/>}/>;";
        let overlay = scan_for_parser(source).expect("nested dynamic parser scan");
        assert_eq!(overlay.dynamic_tag_count(), 2);
        assert_eq!(overlay.view().nodes.len(), 1);
        assert_eq!(
            overlay.view().nodes[0].context,
            super::ControlContext::Expression
        );
        project_for_parser(source, &overlay).expect("nested dynamic parser projection");
    }

    #[test]
    fn jsx_expression_containers_give_direct_root_controls_expression_context() {
        let controls = [
            "@if(ok){A}@else{B}",
            "@for(item of items){item.Tag}@empty{Fallback}",
            "@switch(kind){@case 0:{A}@default:{B}}",
            "@try{A}@pending{B}@catch{C}",
        ];

        for control in controls {
            for source in [
                format!("const x=<main child={{/*root*/ {control}}}/>;"),
                format!("const x=<{{Outer}} child={{/*root*/ {control}}}/>;"),
                format!("const x=<main>{{/*root*/ {control}}}</main>;"),
            ] {
                let overlay = scan_for_parser(&source)
                    .unwrap_or_else(|error| panic!("parser scan failed for `{source}`: {error}"));
                assert!(
                    overlay
                        .view()
                        .nodes
                        .iter()
                        .all(|node| node.context == super::ControlContext::Expression),
                    "container-root control was not classified as an expression for `{source}`"
                );
                project_for_parser(&source, &overlay).unwrap_or_else(|error| {
                    panic!("parser projection failed for `{source}`: {error}")
                });
            }
        }
    }

    #[test]
    fn parser_dynamic_overlays_cannot_cross_projection_lanes() {
        let source = "const x=<{outer}><{inner}/></{outer}>;";
        let ordinary = scan(source).expect("ordinary scan");
        assert!(matches!(
            project_for_parser(source, &ordinary),
            Err(ProjectionError::StructuralMismatch)
        ));

        let parser = scan_for_parser(source).expect("parser scan");
        assert!(matches!(
            project_for_lint(source, &parser),
            Err(ProjectionError::StructuralMismatch)
        ));
        assert!(matches!(
            project_for_format(source, &parser),
            Err(ProjectionError::StructuralMismatch)
        ));
        assert!(matches!(
            project_for_types(source, &parser),
            Err(ProjectionError::StructuralMismatch)
        ));

        let style_source = "const x=<style/>;";
        let parser_style =
            scan_for_parser(style_source).expect("parser-only self-closing style scan");
        assert_eq!(parser_style.style_block_count(), 1);
        assert!(matches!(
            project_for_lint(style_source, &parser_style),
            Err(ProjectionError::StructuralMismatch)
        ));
        assert!(matches!(
            project_for_format(style_source, &parser_style),
            Err(ProjectionError::StructuralMismatch)
        ));
        assert!(matches!(
            project_for_types(style_source, &parser_style),
            Err(ProjectionError::StructuralMismatch)
        ));
        project_for_parser(style_source, &parser_style).expect("parser style stays in parser lane");

        let mut crossing = parser.clone();
        crossing.parser_dynamic_tokens.swap(0, 1);
        assert!(matches!(
            project_for_parser(source, &crossing),
            Err(ProjectionError::StructuralMismatch)
        ));

        let mut bad_subtree = parser;
        bad_subtree.dynamic_tags[0].subtree_end = 0;
        assert!(matches!(
            project_for_parser(source, &bad_subtree),
            Err(ProjectionError::StructuralMismatch)
        ));

        let source = "const x=<><{Outer} child={<{Inner}/>}/><{Sibling}/></>;";
        let parser = scan_for_parser(source).expect("nested attribute and sibling scan");
        assert_eq!(
            parser
                .view()
                .dynamic_tags
                .iter()
                .map(|tag| tag.subtree_end)
                .collect::<Vec<_>>(),
            [2, 2, 3]
        );
        assert!(parser.view().dynamic_tags[0].closing.is_empty());
        assert!(
            parser.view().dynamic_tags[0].closing.end > parser.view().dynamic_tags[0].opening.end
        );
        project_for_parser(source, &parser).expect("valid nested attribute subtree bounds");

        let mut swallowed_sibling = parser.clone();
        swallowed_sibling.dynamic_tags[0].subtree_end = 3;
        assert!(matches!(
            project_for_parser(source, &swallowed_sibling),
            Err(ProjectionError::StructuralMismatch)
        ));

        let mut truncated_descendant = parser;
        truncated_descendant.dynamic_tags[0].subtree_end = 1;
        assert!(matches!(
            project_for_parser(source, &truncated_descendant),
            Err(ProjectionError::StructuralMismatch)
        ));
    }

    #[test]
    fn deeply_nested_dynamic_identity_scanning_is_linear() {
        const DEPTH: usize = 64;
        let mut expression = String::from("Leaf");
        for _ in 0..DEPTH {
            expression = format!("() => <{{{expression}}}/>");
        }
        let source = format!("const x=<{{{expression}}}/>;");
        let (overlay, identity_tokens) = super::Scanner::new_for_parser(&source)
            .finish_with_identity_token_visits()
            .expect("deep parser scan");
        assert_eq!(overlay.dynamic_tag_count(), DEPTH + 1);
        assert!(
            identity_tokens <= DEPTH * 12,
            "identity normalization revisited {identity_tokens} tokens for depth {DEPTH}"
        );
    }

    #[test]
    fn rejects_malformed_dynamic_tag_shapes() {
        for source in [
            "function View() @{ <{} /> }",
            "function View() @{ <{tag}>Hi</{other}> }",
            "function View() @{ <{tag}>Hi }",
        ] {
            assert!(
                matches!(
                    scan(source),
                    Err(ProjectionError::MalformedSyntax { .. }
                        | ProjectionError::UnterminatedSyntax { .. })
                ),
                "{source}"
            );
        }
    }

    #[test]
    fn dynamic_identity_matches_authoritative_trivia_and_outer_parentheses() {
        for source in [
            "function View() @{ <{/*a*/ Tag /*b*/}></{Tag}> }",
            "function View() @{ <{((Tag))}></{Tag}> }",
            "function View() @{ <{ obj }></{obj}> }",
            "function View() @{ <{ok ? Tag : /a*/}></{ok ? Tag : /a*/}> }",
            "function View() @{ <{Tag // open\n}></{Tag // close\n}> }",
            "function View() @{ <{Tag}></{Tag} /* close */> }",
        ] {
            assert!(scan(source).is_ok(), "{source}");
        }
        assert!(
            matches!(
                scan("function View() @{ <{obj . tag}></{obj.tag}> }"),
                Err(ProjectionError::MalformedSyntax { .. })
            ),
            "internal authored whitespace remains part of dynamic closing identity"
        );
    }

    #[test]
    fn embedded_tokens_remain_in_source_order_inside_dynamic_attributes() {
        let source = concat!(
            "function View() @{",
            "<{Outer} child={<{Inner} />} styles={<style>.x{color:red}</style>} />",
            "}"
        );
        let overlay = scan(source).unwrap();
        assert_eq!(overlay.dynamic_tag_count(), 2);
        assert_eq!(overlay.style_block_count(), 1);
        let projection = project_for_format(source, &overlay).unwrap();
        let lifted = lift_formatted(projection.source(), source, &projection).unwrap();
        assert_eq!(lifted, source);
    }

    #[test]
    fn deeply_parenthesized_dynamic_identity_scans_in_one_pass() {
        let depth = 8_192;
        let opening = format!("{}Tag{}", "(".repeat(depth), ")".repeat(depth));
        let source = format!("function View() @{{ <{{{opening}}}></{{Tag}}> }}");
        assert!(scan(&source).is_ok());
    }

    #[test]
    fn rejects_malformed_switch_and_try_clause_shapes() {
        for source in [
            "function View() @{ @case 0: {} }",
            "function View() @{ @default: {} }",
            "function View() @{ @pending {} }",
            "function View() @{ @catch (error) {} }",
            "function View() @{ @switch (x) { @case 0 {} } }",
            "function View() @{ @switch (x) { @default: {} @default: {} } }",
            "function View() @{ @switch (x) { case 0: {} } }",
            "function View() @{ @try {} }",
            "function View() @{ @try {} @catch {} @pending {} }",
            "function View() @{ @try {} @pending {} @pending {} }",
            "function View() @{ @try {} @catch {} @catch {} }",
            "function View() @{ @try {} @catch () {} }",
            "function View() @{ @try {} @catch (error,) {} }",
            "function View() @{ @try {} @catch (error, reset, extra) {} }",
            "function View() @{ @try {} @catch (error, { reset }) {} }",
        ] {
            assert!(
                matches!(scan(source), Err(ProjectionError::MalformedSyntax { .. })),
                "{source}"
            );
        }
    }

    #[test]
    fn switch_case_headers_keep_nested_colons_out_of_the_clause_delimiter() {
        let source = concat!(
            "function View() @{<main>@switch(value){",
            "@case flag ? one : two:{<b/>}",
            "@case ({kind:'ready'}).kind:{<i/>}",
            "@default:{<em/>}}</main>}"
        );
        let projection = project_for_format(source, &scan(source).unwrap()).unwrap();
        let lifted = lift_formatted(projection.source(), source, &projection).unwrap();
        assert_eq!(scan(&lifted).unwrap().control_count(), 1);
    }

    #[test]
    fn projection_maps_only_copied_authored_ranges() {
        let source = "function View() @{<main>@if(ok){debugger;}@else{var value=1;}</main>}";
        let overlay = scan(source).unwrap();
        let projection = project_for_lint(source, &overlay).unwrap();
        let debugger = projection.source().find("debugger;").unwrap();
        let mapped = projection
            .map_range(
                u32::try_from(debugger).unwrap()
                    ..u32::try_from(debugger + "debugger;".len()).unwrap(),
            )
            .unwrap();
        assert_eq!(mapped.start as usize, source.find("debugger;").unwrap());
        let wrapper = projection.source().find("W0").unwrap();
        assert!(
            projection
                .map_range(u32::try_from(wrapper).unwrap()..u32::try_from(wrapper + 1).unwrap())
                .is_none()
        );
    }

    #[test]
    fn unformatted_projection_round_trips_through_checked_lift() {
        let source = concat!(
            "function View() @{<main>@if(ok){<b/>}@else{<i/>}",
            "@for(const x of xs;index i;key x.id){<p>{i}</p>}@empty{<em/>}</main>}"
        );
        let projection = project_for_format(source, &scan(source).unwrap()).unwrap();
        let lifted = lift_formatted(projection.source(), source, &projection).unwrap();
        assert!(!lifted.contains("_t0_"));
        assert_eq!(scan(&lifted).unwrap().control_count(), 2);
    }

    #[test]
    fn switch_try_projection_round_trips_and_checks_method_identity() {
        let source = concat!(
            "function View() @{<main>@switch(value){@case 0:{@try{<b/>}",
            "@pending{<i/>}@catch(error,reset){<button onClick={reset}>{error}</button>}}",
            "@default:{<em/>}}</main>}"
        );
        let projection = project_for_format(source, &scan(source).unwrap()).unwrap();
        assert!(projection.source().contains("_t0_T1_"));
        assert!(projection.source().contains("_t0_C1_"));
        let lifted = lift_formatted(projection.source(), source, &projection).unwrap();
        assert!(!lifted.contains("_t0_"));
        assert_eq!(scan(&lifted).unwrap().control_count(), 2);

        let tampered = projection.source().replace("_t0_C1_", "_t0_X1_");
        assert!(matches!(
            lift_formatted(&tampered, source, &projection),
            Err(ProjectionError::MarkerResidual | ProjectionError::ScaffoldMismatch { .. })
        ));
    }

    #[test]
    fn stale_same_length_overlay_is_rejected() {
        let first = "function View() @{ var one = 1; }";
        let second = "function View() @{ var two = 2; }";
        assert_eq!(first.len(), second.len());
        let overlay = scan(first).unwrap();
        assert!(matches!(
            project_for_lint(second, &overlay),
            Err(ProjectionError::SourceChanged { .. })
        ));
    }

    #[test]
    fn rejects_orphan_clauses_and_invalid_index_annotation() {
        for source in [
            "function View() @{ @else {} }",
            "function View() @{ @empty {} }",
            "function View() @{ @for(const x of xs;index x.value){} }",
        ] {
            assert!(matches!(
                scan(source),
                Err(ProjectionError::MalformedSyntax { .. })
            ));
        }
    }

    #[test]
    fn repeated_generics_do_not_get_swallowed_as_jsx() {
        let mut source = String::from("function View() @{\n");
        for _ in 0..256 {
            source.push_str("const value = state<Conversation[]>([]);\n");
        }
        source.push_str("<Style data-index={1} />;\n}\n");
        let overlay = scan(&source).unwrap();
        assert_eq!(overlay.tokens().len(), 1);
    }

    #[test]
    fn distinguishes_regex_after_a_block_from_division_after_an_object() {
        let source = concat!(
            "function setup() {}\n",
            "/@if/.test(value);\n",
            "const ratio = { value: 1 } / 2;\n",
            "export function View() @{<Style />}\n",
        );
        let overlay = scan(source).unwrap();
        assert_eq!(overlay.tokens().len(), 1);
        assert_eq!(overlay.tokens()[0].kind, StructuralKind::FunctionBody);
    }

    #[test]
    fn deeply_nested_delimiters_spill_without_changing_the_overlay() {
        let opening = "(".repeat(32);
        let closing = ")".repeat(32);
        let source = format!("function View() @{{ const value = {opening}1{closing}; }}");
        let overlay = scan(&source).unwrap();
        assert_eq!(overlay.tokens().len(), 1);
        assert_eq!(overlay.tokens()[0].kind, StructuralKind::FunctionBody);
    }

    #[test]
    fn multi_digit_scaffold_ordinals_remain_unambiguous() {
        let mut source = String::new();
        for index in 0..12 {
            writeln!(
                source,
                "function View{index}() @{{<main>@for(const row of rows;index i;key row.id){{<p>{{i}}</p>}}@empty{{<i/>}}</main>}}"
            )
            .unwrap();
        }
        let projection = project_for_format(&source, &scan(&source).unwrap()).unwrap();
        let lifted = lift_formatted(projection.source(), &source, &projection).unwrap();
        assert_eq!(scan(&lifted).unwrap().control_count(), 12);
    }

    #[test]
    fn multi_digit_try_scaffolds_use_a_collision_free_namespace() {
        let mut source = String::from("const _t0_ = 'authored';\n");
        for index in 0..12 {
            writeln!(
                source,
                "function TryView{index}() @{{<main>@try{{<b/>}}@pending{{<i/>}}@catch(error,reset){{<button onClick={{reset}}>{{error}}</button>}}</main>}}"
            )
            .unwrap();
        }
        let projection = project_for_format(&source, &scan(&source).unwrap()).unwrap();
        assert!(projection.source().contains("_t1_T10_"));
        let lifted = lift_formatted(projection.source(), &source, &projection).unwrap();
        assert_eq!(scan(&lifted).unwrap().control_count(), 12);
    }

    #[test]
    fn deeply_nested_try_scaffolds_lift_in_source_order() {
        let mut source = String::from("function View() @{<main>");
        for _ in 0..24 {
            source.push_str("@try{");
        }
        source.push_str("<b/>");
        for _ in 0..24 {
            source.push_str("}@catch{}");
        }
        source.push_str("</main>}");
        let projection = project_for_format(&source, &scan(&source).unwrap()).unwrap();
        let lifted = lift_formatted(projection.source(), &source, &projection).unwrap();
        assert_eq!(scan(&lifted).unwrap().control_count(), 24);
    }

    #[test]
    fn nested_annotated_headers_project_in_source_order() {
        let source = concat!(
            "function View() @{<main>",
            "@for(const row of rows;index outer;key row.id){<section>",
            "@for(const item of row.items;index inner;key item.id){<p>{inner}</p>}",
            "@empty{<i/>}</section>}@empty{<b/>}</main>}"
        );
        let projection = project_for_format(source, &scan(source).unwrap()).unwrap();
        let lifted = lift_formatted(projection.source(), source, &projection).unwrap();
        assert_eq!(scan(&lifted).unwrap().control_count(), 2);
        assert!(lifted.find("index outer").unwrap() < lifted.find("index inner").unwrap());
    }

    #[test]
    fn checked_lift_rejects_changed_wrapper_identity() {
        let source = "function View() @{<main>@if(ok){<b/>}@else{<i/>}</main>}";
        let projection = project_for_format(source, &scan(source).unwrap()).unwrap();
        let changed = projection.source().replacen("_t0_M0_", "_t0_M9_", 1);
        assert!(matches!(
            lift_formatted(&changed, source, &projection),
            Err(ProjectionError::ScaffoldMismatch { .. })
        ));
    }
}
