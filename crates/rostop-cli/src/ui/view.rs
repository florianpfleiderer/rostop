//! ratatui rendering: 3-pane layout (table / inspector / sparklines) + status bar.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;
use rostop_core::message::{level_rows, path_segments};

use crate::app::{App, Focus};
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
                " rostop ─ {} ─ {} topics ",
                app.backend.label(),
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
    let mode = if app.filter_editing {
        format!("[FILTER: {}_]", app.filter)
    } else if app.paused {
        "[PAUSED]".to_string()
    } else {
        "[LIVE]".to_string()
    };
    let sort = format!("sort:{:?} {:?}", app.sort_key, app.sort_order);
    let help = match app.focus {
        Focus::Topics => {
            "j/k:move  l:inspect  /:filter  s:sort  r:reverse  p:pause  g/G:top/bot  q:quit"
        }
        Focus::Inspector => "j/k:move  l:drill-in  h:drill-out/back  g/G:top/bot  p:pause  q:quit",
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
        Span::raw(format!("  filter:{:?}", app.filter)),
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
