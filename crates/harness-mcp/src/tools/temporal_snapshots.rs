//! Temporal snapshot comparison using TemporalDiff experimental feature.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CompareTemporalSnapshotsRequest {
    pub t1: String, // RFC3339 timestamp
    pub t2: String, // RFC3339 timestamp
    #[serde(default)]
    pub limit: Option<usize>, // Max entities to process
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CompareTemporalSnapshotsResponse {
    pub t1: String,
    pub t2: String,
    pub changes: Vec<EntityChange>,
    pub summary: ChangeSummary,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EntityChange {
    pub entity_id: String,
    pub entity_type: String, // "node" | "edge"
    pub change_type: String, // "added" | "removed" | "modified"
    pub property_diff: Option<PropertyDiff>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PropertyDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: HashMap<String, (String, String)>, // key -> (old_value, new_value)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChangeSummary {
    pub total_changes: usize,
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
}
