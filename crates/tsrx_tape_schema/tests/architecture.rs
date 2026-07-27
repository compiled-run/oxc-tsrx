use std::path::Path;

#[test]
fn the_tape_schema_keeps_one_concept_per_private_module_file() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        "src/lib.rs",
        "src/result/mod.rs",
        "src/result/bounds.rs",
        "src/result/kinds.rs",
        "src/result/packed_text.rs",
        "src/result/records.rs",
        "src/result/spans.rs",
        "src/result/tests.rs",
        "src/tape/mod.rs",
        "src/tape/bounds.rs",
        "src/tape/compact.rs",
        "src/tape/iter.rs",
        "src/tape/record.rs",
        "src/tape/value.rs",
        "src/transfer/mod.rs",
        "src/transfer/binary.rs",
        "src/transfer/binary_records.rs",
        "src/transfer/buffer.rs",
        "src/transfer/common_keys.rs",
        "src/transfer/entry.rs",
        "src/transfer/json.rs",
        "src/transfer/json_owned.rs",
        "src/transfer/walk.rs",
    ] {
        assert!(crate_root.join(path).is_file(), "missing {path}");
    }

    for path in ["src/result.rs", "src/tape.rs", "src/transfer.rs"] {
        assert!(!crate_root.join(path).exists(), "legacy monolith remains: {path}");
    }
}

#[test]
fn no_tape_schema_source_file_exceeds_the_layout_cap() {
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
fn the_crate_root_and_the_transfer_root_declare_modules_and_re_exports_only() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let transfer =
        std::fs::read_to_string(crate_root.join("src/transfer/mod.rs")).expect("transfer root");
    assert!(!transfer.contains("fn "), "the transfer root carries implementation");
    let lib = std::fs::read_to_string(crate_root.join("src/lib.rs")).expect("src/lib.rs");
    assert!(
        !lib.contains("impl FlatTape"),
        "tape entry points belong to the module that owns the tape"
    );
}

/// `rustdoc` records the defining module of every type a sibling crate names in its own public
/// API, so `tsrx_tape_schema::result::{CommentTable, CoordinateDomain, DiagnosticTable,
/// ModuleTable, ParseCompleteness}` and `tsrx_tape_schema::tape::{FlatTape, TapeBuildError}` are
/// pinned paths. Moving any of them into a submodule rewrites `tsrx_parser_engine`'s public API
/// without changing one line of it.
#[test]
fn the_types_sibling_crates_name_stay_in_their_pinned_defining_module() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let result =
        std::fs::read_to_string(crate_root.join("src/result/mod.rs")).expect("result root");
    for item in [
        "pub struct CommentTable",
        "pub enum CoordinateDomain",
        "pub struct DiagnosticTable",
        "pub struct ModuleTable",
        "pub enum ParseCompleteness",
    ] {
        assert!(result.contains(item), "`{item}` left tsrx_tape_schema::result");
    }
    let tape = std::fs::read_to_string(crate_root.join("src/tape/mod.rs")).expect("tape root");
    for item in ["pub struct FlatTape", "pub enum TapeBuildError"] {
        assert!(tape.contains(item), "`{item}` left tsrx_tape_schema::tape");
    }
}

#[test]
fn the_flat_tape_inherent_surface_stays_in_two_impl_blocks() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut blocks = 0_usize;
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
            blocks += source.lines().filter(|line| line.trim() == "impl FlatTape {").count();
        }
    }
    assert_eq!(
        blocks, 2,
        "`cargo public-api` reports one line per inherent impl block, so splitting these grows the \
         frozen public surface"
    );
}
