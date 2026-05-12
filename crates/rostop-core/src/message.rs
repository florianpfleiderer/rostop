//! Dynamic-message representation and tree flattening for the inspector pane.
//!
//! `DynamicValue` mirrors the structural shape of a ROS 2 message (struct of
//! fields with primitives, nested structs, and arrays) without taking on a
//! direct dependency on rclrs. The `rostop-cli` adapter converts an `rclrs`
//! dynamic message into this representation; the TUI reads from it.

/// Structural value of a (possibly nested) ROS 2 message.
#[derive(Debug, Clone, PartialEq)]
pub enum DynamicValue {
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    Str(String),
    Bytes(usize), // length of bytes-like fields; we don't carry the payload
    Array(Vec<DynamicValue>),
    Struct(Vec<(String, DynamicValue)>),
}

/// One row in the flattened inspector view.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeRow {
    pub depth: u16,
    pub name: String,
    pub value_text: String,
    pub has_children: bool,
}

/// Flatten a `DynamicValue` into the row representation rendered in the
/// inspector pane. Containers (structs and arrays) emit a header row with
/// `has_children = true`, followed by their elements at depth + 1.
pub fn flatten_rows(v: &DynamicValue) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    push_value(&mut rows, 0, "", v);
    rows
}

fn push_value(rows: &mut Vec<TreeRow>, depth: u16, name: &str, v: &DynamicValue) {
    match v {
        DynamicValue::Struct(fields) => {
            if !name.is_empty() {
                rows.push(TreeRow {
                    depth,
                    name: name.to_string(),
                    value_text: String::new(),
                    has_children: true,
                });
            }
            let child_depth = if name.is_empty() { depth } else { depth + 1 };
            for (k, child) in fields {
                push_value(rows, child_depth, k, child);
            }
        }
        DynamicValue::Array(items) => {
            rows.push(TreeRow {
                depth,
                name: name.to_string(),
                value_text: format!("[{}]", items.len()),
                has_children: !items.is_empty(),
            });
            for (i, child) in items.iter().enumerate() {
                push_value(rows, depth + 1, &format!("[{i}]"), child);
            }
        }
        scalar => {
            rows.push(TreeRow {
                depth,
                name: name.to_string(),
                value_text: scalar_to_string(scalar),
                has_children: false,
            });
        }
    }
}

fn scalar_to_string(v: &DynamicValue) -> String {
    match v {
        DynamicValue::Bool(b) => b.to_string(),
        DynamicValue::I64(i) => i.to_string(),
        DynamicValue::U64(u) => u.to_string(),
        DynamicValue::F64(f) => format!("{f}"),
        DynamicValue::Str(s) => format!("{s:?}"),
        DynamicValue::Bytes(n) => format!("<{n} bytes>"),
        _ => unreachable!("scalar_to_string called on container"),
    }
}

#[cfg(test)]
mod tests;
