use oxc_adapter::EngineDiagnostic;
use tsrx_syntax::{MappedProjection, TypeProjection};

#[derive(Default)]
pub(crate) struct TranslatedDiagnostics {
    pub(crate) diagnostics: Vec<EngineDiagnostic>,
    pub(crate) suppressed: u32,
    pub(crate) rejected_fixes: u32,
}

pub(crate) fn translate_diagnostics(
    diagnostics: Vec<EngineDiagnostic>,
    projection: Option<&MappedProjection>,
) -> TranslatedDiagnostics {
    let Some(projection) = projection else {
        return TranslatedDiagnostics { diagnostics, ..TranslatedDiagnostics::default() };
    };
    let mut translated = TranslatedDiagnostics::default();
    for mut diagnostic in diagnostics {
        if diagnostic.labels.is_empty() {
            translated.suppressed += 1;
            translated.rejected_fixes += u32::try_from(diagnostic.fixes.len()).unwrap_or(u32::MAX);
            continue;
        }
        let mut labels = Vec::with_capacity(diagnostic.labels.len());
        let mut labels_are_authored = true;
        for mut label in diagnostic.labels {
            let range = label.offset..label.offset.saturating_add(label.length);
            let Some(mapped) = projection.map_range(range) else {
                labels_are_authored = false;
                break;
            };
            label.offset = mapped.start;
            label.length = mapped.end - mapped.start;
            labels.push(label);
        }
        if !labels_are_authored {
            translated.suppressed += 1;
            translated.rejected_fixes += u32::try_from(diagnostic.fixes.len()).unwrap_or(u32::MAX);
            continue;
        }
        diagnostic.labels = labels;
        if diagnostic.rule.as_deref() == Some("require-yield")
            && diagnostic.labels.iter().any(|label| {
                projection.is_synthetic_generator_range(
                    label.offset..label.offset.saturating_add(label.length),
                )
            })
        {
            translated.suppressed = translated.suppressed.saturating_add(1);
            translated.rejected_fixes = translated
                .rejected_fixes
                .saturating_add(u32::try_from(diagnostic.fixes.len()).unwrap_or(u32::MAX));
            continue;
        }
        diagnostic.fixes = diagnostic
            .fixes
            .into_iter()
            .filter_map(|mut fix| {
                let range = fix.offset..fix.offset.saturating_add(fix.length);
                let Some(mapped) = projection.map_fix_range(range) else {
                    translated.rejected_fixes += 1;
                    return None;
                };
                fix.offset = mapped.start;
                fix.length = mapped.end - mapped.start;
                Some(fix)
            })
            .collect();
        translated.diagnostics.push(diagnostic);
    }
    translated
}

pub(crate) fn translate_type_diagnostics(
    diagnostics: Vec<EngineDiagnostic>,
    projection: Option<&TypeProjection>,
) -> TranslatedDiagnostics {
    let Some(projection) = projection else {
        return TranslatedDiagnostics { diagnostics, ..TranslatedDiagnostics::default() };
    };
    let mut translated = TranslatedDiagnostics::default();
    for mut diagnostic in diagnostics {
        if diagnostic.labels.is_empty() {
            translated.suppressed = translated.suppressed.saturating_add(1);
            translated.rejected_fixes = translated
                .rejected_fixes
                .saturating_add(u32::try_from(diagnostic.fixes.len()).unwrap_or(u32::MAX));
            continue;
        }
        let mut labels = Vec::with_capacity(diagnostic.labels.len());
        for mut label in diagnostic.labels {
            let range = label.offset..label.offset.saturating_add(label.length);
            let Some(mapped) = projection.map_range(range) else {
                labels.clear();
                break;
            };
            label.offset = mapped.start;
            label.length = mapped.end - mapped.start;
            labels.push(label);
        }
        if labels.is_empty() {
            translated.suppressed = translated.suppressed.saturating_add(1);
            translated.rejected_fixes = translated
                .rejected_fixes
                .saturating_add(u32::try_from(diagnostic.fixes.len()).unwrap_or(u32::MAX));
            continue;
        }
        diagnostic.labels = labels;
        diagnostic.fixes = diagnostic
            .fixes
            .into_iter()
            .filter_map(|mut fix| {
                let range = fix.offset..fix.offset.saturating_add(fix.length);
                let Some(mapped) = projection.map_fix_range(range) else {
                    translated.rejected_fixes = translated.rejected_fixes.saturating_add(1);
                    return None;
                };
                fix.offset = mapped.start;
                fix.length = mapped.end - mapped.start;
                Some(fix)
            })
            .collect();
        translated.diagnostics.push(diagnostic);
    }
    translated
}

#[cfg(test)]
mod tests {
    use tsrx_syntax::{project_for_lint, scan};

    #[test]
    fn fix_mapping_is_identity_only() {
        let source = "function View() @{ var value = 1; }";
        let overlay = scan(source).unwrap();
        let projection = project_for_lint(source, &overlay).unwrap();
        let projected_var = u32::try_from(projection.source().find("var").unwrap()).unwrap();
        let original_var = u32::try_from(source.find("var").unwrap()).unwrap();
        assert_eq!(
            projection.map_range(projected_var..projected_var + 3),
            Some(original_var..original_var + 3)
        );
        let marker = u32::try_from(projection.source().find("/*").unwrap()).unwrap();
        assert!(projection.map_range(marker..marker + 1).is_none());
    }
}
