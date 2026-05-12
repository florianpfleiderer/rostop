//! ratatui rendering: 3-pane layout (table / inspector / sparklines) + status bar.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;
use rostop_core::message::{flatten_rows, DynamicValue};

use crate::app::App;
use crate::ui::rows::{fmt_bps, TopicTableRow};

pub fn render(f: &mut Frame, app: &App, rows: &[TopicTableRow]) {
    let area = f.area();
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

fn render_topic_table(f: &mut Frame, area: Rect, app: &App, rows: &[TopicTableRow]) {
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

    let table_rows: Vec<Row> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let selected = i == app.selected;
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
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
        Block::default().borders(Borders::ALL).title(format!(
            " rostop ─ {} ─ {} topics ",
            app.backend.label(),
            rows.len()
        )),
    );
    f.render_widget(table, area);
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
    let selected_name = rows.get(app.selected).map(|r| r.name.clone());
    let title = match &selected_name {
        Some(n) => format!(" inspector ─ {n} "),
        None => " inspector ".into(),
    };
    let lines: Vec<Line> = match selected_name.as_ref().and_then(|n| app.last_message.get(n)) {
        Some(value) => render_message_lines(value),
        None => vec![Line::from(Span::styled(
            "  (no message yet)",
            Style::default().fg(Color::DarkGray),
        ))],
    };
    let para = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(para, area);
}

fn render_message_lines(v: &DynamicValue) -> Vec<Line<'static>> {
    let rows = flatten_rows(v);
    rows.into_iter()
        .map(|r| {
            let indent = "  ".repeat(r.depth as usize);
            let bullet = if r.has_children { "▾" } else { "·" };
            let name_style = if r.has_children {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let value_style = Style::default().fg(Color::Green);
            let mut spans = vec![
                Span::raw(indent),
                Span::styled(format!("{bullet} "), Style::default().fg(Color::DarkGray)),
                Span::styled(r.name, name_style),
            ];
            if !r.value_text.is_empty() {
                spans.push(Span::raw(": "));
                spans.push(Span::styled(r.value_text, value_style));
            }
            Line::from(spans)
        })
        .collect()
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
    let mode = if app.filter_editing {
        format!("[FILTER: {}_]", app.filter)
    } else if app.paused {
        "[PAUSED]".to_string()
    } else {
        "[LIVE]".to_string()
    };
    let sort = format!("sort:{:?} {:?}", app.sort_key, app.sort_order);
    let help = "j/k:move  /:filter  s:sort  r:reverse  p:pause  g/G:top/bot  q:quit";
    let line = Line::from(vec![
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
        Span::raw(format!("  filter:{:?}", app.filter)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
