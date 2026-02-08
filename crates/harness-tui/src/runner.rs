//! TUI runner - main event loop for the Hive Mind dashboard.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::Terminal;
use ratatui::prelude::*;

use harness_persistence::{Repository, SessionId};

use crate::widgets;
use crate::{AppMode, AppState};

/// Main TUI runner that owns the event loop and rendering.
pub struct TuiRunner<R: Repository> {
    state: AppState,
    repository: Arc<R>,
    session_id: SessionId,
}

impl<R: Repository + 'static> TuiRunner<R> {
    /// Create a new TUI runner.
    pub fn new(repository: Arc<R>, session_id: SessionId) -> Self {
        Self {
            state: AppState::new(),
            repository,
            session_id,
        }
    }

    /// Run the TUI event loop.
    pub async fn run(&mut self) -> anyhow::Result<()> {
        // Setup terminal
        terminal::enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Initial data fetch
        self.poll_data().await;

        let mut last_poll = Instant::now();
        let poll_interval = Duration::from_millis(500);

        // Main loop
        loop {
            // Render
            let session_id = self.session_id;
            terminal.draw(|frame| {
                self.render(frame, &session_id);
            })?;

            if self.state.should_quit {
                break;
            }

            // Poll for events with 100ms timeout
            if event::poll(Duration::from_millis(100))?
                && let Event::Key(key) = event::read()?
            {
                self.handle_key(key);
            }

            // Poll data every 500ms
            if last_poll.elapsed() >= poll_interval {
                self.poll_data().await;
                last_poll = Instant::now();
            }
        }

        // Teardown terminal
        terminal::disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen, cursor::Show)?;

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match self.state.mode {
            AppMode::Normal => match key.code {
                KeyCode::Char('q') => self.state.should_quit = true,
                KeyCode::Tab => self.state.view_next(),
                KeyCode::BackTab => self.state.view_prev(),
                KeyCode::Char('i') => self.state.enter_input_mode(),
                KeyCode::Up => self.scroll_up(),
                KeyCode::Down => self.scroll_down(),
                _ => {}
            },
            AppMode::Input => match key.code {
                KeyCode::Esc => self.state.exit_input_mode(),
                KeyCode::Enter => {
                    let _msg = self.state.take_input();
                    // TODO: send to Strategoi via DirectMessage
                }
                KeyCode::Backspace => {
                    self.state.input.pop();
                }
                KeyCode::Char(c) => self.state.input.push(c),
                _ => {}
            },
        }
    }

    fn scroll_up(&mut self) {
        match self.state.view {
            crate::DashboardView::Tasks => {
                self.state.selected_task = self.state.selected_task.saturating_sub(1);
            }
            crate::DashboardView::Agents => {
                self.state.selected_agent = self.state.selected_agent.saturating_sub(1);
            }
            crate::DashboardView::Knowledge => {
                self.state.knowledge_scroll = self.state.knowledge_scroll.saturating_sub(1);
            }
        }
    }

    fn scroll_down(&mut self) {
        match self.state.view {
            crate::DashboardView::Tasks => {
                if !self.state.tasks.is_empty() {
                    self.state.selected_task =
                        (self.state.selected_task + 1).min(self.state.tasks.len() - 1);
                }
            }
            crate::DashboardView::Agents => {
                if !self.state.agents.is_empty() {
                    self.state.selected_agent =
                        (self.state.selected_agent + 1).min(self.state.agents.len() - 1);
                }
            }
            crate::DashboardView::Knowledge => {
                if !self.state.recent_knowledge.is_empty() {
                    self.state.knowledge_scroll = (self.state.knowledge_scroll + 1)
                        .min(self.state.recent_knowledge.len().saturating_sub(1));
                }
            }
        }
    }

    async fn poll_data(&mut self) {
        if let Ok(agents) = self.repository.list_agents(self.session_id).await {
            self.state.agents = agents;
        }
        if let Ok(tasks) = self.repository.list_tasks(self.session_id, None).await {
            self.state.tasks = tasks;
        }
        if let Ok(knowledge) = self
            .repository
            .get_recent_knowledge(self.session_id, 50)
            .await
        {
            self.state.recent_knowledge = knowledge;
        }
        self.state.last_update = Some(chrono::Utc::now());
    }

    fn render(&self, frame: &mut Frame, session_id: &SessionId) {
        let area = frame.area();

        // Vertical layout: Header | Main | Knowledge | Input
        let layout = Layout::vertical([
            Constraint::Length(3), // Header
            Constraint::Min(8),    // Main content
            Constraint::Length(8), // Knowledge stream
            Constraint::Length(2), // Input bar
        ])
        .split(area);

        // Header
        widgets::render_header(layout[0], frame.buffer_mut(), &self.state, session_id);

        // Main content based on current view
        match self.state.view {
            crate::DashboardView::Tasks => {
                // Horizontal split: agents sidebar | tasks main
                let main_split =
                    Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)])
                        .split(layout[1]);

                widgets::render_agents(main_split[0], frame.buffer_mut(), &self.state);
                widgets::render_tasks(main_split[1], frame.buffer_mut(), &self.state);
            }
            crate::DashboardView::Agents => {
                widgets::render_agents(layout[1], frame.buffer_mut(), &self.state);
            }
            crate::DashboardView::Knowledge => {
                widgets::render_knowledge(layout[1], frame.buffer_mut(), &self.state);
            }
        }

        // Knowledge stream (always visible at bottom)
        widgets::render_knowledge(layout[2], frame.buffer_mut(), &self.state);

        // Input bar
        widgets::render_input(layout[3], frame.buffer_mut(), &self.state);
    }
}
