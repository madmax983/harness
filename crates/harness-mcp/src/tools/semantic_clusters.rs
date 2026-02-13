//! Semantic clustering using Cartographer experimental feature.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscoverSemanticClustersRequest {
    pub entity_type: String,     // "task" | "knowledge" | "agent"
    pub vector_property: String, // Property name containing vector embeddings
    pub num_clusters: usize,
    pub reify: bool, // Whether to create Region nodes in the graph
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscoverSemanticClustersResponse {
    pub clusters: Vec<ClusterInfo>,
    pub reified_regions: Option<Vec<String>>, // Region node IDs if reify=true
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterInfo {
    pub cluster_id: usize,
    pub centroid: Vec<f32>,
    pub member_ids: Vec<String>, // Entity IDs
    pub size: usize,
}
