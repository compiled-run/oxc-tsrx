use tsrx_syntax::OpaqueSurrogateContext;
use tsrx_tape_schema::{
    CommentTable, DiagnosticTable, FlatTape, ModuleNameRecord, ModuleTable, ParseCompleteness,
    ProjectedCommentKind, RecordIndex, TapeSpan, ValueKind, ValueRef, ValueSpanRecord,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{TsrxParseError, TsrxParseResult, source_bridge::PreparedSource};

#[derive(Debug, Clone, Copy)]
pub(super) enum RepairCopyLane {
    ProgramRaw,
    ProgramSemantic,
    Module,
    Comment,
    Codeframe,
}

pub(super) trait Utf16WorkObserver {
    #[inline(always)]
    fn record_scan(&mut self) {}

    #[inline(always)]
    fn record_bridge(&mut self, _source: &PreparedSource<'_>) {}

    #[inline(always)]
    fn record_projection(&mut self, _projected_bytes: usize, _map_bytes: usize) {}

    #[inline(always)]
    fn record_tape(&mut self, _tape: &FlatTape) {}

    #[inline(always)]
    fn record_copy(&mut self, _lane: RepairCopyLane, _utf16_units: usize) {}

    #[inline(always)]
    fn record_program_compaction(&mut self) {}
}

pub(super) struct NoopUtf16WorkObserver;

impl Utf16WorkObserver for NoopUtf16WorkObserver {}

#[cfg(test)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Utf16Work {
    pub(super) bridge_observations: usize,
    pub(super) bridge: super::source_bridge::BridgeWork,
    pub(super) program_raw_units: usize,
    pub(super) program_semantic_units: usize,
    pub(super) module_units: usize,
    pub(super) comment_units: usize,
    pub(super) codeframe_units: usize,
    pub(super) program_compactions: usize,
}

#[cfg(test)]
impl Utf16Work {
    pub(super) fn restored_units(self) -> usize {
        self.program_raw_units
            .saturating_add(self.program_semantic_units)
            .saturating_add(self.module_units)
            .saturating_add(self.comment_units)
            .saturating_add(self.codeframe_units)
    }

    pub(super) fn restored_bytes(self) -> usize {
        self.restored_units().saturating_mul(size_of::<u16>())
    }
}

#[cfg(test)]
impl Utf16WorkObserver for Utf16Work {
    fn record_bridge(&mut self, source: &PreparedSource<'_>) {
        self.bridge_observations = self.bridge_observations.saturating_add(1);
        self.bridge = source.work();
    }

    fn record_copy(&mut self, lane: RepairCopyLane, utf16_units: usize) {
        let counter = match lane {
            RepairCopyLane::ProgramRaw => &mut self.program_raw_units,
            RepairCopyLane::ProgramSemantic => &mut self.program_semantic_units,
            RepairCopyLane::Module => &mut self.module_units,
            RepairCopyLane::Comment => &mut self.comment_units,
            RepairCopyLane::Codeframe => &mut self.codeframe_units,
        };
        *counter = counter.saturating_add(utf16_units);
    }

    fn record_program_compaction(&mut self) {
        self.program_compactions = self.program_compactions.saturating_add(1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceValueKind {
    JavaScriptLiteral,
    JsxAttributeLiteral,
    TemplateElement,
    JsxText,
    RawStyle,
}

#[derive(Debug, Clone, Copy)]
struct SourceValue {
    object: RecordIndex,
    kind: SourceValueKind,
    context: OpaqueSurrogateContext,
    span: TapeSpan,
}

struct FixupLedger<'source, 'original> {
    source: &'source PreparedSource<'original>,
    states: Vec<u8>,
}

impl<'source, 'original> FixupLedger<'source, 'original> {
    fn new(source: &'source PreparedSource<'original>) -> Self {
        Self { source, states: vec![0; source.fixups().len()] }
    }

    fn claim(
        &mut self,
        span: TapeSpan,
        context: OpaqueSurrogateContext,
    ) -> Result<(), TsrxParseError> {
        let fixups = self.source.fixups();
        let first = fixups.partition_point(|fixup| fixup.byte_start < span.start);
        let last = fixups.partition_point(|fixup| fixup.byte_start < span.end);
        for (relative, fixup) in fixups[first..last].iter().enumerate() {
            if fixup.context != Some(context) {
                continue;
            }
            let index = first + relative;
            let state = self.states.get_mut(index).ok_or_else(|| {
                TsrxParseError::Adapter("fixup ledger index is invalid".to_string())
            })?;
            if *state != 0 {
                return Err(TsrxParseError::Adapter(format!(
                    "surrogate fixup at byte {} has duplicate semantic owners",
                    fixup.byte_start
                )));
            }
            *state = 1;
        }
        Ok(())
    }

    fn claim_rejected(&mut self) -> Result<(), TsrxParseError> {
        for (index, fixup) in self.source.fixups().iter().enumerate() {
            if fixup.context.is_some() {
                continue;
            }
            let state = self.states.get_mut(index).ok_or_else(|| {
                TsrxParseError::Adapter("rejected fixup is absent from ledger".to_string())
            })?;
            if *state != 0 {
                return Err(TsrxParseError::Adapter(format!(
                    "rejected surrogate fixup at byte {} has duplicate owners",
                    fixup.byte_start
                )));
            }
            *state = 2;
        }
        Ok(())
    }

    fn finish(mut self, status: ParseCompleteness) -> Result<(), TsrxParseError> {
        if status == ParseCompleteness::Failed {
            for state in &mut self.states {
                if *state == 0 {
                    // No Program/module value is public on failure; classify the remaining source
                    // substitutions as deliberately discarded rather than semantic owners.
                    *state = 3;
                }
            }
        }
        if let Some(index) = self.states.iter().position(|state| *state == 0) {
            return Err(TsrxParseError::Adapter(format!(
                "surrogate fixup at byte {} has no semantic owner",
                self.source.fixups()[index].byte_start
            )));
        }
        Ok(())
    }
}

pub(super) fn finalize_utf16_result<W: Utf16WorkObserver>(
    result: &mut TsrxParseResult,
    source: &PreparedSource<'_>,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    if source.is_identity() {
        return Ok(());
    }
    let reachable_objects = result.program.as_ref().map(program_reachable_objects).transpose()?;
    let mut repaired_program_values = false;
    if !source.fixups().is_empty() {
        let mut ledger = FixupLedger::new(source);
        ledger.claim_rejected()?;
        if source.has_program_value_fixups()
            && let Some(program) = result.program.as_mut()
        {
            repair_program_values(
                program,
                source,
                reachable_objects
                    .as_deref()
                    .ok_or(TsrxParseError::Unsupported("missing Program reachability"))?,
                &mut ledger,
                observer,
            )?;
            repaired_program_values = true;
        }
        if source.has_context(OpaqueSurrogateContext::QuotedString)
            && let Some(module) = result.module.as_mut()
        {
            repair_module_values(module, source, observer)?;
        }
        if source.has_context(OpaqueSurrogateContext::Comment) {
            repair_comment_values(&mut result.comments, source, &mut ledger, observer)?;
        }
        repair_codeframes(&mut result.errors, source, observer)?;
        ledger.finish(result.status)?;
    }

    if let Some(program) = result.program.as_mut() {
        map_program_spans(
            program,
            source,
            reachable_objects
                .as_deref()
                .ok_or(TsrxParseError::Unsupported("missing Program reachability"))?,
        )?;
        if result.needs_compaction || repaired_program_values {
            program.compact_reachable()?;
            observer.record_program_compaction();
            result.needs_compaction = false;
        }
    }
    if let Some(module) = result.module.as_mut() {
        module.try_map_spans(|span| map_span(source, span))?;
    }
    result.comments.try_map_spans(|span| map_span(source, span))?;
    result.errors.try_map_spans(|span| map_span(source, span))?;
    Ok(())
}

pub(super) fn forbidden_module_name_span(
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

pub(super) fn forbidden_rejection_module_name_span(
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

fn map_span(source: &PreparedSource<'_>, span: TapeSpan) -> Result<TapeSpan, TsrxParseError> {
    let start = source.map_endpoint(span.start).ok_or_else(|| {
        TsrxParseError::Adapter(format!(
            "source span start {} is not an exact UTF-8 boundary",
            span.start
        ))
    })?;
    let end = source.map_endpoint(span.end).ok_or_else(|| {
        TsrxParseError::Adapter(format!(
            "source span end {} is not an exact UTF-8 boundary",
            span.end
        ))
    })?;
    Ok(TapeSpan::new(start, end))
}

fn program_reachable_objects(tape: &FlatTape) -> Result<Vec<bool>, TsrxParseError> {
    let mut objects = vec![false; tape.object_count()];
    let mut lists = vec![false; tape.list_count()];
    let mut pending = vec![tape.root()];
    while let Some(value) = pending.pop() {
        match value.kind() {
            ValueKind::Missing | ValueKind::Scalar => {}
            ValueKind::Object => {
                let object = value.as_object().ok_or_else(|| {
                    TsrxParseError::Adapter("invalid reachable object reference".to_string())
                })?;
                let index = usize::try_from(object.into_raw()).map_err(|_| {
                    TsrxParseError::Unsupported("reachable object index exceeds usize")
                })?;
                let seen = objects.get_mut(index).ok_or_else(|| {
                    TsrxParseError::Adapter("reachable object is outside table".to_string())
                })?;
                if std::mem::replace(seen, true) {
                    continue;
                }
                pending.extend(tape.fields(object).map(|field| field.value));
            }
            ValueKind::List => {
                let list = value.as_list().ok_or_else(|| {
                    TsrxParseError::Adapter("invalid reachable list reference".to_string())
                })?;
                let index = usize::try_from(list.into_raw()).map_err(|_| {
                    TsrxParseError::Unsupported("reachable list index exceeds usize")
                })?;
                let seen = lists.get_mut(index).ok_or_else(|| {
                    TsrxParseError::Adapter("reachable list is outside table".to_string())
                })?;
                if std::mem::replace(seen, true) {
                    continue;
                }
                pending.extend(tape.values(list));
            }
        }
    }
    Ok(objects)
}

fn map_program_spans(
    tape: &mut FlatTape,
    source: &PreparedSource<'_>,
    reachable_objects: &[bool],
) -> Result<(), TsrxParseError> {
    // CSS parser coordinates are relative to the `<style>` payload, not the authored module.
    // Keep the complete StyleSheet-owned graph out of source-global UTF-16 remapping, matching
    // @tsrx/core's coordinate contract while every surrounding JS/TSRX node is still mapped.
    let css_local_objects = css_local_objects(tape, reachable_objects)?;
    let mut field_updates = Vec::new();
    let mut list_updates = Vec::new();
    for raw in 0..tape.object_count() {
        if !reachable_objects.get(raw).copied().unwrap_or(false)
            || css_local_objects.get(raw).copied().unwrap_or(false)
        {
            continue;
        }
        let object = record_index(raw)?;
        for (field_index, field) in tape.fields_indexed(object) {
            match tape.key(field) {
                "start" | "end" => {
                    let byte_offset = tape.scalar_u32(field.value).ok_or_else(|| {
                        TsrxParseError::Adapter("coordinate field is not u32".to_string())
                    })?;
                    let utf16_offset = source.map_endpoint(byte_offset).ok_or_else(|| {
                        TsrxParseError::Adapter(format!(
                            "coordinate {byte_offset} is not an exact UTF-8 boundary"
                        ))
                    })?;
                    field_updates.push((field_index, utf16_offset));
                }
                "range" => {
                    let list = field.value.as_list().ok_or_else(|| {
                        TsrxParseError::Adapter("range field is not a list".to_string())
                    })?;
                    for (entry, value) in tape.values_indexed(list) {
                        let byte_offset = tape.scalar_u32(value).ok_or_else(|| {
                            TsrxParseError::Adapter("range endpoint is not u32".to_string())
                        })?;
                        let utf16_offset = source.map_endpoint(byte_offset).ok_or_else(|| {
                            TsrxParseError::Adapter(format!(
                                "range endpoint {byte_offset} is not an exact UTF-8 boundary"
                            ))
                        })?;
                        list_updates.push((entry, utf16_offset));
                    }
                }
                _ => {}
            }
        }
    }
    for (field, offset) in field_updates {
        let value = tape.push_u32_scalar(offset)?;
        tape.set_field_value(field, value)?;
    }
    for (entry, offset) in list_updates {
        let value = tape.push_u32_scalar(offset)?;
        tape.set_list_value(entry, value)?;
    }
    Ok(())
}

fn css_local_objects(
    tape: &FlatTape,
    reachable_objects: &[bool],
) -> Result<Vec<bool>, TsrxParseError> {
    let mut pending = Vec::new();
    for raw in 0..tape.object_count() {
        if reachable_objects.get(raw).copied().unwrap_or(false) {
            let object = record_index(raw)?;
            if object_type(tape, object) == Some(r#""StyleSheet""#) {
                pending.push(ValueRef::object(object));
            }
        }
    }
    if pending.is_empty() {
        return Ok(Vec::new());
    }
    let mut objects = vec![false; tape.object_count()];
    let mut lists = vec![false; tape.list_count()];
    while let Some(value) = pending.pop() {
        match value.kind() {
            ValueKind::Missing | ValueKind::Scalar => {}
            ValueKind::Object => {
                let object = value.as_object().ok_or_else(|| {
                    TsrxParseError::Adapter("invalid CSS object reference".to_string())
                })?;
                let index = usize::try_from(object.into_raw())
                    .map_err(|_| TsrxParseError::Unsupported("CSS object index exceeds usize"))?;
                let seen = objects.get_mut(index).ok_or_else(|| {
                    TsrxParseError::Adapter("CSS object is outside table".to_string())
                })?;
                if std::mem::replace(seen, true) {
                    continue;
                }
                pending.extend(tape.fields(object).map(|field| field.value));
            }
            ValueKind::List => {
                let list = value.as_list().ok_or_else(|| {
                    TsrxParseError::Adapter("invalid CSS list reference".to_string())
                })?;
                let index = usize::try_from(list.into_raw())
                    .map_err(|_| TsrxParseError::Unsupported("CSS list index exceeds usize"))?;
                let seen = lists.get_mut(index).ok_or_else(|| {
                    TsrxParseError::Adapter("CSS list is outside table".to_string())
                })?;
                if std::mem::replace(seen, true) {
                    continue;
                }
                pending.extend(tape.values(list));
            }
        }
    }
    Ok(objects)
}

fn repair_program_values<W: Utf16WorkObserver>(
    tape: &mut FlatTape,
    source: &PreparedSource<'_>,
    reachable_objects: &[bool],
    ledger: &mut FixupLedger<'_, '_>,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    let (jsx_attribute_literals, directive_parents) =
        classify_program_value_parents(tape, reachable_objects)?;
    let values = collect_program_values(tape, source, reachable_objects, &jsx_attribute_literals)?;
    for value in values {
        repair_program_value(tape, source, &directive_parents, value, observer)?;
        ledger.claim(value.span, value.context)?;
    }
    Ok(())
}

fn classify_program_value_parents(
    tape: &FlatTape,
    reachable_objects: &[bool],
) -> Result<(Vec<bool>, Vec<Option<RecordIndex>>), TsrxParseError> {
    let mut jsx_attribute_literals = vec![false; tape.object_count()];
    let mut directive_parents = vec![None; tape.object_count()];
    for raw in 0..tape.object_count() {
        if !reachable_objects.get(raw).copied().unwrap_or(false) {
            continue;
        }
        let object = record_index(raw)?;
        match object_type(tape, object) {
            Some(r#""JSXAttribute""#) => {
                if let Some(value) = object_field(tape, object, "value") {
                    let index = usize::try_from(value.into_raw()).map_err(|_| {
                        TsrxParseError::Unsupported("JSX attribute value index exceeds usize")
                    })?;
                    *jsx_attribute_literals.get_mut(index).ok_or_else(|| {
                        TsrxParseError::Adapter(
                            "JSX attribute value is outside object table".to_string(),
                        )
                    })? = true;
                }
            }
            Some(r#""ExpressionStatement""#) if tape.field_index(object, "directive").is_some() => {
                let expression = required_object_field(tape, object, "expression")?;
                let index = usize::try_from(expression.into_raw()).map_err(|_| {
                    TsrxParseError::Unsupported("directive expression index exceeds usize")
                })?;
                let parent = directive_parents.get_mut(index).ok_or_else(|| {
                    TsrxParseError::Adapter(
                        "directive expression is outside object table".to_string(),
                    )
                })?;
                if parent.replace(object).is_some() {
                    return Err(TsrxParseError::Adapter(
                        "directive expression has multiple parents".to_string(),
                    ));
                }
            }
            _ => {}
        }
    }
    Ok((jsx_attribute_literals, directive_parents))
}

fn collect_program_values(
    tape: &FlatTape,
    source: &PreparedSource<'_>,
    reachable_objects: &[bool],
    jsx_attribute_literals: &[bool],
) -> Result<Vec<SourceValue>, TsrxParseError> {
    let mut values = Vec::new();
    for raw in 0..tape.object_count() {
        if !reachable_objects.get(raw).copied().unwrap_or(false) {
            continue;
        }
        let object = record_index(raw)?;
        let Some(kind) = object_type(tape, object) else {
            continue;
        };
        let kind = match kind {
            r#""Literal""# if jsx_attribute_literals.get(raw).copied().unwrap_or(false) => {
                SourceValueKind::JsxAttributeLiteral
            }
            r#""Literal""# => SourceValueKind::JavaScriptLiteral,
            r#""TemplateElement""# => SourceValueKind::TemplateElement,
            r#""JSXText""# => SourceValueKind::JsxText,
            r#""JSXStyleElement""# => SourceValueKind::RawStyle,
            _ => continue,
        };
        let span = if kind == SourceValueKind::RawStyle {
            let Some(span) = style_payload_span(tape, object)? else {
                continue;
            };
            span
        } else {
            object_span(tape, object)?
        };
        let context = match kind {
            SourceValueKind::JavaScriptLiteral => match source
                .original_span(span.start, span.end)
                .and_then(|value| value.first().copied())
            {
                Some(unit) if unit == u16::from(b'/') => OpaqueSurrogateContext::RegexBody,
                _ => OpaqueSurrogateContext::QuotedString,
            },
            SourceValueKind::JsxAttributeLiteral => OpaqueSurrogateContext::QuotedString,
            SourceValueKind::TemplateElement => OpaqueSurrogateContext::TemplateRaw,
            SourceValueKind::JsxText => OpaqueSurrogateContext::JsxText,
            SourceValueKind::RawStyle => OpaqueSurrogateContext::RawStyle,
        };
        if source.has_fixup_context_in(span.start, span.end, context) {
            values.push(SourceValue { object, kind, context, span });
        }
    }
    Ok(values)
}

fn repair_program_value<W: Utf16WorkObserver>(
    tape: &mut FlatTape,
    source: &PreparedSource<'_>,
    directive_parents: &[Option<RecordIndex>],
    value: SourceValue,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    let authored = source
        .original_span(value.span.start, value.span.end)
        .ok_or_else(|| TsrxParseError::Adapter("source value span is not exact".to_string()))?;
    match value.kind {
        SourceValueKind::JavaScriptLiteral => {
            repair_literal(tape, value.object, authored, false, observer)?;
            let index = usize::try_from(value.object.into_raw())
                .map_err(|_| TsrxParseError::Unsupported("literal object index exceeds usize"))?;
            if let Some(parent) = directive_parents.get(index).copied().flatten() {
                replace_json_field(
                    tape,
                    parent,
                    "directive",
                    quoted_interior(authored)?,
                    RepairCopyLane::ProgramSemantic,
                    observer,
                )?;
            }
        }
        SourceValueKind::JsxAttributeLiteral => {
            repair_literal(tape, value.object, authored, true, observer)?;
        }
        SourceValueKind::TemplateElement => {
            let value_object = required_object_field(tape, value.object, "value")?;
            replace_json_field(
                tape,
                value_object,
                "raw",
                authored,
                RepairCopyLane::ProgramRaw,
                observer,
            )?;
            let markers = javascript_pua_markers(authored)?;
            patch_json_field(
                tape,
                value_object,
                "cooked",
                &markers,
                true,
                RepairCopyLane::ProgramSemantic,
                observer,
            )?;
        }
        SourceValueKind::JsxText => {
            replace_json_field(
                tape,
                value.object,
                "raw",
                authored,
                RepairCopyLane::ProgramRaw,
                observer,
            )?;
            let markers = jsx_pua_markers(authored);
            patch_json_field(
                tape,
                value.object,
                "value",
                &markers,
                false,
                RepairCopyLane::ProgramSemantic,
                observer,
            )?;
        }
        SourceValueKind::RawStyle => {
            replace_json_field(
                tape,
                value.object,
                "css",
                authored,
                RepairCopyLane::ProgramRaw,
                observer,
            )?;
        }
    }
    Ok(())
}

fn quoted_interior(authored: &[u16]) -> Result<&[u16], TsrxParseError> {
    let Some((&first, rest)) = authored.split_first() else {
        return Err(TsrxParseError::Adapter("directive literal has no opening quote".to_string()));
    };
    if first != u16::from(b'\'') && first != u16::from(b'"') {
        return Err(TsrxParseError::Adapter("directive literal is not quoted".to_string()));
    }
    let Some((&last, interior)) = rest.split_last() else {
        return Err(TsrxParseError::Adapter("directive literal has no closing quote".to_string()));
    };
    if last != first {
        return Err(TsrxParseError::Adapter("directive literal quotes do not match".to_string()));
    }
    Ok(interior)
}

fn repair_literal<W: Utf16WorkObserver>(
    tape: &mut FlatTape,
    object: RecordIndex,
    authored: &[u16],
    jsx_attribute: bool,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    replace_json_field(tape, object, "raw", authored, RepairCopyLane::ProgramRaw, observer)?;
    match authored.first().copied() {
        Some(unit) if unit == u16::from(b'\'') || unit == u16::from(b'"') => {
            let markers = if jsx_attribute {
                jsx_quoted_pua_markers(authored)?
            } else {
                javascript_quoted_pua_markers(authored)?
            };
            patch_json_field(
                tape,
                object,
                "value",
                &markers,
                false,
                RepairCopyLane::ProgramSemantic,
                observer,
            )?;
        }
        Some(unit) if unit == u16::from(b'/') => {
            let pattern = regex_pattern(authored)?;
            let regex = required_object_field(tape, object, "regex")?;
            let markers = literal_pua_markers(pattern);
            patch_json_field(
                tape,
                regex,
                "pattern",
                &markers,
                false,
                RepairCopyLane::ProgramSemantic,
                observer,
            )?;
        }
        _ => {
            return Err(TsrxParseError::Adapter(
                "surrogate-bearing Literal is neither string nor RegExp".to_string(),
            ));
        }
    }
    Ok(())
}

fn repair_module_values<W: Utf16WorkObserver>(
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

fn repair_comment_values<W: Utf16WorkObserver>(
    comments: &mut CommentTable,
    source: &PreparedSource<'_>,
    ledger: &mut FixupLedger<'_, '_>,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    let records = comments.records().to_vec();
    let mut repairs = Vec::new();
    for comment in records {
        if !source.has_fixup_in(comment.span.start, comment.span.end) {
            continue;
        }
        let authored = source
            .original_span(comment.span.start, comment.span.end)
            .ok_or_else(|| TsrxParseError::Adapter("comment span is not exact".to_string()))?;
        let value = match comment.kind {
            ProjectedCommentKind::Line => authored.get(2..).ok_or_else(|| {
                TsrxParseError::Adapter("line comment span is too short".to_string())
            })?,
            ProjectedCommentKind::Block => authored
                .get(
                    2..authored.len().checked_sub(2).ok_or_else(|| {
                        TsrxParseError::Adapter("block comment span is too short".to_string())
                    })?,
                )
                .ok_or_else(|| {
                    TsrxParseError::Adapter("block comment span is invalid".to_string())
                })?,
        };
        repairs.push((comment.value, value, comment.span));
    }
    comments.repair_utf16_batch(repairs.iter().map(|(range, value, _)| (*range, *value)))?;
    observer.record_copy(
        RepairCopyLane::Comment,
        repairs.iter().map(|(_, value, _)| value.len()).sum(),
    );
    for (_, _, span) in repairs {
        ledger.claim(span, OpaqueSurrogateContext::Comment)?;
    }
    Ok(())
}

fn repair_codeframes<W: Utf16WorkObserver>(
    diagnostics: &mut DiagnosticTable,
    source: &PreparedSource<'_>,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    if diagnostics.is_empty()
        || diagnostics.records().iter().all(|diagnostic| diagnostic.codeframe.get().is_none())
    {
        return Ok(());
    }
    let source_index = CodeframeSourceIndex::new(source)?;
    let mut repaired = Vec::new();
    for diagnostic in diagnostics.records().iter().copied() {
        let Some(range) = diagnostic.codeframe.get() else {
            continue;
        };
        let codeframe = diagnostics.string(range).ok_or_else(|| {
            TsrxParseError::Adapter("fresh diagnostic codeframe is not UTF-8".to_string())
        })?;
        let units = repair_codeframe_units(codeframe, &source_index)?;
        repaired.push((range, units));
    }
    diagnostics
        .repair_utf16_batch(repaired.iter().map(|(range, units)| (*range, units.as_slice())))?;
    observer.record_copy(
        RepairCopyLane::Codeframe,
        repaired.iter().map(|(_, units)| units.len()).sum(),
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct CodeframeSourceLine {
    number: u32,
    byte_start: u32,
    byte_end: u32,
    first_fixup: usize,
    last_fixup: usize,
}

struct CodeframeSourceIndex<'source, 'original> {
    source: &'source PreparedSource<'original>,
    lines: Vec<CodeframeSourceLine>,
}

impl<'source, 'original> CodeframeSourceIndex<'source, 'original> {
    fn new(source: &'source PreparedSource<'original>) -> Result<Self, TsrxParseError> {
        let bytes = source.source().as_bytes();
        let fixups = source.fixups();
        let mut lines = Vec::new();
        let mut byte_start = 0_usize;
        let mut number = 1_u32;
        loop {
            let byte_end = bytes[byte_start..]
                .iter()
                .position(|byte| matches!(*byte, b'\r' | b'\n'))
                .map_or(bytes.len(), |relative| byte_start + relative);
            let start = u32::try_from(byte_start)
                .map_err(|_| TsrxParseError::Unsupported("codeframe line exceeds u32"))?;
            let end = u32::try_from(byte_end)
                .map_err(|_| TsrxParseError::Unsupported("codeframe line exceeds u32"))?;
            let first_fixup = fixups.partition_point(|fixup| fixup.byte_start < start);
            let last_fixup = fixups.partition_point(|fixup| fixup.byte_start < end);
            if first_fixup != last_fixup {
                lines.push(CodeframeSourceLine {
                    number,
                    byte_start: start,
                    byte_end: end,
                    first_fixup,
                    last_fixup,
                });
            }
            if byte_end == bytes.len() {
                break;
            }
            byte_start = byte_end + 1;
            if bytes.get(byte_end) == Some(&b'\r') && bytes.get(byte_start) == Some(&b'\n') {
                byte_start += 1;
            }
            number = number
                .checked_add(1)
                .ok_or(TsrxParseError::Unsupported("codeframe line count exceeds u32"))?;
        }
        Ok(Self { source, lines })
    }

    fn line(&self, number: u32) -> Option<CodeframeSourceLine> {
        self.lines
            .binary_search_by_key(&number, |line| line.number)
            .ok()
            .map(|index| self.lines[index])
    }

    fn source_line(&self, line: CodeframeSourceLine) -> Option<&str> {
        let start = usize::try_from(line.byte_start).ok()?;
        let end = usize::try_from(line.byte_end).ok()?;
        self.source.source().get(start..end)
    }

    fn fixups(&self, line: CodeframeSourceLine) -> &[super::source_bridge::SourceFixup] {
        &self.source.fixups()[line.first_fixup..line.last_fixup]
    }
}

#[derive(Debug, Clone, Copy)]
struct RenderedSourceLine<'a> {
    number: u32,
    content: &'a str,
    content_byte_start: usize,
}

#[derive(Debug, Clone, Copy)]
struct LineAlignment {
    output_prefix: usize,
    projected_start: usize,
    visible_length: usize,
}

fn repair_codeframe_units(
    codeframe: &str,
    source: &CodeframeSourceIndex<'_, '_>,
) -> Result<Vec<u16>, TsrxParseError> {
    let mut patches = Vec::new();
    let mut line_byte_start = 0_usize;
    for rendered_with_ending in codeframe.split_inclusive('\n') {
        let rendered_text = rendered_with_ending
            .strip_suffix('\n')
            .unwrap_or(rendered_with_ending)
            .strip_suffix('\r')
            .unwrap_or_else(|| {
                rendered_with_ending.strip_suffix('\n').unwrap_or(rendered_with_ending)
            });
        let Some(rendered) = parse_rendered_source_line(rendered_text) else {
            line_byte_start += rendered_with_ending.len();
            continue;
        };
        let Some(source_line) = source.line(rendered.number) else {
            line_byte_start += rendered_with_ending.len();
            continue;
        };
        let authored_line = source.source_line(source_line).ok_or_else(|| {
            TsrxParseError::Adapter("indexed codeframe line is not UTF-8".to_string())
        })?;
        let mut mapped = map_rendered_line(
            rendered,
            authored_line,
            None,
            source.fixups(source_line),
            source_line.byte_start,
            line_byte_start,
            &mut patches,
        )?;
        if !mapped && authored_line.contains('\t') {
            let projection = expand_tabs(authored_line, 4);
            mapped = map_rendered_line(
                rendered,
                &projection.text,
                Some(&projection),
                source.fixups(source_line),
                source_line.byte_start,
                line_byte_start,
                &mut patches,
            )?;
        }
        let line_fixups = source.fixups(source_line);
        if !mapped && line_fixups.iter().any(|fixup| rendered.content.contains(fixup.placeholder()))
        {
            return Err(TsrxParseError::Adapter(format!(
                "displayed codeframe line {} could not be mapped losslessly",
                rendered.number
            )));
        }
        line_byte_start += rendered_with_ending.len();
    }
    for pair in patches.windows(2) {
        if pair[0].0 > pair[1].0 {
            return Err(TsrxParseError::Adapter(
                "codeframe patches are not emitted in rendered order".to_string(),
            ));
        }
        if pair[0].0 == pair[1].0 && pair[0] != pair[1] {
            return Err(TsrxParseError::Adapter(
                "conflicting position-keyed codeframe patches".to_string(),
            ));
        }
    }
    patches.dedup_by_key(|patch| patch.0);
    let mut output = Vec::with_capacity(codeframe.encode_utf16().count());
    let mut patches = patches.into_iter().peekable();
    for (byte_start, character) in codeframe.char_indices() {
        if patches.peek().is_some_and(|(patch_start, _, _)| *patch_start == byte_start) {
            let (_, unit, expected) = patches.next().expect("peeked codeframe patch exists");
            if character != expected {
                return Err(TsrxParseError::Adapter(
                    "codeframe patch does not target a placeholder".to_string(),
                ));
            }
            output.push(unit);
        } else {
            let mut encoded = [0_u16; 2];
            output.extend(character.encode_utf16(&mut encoded).iter().copied());
        }
    }
    if patches.next().is_some() {
        return Err(TsrxParseError::Adapter(
            "codeframe patch is outside rendered output".to_string(),
        ));
    }
    Ok(output)
}

fn parse_rendered_source_line(line: &str) -> Option<RenderedSourceLine<'_>> {
    let ascii = line.find('|').map(|index| (index, 1_usize));
    let unicode = line.find('│').map(|index| (index, '│'.len_utf8()));
    let (separator, separator_width) = match (ascii, unicode) {
        (Some(ascii), Some(unicode)) => ascii.min(unicode),
        (Some(ascii), None) => ascii,
        (None, Some(unicode)) => unicode,
        (None, None) => return None,
    };
    let number = line.get(..separator)?.trim().parse::<u32>().ok()?;
    let mut content_byte_start = separator.checked_add(separator_width)?;
    if line.get(content_byte_start..).is_some_and(|content| content.starts_with(' ')) {
        content_byte_start += 1;
    }
    Some(RenderedSourceLine {
        number,
        content: line.get(content_byte_start..)?,
        content_byte_start,
    })
}

#[derive(Debug)]
struct TabProjection {
    text: String,
    source_to_display: Vec<(usize, usize)>,
}

fn expand_tabs(source: &str, tab_width: usize) -> TabProjection {
    let mut text = String::with_capacity(source.len());
    let mut source_to_display = Vec::with_capacity(source.chars().count());
    let graphemes = (!source.is_ascii()).then(|| {
        source
            .grapheme_indices(true)
            .map(|(byte_start, grapheme)| (byte_start, grapheme.width()))
            .collect::<Vec<_>>()
    });
    let mut grapheme_index = 0_usize;
    let mut column = 0_usize;
    let mut escaped = false;
    for (source_byte, character) in source.char_indices() {
        source_to_display.push((source_byte, text.len()));
        let width = match (escaped, character) {
            (false, '\t') => tab_width - column % tab_width,
            (false, '\u{1b}') => {
                escaped = true;
                0
            }
            (false, _) => graphemes.as_ref().map_or(1, |boundaries| {
                if boundaries
                    .get(grapheme_index)
                    .is_some_and(|(byte_start, _)| *byte_start == source_byte)
                {
                    let width = boundaries[grapheme_index].1;
                    grapheme_index += 1;
                    width
                } else {
                    0
                }
            }),
            (true, 'm') => {
                escaped = false;
                0
            }
            (true, _) => 0,
        };
        if character == '\t' {
            text.extend(std::iter::repeat_n(' ', width));
        } else {
            text.push(character);
        }
        column += width;
    }
    TabProjection { text, source_to_display }
}

fn map_rendered_line(
    rendered: RenderedSourceLine<'_>,
    projected: &str,
    tab_projection: Option<&TabProjection>,
    fixups: &[super::source_bridge::SourceFixup],
    source_byte_start: u32,
    rendered_line_start: usize,
    patches: &mut Vec<(usize, u16, char)>,
) -> Result<bool, TsrxParseError> {
    let Some(alignment) = align_rendered_content(rendered.content, projected) else {
        return Ok(false);
    };
    let visible_end = alignment
        .projected_start
        .checked_add(alignment.visible_length)
        .ok_or_else(|| TsrxParseError::Adapter("codeframe alignment overflow".to_string()))?;
    for fixup in fixups {
        let relative =
            usize::try_from(fixup.byte_start.checked_sub(source_byte_start).ok_or_else(|| {
                TsrxParseError::Adapter("codeframe fixup precedes its line".to_string())
            })?)
            .map_err(|_| TsrxParseError::Adapter("codeframe fixup overflow".to_string()))?;
        let projected_byte = if let Some(projection) = tab_projection {
            projection
                .source_to_display
                .binary_search_by_key(&relative, |(source_byte, _)| *source_byte)
                .ok()
                .map(|index| projection.source_to_display[index].1)
                .ok_or_else(|| {
                    TsrxParseError::Adapter("tab-expanded fixup is not at a character".to_string())
                })?
        } else {
            relative
        };
        if projected_byte < alignment.projected_start || projected_byte >= visible_end {
            continue;
        }
        let patch = rendered_line_start
            .checked_add(rendered.content_byte_start)
            .and_then(|value| value.checked_add(alignment.output_prefix))
            .and_then(|value| value.checked_add(projected_byte - alignment.projected_start))
            .ok_or_else(|| TsrxParseError::Adapter("codeframe patch overflow".to_string()))?;
        patches.push((patch, fixup.unit, fixup.placeholder()));
    }
    Ok(true)
}

fn align_rendered_content(rendered: &str, projected: &str) -> Option<LineAlignment> {
    if rendered == projected {
        return Some(LineAlignment {
            output_prefix: 0,
            projected_start: 0,
            visible_length: rendered.len(),
        });
    }
    if let Some(start) = unique_substring(projected, rendered) {
        return Some(LineAlignment {
            output_prefix: 0,
            projected_start: start,
            visible_length: rendered.len(),
        });
    }
    let (output_prefix, core) = if let Some(core) = rendered.strip_prefix("...") {
        (3, core)
    } else if let Some(core) = rendered.strip_prefix('…') {
        ('…'.len_utf8(), core)
    } else {
        (0, rendered)
    };
    let core = core.strip_suffix("...").or_else(|| core.strip_suffix('…')).unwrap_or(core);
    let start = unique_substring(projected, core)?;
    Some(LineAlignment { output_prefix, projected_start: start, visible_length: core.len() })
}

fn unique_substring(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    let mut matches = haystack.match_indices(needle);
    let first = matches.next()?.0;
    matches.next().is_none().then_some(first)
}

fn object_type(tape: &FlatTape, object: RecordIndex) -> Option<&str> {
    let field = tape.field_index(object, "type")?;
    tape.scalar(tape.field_value(field)?)
}

fn object_span(tape: &FlatTape, object: RecordIndex) -> Result<TapeSpan, TsrxParseError> {
    Ok(TapeSpan::new(
        scalar_u32_field(tape, object, "start")?,
        scalar_u32_field(tape, object, "end")?,
    ))
}

fn scalar_u32_field(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<u32, TsrxParseError> {
    let field = tape
        .field_index(object, name)
        .ok_or_else(|| TsrxParseError::Adapter(format!("object is missing `{name}` coordinate")))?;
    tape.scalar_u32(
        tape.field_value(field)
            .ok_or_else(|| TsrxParseError::Adapter("invalid coordinate field".to_string()))?,
    )
    .ok_or_else(|| TsrxParseError::Adapter("coordinate is not u32".to_string()))
}

fn object_field(tape: &FlatTape, object: RecordIndex, name: &str) -> Option<RecordIndex> {
    tape.field_index(object, name)
        .and_then(|field| tape.field_value(field))
        .and_then(ValueRef::as_object)
}

fn required_object_field(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<RecordIndex, TsrxParseError> {
    object_field(tape, object, name)
        .ok_or_else(|| TsrxParseError::Adapter(format!("`{name}` is not an object")))
}

fn style_payload_span(
    tape: &FlatTape,
    style: RecordIndex,
) -> Result<Option<TapeSpan>, TsrxParseError> {
    let opening = required_object_field(tape, style, "openingElement")?;
    let Some(closing) = object_field(tape, style, "closingElement") else {
        return Ok(None);
    };
    let start = scalar_u32_field(tape, opening, "end")?;
    let end = scalar_u32_field(tape, closing, "start")?;
    if start > end {
        return Err(TsrxParseError::Adapter(
            "style payload has inverted child boundaries".to_string(),
        ));
    }
    Ok(Some(TapeSpan::new(start, end)))
}

fn replace_json_field<W: Utf16WorkObserver>(
    tape: &mut FlatTape,
    object: RecordIndex,
    name: &str,
    value: &[u16],
    lane: RepairCopyLane,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    let field = tape
        .field_index(object, name)
        .ok_or_else(|| TsrxParseError::Adapter(format!("missing `{name}` scalar field")))?;
    if tape.field_value(field).is_none_or(|value| value.kind() != ValueKind::Scalar) {
        return Err(TsrxParseError::Adapter(format!("`{name}` is not a scalar field")));
    }
    let restored_units = value.len();
    let value = tape.push_json_utf16_scalar(value)?;
    tape.set_field_value(field, value)?;
    observer.record_copy(lane, restored_units);
    Ok(())
}

fn patch_json_field<W: Utf16WorkObserver>(
    tape: &mut FlatTape,
    object: RecordIndex,
    name: &str,
    markers: &[Option<u16>],
    allow_null: bool,
    lane: RepairCopyLane,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    let field = tape
        .field_index(object, name)
        .ok_or_else(|| TsrxParseError::Adapter(format!("missing `{name}` scalar field")))?;
    let value = tape
        .field_value(field)
        .and_then(|value| tape.scalar(value))
        .ok_or_else(|| TsrxParseError::Adapter(format!("`{name}` is not a scalar field")))?
        .to_owned();
    if allow_null && value == "null" {
        return Ok(());
    }
    let mut units = decode_json_string(&value)?;
    apply_pua_markers(&mut units, markers)?;
    let value = tape.push_json_utf16_scalar(&units)?;
    tape.set_field_value(field, value)?;
    observer.record_copy(lane, units.len());
    Ok(())
}

fn decode_json_string(value: &str) -> Result<Vec<u16>, TsrxParseError> {
    let inner = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| TsrxParseError::Adapter("OXC scalar is not a JSON string".to_string()))?;
    let mut output = Vec::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            let mut encoded = [0_u16; 2];
            output.extend(character.encode_utf16(&mut encoded).iter().copied());
            continue;
        }
        let escaped = characters.next().ok_or_else(|| {
            TsrxParseError::Adapter("OXC JSON scalar ends in backslash".to_string())
        })?;
        match escaped {
            '"' => output.push(u16::from(b'"')),
            '\\' => output.push(u16::from(b'\\')),
            '/' => output.push(u16::from(b'/')),
            'b' => output.push(0x08),
            'f' => output.push(0x0c),
            'n' => output.push(0x0a),
            'r' => output.push(0x0d),
            't' => output.push(0x09),
            'u' => {
                let mut scalar = 0_u16;
                for _ in 0..4 {
                    let digit = characters.next().and_then(|value| value.to_digit(16)).ok_or_else(
                        || TsrxParseError::Adapter("invalid OXC JSON Unicode escape".to_string()),
                    )?;
                    scalar = scalar
                        .checked_mul(16)
                        .and_then(|value| value.checked_add(u16::try_from(digit).ok()?))
                        .ok_or_else(|| {
                            TsrxParseError::Adapter("OXC JSON Unicode escape overflow".to_string())
                        })?;
                }
                output.push(scalar);
            }
            _ => {
                return Err(TsrxParseError::Adapter("invalid OXC JSON string escape".to_string()));
            }
        }
    }
    Ok(output)
}

fn apply_pua_markers(value: &mut [u16], markers: &[Option<u16>]) -> Result<(), TsrxParseError> {
    let positions = value
        .iter()
        .enumerate()
        .filter_map(|(index, unit)| (*unit == 0xe000).then_some(index))
        .collect::<Vec<_>>();
    if positions.len() != markers.len() {
        return Err(TsrxParseError::Adapter(format!(
            "OXC placeholder count {} does not match source producer count {}",
            positions.len(),
            markers.len()
        )));
    }
    for (position, marker) in positions.into_iter().zip(markers) {
        if let Some(unit) = marker {
            value[position] = *unit;
        }
    }
    Ok(())
}

fn javascript_quoted_pua_markers(value: &[u16]) -> Result<Vec<Option<u16>>, TsrxParseError> {
    let Some((&quote, inner)) = value.split_first() else {
        return Err(TsrxParseError::Adapter("empty quoted source span".to_string()));
    };
    if !matches!(quote, unit if unit == u16::from(b'\'') || unit == u16::from(b'"'))
        || inner.last().copied() != Some(quote)
    {
        return Err(TsrxParseError::Adapter(
            "quoted source span has unmatched delimiters".to_string(),
        ));
    }
    javascript_pua_markers(&inner[..inner.len() - 1])
}

fn javascript_pua_markers(value: &[u16]) -> Result<Vec<Option<u16>>, TsrxParseError> {
    let mut markers = Vec::new();
    let mut index = 0_usize;
    while index < value.len() {
        if value[index] != u16::from(b'\\') {
            index += push_literal_marker(value, index, &mut markers);
            continue;
        }
        let escaped = *value.get(index + 1).ok_or_else(|| {
            TsrxParseError::Adapter("parsed source ends in a backslash".to_string())
        })?;
        if escaped == 0xe000 || (0xd800..=0xdfff).contains(&escaped) {
            index += 1 + push_literal_marker(value, index + 1, &mut markers);
            continue;
        }
        if escaped == u16::from(b'u') {
            if value.get(index + 2).copied() == Some(u16::from(b'{')) {
                let close = value[index + 3..]
                    .iter()
                    .position(|unit| *unit == u16::from(b'}'))
                    .map(|relative| index + 3 + relative)
                    .ok_or_else(|| {
                        TsrxParseError::Adapter("parsed braced Unicode escape is open".to_string())
                    })?;
                if parse_ascii_radix(&value[index + 3..close], 16) == Some(0xe000) {
                    markers.push(None);
                }
                index = close + 1;
                continue;
            }
            let end = index.checked_add(6).ok_or_else(|| {
                TsrxParseError::Adapter("Unicode escape index overflow".to_string())
            })?;
            let digits = value.get(index + 2..end).ok_or_else(|| {
                TsrxParseError::Adapter("parsed Unicode escape is truncated".to_string())
            })?;
            if parse_ascii_radix(digits, 16) == Some(0xe000) {
                markers.push(None);
            }
            index = end;
            continue;
        }
        if escaped == u16::from(b'x') {
            index = index
                .checked_add(4)
                .ok_or_else(|| TsrxParseError::Adapter("hex escape index overflow".to_string()))?;
            continue;
        }
        if escaped == u16::from(b'\r') && value.get(index + 2).copied() == Some(u16::from(b'\n')) {
            index += 3;
        } else {
            index += 2;
        }
    }
    Ok(markers)
}

fn jsx_quoted_pua_markers(value: &[u16]) -> Result<Vec<Option<u16>>, TsrxParseError> {
    let Some((&quote, inner)) = value.split_first() else {
        return Err(TsrxParseError::Adapter("empty JSX quoted span".to_string()));
    };
    if inner.last().copied() != Some(quote) {
        return Err(TsrxParseError::Adapter(
            "JSX quoted span has unmatched delimiters".to_string(),
        ));
    }
    Ok(jsx_pua_markers(&inner[..inner.len() - 1]))
}

fn jsx_pua_markers(value: &[u16]) -> Vec<Option<u16>> {
    // OXC remains authoritative for JSX entity normalization. Actual source scalars and lone
    // units are the only position-keyed placeholder producers patched here.
    literal_pua_markers(value)
}

fn literal_pua_markers(value: &[u16]) -> Vec<Option<u16>> {
    let mut markers = Vec::new();
    let mut index = 0_usize;
    while index < value.len() {
        index += push_literal_marker(value, index, &mut markers);
    }
    markers
}

fn push_literal_marker(value: &[u16], index: usize, output: &mut Vec<Option<u16>>) -> usize {
    let unit = value[index];
    if (0xd800..=0xdbff).contains(&unit)
        && value.get(index + 1).is_some_and(|next| (0xdc00..=0xdfff).contains(next))
    {
        return 2;
    }
    if unit == 0xe000 {
        output.push(None);
    } else if (0xd800..=0xdfff).contains(&unit) {
        output.push(Some(unit));
    }
    1
}

fn parse_ascii_radix(value: &[u16], radix: u32) -> Option<u32> {
    if value.is_empty() {
        return None;
    }
    value.iter().try_fold(0_u32, |output, unit| {
        let character = char::from_u32(u32::from(*unit))?;
        let digit = character.to_digit(radix)?;
        output.checked_mul(radix)?.checked_add(digit)
    })
}

fn regex_pattern(value: &[u16]) -> Result<&[u16], TsrxParseError> {
    if value.first().copied() != Some(u16::from(b'/')) {
        return Err(TsrxParseError::Adapter("RegExp does not start with slash".to_string()));
    }
    let mut escaped = false;
    let mut in_class = false;
    for index in 1..value.len() {
        match value[index] {
            _ if escaped => escaped = false,
            unit if unit == u16::from(b'\\') => escaped = true,
            unit if unit == u16::from(b'[') => in_class = true,
            unit if unit == u16::from(b']') => in_class = false,
            unit if unit == u16::from(b'/') && !in_class => return Ok(&value[1..index]),
            _ => {}
        }
    }
    Err(TsrxParseError::Adapter("RegExp has no closing slash".to_string()))
}

fn record_index(value: usize) -> Result<RecordIndex, TsrxParseError> {
    u32::try_from(value)
        .map(RecordIndex::new)
        .map_err(|_| TsrxParseError::Unsupported("record index exceeds u32"))
}
