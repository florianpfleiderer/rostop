//! Application state and main event loop.

use std::collections::HashMap;
use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::TableState;
use ratatui::Terminal;
use rostop_core::message::{level_rows, DynamicValue};
use rostop_core::registry::{SortKey, SortOrder, TopicRegistry};
use rostop_core::sparkline::Sparkline;

use crate::backend::{BackendEvent, RosBackend};
use crate::ui;

/// Which pane currently receives j/k/h/l input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Topics,
    Inspector,
}

pub struct App {
    pub backend: Box<dyn RosBackend>,
    pub registry: TopicRegistry,
    pub start: Instant,
    pub selected: usize,
    pub sort_key: SortKey,
    pub sort_order: SortOrder,
    pub paused: bool,
    pub last_message: HashMap<String, DynamicValue>,
    pub hz_sparks: HashMap<String, Sparkline>,
    pub bw_sparks: HashMap<String, Sparkline>,
    pub last_spark_tick: Instant,
    /// Currently focused pane.
    pub focus: Focus,
    /// Indices descended into from the inspector root. Empty = root level.
    pub inspector_path: Vec<usize>,
    /// Cursor position within the current inspector level.
    pub inspector_selected: usize,
    /// Topic name whose message the current `inspector_path` belongs to.
    /// Used to reset drill state when the selected topic changes.
    pub inspector_topic: Option<String>,
    /// Sticky status-bar notice — set once on the first decode failure to hint
    /// at a possible distro/RMW mismatch. Cleared only by restarting rostop.
    pub notice: Option<String>,
    /// Ratatui table state for the topics pane. Persists `offset` between
    /// frames so `render_stateful_widget` can auto-scroll the viewport to
    /// keep the selected row visible when the registry grows past the
    /// table area.
    pub topic_table_state: TableState,
    /// When true, the UI replaces the split-pane layout with a single
    /// dedicated panel for the currently-selected topic — the "focus" mode.
    /// Set by `f` from the topics pane, cleared by `Esc`. Inspector drill
    /// state (`inspector_path` / `inspector_selected`) is shared between
    /// the inspector pane and focus mode, so leaving focus preserves the
    /// drill position. The field is named `fullscreen` because that's what
    /// the layout *does* (one panel filling the screen); the user-facing
    /// name is "focus" because that's what it *means* — zooming in on a
    /// single topic.
    pub fullscreen: bool,
}

impl App {
    /// Test-only hook: drive `ingest` from outside the crate.
    pub fn ingest_for_tests(&mut self, events: Vec<BackendEvent>) {
        self.ingest(events);
    }

    /// Test-only hook: force a sparkline tick.
    pub fn tick_sparklines_for_tests(&mut self) {
        // bypass the rate limiter so tests see something
        self.last_spark_tick = Instant::now() - Duration::from_secs(10);
        self.tick_sparklines();
    }

    pub fn new(backend: Box<dyn RosBackend>) -> Self {
        Self {
            backend,
            registry: TopicRegistry::new(),
            start: Instant::now(),
            selected: 0,
            sort_key: SortKey::Name,
            sort_order: SortOrder::Ascending,
            paused: false,
            last_message: HashMap::new(),
            hz_sparks: HashMap::new(),
            bw_sparks: HashMap::new(),
            last_spark_tick: Instant::now(),
            focus: Focus::Topics,
            inspector_path: Vec::new(),
            inspector_selected: 0,
            inspector_topic: None,
            notice: None,
            topic_table_state: TableState::default(),
            fullscreen: false,
        }
    }

    /// Resolve the message currently feeding the inspector, given the row
    /// name of the topic selected in the table.
    pub fn inspector_message<'a>(
        &'a self,
        selected_topic: Option<&str>,
    ) -> Option<&'a DynamicValue> {
        selected_topic.and_then(|n| self.last_message.get(n))
    }

    /// Reset the drill path when the user picks a different topic — old indices
    /// could point into a completely different message shape.
    pub fn sync_inspector_for_topic(&mut self, selected_topic: Option<&str>) {
        let now = selected_topic.map(|s| s.to_string());
        if self.inspector_topic != now {
            self.inspector_topic = now;
            self.inspector_path.clear();
            self.inspector_selected = 0;
            // If we no longer have a topic, fall back to Topics focus so we
            // don't get stranded in an empty inspector.
            if selected_topic.is_none() {
                self.focus = Focus::Topics;
            }
        }
    }

    /// Step into the currently-selected inspector row, if it has children.
    /// Returns true if the path changed.
    pub fn drill_in(&mut self, selected_topic: Option<&str>) -> bool {
        let Some(msg) = self.inspector_message(selected_topic) else {
            return false;
        };
        let rows = level_rows(msg, &self.inspector_path);
        let Some(row) = rows.get(self.inspector_selected) else {
            return false;
        };
        if !row.has_children {
            return false;
        }
        self.inspector_path.push(self.inspector_selected);
        self.inspector_selected = 0;
        true
    }

    /// Pop one level from the drill path. If already at root, hand focus back
    /// to the topics pane. Returns true if any state changed.
    pub fn drill_out(&mut self) -> bool {
        if let Some(parent_idx) = self.inspector_path.pop() {
            // Re-anchor the cursor on the row we descended from, so the user
            // doesn't lose their place when popping back up.
            self.inspector_selected = parent_idx;
            true
        } else if self.focus == Focus::Inspector {
            self.focus = Focus::Topics;
            true
        } else {
            false
        }
    }

    /// Move the inspector cursor by `delta`, wrapping inside the current level.
    pub fn move_inspector(&mut self, delta: i32, selected_topic: Option<&str>) {
        let len = self
            .inspector_message(selected_topic)
            .map(|m| level_rows(m, &self.inspector_path).len())
            .unwrap_or(0);
        if len == 0 {
            self.inspector_selected = 0;
            return;
        }
        let new = (self.inspector_selected as i32 + delta).rem_euclid(len as i32);
        self.inspector_selected = new as usize;
    }

    pub fn elapsed_ns(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64
    }

    fn ingest(&mut self, events: Vec<BackendEvent>) {
        let elapsed_ns = self.elapsed_ns();
        for ev in events {
            match ev {
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
                BackendEvent::TopicRemoved(name) => {
                    self.registry.remove(&name);
                    self.last_message.remove(&name);
                    self.hz_sparks.remove(&name);
                    self.bw_sparks.remove(&name);
                }
                BackendEvent::Sample {
                    name, bytes, value, ..
                } => {
                    self.registry.record(&name, elapsed_ns, bytes);
                    self.last_message.insert(name, value);
                }
                BackendEvent::DecodeFailure { .. } => {
                    // Set the sticky hint once; suppress later failures so
                    // the message doesn't churn. Wording mirrors README and
                    // names the build target so the user can act on it
                    // (the only fix is rebuilding against the peer's stack).
                    if self.notice.is_none() {
                        let distro = option_env!("ROSTOP_TARGET_DISTRO").unwrap_or("unknown");
                        let rmw = option_env!("ROSTOP_TARGET_RMW").unwrap_or("unknown");
                        self.notice = Some(format!(
                            "INFO: possible distro/RMW mismatch — built against {distro}+{rmw}, some samples failed to decode"
                        ));
                    }
                }
            }
        }
    }

    fn tick_sparklines(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_spark_tick) < Duration::from_millis(500) {
            return;
        }
        self.last_spark_tick = now;
        let elapsed_ns = self.elapsed_ns();
        let rows = self
            .registry
            .sorted_by(self.sort_key, self.sort_order, elapsed_ns);
        for entry in rows {
            let hz = entry.stats.hz(elapsed_ns);
            let bps = entry.stats.bps(elapsed_ns);
            self.hz_sparks
                .entry(entry.name.clone())
                .or_insert_with(|| Sparkline::new(28))
                .push(hz);
            self.bw_sparks
                .entry(entry.name.clone())
                .or_insert_with(|| Sparkline::new(28))
                .push(bps);
        }
    }

    fn move_selection(&mut self, delta: i32, max: usize) {
        if max == 0 {
            self.selected = 0;
            return;
        }
        let new = (self.selected as i32 + delta).rem_euclid(max as i32);
        self.selected = new as usize;
    }

    fn cycle_sort(&mut self) {
        self.sort_key = match self.sort_key {
            SortKey::Name => SortKey::Hz,
            SortKey::Hz => SortKey::Bandwidth,
            SortKey::Bandwidth => SortKey::Type,
            SortKey::Type => SortKey::Name,
        };
        // Pick the order that makes sense for each key — names and types
        // read most naturally alphabetical, rates and bandwidths are most
        // useful highest-first. There is no `r` binding to flip this; if
        // a per-key override becomes a real need, add it back behind a
        // separate keybind rather than re-exposing the global toggle.
        self.sort_order = match self.sort_key {
            SortKey::Name | SortKey::Type => SortOrder::Ascending,
            SortKey::Hz | SortKey::Bandwidth => SortOrder::Descending,
        };
    }
}

pub fn run(mut app: App) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let res = event_loop(&mut terminal, &mut app);
    restore_terminal(&mut terminal)?;
    res
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    let tick = Duration::from_millis(50);
    loop {
        if !app.paused {
            let events = app.backend.poll(Duration::from_millis(0));
            app.ingest(events);
        }
        app.tick_sparklines();

        let elapsed_ns = app.elapsed_ns();
        let rows = ui::rows::build_rows(&app.registry, app.sort_key, app.sort_order, elapsed_ns);
        if app.selected >= rows.len() {
            app.selected = rows.len().saturating_sub(1);
        }
        let selected_topic = rows.get(app.selected).map(|r| r.name.clone());
        app.sync_inspector_for_topic(selected_topic.as_deref());
        // Clamp the inspector cursor in case the underlying level shrank
        // (e.g. an array got shorter between frames).
        if let Some(msg) = app.inspector_message(selected_topic.as_deref()) {
            let len = level_rows(msg, &app.inspector_path).len();
            if app.inspector_selected >= len {
                app.inspector_selected = len.saturating_sub(1);
            }
        }

        terminal.draw(|f| ui::view::render(f, app, &rows))?;

        if event::poll(tick)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if app.fullscreen {
                    // Focus mode shows a single dedicated topic panel and
                    // only honours j/k drill keys + Esc/q. Sort and table
                    // navigation are intentionally inert here.
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('q'), _) => break,
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                        (KeyCode::Esc, _) => {
                            app.fullscreen = false;
                        }
                        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => {
                            app.move_inspector(1, selected_topic.as_deref());
                        }
                        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => {
                            app.move_inspector(-1, selected_topic.as_deref());
                        }
                        (KeyCode::Char('l'), _) | (KeyCode::Right, _) | (KeyCode::Enter, _) => {
                            app.drill_in(selected_topic.as_deref());
                        }
                        (KeyCode::Char('h'), _) | (KeyCode::Left, _) => {
                            // Only pops the inspector path; don't fall back
                            // to focus-change like Focus::Inspector's drill_out.
                            if !app.inspector_path.is_empty() {
                                app.drill_out();
                            }
                        }
                        (KeyCode::Char('p'), _) => app.paused = !app.paused,
                        _ => {}
                    }
                    continue;
                }
                match (key.code, key.modifiers, app.focus) {
                    (KeyCode::Char('q'), _, _) => break,
                    (KeyCode::Char('c'), KeyModifiers::CONTROL, _) => break,
                    // Topic-pane navigation.
                    (KeyCode::Char('j'), _, Focus::Topics) | (KeyCode::Down, _, Focus::Topics) => {
                        app.move_selection(1, rows.len());
                    }
                    (KeyCode::Char('k'), _, Focus::Topics) | (KeyCode::Up, _, Focus::Topics) => {
                        app.move_selection(-1, rows.len());
                    }
                    // From the topics pane, `l`/`→` moves focus down into
                    // the inspector pane; `f` enters focus mode (a
                    // single-topic full-screen panel).
                    (KeyCode::Char('l'), _, Focus::Topics) | (KeyCode::Right, _, Focus::Topics) => {
                        if app.inspector_message(selected_topic.as_deref()).is_some() {
                            app.focus = Focus::Inspector;
                            app.inspector_selected = 0;
                        }
                    }
                    (KeyCode::Char('f'), _, Focus::Topics) => {
                        // Only enter focus mode if we have a topic to show
                        // — empty registry shouldn't lock us into a blank
                        // single-topic panel with no way to drill anything.
                        if selected_topic.is_some() {
                            app.fullscreen = true;
                            // Leave focus pane = Topics so Esc lands the
                            // user back on the table they came from.
                            app.inspector_selected = 0;
                        }
                    }
                    // Inspector-pane navigation.
                    (KeyCode::Char('j'), _, Focus::Inspector)
                    | (KeyCode::Down, _, Focus::Inspector) => {
                        app.move_inspector(1, selected_topic.as_deref());
                    }
                    (KeyCode::Char('k'), _, Focus::Inspector)
                    | (KeyCode::Up, _, Focus::Inspector) => {
                        app.move_inspector(-1, selected_topic.as_deref());
                    }
                    (KeyCode::Char('l'), _, Focus::Inspector)
                    | (KeyCode::Right, _, Focus::Inspector)
                    | (KeyCode::Enter, _, Focus::Inspector) => {
                        app.drill_in(selected_topic.as_deref());
                    }
                    (KeyCode::Char('h'), _, _)
                    | (KeyCode::Left, _, _)
                    | (KeyCode::Esc, _, Focus::Inspector) => {
                        app.drill_out();
                    }
                    // Global keys (work regardless of focus).
                    (KeyCode::Char('s'), _, _) => app.cycle_sort(),
                    (KeyCode::Char('p'), _, _) => app.paused = !app.paused,
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::demo::DemoBackend;

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

    #[test]
    fn first_decode_failure_sets_a_sticky_notice() {
        let backend: Box<dyn RosBackend> = Box::new(DemoBackend::new());
        let mut app = App::new(backend);
        assert!(app.notice.is_none());

        app.ingest_for_tests(vec![BackendEvent::DecodeFailure {
            topic: "/cmd_vel".into(),
            type_name: "geometry_msgs/msg/Twist".into(),
        }]);
        let first = app.notice.clone().expect("notice should be set");
        assert!(
            first.contains("INFO") && first.contains("mismatch"),
            "notice should describe the mismatch, got: {first}"
        );

        // A second failure on a different topic must not replace the notice
        // (it's a one-shot hint, not a per-topic counter).
        app.ingest_for_tests(vec![BackendEvent::DecodeFailure {
            topic: "/scan".into(),
            type_name: "sensor_msgs/msg/LaserScan".into(),
        }]);
        assert_eq!(app.notice.as_deref(), Some(first.as_str()));
    }

    #[test]
    fn default_sort_is_name_ascending() {
        // Regression for #18 — a busy system constantly reshuffles equal-Hz
        // rows under "Hz Descending", so the user lands on a moving target.
        // Calmer default: Name ascending.
        let backend: Box<dyn RosBackend> = Box::new(DemoBackend::new());
        let app = App::new(backend);
        assert_eq!(app.sort_key, SortKey::Name);
        assert_eq!(app.sort_order, SortOrder::Ascending);
    }

    #[test]
    fn cycle_sort_snaps_order_to_a_sensible_default_per_key() {
        // Cycling Name -> Hz must flip to Descending so the user doesn't
        // get the slowest-first surprise. Type -> Name returns to
        // alphabetical Ascending.
        let backend: Box<dyn RosBackend> = Box::new(DemoBackend::new());
        let mut app = App::new(backend);
        assert_eq!(
            (app.sort_key, app.sort_order),
            (SortKey::Name, SortOrder::Ascending)
        );

        app.cycle_sort();
        assert_eq!(
            (app.sort_key, app.sort_order),
            (SortKey::Hz, SortOrder::Descending)
        );

        app.cycle_sort();
        assert_eq!(
            (app.sort_key, app.sort_order),
            (SortKey::Bandwidth, SortOrder::Descending)
        );

        app.cycle_sort();
        assert_eq!(
            (app.sort_key, app.sort_order),
            (SortKey::Type, SortOrder::Ascending)
        );

        app.cycle_sort();
        assert_eq!(
            (app.sort_key, app.sort_order),
            (SortKey::Name, SortOrder::Ascending)
        );
    }
}
