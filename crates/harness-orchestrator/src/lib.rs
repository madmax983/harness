//! Orchestrator for managing Claude agent processes.

mod config;
mod process;
pub mod prompts;

pub use config::{McpServerConfig, McpTransport, OrchestratorConfig};
pub use process::{ProcessError, ProcessManager, ProcessResult};
