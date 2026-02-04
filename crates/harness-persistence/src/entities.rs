//! Domain entities for Harness.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{AgentId, ChannelId, MessageId, SessionId};

/// Status of an agent in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Spawn requested, process not yet started.
    Pending,
    /// Process started, waiting for first message.
    Starting,
    /// Agent is active and communicating.
    Active,
    /// Agent finished naturally.
    Finished,
    /// Agent was killed by user.
    Killed,
    /// Agent process crashed.
    Crashed,
}

/// An agent (Claude instance) in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Unique identifier.
    pub id: AgentId,
    /// Role name (e.g., "architect", "critic").
    pub role: String,
    /// System prompt used to initialize the agent.
    pub system_prompt: String,
    /// ID of the agent that spawned this one, if any.
    pub spawned_by: Option<AgentId>,
    /// Current status.
    pub status: AgentStatus,
    /// Channels this agent is subscribed to.
    pub subscriptions: Vec<ChannelId>,
    /// Session this agent belongs to.
    pub session_id: SessionId,
    /// When the agent was created.
    pub created_at: DateTime<Utc>,
}

impl Agent {
    /// Create a new agent with the given role and system prompt.
    pub fn new(
        role: impl Into<String>,
        system_prompt: impl Into<String>,
        session_id: SessionId,
    ) -> Self {
        Self {
            id: AgentId::new(),
            role: role.into(),
            system_prompt: system_prompt.into(),
            spawned_by: None,
            status: AgentStatus::Pending,
            subscriptions: Vec::new(),
            session_id,
            created_at: Utc::now(),
        }
    }

    /// Set who spawned this agent.
    pub fn with_spawned_by(mut self, spawner: AgentId) -> Self {
        self.spawned_by = Some(spawner);
        self
    }

    /// Subscribe to a channel.
    pub fn subscribe(&mut self, channel: ChannelId) {
        if !self.subscriptions.contains(&channel) {
            self.subscriptions.push(channel);
        }
    }
}

/// A communication channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    /// Channel identifier (includes # prefix).
    pub id: ChannelId,
    /// Human-readable description.
    pub description: String,
    /// When the channel was created.
    pub created_at: DateTime<Utc>,
    /// Session this channel belongs to.
    pub session_id: SessionId,
}

impl Channel {
    /// Create a new channel.
    pub fn new(id: ChannelId, description: impl Into<String>, session_id: SessionId) -> Self {
        Self {
            id,
            description: description.into(),
            created_at: Utc::now(),
            session_id,
        }
    }
}

/// A message in a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique identifier.
    pub id: MessageId,
    /// Channel this message was posted to.
    pub channel_id: ChannelId,
    /// Agent who posted this message.
    pub author_id: AgentId,
    /// Message content.
    pub content: String,
    /// Optional message this is replying to.
    pub reply_to: Option<MessageId>,
    /// When the message was posted.
    pub timestamp: DateTime<Utc>,
    /// Session this message belongs to.
    pub session_id: SessionId,
    /// Vector embedding for semantic search (populated by GallifreyDB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

impl Message {
    /// Create a new message.
    pub fn new(
        channel_id: ChannelId,
        author_id: AgentId,
        content: impl Into<String>,
        session_id: SessionId,
    ) -> Self {
        Self {
            id: MessageId::new(),
            channel_id,
            author_id,
            content: content.into(),
            reply_to: None,
            timestamp: Utc::now(),
            session_id,
            embedding: None,
        }
    }

    /// Set this message as a reply to another.
    pub fn with_reply_to(mut self, reply_to: MessageId) -> Self {
        self.reply_to = Some(reply_to);
        self
    }
}

/// A session grouping agents, channels, and messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique identifier.
    pub id: SessionId,
    /// When the session started.
    pub started_at: DateTime<Utc>,
    /// Population cap for this session.
    pub population_cap: usize,
}

impl Session {
    /// Create a new session with the given population cap.
    pub fn new(population_cap: usize) -> Self {
        Self {
            id: SessionId::new(),
            started_at: Utc::now(),
            population_cap,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_starts_pending() {
        let session = Session::new(8);
        let agent = Agent::new("architect", "You are an architect.", session.id);
        assert_eq!(agent.status, AgentStatus::Pending);
    }

    #[test]
    fn agent_subscribe_is_idempotent() {
        let session = Session::new(8);
        let mut agent = Agent::new("architect", "You are an architect.", session.id);
        let channel = ChannelId::new("general").unwrap();

        agent.subscribe(channel.clone());
        agent.subscribe(channel.clone());

        assert_eq!(agent.subscriptions.len(), 1);
    }

    #[test]
    fn message_reply_chain() {
        let session = Session::new(8);
        let channel = ChannelId::new("general").unwrap();
        let agent = AgentId::new();

        let msg1 = Message::new(channel.clone(), agent, "Hello", session.id);
        let msg2 = Message::new(channel, agent, "Reply", session.id).with_reply_to(msg1.id);

        assert!(msg1.reply_to.is_none());
        assert_eq!(msg2.reply_to, Some(msg1.id));
    }
}
