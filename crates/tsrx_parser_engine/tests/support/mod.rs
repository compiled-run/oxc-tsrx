use tsrx_parser_engine::{TsrxParseRequest, parse_tsrx};
use tsrx_tape_schema::{FlatTape, RecordIndex, ValueKind, ValueRef};

#[allow(dead_code)]
pub fn assert_failed(source: &str) {
    let result = parse_tsrx(&TsrxParseRequest { source }).unwrap_or_else(|error| {
        panic!("authored grammar escaped as an operational error: {error}")
    });
    assert_eq!(result.status, tsrx_tape_schema::ParseCompleteness::Failed);
    assert!(result.program.is_none());
    assert!(result.module.is_none());
    assert!(!result.errors.is_empty());
    assert!(
        result
            .errors
            .records()
            .iter()
            .all(|error| error.phase == tsrx_tape_schema::DiagnosticPhase::Grammar)
    );
}

pub fn optional_field(tape: &FlatTape, object: RecordIndex, name: &str) -> Option<ValueRef> {
    tape.field_index(object, name).and_then(|index| tape.field_value(index))
}

pub fn field(tape: &FlatTape, object: RecordIndex, name: &str) -> ValueRef {
    optional_field(tape, object, name).unwrap_or_else(|| panic!("missing `{name}` field"))
}

pub fn object_field(tape: &FlatTape, object: RecordIndex, name: &str) -> RecordIndex {
    field(tape, object, name).as_object().unwrap_or_else(|| panic!("`{name}` is not an object"))
}

pub fn list_field(tape: &FlatTape, object: RecordIndex, name: &str) -> Vec<ValueRef> {
    let list =
        field(tape, object, name).as_list().unwrap_or_else(|| panic!("`{name}` is not a list"));
    tape.values(list).collect()
}

pub fn scalar_field<'a>(tape: &'a FlatTape, object: RecordIndex, name: &str) -> &'a str {
    tape.scalar(field(tape, object, name)).unwrap_or_else(|| panic!("`{name}` is not a scalar"))
}

pub fn require_type(tape: &FlatTape, object: RecordIndex, expected: &str) {
    assert_eq!(scalar_field(tape, object, "type"), format!(r#""{expected}""#));
}

pub fn span(tape: &FlatTape, object: RecordIndex) -> (u32, u32) {
    (
        tape.scalar_u32(field(tape, object, "start")).expect("numeric start"),
        tape.scalar_u32(field(tape, object, "end")).expect("numeric end"),
    )
}

pub fn one_object(values: &[ValueRef]) -> RecordIndex {
    assert_eq!(values.len(), 1);
    values[0].as_object().expect("one object")
}

pub fn program_body(tape: &FlatTape) -> Vec<ValueRef> {
    let program = tape.root().as_object().expect("Program root");
    require_type(tape, program, "Program");
    list_field(tape, program, "body")
}

pub fn assert_empty_path(tape: &FlatTape, object: RecordIndex) {
    let metadata = object_field(tape, object, "metadata");
    assert!(list_field(tape, metadata, "path").is_empty());
}

pub fn assert_no_scaffold(tape: &FlatTape) {
    assert!(!tape.scalar_storage().contains("_t0_"));
    assert!(!tape.scalar_storage().contains("N0S__"));
    assert!(!tape.scalar_storage().contains("N0E__"));
    assert_all_records_and_scalar_bytes_reachable(tape);
}

pub fn assert_all_records_and_scalar_bytes_reachable(tape: &FlatTape) {
    let mut objects = vec![false; tape.object_count()];
    let mut lists = vec![false; tape.list_count()];
    let mut scalar_bytes = vec![false; tape.scalar_storage().len()];
    let mut field_count = 0_usize;
    let mut list_value_count = 0_usize;
    let mut pending = vec![tape.root()];

    while let Some(value) = pending.pop() {
        match value.kind() {
            ValueKind::Missing => {}
            ValueKind::Scalar => {
                if value.as_inline_u32().is_some() {
                    continue;
                }
                let range = value.as_scalar().expect("scalar range");
                let start = usize::try_from(range.start).expect("scalar start fits usize");
                let length = usize::try_from(range.length).expect("scalar length fits usize");
                let end = start.checked_add(length).expect("scalar range does not overflow");
                for byte in
                    scalar_bytes.get_mut(start..end).expect("scalar range is inside packed storage")
                {
                    *byte = true;
                }
            }
            ValueKind::Object => {
                let object = value.as_object().expect("object index");
                let index = usize::try_from(object.get().expect("object index present"))
                    .expect("object index fits usize");
                if std::mem::replace(&mut objects[index], true) {
                    continue;
                }
                for record in tape.fields(object) {
                    field_count += 1;
                    pending.push(record.value);
                }
            }
            ValueKind::List => {
                let list = value.as_list().expect("list index");
                let index = usize::try_from(list.get().expect("list index present"))
                    .expect("list index fits usize");
                if std::mem::replace(&mut lists[index], true) {
                    continue;
                }
                for value in tape.values(list) {
                    list_value_count += 1;
                    pending.push(value);
                }
            }
        }
    }

    assert!(objects.iter().all(|reachable| *reachable));
    assert!(lists.iter().all(|reachable| *reachable));
    assert!(scalar_bytes.iter().all(|reachable| *reachable));
    assert_eq!(field_count, tape.field_count());
    assert_eq!(list_value_count, tape.list_value_count());
}
