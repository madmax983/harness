//! Knowledge tool request/response types.

use serde::{Deserialize, Serialize};

/// Request to share knowledge.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShareKnowledgeRequest {
    pub content: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

/// Response from share_knowledge.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShareKnowledgeResponse {
    pub knowledge_id: String,
}

/// Request to search the hive mind.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AskHiveRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

/// A knowledge result with similarity score.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KnowledgeResult {
    pub id: String,
    pub content: String,
    pub kind: String,
    pub author: String,
    pub similarity: f32,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

/// Response from ask_hive.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AskHiveResponse {
    pub results: Vec<KnowledgeResult>,
}

/// Request to fish for related knowledge (associative retrieval).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FishKnowledgeRequest {
    /// ID of the knowledge entry to fish from.
    pub knowledge_id: String,
    #[serde(default = "default_fish_limit")]
    pub limit: usize,
}

fn default_fish_limit() -> usize {
    15
}

/// A knowledge result from fishing with score breakdown.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FishResult {
    pub id: String,
    pub content: String,
    pub kind: String,
    pub author: String,
    pub score: f32,
    /// Vector similarity component (0.0-1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_similarity: Option<f32>,
    /// Graph connection paths that led to this result.
    pub connection_paths: Vec<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

/// Response from fish_knowledge.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FishKnowledgeResponse {
    pub starting_from: String,
    pub results: Vec<FishResult>,
}
