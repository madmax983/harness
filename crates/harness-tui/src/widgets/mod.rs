//! Dashboard widgets for the Hive Mind TUI.

mod agents;
mod header;
mod input;
mod knowledge;
mod outputs;
mod tasks;
mod tools;

pub use agents::render_agents;
pub use header::render_header;
pub use input::render_input;
pub use knowledge::render_knowledge;
pub use outputs::render_tool_outputs;
pub use tasks::render_tasks;
pub use tools::render_tools;
