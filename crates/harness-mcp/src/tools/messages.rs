//! Direct message tool request/response types.

use serde::{Deserialize, Serialize};

/// Request to send a direct message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SendDirectMessageRequest {
    pub to_agent: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from send_direct_message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SendDirectMessageResponse {
    pub message_id: String,
}

/// Request to get messages for the current agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetMessagesRequest {
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

fn default_limit() -> usize {
    20
}

/// Message info in response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageInfo {
    pub id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub content: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

/// Response from get_messages.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetMessagesResponse {
    pub messages: Vec<MessageInfo>,
}

/// Request to get thread messages.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetThreadMessagesRequest {
    pub task_id: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Optional agent ID for multi-client support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _agent_id: Option<String>,
}

/// Response from get_thread_messages.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetThreadMessagesResponse {
    pub messages: Vec<MessageInfo>,
}
