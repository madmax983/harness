//! Shared state for the MCP server.

use std::sync::Arc;

use harness_orchestrator::{OrchestratorConfig, ProcessManager};
use harness_persistence::{
    Agent, AgentId, Channel, ChannelId, Message, Repository, RepositoryResult,
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

        let _msg = state
            .send_message(&channel.id, agent.id, "Hello world!", None)
            .await
            .unwrap();

        let messages = state.read_messages(&channel.id, 10).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello world!");
    }
}
