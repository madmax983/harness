//! In-memory repository implementation for testing.

use std::collections::HashMap;
use std::sync::RwLock;

use chrono::{DateTime, Utc};

use crate::{
    Agent, AgentId, AgentStatus, Channel, ChannelId, Message, Repository,
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
                        AgentStatus::Starting | AgentStatus::Active
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

        // Initially pending, not counted as active
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 0);

        // Transition to Starting - now active
        repo.update_agent_status(agent_id, AgentStatus::Starting)
            .await
            .unwrap();
        assert_eq!(repo.count_active_agents(session.id).await.unwrap(), 1);

        // Kill the agent - no longer active
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
