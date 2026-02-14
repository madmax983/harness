//! Collective merit scoring for the hive.

use serde::{Deserialize, Serialize};

/// Multi-dimensional measure of the hive's collective performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectiveMerit {
    /// Overall composite merit score.
    pub score: f64,
    /// How much of the knowledge space is covered.
    pub knowledge_coverage: f64,
    /// Fraction of tasks completed successfully.
    pub task_success_rate: f64,
    /// Fraction of agents actively participating.
    pub agent_participation: f64,
    /// Shannon entropy of knowledge distribution across domains.
    pub knowledge_diversity: f64,
}

impl CollectiveMerit {
    /// Compute merit from component metrics.
    pub fn compute(
        knowledge_coverage: f64,
        task_success_rate: f64,
        agent_participation: f64,
        knowledge_diversity: f64,
    ) -> Self {
        // Composite score: weighted combination of dimensions
        let score = 0.3 * knowledge_coverage
            + 0.3 * task_success_rate
            + 0.2 * agent_participation
            + 0.2 * knowledge_diversity;

        Self {
            score,
            knowledge_coverage,
            task_success_rate,
            agent_participation,
            knowledge_diversity,
        }
    }
}
