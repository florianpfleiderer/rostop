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
fn fullscreen_mode_swaps_layout_to_a_single_topic_panel() {
    let mut handle = AppHandle::demo();
    handle.tick(Duration::from_millis(250));

    // Pick the topic that's deterministically at the top of an Hz-descending sort.
    handle.app.fullscreen = true;
    handle.app.selected = 0;

    let backend = TestBackend::new(160, 28);
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

    // Fullscreen layout markers — title bar carries "fullscreen ─", status
    // bar shows the [FULLSCREEN] mode, help text mentions Esc, and the
    // ordinary split-pane chrome ("inspector ─", "rates ─") is gone.
    assert!(
        dump.contains("fullscreen ─"),
        "expected fullscreen title in buffer:\n{dump}"
    );
    assert!(
        dump.contains("[FULLSCREEN]"),
        "expected [FULLSCREEN] mode label in status bar:\n{dump}"
    );
    assert!(
        dump.contains("Esc:back"),
        "expected Esc:back hint in status bar:\n{dump}"
    );
    assert!(
        !dump.contains("inspector ─"),
        "split-pane inspector chrome should not render in fullscreen:\n{dump}"
    );
    assert!(
        !dump.contains("rates ─"),
        "split-pane sparkline chrome should not render in fullscreen:\n{dump}"
    );
}

#[test]
fn fullscreen_panel_lists_publishers_and_subscribers() {
    let mut handle = AppHandle::demo();
    handle.tick(Duration::from_millis(250));
    handle.app.fullscreen = true;
    handle.app.selected = 0;

    // Use enough rows that the endpoints + message tree both fit.
    let backend = TestBackend::new(160, 40);
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

    assert!(
        dump.contains("PUBLISHERS"),
        "expected PUBLISHERS section in fullscreen panel:\n{dump}"
    );
    assert!(
        dump.contains("SUBSCRIBERS"),
        "expected SUBSCRIBERS section in fullscreen panel:\n{dump}"
    );
    assert!(
        dump.contains("/demo_pub"),
        "expected at least one demo publisher node name to render:\n{dump}"
    );
    assert!(
        dump.contains("Reliable/Volatile"),
        "expected QoS summary on endpoint row:\n{dump}"
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
