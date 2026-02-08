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

use harness_persistence::InMemoryRepository;

use crate::handler::HiveHandler;
use crate::state::HiveState;

/// MCP server handler that wraps a `HiveHandler<InMemoryRepository>`.
///
/// Each incoming `call_tool` request creates a fresh `HiveHandler` so that
/// per-connection agent state (agent_id) stays isolated.
pub struct HiveMcpServer {
    state: Arc<HiveState<InMemoryRepository>>,
}

impl HiveMcpServer {
    /// Create a new MCP server handler backed by shared hive state.
    pub fn new(state: Arc<HiveState<InMemoryRepository>>) -> Self {
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
                "Harness Hive Mind MCP server. Provides 14 tools for multi-agent \
                 coordination: task management, knowledge sharing, agent registration, \
                 and direct messaging."
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
impl ServerHandler for HiveMcpServer {
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
        _runtime: Arc<dyn McpServer>,
    ) -> Result<CallToolResult, CallToolError> {
        let handler = HiveHandler::new(self.state.clone());

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

/// Start the MCP server on the given host and port.
pub async fn start_mcp_server(
    state: Arc<HiveState<InMemoryRepository>>,
    host: &str,
    port: u16,
) -> Result<(), rust_mcp_sdk::error::McpSdkError> {
    let handler = HiveMcpServer::new(state);
    let server_info = HiveMcpServer::server_info();

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

    server.start().await
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

/// Returns the full list of 14 tool definitions with JSON Schema input schemas.
pub fn tool_definitions() -> Vec<Tool> {
    vec![
        // --- Task tools ---
        make_tool(
            "create_task",
            "Create a new task in the hive.",
            vec!["title", "description"],
            HashMap::from([
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
            ]),
        ),
        make_tool(
            "list_tasks",
            "List tasks in the current session, optionally filtered by status.",
            vec![],
            HashMap::from([(
                "status".into(),
                prop(
                    "string",
                    "Filter by status: pending, claimed, in_progress, completed, failed",
                ),
            )]),
        ),
        make_tool(
            "claim_task",
            "Claim an unassigned task for the current agent.",
            vec!["task_id"],
            HashMap::from([("task_id".into(), prop("string", "ID of the task to claim"))]),
        ),
        make_tool(
            "update_task_status",
            "Update the status of a task.",
            vec!["task_id", "status"],
            HashMap::from([
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
            ]),
        ),
        make_tool(
            "assign_task",
            "Assign a task to a specific agent.",
            vec!["task_id", "agent_id"],
            HashMap::from([
                ("task_id".into(), prop("string", "ID of the task to assign")),
                (
                    "agent_id".into(),
                    prop("string", "ID of the agent to assign to"),
                ),
            ]),
        ),
        make_tool(
            "get_task_context",
            "Get full context for a task including knowledge and subtasks.",
            vec!["task_id"],
            HashMap::from([(
                "task_id".into(),
                prop("string", "ID of the task to get context for"),
            )]),
        ),
        // --- Knowledge tools ---
        make_tool(
            "share_knowledge",
            "Share a piece of knowledge with the hive.",
            vec!["content", "kind"],
            HashMap::from([
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
            ]),
        ),
        make_tool(
            "ask_hive",
            "Search the hive's collective knowledge.",
            vec!["query"],
            HashMap::from([
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
            ]),
        ),
        // --- Agent tools ---
        make_tool(
            "register_agent",
            "Register a new agent in the hive with the given role.",
            vec!["role"],
            HashMap::from([(
                "role".into(),
                prop(
                    "string",
                    "Agent role: strategoi, business_analyst, product_manager, architect, developer, tester",
                ),
            )]),
        ),
        make_tool(
            "list_agents",
            "List all agents in the current session.",
            vec![],
            HashMap::new(),
        ),
        make_tool(
            "get_hive_status",
            "Get comprehensive status of the hive including agents, task summary, and recent knowledge.",
            vec![],
            HashMap::new(),
        ),
        // --- Message tools ---
        make_tool(
            "send_direct_message",
            "Send a direct message to another agent.",
            vec!["to_agent", "content"],
            HashMap::from([
                (
                    "to_agent".into(),
                    prop("string", "ID of the recipient agent"),
                ),
                ("content".into(), prop("string", "Message content to send")),
                (
                    "task_id".into(),
                    prop("string", "Optional task ID to thread the message under"),
                ),
            ]),
        ),
        make_tool(
            "get_messages",
            "Get recent direct messages for the current agent.",
            vec![],
            HashMap::from([(
                "limit".into(),
                prop_with_default(
                    "integer",
                    "Maximum number of messages to return",
                    serde_json::Value::Number(20.into()),
                ),
            )]),
        ),
        make_tool(
            "get_thread_messages",
            "Get messages in a task thread.",
            vec!["task_id"],
            HashMap::from([
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
            ]),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_persistence::{Repository, Session};

    #[test]
    fn test_tool_definitions_returns_14_tools() {
        let tools = tool_definitions();
        assert_eq!(tools.len(), 14);
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
        let info = HiveMcpServer::server_info();
        assert_eq!(info.server_info.name, "harness-hive-mind");
        assert!(info.capabilities.tools.is_some());
        assert!(info.instructions.is_some());
    }

    #[tokio::test]
    async fn test_call_tool_dispatches_through_handler() {
        let repo = Arc::new(InMemoryRepository::new());
        let session = Session::new(8);
        repo.create_session(&session).await.expect("session");

        let state = Arc::new(HiveState::new(session, repo));
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
        assert!(props.is_empty());
    }

    #[test]
    fn test_tool_definitions_are_serializable() {
        let tools = tool_definitions();
        let json = serde_json::to_string(&tools).expect("tools should be serializable");
        assert!(json.contains("create_task"));
        assert!(json.contains("get_thread_messages"));
    }
}
