#[allow(dead_code)]
mod support;

use support::assert_failed;
use tsrx_parser_engine::{TsrxParseRequest, parse_tsrx};

#[test]
fn unsupported_tsrx_syntax_returns_no_partial_program() {
    for source in [
        "function View() @{ @switch(x) { @default: { return x } } }",
        "function View() @{ @try { return 1 } @catch {} }",
        "function View() @{ const value = ; <main /> }",
        "function View() @{ <main />; const after = 1; }",
    ] {
        assert_failed(source);
    }
    assert!(
        parse_tsrx(&TsrxParseRequest {
            source: "function Viéw() @{ <main /> }",
        })
        .is_err(),
        "the retained ASCII guard must remain operational"
    );
}

#[test]
fn affine_authored_comments_are_byte_validated_and_accepted() {
    for source in [
        "/* module */ function View() @{ /* render */ <main /> }",
        "function View() @{ @if(ok){/* yes */<b/>}@else{/* no */<i/>} }",
        "function View() @{ // render\n <main /> }",
    ] {
        assert!(
            parse_tsrx(&TsrxParseRequest { source }).is_ok(),
            "authored comment should survive the affine parser lane: {source}"
        );
    }
}
