use std::path::Path;

#[test]
fn the_parser_engine_keeps_one_concept_per_private_module_file() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        "src/lib.rs",
        "src/entry.rs",
        "src/error.rs",
        "src/grammar_result.rs",
        "src/observer.rs",
        "src/parse_result.rs",
        "src/pipeline.rs",
        "src/request.rs",
        "src/utf16_route.rs",
        "src/reconstruct/mod.rs",
        "src/reconstruct/access.rs",
        "src/reconstruct/code_blocks.rs",
        "src/reconstruct/control.rs",
        "src/reconstruct/css.rs",
        "src/reconstruct/dynamic_tags.rs",
        "src/reconstruct/edits.rs",
        "src/reconstruct/if_chain.rs",
        "src/reconstruct/jsx_statements.rs",
        "src/reconstruct/layout_text.rs",
        "src/reconstruct/loops.rs",
        "src/reconstruct/objects.rs",
        "src/reconstruct/program.rs",
        "src/reconstruct/scaffold.rs",
        "src/reconstruct/spans.rs",
        "src/reconstruct/style.rs",
        "src/reconstruct/switch.rs",
        "src/reconstruct/try_catch.rs",
    ] {
        assert!(crate_root.join(path).is_file(), "missing {path}");
    }

    assert!(
        !crate_root.join("src/reconstruct.rs").exists(),
        "legacy monolith remains: src/reconstruct.rs"
    );
}

#[test]
fn the_parser_engine_crate_root_declares_modules_and_re_exports_only() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib = std::fs::read_to_string(crate_root.join("src/lib.rs")).expect("src/lib.rs");
    for line in lib.lines().map(str::trim).filter(|line| !line.is_empty()) {
        assert!(
            line.starts_with("//!")
                || line.starts_with("#[cfg(")
                || line.starts_with("mod ")
                || line.starts_with("pub use ")
                || line.starts_with("};")
                || !line.contains('{'),
            "the crate root carries implementation: {line}"
        );
    }
    assert!(!lib.contains("fn "), "the crate root carries implementation");
    assert!(!lib.contains("#[cfg(test)]"), "crate-root tests belong to the module they cover");
}
