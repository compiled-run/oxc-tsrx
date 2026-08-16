use napi::{
    Env,
    bindgen_prelude::{
        Array, JsObjectValue, Null, Object, ToNapiValue, Uint32Array, Unknown, Utf16String,
    },
};
use napi_derive::napi;
use oxc_adapter::parser::{
    OrdinaryComment, OrdinaryDiagnostic, OrdinaryDynamicImport, OrdinaryExportExportName,
    OrdinaryExportImportName, OrdinaryExportLocalName, OrdinaryImportName, OrdinaryModule,
    OrdinaryNameKind, OrdinarySpan, OrdinaryStaticExport, OrdinaryStaticExportEntry,
    OrdinaryStaticImport, OrdinaryStaticImportEntry, OrdinaryValueSpan,
};
use tsrx_tape_schema::{
    CommentTable, DiagnosticSeverity, DiagnosticTable, ExportExportNameKind, ExportImportNameKind,
    ExportLocalNameKind, FlatTape, ImportNameKind, ListRange, ModuleNameRecord, ModuleTable,
    OptionalStringRange, OptionalValueSpanRecord, OwnedPackedTextStorage, PackedTextRef,
    ProgramBinaryTransfer, ProjectedCommentKind, StaticExportEntryRecord, StaticExportRecord,
    StaticImportEntryRecord, StaticImportRecord, StringRange, TapeSpan, ValueSpanRecord,
};

fn invariant(message: impl Into<String>) -> napi::Error {
    napi::Error::from_reason(format!("ERR_TSRX_INVALID_ARGUMENT: {}", message.into()))
}

fn resource_exhausted(message: impl Into<String>) -> napi::Error {
    napi::Error::from_reason(format!("ERR_TSRX_RESOURCE_EXHAUSTED: {}", message.into()))
}

fn array_length(length: usize) -> napi::Result<u32> {
    u32::try_from(length)
        .map_err(|_| resource_exhausted("JavaScript array exceeds the 32-bit tape limit"))
}

fn string_unknown<'env>(env: &'env Env, value: &str) -> napi::Result<Unknown<'env>> {
    let value = env.create_string(value)?;
    (&value).into_unknown(env)
}

fn packed_text_unknown<'env>(
    env: &'env Env,
    value: PackedTextRef<'_>,
) -> napi::Result<Unknown<'env>> {
    value.as_str().map_or_else(
        || Utf16String::from(value.to_utf16()).into_unknown(env),
        |value| string_unknown(env, value),
    )
}

fn owned_text_unknown<'env>(
    env: &'env Env,
    storage: &OwnedPackedTextStorage,
    range: StringRange,
) -> napi::Result<Unknown<'env>> {
    packed_text_unknown(
        env,
        storage.text(range).ok_or_else(|| invariant("packed string range is invalid"))?,
    )
}

fn null(env: &Env) -> napi::Result<Unknown<'_>> {
    Null.into_unknown(env)
}

fn span_object(env: &Env, span: TapeSpan) -> napi::Result<Object<'_>> {
    let mut output = Object::new(env)?;
    output.set_c_named_property(c"start", span.start)?;
    output.set_c_named_property(c"end", span.end)?;
    Ok(output)
}

#[napi(object, object_from_js = false, object_to_js = true)]
pub(crate) struct OrdinaryJsSpan {
    pub start: u32,
    pub end: u32,
}

impl From<OrdinarySpan> for OrdinaryJsSpan {
    fn from(value: OrdinarySpan) -> Self {
        Self { start: value.start, end: value.end }
    }
}

#[napi(object, object_from_js = false, object_to_js = true)]
pub(crate) struct OrdinaryJsValueSpan {
    pub value: String,
    pub start: u32,
    pub end: u32,
}

impl From<OrdinaryValueSpan> for OrdinaryJsValueSpan {
    fn from(value: OrdinaryValueSpan) -> Self {
        Self { value: value.value, start: value.start, end: value.end }
    }
}

#[napi(object, use_nullable = true, object_from_js = false, object_to_js = true)]
pub(crate) struct OrdinaryJsName {
    pub kind: &'static str,
    pub name: Option<String>,
    pub start: Option<u32>,
    pub end: Option<u32>,
}

fn ordinary_js_name(
    kind: OrdinaryNameKind,
    name: Option<String>,
    start: Option<u32>,
    end: Option<u32>,
) -> OrdinaryJsName {
    OrdinaryJsName { kind: kind.as_str(), name, start, end }
}

impl From<OrdinaryImportName> for OrdinaryJsName {
    fn from(value: OrdinaryImportName) -> Self {
        ordinary_js_name(value.kind, value.name, value.start, value.end)
    }
}

impl From<OrdinaryExportImportName> for OrdinaryJsName {
    fn from(value: OrdinaryExportImportName) -> Self {
        ordinary_js_name(value.kind, value.name, value.start, value.end)
    }
}

impl From<OrdinaryExportExportName> for OrdinaryJsName {
    fn from(value: OrdinaryExportExportName) -> Self {
        ordinary_js_name(value.kind, value.name, value.start, value.end)
    }
}

impl From<OrdinaryExportLocalName> for OrdinaryJsName {
    fn from(value: OrdinaryExportLocalName) -> Self {
        ordinary_js_name(value.kind, value.name, value.start, value.end)
    }
}

#[napi(object, object_from_js = false, object_to_js = true)]
pub(crate) struct OrdinaryJsStaticImportEntry {
    pub import_name: OrdinaryJsName,
    pub local_name: OrdinaryJsValueSpan,
    pub is_type: bool,
}

impl From<OrdinaryStaticImportEntry> for OrdinaryJsStaticImportEntry {
    fn from(value: OrdinaryStaticImportEntry) -> Self {
        Self {
            import_name: value.import_name.into(),
            local_name: value.local_name.into(),
            is_type: value.is_type,
        }
    }
}

#[napi(object, object_from_js = false, object_to_js = true)]
pub(crate) struct OrdinaryJsStaticImport {
    pub start: u32,
    pub end: u32,
    pub module_request: OrdinaryJsValueSpan,
    pub entries: Vec<OrdinaryJsStaticImportEntry>,
}

impl From<OrdinaryStaticImport> for OrdinaryJsStaticImport {
    fn from(value: OrdinaryStaticImport) -> Self {
        Self {
            start: value.start,
            end: value.end,
            module_request: value.module_request.into(),
            entries: value.entries.into_iter().map(Into::into).collect(),
        }
    }
}

#[napi(object, use_nullable = true, object_from_js = false, object_to_js = true)]
pub(crate) struct OrdinaryJsStaticExportEntry {
    pub start: u32,
    pub end: u32,
    pub module_request: Option<OrdinaryJsValueSpan>,
    pub import_name: OrdinaryJsName,
    pub export_name: OrdinaryJsName,
    pub local_name: OrdinaryJsName,
    pub is_type: bool,
}

impl From<OrdinaryStaticExportEntry> for OrdinaryJsStaticExportEntry {
    fn from(value: OrdinaryStaticExportEntry) -> Self {
        Self {
            start: value.start,
            end: value.end,
            module_request: value.module_request.map(Into::into),
            import_name: value.import_name.into(),
            export_name: value.export_name.into(),
            local_name: value.local_name.into(),
            is_type: value.is_type,
        }
    }
}

#[napi(object, object_from_js = false, object_to_js = true)]
pub(crate) struct OrdinaryJsStaticExport {
    pub start: u32,
    pub end: u32,
    pub entries: Vec<OrdinaryJsStaticExportEntry>,
}

impl From<OrdinaryStaticExport> for OrdinaryJsStaticExport {
    fn from(value: OrdinaryStaticExport) -> Self {
        Self {
            start: value.start,
            end: value.end,
            entries: value.entries.into_iter().map(Into::into).collect(),
        }
    }
}

#[napi(object, object_from_js = false, object_to_js = true)]
pub(crate) struct OrdinaryJsDynamicImport {
    pub start: u32,
    pub end: u32,
    pub module_request: OrdinaryJsSpan,
}

impl From<OrdinaryDynamicImport> for OrdinaryJsDynamicImport {
    fn from(value: OrdinaryDynamicImport) -> Self {
        Self { start: value.start, end: value.end, module_request: value.module_request.into() }
    }
}

#[napi(object, object_from_js = false, object_to_js = true)]
pub(crate) struct OrdinaryJsModule {
    pub has_module_syntax: bool,
    pub static_imports: Vec<OrdinaryJsStaticImport>,
    pub static_exports: Vec<OrdinaryJsStaticExport>,
    pub dynamic_imports: Vec<OrdinaryJsDynamicImport>,
    pub import_metas: Vec<OrdinaryJsSpan>,
}

impl From<OrdinaryModule> for OrdinaryJsModule {
    fn from(value: OrdinaryModule) -> Self {
        Self {
            has_module_syntax: value.has_module_syntax,
            static_imports: value.static_imports.into_iter().map(Into::into).collect(),
            static_exports: value.static_exports.into_iter().map(Into::into).collect(),
            dynamic_imports: value.dynamic_imports.into_iter().map(Into::into).collect(),
            import_metas: value.import_metas.into_iter().map(Into::into).collect(),
        }
    }
}

pub(super) fn materialize_ordinary_module(
    env: &Env,
    module: OrdinaryModule,
) -> napi::Result<Unknown<'_>> {
    OrdinaryJsModule::from(module).into_unknown(env)
}

pub(super) fn materialize_ordinary_comments(
    env: &Env,
    comments: Vec<OrdinaryComment>,
) -> napi::Result<Unknown<'_>> {
    let mut output = env.create_array(array_length(comments.len())?)?;
    for (index, comment) in comments.into_iter().enumerate() {
        let mut value = Object::new(env)?;
        value.set_c_named_property(c"type", comment.kind)?;
        value.set_c_named_property(c"value", comment.value)?;
        value.set_c_named_property(c"start", comment.start)?;
        value.set_c_named_property(c"end", comment.end)?;
        output
            .set(u32::try_from(index).map_err(|_| invariant("comment index overflow"))?, value)?;
    }
    output.into_unknown(env)
}

pub(super) fn materialize_ordinary_diagnostics(
    env: &Env,
    diagnostics: Vec<OrdinaryDiagnostic>,
) -> napi::Result<Unknown<'_>> {
    let mut output = env.create_array(array_length(diagnostics.len())?)?;
    for (index, diagnostic) in diagnostics.into_iter().enumerate() {
        let mut labels = env.create_array(array_length(diagnostic.labels.len())?)?;
        for (label_index, label) in diagnostic.labels.into_iter().enumerate() {
            let mut value = Object::new(env)?;
            value.set_c_named_property(
                c"message",
                label.message.map_or_else(|| null(env), |message| message.into_unknown(env))?,
            )?;
            value.set_c_named_property(c"start", label.start)?;
            value.set_c_named_property(c"end", label.end)?;
            labels.set(
                u32::try_from(label_index).map_err(|_| invariant("label index overflow"))?,
                value,
            )?;
        }
        let mut value = Object::new(env)?;
        value.set_c_named_property(c"severity", diagnostic.severity)?;
        value.set_c_named_property(c"message", diagnostic.message)?;
        value.set_c_named_property(c"labels", labels)?;
        value.set_c_named_property(
            c"helpMessage",
            diagnostic
                .help_message
                .map_or_else(|| null(env), |message| message.into_unknown(env))?,
        )?;
        value.set_c_named_property(c"codeframe", diagnostic.codeframe)?;
        output.set(
            u32::try_from(index).map_err(|_| invariant("diagnostic index overflow"))?,
            value,
        )?;
    }
    output.into_unknown(env)
}

fn transfer_error(error: tsrx_tape_schema::TapeBuildError) -> napi::Error {
    match error {
        tsrx_tape_schema::TapeBuildError::CapacityOverflow => {
            resource_exhausted("Program transfer exceeds its bounded capacity")
        }
        tsrx_tape_schema::TapeBuildError::InvalidRecordIndex => {
            invariant("Program transfer contains an invalid tape record")
        }
    }
}

fn transfer_string(
    output: Result<String, tsrx_tape_schema::TapeBuildError>,
) -> napi::Result<String> {
    output.map_err(transfer_error)
}

pub(super) fn program_transfer_string(tape: FlatTape) -> napi::Result<String> {
    transfer_string(tape.program_transfer_owned())
}

#[napi(object, object_from_js = false, object_to_js = true)]
pub struct NativeProgramTransfer {
    pub metadata: String,
    pub words: Uint32Array,
}

pub(super) fn program_transfer_engine_binary(
    tape: FlatTape,
) -> napi::Result<NativeProgramTransfer> {
    tape.program_transfer_tsrx_core_compat_binary_owned()
        .map(|ProgramBinaryTransfer { metadata, words }| NativeProgramTransfer {
            metadata,
            words: words.into(),
        })
        .map_err(transfer_error)
}

pub(super) fn materialize_program(env: &Env, tape: FlatTape) -> napi::Result<Unknown<'_>> {
    program_transfer_string(tape)?.into_unknown(env)
}

fn list_slice<T>(values: &[T], range: ListRange) -> napi::Result<&[T]> {
    let start = usize::try_from(range.start).map_err(|_| invariant("list start overflow"))?;
    let length = usize::try_from(range.length).map_err(|_| invariant("list length overflow"))?;
    let end = start.checked_add(length).ok_or_else(|| invariant("list range overflow"))?;
    values.get(start..end).ok_or_else(|| invariant("packed list range is invalid"))
}

fn value_span<'env>(
    env: &'env Env,
    storage: &OwnedPackedTextStorage,
    value: ValueSpanRecord,
) -> napi::Result<Object<'env>> {
    let mut output = Object::new(env)?;
    output.set_c_named_property(c"value", owned_text_unknown(env, storage, value.value)?)?;
    output.set_c_named_property(c"start", value.span.start)?;
    output.set_c_named_property(c"end", value.span.end)?;
    Ok(output)
}

fn optional_value_span<'env>(
    env: &'env Env,
    storage: &OwnedPackedTextStorage,
    value: OptionalValueSpanRecord,
) -> napi::Result<Unknown<'env>> {
    value
        .get()
        .map_or_else(|| null(env), |value| value_span(env, storage, value)?.into_unknown(env))
}

trait ModuleKind {
    fn name(self) -> &'static str;
}

impl ModuleKind for ImportNameKind {
    fn name(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::NamespaceObject => "NamespaceObject",
            Self::Default => "Default",
        }
    }
}

impl ModuleKind for ExportImportNameKind {
    fn name(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::All => "All",
            Self::AllButDefault => "AllButDefault",
            Self::None => "None",
        }
    }
}

impl ModuleKind for ExportExportNameKind {
    fn name(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Default => "Default",
            Self::None => "None",
        }
    }
}

impl ModuleKind for ExportLocalNameKind {
    fn name(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Default => "Default",
            Self::None => "None",
        }
    }
}

fn module_name<'env, K: ModuleKind + Copy>(
    env: &'env Env,
    storage: &OwnedPackedTextStorage,
    value: ModuleNameRecord<K>,
) -> napi::Result<Object<'env>> {
    let mut output = Object::new(env)?;
    output.set_c_named_property(c"kind", value.kind.name())?;
    output.set_c_named_property(
        c"name",
        value
            .name
            .get()
            .map_or_else(|| null(env), |range| owned_text_unknown(env, storage, range))?,
    )?;
    let span = value.span.get();
    output.set_c_named_property(
        c"start",
        span.map_or_else(|| null(env), |span| span.start.into_unknown(env))?,
    )?;
    output.set_c_named_property(
        c"end",
        span.map_or_else(|| null(env), |span| span.end.into_unknown(env))?,
    )?;
    Ok(output)
}

fn materialize_static_imports<'env>(
    env: &'env Env,
    storage: &OwnedPackedTextStorage,
    imports: Vec<StaticImportRecord>,
    import_entries: &[StaticImportEntryRecord],
) -> napi::Result<Array<'env>> {
    let mut output = env.create_array(array_length(imports.len())?)?;
    for (index, record) in imports.into_iter().enumerate() {
        let entries = list_slice(import_entries, record.entries)?;
        let mut js_entries = env.create_array(array_length(entries.len())?)?;
        for (entry_index, entry) in entries.iter().copied().enumerate() {
            let mut js_entry = Object::new(env)?;
            js_entry.set_c_named_property(
                c"importName",
                module_name(env, storage, entry.import_name)?,
            )?;
            js_entry
                .set_c_named_property(c"localName", value_span(env, storage, entry.local_name)?)?;
            js_entry.set_c_named_property(c"isType", entry.is_type)?;
            js_entries.set(
                u32::try_from(entry_index).map_err(|_| invariant("import entry overflow"))?,
                js_entry,
            )?;
        }
        let mut js_record = Object::new(env)?;
        js_record.set_c_named_property(c"start", record.span.start)?;
        js_record.set_c_named_property(c"end", record.span.end)?;
        js_record.set_c_named_property(
            c"moduleRequest",
            value_span(env, storage, record.module_request)?,
        )?;
        js_record.set_c_named_property(c"entries", js_entries)?;
        output.set(
            u32::try_from(index).map_err(|_| invariant("static import overflow"))?,
            js_record,
        )?;
    }
    Ok(output)
}

fn materialize_static_exports<'env>(
    env: &'env Env,
    storage: &OwnedPackedTextStorage,
    exports: Vec<StaticExportRecord>,
    export_entries: &[StaticExportEntryRecord],
) -> napi::Result<Array<'env>> {
    let mut output = env.create_array(array_length(exports.len())?)?;
    for (index, record) in exports.into_iter().enumerate() {
        let entries = list_slice(export_entries, record.entries)?;
        let mut js_entries = env.create_array(array_length(entries.len())?)?;
        for (entry_index, entry) in entries.iter().copied().enumerate() {
            let mut js_entry = Object::new(env)?;
            js_entry.set_c_named_property(c"start", entry.span.start)?;
            js_entry.set_c_named_property(c"end", entry.span.end)?;
            js_entry.set_c_named_property(
                c"moduleRequest",
                optional_value_span(env, storage, entry.module_request)?,
            )?;
            js_entry.set_c_named_property(
                c"importName",
                module_name(env, storage, entry.import_name)?,
            )?;
            js_entry.set_c_named_property(
                c"exportName",
                module_name(env, storage, entry.export_name)?,
            )?;
            js_entry
                .set_c_named_property(c"localName", module_name(env, storage, entry.local_name)?)?;
            js_entry.set_c_named_property(c"isType", entry.is_type)?;
            js_entries.set(
                u32::try_from(entry_index).map_err(|_| invariant("export entry overflow"))?,
                js_entry,
            )?;
        }
        let mut js_record = Object::new(env)?;
        js_record.set_c_named_property(c"start", record.span.start)?;
        js_record.set_c_named_property(c"end", record.span.end)?;
        js_record.set_c_named_property(c"entries", js_entries)?;
        output.set(
            u32::try_from(index).map_err(|_| invariant("static export overflow"))?,
            js_record,
        )?;
    }
    Ok(output)
}

pub(super) fn materialize_module(env: &Env, mut table: ModuleTable) -> napi::Result<Unknown<'_>> {
    let has_module_syntax = table.has_module_syntax();
    let (imports, import_entries) = table.take_static_imports();
    let (exports, export_entries) = table.take_static_exports();
    let dynamics = table.take_dynamic_imports();
    let metas = table.take_import_metas();
    let storage = table.take_text_storage();
    if !table.is_storage_released() {
        return Err(invariant("module table storage was not fully released"));
    }
    drop(table);

    let static_imports = materialize_static_imports(env, &storage, imports, &import_entries)?;
    let static_exports = materialize_static_exports(env, &storage, exports, &export_entries)?;

    let mut dynamic_imports = env.create_array(array_length(dynamics.len())?)?;
    for (index, record) in dynamics.into_iter().enumerate() {
        let mut js_record = Object::new(env)?;
        js_record.set_c_named_property(c"start", record.span.start)?;
        js_record.set_c_named_property(c"end", record.span.end)?;
        js_record
            .set_c_named_property(c"moduleRequest", span_object(env, record.module_request)?)?;
        dynamic_imports.set(
            u32::try_from(index).map_err(|_| invariant("dynamic import overflow"))?,
            js_record,
        )?;
    }

    let mut import_metas = env.create_array(array_length(metas.len())?)?;
    for (index, span) in metas.into_iter().enumerate() {
        import_metas.set(
            u32::try_from(index).map_err(|_| invariant("import.meta overflow"))?,
            span_object(env, span)?,
        )?;
    }

    let mut output = Object::new(env)?;
    output.set_c_named_property(c"hasModuleSyntax", has_module_syntax)?;
    output.set_c_named_property(c"staticImports", static_imports)?;
    output.set_c_named_property(c"staticExports", static_exports)?;
    output.set_c_named_property(c"dynamicImports", dynamic_imports)?;
    output.set_c_named_property(c"importMetas", import_metas)?;
    output.into_unknown(env)
}

pub(super) fn materialize_comments(
    env: &Env,
    mut table: CommentTable,
) -> napi::Result<Unknown<'_>> {
    let records = table.take_records();
    let storage = table.take_text_storage();
    if !table.is_storage_released() {
        return Err(invariant("comment table storage was not fully released"));
    }
    drop(table);
    let mut output = env.create_array(array_length(records.len())?)?;
    for (index, record) in records.into_iter().enumerate() {
        let mut comment = Object::new(env)?;
        comment.set_c_named_property(
            c"type",
            match record.kind {
                ProjectedCommentKind::Line => "Line",
                ProjectedCommentKind::Block => "Block",
            },
        )?;
        comment.set_c_named_property(c"value", owned_text_unknown(env, &storage, record.value)?)?;
        comment.set_c_named_property(c"start", record.span.start)?;
        comment.set_c_named_property(c"end", record.span.end)?;
        output
            .set(u32::try_from(index).map_err(|_| invariant("comment index overflow"))?, comment)?;
    }
    output.into_unknown(env)
}

fn optional_diagnostic_text<'env>(
    env: &'env Env,
    storage: &OwnedPackedTextStorage,
    value: OptionalStringRange,
) -> napi::Result<Unknown<'env>> {
    value.get().map_or_else(|| null(env), |range| owned_text_unknown(env, storage, range))
}

pub(super) fn materialize_diagnostics(
    env: &Env,
    mut table: DiagnosticTable,
) -> napi::Result<Unknown<'_>> {
    let (records, labels) = table.take_records_and_labels();
    let storage = table.take_text_storage();
    if !table.is_storage_released() {
        return Err(invariant("diagnostic table storage was not fully released"));
    }
    drop(table);
    let mut output = env.create_array(array_length(records.len())?)?;
    for (index, record) in records.into_iter().enumerate() {
        let record_labels = list_slice(&labels, record.labels)?;
        let mut js_labels = env.create_array(array_length(record_labels.len())?)?;
        for (label_index, label) in record_labels.iter().copied().enumerate() {
            let mut js_label = Object::new(env)?;
            js_label.set_c_named_property(
                c"message",
                optional_diagnostic_text(env, &storage, label.message)?,
            )?;
            js_label.set_c_named_property(c"start", label.span.start)?;
            js_label.set_c_named_property(c"end", label.span.end)?;
            js_labels.set(
                u32::try_from(label_index).map_err(|_| invariant("label index overflow"))?,
                js_label,
            )?;
        }
        let mut diagnostic = Object::new(env)?;
        diagnostic.set_c_named_property(
            c"severity",
            match record.severity {
                DiagnosticSeverity::Error => "Error",
                DiagnosticSeverity::Warning => "Warning",
                DiagnosticSeverity::Advice => "Advice",
            },
        )?;
        diagnostic
            .set_c_named_property(c"message", owned_text_unknown(env, &storage, record.message)?)?;
        diagnostic.set_c_named_property(c"labels", js_labels)?;
        diagnostic.set_c_named_property(
            c"helpMessage",
            optional_diagnostic_text(env, &storage, record.help)?,
        )?;
        diagnostic.set_c_named_property(
            c"codeframe",
            optional_diagnostic_text(env, &storage, record.codeframe)?,
        )?;
        output.set(
            u32::try_from(index).map_err(|_| invariant("diagnostic index overflow"))?,
            diagnostic,
        )?;
    }
    output.into_unknown(env)
}
