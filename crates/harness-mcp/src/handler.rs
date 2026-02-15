//! MCP tool handler implementation.
//!
//! Each method implements a tool that can be called by Claude agents
//! via the MCP protocol. The handler dispatches tool calls by name.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use harness_persistence::{
    Agent, AgentId, AgentRole, AgentStatus, DirectMessage, Knowledge, KnowledgeKind,
    LearningTrigger, PatternQuery, Plan, PlanStatus, Priority, Product, ProductId, ProductStatus,
    Project, ProjectId, ProjectStatus, Repository, RepositoryError, Task, TaskId, TaskPattern,
    TaskStatus, TriggerKind,
};
use tokio::sync::RwLock;

use crate::state::{AgentSpawnSpec, HiveState};
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

    #[error("internal error: {0}")]
    InternalError(String),
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
    /// The MCP session ID for this request (set by server).
    mcp_session_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandshakeMode {
    Ring,
    FullMesh,
}

impl HandshakeMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ring => "ring",
            Self::FullMesh => "full_mesh",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TeamTemplateKind {
    Feature,
    Bugfix,
    Incident,
}

impl TeamTemplateKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Feature => "feature",
            Self::Bugfix => "bugfix",
            Self::Incident => "incident",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NudgeMode {
    Auto,
    Nudge,
    Replan,
}

#[derive(Debug, Clone)]
struct TeamTemplateMember {
    name: String,
    role: String,
    cli_command: String,
    cli_args: Vec<String>,
    custom_prompt: Option<String>,
    directive: Option<String>,
}

struct SpawnWorkerParams<'a> {
    role: &'a str,
    cli_command: &'a str,
    cli_args: &'a [String],
    custom_prompt: Option<&'a str>,
    directive: Option<&'a str>,
    poll_interval_secs: u64,
    initial_task_id: Option<&'a str>,
}

impl<R: Repository + 'static> HiveHandler<R> {
    /// Create a new handler.
    pub fn new(state: Arc<HiveState<R>>) -> Self {
        Self {
            state,
            agent_id: RwLock::new(None),
            mcp_session_id: None,
        }
    }

    /// Set the MCP session ID for this handler.
    pub fn with_mcp_session(mut self, session_id: String) -> Self {
        self.mcp_session_id = Some(session_id);
        self
    }

    /// Get a reference to the shared state (for spawning new handlers in tests).
    pub fn state_ref(&self) -> &Arc<HiveState<R>> {
        &self.state
    }

    /// Get the agent ID for this connection (if registered).
    pub async fn agent_id(&self) -> Option<AgentId> {
        *self.agent_id.read().await
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
            "dispatch_ready_tasks" => {
                let req: tools::DispatchReadyTasksRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_dispatch_ready_tasks(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "nudge_or_replan" => {
                let req: tools::NudgeOrReplanRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_nudge_or_replan(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "task_completion_gate" => {
                let req: tools::TaskCompletionGateRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_task_completion_gate(req).await?;
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
            "spawn_team_and_handshake" => {
                let req: tools::SpawnTeamAndHandshakeRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_spawn_team_and_handshake(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "spawn_team_from_template" => {
                let req: tools::SpawnTeamFromTemplateRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_spawn_team_from_template(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "refresh_session" => {
                let req: tools::RefreshSessionRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_refresh_session(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "disconnect_agent" => {
                let req: tools::DisconnectAgentRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_disconnect_agent(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "list_processes" => {
                let req: tools::ListProcessesRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_list_processes(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "kill_process" => {
                let req: tools::KillProcessRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_kill_process(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "cleanup_stale_agents" => {
                let req: tools::CleanupStaleAgentsRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_cleanup_stale_agents(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "supervise_team" => {
                let req: tools::SuperviseTeamRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_supervise_team(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "team_runbook_prompt" => {
                let req: tools::TeamRunbookPromptRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_team_runbook_prompt(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "hive_observability_snapshot" => {
                let req: tools::HiveObservabilitySnapshotRequest =
                    serde_json::from_value(arguments)
                        .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_hive_observability_snapshot(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "get_process_output" => {
                let req: tools::GetProcessOutputRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_get_process_output(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "collect_agent_artifacts" => {
                let req: tools::CollectAgentArtifactsRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_collect_agent_artifacts(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "command_agent" => {
                let req: tools::CommandAgentRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_command_agent(req).await?;
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

            // Nova experimental features
            "find_temporal_path" => {
                let req: tools::FindTemporalPathRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_find_temporal_path(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "navigate_semantic_graph" => {
                let req: tools::NavigateSemanticGraphRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_navigate_semantic_graph(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "discover_semantic_clusters" => {
                let req: tools::DiscoverSemanticClustersRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_discover_semantic_clusters(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "generate_graph_layout" => {
                let req: tools::GenerateGraphLayoutRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_generate_graph_layout(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "find_activity_resonance" => {
                let req: tools::FindActivityResonanceRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_find_activity_resonance(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "compare_temporal_snapshots" => {
                let req: tools::CompareTemporalSnapshotsRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_compare_temporal_snapshots(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "compute_concept_analogy" => {
                let req: tools::ComputeConceptAnalogyRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_compute_concept_analogy(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "predict_missing_connections" => {
                let req: tools::PredictMissingConnectionsRequest =
                    serde_json::from_value(arguments)
                        .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_predict_missing_connections(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "analyze_semantic_spectrum" => {
                let req: tools::AnalyzeSemanticSpectrumRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_analyze_semantic_spectrum(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "predict_semantic_trajectory" => {
                let req: tools::PredictSemanticTrajectoryRequest =
                    serde_json::from_value(arguments)
                        .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_predict_semantic_trajectory(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "generate_history_narrative" => {
                let req: tools::GenerateHistoryNarrativeRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_generate_history_narrative(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }

            // SONA MicroLoRA tools
            "record_agent_trajectory" => {
                let req: tools::RecordAgentTrajectoryRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_record_agent_trajectory(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "get_agent_lora_state" => {
                let req: tools::GetAgentLoraStateRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_get_agent_lora_state(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "apply_agent_optimization" => {
                let req: tools::ApplyAgentOptimizationRequest =
                    serde_json::from_value(arguments)
                        .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_apply_agent_optimization(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "persist_agent_lora" => {
                let req: tools::PersistAgentLoraRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_persist_agent_lora(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "restore_agent_lora" => {
                let req: tools::RestoreAgentLoraRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_restore_agent_lora(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }

            // SONA integration tools
            "get_task_trajectory" => {
                let req: tools::GetTaskTrajectoryRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_get_task_trajectory(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "query_reasoning_bank" => {
                let req: tools::QueryReasoningBankRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_query_reasoning_bank(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "get_learning_status" => {
                let req: tools::GetLearningStatusRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_get_learning_status(req).await?;
                Ok(serde_json::to_value(resp).unwrap())
            }
            "trigger_learning_cycle" => {
                let req: tools::TriggerLearningCycleRequest = serde_json::from_value(arguments)
                    .map_err(|e| HandlerError::InvalidArgs(e.to_string()))?;
                let resp = self.handle_trigger_learning_cycle(req).await?;
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
            "dispatch_ready_tasks",
            "nudge_or_replan",
            "task_completion_gate",
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
            "spawn_team_and_handshake",
            "spawn_team_from_template",
            "refresh_session",
            "disconnect_agent",
            "send_direct_message",
            "get_messages",
            "get_thread_messages",
            "supervise_team",
            "team_runbook_prompt",
            "hive_observability_snapshot",
            "collect_agent_artifacts",
            "create_product",
            "list_products",
            "create_project",
            "list_projects",
            "create_plan",
            "list_plans",
            "find_temporal_path",
            "navigate_semantic_graph",
            "discover_semantic_clusters",
            "generate_graph_layout",
            "find_activity_resonance",
            "compare_temporal_snapshots",
            // SONA MicroLoRA
            "record_agent_trajectory",
            "get_agent_lora_state",
            "apply_agent_optimization",
            "persist_agent_lora",
            "restore_agent_lora",
            // SONA integration
            "get_task_trajectory",
            "query_reasoning_bank",
            "get_learning_status",
            "trigger_learning_cycle",
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
    /// Auto-transitions "starting" agents to "active" on first MCP call.
    async fn resolve_agent_id(&self, provided: Option<&str>) -> HandlerResult<AgentId> {
        let agent_id = if let Some(aid_str) = provided {
            Self::parse_agent_id(aid_str)?
        } else {
            self.require_agent_id().await?
        };

        // Auto-transition "starting" → "active" on first MCP call
        if let Ok(agent) = self.state.repository().get_agent(agent_id).await
            && agent.status == AgentStatus::Starting
        {
            let _ = self
                .state
                .repository()
                .update_agent_status(agent_id, AgentStatus::Active)
                .await;
        }

        Ok(agent_id)
    }

    fn merge_spawn_custom_and_directive(
        custom_prompt: Option<&str>,
        directive: Option<&str>,
    ) -> Option<String> {
        match (custom_prompt, directive) {
            (None, None) => None,
            (Some(custom), None) => Some(custom.to_string()),
            (None, Some(dir)) => Some(format!(
                "IMMEDIATE DIRECTIVE FROM STRATEGOI (execute now):\n{}",
                dir
            )),
            (Some(custom), Some(dir)) => Some(format!(
                "{}\n\nIMMEDIATE DIRECTIVE FROM STRATEGOI (execute now):\n{}",
                custom, dir
            )),
        }
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

    async fn require_strategoi(&self, provided: Option<&str>) -> HandlerResult<AgentId> {
        let caller_id = self.resolve_agent_id(provided).await?;
        let caller = self.state.repository().get_agent(caller_id).await?;
        if !caller.is_strategoi {
            return Err(HandlerError::InvalidArgs(
                "Only strategoi can spawn agents".into(),
            ));
        }
        Ok(caller_id)
    }

    fn parse_handshake_mode(s: &str) -> HandlerResult<HandshakeMode> {
        match s.to_ascii_lowercase().as_str() {
            "ring" => Ok(HandshakeMode::Ring),
            "full_mesh" | "fullmesh" => Ok(HandshakeMode::FullMesh),
            _ => Err(HandlerError::InvalidArgs(format!(
                "Invalid handshake_mode: {s}. Use 'ring' or 'full_mesh'"
            ))),
        }
    }

    fn parse_template_kind(s: &str) -> HandlerResult<TeamTemplateKind> {
        match s.to_ascii_lowercase().as_str() {
            "feature" => Ok(TeamTemplateKind::Feature),
            "bugfix" => Ok(TeamTemplateKind::Bugfix),
            "incident" => Ok(TeamTemplateKind::Incident),
            _ => Err(HandlerError::InvalidArgs(format!(
                "Invalid template: {s}. Use 'feature', 'bugfix', or 'incident'"
            ))),
        }
    }

    fn parse_nudge_mode(s: &str) -> HandlerResult<NudgeMode> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(NudgeMode::Auto),
            "nudge" => Ok(NudgeMode::Nudge),
            "replan" => Ok(NudgeMode::Replan),
            _ => Err(HandlerError::InvalidArgs(format!(
                "Invalid mode: {s}. Use 'auto', 'nudge', or 'replan'"
            ))),
        }
    }

    fn default_completion_checks() -> Vec<String> {
        vec![
            "cargo test -p harness-mcp --lib".to_string(),
            "cargo clippy -p harness-mcp --lib -- -D warnings".to_string(),
        ]
    }

    fn build_team_template_members(
        template: TeamTemplateKind,
        global_cli_command: Option<&str>,
        global_cli_args: Option<&[String]>,
        global_directive: Option<&str>,
    ) -> Vec<TeamTemplateMember> {
        let default_cli_command = global_cli_command.unwrap_or("claude");
        let default_cli_args = global_cli_args.map(|a| a.to_vec()).unwrap_or_else(|| {
            vec![
                "-p".into(),
                "{PROMPT}".into(),
                "--allowedTools".into(),
                "Bash,Read,Edit".into(),
            ]
        });
        let append_global = |base: &str| -> String {
            if let Some(extra) = global_directive
                && !extra.trim().is_empty()
            {
                format!("{}\n\nGlobal directive:\n{}", base, extra.trim())
            } else {
                base.to_string()
            }
        };

        match template {
            TeamTemplateKind::Feature => vec![
                TeamTemplateMember {
                    name: "feature-architect".to_string(),
                    role: "architect".to_string(),
                    cli_command: default_cli_command.to_string(),
                    cli_args: default_cli_args.clone(),
                    custom_prompt: Some(
                        "Own technical decomposition, interface contracts, and risk checkpoints."
                            .to_string(),
                    ),
                    directive: Some(append_global(
                        "Define implementation slices and handoff criteria for coding and testing.",
                    )),
                },
                TeamTemplateMember {
                    name: "feature-developer".to_string(),
                    role: "developer".to_string(),
                    cli_command: default_cli_command.to_string(),
                    cli_args: default_cli_args.clone(),
                    custom_prompt: Some(
                        "Implement high-confidence slices with tight feedback loops and test evidence."
                            .to_string(),
                    ),
                    directive: Some(append_global(
                        "Build core feature path first, then edge behavior and regression coverage.",
                    )),
                },
                TeamTemplateMember {
                    name: "feature-tester".to_string(),
                    role: "tester".to_string(),
                    cli_command: default_cli_command.to_string(),
                    cli_args: default_cli_args,
                    custom_prompt: Some(
                        "Focus on behavior regressions, failure modes, and completion criteria."
                            .to_string(),
                    ),
                    directive: Some(append_global(
                        "Validate acceptance criteria and publish blockers quickly.",
                    )),
                },
            ],
            TeamTemplateKind::Bugfix => vec![
                TeamTemplateMember {
                    name: "bugfix-investigator".to_string(),
                    role: "developer".to_string(),
                    cli_command: default_cli_command.to_string(),
                    cli_args: default_cli_args.clone(),
                    custom_prompt: Some(
                        "Prioritize root-cause analysis, minimal blast radius changes, and regression tests."
                            .to_string(),
                    ),
                    directive: Some(append_global(
                        "Reproduce issue, isolate root cause, and propose minimal corrective patch.",
                    )),
                },
                TeamTemplateMember {
                    name: "bugfix-reviewer".to_string(),
                    role: "tester".to_string(),
                    cli_command: default_cli_command.to_string(),
                    cli_args: default_cli_args,
                    custom_prompt: Some(
                        "Act as independent validator for bugfix quality and side effects.".to_string(),
                    ),
                    directive: Some(append_global(
                        "Confirm fix and verify no regressions in neighboring behavior.",
                    )),
                },
            ],
            TeamTemplateKind::Incident => vec![
                TeamTemplateMember {
                    name: "incident-commander".to_string(),
                    role: "architect".to_string(),
                    cli_command: default_cli_command.to_string(),
                    cli_args: default_cli_args.clone(),
                    custom_prompt: Some(
                        "Coordinate triage order, mitigation actions, and communication cadence."
                            .to_string(),
                    ),
                    directive: Some(append_global(
                        "Drive immediate containment plan and assign concrete follow-ups.",
                    )),
                },
                TeamTemplateMember {
                    name: "incident-responder".to_string(),
                    role: "developer".to_string(),
                    cli_command: default_cli_command.to_string(),
                    cli_args: default_cli_args.clone(),
                    custom_prompt: Some(
                        "Execute mitigations quickly and keep rollback path explicit.".to_string(),
                    ),
                    directive: Some(append_global(
                        "Apply mitigation with explicit verification checks and report outcomes.",
                    )),
                },
                TeamTemplateMember {
                    name: "incident-validation".to_string(),
                    role: "tester".to_string(),
                    cli_command: default_cli_command.to_string(),
                    cli_args: default_cli_args,
                    custom_prompt: Some(
                        "Continuously validate service health and recovery confidence.".to_string(),
                    ),
                    directive: Some(append_global(
                        "Monitor mitigation impact and flag unresolved risk immediately.",
                    )),
                },
            ],
        }
    }

    async fn spawn_worker_agent(&self, params: SpawnWorkerParams<'_>) -> HandlerResult<AgentId> {
        let parsed_role = Self::parse_role(params.role)?;

        // Create agent entity with Starting status
        let agent = Agent::new(parsed_role, self.state.session_id());
        let agent_id = agent.id;
        let agent_id_str = agent_id.as_uuid().to_string();
        self.state.repository().create_agent(&agent).await?;

        // Assign initial task if provided
        if let Some(task_id_str) = params.initial_task_id {
            let task_id = Self::parse_task_id(task_id_str)?;
            self.state
                .repository()
                .assign_task(task_id, agent_id)
                .await?;
        }

        // Generate system prompt with auto-polling instructions
        let merged_custom_prompt =
            Self::merge_spawn_custom_and_directive(params.custom_prompt, params.directive);
        let system_prompt = crate::generate_agent_system_prompt(
            &agent_id_str,
            params.role,
            merged_custom_prompt.as_deref(),
            params.poll_interval_secs,
        );

        // Spawn CLI process using ProcessManager with custom CLI command
        self.state
            .process_manager()
            .spawn_with_cli(
                agent_id,
                params.cli_command,
                params.cli_args,
                &system_prompt,
            )
            .await
            .map_err(|e| HandlerError::InternalError(format!("Failed to spawn process: {}", e)))?;

        self.state
            .set_agent_spawn_spec(
                agent_id,
                AgentSpawnSpec {
                    role: params.role.to_string(),
                    cli_command: params.cli_command.to_string(),
                    cli_args: params.cli_args.to_vec(),
                    custom_prompt: params.custom_prompt.map(ToString::to_string),
                    directive: params.directive.map(ToString::to_string),
                    poll_interval_secs: params.poll_interval_secs,
                },
            )
            .await;

        Ok(agent_id)
    }

    async fn seed_handshake_messages(
        &self,
        agent_ids: &[AgentId],
        handshake_mode: HandshakeMode,
        message_body: &str,
    ) -> HandlerResult<Vec<String>> {
        let mut message_ids = Vec::new();
        match handshake_mode {
            HandshakeMode::Ring => {
                for (idx, from_agent) in agent_ids.iter().enumerate() {
                    let to_agent = agent_ids[(idx + 1) % agent_ids.len()];
                    let dm = DirectMessage::new(
                        *from_agent,
                        to_agent,
                        message_body,
                        self.state.session_id(),
                    );
                    let mid = dm.id.as_uuid().to_string();
                    self.state.repository().create_direct_message(&dm).await?;
                    message_ids.push(mid);
                }
            }
            HandshakeMode::FullMesh => {
                for from_agent in agent_ids {
                    for to_agent in agent_ids {
                        if from_agent == to_agent {
                            continue;
                        }
                        let dm = DirectMessage::new(
                            *from_agent,
                            *to_agent,
                            message_body,
                            self.state.session_id(),
                        );
                        let mid = dm.id.as_uuid().to_string();
                        self.state.repository().create_direct_message(&dm).await?;
                        message_ids.push(mid);
                    }
                }
            }
        }
        Ok(message_ids)
    }

    async fn restart_agent_from_spec(
        &self,
        agent_id: AgentId,
        spec: &AgentSpawnSpec,
    ) -> HandlerResult<()> {
        let merged_custom_prompt = Self::merge_spawn_custom_and_directive(
            spec.custom_prompt.as_deref(),
            spec.directive.as_deref(),
        );
        let system_prompt = crate::generate_agent_system_prompt(
            &agent_id.as_uuid().to_string(),
            &spec.role,
            merged_custom_prompt.as_deref(),
            spec.poll_interval_secs,
        );

        self.state
            .process_manager()
            .spawn_with_cli(agent_id, &spec.cli_command, &spec.cli_args, &system_prompt)
            .await
            .map_err(|e| HandlerError::InternalError(format!("Failed to restart process: {}", e)))
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
            Ok(()) => {
                // SONA: Record claim as trajectory event
                let trigger = LearningTrigger::new(TriggerKind::TaskComplete, agent_id)
                    .with_task_id(task_id)
                    .with_success(true)
                    .with_summary("task claimed");
                self.record_sona_trajectory(trigger, Some(task_id)).await;

                Ok(tools::ClaimTaskResponse {
                    success: true,
                    error: None,
                })
            }
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

        // SONA: Record in_progress transitions as trajectory events.
        if matches!(status, TaskStatus::InProgress) {
            let agent_id = self
                .resolve_agent_id(req._agent_id.as_deref())
                .await
                .unwrap_or_else(|_| AgentId::new());

            let trigger = LearningTrigger::new(TriggerKind::TaskComplete, agent_id)
                .with_task_id(task_id)
                .with_success(true)
                .with_summary("task started");
            self.record_sona_trajectory(trigger, Some(task_id)).await;
        }

        // SONA: Fire TaskComplete learning trigger on completed/failed tasks.
        if matches!(status, TaskStatus::Completed | TaskStatus::Failed) {
            let agent_id = self
                .resolve_agent_id(req._agent_id.as_deref())
                .await
                .unwrap_or_else(|_| AgentId::new());

            let trigger = LearningTrigger::new(TriggerKind::TaskComplete, agent_id)
                .with_task_id(task_id)
                .with_success(status == TaskStatus::Completed)
                .with_summary(req.summary.as_deref().unwrap_or(""));

            self.record_sona_trajectory(trigger, Some(task_id)).await;

            // Auto-store a pattern in the reasoning bank on successful completion
            if status == TaskStatus::Completed {
                if let Ok(task) = self.state.repository().get_task(task_id).await {
                    self.auto_store_pattern(&task, agent_id).await;
                }

                // EWC++: Auto-consolidate on task completion if enabled
                if let Some(engine) = self.state.sona_engine()
                    && engine.auto_consolidate_enabled()
                {
                    self.auto_consolidate_ewc(task_id, agent_id).await;
                }
            }
        }

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

    async fn handle_dispatch_ready_tasks(
        &self,
        req: tools::DispatchReadyTasksRequest,
    ) -> HandlerResult<tools::DispatchReadyTasksResponse> {
        self.require_strategoi(req._agent_id.as_deref()).await?;

        let repo = self.state.repository();
        let pending = repo
            .list_tasks(self.state.session_id(), Some(TaskStatus::Pending))
            .await?;

        let mut ready_tasks = Vec::new();
        for task in pending {
            if task.assigned_to.is_some() {
                continue;
            }
            let blockers = repo.get_blocking_tasks(task.id).await?;
            let is_ready = blockers
                .iter()
                .all(|blocking| blocking.status == TaskStatus::Completed);
            if is_ready {
                ready_tasks.push(task);
            }
        }
        let ready_task_count = ready_tasks.len();

        ready_tasks.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });

        let mut eligible_agents: Vec<Agent> = repo
            .list_active_agents(self.state.session_id())
            .await?
            .into_iter()
            .filter(|agent| !agent.is_strategoi && agent.current_task.is_none())
            .collect();
        let eligible_agent_count = eligible_agents.len();
        eligible_agents.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        let max_assignments = req.max_assignments;
        let assign_count = std::cmp::min(
            max_assignments,
            std::cmp::min(ready_tasks.len(), eligible_agents.len()),
        );

        let mut assignments = Vec::with_capacity(assign_count);
        for idx in 0..assign_count {
            let task = &ready_tasks[idx];
            let agent = &eligible_agents[idx];

            repo.assign_task(task.id, agent.id).await?;
            assignments.push(tools::TaskDispatchAssignment {
                task_id: task.id.as_uuid().to_string(),
                agent_id: agent.id.as_uuid().to_string(),
            });
        }

        Ok(tools::DispatchReadyTasksResponse {
            inspected_at: Utc::now().to_rfc3339(),
            ready_task_count,
            eligible_agent_count,
            assignment_count: assignments.len(),
            assignments,
        })
    }

    async fn handle_nudge_or_replan(
        &self,
        req: tools::NudgeOrReplanRequest,
    ) -> HandlerResult<tools::NudgeOrReplanResponse> {
        let strategoi_id = self.require_strategoi(req._agent_id.as_deref()).await?;
        let task_id = Self::parse_task_id(&req.task_id)?;
        let mode = Self::parse_nudge_mode(&req.mode)?;
        let repo = self.state.repository();

        let task = repo.get_task(task_id).await?;
        let task_knowledge = repo.get_task_knowledge(task_id).await?;

        let mut last_activity = task.created_at;
        if let Some(latest) = task_knowledge.iter().map(|k| k.created_at).max()
            && latest > last_activity
        {
            last_activity = latest;
        }

        let inactivity_minutes_i64 = std::cmp::min(req.inactivity_minutes, i64::MAX as u64) as i64;
        let stale_threshold = chrono::Duration::minutes(inactivity_minutes_i64);
        let stale_by_time = Utc::now().signed_duration_since(last_activity) > stale_threshold;

        let mut action = "no_action".to_string();
        let mut message_id = None;
        let mut nudged_count = 0usize;
        let mut reassigned_count = 0usize;
        let mut actions = Vec::new();

        if let Some(assignee_id) = task.assigned_to {
            let assignee = repo.get_agent(assignee_id).await?;
            let assignee_inactive = matches!(
                assignee.status,
                AgentStatus::Killed | AgentStatus::Crashed | AgentStatus::Finished
            );

            let should_nudge = matches!(mode, NudgeMode::Nudge)
                || (matches!(mode, NudgeMode::Auto) && !assignee_inactive);
            let should_replan = matches!(mode, NudgeMode::Replan)
                || (matches!(mode, NudgeMode::Auto) && assignee_inactive);

            if should_nudge {
                action = "nudged".to_string();
                nudged_count = 1;
                let nudge_text = req.nudge_message.clone().unwrap_or_else(|| {
                    "Please post a progress update for your assigned task.".to_string()
                });

                if !req.dry_run {
                    let mut dm = DirectMessage::new(
                        strategoi_id,
                        assignee.id,
                        nudge_text,
                        self.state.session_id(),
                    );
                    dm = dm.with_task(task_id);
                    let mid = dm.id.as_uuid().to_string();
                    repo.create_direct_message(&dm).await?;
                    message_id = Some(mid.clone());
                }

                actions.push(tools::NudgeOrReplanAction {
                    task_id: task_id.as_uuid().to_string(),
                    action: action.clone(),
                    message_id: message_id.clone(),
                    from_agent_id: Some(strategoi_id.as_uuid().to_string()),
                    to_agent_id: Some(assignee.id.as_uuid().to_string()),
                    reason: Some(if stale_by_time {
                        "stale_inactivity_threshold_exceeded".to_string()
                    } else {
                        "active_assignee_requires_status_refresh".to_string()
                    }),
                });
            } else if should_replan {
                action = "replanned".to_string();
                reassigned_count = 1;

                if !req.dry_run {
                    repo.update_task_status(task_id, TaskStatus::Pending, None)
                        .await?;
                    repo.clear_task_assignment(task_id).await?;
                }

                actions.push(tools::NudgeOrReplanAction {
                    task_id: task_id.as_uuid().to_string(),
                    action: action.clone(),
                    message_id: None,
                    from_agent_id: Some(strategoi_id.as_uuid().to_string()),
                    to_agent_id: Some(assignee.id.as_uuid().to_string()),
                    reason: Some(if assignee_inactive {
                        "assignee_inactive".to_string()
                    } else {
                        "replan_mode_forced".to_string()
                    }),
                });
            }
        }

        let stale_task_count = usize::from(stale_by_time || reassigned_count > 0);

        Ok(tools::NudgeOrReplanResponse {
            task_id: task_id.as_uuid().to_string(),
            action,
            message_id,
            inspected_at: Utc::now().to_rfc3339(),
            stale_task_count,
            nudged_count,
            reassigned_count,
            actions,
        })
    }

    async fn handle_task_completion_gate(
        &self,
        req: tools::TaskCompletionGateRequest,
    ) -> HandlerResult<tools::TaskCompletionGateResponse> {
        self.require_strategoi(req._agent_id.as_deref()).await?;

        let task_id = Self::parse_task_id(&req.task_id)?;
        let required_checks = req
            .required_checks
            .clone()
            .filter(|checks| !checks.is_empty())
            .unwrap_or_else(Self::default_completion_checks);

        let summary = req.summary.as_deref().map(str::trim).unwrap_or("");
        let mut missing_summary_fields = Vec::new();
        if summary.is_empty() {
            missing_summary_fields.push("summary".to_string());
        }

        let check_map: HashMap<&str, bool> = req
            .checks
            .iter()
            .map(|check| (check.name.as_str(), check.passed))
            .collect();

        let mut missing_checks = Vec::new();
        let mut failed_checks = Vec::new();
        for required in &required_checks {
            match check_map.get(required.as_str()) {
                Some(true) => {}
                Some(false) => failed_checks.push(required.clone()),
                None => missing_checks.push(required.clone()),
            }
        }

        let allowed = missing_summary_fields.is_empty()
            && missing_checks.is_empty()
            && failed_checks.is_empty();

        let finalized = if req.finalize && allowed {
            self.state
                .repository()
                .update_task_status(task_id, TaskStatus::Completed, Some(summary))
                .await?;
            true
        } else {
            false
        };

        Ok(tools::TaskCompletionGateResponse {
            task_id: task_id.as_uuid().to_string(),
            allowed,
            missing_summary_fields,
            missing_checks,
            failed_checks,
            required_checks,
            validated_checks: req.checks.len(),
            finalized,
        })
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
        let task_id = knowledge.task_id;
        self.state.repository().create_knowledge(&knowledge).await?;

        // SONA: Fire KnowledgeShare learning trigger.
        {
            let mut trigger = LearningTrigger::new(TriggerKind::KnowledgeShare, agent_id)
                .with_knowledge_kind(kind)
                .with_summary(&req.content);

            if let Some(tid) = task_id {
                trigger = trigger.with_task_id(tid);
            }

            self.record_sona_trajectory(trigger, task_id).await;
            self.state
                .learning_counters()
                .instant_patterns
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

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

        // Register MCP session → agent mapping for automatic context
        if let Some(ref mcp_session_id) = self.mcp_session_id {
            self.state
                .register_session_agent(mcp_session_id.clone(), aid)
                .await;
        }

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
        self.require_strategoi(req._agent_id.as_deref()).await?;
        let role = Self::parse_role(&req.role)?;
        if role != AgentRole::Developer {
            return Err(HandlerError::InvalidArgs(
                "MVP only supports spawning developer agents".into(),
            ));
        }

        let agent_id = self
            .spawn_worker_agent(SpawnWorkerParams {
                role: &req.role,
                cli_command: &req.cli_command,
                cli_args: &req.cli_args,
                custom_prompt: req.custom_prompt.as_deref(),
                directive: req.directive.as_deref(),
                poll_interval_secs: req.poll_interval_secs,
                initial_task_id: req.initial_task_id.as_deref(),
            })
            .await?;
        let agent_id_str = agent_id.as_uuid().to_string();

        // Build CLI command string for response
        let cli_command = if req.cli_args.is_empty() {
            req.cli_command.clone()
        } else {
            format!("{} {}", req.cli_command, req.cli_args.join(" "))
        };

        Ok(tools::SpawnAgentResponse {
            agent_id: agent_id_str,
            process_id: None, // ProcessManager doesn't expose PID currently
            cli_command: Some(cli_command),
            teammate_id: None, // Deprecated - actual process spawned
        })
    }

    async fn handle_refresh_session(
        &self,
        req: tools::RefreshSessionRequest,
    ) -> HandlerResult<tools::RefreshSessionResponse> {
        let mcp_session_id = self.mcp_session_id.clone();

        // Clear current in-memory binding first.
        if let Some(ref sid) = mcp_session_id {
            self.state.clear_session_agent(sid).await;
        }
        *self.agent_id.write().await = None;

        // Resolve requested binding target:
        // 1) explicit `agent_id`, 2) `_agent_id`, 3) persisted session agent.
        let explicit_target = req.agent_id.or(req._agent_id);
        let rebound_agent = if let Some(agent_id_str) = explicit_target {
            let agent_id = Self::parse_agent_id(&agent_id_str)?;
            // Validate explicit target exists.
            self.state.repository().get_agent(agent_id).await?;
            Some(agent_id)
        } else {
            let persisted = self
                .state
                .repository()
                .get_session(self.state.session_id())
                .await?
                .agent_id;
            if let Some(candidate) = persisted {
                // Persisted binding may be stale; only rebind if agent still exists.
                if self.state.repository().get_agent(candidate).await.is_ok() {
                    Some(candidate)
                } else {
                    None
                }
            } else {
                None
            }
        };

        let mut rebound = false;
        let mut bound_agent_id = None;
        if let Some(agent_id) = rebound_agent {
            if let Some(ref sid) = mcp_session_id {
                self.state
                    .register_session_agent(sid.clone(), agent_id)
                    .await;
            }
            self.state
                .repository()
                .set_session_agent(self.state.session_id(), agent_id)
                .await?;
            *self.agent_id.write().await = Some(agent_id);
            rebound = true;
            bound_agent_id = Some(agent_id.as_uuid().to_string());
        }

        Ok(tools::RefreshSessionResponse {
            session_id: self.state.session_id().as_uuid().to_string(),
            mcp_session_id,
            agent_id: bound_agent_id,
            rebound,
        })
    }

    async fn handle_spawn_team_and_handshake(
        &self,
        req: tools::SpawnTeamAndHandshakeRequest,
    ) -> HandlerResult<tools::SpawnTeamAndHandshakeResponse> {
        self.require_strategoi(req._agent_id.as_deref()).await?;
        let role = Self::parse_role(&req.role)?;
        if role != AgentRole::Developer {
            return Err(HandlerError::InvalidArgs(
                "MVP only supports spawning developer agents".into(),
            ));
        }

        if req.agent_count < 2 {
            return Err(HandlerError::InvalidArgs(
                "agent_count must be at least 2".into(),
            ));
        }

        let handshake_mode = Self::parse_handshake_mode(&req.handshake_mode)?;

        let mut agent_ids = Vec::with_capacity(req.agent_count);
        for _ in 0..req.agent_count {
            let agent_id = self
                .spawn_worker_agent(SpawnWorkerParams {
                    role: &req.role,
                    cli_command: &req.cli_command,
                    cli_args: &req.cli_args,
                    custom_prompt: req.custom_prompt.as_deref(),
                    directive: req.directive.as_deref(),
                    poll_interval_secs: req.poll_interval_secs,
                    initial_task_id: None,
                })
                .await?;
            agent_ids.push(agent_id);
        }

        let message_body = req.handshake_message.unwrap_or_else(|| {
            format!(
                "Handshake seeded by spawn_team_and_handshake (mode: {}).",
                handshake_mode.as_str()
            )
        });

        let message_ids = self
            .seed_handshake_messages(&agent_ids, handshake_mode, &message_body)
            .await?;

        Ok(tools::SpawnTeamAndHandshakeResponse {
            agent_ids: agent_ids
                .iter()
                .map(|id| id.as_uuid().to_string())
                .collect(),
            message_ids,
            handshake_mode: handshake_mode.as_str().to_string(),
        })
    }

    async fn handle_spawn_team_from_template(
        &self,
        req: tools::SpawnTeamFromTemplateRequest,
    ) -> HandlerResult<tools::SpawnTeamFromTemplateResponse> {
        self.require_strategoi(req._agent_id.as_deref()).await?;
        let template_kind = Self::parse_template_kind(&req.template)?;
        let handshake_mode = Self::parse_handshake_mode(&req.handshake_mode)?;

        let members = Self::build_team_template_members(
            template_kind,
            req.cli_command.as_deref(),
            req.cli_args.as_deref(),
            req.directive.as_deref(),
        );
        if members.len() < 2 {
            return Err(HandlerError::InternalError(
                "Template must contain at least 2 members".into(),
            ));
        }

        let mut agent_ids = Vec::with_capacity(members.len());
        let mut spawned_members = Vec::with_capacity(members.len());

        for member in members {
            let agent_id = self
                .spawn_worker_agent(SpawnWorkerParams {
                    role: &member.role,
                    cli_command: &member.cli_command,
                    cli_args: &member.cli_args,
                    custom_prompt: member.custom_prompt.as_deref(),
                    directive: member.directive.as_deref(),
                    poll_interval_secs: req.poll_interval_secs,
                    initial_task_id: None,
                })
                .await?;

            agent_ids.push(agent_id);
            spawned_members.push(tools::TemplateSpawnedMember {
                agent_id: agent_id.as_uuid().to_string(),
                name: member.name,
                role: member.role,
                cli_command: member.cli_command,
            });
        }

        let message_body = req.handshake_message.unwrap_or_else(|| {
            format!(
                "Template '{}' handshake seeded (mode: {}).",
                template_kind.as_str(),
                handshake_mode.as_str()
            )
        });

        let message_ids = self
            .seed_handshake_messages(&agent_ids, handshake_mode, &message_body)
            .await?;

        Ok(tools::SpawnTeamFromTemplateResponse {
            template: template_kind.as_str().to_string(),
            members: spawned_members,
            message_ids,
            handshake_mode: handshake_mode.as_str().to_string(),
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

    async fn handle_list_processes(
        &self,
        _req: tools::ListProcessesRequest,
    ) -> HandlerResult<tools::ListProcessesResponse> {
        let agents = self
            .state
            .repository()
            .list_agents(self.state.session_id())
            .await?;

        let mut processes = Vec::new();
        for agent in agents {
            let is_running = self.state.process_manager().is_running(agent.id).await;
            processes.push(tools::ProcessInfo {
                agent_id: agent.id.as_uuid().to_string(),
                is_running,
                status: format!("{:?}", agent.status),
            });
        }

        Ok(tools::ListProcessesResponse {
            total_count: processes.len(),
            processes,
        })
    }

    async fn handle_kill_process(
        &self,
        req: tools::KillProcessRequest,
    ) -> HandlerResult<tools::KillProcessResponse> {
        let agent_id = Self::parse_agent_id(&req.agent_id)?;

        self.state
            .process_manager()
            .kill(agent_id)
            .await
            .map_err(|e| HandlerError::InternalError(format!("Failed to kill process: {}", e)))?;

        Ok(tools::KillProcessResponse { success: true })
    }

    async fn handle_cleanup_stale_agents(
        &self,
        _req: tools::CleanupStaleAgentsRequest,
    ) -> HandlerResult<tools::CleanupStaleAgentsResponse> {
        let agents = self
            .state
            .repository()
            .list_agents(self.state.session_id())
            .await?;

        let mut cleaned_ids = Vec::new();
        for agent in agents {
            // Kill agents stuck in Starting status that aren't actually running
            if agent.status == AgentStatus::Starting {
                let is_running = self.state.process_manager().is_running(agent.id).await;
                if !is_running {
                    // Try to kill (will fail if process doesn't exist, that's ok)
                    let _ = self.state.process_manager().kill(agent.id).await;
                    // Mark as killed
                    self.state
                        .repository()
                        .update_agent_status(agent.id, AgentStatus::Killed)
                        .await?;
                    cleaned_ids.push(agent.id.as_uuid().to_string());
                }
            }
        }

        Ok(tools::CleanupStaleAgentsResponse {
            cleaned_count: cleaned_ids.len(),
            agent_ids: cleaned_ids,
        })
    }

    async fn handle_supervise_team(
        &self,
        req: tools::SuperviseTeamRequest,
    ) -> HandlerResult<tools::SuperviseTeamResponse> {
        self.require_strategoi(req._agent_id.as_deref()).await?;

        let now = Utc::now();
        let stale_after_secs_i64 = std::cmp::min(req.stale_after_secs, i64::MAX as u64) as i64;
        let stale_threshold = chrono::Duration::seconds(stale_after_secs_i64);

        let all_agents = self
            .state
            .repository()
            .list_agents(self.state.session_id())
            .await?;
        let agents: Vec<Agent> = all_agents.into_iter().filter(|a| !a.is_strategoi).collect();

        let recent_knowledge = self
            .state
            .repository()
            .get_recent_knowledge(self.state.session_id(), req.recent_knowledge_limit)
            .await?;
        let mut last_activity: HashMap<AgentId, DateTime<Utc>> = HashMap::new();
        for k in recent_knowledge {
            let entry = last_activity.entry(k.author_id).or_insert(k.created_at);
            if k.created_at > *entry {
                *entry = k.created_at;
            }
        }

        let mut issues = Vec::new();
        let mut escalations = Vec::new();
        let mut restarted_agents = Vec::new();
        let mut healthy_agents = 0usize;

        for agent in agents {
            let is_running = self.state.process_manager().is_running(agent.id).await;
            let last_seen = last_activity
                .get(&agent.id)
                .copied()
                .or(Some(agent.created_at));
            let stale = if let Some(last) = last_seen {
                now.signed_duration_since(last) > stale_threshold
            } else {
                false
            };

            let mut issue: Option<String> = None;
            if !is_running
                && matches!(
                    agent.status,
                    AgentStatus::Starting | AgentStatus::Active | AgentStatus::Idle
                )
            {
                issue = Some("crashed_or_not_running".to_string());
            } else if matches!(agent.status, AgentStatus::Crashed | AgentStatus::Killed) {
                issue = Some("crashed_or_killed".to_string());
            } else if stale && agent.current_task.is_some() {
                issue = Some("blocked".to_string());
            } else if stale {
                issue = Some("stale".to_string());
            } else if agent.status == AgentStatus::Idle {
                issue = Some("idle".to_string());
            }

            if let Some(issue_kind) = issue {
                let mut action_taken = None;
                if req.auto_restart && !is_running {
                    if let Some(spec) = self.state.get_agent_spawn_spec(agent.id).await {
                        match self.restart_agent_from_spec(agent.id, &spec).await {
                            Ok(()) => {
                                let id = agent.id.as_uuid().to_string();
                                restarted_agents.push(id.clone());
                                action_taken = Some("restarted_from_spawn_spec".to_string());
                            }
                            Err(err) => {
                                action_taken = Some(format!("restart_failed: {}", err));
                            }
                        }
                    } else {
                        action_taken = Some("restart_skipped_no_spawn_spec".to_string());
                    }
                }

                let last_activity_at = last_seen.map(|d| d.to_rfc3339());
                issues.push(tools::AgentSupervisionIssue {
                    agent_id: agent.id.as_uuid().to_string(),
                    role: format!("{}", agent.role),
                    status: format!("{:?}", agent.status).to_lowercase(),
                    is_running,
                    issue: issue_kind.clone(),
                    last_activity_at,
                    action_taken: action_taken.clone(),
                });

                let escalation = if let Some(action) = action_taken {
                    format!(
                        "Agent {} reported '{}' (action: {}).",
                        agent.id.as_uuid(),
                        issue_kind,
                        action
                    )
                } else {
                    format!(
                        "Agent {} reported '{}'; manual intervention recommended.",
                        agent.id.as_uuid(),
                        issue_kind
                    )
                };
                escalations.push(escalation);
            } else {
                healthy_agents += 1;
            }
        }

        Ok(tools::SuperviseTeamResponse {
            inspected_at: now.to_rfc3339(),
            total_agents: healthy_agents + issues.len(),
            healthy_agents,
            restarted_agents,
            issues,
            escalations,
        })
    }

    async fn handle_team_runbook_prompt(
        &self,
        req: tools::TeamRunbookPromptRequest,
    ) -> HandlerResult<tools::TeamRunbookPromptResponse> {
        self.require_strategoi(req._agent_id.as_deref()).await?;

        Ok(tools::TeamRunbookPromptResponse {
            protocol_version: "1.0.0".to_string(),
            runbook: crate::team_runbook_protocol().to_string(),
            sections: vec![
                "STARTUP HANDSHAKE".to_string(),
                "STATUS CADENCE".to_string(),
                "BLOCKER FORMAT".to_string(),
                "DONE FORMAT".to_string(),
            ],
        })
    }

    async fn handle_hive_observability_snapshot(
        &self,
        req: tools::HiveObservabilitySnapshotRequest,
    ) -> HandlerResult<tools::HiveObservabilitySnapshotResponse> {
        self.require_strategoi(req._agent_id.as_deref()).await?;

        let repo = self.state.repository();
        let now = Utc::now();
        let window_minutes_i64 = std::cmp::min(req.window_minutes, i64::MAX as u64) as i64;
        let window_start = now - chrono::Duration::minutes(window_minutes_i64);
        let stale_minutes_i64 = std::cmp::min(req.stale_task_minutes, i64::MAX as u64) as i64;
        let stale_threshold = chrono::Duration::minutes(stale_minutes_i64);

        let tasks = repo.list_tasks(self.state.session_id(), None).await?;
        let completed_tasks_last_window = tasks
            .iter()
            .filter(|task| {
                task.completed_at
                    .is_some_and(|completed| completed >= window_start)
            })
            .count();
        let completion_rate_per_hour = if req.window_minutes == 0 {
            completed_tasks_last_window as f64
        } else {
            completed_tasks_last_window as f64 / (req.window_minutes as f64 / 60.0)
        };

        let knowledge_window = repo
            .get_recent_knowledge(self.state.session_id(), 2000)
            .await?;
        let recent_knowledge: Vec<_> = knowledge_window
            .iter()
            .filter(|k| k.created_at >= window_start)
            .collect();
        let knowledge_events_last_window = recent_knowledge.len();

        let mut stuck_tasks = Vec::new();
        for task in &tasks {
            if !matches!(task.status, TaskStatus::Claimed | TaskStatus::InProgress) {
                continue;
            }
            let task_activity = knowledge_window
                .iter()
                .filter(|k| k.task_id == Some(task.id))
                .map(|k| k.created_at)
                .max()
                .unwrap_or(task.created_at);

            let age = now.signed_duration_since(task_activity);
            if age > stale_threshold {
                stuck_tasks.push(tools::SnapshotStuckTask {
                    task_id: task.id.as_uuid().to_string(),
                    status: format!("{:?}", task.status).to_lowercase(),
                    assigned_to: task.assigned_to.map(|a| a.as_uuid().to_string()),
                    minutes_since_activity: age.num_minutes().max(0) as u64,
                });
            }
        }

        let mut events_by_agent: HashMap<AgentId, usize> = HashMap::new();
        for knowledge in &recent_knowledge {
            *events_by_agent.entry(knowledge.author_id).or_default() += 1;
        }
        let mut noisy_agents: Vec<tools::SnapshotNoisyAgent> = events_by_agent
            .into_iter()
            .filter(|(_, count)| *count >= req.noisy_agent_threshold)
            .map(|(agent_id, count)| tools::SnapshotNoisyAgent {
                agent_id: agent_id.as_uuid().to_string(),
                event_count: count,
            })
            .collect();
        noisy_agents.sort_by(|a, b| b.event_count.cmp(&a.event_count));

        let agents = repo.list_agents(self.state.session_id()).await?;
        let mut failed_commands = Vec::new();
        for agent in agents {
            if let Ok((_, stderr)) = self.state.process_manager().get_output(agent.id).await
                && let Some(signal) = stderr
                    .lines()
                    .rev()
                    .find(|line| {
                        let lower = line.to_ascii_lowercase();
                        lower.contains("error") || lower.contains("failed")
                    })
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
            {
                let truncated = if signal.chars().count() > 200 {
                    let prefix: String = signal.chars().take(200).collect();
                    format!("{prefix}...")
                } else {
                    signal.to_string()
                };
                failed_commands.push(tools::SnapshotFailedCommand {
                    agent_id: agent.id.as_uuid().to_string(),
                    signal: truncated,
                });
            }
        }

        let mut latencies = Vec::new();
        for task in &tasks {
            let mut messages = repo.get_thread_messages(task.id, 200).await?;
            messages.sort_by_key(|msg| msg.created_at);
            for pair in messages.windows(2) {
                let first = &pair[0];
                let second = &pair[1];
                if first.from_agent != second.from_agent {
                    let delta = second
                        .created_at
                        .signed_duration_since(first.created_at)
                        .num_milliseconds() as f64
                        / 1000.0;
                    if delta >= 0.0 {
                        latencies.push(delta);
                    }
                }
            }
        }

        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let average_secs = if latencies.is_empty() {
            None
        } else {
            Some(latencies.iter().sum::<f64>() / latencies.len() as f64)
        };
        let p95_secs = if latencies.is_empty() {
            None
        } else {
            let idx = ((latencies.len() - 1) as f64 * 0.95).round() as usize;
            latencies.get(idx).copied()
        };

        Ok(tools::HiveObservabilitySnapshotResponse {
            inspected_at: now.to_rfc3339(),
            throughput: tools::SnapshotThroughput {
                completed_tasks_last_window,
                completion_rate_per_hour,
                knowledge_events_last_window,
            },
            stuck_tasks,
            noisy_agents,
            failed_commands,
            coordination_latency: tools::SnapshotCoordinationLatency {
                average_secs,
                p95_secs,
                sample_count: latencies.len(),
            },
        })
    }

    async fn handle_get_process_output(
        &self,
        req: tools::GetProcessOutputRequest,
    ) -> HandlerResult<tools::GetProcessOutputResponse> {
        let agent_id = Self::parse_agent_id(&req.agent_id)?;

        let is_running = self.state.process_manager().is_running(agent_id).await;
        let (stdout, stderr) = self
            .state
            .process_manager()
            .get_output(agent_id)
            .await
            .map_err(|e| HandlerError::InternalError(format!("Failed to get output: {}", e)))?;

        Ok(tools::GetProcessOutputResponse {
            agent_id: agent_id.as_uuid().to_string(),
            stdout,
            stderr,
            is_running,
        })
    }

    async fn handle_collect_agent_artifacts(
        &self,
        req: tools::CollectAgentArtifactsRequest,
    ) -> HandlerResult<tools::CollectAgentArtifactsResponse> {
        self.require_strategoi(req._agent_id.as_deref()).await?;

        let agent_id = Self::parse_agent_id(&req.agent_id)?;
        let task_id = req
            .task_id
            .as_deref()
            .map(Self::parse_task_id)
            .transpose()?;
        let is_running = self.state.process_manager().is_running(agent_id).await;
        let (stdout, stderr) = self
            .state
            .process_manager()
            .get_output(agent_id)
            .await
            .map_err(|e| HandlerError::InternalError(format!("Failed to get output: {}", e)))?;

        let mut artifacts = Vec::new();
        let outputs = [
            ("stdout", KnowledgeKind::Discovery, stdout),
            ("stderr", KnowledgeKind::Blocker, stderr),
        ];

        for (stream, kind, output) in outputs {
            let trimmed = output.trim();
            if trimmed.is_empty() {
                continue;
            }

            let summary = if trimmed.chars().count() > req.max_chars {
                let truncated: String = trimmed.chars().take(req.max_chars).collect();
                format!("{truncated}...")
            } else {
                trimmed.to_string()
            };

            let content = format!(
                "Agent {} {} output:\n{}",
                agent_id.as_uuid(),
                stream,
                summary
            );
            let mut knowledge = Knowledge::new(
                &content,
                kind,
                self.require_agent_id().await?,
                self.state.session_id(),
            );
            if let Some(tid) = task_id {
                knowledge = knowledge.with_task(tid);
            }

            let knowledge_id = knowledge.id.as_uuid().to_string();
            self.state.repository().create_knowledge(&knowledge).await?;
            artifacts.push(tools::AgentArtifact {
                stream: stream.to_string(),
                kind: format!("{kind:?}").to_lowercase(),
                knowledge_id,
                summary,
            });
        }

        Ok(tools::CollectAgentArtifactsResponse {
            agent_id: agent_id.as_uuid().to_string(),
            is_running,
            created_count: artifacts.len(),
            artifacts,
        })
    }

    async fn handle_command_agent(
        &self,
        req: tools::CommandAgentRequest,
    ) -> HandlerResult<tools::CommandAgentResponse> {
        let agent_id = Self::parse_agent_id(&req.agent_id)?;
        let prompt = match (req.prompt.as_deref(), req.directive.as_deref()) {
            (_, Some(directive)) => crate::generate_strategoi_directive_prompt(
                &req.agent_id,
                directive,
                req.prompt.as_deref(),
            ),
            (Some(raw_prompt), None) => raw_prompt.to_string(),
            (None, None) => {
                return Err(HandlerError::InvalidArgs(
                    "command_agent requires either `prompt` or `directive`".into(),
                ));
            }
        };

        self.state
            .process_manager()
            .command_agent(agent_id, &req.cli_command, &req.cli_args, &prompt)
            .await
            .map_err(|e| HandlerError::InternalError(format!("Failed to command agent: {}", e)))?;

        Ok(tools::CommandAgentResponse {
            success: true,
            agent_id: agent_id.as_uuid().to_string(),
        })
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

    // === Nova Experimental Features ===

    async fn handle_find_temporal_path(
        &self,
        req: tools::FindTemporalPathRequest,
    ) -> HandlerResult<tools::FindTemporalPathResponse> {
        use aletheiadb::experimental::chronos::Chronos;

        let repo = self.state.repository();
        let db = repo.get_raw_db()?;

        let valid_time = chrono::DateTime::parse_from_rfc3339(&req.valid_time)
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid valid_time: {}", e)))?
            .timestamp_micros();
        let tx_time = chrono::DateTime::parse_from_rfc3339(&req.tx_time)
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid tx_time: {}", e)))?
            .timestamp_micros();

        let start_uuid = uuid::Uuid::parse_str(&req.start_entity_id)
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid start_entity_id: {}", e)))?;
        let end_uuid = uuid::Uuid::parse_str(&req.end_entity_id)
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid end_entity_id: {}", e)))?;

        let (start_node_id, end_node_id) = match req.entity_type.as_str() {
            "task" => {
                let start_task_id = harness_persistence::TaskId::from_uuid(start_uuid);
                let end_task_id = harness_persistence::TaskId::from_uuid(end_uuid);
                (
                    repo.get_node_id_for_task(start_task_id)?,
                    repo.get_node_id_for_task(end_task_id)?,
                )
            }
            "knowledge" => {
                let start_knowledge_id = harness_persistence::KnowledgeId::from_uuid(start_uuid);
                let end_knowledge_id = harness_persistence::KnowledgeId::from_uuid(end_uuid);
                (
                    repo.get_node_id_for_knowledge(start_knowledge_id)?,
                    repo.get_node_id_for_knowledge(end_knowledge_id)?,
                )
            }
            _ => {
                return Err(HandlerError::InvalidArgs(format!(
                    "Invalid entity_type: {}",
                    req.entity_type
                )));
            }
        };

        let chronos = Chronos::new(db);
        let valid_ts = aletheiadb::core::hlc::HybridTimestamp::new(valid_time, 0)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;
        let tx_ts = aletheiadb::core::hlc::HybridTimestamp::new(tx_time, 0)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        let path_result = chronos
            .find_path_at_time(start_node_id, end_node_id, valid_ts, tx_ts)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        let (path_option, description) = if let Some(node_path) = path_result {
            let path: Vec<String> = node_path
                .iter()
                .map(|&node_id| format!("node-{}", node_id.as_u64()))
                .collect();
            let desc = format!(
                "Path found with {} hops from {} to {} at valid_time={}",
                path.len() - 1,
                req.start_entity_id,
                req.end_entity_id,
                req.valid_time
            );
            (Some(path), desc)
        } else {
            let desc = format!(
                "No path found from {} to {} at valid_time={}",
                req.start_entity_id, req.end_entity_id, req.valid_time
            );
            (None, desc)
        };

        Ok(tools::FindTemporalPathResponse {
            path: path_option.clone(),
            path_length: path_option.as_ref().map_or(0, |p| p.len()),
            valid_time: req.valid_time,
            tx_time: req.tx_time,
            description,
        })
    }

    async fn handle_navigate_semantic_graph(
        &self,
        req: tools::NavigateSemanticGraphRequest,
    ) -> HandlerResult<tools::NavigateSemanticGraphResponse> {
        use aletheiadb::experimental::semantic_navigator::SemanticNavigator;

        let repo = self.state.repository();
        let db = repo.get_raw_db()?;

        let start_uuid = uuid::Uuid::parse_str(&req.start_entity_id)
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid start_entity_id: {}", e)))?;
        let end_uuid = uuid::Uuid::parse_str(&req.end_entity_id)
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid end_entity_id: {}", e)))?;

        let (start_node_id, end_node_id) = match req.entity_type.as_str() {
            "task" => {
                let start_task_id = harness_persistence::TaskId::from_uuid(start_uuid);
                let end_task_id = harness_persistence::TaskId::from_uuid(end_uuid);
                (
                    repo.get_node_id_for_task(start_task_id)?,
                    repo.get_node_id_for_task(end_task_id)?,
                )
            }
            "knowledge" => {
                let start_knowledge_id = harness_persistence::KnowledgeId::from_uuid(start_uuid);
                let end_knowledge_id = harness_persistence::KnowledgeId::from_uuid(end_uuid);
                (
                    repo.get_node_id_for_knowledge(start_knowledge_id)?,
                    repo.get_node_id_for_knowledge(end_knowledge_id)?,
                )
            }
            _ => {
                return Err(HandlerError::InvalidArgs(format!(
                    "Invalid entity_type: {}",
                    req.entity_type
                )));
            }
        };

        let navigator = SemanticNavigator::new(db);
        let node_path = navigator
            .find_path(start_node_id, end_node_id, &req.vector_property)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        let mut total_cost = 0.0f32;
        for i in 0..node_path.len().saturating_sub(1) {
            let current_id = node_path[i];
            let next_id = node_path[i + 1];
            if let (Ok(current_node), Ok(next_node)) =
                (db.get_node(current_id), db.get_node(next_id))
                && let (Some(v1), Some(v2)) = (
                    current_node
                        .properties
                        .get(&req.vector_property)
                        .and_then(|v| v.as_arc_vector()),
                    next_node
                        .properties
                        .get(&req.vector_property)
                        .and_then(|v| v.as_arc_vector()),
                )
                && let Ok(sim) = aletheiadb::core::vector::cosine_similarity(&v1, &v2)
            {
                total_cost += 1.0 - sim;
            }
        }

        let path: Vec<String> = node_path
            .iter()
            .map(|&node_id| format!("node-{}", node_id.as_u64()))
            .collect();
        let description = format!(
            "Semantic path from {} to {} with {} hops and total cost {:.3}",
            req.start_entity_id,
            req.end_entity_id,
            path.len() - 1,
            total_cost
        );

        Ok(tools::NavigateSemanticGraphResponse {
            path,
            path_length: node_path.len(),
            total_cost,
            description,
        })
    }

    async fn handle_discover_semantic_clusters(
        &self,
        req: tools::DiscoverSemanticClustersRequest,
    ) -> HandlerResult<tools::DiscoverSemanticClustersResponse> {
        use aletheiadb::experimental::cartographer::Cartographer;
        use std::collections::HashMap;

        let repo = self.state.repository();
        let db = repo.get_raw_db()?;

        let cartographer = Cartographer::new(db);
        let clustering = cartographer
            .analyze(&req.vector_property, req.num_clusters)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        let mut cluster_map: HashMap<usize, Vec<String>> = HashMap::new();
        for (node_id, cluster_id) in &clustering.assignments {
            cluster_map
                .entry(*cluster_id)
                .or_default()
                .push(format!("node-{}", node_id.as_u64()));
        }

        let clusters: Vec<tools::ClusterInfo> = clustering
            .centroids
            .iter()
            .enumerate()
            .map(|(cluster_id, centroid)| {
                let member_ids = cluster_map.get(&cluster_id).cloned().unwrap_or_default();
                let size = member_ids.len();
                tools::ClusterInfo {
                    cluster_id,
                    centroid: centroid.clone(),
                    member_ids,
                    size,
                }
            })
            .collect();

        let reified_regions = if req.reify {
            let region_ids = cartographer
                .reify(&clustering)
                .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;
            Some(
                region_ids
                    .iter()
                    .map(|node_id| format!("region-{}", node_id.as_u64()))
                    .collect(),
            )
        } else {
            None
        };

        let description = format!(
            "Found {} semantic clusters with {} total entities (reified: {})",
            clusters.len(),
            clustering.assignments.len(),
            req.reify
        );

        Ok(tools::DiscoverSemanticClustersResponse {
            clusters,
            reified_regions,
            description,
        })
    }

    async fn handle_generate_graph_layout(
        &self,
        req: tools::GenerateGraphLayoutRequest,
    ) -> HandlerResult<tools::GenerateGraphLayoutResponse> {
        use aletheiadb::experimental::kaleidoscope::{LayoutConfig, LayoutEngine};

        let repo = self.state.repository();
        let db = repo.get_raw_db()?;

        let node_ids: Vec<aletheiadb::core::id::NodeId> = match req.entity_type.as_str() {
            "task" => {
                let tasks = repo.list_tasks(self.state.session_id(), None).await?;
                tasks
                    .iter()
                    .filter_map(|t| repo.get_node_id_for_task(t.id).ok())
                    .collect()
            }
            "knowledge" => {
                let knowledge = repo
                    .get_recent_knowledge(self.state.session_id(), 1000)
                    .await?;
                knowledge
                    .iter()
                    .filter_map(|k| repo.get_node_id_for_knowledge(k.id).ok())
                    .collect()
            }
            _ => {
                return Err(HandlerError::InvalidArgs(format!(
                    "Invalid entity_type: {}",
                    req.entity_type
                )));
            }
        };

        let config = LayoutConfig {
            iterations: req.iterations,
            width: req.width,
            height: req.height,
            ..LayoutConfig::default()
        };
        let mut engine = LayoutEngine::new(config);

        for &node_id in &node_ids {
            engine.add_node(node_id);
            for edge_id in db.get_outgoing_edges(node_id) {
                if let Ok(target_id) = db.get_edge_target(edge_id) {
                    engine.add_edge(node_id, target_id);
                }
            }
        }

        if let Some(ref vector_prop) = req.vector_property {
            for i in 0..node_ids.len() {
                for j in (i + 1)..node_ids.len() {
                    let node_a = node_ids[i];
                    let node_b = node_ids[j];
                    if let (Ok(na), Ok(nb)) = (db.get_node(node_a), db.get_node(node_b))
                        && let (Some(va), Some(vb)) = (
                            na.properties
                                .get(vector_prop)
                                .and_then(|v| v.as_arc_vector()),
                            nb.properties
                                .get(vector_prop)
                                .and_then(|v| v.as_arc_vector()),
                        )
                        && let Ok(sim) = aletheiadb::core::vector::cosine_similarity(&va, &vb)
                        && sim > 0.5
                    {
                        engine.add_semantic_link(node_a, node_b, sim);
                    }
                }
            }
        }

        engine.run();

        let positions_map = engine.get_positions();
        let mut positions: Vec<tools::NodePosition> = positions_map
            .iter()
            .map(|(&node_id, &pos)| tools::NodePosition {
                entity_id: format!("node-{}", node_id.as_u64()),
                x: pos.x,
                y: pos.y,
            })
            .collect();
        positions.sort_by(|a, b| a.entity_id.cmp(&b.entity_id));

        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        for pos in positions_map.values() {
            min_x = min_x.min(pos.x);
            max_x = max_x.max(pos.x);
            min_y = min_y.min(pos.y);
            max_y = max_y.max(pos.y);
        }

        let bounds = tools::LayoutBounds {
            min_x,
            max_x,
            min_y,
            max_y,
        };
        let description = format!(
            "Generated layout for {} nodes using {} iterations (bounds: {:.1}x{:.1} to {:.1}x{:.1})",
            positions.len(),
            req.iterations,
            min_x,
            min_y,
            max_x,
            max_y
        );

        Ok(tools::GenerateGraphLayoutResponse {
            positions,
            bounds,
            description,
        })
    }

    async fn handle_find_activity_resonance(
        &self,
        req: tools::FindActivityResonanceRequest,
    ) -> HandlerResult<tools::FindActivityResonanceResponse> {
        use aletheiadb::experimental::echo::{ActivityDensityResonator, EchoChamber};

        let repo = self.state.repository();
        let db = repo.get_raw_db()?;

        let target_uuid = uuid::Uuid::parse_str(&req.target_entity_id)
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid target_entity_id: {}", e)))?;

        let target_node_id = match req.entity_type.as_str() {
            "task" => {
                let task_id = harness_persistence::TaskId::from_uuid(target_uuid);
                repo.get_node_id_for_task(task_id)?
            }
            "knowledge" => {
                let knowledge_id = harness_persistence::KnowledgeId::from_uuid(target_uuid);
                repo.get_node_id_for_knowledge(knowledge_id)?
            }
            _ => {
                return Err(HandlerError::InvalidArgs(format!(
                    "Invalid entity_type: {}",
                    req.entity_type
                )));
            }
        };

        let candidate_ids: Vec<aletheiadb::core::id::NodeId> = match req.entity_type.as_str() {
            "task" => {
                let tasks = repo.list_tasks(self.state.session_id(), None).await?;
                tasks
                    .iter()
                    .filter_map(|t| repo.get_node_id_for_task(t.id).ok())
                    .collect()
            }
            "knowledge" => {
                let knowledge = repo
                    .get_recent_knowledge(self.state.session_id(), 1000)
                    .await?;
                knowledge
                    .iter()
                    .filter_map(|k| repo.get_node_id_for_knowledge(k.id).ok())
                    .collect()
            }
            _ => Vec::new(),
        };

        let resonator = ActivityDensityResonator {
            window_size_us: (req.window_seconds * 1_000_000) as i64,
            num_bins: req.num_bins,
        };
        let chamber = EchoChamber::new(db).with_resonator(resonator);

        let echoes = chamber
            .find_echoes(target_node_id, &candidate_ids)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        let resonant_entities: Vec<tools::ResonantEntity> = echoes
            .iter()
            .filter(|(_, score)| *score >= req.min_similarity)
            .take(req.limit)
            .map(|&(node_id, score)| tools::ResonantEntity {
                entity_id: format!("node-{}", node_id.as_u64()),
                similarity_score: score,
                fingerprint: tools::TemporalFingerprint {
                    bins: vec![],
                    resolution_us: ((req.window_seconds * 1_000_000) / req.num_bins as u64) as i64,
                },
            })
            .collect();

        let description = format!(
            "Found {} resonant entities for {} within {}-second window ({} bins)",
            resonant_entities.len(),
            req.target_entity_id,
            req.window_seconds,
            req.num_bins
        );

        Ok(tools::FindActivityResonanceResponse {
            resonant_entities,
            target_fingerprint: tools::TemporalFingerprint {
                bins: vec![],
                resolution_us: ((req.window_seconds * 1_000_000) / req.num_bins as u64) as i64,
            },
            description,
        })
    }

    async fn handle_compare_temporal_snapshots(
        &self,
        req: tools::CompareTemporalSnapshotsRequest,
    ) -> HandlerResult<tools::CompareTemporalSnapshotsResponse> {
        use aletheiadb::experimental::temporal_diff::TemporalDiff;

        let repo = self.state.repository();
        let db = repo.get_raw_db()?;

        let t1_micros = chrono::DateTime::parse_from_rfc3339(&req.t1)
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid t1: {}", e)))?
            .timestamp_micros();
        let t2_micros = chrono::DateTime::parse_from_rfc3339(&req.t2)
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid t2: {}", e)))?
            .timestamp_micros();

        let t1 = aletheiadb::core::hlc::HybridTimestamp::new(t1_micros, 0)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;
        let t2 = aletheiadb::core::hlc::HybridTimestamp::new(t2_micros, 0)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        let diff_engine = TemporalDiff::new(db);
        let report = diff_engine
            .compute_diff(t1, t2, req.limit)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        let mut added = 0;
        let mut removed = 0;
        let mut modified = 0;

        let changes: Vec<tools::EntityChange> = report
            .changes
            .iter()
            .map(|change| {
                use aletheiadb::experimental::temporal_diff::{
                    ChangeType, EntityChange as AletheiaChange,
                };

                let (entity_id, entity_type, change_type, property_diff) = match change {
                    AletheiaChange::Node { id, change } => {
                        let eid = format!("node-{}", id.as_u64());
                        match change {
                            ChangeType::Added => {
                                added += 1;
                                (eid, "node".to_string(), "added".to_string(), None)
                            }
                            ChangeType::Removed => {
                                removed += 1;
                                (eid, "node".to_string(), "removed".to_string(), None)
                            }
                            ChangeType::Modified { diff } => {
                                modified += 1;
                                let pdiff = tools::PropertyDiff {
                                    added: diff.added.clone(),
                                    removed: diff.removed.clone(),
                                    changed: diff.changed.clone(),
                                };
                                (eid, "node".to_string(), "modified".to_string(), Some(pdiff))
                            }
                        }
                    }
                    AletheiaChange::Edge { id, change } => {
                        let eid = format!("edge-{}", id.as_u64());
                        match change {
                            ChangeType::Added => {
                                added += 1;
                                (eid, "edge".to_string(), "added".to_string(), None)
                            }
                            ChangeType::Removed => {
                                removed += 1;
                                (eid, "edge".to_string(), "removed".to_string(), None)
                            }
                            ChangeType::Modified { diff } => {
                                modified += 1;
                                let pdiff = tools::PropertyDiff {
                                    added: diff.added.clone(),
                                    removed: diff.removed.clone(),
                                    changed: diff.changed.clone(),
                                };
                                (eid, "edge".to_string(), "modified".to_string(), Some(pdiff))
                            }
                        }
                    }
                };

                tools::EntityChange {
                    entity_id,
                    entity_type,
                    change_type,
                    property_diff,
                }
            })
            .collect();

        let summary = tools::ChangeSummary {
            total_changes: changes.len(),
            added,
            removed,
            modified,
        };
        let description = format!(
            "Compared snapshots {} vs {}: {} changes ({} added, {} removed, {} modified)",
            req.t1, req.t2, summary.total_changes, added, removed, modified
        );

        Ok(tools::CompareTemporalSnapshotsResponse {
            t1: req.t1,
            t2: req.t2,
            changes,
            summary,
            description,
        })
    }

    async fn handle_compute_concept_analogy(
        &self,
        req: tools::ComputeConceptAnalogyRequest,
    ) -> HandlerResult<tools::ComputeConceptAnalogyResponse> {
        use aletheiadb::core::id::NodeId;
        use aletheiadb::experimental::concept_algebra::ConceptAlgebra;

        let repo = self.state.repository();
        let db = repo.get_raw_db()?;

        // Parse node IDs
        let parse_node_id = |id_str: &str| -> HandlerResult<NodeId> {
            // Try parsing as "node-123" or just "123"
            let id_part = id_str.strip_prefix("node-").unwrap_or(id_str);
            let id_u64: u64 = id_part
                .parse()
                .map_err(|_| HandlerError::InvalidArgs(format!("Invalid node ID: {}", id_str)))?;
            NodeId::new(id_u64)
                .map_err(|_| HandlerError::InvalidArgs(format!("Invalid node ID: {}", id_str)))
        };

        let a = parse_node_id(&req.concept_a_id)?;
        let b = parse_node_id(&req.concept_b_id)?;
        let c = parse_node_id(&req.concept_c_id)?;

        // Create ConceptAlgebra instance
        let mut algebra = ConceptAlgebra::new(db);
        if let Some(ref prop) = req.property_name {
            algebra = algebra.with_property(prop);
        }

        // Perform analogy: A - B + C = ?
        let results = algebra
            .analogy(a, b, c, req.k)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        // Convert results
        let analogy_results: Vec<tools::ConceptAnalogyResult> = results
            .into_iter()
            .map(|(node_id, score)| tools::ConceptAnalogyResult {
                entity_id: format!("node-{}", node_id.as_u64()),
                score,
                label: None, // Could fetch node labels if needed
            })
            .collect();

        let description = format!(
            "Analogy: {} - {} + {} = ? (found {} results)",
            req.concept_a_id,
            req.concept_b_id,
            req.concept_c_id,
            analogy_results.len()
        );

        Ok(tools::ComputeConceptAnalogyResponse {
            results: analogy_results,
            analogy_description: description,
        })
    }

    async fn handle_predict_missing_connections(
        &self,
        req: tools::PredictMissingConnectionsRequest,
    ) -> HandlerResult<tools::PredictMissingConnectionsResponse> {
        use aletheiadb::experimental::prophet::Prophet;

        let repo = self.state.repository();
        let db = repo.get_raw_db()?;

        // Parse entity ID based on type
        let entity_uuid = uuid::Uuid::parse_str(&req.entity_id)
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid entity_id: {}", e)))?;

        let node_id = match req.entity_type.as_str() {
            "task" => {
                let task_id = harness_persistence::TaskId::from_uuid(entity_uuid);
                repo.get_node_id_for_task(task_id)?
            }
            "knowledge" => {
                let knowledge_id = harness_persistence::KnowledgeId::from_uuid(entity_uuid);
                repo.get_node_id_for_knowledge(knowledge_id)?
            }
            _ => {
                return Err(HandlerError::InvalidArgs(format!(
                    "Invalid entity_type: {}",
                    req.entity_type
                )));
            }
        };

        // Create Prophet instance
        let mut prophet = Prophet::new(db);
        if let Some(ref prop) = req.property_name {
            prophet = prophet.with_property(prop);
        }

        // Predict missing links
        let results = prophet
            .predict_links(node_id, req.limit)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        // Convert results to response format
        let predictions: Vec<tools::LinkPrediction> = results
            .into_iter()
            .map(|(node_id, score)| tools::LinkPrediction {
                target_entity_id: format!("node-{}", node_id.as_u64()),
                target_entity_type: req.entity_type.clone(),
                score,
                reason: format!(
                    "Predicted based on topological structure and semantic similarity (score: {:.3})",
                    score
                ),
            })
            .collect();

        Ok(tools::PredictMissingConnectionsResponse {
            predictions,
            source_entity_id: req.entity_id,
            source_entity_type: req.entity_type,
        })
    }

    async fn handle_analyze_semantic_spectrum(
        &self,
        req: tools::AnalyzeSemanticSpectrumRequest,
    ) -> HandlerResult<tools::AnalyzeSemanticSpectrumResponse> {
        use aletheiadb::core::id::NodeId;
        use aletheiadb::experimental::prism::Prism;

        let repo = self.state.repository();
        let db = repo.get_raw_db()?;

        // Parse node ID helper
        let parse_node_id = |id_str: &str| -> HandlerResult<NodeId> {
            let id_part = id_str.strip_prefix("node-").unwrap_or(id_str);
            let id_u64: u64 = id_part
                .parse()
                .map_err(|_| HandlerError::InvalidArgs(format!("Invalid node ID: {}", id_str)))?;
            NodeId::new(id_u64)
                .map_err(|_| HandlerError::InvalidArgs(format!("Invalid node ID: {}", id_str)))
        };

        let target_id = parse_node_id(&req.target_id)?;

        // Create Prism instance
        let mut prism = Prism::new(db);
        if let Some(ref prop) = req.vector_property {
            prism = prism.with_vector_property(prop);
        }

        // Add axes
        for axis_def in &req.axes {
            let axis_node_id = parse_node_id(&axis_def.reference_id)?;
            prism
                .add_axis_from_node(&axis_def.name, axis_node_id)
                .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;
        }

        // Orthogonalize if requested
        if req.orthogonalize {
            prism.orthogonalize();
        }

        // Analyze target
        let spectrum = prism
            .analyze_node(target_id)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        // Generate explanation
        let mut axis_scores: Vec<(String, f32)> =
            spectrum.iter().map(|(k, v)| (k.clone(), *v)).collect();
        axis_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let explanation = if axis_scores.is_empty() {
            "No axes defined for analysis".to_string()
        } else {
            let top_axis = &axis_scores[0];
            format!(
                "Target decomposes primarily along '{}' axis (score: {:.3}). Full spectrum: {}",
                top_axis.0,
                top_axis.1,
                axis_scores
                    .iter()
                    .map(|(name, score)| format!("{}: {:.3}", name, score))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        Ok(tools::AnalyzeSemanticSpectrumResponse {
            spectrum,
            explanation,
        })
    }

    async fn handle_predict_semantic_trajectory(
        &self,
        req: tools::PredictSemanticTrajectoryRequest,
    ) -> HandlerResult<tools::PredictSemanticTrajectoryResponse> {
        use aletheiadb::core::hlc::HybridTimestamp;
        use aletheiadb::core::temporal::{TimeRange, time};
        use aletheiadb::experimental::dreamer::Dreamer;
        use std::time::Duration;

        let repo = self.state.repository();
        let db = repo.get_raw_db()?;

        // Parse entity ID
        let entity_uuid = uuid::Uuid::parse_str(&req.entity_id)
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid entity_id: {}", e)))?;

        let node_id = match req.entity_type.as_str() {
            "task" => {
                let task_id = harness_persistence::TaskId::from_uuid(entity_uuid);
                repo.get_node_id_for_task(task_id)?
            }
            "knowledge" => {
                let knowledge_id = harness_persistence::KnowledgeId::from_uuid(entity_uuid);
                repo.get_node_id_for_knowledge(knowledge_id)?
            }
            _ => {
                return Err(HandlerError::InvalidArgs(format!(
                    "Invalid entity_type: {}",
                    req.entity_type
                )));
            }
        };

        // Create Dreamer instance
        let dreamer = Dreamer::new(db);

        // Define time window for history
        let now = time::now();
        let history_start_wallclock = now
            .wallclock()
            .saturating_sub(req.history_window_seconds as i64 * 1_000_000);
        let history_start = HybridTimestamp::new(history_start_wallclock, 0)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;
        let history_window = TimeRange::new(history_start, now)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        let future_horizon = Duration::from_secs(req.future_horizon_seconds);

        // Predict future trajectory
        let results = dreamer
            .predict_future(
                node_id,
                &req.property,
                history_window,
                future_horizon,
                req.k,
            )
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        // Convert results
        let predictions: Vec<tools::TrajectoryPrediction> = results
            .into_iter()
            .map(|(node_id, score)| tools::TrajectoryPrediction {
                entity_id: format!("node-{}", node_id.as_u64()),
                entity_type: req.entity_type.clone(),
                similarity_score: score,
            })
            .collect();

        let description = format!(
            "Analyzed {}-second history window, projected {} seconds forward. Found {} semantically similar entities.",
            req.history_window_seconds,
            req.future_horizon_seconds,
            predictions.len()
        );

        Ok(tools::PredictSemanticTrajectoryResponse {
            predictions,
            entity_id: req.entity_id,
            entity_type: req.entity_type,
            description,
        })
    }

    async fn handle_generate_history_narrative(
        &self,
        req: tools::GenerateHistoryNarrativeRequest,
    ) -> HandlerResult<tools::GenerateHistoryNarrativeResponse> {
        use aletheiadb::experimental::temporal_narrative::NarrativeGenerator;

        let repo = self.state.repository();
        let db = repo.get_raw_db()?;

        // Parse entity ID
        let entity_uuid = uuid::Uuid::parse_str(&req.entity_id)
            .map_err(|e| HandlerError::InvalidArgs(format!("Invalid entity_id: {}", e)))?;

        let node_id = match req.entity_type.as_str() {
            "task" => {
                let task_id = harness_persistence::TaskId::from_uuid(entity_uuid);
                repo.get_node_id_for_task(task_id)?
            }
            "knowledge" => {
                let knowledge_id = harness_persistence::KnowledgeId::from_uuid(entity_uuid);
                repo.get_node_id_for_knowledge(knowledge_id)?
            }
            _ => {
                return Err(HandlerError::InvalidArgs(format!(
                    "Invalid entity_type: {}",
                    req.entity_type
                )));
            }
        };

        // Create narrative generator
        let generator = NarrativeGenerator::new(db);

        // Generate narrative
        let events = generator
            .generate_node_narrative(node_id)
            .map_err(|e| HandlerError::Repository(RepositoryError::Database(e.to_string())))?;

        // Convert events to response format
        let narrative_events: Vec<tools::NarrativeEvent> = events
            .into_iter()
            .map(|event| tools::NarrativeEvent {
                timestamp: event.timestamp,
                version_number: event.version_number,
                description: event.description,
                changes: event.changes,
            })
            .collect();

        // Generate summary
        let narrative_summary = if narrative_events.is_empty() {
            format!("No history found for {} {}", req.entity_type, req.entity_id)
        } else {
            format!(
                "{} {} evolved through {} versions from {} to {}",
                req.entity_type
                    .chars()
                    .next()
                    .unwrap()
                    .to_uppercase()
                    .to_string()
                    + &req.entity_type[1..],
                req.entity_id,
                narrative_events.len(),
                narrative_events.first().unwrap().timestamp,
                narrative_events.last().unwrap().timestamp
            )
        };

        Ok(tools::GenerateHistoryNarrativeResponse {
            entity_id: req.entity_id,
            entity_type: req.entity_type,
            events: narrative_events,
            narrative_summary,
        })
    }

    // --- SONA MicroLoRA handlers ---

    async fn handle_record_agent_trajectory(
        &self,
        req: tools::RecordAgentTrajectoryRequest,
    ) -> HandlerResult<tools::RecordAgentTrajectoryResponse> {
        use crate::state::{StoredTrajectory, TrajectoryRecord};

        let agent_id = Self::parse_agent_id(&req.agent_id)?;
        let trajectory_id = uuid::Uuid::new_v4().to_string();
        let steps_count = req.trajectory.len() as u64;

        let records: Vec<TrajectoryRecord> = req
            .trajectory
            .into_iter()
            .map(|s| TrajectoryRecord {
                action: s.action,
                context: s.context,
                outcome: s.outcome,
                reward: s.reward,
            })
            .collect();

        let reward_sum: f64 = records.iter().map(|r| r.reward).sum();

        let stored = StoredTrajectory {
            id: trajectory_id.clone(),
            steps: records,
        };

        let mut lora_map = self.state.micro_lora().write().await;
        let data = lora_map.entry(agent_id).or_default();
        data.trajectories.push(stored);
        data.total_steps += steps_count;
        data.reward_sum += reward_sum;

        Ok(tools::RecordAgentTrajectoryResponse {
            trajectory_id,
            steps_recorded: steps_count,
        })
    }

    async fn handle_get_agent_lora_state(
        &self,
        req: tools::GetAgentLoraStateRequest,
    ) -> HandlerResult<tools::GetAgentLoraStateResponse> {
        let agent_id = Self::parse_agent_id(&req.agent_id)?;

        let lora_map = self.state.micro_lora().read().await;
        let data = lora_map.get(&agent_id);

        match data {
            Some(d) => Ok(tools::GetAgentLoraStateResponse {
                agent_id: req.agent_id,
                trajectories_ingested: d.trajectories_ingested(),
                total_steps: d.total_steps,
                mean_reward: d.mean_reward(),
            }),
            None => Ok(tools::GetAgentLoraStateResponse {
                agent_id: req.agent_id,
                trajectories_ingested: 0,
                total_steps: 0,
                mean_reward: 0.0,
            }),
        }
    }

    async fn handle_apply_agent_optimization(
        &self,
        req: tools::ApplyAgentOptimizationRequest,
    ) -> HandlerResult<tools::ApplyAgentOptimizationResponse> {
        let agent_id = Self::parse_agent_id(&req.agent_id)?;

        let lora_map = self.state.micro_lora().read().await;
        let data = lora_map.get(&agent_id);

        let mut ranked: Vec<tools::RankedAction> = req
            .candidate_actions
            .iter()
            .map(|action| {
                let mean_reward = data
                    .and_then(|d| d.action_mean_reward(action))
                    .unwrap_or(0.5); // Default 0.5 for unknown actions

                // Map reward [-1, 1] to confidence [0, 1]
                let confidence = ((mean_reward + 1.0) / 2.0).clamp(0.0, 1.0);

                tools::RankedAction {
                    action: action.clone(),
                    confidence,
                }
            })
            .collect();

        // Sort by confidence descending
        ranked.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(tools::ApplyAgentOptimizationResponse {
            ranked_actions: ranked,
        })
    }

    async fn handle_persist_agent_lora(
        &self,
        req: tools::PersistAgentLoraRequest,
    ) -> HandlerResult<tools::PersistAgentLoraResponse> {
        let agent_id = Self::parse_agent_id(&req.agent_id)?;

        let lora_map = self.state.micro_lora().read().await;
        if let Some(data) = lora_map.get(&agent_id) {
            // Persist as knowledge entry with special kind
            let serialized = serde_json::to_string(data).map_err(|e| {
                HandlerError::InternalError(format!("Failed to serialize LoRA state: {e}"))
            })?;

            let knowledge = Knowledge::new(
                format!("__micro_lora_state__:{}", req.agent_id),
                KnowledgeKind::Activity,
                agent_id,
                self.state.session_id(),
            );

            // Store the serialized LoRA data as knowledge content
            let mut knowledge_with_data = knowledge;
            knowledge_with_data.content = serialized;

            self.state
                .repository()
                .create_knowledge(&knowledge_with_data)
                .await?;

            Ok(tools::PersistAgentLoraResponse { persisted: true })
        } else {
            Ok(tools::PersistAgentLoraResponse { persisted: false })
        }
    }

    async fn handle_restore_agent_lora(
        &self,
        req: tools::RestoreAgentLoraRequest,
    ) -> HandlerResult<tools::RestoreAgentLoraResponse> {
        use crate::state::AgentLoraData;

        let agent_id = Self::parse_agent_id(&req.agent_id)?;

        // Search for persisted LoRA state in knowledge entries
        let knowledge_entries = self
            .state
            .repository()
            .get_recent_knowledge(self.state.session_id(), 100)
            .await?;

        // Find the most recent LoRA state for this agent
        // Knowledge entries are returned with most recent first by convention,
        // but we search for the matching prefix regardless
        let persisted_entry = knowledge_entries
            .iter()
            .find(|k| {
                k.content.starts_with('{') && {
                    // Try to deserialize -- if it works and was stored with the right agent, use it
                    if let Ok(data) = serde_json::from_str::<AgentLoraData>(&k.content) {
                        // Verify it belongs to this agent by checking the author
                        k.author_id == agent_id && data.trajectories_ingested() > 0
                    } else {
                        false
                    }
                }
            })
            .or_else(|| {
                // Fallback: look for entries matching the prefix pattern
                // (from persist_agent_lora which stores raw serialized JSON)
                knowledge_entries
                    .iter()
                    .find(|k| k.author_id == agent_id && k.content.starts_with('{'))
            });

        if let Some(entry) = persisted_entry {
            match serde_json::from_str::<AgentLoraData>(&entry.content) {
                Ok(data) => {
                    let mut lora_map = self.state.micro_lora().write().await;
                    lora_map.insert(agent_id, data);
                    Ok(tools::RestoreAgentLoraResponse { restored: true })
                }
                Err(_) => Ok(tools::RestoreAgentLoraResponse { restored: false }),
            }
        } else {
            Ok(tools::RestoreAgentLoraResponse { restored: false })
        }
    }

    // --- SONA Integration handlers ---

    /// Record a trajectory event and associate it with a task.
    /// Called internally by task/knowledge handlers for auto-recording.
    async fn record_sona_trajectory(&self, trigger: LearningTrigger, task_id: Option<TaskId>) {
        let recorder = self.state.trajectory_recorder();
        match recorder.record(trigger).await {
            Ok(event_id) => {
                if let Some(tid) = task_id {
                    let mut map = self.state.task_trajectories().write().await;
                    map.entry(tid).or_default().push(event_id.to_string());
                }
                self.state
                    .learning_counters()
                    .total_events
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => {
                tracing::warn!("Failed to record SONA trajectory: {e}");
            }
        }
    }

    /// Auto-store a pattern in the reasoning bank from a completed task.
    async fn auto_store_pattern(&self, task: &Task, agent_id: AgentId) {
        let agent = self.state.repository().get_agent(agent_id).await.ok();
        let role = agent.map(|a| a.role).unwrap_or(AgentRole::Developer);

        let description = format!(
            "{}. {}",
            task.title,
            task.summary.as_deref().unwrap_or(&task.description)
        );

        let pattern = TaskPattern::new(&task.title, role, true, &description);

        let bank = self.state.reasoning_bank();
        match bank.store_pattern(pattern).await {
            Ok(_id) => {
                self.state
                    .learning_counters()
                    .instant_patterns
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => {
                tracing::warn!("Failed to store pattern in reasoning bank: {e}");
            }
        }
    }

    /// Auto-consolidate EWC++ weights from task trajectory on completion.
    async fn auto_consolidate_ewc(&self, task_id: TaskId, agent_id: AgentId) {
        if let Some(engine) = self.state.sona_engine() {
            // Get trajectory events for this task
            let recorder = self.state.trajectory_recorder();
            let all_events = recorder
                .query(harness_persistence::TrajectoryQuery::new().with_limit(1000))
                .await
                .unwrap_or_default();

            // Collect steps for this task
            let mut task_steps = Vec::new();
            for event in &all_events {
                if event.task_id() == Some(task_id) {
                    task_steps.extend_from_slice(event.steps());
                }
            }

            if task_steps.is_empty() {
                tracing::debug!(
                    task_id = %task_id,
                    agent_id = %agent_id,
                    "No trajectory steps found for EWC consolidation"
                );
                return;
            }

            // Fixed dimensionality for now (configurable in production)
            const WEIGHT_DIM: usize = 128;

            // Extract synthetic gradients from trajectory steps
            let gradients = engine.extract_gradients_from_trajectory(&task_steps, WEIGHT_DIM);

            // Synthetic weights (in production, these would come from actual model)
            let weights = vec![0.0f32; WEIGHT_DIM];

            // Consolidate
            match engine
                .on_task_complete(
                    agent_id,
                    &task_id.as_uuid().to_string(),
                    &weights,
                    &gradients,
                )
                .await
            {
                Ok(()) => {
                    tracing::info!(
                        task_id = %task_id,
                        agent_id = %agent_id,
                        num_gradients = gradients.len(),
                        "EWC++ auto-consolidation completed"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        task_id = %task_id,
                        agent_id = %agent_id,
                        error = %e,
                        "EWC++ auto-consolidation failed"
                    );
                }
            }
        }
    }

    async fn handle_get_task_trajectory(
        &self,
        req: tools::GetTaskTrajectoryRequest,
    ) -> HandlerResult<tools::GetTaskTrajectoryResponse> {
        let task_id = Self::parse_task_id(&req.task_id)?;

        let recorder = self.state.trajectory_recorder();
        let all_events = recorder
            .query(harness_persistence::TrajectoryQuery::new().with_limit(1000))
            .await
            .unwrap_or_default();

        let mut all_steps = Vec::new();
        let mut event_count = 0;

        for event in &all_events {
            if event.task_id() == Some(task_id) {
                event_count += 1;
                for step in event.steps() {
                    all_steps.push(tools::TrajectoryStepResponse {
                        kind: step.kind().to_string(),
                        agent_id: step.agent_id().as_uuid().to_string(),
                        payload: step.payload().clone(),
                    });
                }
            }
        }

        Ok(tools::GetTaskTrajectoryResponse {
            task_id: req.task_id,
            steps: all_steps,
            event_count,
        })
    }

    async fn handle_query_reasoning_bank(
        &self,
        req: tools::QueryReasoningBankRequest,
    ) -> HandlerResult<tools::QueryReasoningBankResponse> {
        let bank = self.state.reasoning_bank();

        let query = PatternQuery::new(&req.query).with_limit(req.limit);
        let similar = bank.find_similar(query).await?;

        let total_patterns = bank
            .query_patterns(PatternQuery::new("").with_limit(10000))
            .await
            .map(|p| p.len())
            .unwrap_or(0);

        let mut patterns: Vec<tools::PatternResponse> = similar
            .into_iter()
            .map(|sp| tools::PatternResponse {
                id: sp.pattern.id().to_string(),
                task_type: sp.pattern.task_type().to_string(),
                agent_role: format!("{:?}", sp.pattern.agent_role()),
                success: sp.pattern.success(),
                content: sp.pattern.description().to_string(),
                confidence: sp.similarity,
                source_trajectory_id: Some("auto-recorded".to_string()),
            })
            .collect();

        // Fallback: if no patterns in the in-memory store (e.g. after restart),
        // search knowledge entries from the persistent repo as a source of patterns.
        // Uses prefix-based matching so "caching" matches "cache", etc.
        if patterns.is_empty() && !req.query.is_empty() {
            let knowledge_entries: Vec<(harness_persistence::Knowledge, f32)> = self
                .state
                .repository()
                .get_recent_knowledge(self.state.session_id(), 100)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|k| (k, 0.0))
                .collect();

            let query_lower = req.query.to_lowercase();
            let query_words: Vec<String> = query_lower
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() >= 3)
                .map(String::from)
                .collect();

            for (entry, _score) in &knowledge_entries {
                let content_lower = entry.content.to_lowercase();
                let content_words: Vec<String> = content_lower
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| w.len() >= 3)
                    .map(String::from)
                    .collect();

                // Score: count query words that share a stem with content words.
                // Uses prefix matching (min 3 chars) so "caching" matches "cache".
                let matches = query_words
                    .iter()
                    .filter(|qw| {
                        let prefix = &qw[..qw.len().min(4)];
                        content_words.iter().any(|cw| {
                            cw.starts_with(prefix) || qw.starts_with(&cw[..cw.len().min(4)])
                        })
                    })
                    .count();

                if matches > 0 {
                    let sim = matches as f32 / query_words.len().max(1) as f32;
                    patterns.push(tools::PatternResponse {
                        id: entry.id.as_uuid().to_string(),
                        task_type: "knowledge".to_string(),
                        agent_role: "Developer".to_string(),
                        success: true,
                        content: entry.content.clone(),
                        confidence: sim,
                        source_trajectory_id: Some("repo-knowledge".to_string()),
                    });
                }
            }

            patterns.sort_by(|a, b| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            patterns.truncate(req.limit);
        }

        let total = total_patterns + patterns.len().saturating_sub(total_patterns);

        Ok(tools::QueryReasoningBankResponse {
            patterns,
            total_patterns: total,
        })
    }

    async fn handle_get_learning_status(
        &self,
        req: tools::GetLearningStatusRequest,
    ) -> HandlerResult<tools::GetLearningStatusResponse> {
        let counters = self.state.learning_counters();
        let total_events = counters
            .total_events
            .load(std::sync::atomic::Ordering::Relaxed);

        let patterns_learned = match req.loop_type.as_str() {
            "instant" => counters
                .instant_patterns
                .load(std::sync::atomic::Ordering::Relaxed),
            "background" => counters
                .background_optimizations
                .load(std::sync::atomic::Ordering::Relaxed),
            "coordination" => counters
                .coordination_patterns
                .load(std::sync::atomic::Ordering::Relaxed),
            _ => 0,
        };

        Ok(tools::GetLearningStatusResponse {
            loop_type: req.loop_type,
            enabled: self.state.is_sona_enabled(),
            patterns_learned,
            total_events,
        })
    }

    async fn handle_trigger_learning_cycle(
        &self,
        req: tools::TriggerLearningCycleRequest,
    ) -> HandlerResult<tools::TriggerLearningCycleResponse> {
        let counters = self.state.learning_counters();

        match req.loop_type.as_str() {
            "background" => {
                let recorder = self.state.trajectory_recorder();
                let all_events = recorder
                    .query(
                        harness_persistence::TrajectoryQuery::new()
                            .with_trigger_kind(TriggerKind::TaskComplete)
                            .with_limit(1000),
                    )
                    .await
                    .unwrap_or_default();

                let optimizations = all_events.len() as u64;
                counters
                    .background_optimizations
                    .fetch_add(optimizations, std::sync::atomic::Ordering::Relaxed);

                Ok(tools::TriggerLearningCycleResponse {
                    loop_type: req.loop_type,
                    optimizations_applied: optimizations,
                    patterns_created: optimizations,
                    cross_agent_patterns: None,
                })
            }
            "coordination" => {
                let recorder = self.state.trajectory_recorder();
                let all_events = recorder
                    .query(harness_persistence::TrajectoryQuery::new().with_limit(1000))
                    .await
                    .unwrap_or_default();

                let mut agent_knowledge: std::collections::HashMap<String, Vec<String>> =
                    std::collections::HashMap::new();

                for event in &all_events {
                    let agent_str = event.agent_id().as_uuid().to_string();
                    agent_knowledge
                        .entry(agent_str)
                        .or_default()
                        .push(event.summary().to_string());
                }

                let mut cross_patterns = Vec::new();
                let agents: Vec<String> = agent_knowledge.keys().cloned().collect();

                if agents.len() >= 2 {
                    for i in 0..agents.len() {
                        for j in (i + 1)..agents.len() {
                            let agent_a = &agents[i];
                            let agent_b = &agents[j];
                            let summaries_a = &agent_knowledge[agent_a];
                            let summaries_b = &agent_knowledge[agent_b];

                            let words_a: std::collections::HashSet<String> = summaries_a
                                .iter()
                                .flat_map(|s| {
                                    s.to_lowercase()
                                        .split_whitespace()
                                        .map(String::from)
                                        .collect::<Vec<_>>()
                                })
                                .collect();

                            let words_b: std::collections::HashSet<String> = summaries_b
                                .iter()
                                .flat_map(|s| {
                                    s.to_lowercase()
                                        .split_whitespace()
                                        .map(String::from)
                                        .collect::<Vec<_>>()
                                })
                                .collect();

                            let overlap: Vec<&String> = words_a.intersection(&words_b).collect();

                            if !overlap.is_empty() {
                                let shared_topics: Vec<String> = overlap
                                    .iter()
                                    .filter(|w| w.len() > 3)
                                    .take(5)
                                    .map(|w| w.to_string())
                                    .collect();

                                if !shared_topics.is_empty() {
                                    cross_patterns.push(tools::CrossAgentPattern {
                                        description: format!(
                                            "Cross-agent pattern: shared topics [{}]",
                                            shared_topics.join(", ")
                                        ),
                                        agents_involved: vec![agent_a.clone(), agent_b.clone()],
                                        confidence: 0.7,
                                    });
                                }
                            }
                        }
                    }
                }

                let patterns_created = cross_patterns.len() as u64;
                counters
                    .coordination_patterns
                    .fetch_add(patterns_created, std::sync::atomic::Ordering::Relaxed);

                Ok(tools::TriggerLearningCycleResponse {
                    loop_type: req.loop_type,
                    optimizations_applied: 0,
                    patterns_created,
                    cross_agent_patterns: Some(cross_patterns),
                })
            }
            other => Err(HandlerError::InvalidArgs(format!(
                "Unknown loop type: {}. Expected 'background' or 'coordination'.",
                other
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aletheiadb::AletheiaDB;
    use harness_orchestrator::{OrchestratorConfig, ProcessManager};
    use harness_persistence::{AletheiaRepository, InMemoryRepository, Session};

    async fn setup() -> (
        Arc<HiveState<InMemoryRepository>>,
        HiveHandler<InMemoryRepository>,
    ) {
        let repo = Arc::new(InMemoryRepository::new());
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let config = OrchestratorConfig::default();
        let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));

        let state = Arc::new(HiveState::new(session, repo, process_manager));
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
        assert!(names.contains(&"dispatch_ready_tasks"));
        assert!(names.contains(&"nudge_or_replan"));
        assert!(names.contains(&"task_completion_gate"));
        assert!(names.contains(&"spawn_team_and_handshake"));
        assert!(names.contains(&"spawn_team_from_template"));
        assert!(names.contains(&"supervise_team"));
        assert!(names.contains(&"team_runbook_prompt"));
        assert!(names.contains(&"hive_observability_snapshot"));
        assert!(names.contains(&"collect_agent_artifacts"));
        assert!(names.contains(&"refresh_session"));
        assert_eq!(names.len(), 59);
    }

    #[tokio::test]
    async fn test_dispatch_ready_tasks_assigns_only_ready_unblocked_tasks() {
        let (state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let idle_worker = Agent::new(AgentRole::Developer, state.session_id());
        let idle_worker_id = idle_worker.id.as_uuid().to_string();
        state.repository().create_agent(&idle_worker).await.unwrap();

        let blocker = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Blocker",
                    "description": "Must finish first",
                    "priority": "high"
                }),
            )
            .await
            .unwrap();
        let blocker: tools::CreateTaskResponse = serde_json::from_value(blocker).unwrap();

        let blocked = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Blocked task",
                    "description": "Waits on blocker",
                    "priority": "critical"
                }),
            )
            .await
            .unwrap();
        let blocked: tools::CreateTaskResponse = serde_json::from_value(blocked).unwrap();

        let ready = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Ready task",
                    "description": "Should dispatch now",
                    "priority": "medium"
                }),
            )
            .await
            .unwrap();
        let ready: tools::CreateTaskResponse = serde_json::from_value(ready).unwrap();

        handler
            .call_tool(
                "add_task_dependency",
                serde_json::json!({
                    "task_id": blocker.task_id,
                    "blocked_task_id": blocked.task_id,
                }),
            )
            .await
            .unwrap();

        // Keep the blocker out of the pending pool so this test isolates blocked-task filtering.
        handler
            .call_tool(
                "update_task_status",
                serde_json::json!({
                    "task_id": blocker.task_id,
                    "status": "in_progress"
                }),
            )
            .await
            .unwrap();

        let resp = handler
            .call_tool(
                "dispatch_ready_tasks",
                serde_json::json!({
                    "max_assignments": 10
                }),
            )
            .await
            .unwrap();

        let assignment_count = resp
            .get("assignment_count")
            .and_then(serde_json::Value::as_u64);
        assert_eq!(assignment_count, Some(1));

        let assignments = resp
            .get("assignments")
            .and_then(serde_json::Value::as_array)
            .unwrap();
        assert_eq!(assignments.len(), 1);
        assert_eq!(
            assignments[0]
                .get("task_id")
                .and_then(serde_json::Value::as_str),
            Some(ready.task_id.as_str())
        );
        assert_eq!(
            assignments[0]
                .get("agent_id")
                .and_then(serde_json::Value::as_str),
            Some(idle_worker_id.as_str())
        );

        let listed = handler
            .call_tool("list_tasks", serde_json::json!({}))
            .await
            .unwrap();
        let listed: tools::ListTasksResponse = serde_json::from_value(listed).unwrap();

        let ready_task = listed.tasks.iter().find(|t| t.id == ready.task_id).unwrap();
        let blocked_task = listed
            .tasks
            .iter()
            .find(|t| t.id == blocked.task_id)
            .unwrap();

        assert_eq!(
            ready_task.assigned_to.as_deref(),
            Some(idle_worker_id.as_str())
        );
        assert_eq!(blocked_task.assigned_to, None);
        assert_eq!(blocked_task.status, "pending");
    }

    #[tokio::test]
    async fn test_dispatch_ready_tasks_prefers_priority_then_oldest_task() {
        let (state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let idle_worker = Agent::new(AgentRole::Developer, state.session_id());
        state.repository().create_agent(&idle_worker).await.unwrap();

        let first_high = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "High - oldest",
                    "description": "Should be picked first among highs",
                    "priority": "high"
                }),
            )
            .await
            .unwrap();
        let first_high: tools::CreateTaskResponse = serde_json::from_value(first_high).unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;

        let second_high = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "High - newer",
                    "description": "Should lose age tie-break",
                    "priority": "high"
                }),
            )
            .await
            .unwrap();
        let second_high: tools::CreateTaskResponse = serde_json::from_value(second_high).unwrap();

        handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Medium",
                    "description": "Lower priority",
                    "priority": "medium"
                }),
            )
            .await
            .unwrap();

        let resp = handler
            .call_tool(
                "dispatch_ready_tasks",
                serde_json::json!({
                    "max_assignments": 1
                }),
            )
            .await
            .unwrap();

        let assignments = resp
            .get("assignments")
            .and_then(serde_json::Value::as_array)
            .unwrap();
        assert_eq!(assignments.len(), 1);
        assert_eq!(
            assignments[0]
                .get("task_id")
                .and_then(serde_json::Value::as_str),
            Some(first_high.task_id.as_str())
        );
        assert_ne!(
            assignments[0]
                .get("task_id")
                .and_then(serde_json::Value::as_str),
            Some(second_high.task_id.as_str())
        );
    }

    #[tokio::test]
    async fn test_dispatch_ready_tasks_skips_busy_or_inactive_agents() {
        let (state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let busy_agent = Agent::new(AgentRole::Developer, state.session_id());
        state.repository().create_agent(&busy_agent).await.unwrap();

        let busy_task = Task::new(
            "Busy work",
            "Marks worker as occupied",
            Priority::Low,
            state.session_id(),
        );
        state.repository().create_task(&busy_task).await.unwrap();
        state
            .repository()
            .update_agent_task(busy_agent.id, Some(busy_task.id))
            .await
            .unwrap();

        let inactive_agent = Agent::new(AgentRole::Developer, state.session_id());
        state
            .repository()
            .create_agent(&inactive_agent)
            .await
            .unwrap();
        state
            .repository()
            .update_agent_status(inactive_agent.id, AgentStatus::Killed)
            .await
            .unwrap();

        let idle_agent = Agent::new(AgentRole::Developer, state.session_id());
        let idle_agent_id = idle_agent.id.as_uuid().to_string();
        state.repository().create_agent(&idle_agent).await.unwrap();

        let ready = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Dispatch me",
                    "description": "Ready and unblocked",
                    "priority": "high"
                }),
            )
            .await
            .unwrap();
        let ready: tools::CreateTaskResponse = serde_json::from_value(ready).unwrap();

        let resp = handler
            .call_tool(
                "dispatch_ready_tasks",
                serde_json::json!({
                    "max_assignments": 1
                }),
            )
            .await
            .unwrap();

        let assignments = resp
            .get("assignments")
            .and_then(serde_json::Value::as_array)
            .unwrap();
        assert_eq!(assignments.len(), 1);
        assert_eq!(
            assignments[0]
                .get("task_id")
                .and_then(serde_json::Value::as_str),
            Some(ready.task_id.as_str())
        );
        assert_eq!(
            assignments[0]
                .get("agent_id")
                .and_then(serde_json::Value::as_str),
            Some(idle_agent_id.as_str())
        );
    }

    #[tokio::test]
    async fn test_nudge_or_replan_nudges_active_assignee() {
        let (state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let worker = Agent::new(AgentRole::Developer, state.session_id());
        let worker_id = worker.id.as_uuid().to_string();
        state.repository().create_agent(&worker).await.unwrap();

        let task = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Needs a nudge",
                    "description": "Waiting on assignee update",
                    "priority": "high"
                }),
            )
            .await
            .unwrap();
        let task: tools::CreateTaskResponse = serde_json::from_value(task).unwrap();

        handler
            .call_tool(
                "assign_task",
                serde_json::json!({
                    "task_id": task.task_id,
                    "agent_id": worker_id,
                }),
            )
            .await
            .unwrap();

        let resp = handler
            .call_tool(
                "nudge_or_replan",
                serde_json::json!({
                    "task_id": task.task_id,
                    "nudge_message": "Please provide a progress update in the task thread."
                }),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.get("task_id").and_then(serde_json::Value::as_str),
            Some(task.task_id.as_str())
        );
        assert_eq!(
            resp.get("action").and_then(serde_json::Value::as_str),
            Some("nudged")
        );
        assert!(
            resp.get("message_id")
                .and_then(serde_json::Value::as_str)
                .is_some(),
            "expected nudge_or_replan to include message_id when assignee is nudged"
        );
    }

    #[tokio::test]
    async fn test_nudge_or_replan_replans_when_assignee_inactive() {
        let (state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let worker = Agent::new(AgentRole::Developer, state.session_id());
        let worker_id = worker.id.as_uuid().to_string();
        state.repository().create_agent(&worker).await.unwrap();

        let task = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Recover ownership",
                    "description": "Assigned worker became inactive",
                    "priority": "critical"
                }),
            )
            .await
            .unwrap();
        let task: tools::CreateTaskResponse = serde_json::from_value(task).unwrap();

        handler
            .call_tool(
                "assign_task",
                serde_json::json!({
                    "task_id": task.task_id,
                    "agent_id": worker_id,
                }),
            )
            .await
            .unwrap();

        state
            .repository()
            .update_agent_status(worker.id, AgentStatus::Killed)
            .await
            .unwrap();

        let resp = handler
            .call_tool(
                "nudge_or_replan",
                serde_json::json!({
                    "task_id": task.task_id
                }),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.get("task_id").and_then(serde_json::Value::as_str),
            Some(task.task_id.as_str())
        );
        assert_eq!(
            resp.get("action").and_then(serde_json::Value::as_str),
            Some("replanned")
        );

        let listed = handler
            .call_tool("list_tasks", serde_json::json!({}))
            .await
            .unwrap();
        let listed: tools::ListTasksResponse = serde_json::from_value(listed).unwrap();
        let replanned = listed.tasks.iter().find(|t| t.id == task.task_id).unwrap();
        assert_eq!(replanned.assigned_to, None);
        assert_eq!(replanned.status, "pending");
    }

    #[tokio::test]
    async fn test_nudge_or_replan_requires_strategoi() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();

        let task = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Strategoi gate",
                    "description": "Only strategoi should invoke nudge_or_replan",
                    "priority": "medium"
                }),
            )
            .await
            .unwrap();
        let task: tools::CreateTaskResponse = serde_json::from_value(task).unwrap();

        let err = handler
            .call_tool(
                "nudge_or_replan",
                serde_json::json!({
                    "task_id": task.task_id
                }),
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("Only strategoi"));
    }

    #[tokio::test]
    async fn test_nudge_or_replan_auto_mode_reports_operational_counts() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let task = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Auto mode stale task",
                    "description": "Should produce stale task inspection metrics",
                    "priority": "high"
                }),
            )
            .await
            .unwrap();
        let task: tools::CreateTaskResponse = serde_json::from_value(task).unwrap();

        let resp = handler
            .call_tool(
                "nudge_or_replan",
                serde_json::json!({
                    "task_id": task.task_id,
                    "mode": "auto",
                    "inactivity_minutes": 30,
                    "nudge_message": "Please post a progress heartbeat."
                }),
            )
            .await
            .unwrap();

        assert!(
            resp.get("inspected_at")
                .and_then(serde_json::Value::as_str)
                .is_some()
        );
        assert!(
            resp.get("stale_task_count")
                .and_then(serde_json::Value::as_u64)
                .is_some()
        );
        assert!(
            resp.get("nudged_count")
                .and_then(serde_json::Value::as_u64)
                .is_some()
        );
        assert!(
            resp.get("reassigned_count")
                .and_then(serde_json::Value::as_u64)
                .is_some()
        );
        assert!(
            resp.get("actions")
                .and_then(serde_json::Value::as_array)
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_task_completion_gate_reports_required_check_state() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let task = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Completion gate target",
                    "description": "Needs completion checks before done",
                    "priority": "high"
                }),
            )
            .await
            .unwrap();
        let task: tools::CreateTaskResponse = serde_json::from_value(task).unwrap();

        let resp = handler
            .call_tool(
                "task_completion_gate",
                serde_json::json!({
                    "task_id": task.task_id,
                    "summary": "Implemented and validated behavior.",
                    "checks": [
                        {"name": "cargo test -p harness-mcp --lib", "passed": true},
                        {"name": "cargo clippy -p harness-mcp --lib -- -D warnings", "passed": true}
                    ],
                    "finalize": false
                }),
            )
            .await
            .unwrap();

        assert!(
            resp.get("allowed")
                .and_then(serde_json::Value::as_bool)
                .is_some()
        );
        assert!(
            resp.get("missing_checks")
                .and_then(serde_json::Value::as_array)
                .is_some()
        );
        assert!(
            resp.get("failed_checks")
                .and_then(serde_json::Value::as_array)
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_team_runbook_prompt_returns_required_protocol_sections() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let resp = handler
            .call_tool("team_runbook_prompt", serde_json::json!({}))
            .await
            .unwrap();

        let runbook = resp
            .get("runbook")
            .and_then(serde_json::Value::as_str)
            .unwrap();

        assert!(runbook.contains("STARTUP HANDSHAKE"));
        assert!(runbook.contains("STATUS CADENCE"));
        assert!(runbook.contains("BLOCKER FORMAT"));
        assert!(runbook.contains("DONE FORMAT"));
    }

    #[tokio::test]
    async fn test_hive_observability_snapshot_includes_operational_signals() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let resp = handler
            .call_tool(
                "hive_observability_snapshot",
                serde_json::json!({
                    "window_minutes": 60
                }),
            )
            .await
            .unwrap();

        assert!(resp.get("throughput").is_some());
        assert!(resp.get("stuck_tasks").is_some());
        assert!(resp.get("noisy_agents").is_some());
        assert!(resp.get("failed_commands").is_some());
        assert!(resp.get("coordination_latency").is_some());
    }

    #[tokio::test]
    async fn test_spawn_team_and_handshake_requires_minimum_two_agents() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let err = handler
            .call_tool(
                "spawn_team_and_handshake",
                serde_json::json!({
                    "role": "developer",
                    "agent_count": 1,
                    "cli_command": "codex",
                    "cli_args": ["exec", "{PROMPT}"]
                }),
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("agent_count must be at least 2"));
    }

    #[tokio::test]
    async fn test_spawn_team_and_handshake_validates_mode() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let err = handler
            .call_tool(
                "spawn_team_and_handshake",
                serde_json::json!({
                    "role": "developer",
                    "agent_count": 2,
                    "handshake_mode": "triangle",
                    "cli_command": "codex",
                    "cli_args": ["exec", "{PROMPT}"]
                }),
            )
            .await
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("Invalid handshake_mode: triangle. Use 'ring' or 'full_mesh'")
        );
    }

    #[tokio::test]
    async fn test_spawn_team_from_template_validates_template() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let err = handler
            .call_tool(
                "spawn_team_from_template",
                serde_json::json!({
                    "template": "unknown"
                }),
            )
            .await
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("Invalid template: unknown. Use 'feature', 'bugfix', or 'incident'")
        );
    }

    #[tokio::test]
    async fn test_supervise_team_empty_when_no_workers() {
        let (_state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let resp = handler
            .call_tool("supervise_team", serde_json::json!({}))
            .await
            .unwrap();
        let resp: tools::SuperviseTeamResponse = serde_json::from_value(resp).unwrap();

        assert_eq!(resp.total_agents, 0);
        assert_eq!(resp.healthy_agents, 0);
        assert!(resp.restarted_agents.is_empty());
        assert!(resp.issues.is_empty());
        assert!(resp.escalations.is_empty());
    }

    fn test_shell_for_output(stdout: &str, stderr: &str) -> (String, Vec<String>) {
        if cfg!(windows) {
            let command = format!("echo {stdout} && echo {stderr} 1>&2");
            ("cmd.exe".to_string(), vec!["/c".to_string(), command])
        } else {
            let command = format!("echo {stdout}; echo {stderr} 1>&2");
            ("sh".to_string(), vec!["-c".to_string(), command])
        }
    }

    fn test_shell_for_empty_output() -> (String, Vec<String>) {
        if cfg!(windows) {
            (
                "cmd.exe".to_string(),
                vec!["/c".to_string(), "ver > nul".to_string()],
            )
        } else {
            ("sh".to_string(), vec!["-c".to_string(), ":".to_string()])
        }
    }

    #[tokio::test]
    async fn test_collect_agent_artifacts_extracts_stdout_and_stderr_to_knowledge() {
        let (state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let worker = Agent::new(AgentRole::Developer, state.session_id());
        state.repository().create_agent(&worker).await.unwrap();

        let task_resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "artifact task",
                    "description": "collect worker output",
                    "priority": "high"
                }),
            )
            .await
            .unwrap();
        let task_resp: tools::CreateTaskResponse = serde_json::from_value(task_resp).unwrap();

        let (command, args) = test_shell_for_output("artifact_done", "artifact_failed");
        state
            .process_manager()
            .spawn_with_cli(worker.id, &command, &args, "capture artifacts")
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let response = handler
            .call_tool(
                "collect_agent_artifacts",
                serde_json::json!({
                    "agent_id": worker.id.as_uuid().to_string(),
                    "task_id": task_resp.task_id,
                }),
            )
            .await
            .unwrap();

        let created_count = response
            .get("created_count")
            .and_then(serde_json::Value::as_u64);
        assert_eq!(created_count, Some(2));

        let artifacts = response
            .get("artifacts")
            .and_then(serde_json::Value::as_array)
            .unwrap();
        assert_eq!(artifacts.len(), 2);

        let streams: std::collections::HashSet<&str> = artifacts
            .iter()
            .filter_map(|a| a.get("stream").and_then(serde_json::Value::as_str))
            .collect();
        assert!(streams.contains("stdout"));
        assert!(streams.contains("stderr"));

        let context = handler
            .call_tool(
                "get_task_context",
                serde_json::json!({
                    "task_id": task_resp.task_id
                }),
            )
            .await
            .unwrap();
        let context: tools::GetTaskContextResponse = serde_json::from_value(context).unwrap();

        let kinds: std::collections::HashSet<String> =
            context.knowledge.iter().map(|k| k.kind.clone()).collect();
        assert!(kinds.contains("discovery"));
        assert!(kinds.contains("blocker"));
    }

    #[tokio::test]
    async fn test_collect_agent_artifacts_skips_empty_output() {
        let (state, handler) = setup().await;

        handler
            .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
            .await
            .unwrap();

        let worker = Agent::new(AgentRole::Developer, state.session_id());
        state.repository().create_agent(&worker).await.unwrap();

        let (command, args) = test_shell_for_empty_output();
        state
            .process_manager()
            .spawn_with_cli(worker.id, &command, &args, "capture artifacts")
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let response = handler
            .call_tool(
                "collect_agent_artifacts",
                serde_json::json!({
                    "agent_id": worker.id.as_uuid().to_string()
                }),
            )
            .await
            .unwrap();

        let created_count = response
            .get("created_count")
            .and_then(serde_json::Value::as_u64);
        assert_eq!(created_count, Some(0));

        let artifacts = response
            .get("artifacts")
            .and_then(serde_json::Value::as_array)
            .unwrap();
        assert!(artifacts.is_empty());
    }

    #[tokio::test]
    async fn test_refresh_session_rebinds_from_persisted_session_agent() {
        let (state, _handler) = setup().await;
        let handler = HiveHandler::new(state.clone()).with_mcp_session("mcp-test-refresh".into());

        let reg = handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();
        let reg: tools::RegisterAgentResponse = serde_json::from_value(reg).unwrap();

        let refreshed = handler
            .call_tool("refresh_session", serde_json::json!({}))
            .await
            .unwrap();
        let refreshed: tools::RefreshSessionResponse = serde_json::from_value(refreshed).unwrap();

        assert!(refreshed.rebound);
        assert_eq!(refreshed.agent_id, Some(reg.agent_id));
        assert_eq!(
            refreshed.mcp_session_id,
            Some("mcp-test-refresh".to_string())
        );
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

    #[tokio::test]
    async fn test_ewc_auto_consolidation_on_task_complete() {
        use harness_sona::SonaEngine;
        use harness_sona::ewc::EwcConfig;
        use std::sync::Arc;

        // Setup with SONA engine enabled
        let db = Arc::new(AletheiaDB::new().unwrap());
        let repo = Arc::new(AletheiaRepository::new_anon(db));
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let config = OrchestratorConfig::default();
        let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));

        // Create SONA engine with auto-consolidation enabled
        let ewc_config = EwcConfig {
            lambda: 0.5,
            gamma: 0.9,
            max_tasks: 10,
            normalize_fisher: true,
        };
        let sona_engine = Arc::new(
            SonaEngine::builder()
                .with_ewc(ewc_config)
                .with_auto_consolidate(true)
                .build()
                .unwrap(),
        );

        let state = Arc::new(
            HiveState::new(session, repo.clone(), process_manager)
                .with_sona_engine(sona_engine.clone()),
        );
        let handler = HiveHandler::new(state.clone());

        // Register agent
        let agent_resp = handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .unwrap();
        let agent_resp: tools::RegisterAgentResponse = serde_json::from_value(agent_resp).unwrap();
        let agent_id = AgentId::from_uuid(uuid::Uuid::parse_str(&agent_resp.agent_id).unwrap());

        // Create task
        let task_resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": "Test EWC consolidation",
                    "description": "Verify auto-consolidation works",
                    "priority": "medium"
                }),
            )
            .await
            .unwrap();
        let task_resp: tools::CreateTaskResponse = serde_json::from_value(task_resp).unwrap();

        // Claim task to start trajectory recording
        handler
            .call_tool(
                "claim_task",
                serde_json::json!({"task_id": task_resp.task_id}),
            )
            .await
            .unwrap();

        // Mark as in progress
        handler
            .call_tool(
                "update_task_status",
                serde_json::json!({
                    "task_id": task_resp.task_id,
                    "status": "in_progress",
                    "summary": "Working on it"
                }),
            )
            .await
            .unwrap();

        // Complete the task (should trigger auto-consolidation)
        handler
            .call_tool(
                "update_task_status",
                serde_json::json!({
                    "task_id": task_resp.task_id,
                    "status": "completed",
                    "summary": "Successfully completed"
                }),
            )
            .await
            .unwrap();

        // Verify EWC consolidation occurred
        let ewc_state = sona_engine.agent_ewc_state(agent_id).await;
        assert!(
            ewc_state.is_some(),
            "EWC consolidator should exist for agent"
        );

        let consolidator = ewc_state.unwrap();
        assert!(
            consolidator.task_count() > 0,
            "EWC consolidator should have consolidated at least one task"
        );
    }
}
