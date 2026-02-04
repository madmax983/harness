//! MCP tool definitions for Harness chat.

use serde::{Deserialize, Serialize};

// === Standard Tools ===

/// Request to send a message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SendMessageRequest {
    /// Channel to send to (e.g., "#general").
    pub channel: String,
    /// Message content.
    pub content: String,
    /// Optional message ID to reply to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
}

/// Response from send_message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SendMessageResponse {
    /// ID of the created message.
    pub message_id: String,
    /// Timestamp of the message.
    pub timestamp: String,
}

/// Request to read messages.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReadMessagesRequest {
    /// Channel to read from.
    pub channel: String,
    /// Maximum number of messages.
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Optional semantic query for filtering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_query: Option<String>,
}

fn default_limit() -> usize {
    20
}

/// A message in read_messages response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageInfo {
    /// Message ID.
    pub id: String,
    /// Author's agent ID.
    pub author: String,
    /// Author's role.
    pub role: String,
    /// Message content.
    pub content: String,
    /// Timestamp.
    pub timestamp: String,
    /// ID of message this replies to, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
}

/// Response from read_messages.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReadMessagesResponse {
    /// List of messages.
    pub messages: Vec<MessageInfo>,
}

/// Request to list channels.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListChannelsRequest {}

/// A channel in list_channels response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChannelInfo {
    /// Channel name (with # prefix).
    pub name: String,
    /// Channel description.
    pub description: String,
}

/// Response from list_channels.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListChannelsResponse {
    /// List of channels.
    pub channels: Vec<ChannelInfo>,
}

/// Request to create a channel.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateChannelRequest {
    /// Channel name (# prefix optional).
    pub name: String,
    /// Channel description.
    pub description: String,
}

/// Response from create_channel.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateChannelResponse {
    /// The created channel's name.
    pub name: String,
}

/// Request for whoami.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WhoamiRequest {}

/// Response from whoami.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WhoamiResponse {
    /// Agent ID.
    pub id: String,
    /// Agent role.
    pub role: String,
    /// Channels the agent is subscribed to.
    pub subscriptions: Vec<String>,
}

/// Request to list agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListAgentsRequest {
    /// Optional channel to filter by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

/// An agent in list_agents response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentInfo {
    /// Agent ID.
    pub id: String,
    /// Agent role.
    pub role: String,
    /// Agent status.
    pub status: String,
}

/// Response from list_agents.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListAgentsResponse {
    /// List of agents.
    pub agents: Vec<AgentInfo>,
}

/// Request to spawn an agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestSpawnRequest {
    /// Role for the new agent.
    pub role: String,
    /// Reason for spawning.
    pub reason: String,
    /// Channel to subscribe the agent to.
    pub channel: String,
}

/// Response from request_spawn.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestSpawnResponse {
    /// Status of the request.
    pub status: SpawnStatus,
    /// Agent ID if spawned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Queue position if queued.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_position: Option<usize>,
}

/// Status of a spawn request.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnStatus {
    /// Agent was spawned.
    Spawned,
    /// Request is queued.
    Queued,
    /// Request was denied.
    Denied,
}

// === God-Mode Tools ===

/// Request to spawn an agent (god-mode).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnAgentRequest {
    /// Role for the new agent.
    pub role: String,
    /// Optional custom system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Channels to subscribe the agent to.
    pub channels: Vec<String>,
}

/// Response from spawn_agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpawnAgentResponse {
    /// The spawned agent's ID.
    pub agent_id: String,
}

/// Request to kill an agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KillAgentRequest {
    /// ID of the agent to kill.
    pub agent_id: String,
}

/// Response from kill_agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KillAgentResponse {
    /// Whether the kill succeeded.
    pub success: bool,
}

/// Request to broadcast a message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BroadcastRequest {
    /// Message content.
    pub content: String,
}

/// Response from broadcast.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BroadcastResponse {
    /// Number of channels the message was sent to.
    pub channels_sent: usize,
}

/// Request to clear a channel.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClearChannelRequest {
    /// Channel to clear.
    pub channel: String,
}

/// Response from clear_channel.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClearChannelResponse {
    /// Whether the clear succeeded.
    pub success: bool,
}

/// Request to set population cap.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetPopulationCapRequest {
    /// New population cap.
    pub max: usize,
}

/// Response from set_population_cap.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetPopulationCapResponse {
    /// The new cap value.
    pub new_cap: usize,
}
