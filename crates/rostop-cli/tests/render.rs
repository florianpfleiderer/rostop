//! End-to-end render smoke test: tick the demo backend, then render the full
//! view to a `TestBackend` buffer and assert that key topic names appear.

use std::time::Duration;

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use rostop_cli::test_support::{render_once, AppHandle};

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
fn rendering_with_no_data_does_not_panic() {
    let mut app = AppHandle::demo();
    // Don't tick the backend — registry is empty.
    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    render_once(&mut terminal, &mut app);
    // Just reaching here without a panic is the assertion.
}
