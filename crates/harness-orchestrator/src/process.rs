//! Process management for Claude agents.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;

use harness_persistence::{Agent, AgentId, AgentRole, AgentStatus, Repository, SessionId};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

use crate::config::OrchestratorConfig;
use crate::prompts;

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

/// Manages Claude processes.
pub struct ProcessManager<R: Repository> {
    config: OrchestratorConfig,
    repository: Arc<R>,
    processes: RwLock<HashMap<AgentId, Child>>,
}

impl<R: Repository + 'static> ProcessManager<R> {
    /// Create a new process manager.
    pub fn new(config: OrchestratorConfig, repository: Arc<R>) -> Self {
        Self {
            config,
            repository,
            processes: RwLock::new(HashMap::new()),
        }
    }

    /// Spawn the Strategoi agent.
    ///
    /// Creates the agent record and spawns a Claude process with the Strategoi prompt.
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
    /// Creates the agent record and spawns a Claude process with the role-specific prompt.
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

    /// Spawn a Claude process for a pre-existing agent with a custom prompt.
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

    /// Internal: spawn a Claude process and track it.
    async fn spawn_process(&self, agent_id: AgentId, prompt: &str) -> ProcessResult<()> {
        let mcp_json = self.config.mcp_config.to_json();

        let child = Command::new(&self.config.claude_path)
            .arg("-p")
            .arg(prompt)
            .arg("--mcp-config")
            .arg(&mcp_json)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ProcessError::SpawnFailed(e.to_string()))?;

        // Update status to Starting
        self.repository
            .update_agent_status(agent_id, AgentStatus::Starting)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        // Store process handle
        self.processes.write().await.insert(agent_id, child);

        Ok(())
    }

    /// Kill an agent's process.
    pub async fn kill(&self, agent_id: AgentId) -> ProcessResult<()> {
        let mut processes = self.processes.write().await;
        let child = processes
            .get_mut(&agent_id)
            .ok_or(ProcessError::NotFound(agent_id))?;

        child.kill().await?;
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
        if let Some(child) = processes.get(&agent_id) {
            child.id().is_some()
        } else {
            false
        }
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

    #[tokio::test]
    async fn test_cap_enforcement() {
        let repo = Arc::new(InMemoryRepository::new());
        let config = OrchestratorConfig {
            population_cap: 1,
            claude_path: "echo".into(),
            ..Default::default()
        };

        let manager = ProcessManager::new(config, repo.clone());

        let session = Session::new(1);
        repo.create_session(&session).await.unwrap();

        let agent1 = Agent::new(AgentRole::Developer, session.id);
        let agent2 = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent1).await.unwrap();
        repo.create_agent(&agent2).await.unwrap();

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
            claude_path: "echo".into(),
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
            claude_path: "echo".into(),
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
}
