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

/// Request to spawn a new agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnAgentRequest {
    /// Role for the spawned agent (MVP: only "developer" supported).
    pub role: String,
    /// Name for the spawned teammate.
    pub name: String,
    /// Optional task to assign immediately after spawn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_task_id: Option<String>,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from spawn_agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnAgentResponse {
    /// Harness agent ID.
    pub agent_id: String,
    /// Claude Code teammate ID (from Task tool).
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
