use super::*;

#[test]
fn empty_struct_flattens_to_no_rows() {
    let v = DynamicValue::Struct(vec![]);
    let rows = flatten_rows(&v);
    assert!(rows.is_empty(), "got {rows:?}");
}

#[test]
fn flat_struct_emits_one_row_per_field() {
    let v = DynamicValue::Struct(vec![
        ("width".into(), DynamicValue::U64(1280)),
        ("height".into(), DynamicValue::U64(720)),
        ("encoding".into(), DynamicValue::Str("rgb8".into())),
    ]);
    let rows = flatten_rows(&v);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].name, "width");
    assert_eq!(rows[0].value_text, "1280");
    assert_eq!(rows[0].depth, 0);
    assert_eq!(rows[2].value_text, "\"rgb8\"");
}

#[test]
fn nested_struct_indents_children() {
    let v = DynamicValue::Struct(vec![
        (
            "header".into(),
            DynamicValue::Struct(vec![
                ("frame_id".into(), DynamicValue::Str("base_link".into())),
                ("seq".into(), DynamicValue::U64(42)),
            ]),
        ),
        ("temperature".into(), DynamicValue::F64(23.5)),
    ]);
    let rows = flatten_rows(&v);
    assert_eq!(rows[0].name, "header");
    assert!(rows[0].has_children);
    assert_eq!(rows[0].depth, 0);
    assert_eq!(rows[1].name, "frame_id");
    assert_eq!(rows[1].depth, 1);
    assert_eq!(rows[2].name, "seq");
    assert_eq!(rows[2].depth, 1);
    assert_eq!(rows[3].name, "temperature");
    assert_eq!(rows[3].depth, 0);
}

#[test]
fn array_field_emits_indexed_children() {
    let v = DynamicValue::Struct(vec![(
        "ranges".into(),
        DynamicValue::Array(vec![
            DynamicValue::F64(1.0),
            DynamicValue::F64(1.1),
            DynamicValue::F64(1.2),
        ]),
    )]);
    let rows = flatten_rows(&v);
    assert_eq!(rows[0].name, "ranges");
    assert_eq!(rows[0].value_text, "[3]");
    assert!(rows[0].has_children);
    assert_eq!(rows[1].name, "[0]");
    assert_eq!(rows[1].depth, 1);
    assert_eq!(rows[3].name, "[2]");
}

#[test]
fn bytes_field_renders_as_size_hint() {
    let v = DynamicValue::Struct(vec![("data".into(), DynamicValue::Bytes(2_764_800))]);
    let rows = flatten_rows(&v);
    assert_eq!(rows[0].value_text, "<2764800 bytes>");
    assert!(!rows[0].has_children);
}
