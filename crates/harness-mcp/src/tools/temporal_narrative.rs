//! Temporal narrative generation for entity history.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenerateHistoryNarrativeRequest {
    pub entity_id: String,
    pub entity_type: String, // "task" | "knowledge" | "agent"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenerateHistoryNarrativeResponse {
    pub entity_id: String,
    pub entity_type: String,
    pub events: Vec<NarrativeEvent>,
    pub narrative_summary: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NarrativeEvent {
    pub timestamp: String, // RFC3339
    pub version_number: u64,
    pub description: String,
    pub changes: Vec<String>,
}
