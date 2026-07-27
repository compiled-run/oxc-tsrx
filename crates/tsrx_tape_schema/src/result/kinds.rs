/// The public kind of a static import name.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportNameKind {
    Name = 1,
    NamespaceObject = 2,
    Default = 3,
}

/// The public kind of the imported side of a static export entry.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportImportNameKind {
    Name = 1,
    All = 2,
    AllButDefault = 3,
    None = 4,
}

/// The public kind of the exported side of a static export entry.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportExportNameKind {
    Name = 1,
    Default = 2,
    None = 3,
}

/// The public kind of the local side of a static export entry.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportLocalNameKind {
    Name = 1,
    Default = 2,
    None = 3,
}

/// Stable diagnostic severity independent of the pinned OXC revision.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Advice = 3,
}

/// The pass that emitted a diagnostic.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticPhase {
    Grammar = 1,
    Semantic = 2,
    Recovery = 3,
}
