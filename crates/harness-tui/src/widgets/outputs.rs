//! Tool output stream widget.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::AppState;

/// Render recent MCP tool call outputs.
pub fn render_tool_outputs(area: Rect, buf: &mut Buffer, state: &AppState) {
    let items: Vec<ListItem> = state
        .tool_outputs
        .iter()
        .rev()
        .enumerate()
        .map(|(index, entry)| {
            let status = if entry.is_error { "ERR" } else { "OK" };
            let status_style = if entry.is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };
            let row_style = if index == state.selected_output && state.focus_output {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            let first_line = entry.output.lines().next().unwrap_or_default();
            let summary = if first_line.len() > 120 {
                format!("{}...", &first_line[..120])
            } else {
                first_line.to_string()
            };
            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", entry.at.format("%H:%M:%S")),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("[{status}] "), status_style),
                Span::styled(
                    format!("{}: ", entry.tool),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(summary),
            ]);
            ListItem::new(line).style(row_style)
        })
        .collect();

    let title = if state.focus_output {
        " Tool Output [focused] "
    } else {
        " Tool Output "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_style(Style::default().fg(Color::Green));

    let list = List::new(items).block(block);
    Widget::render(list, area, buf);
}
