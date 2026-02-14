//! Agent tool request/response types.

use serde::{Deserialize, Serialize};

/// Request to register an agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegisterAgentRequest {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// For spawned agents: pre-created agent ID to activate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// Response from register_agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegisterAgentResponse {
    pub agent_id: String,
}

/// Request to list agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListAgentsRequest {
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Agent info in list response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub role: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    pub is_strategoi: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
}

/// Response from list_agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListAgentsResponse {
    pub agents: Vec<AgentInfo>,
}

/// Request to get full hive status.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetHiveStatusRequest {
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Task summary counts.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskSummary {
    pub pending: usize,
    pub claimed: usize,
    pub in_progress: usize,
    pub completed: usize,
    pub failed: usize,
}

/// Response from get_hive_status.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetHiveStatusResponse {
    pub agents: Vec<AgentInfo>,
    pub task_summary: TaskSummary,
    pub recent_knowledge: Vec<super::knowledge::KnowledgeResult>,
    /// Message inbox for current agent (if registered).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inbox: Option<MessageInbox>,
    /// Active conversation threads in the hive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_threads: Option<Vec<ActiveThread>>,
}

/// Request to disconnect an agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DisconnectAgentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from disconnect_agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DisconnectAgentResponse {
    pub success: bool,
}

/// Request to refresh MCP session context and rebind agent identity.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RefreshSessionRequest {
    /// Optional explicit agent ID to bind to this session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from refresh_session.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RefreshSessionResponse {
    /// Current Harness session ID.
    pub session_id: String,
    /// Current MCP transport session ID, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_session_id: Option<String>,
    /// Bound agent ID after refresh, if one was restored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Whether the refresh operation rebound an agent identity.
    pub rebound: bool,
}

/// Request to spawn a new agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnAgentRequest {
    /// Role for the spawned agent (MVP: only "developer" supported).
    pub role: String,
    /// Name for the spawned teammate.
    pub name: String,
    /// CLI command to execute (e.g., "claude", "codex", "gemini").
    pub cli_command: String,
    /// CLI arguments with {PROMPT} placeholder for system prompt injection.
    pub cli_args: Vec<String>,
    /// Custom instructions to include in the generated system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,
    /// Strategoi directive appended to the standard agent template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
    /// Polling interval in seconds for auto-polling get_messages and get_hive_status.
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    /// Optional task to assign immediately after spawn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_task_id: Option<String>,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

fn default_poll_interval() -> u64 {
    30
}

fn default_handshake_mode() -> String {
    "ring".to_string()
}

/// Request to spawn a team of agents and seed handshake DMs between them.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnTeamAndHandshakeRequest {
    /// Role for spawned agents (MVP: only "developer" supported).
    pub role: String,
    /// Number of agents to spawn.
    pub agent_count: usize,
    /// CLI command to execute (e.g., "claude", "codex", "gemini").
    pub cli_command: String,
    /// CLI arguments with {PROMPT} placeholder for system prompt injection.
    pub cli_args: Vec<String>,
    /// Custom instructions to include in each generated system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_prompt: Option<String>,
    /// Strategoi directive appended to each agent template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
    /// Polling interval in seconds for auto-polling get_messages and get_hive_status.
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    /// Handshake topology: "ring" or "full_mesh".
    #[serde(default = "default_handshake_mode")]
    pub handshake_mode: String,
    /// Optional custom message body used for seeded handshake DMs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handshake_message: Option<String>,
    /// Optional agent ID for multi-client support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from spawn_team_and_handshake.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnTeamAndHandshakeResponse {
    /// Spawned Harness agent IDs.
    pub agent_ids: Vec<String>,
    /// IDs of direct messages created to establish handshake links.
    pub message_ids: Vec<String>,
    /// Handshake topology used.
    pub handshake_mode: String,
}

/// Request to list running processes.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListProcessesRequest {
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Process information.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProcessInfo {
    pub agent_id: String,
    pub is_running: bool,
    pub status: String,
}

/// Response from list_processes.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListProcessesResponse {
    pub processes: Vec<ProcessInfo>,
    pub total_count: usize,
}

/// Request to kill a process.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KillProcessRequest {
    pub agent_id: String,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from kill_process.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KillProcessResponse {
    pub success: bool,
}

/// Request to cleanup stale agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CleanupStaleAgentsRequest {
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from cleanup_stale_agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CleanupStaleAgentsResponse {
    pub cleaned_count: usize,
    pub agent_ids: Vec<String>,
}

/// Request to get process output.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetProcessOutputRequest {
    pub agent_id: String,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from get_process_output.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetProcessOutputResponse {
    pub agent_id: String,
    pub stdout: String,
    pub stderr: String,
    pub is_running: bool,
}

/// Request to command an agent with a new prompt.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandAgentRequest {
    pub agent_id: String,
    /// Raw prompt to send to the agent (legacy path).
    /// Optional when using `directive`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Strategoi directive to wrap with the command prompt template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directive: Option<String>,
    pub cli_command: String,
    pub cli_args: Vec<String>,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from command_agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandAgentResponse {
    pub success: bool,
    pub agent_id: String,
}

/// Response from spawn_agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnAgentResponse {
    /// Harness agent ID.
    pub agent_id: String,
    /// Process ID of spawned CLI agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
    /// CLI command that was executed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_command: Option<String>,
    /// Claude Code teammate ID (from Task tool) - DEPRECATED.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teammate_id: Option<String>,
}

// === Enhanced Message Visibility ===

/// Direct message info for inbox display.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DirectMessageInfo {
    /// Message ID.
    pub id: String,
    /// Sending agent ID.
    pub from_agent: String,
    /// Sending agent's role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_agent_role: Option<String>,
    /// Sending agent's project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_agent_project: Option<String>,
    /// Receiving agent ID.
    pub to_agent: String,
    /// Message content.
    pub content: String,
    /// Task context for threading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// When sent.
    pub created_at: String,
}

/// Message inbox showing recent DMs for current agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageInbox {
    /// Recent messages (last N).
    pub recent_messages: Vec<DirectMessageInfo>,
    /// Total message count shown.
    pub count: usize,
}

/// Active conversation thread grouped by task or participants.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActiveThread {
    /// Task context (None for general DM threads).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Task title if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
    /// Agent IDs participating in thread.
    pub participants: Vec<String>,
    /// Total messages in thread.
    pub message_count: usize,
    /// Most recent message timestamp.
    pub last_message_at: String,
    /// Preview of last message (first 100 chars).
    pub last_message_preview: String,
}
