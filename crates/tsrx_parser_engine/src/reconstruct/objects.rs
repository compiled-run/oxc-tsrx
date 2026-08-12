//! Bucketing the projected tape's objects by `ESTree` type and sorting them by start offset, so a
//! pass locates its node by position instead of walking the tree again.

use tsrx_tape_schema::RecordIndex;

use crate::TsrxParseError;

fn is_module_declaration_type(kind: &str) -> bool {
    matches!(
        kind,
        r#""ImportDeclaration""#
            | r#""ExportNamedDeclaration""#
            | r#""ExportDefaultDeclaration""#
            | r#""ExportAllDeclaration""#
            | r#""TSExportAssignment""#
            | r#""TSNamespaceExportDeclaration""#
    )
}

pub(super) struct ProjectedObjectIndex {
    pub(super) if_objects: Vec<(u32, RecordIndex)>,
    pub(super) loop_objects: Vec<(u32, RecordIndex)>,
    pub(super) switch_objects: Vec<(u32, RecordIndex)>,
    pub(super) block_objects: Vec<(u32, RecordIndex)>,
    pub(super) jsx_containers: Vec<(u32, RecordIndex)>,
    pub(super) jsx_attributes: Vec<(u32, RecordIndex)>,
    pub(super) jsx_opening_elements: Vec<(u32, RecordIndex)>,
    pub(super) patterns: Vec<(u32, RecordIndex)>,
    pub(super) layout_containers: Vec<RecordIndex>,
    pub(super) call_objects: Vec<RecordIndex>,
    pub(super) module_objects: Vec<RecordIndex>,
}

impl ProjectedObjectIndex {
    pub(super) fn new() -> Self {
        Self {
            if_objects: Vec::new(),
            loop_objects: Vec::new(),
            switch_objects: Vec::new(),
            block_objects: Vec::new(),
            jsx_containers: Vec::new(),
            jsx_attributes: Vec::new(),
            jsx_opening_elements: Vec::new(),
            patterns: Vec::new(),
            layout_containers: Vec::new(),
            call_objects: Vec::new(),
            module_objects: Vec::new(),
        }
    }

    pub(super) fn record(
        &mut self,
        object: RecordIndex,
        kind: Option<&str>,
        start: Option<u32>,
    ) -> Result<(), TsrxParseError> {
        if matches!(kind, Some(r#""JSXElement""# | r#""JSXFragment""#)) {
            self.layout_containers.push(object);
        }
        let target = match kind {
            Some(r#""IfStatement""#) => Some(&mut self.if_objects),
            Some(r#""ForStatement""# | r#""ForInStatement""# | r#""ForOfStatement""#) => {
                Some(&mut self.loop_objects)
            }
            Some(r#""SwitchStatement""#) => Some(&mut self.switch_objects),
            Some(r#""BlockStatement""#) => Some(&mut self.block_objects),
            Some(r#""JSXExpressionContainer""#) => Some(&mut self.jsx_containers),
            Some(r#""JSXAttribute""#) => Some(&mut self.jsx_attributes),
            Some(r#""JSXOpeningElement""#) => Some(&mut self.jsx_opening_elements),
            Some(r#""ArrayPattern""# | r#""ObjectPattern""#) => Some(&mut self.patterns),
            Some(r#""CallExpression""#) => {
                self.call_objects.push(object);
                None
            }
            _ => None,
        };
        if kind.is_some_and(is_module_declaration_type) {
            self.module_objects.push(object);
        }
        if let Some(target) = target {
            let start =
                start.ok_or(TsrxParseError::Unsupported("required ESTree field is not scalar"))?;
            target.push((start, object));
        }
        Ok(())
    }

    pub(super) fn sort(&mut self) {
        for objects in [
            &mut self.if_objects,
            &mut self.loop_objects,
            &mut self.switch_objects,
            &mut self.block_objects,
            &mut self.jsx_containers,
            &mut self.jsx_attributes,
            &mut self.jsx_opening_elements,
            &mut self.patterns,
        ] {
            objects.sort_unstable_by_key(|(start, _)| *start);
        }
    }
}

pub(super) fn find_unique_start(
    objects: &[(u32, RecordIndex)],
    start: u32,
    shape: &'static str,
) -> Result<RecordIndex, TsrxParseError> {
    let first = objects.partition_point(|(candidate, _)| *candidate < start);
    let last = objects.partition_point(|(candidate, _)| *candidate <= start);
    if last - first != 1 {
        return Err(TsrxParseError::Unsupported(shape));
    }
    Ok(objects[first].1)
}

pub(super) fn find_optional_start(
    objects: &[(u32, RecordIndex)],
    start: u32,
    shape: &'static str,
) -> Result<Option<RecordIndex>, TsrxParseError> {
    let first = objects.partition_point(|(candidate, _)| *candidate < start);
    let last = objects.partition_point(|(candidate, _)| *candidate <= start);
    match last - first {
        0 => Ok(None),
        1 => Ok(Some(objects[first].1)),
        _ => Err(TsrxParseError::Unsupported(shape)),
    }
}
