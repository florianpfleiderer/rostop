//! End-to-end render smoke test: tick the demo backend, then render the full
//! view to a `TestBackend` buffer and assert that key topic names appear.

use std::time::{Duration, Instant};

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use rostop_cli::backend::BackendEvent;
use rostop_cli::domain::DomainProbeResult;
use rostop_cli::test_support::{render_once, AppHandle};
use rostop_core::message::DynamicValue;

fn selected_topic(handle: &AppHandle) -> String {
    handle
        .app
        .registry
        .sorted_by(
            handle.app.sort_key,
            handle.app.sort_order,
            handle.app.elapsed_ns(),
        )
        .get(handle.app.selected)
        .expect("selected demo topic")
        .name
        .clone()
}

fn render_dump(handle: &mut AppHandle, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    render_once(&mut terminal, handle);
    let buf = terminal.backend().buffer();
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
fn focus_mode_swaps_layout_to_a_single_topic_panel() {
    let mut handle = AppHandle::demo();
    handle.tick(Duration::from_millis(250));

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

    // Focus-mode layout markers — title bar carries "focus ─", status bar
    // shows the [FOCUS] mode, help text mentions Esc, and the ordinary
    // split-pane chrome ("inspector ─", "rates ─") is gone.
    assert!(
        dump.contains("focus ─"),
        "expected focus title in buffer:\n{dump}"
    );
    assert!(
        dump.contains("[FOCUS]"),
        "expected [FOCUS] mode label in status bar:\n{dump}"
    );
    assert!(
        dump.contains("f/Esc:back"),
        "expected f/Esc:back hint in status bar (f toggles focus mode, Esc still works):\n{dump}"
    );
    assert!(
        !dump.contains("inspector ─"),
        "split-pane inspector chrome should not render in focus mode:\n{dump}"
    );
    assert!(
        !dump.contains("rates ─"),
        "split-pane sparkline chrome should not render in focus mode:\n{dump}"
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
fn waveform_scope_renders_live_numeric_signal_and_controls() {
    let mut handle = AppHandle::demo();
    handle.tick(Duration::from_millis(100));
    let elapsed = handle.app.elapsed_ns();
    let rows = handle.app.registry.sorted_by(
        rostop_core::registry::SortKey::Name,
        rostop_core::registry::SortOrder::Ascending,
        elapsed,
    );
    handle.app.selected = rows
        .iter()
        .position(|entry| entry.name == "/cmd_vel")
        .expect("demo exposes /cmd_vel");
    handle.app.fullscreen = true;
    handle.app.enter_scope("/cmd_vel");
    handle.tick(Duration::from_millis(250));

    let backend = TestBackend::new(120, 32);
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

    for needle in [
        "waveform",
        "/cmd_vel",
        "linear.x",
        "NOW",
        "MIN",
        "MAX",
        "MEAN",
        "[SCOPE]",
        "Tab:field",
    ] {
        assert!(
            dump.contains(needle),
            "expected {needle:?} in waveform scope:\n{dump}"
        );
    }
}

#[test]
fn domain_scan_modal_reports_visible_domains() {
    let mut handle = AppHandle::demo();
    handle.tick(Duration::from_millis(50));
    handle.app.domain_scan_view.active = true;
    handle.app.domain_scan_view.total = 11;
    handle.app.domain_scan_view.started = 11;
    handle.app.domain_scan_view.completed = 11;
    handle.app.domain_scan_view.finished = true;
    handle.app.domain_scan_view.visible.push(DomainProbeResult {
        protocol_version: DomainProbeResult::PROTOCOL_VERSION,
        domain_id: 7,
        visible_topics: 42,
        visible_nodes: 8,
        discovery_ms: 812,
    });

    let backend = TestBackend::new(120, 32);
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

    for needle in ["visible ROS domains", "complete", "42", "812 ms", "D/Esc"] {
        assert!(
            dump.contains(needle),
            "expected {needle:?} in domain modal:\n{dump}"
        );
    }
}

#[test]
fn node_graph_renders_live_animated_pub_to_sub_flow() {
    let mut handle = AppHandle::demo();
    handle.tick(Duration::from_millis(250));
    let topic = selected_topic(&handle);
    handle.app.enter_node_graph(&topic);

    let dump = render_dump(&mut handle, 140, 30);
    for needle in [
        "NODE GRAPH",
        "PUBLISHERS",
        "TOPIC",
        "SUBSCRIBERS",
        "TRAFFIC",
        "◆",
        "/demo_pub",
        "/demo_sub",
        "[GRAPH]",
        "g/Esc:back",
    ] {
        assert!(
            dump.contains(needle),
            "expected {needle:?} in active node graph:\n{dump}"
        );
    }
}

#[test]
fn node_graph_deduplicates_bounds_and_labels_unavailable_idle_side() {
    let mut handle = AppHandle::demo();
    handle.tick(Duration::from_millis(100));
    let topic = selected_topic(&handle);
    let template = handle
        .app
        .endpoints
        .get(&topic)
        .and_then(|(publishers, _)| publishers.as_ref())
        .and_then(|publishers| publishers.first())
        .expect("demo publisher endpoint")
        .clone();
    let mut publishers = Vec::new();
    for index in 0..14 {
        let mut endpoint = template.clone();
        endpoint.node_name = format!("publisher_{index:02}");
        endpoint.endpoint_gid = vec![index as u8];
        publishers.push(endpoint);
    }
    let mut duplicate = template;
    duplicate.node_name = "publisher_00".into();
    duplicate.endpoint_gid = vec![255];
    publishers.push(duplicate);
    handle
        .app
        .endpoints
        .insert(topic.clone(), (Some(publishers), None));
    handle.app.topic_activity.remove(&topic);
    handle.app.enter_node_graph(&topic);

    let dump = render_dump(&mut handle, 100, 16);
    for needle in ["IDLE", "×2", "+", "(not available)", "topology live"] {
        assert!(
            dump.contains(needle),
            "expected {needle:?} in bounded idle graph:\n{dump}"
        );
    }
    assert!(
        !dump.contains('◆'),
        "idle graph must not show moving activity marker:\n{dump}"
    );
}

#[test]
fn node_graph_handles_disappeared_topic() {
    let mut handle = AppHandle::demo();
    handle.app.enter_node_graph("/topic_that_disappeared");
    let dump = render_dump(&mut handle, 100, 18);
    assert!(dump.contains("topic unavailable"), "{dump}");
    assert!(dump.contains("disappeared from the live graph"), "{dump}");
    assert!(dump.contains("g or Esc to return"), "{dump}");
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
