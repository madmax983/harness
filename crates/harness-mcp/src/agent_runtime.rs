//! Agent runtime for spawning and managing CLI agent processes.
//!
//! This module provides the [`AgentRuntime`] for orchestrating multi-model
//! agent teams by spawning CLI processes (e.g., `claude`, `codex`, `gemini`).
//!
//! # Example
//!
//! ```no_run
//! use harness_mcp::AgentRuntime;
//!
//! let mut runtime = AgentRuntime::new();
//!
//! // Spawn a Claude agent
//! let pid = runtime.spawn_cli_agent(
//!     "agent-1".to_string(),
//!     "claude",
//!     &["-p".to_string(), "{PROMPT}".to_string()],
//!     "You are a Rust developer...",
//! ).unwrap();
//!
//! // List running agents
//! let running = runtime.list_running();
//!
//! // Kill an agent
//! runtime.kill_agent("agent-1").unwrap();
//! ```

use std::collections::HashMap;
use std::process::{Child, Command};
use thiserror::Error;

/// Errors that can occur in the agent runtime.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Failed to spawn process: {0}")]
    SpawnFailed(#[from] std::io::Error),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Invalid command template: {0}")]
    InvalidTemplate(String),
}

pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// Tracks a spawned agent process.
#[derive(Debug)]
struct AgentProcess {
    child: Child,
    cli_command: String,
    cli_args: Vec<String>,
    started_at: std::time::SystemTime,
}

/// Runtime for managing spawned CLI agent processes.
pub struct AgentRuntime {
    processes: HashMap<String, AgentProcess>,
}

impl AgentRuntime {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
        }
    }

    /// Spawn a CLI agent process with the given system prompt.
    ///
    /// The `{PROMPT}` placeholder in `cli_args` will be replaced with `system_prompt`.
    pub fn spawn_cli_agent(
        &mut self,
        agent_id: String,
        cli_command: &str,
        cli_args: &[String],
        system_prompt: &str,
    ) -> RuntimeResult<u32> {
        // Replace {PROMPT} placeholder in args
        let processed_args: Vec<String> = cli_args
            .iter()
            .map(|arg| arg.replace("{PROMPT}", system_prompt))
            .collect();

        // Spawn the CLI process
        let child = Command::new(cli_command)
            .args(&processed_args)
            .env("HARNESS_AGENT_ID", &agent_id)
            .env("HARNESS_MCP_ENDPOINT", "http://localhost:3000")
            .spawn()?;

        let pid = child.id();

        // Track the process
        self.processes.insert(
            agent_id.clone(),
            AgentProcess {
                child,
                cli_command: cli_command.to_string(),
                cli_args: processed_args,
                started_at: std::time::SystemTime::now(),
            },
        );

        Ok(pid)
    }

    /// Kill an agent process.
    pub fn kill_agent(&mut self, agent_id: &str) -> RuntimeResult<()> {
        if let Some(mut process) = self.processes.remove(agent_id) {
            process.child.kill()?;
            Ok(())
        } else {
            Err(RuntimeError::AgentNotFound(agent_id.to_string()))
        }
    }

    /// List running agent IDs.
    pub fn list_running(&self) -> Vec<String> {
        self.processes.keys().cloned().collect()
    }

    /// Check if an agent is currently running.
    pub fn is_running(&self, agent_id: &str) -> bool {
        self.processes.contains_key(agent_id)
    }

    /// Get process info for an agent.
    pub fn get_process_info(&self, agent_id: &str) -> Option<AgentProcessInfo> {
        self.processes.get(agent_id).map(|p| AgentProcessInfo {
            agent_id: agent_id.to_string(),
            pid: p.child.id(),
            cli_command: p.cli_command.clone(),
            cli_args: p.cli_args.clone(),
            started_at: p.started_at,
        })
    }

    /// Get info for all running processes.
    pub fn list_process_info(&self) -> Vec<AgentProcessInfo> {
        self.processes
            .keys()
            .filter_map(|id| self.get_process_info(id))
            .collect()
    }
}

/// Information about a running agent process.
#[derive(Debug, Clone)]
pub struct AgentProcessInfo {
    pub agent_id: String,
    pub pid: u32,
    pub cli_command: String,
    pub cli_args: Vec<String>,
    pub started_at: std::time::SystemTime,
}

impl Default for AgentRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick_command_with_prompt() -> (&'static str, Vec<String>) {
        if cfg!(windows) {
            (
                "cmd",
                vec!["/C".to_string(), "echo".to_string(), "{PROMPT}".to_string()],
            )
        } else {
            (
                "sh",
                vec!["-c".to_string(), "echo \"{PROMPT}\"".to_string()],
            )
        }
    }

    fn long_running_command() -> (&'static str, Vec<String>) {
        if cfg!(windows) {
            (
                r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
                vec![
                    "-NoProfile".to_string(),
                    "-Command".to_string(),
                    "Start-Sleep -Seconds 30".to_string(),
                ],
            )
        } else {
            ("sleep", vec!["30".to_string()])
        }
    }

    #[test]
    fn test_new_runtime_is_empty() {
        let runtime = AgentRuntime::new();
        assert_eq!(runtime.list_running().len(), 0);
    }

    #[test]
    fn test_spawn_cli_agent_basic() {
        let mut runtime = AgentRuntime::new();
        let (cli_command, cli_args) = quick_command_with_prompt();

        let result = runtime.spawn_cli_agent(
            "test-agent-1".to_string(),
            cli_command,
            &cli_args,
            "agent-1",
        );

        assert!(result.is_ok(), "Should spawn process successfully");
        let pid = result.unwrap();
        assert!(pid > 0, "Should return valid PID");
    }

    #[test]
    fn test_spawn_replaces_prompt_placeholder() {
        let mut runtime = AgentRuntime::new();
        let (cli_command, cli_args) = quick_command_with_prompt();
        let prompt = "custom system prompt";

        let result =
            runtime.spawn_cli_agent("test-agent-2".to_string(), cli_command, &cli_args, prompt);

        assert!(result.is_ok());
        let process_info = runtime.get_process_info("test-agent-2").unwrap();
        let joined = process_info.cli_args.join(" ");
        assert!(!joined.contains("{PROMPT}"));
        assert!(joined.contains(prompt));
    }

    #[test]
    fn test_list_running_agents() {
        let mut runtime = AgentRuntime::new();
        let (cli_command, cli_args) = long_running_command();

        runtime
            .spawn_cli_agent("agent-1".to_string(), cli_command, &cli_args, "test")
            .unwrap();

        runtime
            .spawn_cli_agent("agent-2".to_string(), cli_command, &cli_args, "test")
            .unwrap();

        let running = runtime.list_running();
        assert_eq!(running.len(), 2);
        assert!(running.contains(&"agent-1".to_string()));
        assert!(running.contains(&"agent-2".to_string()));

        runtime.kill_agent("agent-1").unwrap();
        runtime.kill_agent("agent-2").unwrap();
    }

    #[test]
    fn test_kill_agent() {
        let mut runtime = AgentRuntime::new();
        let (cli_command, cli_args) = long_running_command();

        runtime
            .spawn_cli_agent("agent-to-kill".to_string(), cli_command, &cli_args, "test")
            .unwrap();

        assert!(runtime.is_running("agent-to-kill"));

        // Kill it
        let result = runtime.kill_agent("agent-to-kill");
        assert!(result.is_ok());

        // Should no longer be running
        assert!(!runtime.is_running("agent-to-kill"));
    }

    #[test]
    fn test_kill_nonexistent_agent_fails() {
        let mut runtime = AgentRuntime::new();

        let result = runtime.kill_agent("nonexistent");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RuntimeError::AgentNotFound(_)
        ));
    }

    #[test]
    fn test_is_running() {
        let mut runtime = AgentRuntime::new();
        let (cli_command, cli_args) = long_running_command();

        assert!(!runtime.is_running("agent-1"));

        runtime
            .spawn_cli_agent("agent-1".to_string(), cli_command, &cli_args, "test")
            .unwrap();

        assert!(runtime.is_running("agent-1"));
        runtime.kill_agent("agent-1").unwrap();
    }
}
