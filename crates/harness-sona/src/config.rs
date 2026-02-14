//! SONA configuration for runtime toggles and parameter tuning.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::ewc::EwcConfig;
use crate::types::AggregationStrategy;

/// Top-level SONA configuration for the hive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SonaConfig {
    /// Whether SONA features are enabled.
    pub enabled: bool,
    /// Configuration for EWC++ (catastrophic forgetting prevention).
    pub ewc: EwcConfig,
    /// Configuration for BaseLoRA (collective learning).
    pub base_lora: BaseLoRAConfig,
    /// Configuration for the background learning loop.
    pub learning_loop: LearningLoopConfig,
}

/// Configuration for BaseLoRA collective learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseLoRAConfig {
    /// Rank of the BaseLoRA (dimension of adaptation).
    pub rank: usize,
    /// Scaling factor for LoRA (alpha).
    pub alpha: f32,
    /// Aggregation strategy for merging agent deltas.
    pub aggregation_strategy: AggregationStrategy,
    /// Minimum number of agents required for aggregation.
    pub min_participants: usize,
    /// Age threshold for stale contributions (seconds).
    pub staleness_threshold_secs: u64,
}

/// Configuration for the background learning loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningLoopConfig {
    /// How often the learning loop runs (seconds).
    pub interval_secs: u64,
}

impl Default for SonaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ewc: EwcConfig::default(),
            base_lora: BaseLoRAConfig::default(),
            learning_loop: LearningLoopConfig::default(),
        }
    }
}

impl Default for EwcConfig {
    fn default() -> Self {
        Self {
            lambda: 0.4,
            gamma: 0.9,
            max_tasks: 10,
            normalize_fisher: true,
        }
    }
}

impl Default for BaseLoRAConfig {
    fn default() -> Self {
        Self {
            rank: 8,
            alpha: 16.0,
            aggregation_strategy: AggregationStrategy::WeightedAverage,
            min_participants: 1,
            staleness_threshold_secs: 3600, // 1 hour
        }
    }
}

impl Default for LearningLoopConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300, // 5 minutes
        }
    }
}

impl SonaConfig {
    /// Create a new SONA configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a disabled SONA configuration.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Builder: Set enabled flag.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Builder: Set EWC lambda (penalty strength).
    pub fn with_ewc_lambda(mut self, lambda: f32) -> Self {
        self.ewc.lambda = lambda;
        self
    }

    /// Builder: Set EWC gamma (online decay factor).
    pub fn with_ewc_gamma(mut self, gamma: f32) -> Self {
        self.ewc.gamma = gamma;
        self
    }

    /// Builder: Set BaseLoRA rank.
    pub fn with_lora_rank(mut self, rank: usize) -> Self {
        self.base_lora.rank = rank;
        self
    }

    /// Builder: Set learning loop interval.
    pub fn with_learning_interval(mut self, secs: u64) -> Self {
        self.learning_loop.interval_secs = secs;
        self
    }

    /// Get the learning loop interval as a Duration.
    pub fn learning_interval(&self) -> Duration {
        Duration::from_secs(self.learning_loop.interval_secs)
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            // If SONA is disabled, no need to validate parameters
            return Ok(());
        }

        // Validate EWC config
        self.ewc.validate().map_err(|e| e.to_string())?;

        // Validate BaseLoRA config
        if self.base_lora.rank == 0 {
            return Err("BaseLoRA rank must be > 0".into());
        }
        if self.base_lora.alpha <= 0.0 {
            return Err("BaseLoRA alpha must be positive".into());
        }
        if self.base_lora.min_participants == 0 {
            return Err("min_participants must be > 0".into());
        }

        // Validate learning loop config
        if self.learning_loop.interval_secs == 0 {
            return Err("learning interval must be > 0".into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SonaConfig::default();
        assert!(config.enabled);
        assert_eq!(config.ewc.lambda, 0.4);
        assert_eq!(config.ewc.gamma, 0.9);
        assert_eq!(config.base_lora.rank, 8);
        assert_eq!(config.learning_loop.interval_secs, 300);
    }

    #[test]
    fn test_disabled_config() {
        let config = SonaConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_builder_pattern() {
        let config = SonaConfig::new()
            .with_enabled(false)
            .with_ewc_lambda(1.0)
            .with_ewc_gamma(0.95)
            .with_lora_rank(16)
            .with_learning_interval(600);

        assert!(!config.enabled);
        assert_eq!(config.ewc.lambda, 1.0);
        assert_eq!(config.ewc.gamma, 0.95);
        assert_eq!(config.base_lora.rank, 16);
        assert_eq!(config.learning_loop.interval_secs, 600);
    }

    #[test]
    fn test_validation_success() {
        let config = SonaConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_disabled_always_valid() {
        let config = SonaConfig::disabled();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_ewc_lambda_negative() {
        let config = SonaConfig::new().with_ewc_lambda(-1.0);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_ewc_gamma_out_of_range() {
        let config = SonaConfig::new().with_ewc_gamma(1.5);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_lora_rank_zero() {
        let config = SonaConfig::new().with_lora_rank(0);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_learning_interval_zero() {
        let config = SonaConfig::new().with_learning_interval(0);
        assert!(config.validate().is_err());
    }
}
