//! Agent tool request/response types.

use serde::{Deserialize, Serialize};

/// Request to register an agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegisterAgentRequest {
    pub role: String,
}

/// Response from register_agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegisterAgentResponse {
    pub agent_id: String,
}

/// Request to list agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListAgentsRequest {}

/// Agent info in list response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub role: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    pub is_strategoi: bool,
}

/// Response from list_agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListAgentsResponse {
    pub agents: Vec<AgentInfo>,
}

/// Request to get full hive status.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetHiveStatusRequest {}

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
}
