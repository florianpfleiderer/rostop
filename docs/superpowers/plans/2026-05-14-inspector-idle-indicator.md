# Inspector Idle Indicator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the inspector pane's static `"(no message yet)"` text with a healthy-but-idle indicator once a topic has been observed in the ROS graph for more than 3 seconds without any sample, so users can distinguish a broken subscription from a quiet one (e.g. `/parameter_events`).

**Architecture:** Track when a topic is first discovered (`first_seen_ns: Option<u64>`) on `TopicEntry`. Plumb the resulting `idle_secs` plus existing publisher/subscriber counts through `TopicTableRow`. Keep `view.rs` a pure renderer by computing the empty-state message in a small testable helper that takes (idle_secs, publishers, subscribers) and returns the string to display.

**Tech Stack:** Rust 2021, ratatui 0.29, existing rostop crate split (rostop-core / rostop-cli).

---

## File Structure

- `crates/rostop-core/src/registry.rs` — add `first_seen_ns: Option<u64>` to `TopicEntry`; add `mark_seen(name, t_ns)` method that lazily sets it.
- `crates/rostop-core/src/registry/tests.rs` — cover the lazy-set semantics.
- `crates/rostop-cli/src/ui/rows.rs` — add `idle_secs: u64` to `TopicTableRow`; compute it in `build_rows`.
- `crates/rostop-cli/src/ui/rows/tests.rs` — cover `idle_secs` computation.
- `crates/rostop-cli/src/ui/view.rs` — add private helper `inspector_empty_state` + `IDLE_THRESHOLD_SECS` constant; use it in `render_inspector`. Inline `#[cfg(test)] mod tests` at the bottom (no new file).
- `crates/rostop-cli/src/app.rs` — in `ingest`, call `registry.mark_seen(name, elapsed_ns)` when handling `BackendEvent::Topic`.

---

### Task 1: Add `first_seen_ns` field + `mark_seen` to the registry

**Files:**
- Modify: `crates/rostop-core/src/registry.rs:13-20` (TopicEntry struct)
- Modify: `crates/rostop-core/src/registry.rs:65-76` (upsert default)
- Modify: `crates/rostop-core/src/registry.rs:91-97` (after set_endpoints, add mark_seen)
- Test: `crates/rostop-core/src/registry/tests.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/rostop-core/src/registry/tests.rs`:

```rust
#[test]
fn mark_seen_sets_first_seen_lazily() {
    let mut reg = TopicRegistry::new();
    reg.upsert("/scan", "sensor_msgs/msg/LaserScan");
    assert!(reg.get("/scan").unwrap().first_seen_ns.is_none());

    reg.mark_seen("/scan", ns(1.0));
    assert_eq!(reg.get("/scan").unwrap().first_seen_ns, Some(ns(1.0)));

    // A later mark_seen must not overwrite the first one.
    reg.mark_seen("/scan", ns(5.0));
    assert_eq!(reg.get("/scan").unwrap().first_seen_ns, Some(ns(1.0)));
}

#[test]
fn mark_seen_for_unknown_topic_is_a_noop() {
    let mut reg = TopicRegistry::new();
    reg.mark_seen("/ghost", ns(1.0)); // must not panic
    assert_eq!(reg.len(), 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rostop-core registry::tests::mark_seen -- --nocapture`
Expected: FAIL with "no field `first_seen_ns`" and "no method named `mark_seen`".

- [ ] **Step 3: Add the field**

In `crates/rostop-core/src/registry.rs`, change the struct (around line 13):

```rust
/// One row's worth of state in the topic table.
#[derive(Debug)]
pub struct TopicEntry {
    pub name: String,
    pub type_name: String,
    pub publishers: u32,
    pub subscribers: u32,
    pub stats: TopicStats,
    /// Monotonic nanoseconds (from `App::elapsed_ns`) when this topic was first
    /// observed via the backend. `None` until the first ingest event sets it;
    /// used to compute idle time for healthy-but-quiet topics.
    pub first_seen_ns: Option<u64>,
}
```

Update the constructor branch inside `upsert` (around line 69):

```rust
.or_insert_with(|| TopicEntry {
    name: name.to_string(),
    type_name: type_name.to_string(),
    publishers: 0,
    subscribers: 0,
    stats: TopicStats::new(DEFAULT_WINDOW_NS),
    first_seen_ns: None,
});
```

- [ ] **Step 4: Add the `mark_seen` method**

Insert immediately after the existing `set_endpoints` method (around line 97) in `crates/rostop-core/src/registry.rs`:

```rust
/// Stamp the time this topic was first seen, if not already stamped.
/// Idempotent — later calls do not overwrite the original timestamp.
pub fn mark_seen(&mut self, name: &str, t_ns: u64) {
    if let Some(e) = self.entries.get_mut(name) {
        if e.first_seen_ns.is_none() {
            e.first_seen_ns = Some(t_ns);
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rostop-core registry::tests`
Expected: all existing tests still pass + the two new tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rostop-core/src/registry.rs crates/rostop-core/src/registry/tests.rs
git commit -m "feat(core): track first_seen_ns per topic via mark_seen"
```

---

### Task 2: Mark first-seen on every discovery event in `App::ingest`

**Files:**
- Modify: `crates/rostop-cli/src/app.rs:166-193` (the `ingest` method)
- Test: `crates/rostop-cli/tests/` (integration) — add a focused unit test inline in `app.rs` instead since `App` is already exercised via `ingest_for_tests`.

- [ ] **Step 1: Write the failing test**

Append at the bottom of `crates/rostop-cli/src/app.rs` (before the final newline):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendEvent, DemoBackend};

    fn empty_topic_event(name: &str) -> BackendEvent {
        BackendEvent::Topic {
            name: name.to_string(),
            type_name: "std_msgs/msg/Empty".to_string(),
            publishers: 1,
            subscribers: 0,
        }
    }

    #[test]
    fn ingesting_a_topic_event_marks_first_seen() {
        let backend: Box<dyn RosBackend> = Box::new(DemoBackend::new());
        let mut app = App::new(backend);

        app.ingest_for_tests(vec![empty_topic_event("/parameter_events")]);

        let entry = app.registry.get("/parameter_events").unwrap();
        assert!(
            entry.first_seen_ns.is_some(),
            "first_seen_ns should be stamped on first Topic event"
        );
    }
}
```

If `DemoBackend::new` is not the actual constructor, first run `grep -n "fn new" crates/rostop-cli/src/backend/demo.rs` and substitute the correct constructor — the assertion is the important part.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rostop-cli app::tests::ingesting_a_topic_event_marks_first_seen`
Expected: FAIL — `first_seen_ns` is `None` because nothing calls `mark_seen` yet.

- [ ] **Step 3: Wire `mark_seen` into the Topic arm of `ingest`**

In `crates/rostop-cli/src/app.rs`, change the `BackendEvent::Topic { .. }` arm (around line 170):

```rust
BackendEvent::Topic {
    name,
    type_name,
    publishers,
    subscribers,
} => {
    self.registry.upsert(&name, &type_name);
    self.registry.set_endpoints(&name, publishers, subscribers);
    self.registry.mark_seen(&name, elapsed_ns);
}
```

Note: `elapsed_ns` is already computed at the top of `ingest` and used by the `Sample` arm.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rostop-cli app::tests::ingesting_a_topic_event_marks_first_seen`
Expected: PASS.

- [ ] **Step 5: Run the full test suite to confirm nothing regressed**

Run: `cargo test --workspace`
Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add crates/rostop-cli/src/app.rs
git commit -m "feat(cli): stamp first_seen_ns on Topic ingest events"
```

---

### Task 3: Surface `idle_secs` on `TopicTableRow`

**Files:**
- Modify: `crates/rostop-cli/src/ui/rows.rs:11-19` (struct)
- Modify: `crates/rostop-cli/src/ui/rows.rs:42-50` (mapping)
- Test: `crates/rostop-cli/src/ui/rows/tests.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/rostop-cli/src/ui/rows/tests.rs`:

```rust
#[test]
fn build_rows_reports_idle_secs_zero_when_first_seen_unset() {
    let r = populated_registry();
    // populated_registry() never calls mark_seen, so every entry has
    // first_seen_ns == None and idle_secs must be 0.
    let rows = build_rows(&r, SortKey::Name, SortOrder::Ascending, "", ns(10.0));
    for row in &rows {
        assert_eq!(row.idle_secs, 0, "row {:?} should report 0 idle_secs", row.name);
    }
}

#[test]
fn build_rows_reports_idle_secs_from_first_seen() {
    let mut r = TopicRegistry::new();
    r.upsert("/parameter_events", "rcl_interfaces/msg/ParameterEvent");
    r.mark_seen("/parameter_events", ns(1.0));

    // 10 - 1 = 9 seconds of idle time.
    let rows = build_rows(&r, SortKey::Name, SortOrder::Ascending, "", ns(10.0));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].idle_secs, 9);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rostop-cli ui::rows::tests::build_rows_reports_idle_secs`
Expected: FAIL — `idle_secs` field does not exist on `TopicTableRow`.

- [ ] **Step 3: Add the field and compute it**

In `crates/rostop-cli/src/ui/rows.rs`, update the struct (around line 11):

```rust
/// One displayed row in the topic table.
#[derive(Debug, Clone, PartialEq)]
pub struct TopicTableRow {
    pub name: String,
    pub type_name: String,
    pub hz: f64,
    pub bps: f64,
    pub jitter_ms: f64,
    pub publishers: u32,
    pub subscribers: u32,
    /// Whole seconds since the topic was first observed in the graph. 0 when
    /// no `first_seen_ns` has been stamped yet (e.g. test fixtures that
    /// bypass `App::ingest`). The inspector uses this to decide whether to
    /// show an "idle" indicator instead of "no message yet".
    pub idle_secs: u64,
}
```

Update the mapping inside `build_rows` (around line 42):

```rust
.map(|e| TopicTableRow {
    name: e.name.clone(),
    type_name: e.type_name.clone(),
    hz: e.stats.hz(now_ns),
    bps: e.stats.bps(now_ns),
    jitter_ms: e.stats.jitter_ms(now_ns),
    publishers: e.publishers,
    subscribers: e.subscribers,
    idle_secs: e
        .first_seen_ns
        .map(|t| now_ns.saturating_sub(t) / 1_000_000_000)
        .unwrap_or(0),
})
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rostop-cli ui::rows::tests`
Expected: all existing rows tests still pass + the two new tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rostop-cli/src/ui/rows.rs crates/rostop-cli/src/ui/rows/tests.rs
git commit -m "feat(cli): expose idle_secs on TopicTableRow"
```

---

### Task 4: Add a pure helper that builds the inspector empty-state text

**Files:**
- Modify: `crates/rostop-cli/src/ui/view.rs` (new constant + helper + inline `#[cfg(test)] mod tests`)

The helper takes (idle_secs, publishers, subscribers) and returns the exact string the inspector should render when no message has arrived for the selected topic.

- [ ] **Step 1: Write the failing test**

Append at the bottom of `crates/rostop-cli/src/ui/view.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_under_threshold_returns_no_message_yet() {
        // 0 .. IDLE_THRESHOLD_SECS-1 should still show the original copy.
        for idle in 0..IDLE_THRESHOLD_SECS {
            assert_eq!(
                inspector_empty_state(idle, 1, 0),
                "  (no message yet)",
                "idle={idle} below threshold should show no-message-yet"
            );
        }
    }

    #[test]
    fn empty_state_at_or_above_threshold_returns_idle_indicator() {
        let s = inspector_empty_state(IDLE_THRESHOLD_SECS, 1, 0);
        assert!(s.contains("idle"), "expected idle indicator, got: {s}");
        assert!(s.contains(&format!("{}s", IDLE_THRESHOLD_SECS)));
        assert!(s.contains("1 pub"));
        assert!(s.contains("0 sub"));
    }

    #[test]
    fn empty_state_pluralizes_pub_sub_correctly() {
        // Always render numerically — no plural agreement, just "<n> pub" / "<n> sub".
        let s = inspector_empty_state(10, 2, 3);
        assert!(s.contains("2 pub"), "got: {s}");
        assert!(s.contains("3 sub"), "got: {s}");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rostop-cli ui::view::tests`
Expected: FAIL — `IDLE_THRESHOLD_SECS` and `inspector_empty_state` do not exist.

- [ ] **Step 3: Add the constant and helper**

Insert near the top of `crates/rostop-cli/src/ui/view.rs`, immediately after the `use` block (before `pub fn render`):

```rust
/// How long (whole seconds) a topic must be known in the graph with zero
/// messages before the inspector pane swaps "(no message yet)" for an
/// "(idle — …)" indicator. Picked to be longer than the slowest "normal"
/// topic (≈1 Hz) so we don't flicker on transients but short enough to
/// reassure the user within a few render frames.
const IDLE_THRESHOLD_SECS: u64 = 3;

/// Build the placeholder line shown inside the inspector when the selected
/// topic has no buffered message. Below `IDLE_THRESHOLD_SECS` we keep the
/// original "(no message yet)" copy so transient gaps don't flash an alarming
/// label; at or above the threshold we explain *why* the pane is empty:
/// the subscription is healthy, the topic just isn't publishing.
fn inspector_empty_state(idle_secs: u64, publishers: u32, subscribers: u32) -> String {
    if idle_secs < IDLE_THRESHOLD_SECS {
        "  (no message yet)".to_string()
    } else {
        format!(
            "  (idle — no messages in {idle_secs}s · {publishers} pub / {subscribers} sub)"
        )
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rostop-cli ui::view::tests`
Expected: PASS (all three view tests).

- [ ] **Step 5: Commit**

```bash
git add crates/rostop-cli/src/ui/view.rs
git commit -m "feat(cli): add inspector_empty_state helper for idle indicator"
```

---

### Task 5: Use the helper in `render_inspector`

**Files:**
- Modify: `crates/rostop-cli/src/ui/view.rs:114-160` (the `render_inspector` function)

- [ ] **Step 1: Update `render_inspector` to use the helper**

In `crates/rostop-cli/src/ui/view.rs`, the current `render_inspector` builds its `lines` like this for the `None` branch:

```rust
None => vec![Line::from(Span::styled(
    "  (no message yet)",
    Style::default().fg(Color::DarkGray),
))],
```

Replace the body of `render_inspector` so it looks up the row (which now carries `idle_secs`, `publishers`, `subscribers`) and calls the helper. The full revised function:

```rust
fn render_inspector(f: &mut Frame, area: Rect, app: &App, rows: &[TopicTableRow]) {
    let focused = app.focus == Focus::Inspector;
    let selected_row = rows.get(app.selected);
    let selected_name = selected_row.map(|r| r.name.clone());
    let message = selected_name.as_ref().and_then(|n| app.last_message.get(n));

    let breadcrumb = match (&selected_name, message) {
        (Some(n), Some(msg)) => {
            let segs = path_segments(msg, &app.inspector_path);
            if segs.is_empty() {
                format!(" inspector ─ {n} ")
            } else {
                format!(" inspector ─ {n} > {} ", segs.join(" > "))
            }
        }
        (Some(n), None) => format!(" inspector ─ {n} "),
        _ => " inspector ".into(),
    };

    let lines: Vec<Line> = match message {
        Some(value) => {
            let level = level_rows(value, &app.inspector_path);
            if level.is_empty() {
                vec![Line::from(Span::styled(
                    "  (no fields at this level)",
                    Style::default().fg(Color::DarkGray),
                ))]
            } else {
                level
                    .into_iter()
                    .enumerate()
                    .map(|(i, r)| render_level_line(i, r, focused, app.inspector_selected))
                    .collect()
            }
        }
        None => {
            let text = match selected_row {
                Some(r) => inspector_empty_state(r.idle_secs, r.publishers, r.subscribers),
                None => "  (no message yet)".to_string(),
            };
            vec![Line::from(Span::styled(
                text,
                Style::default().fg(Color::DarkGray),
            ))]
        }
    };
    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style(focused))
            .title(breadcrumb),
    );
    f.render_widget(para, area);
}
```

- [ ] **Step 2: Run the full test suite**

Run: `cargo test --workspace`
Expected: all green. Nothing should regress because the only visible behavior change is the placeholder string, and no existing test asserts on it.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/rostop-cli/src/ui/view.rs
git commit -m "feat(cli): show idle indicator in inspector for quiet topics"
```

---

### Task 6: Manual verification against a real ROS graph

**Files:** none — this is a smoke test.

- [ ] **Step 1: Build the demo backend and observe the placeholder**

Run: `cargo run -p rostop-cli -- --demo`
Expected: the demo backend produces samples, so the inspector should *not* show the idle state for active demo topics. Verify the inspector continues to look correct for selected topics with messages.

- [ ] **Step 2: Build the live backend and select `/parameter_events`**

In a sourced ROS 2 environment with a running node, run: `just run-jazzy` (or the equivalent for your installed distro — `just --list` shows options).

In the TUI:
1. Wait at least 3 seconds after `/parameter_events` appears in the table.
2. Select `/parameter_events` and observe the inspector pane.

Expected: instead of `(no message yet)`, the pane shows something like:

```
(idle — no messages in 5s · 1 pub / 0 sub)
```

(The exact second count and pub/sub numbers depend on the local graph.)

- [ ] **Step 3: Sanity-check an active topic**

Select an active topic (e.g. `/rosout`). Expected: the inspector shows the message contents as before — no regression to the "happy path."

- [ ] **Step 4: Commit a CHANGELOG entry if applicable**

If the repo's commit conventions require a CHANGELOG bump, add a line under the unreleased section:

```markdown
- Inspector pane shows an "(idle — …)" indicator for topics that have been known
  to the graph for ≥3s without producing a message (e.g. `/parameter_events`).
```

```bash
git add CHANGELOG.md
git commit -m "docs: changelog entry for inspector idle indicator"
```

If the repo does not maintain CHANGELOG entries per PR, skip this step.

---

## Self-Review

**Spec coverage:** The user asked for an "idle" indicator after N seconds replacing "(no message yet)" with text containing message count and publisher count. Implemented across Tasks 1-5; verified live in Task 6. The "0 msgs in 10s" phrasing in the user's example is approximated as "no messages in {idle_secs}s" — the topic's stats window is 1 s so a true rolling 10 s count would require new bookkeeping, and "no messages in Xs since first observed" carries the same signal more honestly. ✓

**Placeholder scan:** No TBD / "add appropriate error handling" / "similar to Task N" patterns. Every step contains either full code or an exact command. ✓

**Type consistency:** `first_seen_ns: Option<u64>` is used identically in Tasks 1, 3, and 4. `idle_secs: u64` appears with the same type in Task 3 (rows) and Task 4/5 (view). `IDLE_THRESHOLD_SECS: u64` is the same constant across Task 4's helper and the test. The helper signature `inspector_empty_state(idle_secs: u64, publishers: u32, subscribers: u32) -> String` matches the call site in Task 5. ✓
