//! Configuration for the orchestrator.

use serde::{Deserialize, Serialize};

/// Configuration for spawning agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    /// Maximum number of concurrent agents.
    pub population_cap: usize,
    /// Path to the claude CLI executable.
    pub claude_path: String,
    /// MCP server configuration to pass to agents.
    pub mcp_config: McpServerConfig,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            population_cap: 8,
            claude_path: "claude".into(),
            mcp_config: McpServerConfig::default(),
        }
    }
}

/// MCP server configuration passed to Claude agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Server name (used in MCP config JSON).
    pub name: String,
    /// Command to run the MCP server.
    pub command: String,
    /// Arguments for the command.
    pub args: Vec<String>,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            name: "harness".into(),
            command: "harness".into(),
            args: vec!["mcp-server".into()],
        }
    }
}

impl McpServerConfig {
    /// Generate the JSON config string for --mcp-config.
    pub fn to_json(&self) -> String {
        serde_json::json!({
            &self.name: {
                "command": &self.command,
                "args": &self.args
            }
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_config_json_format() {
        let config = McpServerConfig {
            name: "harness".into(),
            command: "harness-mcp".into(),
            args: vec!["--session".into(), "abc123".into()],
        };
        let json = config.to_json();
        assert!(json.contains("\"harness\""));
        assert!(json.contains("\"command\""));
        assert!(json.contains("harness-mcp"));
    }
}
