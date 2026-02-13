//! MCP tool handler implementation.
//!
//! Each method implements a tool that can be called by Claude agents
//! via the MCP protocol. The handler dispatches tool calls by name.

use std::sync::Arc;

use harness_persistence::{
    Agent, AgentId, AgentRole, DirectMessage, Knowledge, KnowledgeKind, Plan, PlanId, PlanStatus,
    Priority, Product, ProductId, ProductStatus, Project, ProjectId, ProjectStatus, Repository,
    RepositoryError, Task, TaskId, TaskStatus,
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
            "share_knowledge",
            "ask_hive",
            "fish_knowledge",
            "register_agent",
            "list_agents",
            "get_hive_status",
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

    fn parse_plan_id(s: &str) -> HandlerResult<PlanId> {
        let uuid = uuid::Uuid::parse_str(s)
            .map_err(|_| HandlerError::Parse(format!("Invalid plan ID: {s}")))?;
        Ok(PlanId::from_uuid(uuid))
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
        }
    }

    fn agent_to_info(a: &Agent) -> tools::AgentInfo {
        tools::AgentInfo {
            id: a.id.as_uuid().to_string(),
            role: format!("{}", a.role),
            status: format!("{:?}", a.status).to_lowercase(),
            current_task: a.current_task.map(|t| t.as_uuid().to_string()),
            is_strategoi: a.is_strategoi,
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
        let agent_id = self.require_agent_id().await?;

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

        Ok(tools::GetTaskContextResponse {
            task: Self::task_to_info(&task),
            knowledge: knowledge
                .iter()
                .map(|k| Self::knowledge_to_result(k, 0.0))
                .collect(),
            subtasks: subtasks.iter().map(Self::task_to_info).collect(),
        })
    }

    // --- Knowledge handlers ---

    async fn handle_share_knowledge(
        &self,
        req: tools::ShareKnowledgeRequest,
    ) -> HandlerResult<tools::ShareKnowledgeResponse> {
        let agent_id = self.require_agent_id().await?;
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
        if let Some(embedding) = &start_knowledge.embedding {
            if let Ok(similar) = self
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
        }

        // 2. Get graph-connected knowledge (structural component)
        // Knowledge connected via same task
        if let Some(task_id) = start_knowledge.task_id {
            if let Ok(task_knowledge) = self.state.repository().get_task_knowledge(task_id).await {
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
                    vector_similarity: k.embedding.as_ref().and_then(|_| Some(score * 0.7)),
                    connection_paths: paths,
                    created_at: k.created_at.to_rfc3339(),
                    task_id: k.task_id.map(|t| t.as_uuid().to_string()),
                })
                .collect(),
        })
    }

    // --- Agent handlers ---

    async fn handle_register_agent(
        &self,
        req: tools::RegisterAgentRequest,
    ) -> HandlerResult<tools::RegisterAgentResponse> {
        let role = Self::parse_role(&req.role)?;
        let agent = Agent::new(role, self.state.session_id());
        let aid = agent.id;

        self.state.repository().create_agent(&agent).await?;

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

        Ok(tools::GetHiveStatusResponse {
            agents: agents.iter().map(Self::agent_to_info).collect(),
            task_summary,
            recent_knowledge: recent
                .iter()
                .map(|k| Self::knowledge_to_result(k, 0.0))
                .collect(),
        })
    }

    // --- Message handlers ---

    async fn handle_send_dm(
        &self,
        req: tools::SendDirectMessageRequest,
    ) -> HandlerResult<tools::SendDirectMessageResponse> {
        let from_agent = self.require_agent_id().await?;
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
        let agent_id = self.require_agent_id().await?;
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
        assert_eq!(names.len(), 21);
    }
}
