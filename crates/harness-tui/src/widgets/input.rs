//! Input bar widget for tool invocation commands.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::{AppMode, AppState};

/// Render the input bar at the bottom of the screen.
pub fn render_input(area: Rect, buf: &mut Buffer, state: &AppState) {
    match &state.mode {
        AppMode::Normal => {
            let help = Line::from(vec![
                Span::styled(
                    " 'i'",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" run tool command", Style::default().fg(Color::DarkGray)),
                Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "Tab",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" switch view", Style::default().fg(Color::DarkGray)),
                Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "Up/Down",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" navigate", Style::default().fg(Color::DarkGray)),
                Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "o",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" focus output", Style::default().fg(Color::DarkGray)),
                Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" inspect selected", Style::default().fg(Color::DarkGray)),
                Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "q",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" quit ", Style::default().fg(Color::DarkGray)),
            ]);

            Paragraph::new(help).render(area, buf);
        }
        AppMode::Input => {
            let input_line = Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Green)),
                Span::raw(&state.input),
                Span::styled("_", Style::default().fg(Color::Gray)),
            ]);

            let block = Block::default()
                .borders(Borders::TOP)
                .title(" Tool Command (tool_name {json_args}) ")
                .title_style(Style::default().fg(Color::Green));

            Paragraph::new(input_line).block(block).render(area, buf);
        }
    }
}
