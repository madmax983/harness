//! Graph layout generation using Kaleidoscope experimental feature.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenerateGraphLayoutRequest {
    pub entity_type: String,             // "task" | "knowledge" | "agent"
    pub vector_property: Option<String>, // Optional vector property for semantic gravity
    #[serde(default = "default_iterations")]
    pub iterations: usize,
    #[serde(default = "default_width")]
    pub width: f32,
    #[serde(default = "default_height")]
    pub height: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenerateGraphLayoutResponse {
    pub positions: Vec<NodePosition>,
    pub bounds: LayoutBounds,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodePosition {
    pub entity_id: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LayoutBounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

fn default_iterations() -> usize {
    100
}

fn default_width() -> f32 {
    1000.0
}

fn default_height() -> f32 {
    1000.0
}
