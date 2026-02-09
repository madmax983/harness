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
///
/// Supports two transports:
/// - **Stdio** (default): Spawns the MCP server as a subprocess per agent.
/// - **HTTP/SSE**: Agents connect to a shared HTTP MCP server via URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Server name (used in MCP config JSON).
    pub name: String,
    /// Transport configuration.
    pub transport: McpTransport,
}

/// MCP transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpTransport {
    /// Stdio transport: spawn command per agent.
    Stdio {
        /// Command to run the MCP server.
        command: String,
        /// Arguments for the command.
        args: Vec<String>,
    },
    /// HTTP/SSE transport: agents connect to a shared server via URL.
    HttpSse {
        /// URL of the MCP server (e.g., "http://localhost:3000/sse").
        url: String,
    },
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            name: "harness".into(),
            transport: McpTransport::HttpSse {
                url: "http://localhost:3000/sse".into(),
            },
        }
    }
}

impl McpServerConfig {
    /// Create a new HTTP/SSE MCP config pointing to a URL.
    pub fn http_sse(url: impl Into<String>) -> Self {
        Self {
            name: "harness".into(),
            transport: McpTransport::HttpSse { url: url.into() },
        }
    }

    /// Create a new stdio MCP config with a command.
    pub fn stdio(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            name: "harness".into(),
            transport: McpTransport::Stdio {
                command: command.into(),
                args,
            },
        }
    }

    /// Generate the JSON config string for `claude --mcp-config`.
    pub fn to_json(&self) -> String {
        match &self.transport {
            McpTransport::Stdio { command, args } => serde_json::json!({
                &self.name: {
                    "command": command,
                    "args": args
                }
            })
            .to_string(),
            McpTransport::HttpSse { url } => serde_json::json!({
                &self.name: {
                    "url": url
                }
            })
            .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_config_stdio_json() {
        let config =
            McpServerConfig::stdio("harness-mcp", vec!["--session".into(), "abc123".into()]);
        let json = config.to_json();
        assert!(json.contains("\"harness\""));
        assert!(json.contains("\"command\""));
        assert!(json.contains("harness-mcp"));
        assert!(!json.contains("\"url\""));
    }

    #[test]
    fn mcp_config_http_sse_json() {
        let config = McpServerConfig::http_sse("http://localhost:3000/sse");
        let json = config.to_json();
        assert!(json.contains("\"harness\""));
        assert!(json.contains("\"url\""));
        assert!(json.contains("http://localhost:3000/sse"));
        assert!(!json.contains("\"command\""));
    }

    #[test]
    fn default_config_uses_http_sse() {
        let config = McpServerConfig::default();
        assert!(matches!(config.transport, McpTransport::HttpSse { .. }));
        let json = config.to_json();
        assert!(json.contains("\"url\""));
    }
}
