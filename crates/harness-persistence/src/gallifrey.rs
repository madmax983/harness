//! GallifreyDB repository implementation.
//!
//! # Status: BLOCKED - Waiting on GallifreyDB API improvements
//!
//! This implementation is ~80% complete but blocked on GallifreyDB API gaps:
//!
//! ## Required GallifreyDB features:
//! 1. **Property-based lookup**: `find_nodes_by_property(label, key, value) -> Vec<NodeId>`
//! 2. **Node scanning**: `scan_nodes_by_label(label) -> Iterator<NodeId>`
//! 3. **Flexible error types**: Allow closures to return custom error types
//!
//! ## Current workarounds (to be removed after GallifreyDB updates):
//! - Custom Index node (.harness-index file) for UUID -> NodeId mapping
//! - Error handling gymnastics inside transaction closures
//!
//! # Design
//!
//! This repository uses GallifreyDB for persistent storage with a single "Index" node
//! that maps domain IDs (SessionId, AgentId, etc.) to GallifreyDB NodeIds for efficient lookups.
//!
//! The Index node stores mappings as properties with keys like:
//! - "session:<uuid>" -> <node-id>
//! - "agent:<uuid>" -> <node-id>
//! - "channel:#name" -> <node-id>
//! - "message:<uuid>" -> <node-id>

use std::sync::Arc;

use chrono::{DateTime, Utc};
use gallifreydb::api::transaction::{ReadOps, WriteOps};
use gallifreydb::core::id::NodeId;
use gallifreydb::core::property::PropertyMapBuilder;
use gallifreydb::core::Node;
use gallifreydb::GallifreyDB;
use parking_lot::RwLock;

use crate::{
    Agent, AgentId, AgentStatus, Channel, ChannelId, Message, MessageId, Repository,
    RepositoryError, RepositoryResult, Session, SessionId,
};

/// Label constants for node types
const LABEL_SESSION: &str = "Session";
const LABEL_AGENT: &str = "Agent";
const LABEL_CHANNEL: &str = "Channel";
const LABEL_MESSAGE: &str = "Message";
const LABEL_INDEX: &str = "HarnessIndex";

/// Label constants for edge types
const LABEL_AGENT_SUBSCRIPTION: &str = "SUBSCRIBED_TO";
const LABEL_CONTAINS_AGENT: &str = "CONTAINS_AGENT";
const LABEL_CONTAINS_CHANNEL: &str = "CONTAINS_CHANNEL";
const LABEL_CONTAINS_MESSAGE: &str = "CONTAINS_MESSAGE";

/// Property key for the Index node ID (stored in Index node itself)
const INDEX_KEY_SELF: &str = "_index_node_id";

/// GallifreyDB-backed repository for production use.
pub struct GallifreyRepository {
    db: Arc<GallifreyDB>,
    /// Cached NodeId of the Index node (lazily initialized)
    index_node_id: RwLock<Option<NodeId>>,
}

impl GallifreyRepository {
    /// Create a new GallifreyRepository wrapping a GallifreyDB instance.
    pub fn new(db: Arc<GallifreyDB>) -> Self {
        Self {
            db,
            index_node_id: RwLock::new(None),
        }
    }

    /// Get or create the Index node.
    ///
    /// The Index node stores all domain ID -> NodeId mappings as properties.
    /// The Index NodeId is persisted in a `.harness-index` file to avoid scanning.
    fn get_or_create_index_node(&self) -> RepositoryResult<NodeId> {
        use std::fs;
        use std::path::Path;

        // Fast path: check cache
        {
            let cache = self.index_node_id.read();
            if let Some(node_id) = *cache {
                return Ok(node_id);
            }
        }

        // Slow path: read from file or create
        let index_file = Path::new(".harness-index");

        let node_id = if index_file.exists() {
            // Read existing Index NodeId from file
            let content = fs::read_to_string(index_file)
                .map_err(|e| RepositoryError::Database(format!("Failed to read index file: {}", e)))?;

            let id_u64: u64 = content.trim().parse()
                .map_err(|e| RepositoryError::Database(format!("Invalid index file content: {}", e)))?;

            NodeId::new(id_u64)
                .map_err(|e| RepositoryError::Database(format!("Invalid NodeId in index file: {}", e)))?
        } else {
            // Create new Index node
            let node_id = self.db.write(|tx| {
                let props = PropertyMapBuilder::new()
                    .insert(INDEX_KEY_SELF, "harness-index")
                    .build();

                tx.create_node(LABEL_INDEX, props)
            }).map_err(|e| RepositoryError::Database(e.to_string()))?;

            // Write NodeId to file
            fs::write(index_file, node_id.as_u64().to_string())
                .map_err(|e| RepositoryError::Database(format!("Failed to write index file: {}", e)))?;

            node_id
        };

        // Update cache
        {
            let mut cache = self.index_node_id.write();
            *cache = Some(node_id);
        }

        Ok(node_id)
    }

    /// Store a domain ID -> NodeId mapping in the Index.
    fn index_set(&self, key: &str, node_id: NodeId) -> RepositoryResult<()> {
        let index_id = self.get_or_create_index_node()?;

        self.db.write(|tx| {
            // Update index node with new mapping
            let props = PropertyMapBuilder::new()
                .insert(key, node_id.as_u64() as i64)
                .build();

            tx.update_node(index_id, props)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    /// Get a NodeId from the Index by domain ID.
    fn index_get(&self, key: &str) -> RepositoryResult<NodeId> {
        let index_id = self.get_or_create_index_node()?;

        let node = self.db.read(|tx| {
            tx.get_node(index_id)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        let node_id_i64 = node.get_property(key)
            .and_then(|v| v.as_int())
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "IndexEntry".into(),
                id: key.to_string(),
            })?;

        NodeId::new(node_id_i64 as u64)
            .map_err(|e| RepositoryError::Database(format!("Invalid NodeId: {}", e)))
    }

    /// Format a SessionId as an index key.
    fn session_index_key(id: SessionId) -> String {
        format!("session:{}", id.as_uuid())
    }

    /// Format an AgentId as an index key.
    fn agent_index_key(id: AgentId) -> String {
        format!("agent:{}", id.as_uuid())
    }

    /// Format a ChannelId as an index key.
    fn channel_index_key(id: &ChannelId) -> String {
        format!("channel:{}", id.as_str())
    }

    /// Format a MessageId as an index key.
    fn message_index_key(id: crate::MessageId) -> String {
        format!("message:{}", id.as_uuid())
    }

    /// Helper to convert chrono DateTime to GallifreyDB timestamp (u64 nanos).
    fn datetime_to_timestamp(dt: DateTime<Utc>) -> u64 {
        dt.timestamp_nanos_opt().unwrap_or(0) as u64
    }

    /// Helper to convert GallifreyDB timestamp to chrono DateTime.
    fn timestamp_to_datetime(ts: u64) -> DateTime<Utc> {
        DateTime::from_timestamp_nanos(ts as i64)
    }

    /// Helper to convert SessionId to string for storage.
    fn session_id_to_string(id: SessionId) -> String {
        id.as_uuid().to_string()
    }

    /// Helper to parse SessionId from string.
    fn string_to_session_id(s: &str) -> RepositoryResult<SessionId> {
        let uuid = uuid::Uuid::parse_str(s).map_err(|_| RepositoryError::Database(
            format!("Invalid SessionId UUID: {}", s)
        ))?;
        Ok(SessionId::from_uuid(uuid))
    }

    /// Helper to convert AgentId to string for storage.
    fn agent_id_to_string(id: AgentId) -> String {
        id.as_uuid().to_string()
    }

    /// Helper to parse AgentId from string.
    fn string_to_agent_id(s: &str) -> RepositoryResult<AgentId> {
        let uuid = uuid::Uuid::parse_str(s).map_err(|_| RepositoryError::Database(
            format!("Invalid AgentId UUID: {}", s)
        ))?;
        Ok(AgentId::from_uuid(uuid))
    }

    /// Helper to convert AgentStatus to string.
    fn status_to_string(status: AgentStatus) -> &'static str {
        match status {
            AgentStatus::Pending => "pending",
            AgentStatus::Starting => "starting",
            AgentStatus::Active => "active",
            AgentStatus::Finished => "finished",
            AgentStatus::Killed => "killed",
            AgentStatus::Crashed => "crashed",
        }
    }

    /// Helper to parse AgentStatus from string.
    fn string_to_status(s: &str) -> RepositoryResult<AgentStatus> {
        match s {
            "pending" => Ok(AgentStatus::Pending),
            "starting" => Ok(AgentStatus::Starting),
            "active" => Ok(AgentStatus::Active),
            "finished" => Ok(AgentStatus::Finished),
            "killed" => Ok(AgentStatus::Killed),
            "crashed" => Ok(AgentStatus::Crashed),
            _ => Err(RepositoryError::Database(format!("Invalid agent status: {}", s))),
        }
    }

    /// Helper to convert Node to Session.
    fn node_to_session(node: &Node) -> RepositoryResult<Session> {
        let id_str = node.get_property("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RepositoryError::Database("Session node missing 'id' property".into()))?;
        let id = Self::string_to_session_id(id_str)?;

        let population_cap = node.get_property("population_cap")
            .and_then(|v| v.as_int())
            .ok_or_else(|| RepositoryError::Database("Session node missing 'population_cap' property".into()))? as usize;

        let started_at_ts = node.get_property("started_at")
            .and_then(|v| v.as_int())
            .ok_or_else(|| RepositoryError::Database("Session node missing 'started_at' property".into()))? as u64;
        let started_at = Self::timestamp_to_datetime(started_at_ts);

        Ok(Session {
            id,
            population_cap,
            started_at,
        })
    }

    /// Helper to convert Node to Agent.
    fn node_to_agent(node: &Node) -> RepositoryResult<Agent> {
        let id_str = node.get_property("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RepositoryError::Database("Agent node missing 'id' property".into()))?;
        let id = Self::string_to_agent_id(id_str)?;

        let role = node.get_property("role")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RepositoryError::Database("Agent node missing 'role' property".into()))?
            .to_string();

        let system_prompt = node.get_property("system_prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RepositoryError::Database("Agent node missing 'system_prompt' property".into()))?
            .to_string();

        let status_str = node.get_property("status")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RepositoryError::Database("Agent node missing 'status' property".into()))?;
        let status = Self::string_to_status(status_str)?;

        let session_id_str = node.get_property("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RepositoryError::Database("Agent node missing 'session_id' property".into()))?;
        let session_id = Self::string_to_session_id(session_id_str)?;

        let spawned_by = node.get_property("spawned_by")
            .and_then(|v| v.as_str())
            .and_then(|s| Self::string_to_agent_id(s).ok());

        let created_at_ts = node.get_property("created_at")
            .and_then(|v| v.as_int())
            .ok_or_else(|| RepositoryError::Database("Agent node missing 'created_at' property".into()))? as u64;
        let created_at = Self::timestamp_to_datetime(created_at_ts);

        // Subscriptions will be loaded separately via edges
        Ok(Agent {
            id,
            role,
            system_prompt,
            spawned_by,
            status,
            subscriptions: Vec::new(), // Will be populated by caller if needed
            session_id,
            created_at,
        })
    }
}

impl Repository for GallifreyRepository {
    async fn create_session(&self, session: &Session) -> RepositoryResult<()> {
        let session_id_str = Self::session_id_to_string(session.id);
        let started_at_ts = Self::datetime_to_timestamp(session.started_at);

        // Create session node
        let node_id = self.db.write(|tx| {
            let props = PropertyMapBuilder::new()
                .insert("id", session_id_str.as_str())
                .insert("population_cap", session.population_cap as i64)
                .insert("started_at", started_at_ts as i64)
                .build();

            tx.create_node(LABEL_SESSION, props)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        // Add to index
        let index_key = Self::session_index_key(session.id);
        self.index_set(&index_key, node_id)?;

        Ok(())
    }

    async fn get_session(&self, id: SessionId) -> RepositoryResult<Session> {
        // Look up NodeId in index
        let index_key = Self::session_index_key(id);
        let node_id = self.index_get(&index_key)?;

        // Get node from database
        let node = self.db.read(|tx| {
            tx.get_node(node_id)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        // Convert to Session
        Self::node_to_session(&node)
    }

    async fn create_agent(&self, agent: &Agent) -> RepositoryResult<()> {
        let agent_id_str = Self::agent_id_to_string(agent.id);
        let session_id_str = Self::session_id_to_string(agent.session_id);
        let created_at_ts = Self::datetime_to_timestamp(agent.created_at);
        let status = Self::status_to_string(agent.status);

        // Look up session node
        let session_index_key = Self::session_index_key(agent.session_id);
        let session_node_id = self.index_get(&session_index_key)?;

        // Create agent node and edge from session
        let agent_node_id = self.db.write(|tx| {
            let mut props = PropertyMapBuilder::new()
                .insert("id", agent_id_str.as_str())
                .insert("role", agent.role.as_str())
                .insert("system_prompt", agent.system_prompt.as_str())
                .insert("status", status)
                .insert("session_id", session_id_str.as_str())
                .insert("created_at", created_at_ts as i64);

            if let Some(spawned_by) = agent.spawned_by {
                props = props.insert("spawned_by", Self::agent_id_to_string(spawned_by).as_str());
            }

            let agent_node_id = tx.create_node(LABEL_AGENT, props.build())?;

            // Create edge: Session --CONTAINS_AGENT--> Agent
            tx.create_edge(
                session_node_id,
                agent_node_id,
                LABEL_CONTAINS_AGENT,
                PropertyMapBuilder::new().build(),
            )?;

            Ok(agent_node_id)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        // Add to index
        let index_key = Self::agent_index_key(agent.id);
        self.index_set(&index_key, agent_node_id)?;

        Ok(())
    }

    async fn get_agent(&self, id: AgentId) -> RepositoryResult<Agent> {
        // Look up NodeId in index
        let index_key = Self::agent_index_key(id);
        let node_id = self.index_get(&index_key)?;

        // Get node from database
        let node = self.db.read(|tx| {
            tx.get_node(node_id)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        // Convert to Agent (subscriptions will be loaded separately)
        Self::node_to_agent(&node)
    }

    async fn update_agent_status(&self, id: AgentId, status: AgentStatus) -> RepositoryResult<()> {
        // Look up NodeId in index
        let index_key = Self::agent_index_key(id);
        let node_id = self.index_get(&index_key)?;

        // Update agent status
        let status_str = Self::status_to_string(status);
        self.db.write(|tx| {
            let props = PropertyMapBuilder::new()
                .insert("status", status_str)
                .build();

            tx.update_node(node_id, props)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn list_active_agents(&self, session_id: SessionId) -> RepositoryResult<Vec<Agent>> {
        // Look up session node
        let session_index_key = Self::session_index_key(session_id);
        let session_node_id = self.index_get(&session_index_key)?;

        // Get all agents in this session via graph traversal
        let agents = self.db.read(|tx| {
            let edge_ids = tx.get_outgoing_edges_with_label(session_node_id, LABEL_CONTAINS_AGENT);

            let mut agents = Vec::new();
            for edge_id in edge_ids {
                let edge = tx.get_edge(edge_id)?;
                let agent_node = tx.get_node(edge.target)?;

                // Check if agent is active (Starting or Active status)
                if let Some(status_str) = agent_node.get_property("status").and_then(|v| v.as_str()) {
                    if let Ok(status) = Self::string_to_status(status_str) {
                        if matches!(status, AgentStatus::Starting | AgentStatus::Active) {
                            if let Ok(agent) = Self::node_to_agent(&agent_node) {
                                agents.push(agent);
                            }
                        }
                    }
                }
            }

            Ok(agents)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(agents)
    }

    async fn count_active_agents(&self, session_id: SessionId) -> RepositoryResult<usize> {
        Ok(self.list_active_agents(session_id).await?.len())
    }

    async fn create_channel(&self, channel: &Channel) -> RepositoryResult<()> {
        let session_id_str = Self::session_id_to_string(channel.session_id);
        let created_at_ts = Self::datetime_to_timestamp(channel.created_at);

        // Look up session node
        let session_index_key = Self::session_index_key(channel.session_id);
        let session_node_id = self.index_get(&session_index_key)?;

        // Create channel node and edge from session
        let channel_node_id = self.db.write(|tx| {
            let props = PropertyMapBuilder::new()
                .insert("id", channel.id.as_str())
                .insert("description", channel.description.as_str())
                .insert("session_id", session_id_str.as_str())
                .insert("created_at", created_at_ts as i64)
                .build();

            let channel_node_id = tx.create_node(LABEL_CHANNEL, props)?;

            // Create edge: Session --CONTAINS_CHANNEL--> Channel
            tx.create_edge(
                session_node_id,
                channel_node_id,
                LABEL_CONTAINS_CHANNEL,
                PropertyMapBuilder::new().build(),
            )?;

            Ok(channel_node_id)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        // Add to index
        let index_key = Self::channel_index_key(&channel.id);
        self.index_set(&index_key, channel_node_id)?;

        Ok(())
    }

    async fn get_channel(&self, id: &ChannelId) -> RepositoryResult<Channel> {
        // Look up NodeId in index
        let index_key = Self::channel_index_key(id);
        let node_id = self.index_get(&index_key)?;

        // Get node from database
        let node = self.db.read(|tx| {
            tx.get_node(node_id)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        // Convert to Channel
        let id_str = node.get_property("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RepositoryError::Database("Channel node missing 'id' property".into()))?;
        let channel_id = ChannelId::new(id_str)
            .map_err(|e| RepositoryError::Database(e.to_string()))?;

        let description = node.get_property("description")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RepositoryError::Database("Channel node missing 'description' property".into()))?
            .to_string();

        let session_id_str = node.get_property("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RepositoryError::Database("Channel node missing 'session_id' property".into()))?;
        let session_id = Self::string_to_session_id(session_id_str)?;

        let created_at_ts = node.get_property("created_at")
            .and_then(|v| v.as_int())
            .ok_or_else(|| RepositoryError::Database("Channel node missing 'created_at' property".into()))? as u64;
        let created_at = Self::timestamp_to_datetime(created_at_ts);

        Ok(Channel {
            id: channel_id,
            description,
            session_id,
            created_at,
        })
    }

    async fn list_channels(&self, session_id: SessionId) -> RepositoryResult<Vec<Channel>> {
        // Look up session node
        let session_index_key = Self::session_index_key(session_id);
        let session_node_id = self.index_get(&session_index_key)?;

        // Get all channels in this session via graph traversal
        let channels = self.db.read(|tx| {
            let edge_ids = tx.get_outgoing_edges_with_label(session_node_id, LABEL_CONTAINS_CHANNEL);

            let mut channels = Vec::new();
            for edge_id in edge_ids {
                let edge = tx.get_edge(edge_id)?;
                let channel_node = tx.get_node(edge.target)?;

                let id_str = channel_node.get_property("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RepositoryError::Database("Channel node missing 'id' property".into()))?;
                let channel_id = ChannelId::new(id_str)
                    .map_err(|e| RepositoryError::Database(e.to_string()))?;

                let description = channel_node.get_property("description")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RepositoryError::Database("Channel node missing 'description' property".into()))?
                    .to_string();

                let session_id_str = channel_node.get_property("session_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RepositoryError::Database("Channel node missing 'session_id' property".into()))?;
                let channel_session_id = Self::string_to_session_id(session_id_str)?;

                let created_at_ts = channel_node.get_property("created_at")
                    .and_then(|v| v.as_int())
                    .ok_or_else(|| RepositoryError::Database("Channel node missing 'created_at' property".into()))? as u64;
                let created_at = Self::timestamp_to_datetime(created_at_ts);

                channels.push(Channel {
                    id: channel_id,
                    description,
                    session_id: channel_session_id,
                    created_at,
                });
            }

            Ok(channels)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(channels)
    }

    async fn create_message(&self, message: &Message) -> RepositoryResult<()> {
        let session_id_str = Self::session_id_to_string(message.session_id);
        let author_id_str = Self::agent_id_to_string(message.author_id);
        let timestamp_ts = Self::datetime_to_timestamp(message.timestamp);

        // Look up channel node
        let channel_index_key = Self::channel_index_key(&message.channel_id);
        let channel_node_id = self.index_get(&channel_index_key)?;

        // Create message node and edge from channel
        let message_node_id = self.db.write(|tx| {
            let mut props = PropertyMapBuilder::new()
                .insert("id", message.id.as_uuid().to_string().as_str())
                .insert("content", message.content.as_str())
                .insert("channel_id", message.channel_id.as_str())
                .insert("author_id", author_id_str.as_str())
                .insert("session_id", session_id_str.as_str())
                .insert("timestamp", timestamp_ts as i64);

            if let Some(reply_to) = message.reply_to {
                props = props.insert("reply_to", reply_to.as_uuid().to_string().as_str());
            }

            let message_node_id = tx.create_node(LABEL_MESSAGE, props.build())?;

            // Create edge: Channel --CONTAINS_MESSAGE--> Message
            tx.create_edge(
                channel_node_id,
                message_node_id,
                LABEL_CONTAINS_MESSAGE,
                PropertyMapBuilder::new().build(),
            )?;

            Ok(message_node_id)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        // Add to index
        let index_key = Self::message_index_key(message.id);
        self.index_set(&index_key, message_node_id)?;

        Ok(())
    }

    async fn get_messages(
        &self,
        channel_id: &ChannelId,
        limit: usize,
        since: Option<DateTime<Utc>>,
    ) -> RepositoryResult<Vec<Message>> {
        // Look up channel node
        let channel_index_key = Self::channel_index_key(channel_id);
        let channel_node_id = self.index_get(&channel_index_key)?;

        // Get messages in this channel via graph traversal
        let mut messages = self.db.read(|tx| {
            let edge_ids = tx.get_outgoing_edges_with_label(channel_node_id, LABEL_CONTAINS_MESSAGE);

            let mut messages = Vec::new();
            for edge_id in edge_ids {
                let edge = tx.get_edge(edge_id)?;
                let msg_node = tx.get_node(edge.target)?;

                // Parse message from node
                let id_str = msg_node.get_property("id").and_then(|v| v.as_str())
                    .ok_or_else(|| RepositoryError::Database("Message missing 'id'".into()))?;
                let id = MessageId::from_uuid(uuid::Uuid::parse_str(id_str)
                    .map_err(|_| RepositoryError::Database("Invalid message id".into()))?);

                let timestamp_ts = msg_node.get_property("timestamp").and_then(|v| v.as_int())
                    .ok_or_else(|| RepositoryError::Database("Message missing 'timestamp'".into()))? as u64;
                let timestamp = Self::timestamp_to_datetime(timestamp_ts);

                // Filter by since
                if let Some(since_time) = since {
                    if timestamp <= since_time {
                        continue;
                    }
                }

                let content = msg_node.get_property("content").and_then(|v| v.as_str())
                    .ok_or_else(|| RepositoryError::Database("Message missing 'content'".into()))?
                    .to_string();

                let channel_id_str = msg_node.get_property("channel_id").and_then(|v| v.as_str())
                    .ok_or_else(|| RepositoryError::Database("Message missing 'channel_id'".into()))?;
                let msg_channel_id = ChannelId::new(channel_id_str)
                    .map_err(|e| RepositoryError::Database(e.to_string()))?;

                let author_id_str = msg_node.get_property("author_id").and_then(|v| v.as_str())
                    .ok_or_else(|| RepositoryError::Database("Message missing 'author_id'".into()))?;
                let author_id = Self::string_to_agent_id(author_id_str)?;

                let session_id_str = msg_node.get_property("session_id").and_then(|v| v.as_str())
                    .ok_or_else(|| RepositoryError::Database("Message missing 'session_id'".into()))?;
                let session_id = Self::string_to_session_id(session_id_str)?;

                let reply_to = msg_node.get_property("reply_to")
                    .and_then(|v| v.as_str())
                    .and_then(|s| uuid::Uuid::parse_str(s).ok())
                    .map(MessageId::from_uuid);

                messages.push(Message {
                    id,
                    content,
                    channel_id: msg_channel_id,
                    author_id,
                    timestamp,
                    session_id,
                    reply_to,
                    embedding: None, // TODO: Load embeddings from GallifreyDB
                });
            }

            Ok(messages)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        // Sort by timestamp and apply limit
        messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        messages.truncate(limit);

        Ok(messages)
    }

    async fn search_messages_semantic(
        &self,
        _query: &str,
        _limit: usize,
    ) -> RepositoryResult<Vec<Message>> {
        // TODO: Implement with GallifreyDB vector embeddings
        // For now, return empty - semantic search requires embedding generation
        Ok(Vec::new())
    }

    async fn subscribe_agent(
        &self,
        agent_id: AgentId,
        channel_id: &ChannelId,
    ) -> RepositoryResult<()> {
        // Look up agent and channel nodes
        let agent_index_key = Self::agent_index_key(agent_id);
        let agent_node_id = self.index_get(&agent_index_key)?;

        let channel_index_key = Self::channel_index_key(channel_id);
        let channel_node_id = self.index_get(&channel_index_key)?;

        // Create subscription edge
        self.db.write(|tx| {
            tx.create_edge(
                agent_node_id,
                channel_node_id,
                LABEL_AGENT_SUBSCRIPTION,
                PropertyMapBuilder::new().build(),
            )?;
            Ok(())
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(())
    }

    async fn get_channel_subscribers(&self, channel_id: &ChannelId) -> RepositoryResult<Vec<Agent>> {
        // Look up channel node
        let channel_index_key = Self::channel_index_key(channel_id);
        let channel_node_id = self.index_get(&channel_index_key)?;

        // Get all agents subscribed to this channel via graph traversal
        let agents = self.db.read(|tx| {
            let edge_ids = tx.get_incoming_edges(channel_node_id);

            let mut agents = Vec::new();
            for edge_id in edge_ids {
                let edge = tx.get_edge(edge_id)?;

                // Only process SUBSCRIBED_TO edges
                if !edge.has_label_str(LABEL_AGENT_SUBSCRIPTION) {
                    continue;
                }

                let agent_node = tx.get_node(edge.source)?;

                // Convert node to agent
                if let Ok(mut agent) = Self::node_to_agent(&agent_node) {
                    // Load subscriptions for this agent (we know at least this channel)
                    agent.subscriptions.push(channel_id.clone());
                    agents.push(agent);
                }
            }

            Ok(agents)
        }).map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(agents)
    }
}
