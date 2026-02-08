//! In-memory repository implementation for testing.

use std::collections::HashMap;
use std::sync::RwLock;

use chrono::Utc;

use crate::{
    Agent, AgentId, AgentRole, AgentStatus, DirectMessage, Knowledge, KnowledgeId, Repository,
    RepositoryError, RepositoryResult, Session, SessionId, Task, TaskId, TaskStatus,
};

/// In-memory repository for testing and development.
#[derive(Debug, Default)]
pub struct InMemoryRepository {
    sessions: RwLock<HashMap<SessionId, Session>>,
    agents: RwLock<HashMap<AgentId, Agent>>,
    tasks: RwLock<HashMap<TaskId, Task>>,
    knowledge: RwLock<HashMap<KnowledgeId, Knowledge>>,
    direct_messages: RwLock<Vec<DirectMessage>>,
}

impl InMemoryRepository {
    /// Create a new empty repository.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Repository for InMemoryRepository {
    // === Session operations ===

    async fn create_session(&self, session: &Session) -> RepositoryResult<()> {
        self.sessions
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .insert(session.id, session.clone());
        Ok(())
    }

    async fn get_session(&self, id: SessionId) -> RepositoryResult<Session> {
        self.sessions
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .get(&id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "Session".into(),
                id: id.as_uuid().to_string(),
            })
    }

    // === Agent operations ===

    async fn create_agent(&self, agent: &Agent) -> RepositoryResult<()> {
        self.agents
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .insert(agent.id, agent.clone());
        Ok(())
    }

    async fn get_agent(&self, id: AgentId) -> RepositoryResult<Agent> {
        self.agents
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .get(&id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "Agent".into(),
                id: id.to_string(),
            })
    }

    async fn update_agent_status(&self, id: AgentId, status: AgentStatus) -> RepositoryResult<()> {
        let mut agents = self
            .agents
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let agent = agents
            .get_mut(&id)
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "Agent".into(),
                id: id.to_string(),
            })?;
        agent.status = status;
        Ok(())
    }

    async fn update_agent_task(&self, id: AgentId, task: Option<TaskId>) -> RepositoryResult<()> {
        let mut agents = self
            .agents
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let agent = agents
            .get_mut(&id)
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "Agent".into(),
                id: id.to_string(),
            })?;
        agent.current_task = task;
        Ok(())
    }

    async fn list_agents(&self, session_id: SessionId) -> RepositoryResult<Vec<Agent>> {
        let agents = self
            .agents
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(agents
            .values()
            .filter(|a| a.session_id == session_id)
            .cloned()
            .collect())
    }

    async fn list_active_agents(&self, session_id: SessionId) -> RepositoryResult<Vec<Agent>> {
        let agents = self
            .agents
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(agents
            .values()
            .filter(|a| {
                a.session_id == session_id
                    && matches!(a.status, AgentStatus::Starting | AgentStatus::Active)
            })
            .cloned()
            .collect())
    }

    async fn count_active_agents(&self, session_id: SessionId) -> RepositoryResult<usize> {
        Ok(self.list_active_agents(session_id).await?.len())
    }

    async fn find_agents_by_role(
        &self,
        session_id: SessionId,
        role: AgentRole,
    ) -> RepositoryResult<Vec<Agent>> {
        let agents = self
            .agents
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(agents
            .values()
            .filter(|a| a.session_id == session_id && a.role == role)
            .cloned()
            .collect())
    }

    // === Task operations ===

    async fn create_task(&self, task: &Task) -> RepositoryResult<()> {
        self.tasks
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .insert(task.id, task.clone());
        Ok(())
    }

    async fn get_task(&self, id: TaskId) -> RepositoryResult<Task> {
        self.tasks
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .get(&id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "Task".into(),
                id: id.to_string(),
            })
    }

    async fn update_task_status(
        &self,
        id: TaskId,
        status: TaskStatus,
        summary: Option<&str>,
    ) -> RepositoryResult<()> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let task = tasks
            .get_mut(&id)
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "Task".into(),
                id: id.to_string(),
            })?;
        task.status = status;
        if let Some(s) = summary {
            task.summary = Some(s.to_string());
        }
        if matches!(status, TaskStatus::Completed | TaskStatus::Failed) {
            task.completed_at = Some(Utc::now());
        }
        Ok(())
    }

    async fn claim_task(&self, task_id: TaskId, agent_id: AgentId) -> RepositoryResult<()> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let task = tasks
            .get_mut(&task_id)
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "Task".into(),
                id: task_id.to_string(),
            })?;

        // Must be Pending to claim
        if task.status != TaskStatus::Pending {
            return Err(RepositoryError::Conflict(format!(
                "task {} is {:?}, not Pending",
                task_id, task.status
            )));
        }

        // If pre-assigned, only that agent can claim
        if let Some(assigned) = task.assigned_to
            && assigned != agent_id
        {
            return Err(RepositoryError::Conflict(format!(
                "task {} is assigned to {}, not {}",
                task_id, assigned, agent_id
            )));
        }

        task.status = TaskStatus::Claimed;
        task.assigned_to = Some(agent_id);
        Ok(())
    }

    async fn assign_task(&self, task_id: TaskId, agent_id: AgentId) -> RepositoryResult<()> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let task = tasks
            .get_mut(&task_id)
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "Task".into(),
                id: task_id.to_string(),
            })?;
        task.assigned_to = Some(agent_id);
        Ok(())
    }

    async fn list_tasks(
        &self,
        session_id: SessionId,
        status: Option<TaskStatus>,
    ) -> RepositoryResult<Vec<Task>> {
        let tasks = self
            .tasks
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(tasks
            .values()
            .filter(|t| t.session_id == session_id && status.is_none_or(|s| t.status == s))
            .cloned()
            .collect())
    }

    async fn get_subtasks(&self, parent_id: TaskId) -> RepositoryResult<Vec<Task>> {
        let tasks = self
            .tasks
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(tasks
            .values()
            .filter(|t| t.parent_task == Some(parent_id))
            .cloned()
            .collect())
    }

    // === Knowledge operations ===

    async fn create_knowledge(&self, knowledge: &Knowledge) -> RepositoryResult<()> {
        self.knowledge
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .insert(knowledge.id, knowledge.clone());
        Ok(())
    }

    async fn get_knowledge(&self, id: KnowledgeId) -> RepositoryResult<Knowledge> {
        self.knowledge
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .get(&id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "Knowledge".into(),
                id: id.to_string(),
            })
    }

    async fn get_task_knowledge(&self, task_id: TaskId) -> RepositoryResult<Vec<Knowledge>> {
        let knowledge = self
            .knowledge
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let mut result: Vec<_> = knowledge
            .values()
            .filter(|k| k.task_id == Some(task_id))
            .cloned()
            .collect();
        result.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(result)
    }

    async fn get_recent_knowledge(
        &self,
        session_id: SessionId,
        limit: usize,
    ) -> RepositoryResult<Vec<Knowledge>> {
        let knowledge = self
            .knowledge
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let mut result: Vec<_> = knowledge
            .values()
            .filter(|k| k.session_id == session_id)
            .cloned()
            .collect();
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        result.truncate(limit);
        Ok(result)
    }

    async fn search_knowledge(
        &self,
        _query_embedding: &[f32],
        limit: usize,
    ) -> RepositoryResult<Vec<(Knowledge, f32)>> {
        // In-memory doesn't support vector search; return recent entries with score 0.0
        let knowledge = self
            .knowledge
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let mut result: Vec<_> = knowledge.values().cloned().collect();
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        result.truncate(limit);
        Ok(result.into_iter().map(|k| (k, 0.0)).collect())
    }

    // === Direct Message operations ===

    async fn create_direct_message(&self, message: &DirectMessage) -> RepositoryResult<()> {
        self.direct_messages
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .push(message.clone());
        Ok(())
    }

    async fn get_direct_messages(
        &self,
        agent_id: AgentId,
        limit: usize,
    ) -> RepositoryResult<Vec<DirectMessage>> {
        let messages = self
            .direct_messages
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let mut result: Vec<_> = messages
            .iter()
            .filter(|m| m.to_agent == agent_id)
            .cloned()
            .collect();
        result.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        if result.len() > limit {
            result = result.split_off(result.len() - limit);
        }
        Ok(result)
    }

    async fn get_thread_messages(
        &self,
        task_id: TaskId,
        limit: usize,
    ) -> RepositoryResult<Vec<DirectMessage>> {
        let messages = self
            .direct_messages
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let mut result: Vec<_> = messages
            .iter()
            .filter(|m| m.task_id == Some(task_id))
            .cloned()
            .collect();
        result.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        result.truncate(limit);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KnowledgeKind, Priority};

    #[tokio::test]
    async fn test_session_crud() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);

        repo.create_session(&session).await.unwrap();
        let fetched = repo.get_session(session.id).await.unwrap();

        assert_eq!(fetched.id, session.id);
        assert_eq!(fetched.population_cap, 8);
    }

    #[tokio::test]
    async fn test_agent_lifecycle() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        let agent_id = agent.id;
        repo.create_agent(&agent).await.unwrap();

        // Initially pending, not counted as active
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 0);

        // Transition to Starting - now active
        repo.update_agent_status(agent_id, AgentStatus::Starting)
            .await
            .unwrap();
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 1);

        // Transition to Active - still active
        repo.update_agent_status(agent_id, AgentStatus::Active)
            .await
            .unwrap();
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 1);

        // Kill - no longer active
        repo.update_agent_status(agent_id, AgentStatus::Killed)
            .await
            .unwrap();
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_agent_simplified() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Architect, session.id);
        repo.create_agent(&agent).await.unwrap();

        let fetched = repo.get_agent(agent.id).await.unwrap();
        assert_eq!(fetched.role, AgentRole::Architect);
        assert!(fetched.current_task.is_none());
        assert!(!fetched.is_strategoi);
    }

    #[tokio::test]
    async fn test_agent_task_update() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        let task = Task::new("Build thing", "Details", Priority::Medium, session.id);
        repo.create_task(&task).await.unwrap();

        repo.update_agent_task(agent.id, Some(task.id))
            .await
            .unwrap();
        let fetched = repo.get_agent(agent.id).await.unwrap();
        assert_eq!(fetched.current_task, Some(task.id));

        repo.update_agent_task(agent.id, None).await.unwrap();
        let fetched = repo.get_agent(agent.id).await.unwrap();
        assert!(fetched.current_task.is_none());
    }

    #[tokio::test]
    async fn test_find_agents_by_role() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        repo.create_agent(&Agent::new(AgentRole::Developer, session.id))
            .await
            .unwrap();
        repo.create_agent(&Agent::new(AgentRole::Developer, session.id))
            .await
            .unwrap();
        repo.create_agent(&Agent::new(AgentRole::Tester, session.id))
            .await
            .unwrap();

        let devs = repo
            .find_agents_by_role(session.id, AgentRole::Developer)
            .await
            .unwrap();
        assert_eq!(devs.len(), 2);

        let testers = repo
            .find_agents_by_role(session.id, AgentRole::Tester)
            .await
            .unwrap();
        assert_eq!(testers.len(), 1);
    }

    #[tokio::test]
    async fn test_task_lifecycle() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        let task = Task::new("Design API", "REST API design", Priority::High, session.id);
        let task_id = task.id;
        repo.create_task(&task).await.unwrap();

        // Pending → Claimed
        repo.claim_task(task_id, agent.id).await.unwrap();
        let t = repo.get_task(task_id).await.unwrap();
        assert_eq!(t.status, TaskStatus::Claimed);
        assert_eq!(t.assigned_to, Some(agent.id));

        // Claimed → InProgress
        repo.update_task_status(task_id, TaskStatus::InProgress, None)
            .await
            .unwrap();
        let t = repo.get_task(task_id).await.unwrap();
        assert_eq!(t.status, TaskStatus::InProgress);

        // InProgress → Completed with summary
        repo.update_task_status(
            task_id,
            TaskStatus::Completed,
            Some("API designed with 5 endpoints"),
        )
        .await
        .unwrap();
        let t = repo.get_task(task_id).await.unwrap();
        assert_eq!(t.status, TaskStatus::Completed);
        assert_eq!(t.summary.as_deref(), Some("API designed with 5 endpoints"));
        assert!(t.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_task_claim_atomic() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent_a = Agent::new(AgentRole::Developer, session.id);
        let agent_b = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent_a).await.unwrap();
        repo.create_agent(&agent_b).await.unwrap();

        let task = Task::new(
            "Contested task",
            "Only one can claim",
            Priority::High,
            session.id,
        );
        let task_id = task.id;
        repo.create_task(&task).await.unwrap();

        // First claim succeeds
        repo.claim_task(task_id, agent_a.id).await.unwrap();

        // Second claim fails
        let result = repo.claim_task(task_id, agent_b.id).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RepositoryError::Conflict(_)));
    }

    #[tokio::test]
    async fn test_task_assign() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        let task = Task::new("Assigned work", "Details", Priority::Medium, session.id);
        let task_id = task.id;
        repo.create_task(&task).await.unwrap();

        repo.assign_task(task_id, agent.id).await.unwrap();
        let t = repo.get_task(task_id).await.unwrap();
        assert_eq!(t.assigned_to, Some(agent.id));
    }

    #[tokio::test]
    async fn test_task_assign_then_claim() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent_a = Agent::new(AgentRole::Developer, session.id);
        let agent_b = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent_a).await.unwrap();
        repo.create_agent(&agent_b).await.unwrap();

        let task = Task::new("Pre-assigned", "Details", Priority::Medium, session.id);
        let task_id = task.id;
        repo.create_task(&task).await.unwrap();

        // Strategoi assigns to agent_a
        repo.assign_task(task_id, agent_a.id).await.unwrap();

        // agent_b tries to claim - fails (pre-assigned to agent_a)
        let result = repo.claim_task(task_id, agent_b.id).await;
        assert!(result.is_err());

        // agent_a claims - succeeds
        repo.claim_task(task_id, agent_a.id).await.unwrap();
        let t = repo.get_task(task_id).await.unwrap();
        assert_eq!(t.status, TaskStatus::Claimed);
    }

    #[tokio::test]
    async fn test_subtask_listing() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let parent = Task::new("Epic", "Big task", Priority::High, session.id);
        let parent_id = parent.id;
        repo.create_task(&parent).await.unwrap();

        let child1 = Task::new("Sub 1", "First subtask", Priority::Medium, session.id)
            .with_parent(parent_id);
        let child2 = Task::new("Sub 2", "Second subtask", Priority::Medium, session.id)
            .with_parent(parent_id);
        repo.create_task(&child1).await.unwrap();
        repo.create_task(&child2).await.unwrap();

        let subtasks = repo.get_subtasks(parent_id).await.unwrap();
        assert_eq!(subtasks.len(), 2);
    }

    #[tokio::test]
    async fn test_list_tasks_by_status() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let t1 = Task::new("Pending", "Details", Priority::Medium, session.id);
        let t2 = Task::new("Also pending", "Details", Priority::Low, session.id);
        repo.create_task(&t1).await.unwrap();
        repo.create_task(&t2).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();
        repo.claim_task(t1.id, agent.id).await.unwrap();

        let pending = repo
            .list_tasks(session.id, Some(TaskStatus::Pending))
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);

        let claimed = repo
            .list_tasks(session.id, Some(TaskStatus::Claimed))
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);

        let all = repo.list_tasks(session.id, None).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_knowledge_crud() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        let k = Knowledge::new(
            "Found race condition in pool.rs",
            KnowledgeKind::Discovery,
            agent.id,
            session.id,
        );
        let k_id = k.id;
        repo.create_knowledge(&k).await.unwrap();

        let fetched = repo.get_knowledge(k_id).await.unwrap();
        assert_eq!(fetched.content, "Found race condition in pool.rs");
        assert_eq!(fetched.kind, KnowledgeKind::Discovery);
    }

    #[tokio::test]
    async fn test_knowledge_by_task() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        let task = Task::new("Implement auth", "Details", Priority::High, session.id);
        repo.create_task(&task).await.unwrap();

        let k1 = Knowledge::new("Using JWT", KnowledgeKind::Decision, agent.id, session.id)
            .with_task(task.id);
        let k2 = Knowledge::new(
            "bcrypt for passwords",
            KnowledgeKind::Decision,
            agent.id,
            session.id,
        )
        .with_task(task.id);
        let k3 = Knowledge::new(
            "Unrelated discovery",
            KnowledgeKind::Discovery,
            agent.id,
            session.id,
        );

        repo.create_knowledge(&k1).await.unwrap();
        repo.create_knowledge(&k2).await.unwrap();
        repo.create_knowledge(&k3).await.unwrap();

        let task_knowledge = repo.get_task_knowledge(task.id).await.unwrap();
        assert_eq!(task_knowledge.len(), 2);
    }

    #[tokio::test]
    async fn test_recent_knowledge() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        for i in 0..5 {
            let k = Knowledge::new(
                format!("Knowledge {i}"),
                KnowledgeKind::Activity,
                agent.id,
                session.id,
            );
            repo.create_knowledge(&k).await.unwrap();
        }

        let recent = repo.get_recent_knowledge(session.id, 3).await.unwrap();
        assert_eq!(recent.len(), 3);
    }

    #[tokio::test]
    async fn test_search_knowledge_stub() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new(AgentRole::Developer, session.id);
        repo.create_agent(&agent).await.unwrap();

        let k = Knowledge::new(
            "Some discovery",
            KnowledgeKind::Discovery,
            agent.id,
            session.id,
        );
        repo.create_knowledge(&k).await.unwrap();

        let results = repo.search_knowledge(&[0.1, 0.2, 0.3], 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, 0.0); // stub returns 0.0 similarity
    }

    #[tokio::test]
    async fn test_direct_message_send_receive() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let ba = Agent::new(AgentRole::BusinessAnalyst, session.id);
        let pm = Agent::new(AgentRole::ProductManager, session.id);
        repo.create_agent(&ba).await.unwrap();
        repo.create_agent(&pm).await.unwrap();

        let dm1 = DirectMessage::new(ba.id, pm.id, "What are the requirements?", session.id);
        let dm2 = DirectMessage::new(pm.id, ba.id, "Here are the requirements...", session.id);
        repo.create_direct_message(&dm1).await.unwrap();
        repo.create_direct_message(&dm2).await.unwrap();

        // PM received 1 message
        let pm_inbox = repo.get_direct_messages(pm.id, 10).await.unwrap();
        assert_eq!(pm_inbox.len(), 1);
        assert_eq!(pm_inbox[0].content, "What are the requirements?");

        // BA received 1 message
        let ba_inbox = repo.get_direct_messages(ba.id, 10).await.unwrap();
        assert_eq!(ba_inbox.len(), 1);
        assert_eq!(ba_inbox[0].content, "Here are the requirements...");
    }

    #[tokio::test]
    async fn test_thread_messages() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let ba = Agent::new(AgentRole::BusinessAnalyst, session.id);
        let pm = Agent::new(AgentRole::ProductManager, session.id);
        repo.create_agent(&ba).await.unwrap();
        repo.create_agent(&pm).await.unwrap();

        let task = Task::new(
            "PRD Interview",
            "Gather requirements",
            Priority::High,
            session.id,
        );
        repo.create_task(&task).await.unwrap();

        let dm1 = DirectMessage::new(ba.id, pm.id, "Question 1?", session.id).with_task(task.id);
        let dm2 = DirectMessage::new(pm.id, ba.id, "Answer 1.", session.id).with_task(task.id);
        let unrelated = DirectMessage::new(ba.id, pm.id, "Off-topic", session.id);

        repo.create_direct_message(&dm1).await.unwrap();
        repo.create_direct_message(&dm2).await.unwrap();
        repo.create_direct_message(&unrelated).await.unwrap();

        let thread = repo.get_thread_messages(task.id, 10).await.unwrap();
        assert_eq!(thread.len(), 2);
    }
}
