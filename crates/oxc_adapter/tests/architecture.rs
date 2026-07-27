use std::path::Path;

#[test]
fn the_adapter_keeps_one_concept_per_private_module_file() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        "src/lib.rs",
        "src/toolchain/mod.rs",
        "src/toolchain/config.rs",
        "src/toolchain/diagnostics.rs",
        "src/toolchain/engine.rs",
        "src/toolchain/format.rs",
        "src/toolchain/session.rs",
        "src/toolchain/timings.rs",
        "src/toolchain/tsgolint/mod.rs",
        "src/toolchain/tsgolint/batch.rs",
        "src/toolchain/tsgolint/discovery.rs",
        "src/toolchain/tsgolint/protocol.rs",
        "src/parser/mod.rs",
    ] {
        assert!(crate_root.join(path).is_file(), "missing {path}");
    }

    for path in ["src/toolchain.rs", "src/parser.rs"] {
        assert!(!crate_root.join(path).exists(), "legacy monolith remains: {path}");
    }
}

#[test]
fn no_adapter_source_file_exceeds_the_layout_cap() {
    for path in source_files() {
        let lines = std::fs::read_to_string(&path).expect("readable source file").lines().count();
        assert!(lines <= 1500, "{} carries {lines} lines", path.display());
    }
}

/// `rustdoc` records the defining module of every foreign type a crate names in its own public
/// API, and `tsrx_lint::LintSession`'s four constructors name `oxc_adapter::toolchain::RuleFilter`.
/// Moving `RuleFilter` (or the `RuleSeverity` its field is typed by) into a submodule was measured
/// to rewrite those four lines of `tsrx_lint`'s frozen public surface while leaving `oxc_adapter`'s
/// own 672 items untouched, so no signature would have flagged it. Both types stay in the module
/// root, which is why that root carries logic where the layout rules would rather it did not.
#[test]
fn the_rule_filter_a_sibling_crate_names_stays_in_the_toolchain_root() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = std::fs::read_to_string(crate_root.join("src/toolchain/mod.rs"))
        .expect("toolchain module root");
    for item in ["pub enum RuleSeverity", "pub struct RuleFilter"] {
        assert!(root.contains(item), "`{item}` left oxc_adapter::toolchain");
    }
}

/// `cargo public-api` emits one `impl <Type>` line per inherent impl block, so a third block would
/// grow the frozen surface from 672 items to 673 without adding a single callable function.
#[test]
fn the_lint_engine_inherent_surface_stays_in_two_impl_blocks() {
    let blocks: usize = source_files()
        .into_iter()
        .map(|path| {
            let source = std::fs::read_to_string(&path).expect("readable source file");
            source.lines().filter(|line| line.trim() == "impl LintEngine {").count()
        })
        .sum();
    assert_eq!(
        blocks, 2,
        "`cargo public-api` reports one line per inherent impl block, so splitting these grows the \
         frozen public surface"
    );
}

fn source_files() -> Vec<std::path::PathBuf> {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    let mut pending = vec![crate_root.join("src")];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("readable source directory") {
            let path = entry.expect("readable source entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files
}
