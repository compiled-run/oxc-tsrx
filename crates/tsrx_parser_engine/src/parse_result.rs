use oxc_adapter::parser::RejectionModuleNames;
use tsrx_tape_schema::{
    CommentTable, Completeness, CoordinateDomain, DiagnosticTable, FlatTape, ModuleTable,
    ParseCompleteness,
};

use crate::TsrxParseError;

#[derive(Debug)]
pub struct TsrxParseResult {
    pub status: ParseCompleteness,
    pub coordinate_domain: CoordinateDomain,
    pub completeness: Completeness,
    pub program: Option<FlatTape>,
    pub module: Option<ModuleTable>,
    pub comments: CommentTable,
    pub errors: DiagnosticTable,
    pub suppressed_diagnostics: u32,
    pub(super) needs_compaction: bool,
    pub(super) rejection_module_names: RejectionModuleNames,
}

impl TsrxParseResult {
    /// Returns the complete Program for production callers that already require parse success.
    ///
    /// # Panics
    ///
    /// Panics when called on a failed or future recovered result without a Program.
    #[must_use]
    pub fn program(&self) -> &FlatTape {
        self.program.as_ref().expect("complete TSRX result must contain a Program")
    }

    pub(super) fn complete(
        program: FlatTape,
        module: Option<ModuleTable>,
        comments: CommentTable,
        errors: DiagnosticTable,
        suppressed_diagnostics: u32,
        needs_compaction: bool,
        rejection_module_names: RejectionModuleNames,
    ) -> Self {
        let mut completeness = Completeness::COMPLETE.with(Completeness::HAS_PROGRAM);
        if module.is_some() {
            completeness = completeness.with(Completeness::HAS_MODULE);
        }
        if !comments.is_empty() {
            completeness = completeness.with(Completeness::HAS_COMMENTS);
        }
        if !errors.is_empty() {
            completeness = completeness.with(Completeness::HAS_ERRORS);
        }
        Self {
            status: ParseCompleteness::Complete,
            coordinate_domain: CoordinateDomain::AuthoredUtf8Bytes,
            completeness,
            program: Some(program),
            module,
            comments,
            errors,
            suppressed_diagnostics,
            needs_compaction,
            rejection_module_names,
        }
    }

    pub(super) fn failed(
        comments: CommentTable,
        errors: DiagnosticTable,
        suppressed_diagnostics: u32,
    ) -> Result<Self, TsrxParseError> {
        if errors.is_empty() {
            return Err(TsrxParseError::Unsupported(
                "failed TSRX result has no authored diagnostic",
            ));
        }
        let mut completeness = Completeness::HAS_ERRORS;
        if !comments.is_empty() {
            completeness = completeness.with(Completeness::HAS_COMMENTS);
        }
        Ok(Self {
            status: ParseCompleteness::Failed,
            coordinate_domain: CoordinateDomain::AuthoredUtf8Bytes,
            completeness,
            program: None,
            module: None,
            comments,
            errors,
            suppressed_diagnostics,
            needs_compaction: false,
            rejection_module_names: RejectionModuleNames::default(),
        })
    }

    pub(super) fn failed_with_rejection_module_names(
        comments: CommentTable,
        errors: DiagnosticTable,
        suppressed_diagnostics: u32,
        rejection_module_names: RejectionModuleNames,
    ) -> Result<Self, TsrxParseError> {
        let mut result = Self::failed(comments, errors, suppressed_diagnostics)?;
        result.rejection_module_names = rejection_module_names;
        Ok(result)
    }
}
