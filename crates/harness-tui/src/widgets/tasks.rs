//! Task queue widget showing a table of tasks.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Row, Table};

use crate::AppState;

fn status_icon(status: &str) -> &'static str {
    if status.eq_ignore_ascii_case("pending") {
        "[ ]"
    } else if status.eq_ignore_ascii_case("claimed") {
        "[>]"
    } else if status.eq_ignore_ascii_case("in_progress") {
        "[~]"
    } else if status.eq_ignore_ascii_case("completed") {
        "[v]"
    } else if status.eq_ignore_ascii_case("failed") {
        "[x]"
    } else {
        "[?]"
    }
}

fn priority_style(priority: &str) -> Style {
    if priority.eq_ignore_ascii_case("critical") {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if priority.eq_ignore_ascii_case("high") {
        Style::default().fg(Color::Yellow)
    } else if priority.eq_ignore_ascii_case("medium") {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
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

            let assignee = task.assigned_to.clone().unwrap_or_else(|| "-".to_string());

            Row::new(vec![
                Cell::from(status_icon(&task.status)),
                Cell::from(task.title.as_str()),
                Cell::from(assignee),
                Cell::from(task.priority.clone()).style(priority_style(&task.priority)),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(5),
        Constraint::Min(24),
        Constraint::Length(18),
        Constraint::Length(10),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Tasks ")
        .title_style(Style::default().fg(Color::Green));

    let table = Table::new(rows, widths).header(header).block(block);
    Widget::render(table, area, buf);
}
