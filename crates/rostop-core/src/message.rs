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

/// One row at a single drill level (direct children of the current path).
///
/// Unlike `TreeRow`, this representation is non-recursive: a container child
/// is shown as a single summary row with `has_children = true`, never expanded.
/// Drill-down navigation in the UI selects one of these rows to descend into.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelRow {
    pub name: String,
    pub value_text: String,
    pub has_children: bool,
}

/// Resolve `path` against `root`, returning the value at that depth.
///
/// Each path entry is the index of a field (for `Struct`) or element (for
/// `Array`) at the corresponding level. Returns `None` if any index is out of
/// bounds or the value at that level is a scalar.
pub fn resolve_path<'a>(root: &'a DynamicValue, path: &[usize]) -> Option<&'a DynamicValue> {
    let mut cur = root;
    for &i in path {
        cur = match cur {
            DynamicValue::Struct(fields) => fields.get(i).map(|(_, v)| v)?,
            DynamicValue::Array(items) => items.get(i)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// Direct children of the value at `path`, formatted for one level of display.
///
/// If `path` resolves to a scalar (or is invalid), returns an empty vector —
/// the caller should treat that as "nothing to drill into".
pub fn level_rows(root: &DynamicValue, path: &[usize]) -> Vec<LevelRow> {
    let Some(v) = resolve_path(root, path) else {
        return Vec::new();
    };
    match v {
        DynamicValue::Struct(fields) => fields
            .iter()
            .map(|(name, child)| LevelRow {
                name: name.clone(),
                value_text: summarize(child),
                has_children: is_container(child),
            })
            .collect(),
        DynamicValue::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, child)| LevelRow {
                name: format!("[{i}]"),
                value_text: summarize(child),
                has_children: is_container(child),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Breadcrumb segments labelling each step of `path` from `root`.
///
/// For a struct step the segment is the field name; for an array step it is
/// `[i]`. Stops early if the path runs into a scalar or out-of-bounds index.
pub fn path_segments(root: &DynamicValue, path: &[usize]) -> Vec<String> {
    let mut out = Vec::with_capacity(path.len());
    let mut cur = root;
    for &i in path {
        match cur {
            DynamicValue::Struct(fields) => match fields.get(i) {
                Some((name, child)) => {
                    out.push(name.clone());
                    cur = child;
                }
                None => break,
            },
            DynamicValue::Array(items) => match items.get(i) {
                Some(child) => {
                    out.push(format!("[{i}]"));
                    cur = child;
                }
                None => break,
            },
            _ => break,
        }
    }
    out
}

fn is_container(v: &DynamicValue) -> bool {
    matches!(v, DynamicValue::Struct(_) | DynamicValue::Array(_))
}

fn summarize(v: &DynamicValue) -> String {
    match v {
        DynamicValue::Struct(fields) => format!("{{{} fields}}", fields.len()),
        DynamicValue::Array(items) => format!("[{}]", items.len()),
        scalar => scalar_to_string(scalar),
    }
}

#[cfg(test)]
mod tests;
