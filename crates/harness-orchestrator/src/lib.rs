//! Orchestrator for managing multi-provider agent CLI processes.

mod config;
mod process;
pub mod prompts;
mod runtime;

pub use config::{McpServerConfig, McpTransport, OrchestratorConfig};
pub use process::{ProcessError, ProcessManager, ProcessResult};
pub use runtime::{AgentRuntime, AgentRuntimeKind, CommandSpec, RuntimeError, build_runtime};
