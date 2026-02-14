//! SonaEngine - top-level orchestrator for SONA adaptive learning.

use std::collections::HashMap;
use std::sync::Arc;

use harness_persistence::AgentId;
use tokio::sync::RwLock;

use crate::error::{SonaError, SonaResult};
use crate::ewc::{EwcConfig, EwcConsolidator};

/// The SONA engine provides adaptive learning capabilities for harness agents.
/// Manages per-agent EWC consolidators and other learning components.
pub struct SonaEngine {
    ewc_config: Option<EwcConfig>,
    /// Per-agent EWC consolidators.
    agent_ewc: Arc<RwLock<HashMap<AgentId, EwcConsolidator>>>,
    /// Whether to automatically consolidate on task completion.
    auto_consolidate: bool,
}

impl SonaEngine {
    /// Create a builder for configuring the engine.
    pub fn builder() -> SonaEngineBuilder {
        SonaEngineBuilder::default()
    }

    /// Handle task completion event: consolidate the agent's EWC state if auto-consolidation is enabled.
    ///
    /// This is called automatically by MCP handlers when a task status changes to "completed"
    /// if `auto_consolidate` is true. Can also be called manually for explicit consolidation.
    pub async fn on_task_complete(
        &self,
        agent_id: AgentId,
        task_id: &str,
        weights: &[f32],
        gradients: &[Vec<f32>],
    ) -> SonaResult<()> {
        let config = self
            .ewc_config
            .ok_or_else(|| SonaError::NotConfigured("EWC not enabled".into()))?;

        let mut map = self.agent_ewc.write().await;
        let consolidator = map
            .entry(agent_id)
            .or_insert_with(|| EwcConsolidator::new(config));

        consolidator.consolidate_task(task_id, weights, gradients)?;
        Ok(())
    }

    /// Check if auto-consolidation is enabled.
    pub fn auto_consolidate_enabled(&self) -> bool {
        self.auto_consolidate
    }

    /// Get a clone of an agent's EWC consolidator state.
    pub async fn agent_ewc_state(&self, agent_id: AgentId) -> Option<EwcConsolidator> {
        let map = self.agent_ewc.read().await;
        map.get(&agent_id).cloned()
    }

    /// Extract synthetic gradients from trajectory steps.
    ///
    /// For each trajectory step, creates a synthetic gradient vector based on:
    /// - Step success/failure (positive/negative signal)
    /// - Step kind (different weight patterns for different actions)
    /// - Hash of payload content (deterministic variation)
    ///
    /// This provides a rough approximation of "importance" for use in FIM computation.
    pub fn extract_gradients_from_trajectory(
        &self,
        steps: &[harness_persistence::TrajectoryStep],
        dim: usize,
    ) -> Vec<Vec<f32>> {
        steps
            .iter()
            .map(|step| {
                // Base gradient magnitude from step kind
                let base_magnitude = match step.kind() {
                    "task_outcome" => 1.0,
                    "knowledge_acquired" => 0.7,
                    "project_consolidation" => 1.2,
                    _ => 0.5,
                };

                // Sign from success flag (if available in payload)
                let sign = step
                    .payload()
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .map(|s| if s { 1.0 } else { -0.5 })
                    .unwrap_or(1.0);

                // Hash payload for deterministic variation across dimensions
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                use std::hash::{Hash, Hasher};
                format!("{:?}", step.payload()).hash(&mut hasher);
                let hash = hasher.finish();

                // Generate gradient vector with variation
                (0..dim)
                    .map(|i| {
                        let seed = hash.wrapping_add(i as u64);
                        let normalized = (seed % 1000) as f32 / 1000.0; // [0, 1)
                        base_magnitude * sign * (normalized - 0.5) * 2.0 // [-magnitude, magnitude]
                    })
                    .collect()
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// SonaEngineBuilder (public for external configuration)
// ---------------------------------------------------------------------------

/// Builder for configuring and constructing a SonaEngine.
#[derive(Default)]
pub struct SonaEngineBuilder {
    ewc_config: Option<EwcConfig>,
    auto_consolidate: bool,
}

impl SonaEngineBuilder {
    /// Enable EWC++ with the given configuration.
    pub fn with_ewc(mut self, config: EwcConfig) -> Self {
        self.ewc_config = Some(config);
        self
    }

    /// Enable automatic consolidation on task completion.
    ///
    /// When enabled, the engine will automatically call `on_task_complete()`
    /// when MCP handlers detect a task status change to "completed".
    ///
    /// Default: false (manual consolidation only).
    pub fn with_auto_consolidate(mut self, enabled: bool) -> Self {
        self.auto_consolidate = enabled;
        self
    }

    /// Build the SonaEngine.
    pub fn build(self) -> SonaResult<SonaEngine> {
        if let Some(ref config) = self.ewc_config {
            config.validate()?;
        }

        Ok(SonaEngine {
            ewc_config: self.ewc_config,
            agent_ewc: Arc::new(RwLock::new(HashMap::new())),
            auto_consolidate: self.auto_consolidate,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ewc::EwcConfig;

    #[test]
    fn test_builder_auto_consolidate_flag() {
        let config = EwcConfig {
            lambda: 0.5,
            gamma: 0.9,
            max_tasks: 10,
            normalize_fisher: true,
        };

        // Default: auto-consolidate disabled
        let engine = SonaEngine::builder()
            .with_ewc(config)
            .build()
            .unwrap();
        assert!(!engine.auto_consolidate_enabled());

        // Explicitly enabled
        let engine = SonaEngine::builder()
            .with_ewc(config)
            .with_auto_consolidate(true)
            .build()
            .unwrap();
        assert!(engine.auto_consolidate_enabled());

        // Explicitly disabled
        let engine = SonaEngine::builder()
            .with_ewc(config)
            .with_auto_consolidate(false)
            .build()
            .unwrap();
        assert!(!engine.auto_consolidate_enabled());
    }

    #[tokio::test]
    async fn test_on_task_complete_consolidation() {
        let config = EwcConfig {
            lambda: 0.5,
            gamma: 0.9,
            max_tasks: 10,
            normalize_fisher: true,
        };

        let engine = SonaEngine::builder()
            .with_ewc(config)
            .with_auto_consolidate(true)
            .build()
            .unwrap();

        let agent_id = AgentId::new();
        let task_id = "test-task-1";
        let weights = vec![0.1, 0.2, 0.3, 0.4];
        let gradients = vec![
            vec![1.0, 0.5, -0.5, 0.2],
            vec![0.8, 0.3, -0.3, 0.1],
        ];

        // Consolidate task
        engine
            .on_task_complete(agent_id, task_id, &weights, &gradients)
            .await
            .unwrap();

        // Verify consolidator exists
        let state = engine.agent_ewc_state(agent_id).await;
        assert!(state.is_some());

        let consolidator = state.unwrap();
        assert_eq!(consolidator.task_count(), 1);
        assert!(consolidator.get_task_snapshot(task_id).is_some());
    }

    #[test]
    fn test_extract_gradients_from_trajectory() {
        use harness_persistence::TrajectoryStep;
        use serde_json::json;

        let config = EwcConfig {
            lambda: 0.5,
            gamma: 0.9,
            max_tasks: 5,
            normalize_fisher: true,
        };

        let engine = SonaEngine::builder()
            .with_ewc(config)
            .build()
            .unwrap();

        // Create mock trajectory steps (using public constructor if available)
        // For now, we'll test with empty steps to verify structure
        let steps: Vec<TrajectoryStep> = vec![];
        let dim = 64;

        let gradients = engine.extract_gradients_from_trajectory(&steps, dim);
        assert_eq!(gradients.len(), 0);

        // In a real test, we'd create actual TrajectoryStep instances
        // and verify the gradient extraction logic
    }
}
