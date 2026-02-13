//! Semantic navigation using SemanticNavigator experimental feature.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NavigateSemanticGraphRequest {
    pub start_entity_id: String,
    pub end_entity_id: String,
    pub entity_type: String,     // "task" | "knowledge" | "agent"
    pub vector_property: String, // Property name containing vector embeddings
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NavigateSemanticGraphResponse {
    pub path: Vec<String>, // Vec of entity IDs
    pub path_length: usize,
    pub total_cost: f32, // Sum of semantic costs (1.0 - similarity)
    pub description: String,
}

fn default_limit() -> usize {
    100
}
