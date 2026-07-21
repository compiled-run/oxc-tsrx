use tsrx_syntax::{
    OpaqueSurrogateContext, ProjectionError, classify_wtf8_surrogates,
    classify_wtf8_surrogates_detailed, scan, scan_for_parser,
};

const HIGH_SURROGATE: [u8; 3] = [0xED, 0xA0, 0x80];
const LOW_SURROGATE: [u8; 3] = [0xED, 0xB0, 0x80];

#[derive(Default)]
struct Wtf8Fixture {
    source: Vec<u8>,
    probes: Vec<u32>,
}

impl Wtf8Fixture {
    fn text(&mut self, text: &[u8]) -> &mut Self {
        self.source.extend_from_slice(text);
        self
    }

    fn high(&mut self) -> &mut Self {
        self.surrogate(HIGH_SURROGATE)
    }

    fn low(&mut self) -> &mut Self {
        self.surrogate(LOW_SURROGATE)
    }

    fn surrogate(&mut self, encoded: [u8; 3]) -> &mut Self {
        self.probes
            .push(u32::try_from(self.source.len()).expect("fixture fits a source offset"));
        self.source.extend_from_slice(&encoded);
        self
    }

    fn classify(&self) -> Vec<Option<OpaqueSurrogateContext>> {
        classify_wtf8_surrogates(&self.source, &self.probes)
    }
}

#[test]
fn quoted_strings_accept_both_surrogate_halves_next_to_escapes() {
    let mut fixture = Wtf8Fixture::default();
    fixture
        .text(b"const high = \"escaped\\")
        .high()
        .text(b"tail\"; const low = '")
        .low()
        .text(b"\\n';");

    assert_eq!(
        fixture.classify(),
        [
            Some(OpaqueSurrogateContext::QuotedString),
            Some(OpaqueSurrogateContext::QuotedString),
        ]
    );
}

#[test]
fn template_raw_is_opaque_but_interpolation_is_active() {
    let mut fixture = Wtf8Fixture::default();
    fixture
        .text(b"const value = `raw ")
        .high()
        .text(b" ${")
        .low()
        .text(b"} tail`;");

    assert_eq!(
        fixture.classify(),
        [Some(OpaqueSurrogateContext::TemplateRaw), None]
    );
}

#[test]
fn regex_body_is_opaque_but_flags_are_active() {
    let mut fixture = Wtf8Fixture::default();
    fixture
        .text(b"const value = /head[")
        .high()
        .text(b"]tail/gu")
        .low()
        .text(b";");

    assert_eq!(
        fixture.classify(),
        [Some(OpaqueSurrogateContext::RegexBody), None]
    );
}

#[test]
fn line_and_block_comments_are_opaque() {
    let mut fixture = Wtf8Fixture::default();
    fixture
        .text(b"// high ")
        .high()
        .text(b"\n/* low ")
        .low()
        .text(b" */\nconst value = 1;");

    assert_eq!(
        fixture.classify(),
        [
            Some(OpaqueSurrogateContext::Comment),
            Some(OpaqueSurrogateContext::Comment),
        ]
    );
}

#[test]
fn jsx_text_and_quoted_attributes_are_opaque() {
    let mut fixture = Wtf8Fixture::default();
    fixture
        .text(b"const value = <main title=\"")
        .high()
        .text(b"\">child ")
        .low()
        .text(b"</main>;");

    assert_eq!(
        fixture.classify(),
        [
            Some(OpaqueSurrogateContext::QuotedString),
            Some(OpaqueSurrogateContext::JsxText),
        ]
    );
}

#[test]
fn raw_style_content_is_one_opaque_region() {
    let mut fixture = Wtf8Fixture::default();
    fixture
        .text(b"const value = <style>.a::before{content:'")
        .high()
        .text(b"'}/* ")
        .low()
        .text(b" */</style>;");

    assert_eq!(
        fixture.classify(),
        [
            Some(OpaqueSurrogateContext::RawStyle),
            Some(OpaqueSurrogateContext::RawStyle),
        ]
    );
}

#[test]
fn executable_positions_fail_closed() {
    let active_fixtures = [
        {
            let mut fixture = Wtf8Fixture::default();
            fixture.text(b"const value = ").high().text(b";");
            fixture
        },
        {
            let mut fixture = Wtf8Fixture::default();
            fixture.text(b"const value = `raw ${").low().text(b"}`;");
            fixture
        },
        {
            let mut fixture = Wtf8Fixture::default();
            fixture.text(b"const value = /body/g").high().text(b";");
            fixture
        },
        {
            let mut fixture = Wtf8Fixture::default();
            fixture
                .text(b"const value = <main>{")
                .low()
                .text(b"}</main>;");
            fixture
        },
    ];

    for fixture in active_fixtures {
        assert_eq!(fixture.classify(), [None], "{:?}", fixture.source);
    }
}

#[test]
fn speculative_jsx_marks_are_rolled_back() {
    let mut accepted = Wtf8Fixture::default();
    accepted
        .text(b"const value = <A{}>committed ")
        .high()
        .text(b"</A>;");
    assert_eq!(accepted.classify(), [Some(OpaqueSurrogateContext::JsxText)]);

    let mut fixture = Wtf8Fixture::default();
    fixture
        .text(b"const value = <A{}>speculative ")
        .high()
        .low();

    assert_eq!(fixture.classify(), [None, None]);
}

#[test]
fn empty_probe_list_stays_on_the_no_probe_path() {
    assert!(classify_wtf8_surrogates(b"const value = 1;", &[]).is_empty());
}

#[test]
fn unicode_identifiers_preserve_expression_and_jsx_scanner_state() {
    for source in [
        "const π = 4; const ratio = π / 2;",
        "const cafe\u{301} = 4; const ratio = cafe\u{301} / 2;",
        "const 𐐀 = 4; const ratio = 𐐀 / 2;",
        "function View() @{ @if (π / 2) { <Καλημέρα δεδομένα={cafe\u{301} / 2}/> } }",
    ] {
        scan_for_parser(source)
            .unwrap_or_else(|error| panic!("Unicode fixture failed to scan: {source:?}: {error}"));
    }

    let overlay = scan("const value = @ifπ; const ratio = π / 2;")
        .expect("a Unicode continuation prevents an ASCII TSRX keyword-prefix match");
    assert_eq!(overlay.control_count(), 0);
}

#[test]
fn detailed_classification_retains_an_earlier_structural_failure() {
    let mut fixture = Wtf8Fixture::default();
    fixture
        .text(b"function View() @{ @else{} const value=")
        .high()
        .text(b"; }");
    let classification = classify_wtf8_surrogates_detailed(&fixture.source, &fixture.probes);
    assert_eq!(classification.contexts, [None]);
    assert!(matches!(
        classification.earlier_error,
        Some(ProjectionError::MalformedSyntax {
            expected: "an owning TSRX control",
            ..
        })
    ));
}
