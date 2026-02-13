//! Link prediction using Prophet experimental feature.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PredictMissingConnectionsRequest {
    pub entity_type: String, // "task" | "knowledge" | "agent"
    pub entity_id: String,   // UUID of the entity to predict links for
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_name: Option<String>, // Optional vector property for semantic scoring
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PredictMissingConnectionsResponse {
    pub predictions: Vec<LinkPrediction>,
    pub source_entity_id: String,
    pub source_entity_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LinkPrediction {
    pub target_entity_id: String,
    pub target_entity_type: String,
    pub score: f32,
    pub reason: String, // Description of why this link is predicted
}

fn default_limit() -> usize {
    10
}
