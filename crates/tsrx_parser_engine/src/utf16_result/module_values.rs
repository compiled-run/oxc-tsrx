use tsrx_syntax::OpaqueSurrogateContext;
use tsrx_tape_schema::{ModuleNameRecord, ModuleTable, TapeSpan, ValueSpanRecord};

use crate::{TsrxParseError, source_bridge::PreparedSource};

use super::{
    observer::{RepairCopyLane, Utf16WorkObserver},
    pua_markers::{apply_pua_markers, javascript_quoted_pua_markers},
};

pub(crate) fn forbidden_module_name_span(
    module: &ModuleTable,
    source: &PreparedSource<'_>,
) -> Result<Option<TapeSpan>, TsrxParseError> {
    let mut forbidden = None;
    for import in module.static_imports() {
        for entry in module.static_import_entries(import.entries).ok_or_else(|| {
            TsrxParseError::Adapter("invalid static import entry range".to_string())
        })? {
            retain_forbidden_name(&mut forbidden, &entry.import_name, source);
        }
    }
    for export in module.static_exports() {
        for entry in module.static_export_entries(export.entries).ok_or_else(|| {
            TsrxParseError::Adapter("invalid static export entry range".to_string())
        })? {
            retain_forbidden_name(&mut forbidden, &entry.import_name, source);
            retain_forbidden_name(&mut forbidden, &entry.export_name, source);
            retain_forbidden_name(&mut forbidden, &entry.local_name, source);
        }
    }
    Ok(forbidden)
}

pub(crate) fn forbidden_rejection_module_name_span(
    spans: &[TapeSpan],
    source: &PreparedSource<'_>,
) -> Option<TapeSpan> {
    spans
        .iter()
        .copied()
        .filter(|span| {
            source.has_fixup_context_in(span.start, span.end, OpaqueSurrogateContext::QuotedString)
        })
        .min_by_key(|span| (span.start, span.end))
}

fn retain_forbidden_name<K>(
    forbidden: &mut Option<TapeSpan>,
    name: &ModuleNameRecord<K>,
    source: &PreparedSource<'_>,
) {
    let Some(span) = name.span.get() else {
        return;
    };
    if !source.has_fixup_context_in(span.start, span.end, OpaqueSurrogateContext::QuotedString) {
        return;
    }
    if forbidden.is_none_or(|current| (span.start, span.end) < (current.start, current.end)) {
        *forbidden = Some(span);
    }
}

pub(super) fn repair_module_values<W: Utf16WorkObserver>(
    module: &mut ModuleTable,
    source: &PreparedSource<'_>,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    let mut requests = Vec::new();
    let imports = module.static_imports();
    let exports = module.static_exports();
    let mut import_index = 0_usize;
    let mut export_index = 0_usize;
    while import_index < imports.len() || export_index < exports.len() {
        let take_import = match (imports.get(import_index), exports.get(export_index)) {
            (Some(import), Some(export)) => import.span.start < export.span.start,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        if take_import {
            let import = imports[import_index];
            let entries = module.static_import_entries(import.entries).ok_or_else(|| {
                TsrxParseError::Adapter("invalid static import entry range".to_string())
            })?;
            for entry in entries {
                append_module_name(&mut requests, &entry.import_name)?;
            }
            append_module_value(&mut requests, import.module_request)?;
            import_index += 1;
        } else {
            let export = exports[export_index];
            let entries = module.static_export_entries(export.entries).ok_or_else(|| {
                TsrxParseError::Adapter("invalid static export entry range".to_string())
            })?;
            let mut shared_request = None;
            for entry in entries {
                if let Some(request) = entry.module_request.get() {
                    match shared_request {
                        None => {
                            append_module_value(&mut requests, request)?;
                            shared_request = Some(request.value);
                        }
                        Some(value) if value == request.value => {}
                        Some(_) => {
                            return Err(TsrxParseError::Adapter(
                                "one export statement has multiple packed module requests"
                                    .to_string(),
                            ));
                        }
                    }
                }
                append_module_name(&mut requests, &entry.import_name)?;
                append_module_name(&mut requests, &entry.export_name)?;
                append_module_name(&mut requests, &entry.local_name)?;
            }
            export_index += 1;
        }
    }
    let mut repairs = Vec::new();
    for request in requests {
        if !source.has_fixup_context_in(
            request.span.start,
            request.span.end,
            OpaqueSurrogateContext::QuotedString,
        ) {
            continue;
        }
        let authored =
            source.original_span(request.span.start, request.span.end).ok_or_else(|| {
                TsrxParseError::Adapter("module request span is not exact".to_string())
            })?;
        let markers = javascript_quoted_pua_markers(authored)?;
        let current = module
            .text(request.value)
            .and_then(tsrx_tape_schema::PackedTextRef::as_str)
            .ok_or_else(|| {
                TsrxParseError::Adapter("unrepaired module value is not UTF-8".to_string())
            })?;
        let mut units = current.encode_utf16().collect::<Vec<_>>();
        apply_pua_markers(&mut units, &markers)?;
        repairs.push((request.value, units));
    }
    module.repair_utf16_batch(repairs.iter().map(|(range, units)| (*range, units.as_slice())))?;
    observer
        .record_copy(RepairCopyLane::Module, repairs.iter().map(|(_, units)| units.len()).sum());
    Ok(())
}

fn append_module_name<K>(
    output: &mut Vec<ValueSpanRecord>,
    name: &ModuleNameRecord<K>,
) -> Result<(), TsrxParseError> {
    if let (Some(value), Some(span)) = (name.name.get(), name.span.get()) {
        append_module_value(output, ValueSpanRecord::new(value, span))?;
    }
    Ok(())
}

fn append_module_value(
    output: &mut Vec<ValueSpanRecord>,
    value: ValueSpanRecord,
) -> Result<(), TsrxParseError> {
    if let Some(previous) = output.last() {
        if previous.value == value.value {
            return Ok(());
        }
        if (value.value.start, value.value.length) < (previous.value.start, previous.value.length) {
            return Err(TsrxParseError::Adapter(
                "module value storage is not source-insertion ordered".to_string(),
            ));
        }
    }
    output.push(value);
    Ok(())
}
