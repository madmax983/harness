//! TUI application state for Harness v2 Hive Mind dashboard.

use chrono::{DateTime, Utc};
use harness_persistence::{Agent, Knowledge, Task};

/// Which view is currently displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DashboardView {
    #[default]
    Tasks,
    Agents,
    Knowledge,
}

impl DashboardView {
    /// Cycle to the next view.
    pub fn next(self) -> Self {
        match self {
            Self::Tasks => Self::Agents,
            Self::Agents => Self::Knowledge,
            Self::Knowledge => Self::Tasks,
        }
    }

    /// Cycle to the previous view.
    pub fn prev(self) -> Self {
        match self {
            Self::Tasks => Self::Knowledge,
            Self::Agents => Self::Tasks,
            Self::Knowledge => Self::Agents,
        }
    }
}

/// Application mode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AppMode {
    #[default]
    Normal,
    /// Input mode - typing a message to the Strategoi.
    Input,
}

/// Application state for the Hive Mind dashboard.
#[derive(Debug, Default)]
pub struct AppState {
    /// Current dashboard view.
    pub view: DashboardView,
    /// Current input mode.
    pub mode: AppMode,
    /// Selected task index in the task list.
    pub selected_task: usize,
    /// Selected agent index in the agent list.
    pub selected_agent: usize,
    /// Scroll offset in the knowledge stream.
    pub knowledge_scroll: usize,
    /// Whether the app should quit.
    pub should_quit: bool,
    /// Input buffer for messages to the Strategoi.
    pub input: String,

    // Cached data (updated by polling)
    /// Agents in the current session.
    pub agents: Vec<Agent>,
    /// Tasks in the current session.
    pub tasks: Vec<Task>,
    /// Recent knowledge entries.
    pub recent_knowledge: Vec<Knowledge>,
    /// Last time data was refreshed.
    pub last_update: Option<DateTime<Utc>>,
}

impl AppState {
    /// Create a new default app state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cycle to the next dashboard view.
    pub fn view_next(&mut self) {
        self.view = self.view.next();
    }

    /// Cycle to the previous dashboard view.
    pub fn view_prev(&mut self) {
        self.view = self.view.prev();
    }

    /// Enter input mode for talking to the Strategoi.
    pub fn enter_input_mode(&mut self) {
        self.mode = AppMode::Input;
    }

    /// Exit input mode back to normal.
    pub fn exit_input_mode(&mut self) {
        self.mode = AppMode::Normal;
    }

    /// Take the current input text and clear the buffer. Returns the text.
    pub fn take_input(&mut self) -> String {
        std::mem::take(&mut self.input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state() {
        let state = AppState::new();
        assert_eq!(state.view, DashboardView::Tasks);
        assert_eq!(state.mode, AppMode::Normal);
        assert_eq!(state.selected_task, 0);
        assert_eq!(state.selected_agent, 0);
        assert_eq!(state.knowledge_scroll, 0);
        assert!(!state.should_quit);
        assert!(state.input.is_empty());
        assert!(state.agents.is_empty());
        assert!(state.tasks.is_empty());
        assert!(state.recent_knowledge.is_empty());
        assert!(state.last_update.is_none());
    }

    #[test]
    fn view_cycles_forward() {
        let mut state = AppState::new();
        assert_eq!(state.view, DashboardView::Tasks);

        state.view_next();
        assert_eq!(state.view, DashboardView::Agents);

        state.view_next();
        assert_eq!(state.view, DashboardView::Knowledge);

        state.view_next();
        assert_eq!(state.view, DashboardView::Tasks);
    }

    #[test]
    fn view_cycles_backward() {
        let mut state = AppState::new();
        assert_eq!(state.view, DashboardView::Tasks);

        state.view_prev();
        assert_eq!(state.view, DashboardView::Knowledge);

        state.view_prev();
        assert_eq!(state.view, DashboardView::Agents);

        state.view_prev();
        assert_eq!(state.view, DashboardView::Tasks);
    }

    #[test]
    fn input_mode_toggle() {
        let mut state = AppState::new();
        assert_eq!(state.mode, AppMode::Normal);

        state.enter_input_mode();
        assert_eq!(state.mode, AppMode::Input);

        state.exit_input_mode();
        assert_eq!(state.mode, AppMode::Normal);
    }

    #[test]
    fn take_input_returns_and_clears() {
        let mut state = AppState::new();
        state.input = "hello strategoi".to_string();

        let taken = state.take_input();
        assert_eq!(taken, "hello strategoi");
        assert!(state.input.is_empty());
    }

    #[test]
    fn take_input_empty_returns_empty() {
        let mut state = AppState::new();
        let taken = state.take_input();
        assert!(taken.is_empty());
        assert!(state.input.is_empty());
    }
}
