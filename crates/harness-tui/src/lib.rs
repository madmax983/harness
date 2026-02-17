//! TUI interface for Harness v2 - Hive Mind dashboard.

mod app;
mod mcp;
mod parsing;
mod runner;
mod widgets;

pub use app::{AppMode, AppState, DashboardView};
pub use mcp::{McpToolClient, ToolCallOutput};
pub use parsing::{ParsedKnowledgeContent, parse_knowledge_content};
pub use runner::TuiRunner;
