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
use rostop_core::endpoint::EndpointSets;
use rostop_core::message::{level_rows, DynamicValue};
use rostop_core::registry::{SortKey, SortOrder, TopicRegistry};
use rostop_core::scope::{display_path, numeric_paths, numeric_value, NumericPath, TimeSeries};
use rostop_core::sparkline::Sparkline;

use crate::backend::{BackendEvent, RosBackend};
use crate::ui;

#[derive(Default)]
pub struct DomainScanView {
    pub active: bool,
    pub total: usize,
    pub started: usize,
    pub completed: usize,
    pub finished: bool,
    pub visible: Vec<crate::domain::DomainProbeResult>,
    pub failures: Vec<(u16, String)>,
}

/// Which pane currently receives j/k/h/l input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Topics,
    Inspector,
}

pub struct ScopeState {
    pub active: bool,
    pub topic: Option<String>,
    pub fields: Vec<NumericPath>,
    pub selected_field: usize,
    pub series: TimeSeries,
    pub window: Duration,
    pub locked_y: Option<(f64, f64)>,
}

impl ScopeState {
    fn new() -> Self {
        Self {
            active: false,
            topic: None,
            fields: Vec::new(),
            selected_field: 0,
            series: TimeSeries::new(Duration::from_secs(30), 60_000),
            window: Duration::from_secs(5),
            locked_y: None,
        }
    }

    pub fn field_label(&self) -> String {
        self.fields
            .get(self.selected_field)
            .map(|path| display_path(path))
            .unwrap_or_else(|| "(no numeric fields)".into())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TopicActivity {
    pub last_sample_ns: u64,
    pub sequence: u64,
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
    /// Latest known publisher / subscriber endpoint lists per topic.
    /// `None` in either slot means the backend cannot determine that side
    /// (rendered as "(not available)"). Replaced wholesale on every
    /// `BackendEvent::Endpoints`; cleared when a topic disappears.
    pub endpoints: HashMap<String, EndpointSets>,
    /// Full-screen publisher → topic → subscriber mini-map for the selected topic.
    pub node_graph_active: bool,
    /// Topic captured on entry so graph churn cannot silently retarget the view.
    pub node_graph_topic: Option<String>,
    /// Last observed sample per topic, used to animate graph edges without
    /// claiming which individual publisher produced a sample.
    pub topic_activity: HashMap<String, TopicActivity>,
    /// Waveform scope state for the selected topic in focus mode.
    pub scope: ScopeState,
    pub domain_scan_view: DomainScanView,
    #[cfg(feature = "live")]
    domain_scan: Option<crate::domain_scan::DomainScan>,
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
            endpoints: HashMap::new(),
            node_graph_active: false,
            node_graph_topic: None,
            topic_activity: HashMap::new(),
            scope: ScopeState::new(),
            domain_scan_view: DomainScanView::default(),
            #[cfg(feature = "live")]
            domain_scan: None,
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
                    self.endpoints.remove(&name);
                    self.topic_activity.remove(&name);
                    if self.scope.topic.as_deref() == Some(name.as_str()) {
                        self.scope.active = false;
                        self.scope.topic = None;
                        self.scope.fields.clear();
                        self.scope.series.clear();
                    }
                }
                BackendEvent::Sample {
                    name,
                    bytes,
                    value,
                    at,
                } => {
                    self.registry.record(&name, elapsed_ns, bytes);
                    self.capture_scope_sample(&name, &value, at);
                    let activity = self.topic_activity.entry(name.clone()).or_default();
                    activity.last_sample_ns = elapsed_ns;
                    activity.sequence = activity.sequence.wrapping_add(1);
                    self.last_message.insert(name, value);
                }
                BackendEvent::Endpoints {
                    topic,
                    publishers,
                    subscribers,
                } => {
                    let publisher_count = publishers.as_ref().map(|items| items.len() as u32);
                    let subscriber_count = subscribers.as_ref().map(|items| items.len() as u32);
                    if let Some((current_publishers, current_subscribers)) = self
                        .registry
                        .get(&topic)
                        .map(|entry| (entry.publishers, entry.subscribers))
                    {
                        self.registry.set_endpoints(
                            &topic,
                            publisher_count.unwrap_or(current_publishers),
                            subscriber_count.unwrap_or(current_subscribers),
                        );
                    }
                    self.endpoints.insert(topic, (publishers, subscribers));
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

    pub fn enter_scope(&mut self, topic: &str) {
        self.scope.active = true;
        self.scope.topic = Some(topic.to_string());
        self.scope.fields = self
            .last_message
            .get(topic)
            .map(numeric_paths)
            .unwrap_or_default();
        self.scope.selected_field = 0;
        self.scope.locked_y = None;
        self.scope.series.clear();
    }

    pub fn leave_scope(&mut self) {
        self.scope.active = false;
    }

    pub fn enter_node_graph(&mut self, topic: &str) {
        self.node_graph_active = true;
        self.node_graph_topic = Some(topic.to_string());
    }

    pub fn leave_node_graph(&mut self) {
        self.node_graph_active = false;
        self.node_graph_topic = None;
    }

    pub fn cycle_scope_field(&mut self, delta: i32) {
        if self.scope.fields.is_empty() {
            return;
        }
        self.scope.selected_field = (self.scope.selected_field as i32 + delta)
            .rem_euclid(self.scope.fields.len() as i32)
            as usize;
        self.scope.series.clear();
        self.scope.locked_y = None;
    }

    pub fn zoom_scope(&mut self, direction: i32) {
        const WINDOWS: [u64; 5] = [1, 2, 5, 10, 30];
        let current = WINDOWS
            .iter()
            .position(|seconds| *seconds == self.scope.window.as_secs())
            .unwrap_or(2);
        let next = (current as i32 + direction).clamp(0, WINDOWS.len() as i32 - 1) as usize;
        self.scope.window = Duration::from_secs(WINDOWS[next]);
        self.scope.locked_y = None;
    }

    pub fn reset_scope_window(&mut self) {
        self.scope.window = Duration::from_secs(5);
        self.scope.locked_y = None;
    }

    pub fn toggle_scope_y_lock(&mut self) {
        if self.scope.locked_y.is_some() {
            self.scope.locked_y = None;
            return;
        }
        if let Some(stats) = self.scope.series.stats(Instant::now(), self.scope.window) {
            self.scope.locked_y = Some(padded_bounds(stats.min, stats.max));
        }
    }

    fn capture_scope_sample(&mut self, topic: &str, value: &DynamicValue, at: Instant) {
        if !self.scope.active || self.scope.topic.as_deref() != Some(topic) {
            return;
        }
        if self.scope.fields.is_empty() {
            self.scope.fields = numeric_paths(value);
        }
        let Some(path) = self.scope.fields.get(self.scope.selected_field) else {
            return;
        };
        if let Some(value) = numeric_value(value, path) {
            self.scope.series.push(at, value);
        }
    }

    pub fn start_domain_scan(&mut self) {
        let Some(current) = self.backend.domain_id() else {
            self.notice = Some("INFO: domain scanning requires the live backend".into());
            return;
        };
        #[cfg(feature = "live")]
        {
            let mut domains: Vec<_> = (0..=10)
                .filter_map(|value| crate::domain::DomainId::new(value).ok())
                .collect();
            if !domains.contains(&current) {
                domains.push(current);
                domains.sort();
            }
            match crate::domain_scan::DomainScan::start(
                domains.iter().copied(),
                crate::domain_scan::ScanConfig::default(),
            ) {
                Ok(scan) => {
                    self.domain_scan_view = DomainScanView {
                        active: true,
                        total: domains.len(),
                        ..DomainScanView::default()
                    };
                    self.domain_scan = Some(scan);
                }
                Err(error) => {
                    self.notice = Some(format!("INFO: domain scan failed to start: {error}"));
                }
            }
        }
        #[cfg(not(feature = "live"))]
        let _ = current;
    }

    pub fn poll_domain_scan(&mut self) {
        #[cfg(feature = "live")]
        if let Some(scan) = self.domain_scan.as_ref() {
            while let Ok(update) = scan.updates.try_recv() {
                match update {
                    crate::domain_scan::ScanUpdate::Started(_) => {
                        self.domain_scan_view.started += 1;
                    }
                    crate::domain_scan::ScanUpdate::Probed(result) => {
                        self.domain_scan_view.completed += 1;
                        if result.is_visible() {
                            self.domain_scan_view.visible.push(result);
                            self.domain_scan_view
                                .visible
                                .sort_by_key(|result| result.domain_id);
                        }
                    }
                    crate::domain_scan::ScanUpdate::Failed { domain, message } => {
                        self.domain_scan_view.completed += 1;
                        if message != "scan cancelled" {
                            self.domain_scan_view.failures.push((domain.get(), message));
                        }
                    }
                    crate::domain_scan::ScanUpdate::Finished => {
                        self.domain_scan_view.finished = true;
                    }
                }
            }
        }
    }

    pub fn close_domain_scan(&mut self) {
        #[cfg(feature = "live")]
        if let Some(scan) = self.domain_scan.take() {
            scan.cancel();
            drop(scan);
        }
        self.domain_scan_view.active = false;
    }
}

pub fn padded_bounds(min: f64, max: f64) -> (f64, f64) {
    if (max - min).abs() < f64::EPSILON {
        let pad = min.abs().max(1.0) * 0.05;
        (min - pad, max + pad)
    } else {
        let pad = (max - min) * 0.08;
        (min - pad, max + pad)
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
        app.poll_domain_scan();
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
                if app.domain_scan_view.active {
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('q'), _) => break,
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                        (KeyCode::Esc, _) | (KeyCode::Char('D'), _) => {
                            app.close_domain_scan();
                        }
                        _ => {}
                    }
                    continue;
                }
                if app.node_graph_active {
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('q'), _) => break,
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                        (KeyCode::Esc, _) | (KeyCode::Char('g'), _) => {
                            app.leave_node_graph();
                        }
                        (KeyCode::Char('f'), _) => {
                            app.leave_node_graph();
                            app.fullscreen = true;
                        }
                        (KeyCode::Char('w'), _) => {
                            if let Some(topic) = app.node_graph_topic.clone() {
                                app.leave_node_graph();
                                app.fullscreen = true;
                                app.enter_scope(&topic);
                            }
                        }
                        (KeyCode::Char('p'), _) => app.paused = !app.paused,
                        _ => {}
                    }
                    continue;
                }
                if app.fullscreen {
                    // Focus mode shows a single dedicated topic panel and
                    // only honours j/k drill keys + f/Esc/q. Sort and
                    // table navigation are intentionally inert here.
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('q'), _) => break,
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                        (KeyCode::Esc, _) if app.scope.active => app.leave_scope(),
                        (KeyCode::Char('w'), _) if app.scope.active => app.leave_scope(),
                        (KeyCode::Tab, _) if app.scope.active => app.cycle_scope_field(1),
                        (KeyCode::BackTab, _) if app.scope.active => app.cycle_scope_field(-1),
                        (KeyCode::Char('+') | KeyCode::Char('='), _) if app.scope.active => {
                            app.zoom_scope(-1);
                        }
                        (KeyCode::Char('-'), _) if app.scope.active => app.zoom_scope(1),
                        (KeyCode::Char('0'), _) if app.scope.active => app.reset_scope_window(),
                        (KeyCode::Char('a'), _) if app.scope.active => {
                            app.toggle_scope_y_lock();
                        }
                        (KeyCode::Char('p'), _) if app.scope.active => {
                            app.paused = !app.paused;
                        }
                        (_, _) if app.scope.active => {}
                        (KeyCode::Char('g'), _) => {
                            if let Some(topic) = selected_topic.as_deref() {
                                app.enter_node_graph(topic);
                            }
                        }
                        (KeyCode::Char('w'), _) => {
                            if let Some(topic) = selected_topic.as_deref() {
                                app.enter_scope(topic);
                            }
                        }
                        // `f` toggles focus mode on the way in and on the
                        // way out, so the user can keep their hand on the
                        // same key. `Esc` works too for muscle memory.
                        (KeyCode::Esc, _) | (KeyCode::Char('f'), _) => {
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
                    (KeyCode::Char('w'), _, Focus::Topics) => {
                        if let Some(topic) = selected_topic.as_deref() {
                            app.fullscreen = true;
                            app.enter_scope(topic);
                        }
                    }
                    (KeyCode::Char('g'), _, Focus::Topics) if selected_topic.is_some() => {
                        app.enter_node_graph(selected_topic.as_deref().expect("guarded above"));
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
                    (KeyCode::Char('D'), _, _) => app.start_domain_scan(),
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
    fn endpoint_refresh_updates_topic_counts() {
        let backend: Box<dyn RosBackend> = Box::new(DemoBackend::new());
        let mut app = App::new(backend);
        app.ingest_for_tests(vec![empty_topic_event("/parameter_events")]);

        app.ingest_for_tests(vec![BackendEvent::Endpoints {
            topic: "/parameter_events".into(),
            publishers: Some(Vec::new()),
            subscribers: Some(Vec::new()),
        }]);

        let entry = app.registry.get("/parameter_events").unwrap();
        assert_eq!(entry.publishers, 0);
        assert_eq!(entry.subscribers, 0);
    }

    #[test]
    fn scope_captures_selected_numeric_field_with_event_time() {
        let backend: Box<dyn RosBackend> = Box::new(DemoBackend::new());
        let mut app = App::new(backend);
        let initial = DynamicValue::Struct(vec![("value".into(), DynamicValue::F64(1.0))]);
        app.last_message.insert("/signal".into(), initial);
        app.enter_scope("/signal");

        let at = Instant::now();
        app.ingest_for_tests(vec![BackendEvent::Sample {
            name: "/signal".into(),
            bytes: 8,
            value: DynamicValue::Struct(vec![("value".into(), DynamicValue::F64(2.5))]),
            at,
        }]);

        let stats = app
            .scope
            .series
            .stats(at, Duration::from_secs(1))
            .expect("scope should contain the sample");
        assert_eq!(stats.current, 2.5);
        assert_eq!(app.scope.field_label(), "value");
    }

    #[test]
    fn cycling_scope_field_clears_old_series() {
        let backend: Box<dyn RosBackend> = Box::new(DemoBackend::new());
        let mut app = App::new(backend);
        app.last_message.insert(
            "/signal".into(),
            DynamicValue::Struct(vec![
                ("a".into(), DynamicValue::F64(1.0)),
                ("b".into(), DynamicValue::F64(2.0)),
            ]),
        );
        app.enter_scope("/signal");
        app.scope.series.push(Instant::now(), 1.0);

        app.cycle_scope_field(1);

        assert_eq!(app.scope.field_label(), "b");
        assert!(app
            .scope
            .series
            .stats(Instant::now(), Duration::from_secs(1))
            .is_none());
    }

    #[test]
    fn padded_scope_bounds_handle_constant_signals() {
        let bounds = padded_bounds(-2.0, -2.0);
        assert!(bounds.0 < -2.0);
        assert!(bounds.1 > -2.0);
    }

    #[test]
    fn unavailable_endpoint_side_preserves_previous_count() {
        let backend: Box<dyn RosBackend> = Box::new(DemoBackend::new());
        let mut app = App::new(backend);
        app.ingest_for_tests(vec![BackendEvent::Topic {
            name: "/parameter_events".into(),
            type_name: "std_msgs/msg/Empty".into(),
            publishers: 3,
            subscribers: 2,
        }]);

        app.ingest_for_tests(vec![BackendEvent::Endpoints {
            topic: "/parameter_events".into(),
            publishers: Some(Vec::new()),
            subscribers: None,
        }]);

        let entry = app.registry.get("/parameter_events").unwrap();
        assert_eq!(entry.publishers, 0);
        assert_eq!(entry.subscribers, 2);
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
