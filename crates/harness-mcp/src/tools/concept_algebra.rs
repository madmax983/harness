//! Concept algebra: vector arithmetic for semantic reasoning.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ComputeConceptAnalogyRequest {
    /// ID of the first concept node (A in "A - B + C")
    pub concept_a_id: String,
    /// ID of the concept to subtract (B in "A - B + C")
    pub concept_b_id: String,
    /// ID of the concept to add (C in "A - B + C")
    pub concept_c_id: String,
    /// Number of results to return
    #[serde(default = "default_k")]
    pub k: usize,
    /// Optional property name for vector lookup (auto-detected if omitted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ComputeConceptAnalogyResponse {
    pub results: Vec<ConceptAnalogyResult>,
    pub analogy_description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConceptAnalogyResult {
    /// ID of the result node (knowledge, task, etc.)
    pub entity_id: String,
    /// Similarity score (higher = more similar)
    pub score: f32,
    /// Optional label/name for display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

fn default_k() -> usize {
    5
}
