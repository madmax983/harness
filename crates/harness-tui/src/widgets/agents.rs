//! Agent panel widget showing a table of agents.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Row, Table};

use crate::AppState;

fn status_style(status: &str) -> Style {
    if status.eq_ignore_ascii_case("active") {
        Style::default().fg(Color::Green)
    } else if status.eq_ignore_ascii_case("starting") {
        Style::default().fg(Color::Yellow)
    } else if status.eq_ignore_ascii_case("idle") {
        Style::default().fg(Color::DarkGray)
    } else if status.eq_ignore_ascii_case("pending") {
        Style::default().fg(Color::White)
    } else if status.eq_ignore_ascii_case("finished") {
        Style::default().fg(Color::Gray)
    } else if status.eq_ignore_ascii_case("killed") || status.eq_ignore_ascii_case("crashed") {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    }
}

/// Render the agent table panel.
pub fn render_agents(area: Rect, buf: &mut Buffer, state: &AppState) {
    let header = Row::new(vec!["ID", "Role", "Status", "Task"])
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .bottom_margin(1);

    let rows: Vec<Row> = state
        .agents
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let row_style = if i == state.selected_agent {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            let task_display = agent
                .current_task
                .clone()
                .unwrap_or_else(|| "-".to_string());
            Row::new(vec![
                Cell::from(agent.id.clone()),
                Cell::from(agent.role.clone()),
                Cell::from(agent.status.clone()).style(status_style(&agent.status)),
                Cell::from(task_display),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(14),
        Constraint::Length(14),
        Constraint::Length(12),
        Constraint::Min(16),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Agents ")
        .title_style(Style::default().fg(Color::Yellow));

    let table = Table::new(rows, widths).header(header).block(block);
    Widget::render(table, area, buf);
}
