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
use ratatui::Terminal;
use rostop_core::message::DynamicValue;
use rostop_core::registry::{SortKey, SortOrder, TopicRegistry};
use rostop_core::sparkline::Sparkline;

use crate::backend::{BackendEvent, RosBackend};
use crate::ui;

pub struct App {
    pub backend: Box<dyn RosBackend>,
    pub registry: TopicRegistry,
    pub start: Instant,
    pub selected: usize,
    pub sort_key: SortKey,
    pub sort_order: SortOrder,
    pub filter: String,
    pub filter_editing: bool,
    pub paused: bool,
    pub last_message: HashMap<String, DynamicValue>,
    pub hz_sparks: HashMap<String, Sparkline>,
    pub bw_sparks: HashMap<String, Sparkline>,
    pub last_spark_tick: Instant,
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
            sort_key: SortKey::Hz,
            sort_order: SortOrder::Descending,
            filter: String::new(),
            filter_editing: false,
            paused: false,
            last_message: HashMap::new(),
            hz_sparks: HashMap::new(),
            bw_sparks: HashMap::new(),
            last_spark_tick: Instant::now(),
        }
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
    }

    fn toggle_order(&mut self) {
        self.sort_order = match self.sort_order {
            SortOrder::Ascending => SortOrder::Descending,
            SortOrder::Descending => SortOrder::Ascending,
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
        let rows = ui::rows::build_rows(
            &app.registry,
            app.sort_key,
            app.sort_order,
            &app.filter,
            elapsed_ns,
        );
        if app.selected >= rows.len() {
            app.selected = rows.len().saturating_sub(1);
        }

        terminal.draw(|f| ui::view::render(f, app, &rows))?;

        if event::poll(tick)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if app.filter_editing {
                    match key.code {
                        KeyCode::Esc => {
                            app.filter.clear();
                            app.filter_editing = false;
                        }
                        KeyCode::Enter => app.filter_editing = false,
                        KeyCode::Backspace => {
                            app.filter.pop();
                        }
                        KeyCode::Char(c) => app.filter.push(c),
                        _ => {}
                    }
                    continue;
                }
                match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), _) => break,
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                    (KeyCode::Char('j'), _) | (KeyCode::Down, _) => {
                        app.move_selection(1, rows.len());
                    }
                    (KeyCode::Char('k'), _) | (KeyCode::Up, _) => {
                        app.move_selection(-1, rows.len());
                    }
                    (KeyCode::Char('g'), _) => app.selected = 0,
                    (KeyCode::Char('G'), _) => {
                        app.selected = rows.len().saturating_sub(1);
                    }
                    (KeyCode::Char('/'), _) => {
                        app.filter_editing = true;
                    }
                    (KeyCode::Char('s'), _) => app.cycle_sort(),
                    (KeyCode::Char('r'), _) => app.toggle_order(),
                    (KeyCode::Char('p'), _) => app.paused = !app.paused,
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
