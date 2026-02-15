//! SONA (Self-Organizing Neural Architecture) integration for Harness.
//!
//! Provides adaptive learning capabilities for the hive mind:
//! - ReasoningBank: Pattern storage for successful task executions
//! - MicroLoRA: Per-agent adaptation
//! - BaseLoRA: Collective hive learning
//! - EWC++: Catastrophic forgetting prevention
//! - Trajectory: Event-driven learning trigger system

pub mod config;
mod engine;
pub mod ewc;

mod base_lora;
mod contribution;
mod coordinator;
mod error;
mod learning_loop;
mod merit;
mod service;
mod types;

pub use base_lora::BaseLoRA;
pub use config::{BaseLoRAConfig, LearningLoopConfig, SonaConfig};
pub use contribution::ContributionWeight;
pub use coordinator::{
    ConvergenceMetrics, FederatedConfig, FederatedCoordinator, RoundResult, RoundStatus,
};
pub use engine::{SonaEngine, SonaEngineBuilder};
pub use error::SonaError;
pub use ewc::EwcConfig;
pub use learning_loop::LearningLoop;
pub use merit::CollectiveMerit;
pub use service::{
    AgentInheritance, BaseLoRAState, HiveLearningConfig, HiveLearningService, LoopStats,
};
pub use types::{AggregatedWeights, AggregationStrategy, ContributionStats, LoRADelta};
