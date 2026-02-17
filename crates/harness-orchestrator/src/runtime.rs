//! Agent runtime adapter layer.
//!
//! Builds provider-specific CLI invocations while keeping process lifecycle
//! management shared in `ProcessManager`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::config::{McpServerConfig, McpTransport};

/// Runtime flavor used to spawn agent CLI processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeKind {
    /// Anthropic Claude CLI-compatible invocation (`-p` + JSON output/tool flags).
    #[default]
    Claude,
    /// OpenAI Codex CLI invocation (`exec`).
    Codex,
    /// Google Gemini CLI invocation (`-p` + `--output-format json` + `--yolo`).
    Gemini,
    /// Fallback for unknown CLIs that are Claude-compatible.
    ClaudeCompatible,
}

impl AgentRuntimeKind {
    /// Infer runtime kind from executable path/name.
    pub fn infer_from_cli(cli_path: &str) -> Self {
        let normalized = std::path::Path::new(cli_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(cli_path)
            .to_ascii_lowercase();

        if normalized.contains("codex") {
            Self::Codex
        } else if normalized.contains("gemini") {
            Self::Gemini
        } else if normalized.contains("claude") {
            Self::Claude
        } else {
            Self::ClaudeCompatible
        }
    }

    /// Parse from CLI flag value.
    pub fn from_flag(value: &str) -> Option<Self> {
        match value {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "gemini" => Some(Self::Gemini),
            "claude_compatible" => Some(Self::ClaudeCompatible),
            _ => None,
        }
    }
}

/// Spawn specification consumed by `ProcessManager`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }
}

/// Runtime adapter error.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// Runtime cannot consume inline stdio transport.
    #[error(
        "{runtime:?} runtime requires HTTP/SSE MCP transport and external MCP registration in the CLI"
    )]
    UnsupportedInlineMcp { runtime: AgentRuntimeKind },
}

/// Runtime adapter trait.
pub trait AgentRuntime: Send + Sync {
    /// Build command invocation for one agent run.
    fn build_command(
        &self,
        cli_path: &str,
        prompt: &str,
        mcp_config: &McpServerConfig,
    ) -> Result<CommandSpec, RuntimeError>;
}

/// Construct runtime adapter for kind.
pub fn build_runtime(kind: AgentRuntimeKind) -> Arc<dyn AgentRuntime> {
    match kind {
        AgentRuntimeKind::Claude => Arc::new(ClaudeRuntime),
        AgentRuntimeKind::Codex => Arc::new(CodexRuntime),
        AgentRuntimeKind::Gemini => Arc::new(GeminiRuntime),
        AgentRuntimeKind::ClaudeCompatible => Arc::new(ClaudeRuntime),
    }
}

struct ClaudeRuntime;

impl AgentRuntime for ClaudeRuntime {
    fn build_command(
        &self,
        cli_path: &str,
        prompt: &str,
        _mcp_config: &McpServerConfig,
    ) -> Result<CommandSpec, RuntimeError> {
        Ok(CommandSpec::new(
            cli_path,
            vec![
                "-p".to_string(),
                prompt.to_string(),
                "--output-format".to_string(),
                "json".to_string(),
                "--allowedTools".to_string(),
                r#""Bash,Read,Edit""#.to_string(),
            ],
        ))
    }
}

struct CodexRuntime;

impl AgentRuntime for CodexRuntime {
    fn build_command(
        &self,
        cli_path: &str,
        prompt: &str,
        mcp_config: &McpServerConfig,
    ) -> Result<CommandSpec, RuntimeError> {
        if !matches!(mcp_config.transport, McpTransport::HttpSse { .. }) {
            return Err(RuntimeError::UnsupportedInlineMcp {
                runtime: AgentRuntimeKind::Codex,
            });
        }

        Ok(CommandSpec::new(
            cli_path,
            vec!["exec".to_string(), prompt.to_string(), "--json".to_string()],
        ))
    }
}

struct GeminiRuntime;

impl AgentRuntime for GeminiRuntime {
    fn build_command(
        &self,
        cli_path: &str,
        prompt: &str,
        mcp_config: &McpServerConfig,
    ) -> Result<CommandSpec, RuntimeError> {
        if !matches!(mcp_config.transport, McpTransport::HttpSse { .. }) {
            return Err(RuntimeError::UnsupportedInlineMcp {
                runtime: AgentRuntimeKind::Gemini,
            });
        }

        Ok(CommandSpec::new(
            cli_path,
            vec![
                "-p".to_string(),
                prompt.to_string(),
                "--output-format".to_string(),
                "json".to_string(),
                "--yolo".to_string(), // Auto-approve actions for autonomous operation
            ],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::McpServerConfig;

    #[test]
    fn infer_runtime_from_cli_name() {
        assert_eq!(
            AgentRuntimeKind::infer_from_cli("codex"),
            AgentRuntimeKind::Codex
        );
        assert_eq!(
            AgentRuntimeKind::infer_from_cli("C:\\tools\\gemini.cmd"),
            AgentRuntimeKind::Gemini
        );
        assert_eq!(
            AgentRuntimeKind::infer_from_cli("claude.exe"),
            AgentRuntimeKind::Claude
        );
        assert_eq!(
            AgentRuntimeKind::infer_from_cli("custom-agent"),
            AgentRuntimeKind::ClaudeCompatible
        );
    }

    #[test]
    fn claude_runtime_uses_prompt_json_and_allowed_tools_flags() {
        let runtime = build_runtime(AgentRuntimeKind::Claude);
        let spec = runtime
            .build_command(
                "claude",
                "hello",
                &McpServerConfig::http_sse("http://localhost:3000/sse"),
            )
            .expect("build command");

        assert_eq!(spec.program, "claude");
        assert_eq!(
            spec.args,
            vec![
                "-p".to_string(),
                "hello".to_string(),
                "--output-format".to_string(),
                "json".to_string(),
                "--allowedTools".to_string(),
                r#""Bash,Read,Edit""#.to_string(),
            ]
        );
    }

    #[test]
    fn codex_runtime_uses_exec_json() {
        let runtime = build_runtime(AgentRuntimeKind::Codex);
        let spec = runtime
            .build_command(
                "codex",
                "do work",
                &McpServerConfig::http_sse("http://localhost:3000/sse"),
            )
            .expect("build command");

        assert_eq!(spec.program, "codex");
        assert_eq!(
            spec.args,
            vec![
                "exec".to_string(),
                "do work".to_string(),
                "--json".to_string(),
            ]
        );
    }

    #[test]
    fn gemini_runtime_uses_prompt_json_output() {
        let runtime = build_runtime(AgentRuntimeKind::Gemini);
        let spec = runtime
            .build_command(
                "gemini",
                "do work",
                &McpServerConfig::http_sse("http://localhost:3000/sse"),
            )
            .expect("build command");

        assert_eq!(spec.program, "gemini");
        assert_eq!(
            spec.args,
            vec![
                "-p".to_string(),
                "do work".to_string(),
                "--output-format".to_string(),
                "json".to_string(),
                "--yolo".to_string(),
            ]
        );
    }

    #[test]
    fn codex_rejects_stdio_mcp_transport() {
        let runtime = build_runtime(AgentRuntimeKind::Codex);
        let err = runtime
            .build_command(
                "codex",
                "prompt",
                &McpServerConfig::stdio("harness-mcp", vec![]),
            )
            .expect_err("must reject stdio");

        assert!(matches!(
            err,
            RuntimeError::UnsupportedInlineMcp {
                runtime: AgentRuntimeKind::Codex
            }
        ));
    }
}
