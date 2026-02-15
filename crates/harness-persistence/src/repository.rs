//! Repository trait for persistence operations.

use async_trait::async_trait;

use crate::{
    Agent, AgentId, AgentRole, AgentStatus, DirectMessage, Knowledge, Plan, PlanId, PlanStatus,
    Product, ProductId, ProductStatus, Project, ProjectId, ProjectStatus, Session, SessionId, Task,
    TaskId, TaskStatus,
};

/// Error type for repository operations.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// Entity not found.
    #[error("{entity_type} with id {id} not found")]
    NotFound { entity_type: String, id: String },

    /// Conflict (e.g., task already claimed).
    #[error("conflict: {0}")]
    Conflict(String),

    /// Database error.
    #[error("database error: {0}")]
    Database(String),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<aletheiadb::Error> for RepositoryError {
    fn from(e: aletheiadb::Error) -> Self {
        Self::Database(e.to_string())
    }
}

/// Result type for repository operations.
pub type RepositoryResult<T> = Result<T, RepositoryError>;

/// Repository for Harness v2 hive mind entities.
///
/// Abstracts over AletheiaDB for production and in-memory for testing.
#[async_trait]
pub trait Repository: Send + Sync {
    // === Session operations ===

    /// Create a new session.
    async fn create_session(&self, session: &Session) -> RepositoryResult<()>;

    /// Get a session by ID.
    async fn get_session(&self, id: SessionId) -> RepositoryResult<Session>;

    /// Set the agent ID for a session (for MCP client persistence).
    async fn set_session_agent(
        &self,
        session_id: SessionId,
        agent_id: AgentId,
    ) -> RepositoryResult<()>;

    // === Agent operations ===

    /// Create a new agent.
    async fn create_agent(&self, agent: &Agent) -> RepositoryResult<()>;

    /// Get an agent by ID.
    async fn get_agent(&self, id: AgentId) -> RepositoryResult<Agent>;

    /// Update an agent's status.
    async fn update_agent_status(&self, id: AgentId, status: AgentStatus) -> RepositoryResult<()>;

    /// Update which task an agent is working on.
    async fn update_agent_task(&self, id: AgentId, task: Option<TaskId>) -> RepositoryResult<()>;

    /// List all agents in a session.
    async fn list_agents(&self, session_id: SessionId) -> RepositoryResult<Vec<Agent>>;

    /// List active agents (Starting or Active status) in a session.
    async fn list_active_agents(&self, session_id: SessionId) -> RepositoryResult<Vec<Agent>>;

    /// Count active agents in a session.
    async fn count_active_agents(&self, session_id: SessionId) -> RepositoryResult<usize>;

    /// Find agents by role in a session.
    async fn find_agents_by_role(
        &self,
        session_id: SessionId,
        role: AgentRole,
    ) -> RepositoryResult<Vec<Agent>>;

    // === Task operations ===

    /// Create a new task.
    async fn create_task(&self, task: &Task) -> RepositoryResult<()>;

    /// Get a task by ID.
    async fn get_task(&self, id: TaskId) -> RepositoryResult<Task>;

    /// Update a task's status, optionally setting a completion summary.
    async fn update_task_status(
        &self,
        id: TaskId,
        status: TaskStatus,
        summary: Option<&str>,
    ) -> RepositoryResult<()>;

    /// Atomically claim a pending task for an agent.
    /// Fails with `Conflict` if the task is not pending or already claimed by another agent.
    async fn claim_task(&self, task_id: TaskId, agent_id: AgentId) -> RepositoryResult<()>;

    /// Assign a task to an agent (Strategoi assigns work).
    async fn assign_task(&self, task_id: TaskId, agent_id: AgentId) -> RepositoryResult<()>;

    /// Clear any explicit assignment from a task.
    async fn clear_task_assignment(&self, task_id: TaskId) -> RepositoryResult<()>;

    /// List tasks in a session, optionally filtered by status.
    async fn list_tasks(
        &self,
        session_id: SessionId,
        status: Option<TaskStatus>,
    ) -> RepositoryResult<Vec<Task>>;

    /// Get subtasks of a parent task.
    async fn get_subtasks(&self, parent_id: TaskId) -> RepositoryResult<Vec<Task>>;

    /// Add a blocking dependency: task_id blocks blocked_task_id from starting.
    async fn add_task_dependency(
        &self,
        task_id: TaskId,
        blocked_task_id: TaskId,
    ) -> RepositoryResult<()>;

    /// Remove a blocking dependency between tasks.
    async fn remove_task_dependency(
        &self,
        task_id: TaskId,
        blocked_task_id: TaskId,
    ) -> RepositoryResult<()>;

    /// Get tasks that are blocking this task (must complete before this task can start).
    async fn get_blocking_tasks(&self, task_id: TaskId) -> RepositoryResult<Vec<Task>>;

    /// Get tasks that this task is blocking (waiting for this task to complete).
    async fn get_blocked_tasks(&self, task_id: TaskId) -> RepositoryResult<Vec<Task>>;

    /// Get the complete version history of a task.
    /// Returns all versions in chronological order (oldest first).
    async fn get_task_history(&self, task_id: TaskId) -> RepositoryResult<Vec<Task>>;

    /// Search tasks by vector similarity using semantic embeddings.
    /// Returns (task, similarity_score) pairs ordered by descending similarity.
    /// Optionally filter by task status.
    async fn search_tasks(
        &self,
        query_embedding: &[f32],
        limit: usize,
        status: Option<TaskStatus>,
    ) -> RepositoryResult<Vec<(Task, f32)>>;

    /// Get all top-level tasks (tasks with no parent) in a session.
    async fn get_root_tasks(&self, session_id: SessionId) -> RepositoryResult<Vec<Task>>;

    // === Knowledge operations ===

    /// Create a knowledge entry.
    async fn create_knowledge(&self, knowledge: &Knowledge) -> RepositoryResult<()>;

    /// Get a knowledge entry by ID.
    async fn get_knowledge(&self, id: crate::KnowledgeId) -> RepositoryResult<Knowledge>;

    /// Get all knowledge related to a task.
    async fn get_task_knowledge(&self, task_id: TaskId) -> RepositoryResult<Vec<Knowledge>>;

    /// Get recent knowledge entries in a session.
    async fn get_recent_knowledge(
        &self,
        session_id: SessionId,
        limit: usize,
    ) -> RepositoryResult<Vec<Knowledge>>;

    /// Search knowledge by vector similarity.
    /// Returns (knowledge, similarity_score) pairs ordered by descending similarity.
    /// Falls back to recent-first ordering when vector search is unavailable.
    async fn search_knowledge(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> RepositoryResult<Vec<(Knowledge, f32)>>;

    // === Direct Message operations ===

    /// Send a direct message between agents.
    async fn create_direct_message(&self, message: &DirectMessage) -> RepositoryResult<()>;

    /// Get direct messages for an agent (received), ordered by time ascending.
    async fn get_direct_messages(
        &self,
        agent_id: AgentId,
        limit: usize,
    ) -> RepositoryResult<Vec<DirectMessage>>;

    /// Get direct messages in a task thread between any agents.
    async fn get_thread_messages(
        &self,
        task_id: TaskId,
        limit: usize,
    ) -> RepositoryResult<Vec<DirectMessage>>;

    // === Product operations ===

    /// Create a new product.
    async fn create_product(&self, product: &Product) -> RepositoryResult<()>;

    /// Get a product by ID.
    async fn get_product(&self, id: ProductId) -> RepositoryResult<Product>;

    /// List products in a session, optionally filtered by status.
    async fn list_products(
        &self,
        session_id: SessionId,
        status: Option<ProductStatus>,
    ) -> RepositoryResult<Vec<Product>>;

    // === Project operations ===

    /// Create a new project.
    async fn create_project(&self, project: &Project) -> RepositoryResult<()>;

    /// Get a project by ID.
    async fn get_project(&self, id: ProjectId) -> RepositoryResult<Project>;

    /// List projects in a session, optionally filtered by product and/or status.
    async fn list_projects(
        &self,
        session_id: SessionId,
        product_id: Option<ProductId>,
        status: Option<ProjectStatus>,
    ) -> RepositoryResult<Vec<Project>>;

    // === Plan operations ===

    /// Create a new plan.
    async fn create_plan(&self, plan: &Plan) -> RepositoryResult<()>;

    /// Get a plan by ID.
    async fn get_plan(&self, id: PlanId) -> RepositoryResult<Plan>;

    /// List plans in a session, optionally filtered by project and/or status.
    async fn list_plans(
        &self,
        session_id: SessionId,
        project_id: Option<ProjectId>,
        status: Option<PlanStatus>,
    ) -> RepositoryResult<Vec<Plan>>;

    // === Experimental features access ===

    /// Get raw AletheiaDB instance for experimental features (nova feature flag).
    fn get_raw_db(&self) -> RepositoryResult<&aletheiadb::AletheiaDB>;

    /// Get the AletheiaDB NodeId for a task.
    fn get_node_id_for_task(
        &self,
        task_id: TaskId,
    ) -> RepositoryResult<aletheiadb::core::id::NodeId>;

    /// Get the AletheiaDB NodeId for a knowledge entry.
    fn get_node_id_for_knowledge(
        &self,
        knowledge_id: crate::KnowledgeId,
    ) -> RepositoryResult<aletheiadb::core::id::NodeId>;

    /// Get the AletheiaDB NodeId for an agent.
    fn get_node_id_for_agent(
        &self,
        agent_id: AgentId,
    ) -> RepositoryResult<aletheiadb::core::id::NodeId>;

    // === Trajectory operations ===

    /// Create a trajectory event in persistent storage.
    /// Stores the raw event data (without materialized steps for efficiency).
    async fn create_trajectory_event(
        &self,
        event: &crate::trajectory::RawEvent,
    ) -> RepositoryResult<()>;

    /// Get all trajectory events for a session.
    /// Returns raw events (steps are materialized on-demand by TrajectoryRecorder).
    async fn get_trajectory_events(
        &self,
        session_id: SessionId,
    ) -> RepositoryResult<Vec<crate::trajectory::RawEvent>>;
}
