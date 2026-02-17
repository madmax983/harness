//! Knowledge stream widget showing recent knowledge entries.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::AppState;
use crate::parse_knowledge_content;

fn kind_style(kind: &str) -> Style {
    if kind.eq_ignore_ascii_case("discovery") {
        Style::default().fg(Color::Yellow)
    } else if kind.eq_ignore_ascii_case("decision") {
        Style::default().fg(Color::Blue)
    } else if kind.eq_ignore_ascii_case("activity") {
        Style::default().fg(Color::DarkGray)
    } else if kind.eq_ignore_ascii_case("blocker") {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Gray)
    }
}

/// Render the knowledge stream panel.
pub fn render_knowledge(area: Rect, buf: &mut Buffer, state: &AppState) {
    let items: Vec<ListItem> = state
        .recent_knowledge
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let parsed = parse_knowledge_content(&entry.content);
            let time = if entry.created_at.len() >= 16 {
                entry.created_at[11..16].to_string()
            } else {
                entry.created_at.clone()
            };
            let row_style = if index == state.selected_knowledge {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            let line = Line::from(vec![
                Span::styled(format!("{time} "), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("[{}]", entry.kind), kind_style(&entry.kind)),
                Span::raw(format!(" {}: ", entry.author)),
                Span::styled(
                    format!("{} ", parsed.kind),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(parsed.summary),
            ]);
            ListItem::new(line).style(row_style)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Knowledge Stream ")
        .title_style(Style::default().fg(Color::Magenta));

    let list = List::new(items).block(block);
    Widget::render(list, area, buf);
}
