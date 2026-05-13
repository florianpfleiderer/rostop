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

fn tf_like_message() -> DynamicValue {
    DynamicValue::Struct(vec![(
        "transforms".into(),
        DynamicValue::Array(vec![
            DynamicValue::Struct(vec![
                (
                    "child_frame_id".into(),
                    DynamicValue::Str("base_link".into()),
                ),
                ("parent_frame_id".into(), DynamicValue::Str("odom".into())),
                ("translation_x".into(), DynamicValue::F64(2.5)),
            ]),
            DynamicValue::Struct(vec![
                (
                    "child_frame_id".into(),
                    DynamicValue::Str("camera_link".into()),
                ),
                (
                    "parent_frame_id".into(),
                    DynamicValue::Str("base_link".into()),
                ),
                ("translation_x".into(), DynamicValue::F64(0.1)),
            ]),
        ]),
    )])
}

#[test]
fn level_rows_at_root_shows_top_level_fields_only() {
    let v = tf_like_message();
    let rows = level_rows(&v, &[]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "transforms");
    assert!(rows[0].has_children);
    assert_eq!(rows[0].value_text, "[2]");
}

#[test]
fn level_rows_into_array_lists_indexed_entries() {
    let v = tf_like_message();
    let rows = level_rows(&v, &[0]);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "[0]");
    assert_eq!(rows[1].name, "[1]");
    assert!(rows[0].has_children);
    assert_eq!(rows[0].value_text, "{3 fields}");
}

#[test]
fn level_rows_into_one_transform_lists_its_fields() {
    let v = tf_like_message();
    let rows = level_rows(&v, &[0, 1]);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].name, "child_frame_id");
    assert_eq!(rows[0].value_text, "\"camera_link\"");
    assert!(!rows[0].has_children);
    assert_eq!(rows[2].name, "translation_x");
    assert_eq!(rows[2].value_text, "0.1");
}

#[test]
fn level_rows_on_scalar_returns_empty() {
    let v = DynamicValue::Struct(vec![("x".into(), DynamicValue::F64(1.0))]);
    let rows = level_rows(&v, &[0]);
    assert!(rows.is_empty());
}

#[test]
fn level_rows_with_out_of_bounds_path_returns_empty() {
    let v = tf_like_message();
    let rows = level_rows(&v, &[0, 99]);
    assert!(rows.is_empty());
}

#[test]
fn path_segments_uses_field_names_and_indices() {
    let v = tf_like_message();
    assert!(path_segments(&v, &[]).is_empty());
    assert_eq!(path_segments(&v, &[0]), vec!["transforms".to_string()]);
    assert_eq!(
        path_segments(&v, &[0, 1]),
        vec!["transforms".to_string(), "[1]".to_string()]
    );
    assert_eq!(
        path_segments(&v, &[0, 1, 0]),
        vec![
            "transforms".to_string(),
            "[1]".to_string(),
            "child_frame_id".to_string()
        ]
    );
}

#[test]
fn path_segments_stops_at_scalar() {
    let v = tf_like_message();
    // [0,1,0] = a string scalar; further descent should produce nothing extra.
    assert_eq!(path_segments(&v, &[0, 1, 0, 0]).len(), 3);
}

#[test]
fn resolve_path_returns_inner_value() {
    let v = tf_like_message();
    let inner = resolve_path(&v, &[0, 0]).expect("path resolves");
    match inner {
        DynamicValue::Struct(fields) => assert_eq!(fields.len(), 3),
        other => panic!("expected struct, got {other:?}"),
    }
}
