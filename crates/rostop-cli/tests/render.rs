//! End-to-end render smoke test: tick the demo backend, then render the full
//! view to a `TestBackend` buffer and assert that key topic names appear.

use std::time::{Duration, Instant};

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use rostop_cli::backend::BackendEvent;
use rostop_cli::test_support::{render_once, AppHandle};
use rostop_core::message::DynamicValue;

#[test]
fn demo_backend_topics_render_after_a_few_ticks() {
    let mut app = AppHandle::demo();
    // Let the demo backend run for ~250 ms so every topic publishes at least once.
    app.tick(Duration::from_millis(250));

    let backend = TestBackend::new(160, 28);
    let mut terminal = Terminal::new(backend).unwrap();
    render_once(&mut terminal, &mut app);

    let buf = terminal.backend().buffer().clone();
    let dump = (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    for needle in [
        "/scan",
        "/camera/image_raw",
        "/cmd_vel",
        "rostop",
        "j/k:move",
    ] {
        assert!(
            dump.contains(needle),
            "expected {needle:?} in rendered UI but did not find it:\n{dump}"
        );
    }
}

#[test]
fn selecting_a_row_below_the_table_area_keeps_it_on_screen() {
    // Regression for #14 — without a stateful table, ratatui clipped from
    // row 0 and the highlight disappeared once `selected` went past the
    // visible row count. Populate the registry with more topics than fit
    // and check the last-selected row name is still in the rendered buffer.
    let mut handle = AppHandle::demo();
    let topic_count = 40usize;
    let events: Vec<BackendEvent> = (0..topic_count)
        .flat_map(|i| {
            let name = format!("/t{i:02}");
            let now = Instant::now();
            [
                BackendEvent::Topic {
                    name: name.clone(),
                    type_name: "std_msgs/msg/Empty".into(),
                    publishers: 1,
                    subscribers: 0,
                },
                BackendEvent::Sample {
                    name,
                    bytes: 8,
                    value: DynamicValue::Bytes(8),
                    at: now,
                },
            ]
        })
        .collect();
    handle.app.ingest_for_tests(events);
    // Force a deterministic sort so the "last" row is predictable.
    handle.app.sort_key = rostop_core::registry::SortKey::Name;
    handle.app.sort_order = rostop_core::registry::SortOrder::Ascending;
    handle.app.selected = topic_count - 1;

    // Deliberately undersized: the table area is the top "Min(8)" chunk of
    // a 24-row terminal, so only a handful of rows fit at once.
    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    render_once(&mut terminal, &mut handle);

    let buf = terminal.backend().buffer().clone();
    let dump = (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    let needle = format!("/t{:02}", topic_count - 1);
    assert!(
        dump.contains(&needle),
        "selected row {needle:?} should auto-scroll into view, but the rendered table did not contain it:\n{dump}"
    );
    // The selection pointer must be on the visible /t39 line, not stranded
    // off-screen. Cheapest way to verify: the same line that contains the
    // needle should also contain the pointer "▸".
    let selected_line = dump
        .lines()
        .find(|l| l.contains(&needle))
        .expect("needle present");
    assert!(
        selected_line.contains('▸'),
        "selected row {needle:?} should carry the ▸ pointer, but its line was:\n{selected_line}"
    );
}

#[test]
fn rendering_with_no_data_does_not_panic() {
    let mut app = AppHandle::demo();
    // Don't tick the backend — registry is empty.
    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    render_once(&mut terminal, &mut app);
    // Just reaching here without a panic is the assertion.
}
