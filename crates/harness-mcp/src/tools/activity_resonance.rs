//! Activity resonance finding using Echo experimental feature.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FindActivityResonanceRequest {
    pub target_entity_id: String,
    pub entity_type: String, // "task" | "knowledge" | "agent"
    #[serde(default = "default_window_seconds")]
    pub window_seconds: u64,
    #[serde(default = "default_num_bins")]
    pub num_bins: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_min_similarity")]
    pub min_similarity: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FindActivityResonanceResponse {
    pub resonant_entities: Vec<ResonantEntity>,
    pub target_fingerprint: TemporalFingerprint,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResonantEntity {
    pub entity_id: String,
    pub similarity_score: f32, // 0.0 to 1.0
    pub fingerprint: TemporalFingerprint,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemporalFingerprint {
    pub bins: Vec<f32>,
    pub resolution_us: i64,
}

fn default_window_seconds() -> u64 {
    3600 // 1 hour
}

fn default_num_bins() -> usize {
    60
}

fn default_limit() -> usize {
    10
}

fn default_min_similarity() -> f32 {
    0.1
}
