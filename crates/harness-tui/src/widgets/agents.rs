//! Agent panel widget showing a table of agents.

use harness_persistence::AgentStatus;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Row, Table};

use crate::AppState;

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
            let status_style = match agent.status {
                AgentStatus::Active => Style::default().fg(Color::Green),
                AgentStatus::Starting => Style::default().fg(Color::Yellow),
                AgentStatus::Idle => Style::default().fg(Color::DarkGray),
                AgentStatus::Pending => Style::default().fg(Color::White),
                AgentStatus::Finished => Style::default().fg(Color::Gray),
                AgentStatus::Killed => Style::default().fg(Color::Red),
                AgentStatus::Crashed => Style::default().fg(Color::Red),
            };

            let row_style = if i == state.selected_agent {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            let task_display = agent
                .current_task
                .map(|t| t.to_string())
                .unwrap_or_else(|| "-".to_string());

            Row::new(vec![
                Cell::from(agent.id.to_string()),
                Cell::from(agent.role.to_string()),
                Cell::from(format!("{:?}", agent.status)).style(status_style),
                Cell::from(task_display),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(14),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Min(10),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Agents ")
        .title_style(Style::default().fg(Color::Yellow));

    let table = Table::new(rows, widths).header(header).block(block);

    Widget::render(table, area, buf);
}
