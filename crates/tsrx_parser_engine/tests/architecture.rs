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
        "src/projection/mod.rs",
        "src/projection/clauses.rs",
        "src/projection/comments.rs",
        "src/projection/embedded.rs",
        "src/projection/gaps.rs",
        "src/projection/mapping.rs",
        "src/projection/marker.rs",
        "src/projection/marker_validation.rs",
        "src/projection/overlay.rs",
        "src/projection/text.rs",
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
        "src/utf16_result/mod.rs",
        "src/utf16_result/codeframe.rs",
        "src/utf16_result/comments.rs",
        "src/utf16_result/finalize.rs",
        "src/utf16_result/ledger.rs",
        "src/utf16_result/module_values.rs",
        "src/utf16_result/observer.rs",
        "src/utf16_result/program_values.rs",
        "src/utf16_result/pua_markers.rs",
        "src/utf16_result/reachability.rs",
        "src/utf16_result/tape_fields.rs",
    ] {
        assert!(crate_root.join(path).is_file(), "missing {path}");
    }

    for path in ["src/reconstruct.rs", "src/projection.rs", "src/utf16_result.rs"] {
        assert!(!crate_root.join(path).exists(), "legacy monolith remains: {path}");
    }
}

#[test]
fn no_parser_engine_source_file_exceeds_the_layout_cap() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut pending = vec![crate_root.join("src")];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("readable source directory") {
            let path = entry.expect("readable source entry").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let lines =
                std::fs::read_to_string(&path).expect("readable source file").lines().count();
            assert!(lines <= 1500, "{} carries {lines} lines", path.display());
        }
    }
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

#[test]
fn no_source_file_carries_a_module_wide_lint_suppression() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut pending = vec![crate_root.join("src")];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("readable source directory") {
            let path = entry.expect("readable source entry").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("readable source file");
            assert!(
                !source.contains("#![allow("),
                "module-wide allow in {}",
                path.strip_prefix(crate_root).expect("source file under the crate root").display()
            );
        }
    }
}
