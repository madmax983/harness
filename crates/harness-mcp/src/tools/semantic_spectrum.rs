//! Semantic spectrum analysis using Prism experimental feature.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnalyzeSemanticSpectrumRequest {
    pub target_id: String, // ID of entity to analyze
    pub axes: Vec<AxisDefinition>,
    #[serde(default)]
    pub vector_property: Option<String>, // Which vector property to use
    #[serde(default)]
    pub orthogonalize: bool, // Apply Gram-Schmidt orthogonalization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AxisDefinition {
    pub name: String,         // Human-readable axis name (e.g., "Technical", "Business")
    pub reference_id: String, // ID of entity whose vector defines this axis
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnalyzeSemanticSpectrumResponse {
    pub spectrum: HashMap<String, f32>, // Axis name -> score
    pub explanation: String,
}
