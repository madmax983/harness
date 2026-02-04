//! TUI application state.

use harness_persistence::AgentId;

/// Which panel is currently focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusedPanel {
    /// Channel list panel.
    #[default]
    Channels,
    /// Message view panel.
    Messages,
    /// Input field.
    Input,
    /// Agent list panel.
    Agents,
}

impl FocusedPanel {
    /// Cycle to the next panel.
    pub fn next(self) -> Self {
        match self {
            Self::Channels => Self::Messages,
            Self::Messages => Self::Input,
            Self::Input => Self::Agents,
            Self::Agents => Self::Channels,
        }
    }

    /// Cycle to the previous panel.
    pub fn prev(self) -> Self {
        match self {
            Self::Channels => Self::Agents,
            Self::Messages => Self::Channels,
            Self::Input => Self::Messages,
            Self::Agents => Self::Input,
        }
    }
}

/// Application mode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AppMode {
    /// Normal mode.
    #[default]
    Normal,
    /// Command mode (after pressing /).
    Command,
    /// Spawn dialog.
    SpawnDialog,
    /// Kill confirmation dialog.
    KillConfirm { agent_id: AgentId },
}

/// Application state.
#[derive(Debug, Default)]
pub struct AppState {
    /// Currently focused panel.
    pub focus: FocusedPanel,
    /// Current mode.
    pub mode: AppMode,
    /// Selected channel index.
    pub selected_channel: usize,
    /// Selected agent index.
    pub selected_agent: usize,
    /// Message scroll offset.
    pub message_scroll: usize,
    /// Input buffer.
    pub input: String,
    /// Command buffer (when in command mode).
    pub command: String,
    /// Whether the app should quit.
    pub should_quit: bool,
    /// Status message to display.
    pub status_message: Option<String>,
}

impl AppState {
    /// Create a new app state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cycle focus to the next panel.
    pub fn focus_next(&mut self) {
        self.focus = self.focus.next();
    }

    /// Cycle focus to the previous panel.
    pub fn focus_prev(&mut self) {
        self.focus = self.focus.prev();
    }

    /// Enter command mode.
    pub fn enter_command_mode(&mut self) {
        self.mode = AppMode::Command;
        self.command.clear();
    }

    /// Exit command mode.
    pub fn exit_command_mode(&mut self) {
        self.mode = AppMode::Normal;
        self.command.clear();
    }

    /// Set a status message.
    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    /// Clear the status message.
    pub fn clear_status(&mut self) {
        self.status_message = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_cycles() {
        let mut state = AppState::new();
        assert_eq!(state.focus, FocusedPanel::Channels);

        state.focus_next();
        assert_eq!(state.focus, FocusedPanel::Messages);

        state.focus_next();
        assert_eq!(state.focus, FocusedPanel::Input);

        state.focus_next();
        assert_eq!(state.focus, FocusedPanel::Agents);

        state.focus_next();
        assert_eq!(state.focus, FocusedPanel::Channels);
    }

    #[test]
    fn command_mode_toggle() {
        let mut state = AppState::new();
        assert_eq!(state.mode, AppMode::Normal);

        state.enter_command_mode();
        assert_eq!(state.mode, AppMode::Command);

        state.exit_command_mode();
        assert_eq!(state.mode, AppMode::Normal);
    }
}
