use tsrx_tape_schema::{
    FieldRecord, FlatTape, ListRecord, ListValueRecord, ObjectRecord, RecordIndex, StringRange,
    TapeBuildError, ValueKind, ValueRef,
};

fn object(tape: &mut FlatTape) -> RecordIndex {
    tape.push_object_record(ObjectRecord::default()).expect("object record")
}

fn json_string(tape: &mut FlatTape, value: &str) -> ValueRef {
    tape.push_json_string_scalar(value).expect("string scalar")
}

fn one_field_tape(key: StringRange, value: ValueRef, tape: &mut FlatTape) {
    let field = tape
        .push_field_record(FieldRecord { key, value, next: RecordIndex::NONE })
        .expect("field record");
    let root = tape
        .push_object_record(ObjectRecord { first_field: field, field_count: 1 })
        .expect("root object");
    tape.set_root(ValueRef::object(root));
}

fn special_value_tape() -> FlatTape {
    let mut tape = FlatTape::default();
    let literal = object(&mut tape);
    let literal_type = json_string(&mut tape, "Literal");
    tape.append_field(literal, "type", literal_type).expect("literal type");
    let null = tape.push_scalar("null").expect("null scalar");
    let special = ValueRef::scalar(null.as_scalar().expect("scalar range"), true);
    tape.append_field(literal, "value", special).expect("literal value");
    let bigint = json_string(&mut tape, "9007199254740993");
    tape.append_field(literal, "bigint", bigint).expect("bigint metadata");

    let body_value = tape
        .push_list_value_record(ListValueRecord {
            value: ValueRef::object(literal),
            next: RecordIndex::NONE,
        })
        .expect("body value");
    let body = tape
        .push_list_record(ListRecord { first_value: body_value, length: 1 })
        .expect("body list");
    let program = object(&mut tape);
    let program_type = json_string(&mut tape, "Program");
    tape.append_field(program, "type", program_type).expect("program type");
    tape.append_field(program, "body", ValueRef::list(body)).expect("program body");
    tape.set_root(ValueRef::object(program));
    tape
}

#[test]
fn packed_keys_transfer_and_fail_closed() {
    let mut tape = FlatTape::default();
    let key = tape.push_key("position").expect("packed key");
    one_field_tape(key, ValueRef::inline_u32(u32::MAX), &mut tape);
    let expected = r#"{"version":1,"node":{"position":4294967295},"fixes":[]}"#;
    assert_eq!(tape.program_transfer().expect("packed transfer"), expected);
    tape.compact_reachable().expect("compact packed-key tape");
    assert_eq!(tape.program_transfer().expect("compacted transfer"), expected);

    let mut malformed = FlatTape::default();
    one_field_tape(StringRange::new(7, u32::MAX), ValueRef::inline_u32(1), &mut malformed);
    assert_eq!(malformed.program_transfer(), Err(TapeBuildError::InvalidRecordIndex),);
}

#[test]
fn owned_transfer_matches_borrowed_and_rejects_shared_containers() {
    let mut borrowed = FlatTape::default();
    let key = borrowed.push_key("position").expect("packed key");
    one_field_tape(key, ValueRef::inline_u32(42), &mut borrowed);
    let expected = borrowed.program_transfer().expect("borrowed transfer");

    let mut owned = FlatTape::default();
    let key = owned.push_key("position").expect("packed key");
    one_field_tape(key, ValueRef::inline_u32(42), &mut owned);
    assert_eq!(owned.program_transfer_owned().expect("owned transfer"), expected);

    let mut shared = FlatTape::default();
    let child = object(&mut shared);
    let first = shared
        .push_list_value_record(ListValueRecord {
            value: ValueRef::object(child),
            next: RecordIndex::NONE,
        })
        .expect("first entry");
    let second = shared
        .push_list_value_record(ListValueRecord {
            value: ValueRef::object(child),
            next: RecordIndex::NONE,
        })
        .expect("second entry");
    shared.set_list_value_next(first, second).expect("link entries");
    let root =
        shared.push_list_record(ListRecord { first_value: first, length: 2 }).expect("root list");
    shared.set_root(ValueRef::list(root));
    assert_eq!(shared.program_transfer_owned(), Err(TapeBuildError::InvalidRecordIndex),);
}

#[test]
fn inline_u32_transfer_covers_every_decimal_width_boundary() {
    let values = [
        0,
        9,
        10,
        99,
        100,
        999,
        1_000,
        9_999,
        10_000,
        99_999,
        100_000,
        999_999,
        1_000_000,
        9_999_999,
        10_000_000,
        99_999_999,
        100_000_000,
        999_999_999,
        1_000_000_000,
        u32::MAX,
    ];
    let mut tape = FlatTape::default();
    let mut first = RecordIndex::NONE;
    let mut previous = RecordIndex::NONE;
    for value in values {
        let entry = tape
            .push_list_value_record(ListValueRecord {
                value: ValueRef::inline_u32(value),
                next: RecordIndex::NONE,
            })
            .expect("u32 entry");
        if first.is_none() {
            first = entry;
        } else {
            tape.set_list_value_next(previous, entry).expect("u32 link");
        }
        previous = entry;
    }
    let root = tape
        .push_list_record(ListRecord {
            first_value: first,
            length: u32::try_from(values.len()).expect("u32 value count"),
        })
        .expect("u32 list");
    tape.set_root(ValueRef::list(root));

    assert_eq!(
        tape.program_transfer().expect("u32 transfer"),
        concat!(
            r#"{"version":1,"node":[0,9,10,99,100,999,1000,9999,10000,99999,"#,
            r#"100000,999999,1000000,9999999,10000000,99999999,100000000,"#,
            r#"999999999,1000000000,4294967295],"fixes":[]}"#,
        ),
    );
}

#[test]
fn compact_value_tags_preserve_scalar_indices_lengths_and_kinds() {
    let empty = ValueRef::scalar(StringRange::new(u32::MAX, 0), false);
    assert_eq!(empty.kind(), ValueKind::Scalar);
    assert_eq!(empty.as_scalar(), Some(StringRange::new(u32::MAX, 0)));
    assert_eq!(empty.as_inline_u32(), None);

    let longest_practical = ValueRef::scalar(StringRange::new(u32::MAX, u32::MAX - 5), false);
    assert_eq!(longest_practical.as_scalar(), Some(StringRange::new(u32::MAX, u32::MAX - 5)),);

    let fix = ValueRef::scalar(StringRange::new(u32::MAX, 4), true);
    assert_eq!(fix.kind(), ValueKind::Scalar);
    assert_eq!(fix.as_scalar(), Some(StringRange::new(u32::MAX, 4)));
    assert!(fix.needs_fix());

    let object = ValueRef::object(RecordIndex::new(u32::MAX - 1));
    let list = ValueRef::list(RecordIndex::new(u32::MAX - 2));
    assert_eq!(object.kind(), ValueKind::Object);
    assert_eq!(object.as_object(), Some(RecordIndex::new(u32::MAX - 1)));
    assert_eq!(list.kind(), ValueKind::List);
    assert_eq!(list.as_list(), Some(RecordIndex::new(u32::MAX - 2)));
    assert_eq!(ValueRef::MISSING.kind(), ValueKind::Missing);
}

#[test]
#[should_panic(expected = "assertion failed: !needs_fix || range.length == FIX_SCALAR_LENGTH")]
fn compact_value_tags_reject_non_null_fix_scalars() {
    let _ = ValueRef::scalar(StringRange::new(0, 3), true);
}

#[test]
#[should_panic(expected = "assertion failed: range.length < FIX_SCALAR_TAG")]
fn compact_value_tags_reject_reserved_scalar_lengths() {
    let _ = ValueRef::scalar(StringRange::new(0, u32::MAX - 4), false);
}

#[test]
fn transfer_is_versioned_and_records_oxc_special_value_paths() {
    let expected = concat!(
        r#"{"version":1,"node":{"type":"Program","body":["#,
        r#"{"type":"Literal","value":null,"bigint":"9007199254740993"}"#,
        r#"]},"fixes":[["body",0]]}"#,
    );
    assert_eq!(special_value_tape().program_transfer().expect("Program transfer"), expected,);
    assert_eq!(
        special_value_tape().program_transfer_engine_owned().expect("engine Program transfer"),
        expected,
    );

    let binary = special_value_tape()
        .program_transfer_engine_binary_owned()
        .expect("binary engine Program transfer");
    assert_eq!(
        binary.metadata,
        concat!(
            r#"[["bigint"],"#,
            r#"["Program","Literal",null,"9007199254740993"],"#,
            r#"[["body",0]]]"#,
        ),
    );
    assert_eq!(
        binary.words,
        vec![
            0x4252_5354,
            1,
            2,
            5,
            1,
            1,
            1,
            1,
            1,
            4,
            1,
            0,
            2,
            3,
            0,
            2,
            0x8000_0005,
            0,
            0x8000_0000,
            0x8000_0000,
            0x8000_0005,
            1,
            0x8000_0015,
            2,
            0,
            3,
            0,
            1,
            0x4000_0000,
        ],
    );
}

#[test]
fn binary_transfer_rejects_non_program_roots_and_shared_containers() {
    let mut scalar_root = FlatTape::default();
    let scalar = scalar_root.push_json_string_scalar("Program").expect("scalar root");
    scalar_root.set_root(scalar);
    assert!(matches!(
        scalar_root.program_transfer_engine_binary_owned(),
        Err(TapeBuildError::InvalidRecordIndex),
    ));

    let mut shared = FlatTape::default();
    let child = object(&mut shared);
    let root = object(&mut shared);
    shared.append_field(root, "first", ValueRef::object(child)).expect("first child");
    shared.append_field(root, "second", ValueRef::object(child)).expect("shared child");
    shared.set_root(ValueRef::object(root));
    assert!(matches!(
        shared.program_transfer_engine_binary_owned(),
        Err(TapeBuildError::InvalidRecordIndex),
    ));
}

#[test]
fn engine_transfer_retains_key_escaping_and_graph_rejection() {
    let mut escaped = FlatTape::default();
    let key = escaped.push_key("wide\"key\nvalue").expect("escaped key");
    one_field_tape(key, ValueRef::inline_u32(7), &mut escaped);
    assert_eq!(
        escaped.program_transfer_engine_owned().expect("escaped engine transfer"),
        r#"{"version":1,"node":{"wide\"key\nvalue":7},"fixes":[]}"#,
    );

    let mut cyclic = FlatTape::default();
    let cycle = object(&mut cyclic);
    cyclic.append_field(cycle, "self", ValueRef::object(cycle)).expect("cycle field");
    cyclic.set_root(ValueRef::object(cycle));
    assert_eq!(cyclic.program_transfer_engine_owned(), Err(TapeBuildError::InvalidRecordIndex),);
}

#[test]
fn transfer_preserves_lone_utf16_units_as_json_escapes() {
    let mut tape = FlatTape::default();
    let root = object(&mut tape);
    let value = tape
        .push_json_utf16_scalar(&[u16::from(b'a'), 0xd800, u16::from(b'b'), 0xdc00])
        .expect("UTF-16 scalar");
    tape.append_field(root, "text", value).expect("text field");
    tape.set_root(ValueRef::object(root));

    assert_eq!(
        tape.program_transfer().expect("Program transfer"),
        r#"{"version":1,"node":{"text":"a\ud800b\udc00"},"fixes":[]}"#,
    );
}

#[test]
fn transfer_walk_is_iterative_for_deep_programs() {
    let mut tape = FlatTape::default();
    let mut value = tape.push_scalar("null").expect("leaf");
    for _ in 0..12_000 {
        let parent = object(&mut tape);
        tape.append_field(parent, "child", value).expect("nested child");
        value = ValueRef::object(parent);
    }
    tape.set_root(value);

    let payload = tape.program_transfer().expect("deep Program transfer");
    assert!(payload.starts_with(r#"{"version":1,"node":{"child":{"child":"#));
    assert!(payload.ends_with(",\"fixes\":[]}"));
    assert_eq!(payload.matches(r#"{"child":"#).count(), 12_000);
}

#[test]
fn transfer_rejects_cycles_and_shared_containers() {
    let mut cyclic = FlatTape::default();
    let cycle = object(&mut cyclic);
    cyclic.append_field(cycle, "self", ValueRef::object(cycle)).expect("cycle field");
    cyclic.set_root(ValueRef::object(cycle));
    assert_eq!(cyclic.program_transfer(), Err(TapeBuildError::InvalidRecordIndex),);

    let mut shared = FlatTape::default();
    let child = object(&mut shared);
    let first = shared
        .push_list_value_record(ListValueRecord {
            value: ValueRef::object(child),
            next: RecordIndex::NONE,
        })
        .expect("first child");
    let second = shared
        .push_list_value_record(ListValueRecord {
            value: ValueRef::object(child),
            next: RecordIndex::NONE,
        })
        .expect("second child");
    shared.set_list_value_next(first, second).expect("link shared children");
    let list =
        shared.push_list_record(ListRecord { first_value: first, length: 2 }).expect("shared list");
    shared.set_root(ValueRef::list(list));
    assert_eq!(shared.program_transfer(), Err(TapeBuildError::InvalidRecordIndex),);
}

#[test]
fn batch_list_removal_preserves_identity_order_and_transfer() {
    let mut tape = FlatTape::default();
    let one = tape.push_scalar("1").expect("one");
    let two = tape.push_scalar("2").expect("two");
    let three = tape.push_scalar("3").expect("three");
    let first = tape
        .push_list_value_record(ListValueRecord { value: one, next: RecordIndex::NONE })
        .expect("first entry");
    let second = tape
        .push_list_value_record(ListValueRecord { value: two, next: RecordIndex::NONE })
        .expect("second entry");
    let third = tape
        .push_list_value_record(ListValueRecord { value: three, next: RecordIndex::NONE })
        .expect("third entry");
    tape.set_list_value_next(first, second).expect("first link");
    tape.set_list_value_next(second, third).expect("second link");
    let list = tape.push_list_record(ListRecord { first_value: first, length: 3 }).expect("list");
    tape.set_root(ValueRef::list(list));

    tape.remove_list_values(&[(list, second)]).expect("batch removal");

    assert_eq!(tape.list_length(list), Some(2));
    assert_eq!(tape.values(list).collect::<Vec<_>>(), vec![one, three]);
    assert_eq!(
        tape.program_transfer().expect("Program transfer"),
        r#"{"version":1,"node":[1,3],"fixes":[]}"#,
    );
}

#[test]
fn batch_list_removal_fails_before_mutation_for_duplicates_and_shared_entries() {
    let mut duplicate = FlatTape::default();
    let value = duplicate.push_scalar("1").expect("value");
    let entry = duplicate
        .push_list_value_record(ListValueRecord { value, next: RecordIndex::NONE })
        .expect("entry");
    let list =
        duplicate.push_list_record(ListRecord { first_value: entry, length: 1 }).expect("list");
    duplicate.set_root(ValueRef::list(list));
    let before = duplicate.program_transfer().expect("before transfer");
    assert_eq!(
        duplicate.remove_list_values(&[(list, entry), (list, entry)]),
        Err(TapeBuildError::InvalidRecordIndex),
    );
    assert_eq!(duplicate.program_transfer().expect("after transfer"), before);

    let mut shared = FlatTape::default();
    let value = shared.push_scalar("1").expect("value");
    let entry = shared
        .push_list_value_record(ListValueRecord { value, next: RecordIndex::NONE })
        .expect("entry");
    let left =
        shared.push_list_record(ListRecord { first_value: entry, length: 1 }).expect("left list");
    let _right =
        shared.push_list_record(ListRecord { first_value: entry, length: 1 }).expect("right list");
    assert_eq!(
        shared.remove_list_values(&[(left, entry)]),
        Err(TapeBuildError::InvalidRecordIndex),
    );
}
