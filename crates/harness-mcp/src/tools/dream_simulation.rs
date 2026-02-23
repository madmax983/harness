//! Dream simulation tool request/response types.

use serde::{Deserialize, Serialize};

/// Request to run a dream simulation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DreamSimulationRequest {
    /// Optional project ID to focus the dream on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Number of recent events to analyze (default: 50).
    #[serde(default = "default_lookback_limit")]
    pub lookback_limit: usize,
    /// Whether to return detailed patterns (default: false).
    #[serde(default)]
    pub detailed_patterns: bool,
}

fn default_lookback_limit() -> usize {
    50
}

/// A synthesized risk or opportunity.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DreamInsight {
    pub kind: String, // "risk", "opportunity", "pattern"
    pub description: String,
    pub confidence: f32,
    pub related_task_ids: Vec<String>,
}

/// Response from dream_simulation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DreamSimulationResponse {
    pub simulation_id: String,
    pub timestamp: String,
    pub recent_events_count: usize,
    pub active_tasks_count: usize,
    pub patterns_found: usize,
    pub insights: Vec<DreamInsight>,
    pub narrative_forecast: String,
}
