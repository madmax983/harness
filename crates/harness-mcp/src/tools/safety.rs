//! SafetyNet tool request/response types.

use harness_sona::experimental::safety_net::RiskLevel;
use serde::{Deserialize, Serialize};

/// Request to check the safety of an action.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckSafetyRequest {
    /// Description of the action to check.
    pub action: String,
    /// Optional similarity threshold (0.0 - 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f32>,
}

/// Response from safety check.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckSafetyResponse {
    pub risk_level: RiskLevel,
    pub warning: Option<String>,
    pub similar_failures: Vec<String>,
    pub similar_successes: Vec<String>,
    pub confidence: f32,
}
