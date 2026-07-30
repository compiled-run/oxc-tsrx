//! The checked lift has to survive canonical Oxfmt's line breaking, not just its own
//! projection text.
//!
//! Every test in this crate before this file lifted `projection.source()` verbatim: the
//! projection exactly as it was built, on one line, with no formatter in between. The
//! formatter is the whole point of the projection, and it reflows. A call whose arguments
//! do not fit the print width gets broken across lines, and a broken call gets a trailing
//! comma after its last argument. That is ordinary, correct Oxfmt output, and the lift has
//! to read it.
//!
//! `reflow_broken_calls` below reproduces that one transformation on the scaffold calls the
//! lift parses by hand, which is what a deep or wide `@for` header really receives. The
//! end-to-end proof against the real formatter lives in `tsrx_format`, which owns it; these
//! tests keep this crate's own contract honest without giving it a formatter dependency.

use tsrx_syntax::{lift_formatted, project_for_format, scan};

/// Inserts the trailing comma canonical Oxfmt writes when it breaks the `@for` header's
/// inner `index` and `key` helper calls across lines.
///
/// Scaffold end markers are spelled `/*_<prefix>_<kind><ordinal>E__*/`, and the inner
/// helper call closes on the first `)` after one. Only `I` (index) and `K` (key) name an
/// inner call; `R` (the right-hand expression) and `N` (a wrapper body) sit inside calls
/// that continue past their marker, so they are left alone.
fn reflow_broken_calls(projected: &str) -> String {
    const MARKER_END: &str = "E__*/";

    let bytes = projected.as_bytes();
    let mut output = String::with_capacity(projected.len() + 64);
    let mut copied = 0usize;
    let mut search = 0usize;
    while let Some(offset) = projected[search..].find(MARKER_END) {
        let kind_at = search + offset;
        let marker_end = kind_at + MARKER_END.len();
        search = marker_end;

        let mut ordinal_start = kind_at;
        while ordinal_start > 0 && bytes[ordinal_start - 1].is_ascii_digit() {
            ordinal_start -= 1;
        }
        if ordinal_start == kind_at {
            continue;
        }
        if !matches!(bytes.get(ordinal_start - 1), Some(b'I' | b'K')) {
            continue;
        }

        let Some(close) = projected[marker_end..].find(')') else { continue };
        let close = marker_end + close;
        output.push_str(&projected[copied..close]);
        output.push_str(",\n          ");
        copied = close;
    }
    output.push_str(&projected[copied..]);
    output
}

fn assert_lifts_the_same_broken_or_not(source: &str, controls: usize) {
    let projection = project_for_format(source, &scan(source).unwrap()).unwrap();
    let straight = lift_formatted(projection.source(), source, &projection).unwrap();

    let broken = reflow_broken_calls(projection.source());
    assert_ne!(broken, projection.source(), "the reflow did not change anything to test");

    let lifted = lift_formatted(&broken, source, &projection)
        .unwrap_or_else(|error| panic!("broken-call lift failed: {error}\n{broken}"));

    assert!(!lifted.contains("_t"), "a scaffold identifier survived the lift: {lifted}");
    assert_eq!(scan(&lifted).unwrap().control_count(), controls);
    assert_eq!(
        lifted.split_ascii_whitespace().collect::<Vec<_>>(),
        straight.split_ascii_whitespace().collect::<Vec<_>>(),
        "breaking the scaffold calls changed the lifted program"
    );
}

#[test]
fn keyed_header_lifts_when_its_key_helper_call_is_broken() {
    assert_lifts_the_same_broken_or_not(
        "function View() @{<ul>@for(const row of rows;key row.id){<li>{row.label}</li>}</ul>}",
        1,
    );
}

#[test]
fn indexed_header_lifts_when_its_index_helper_call_is_broken() {
    assert_lifts_the_same_broken_or_not(
        "function View() @{<ul>@for(const row of rows;index position){<li>{position}</li>}</ul>}",
        1,
    );
}

#[test]
fn annotated_header_lifts_when_both_helper_calls_are_broken() {
    assert_lifts_the_same_broken_or_not(
        concat!(
            "function View() @{<ul>@for(const row of rows;index position;key row.id)",
            "{<li>{position}{row.label}</li>}@empty{<i/>}</ul>}"
        ),
        1,
    );
}

#[test]
fn header_inside_a_try_arm_lifts_when_its_helper_calls_are_broken() {
    // The shape that reached this defect from real code: a keyed `@for` two element
    // levels inside a `@try` arm. The nesting is what pushes the header past the print
    // width, so the formatter breaks the inner calls that the lift then has to read.
    assert_lifts_the_same_broken_or_not(
        concat!(
            "function View() @{<div>@try{<Frame><ul>",
            "@for(const row of rows;key row.id){<li>{row.label}</li>}",
            "</ul></Frame>}@pending{<p>loading</p>}@catch{<p>failed</p>}</div>}"
        ),
        2,
    );
}

#[test]
fn nested_headers_lift_when_every_helper_call_is_broken() {
    assert_lifts_the_same_broken_or_not(
        concat!(
            "function View() @{<main>",
            "@for(const row of rows;index outer;key row.id){<section>",
            "@for(const item of row.items;index inner;key item.id){<p>{inner}</p>}",
            "@empty{<i/>}</section>}@empty{<b/>}</main>}"
        ),
        2,
    );
}

#[test]
fn breaking_a_scaffold_call_does_not_make_the_lift_accept_a_changed_scaffold() {
    // Tolerating the formatter's trailing comma must not weaken the check that makes
    // oxfmt fail safe. A scaffold whose identity really did change still has to be
    // rejected, broken across lines or not.
    let source = "function View() @{<ul>@for(const row of rows;key row.id){<li/>}</ul>}";
    let projection = project_for_format(source, &scan(source).unwrap()).unwrap();
    let broken = reflow_broken_calls(projection.source());

    for tampered in [
        broken.replacen("_KH0_", "_KH9_", 1),
        broken.replacen("_HE0_", "_HE9_", 1),
        broken.replacen("_H0_(", "_H9_(", 1),
    ] {
        assert!(
            lift_formatted(&tampered, source, &projection).is_err(),
            "a changed scaffold lifted anyway: {tampered}"
        );
    }
}
