//! SONA integration tool request/response types.
//!
//! Four MCP tools for the SONA learning pipeline:
//! - get_task_trajectory: Retrieve trajectory steps for a task
//! - query_reasoning_bank: Search learned patterns
//! - get_learning_status: Check learning loop status
//! - trigger_learning_cycle: Force a learning cycle

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// get_task_trajectory
// ---------------------------------------------------------------------------

/// Request to get trajectory steps for a task.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTaskTrajectoryRequest {
    pub task_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// A single trajectory step in the response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrajectoryStepResponse {
    pub kind: String,
    pub agent_id: String,
    pub payload: serde_json::Map<String, serde_json::Value>,
}

/// Response from get_task_trajectory.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetTaskTrajectoryResponse {
    pub task_id: String,
    pub steps: Vec<TrajectoryStepResponse>,
    pub event_count: usize,
}

// ---------------------------------------------------------------------------
// query_reasoning_bank
// ---------------------------------------------------------------------------

/// Request to query learned patterns from the reasoning bank.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryReasoningBankRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

fn default_limit() -> usize {
    10
}

/// A single pattern in the query response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PatternResponse {
    pub id: String,
    pub task_type: String,
    pub agent_role: String,
    pub success: bool,
    pub content: String,
    pub confidence: f32,
    pub source_trajectory_id: Option<String>,
}

/// Response from query_reasoning_bank.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryReasoningBankResponse {
    pub patterns: Vec<PatternResponse>,
    pub total_patterns: usize,
}

// ---------------------------------------------------------------------------
// get_learning_status
// ---------------------------------------------------------------------------

/// Request to get learning loop status.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetLearningStatusRequest {
    pub loop_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from get_learning_status.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetLearningStatusResponse {
    pub loop_type: String,
    pub enabled: bool,
    pub patterns_learned: u64,
    pub total_events: u64,
}

// ---------------------------------------------------------------------------
// trigger_learning_cycle
// ---------------------------------------------------------------------------

/// Request to trigger a learning cycle.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TriggerLearningCycleRequest {
    pub loop_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// A cross-agent pattern discovered during coordination learning.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CrossAgentPattern {
    pub description: String,
    pub agents_involved: Vec<String>,
    pub confidence: f32,
}

/// Request payload for distilling trajectories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillTrajectoriesRequest {
    /// Optional limit on the number of trajectory events to consider.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Optional agent ID.
    pub _agent_id: Option<String>,
}

/// Response payload for distilling trajectories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillTrajectoriesResponse {
    /// The distilled dataset as a string (JSONL format).
    pub dataset: String,
    /// How many events were successfully distilled.
    pub distilled_count: usize,
}

/// Response from trigger_learning_cycle.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TriggerLearningCycleResponse {
    pub loop_type: String,
    pub optimizations_applied: u64,
    pub patterns_created: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_agent_patterns: Option<Vec<CrossAgentPattern>>,
}
