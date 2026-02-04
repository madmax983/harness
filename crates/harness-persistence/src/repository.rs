//! Repository trait for persistence operations.

use crate::{
    Agent, AgentId, AgentStatus, Channel, ChannelId, Message, Session, SessionId,
};
use chrono::{DateTime, Utc};

/// Error type for repository operations.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// Entity not found.
    #[error("{entity_type} with id {id} not found")]
    NotFound { entity_type: String, id: String },

    /// Database error.
    #[error("database error: {0}")]
    Database(String),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Result type for repository operations.
pub type RepositoryResult<T> = Result<T, RepositoryError>;

/// Repository for Harness entities.
///
/// This trait abstracts over the storage backend (GallifreyDB),
/// enabling testing with in-memory implementations.
#[allow(async_fn_in_trait)]
pub trait Repository: Send + Sync {
    // === Session operations ===

    /// Create a new session.
    async fn create_session(&self, session: &Session) -> RepositoryResult<()>;

    /// Get a session by ID.
    async fn get_session(&self, id: SessionId) -> RepositoryResult<Session>;

    // === Agent operations ===

    /// Create a new agent.
    async fn create_agent(&self, agent: &Agent) -> RepositoryResult<()>;

    /// Get an agent by ID.
    async fn get_agent(&self, id: AgentId) -> RepositoryResult<Agent>;

    /// Update an agent's status.
    async fn update_agent_status(&self, id: AgentId, status: AgentStatus) -> RepositoryResult<()>;

    /// List all active agents in a session.
    async fn list_active_agents(&self, session_id: SessionId) -> RepositoryResult<Vec<Agent>>;

    /// Count active agents in a session.
    async fn count_active_agents(&self, session_id: SessionId) -> RepositoryResult<usize>;

    // === Channel operations ===

    /// Create a new channel.
    async fn create_channel(&self, channel: &Channel) -> RepositoryResult<()>;

    /// Get a channel by ID.
    async fn get_channel(&self, id: &ChannelId) -> RepositoryResult<Channel>;

    /// List all channels in a session.
    async fn list_channels(&self, session_id: SessionId) -> RepositoryResult<Vec<Channel>>;

    // === Message operations ===

    /// Create a new message.
    async fn create_message(&self, message: &Message) -> RepositoryResult<()>;

    /// Get messages in a channel.
    async fn get_messages(
        &self,
        channel_id: &ChannelId,
        limit: usize,
        since: Option<DateTime<Utc>>,
    ) -> RepositoryResult<Vec<Message>>;

    /// Search messages by semantic similarity.
    async fn search_messages_semantic(
        &self,
        query: &str,
        limit: usize,
    ) -> RepositoryResult<Vec<Message>>;

    // === Subscription operations ===

    /// Subscribe an agent to a channel.
    async fn subscribe_agent(&self, agent_id: AgentId, channel_id: &ChannelId)
        -> RepositoryResult<()>;

    /// Get agents subscribed to a channel.
    async fn get_channel_subscribers(&self, channel_id: &ChannelId) -> RepositoryResult<Vec<Agent>>;
}
