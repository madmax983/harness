//! Process management for agent CLI processes.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;

use harness_persistence::{Agent, AgentId, AgentRole, AgentStatus, Repository, SessionId};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

use crate::config::OrchestratorConfig;
use crate::prompts;
use crate::runtime::{AgentRuntime, AgentRuntimeKind, build_runtime};

/// Process handle with captured output and session tracking.
struct ProcessHandle {
    child: Child,
    stdout_buffer: Arc<RwLock<String>>,
    stderr_buffer: Arc<RwLock<String>>,
    /// CLI-specific session ID (e.g., codex thread_id, claude session_id).
    /// Used for resuming conversations with `command_agent()`.
    cli_session_id: Option<String>,
}

/// Error type for process operations.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    /// Failed to spawn process.
    #[error("failed to spawn process: {0}")]
    SpawnFailed(String),

    /// Process not found.
    #[error("process for agent {0} not found")]
    NotFound(AgentId),

    /// Population cap reached.
    #[error("population cap ({cap}) reached, cannot spawn more agents")]
    CapReached { cap: usize },

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Repository error.
    #[error("repository error: {0}")]
    Repository(String),
}

/// Result type for process operations.
pub type ProcessResult<T> = Result<T, ProcessError>;

/// Manages agent processes.
pub struct ProcessManager<R: Repository> {
    config: OrchestratorConfig,
    runtime: Arc<dyn AgentRuntime>,
    repository: Arc<R>,
    processes: RwLock<HashMap<AgentId, ProcessHandle>>,
}

impl<R: Repository + 'static> ProcessManager<R> {
    /// Create a new process manager.
    pub fn new(config: OrchestratorConfig, repository: Arc<R>) -> Self {
        let runtime = build_runtime(config.agent_runtime);
        Self {
            config,
            runtime,
            repository,
            processes: RwLock::new(HashMap::new()),
        }
    }

    /// Spawn the Strategoi agent.
    ///
    /// Creates the agent record and spawns a process with the Strategoi prompt.
    pub async fn spawn_strategoi(
        &self,
        session_id: SessionId,
        session_context: &str,
    ) -> ProcessResult<AgentId> {
        self.spawn_role(AgentRole::Strategoi, session_id, session_context)
            .await
    }

    /// Spawn a worker agent with a specific BMAD role.
    ///
    /// Creates the agent record and spawns a process with the role-specific prompt.
    pub async fn spawn_worker(
        &self,
        role: AgentRole,
        session_id: SessionId,
        session_context: &str,
    ) -> ProcessResult<AgentId> {
        if role == AgentRole::Strategoi {
            return Err(ProcessError::SpawnFailed(
                "Use spawn_strategoi() for Strategoi role".into(),
            ));
        }
        self.spawn_role(role, session_id, session_context).await
    }

    /// Spawn an agent with a given role.
    async fn spawn_role(
        &self,
        role: AgentRole,
        session_id: SessionId,
        session_context: &str,
    ) -> ProcessResult<AgentId> {
        // Check population cap
        let active_count = self
            .repository
            .count_active_agents(session_id)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        if active_count >= self.config.population_cap {
            return Err(ProcessError::CapReached {
                cap: self.config.population_cap,
            });
        }

        // Create agent record
        let agent = Agent::new(role, session_id);
        let agent_id = agent.id;
        self.repository
            .create_agent(&agent)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        // Generate role-specific prompt
        let prompt = prompts::system_prompt(role, session_context);

        // Spawn process
        self.spawn_process(agent_id, &prompt).await?;

        Ok(agent_id)
    }

    /// Spawn a process for a pre-existing agent with a custom prompt.
    pub async fn spawn(&self, agent_id: AgentId, prompt: &str) -> ProcessResult<()> {
        // Check population cap
        let agent = self
            .repository
            .get_agent(agent_id)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        let active_count = self
            .repository
            .count_active_agents(agent.session_id)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        if active_count >= self.config.population_cap {
            return Err(ProcessError::CapReached {
                cap: self.config.population_cap,
            });
        }

        self.spawn_process(agent_id, prompt).await
    }

    /// Spawn a process with a custom CLI command (multi-CLI orchestration).
    ///
    /// Use this when spawning agents from different CLI tools (Codex, Gemini, etc.)
    /// instead of using the daemon's configured default runtime.
    pub async fn spawn_with_cli(
        &self,
        agent_id: AgentId,
        cli_command: &str,
        cli_args: &[String],
        prompt: &str,
    ) -> ProcessResult<()> {
        // Check population cap
        let agent = self
            .repository
            .get_agent(agent_id)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        let active_count = self
            .repository
            .count_active_agents(agent.session_id)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        if active_count >= self.config.population_cap {
            return Err(ProcessError::CapReached {
                cap: self.config.population_cap,
            });
        }

        self.spawn_process_with_cli(agent_id, cli_command, cli_args, prompt)
            .await
    }

    /// Internal: create a process handle with output capture.
    fn create_process_handle(mut child: Child) -> ProcessHandle {
        let stdout_buffer = Arc::new(RwLock::new(String::new()));
        let stderr_buffer = Arc::new(RwLock::new(String::new()));

        // Spawn task to read stdout
        if let Some(stdout) = child.stdout.take() {
            let buffer = stdout_buffer.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let mut buf = buffer.write().await;
                    buf.push_str(&line);
                    buf.push('\n');
                }
            });
        }

        // Spawn task to read stderr
        if let Some(stderr) = child.stderr.take() {
            let buffer = stderr_buffer.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let mut buf = buffer.write().await;
                    buf.push_str(&line);
                    buf.push('\n');
                }
            });
        }

        ProcessHandle {
            child,
            stdout_buffer,
            stderr_buffer,
            cli_session_id: None,
        }
    }

    /// Internal: spawn an agent process and track it.
    async fn spawn_process(&self, agent_id: AgentId, prompt: &str) -> ProcessResult<()> {
        let spec = self
            .runtime
            .build_command(&self.config.agent_cli_path, prompt, &self.config.mcp_config)
            .map_err(|e| ProcessError::SpawnFailed(e.to_string()))?;

        let child = Command::new(&spec.program)
            .args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ProcessError::SpawnFailed(e.to_string()))?;

        // Create process handle with output capture
        let handle = Self::create_process_handle(child);

        // Update status to Starting
        self.repository
            .update_agent_status(agent_id, AgentStatus::Starting)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        // Store process handle
        self.processes.write().await.insert(agent_id, handle);

        Ok(())
    }

    /// Build MCP-augmented prompt with connection instructions.
    fn build_mcp_prompt(&self, prompt: &str) -> String {
        let mcp_json = self.config.mcp_config.to_json();
        format!(
            "{}\n\n# MCP Connection\nConnect to harness MCP server with this config:\n```json\n{}\n```",
            prompt, mcp_json
        )
    }

    /// Internal: spawn an agent process with custom CLI command.
    async fn spawn_process_with_cli(
        &self,
        agent_id: AgentId,
        cli_command: &str,
        cli_args: &[String],
        prompt: &str,
    ) -> ProcessResult<()> {
        // Build full prompt with MCP connection instructions
        let full_prompt = self.build_mcp_prompt(prompt);

        // Replace {PROMPT} placeholder in args with actual prompt
        let mut resolved_args: Vec<String> = cli_args
            .iter()
            .map(|arg| arg.replace("{PROMPT}", &full_prompt))
            .collect();

        // Platform-aware command resolution
        let (program, args) = if cfg!(windows) && (cli_command.ends_with(".cmd") || cli_command.ends_with(".bat")) {
            // Windows batch files need cmd.exe wrapper
            let mut cmd_args = vec!["/c".to_string(), cli_command.to_string()];
            cmd_args.append(&mut resolved_args);
            ("cmd.exe".to_string(), cmd_args)
        } else if cfg!(windows) && !cli_command.contains('\\') && !cli_command.contains('/') && !cli_command.ends_with(".exe") {
            // Bare command on Windows - might be .cmd in PATH, try cmd.exe wrapper
            let mut cmd_args = vec!["/c".to_string(), cli_command.to_string()];
            cmd_args.append(&mut resolved_args);
            ("cmd.exe".to_string(), cmd_args)
        } else {
            // Direct execution (Unix or Windows .exe)
            (cli_command.to_string(), resolved_args)
        };

        let child = Command::new(&program)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ProcessError::SpawnFailed(format!("Failed to spawn '{}': {}", program, e)))?;

        // Create process handle with output capture
        let handle = Self::create_process_handle(child);

        // Update status to Starting
        self.repository
            .update_agent_status(agent_id, AgentStatus::Starting)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        // Store process handle
        self.processes.write().await.insert(agent_id, handle);

        Ok(())
    }

    /// Kill an agent's process.
    pub async fn kill(&self, agent_id: AgentId) -> ProcessResult<()> {
        let mut processes = self.processes.write().await;
        let handle = processes
            .get_mut(&agent_id)
            .ok_or(ProcessError::NotFound(agent_id))?;

        handle.child.kill().await?;
        processes.remove(&agent_id);

        // Update status to Killed
        self.repository
            .update_agent_status(agent_id, AgentStatus::Killed)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        Ok(())
    }

    /// Check if a process is still running.
    pub async fn is_running(&self, agent_id: AgentId) -> bool {
        let processes = self.processes.read().await;
        if let Some(handle) = processes.get(&agent_id) {
            handle.child.id().is_some()
        } else {
            false
        }
    }

    /// Get captured output from a process.
    pub async fn get_output(&self, agent_id: AgentId) -> ProcessResult<(String, String)> {
        let processes = self.processes.read().await;
        let handle = processes
            .get(&agent_id)
            .ok_or(ProcessError::NotFound(agent_id))?;

        let stdout = handle.stdout_buffer.read().await.clone();
        let stderr = handle.stderr_buffer.read().await.clone();

        Ok((stdout, stderr))
    }

    /// Extract CLI session ID from process output.
    /// For codex: looks for thread_id in JSON output.
    /// For claude: looks for session_id pattern.
    /// For gemini: looks for session_id pattern.
    async fn extract_session_id(&self, agent_id: AgentId, runtime_kind: AgentRuntimeKind) -> Option<String> {
        let processes = self.processes.read().await;
        let handle = processes.get(&agent_id)?;

        // First check if we already have a stored session ID
        if let Some(ref stored_id) = handle.cli_session_id {
            return Some(stored_id.clone());
        }

        let stdout = handle.stdout_buffer.read().await;
        let stderr = handle.stderr_buffer.read().await;

        // Combine stdout and stderr for scanning
        let combined = format!("{}\n{}", *stdout, *stderr);

        // Try runtime-specific patterns
        match runtime_kind {
            AgentRuntimeKind::Codex => {
                // Try JSON parsing first for codex --json output
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&combined)
                    && let Some(thread_id) = json.get("thread_id").and_then(|v| v.as_str())
                {
                    return Some(thread_id.to_string());
                }

                // Fallback to string search for partial JSON
                if let Some(start) = combined.find("\"thread_id\":\"") {
                    let id_start = start + "\"thread_id\":\"".len();
                    if let Some(end) = combined[id_start..].find('"') {
                        return Some(combined[id_start..id_start + end].to_string());
                    }
                }
            }
            AgentRuntimeKind::Claude | AgentRuntimeKind::ClaudeCompatible => {
                // Look for "session id: <uuid>" pattern in Claude output
                if let Some(start) = combined.find("session id: ") {
                    let id_start = start + "session id: ".len();
                    // UUID format: 8-4-4-4-12 hex chars with dashes
                    if let Some(end) = combined[id_start..].find(|c: char| !c.is_ascii_hexdigit() && c != '-') {
                        let candidate = &combined[id_start..id_start + end];
                        if candidate.len() == 36 {  // Standard UUID length
                            return Some(candidate.to_string());
                        }
                    }
                }
            }
            AgentRuntimeKind::Gemini => {
                // Look for "Session ID: <uuid>" pattern in Gemini output
                if let Some(start) = combined.find("Session ID: ") {
                    let id_start = start + "Session ID: ".len();
                    if let Some(end) = combined[id_start..].find(|c: char| !c.is_ascii_hexdigit() && c != '-') {
                        let candidate = &combined[id_start..id_start + end];
                        if candidate.len() == 36 {
                            return Some(candidate.to_string());
                        }
                    }
                }
            }
        }

        None
    }

    /// Command an agent to continue with a new prompt (strategoi-centric orchestration).
    /// Uses CLI-specific resume functionality (e.g., `codex exec resume <SESSION_ID>`).
    pub async fn command_agent(
        &self,
        agent_id: AgentId,
        cli_command: &str,
        cli_args: &[String],
        prompt: &str,
    ) -> ProcessResult<()> {
        // Infer runtime kind from CLI command
        let runtime_kind = AgentRuntimeKind::infer_from_cli(cli_command);

        // Extract session ID from current process (checks stored ID first, then parses output)
        let session_id = self.extract_session_id(agent_id, runtime_kind).await;

        if session_id.is_none() {
            tracing::warn!(
                agent_id = %agent_id,
                runtime = ?runtime_kind,
                "No session ID found for agent, spawning fresh"
            );
        }

        // Kill existing process
        let _ = self.kill(agent_id).await; // Ignore errors if already dead

        // Build MCP-augmented prompt
        let full_prompt = self.build_mcp_prompt(prompt);

        // Build resume command with runtime-specific logic
        let mut resolved_args: Vec<String> = match (runtime_kind, session_id.as_ref()) {
            // Codex with session: exec resume <SESSION_ID> <PROMPT> [--json and other flags]
            (AgentRuntimeKind::Codex, Some(sid)) => {
                let mut args = vec![
                    "exec".to_string(),
                    "resume".to_string(),
                    sid.clone(),
                    full_prompt.clone(),
                ];
                // Preserve flags like --json from original cli_args
                args.extend(
                    cli_args
                        .iter()
                        .filter(|arg| arg.starts_with("--") || arg.starts_with('-'))
                        .cloned(),
                );
                args
            }
            // Claude/Gemini with session: not implemented yet, fall back to fresh
            (AgentRuntimeKind::Claude | AgentRuntimeKind::Gemini | AgentRuntimeKind::ClaudeCompatible, Some(_sid)) => {
                tracing::warn!(
                    runtime = ?runtime_kind,
                    "Session resume not implemented for this runtime, spawning fresh"
                );
                cli_args.iter().map(|arg| arg.replace("{PROMPT}", &full_prompt)).collect()
            }
            // No session: use original args with prompt replacement
            (_, None) => {
                cli_args.iter().map(|arg| arg.replace("{PROMPT}", &full_prompt)).collect()
            }
        };

        // Platform-aware command resolution (same as spawn_process_with_cli)
        let (program, args) = if cfg!(windows)
            && (cli_command.ends_with(".cmd")
                || cli_command.ends_with(".bat")
                || (!cli_command.contains('\\')
                    && !cli_command.contains('/')
                    && !cli_command.ends_with(".exe")))
        {
            let mut cmd_args = vec!["/c".to_string(), cli_command.to_string()];
            cmd_args.append(&mut resolved_args);
            ("cmd.exe".to_string(), cmd_args)
        } else {
            (cli_command.to_string(), resolved_args)
        };

        // Spawn new process
        let child = Command::new(&program)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ProcessError::SpawnFailed(format!("Failed to command '{}': {}", program, e)))?;

        let mut handle = Self::create_process_handle(child);

        // Preserve session ID if we had one
        if let Some(sid) = session_id {
            handle.cli_session_id = Some(sid);
        }

        // Update status to Starting
        self.repository
            .update_agent_status(agent_id, AgentStatus::Starting)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        // Store new process handle
        self.processes.write().await.insert(agent_id, handle);

        Ok(())
    }

    /// Get the number of running processes.
    pub async fn running_count(&self) -> usize {
        self.processes.read().await.len()
    }

    /// Get the orchestrator configuration.
    pub fn config(&self) -> &OrchestratorConfig {
        &self.config
    }

    /// Get the repository reference.
    pub fn repository(&self) -> &Arc<R> {
        &self.repository
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_persistence::{InMemoryRepository, Session};

    fn test_cli_path() -> String {
        if cfg!(windows) {
            std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string())
        } else {
            "sh".to_string()
        }
    }

    #[tokio::test]
    async fn test_cap_enforcement() {
        let repo = Arc::new(InMemoryRepository::new());
        let config = OrchestratorConfig {
            population_cap: 1,
            agent_cli_path: test_cli_path(),
            ..Default::default()
        };

        let manager = ProcessManager::new(config, repo.clone());

        let session = Session::new(1);
        repo.create_session(&session).await.unwrap();

        let agent1 = Agent::new(AgentRole::Developer, session.id);
        let agent2 = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent1).await.unwrap();
        repo.create_agent(&agent2).await.unwrap();

        // Set agents to Pending to test spawn cap enforcement
        // (agents auto-activate on creation, so we need to reset them)
        repo.update_agent_status(agent1.id, AgentStatus::Pending)
            .await
            .unwrap();
        repo.update_agent_status(agent2.id, AgentStatus::Pending)
            .await
            .unwrap();

        // First spawn should succeed
        manager.spawn(agent1.id, "test prompt").await.unwrap();

        // Second spawn should fail due to cap
        let result = manager.spawn(agent2.id, "test prompt").await;
        assert!(matches!(result, Err(ProcessError::CapReached { cap: 1 })));
    }

    #[tokio::test]
    async fn test_spawn_strategoi_uses_strategoi_role() {
        let repo = Arc::new(InMemoryRepository::new());
        let config = OrchestratorConfig {
            population_cap: 8,
            agent_cli_path: test_cli_path(),
            ..Default::default()
        };

        let manager = ProcessManager::new(config, repo.clone());
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent_id = manager
            .spawn_strategoi(session.id, "Test project")
            .await
            .unwrap();

        let agent = repo.get_agent(agent_id).await.unwrap();
        assert_eq!(agent.role, AgentRole::Strategoi);
        assert!(agent.is_strategoi);
    }

    #[tokio::test]
    async fn test_spawn_worker_rejects_strategoi_role() {
        let repo = Arc::new(InMemoryRepository::new());
        let config = OrchestratorConfig {
            population_cap: 8,
            agent_cli_path: test_cli_path(),
            ..Default::default()
        };

        let manager = ProcessManager::new(config, repo.clone());
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let result = manager
            .spawn_worker(AgentRole::Strategoi, session.id, "Test")
            .await;
        assert!(matches!(result, Err(ProcessError::SpawnFailed(_))));
    }

    #[tokio::test]
    async fn test_extract_session_id_codex_json() {
        use crate::runtime::AgentRuntimeKind;

        let repo = Arc::new(InMemoryRepository::new());
        let config = OrchestratorConfig {
            population_cap: 8,
            agent_cli_path: test_cli_path(),
            ..Default::default()
        };

        let manager = ProcessManager::new(config, repo.clone());
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        // Simulate codex JSON output with thread_id
        let child = Command::new(test_cli_path())
            .arg(if cfg!(windows) { "/c" } else { "-c" })
            .arg("echo")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let handle = ProcessManager::<InMemoryRepository>::create_process_handle(child);

        // Inject codex-style output
        {
            let mut buf = handle.stdout_buffer.write().await;
            buf.push_str(r#"{"thread_id": "019c5833-db2e-73d3-8b81-3faa95772466", "status": "active"}"#);
        }

        manager.processes.write().await.insert(agent.id, handle);

        let session_id = manager.extract_session_id(agent.id, AgentRuntimeKind::Codex).await;
        assert_eq!(session_id, Some("019c5833-db2e-73d3-8b81-3faa95772466".to_string()));
    }

    #[tokio::test]
    async fn test_extract_session_id_uses_stored_value() {
        use crate::runtime::AgentRuntimeKind;

        let repo = Arc::new(InMemoryRepository::new());
        let config = OrchestratorConfig {
            population_cap: 8,
            agent_cli_path: test_cli_path(),
            ..Default::default()
        };

        let manager = ProcessManager::new(config, repo.clone());
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        let child = Command::new(test_cli_path())
            .arg(if cfg!(windows) { "/c" } else { "-c" })
            .arg("echo")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut handle = ProcessManager::<InMemoryRepository>::create_process_handle(child);

        // Pre-set a stored session ID
        handle.cli_session_id = Some("stored-session-123".to_string());

        manager.processes.write().await.insert(agent.id, handle);

        // Should return stored ID without parsing output
        let session_id = manager.extract_session_id(agent.id, AgentRuntimeKind::Codex).await;
        assert_eq!(session_id, Some("stored-session-123".to_string()));
    }

    #[tokio::test]
    async fn test_extract_session_id_fallback_string_search() {
        use crate::runtime::AgentRuntimeKind;

        let repo = Arc::new(InMemoryRepository::new());
        let config = OrchestratorConfig {
            population_cap: 8,
            agent_cli_path: test_cli_path(),
            ..Default::default()
        };

        let manager = ProcessManager::new(config, repo.clone());
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        let child = Command::new(test_cli_path())
            .arg(if cfg!(windows) { "/c" } else { "-c" })
            .arg("echo")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let handle = ProcessManager::<InMemoryRepository>::create_process_handle(child);

        // Inject partial JSON output (not valid JSON but contains thread_id)
        {
            let mut buf = handle.stdout_buffer.write().await;
            buf.push_str(r#"Some output before "thread_id":"abc-123-def" and after"#);
        }

        manager.processes.write().await.insert(agent.id, handle);

        let session_id = manager.extract_session_id(agent.id, AgentRuntimeKind::Codex).await;
        assert_eq!(session_id, Some("abc-123-def".to_string()));
    }

    #[tokio::test]
    async fn test_build_mcp_prompt_includes_connection_info() {
        let repo = Arc::new(InMemoryRepository::new());
        let config = OrchestratorConfig {
            population_cap: 8,
            agent_cli_path: test_cli_path(),
            ..Default::default()
        };

        let manager = ProcessManager::new(config, repo.clone());
        let prompt = manager.build_mcp_prompt("Test prompt");

        assert!(prompt.contains("Test prompt"));
        assert!(prompt.contains("MCP Connection"));
        assert!(prompt.contains("harness MCP server"));
        assert!(prompt.contains("```json"));
    }
}
