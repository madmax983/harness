//! MCP client bridge for the Harness TUI.

use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;
use rust_mcp_sdk::mcp_client::{ClientHandler, ClientRuntime, McpClientOptions, client_runtime};
use rust_mcp_sdk::schema::{
    CallToolRequestParams, CallToolResult, ClientCapabilities, Implementation,
    InitializeRequestParams, LATEST_PROTOCOL_VERSION, ListToolsResult,
};
use rust_mcp_sdk::{ClientSseTransport, ClientSseTransportOptions, McpClient, ToMcpClientHandler};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// Raw output from an MCP tool call.
#[derive(Debug, Clone)]
pub struct ToolCallOutput {
    pub tool_name: String,
    pub text: String,
    pub is_error: bool,
}

/// Thin wrapper around rust-mcp-sdk client runtime for tool-driven TUI operations.
pub struct McpToolClient {
    client: Arc<ClientRuntime>,
}

impl McpToolClient {
    /// Connect to an MCP server via SSE and initialize a client runtime.
    pub async fn connect(server_url: &str) -> Result<Self> {
        let transport = ClientSseTransport::new(server_url, ClientSseTransportOptions::default())
            .map_err(|error| {
            anyhow!("failed to create SSE transport for {server_url}: {error}")
        })?;

        let client_details = InitializeRequestParams {
            capabilities: ClientCapabilities::default(),
            client_info: Implementation {
                name: "harness-tui".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: Some("Harness TUI".into()),
                description: Some("Harness operator terminal UI client".into()),
                icons: vec![],
                website_url: None,
            },
            protocol_version: LATEST_PROTOCOL_VERSION.into(),
            meta: None,
        };

        let handler = NoopClientHandler;
        let client = client_runtime::create_client(McpClientOptions {
            client_details,
            transport,
            handler: handler.to_mcp_client_handler(),
            task_store: None,
            server_task_store: None,
        });

        client
            .clone()
            .start()
            .await
            .map_err(|error| anyhow!("failed to connect MCP client to {server_url}: {error}"))?;

        Ok(Self { client })
    }

    /// Return current MCP session ID if established.
    pub async fn session_id(&self) -> Option<String> {
        self.client.session_id().await.map(|id| id.to_string())
    }

    /// Retrieve full tool definitions from the MCP server.
    pub async fn list_tools(&self) -> Result<ListToolsResult> {
        self.client
            .request_tool_list(None)
            .await
            .map_err(|error| anyhow!("list_tools request failed: {error}"))
    }

    /// Invoke one tool and return raw text payload + MCP error flag.
    pub async fn call_tool_raw(&self, tool_name: &str, args: Value) -> Result<ToolCallOutput> {
        let arguments = normalize_arguments(args)?;
        let result = self
            .client
            .request_tool_call(CallToolRequestParams {
                name: tool_name.to_string(),
                arguments: Some(arguments),
                meta: None,
                task: None,
            })
            .await
            .map_err(|error| anyhow!("tool call failed for {tool_name}: {error}"))?;

        let text = first_text_content(&result).unwrap_or_default();
        Ok(ToolCallOutput {
            tool_name: tool_name.to_string(),
            text,
            is_error: result.is_error.unwrap_or(false),
        })
    }

    /// Invoke one tool and parse JSON response into `T`.
    pub async fn call_tool_json<T: DeserializeOwned>(
        &self,
        tool_name: &str,
        args: Value,
    ) -> Result<T> {
        let output = self.call_tool_raw(tool_name, args).await?;
        if output.is_error {
            bail!(
                "tool returned error: {} ({})",
                output.tool_name,
                output.text
            );
        }

        serde_json::from_str(&output.text).map_err(|error| {
            anyhow!(
                "failed to parse JSON response from tool {}: {} ({error})",
                output.tool_name,
                output.text
            )
        })
    }

    /// Shutdown the underlying MCP client runtime.
    pub async fn shutdown(&self) -> Result<()> {
        self.client
            .shut_down()
            .await
            .map_err(|error| anyhow!("failed to shutdown MCP client: {error}"))
    }
}

fn normalize_arguments(args: Value) -> Result<Map<String, Value>> {
    match args {
        Value::Null => Ok(Map::new()),
        Value::Object(map) => Ok(map),
        _ => bail!("tool arguments must be a JSON object"),
    }
}

fn first_text_content(result: &CallToolResult) -> Option<String> {
    result.content.iter().find_map(|entry| {
        entry
            .as_text_content()
            .ok()
            .map(|content| content.text.clone())
    })
}

#[derive(Default)]
struct NoopClientHandler;

#[async_trait]
impl ClientHandler for NoopClientHandler {}
