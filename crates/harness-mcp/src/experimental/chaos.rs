//! Chaos Engine: Injects controlled entropy into the hive.
//!
//! Provides capabilities to simulate adverse conditions like agent failure
//! and task blocking to test system resilience.

use crate::state::HiveState;
use crate::tools;
use harness_persistence::{Repository, TaskStatus};
use rand::seq::SliceRandom;
use std::sync::Arc;

/// The Chaos Engine allows injecting failures into the hive.
pub struct ChaosEngine<R: Repository> {
    state: Arc<HiveState<R>>,
}

impl<R: Repository + 'static> ChaosEngine<R> {
    /// Create a new Chaos Engine instance.
    pub fn new(state: Arc<HiveState<R>>) -> Self {
        Self { state }
    }

    /// Execute a chaos injection request.
    pub async fn inject(
        &self,
        req: tools::InjectChaosRequest,
    ) -> Result<tools::InjectChaosResponse, String> {
        match req.kind.as_str() {
            "kill_agent" => self.kill_random_agent(req.target_agent_id).await,
            "block_task" => self.block_random_task(req.target_task_id).await,
            _ => Err(format!("Unknown chaos kind: {}", req.kind)),
        }
    }

    /// Terminate an agent process randomly or by ID.
    async fn kill_random_agent(
        &self,
        target_id: Option<String>,
    ) -> Result<tools::InjectChaosResponse, String> {
        let agent_id = if let Some(id_str) = target_id {
            // Target specific agent
            let uuid = uuid::Uuid::parse_str(&id_str).map_err(|e| e.to_string())?;
            harness_persistence::AgentId::from_uuid(uuid)
        } else {
            // Pick random active agent
            let agents = self
                .state
                .repository()
                .list_active_agents(self.state.session_id())
                .await
                .map_err(|e| e.to_string())?;

            // Filter out strategoi to avoid killing the commander (unless explicit)
            let eligible: Vec<_> = agents.into_iter().filter(|a| !a.is_strategoi).collect();

            if eligible.is_empty() {
                return Err(
                    "No eligible agents to kill (strategoi is protected from random kills)"
                        .to_string(),
                );
            }

            let mut rng = rand::thread_rng();
            eligible.choose(&mut rng).unwrap().id
        };

        // Kill the process
        self.state
            .process_manager()
            .kill(agent_id)
            .await
            .map_err(|e| format!("Failed to kill agent: {}", e))?;

        Ok(tools::InjectChaosResponse {
            action_taken: "kill_agent".to_string(),
            description: format!("Terminated agent process for {}", agent_id.as_uuid()),
            affected_entity_id: Some(agent_id.as_uuid().to_string()),
            chaos_level: 0.8,
        })
    }

    /// Block (fail) a task randomly or by ID.
    async fn block_random_task(
        &self,
        target_id: Option<String>,
    ) -> Result<tools::InjectChaosResponse, String> {
        let task_id = if let Some(id_str) = target_id {
            let uuid = uuid::Uuid::parse_str(&id_str).map_err(|e| e.to_string())?;
            harness_persistence::TaskId::from_uuid(uuid)
        } else {
            // Pick random in-progress task
            let tasks = self
                .state
                .repository()
                .list_tasks(self.state.session_id(), Some(TaskStatus::InProgress))
                .await
                .map_err(|e| e.to_string())?;

            if tasks.is_empty() {
                return Err("No in-progress tasks to block".to_string());
            }

            let mut rng = rand::thread_rng();
            tasks.choose(&mut rng).unwrap().id
        };

        // Fail the task with a chaos summary
        self.state
            .repository()
            .update_task_status(
                task_id,
                TaskStatus::Failed,
                Some("Chaos Monkey: Task randomly failed to test resilience."),
            )
            .await
            .map_err(|e| e.to_string())?;

        Ok(tools::InjectChaosResponse {
            action_taken: "block_task".to_string(),
            description: format!("Failed task {} via Chaos Monkey", task_id.as_uuid()),
            affected_entity_id: Some(task_id.as_uuid().to_string()),
            chaos_level: 0.5,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_orchestrator::{OrchestratorConfig, ProcessManager};
    use harness_persistence::{Agent, AgentRole, InMemoryRepository, Priority, Session, Task};
    use std::sync::Arc;

    async fn setup() -> (
        Arc<HiveState<InMemoryRepository>>,
        ChaosEngine<InMemoryRepository>,
    ) {
        let repo = Arc::new(InMemoryRepository::new());
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let config = OrchestratorConfig::default();
        let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));

        let state = Arc::new(HiveState::new(session, repo, process_manager));
        let engine = ChaosEngine::new(state.clone());
        (state, engine)
    }

    #[tokio::test]
    async fn test_kill_random_agent() {
        let (state, engine) = setup().await;

        // Create a worker agent
        let worker = Agent::new(AgentRole::Developer, state.session_id());
        state.repository().create_agent(&worker).await.unwrap();

        // Mock process manager state is hard without actual processes,
        // but ProcessManager::kill updates repository status.
        // We need to simulate the process existence or ProcessManager::kill will fail with NotFound.
        // InMemory ProcessManager requires actual spawned process.
        // So we might skip the actual kill call check or handle the error gracefully?
        // Wait, ProcessManager checks `self.processes`. If empty, kill fails.
        // So we can't easily integration test kill without spawning a process.
        // But we CAN test that it fails correctly if no process exists.

        let res = engine.kill_random_agent(None).await;
        // Should find the agent but fail to kill because no process handle exists
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("process for agent"));
    }

    #[tokio::test]
    async fn test_block_random_task() {
        let (state, engine) = setup().await;

        let task = Task::new("Work", "Do work", Priority::Medium, state.session_id());
        state.repository().create_task(&task).await.unwrap();
        state
            .repository()
            .update_task_status(task.id, TaskStatus::InProgress, None)
            .await
            .unwrap();

        let res = engine.block_random_task(None).await.unwrap();

        assert_eq!(res.action_taken, "block_task");
        assert_eq!(res.affected_entity_id, Some(task.id.as_uuid().to_string()));

        let updated = state.repository().get_task(task.id).await.unwrap();
        assert_eq!(updated.status, TaskStatus::Failed);
        assert!(updated.summary.unwrap().contains("Chaos Monkey"));
    }
}
