//! Chaos Engine tool request/response types.

use serde::{Deserialize, Serialize};

/// Request to inject chaos into the hive.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InjectChaosRequest {
    /// The type of chaos to inject.
    /// Values: "kill_agent", "block_task"
    pub kind: String,

    /// Optional target agent ID (for kill_agent).
    /// If not provided, a random agent will be selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_agent_id: Option<String>,

    /// Optional target task ID (for block_task).
    /// If not provided, a random task will be selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_task_id: Option<String>,

    /// Optional duration in seconds (future use for latency injection).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,

    /// Optional agent ID for authorization (strategoi check).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from inject_chaos.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InjectChaosResponse {
    /// The action that was taken (e.g., "killed_agent").
    pub action_taken: String,

    /// Description of the impact.
    pub description: String,

    /// ID of the affected entity.
    pub affected_entity_id: Option<String>,

    /// The simulated chaos level (0.0-1.0).
    pub chaos_level: f32,
}
