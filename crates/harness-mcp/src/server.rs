//! MCP server implementation wrapping HiveHandler via rust-mcp-sdk.
//!
//! Implements the `ServerHandler` trait from rust-mcp-sdk v0.8 to expose
//! all 14 Hive Mind tools over HTTP/SSE.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rust_mcp_sdk::McpServer;
use rust_mcp_sdk::mcp_server::{HyperServerOptions, ServerHandler, ToMcpServerHandler};
use rust_mcp_sdk::schema::{
    CallToolError, CallToolRequestParams, CallToolResult, ContentBlock, Implementation,
    InitializeResult, LATEST_PROTOCOL_VERSION, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerCapabilitiesTools, TextContent, Tool, ToolInputSchema,
};

use harness_persistence::Repository;

use crate::handler::HiveHandler;
use crate::state::HiveState;

/// MCP server handler that wraps a `HiveHandler<R>`.
///
/// Tracks MCP session → agent_id mappings in HiveState so clients don't need
/// to pass `_agent_id` on every tool call.
pub struct HiveMcpServer<R: Repository + 'static> {
    state: Arc<HiveState<R>>,
}

const CODING_AGENT_SCHEDULER_HEARTBEAT_SECS: u64 = 30;

impl<R: Repository + 'static> HiveMcpServer<R> {
    /// Create a new MCP server handler backed by shared hive state.
    pub fn new(state: Arc<HiveState<R>>) -> Self {
        Self { state }
    }

    /// Build the `InitializeResult` describing this server's capabilities.
    pub fn server_info() -> InitializeResult {
        InitializeResult {
            capabilities: ServerCapabilities {
                tools: Some(ServerCapabilitiesTools {
                    list_changed: Some(false),
                }),
                completions: None,
                experimental: None,
                logging: None,
                prompts: None,
                resources: None,
                tasks: None,
            },
            instructions: Some(
                "Harness Hive Mind MCP server. Provides 42 tools for multi-agent \
                 coordination: task management, knowledge sharing, agent registration, \
                 direct messaging, planning (products, projects, plans), process management, \
                 strategoi-centric agent commanding, and AletheiaDB Nova experimental features \
                 (temporal paths, semantic navigation, clustering, graph layout, activity resonance, \
                 temporal snapshots)."
                    .into(),
            ),
            meta: None,
            protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
            server_info: Implementation {
                name: "harness-hive-mind".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                description: Some("Harness v2 Hive Mind MCP Server".into()),
                icons: vec![],
                title: None,
                website_url: None,
            },
        }
    }
}

#[async_trait]
impl<R: Repository + 'static> ServerHandler for HiveMcpServer<R> {
    async fn handle_list_tools_request(
        &self,
        _params: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> Result<ListToolsResult, rust_mcp_sdk::schema::RpcError> {
        Ok(ListToolsResult {
            tools: tool_definitions(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        runtime: Arc<dyn McpServer>,
    ) -> Result<CallToolResult, CallToolError> {
        // Extract MCP session ID from runtime (may be None for some transports)
        let session_id_opt = runtime.session_id();

        // Create handler with MCP session context if available
        let handler = if let Some(ref session_id) = session_id_opt {
            let h = HiveHandler::new(self.state.clone()).with_mcp_session(session_id.clone());

            tracing::debug!(
                session_id = %session_id,
                tool = %params.name,
                "MCP tool call received"
            );

            // Look up registered agent for this MCP session
            if let Some(agent_id) = self.state.get_session_agent(session_id).await {
                h.restore_agent_id(agent_id).await;
                tracing::debug!(
                    session_id = %session_id,
                    agent_id = %agent_id,
                    "Restored agent context from MCP session"
                );
            } else if let Ok(session) = self
                .state
                .repository()
                .get_session(self.state.session_id())
                .await
                && let Some(agent_id) = session.agent_id
                && self.state.repository().get_agent(agent_id).await.is_ok()
            {
                // Auto-rebind after daemon restart: in-memory MCP session map is empty,
                // but the session's last agent is persisted.
                self.state
                    .register_session_agent(session_id.clone(), agent_id)
                    .await;
                h.restore_agent_id(agent_id).await;
                tracing::info!(
                    session_id = %session_id,
                    agent_id = %agent_id,
                    "Recovered MCP session agent context from persisted session state"
                );
            }
            h
        } else {
            tracing::debug!(tool = %params.name, "MCP tool call received (no session ID)");
            HiveHandler::new(self.state.clone())
        };

        let arguments = match params.arguments {
            Some(map) => serde_json::Value::Object(map),
            None => serde_json::Value::Object(serde_json::Map::new()),
        };

        match handler.call_tool(&params.name, arguments).await {
            Ok(value) => {
                let text =
                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
                Ok(CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent::new(
                        text, None, None,
                    ))],
                    is_error: None,
                    meta: None,
                    structured_content: None,
                })
            }
            Err(e) => {
                // Return tool errors as content with is_error=true (MCP convention).
                Ok(CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent::new(
                        e.to_string(),
                        None,
                        None,
                    ))],
                    is_error: Some(true),
                    meta: None,
                    structured_content: None,
                })
            }
        }
    }
}

/// Cleanup zombie agents on daemon startup.
/// Marks agents with "active" or "starting" status as "killed" if their process isn't running.
async fn cleanup_zombie_agents<R: Repository + 'static>(state: &HiveState<R>) {
    use harness_persistence::AgentStatus;

    tracing::info!("Starting zombie agent cleanup...");

    let agents = match state.repository().list_agents(state.session_id()).await {
        Ok(agents) => agents,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to list agents for cleanup");
            return;
        }
    };

    let mut cleaned_count = 0;
    for agent in agents {
        // Only check agents that should be running
        if !matches!(agent.status, AgentStatus::Active | AgentStatus::Starting) {
            continue;
        }

        let is_running = state.process_manager().is_running(agent.id).await;
        if !is_running {
            tracing::info!(
                agent_id = %agent.id,
                status = ?agent.status,
                "Found zombie agent, marking as killed"
            );

            if let Err(e) = state
                .repository()
                .update_agent_status(agent.id, AgentStatus::Killed)
                .await
            {
                tracing::warn!(
                    agent_id = %agent.id,
                    error = %e,
                    "Failed to update zombie agent status"
                );
            } else {
                cleaned_count += 1;
            }
        }
    }

    if cleaned_count > 0 {
        tracing::info!(count = cleaned_count, "Zombie agent cleanup complete");
    } else {
        tracing::info!("No zombie agents found");
    }
}

/// Start the MCP server on the given host and port.
pub async fn start_mcp_server<R: Repository + 'static>(
    state: Arc<HiveState<R>>,
    host: &str,
    port: u16,
) -> Result<(), rust_mcp_sdk::error::McpSdkError> {
    // Cleanup zombie agents from previous crashes/sessions
    cleanup_zombie_agents(&state).await;

    let scheduler_state = state.clone();
    let scheduler_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            CODING_AGENT_SCHEDULER_HEARTBEAT_SECS,
        ));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            let heartbeat_handler = HiveHandler::new(scheduler_state.clone());
            match heartbeat_handler
                .run_coding_agent_scheduler_heartbeat_once()
                .await
            {
                Ok(run) => {
                    if run.executed_count > 0 || run.workflow_runs_consumed > 0 {
                        tracing::info!(
                            inspected_count = run.inspected_count,
                            executed_count = run.executed_count,
                            workflow_runs_consumed = run.workflow_runs_consumed,
                            "Coding-agent scheduler heartbeat completed work"
                        );
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "Coding-agent scheduler heartbeat iteration failed"
                    );
                }
            }
        }
    });

    let handler = HiveMcpServer::new(state.clone());
    let server_info = HiveMcpServer::<R>::server_info();

    let options = HyperServerOptions {
        host: host.to_string(),
        port,
        ..Default::default()
    };

    let server = rust_mcp_sdk::mcp_server::hyper_server::create_server(
        server_info,
        handler.to_mcp_server_handler(),
        options,
    );

    // Set up graceful shutdown signal handler
    let shutdown_signal = async {
        #[cfg(unix)]
        {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("failed to install SIGTERM handler");
            let mut sigint =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                    .expect("failed to install SIGINT handler");

            tokio::select! {
                _ = sigterm.recv() => tracing::info!("Received SIGTERM"),
                _ = sigint.recv() => tracing::info!("Received SIGINT"),
            }
        }

        #[cfg(windows)]
        {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
            tracing::info!("Received Ctrl+C");
        }
    };

    // Run server with graceful shutdown
    tracing::info!("MCP server starting, press Ctrl+C to shutdown gracefully");

    let result = tokio::select! {
        result = server.start() => {
            result
        }
        _ = shutdown_signal => {
            tracing::info!("Shutdown signal received, stopping server gracefully...");

            // Give pending operations a moment to complete
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            tracing::info!(
                session_id = %state.session_id(),
                "Graceful shutdown complete - session state persisted via WAL"
            );
            Ok(())
        }
    };

    scheduler_task.abort();
    let _ = scheduler_task.await;

    result
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

/// Helper to build a JSON Schema property map entry.
fn prop(type_str: &str, description: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    m.insert("type".into(), serde_json::Value::String(type_str.into()));
    m.insert(
        "description".into(),
        serde_json::Value::String(description.into()),
    );
    m
}

/// Helper to build a JSON Schema property with a default value.
fn prop_with_default(
    type_str: &str,
    description: &str,
    default: serde_json::Value,
) -> serde_json::Map<String, serde_json::Value> {
    let mut m = prop(type_str, description);
    m.insert("default".into(), default);
    m
}

fn make_tool(
    name: &str,
    description: &str,
    required: Vec<&str>,
    properties: HashMap<String, serde_json::Map<String, serde_json::Value>>,
) -> Tool {
    Tool {
        name: name.into(),
        description: Some(description.into()),
        input_schema: ToolInputSchema::new(
            required.into_iter().map(String::from).collect(),
            Some(properties),
            None,
        ),
        annotations: None,
        execution: None,
        icons: vec![],
        meta: None,
        output_schema: None,
        title: None,
    }
}

/// Helper to add _agent_id parameter to tool properties.
fn with_agent_id(
    mut props: HashMap<String, serde_json::Map<String, serde_json::Value>>,
) -> HashMap<String, serde_json::Map<String, serde_json::Value>> {
    props.insert(
        "_agent_id".into(),
        prop("string", "Optional agent ID for multi-client support"),
    );
    props
}

/// Returns the full list of tool definitions with JSON Schema input schemas.
pub fn tool_definitions() -> Vec<Tool> {
    vec![
        // --- Task tools ---
        make_tool(
            "create_task",
            "Create a new task in the hive.",
            vec!["title", "description"],
            with_agent_id(HashMap::from([
                ("title".into(), prop("string", "Title of the task")),
                (
                    "description".into(),
                    prop("string", "Detailed description of the task"),
                ),
                (
                    "priority".into(),
                    prop_with_default(
                        "string",
                        "Priority: low, medium, high, critical",
                        serde_json::Value::String("medium".into()),
                    ),
                ),
                (
                    "parent_task".into(),
                    prop("string", "Optional parent task ID"),
                ),
            ])),
        ),
        make_tool(
            "list_tasks",
            "List tasks in the current session, optionally filtered by status.",
            vec![],
            with_agent_id(HashMap::from([(
                "status".into(),
                prop(
                    "string",
                    "Filter by status: pending, claimed, in_progress, completed, failed",
                ),
            )])),
        ),
        make_tool(
            "claim_task",
            "Claim an unassigned task for the current agent.",
            vec!["task_id"],
            with_agent_id(HashMap::from([(
                "task_id".into(),
                prop("string", "ID of the task to claim"),
            )])),
        ),
        make_tool(
            "update_task_status",
            "Update the status of a task.",
            vec!["task_id", "status"],
            with_agent_id(HashMap::from([
                ("task_id".into(), prop("string", "ID of the task to update")),
                (
                    "status".into(),
                    prop(
                        "string",
                        "New status: pending, claimed, in_progress, completed, failed",
                    ),
                ),
                (
                    "summary".into(),
                    prop("string", "Optional completion summary"),
                ),
            ])),
        ),
        make_tool(
            "assign_task",
            "Assign a task to a specific agent.",
            vec!["task_id", "agent_id"],
            with_agent_id(HashMap::from([
                ("task_id".into(), prop("string", "ID of the task to assign")),
                (
                    "agent_id".into(),
                    prop("string", "ID of the agent to assign to"),
                ),
            ])),
        ),
        make_tool(
            "dispatch_ready_tasks",
            "Auto-assign ready pending tasks to eligible active agents while respecting blocking dependencies.",
            vec![],
            with_agent_id(HashMap::from([(
                "max_assignments".into(),
                prop_with_default(
                    "integer",
                    "Maximum number of task assignments to create in this dispatch pass",
                    serde_json::Value::Number(10.into()),
                ),
            )])),
        ),
        make_tool(
            "nudge_or_replan",
            "Nudge active assignees for stale tasks or replan ownership when assignees are inactive.",
            vec!["task_id"],
            with_agent_id(HashMap::from([
                (
                    "task_id".into(),
                    prop("string", "Task ID to inspect for nudge/replan"),
                ),
                (
                    "mode".into(),
                    prop_with_default(
                        "string",
                        "Execution mode: 'auto', 'nudge', or 'replan'",
                        serde_json::Value::String("auto".into()),
                    ),
                ),
                (
                    "inactivity_minutes".into(),
                    prop_with_default(
                        "integer",
                        "Minutes since last activity before task is considered stale",
                        serde_json::Value::Number(15.into()),
                    ),
                ),
                (
                    "max_actions".into(),
                    prop_with_default(
                        "integer",
                        "Maximum nudge/replan actions to perform in one call",
                        serde_json::Value::Number(10.into()),
                    ),
                ),
                (
                    "nudge_message".into(),
                    prop(
                        "string",
                        "Optional direct message content used when nudging assignees",
                    ),
                ),
                (
                    "directive".into(),
                    prop("string", "Optional strategoi directive attached to nudges"),
                ),
                (
                    "dry_run".into(),
                    prop_with_default(
                        "boolean",
                        "Compute nudge/replan actions without mutating task ownership or sending messages",
                        serde_json::Value::Bool(false),
                    ),
                ),
            ])),
        ),
        make_tool(
            "task_completion_gate",
            "Before marking complete, enforce required checks and summary fields.",
            vec!["task_id"],
            with_agent_id(HashMap::from([
                (
                    "task_id".into(),
                    prop("string", "Task ID to validate for completion"),
                ),
                (
                    "summary".into(),
                    prop("string", "Completion summary required for finalize path"),
                ),
                ("checks".into(), {
                    let mut checks_prop = serde_json::Map::new();
                    checks_prop.insert("type".to_string(), serde_json::json!("array"));
                    checks_prop.insert(
                        "description".to_string(),
                        serde_json::json!(
                            "Reported command checks, each with name and passed boolean"
                        ),
                    );
                    checks_prop.insert(
                        "items".to_string(),
                        serde_json::json!({
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "passed": {"type": "boolean"}
                            },
                            "required": ["name", "passed"]
                        }),
                    );
                    checks_prop
                }),
                ("required_checks".into(), {
                    let mut required_prop = serde_json::Map::new();
                    required_prop.insert("type".to_string(), serde_json::json!("array"));
                    required_prop.insert(
                        "description".to_string(),
                        serde_json::json!(
                            "Required check names; defaults to cargo test/clippy/code_review"
                        ),
                    );
                    required_prop
                        .insert("items".to_string(), serde_json::json!({"type": "string"}));
                    required_prop
                }),
                (
                    "finalize".into(),
                    prop_with_default(
                        "boolean",
                        "If true and gate passes, update task status to completed",
                        serde_json::Value::Bool(false),
                    ),
                ),
            ])),
        ),
        make_tool(
            "get_task_context",
            "Get full context for a task including knowledge and subtasks.",
            vec!["task_id"],
            with_agent_id(HashMap::from([(
                "task_id".into(),
                prop("string", "ID of the task to get context for"),
            )])),
        ),
        make_tool(
            "task_statistics",
            "Get task statistics for analytics dashboard including status counts, priority distribution, and completion metrics.",
            vec![],
            with_agent_id(HashMap::new()),
        ),
        make_tool(
            "cold_storage_query",
            "Query cold storage statistics and historical data availability from AletheiaDB's disk-based storage tier.",
            vec![],
            with_agent_id(HashMap::new()),
        ),
        make_tool(
            "export_project_graph",
            "Export the project graph for visualization in Graphviz DOT or JSON format, including tasks, agents, and knowledge connections.",
            vec![],
            with_agent_id(HashMap::from([
                (
                    "format".into(),
                    prop_with_default(
                        "string",
                        "Export format: 'dot' (Graphviz) or 'json'",
                        serde_json::Value::String("dot".into()),
                    ),
                ),
                (
                    "include_tasks".into(),
                    prop_with_default(
                        "boolean",
                        "Include task dependencies in the graph",
                        serde_json::Value::Bool(true),
                    ),
                ),
                (
                    "include_agents".into(),
                    prop_with_default(
                        "boolean",
                        "Include agent relationships in the graph",
                        serde_json::Value::Bool(true),
                    ),
                ),
                (
                    "include_knowledge".into(),
                    prop_with_default(
                        "boolean",
                        "Include knowledge graph connections",
                        serde_json::Value::Bool(false),
                    ),
                ),
            ])),
        ),
        make_tool(
            "add_task_dependency",
            "Add a blocking dependency between tasks: task_id blocks blocked_task_id from starting.",
            vec!["task_id", "blocked_task_id"],
            with_agent_id(HashMap::from([
                ("task_id".into(), prop("string", "ID of the blocking task")),
                (
                    "blocked_task_id".into(),
                    prop("string", "ID of the task being blocked"),
                ),
            ])),
        ),
        make_tool(
            "remove_task_dependency",
            "Remove a blocking dependency between tasks.",
            vec!["task_id", "blocked_task_id"],
            with_agent_id(HashMap::from([
                ("task_id".into(), prop("string", "ID of the blocking task")),
                (
                    "blocked_task_id".into(),
                    prop("string", "ID of the task being blocked"),
                ),
            ])),
        ),
        make_tool(
            "find_path",
            "Find a path between two tasks in the dependency graph using breadth-first search.",
            vec!["from_task_id", "to_task_id"],
            with_agent_id(HashMap::from([
                ("from_task_id".into(), prop("string", "Starting task ID")),
                ("to_task_id".into(), prop("string", "Target task ID")),
                (
                    "max_depth".into(),
                    prop_with_default(
                        "number",
                        "Maximum search depth to prevent infinite loops",
                        serde_json::Value::Number(10.into()),
                    ),
                ),
            ])),
        ),
        make_tool(
            "get_task_history",
            "Get the complete version history of a task showing all changes over time.",
            vec!["task_id"],
            with_agent_id(HashMap::from([(
                "task_id".into(),
                prop("string", "ID of the task to get history for"),
            )])),
        ),
        make_tool(
            "semantic_search_tasks",
            "Search tasks using semantic vector similarity based on title and description.",
            vec!["query"],
            with_agent_id(HashMap::from([
                (
                    "query".into(),
                    prop("string", "Search query for finding similar tasks"),
                ),
                (
                    "limit".into(),
                    prop_with_default(
                        "integer",
                        "Maximum number of results to return",
                        serde_json::Value::Number(10.into()),
                    ),
                ),
            ])),
        ),
        make_tool(
            "get_task_tree",
            "Get hierarchical task tree structure showing parent-child relationships.",
            vec![],
            with_agent_id(HashMap::from([
                (
                    "root_task_id".into(),
                    prop(
                        "string",
                        "Optional root task ID. If provided, returns tree rooted at this task. If not provided, returns all top-level tasks.",
                    ),
                ),
                (
                    "max_depth".into(),
                    prop("integer", "Maximum depth to traverse (default: unlimited)"),
                ),
            ])),
        ),
        make_tool(
            "get_task_as_of",
            "Retrieve task state at a specific point in bi-temporal time (time-travel query).",
            vec!["task_id", "valid_time"],
            with_agent_id(HashMap::from([
                ("task_id".into(), prop("string", "ID of the task to query")),
                (
                    "valid_time".into(),
                    prop(
                        "string",
                        "Valid time (when the fact was true) in RFC3339 format",
                    ),
                ),
                (
                    "transaction_time".into(),
                    prop(
                        "string",
                        "Transaction time (when recorded) in RFC3339 format, defaults to current time",
                    ),
                ),
            ])),
        ),
        // --- Knowledge tools ---
        make_tool(
            "share_knowledge",
            "Share a piece of knowledge with the hive.",
            vec!["content", "kind"],
            with_agent_id(HashMap::from([
                (
                    "content".into(),
                    prop("string", "The knowledge content to share"),
                ),
                (
                    "kind".into(),
                    prop(
                        "string",
                        "Kind of knowledge: activity, discovery, decision, blocker",
                    ),
                ),
                (
                    "task_id".into(),
                    prop("string", "Optional task ID to associate with"),
                ),
            ])),
        ),
        make_tool(
            "ask_hive",
            "Search the hive's collective knowledge.",
            vec!["query"],
            with_agent_id(HashMap::from([
                (
                    "query".into(),
                    prop("string", "Search query for the hive knowledge"),
                ),
                (
                    "limit".into(),
                    prop_with_default(
                        "integer",
                        "Maximum number of results to return",
                        serde_json::Value::Number(10.into()),
                    ),
                ),
            ])),
        ),
        make_tool(
            "fish_knowledge",
            "Retrieve knowledge through associative memory retrieval (vector + graph connections).",
            vec!["knowledge_id"],
            with_agent_id(HashMap::from([
                (
                    "knowledge_id".into(),
                    prop("string", "ID of the knowledge entry to fish from"),
                ),
                (
                    "limit".into(),
                    prop_with_default(
                        "integer",
                        "Maximum number of results to return",
                        serde_json::Value::Number(15.into()),
                    ),
                ),
            ])),
        ),
        make_tool(
            "knowledge_clusters",
            "Auto-cluster related knowledge entries using vector similarity to discover natural groupings.",
            vec![],
            with_agent_id(HashMap::from([
                (
                    "similarity_threshold".into(),
                    prop_with_default(
                        "number",
                        "Minimum similarity (0.0-1.0) for clustering",
                        serde_json::Value::Number(serde_json::Number::from_f64(0.7).unwrap()),
                    ),
                ),
                (
                    "min_cluster_size".into(),
                    prop_with_default(
                        "integer",
                        "Minimum number of members to form a cluster",
                        serde_json::Value::Number(2.into()),
                    ),
                ),
            ])),
        ),
        // --- Agent tools ---
        make_tool(
            "register_agent",
            "Register a new agent in the hive with the given role.",
            vec!["role"],
            with_agent_id(HashMap::from([
                (
                    "role".into(),
                    prop(
                        "string",
                        "Agent role: strategoi, business_analyst, product_manager, architect, developer, tester",
                    ),
                ),
                (
                    "project_name".into(),
                    prop(
                        "string",
                        "Optional project name (e.g., 'harness', 'arthropod')",
                    ),
                ),
                (
                    "project_path".into(),
                    prop("string", "Optional project directory path"),
                ),
                (
                    "agent_id".into(),
                    prop(
                        "string",
                        "For spawned agents: pre-created agent ID to activate",
                    ),
                ),
            ])),
        ),
        make_tool(
            "list_agents",
            "List all agents in the current session.",
            vec![],
            with_agent_id(HashMap::new()),
        ),
        make_tool(
            "get_hive_status",
            "Get comprehensive status of the hive including agents, task summary, and recent knowledge.",
            vec![],
            with_agent_id(HashMap::new()),
        ),
        make_tool(
            "refresh_session",
            "Refresh MCP session agent binding after daemon restart or transport reconnect.",
            vec![],
            with_agent_id(HashMap::from([(
                "agent_id".into(),
                prop(
                    "string",
                    "Optional explicit agent ID to bind for this MCP session",
                ),
            )])),
        ),
        make_tool(
            "spawn_agent",
            "Spawn a new agent to work on tasks (strategoi only). Supports multi-CLI orchestration (Claude, Codex, Gemini, etc.).",
            vec!["role", "name", "cli_command", "cli_args"],
            with_agent_id(HashMap::from([
                (
                    "role".into(),
                    prop(
                        "string",
                        "Role for spawned agent (e.g., developer, architect, tester)",
                    ),
                ),
                (
                    "name".into(),
                    prop("string", "Name for the spawned teammate"),
                ),
                (
                    "cli_command".into(),
                    prop(
                        "string",
                        "CLI command to execute (e.g., 'claude', 'codex', 'gemini')",
                    ),
                ),
                ("cli_args".into(), {
                    let mut m = serde_json::Map::new();
                    m.insert("type".into(), serde_json::Value::String("array".into()));
                    m.insert(
                        "description".into(),
                        serde_json::Value::String(
                            "CLI arguments with {PROMPT} placeholder for system prompt injection"
                                .into(),
                        ),
                    );
                    let mut items = serde_json::Map::new();
                    items.insert("type".into(), serde_json::Value::String("string".into()));
                    m.insert("items".into(), serde_json::Value::Object(items));
                    m
                }),
                (
                    "custom_prompt".into(),
                    prop(
                        "string",
                        "Custom instructions to include in the generated system prompt",
                    ),
                ),
                (
                    "directive".into(),
                    prop(
                        "string",
                        "Strategoi directive appended to the standard agent template",
                    ),
                ),
                (
                    "poll_interval_secs".into(),
                    prop_with_default(
                        "number",
                        "Polling interval in seconds for auto-polling get_messages and get_hive_status",
                        serde_json::Value::Number(30.into()),
                    ),
                ),
                (
                    "initial_task_id".into(),
                    prop("string", "Optional task to assign immediately"),
                ),
            ])),
        ),
        make_tool(
            "spawn_team_and_handshake",
            "Spawn n agents and seed handshake direct messages between them (strategoi only).",
            vec!["role", "agent_count", "cli_command", "cli_args"],
            with_agent_id(HashMap::from([
                (
                    "role".into(),
                    prop(
                        "string",
                        "Role for spawned agents (MVP supports 'developer')",
                    ),
                ),
                (
                    "agent_count".into(),
                    prop("integer", "Number of agents to spawn (minimum 2)"),
                ),
                (
                    "cli_command".into(),
                    prop(
                        "string",
                        "CLI command to execute for each agent (e.g., 'claude', 'codex', 'gemini')",
                    ),
                ),
                ("cli_args".into(), {
                    let mut m = serde_json::Map::new();
                    m.insert("type".into(), serde_json::Value::String("array".into()));
                    m.insert(
                        "description".into(),
                        serde_json::Value::String(
                            "CLI arguments with {PROMPT} placeholder for system prompt injection"
                                .into(),
                        ),
                    );
                    let mut items = serde_json::Map::new();
                    items.insert("type".into(), serde_json::Value::String("string".into()));
                    m.insert("items".into(), serde_json::Value::Object(items));
                    m
                }),
                (
                    "custom_prompt".into(),
                    prop(
                        "string",
                        "Custom instructions to include in each generated system prompt",
                    ),
                ),
                (
                    "directive".into(),
                    prop(
                        "string",
                        "Strategoi directive appended to each spawned agent template",
                    ),
                ),
                (
                    "poll_interval_secs".into(),
                    prop_with_default(
                        "number",
                        "Polling interval in seconds for auto-polling get_messages and get_hive_status",
                        serde_json::Value::Number(30.into()),
                    ),
                ),
                (
                    "handshake_mode".into(),
                    prop_with_default(
                        "string",
                        "Handshake topology to seed: 'ring' or 'full_mesh'",
                        serde_json::Value::String("ring".into()),
                    ),
                ),
                (
                    "handshake_message".into(),
                    prop("string", "Optional custom body for seeded handshake DMs"),
                ),
            ])),
        ),
        make_tool(
            "spawn_team_from_template",
            "Spawn a predefined team shape (feature, bugfix, incident) with role-specific defaults and seeded handshakes.",
            vec!["template"],
            with_agent_id(HashMap::from([
                (
                    "template".into(),
                    prop("string", "Template name: feature, bugfix, incident"),
                ),
                (
                    "template_path".into(),
                    prop(
                        "string",
                        "Optional path to a TOML template file (supports custom template names)",
                    ),
                ),
                (
                    "cli_command".into(),
                    prop(
                        "string",
                        "Optional global CLI command override for all template members",
                    ),
                ),
                ("cli_args".into(), {
                    let mut m = serde_json::Map::new();
                    m.insert("type".into(), serde_json::Value::String("array".into()));
                    m.insert(
                        "description".into(),
                        serde_json::Value::String(
                            "Optional global CLI args override for all template members".into(),
                        ),
                    );
                    let mut items = serde_json::Map::new();
                    items.insert("type".into(), serde_json::Value::String("string".into()));
                    m.insert("items".into(), serde_json::Value::Object(items));
                    m
                }),
                (
                    "directive".into(),
                    prop(
                        "string",
                        "Optional directive appended to each template member's default directive",
                    ),
                ),
                (
                    "poll_interval_secs".into(),
                    prop_with_default(
                        "number",
                        "Polling interval in seconds for auto-polling get_messages and get_hive_status",
                        serde_json::Value::Number(30.into()),
                    ),
                ),
                (
                    "handshake_mode".into(),
                    prop_with_default(
                        "string",
                        "Handshake topology to seed: 'ring' or 'full_mesh'",
                        serde_json::Value::String("ring".into()),
                    ),
                ),
                (
                    "handshake_message".into(),
                    prop("string", "Optional custom body for seeded handshake DMs"),
                ),
            ])),
        ),
        make_tool(
            "create_workflow",
            "Create a workflow definition for orchestration control-plane execution.",
            vec!["name", "definition"],
            with_agent_id(HashMap::from([
                (
                    "name".into(),
                    prop("string", "Human-readable workflow name"),
                ),
                (
                    "description".into(),
                    prop("string", "Optional workflow description"),
                ),
                (
                    "definition".into(),
                    prop(
                        "object",
                        "Workflow definition payload (steps, retries, timeout, backoff, failure policy)",
                    ),
                ),
            ])),
        ),
        make_tool(
            "list_workflows",
            "List registered workflow definitions.",
            vec![],
            with_agent_id(HashMap::new()),
        ),
        make_tool(
            "trigger_workflow",
            "Queue a workflow run for executor heartbeat consumption.",
            vec!["workflow_id"],
            with_agent_id(HashMap::from([
                (
                    "workflow_id".into(),
                    prop("string", "Workflow definition ID to trigger"),
                ),
                (
                    "payload".into(),
                    prop("object", "Optional run-scoped payload"),
                ),
            ])),
        ),
        make_tool(
            "list_workflow_runs",
            "List workflow runs and their latest persisted transition snapshot.",
            vec![],
            with_agent_id(HashMap::from([(
                "workflow_id".into(),
                prop("string", "Optional workflow_id filter"),
            )])),
        ),
        make_tool(
            "get_workflow_run",
            "Get one workflow run by ID including step evidence state.",
            vec!["workflow_run_id"],
            with_agent_id(HashMap::from([(
                "workflow_run_id".into(),
                prop("string", "Workflow run ID"),
            )])),
        ),
        make_tool(
            "pause_workflow",
            "Pause a workflow definition so new triggers are blocked.",
            vec!["workflow_id"],
            with_agent_id(HashMap::from([(
                "workflow_id".into(),
                prop("string", "Workflow definition ID"),
            )])),
        ),
        make_tool(
            "resume_workflow",
            "Resume a paused workflow definition.",
            vec!["workflow_id"],
            with_agent_id(HashMap::from([(
                "workflow_id".into(),
                prop("string", "Workflow definition ID"),
            )])),
        ),
        make_tool(
            "retry_step",
            "Retry a blocked/failed step in an existing workflow run.",
            vec!["workflow_run_id", "step_id"],
            with_agent_id(HashMap::from([
                ("workflow_run_id".into(), prop("string", "Workflow run ID")),
                ("step_id".into(), prop("string", "Step ID to retry")),
            ])),
        ),
        make_tool(
            "backfill_workflow",
            "Backfill workflow runs across a historical interval.",
            vec!["workflow_id", "from", "to"],
            with_agent_id(HashMap::from([
                (
                    "workflow_id".into(),
                    prop("string", "Workflow definition ID"),
                ),
                ("from".into(), prop("string", "RFC3339 start timestamp")),
                ("to".into(), prop("string", "RFC3339 end timestamp")),
                (
                    "dry_run".into(),
                    prop_with_default(
                        "boolean",
                        "When true, compute queued runs without creating them",
                        serde_json::Value::Bool(false),
                    ),
                ),
            ])),
        ),
        make_tool(
            "schedule_coding_agents",
            "Create a recurring schedule that generates tasks from a reusable prompt template.",
            vec!["name", "prompt_template"],
            with_agent_id(HashMap::from([
                (
                    "name".into(),
                    prop("string", "Human-readable schedule name"),
                ),
                (
                    "cadence_minutes".into(),
                    prop_with_default(
                        "integer",
                        "Run cadence in minutes",
                        serde_json::Value::Number(1440.into()),
                    ),
                ),
                (
                    "prompt_template".into(),
                    prop(
                        "string",
                        "Reusable prompt template. Supports placeholders: {name}, {run_at}",
                    ),
                ),
                (
                    "task_title_template".into(),
                    prop_with_default(
                        "string",
                        "Optional task title template. Supports placeholders: {name}, {run_at}",
                        serde_json::Value::String("Scheduled coding-agent run [{name}]".into()),
                    ),
                ),
                (
                    "task_priority".into(),
                    prop_with_default(
                        "string",
                        "Generated task priority: low, medium, high, critical",
                        serde_json::Value::String("medium".into()),
                    ),
                ),
                (
                    "auto_dispatch".into(),
                    prop_with_default(
                        "boolean",
                        "Auto-assign generated tasks to idle active worker agents after creation",
                        serde_json::Value::Bool(true),
                    ),
                ),
                (
                    "backend".into(),
                    prop_with_default(
                        "string",
                        "Execution backend: local_cli (default) or jules",
                        serde_json::Value::String("local_cli".into()),
                    ),
                ),
                (
                    "jules_source".into(),
                    prop(
                        "string",
                        "Jules source context (required when backend=jules)",
                    ),
                ),
                (
                    "jules_state_path".into(),
                    prop("string", "Optional director state file path for Jules runs"),
                ),
                (
                    "jules_max_cycles".into(),
                    prop(
                        "integer",
                        "Optional max cycles for `director run --max-cycles`",
                    ),
                ),
                (
                    "start_at".into(),
                    prop(
                        "string",
                        "Optional first-run RFC3339 timestamp; defaults to now",
                    ),
                ),
            ])),
        ),
        make_tool(
            "list_coding_agent_schedules",
            "List strategoi-defined coding-agent schedules.",
            vec![],
            with_agent_id(HashMap::from([(
                "enabled_only".into(),
                prop_with_default(
                    "boolean",
                    "When true, return only enabled schedules",
                    serde_json::Value::Bool(false),
                ),
            )])),
        ),
        make_tool(
            "run_coding_agent_schedules",
            "Execute due coding-agent schedules and materialize maintenance tasks.",
            vec![],
            with_agent_id(HashMap::from([
                (
                    "schedule_id".into(),
                    prop("string", "Optional schedule ID to run a single schedule"),
                ),
                (
                    "max_schedules".into(),
                    prop_with_default(
                        "integer",
                        "Maximum schedules to inspect in this run",
                        serde_json::Value::Number(10.into()),
                    ),
                ),
                (
                    "force_run".into(),
                    prop_with_default(
                        "boolean",
                        "Run selected schedules even when they are not yet due",
                        serde_json::Value::Bool(false),
                    ),
                ),
                (
                    "dry_run".into(),
                    prop_with_default(
                        "boolean",
                        "Compute schedule execution outcomes without creating tasks",
                        serde_json::Value::Bool(false),
                    ),
                ),
            ])),
        ),
        make_tool(
            "supervise_team",
            "Inspect worker health and optionally auto-restart unhealthy agents from stored spawn specs.",
            vec![],
            with_agent_id(HashMap::from([
                (
                    "stale_after_secs".into(),
                    prop_with_default(
                        "number",
                        "Consider an agent stale if no heartbeat is observed for this many seconds",
                        serde_json::Value::Number(300.into()),
                    ),
                ),
                (
                    "recent_knowledge_limit".into(),
                    prop_with_default(
                        "integer",
                        "Number of recent knowledge entries to scan for heartbeat/activity",
                        serde_json::Value::Number(200.into()),
                    ),
                ),
                (
                    "auto_restart".into(),
                    prop_with_default(
                        "boolean",
                        "Restart non-running agents when spawn metadata is available",
                        serde_json::Value::Bool(false),
                    ),
                ),
            ])),
        ),
        make_tool(
            "team_runbook_prompt",
            "Return the strict shared team protocol all spawned agents should follow.",
            vec![],
            with_agent_id(HashMap::new()),
        ),
        make_tool(
            "hive_observability_snapshot",
            "Single tool to summarize throughput, stuck tasks, noisy agents, failed commands, and coordination latency.",
            vec![],
            with_agent_id(HashMap::from([
                (
                    "window_minutes".into(),
                    prop_with_default(
                        "integer",
                        "Lookback window for throughput and noise metrics in minutes",
                        serde_json::Value::Number(60.into()),
                    ),
                ),
                (
                    "stale_task_minutes".into(),
                    prop_with_default(
                        "integer",
                        "Minutes without task activity before classifying as stuck",
                        serde_json::Value::Number(30.into()),
                    ),
                ),
                (
                    "noisy_agent_threshold".into(),
                    prop_with_default(
                        "integer",
                        "Minimum knowledge-event count in window before flagging an agent as noisy",
                        serde_json::Value::Number(5.into()),
                    ),
                ),
            ])),
        ),
        make_tool(
            "disconnect_agent",
            "Disconnect an agent from the hive.",
            vec![],
            with_agent_id(HashMap::from([(
                "agent_id".into(),
                prop(
                    "string",
                    "Optional agent ID to disconnect (defaults to self)",
                ),
            )])),
        ),
        make_tool(
            "list_processes",
            "List all spawned agent processes with their running status.",
            vec![],
            with_agent_id(HashMap::new()),
        ),
        make_tool(
            "kill_process",
            "Kill a spawned agent process.",
            vec!["agent_id"],
            with_agent_id(HashMap::from([(
                "agent_id".into(),
                prop("string", "Agent ID whose process to kill"),
            )])),
        ),
        make_tool(
            "cleanup_stale_agents",
            "Cleanup agents stuck in 'starting' status that aren't actually running.",
            vec![],
            with_agent_id(HashMap::new()),
        ),
        make_tool(
            "get_process_output",
            "Get captured stdout and stderr from a spawned agent process.",
            vec!["agent_id"],
            with_agent_id(HashMap::from([(
                "agent_id".into(),
                prop("string", "Agent ID whose output to retrieve"),
            )])),
        ),
        make_tool(
            "collect_agent_artifacts",
            "Collect structured knowledge artifacts from a spawned agent's stdout/stderr output.",
            vec!["agent_id"],
            with_agent_id(HashMap::from([
                (
                    "agent_id".into(),
                    prop(
                        "string",
                        "Agent ID whose output to summarize into knowledge",
                    ),
                ),
                (
                    "task_id".into(),
                    prop(
                        "string",
                        "Optional task ID to associate extracted knowledge artifacts with",
                    ),
                ),
                (
                    "max_chars".into(),
                    prop_with_default(
                        "integer",
                        "Maximum characters to retain per stream summary",
                        serde_json::Value::Number(1200.into()),
                    ),
                ),
            ])),
        ),
        make_tool(
            "command_agent",
            "Command a spawned agent using either a raw prompt or a templated strategoi directive.",
            vec!["agent_id", "cli_command", "cli_args"],
            with_agent_id(HashMap::from([
                ("agent_id".into(), prop("string", "Agent ID to command")),
                (
                    "prompt".into(),
                    prop(
                        "string",
                        "Raw prompt to send to the agent (optional when using `directive`)",
                    ),
                ),
                (
                    "directive".into(),
                    prop(
                        "string",
                        "Strategoi directive to inject into the standard command template",
                    ),
                ),
                (
                    "cli_command".into(),
                    prop("string", "CLI command (e.g., 'codex', 'gemini')"),
                ),
                ("cli_args".into(), {
                    let mut args_prop = serde_json::Map::new();
                    args_prop.insert("type".to_string(), serde_json::json!("array"));
                    args_prop.insert("items".to_string(), serde_json::json!({"type": "string"}));
                    args_prop.insert(
                            "description".to_string(),
                            serde_json::json!("CLI arguments array (session resume will be added automatically for codex)"),
                        );
                    args_prop
                }),
            ])),
        ),
        // --- Message tools ---
        make_tool(
            "send_direct_message",
            "Send a direct message to another agent.",
            vec!["to_agent", "content"],
            with_agent_id(HashMap::from([
                (
                    "to_agent".into(),
                    prop("string", "ID of the recipient agent"),
                ),
                ("content".into(), prop("string", "Message content to send")),
                (
                    "task_id".into(),
                    prop("string", "Optional task ID to thread the message under"),
                ),
            ])),
        ),
        make_tool(
            "get_messages",
            "Get recent direct messages for the current agent.",
            vec![],
            with_agent_id(HashMap::from([(
                "limit".into(),
                prop_with_default(
                    "integer",
                    "Maximum number of messages to return",
                    serde_json::Value::Number(20.into()),
                ),
            )])),
        ),
        make_tool(
            "get_thread_messages",
            "Get messages in a task thread.",
            vec!["task_id"],
            with_agent_id(HashMap::from([
                (
                    "task_id".into(),
                    prop("string", "ID of the task thread to get messages for"),
                ),
                (
                    "limit".into(),
                    prop_with_default(
                        "integer",
                        "Maximum number of messages to return",
                        serde_json::Value::Number(20.into()),
                    ),
                ),
            ])),
        ),
        // --- Planning tools ---
        make_tool(
            "create_product",
            "Create a new product.",
            vec!["name", "description"],
            with_agent_id(HashMap::from([
                ("name".into(), prop("string", "Name of the product")),
                (
                    "description".into(),
                    prop("string", "Detailed description of the product"),
                ),
            ])),
        ),
        make_tool(
            "list_products",
            "List products in the current session, optionally filtered by status.",
            vec![],
            with_agent_id(HashMap::from([(
                "status".into(),
                prop(
                    "string",
                    "Filter by status: concept, active, maintenance, archived",
                ),
            )])),
        ),
        make_tool(
            "create_project",
            "Create a new project within a product.",
            vec!["product_id", "name", "description"],
            with_agent_id(HashMap::from([
                (
                    "product_id".into(),
                    prop("string", "ID of the parent product"),
                ),
                ("name".into(), prop("string", "Name of the project")),
                (
                    "description".into(),
                    prop("string", "Detailed description of the project"),
                ),
            ])),
        ),
        make_tool(
            "list_projects",
            "List projects in the current session, optionally filtered by product and/or status.",
            vec![],
            with_agent_id(HashMap::from([
                (
                    "product_id".into(),
                    prop("string", "Filter by parent product ID"),
                ),
                (
                    "status".into(),
                    prop(
                        "string",
                        "Filter by status: planning, active, on_hold, completed, archived",
                    ),
                ),
            ])),
        ),
        make_tool(
            "create_plan",
            "Create a new plan within a project.",
            vec!["project_id", "name", "strategy"],
            with_agent_id(HashMap::from([
                (
                    "project_id".into(),
                    prop("string", "ID of the parent project"),
                ),
                ("name".into(), prop("string", "Name of the plan")),
                (
                    "strategy".into(),
                    prop("string", "Strategic approach or execution strategy"),
                ),
            ])),
        ),
        make_tool(
            "list_plans",
            "List plans in the current session, optionally filtered by project and/or status.",
            vec![],
            with_agent_id(HashMap::from([
                (
                    "project_id".into(),
                    prop("string", "Filter by parent project ID"),
                ),
                (
                    "status".into(),
                    prop(
                        "string",
                        "Filter by status: draft, approved, in_execution, paused, completed, abandoned",
                    ),
                ),
            ])),
        ),
        // --- Nova Experimental Features ---
        make_tool(
            "find_temporal_path",
            "Find temporal path between two entities at a specific point in time using Chronos temporal navigation.",
            vec![
                "entity_type",
                "start_entity_id",
                "end_entity_id",
                "valid_time",
                "tx_time",
            ],
            with_agent_id(HashMap::from([
                (
                    "entity_type".into(),
                    prop("string", "Entity type: task | knowledge"),
                ),
                (
                    "start_entity_id".into(),
                    prop("string", "Starting entity UUID"),
                ),
                ("end_entity_id".into(), prop("string", "Target entity UUID")),
                ("valid_time".into(), prop("string", "Valid time (RFC3339)")),
                (
                    "tx_time".into(),
                    prop("string", "Transaction time (RFC3339)"),
                ),
            ])),
        ),
        make_tool(
            "navigate_semantic_graph",
            "Navigate graph using semantic similarity to find shortest path weighted by vector distance.",
            vec![
                "entity_type",
                "start_entity_id",
                "end_entity_id",
                "vector_property",
            ],
            with_agent_id(HashMap::from([
                (
                    "entity_type".into(),
                    prop("string", "Entity type: task | knowledge"),
                ),
                (
                    "start_entity_id".into(),
                    prop("string", "Starting entity UUID"),
                ),
                ("end_entity_id".into(), prop("string", "Target entity UUID")),
                (
                    "vector_property".into(),
                    prop(
                        "string",
                        "Vector property name to use for semantic distance",
                    ),
                ),
            ])),
        ),
        make_tool(
            "discover_semantic_clusters",
            "Discover semantic clusters using Cartographer k-means clustering on entity vectors.",
            vec!["vector_property", "num_clusters"],
            with_agent_id(HashMap::from([
                (
                    "vector_property".into(),
                    prop("string", "Vector property name to cluster on"),
                ),
                (
                    "num_clusters".into(),
                    prop("integer", "Number of clusters to discover"),
                ),
                (
                    "reify".into(),
                    prop_with_default(
                        "boolean",
                        "Create graph nodes for cluster regions",
                        serde_json::Value::Bool(false),
                    ),
                ),
            ])),
        ),
        make_tool(
            "generate_graph_layout",
            "Generate 2D graph layout using Kaleidoscope force-directed algorithm with optional semantic links.",
            vec!["entity_type"],
            with_agent_id(HashMap::from([
                (
                    "entity_type".into(),
                    prop("string", "Entity type: task | knowledge"),
                ),
                (
                    "iterations".into(),
                    prop_with_default(
                        "integer",
                        "Number of layout iterations",
                        serde_json::Value::Number(50.into()),
                    ),
                ),
                (
                    "width".into(),
                    prop_with_default(
                        "number",
                        "Layout width",
                        serde_json::Value::Number(serde_json::Number::from_f64(800.0).unwrap()),
                    ),
                ),
                (
                    "height".into(),
                    prop_with_default(
                        "number",
                        "Layout height",
                        serde_json::Value::Number(serde_json::Number::from_f64(600.0).unwrap()),
                    ),
                ),
                (
                    "vector_property".into(),
                    prop("string", "Optional vector property for semantic attraction"),
                ),
            ])),
        ),
        make_tool(
            "find_activity_resonance",
            "Find entities with similar temporal activity patterns using Echo Chamber activity density analysis.",
            vec![
                "entity_type",
                "target_entity_id",
                "window_seconds",
                "num_bins",
            ],
            with_agent_id(HashMap::from([
                (
                    "entity_type".into(),
                    prop("string", "Entity type: task | knowledge"),
                ),
                (
                    "target_entity_id".into(),
                    prop("string", "Target entity UUID to match activity against"),
                ),
                (
                    "window_seconds".into(),
                    prop("integer", "Time window for activity analysis (seconds)"),
                ),
                (
                    "num_bins".into(),
                    prop("integer", "Number of temporal bins for fingerprint"),
                ),
                (
                    "min_similarity".into(),
                    prop_with_default(
                        "number",
                        "Minimum similarity score (0-1)",
                        serde_json::Value::Number(serde_json::Number::from_f64(0.5).unwrap()),
                    ),
                ),
                (
                    "limit".into(),
                    prop_with_default(
                        "integer",
                        "Max results",
                        serde_json::Value::Number(10.into()),
                    ),
                ),
            ])),
        ),
        make_tool(
            "compare_temporal_snapshots",
            "Compare two temporal snapshots to identify added, removed, and modified entities using TemporalDiff.",
            vec!["t1", "t2"],
            with_agent_id(HashMap::from([
                ("t1".into(), prop("string", "First timestamp (RFC3339)")),
                ("t2".into(), prop("string", "Second timestamp (RFC3339)")),
                (
                    "limit".into(),
                    prop_with_default(
                        "integer",
                        "Max changes to return",
                        serde_json::Value::Number(100.into()),
                    ),
                ),
            ])),
        ),
        make_tool(
            "compute_concept_analogy",
            "Perform vector arithmetic for semantic reasoning: A - B + C = ? (e.g. 'king' - 'man' + 'woman' = 'queen'). Requires nodes with vector embeddings.",
            vec!["concept_a_id", "concept_b_id", "concept_c_id"],
            with_agent_id(HashMap::from([
                (
                    "concept_a_id".into(),
                    prop("string", "ID of the first concept node (A in 'A - B + C')"),
                ),
                (
                    "concept_b_id".into(),
                    prop("string", "ID of the concept to subtract (B in 'A - B + C')"),
                ),
                (
                    "concept_c_id".into(),
                    prop("string", "ID of the concept to add (C in 'A - B + C')"),
                ),
                (
                    "k".into(),
                    prop_with_default(
                        "integer",
                        "Number of results to return",
                        serde_json::Value::Number(5.into()),
                    ),
                ),
                (
                    "property_name".into(),
                    prop(
                        "string",
                        "Optional property name for vector lookup (auto-detected if omitted)",
                    ),
                ),
            ])),
        ),
        make_tool(
            "predict_missing_connections",
            "Predict missing connections using Prophet link prediction (topological + semantic analysis). Suggests which entities should be connected based on graph structure and vector similarity.",
            vec!["entity_type", "entity_id", "limit"],
            with_agent_id(HashMap::from([
                (
                    "entity_type".into(),
                    prop("string", "Entity type: task | knowledge | agent"),
                ),
                (
                    "entity_id".into(),
                    prop("string", "UUID of the entity to predict links for"),
                ),
                (
                    "limit".into(),
                    prop_with_default(
                        "integer",
                        "Maximum number of predictions to return",
                        serde_json::Value::Number(10.into()),
                    ),
                ),
                (
                    "property_name".into(),
                    prop(
                        "string",
                        "Optional vector property name for semantic scoring",
                    ),
                ),
            ])),
        ),
        make_tool(
            "analyze_semantic_spectrum",
            "Decompose a concept into semantic components using Prism spectroscopy. Projects a vector onto multiple axes to understand 'why' it's positioned where it is (e.g., 80% Technical + 20% Business).",
            vec!["target_id", "axes"],
            with_agent_id(HashMap::from([
                (
                    "target_id".into(),
                    prop("string", "ID of the entity to analyze"),
                ),
                (
                    "axes".into(),
                    prop(
                        "array",
                        "Array of axis definitions, each with 'name' and 'reference_id' fields",
                    ),
                ),
                (
                    "vector_property".into(),
                    prop(
                        "string",
                        "Optional vector property name (auto-detected if omitted)",
                    ),
                ),
                (
                    "orthogonalize".into(),
                    prop_with_default(
                        "boolean",
                        "Apply Gram-Schmidt orthogonalization for additive decomposition",
                        serde_json::Value::Bool(false),
                    ),
                ),
            ])),
        ),
        make_tool(
            "predict_semantic_trajectory",
            "Predict future semantic evolution using Dreamer temporal analysis. Analyzes historical vector changes to extrapolate where a concept is heading (e.g., 'Apple' moved from Fruit to Tech, where next?).",
            vec![
                "entity_type",
                "entity_id",
                "property",
                "history_window_seconds",
                "future_horizon_seconds",
            ],
            with_agent_id(HashMap::from([
                (
                    "entity_type".into(),
                    prop("string", "Entity type: task | knowledge | agent"),
                ),
                (
                    "entity_id".into(),
                    prop("string", "UUID of the entity to analyze"),
                ),
                (
                    "property".into(),
                    prop(
                        "string",
                        "Vector property name to track (e.g., 'embedding')",
                    ),
                ),
                (
                    "history_window_seconds".into(),
                    prop(
                        "integer",
                        "Time window in seconds to analyze for trajectory calculation",
                    ),
                ),
                (
                    "future_horizon_seconds".into(),
                    prop("integer", "How far into the future to project (in seconds)"),
                ),
                (
                    "k".into(),
                    prop_with_default(
                        "integer",
                        "Number of semantic neighbors to return",
                        serde_json::Value::Number(5.into()),
                    ),
                ),
            ])),
        ),
        make_tool(
            "generate_history_narrative",
            "Generate a human-readable narrative of an entity's temporal history. Creates a story of how the entity evolved over time with version-by-version change descriptions.",
            vec!["entity_id", "entity_type"],
            with_agent_id(HashMap::from([
                (
                    "entity_id".into(),
                    prop("string", "ID of the entity to narrate"),
                ),
                (
                    "entity_type".into(),
                    prop("string", "Entity type: task | knowledge | agent"),
                ),
            ])),
        ),
        // --- SONA MicroLoRA tools ---
        make_tool(
            "record_agent_trajectory",
            "Record a trajectory (sequence of action-context-outcome-reward steps) for an agent. Used to train per-agent MicroLoRA models.",
            vec!["agent_id", "trajectory"],
            with_agent_id(HashMap::from([
                (
                    "agent_id".into(),
                    prop("string", "Agent UUID to record trajectory for"),
                ),
                (
                    "trajectory".into(),
                    prop(
                        "array",
                        "Array of trajectory steps, each with action, context, outcome, and reward fields",
                    ),
                ),
            ])),
        ),
        make_tool(
            "get_agent_lora_state",
            "Get the current MicroLoRA state for an agent including trajectories ingested, mean reward, and action-specific statistics.",
            vec!["agent_id"],
            with_agent_id(HashMap::from([(
                "agent_id".into(),
                prop("string", "Agent UUID to get LoRA state for"),
            )])),
        ),
        make_tool(
            "apply_agent_optimization",
            "Apply an optimization to an agent's behavior based on accumulated trajectory data. Returns ranked candidate actions based on learned preferences.",
            vec!["agent_id", "context", "candidate_actions"],
            with_agent_id(HashMap::from([
                ("agent_id".into(), prop("string", "Agent UUID to optimize")),
                (
                    "context".into(),
                    prop("string", "Current context/situation description"),
                ),
                (
                    "candidate_actions".into(),
                    prop(
                        "array",
                        "Array of possible actions to rank by learned preferences",
                    ),
                ),
            ])),
        ),
        make_tool(
            "persist_agent_lora",
            "Persist an agent's MicroLoRA state to durable storage for later restoration.",
            vec!["agent_id"],
            with_agent_id(HashMap::from([(
                "agent_id".into(),
                prop("string", "Agent UUID whose LoRA state to persist"),
            )])),
        ),
        make_tool(
            "restore_agent_lora",
            "Restore an agent's MicroLoRA state from durable storage.",
            vec!["agent_id"],
            with_agent_id(HashMap::from([(
                "agent_id".into(),
                prop("string", "Agent UUID whose LoRA state to restore"),
            )])),
        ),
        // --- SONA Integration tools ---
        make_tool(
            "get_task_trajectory",
            "Retrieve trajectory steps recorded for a specific task, including all learning events triggered by task completion.",
            vec!["task_id"],
            with_agent_id(HashMap::from([(
                "task_id".into(),
                prop("string", "Task UUID to get trajectory for"),
            )])),
        ),
        make_tool(
            "query_reasoning_bank",
            "Search learned patterns from the reasoning bank using text similarity. Returns patterns with confidence scores.",
            vec!["query"],
            with_agent_id(HashMap::from([
                (
                    "query".into(),
                    prop("string", "Search query for finding similar patterns"),
                ),
                (
                    "limit".into(),
                    prop_with_default(
                        "integer",
                        "Maximum number of results to return",
                        serde_json::Value::Number(10.into()),
                    ),
                ),
            ])),
        ),
        make_tool(
            "get_learning_status",
            "Check the status of a SONA learning loop including patterns learned and total events recorded.",
            vec!["loop_type"],
            with_agent_id(HashMap::from([(
                "loop_type".into(),
                prop(
                    "string",
                    "Learning loop type: instant, background, or coordination",
                ),
            )])),
        ),
        make_tool(
            "trigger_learning_cycle",
            "Force a learning cycle for a specific loop type. Flushes buffered events and runs optimization.",
            vec!["loop_type"],
            with_agent_id(HashMap::from([(
                "loop_type".into(),
                prop(
                    "string",
                    "Learning loop type: instant, background, or coordination",
                ),
            )])),
        ),
        // --- Dream Simulation ---
        make_tool(
            "dream_simulation",
            "Generates a simulation context based on recent hive activity and historical patterns. Use this to predict future blockers or opportunities.",
            vec![],
            with_agent_id(HashMap::from([
                (
                    "project_id".into(),
                    prop("string", "Optional project ID to focus the dream on"),
                ),
                (
                    "lookback_limit".into(),
                    prop_with_default(
                        "integer",
                        "Number of recent events to analyze (default: 50)",
                        serde_json::Value::Number(50.into()),
                    ),
                ),
                (
                    "detailed_patterns".into(),
                    prop_with_default(
                        "boolean",
                        "Whether to return detailed pattern data",
                        serde_json::Value::Bool(false),
                    ),
                ),
            ])),
        ),
        // --- Chaos Engine ---
        make_tool(
            "inject_chaos",
            "Inject controlled failures (kill agents, block tasks) into the hive to test resilience.",
            vec!["kind"],
            with_agent_id(HashMap::from([
                (
                    "kind".into(),
                    prop("string", "Chaos kind: 'kill_agent' or 'block_task'"),
                ),
                (
                    "target_agent_id".into(),
                    prop("string", "Optional agent ID to kill (random if omitted)"),
                ),
                (
                    "target_task_id".into(),
                    prop("string", "Optional task ID to block (random if omitted)"),
                ),
                (
                    "duration".into(),
                    prop(
                        "integer",
                        "Optional duration in seconds (reserved for future use)",
                    ),
                ),
            ])),
        ),
        // --- Safety Net ---
        make_tool(
            "check_safety",
            "Proactively check if a planned action matches known failure patterns from the hive's collective experience.",
            vec!["action"],
            with_agent_id(HashMap::from([
                (
                    "action".into(),
                    prop("string", "Description of the action to check"),
                ),
                (
                    "threshold".into(),
                    prop_with_default(
                        "number",
                        "Minimum similarity score (0.0-1.0) to trigger a warning",
                        serde_json::Value::Number(serde_json::Number::from_f64(0.1).unwrap()),
                    ),
                ),
            ])),
        ),
        // --- Distiller ---
        make_tool(
            "distill_trajectories",
            "Export hive mind trajectories into a JSONL format suitable for local LLM fine-tuning.",
            vec![],
            with_agent_id(HashMap::from([(
                "limit".into(),
                prop_with_default(
                    "integer",
                    "Optional limit on the number of trajectory events to process (default: 100)",
                    serde_json::Value::Number(100.into()),
                ),
            )])),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_orchestrator::{OrchestratorConfig, ProcessManager};
    use harness_persistence::{InMemoryRepository, Session};

    #[test]
    fn test_tool_definitions_returns_expected_count() {
        let tools = tool_definitions();
        assert_eq!(tools.len(), 85);
    }

    #[test]
    fn test_workflow_control_plane_tools_are_registered() {
        let tools = tool_definitions();
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
        let expected = [
            "create_workflow",
            "list_workflows",
            "trigger_workflow",
            "list_workflow_runs",
            "get_workflow_run",
            "pause_workflow",
            "resume_workflow",
            "retry_step",
            "backfill_workflow",
        ];

        for name in expected {
            assert!(
                names.contains(&name),
                "Missing workflow control-plane tool definition: {name}"
            );
        }
    }

    #[test]
    fn test_workflow_control_plane_tools_expose_schema_contracts() {
        let tools = tool_definitions();
        let expected_contracts = [
            ("create_workflow", vec!["name", "definition"]),
            ("list_workflows", vec![]),
            ("trigger_workflow", vec!["workflow_id"]),
            ("list_workflow_runs", vec![]),
            ("get_workflow_run", vec!["workflow_run_id"]),
            ("pause_workflow", vec!["workflow_id"]),
            ("resume_workflow", vec!["workflow_id"]),
            ("retry_step", vec!["workflow_run_id", "step_id"]),
            ("backfill_workflow", vec!["workflow_id", "from", "to"]),
        ];

        for (name, required_fields) in expected_contracts {
            let tool = tools
                .iter()
                .find(|tool| tool.name == name)
                .unwrap_or_else(|| panic!("Missing workflow tool schema: {name}"));

            for field in required_fields {
                assert!(
                    tool.input_schema.required.contains(&field.to_string()),
                    "Workflow tool {name} missing required field: {field}"
                );
            }
        }
    }

    #[test]
    fn test_tool_names_match_handler() {
        let tools = tool_definitions();
        let expected_names = HiveHandler::<InMemoryRepository>::tool_names();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        for name in expected_names {
            assert!(
                tool_names.contains(name),
                "Missing tool definition for: {name}"
            );
        }
    }

    #[test]
    fn test_all_tools_have_descriptions() {
        let tools = tool_definitions();
        for tool in &tools {
            assert!(
                tool.description.is_some(),
                "Tool {} missing description",
                tool.name
            );
        }
    }

    #[test]
    fn test_all_tools_have_object_input_schema() {
        let tools = tool_definitions();
        for tool in &tools {
            assert_eq!(
                tool.input_schema.type_(),
                "object",
                "Tool {} input schema type is not 'object'",
                tool.name
            );
        }
    }

    #[test]
    fn test_server_info() {
        let info = HiveMcpServer::<InMemoryRepository>::server_info();
        assert_eq!(info.server_info.name, "harness-hive-mind");
        assert!(info.capabilities.tools.is_some());
        assert!(info.instructions.is_some());
    }

    #[test]
    fn test_hive_mcp_server_is_generic() {
        // Verify HiveMcpServer can be instantiated with InMemoryRepository explicitly.
        let repo = Arc::new(InMemoryRepository::new());
        let session = harness_persistence::Session::new(4);
        let config = OrchestratorConfig::default();
        let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));
        let state = Arc::new(HiveState::new(session, repo, process_manager));
        let _server: HiveMcpServer<InMemoryRepository> = HiveMcpServer::new(state);
    }

    #[tokio::test]
    async fn test_call_tool_dispatches_through_handler() {
        let repo = Arc::new(InMemoryRepository::new());
        let session = Session::new(8);
        repo.create_session(&session).await.expect("session");

        let config = OrchestratorConfig::default();
        let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));

        let state = Arc::new(HiveState::new(session, repo, process_manager));
        let _server = HiveMcpServer::new(state.clone());

        // Verify tool dispatch works via the underlying HiveHandler
        let handler = HiveHandler::new(state);
        let reg_result = handler
            .call_tool("register_agent", serde_json::json!({"role": "developer"}))
            .await
            .expect("register_agent");

        assert!(reg_result.get("agent_id").is_some());
    }

    #[test]
    fn test_create_task_tool_has_required_fields() {
        let tools = tool_definitions();
        let create_task = tools.iter().find(|t| t.name == "create_task").unwrap();
        assert!(create_task.input_schema.required.contains(&"title".into()));
        assert!(
            create_task
                .input_schema
                .required
                .contains(&"description".into())
        );
    }

    #[test]
    fn test_list_agents_tool_has_empty_properties() {
        let tools = tool_definitions();
        let list_agents = tools.iter().find(|t| t.name == "list_agents").unwrap();
        assert!(list_agents.input_schema.required.is_empty());
        let props = list_agents.input_schema.properties.as_ref().unwrap();
        // list_agents only has optional _agent_id parameter
        assert_eq!(props.len(), 1);
        assert!(props.contains_key("_agent_id"));
    }

    #[test]
    fn test_tool_definitions_are_serializable() {
        let tools = tool_definitions();
        let json = serde_json::to_string(&tools).expect("tools should be serializable");
        assert!(json.contains("create_task"));
        assert!(json.contains("get_thread_messages"));
    }
}
