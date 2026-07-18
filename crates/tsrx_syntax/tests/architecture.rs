use std::{fs, path::Path};

use tsrx_syntax::{
    ByteSpan, FormatProjection, MappedProjection, Overlay, ProjectionError, StructuralKind,
    StructuralToken, TypeProjection, lift_formatted, project, project_for_format, project_for_lint,
    project_for_types, scan,
};

#[test]
fn syntax_core_has_upstream_oriented_private_module_boundaries() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        "src/diagnostics.rs",
        "src/model.rs",
        "src/scanner/mod.rs",
        "src/scanner/stack.rs",
        "src/scanner/control.rs",
        "src/scanner/header.rs",
        "src/scanner/jsx.rs",
        "src/scanner/lexical.rs",
        "src/scanner/overlay.rs",
        "src/projection/mod.rs",
        "src/projection/mapping.rs",
        "src/projection/builder.rs",
        "src/projection/lint.rs",
        "src/projection/types.rs",
        "src/projection/format.rs",
        "src/projection/marker.rs",
        "src/projection/lift/mod.rs",
        "src/projection/lift/embedded.rs",
        "src/projection/lift/scaffold.rs",
        "src/projection/lift/writer.rs",
        "src/projection/lift/tokens.rs",
        "src/projection/lift/text.rs",
    ] {
        assert!(crate_root.join(path).is_file(), "missing {path}");
    }

    for path in ["src/scanner.rs", "src/projection.rs"] {
        assert!(
            !crate_root.join(path).exists(),
            "legacy monolith remains: {path}"
        );
    }
}

#[test]
fn syntax_core_uses_only_the_oxc_unicode_table_and_exposes_its_root_api() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(crate_root.join("Cargo.toml")).unwrap();
    let dependencies = manifest
        .split_once("[dependencies]")
        .and_then(|(_, rest)| rest.split_once("\n[").map(|(section, _)| section))
        .unwrap();
    assert_eq!(
        dependencies
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect::<Vec<_>>(),
        ["unicode-id-start = \"1\""]
    );
    assert!(!manifest.contains("[dev-dependencies]"));
    assert!(!dependencies.contains("git"));
    assert!(!dependencies.contains("oxc_"));

    let _: fn(&str) -> Result<Overlay, ProjectionError> = scan;
    let _: fn(&str, &Overlay) -> Result<String, ProjectionError> = project;
    let _: fn(&str, &Overlay) -> Result<MappedProjection, ProjectionError> = project_for_lint;
    let _: fn(&str, &Overlay) -> Result<TypeProjection, ProjectionError> = project_for_types;
    let _: fn(&str, &Overlay) -> Result<FormatProjection, ProjectionError> = project_for_format;
    let _: fn(&str, &str, &FormatProjection) -> Result<String, ProjectionError> = lift_formatted;
    let _: Option<ByteSpan> = None;
    let _: Option<StructuralKind> = None;
    let _: Option<StructuralToken> = None;

    let overlay = scan("function View() @{<main />} ").unwrap();
    assert_eq!(overlay.control_count(), 0);
    assert_eq!(overlay.tokens().len(), 1);
}
