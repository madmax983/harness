//! Knowledge tool request/response types.

use serde::{Deserialize, Serialize};

/// Request to share knowledge.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShareKnowledgeRequest {
    pub content: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
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
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
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
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
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

/// Request to auto-cluster knowledge entries by similarity.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KnowledgeClustersRequest {
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f32,
    #[serde(default = "default_min_cluster_size")]
    pub min_cluster_size: usize,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

fn default_similarity_threshold() -> f32 {
    0.7
}

fn default_min_cluster_size() -> usize {
    2
}

/// A single cluster of related knowledge entries.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KnowledgeCluster {
    pub cluster_id: usize,
    pub size: usize,
    pub avg_similarity: f32,
    pub representative_content: String,
    pub members: Vec<KnowledgeResult>,
}

/// Response from knowledge_clusters.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KnowledgeClustersResponse {
    pub clusters: Vec<KnowledgeCluster>,
    pub total_knowledge_count: usize,
    pub clustered_count: usize,
    pub unclustered_count: usize,
}
