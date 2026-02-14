//! MicroLoRA tool request/response types for per-agent learning.

use serde::{Deserialize, Serialize};

/// A single step in a trajectory: action taken in context with outcome and reward.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrajectoryStep {
    pub action: String,
    pub context: String,
    pub outcome: String,
    pub reward: f64,
}

/// Request to record an agent's trajectory.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecordAgentTrajectoryRequest {
    pub agent_id: String,
    pub trajectory: Vec<TrajectoryStep>,
}

/// Response from record_agent_trajectory.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecordAgentTrajectoryResponse {
    pub trajectory_id: String,
    pub steps_recorded: u64,
}

/// Request to get an agent's LoRA state.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetAgentLoraStateRequest {
    pub agent_id: String,
}

/// Response from get_agent_lora_state.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetAgentLoraStateResponse {
    pub agent_id: String,
    pub trajectories_ingested: u64,
    pub total_steps: u64,
    pub mean_reward: f64,
}

/// Request to apply learned optimizations for action ranking.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApplyAgentOptimizationRequest {
    pub agent_id: String,
    pub context: String,
    pub candidate_actions: Vec<String>,
}

/// A ranked action with confidence score.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RankedAction {
    pub action: String,
    pub confidence: f64,
}

/// Response from apply_agent_optimization.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApplyAgentOptimizationResponse {
    pub ranked_actions: Vec<RankedAction>,
}

/// Request to persist an agent's LoRA state.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PersistAgentLoraRequest {
    pub agent_id: String,
}

/// Response from persist_agent_lora.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PersistAgentLoraResponse {
    pub persisted: bool,
}

/// Request to restore an agent's LoRA state.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RestoreAgentLoraRequest {
    pub agent_id: String,
}

/// Response from restore_agent_lora.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RestoreAgentLoraResponse {
    pub restored: bool,
}
