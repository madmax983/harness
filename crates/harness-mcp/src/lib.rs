//! MCP server for Harness v2 - Hive Mind.

mod handler;
mod state;
pub mod tools;

pub use handler::{HandlerError, HandlerResult, HiveHandler};
pub use state::HiveState;
