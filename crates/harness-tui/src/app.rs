//! TUI application state for Harness MCP operator dashboard.

use chrono::{DateTime, Utc};

/// Which view is currently displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DashboardView {
    #[default]
    Tasks,
    Agents,
    Knowledge,
    Tools,
}

impl DashboardView {
    /// Cycle to the next view.
    pub fn next(self) -> Self {
        match self {
            Self::Tasks => Self::Agents,
            Self::Agents => Self::Knowledge,
            Self::Knowledge => Self::Tools,
            Self::Tools => Self::Tasks,
        }
    }

    /// Cycle to the previous view.
    pub fn prev(self) -> Self {
        match self {
            Self::Tasks => Self::Tools,
            Self::Agents => Self::Tasks,
            Self::Knowledge => Self::Agents,
            Self::Tools => Self::Knowledge,
        }
    }
}

/// Application mode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AppMode {
    #[default]
    Normal,
    /// Input mode for `tool_name {json_args}` command entry.
    Input,
}

/// Lightweight task row returned by MCP tool responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRow {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assigned_to: Option<String>,
    pub created_at: String,
    pub summary: Option<String>,
}

/// Lightweight agent row returned by MCP tool responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRow {
    pub id: String,
    pub role: String,
    pub status: String,
    pub current_task: Option<String>,
    pub is_strategoi: bool,
}

/// Lightweight knowledge row returned by MCP tool responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeRow {
    pub id: String,
    pub content: String,
    pub kind: String,
    pub author: String,
    pub created_at: String,
    pub task_id: Option<String>,
}

/// Tool definition shown in the tools catalog view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDefinitionRow {
    pub name: String,
    pub description: String,
}

/// One tool invocation output captured by the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutputRow {
    pub at: DateTime<Utc>,
    pub tool: String,
    pub is_error: bool,
    pub output: String,
}

/// Application state for the Harness MCP dashboard.
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
    /// Selected tool index in the tool catalog.
    pub selected_tool: usize,
    /// Selected row index in the knowledge stream.
    pub selected_knowledge: usize,
    /// Selected row index in the tool output stream (newest first).
    pub selected_output: usize,
    /// If true, Up/Down/Enter operate on the output panel.
    pub focus_output: bool,
    /// Whether the app should quit.
    pub should_quit: bool,
    /// Input buffer for `tool_name {json}` commands.
    pub input: String,

    // Cached data (updated by polling MCP tools)
    /// Agents in the current session.
    pub agents: Vec<AgentRow>,
    /// Tasks in the current session.
    pub tasks: Vec<TaskRow>,
    /// Recent knowledge entries.
    pub recent_knowledge: Vec<KnowledgeRow>,
    /// Server tool definitions.
    pub tools: Vec<ToolDefinitionRow>,
    /// Tool invocation outputs (including errors).
    pub tool_outputs: Vec<ToolOutputRow>,
    /// Last error seen while polling or invoking tools.
    pub last_error: Option<String>,
    /// Fingerprint of the last poll-path error already emitted to output (for deduplication).
    pub last_poll_error_fingerprint: Option<String>,
    /// Last known MCP transport session ID.
    pub mcp_session_id: Option<String>,
    /// Optional inspect panel shown as an overlay.
    pub inspect: Option<InspectPanel>,
    /// Last time data was refreshed.
    pub last_update: Option<DateTime<Utc>>,
}

/// Inspect overlay payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectPanel {
    pub title: String,
    pub body: String,
    pub scroll: usize,
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

    /// Enter input mode.
    pub fn enter_input_mode(&mut self) {
        self.mode = AppMode::Input;
    }

    /// Exit input mode back to normal.
    pub fn exit_input_mode(&mut self) {
        self.mode = AppMode::Normal;
    }

    /// Take the current input text and clear the buffer.
    pub fn take_input(&mut self) -> String {
        std::mem::take(&mut self.input)
    }

    /// Push one tool output entry and keep stream size bounded.
    pub fn push_tool_output(&mut self, tool: String, is_error: bool, output: String) {
        self.tool_outputs.push(ToolOutputRow {
            at: Utc::now(),
            tool,
            is_error,
            output,
        });

        const MAX_OUTPUTS: usize = 200;
        if self.tool_outputs.len() > MAX_OUTPUTS {
            let overflow = self.tool_outputs.len() - MAX_OUTPUTS;
            self.tool_outputs.drain(0..overflow);
            self.selected_output = self
                .selected_output
                .min(self.tool_outputs.len().saturating_sub(1));
        }

        // Newest entry is shown first in output panel.
        self.selected_output = 0;
    }

    /// Emit one poll error into output stream only when it changes.
    pub fn push_poll_error_once(&mut self, source: &str, message: String) {
        let fingerprint = format!("{source}:{message}");
        if self.last_poll_error_fingerprint.as_deref() == Some(fingerprint.as_str()) {
            return;
        }

        self.last_poll_error_fingerprint = Some(fingerprint);
        self.push_tool_output(format!("poll:{source}"), true, message);
    }

    /// Clear poll error fingerprint after a successful poll cycle.
    pub fn clear_poll_error_fingerprint(&mut self) {
        self.last_poll_error_fingerprint = None;
    }

    /// Open inspect overlay.
    pub fn open_inspect(&mut self, title: String, body: String) {
        self.inspect = Some(InspectPanel {
            title,
            body,
            scroll: 0,
        });
    }

    /// Close inspect overlay.
    pub fn close_inspect(&mut self) {
        self.inspect = None;
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
        assert_eq!(state.selected_tool, 0);
        assert_eq!(state.selected_knowledge, 0);
        assert_eq!(state.selected_output, 0);
        assert!(!state.focus_output);
        assert!(!state.should_quit);
        assert!(state.input.is_empty());
        assert!(state.agents.is_empty());
        assert!(state.tasks.is_empty());
        assert!(state.recent_knowledge.is_empty());
        assert!(state.tools.is_empty());
        assert!(state.tool_outputs.is_empty());
        assert!(state.last_error.is_none());
        assert!(state.last_poll_error_fingerprint.is_none());
        assert!(state.inspect.is_none());
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
        assert_eq!(state.view, DashboardView::Tools);

        state.view_next();
        assert_eq!(state.view, DashboardView::Tasks);
    }

    #[test]
    fn view_cycles_backward() {
        let mut state = AppState::new();
        assert_eq!(state.view, DashboardView::Tasks);

        state.view_prev();
        assert_eq!(state.view, DashboardView::Tools);

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
        state.input = "list_agents {}".to_string();

        let taken = state.take_input();
        assert_eq!(taken, "list_agents {}");
        assert!(state.input.is_empty());
    }

    #[test]
    fn tool_output_stream_is_bounded() {
        let mut state = AppState::new();
        for idx in 0..210 {
            state.push_tool_output("tool".to_string(), false, format!("output-{idx}"));
        }

        assert_eq!(state.tool_outputs.len(), 200);
        assert_eq!(state.tool_outputs[0].output, "output-10");
        assert_eq!(state.selected_output, 0);
    }

    #[test]
    fn inspect_panel_open_close() {
        let mut state = AppState::new();
        state.open_inspect("Knowledge".to_string(), "body".to_string());
        assert!(state.inspect.is_some());
        assert_eq!(state.inspect.as_ref().expect("panel").title, "Knowledge");
        state.close_inspect();
        assert!(state.inspect.is_none());
    }

    #[test]
    fn poll_error_is_deduplicated_until_cleared() {
        let mut state = AppState::new();
        state.push_poll_error_once("list_tasks", "boom".to_string());
        state.push_poll_error_once("list_tasks", "boom".to_string());
        assert_eq!(state.tool_outputs.len(), 1);

        state.clear_poll_error_fingerprint();
        state.push_poll_error_once("list_tasks", "boom".to_string());
        assert_eq!(state.tool_outputs.len(), 2);
    }
}
