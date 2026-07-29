//! Library surface so integration tests can drive the app without a real terminal.
//!
//! The binary itself doesn't use this — it has its own `main.rs` that wires
//! the CLI flags and the demo backend. The library re-exports a thin
//! `test_support` shim used by `tests/render.rs`.

pub mod app;
pub mod backend;
pub mod domain;
pub mod ui;

pub mod test_support {
    use std::io;
    use std::time::Duration;

    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;
    use crate::backend::demo::DemoBackend;
    use crate::ui;

    /// Thin wrapper that owns an `App` running against the demo backend so that
    /// integration tests can drive it without taking over the terminal.
    pub struct AppHandle {
        pub app: App,
    }

    impl AppHandle {
        pub fn demo() -> Self {
            Self {
                app: App::new(Box::new(DemoBackend::new())),
            }
        }

        /// Pull events from the backend for `duration` then ingest them.
        pub fn tick(&mut self, duration: Duration) {
            let events = self.app.backend.poll(duration);
            for_each_ingest(&mut self.app, events);
            self.app.tick_sparklines_for_tests();
        }
    }

    fn for_each_ingest(app: &mut App, events: Vec<crate::backend::BackendEvent>) {
        app.ingest_for_tests(events);
    }

    /// Render the current app state once using the given terminal.
    pub fn render_once(terminal: &mut Terminal<TestBackend>, app_handle: &mut AppHandle) {
        let app = &mut app_handle.app;
        let elapsed_ns = app.elapsed_ns();
        let rows = ui::rows::build_rows(&app.registry, app.sort_key, app.sort_order, elapsed_ns);
        let selected_topic = rows.get(app.selected).map(|r| r.name.clone());
        app.sync_inspector_for_topic(selected_topic.as_deref());
        let _: io::Result<()> = terminal
            .draw(|f| ui::view::render(f, app, &rows))
            .map(|_| ());
    }
}
