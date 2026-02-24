//! Oracle: Predictive analysis for task estimation and role recommendation.
//!
//! "The Oracle" uses the ReasoningBank to find similar past successful tasks
//! and predicts the best agent role, estimated complexity, and potential risks
//! for a new task.

use std::collections::HashMap;

use harness_persistence::{AgentRole, PatternQuery, ReasoningBank};
use serde::{Deserialize, Serialize};

/// Complexity estimation for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Complexity {
    Low,
    Medium,
    High,
    Unknown,
}

/// Prediction result from the Oracle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    /// The recommended agent role for this task.
    pub suggested_role: AgentRole,
    /// Confidence score of the prediction (0.0 to 1.0).
    pub confidence: f32,
    /// Estimated complexity of the task.
    pub estimated_complexity: Complexity,
    /// Descriptions of similar past patterns used for this prediction.
    pub similar_patterns: Vec<String>,
}

/// The Oracle service for predictive task analysis.
pub struct Oracle {
    confidence_threshold: f32,
}

impl Oracle {
    /// Create a new Oracle with default settings.
    pub fn new() -> Self {
        Self {
            confidence_threshold: 0.1,
        }
    }

    /// Set the minimum confidence threshold for predictions.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.confidence_threshold = threshold;
        self
    }

    /// Predict task requirements based on description.
    pub async fn predict(
        &self,
        bank: &ReasoningBank,
        task_description: &str,
    ) -> Result<Prediction, anyhow::Error> {
        let query = PatternQuery::new(task_description)
            .with_limit(5)
            .with_success_only(true);

        let results = bank.find_similar(query).await?;

        // Filter by threshold
        let relevant_results: Vec<_> = results
            .into_iter()
            .filter(|r| r.similarity >= self.confidence_threshold)
            .collect();

        if relevant_results.is_empty() {
            return Ok(Prediction {
                suggested_role: AgentRole::Developer, // Default fallback
                confidence: 0.0,
                estimated_complexity: Complexity::Unknown,
                similar_patterns: vec![],
            });
        }

        // Aggregate roles
        let mut role_counts = HashMap::new();
        for result in &relevant_results {
            *role_counts.entry(result.pattern.agent_role()).or_insert(0) += 1;
        }

        // Find the most common role
        let best_role = role_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(role, _)| role)
            .unwrap_or(AgentRole::Developer);

        // Estimate complexity based on pattern description length
        // Heuristic: Longer descriptions often imply more complex tasks/reasoning
        let avg_len: usize = relevant_results
            .iter()
            .map(|r| r.pattern.description().len())
            .sum::<usize>()
            / relevant_results.len();

        let complexity = if avg_len > 500 {
            Complexity::High
        } else if avg_len > 200 {
            Complexity::Medium
        } else {
            Complexity::Low
        };

        // Use the similarity of the top result as the confidence score
        let confidence = relevant_results[0].similarity;

        Ok(Prediction {
            suggested_role: best_role,
            confidence,
            estimated_complexity: complexity,
            similar_patterns: relevant_results
                .iter()
                .map(|r| r.pattern.description().to_string())
                .collect(),
        })
    }
}

impl Default for Oracle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_persistence::{PatternStore, SessionId, TaskPattern};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_oracle_prediction() {
        let store = Arc::new(PatternStore::new());
        let session_id = SessionId::new();
        let bank = ReasoningBank::new(store, session_id);

        // Seed the bank with some patterns
        let patterns = vec![
            TaskPattern::new(
                "implement_feature",
                AgentRole::Developer,
                true,
                "Implemented OAuth2 authentication flow with Google provider, handling redirects and token exchange.",
            ),
            TaskPattern::new(
                "design_schema",
                AgentRole::Architect,
                true,
                "Designed database schema for user permissions and roles, ensuring referential integrity.",
            ),
        ];

        for p in patterns {
            bank.store_pattern(p).await.unwrap();
        }

        let oracle = Oracle::new().with_threshold(0.01); // Low threshold for mock embeddings

        // Predict for a developer task
        let _pred = oracle
            .predict(&bank, "Need to add login with GitHub")
            .await
            .unwrap();

        // Note: With mock embeddings (if any) or simple TF-IDF, similarity might be low.
        // We just check that we get a result.
        // If similarity is too low (empty results), default is Developer/Unknown/0.0

        // Since we can't guarantee high similarity with the simple embedding in `PatternStore` without
        // matching words, let's assume "login" and "authentication" might not match if not shared words.
        // But "OAuth2" and "Google" might not match "GitHub".
        // Let's try to match words that exist in the stored pattern.

        let pred_match = oracle
            .predict(&bank, "Implemented OAuth2 authentication")
            .await
            .unwrap();

        assert_eq!(pred_match.suggested_role, AgentRole::Developer);
        // Complexity might be Medium or High depending on length.
        // The description is ~90 chars, so Low (<200).
        assert_eq!(pred_match.estimated_complexity, Complexity::Low);
        assert!(pred_match.confidence > 0.0);
        assert!(!pred_match.similar_patterns.is_empty());
    }

    #[tokio::test]
    async fn test_oracle_no_match() {
        let store = Arc::new(PatternStore::new());
        let session_id = SessionId::new();
        let bank = ReasoningBank::new(store, session_id);
        let oracle = Oracle::new();

        let pred = oracle
            .predict(&bank, "Something completely unrelated to anything stored")
            .await
            .unwrap();

        assert_eq!(pred.confidence, 0.0);
        assert_eq!(pred.estimated_complexity, Complexity::Unknown);
        assert!(pred.similar_patterns.is_empty());
    }
}
