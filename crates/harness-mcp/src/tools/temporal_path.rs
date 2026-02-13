//! Temporal pathfinding using Chronos experimental feature.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FindTemporalPathRequest {
    pub start_entity_id: String,
    pub end_entity_id: String,
    pub entity_type: String, // "task" | "knowledge" | "agent"
    pub valid_time: String,  // RFC3339
    pub tx_time: String,     // RFC3339
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FindTemporalPathResponse {
    pub path: Option<Vec<String>>, // Vec of entity IDs
    pub path_length: usize,
    pub valid_time: String,
    pub tx_time: String,
    pub description: String,
}
