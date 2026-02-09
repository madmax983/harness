//! Domain entities for Harness v2 - Hive Mind.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    AgentId, AgentRole, DirectMessageId, KnowledgeId, KnowledgeKind, Priority, SessionId, TaskId,
    TaskStatus,
};

/// Status of an agent in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Spawn requested, process not yet started.
    Pending,
    /// Process started, waiting for first action.
    Starting,
    /// Agent is active and working.
    Active,
    /// Agent has no current task, waiting for assignment.
    Idle,
    /// Agent finished naturally.
    Finished,
    /// Agent was killed by user.
    Killed,
    /// Agent process crashed.
    Crashed,
}

/// An agent (Claude instance) in the hive mind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Unique identifier.
    pub id: AgentId,
    /// BMAD role this agent fulfills.
    pub role: AgentRole,
    /// Current status.
    pub status: AgentStatus,
    /// Task this agent is currently working on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_task: Option<TaskId>,
    /// Whether this agent is the Strategoi (coordinator).
    pub is_strategoi: bool,
    /// Session this agent belongs to.
    pub session_id: SessionId,
    /// When the agent was created.
    pub created_at: DateTime<Utc>,
}

impl Agent {
    /// Create a new agent with the given BMAD role.
    pub fn new(role: AgentRole, session_id: SessionId) -> Self {
        let is_strategoi = role == AgentRole::Strategoi;
        Self {
            id: AgentId::new(),
            role,
            status: AgentStatus::Pending,
            current_task: None,
            is_strategoi,
            session_id,
            created_at: Utc::now(),
        }
    }

    /// Create a new agent with a specific ID (for restoring from persistence).
    pub fn with_id(mut self, id: AgentId) -> Self {
        self.id = id;
        self
    }
}

/// A unit of work assigned to agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier.
    pub id: TaskId,
    /// Short title describing the task.
    pub title: String,
    /// Detailed description of what needs to be done.
    pub description: String,
    /// Current status in the lifecycle.
    pub status: TaskStatus,
    /// Priority level.
    pub priority: Priority,
    /// Agent assigned to this task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to: Option<AgentId>,
    /// Agent who created this task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<AgentId>,
    /// Parent task for subtask relationships.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_task: Option<TaskId>,
    /// When the task was created.
    pub created_at: DateTime<Utc>,
    /// When the task was completed (or failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Completion summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Session this task belongs to.
    pub session_id: SessionId,
}

impl Task {
    /// Create a new pending task.
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        priority: Priority,
        session_id: SessionId,
    ) -> Self {
        Self {
            id: TaskId::new(),
            title: title.into(),
            description: description.into(),
            status: TaskStatus::Pending,
            priority,
            assigned_to: None,
            created_by: None,
            parent_task: None,
            created_at: Utc::now(),
            completed_at: None,
            summary: None,
            session_id,
        }
    }

    /// Set who created this task.
    pub fn with_created_by(mut self, creator: AgentId) -> Self {
        self.created_by = Some(creator);
        self
    }

    /// Set this as a subtask of another task.
    pub fn with_parent(mut self, parent: TaskId) -> Self {
        self.parent_task = Some(parent);
        self
    }
}

/// A piece of shared knowledge in the hive mind with optional vector embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Knowledge {
    /// Unique identifier.
    pub id: KnowledgeId,
    /// The knowledge content.
    pub content: String,
    /// What kind of knowledge this is.
    pub kind: KnowledgeKind,
    /// Agent who shared this knowledge.
    pub author_id: AgentId,
    /// Task this knowledge relates to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// Vector embedding for semantic search.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// When the knowledge was shared.
    pub created_at: DateTime<Utc>,
    /// Session this knowledge belongs to.
    pub session_id: SessionId,
}

impl Knowledge {
    /// Create a new knowledge entry.
    pub fn new(
        content: impl Into<String>,
        kind: KnowledgeKind,
        author_id: AgentId,
        session_id: SessionId,
    ) -> Self {
        Self {
            id: KnowledgeId::new(),
            content: content.into(),
            kind,
            author_id,
            task_id: None,
            embedding: None,
            created_at: Utc::now(),
            session_id,
        }
    }

    /// Link this knowledge to a task.
    pub fn with_task(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }

    /// Set the vector embedding.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// A direct message between two agents (for BMAD interviews, coordination).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectMessage {
    /// Unique identifier.
    pub id: DirectMessageId,
    /// Sending agent.
    pub from_agent: AgentId,
    /// Receiving agent.
    pub to_agent: AgentId,
    /// Message content.
    pub content: String,
    /// Task context for this conversation thread.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// When the message was sent.
    pub created_at: DateTime<Utc>,
    /// Session this message belongs to.
    pub session_id: SessionId,
}

impl DirectMessage {
    /// Create a new direct message.
    pub fn new(
        from_agent: AgentId,
        to_agent: AgentId,
        content: impl Into<String>,
        session_id: SessionId,
    ) -> Self {
        Self {
            id: DirectMessageId::new(),
            from_agent,
            to_agent,
            content: content.into(),
            task_id: None,
            created_at: Utc::now(),
            session_id,
        }
    }

    /// Associate this message with a task thread.
    pub fn with_task(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }
}

/// A session grouping agents, tasks, and knowledge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique identifier.
    pub id: SessionId,
    /// When the session started.
    pub started_at: DateTime<Utc>,
    /// Maximum number of agents allowed.
    pub population_cap: usize,
}

impl Session {
    /// Create a new session with the given population cap.
    pub fn new(population_cap: usize) -> Self {
        Self {
            id: SessionId::new(),
            started_at: Utc::now(),
            population_cap,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_starts_pending() {
        let session = Session::new(8);
        let task = Task::new(
            "Design API",
            "Design the REST API",
            Priority::High,
            session.id,
        );
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.assigned_to.is_none());
        assert!(task.completed_at.is_none());
        assert!(task.summary.is_none());
    }

    #[test]
    fn task_with_parent() {
        let session = Session::new(8);
        let parent = Task::new("Epic", "Big task", Priority::High, session.id);
        let child =
            Task::new("Subtask", "Small task", Priority::Medium, session.id).with_parent(parent.id);
        assert_eq!(child.parent_task, Some(parent.id));
    }

    #[test]
    fn task_with_creator() {
        let session = Session::new(8);
        let agent = Agent::new(AgentRole::Strategoi, session.id);
        let task = Task::new("Do thing", "Details", Priority::Medium, session.id)
            .with_created_by(agent.id);
        assert_eq!(task.created_by, Some(agent.id));
    }

    #[test]
    fn knowledge_creation_with_kind() {
        let session = Session::new(8);
        let agent = Agent::new(AgentRole::Developer, session.id);
        let knowledge = Knowledge::new(
            "Found race condition in pool.rs",
            KnowledgeKind::Discovery,
            agent.id,
            session.id,
        );
        assert_eq!(knowledge.kind, KnowledgeKind::Discovery);
        assert!(knowledge.task_id.is_none());
        assert!(knowledge.embedding.is_none());
    }

    #[test]
    fn knowledge_with_task_and_embedding() {
        let session = Session::new(8);
        let agent = Agent::new(AgentRole::Architect, session.id);
        let task = Task::new("Design DB", "Details", Priority::High, session.id);
        let embedding = vec![0.1, 0.2, 0.3];
        let knowledge = Knowledge::new(
            "Using B-tree index",
            KnowledgeKind::Decision,
            agent.id,
            session.id,
        )
        .with_task(task.id)
        .with_embedding(embedding.clone());
        assert_eq!(knowledge.task_id, Some(task.id));
        assert_eq!(knowledge.embedding, Some(embedding));
    }

    #[test]
    fn agent_without_subscriptions_or_system_prompt() {
        let session = Session::new(8);
        let agent = Agent::new(AgentRole::Developer, session.id);
        // v2 Agent has no subscriptions or system_prompt fields
        assert_eq!(agent.status, AgentStatus::Pending);
        assert!(agent.current_task.is_none());
        assert!(!agent.is_strategoi);
    }

    #[test]
    fn agent_strategoi_flag_auto_set() {
        let session = Session::new(8);
        let strategoi = Agent::new(AgentRole::Strategoi, session.id);
        assert!(strategoi.is_strategoi);
        assert_eq!(strategoi.role, AgentRole::Strategoi);
    }

    #[test]
    fn direct_message_creation() {
        let session = Session::new(8);
        let ba = Agent::new(AgentRole::BusinessAnalyst, session.id);
        let pm = Agent::new(AgentRole::ProductManager, session.id);
        let dm = DirectMessage::new(ba.id, pm.id, "What are the requirements?", session.id);
        assert_eq!(dm.from_agent, ba.id);
        assert_eq!(dm.to_agent, pm.id);
        assert!(dm.task_id.is_none());
    }

    #[test]
    fn direct_message_with_task_thread() {
        let session = Session::new(8);
        let ba = Agent::new(AgentRole::BusinessAnalyst, session.id);
        let pm = Agent::new(AgentRole::ProductManager, session.id);
        let task = Task::new(
            "Gather reqs",
            "Interview stakeholders",
            Priority::High,
            session.id,
        );
        let dm =
            DirectMessage::new(ba.id, pm.id, "Question about auth", session.id).with_task(task.id);
        assert_eq!(dm.task_id, Some(task.id));
    }
}
