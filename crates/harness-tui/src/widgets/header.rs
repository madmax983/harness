//! Header bar widget showing session info and summary stats.

use harness_persistence::{SessionId, TaskStatus};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::AppState;

/// Render the header bar with session info and task/agent summary.
pub fn render_header(area: Rect, buf: &mut Buffer, state: &AppState, session_id: &SessionId) {
    let pending = state
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Pending)
        .count();
    let in_progress = state
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::InProgress || t.status == TaskStatus::Claimed)
        .count();
    let completed = state
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Completed)
        .count();

    let session_short = &session_id.to_string()[..8.min(session_id.to_string().len())];

    let uptime = state.last_update.map(|_| "running").unwrap_or("starting");

    let header_text = Line::from(vec![
        Span::styled(" Session: ", Style::default().fg(Color::DarkGray)),
        Span::styled(session_short, Style::default().fg(Color::Cyan)),
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        Span::styled(uptime, Style::default().fg(Color::Green)),
        Span::styled(" | Agents: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            state.agents.len().to_string(),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(" | Tasks: ", Style::default().fg(Color::DarkGray)),
        Span::styled(pending.to_string(), Style::default().fg(Color::White)),
        Span::styled(" pending / ", Style::default().fg(Color::DarkGray)),
        Span::styled(in_progress.to_string(), Style::default().fg(Color::Yellow)),
        Span::styled(" active / ", Style::default().fg(Color::DarkGray)),
        Span::styled(completed.to_string(), Style::default().fg(Color::Green)),
        Span::styled(" done ", Style::default().fg(Color::DarkGray)),
    ]);

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .title(" Harness Hive Mind ")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    Paragraph::new(header_text).block(block).render(area, buf);
}
