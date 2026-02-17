//! TUI runner - MCP-connected event loop for the Harness operator dashboard.

use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use harness_mcp::tools::{
    AgentInfo, GetHiveStatusResponse, KnowledgeResult, ListAgentsResponse, ListTasksResponse,
    TaskInfo,
};
use ratatui::Terminal;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use serde_json::{Value, json};

use crate::app::{AgentRow, KnowledgeRow, TaskRow, ToolDefinitionRow};
use crate::widgets;
use crate::{AppMode, AppState, DashboardView, McpToolClient, parse_knowledge_content};

/// Main TUI runner that owns the event loop, MCP client, and rendering.
pub struct TuiRunner {
    state: AppState,
    mcp: McpToolClient,
    server_url: String,
}

impl TuiRunner {
    /// Connect to MCP server and build a new TUI runner.
    pub async fn connect(server_url: impl Into<String>) -> Result<Self> {
        let server_url = server_url.into();
        let mcp = McpToolClient::connect(&server_url).await?;
        let mut state = AppState::new();
        state.mcp_session_id = mcp.session_id().await;

        let mut runner = Self {
            state,
            mcp,
            server_url,
        };

        runner.refresh_tool_definitions().await;
        runner.poll_data().await;
        Ok(runner)
    }

    /// Run the TUI event loop.
    pub async fn run(&mut self) -> Result<()> {
        terminal::enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut last_poll = Instant::now();
        let poll_interval = Duration::from_millis(750);

        loop {
            terminal.draw(|frame| {
                self.render(frame);
            })?;

            if self.state.should_quit {
                break;
            }

            if event::poll(Duration::from_millis(100))?
                && let Event::Key(key) = event::read()?
                && should_process_key_event(&key)
            {
                self.handle_key(key).await;
            }

            if last_poll.elapsed() >= poll_interval {
                self.poll_data().await;
                last_poll = Instant::now();
            }
        }

        terminal::disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen, cursor::Show)?;
        self.mcp.shutdown().await?;
        Ok(())
    }

    async fn handle_key(&mut self, key: KeyEvent) {
        match self.state.mode {
            AppMode::Normal => {
                if self.state.inspect.is_some() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter => self.state.close_inspect(),
                        KeyCode::Up => {
                            if let Some(panel) = self.state.inspect.as_mut() {
                                panel.scroll = panel.scroll.saturating_sub(1);
                            }
                        }
                        KeyCode::Down => {
                            if let Some(panel) = self.state.inspect.as_mut() {
                                panel.scroll = panel.scroll.saturating_add(1);
                            }
                        }
                        _ => {}
                    }
                    return;
                }

                match key.code {
                    KeyCode::Char('q') => self.state.should_quit = true,
                    KeyCode::Tab => self.state.view_next(),
                    KeyCode::BackTab => self.state.view_prev(),
                    KeyCode::Char('i') => self.state.enter_input_mode(),
                    KeyCode::Char('o') => self.state.focus_output = !self.state.focus_output,
                    KeyCode::Enter => self.open_detail(),
                    KeyCode::Up => self.scroll_up(),
                    KeyCode::Down => self.scroll_down(),
                    _ => {}
                }
            }
            AppMode::Input => match key.code {
                KeyCode::Esc => self.state.exit_input_mode(),
                KeyCode::Enter => {
                    self.execute_input_command().await;
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
        if self.state.focus_output {
            self.state.selected_output = self.state.selected_output.saturating_sub(1);
            return;
        }

        match self.state.view {
            DashboardView::Tasks => {
                self.state.selected_task = self.state.selected_task.saturating_sub(1);
            }
            DashboardView::Agents => {
                self.state.selected_agent = self.state.selected_agent.saturating_sub(1);
            }
            DashboardView::Knowledge => {
                self.state.selected_knowledge = self.state.selected_knowledge.saturating_sub(1);
            }
            DashboardView::Tools => {
                self.state.selected_tool = self.state.selected_tool.saturating_sub(1);
            }
        }
    }

    fn scroll_down(&mut self) {
        if self.state.focus_output {
            if !self.state.tool_outputs.is_empty() {
                self.state.selected_output = (self.state.selected_output + 1)
                    .min(self.state.tool_outputs.len().saturating_sub(1));
            }
            return;
        }

        match self.state.view {
            DashboardView::Tasks => {
                if !self.state.tasks.is_empty() {
                    self.state.selected_task =
                        (self.state.selected_task + 1).min(self.state.tasks.len() - 1);
                }
            }
            DashboardView::Agents => {
                if !self.state.agents.is_empty() {
                    self.state.selected_agent =
                        (self.state.selected_agent + 1).min(self.state.agents.len() - 1);
                }
            }
            DashboardView::Knowledge => {
                if !self.state.recent_knowledge.is_empty() {
                    self.state.selected_knowledge = (self.state.selected_knowledge + 1)
                        .min(self.state.recent_knowledge.len().saturating_sub(1));
                }
            }
            DashboardView::Tools => {
                if !self.state.tools.is_empty() {
                    self.state.selected_tool =
                        (self.state.selected_tool + 1).min(self.state.tools.len() - 1);
                }
            }
        }
    }

    fn open_detail(&mut self) {
        if self.state.focus_output {
            self.open_output_detail();
            return;
        }

        match self.state.view {
            DashboardView::Knowledge => {
                let Some(entry) = self
                    .state
                    .recent_knowledge
                    .get(self.state.selected_knowledge)
                    .cloned()
                else {
                    return;
                };

                let parsed = parse_knowledge_content(&entry.content);
                let body = if let Some(pretty) = parsed.pretty_json {
                    format!(
                        "Summary\n{}\n\nRaw\n{}\n\nParsed JSON\n{}",
                        parsed.summary, entry.content, pretty
                    )
                } else {
                    entry.content
                };

                self.state
                    .open_inspect(format!("Knowledge ({})", parsed.kind), body);
            }
            DashboardView::Tools => {
                let Some(tool) = self.state.tools.get(self.state.selected_tool).cloned() else {
                    return;
                };

                let body = format!(
                    "Tool\n{}\n\nDescription\n{}\n\nRun\n{} {{}}",
                    tool.name, tool.description, tool.name
                );
                self.state.open_inspect("Tool Definition".to_string(), body);
            }
            DashboardView::Tasks | DashboardView::Agents => {}
        }
    }

    fn open_output_detail(&mut self) {
        let Some(entry) = self
            .state
            .tool_outputs
            .iter()
            .rev()
            .nth(self.state.selected_output)
            .cloned()
        else {
            return;
        };

        let body = format_output_detail_body(&entry.output);

        self.state.open_inspect(
            format!(
                "Tool Output ({}) {}",
                if entry.is_error { "ERR" } else { "OK" },
                entry.tool
            ),
            body,
        );
    }

    async fn execute_input_command(&mut self) {
        let raw = self.state.take_input();
        self.state.exit_input_mode();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }

        let (tool_name, args) = match parse_tool_command(trimmed) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.state
                    .push_tool_output("<parser>".into(), true, error.to_string());
                self.state.last_error = Some(error.to_string());
                return;
            }
        };

        match self.mcp.call_tool_raw(&tool_name, args).await {
            Ok(output) => {
                self.state.push_tool_output(
                    output.tool_name.clone(),
                    output.is_error,
                    output.text.clone(),
                );

                if output.is_error {
                    self.state.last_error = Some(output.text);
                } else {
                    self.state.last_error = None;
                    if tool_name == "list_tools" {
                        self.refresh_tool_definitions().await;
                    } else {
                        self.poll_data().await;
                    }
                }
            }
            Err(error) => {
                self.state
                    .push_tool_output(tool_name, true, format!("request failed: {error}"));
                self.state.last_error = Some(error.to_string());
            }
        }
    }

    async fn refresh_tool_definitions(&mut self) {
        match self.mcp.list_tools().await {
            Ok(list) => {
                self.state.tools = list
                    .tools
                    .into_iter()
                    .map(|tool| ToolDefinitionRow {
                        name: tool.name,
                        description: tool.description.unwrap_or_default(),
                    })
                    .collect();
                self.state.selected_tool = self
                    .state
                    .selected_tool
                    .min(self.state.tools.len().saturating_sub(1));
            }
            Err(error) => {
                let message = format!("list_tools failed: {error}");
                self.state.last_error = Some(message.clone());
                self.state.push_poll_error_once("list_tools", message);
            }
        }
    }

    async fn poll_data(&mut self) {
        let agents = self
            .mcp
            .call_tool_json::<ListAgentsResponse>("list_agents", json!({}))
            .await;
        let tasks = self
            .mcp
            .call_tool_json::<ListTasksResponse>("list_tasks", json!({}))
            .await;
        let hive_status = self
            .mcp
            .call_tool_json::<GetHiveStatusResponse>("get_hive_status", json!({}))
            .await;
        let mut had_error = false;

        match agents {
            Ok(response) => {
                self.state.agents = filter_visible_agents(response.agents);
                self.state.selected_agent = self
                    .state
                    .selected_agent
                    .min(self.state.agents.len().saturating_sub(1));
            }
            Err(error) => {
                had_error = true;
                let message = format!("list_agents failed: {error}");
                self.state.last_error = Some(message.clone());
                self.state.push_poll_error_once("list_agents", message);
            }
        }

        match tasks {
            Ok(response) => {
                self.state.tasks = response.tasks.into_iter().map(map_task).collect();
                self.state.selected_task = self
                    .state
                    .selected_task
                    .min(self.state.tasks.len().saturating_sub(1));
            }
            Err(error) => {
                had_error = true;
                let message = format!("list_tasks failed: {error}");
                self.state.last_error = Some(message.clone());
                self.state.push_poll_error_once("list_tasks", message);
            }
        }

        match hive_status {
            Ok(response) => {
                self.state.recent_knowledge = response
                    .recent_knowledge
                    .into_iter()
                    .map(map_knowledge)
                    .collect();
                self.state.selected_knowledge = self
                    .state
                    .selected_knowledge
                    .min(self.state.recent_knowledge.len().saturating_sub(1));
            }
            Err(error) => {
                had_error = true;
                let message = format!("get_hive_status failed: {error}");
                self.state.last_error = Some(message.clone());
                self.state.push_poll_error_once("get_hive_status", message);
            }
        }

        if !had_error {
            self.state.last_error = None;
            self.state.clear_poll_error_fingerprint();
        }

        self.state.last_update = Some(chrono::Utc::now());
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        // Vertical layout: Header | Main | Tool output stream | Input
        let layout = Layout::vertical([
            Constraint::Length(3), // Header
            Constraint::Min(8),    // Main content
            Constraint::Length(9), // Tool output stream
            Constraint::Length(2), // Input bar
        ])
        .split(area);

        widgets::render_header(layout[0], frame.buffer_mut(), &self.state, &self.server_url);

        match self.state.view {
            DashboardView::Tasks => {
                let main_split =
                    Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)])
                        .split(layout[1]);
                widgets::render_agents(main_split[0], frame.buffer_mut(), &self.state);
                widgets::render_tasks(main_split[1], frame.buffer_mut(), &self.state);
            }
            DashboardView::Agents => {
                widgets::render_agents(layout[1], frame.buffer_mut(), &self.state);
            }
            DashboardView::Knowledge => {
                widgets::render_knowledge(layout[1], frame.buffer_mut(), &self.state);
            }
            DashboardView::Tools => {
                widgets::render_tools(layout[1], frame.buffer_mut(), &self.state);
            }
        }

        widgets::render_tool_outputs(layout[2], frame.buffer_mut(), &self.state);
        widgets::render_input(layout[3], frame.buffer_mut(), &self.state);

        if let Some(panel) = &self.state.inspect {
            let popup = centered_rect(85, 80, area);
            Clear.render(popup, frame.buffer_mut());
            let paragraph = Paragraph::new(panel.body.as_str())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(panel.title.as_str()),
                )
                .wrap(Wrap { trim: false })
                .scroll((panel.scroll.min(u16::MAX as usize) as u16, 0));
            paragraph.render(popup, frame.buffer_mut());
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    let horizontal = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1]);
    horizontal[1]
}

fn map_agent(agent: AgentInfo) -> AgentRow {
    AgentRow {
        id: agent.id,
        role: agent.role,
        status: agent.status,
        current_task: agent.current_task,
        is_strategoi: agent.is_strategoi,
    }
}

fn filter_visible_agents(agents: Vec<AgentInfo>) -> Vec<AgentRow> {
    agents
        .into_iter()
        .filter(|agent| !agent.status.eq_ignore_ascii_case("killed"))
        .map(map_agent)
        .collect()
}

fn map_task(task: TaskInfo) -> TaskRow {
    TaskRow {
        id: task.id,
        title: task.title,
        status: task.status,
        priority: task.priority,
        assigned_to: task.assigned_to,
        created_at: task.created_at,
        summary: task.summary,
    }
}

fn map_knowledge(entry: KnowledgeResult) -> KnowledgeRow {
    KnowledgeRow {
        id: entry.id,
        content: entry.content,
        kind: entry.kind,
        author: entry.author,
        created_at: entry.created_at,
        task_id: entry.task_id,
    }
}

fn parse_tool_command(input: &str) -> Result<(String, Value)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("command is empty");
    }

    let split_index = trimmed.find(char::is_whitespace);
    let (tool_name, args_str) = match split_index {
        Some(index) => (&trimmed[..index], trimmed[index..].trim()),
        None => (trimmed, ""),
    };

    if tool_name.is_empty() {
        bail!("tool name is required");
    }

    if args_str.is_empty() {
        return Ok((tool_name.to_string(), json!({})));
    }

    let args: Value = serde_json::from_str(args_str)
        .map_err(|error| anyhow!("invalid JSON arguments for tool `{tool_name}`: {error}"))?;

    if !args.is_object() {
        return Err(anyhow!(
            "tool `{tool_name}` arguments must be a JSON object"
        ));
    }

    Ok((tool_name.to_string(), args))
}

fn should_process_key_event(key: &KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

fn format_output_detail_body(output: &str) -> String {
    match serde_json::from_str::<Value>(output) {
        Ok(value) => match serde_json::to_string_pretty(&value) {
            Ok(pretty) => format!("Raw\n{}\n\nParsed JSON\n{}", output, pretty),
            Err(_) => output.to_string(),
        },
        Err(_) => output.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        filter_visible_agents, format_output_detail_body, parse_tool_command,
        should_process_key_event,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use harness_mcp::tools::AgentInfo;
    use serde_json::json;

    #[test]
    fn parse_tool_command_defaults_to_empty_object() {
        let parsed = parse_tool_command("list_agents").expect("parse");
        assert_eq!(parsed.0, "list_agents");
        assert_eq!(parsed.1, json!({}));
    }

    #[test]
    fn parse_tool_command_with_json_arguments() {
        let parsed =
            parse_tool_command(r#"create_task {"title":"X","description":"Y"}"#).expect("parse");
        assert_eq!(parsed.0, "create_task");
        assert_eq!(parsed.1, json!({"title":"X","description":"Y"}));
    }

    #[test]
    fn parse_tool_command_rejects_invalid_json() {
        let err = parse_tool_command(r#"create_task {"title":"X""#).expect_err("must fail");
        assert!(err.to_string().contains("invalid JSON arguments"));
    }

    #[test]
    fn parse_tool_command_requires_object_arguments() {
        let err = parse_tool_command("list_tasks []").expect_err("must fail");
        assert!(err.to_string().contains("must be a JSON object"));
    }

    #[test]
    fn format_output_detail_body_pretty_prints_json() {
        let output = r#"{"ok":true,"count":2}"#;
        let body = format_output_detail_body(output);
        assert!(body.contains("Parsed JSON"));
        assert!(body.contains("\"count\": 2"));
    }

    #[test]
    fn filter_visible_agents_excludes_killed() {
        let visible = AgentInfo {
            id: "a1".to_string(),
            role: "developer".to_string(),
            status: "active".to_string(),
            current_task: None,
            is_strategoi: false,
            project_name: None,
            project_path: None,
        };
        let killed = AgentInfo {
            id: "a2".to_string(),
            role: "developer".to_string(),
            status: "killed".to_string(),
            current_task: None,
            is_strategoi: false,
            project_name: None,
            project_path: None,
        };

        let rows = filter_visible_agents(vec![visible, killed]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "a1");
    }

    #[test]
    fn should_process_key_event_ignores_release() {
        let key =
            KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Release);
        assert!(!should_process_key_event(&key));
    }

    #[test]
    fn should_process_key_event_accepts_press_and_repeat() {
        let press =
            KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Press);
        let repeat =
            KeyEvent::new_with_kind(KeyCode::Down, KeyModifiers::NONE, KeyEventKind::Repeat);
        assert!(should_process_key_event(&press));
        assert!(should_process_key_event(&repeat));
    }
}
