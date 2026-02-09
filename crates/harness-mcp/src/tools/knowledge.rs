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
