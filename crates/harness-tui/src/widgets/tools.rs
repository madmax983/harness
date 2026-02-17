//! Tool catalog widget showing all server-exposed MCP tools.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::AppState;

/// Render MCP tool catalog.
pub fn render_tools(area: Rect, buf: &mut Buffer, state: &AppState) {
    let items: Vec<ListItem> = state
        .tools
        .iter()
        .enumerate()
        .map(|(idx, tool)| {
            let style = if idx == state.selected_tool {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", tool.name),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(tool.description.as_str()),
            ]);

            ListItem::new(line).style(style)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" MCP Tools ")
        .title_style(Style::default().fg(Color::Cyan));

    let list = List::new(items).block(block);
    Widget::render(list, area, buf);
}
