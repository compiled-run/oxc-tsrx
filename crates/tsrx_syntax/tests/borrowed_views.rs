use tsrx_syntax::{
    ByteSpan, ClauseRole, ControlKind, EmbeddedKind, ForHeader, NONE_INDEX, OverlayClause,
    OverlayDynamicTag, OverlayEmbedded, OverlayNode, OverlayStyleBlock, OverlayToken,
    ParserCodeBlock, ParserDynamicToken, ProjectionSegment, project_for_lint, scan,
    scan_for_parser,
};

const fn assert_copy<T: Copy>() {}

#[test]
fn borrowed_records_have_frozen_fixed_width_layouts() {
    assert_copy::<ByteSpan>();
    assert_copy::<OverlayToken>();
    assert_copy::<OverlayNode>();
    assert_copy::<OverlayClause>();
    assert_copy::<OverlayEmbedded>();
    assert_copy::<ParserDynamicToken>();
    assert_copy::<ParserCodeBlock>();
    assert_copy::<OverlayDynamicTag>();
    assert_copy::<OverlayStyleBlock>();
    assert_copy::<ForHeader>();
    assert_copy::<ProjectionSegment>();

    assert_eq!(size_of::<ByteSpan>(), 8);
    assert_eq!(size_of::<OverlayToken>(), 16);
    assert_eq!(size_of::<OverlayNode>(), 36);
    assert_eq!(size_of::<OverlayClause>(), 72);
    assert_eq!(size_of::<OverlayEmbedded>(), 16);
    assert_eq!(size_of::<ParserDynamicToken>(), 12);
    assert_eq!(size_of::<ParserCodeBlock>(), 12);
    assert_eq!(size_of::<OverlayDynamicTag>(), 48);
    assert_eq!(size_of::<OverlayStyleBlock>(), 20);
    assert_eq!(size_of::<ForHeader>(), 36);
    assert_eq!(size_of::<ProjectionSegment>(), 16);
}

#[test]
fn overlay_view_borrows_flat_root_child_and_clause_chains() {
    let source = concat!(
        "function A() @{",
        "@if (ready) {@for(const item of items;index i;key item.id){<b/>}@empty{<i/>}}",
        "@else {<em/>}",
        "}",
        "const value = @switch(kind){@case 1:{<b/>}@default:{<i/>}};",
    );
    let overlay = scan(source).expect("valid nested TSRX");
    let view = overlay.view();
    let repeated = overlay.view();

    assert_eq!(view.tokens.as_ptr(), overlay.tokens().as_ptr());
    assert_eq!(view.nodes.as_ptr(), repeated.nodes.as_ptr());
    assert_eq!(view.clauses.as_ptr(), repeated.clauses.as_ptr());
    assert_eq!(view.embedded.as_ptr(), repeated.embedded.as_ptr());
    assert_eq!(view.parser_dynamic.as_ptr(), repeated.parser_dynamic.as_ptr());
    assert_eq!(view.parser_code_blocks.as_ptr(), repeated.parser_code_blocks.as_ptr());
    assert_eq!(view.dynamic_tags.as_ptr(), repeated.dynamic_tags.as_ptr());
    assert_eq!(view.dynamic_comments.as_ptr(), repeated.dynamic_comments.as_ptr());
    assert_eq!(view.style_blocks.as_ptr(), repeated.style_blocks.as_ptr());
    assert_eq!(view.source_len, u32::try_from(source.len()).expect("fixture fits OXC spans"));
    assert_ne!(view.first_root, NONE_INDEX);

    let first_root = &view.nodes[view.first_root as usize];
    assert_eq!(first_root.kind, ControlKind::If);
    assert_ne!(first_root.first_child, NONE_INDEX);
    assert_ne!(first_root.next_sibling, NONE_INDEX);

    let child = &view.nodes[first_root.first_child as usize];
    assert_eq!(child.kind, ControlKind::For);
    assert_eq!(child.parent, view.first_root);
    assert_eq!(child.next_sibling, NONE_INDEX);

    let first_clause = &view.clauses[first_root.first_clause as usize];
    assert_eq!(first_clause.role, ClauseRole::If);
    let alternate = &view.clauses[first_clause.next as usize];
    assert_eq!(alternate.role, ClauseRole::Else);

    let second_root = &view.nodes[first_root.next_sibling as usize];
    assert_eq!(second_root.kind, ControlKind::Switch);
    assert_eq!(second_root.next_sibling, NONE_INDEX);
}

#[test]
fn overlay_view_exposes_dynamic_style_and_embedded_records_without_copying() {
    let source = "function View() @{ <{tag}><style>.x{color:red}</style></{tag}> }";
    let overlay = scan_for_parser(source).expect("valid dynamic tag and style");
    let view = overlay.view();
    let repeated = overlay.view();

    assert!(!view.parser_dynamic.is_empty());
    assert_eq!(view.parser_dynamic.as_ptr(), repeated.parser_dynamic.as_ptr());
    assert_eq!(view.dynamic_tags.len(), 1);
    assert!(!view.dynamic_tags[0].self_closing);
    assert_eq!(view.style_blocks.len(), 1);
    let style_start = u32::try_from(source.find("<style>").unwrap()).unwrap();
    let style_end = u32::try_from(source.find("</style>").unwrap() + "</style>".len()).unwrap();
    assert_eq!(view.style_blocks[0].element, ByteSpan::new(style_start, style_end));
    assert!(!view.style_blocks[0].self_closing);
    assert!(view.embedded.iter().any(|record| record.kind == EmbeddedKind::DynamicOpen));
    assert!(view.embedded.iter().any(|record| record.kind == EmbeddedKind::StyleContent));
}

#[test]
fn parser_view_records_sparse_jsx_child_code_block_boundaries_without_rescanning() {
    let source = "function F() @{ <main>@{ const x=1; <A>{x}</A> }</main> }";
    let ordinary = scan(source).expect("ordinary overlay");
    assert!(ordinary.view().parser_code_blocks.is_empty());

    let overlay = scan_for_parser(source).expect("parser overlay");
    let view = overlay.view();
    assert_eq!(view.parser_code_blocks.len(), 1);
    let block = view.parser_code_blocks[0];
    assert_eq!(view.tokens[block.token as usize].kind, tsrx_syntax::StructuralKind::FunctionBody);
    assert_eq!(
        &source[block.body.start as usize..block.body.end as usize],
        "{ const x=1; <A>{x}</A> }"
    );
}

#[test]
fn projection_view_borrows_affine_segments() {
    let source = "function View() @{ @if (ready) { <p>{value}</p> } }";
    let overlay = scan(source).expect("valid TSRX");
    let projection = project_for_lint(source, &overlay).expect("legal TSX projection");
    let view = projection.view();
    let repeated = projection.view();

    assert_eq!(view.source.as_ptr(), projection.source().as_ptr());
    assert_eq!(view.segments.as_ptr(), repeated.segments.as_ptr());
    assert!(!view.segments.is_empty());
    let projected_len = u32::try_from(view.source.len()).expect("fixture fits OXC spans");
    assert!(view.segments.iter().all(|segment| {
        segment.projected.start <= segment.projected.end && segment.projected.end <= projected_len
    }));
}
