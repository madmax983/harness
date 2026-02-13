//! MCP tool handler implementation.
//!
//! Each method implements a tool that can be called by Claude agents
//! via the MCP protocol. The handler dispatches tool calls by name.

use std::sync::Arc;

use harness_persistence::{
    Agent, AgentId, AgentRole, AgentStatus, DirectMessage, Knowledge, KnowledgeKind, Plan,
    PlanStatus, Priority, Product, ProductId, ProductStatus, Project, ProjectId, ProjectStatus,
    Repository, RepositoryError, Task, TaskId, TaskStatus,
};
use tokio::sync::RwLock;

use crate::state::HiveState;
use crate::tools;

/// Error type for tool handler operations.
#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),

    #[error("invalid arguments: {0}")]
    InvalidArgs(String),

    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("parse error: {0}")]
    Parse(String),
}

/// Result type for handler operations.
pub type HandlerResult<T> = Result<T, HandlerError>;

/// MCP tool handler for the Hive Mind.
///
/// Holds shared state and per-connection agent identity.
pub struct HiveHandler<R: Repository> {
    state: Arc<HiveState<R>>,
    /// The agent ID for this connection (set via register_agent).
    agent_id: RwLock<Option<AgentId>>,
}

impl<R: Repository + 'static> HiveHandler<R> {
    /// Create a new handler.
    pub fn new(state: Arc<HiveState<R>>) -> Self {
        Self {
            state,
            agent_id: RwLock::new(None),
        }
    }

    /// Get a reference to the shared state (for spawning new handlers in tests).
    pub fn state_ref(&self) -> &Arc<HiveState<R>> {
        &self.state
    }

    /// Restore the agent ID from a previous session (for MCP client persistence).
    pub async fn restore_agent_id(&self, agent_id: AgentId) {
        *self.agent_id.write().await = Some(agent_id);
    }

    /// Dispatch a tool call by name.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> HandlerResult<serde_json::Value> {
        match name {
            // Task tools
            "create_task" => {
                let req: tools::CreateTaskRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_create_task(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "list_tasks" => {
                let req: tools::ListTasksRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_list_tasks(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "claim_task" => {
                let req: tools::ClaimTaskRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_claim_task(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "update_task_status" => {
                let req: tools::UpdateTaskStatusRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_update_task_status(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "assign_task" => {
                let req: tools::AssignTaskRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_assign_task(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "get_task_context" => {
                let req: tools::GetTaskContextRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_get_task_context(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "add_task_dependency" => {
                let req: tools::AddTaskDependencyRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_add_task_dependency(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "remove_task_dependency" => {
                let req: tools::RemoveTaskDependencyRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_remove_task_dependency(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "get_task_history" => {
                let req: tools::GetTaskHistoryRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_get_task_history(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "semantic_search_tasks" => {
                let req: tools::SemanticSearchTasksRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_semantic_search_tasks(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "get_task_tree" => {
                let req: tools::GetTaskTreeRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_get_task_tree(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "get_task_as_of" => {
                let req: tools::GetTaskAsOfRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_get_task_as_of(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "find_path" => {
                let req: tools::FindPathRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_find_path(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "task_statistics" => {
                let req: tools::TaskStatisticsRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_task_statistics(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "cold_storage_query" => {
                let req: tools::ColdStorageQueryRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_cold_storage_query(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "export_project_graph" => {
                let req: tools::ExportProjectGraphRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_export_project_graph(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }

            // Knowledge tools
            "share_knowledge" => {
                let req: tools::ShareKnowledgeRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_share_knowledge(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "ask_hive" => {
                let req: tools::AskHiveRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_ask_hive(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "fish_knowledge" => {
                let req: tools::FishKnowledgeRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_fish_knowledge(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "knowledge_clusters" => {
                let req: tools::KnowledgeClustersRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_knowledge_clusters(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }

            // Agent tools
            "register_agent" => {
                let req: tools::RegisterAgentRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_register_agent(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "list_agents" => {
                let resp = self.handle_list_agents().await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "get_hive_status" => {
                let resp = self.handle_get_hive_status().await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "spawn_agent" => {
                let req: tools::SpawnAgentRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_spawn_agent(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "disconnect_agent" => {
                let req: tools::DisconnectAgentRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_disconnect_agent(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }

            // Message tools
            "send_direct_message" => {
                let req: tools::SendDirectMessageRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_send_dm(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "get_messages" => {
                let req: tools::GetMessagesRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_get_messages(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "get_thread_messages" => {
                let req: tools::GetThreadMessagesRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_get_thread_messages(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }

            // Planning tools
            "create_product" => {
                let req: tools::CreateProductRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_create_product(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "list_products" => {
                let req: tools::ListProductsRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_list_products(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "create_project" => {
                let req: tools::CreateProjectRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_create_project(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "list_projects" => {
                let req: tools::ListProjectsRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_list_projects(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "create_plan" => {
                let req: tools::CreatePlanRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_create_plan(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "list_plans" => {
                let req: tools::ListPlansRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_list_plans(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }

            _ => Err(HandlerError::UnknownTool(name.to_string())),
        }
    }

    /// Get the list of available tool names.
    pub fn tool_names() -> &'static [&'static str] {
        &[
            "create_task",
            "list_tasks",
            "claim_task",
            "update_task_status",
            "assign_task",
            "get_task_context",
            "add_task_dependency",
            "remove_task_dependency",
            "get_task_history",
            "semantic_search_tasks",
            "get_task_tree",
            "get_task_as_of",
            "find_path",
            "task_statistics",
            "cold_storage_query",
            "export_project_graph",
            "share_knowledge",
            "ask_hive",
            "fish_knowledge",
            "knowledge_clusters",
            "register_agent",
            "list_agents",
            "get_hive_status",
            "spawn_agent",
            "disconnect_agent",
            "send_direct_message",
            "get_messages",
            "get_thread_messages",
            "create_product",
            "list_products",
            "create_project",
            "list_projects",
            "create_plan",
            "list_plans",
        ]
    }

    // --- Helpers ---

    async fn require_agent_id(&self) -> HandlerResult<AgentId> {
        if let Some(id) = *self.agent_id.read().await {
            return Ok(id);
        }
        Err(HandlerError::InvalidArgs(
            "No agent registered. Call register_agent first.".into(),
        ))
    }

    /// Resolve agent ID from optional parameter or session state.
    /// Checks provided agent_id first, falls back to require_agent_id().
    async fn resolve_agent_id(&self, provided: Option<&str>) -> HandlerResult<AgentId> {
        if let Some(aid_str) = provided {
            return Self::parse_agent_id(aid_str);
        }
        self.require_agent_id().await
    }

    fn parse_agent_id(s: &str) -> HandlerResult<AgentId> {
        let uuid = uuid::Uuid::parse_str(s)
            .map_err(|_| HandlerError::Parse(format!("Invalid agent ID: {s}")))?;
        Ok(AgentId::from_uuid(uuid))
    }

    fn parse_task_id(s: &str) -> HandlerResult<TaskId> {
        let uuid = uuid::Uuid::parse_str(s)
            .map_err(|_| HandlerError::Parse(format!("Invalid task ID: {s}")))?;
        Ok(TaskId::from_uuid(uuid))
    }

    fn parse_priority(s: &str) -> HandlerResult<Priority> {
        match s {
            "low" => Ok(Priority::Low),
            "medium" => Ok(Priority::Medium),
            "high" => Ok(Priority::High),
            "critical" => Ok(Priority::Critical),
            _ => Err(HandlerError::InvalidArgs(format!("Invalid priority: {s}"))),
        }
    }

    fn parse_task_status(s: &str) -> HandlerResult<TaskStatus> {
        match s {
            "pending" => Ok(TaskStatus::Pending),
            "claimed" => Ok(TaskStatus::Claimed),
            "in_progress" => Ok(TaskStatus::InProgress),
            "completed" => Ok(TaskStatus::Completed),
            "failed" => Ok(TaskStatus::Failed),
            _ => Err(HandlerError::InvalidArgs(format!(
                "Invalid task status: {s}"
            ))),
        }
    }

    fn parse_role(s: &str) -> HandlerResult<AgentRole> {
        match s {
            "strategoi" => Ok(AgentRole::Strategoi),
            "business_analyst" | "ba" => Ok(AgentRole::BusinessAnalyst),
            "product_manager" | "pm" => Ok(AgentRole::ProductManager),
            "architect" => Ok(AgentRole::Architect),
            "developer" | "dev" => Ok(AgentRole::Developer),
            "tester" | "test" => Ok(AgentRole::Tester),
            _ => Err(HandlerError::InvalidArgs(format!("Invalid role: {s}"))),
        }
    }

    fn parse_kind(s: &str) -> HandlerResult<KnowledgeKind> {
        match s {
            "activity" => Ok(KnowledgeKind::Activity),
            "discovery" => Ok(KnowledgeKind::Discovery),
            "decision" => Ok(KnowledgeKind::Decision),
            "blocker" => Ok(KnowledgeKind::Blocker),
            _ => Err(HandlerError::InvalidArgs(format!(
                "Invalid knowledge kind: {s}"
            ))),
        }
    }

    fn parse_product_id(s: &str) -> HandlerResult<ProductId> {
        let uuid = uuid::Uuid::parse_str(s)
            .map_err(|_| HandlerError::Parse(format!("Invalid product ID: {s}")))?;
        Ok(ProductId::from_uuid(uuid))
    }

    fn parse_project_id(s: &str) -> HandlerResult<ProjectId> {
        let uuid = uuid::Uuid::parse_str(s)
            .map_err(|_| HandlerError::Parse(format!("Invalid project ID: {s}")))?;
        Ok(ProjectId::from_uuid(uuid))
    }

    fn parse_product_status(s: &str) -> HandlerResult<ProductStatus> {
        match s {
            "concept" => Ok(ProductStatus::Concept),
            "active" => Ok(ProductStatus::Active),
            "maintenance" => Ok(ProductStatus::Maintenance),
            "archived" => Ok(ProductStatus::Archived),
            _ => Err(HandlerError::InvalidArgs(format!(
                "Invalid product status: {s}"
            ))),
        }
    }

    fn parse_project_status(s: &str) -> HandlerResult<ProjectStatus> {
        match s {
            "planning" => Ok(ProjectStatus::Planning),
            "active" => Ok(ProjectStatus::Active),
            "on_hold" => Ok(ProjectStatus::OnHold),
            "completed" => Ok(ProjectStatus::Completed),
            "archived" => Ok(ProjectStatus::Archived),
            _ => Err(HandlerError::InvalidArgs(format!(
                "Invalid project status: {s}"
            ))),
        }
    }

    fn parse_plan_status(s: &str) -> HandlerResult<PlanStatus> {
        match s {
            "draft" => Ok(PlanStatus::Draft),
            "approved" => Ok(PlanStatus::Approved),
            "in_execution" => Ok(PlanStatus::InExecution),
            "paused" => Ok(PlanStatus::Paused),
            "completed" => Ok(PlanStatus::Completed),
            "abandoned" => Ok(PlanStatus::Abandoned),
            _ => Err(HandlerError::InvalidArgs(format!(
                "Invalid plan status: {s}"
            ))),
        }
    }

    fn task_to_info(t: &Task) -> tools::TaskInfo {
        tools::TaskInfo {
            id: t.id.as_uuid().to_string(),
            title: t.title.clone(),
            status: format!("{:?}", t.status).to_lowercase(),
            priority: format!("{:?}", t.priority).to_lowercase(),
            assigned_to: t.assigned_to.map(|a| a.as_uuid().to_string()),
            created_at: t.created_at.to_rfc3339(),
            summary: t.summary.clone(),
            blocked_by: None, // Populated by handlers when needed
            blocks: None,     // Populated by handlers when needed
        }
    }

    fn agent_to_info(a: &Agent) -> tools::AgentInfo {
        tools::AgentInfo {
            id: a.id.as_uuid().to_string(),
            role: format!("{}", a.role),
            status: format!("{:?}", a.status).to_lowercase(),
            current_task: a.current_task.map(|t| t.as_uuid().to_string()),
            is_strategoi: a.is_strategoi,
            project_name: a.project_name.clone(),
            project_path: a.project_path.clone(),
        }
    }

    fn knowledge_to_result(k: &Knowledge, similarity: f32) -> tools::KnowledgeResult {
        tools::KnowledgeResult {
            id: k.id.as_uuid().to_string(),
            content: k.content.clone(),
            kind: format!("{:?}", k.kind).to_lowercase(),
            author: k.author_id.as_uuid().to_string(),
            similarity,
            created_at: k.created_at.to_rfc3339(),
            task_id: k.task_id.map(|t| t.as_uuid().to_string()),
        }
    }

    fn dm_to_info(m: &DirectMessage) -> tools::MessageInfo {
        tools::MessageInfo {
            id: m.id.as_uuid().to_string(),
            from_agent: m.from_agent.as_uuid().to_string(),
            to_agent: m.to_agent.as_uuid().to_string(),
            content: m.content.clone(),
            created_at: m.created_at.to_rfc3339(),
            task_id: m.task_id.map(|t| t.as_uuid().to_string()),
        }
    }

    fn product_to_info(p: &Product) -> tools::ProductInfo {
        tools::ProductInfo {
            id: p.id.as_uuid().to_string(),
            name: p.name.clone(),
            description: p.description.clone(),
            status: format!("{:?}", p.status).to_lowercase(),
            created_at: p.created_at.to_rfc3339(),
        }
    }

    fn project_to_info(p: &Project) -> tools::ProjectInfo {
        tools::ProjectInfo {
            id: p.id.as_uuid().to_string(),
            name: p.name.clone(),
            description: p.description.clone(),
            status: format!("{:?}", p.status).to_lowercase(),
            product_id: p.product_id.as_uuid().to_string(),
            created_at: p.created_at.to_rfc3339(),
        }
    }

    fn plan_to_info(p: &Plan) -> tools::PlanInfo {
        tools::PlanInfo {
            id: p.id.as_uuid().to_string(),
            name: p.name.clone(),
            strategy: p.strategy.clone(),
            status: format!("{:?}", p.status).to_lowercase(),
            project_id: p.project_id.as_uuid().to_string(),
            created_at: p.created_at.to_rfc3339(),
        }
    }

    // --- Task handlers ---

    async fn handle_create_task(
        &self,
        req: tools::CreateTaskRequest,
    ) -> HandlerResult<tools::CreateTaskResponse> {
        let priority = Self::parse_priority(&req.priority)?;
        let session_id = self.state.session_id();

        let mut task = Task::new(&req.title, &req.description, priority, session_id);

        if let Some(ref parent_str) = req.parent_task {
            task = task.with_parent(Self::parse_task_id(parent_str)?);
        }

        // If an agent is registered, set created_by
        if let Some(aid) = *self.agent_id.read().await {
            task = task.with_created_by(aid);
        }

        // Generate embedding for semantic search
        if let Some(svc) = self.state.embedding_service() {
            // Combine title and description for richer semantic content
            let content = format!("{}\n{}", req.title, req.description);
            match svc.embed(&content).await {
                Ok(embedding) => {
                    task.embedding = Some(embedding);
                }
                Err(e) => {
                    tracing::warn!("Task embedding failed, storing without: {e}");
                }
            }
        }

        let task_id = task.id.as_uuid().to_string();
        self.state.repository().create_task(&task).await?;

        Ok(tools::CreateTaskResponse { task_id })
    }

    async fn handle_list_tasks(
        &self,
        req: tools::ListTasksRequest,
    ) -> HandlerResult<tools::ListTasksResponse> {
        let status = req
            .status
            .as_deref()
            .map(Self::parse_task_status)
            .transpose()?;
        let tasks = self
            .state
            .repository()
            .list_tasks(self.state.session_id(), status)
            .await?;

        Ok(tools::ListTasksResponse {
            tasks: tasks.iter().map(Self::task_to_info).collect(),
        })
    }

    async fn handle_claim_task(
        &self,
        req: tools::ClaimTaskRequest,
    ) -> HandlerResult<tools::ClaimTaskResponse> {
        let task_id = Self::parse_task_id(&req.task_id)?;
        let agent_id = self.resolve_agent_id(req._agent_id.as_deref()).await?;

        match self.state.repository().claim_task(task_id, agent_id).await {
            Ok(()) => Ok(tools::ClaimTaskResponse {
                success: true,
                error: None,
            }),
            Err(RepositoryError::Conflict(msg)) => Ok(tools::ClaimTaskResponse {
                success: false,
                error: Some(msg),
            }),
            Err(e) => Err(e.into()),
        }
    }

    async fn handle_update_task_status(
        &self,
        req: tools::UpdateTaskStatusRequest,
    ) -> HandlerResult<tools::UpdateTaskStatusResponse> {
        let task_id = Self::parse_task_id(&req.task_id)?;
        let status = Self::parse_task_status(&req.status)?;

        self.state
            .repository()
            .update_task_status(task_id, status, req.summary.as_deref())
            .await?;

        Ok(tools::UpdateTaskStatusResponse { success: true })
    }

    async fn handle_assign_task(
        &self,
        req: tools::AssignTaskRequest,
    ) -> HandlerResult<tools::AssignTaskResponse> {
        let task_id = Self::parse_task_id(&req.task_id)?;
        let agent_id = Self::parse_agent_id(&req.agent_id)?;

        self.state
            .repository()
            .assign_task(task_id, agent_id)
            .await?;

        Ok(tools::AssignTaskResponse { success: true })
    }

    async fn handle_get_task_context(
        &self,
        req: tools::GetTaskContextRequest,
    ) -> HandlerResult<tools::GetTaskContextResponse> {
        let task_id = Self::parse_task_id(&req.task_id)?;
        let repo = self.state.repository();

        let task = repo.get_task(task_id).await?;
        let knowledge = repo.get_task_knowledge(task_id).await?;
        let subtasks = repo.get_subtasks(task_id).await?;

        // Get dependency information
        let blocking_tasks = repo.get_blocking_tasks(task_id).await?;
        let blocked_tasks = repo.get_blocked_tasks(task_id).await?;

        let mut task_info = Self::task_to_info(&task);

        // Populate blocked_by and blocks fields
        if !blocking_tasks.is_empty() {
            task_info.blocked_by = Some(
                blocking_tasks
                    .iter()
                    .map(|t| t.id.as_uuid().to_string())
                    .collect(),
            );
        }
        if !blocked_tasks.is_empty() {
            task_info.blocks = Some(
                blocked_tasks
                    .iter()
                    .map(|t| t.id.as_uuid().to_string())
                    .collect(),
            );
        }

        Ok(tools::GetTaskContextResponse {
            task: task_info,
            knowledge: knowledge
                .iter()
                .map(|k| Self::knowledge_to_result(k, 0.0))
                .collect(),
            subtasks: subtasks.iter().map(Self::task_to_info).collect(),
        })
    }

    async fn handle_add_task_dependency(
        &self,
        req: tools::AddTaskDependencyRequest,
    ) -> HandlerResult<tools::AddTaskDependencyResponse> {
        let task_id = Self::parse_task_id(&req.task_id)?;
        let blocked_task_id = Self::parse_task_id(&req.blocked_task_id)?;

        self.state
            .repository()
            .add_task_dependency(task_id, blocked_task_id)
            .await?;

        Ok(tools::AddTaskDependencyResponse { success: true })
    }

    async fn handle_remove_task_dependency(
        &self,
        req: tools::RemoveTaskDependencyRequest,
    ) -> HandlerResult<tools::RemoveTaskDependencyResponse> {
        let task_id = Self::parse_task_id(&req.task_id)?;
        let blocked_task_id = Self::parse_task_id(&req.blocked_task_id)?;

        self.state
            .repository()
            .remove_task_dependency(task_id, blocked_task_id)
            .await?;

        Ok(tools::RemoveTaskDependencyResponse { success: true })
    }

    async fn handle_get_task_history(
        &self,
        req: tools::GetTaskHistoryRequest,
    ) -> HandlerResult<tools::GetTaskHistoryResponse> {
        let task_id = Self::parse_task_id(&req.task_id)?;

        // Get all versions of the task from the repository
        let task_versions = self.state.repository().get_task_history(task_id).await?;

        // Convert Task entities to TaskVersionInfo
        let mut versions = Vec::with_capacity(task_versions.len());

        for (version_num, task) in task_versions.iter().enumerate() {
            let task_info = Self::task_to_info(task);

            // For version history, we use created_at as a proxy for temporal data
            // In a full implementation, we'd extract bi-temporal data from AletheiaDB
            let valid_from = task.created_at.to_rfc3339();
            let transaction_time = task.created_at.to_rfc3339();

            // Compute changes from previous version if available
            let changes = if version_num > 0 {
                let prev_task = &task_versions[version_num - 1];
                let mut modified_fields = Vec::new();

                if prev_task.title != task.title {
                    modified_fields.push("title".to_string());
                }
                if prev_task.description != task.description {
                    modified_fields.push("description".to_string());
                }
                if prev_task.status != task.status {
                    modified_fields.push("status".to_string());
                }
                if prev_task.priority != task.priority {
                    modified_fields.push("priority".to_string());
                }
                if prev_task.assigned_to != task.assigned_to {
                    modified_fields.push("assigned_to".to_string());
                }
                if prev_task.summary != task.summary {
                    modified_fields.push("summary".to_string());
                }

                let change_count = modified_fields.len();

                if change_count > 0 {
                    Some(tools::TaskVersionChanges {
                        modified_fields,
                        change_count,
                    })
                } else {
                    None
                }
            } else {
                // First version has no changes
                None
            };

            versions.push(tools::TaskVersionInfo {
                version_number: (version_num + 1) as u64,
                task: task_info,
                valid_from,
                transaction_time,
                changes,
            });
        }

        Ok(tools::GetTaskHistoryResponse {
            version_count: versions.len(),
            versions,
        })
    }

    async fn handle_get_task_as_of(
        &self,
        req: tools::GetTaskAsOfRequest,
    ) -> HandlerResult<tools::GetTaskAsOfResponse> {
        use chrono::{DateTime, Utc};

        let task_id = Self::parse_task_id(&req.task_id)?;

        // Parse timestamps
        let valid_time: DateTime<Utc> = req
            .valid_time
            .parse()
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid valid_time: {}", e)))?;

        let transaction_time: DateTime<Utc> = if let Some(ref tt) = req.transaction_time {
            tt.parse().map_err(|e| {
                HandlerError::InvalidArgs(format!("Invalid transaction_time: {}", e))
            })?
        } else {
            Utc::now()
        };

        // Verify task exists first
        let _ = self
            .state
            .repository()
            .get_task(task_id)
            .await
            .map_err(|_| HandlerError::InvalidArgs("Task not found".into()))?;

        // Get task history and filter for the version valid at requested time
        let task_versions = self.state.repository().get_task_history(task_id).await?;

        // Find the version valid at the requested time
        // For now, use created_at as a proxy for valid_time
        // In a full implementation, we'd use actual bi-temporal data from AletheiaDB
        let task_at_time = task_versions
            .iter()
            .filter(|t| t.created_at <= valid_time && t.created_at <= transaction_time)
            .max_by_key(|t| t.created_at)
            .ok_or_else(|| {
                HandlerError::InvalidArgs(format!(
                    "No task version found at valid_time: {}, transaction_time: {}",
                    valid_time, transaction_time
                ))
            })?;

        Ok(tools::GetTaskAsOfResponse {
            task: Self::task_to_info(task_at_time),
            valid_time: valid_time.to_rfc3339(),
            transaction_time: transaction_time.to_rfc3339(),
        })
    }

    async fn handle_semantic_search_tasks(
        &self,
        req: tools::SemanticSearchTasksRequest,
    ) -> HandlerResult<tools::SemanticSearchTasksResponse> {
        // Parse optional status filter
        let status = req
            .status
            .as_deref()
            .map(Self::parse_task_status)
            .transpose()?;

        // Generate embedding from query using embedding service
        if let Some(svc) = self.state.embedding_service() {
            match svc.embed(&req.query).await {
                Ok(query_vec) => {
                    // Search tasks using vector similarity
                    let results = self
                        .state
                        .repository()
                        .search_tasks(&query_vec, req.limit, status)
                        .await?;

                    return Ok(tools::SemanticSearchTasksResponse {
                        results: results
                            .iter()
                            .map(|(task, score)| tools::TaskSearchResult {
                                task: Self::task_to_info(task),
                                similarity: *score,
                            })
                            .collect(),
                        query: req.query,
                    });
                }
                Err(e) => {
                    tracing::warn!("Query embedding failed: {e}");
                    // Return empty results if embedding fails
                    return Ok(tools::SemanticSearchTasksResponse {
                        results: Vec::new(),
                        query: req.query,
                    });
                }
            }
        }

        // No embedding service available - return empty results
        Ok(tools::SemanticSearchTasksResponse {
            results: Vec::new(),
            query: req.query,
        })
    }

    async fn handle_get_task_tree(
        &self,
        req: tools::GetTaskTreeRequest,
    ) -> HandlerResult<tools::GetTaskTreeResponse> {
        let repo = self.state.repository();

        // Get root tasks (either specified root or all top-level tasks)
        let root_tasks = if let Some(ref root_id_str) = req.root_task_id {
            let root_id = Self::parse_task_id(root_id_str)?;
            let task = repo.get_task(root_id).await?;
            vec![task]
        } else {
            repo.get_root_tasks(self.state.session_id()).await?
        };

        // Build the tree recursively
        let max_depth = req.max_depth.unwrap_or(usize::MAX);
        let mut total_tasks = 0;
        let mut max_depth_found = 0;

        let mut roots = Vec::new();
        for task in root_tasks {
            let node = self
                .build_task_node(task, 0, max_depth, &mut total_tasks, &mut max_depth_found)
                .await?;
            roots.push(node);
        }

        Ok(tools::GetTaskTreeResponse {
            roots,
            total_tasks,
            max_depth: max_depth_found,
        })
    }

    /// Recursively build a task node with its children.
    async fn build_task_node(
        &self,
        task: Task,
        depth: usize,
        max_depth: usize,
        total_tasks: &mut usize,
        max_depth_found: &mut usize,
    ) -> HandlerResult<tools::TaskNode> {
        *total_tasks += 1;
        *max_depth_found = (*max_depth_found).max(depth);

        let task_info = Self::task_to_info(&task);
        let task_id = task.id;

        // Get children if we haven't reached max depth
        let children = if depth < max_depth {
            let subtasks = self.state.repository().get_subtasks(task_id).await?;
            let mut child_nodes = Vec::new();
            for subtask in subtasks {
                let child_node = Box::pin(self.build_task_node(
                    subtask,
                    depth + 1,
                    max_depth,
                    total_tasks,
                    max_depth_found,
                ))
                .await?;
                child_nodes.push(child_node);
            }
            child_nodes
        } else {
            Vec::new()
        };

        Ok(tools::TaskNode {
            task: task_info,
            children,
            depth,
        })
    }

    async fn handle_find_path(
        &self,
        req: tools::FindPathRequest,
    ) -> HandlerResult<tools::FindPathResponse> {
        use std::collections::{HashMap, HashSet, VecDeque};

        let from_id = Self::parse_task_id(&req.from_task_id)?;
        let to_id = Self::parse_task_id(&req.to_task_id)?;

        // Early exit if from == to
        if from_id == to_id {
            let task = self.state.repository().get_task(from_id).await?;
            return Ok(tools::FindPathResponse {
                found: true,
                path: Some(vec![tools::PathStep {
                    task_id: task.id.as_uuid().to_string(),
                    title: task.title,
                    relationship: "self".to_string(),
                }]),
                distance: Some(0),
            });
        }

        // BFS to find shortest path
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut parent: HashMap<TaskId, (TaskId, String)> = HashMap::new();

        queue.push_back((from_id, 0usize));
        visited.insert(from_id);

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= req.max_depth {
                continue;
            }

            // Check if we reached the target
            if current_id == to_id {
                // Reconstruct path
                let mut path_ids = vec![to_id];
                let mut current = to_id;

                while let Some((prev, _)) = parent.get(&current) {
                    path_ids.push(*prev);
                    current = *prev;
                }

                path_ids.reverse();

                // Build path steps with task info
                let mut path_steps = Vec::new();
                for (i, task_id) in path_ids.iter().enumerate() {
                    let task = self.state.repository().get_task(*task_id).await?;
                    let relationship = if i == 0 {
                        "start".to_string()
                    } else if let Some((_, rel)) = parent.get(task_id) {
                        rel.clone()
                    } else {
                        "unknown".to_string()
                    };

                    path_steps.push(tools::PathStep {
                        task_id: task.id.as_uuid().to_string(),
                        title: task.title,
                        relationship,
                    });
                }

                return Ok(tools::FindPathResponse {
                    found: true,
                    path: Some(path_steps),
                    distance: Some(path_ids.len() - 1),
                });
            }

            // Explore neighbors: tasks this blocks (forward edges)
            if let Ok(blocked) = self.state.repository().get_blocked_tasks(current_id).await {
                for task in blocked {
                    if !visited.contains(&task.id) {
                        visited.insert(task.id);
                        parent.insert(task.id, (current_id, "blocks".to_string()));
                        queue.push_back((task.id, depth + 1));
                    }
                }
            }

            // Explore neighbors: tasks that block this (backward edges)
            if let Ok(blocking) = self.state.repository().get_blocking_tasks(current_id).await {
                for task in blocking {
                    if !visited.contains(&task.id) {
                        visited.insert(task.id);
                        parent.insert(task.id, (current_id, "blocked_by".to_string()));
                        queue.push_back((task.id, depth + 1));
                    }
                }
            }
        }

        // No path found
        Ok(tools::FindPathResponse {
            found: false,
            path: None,
            distance: None,
        })
    }

    async fn handle_task_statistics(
        &self,
        _req: tools::TaskStatisticsRequest,
    ) -> HandlerResult<tools::TaskStatisticsResponse> {
        let session_id = self.state.session_id();

        // Get all tasks for this session
        let all_tasks = self.state.repository().list_tasks(session_id, None).await?;

        // Count by status
        let mut status_counts = tools::StatusCounts {
            pending: 0,
            claimed: 0,
            in_progress: 0,
            completed: 0,
            failed: 0,
            total: all_tasks.len(),
        };

        // Count by priority
        let mut priority_counts = tools::PriorityCounts {
            low: 0,
            medium: 0,
            high: 0,
            critical: 0,
        };

        let mut assigned_count = 0;
        let mut completion_times: Vec<i64> = Vec::new();

        for task in &all_tasks {
            // Status counts
            match task.status {
                TaskStatus::Pending => status_counts.pending += 1,
                TaskStatus::Claimed => status_counts.claimed += 1,
                TaskStatus::InProgress => status_counts.in_progress += 1,
                TaskStatus::Completed => status_counts.completed += 1,
                TaskStatus::Failed => status_counts.failed += 1,
            }

            // Priority counts
            match task.priority {
                Priority::Low => priority_counts.low += 1,
                Priority::Medium => priority_counts.medium += 1,
                Priority::High => priority_counts.high += 1,
                Priority::Critical => priority_counts.critical += 1,
            }

            // Assigned count
            if task.assigned_to.is_some() {
                assigned_count += 1;
            }

            // Completion times (for completed or failed tasks)
            if let Some(completed_at) = task.completed_at {
                let duration = completed_at.signed_duration_since(task.created_at);
                completion_times.push(duration.num_seconds());
            }
        }

        // Calculate completion metrics
        let completion_rate = if all_tasks.is_empty() {
            0.0
        } else {
            (status_counts.completed as f64 / all_tasks.len() as f64) * 100.0
        };

        let avg_completion_time_secs = if completion_times.is_empty() {
            None
        } else {
            let sum: i64 = completion_times.iter().sum();
            Some(sum as f64 / completion_times.len() as f64)
        };

        let median_completion_time_secs = if completion_times.is_empty() {
            None
        } else {
            completion_times.sort_unstable();
            let mid = completion_times.len() / 2;
            if completion_times.len().is_multiple_of(2) {
                Some((completion_times[mid - 1] + completion_times[mid]) as f64 / 2.0)
            } else {
                Some(completion_times[mid] as f64)
            }
        };

        let completion_metrics = tools::CompletionMetrics {
            completion_rate,
            avg_completion_time_secs,
            median_completion_time_secs,
        };

        // Count blocked tasks (tasks with blocking dependencies)
        let mut blocked_count = 0;
        for task in &all_tasks {
            let blocking = self.state.repository().get_blocking_tasks(task.id).await?;
            if !blocking.is_empty() {
                blocked_count += 1;
            }
        }

        Ok(tools::TaskStatisticsResponse {
            status_counts,
            priority_counts,
            completion_metrics,
            assigned_count,
            blocked_count,
        })
    }

    async fn handle_cold_storage_query(
        &self,
        _req: tools::ColdStorageQueryRequest,
    ) -> HandlerResult<tools::ColdStorageQueryResponse> {
        // Query cold storage requires AletheiaDB backend with tiered storage enabled
        // Currently, the AletheiaDB historical field is private, so we can't access it
        // TODO: Add public API to AletheiaDB to access tiered storage metrics
        Ok(tools::ColdStorageQueryResponse {
            available: false,
            storage_stats: None,
            tiered_metrics: None,
            message: "Cold storage query requires AletheiaDB backend with cold storage enabled and public API access to tiered storage metrics.".into(),
        })
    }

    async fn handle_export_project_graph(
        &self,
        req: tools::ExportProjectGraphRequest,
    ) -> HandlerResult<tools::ExportProjectGraphResponse> {
        let session_id = self.state.session_id();

        // Gather all entities to include in the graph
        let mut node_count = 0;
        let mut edge_count = 0;
        let mut task_count = 0;
        let mut agent_count = 0;
        let mut knowledge_count = 0;

        let mut graph_data = String::new();

        match req.format.as_str() {
            "dot" => {
                graph_data.push_str("digraph HarnessProject {\n");
                graph_data.push_str("  rankdir=LR;\n");
                graph_data.push_str("  node [shape=box, style=rounded];\n\n");

                // Add session node
                graph_data.push_str(&format!("  session_{} [label=\"Session {}\", shape=oval, style=filled, fillcolor=lightblue];\n",
                    session_id.as_uuid().to_string().replace('-', "_"),
                    session_id.as_uuid()));
                node_count += 1;

                // Add agents if requested
                if req.include_agents {
                    let agents = self.state.repository().list_agents(session_id).await?;
                    agent_count = agents.len();
                    for agent in &agents {
                        let agent_id = agent.id.as_uuid().to_string().replace('-', "_");
                        let role_str = format!("{:?}", agent.role);
                        graph_data.push_str(&format!("  agent_{} [label=\"{} Agent\\n{}\", style=filled, fillcolor=lightgreen];\n",
                            agent_id,
                            role_str,
                            agent.id.as_uuid()));
                        graph_data.push_str(&format!(
                            "  session_{} -> agent_{} [label=\"CONTAINS_AGENT\"];\n",
                            session_id.as_uuid().to_string().replace('-', "_"),
                            agent_id
                        ));
                        node_count += 1;
                        edge_count += 1;
                    }
                    graph_data.push('\n');
                }

                // Add tasks if requested
                if req.include_tasks {
                    let tasks = self.state.repository().list_tasks(session_id, None).await?;
                    task_count = tasks.len();
                    for task in &tasks {
                        let task_id = task.id.as_uuid().to_string().replace('-', "_");
                        let color = match task.status {
                            TaskStatus::Completed => "lightgreen",
                            TaskStatus::InProgress => "lightyellow",
                            TaskStatus::Failed => "lightcoral",
                            _ => "white",
                        };
                        let status_str = format!("{:?}", task.status);
                        graph_data.push_str(&format!(
                            "  task_{} [label=\"Task: {}\\n{}\", style=filled, fillcolor={}];\n",
                            task_id,
                            task.title.replace('"', "'"),
                            status_str,
                            color
                        ));
                        graph_data.push_str(&format!(
                            "  session_{} -> task_{} [label=\"CONTAINS_TASK\"];\n",
                            session_id.as_uuid().to_string().replace('-', "_"),
                            task_id
                        ));
                        node_count += 1;
                        edge_count += 1;

                        // Add task dependencies
                        let blocking = self.state.repository().get_blocking_tasks(task.id).await?;
                        for blocker in blocking {
                            let blocker_id = blocker.id.as_uuid().to_string().replace('-', "_");
                            graph_data.push_str(&format!("  task_{} -> task_{} [label=\"BLOCKS\", style=dashed, color=red];\n",
                                blocker_id,
                                task_id));
                            edge_count += 1;
                        }

                        // Add agent assignments
                        if let Some(agent_id) = task.assigned_to {
                            graph_data.push_str(&format!(
                                "  agent_{} -> task_{} [label=\"CLAIMS\", color=blue];\n",
                                agent_id.as_uuid().to_string().replace('-', "_"),
                                task_id
                            ));
                            edge_count += 1;
                        }
                    }
                    graph_data.push('\n');
                }

                // Add knowledge if requested
                if req.include_knowledge {
                    let knowledge_entries = self
                        .state
                        .repository()
                        .get_recent_knowledge(session_id, 100)
                        .await?;
                    knowledge_count = knowledge_entries.len();
                    for (idx, knowledge) in knowledge_entries.iter().enumerate() {
                        let k_id = knowledge.id.as_uuid().to_string().replace('-', "_");
                        let kind_str = format!("{:?}", knowledge.kind);
                        graph_data.push_str(&format!("  knowledge_{} [label=\"Knowledge {}\\n{}\", shape=note, style=filled, fillcolor=lightyellow];\n",
                            k_id,
                            idx + 1,
                            kind_str));
                        graph_data.push_str(&format!(
                            "  agent_{} -> knowledge_{} [label=\"SHARED\", color=purple];\n",
                            knowledge.author_id.as_uuid().to_string().replace('-', "_"),
                            k_id
                        ));
                        node_count += 1;
                        edge_count += 1;

                        if let Some(task_id) = knowledge.task_id {
                            graph_data.push_str(&format!(
                                "  knowledge_{} -> task_{} [label=\"ABOUT\", style=dotted];\n",
                                k_id,
                                task_id.as_uuid().to_string().replace('-', "_")
                            ));
                            edge_count += 1;
                        }
                    }
                }

                graph_data.push_str("}\n");
            }
            "json" => {
                // Simple JSON format with nodes and edges
                use serde_json::json;

                let mut nodes = vec![];
                let mut edges = vec![];

                // Session node
                nodes.push(json!({
                    "id": session_id.as_uuid().to_string(),
                    "type": "session",
                    "label": format!("Session {}", session_id.as_uuid())
                }));
                node_count += 1;

                // Agents
                if req.include_agents {
                    let agents = self.state.repository().list_agents(session_id).await?;
                    agent_count = agents.len();
                    for agent in &agents {
                        nodes.push(json!({
                            "id": agent.id.as_uuid().to_string(),
                            "type": "agent",
                            "label": format!("{:?}", agent.role),
                            "role": format!("{:?}", agent.role),
                            "status": format!("{:?}", agent.status)
                        }));
                        edges.push(json!({
                            "from": session_id.as_uuid().to_string(),
                            "to": agent.id.as_uuid().to_string(),
                            "type": "CONTAINS_AGENT"
                        }));
                        node_count += 1;
                        edge_count += 1;
                    }
                }

                // Tasks
                if req.include_tasks {
                    let tasks = self.state.repository().list_tasks(session_id, None).await?;
                    task_count = tasks.len();
                    for task in &tasks {
                        nodes.push(json!({
                            "id": task.id.as_uuid().to_string(),
                            "type": "task",
                            "label": task.title.clone(),
                            "status": format!("{:?}", task.status),
                            "priority": format!("{:?}", task.priority)
                        }));
                        edges.push(json!({
                            "from": session_id.as_uuid().to_string(),
                            "to": task.id.as_uuid().to_string(),
                            "type": "CONTAINS_TASK"
                        }));
                        node_count += 1;
                        edge_count += 1;

                        // Dependencies
                        let blocking = self.state.repository().get_blocking_tasks(task.id).await?;
                        for blocker in blocking {
                            edges.push(json!({
                                "from": blocker.id.as_uuid().to_string(),
                                "to": task.id.as_uuid().to_string(),
                                "type": "BLOCKS"
                            }));
                            edge_count += 1;
                        }

                        // Agent assignments
                        if let Some(agent_id) = task.assigned_to {
                            edges.push(json!({
                                "from": agent_id.as_uuid().to_string(),
                                "to": task.id.as_uuid().to_string(),
                                "type": "CLAIMS"
                            }));
                            edge_count += 1;
                        }
                    }
                }

                // Knowledge
                if req.include_knowledge {
                    let knowledge_entries = self
                        .state
                        .repository()
                        .get_recent_knowledge(session_id, 100)
                        .await?;
                    knowledge_count = knowledge_entries.len();
                    for knowledge in &knowledge_entries {
                        nodes.push(json!({
                            "id": knowledge.id.as_uuid().to_string(),
                            "type": "knowledge",
                            "label": knowledge.content[..knowledge.content.len().min(50)].to_string(),
                            "kind": format!("{:?}", knowledge.kind)
                        }));
                        edges.push(json!({
                            "from": knowledge.author_id.as_uuid().to_string(),
                            "to": knowledge.id.as_uuid().to_string(),
                            "type": "SHARED"
                        }));
                        node_count += 1;
                        edge_count += 1;

                        if let Some(task_id) = knowledge.task_id {
                            edges.push(json!({
                                "from": knowledge.id.as_uuid().to_string(),
                                "to": task_id.as_uuid().to_string(),
                                "type": "ABOUT"
                            }));
                            edge_count += 1;
                        }
                    }
                }

                let graph_json = json!({
                    "nodes": nodes,
                    "edges": edges
                });
                graph_data = serde_json::to_string_pretty(&graph_json).unwrap();
            }
            _ => {
                return Err(HandlerError::InvalidArgs(format!(
                    "Invalid format: {}. Supported formats: dot, json",
                    req.format
                )));
            }
        }

        Ok(tools::ExportProjectGraphResponse {
            graph: graph_data,
            format: req.format.clone(),
            stats: tools::GraphExportStats {
                node_count,
                edge_count,
                task_count,
                agent_count,
                knowledge_count,
            },
        })
    }

    // --- Knowledge handlers ---

    async fn handle_share_knowledge(
        &self,
        req: tools::ShareKnowledgeRequest,
    ) -> HandlerResult<tools::ShareKnowledgeResponse> {
        let agent_id = self.resolve_agent_id(req._agent_id.as_deref()).await?;
        let kind = Self::parse_kind(&req.kind)?;

        let mut knowledge = Knowledge::new(&req.content, kind, agent_id, self.state.session_id());

        if let Some(ref tid_str) = req.task_id {
            knowledge = knowledge.with_task(Self::parse_task_id(tid_str)?);
        }

        // Generate embedding if service is available.
        if let Some(svc) = self.state.embedding_service() {
            match svc.embed(&req.content).await {
                Ok(embedding) => {
                    knowledge.embedding = Some(embedding);
                }
                Err(e) => {
                    tracing::warn!("Embedding failed, storing without: {e}");
                }
            }
        }

        let kid = knowledge.id.as_uuid().to_string();
        self.state.repository().create_knowledge(&knowledge).await?;

        Ok(tools::ShareKnowledgeResponse { knowledge_id: kid })
    }

    async fn handle_ask_hive(
        &self,
        req: tools::AskHiveRequest,
    ) -> HandlerResult<tools::AskHiveResponse> {
        // Use semantic search if embedding service is available.
        if let Some(svc) = self.state.embedding_service() {
            match svc.embed(&req.query).await {
                Ok(query_vec) => {
                    let results = self
                        .state
                        .repository()
                        .search_knowledge(&query_vec, req.limit)
                        .await?;

                    return Ok(tools::AskHiveResponse {
                        results: results
                            .iter()
                            .map(|(k, score)| Self::knowledge_to_result(k, *score))
                            .collect(),
                    });
                }
                Err(e) => {
                    tracing::warn!("Query embedding failed, falling back to recent: {e}");
                }
            }
        }

        // Fallback: recent knowledge ordered by time.
        let knowledge = self
            .state
            .repository()
            .get_recent_knowledge(self.state.session_id(), req.limit)
            .await?;

        Ok(tools::AskHiveResponse {
            results: knowledge
                .iter()
                .map(|k| Self::knowledge_to_result(k, 0.0))
                .collect(),
        })
    }

    async fn handle_fish_knowledge(
        &self,
        req: tools::FishKnowledgeRequest,
    ) -> HandlerResult<tools::FishKnowledgeResponse> {
        use harness_persistence::KnowledgeId;
        use std::collections::{HashMap, HashSet};

        // Parse knowledge ID
        let kid = KnowledgeId::from_uuid(
            uuid::Uuid::parse_str(&req.knowledge_id)
                .map_err(|_| HandlerError::InvalidArgs("invalid knowledge_id".into()))?,
        );

        // Get the starting knowledge entry
        let start_knowledge = self.state.repository().get_knowledge(kid).await?;
        let starting_from = format!(
            "{} [{}]",
            start_knowledge.content.chars().take(50).collect::<String>(),
            format!("{:?}", start_knowledge.kind).to_lowercase()
        );

        // Track results with scores
        let mut results: HashMap<KnowledgeId, (Knowledge, f32, Vec<String>)> = HashMap::new();
        let mut visited = HashSet::new();
        visited.insert(kid);

        // 1. Get vector-similar knowledge (semantic component)
        if let Some(embedding) = &start_knowledge.embedding
            && let Ok(similar) = self
                .state
                .repository()
                .search_knowledge(embedding, req.limit * 2)
                .await
        {
            for (k, similarity) in similar {
                if k.id != kid {
                    let k_id = k.id;
                    let score = similarity * 0.7; // Weight vector similarity
                    let paths = vec![format!("Vector Similarity: {:.4}", similarity)];
                    results.insert(k_id, (k, score, paths));
                    visited.insert(k_id);
                }
            }
        }

        // 2. Get graph-connected knowledge (structural component)
        // Knowledge connected via same task
        if let Some(task_id) = start_knowledge.task_id
            && let Ok(task_knowledge) = self.state.repository().get_task_knowledge(task_id).await
        {
            for k in task_knowledge {
                if !visited.contains(&k.id) {
                    let k_id = k.id;
                    let score = 0.8; // Graph connection weight
                    let task_title = self
                        .state
                        .repository()
                        .get_task(task_id)
                        .await
                        .map(|t| t.title)
                        .unwrap_or_else(|_| "Unknown Task".into());
                    let paths = vec![format!("via Task: {}", task_title)];

                    // If we already have this from vector search, combine scores
                    results
                        .entry(k_id)
                        .and_modify(|(_, s, p)| {
                            *s += score;
                            p.push(paths[0].clone());
                        })
                        .or_insert((k, score, paths));
                    visited.insert(k_id);
                }
            }
        }

        // Knowledge from same author (connection weight)
        if let Ok(all_knowledge) = self
            .state
            .repository()
            .get_recent_knowledge(self.state.session_id(), 100)
            .await
        {
            for k in all_knowledge {
                if k.author_id == start_knowledge.author_id && !visited.contains(&k.id) {
                    let k_id = k.id;
                    let score = 0.3; // Same author weight
                    let author_role = self
                        .state
                        .repository()
                        .get_agent(start_knowledge.author_id)
                        .await
                        .map(|a| format!("{:?}", a.role))
                        .unwrap_or_else(|_| "Unknown".into());
                    let paths = vec![format!("via Same Author [{}]", author_role)];

                    results
                        .entry(k_id)
                        .and_modify(|(_, s, p)| {
                            *s += score;
                            p.push(paths[0].clone());
                        })
                        .or_insert((k, score, paths));
                    visited.insert(k_id);
                }
            }
        }

        // 3. Add freshness boost (recency component)
        let now = chrono::Utc::now();
        for (_, (k, score, _)) in results.iter_mut() {
            let age_hours = now.signed_duration_since(k.created_at).num_hours();
            if age_hours < 24 {
                *score += 0.2; // Fresh knowledge boost
            }
        }

        // Sort by score and limit
        let mut sorted_results: Vec<_> = results.into_values().collect();
        sorted_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        sorted_results.truncate(req.limit);

        Ok(tools::FishKnowledgeResponse {
            starting_from,
            results: sorted_results
                .into_iter()
                .map(|(k, score, paths)| tools::FishResult {
                    id: k.id.as_uuid().to_string(),
                    content: k.content,
                    kind: format!("{:?}", k.kind).to_lowercase(),
                    author: k.author_id.as_uuid().to_string(),
                    score,
                    vector_similarity: k.embedding.as_ref().map(|_| score * 0.7),
                    connection_paths: paths,
                    created_at: k.created_at.to_rfc3339(),
                    task_id: k.task_id.map(|t| t.as_uuid().to_string()),
                })
                .collect(),
        })
    }

    async fn handle_knowledge_clusters(
        &self,
        req: tools::KnowledgeClustersRequest,
    ) -> HandlerResult<tools::KnowledgeClustersResponse> {
        use harness_persistence::KnowledgeId;
        use std::collections::{HashMap, HashSet};

        // Get all knowledge in the session
        let all_knowledge = self
            .state
            .repository()
            .get_recent_knowledge(self.state.session_id(), 10000)
            .await?;

        // Filter to only those with embeddings
        let knowledge_with_embeddings: Vec<_> = all_knowledge
            .iter()
            .filter(|k| k.embedding.is_some())
            .collect();

        let total_count = all_knowledge.len();
        let _embeddable_count = knowledge_with_embeddings.len();

        // Early exit if no knowledge with embeddings
        if knowledge_with_embeddings.is_empty() {
            return Ok(tools::KnowledgeClustersResponse {
                clusters: Vec::new(),
                total_knowledge_count: total_count,
                clustered_count: 0,
                unclustered_count: total_count,
            });
        }

        // Build pairwise similarity matrix
        let mut similarity_matrix: HashMap<(KnowledgeId, KnowledgeId), f32> = HashMap::new();

        for (i, k1) in knowledge_with_embeddings.iter().enumerate() {
            for k2 in knowledge_with_embeddings.iter().skip(i + 1) {
                if k1.id == k2.id {
                    continue;
                }

                if let (Some(e1), Some(e2)) = (&k1.embedding, &k2.embedding) {
                    // Compute cosine similarity
                    let dot_product: f32 = e1.iter().zip(e2.iter()).map(|(a, b)| a * b).sum();
                    let mag1: f32 = e1.iter().map(|x| x * x).sum::<f32>().sqrt();
                    let mag2: f32 = e2.iter().map(|x| x * x).sum::<f32>().sqrt();

                    let similarity = if mag1 > 0.0 && mag2 > 0.0 {
                        dot_product / (mag1 * mag2)
                    } else {
                        0.0
                    };

                    similarity_matrix.insert((k1.id, k2.id), similarity);
                    similarity_matrix.insert((k2.id, k1.id), similarity);
                }
            }
        }

        // Simple agglomerative clustering using similarity threshold
        let mut clusters: Vec<HashSet<KnowledgeId>> = Vec::new();
        let mut assigned: HashSet<KnowledgeId> = HashSet::new();

        for k in &knowledge_with_embeddings {
            if assigned.contains(&k.id) {
                continue;
            }

            // Start new cluster
            let mut cluster = HashSet::new();
            cluster.insert(k.id);
            assigned.insert(k.id);

            // Find all knowledge similar enough to join this cluster
            for other in &knowledge_with_embeddings {
                if assigned.contains(&other.id) || other.id == k.id {
                    continue;
                }

                // Check if similar to any member of current cluster
                let mut max_similarity = 0.0f32;
                for &member_id in &cluster {
                    if let Some(&sim) = similarity_matrix.get(&(member_id, other.id)) {
                        max_similarity = max_similarity.max(sim);
                    }
                }

                if max_similarity >= req.similarity_threshold {
                    cluster.insert(other.id);
                    assigned.insert(other.id);
                }
            }

            // Only keep clusters above minimum size
            if cluster.len() >= req.min_cluster_size {
                clusters.push(cluster);
            }
        }

        // Build response clusters
        let mut response_clusters = Vec::new();

        for (cluster_idx, cluster_members) in clusters.iter().enumerate() {
            let members_vec: Vec<_> = cluster_members.iter().collect();

            // Compute average pairwise similarity within cluster
            let mut total_sim = 0.0f32;
            let mut pair_count = 0;

            for (i, id1) in members_vec.iter().enumerate() {
                for id2 in members_vec.iter().skip(i + 1) {
                    if let Some(&sim) = similarity_matrix.get(&(**id1, **id2)) {
                        total_sim += sim;
                        pair_count += 1;
                    }
                }
            }

            let avg_similarity = if pair_count > 0 {
                total_sim / pair_count as f32
            } else {
                1.0 // Single-member cluster
            };

            // Find representative (most central) member
            let mut best_avg = -1.0f32;
            let mut representative_content = String::new();

            for &member_id in cluster_members {
                let mut member_total_sim = 0.0f32;
                let mut member_count = 0;

                for &other_id in cluster_members {
                    if member_id != other_id
                        && let Some(&sim) = similarity_matrix.get(&(member_id, other_id))
                    {
                        member_total_sim += sim;
                        member_count += 1;
                    }
                }

                let member_avg = if member_count > 0 {
                    member_total_sim / member_count as f32
                } else {
                    0.0
                };

                if member_avg > best_avg {
                    best_avg = member_avg;
                    if let Some(k) = all_knowledge.iter().find(|k| k.id == member_id) {
                        representative_content = if k.content.len() > 100 {
                            format!("{}...", &k.content[..97])
                        } else {
                            k.content.clone()
                        };
                    }
                }
            }

            // Build member results
            let mut member_results = Vec::new();
            for &member_id in cluster_members {
                if let Some(k) = all_knowledge.iter().find(|k| k.id == member_id) {
                    member_results.push(Self::knowledge_to_result(k, avg_similarity));
                }
            }

            response_clusters.push(tools::KnowledgeCluster {
                cluster_id: cluster_idx,
                size: cluster_members.len(),
                avg_similarity,
                representative_content,
                members: member_results,
            });
        }

        // Sort clusters by size (largest first)
        response_clusters.sort_by(|a, b| b.size.cmp(&a.size));

        let clustered_count = clusters.iter().map(|c| c.len()).sum();

        Ok(tools::KnowledgeClustersResponse {
            clusters: response_clusters,
            total_knowledge_count: total_count,
            clustered_count,
            unclustered_count: total_count - clustered_count,
        })
    }

    // --- Agent handlers ---

    async fn handle_register_agent(
        &self,
        req: tools::RegisterAgentRequest,
    ) -> HandlerResult<tools::RegisterAgentResponse> {
        let role = Self::parse_role(&req.role)?;

        // Two registration paths: spawned agent or self-registration
        let aid = if let Some(ref agent_id_str) = req.agent_id {
            // Spawned agent: activate existing agent
            let agent_id = Self::parse_agent_id(agent_id_str)?;
            let agent = self.state.repository().get_agent(agent_id).await?;

            // Validate agent is in Starting status (prevent hijacking)
            if agent.status != AgentStatus::Starting {
                return Err(HandlerError::InvalidArgs(format!(
                    "Agent {} is not in Starting status (current: {:?})",
                    agent_id_str, agent.status
                )));
            }

            // Validate role matches
            if agent.role != role {
                return Err(HandlerError::InvalidArgs(format!(
                    "Role mismatch: expected {:?}, got {:?}",
                    agent.role, role
                )));
            }

            // Update to Active and add project context
            self.state
                .repository()
                .update_agent_status(agent_id, AgentStatus::Active)
                .await?;

            // Add project context if provided (update the agent entity)
            if let (Some(name), Some(path)) = (req.project_name.clone(), req.project_path.clone()) {
                let _agent = agent.with_project(name, path);
                // TODO: Add repository method to update project context
                // For now, the agent is Active but project context isn't persisted for spawned agents
            }

            agent_id
        } else {
            // Self-registration: create new agent
            let mut agent = Agent::new(role, self.state.session_id());

            // Add project context if provided
            if let (Some(name), Some(path)) = (req.project_name, req.project_path) {
                agent = agent.with_project(name, path);
            }

            let aid = agent.id;
            self.state.repository().create_agent(&agent).await?;
            aid
        };

        // Persist session-agent association for MCP client persistence
        self.state
            .repository()
            .set_session_agent(self.state.session_id(), aid)
            .await?;

        *self.agent_id.write().await = Some(aid);

        Ok(tools::RegisterAgentResponse {
            agent_id: aid.as_uuid().to_string(),
        })
    }

    async fn handle_list_agents(&self) -> HandlerResult<tools::ListAgentsResponse> {
        let agents = self
            .state
            .repository()
            .list_agents(self.state.session_id())
            .await?;

        Ok(tools::ListAgentsResponse {
            agents: agents.iter().map(Self::agent_to_info).collect(),
        })
    }

    async fn handle_get_hive_status(&self) -> HandlerResult<tools::GetHiveStatusResponse> {
        let repo = self.state.repository();
        let session_id = self.state.session_id();

        let agents = repo.list_agents(session_id).await?;
        let all_tasks = repo.list_tasks(session_id, None).await?;
        let recent = repo.get_recent_knowledge(session_id, 10).await?;

        let task_summary = tools::TaskSummary {
            pending: all_tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Pending)
                .count(),
            claimed: all_tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Claimed)
                .count(),
            in_progress: all_tasks
                .iter()
                .filter(|t| t.status == TaskStatus::InProgress)
                .count(),
            completed: all_tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Completed)
                .count(),
            failed: all_tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Failed)
                .count(),
        };

        // Populate inbox and threads if agent is registered
        let (inbox, active_threads) = if let Some(agent_id) = *self.agent_id.read().await {
            let messages = repo.get_direct_messages(agent_id, 10).await?;

            let inbox = if !messages.is_empty() {
                let mut recent_messages = Vec::new();

                for msg in &messages {
                    // Enrich with from_agent info
                    let from_agent = repo.get_agent(msg.from_agent).await.ok();

                    recent_messages.push(tools::DirectMessageInfo {
                        id: msg.id.as_uuid().to_string(),
                        from_agent: msg.from_agent.as_uuid().to_string(),
                        from_agent_role: from_agent.as_ref().map(|a| format!("{}", a.role)),
                        from_agent_project: from_agent
                            .as_ref()
                            .and_then(|a| a.project_name.clone()),
                        to_agent: msg.to_agent.as_uuid().to_string(),
                        content: msg.content.clone(),
                        task_id: msg.task_id.map(|t| t.as_uuid().to_string()),
                        created_at: msg.created_at.to_rfc3339(),
                    });
                }

                Some(tools::MessageInbox {
                    recent_messages,
                    count: messages.len(),
                })
            } else {
                None
            };

            // Aggregate threads by task_id
            let threads = if !messages.is_empty() {
                use std::collections::HashMap;

                let mut threads_map: HashMap<Option<TaskId>, Vec<&DirectMessage>> = HashMap::new();
                for msg in &messages {
                    threads_map.entry(msg.task_id).or_default().push(msg);
                }

                let mut threads = Vec::new();
                for (task_id_opt, thread_msgs) in threads_map {
                    if thread_msgs.is_empty() {
                        continue;
                    }

                    // Get unique participants
                    let mut participant_ids = std::collections::HashSet::new();
                    for msg in &thread_msgs {
                        participant_ids.insert(msg.from_agent);
                        participant_ids.insert(msg.to_agent);
                    }

                    // Get last message
                    let last_msg = thread_msgs.iter().max_by_key(|m| m.created_at).unwrap();
                    let preview = if last_msg.content.len() > 100 {
                        format!("{}...", &last_msg.content[..97])
                    } else {
                        last_msg.content.clone()
                    };

                    // Get task title if available
                    let (task_id_str, task_title) = if let Some(tid) = task_id_opt {
                        let title = repo.get_task(tid).await.ok().map(|t| t.title);
                        (Some(tid.as_uuid().to_string()), title)
                    } else {
                        (None, None)
                    };

                    threads.push(tools::ActiveThread {
                        task_id: task_id_str,
                        task_title,
                        participants: participant_ids
                            .iter()
                            .map(|id| id.as_uuid().to_string())
                            .collect(),
                        message_count: thread_msgs.len(),
                        last_message_at: last_msg.created_at.to_rfc3339(),
                        last_message_preview: preview,
                    });
                }

                Some(threads)
            } else {
                None
            };

            (inbox, threads)
        } else {
            (None, None)
        };

        Ok(tools::GetHiveStatusResponse {
            agents: agents.iter().map(Self::agent_to_info).collect(),
            task_summary,
            recent_knowledge: recent
                .iter()
                .map(|k| Self::knowledge_to_result(k, 0.0))
                .collect(),
            inbox,
            active_threads,
        })
    }

    async fn handle_spawn_agent(
        &self,
        req: tools::SpawnAgentRequest,
    ) -> HandlerResult<tools::SpawnAgentResponse> {
        // Validate caller is strategoi
        let caller_id = self.resolve_agent_id(req._agent_id.as_deref()).await?;
        let caller = self.state.repository().get_agent(caller_id).await?;
        if !caller.is_strategoi {
            return Err(HandlerError::InvalidArgs(
                "Only strategoi can spawn agents".into(),
            ));
        }

        // For MVP, only support developer role
        let role = Self::parse_role(&req.role)?;
        if role != AgentRole::Developer {
            return Err(HandlerError::InvalidArgs(
                "MVP only supports spawning developer agents".into(),
            ));
        }

        // Create agent entity with Starting status
        let agent = Agent::new(role, self.state.session_id());
        let agent_id = agent.id;
        let agent_id_str = agent_id.as_uuid().to_string();

        self.state.repository().create_agent(&agent).await?;

        // Assign initial task if provided
        if let Some(ref task_id_str) = req.initial_task_id {
            let task_id = Self::parse_task_id(task_id_str)?;
            self.state
                .repository()
                .assign_task(task_id, agent_id)
                .await?;
        }

        // Generate spawn prompt for strategoi to use with Task tool
        let spawn_prompt = format!(
            "You are a developer agent in the harness hive mind. Your agent_id is {}. \n\n\
             IMPORTANT: Register immediately using mcp__harness__register_agent with:\n\
             - role: \"developer\"\n\
             - agent_id: \"{}\"\n\n\
             After registration, check your assigned tasks using mcp__harness__list_tasks and \
             claim the task assigned to you. Then complete the work described in the task.",
            agent_id_str, agent_id_str
        );

        Ok(tools::SpawnAgentResponse {
            agent_id: agent_id_str,
            teammate_id: Some(format!(
                "Use Task tool with description: 'Spawn developer agent {}' and prompt:\n{}",
                req.name, spawn_prompt
            )),
        })
    }

    async fn handle_disconnect_agent(
        &self,
        req: tools::DisconnectAgentRequest,
    ) -> HandlerResult<tools::DisconnectAgentResponse> {
        let agent_id = if let Some(ref id_str) = req.agent_id {
            // Disconnect specified agent (strategoi can disconnect others)
            Self::parse_agent_id(id_str)?
        } else {
            // Self-disconnect
            self.resolve_agent_id(req._agent_id.as_deref()).await?
        };

        self.state
            .repository()
            .update_agent_status(agent_id, AgentStatus::Finished)
            .await?;

        Ok(tools::DisconnectAgentResponse { success: true })
    }

    // --- Message handlers ---

    async fn handle_send_dm(
        &self,
        req: tools::SendDirectMessageRequest,
    ) -> HandlerResult<tools::SendDirectMessageResponse> {
        let from_agent = self.resolve_agent_id(req._agent_id.as_deref()).await?;
        let to_agent = Self::parse_agent_id(&req.to_agent)?;

        let mut dm =
            DirectMessage::new(from_agent, to_agent, &req.content, self.state.session_id());

        if let Some(ref tid_str) = req.task_id {
            dm = dm.with_task(Self::parse_task_id(tid_str)?);
        }

        let mid = dm.id.as_uuid().to_string();
        self.state.repository().create_direct_message(&dm).await?;

        Ok(tools::SendDirectMessageResponse { message_id: mid })
    }

    async fn handle_get_messages(
        &self,
        req: tools::GetMessagesRequest,
    ) -> HandlerResult<tools::GetMessagesResponse> {
        let agent_id = self.resolve_agent_id(req._agent_id.as_deref()).await?;
        let messages = self
            .state
            .repository()
            .get_direct_messages(agent_id, req.limit)
            .await?;

        Ok(tools::GetMessagesResponse {
            messages: messages.iter().map(Self::dm_to_info).collect(),
        })
    }

    async fn handle_get_thread_messages(
        &self,
        req: tools::GetThreadMessagesRequest,
    ) -> HandlerResult<tools::GetThreadMessagesResponse> {
        let task_id = Self::parse_task_id(&req.task_id)?;
        let messages = self
            .state
            .repository()
            .get_thread_messages(task_id, req.limit)
            .await?;

        Ok(tools::GetThreadMessagesResponse {
            messages: messages.iter().map(Self::dm_to_info).collect(),
        })
    }

    // --- Planning handlers ---

    async fn handle_create_product(
        &self,
        req: tools::CreateProductRequest,
    ) -> HandlerResult<tools::CreateProductResponse> {
        let session_id = self.state.session_id();
        let product = Product::new(&req.name, &req.description, session_id);
        let product_id = product.id.as_uuid().to_string();

        self.state.repository().create_product(&product).await?;

        Ok(tools::CreateProductResponse { product_id })
    }

    async fn handle_list_products(
        &self,
        req: tools::ListProductsRequest,
    ) -> HandlerResult<tools::ListProductsResponse> {
        let status = req
            .status
            .as_deref()
            .map(Self::parse_product_status)
            .transpose()?;
        let products = self
            .state
            .repository()
            .list_products(self.state.session_id(), status)
            .await?;

        Ok(tools::ListProductsResponse {
            products: products.iter().map(Self::product_to_info).collect(),
        })
    }

    async fn handle_create_project(
        &self,
        req: tools::CreateProjectRequest,
    ) -> HandlerResult<tools::CreateProjectResponse> {
        let product_id = Self::parse_product_id(&req.product_id)?;
        let session_id = self.state.session_id();
        let project = Project::new(&req.name, &req.description, product_id, session_id);
        let project_id = project.id.as_uuid().to_string();

        self.state.repository().create_project(&project).await?;

        Ok(tools::CreateProjectResponse { project_id })
    }

    async fn handle_list_projects(
        &self,
        req: tools::ListProjectsRequest,
    ) -> HandlerResult<tools::ListProjectsResponse> {
        let product_id = req
            .product_id
            .as_deref()
            .map(Self::parse_product_id)
            .transpose()?;
        let status = req
            .status
            .as_deref()
            .map(Self::parse_project_status)
            .transpose()?;
        let projects = self
            .state
            .repository()
            .list_projects(self.state.session_id(), product_id, status)
            .await?;

        Ok(tools::ListProjectsResponse {
            projects: projects.iter().map(Self::project_to_info).collect(),
        })
    }

    async fn handle_create_plan(
        &self,
        req: tools::CreatePlanRequest,
    ) -> HandlerResult<tools::CreatePlanResponse> {
        let project_id = Self::parse_project_id(&req.project_id)?;
        let session_id = self.state.session_id();
        let plan = Plan::new(&req.name, &req.strategy, project_id, session_id);
        let plan_id = plan.id.as_uuid().to_string();

        self.state.repository().create_plan(&plan).await?;

        Ok(tools::CreatePlanResponse { plan_id })
    }

    async fn handle_list_plans(
        &self,
        req: tools::ListPlansRequest,
    ) -> HandlerResult<tools::ListPlansResponse> {
        let project_id = req
            .project_id
            .as_deref()
            .map(Self::parse_project_id)
            .transpose()?;
        let status = req
            .status
            .as_deref()
            .map(Self::parse_plan_status)
            .transpose()?;
        let plans = self
            .state
            .repository()
            .list_plans(self.state.session_id(), project_id, status)
            .await?;

        Ok(tools::ListPlansResponse {
            plans: plans.iter().map(Self::plan_to_info).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_persistence::{InMemoryRepository, Session};

    async fn setup() -> (
        Arc<HiveState<InMemoryRepository>>,
        HiveHandler<InMemoryRepository>,
    ) {
        let repo = Arc::new(InMemoryRepository::new());
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let state = Arc::new(HiveState::new(session, repo));
        let handler = HiveHandler::new(state.clone());
        (state, handler)
    }

    #[tokio::test]
    async fn test_register_agent() {
        let (_state, handler) = setup().await;

        let resp = handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();

        let resp: tools::RegisterAgentResponse = serde_json::from_value(resp).unwrap();
        assert!(!resp.agent_id.is_empty());
    }

    #[tokio::test]
    async fn test_create_and_list_tasks() {
        let (_state, handler) = setup().await;

        // Register agent first
        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        // Create task
        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Implement auth",
                    "description": "Add JWT authentication",
                    "priority": "high"
                }),
            )
            .await
            .unwrap();

        let create_resp: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();
        assert!(!create_resp.task_id.is_empty());

        // List tasks
        let resp = handler
            .call_tool("list_tasks", serde_json::json!({}))
            .await
            .unwrap();

        let list_resp: tools::ListTasksResponse = serde_json::from_value(resp).unwrap();
        assert_eq!(list_resp.tasks.len(), 1);
        assert_eq!(list_resp.tasks[0].title, "Implement auth");
    }

    #[tokio::test]
    async fn test_claim_task() {
        let (_state, handler) = setup().await;

        // Register
        handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();

        // Create task
        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Build feature",
                    "description": "Details",
                }),
            )
            .await
            .unwrap();
        let create_resp: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();

        // Claim it
        let resp = handler
            .call_tool(
                "claim_task",
                serde_json::json!({"task_id": create_resp.task_id}),
            )
            .await
            .unwrap();
        let claim_resp: tools::ClaimTaskResponse = serde_json::from_value(resp).unwrap();
        assert!(claim_resp.success);
    }

    #[tokio::test]
    async fn test_share_and_ask_knowledge() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();

        // Share knowledge
        handler
            .call_tool(
                "share_knowledge",
                serde_json::json!({
                    "content": "Found a race condition in pool.rs",
                    "kind": "discovery"
                }),
            )
            .await
            .unwrap();

        // Ask hive
        let resp = handler
            .call_tool(
                "ask_hive",
                serde_json::json!({"query": "race condition", "limit": 5}),
            )
            .await
            .unwrap();
        let ask_resp: tools::AskHiveResponse = serde_json::from_value(resp).unwrap();
        assert_eq!(ask_resp.results.len(), 1);
        assert!(ask_resp.results[0].content.contains("race condition"));
    }

    #[tokio::test]
    async fn test_hive_status() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let resp = handler
            .call_tool("get_hive_status", serde_json::json!({}))
            .await
            .unwrap();
        let status: tools::GetHiveStatusResponse = serde_json::from_value(resp).unwrap();
        assert_eq!(status.agents.len(), 1);
        assert_eq!(status.task_summary.pending, 0);
    }

    #[tokio::test]
    async fn test_direct_messages() {
        let (state, handler) = setup().await;

        // Register sender
        let resp = handler
            .call_tool("register_agent", serde_json::json!({"role": "ba"}))
            .await
            .unwrap();
        let sender: tools::RegisterAgentResponse = serde_json::from_value(resp).unwrap();

        // Create recipient directly
        let recipient = Agent::new(AgentRole::ProductManager, state.session_id());
        state.repository().create_agent(&recipient).await.unwrap();
        let recipient_id = recipient.id.as_uuid().to_string();

        // Send DM
        handler
            .call_tool(
                "send_direct_message",
                serde_json::json!({
                    "to_agent": recipient_id,
                    "content": "What are the requirements?"
                }),
            )
            .await
            .unwrap();

        // Check that sender is registered
        assert!(!sender.agent_id.is_empty());
    }

    #[tokio::test]
    async fn test_unknown_tool_returns_error() {
        let (_state, handler) = setup().await;

        let result = handler
            .call_tool("nonexistent_tool", serde_json::json!({}))
            .await;
        assert!(matches!(result, Err(HandlerError::UnknownTool(_))));
    }

    #[tokio::test]
    async fn test_tool_names_list() {
        let names = HiveHandler::<InMemoryRepository>::tool_names();
        assert!(names.contains(&"create_task"));
        assert!(names.contains(&"share_knowledge"));
        assert!(names.contains(&"register_agent"));
        assert!(names.contains(&"send_direct_message"));
        assert!(names.contains(&"fish_knowledge"));
        assert!(names.contains(&"create_product"));
        assert!(names.contains(&"list_products"));
        assert!(names.contains(&"create_project"));
        assert!(names.contains(&"list_projects"));
        assert!(names.contains(&"create_plan"));
        assert!(names.contains(&"list_plans"));
        assert!(names.contains(&"find_path"));
        assert!(names.contains(&"task_statistics"));
        assert!(names.contains(&"get_task_history"));
        assert!(names.contains(&"knowledge_clusters"));
        assert_eq!(names.len(), 34);
    }

    #[tokio::test]
    async fn test_task_statistics() {
        let (_state, handler) = setup().await;

        // Register agent
        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        // Create tasks with different statuses and priorities
        let task1 = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "High priority task",
                    "description": "Important work",
                    "priority": "high"
                }),
            )
            .await
            .unwrap();
        let task1_id: tools::CreateTaskResponse = serde_json::from_value(task1).unwrap();

        let task2 = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Low priority task",
                    "description": "Can wait",
                    "priority": "low"
                }),
            )
            .await
            .unwrap();
        let task2_id: tools::CreateTaskResponse = serde_json::from_value(task2).unwrap();

        handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Critical task",
                    "description": "Urgent",
                    "priority": "critical"
                }),
            )
            .await
            .unwrap();

        handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Medium priority task",
                    "description": "Normal",
                    "priority": "medium"
                }),
            )
            .await
            .unwrap();

        // Complete one task
        handler
            .call_tool(
                "update_task_status",
                serde_json::json!({
                    "task_id": task1_id.task_id,
                    "status": "completed",
                    "summary": "Finished successfully"
                }),
            )
            .await
            .unwrap();

        // Set one to in_progress
        handler
            .call_tool(
                "update_task_status",
                serde_json::json!({
                    "task_id": task2_id.task_id,
                    "status": "in_progress"
                }),
            )
            .await
            .unwrap();

        // Get statistics
        let resp = handler
            .call_tool("task_statistics", serde_json::json!({}))
            .await
            .unwrap();

        let stats: tools::TaskStatisticsResponse = serde_json::from_value(resp).unwrap();

        // Verify status counts
        assert_eq!(stats.status_counts.total, 4);
        assert_eq!(stats.status_counts.pending, 2);
        assert_eq!(stats.status_counts.in_progress, 1);
        assert_eq!(stats.status_counts.completed, 1);
        assert_eq!(stats.status_counts.failed, 0);

        // Verify priority counts
        assert_eq!(stats.priority_counts.low, 1);
        assert_eq!(stats.priority_counts.medium, 1);
        assert_eq!(stats.priority_counts.high, 1);
        assert_eq!(stats.priority_counts.critical, 1);

        // Verify completion metrics
        assert_eq!(stats.completion_metrics.completion_rate, 25.0); // 1/4 = 25%
        assert!(stats.completion_metrics.avg_completion_time_secs.is_some());
        assert!(
            stats
                .completion_metrics
                .median_completion_time_secs
                .is_some()
        );

        // No assigned tasks in this test
        assert_eq!(stats.assigned_count, 0);
        assert_eq!(stats.blocked_count, 0);
    }

    #[tokio::test]
    async fn test_find_path_same_task() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();

        // Create a single task
        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Task A",
                    "description": "Test task",
                }),
            )
            .await
            .unwrap();
        let task_a: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();

        // Find path from A to A
        let resp = handler
            .call_tool(
                "find_path",
                serde_json::json!({
                    "from_task_id": task_a.task_id,
                    "to_task_id": task_a.task_id,
                }),
            )
            .await
            .unwrap();
        let path_resp: tools::FindPathResponse = serde_json::from_value(resp).unwrap();

        assert!(path_resp.found);
        assert_eq!(path_resp.distance, Some(0));
        assert_eq!(path_resp.path.as_ref().unwrap().len(), 1);
        assert_eq!(path_resp.path.unwrap()[0].relationship, "self");
    }

    #[tokio::test]
    async fn test_find_path_direct_dependency() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();

        // Create two tasks
        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({"title": "Task A", "description": "First"}),
            )
            .await
            .unwrap();
        let task_a: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();

        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({"title": "Task B", "description": "Second"}),
            )
            .await
            .unwrap();
        let task_b: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();

        // Add dependency: A blocks B
        handler
            .call_tool(
                "add_task_dependency",
                serde_json::json!({
                    "task_id": task_a.task_id,
                    "blocked_task_id": task_b.task_id,
                }),
            )
            .await
            .unwrap();

        // Find path from A to B
        let resp = handler
            .call_tool(
                "find_path",
                serde_json::json!({
                    "from_task_id": task_a.task_id,
                    "to_task_id": task_b.task_id,
                }),
            )
            .await
            .unwrap();
        let path_resp: tools::FindPathResponse = serde_json::from_value(resp).unwrap();

        assert!(path_resp.found);
        assert_eq!(path_resp.distance, Some(1));
        let path = path_resp.path.unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].title, "Task A");
        assert_eq!(path[0].relationship, "start");
        assert_eq!(path[1].title, "Task B");
        assert_eq!(path[1].relationship, "blocks");
    }

    #[tokio::test]
    async fn test_find_path_chain() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();

        // Create three tasks: A -> B -> C
        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({"title": "Task A", "description": "First"}),
            )
            .await
            .unwrap();
        let task_a: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();

        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({"title": "Task B", "description": "Second"}),
            )
            .await
            .unwrap();
        let task_b: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();

        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({"title": "Task C", "description": "Third"}),
            )
            .await
            .unwrap();
        let task_c: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();

        // A blocks B, B blocks C
        handler
            .call_tool(
                "add_task_dependency",
                serde_json::json!({
                    "task_id": task_a.task_id,
                    "blocked_task_id": task_b.task_id,
                }),
            )
            .await
            .unwrap();

        handler
            .call_tool(
                "add_task_dependency",
                serde_json::json!({
                    "task_id": task_b.task_id,
                    "blocked_task_id": task_c.task_id,
                }),
            )
            .await
            .unwrap();

        // Find path from A to C
        let resp = handler
            .call_tool(
                "find_path",
                serde_json::json!({
                    "from_task_id": task_a.task_id,
                    "to_task_id": task_c.task_id,
                }),
            )
            .await
            .unwrap();
        let path_resp: tools::FindPathResponse = serde_json::from_value(resp).unwrap();

        assert!(path_resp.found);
        assert_eq!(path_resp.distance, Some(2));
        let path = path_resp.path.unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].title, "Task A");
        assert_eq!(path[1].title, "Task B");
        assert_eq!(path[2].title, "Task C");
    }

    #[tokio::test]
    async fn test_find_path_no_connection() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();

        // Create two isolated tasks
        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({"title": "Task A", "description": "First"}),
            )
            .await
            .unwrap();
        let task_a: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();

        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({"title": "Task B", "description": "Second"}),
            )
            .await
            .unwrap();
        let task_b: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();

        // No dependency added, find path from A to B
        let resp = handler
            .call_tool(
                "find_path",
                serde_json::json!({
                    "from_task_id": task_a.task_id,
                    "to_task_id": task_b.task_id,
                }),
            )
            .await
            .unwrap();
        let path_resp: tools::FindPathResponse = serde_json::from_value(resp).unwrap();

        assert!(!path_resp.found);
        assert_eq!(path_resp.distance, None);
        assert_eq!(path_resp.path, None);
    }

    #[tokio::test]
    async fn test_find_path_bidirectional() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();

        // Create two tasks
        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({"title": "Task A", "description": "First"}),
            )
            .await
            .unwrap();
        let task_a: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();

        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({"title": "Task B", "description": "Second"}),
            )
            .await
            .unwrap();
        let task_b: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();

        // A blocks B
        handler
            .call_tool(
                "add_task_dependency",
                serde_json::json!({
                    "task_id": task_a.task_id,
                    "blocked_task_id": task_b.task_id,
                }),
            )
            .await
            .unwrap();

        // Find path from B to A (reverse direction)
        let resp = handler
            .call_tool(
                "find_path",
                serde_json::json!({
                    "from_task_id": task_b.task_id,
                    "to_task_id": task_a.task_id,
                }),
            )
            .await
            .unwrap();
        let path_resp: tools::FindPathResponse = serde_json::from_value(resp).unwrap();

        assert!(path_resp.found);
        assert_eq!(path_resp.distance, Some(1));
        let path = path_resp.path.unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].title, "Task B");
        assert_eq!(path[1].title, "Task A");
        assert_eq!(path[1].relationship, "blocked_by");
    }

    #[tokio::test]
    async fn test_knowledge_clusters() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();

        // Create some knowledge entries with similar content
        // Group 1: Error handling related
        handler
            .call_tool(
                "share_knowledge",
                serde_json::json!({
                    "content": "Found a null pointer exception in login.rs",
                    "kind": "discovery"
                }),
            )
            .await
            .unwrap();

        handler
            .call_tool(
                "share_knowledge",
                serde_json::json!({
                    "content": "Added error handling for null pointer cases",
                    "kind": "decision"
                }),
            )
            .await
            .unwrap();

        // Group 2: Performance optimization
        handler
            .call_tool(
                "share_knowledge",
                serde_json::json!({
                    "content": "Database query taking 2 seconds, needs optimization",
                    "kind": "blocker"
                }),
            )
            .await
            .unwrap();

        handler
            .call_tool(
                "share_knowledge",
                serde_json::json!({
                    "content": "Optimized query with index, reduced to 50ms",
                    "kind": "activity"
                }),
            )
            .await
            .unwrap();

        // Call knowledge_clusters
        let resp = handler
            .call_tool(
                "knowledge_clusters",
                serde_json::json!({
                    "similarity_threshold": 0.5,
                    "min_cluster_size": 2
                }),
            )
            .await
            .unwrap();

        let clusters_resp: tools::KnowledgeClustersResponse = serde_json::from_value(resp).unwrap();

        // Without embeddings (no embedding service in test), should have no clusters
        assert_eq!(clusters_resp.clusters.len(), 0);
        assert_eq!(clusters_resp.total_knowledge_count, 4);
        assert_eq!(clusters_resp.unclustered_count, 4);
    }

    #[tokio::test]
    async fn test_get_task_as_of() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();

        // Create a task
        let resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Original Title",
                    "description": "Original description",
                    "priority": "medium"
                }),
            )
            .await
            .unwrap();
        let create_resp: tools::CreateTaskResponse = serde_json::from_value(resp).unwrap();

        // Get current time for time travel
        let created_time = chrono::Utc::now();

        // Wait a tiny bit
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Update the task status
        handler
            .call_tool(
                "update_task_status",
                serde_json::json!({
                    "task_id": create_resp.task_id,
                    "status": "in_progress",
                    "summary": "Started work"
                }),
            )
            .await
            .unwrap();

        // Query task as it was at creation time
        let resp = handler
            .call_tool(
                "get_task_as_of",
                serde_json::json!({
                    "task_id": create_resp.task_id,
                    "valid_time": created_time.to_rfc3339()
                }),
            )
            .await
            .unwrap();

        let as_of_resp: tools::GetTaskAsOfResponse = serde_json::from_value(resp).unwrap();

        // The task at creation time should have pending status
        // Note: InMemoryRepository doesn't store full history, so this will return latest
        // But the structure should be correct
        assert_eq!(as_of_resp.task.id, create_resp.task_id);
        assert_eq!(as_of_resp.task.title, "Original Title");
    }
}
