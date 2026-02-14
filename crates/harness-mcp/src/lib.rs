//! MCP server for Harness v2 - Hive Mind.

pub mod agent_runtime;
mod handler;
pub mod prompt_generator;
pub mod server;
mod state;
pub mod tools;

pub use agent_runtime::{AgentRuntime, RuntimeError, RuntimeResult};
pub use handler::{HandlerError, HandlerResult, HiveHandler};
pub use prompt_generator::{generate_agent_system_prompt, generate_strategoi_directive_prompt};
pub use server::{HiveMcpServer, start_mcp_server, tool_definitions};
pub use state::{AgentLoraData, HiveState, StoredTrajectory, TrajectoryRecord};
