//! SafetyNet: Real-time risk assessment for agent actions.
//!
//! Uses the ReasoningBank to detect potential failure patterns and warn agents
//! before they execute risky actions.

use harness_persistence::{PatternQuery, ReasoningBank, TaskPattern};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub risk_level: RiskLevel,
    pub warning: Option<String>,
    pub similar_failures: Vec<String>,
    pub similar_successes: Vec<String>,
    pub confidence: f32,
}

pub struct SafetyNet {
    threshold: f32,
}

impl SafetyNet {
    pub fn new() -> Self {
        Self { threshold: 0.1 }
    }

    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    pub async fn assess_risk(
        &self,
        bank: &ReasoningBank,
        action_description: &str,
    ) -> anyhow::Result<RiskAssessment> {
        let query = PatternQuery::new(action_description)
            .with_limit(10)
            .with_success_only(false); // Get both success and failure

        let similars = bank.find_similar(query).await?;

        let mut failures: Vec<TaskPattern> = Vec::new();
        let mut successes: Vec<TaskPattern> = Vec::new();
        let mut max_fail_sim = 0.0f32;
        let mut max_success_sim = 0.0f32;

        for sim in similars {
            if sim.similarity < self.threshold {
                continue;
            }
            if sim.pattern.success() {
                if sim.similarity > max_success_sim {
                    max_success_sim = sim.similarity;
                }
                successes.push(sim.pattern);
            } else {
                if sim.similarity > max_fail_sim {
                    max_fail_sim = sim.similarity;
                }
                failures.push(sim.pattern);
            }
        }

        let risk_level = if !failures.is_empty() && max_fail_sim > 0.8 {
            RiskLevel::Critical
        } else if !failures.is_empty() && max_fail_sim > 0.5 {
            RiskLevel::High
        } else if !failures.is_empty() && max_fail_sim > 0.3 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };

        let warning = if risk_level != RiskLevel::Low {
            Some(format!(
                "Action matches {} known failure patterns (max similarity: {:.2})",
                failures.len(),
                max_fail_sim
            ))
        } else {
            None
        };

        Ok(RiskAssessment {
            risk_level,
            warning,
            similar_failures: failures
                .iter()
                .take(3)
                .map(|p| p.description().to_string())
                .collect(),
            similar_successes: successes
                .iter()
                .take(3)
                .map(|p| p.description().to_string())
                .collect(),
            confidence: max_fail_sim.max(max_success_sim),
        })
    }
}

impl Default for SafetyNet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_persistence::{AgentRole, PatternStore, SessionId, TaskPattern};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_assess_risk_identifies_failure_patterns() {
        let store = Arc::new(PatternStore::new());
        let session_id = SessionId::new();
        let bank = ReasoningBank::new(store, session_id);
        let safety_net = SafetyNet::new().with_threshold(0.01); // Low threshold for mock embeddings

        // Seed a known failure
        let failure = TaskPattern::new(
            "deploy",
            AgentRole::Developer,
            false,
            "Deployed to production without running migration tests, causing downtime.",
        );
        bank.store_pattern(failure).await.unwrap();

        // Seed a known success
        let success = TaskPattern::new(
            "deploy",
            AgentRole::Developer,
            true,
            "Deployed to staging and ran integration tests successfully.",
        );
        bank.store_pattern(success).await.unwrap();

        // Check risky action
        let assessment = safety_net
            .assess_risk(&bank, "Deploy to production without tests")
            .await
            .unwrap();

        // With simple embeddings, "Deploy to production without tests" should match the failure pattern well
        // Note: The mock embedding is simple TF-IDF, so word overlap matters.
        // "Deployed to production without running migration tests" vs "Deploy to production without tests"
        // Overlap: deploy(ed), production, without, tests. High overlap.

        // It should match the failure.
        assert!(!assessment.similar_failures.is_empty());
        // Risk level might be Medium or High depending on exact similarity score
        assert!(
            assessment.risk_level != RiskLevel::Low,
            "Should identify risk"
        );
        assert!(assessment.warning.is_some());
    }

    #[tokio::test]
    async fn test_assess_risk_identifies_safe_patterns() {
        let store = Arc::new(PatternStore::new());
        let session_id = SessionId::new();
        let bank = ReasoningBank::new(store, session_id);
        let safety_net = SafetyNet::new().with_threshold(0.01);

        // Seed a known success
        let success = TaskPattern::new(
            "refactor",
            AgentRole::Developer,
            true,
            "Refactored user module with comprehensive unit tests.",
        );
        bank.store_pattern(success).await.unwrap();

        // Check safe action
        let assessment = safety_net
            .assess_risk(&bank, "Refactor user module with tests")
            .await
            .unwrap();

        // Should match success
        assert!(!assessment.similar_successes.is_empty());
        // Should have no failures (empty DB for failures)
        assert!(assessment.similar_failures.is_empty());
        assert_eq!(assessment.risk_level, RiskLevel::Low);
    }
}
