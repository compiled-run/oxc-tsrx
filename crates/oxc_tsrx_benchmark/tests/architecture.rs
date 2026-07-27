use std::path::Path;

#[test]
fn the_lint_harness_keeps_one_concept_per_module_file() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        "src/main.rs",
        "src/budgets.rs",
        "src/fixtures.rs",
        "src/in_process.rs",
        "src/process.rs",
        "src/report.rs",
        "src/signatures.rs",
        "src/stats.rs",
    ] {
        assert!(crate_root.join(path).is_file(), "missing {path}");
    }
}

#[test]
fn no_lint_harness_source_file_exceeds_the_layout_cap() {
    for path in source_files() {
        let lines = std::fs::read_to_string(&path).expect("readable source file").lines().count();
        assert!(lines <= 1500, "{} carries {lines} lines", path.display());
    }
}

/// `clippy::allow_attributes` is deny-level but only fires on outer `#[allow]`, so an inner
/// `#![allow]` at a crate or module root is invisible to it and never self-invalidates. That is
/// how a crate-level suppression here kept `clippy::cast_sign_loss` listed long after it had
/// stopped firing. `#![expect(..., reason = "...")]` is the only permitted form, because
/// `unfulfilled_lint_expectations` turns a stale entry into a build failure.
#[test]
fn the_lint_harness_suppresses_lints_only_with_expect() {
    for path in source_files() {
        let source = std::fs::read_to_string(&path).expect("readable source file");
        assert!(
            !source.lines().any(|line| line.trim_start().starts_with("#![allow(")),
            "{} carries an inner #![allow] that can never go stale",
            path.display()
        );
    }
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
