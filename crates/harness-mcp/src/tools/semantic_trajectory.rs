//! Semantic trajectory prediction using Dreamer experimental feature.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PredictSemanticTrajectoryRequest {
    pub entity_type: String, // "task" | "knowledge" | "agent"
    pub entity_id: String,   // UUID of the entity
    pub property: String,    // Vector property name (e.g., "embedding")
    pub history_window_seconds: u64,
    pub future_horizon_seconds: u64,
    #[serde(default = "default_k")]
    pub k: usize, // Number of neighbors to return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PredictSemanticTrajectoryResponse {
    pub predictions: Vec<TrajectoryPrediction>,
    pub entity_id: String,
    pub entity_type: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrajectoryPrediction {
    pub entity_id: String,
    pub entity_type: String,
    pub similarity_score: f32,
}

fn default_k() -> usize {
    5
}
