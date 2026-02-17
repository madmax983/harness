//! Header bar widget showing server/session info and summary stats.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::{AppMode, AppState};

/// Render the header bar with server and summary information.
pub fn render_header(area: Rect, buf: &mut Buffer, state: &AppState, server_url: &str) {
    let pending = state
        .tasks
        .iter()
        .filter(|t| t.status.eq_ignore_ascii_case("pending"))
        .count();
    let in_progress = state
        .tasks
        .iter()
        .filter(|t| {
            t.status.eq_ignore_ascii_case("in_progress") || t.status.eq_ignore_ascii_case("claimed")
        })
        .count();
    let completed = state
        .tasks
        .iter()
        .filter(|t| t.status.eq_ignore_ascii_case("completed"))
        .count();

    let status_text = if state.last_error.is_some() {
        ("degraded", Color::Red)
    } else {
        ("connected", Color::Green)
    };

    let session_short = state
        .mcp_session_id
        .as_ref()
        .map(|id| id[..8.min(id.len())].to_string())
        .unwrap_or_else(|| "-".to_string());

    let (focus_label, focus_color) = focus_badge(state);
    let input_active = matches!(state.mode, AppMode::Input);
    let output_active = matches!(state.mode, AppMode::Normal) && state.focus_output;

    let header_text = Line::from(vec![
        Span::styled(" Server: ", Style::default().fg(Color::DarkGray)),
        Span::styled(server_url, Style::default().fg(Color::Cyan)),
        Span::styled(" | Session: ", Style::default().fg(Color::DarkGray)),
        Span::styled(session_short, Style::default().fg(Color::Yellow)),
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        Span::styled(status_text.0, Style::default().fg(status_text.1)),
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
        Span::styled(" done", Style::default().fg(Color::DarkGray)),
        Span::styled(" | Focus: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            focus_label,
            Style::default()
                .fg(focus_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::default().fg(Color::DarkGray)),
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "i",
            Style::default()
                .fg(hint_color(input_active, Color::Magenta))
                .add_modifier(if input_active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        Span::styled("] ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "input",
            Style::default().fg(hint_color(input_active, Color::DarkGray)),
        ),
        Span::styled(" ", Style::default().fg(Color::DarkGray)),
        Span::styled("[", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "o",
            Style::default()
                .fg(hint_color(output_active, Color::Cyan))
                .add_modifier(if output_active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        Span::styled("] ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "output ",
            Style::default().fg(hint_color(output_active, Color::DarkGray)),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .title(" Harness MCP Operator ")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    Paragraph::new(header_text).block(block).render(area, buf);
}

fn focus_badge(state: &AppState) -> (&'static str, Color) {
    match state.mode {
        AppMode::Input => ("INPUT", Color::Magenta),
        AppMode::Normal if state.focus_output => ("OUTPUT", Color::Cyan),
        AppMode::Normal => ("MAIN", Color::White),
    }
}

fn hint_color(active: bool, active_color: Color) -> Color {
    if active {
        active_color
    } else {
        Color::DarkGray
    }
}

#[cfg(test)]
mod tests {
    use super::{focus_badge, hint_color};
    use crate::{AppMode, AppState};
    use ratatui::style::Color;

    #[test]
    fn focus_badge_prefers_input_mode() {
        let mut state = AppState::new();
        state.mode = AppMode::Input;
        state.focus_output = true;

        let (label, color) = focus_badge(&state);
        assert_eq!(label, "INPUT");
        assert_eq!(color, Color::Magenta);
    }

    #[test]
    fn focus_badge_shows_output_when_normal_and_output_focused() {
        let mut state = AppState::new();
        state.mode = AppMode::Normal;
        state.focus_output = true;

        let (label, color) = focus_badge(&state);
        assert_eq!(label, "OUTPUT");
        assert_eq!(color, Color::Cyan);
    }

    #[test]
    fn focus_badge_shows_main_by_default() {
        let state = AppState::new();
        let (label, color) = focus_badge(&state);
        assert_eq!(label, "MAIN");
        assert_eq!(color, Color::White);
    }

    #[test]
    fn hint_color_uses_active_color_only_when_active() {
        assert_eq!(hint_color(true, Color::Cyan), Color::Cyan);
        assert_eq!(hint_color(false, Color::Cyan), Color::DarkGray);
    }
}
