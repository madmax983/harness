# Harness Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a multi-Claude orchestration system with MCP chat server, AletheiaDB persistence, and Ratatui TUI.

**Architecture:** Workspace with 4 crates (harness-mcp, harness-orchestrator, harness-persistence, harness-tui) plus a binary that wires them together. MCP server exposes chat tools, orchestrator manages Claude processes, persistence layer talks to AletheiaDB, TUI provides god-mode interface.

**Tech Stack:** rust-mcp-sdk, aletheiadb (local path dependency), ratatui + crossterm, tokio

---

## Phase 1: Workspace Scaffolding

### Task 1.1: Convert to Workspace

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/harness-mcp/Cargo.toml`
- Create: `crates/harness-mcp/src/lib.rs`
- Create: `crates/harness-orchestrator/Cargo.toml`
- Create: `crates/harness-orchestrator/src/lib.rs`
- Create: `crates/harness-persistence/Cargo.toml`
- Create: `crates/harness-persistence/src/lib.rs`
- Create: `crates/harness-tui/Cargo.toml`
- Create: `crates/harness-tui/src/lib.rs`

**Step 1: Create crate directories**

```bash
mkdir -p crates/harness-mcp/src
mkdir -p crates/harness-orchestrator/src
mkdir -p crates/harness-persistence/src
mkdir -p crates/harness-tui/src
```

**Step 2: Write workspace root Cargo.toml**

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2024"
authors = ["Mark Manning"]
license = "MIT"

[workspace.dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# MCP server
rust-mcp-sdk = { version = "0.8", features = ["server", "macros"] }
rust-mcp-schema = "0.8"

# Persistence (local path)
aletheiadb = { path = "../aletheiadb", features = ["mcp-server", "embeddings"] }

# TUI
ratatui = "0.29"
crossterm = "0.28"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Error handling
thiserror = "2"
anyhow = "1"

# Utilities
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[package]
name = "harness"
version.workspace = true
edition.workspace = true

[dependencies]
harness-mcp = { path = "crates/harness-mcp" }
harness-orchestrator = { path = "crates/harness-orchestrator" }
harness-persistence = { path = "crates/harness-persistence" }
harness-tui = { path = "crates/harness-tui" }
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
anyhow.workspace = true
```

**Step 3: Write harness-persistence Cargo.toml**

```toml
[package]
name = "harness-persistence"
version.workspace = true
edition.workspace = true

[dependencies]
aletheiadb.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
uuid.workspace = true
chrono.workspace = true
tracing.workspace = true
```

**Step 4: Write harness-orchestrator Cargo.toml**

```toml
[package]
name = "harness-orchestrator"
version.workspace = true
edition.workspace = true

[dependencies]
harness-persistence = { path = "../harness-persistence" }
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
uuid.workspace = true
tracing.workspace = true
```

**Step 5: Write harness-mcp Cargo.toml**

```toml
[package]
name = "harness-mcp"
version.workspace = true
edition.workspace = true

[dependencies]
harness-persistence = { path = "../harness-persistence" }
harness-orchestrator = { path = "../harness-orchestrator" }
rust-mcp-sdk.workspace = true
rust-mcp-schema.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
```

**Step 6: Write harness-tui Cargo.toml**

```toml
[package]
name = "harness-tui"
version.workspace = true
edition.workspace = true

[dependencies]
harness-persistence = { path = "../harness-persistence" }
harness-orchestrator = { path = "../harness-orchestrator" }
harness-mcp = { path = "../harness-mcp" }
ratatui.workspace = true
crossterm.workspace = true
tokio.workspace = true
serde.workspace = true
thiserror.workspace = true
tracing.workspace = true
```

**Step 7: Write stub lib.rs for each crate**

For `crates/harness-persistence/src/lib.rs`:
```rust
//! Persistence layer for Harness using AletheiaDB.

pub fn placeholder() {}
```

For `crates/harness-orchestrator/src/lib.rs`:
```rust
//! Orchestrator for managing Claude processes.

pub fn placeholder() {}
```

For `crates/harness-mcp/src/lib.rs`:
```rust
//! MCP chat server for Harness.

pub fn placeholder() {}
```

For `crates/harness-tui/src/lib.rs`:
```rust
//! TUI interface for Harness.

pub fn placeholder() {}
```

**Step 8: Update src/main.rs**

```rust
use anyhow::Result;

fn main() -> Result<()> {
    println!("Harness - Multi-Claude Orchestration");
    Ok(())
}
```

**Step 9: Verify workspace compiles**

Run: `cargo build`
Expected: Compiles successfully with all crates

**Step 10: Commit**

```bash
git add -A
git commit -m "feat: scaffold workspace with 4 crates

- harness-persistence: GallifreyDB integration
- harness-orchestrator: Claude process management
- harness-mcp: MCP chat server
- harness-tui: Ratatui interface

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Phase 2: Core Types (harness-persistence)

### Task 2.1: Define Domain Types

**Files:**
- Create: `crates/harness-persistence/src/types.rs`
- Modify: `crates/harness-persistence/src/lib.rs`

**Step 1: Write failing test for AgentId**

Create `crates/harness-persistence/src/types.rs`:
```rust
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
}
```

**Step 2: Run test to verify it passes**

Run: `cargo test -p harness-persistence agent_id`
Expected: PASS

**Step 3: Add remaining ID types**

Append to `crates/harness-persistence/src/types.rs`:
```rust
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

/// Error type for type validation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum TypeError {
    /// Invalid channel name.
    #[error("invalid channel name: {0}")]
    InvalidChannelName(String),
}
```

**Step 4: Add tests for ChannelId validation**

Append to tests module:
```rust
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
```

**Step 5: Run tests**

Run: `cargo test -p harness-persistence`
Expected: All tests PASS

**Step 6: Update lib.rs to export types**

Replace `crates/harness-persistence/src/lib.rs`:
```rust
//! Persistence layer for Harness using AletheiaDB.

mod types;

pub use types::{AgentId, ChannelId, MessageId, SessionId, TypeError};
```

**Step 7: Verify it compiles**

Run: `cargo build -p harness-persistence`
Expected: Compiles successfully

**Step 8: Commit**

```bash
git add -A
git commit -m "feat(persistence): add core ID types

- AgentId, MessageId, SessionId with UUID backing
- ChannelId with validation (alphanumeric, hyphen, underscore)
- Short display format for AgentId (agent-xxxxxxxx)

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 2.2: Define Entity Types

**Files:**
- Create: `crates/harness-persistence/src/entities.rs`
- Modify: `crates/harness-persistence/src/lib.rs`

**Step 1: Write Agent entity**

Create `crates/harness-persistence/src/entities.rs`:
```rust
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
        let msg2 = Message::new(channel, agent, "Reply").with_reply_to(msg1.id);

        assert!(msg1.reply_to.is_none());
        assert_eq!(msg2.reply_to, Some(msg1.id));
    }
}
```

**Step 2: Run tests**

Run: `cargo test -p harness-persistence`
Expected: All tests PASS

**Step 3: Update lib.rs**

Replace `crates/harness-persistence/src/lib.rs`:
```rust
//! Persistence layer for Harness using AletheiaDB.

mod entities;
mod types;

pub use entities::{Agent, AgentStatus, Channel, Message, Session};
pub use types::{AgentId, ChannelId, MessageId, SessionId, TypeError};
```

**Step 4: Verify compilation**

Run: `cargo build -p harness-persistence`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add -A
git commit -m "feat(persistence): add domain entities

- Agent with lifecycle status and subscriptions
- Channel with description
- Message with reply_to for threading
- Session with population cap

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 2.3: AletheiaDB Repository Trait

**Files:**
- Create: `crates/harness-persistence/src/repository.rs`
- Modify: `crates/harness-persistence/src/lib.rs`

**Step 1: Define the Repository trait**

Create `crates/harness-persistence/src/repository.rs`:
```rust
//! Repository trait for persistence operations.

use crate::{
    Agent, AgentId, AgentStatus, Channel, ChannelId, Message, MessageId, Session, SessionId,
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
/// This trait abstracts over the storage backend (AletheiaDB),
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
```

**Step 2: Add to lib.rs**

Update `crates/harness-persistence/src/lib.rs`:
```rust
//! Persistence layer for Harness using AletheiaDB.

mod entities;
mod repository;
mod types;

pub use entities::{Agent, AgentStatus, Channel, Message, Session};
pub use repository::{Repository, RepositoryError, RepositoryResult};
pub use types::{AgentId, ChannelId, MessageId, SessionId, TypeError};
```

**Step 3: Verify compilation**

Run: `cargo build -p harness-persistence`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add -A
git commit -m "feat(persistence): add Repository trait

Defines async interface for all persistence operations:
- Session, Agent, Channel, Message CRUD
- Semantic message search
- Agent subscriptions

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 2.4: In-Memory Repository for Testing

**Files:**
- Create: `crates/harness-persistence/src/memory.rs`
- Modify: `crates/harness-persistence/src/lib.rs`

**Step 1: Write in-memory implementation**

Create `crates/harness-persistence/src/memory.rs`:
```rust
//! In-memory repository implementation for testing.

use std::collections::HashMap;
use std::sync::RwLock;

use chrono::{DateTime, Utc};

use crate::{
    Agent, AgentId, AgentStatus, Channel, ChannelId, Message, MessageId, Repository,
    RepositoryError, RepositoryResult, Session, SessionId,
};

/// In-memory repository for testing.
#[derive(Debug, Default)]
pub struct InMemoryRepository {
    sessions: RwLock<HashMap<SessionId, Session>>,
    agents: RwLock<HashMap<AgentId, Agent>>,
    channels: RwLock<HashMap<ChannelId, Channel>>,
    messages: RwLock<Vec<Message>>,
}

impl InMemoryRepository {
    /// Create a new empty repository.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Repository for InMemoryRepository {
    async fn create_session(&self, session: &Session) -> RepositoryResult<()> {
        self.sessions
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .insert(session.id, session.clone());
        Ok(())
    }

    async fn get_session(&self, id: SessionId) -> RepositoryResult<Session> {
        self.sessions
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .get(&id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "Session".into(),
                id: id.as_uuid().to_string(),
            })
    }

    async fn create_agent(&self, agent: &Agent) -> RepositoryResult<()> {
        self.agents
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .insert(agent.id, agent.clone());
        Ok(())
    }

    async fn get_agent(&self, id: AgentId) -> RepositoryResult<Agent> {
        self.agents
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .get(&id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "Agent".into(),
                id: id.to_string(),
            })
    }

    async fn update_agent_status(&self, id: AgentId, status: AgentStatus) -> RepositoryResult<()> {
        let mut agents = self
            .agents
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let agent = agents.get_mut(&id).ok_or_else(|| RepositoryError::NotFound {
            entity_type: "Agent".into(),
            id: id.to_string(),
        })?;
        agent.status = status;
        Ok(())
    }

    async fn list_active_agents(&self, session_id: SessionId) -> RepositoryResult<Vec<Agent>> {
        let agents = self
            .agents
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(agents
            .values()
            .filter(|a| {
                a.session_id == session_id
                    && matches!(
                        a.status,
                        AgentStatus::Pending | AgentStatus::Starting | AgentStatus::Active
                    )
            })
            .cloned()
            .collect())
    }

    async fn count_active_agents(&self, session_id: SessionId) -> RepositoryResult<usize> {
        Ok(self.list_active_agents(session_id).await?.len())
    }

    async fn create_channel(&self, channel: &Channel) -> RepositoryResult<()> {
        self.channels
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .insert(channel.id.clone(), channel.clone());
        Ok(())
    }

    async fn get_channel(&self, id: &ChannelId) -> RepositoryResult<Channel> {
        self.channels
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .get(id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "Channel".into(),
                id: id.to_string(),
            })
    }

    async fn list_channels(&self, session_id: SessionId) -> RepositoryResult<Vec<Channel>> {
        let channels = self
            .channels
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(channels
            .values()
            .filter(|c| c.session_id == session_id)
            .cloned()
            .collect())
    }

    async fn create_message(&self, message: &Message) -> RepositoryResult<()> {
        self.messages
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?
            .push(message.clone());
        Ok(())
    }

    async fn get_messages(
        &self,
        channel_id: &ChannelId,
        limit: usize,
        since: Option<DateTime<Utc>>,
    ) -> RepositoryResult<Vec<Message>> {
        let messages = self
            .messages
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let mut result: Vec<_> = messages
            .iter()
            .filter(|m| {
                &m.channel_id == channel_id && since.map_or(true, |s| m.timestamp > s)
            })
            .cloned()
            .collect();
        result.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        result.truncate(limit);
        Ok(result)
    }

    async fn search_messages_semantic(
        &self,
        _query: &str,
        limit: usize,
    ) -> RepositoryResult<Vec<Message>> {
        // In-memory impl doesn't support semantic search, return recent messages
        let messages = self
            .messages
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let mut result: Vec<_> = messages.iter().cloned().collect();
        result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        result.truncate(limit);
        Ok(result)
    }

    async fn subscribe_agent(
        &self,
        agent_id: AgentId,
        channel_id: &ChannelId,
    ) -> RepositoryResult<()> {
        let mut agents = self
            .agents
            .write()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        let agent = agents.get_mut(&agent_id).ok_or_else(|| RepositoryError::NotFound {
            entity_type: "Agent".into(),
            id: agent_id.to_string(),
        })?;
        agent.subscribe(channel_id.clone());
        Ok(())
    }

    async fn get_channel_subscribers(&self, channel_id: &ChannelId) -> RepositoryResult<Vec<Agent>> {
        let agents = self
            .agents
            .read()
            .map_err(|e| RepositoryError::Database(e.to_string()))?;
        Ok(agents
            .values()
            .filter(|a| a.subscriptions.contains(channel_id))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_crud() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);

        repo.create_session(&session).await.unwrap();
        let fetched = repo.get_session(session.id).await.unwrap();

        assert_eq!(fetched.id, session.id);
        assert_eq!(fetched.population_cap, 8);
    }

    #[tokio::test]
    async fn test_agent_lifecycle() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let agent = Agent::new("architect", "You are an architect.", session.id);
        let agent_id = agent.id;
        repo.create_agent(&agent).await.unwrap();

        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 1);

        repo.update_agent_status(agent_id, AgentStatus::Killed)
            .await
            .unwrap();

        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_message_ordering() {
        let repo = InMemoryRepository::new();
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let channel = ChannelId::new("general").unwrap();
        repo.create_channel(&Channel::new(channel.clone(), "General chat", session.id))
            .await
            .unwrap();

        let agent = AgentId::new();

        // Create messages with slight delays to ensure ordering
        for i in 0..5 {
            let msg = Message::new(channel.clone(), agent, format!("Message {i}"), session.id);
            repo.create_message(&msg).await.unwrap();
        }

        let messages = repo.get_messages(&channel, 3, None).await.unwrap();
        assert_eq!(messages.len(), 3);
        assert!(messages[0].content.contains('0'));
        assert!(messages[1].content.contains('1'));
        assert!(messages[2].content.contains('2'));
    }
}
```

**Step 2: Run tests**

Run: `cargo test -p harness-persistence`
Expected: All tests PASS

**Step 3: Update lib.rs**

```rust
//! Persistence layer for Harness using AletheiaDB.

mod entities;
mod memory;
mod repository;
mod types;

pub use entities::{Agent, AgentStatus, Channel, Message, Session};
pub use memory::InMemoryRepository;
pub use repository::{Repository, RepositoryError, RepositoryResult};
pub use types::{AgentId, ChannelId, MessageId, SessionId, TypeError};
```

**Step 4: Verify compilation**

Run: `cargo build -p harness-persistence`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add -A
git commit -m "feat(persistence): add InMemoryRepository

Implements Repository trait for testing without AletheiaDB.
Includes tests for session, agent lifecycle, message ordering.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Phase 3: Orchestrator (harness-orchestrator)

### Task 3.1: Agent Spawn Configuration

**Files:**
- Create: `crates/harness-orchestrator/src/config.rs`
- Modify: `crates/harness-orchestrator/src/lib.rs`

**Step 1: Write config types**

Create `crates/harness-orchestrator/src/config.rs`:
```rust
//! Configuration for the orchestrator.

use serde::{Deserialize, Serialize};

/// Configuration for spawning agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    /// Maximum number of concurrent agents.
    pub population_cap: usize,
    /// Path to the claude CLI executable.
    pub claude_path: String,
    /// MCP server configuration to pass to agents.
    pub mcp_config: McpServerConfig,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            population_cap: 8,
            claude_path: "claude".into(),
            mcp_config: McpServerConfig::default(),
        }
    }
}

/// MCP server configuration passed to Claude agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Server name (used in MCP config JSON).
    pub name: String,
    /// Command to run the MCP server.
    pub command: String,
    /// Arguments for the command.
    pub args: Vec<String>,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            name: "harness".into(),
            command: "harness".into(),
            args: vec!["mcp-server".into()],
        }
    }
}

impl McpServerConfig {
    /// Generate the JSON config string for --mcp-config.
    pub fn to_json(&self) -> String {
        serde_json::json!({
            &self.name: {
                "command": &self.command,
                "args": &self.args
            }
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_config_json_format() {
        let config = McpServerConfig {
            name: "harness".into(),
            command: "harness-mcp".into(),
            args: vec!["--session".into(), "abc123".into()],
        };
        let json = config.to_json();
        assert!(json.contains("\"harness\""));
        assert!(json.contains("\"command\""));
        assert!(json.contains("harness-mcp"));
    }
}
```

**Step 2: Run test**

Run: `cargo test -p harness-orchestrator`
Expected: PASS

**Step 3: Update lib.rs**

```rust
//! Orchestrator for managing Claude processes.

mod config;

pub use config::{McpServerConfig, OrchestratorConfig};
```

**Step 4: Commit**

```bash
git add -A
git commit -m "feat(orchestrator): add configuration types

- OrchestratorConfig with population cap, claude path
- McpServerConfig generates --mcp-config JSON

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 3.2: Process Manager

**Files:**
- Create: `crates/harness-orchestrator/src/process.rs`
- Modify: `crates/harness-orchestrator/src/lib.rs`

**Step 1: Write process manager**

Create `crates/harness-orchestrator/src/process.rs`:
```rust
//! Process management for Claude agents.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;

use harness_persistence::{AgentId, AgentStatus, Repository};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;

use crate::OrchestratorConfig;

/// Error type for process operations.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    /// Failed to spawn process.
    #[error("failed to spawn process: {0}")]
    SpawnFailed(String),

    /// Process not found.
    #[error("process for agent {0} not found")]
    NotFound(AgentId),

    /// Population cap reached.
    #[error("population cap ({cap}) reached, cannot spawn more agents")]
    CapReached { cap: usize },

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Repository error.
    #[error("repository error: {0}")]
    Repository(String),
}

/// Result type for process operations.
pub type ProcessResult<T> = Result<T, ProcessError>;

/// Manages Claude processes.
pub struct ProcessManager<R: Repository> {
    config: OrchestratorConfig,
    repository: Arc<R>,
    processes: RwLock<HashMap<AgentId, Child>>,
}

impl<R: Repository> ProcessManager<R> {
    /// Create a new process manager.
    pub fn new(config: OrchestratorConfig, repository: Arc<R>) -> Self {
        Self {
            config,
            repository,
            processes: RwLock::new(HashMap::new()),
        }
    }

    /// Spawn a new Claude process for an agent.
    pub async fn spawn(&self, agent_id: AgentId, prompt: &str) -> ProcessResult<()> {
        // Check population cap
        let agent = self
            .repository
            .get_agent(agent_id)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        let active_count = self
            .repository
            .count_active_agents(agent.session_id)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        if active_count >= self.config.population_cap {
            return Err(ProcessError::CapReached {
                cap: self.config.population_cap,
            });
        }

        // Build command
        let mcp_json = self.config.mcp_config.to_json();

        let child = Command::new(&self.config.claude_path)
            .arg("-p")
            .arg(prompt)
            .arg("--mcp-config")
            .arg(&mcp_json)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ProcessError::SpawnFailed(e.to_string()))?;

        // Update status to Starting
        self.repository
            .update_agent_status(agent_id, AgentStatus::Starting)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        // Store process handle
        self.processes.write().await.insert(agent_id, child);

        Ok(())
    }

    /// Kill an agent's process.
    pub async fn kill(&self, agent_id: AgentId) -> ProcessResult<()> {
        let mut processes = self.processes.write().await;
        let child = processes
            .get_mut(&agent_id)
            .ok_or(ProcessError::NotFound(agent_id))?;

        child.kill().await?;
        processes.remove(&agent_id);

        // Update status to Killed
        self.repository
            .update_agent_status(agent_id, AgentStatus::Killed)
            .await
            .map_err(|e| ProcessError::Repository(e.to_string()))?;

        Ok(())
    }

    /// Check if a process is still running.
    pub async fn is_running(&self, agent_id: AgentId) -> bool {
        let processes = self.processes.read().await;
        if let Some(child) = processes.get(&agent_id) {
            // try_wait returns Ok(None) if still running
            matches!(child.id(), Some(_))
        } else {
            false
        }
    }

    /// Get the number of running processes.
    pub async fn running_count(&self) -> usize {
        self.processes.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_persistence::{Agent, InMemoryRepository, Session};

    #[tokio::test]
    async fn test_cap_enforcement() {
        let repo = Arc::new(InMemoryRepository::new());
        let config = OrchestratorConfig {
            population_cap: 1,
            claude_path: "echo".into(), // Use echo for testing
            ..Default::default()
        };

        let manager = ProcessManager::new(config, repo.clone());

        // Create session and agents
        let session = Session::new(1);
        repo.create_session(&session).await.unwrap();

        let agent1 = Agent::new("agent1", "test", session.id);
        let agent2 = Agent::new("agent2", "test", session.id);
        repo.create_agent(&agent1).await.unwrap();
        repo.create_agent(&agent2).await.unwrap();

        // First spawn should succeed
        manager.spawn(agent1.id, "test prompt").await.unwrap();

        // Second spawn should fail due to cap
        let result = manager.spawn(agent2.id, "test prompt").await;
        assert!(matches!(result, Err(ProcessError::CapReached { cap: 1 })));
    }
}
```

**Step 2: Run test**

Run: `cargo test -p harness-orchestrator`
Expected: PASS

**Step 3: Update lib.rs**

```rust
//! Orchestrator for managing Claude processes.

mod config;
mod process;

pub use config::{McpServerConfig, OrchestratorConfig};
pub use process::{ProcessError, ProcessManager, ProcessResult};
```

**Step 4: Commit**

```bash
git add -A
git commit -m "feat(orchestrator): add ProcessManager

Spawns claude -p processes with MCP config injection.
Enforces population cap, tracks process handles, supports kill.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Phase 4: MCP Server (harness-mcp)

### Task 4.1: Chat State Manager

**Files:**
- Create: `crates/harness-mcp/src/state.rs`
- Modify: `crates/harness-mcp/src/lib.rs`

**Step 1: Write state manager**

Create `crates/harness-mcp/src/state.rs`:
```rust
//! Shared state for the MCP server.

use std::sync::Arc;

use harness_orchestrator::{OrchestratorConfig, ProcessManager};
use harness_persistence::{
    Agent, AgentId, AgentStatus, Channel, ChannelId, Message, Repository, RepositoryResult,
    Session, SessionId,
};

/// Shared state for MCP tool handlers.
pub struct ChatState<R: Repository> {
    /// Current session.
    session: Session,
    /// Repository for persistence.
    repository: Arc<R>,
    /// Process manager for spawning agents.
    process_manager: Arc<ProcessManager<R>>,
}

impl<R: Repository + 'static> ChatState<R> {
    /// Create a new chat state.
    pub fn new(
        session: Session,
        repository: Arc<R>,
        config: OrchestratorConfig,
    ) -> Self {
        let process_manager = Arc::new(ProcessManager::new(config, repository.clone()));
        Self {
            session,
            repository,
            process_manager,
        }
    }

    /// Get the current session ID.
    pub fn session_id(&self) -> SessionId {
        self.session.id
    }

    /// Get the population cap.
    pub fn population_cap(&self) -> usize {
        self.session.population_cap
    }

    // === Channel operations ===

    /// Create a channel.
    pub async fn create_channel(
        &self,
        name: &str,
        description: &str,
    ) -> RepositoryResult<Channel> {
        let channel_id = ChannelId::new(name).map_err(|e| {
            harness_persistence::RepositoryError::Database(e.to_string())
        })?;
        let channel = Channel::new(channel_id, description, self.session.id);
        self.repository.create_channel(&channel).await?;
        Ok(channel)
    }

    /// List all channels.
    pub async fn list_channels(&self) -> RepositoryResult<Vec<Channel>> {
        self.repository.list_channels(self.session.id).await
    }

    // === Message operations ===

    /// Send a message.
    pub async fn send_message(
        &self,
        channel_id: &ChannelId,
        author_id: AgentId,
        content: &str,
        reply_to: Option<harness_persistence::MessageId>,
    ) -> RepositoryResult<Message> {
        let mut message = Message::new(channel_id.clone(), author_id, content, self.session.id);
        if let Some(reply) = reply_to {
            message = message.with_reply_to(reply);
        }
        self.repository.create_message(&message).await?;
        Ok(message)
    }

    /// Read messages from a channel.
    pub async fn read_messages(
        &self,
        channel_id: &ChannelId,
        limit: usize,
    ) -> RepositoryResult<Vec<Message>> {
        self.repository.get_messages(channel_id, limit, None).await
    }

    /// Search messages semantically.
    pub async fn search_messages(&self, query: &str, limit: usize) -> RepositoryResult<Vec<Message>> {
        self.repository.search_messages_semantic(query, limit).await
    }

    // === Agent operations ===

    /// Create an agent.
    pub async fn create_agent(&self, role: &str, system_prompt: &str) -> RepositoryResult<Agent> {
        let agent = Agent::new(role, system_prompt, self.session.id);
        self.repository.create_agent(&agent).await?;
        Ok(agent)
    }

    /// List active agents.
    pub async fn list_agents(&self) -> RepositoryResult<Vec<Agent>> {
        self.repository.list_active_agents(self.session.id).await
    }

    /// Count active agents.
    pub async fn count_agents(&self) -> RepositoryResult<usize> {
        self.repository.count_active_agents(self.session.id).await
    }

    /// Get an agent by ID.
    pub async fn get_agent(&self, id: AgentId) -> RepositoryResult<Agent> {
        self.repository.get_agent(id).await
    }

    /// Get the process manager.
    pub fn process_manager(&self) -> &Arc<ProcessManager<R>> {
        &self.process_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_persistence::InMemoryRepository;

    #[tokio::test]
    async fn test_channel_creation() {
        let repo = Arc::new(InMemoryRepository::new());
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let state = ChatState::new(session, repo, OrchestratorConfig::default());

        let channel = state.create_channel("general", "General chat").await.unwrap();
        assert_eq!(channel.id.as_str(), "#general");

        let channels = state.list_channels().await.unwrap();
        assert_eq!(channels.len(), 1);
    }

    #[tokio::test]
    async fn test_message_flow() {
        let repo = Arc::new(InMemoryRepository::new());
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let state = ChatState::new(session, repo, OrchestratorConfig::default());

        let channel = state.create_channel("general", "General chat").await.unwrap();
        let agent = state.create_agent("tester", "You are a tester.").await.unwrap();

        let msg = state
            .send_message(&channel.id, agent.id, "Hello world!", None)
            .await
            .unwrap();

        let messages = state.read_messages(&channel.id, 10).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello world!");
    }
}
```

**Step 2: Run tests**

Run: `cargo test -p harness-mcp`
Expected: PASS

**Step 3: Update lib.rs**

```rust
//! MCP chat server for Harness.

mod state;

pub use state::ChatState;
```

**Step 4: Commit**

```bash
git add -A
git commit -m "feat(mcp): add ChatState manager

Coordinates channels, messages, and agents.
Wraps Repository and ProcessManager for tool handlers.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 4.2: MCP Tool Definitions

**Files:**
- Create: `crates/harness-mcp/src/tools.rs`
- Modify: `crates/harness-mcp/src/lib.rs`

**Step 1: Define tool request/response types**

Create `crates/harness-mcp/src/tools.rs`:
```rust
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
```

**Step 2: Update lib.rs**

```rust
//! MCP chat server for Harness.

mod state;
mod tools;

pub use state::ChatState;
pub use tools::*;
```

**Step 3: Verify compilation**

Run: `cargo build -p harness-mcp`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add -A
git commit -m "feat(mcp): add tool request/response types

Standard tools: send_message, read_messages, list_channels,
  create_channel, whoami, list_agents, request_spawn

God-mode tools: spawn_agent, kill_agent, broadcast,
  clear_channel, set_population_cap

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Phase 5: TUI Foundation (harness-tui)

### Task 5.1: App State

**Files:**
- Create: `crates/harness-tui/src/app.rs`
- Modify: `crates/harness-tui/src/lib.rs`

**Step 1: Write app state**

Create `crates/harness-tui/src/app.rs`:
```rust
//! TUI application state.

use harness_persistence::{AgentId, ChannelId};

/// Which panel is currently focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusedPanel {
    /// Channel list panel.
    #[default]
    Channels,
    /// Message view panel.
    Messages,
    /// Input field.
    Input,
    /// Agent list panel.
    Agents,
}

impl FocusedPanel {
    /// Cycle to the next panel.
    pub fn next(self) -> Self {
        match self {
            Self::Channels => Self::Messages,
            Self::Messages => Self::Input,
            Self::Input => Self::Agents,
            Self::Agents => Self::Channels,
        }
    }

    /// Cycle to the previous panel.
    pub fn prev(self) -> Self {
        match self {
            Self::Channels => Self::Agents,
            Self::Messages => Self::Channels,
            Self::Input => Self::Messages,
            Self::Agents => Self::Input,
        }
    }
}

/// Application mode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AppMode {
    /// Normal mode.
    #[default]
    Normal,
    /// Command mode (after pressing /).
    Command,
    /// Spawn dialog.
    SpawnDialog,
    /// Kill confirmation dialog.
    KillConfirm { agent_id: AgentId },
}

/// Application state.
#[derive(Debug, Default)]
pub struct AppState {
    /// Currently focused panel.
    pub focus: FocusedPanel,
    /// Current mode.
    pub mode: AppMode,
    /// Selected channel index.
    pub selected_channel: usize,
    /// Selected agent index.
    pub selected_agent: usize,
    /// Message scroll offset.
    pub message_scroll: usize,
    /// Input buffer.
    pub input: String,
    /// Command buffer (when in command mode).
    pub command: String,
    /// Whether the app should quit.
    pub should_quit: bool,
    /// Status message to display.
    pub status_message: Option<String>,
}

impl AppState {
    /// Create a new app state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cycle focus to the next panel.
    pub fn focus_next(&mut self) {
        self.focus = self.focus.next();
    }

    /// Cycle focus to the previous panel.
    pub fn focus_prev(&mut self) {
        self.focus = self.focus.prev();
    }

    /// Enter command mode.
    pub fn enter_command_mode(&mut self) {
        self.mode = AppMode::Command;
        self.command.clear();
    }

    /// Exit command mode.
    pub fn exit_command_mode(&mut self) {
        self.mode = AppMode::Normal;
        self.command.clear();
    }

    /// Set a status message.
    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    /// Clear the status message.
    pub fn clear_status(&mut self) {
        self.status_message = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_cycles() {
        let mut state = AppState::new();
        assert_eq!(state.focus, FocusedPanel::Channels);

        state.focus_next();
        assert_eq!(state.focus, FocusedPanel::Messages);

        state.focus_next();
        assert_eq!(state.focus, FocusedPanel::Input);

        state.focus_next();
        assert_eq!(state.focus, FocusedPanel::Agents);

        state.focus_next();
        assert_eq!(state.focus, FocusedPanel::Channels);
    }

    #[test]
    fn command_mode_toggle() {
        let mut state = AppState::new();
        assert_eq!(state.mode, AppMode::Normal);

        state.enter_command_mode();
        assert_eq!(state.mode, AppMode::Command);

        state.exit_command_mode();
        assert_eq!(state.mode, AppMode::Normal);
    }
}
```

**Step 2: Run tests**

Run: `cargo test -p harness-tui`
Expected: PASS

**Step 3: Update lib.rs**

```rust
//! TUI interface for Harness.

mod app;

pub use app::{AppMode, AppState, FocusedPanel};
```

**Step 4: Commit**

```bash
git add -A
git commit -m "feat(tui): add AppState

Tracks focus, mode, selection, scroll, input buffers.
Supports normal, command, spawn dialog, kill confirm modes.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Phase 6: Integration

### Task 6.1: Main Binary

**Files:**
- Modify: `src/main.rs`

**Step 1: Wire up components**

Replace `src/main.rs`:
```rust
//! Harness - Multi-Claude Orchestration System

use std::sync::Arc;

use anyhow::Result;
use harness_mcp::ChatState;
use harness_orchestrator::OrchestratorConfig;
use harness_persistence::{InMemoryRepository, Session};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Harness - Multi-Claude Orchestration System");

    // Create repository (will be replaced with AletheiaDB)
    let repository = Arc::new(InMemoryRepository::new());

    // Create session
    let session = Session::new(8);
    repository.create_session(&session).await?;

    // Create chat state
    let config = OrchestratorConfig::default();
    let state = ChatState::new(session, repository, config);

    // Create default channel
    state.create_channel("general", "General discussion").await?;

    tracing::info!(
        session_id = %state.session_id(),
        population_cap = state.population_cap(),
        "Session started"
    );

    // TODO: Start TUI or MCP server based on args
    println!("Harness initialized. Session: {}", state.session_id());
    println!("Population cap: {}", state.population_cap());

    let channels = state.list_channels().await?;
    for channel in channels {
        println!("Channel: {} - {}", channel.id, channel.description);
    }

    Ok(())
}
```

**Step 2: Verify it runs**

Run: `cargo run`
Expected: Prints session info and channel list

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: wire up main binary

Initializes repository, session, and chat state.
Creates default #general channel.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Summary

This plan covers the foundational layers:

1. **Phase 1**: Workspace scaffolding with 4 crates
2. **Phase 2**: Core types and in-memory repository
3. **Phase 3**: Orchestrator with process management
4. **Phase 4**: MCP tool definitions and state manager
5. **Phase 5**: TUI app state foundation
6. **Phase 6**: Main binary integration

**Next phases to plan:**
- MCP server implementation with rust-mcp-sdk
- AletheiaDB repository implementation
- Full TUI rendering with ratatui
- End-to-end testing with real Claude instances
