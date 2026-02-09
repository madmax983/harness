//! MCP server for Harness v2 - Hive Mind.

mod handler;
pub mod server;
mod state;
pub mod tools;

pub use handler::{HandlerError, HandlerResult, HiveHandler};
pub use server::{HiveMcpServer, start_mcp_server, tool_definitions};
pub use state::HiveState;
