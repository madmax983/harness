//! Orchestrator for managing Claude processes.

mod config;
mod process;

pub use config::{McpServerConfig, OrchestratorConfig};
pub use process::{ProcessError, ProcessManager, ProcessResult};
