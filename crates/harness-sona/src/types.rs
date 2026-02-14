//! Core types for SONA collective learning.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use harness_persistence::AgentId;
use serde::{Deserialize, Serialize};

/// Strategy for aggregating LoRA deltas from multiple agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregationStrategy {
    /// Simple average: each delta contributes equally.
    FederatedAverage,
    /// Weighted by task_count * success_rate (and optionally recency).
    WeightedAverage,
}

/// A LoRA weight delta from an individual agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRADelta {
    /// The agent that generated this delta.
    pub agent_id: AgentId,
    /// Knowledge domain (e.g. "authentication", "database").
    pub domain: String,
    /// Weight values for this delta.
    pub weights: Vec<f32>,
    /// Number of tasks completed that contributed to this delta.
    pub task_count: u32,
    /// Success rate of the agent's tasks (0.0 - 1.0).
    pub success_rate: f64,
    /// When this delta was created.
    pub timestamp: DateTime<Utc>,
}

impl LoRADelta {
    /// Create a synthetic delta for testing (no real agent_id).
    pub fn synthetic(domain: &str, weights: Vec<f32>, task_count: u32, success_rate: f64) -> Self {
        Self {
            agent_id: AgentId::new(),
            domain: domain.to_string(),
            weights,
            task_count,
            success_rate,
            timestamp: Utc::now(),
        }
    }
}

/// Aggregated weights from a BaseLoRA aggregation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedWeights {
    /// The aggregated weight values.
    pub weights: Vec<f32>,
    /// Which domain these weights cover (None = all domains combined).
    pub domain: Option<String>,
    /// How many deltas were aggregated.
    pub delta_count: usize,
}

/// Statistics about contributions to a BaseLoRA.
#[derive(Debug, Clone, Default)]
pub struct ContributionStats {
    /// Number of unique contributing agents.
    pub total_contributors: usize,
    /// Total number of deltas received.
    pub total_deltas: usize,
    /// Number of currently active contributors (not disconnected).
    pub active_contributors: usize,
    /// Set of domains with contributions.
    pub domains: HashSet<String>,
}
