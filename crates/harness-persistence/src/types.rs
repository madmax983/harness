//! Core domain types for Harness.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(Uuid);

impl AgentId {
    /// Create a new random agent ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create an AgentId from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "agent-{}", &self.0.to_string()[..8])
    }
}

/// Unique identifier for a channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelId(String);

impl ChannelId {
    /// Create a channel ID from a name.
    /// Channel names must start with '#' and contain only alphanumeric/hyphen/underscore.
    pub fn new(name: &str) -> Result<Self, TypeError> {
        let name = if name.starts_with('#') {
            name.to_string()
        } else {
            format!("#{name}")
        };

        // Validate: only alphanumeric, hyphen, underscore after #
        let rest = &name[1..];
        if rest.is_empty() {
            return Err(TypeError::InvalidChannelName("channel name cannot be empty".into()));
        }
        if !rest.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(TypeError::InvalidChannelName(
                "channel name can only contain alphanumeric, hyphen, underscore".into(),
            ));
        }

        Ok(Self(name))
    }

    /// Get the channel name including the '#' prefix.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(Uuid);

impl MessageId {
    /// Create a new random message ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a MessageId from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(Uuid);

impl SessionId {
    /// Create a new random session ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a SessionId from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Error type for type validation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum TypeError {
    /// Invalid channel name.
    #[error("invalid channel name: {0}")]
    InvalidChannelName(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_display_is_short() {
        let id = AgentId::new();
        let display = id.to_string();
        assert!(display.starts_with("agent-"));
        assert_eq!(display.len(), 14); // "agent-" + 8 chars
    }

    #[test]
    fn channel_id_adds_hash_prefix() {
        let id = ChannelId::new("general").unwrap();
        assert_eq!(id.as_str(), "#general");
    }

    #[test]
    fn channel_id_accepts_hash_prefix() {
        let id = ChannelId::new("#general").unwrap();
        assert_eq!(id.as_str(), "#general");
    }

    #[test]
    fn channel_id_rejects_empty() {
        let result = ChannelId::new("");
        assert!(result.is_err());
    }

    #[test]
    fn channel_id_rejects_spaces() {
        let result = ChannelId::new("my channel");
        assert!(result.is_err());
    }

    #[test]
    fn channel_id_allows_hyphen_underscore() {
        let id = ChannelId::new("my-channel_1").unwrap();
        assert_eq!(id.as_str(), "#my-channel_1");
    }
}
