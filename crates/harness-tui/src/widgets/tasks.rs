//! Task queue widget showing a table of tasks.

use harness_persistence::{Priority, TaskStatus};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Row, Table};

use crate::AppState;

fn status_icon(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "[ ]",
        TaskStatus::Claimed => "[>]",
        TaskStatus::InProgress => "[~]",
        TaskStatus::Completed => "[v]",
        TaskStatus::Failed => "[x]",
    }
}

fn priority_style(priority: Priority) -> Style {
    match priority {
        Priority::Critical => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        Priority::High => Style::default().fg(Color::Yellow),
        Priority::Medium => Style::default().fg(Color::White),
        Priority::Low => Style::default().fg(Color::DarkGray),
    }
}

/// Render the task queue panel.
pub fn render_tasks(area: Rect, buf: &mut Buffer, state: &AppState) {
    let header = Row::new(vec!["Status", "Title", "Assignee", "Priority"])
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .bottom_margin(1);

    let rows: Vec<Row> = state
        .tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let row_style = if i == state.selected_task {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            let assignee = task
                .assigned_to
                .map(|a| a.to_string())
                .unwrap_or_else(|| "-".to_string());

            Row::new(vec![
                Cell::from(status_icon(task.status)),
                Cell::from(task.title.as_str()),
                Cell::from(assignee),
                Cell::from(format!("{:?}", task.priority)).style(priority_style(task.priority)),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(5),
        Constraint::Min(20),
        Constraint::Length(14),
        Constraint::Length(10),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Tasks ")
        .title_style(Style::default().fg(Color::Green));

    let table = Table::new(rows, widths).header(header).block(block);

    Widget::render(table, area, buf);
}
