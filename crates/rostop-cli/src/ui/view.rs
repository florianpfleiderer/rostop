//! ratatui rendering: 3-pane layout (table / inspector / sparklines) + status bar.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, Borders, Cell, Chart, Dataset, GraphType, Paragraph, Row, Table,
};
use ratatui::Frame;
use rostop_core::endpoint::{gid_hex_short, sort_endpoints, EndpointInfo, EndpointSets};
use rostop_core::message::{level_rows, path_segments};
use rostop_core::registry::SortOrder;

use crate::app::{padded_bounds, App, Focus};
use crate::ui::rows::{fmt_bps, TopicTableRow};

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
        format!("  (idle — no messages in {idle_secs}s · {publishers} pub / {subscribers} sub)")
    }
}

pub fn render(f: &mut Frame, app: &mut App, rows: &[TopicTableRow]) {
    let area = f.area();
    if app.fullscreen {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(8), Constraint::Length(1)])
            .split(area);
        render_fullscreen_topic(f, chunks[0], app, rows);
        render_status_bar(f, chunks[1], app);
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(13),
            Constraint::Length(1),
        ])
        .split(area);
    render_topic_table(f, chunks[0], app, rows);
    render_bottom(f, chunks[1], app, rows);
    render_status_bar(f, chunks[2], app);
}

fn render_fullscreen_topic(f: &mut Frame, area: Rect, app: &App, rows: &[TopicTableRow]) {
    let Some(row) = rows.get(app.selected) else {
        // Registry emptied out from under us — fall through to a stub
        // panel rather than panicking. Esc will land the user back on
        // the table.
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" focus ─ (no topic) ");
        f.render_widget(
            Paragraph::new(Line::from("  (the selected topic disappeared — press Esc)"))
                .block(block),
            area,
        );
        return;
    };

    if app.scope.active {
        render_scope(f, area, app, row);
        return;
    }

    let title = format!(
        " focus ─ {} ─ {} ─ {}+{} ",
        row.name,
        row.type_name,
        option_env!("ROSTOP_TARGET_DISTRO").unwrap_or("?"),
        option_env!("ROSTOP_TARGET_RMW").unwrap_or("?"),
    );

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(true))
        .title(title);
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    // Metrics strip, then publisher/subscriber lists, then the message tree.
    // Endpoint section height is bounded so a pathological topic with dozens
    // of endpoints can't squeeze the message tree out of view.
    let endpoints = app.endpoints.get(&row.name);
    let endpoints_height = endpoint_section_height(endpoints);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(endpoints_height),
            Constraint::Min(1),
        ])
        .split(inner);

    render_fullscreen_metrics(f, layout[0], app, row);
    render_fullscreen_endpoints(f, layout[1], endpoints);
    render_fullscreen_message_tree(f, layout[2], app, &row.name);
}

fn render_scope(f: &mut Frame, area: Rect, app: &App, row: &TopicTableRow) {
    let field = app.scope.field_label();
    let window_secs = app.scope.window.as_secs_f64();
    let now = std::time::Instant::now();
    let stats = app.scope.series.stats(now, app.scope.window);
    let bounds = app
        .scope
        .locked_y
        .or_else(|| stats.map(|stats| padded_bounds(stats.min, stats.max)))
        .unwrap_or((-1.0, 1.0));
    let plot_width = area.width.saturating_sub(12) as usize;
    let points = app
        .scope
        .series
        .plot_points(now, app.scope.window, plot_width);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        )
        .title(Line::from(vec![
            Span::styled(
                " waveform ",
                Style::default().fg(Color::Black).bg(Color::Cyan),
            ),
            Span::styled(row.name.clone(), Style::default().fg(Color::Yellow)),
            Span::raw(" ─ "),
            Span::styled(field.clone(), Style::default().fg(Color::LightCyan)),
            Span::raw(" "),
        ]));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(inner);

    let (current, min, max, mean) = stats
        .map(|stats| (stats.current, stats.min, stats.max, stats.mean))
        .unwrap_or((0.0, 0.0, 0.0, 0.0));
    let lock = if app.scope.locked_y.is_some() {
        "LOCKED"
    } else {
        "AUTO"
    };
    let summary = vec![
        Line::from(vec![
            metric("NOW", current, Color::Yellow),
            Span::raw("    "),
            metric("MIN", min, Color::Blue),
            Span::raw("    "),
            metric("MAX", max, Color::Magenta),
            Span::raw("    "),
            metric("MEAN", mean, Color::Green),
        ]),
        Line::from(vec![
            Span::styled(
                format!(" {window_secs:>4.0}s window "),
                Style::default().fg(Color::Black).bg(Color::DarkGray),
            ),
            Span::raw("  "),
            Span::styled(
                format!(" Y {lock} "),
                if app.scope.locked_y.is_some() {
                    Style::default().fg(Color::Black).bg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Black).bg(Color::Green)
                },
            ),
            Span::raw(format!(
                "  field {}/{}",
                app.scope.selected_field.saturating_add(1),
                app.scope.fields.len()
            )),
        ]),
    ];
    f.render_widget(Paragraph::new(summary), chunks[0]);

    if points.is_empty() {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    if app.scope.fields.is_empty() {
                        "  No numeric fields in this message"
                    } else {
                        "  Collecting samples…"
                    },
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .block(Block::default().borders(Borders::TOP)),
            chunks[1],
        );
        return;
    }

    let datasets = vec![Dataset::default()
        .name(field)
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        )
        .data(&points)];
    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .x_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([-window_secs, 0.0])
                .labels(vec![
                    Span::raw(format!("-{window_secs:.0}s")),
                    Span::raw(format!("-{:.0}s", window_secs / 2.0)),
                    Span::raw("now"),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([bounds.0, bounds.1])
                .labels(vec![
                    Span::raw(format_value(bounds.0)),
                    Span::raw(format_value((bounds.0 + bounds.1) / 2.0)),
                    Span::raw(format_value(bounds.1)),
                ]),
        );
    f.render_widget(chart, chunks[1]);
}

fn metric(label: &'static str, value: f64, color: Color) -> Span<'static> {
    Span::styled(
        format!(" {label} {} ", format_value(value)),
        Style::default()
            .fg(Color::Black)
            .bg(color)
            .add_modifier(Modifier::BOLD),
    )
}

fn format_value(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude >= 100_000.0 || (magnitude > 0.0 && magnitude < 0.001) {
        format!("{value:.2e}")
    } else if magnitude >= 100.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.3}")
    }
}

/// Per-section row cap. Each endpoint takes 2 lines (header + qos detail).
/// A pathological topic with many endpoints will show a "+N more" footer.
const MAX_ENDPOINT_ROWS: usize = 6;

fn endpoint_section_height(endpoints: Option<&EndpointSets>) -> u16 {
    let (pubs, subs): (Option<usize>, Option<usize>) = match endpoints {
        Some((p, s)) => (p.as_ref().map(Vec::len), s.as_ref().map(Vec::len)),
        None => (None, None),
    };
    let mut h = 0usize;
    for slot in [pubs, subs] {
        h += 1; // section heading
        match slot {
            None => h += 1,    // "(not available)" placeholder
            Some(0) => h += 1, // "(none)" placeholder
            Some(n) => {
                h += n.min(MAX_ENDPOINT_ROWS) * 2;
                if n > MAX_ENDPOINT_ROWS {
                    h += 1; // "+N more" footer
                }
            }
        }
    }
    h as u16
}

fn render_fullscreen_endpoints(f: &mut Frame, area: Rect, endpoints: Option<&EndpointSets>) {
    let mut lines: Vec<Line> = Vec::new();

    let (pubs, subs): (Option<Vec<EndpointInfo>>, Option<Vec<EndpointInfo>>) = match endpoints {
        Some((p, s)) => {
            let mut p = p.clone();
            let mut s = s.clone();
            if let Some(v) = &mut p {
                sort_endpoints(v);
            }
            if let Some(v) = &mut s {
                sort_endpoints(v);
            }
            (p, s)
        }
        None => (None, None),
    };

    push_endpoint_section(&mut lines, "PUBLISHERS", pubs.as_deref());
    push_endpoint_section(&mut lines, "SUBSCRIBERS", subs.as_deref());

    f.render_widget(Paragraph::new(lines), area);
}

fn push_endpoint_section(
    lines: &mut Vec<Line<'_>>,
    title: &'static str,
    items: Option<&[EndpointInfo]>,
) {
    lines.push(Line::from(Span::styled(
        title,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    let items = match items {
        None => {
            lines.push(Line::from(Span::styled(
                "  (not available)",
                Style::default().fg(Color::DarkGray),
            )));
            return;
        }
        Some(items) => items,
    };
    if items.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(Color::DarkGray),
        )));
        return;
    }
    let yellow = Style::default().fg(Color::Yellow);
    let dim = Style::default().fg(Color::DarkGray);
    for ep in items.iter().take(MAX_ENDPOINT_ROWS) {
        let ns = if ep.node_namespace == "/" {
            String::new()
        } else {
            format!(" ({})", ep.node_namespace)
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(ep.node_name.clone(), yellow),
            Span::styled(ns, dim),
            Span::raw("  "),
            Span::styled(
                format!(
                    "{}/{}  {}",
                    ep.qos.reliability.as_str(),
                    ep.qos.durability.as_str(),
                    ep.qos.history_display()
                ),
                Style::default().fg(Color::Green),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!(
                    "liveliness {}   gid {}",
                    ep.qos.liveliness.as_str(),
                    gid_hex_short(&ep.endpoint_gid),
                ),
                dim,
            ),
        ]));
    }
    if items.len() > MAX_ENDPOINT_ROWS {
        lines.push(Line::from(Span::styled(
            format!("  +{} more", items.len() - MAX_ENDPOINT_ROWS),
            dim,
        )));
    }
}

fn render_fullscreen_metrics(f: &mut Frame, area: Rect, app: &App, row: &TopicTableRow) {
    // Sparklines are wider than in the side panel — eat ~half the row.
    let hz_spark = app
        .hz_sparks
        .get(&row.name)
        .map(|s| s.render())
        .unwrap_or_else(|| " ".repeat(28));
    let bw_spark = app
        .bw_sparks
        .get(&row.name)
        .map(|s| s.render())
        .unwrap_or_else(|| " ".repeat(28));

    let yellow = Style::default().fg(Color::Yellow);
    let lines = vec![
        Line::from(vec![
            Span::styled("HZ      ", Style::default().fg(Color::Cyan)),
            Span::styled(format!("{:>8.1}", row.hz), yellow),
            Span::raw("   "),
            Span::styled(hz_spark, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("BW      ", Style::default().fg(Color::Magenta)),
            Span::styled(format!("{:>8}", fmt_bps(row.bps)), yellow),
            Span::raw("   "),
            Span::styled(bw_spark, Style::default().fg(Color::Magenta)),
        ]),
        Line::from(vec![
            Span::styled("JIT     ", Style::default().fg(Color::Blue)),
            Span::styled(format!("{:>5.1} ms", row.jitter_ms), yellow),
        ]),
        Line::from(vec![
            Span::styled("PUB/SUB ", Style::default().fg(Color::Green)),
            Span::styled(format!("{}/{}", row.publishers, row.subscribers), yellow),
        ]),
        Line::from(vec![
            Span::styled("IDLE    ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} s", row.idle_secs), yellow),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn render_fullscreen_message_tree(f: &mut Frame, area: Rect, app: &App, topic_name: &str) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Line::from(vec![
            Span::styled(" message", Style::default().fg(Color::Cyan)),
            Span::raw(" "),
            {
                let message = app.last_message.get(topic_name);
                if let Some(msg) = message {
                    let segs = path_segments(msg, &app.inspector_path);
                    if segs.is_empty() {
                        Span::raw("")
                    } else {
                        Span::styled(
                            format!("> {} ", segs.join(" > ")),
                            Style::default().fg(Color::DarkGray),
                        )
                    }
                } else {
                    Span::raw("")
                }
            },
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let message = app.last_message.get(topic_name);
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
                    .map(|(i, r)| render_level_line(i, r, true, app.inspector_selected))
                    .collect()
            }
        }
        None => vec![Line::from(Span::styled(
            "  (no message yet)",
            Style::default().fg(Color::DarkGray),
        ))],
    };
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_topic_table(f: &mut Frame, area: Rect, app: &mut App, rows: &[TopicTableRow]) {
    let header = Row::new([
        Cell::from(" TOPIC"),
        Cell::from("HZ"),
        Cell::from("BW"),
        Cell::from("JIT(ms)"),
        Cell::from("TYPE"),
        Cell::from("P/S"),
    ])
    .style(Style::default().fg(Color::Black).bg(Color::Cyan))
    .height(1);

    let focused = app.focus == Focus::Topics;
    let table_rows: Vec<Row> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let selected = i == app.selected;
            let style = if selected && focused {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                // Dim cursor so the user still sees where they are when focus
                // is in the inspector pane.
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let pointer = if selected { "▸ " } else { "  " };
            Row::new([
                Cell::from(format!("{pointer}{}", r.name)),
                Cell::from(format!("{:>6.1}", r.hz)),
                Cell::from(format!("{:>9}", fmt_bps(r.bps))),
                Cell::from(format!("{:>6.1}", r.jitter_ms)),
                Cell::from(r.type_name.clone()),
                Cell::from(format!("{}/{}", r.publishers, r.subscribers)),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(30),
        Constraint::Length(8),
        Constraint::Length(11),
        Constraint::Length(8),
        Constraint::Min(20),
        Constraint::Length(6),
    ];
    let table = Table::new(table_rows, widths).header(header).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style(focused))
            .title(format!(
                " rostop ─ {}{} ─ {} topics ",
                app.backend.label(),
                app.backend
                    .domain_id()
                    .map(|domain| format!(" ─ domain {domain}"))
                    .unwrap_or_default(),
                rows.len()
            )),
    );
    // Stateful render so the viewport auto-scrolls to keep the selected row
    // visible. Without this, `j`/`G` past the table area moves `app.selected`
    // off-screen and the highlight effectively disappears.
    if rows.is_empty() {
        app.topic_table_state.select(None);
    } else {
        app.topic_table_state.select(Some(app.selected));
    }
    f.render_stateful_widget(table, area, &mut app.topic_table_state);
}

fn border_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn render_bottom(f: &mut Frame, area: Rect, app: &App, rows: &[TopicTableRow]) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);
    render_inspector(f, chunks[0], app, rows);
    render_sparklines(f, chunks[1], app, rows);
}

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

fn render_level_line(
    i: usize,
    r: rostop_core::message::LevelRow,
    focused: bool,
    selected: usize,
) -> Line<'static> {
    let is_sel = i == selected;
    let bullet = if r.has_children { "▸" } else { "·" };
    let mut name_style = if r.has_children {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let mut value_style = Style::default().fg(Color::Green);
    let mut bullet_style = Style::default().fg(Color::DarkGray);
    if is_sel && focused {
        let sel = Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        name_style = sel;
        value_style = sel;
        bullet_style = sel;
    } else if is_sel {
        let dim = Style::default()
            .fg(Color::Black)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);
        name_style = dim;
        value_style = dim;
        bullet_style = dim;
    }
    let mut spans = vec![
        Span::styled(format!(" {bullet} "), bullet_style),
        Span::styled(r.name, name_style),
    ];
    if !r.value_text.is_empty() {
        spans.push(Span::styled(": ", value_style));
        spans.push(Span::styled(r.value_text, value_style));
    }
    Line::from(spans)
}

fn render_sparklines(f: &mut Frame, area: Rect, app: &App, rows: &[TopicTableRow]) {
    let selected = rows.get(app.selected);
    let title = match selected {
        Some(r) => format!(" rates ─ {} ", r.name),
        None => " rates ".into(),
    };
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(r) = selected {
        let hz_spark = app
            .hz_sparks
            .get(&r.name)
            .map(|s| s.render())
            .unwrap_or_else(|| " ".repeat(28));
        let bw_spark = app
            .bw_sparks
            .get(&r.name)
            .map(|s| s.render())
            .unwrap_or_else(|| " ".repeat(28));
        lines.push(Line::from(vec![
            Span::styled("Hz     ", Style::default().fg(Color::Cyan)),
            Span::styled(format!("{:>6.1}", r.hz), Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled(hz_spark, Style::default().fg(Color::Cyan)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("BW     ", Style::default().fg(Color::Magenta)),
            Span::styled(
                format!("{:>9}", fmt_bps(r.bps)),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  "),
            Span::styled(bw_spark, Style::default().fg(Color::Magenta)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("JIT    ", Style::default().fg(Color::Blue)),
            Span::styled(
                format!("{:>6.1} ms", r.jitter_ms),
                Style::default().fg(Color::Yellow),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("PUB/SUB ", Style::default().fg(Color::Green)),
            Span::styled(
                format!("{}/{}", r.publishers, r.subscribers),
                Style::default().fg(Color::Yellow),
            ),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "(sparklines auto-scale to the highest sample in the window)",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        lines.push(Line::from("(no topic selected)"));
    }
    let para = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(para, area);
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let mode = if app.scope.active {
        "[SCOPE]".to_string()
    } else if app.fullscreen {
        "[FOCUS]".to_string()
    } else if app.paused {
        "[PAUSED]".to_string()
    } else {
        "[LIVE]".to_string()
    };
    // Filled triangles follow the htop / `top` convention: ▼ for Descending
    // (high values flow downward — i.e. listed first) and ▲ for Ascending.
    // Tight enough to keep the status bar readable in 80-column terminals
    // and crisper than ↑/↓ arrows in low-quality terminal fonts.
    let arrow = match app.sort_order {
        SortOrder::Ascending => "▲",
        SortOrder::Descending => "▼",
    };
    let sort = format!("sort:{:?}{arrow}", app.sort_key);
    let help = if app.scope.active {
        "Tab:field  +/-:window  0:reset  a:auto/lock  p:pause  w/Esc:back  q:quit"
    } else if app.fullscreen {
        "j/k:move  l/Enter:drill-in  h:drill-out  w:waveform  f/Esc:back  q:quit"
    } else {
        match app.focus {
            Focus::Topics => "j/k:move  l:inspect  f:focus  w:waveform  s:sort  p:pause  q:quit",
            Focus::Inspector => "j/k:move  l:drill-in  h:drill-out/back  p:pause  q:quit",
        }
    };
    let mut spans = vec![
        Span::styled(
            mode,
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(sort, Style::default().fg(Color::Cyan)),
        Span::raw("   "),
        Span::styled(help, Style::default().fg(Color::DarkGray)),
    ];
    if let Some(notice) = app.notice.as_deref() {
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            notice.to_string(),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

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
        assert!(s.contains(&format!("{IDLE_THRESHOLD_SECS}s")));
        assert!(s.contains("1 pub"));
        assert!(s.contains("0 sub"));
    }

    #[test]
    fn empty_state_renders_arbitrary_pub_sub_counts() {
        let s = inspector_empty_state(10, 2, 3);
        assert!(s.contains("2 pub"), "got: {s}");
        assert!(s.contains("3 sub"), "got: {s}");
    }
}
