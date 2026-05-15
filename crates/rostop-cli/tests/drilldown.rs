//! Integration test for the inspector drill-down feature.
//!
//! Drives the demo backend until `/tf` (a `TFMessage` with a `transforms`
//! array) is in the registry, then exercises the focus / drill-in / drill-out
//! state machine and renders each level to a `TestBackend` buffer.

use std::time::Duration;

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use rostop_cli::app::Focus;
use rostop_cli::test_support::{render_once, AppHandle};

fn buffer_dump(terminal: &Terminal<TestBackend>) -> String {
    let buf = terminal.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn drill_into_tf_transform_and_back_out() {
    let mut h = AppHandle::demo();
    // Tick long enough that every demo topic has published at least once.
    h.tick(Duration::from_millis(300));

    // Select /tf in the topic table — walk the sorted rows rather than
    // relying on a stable index, since the default sort is set by
    // App::new and the demo emits many topics.
    let elapsed_ns = h.app.elapsed_ns();
    let rows = rostop_cli::ui::rows::build_rows(
        &h.app.registry,
        h.app.sort_key,
        h.app.sort_order,
        elapsed_ns,
    );
    let tf_idx = rows
        .iter()
        .position(|r| r.name == "/tf")
        .expect("demo backend exposes /tf");
    h.app.selected = tf_idx;

    // Render once so sync_inspector_for_topic anchors the path on /tf.
    let backend = TestBackend::new(160, 28);
    let mut terminal = Terminal::new(backend).unwrap();
    render_once(&mut terminal, &mut h);
    assert_eq!(h.app.focus, Focus::Topics);
    assert!(h.app.inspector_path.is_empty());

    // Step into the inspector. The top-level shape of TFMessage is
    // a single field, `transforms`, so the cursor lands on it.
    h.app.focus = Focus::Inspector;
    h.app.inspector_selected = 0;
    render_once(&mut terminal, &mut h);
    let dump = buffer_dump(&terminal);
    assert!(
        dump.contains("transforms"),
        "expected 'transforms' at the inspector root, got:\n{dump}"
    );

    // Drill into `transforms` — we should now see the array entries.
    let drilled = h.app.drill_in(Some("/tf"));
    assert!(drilled, "drill_in should descend into the transforms array");
    assert_eq!(h.app.inspector_path, vec![0]);
    assert_eq!(h.app.inspector_selected, 0);
    render_once(&mut terminal, &mut h);
    let dump = buffer_dump(&terminal);
    assert!(
        dump.contains("[0]"),
        "expected indexed entry [0] inside transforms, got:\n{dump}"
    );
    assert!(
        dump.contains("/tf > transforms"),
        "breadcrumb should reflect the drill path, got:\n{dump}"
    );

    // One more level: into transforms[0]. The demo emits a struct with
    // child_frame_id / parent_frame_id / translation_x.
    let drilled = h.app.drill_in(Some("/tf"));
    assert!(drilled);
    assert_eq!(h.app.inspector_path, vec![0, 0]);
    render_once(&mut terminal, &mut h);
    let dump = buffer_dump(&terminal);
    assert!(
        dump.contains("child_frame_id"),
        "expected fields of the chosen transform, got:\n{dump}"
    );

    // h pops one level. Cursor should re-anchor on the row we descended from.
    h.app.drill_out();
    assert_eq!(h.app.inspector_path, vec![0]);
    assert_eq!(h.app.inspector_selected, 0);

    // Pop again — back to root of the inspector (still focused).
    h.app.drill_out();
    assert!(h.app.inspector_path.is_empty());
    assert_eq!(h.app.focus, Focus::Inspector);

    // One more h returns focus to the topic table.
    h.app.drill_out();
    assert_eq!(h.app.focus, Focus::Topics);
}

#[test]
fn drill_in_on_scalar_is_a_noop() {
    let mut h = AppHandle::demo();
    h.tick(Duration::from_millis(300));

    let elapsed_ns = h.app.elapsed_ns();
    let rows = rostop_cli::ui::rows::build_rows(
        &h.app.registry,
        h.app.sort_key,
        h.app.sort_order,
        elapsed_ns,
    );
    let cmd_vel_idx = rows
        .iter()
        .position(|r| r.name == "/cmd_vel")
        .expect("demo backend exposes /cmd_vel");
    h.app.selected = cmd_vel_idx;

    let backend = TestBackend::new(160, 28);
    let mut terminal = Terminal::new(backend).unwrap();
    render_once(&mut terminal, &mut h);

    // /cmd_vel root has two struct fields (linear, angular). Drill into the
    // first (linear), then to its `x` field which is a scalar — drilling
    // further should change nothing.
    h.app.focus = Focus::Inspector;
    assert!(h.app.drill_in(Some("/cmd_vel")));
    assert_eq!(h.app.inspector_path, vec![0]);
    // Move cursor to a scalar child and attempt to drill in.
    h.app.inspector_selected = 0; // `x`
    let before = h.app.inspector_path.clone();
    let changed = h.app.drill_in(Some("/cmd_vel"));
    assert!(!changed, "drill_in on scalar must report no change");
    assert_eq!(h.app.inspector_path, before);
}

#[test]
fn switching_topic_resets_drill_path() {
    let mut h = AppHandle::demo();
    h.tick(Duration::from_millis(300));

    let elapsed_ns = h.app.elapsed_ns();
    let rows = rostop_cli::ui::rows::build_rows(
        &h.app.registry,
        h.app.sort_key,
        h.app.sort_order,
        elapsed_ns,
    );
    let tf_idx = rows.iter().position(|r| r.name == "/tf").unwrap();
    let cmd_vel_idx = rows.iter().position(|r| r.name == "/cmd_vel").unwrap();

    h.app.selected = tf_idx;
    let backend = TestBackend::new(160, 28);
    let mut terminal = Terminal::new(backend).unwrap();
    render_once(&mut terminal, &mut h);
    h.app.focus = Focus::Inspector;
    h.app.drill_in(Some("/tf"));
    h.app.drill_in(Some("/tf"));
    assert_eq!(h.app.inspector_path.len(), 2);

    // Pick a different topic — drill state should reset.
    h.app.selected = cmd_vel_idx;
    render_once(&mut terminal, &mut h);
    assert!(
        h.app.inspector_path.is_empty(),
        "switching topic should reset inspector_path"
    );
}
