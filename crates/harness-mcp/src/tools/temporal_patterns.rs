//! Temporal pattern matching using Sherlock experimental feature.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FindTemporalPatternsRequest {
    pub entity_type: String, // "task" | "knowledge" | "agent"
    pub pattern_sequence: Vec<PatternClue>,
    pub time_window_seconds: u64,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PatternClue {
    pub property: String, // Property key like "status", "priority"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>, // None = any value
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FindTemporalPatternsResponse {
    pub matches: Vec<PatternMatch>,
    pub pattern_description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PatternMatch {
    pub entity_id: String,
    pub entity_type: String,
    pub events: Vec<PatternEvent>,
    pub time_span_seconds: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PatternEvent {
    pub timestamp: String, // RFC3339
    pub property: String,
    pub value: String,
}

fn default_limit() -> usize {
    10
}
