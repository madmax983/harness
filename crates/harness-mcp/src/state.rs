//! Shared state for the Hive Mind MCP server.

use std::sync::Arc;

use harness_persistence::{Repository, Session, SessionId};

/// Shared state for MCP tool handlers.
pub struct HiveState<R: Repository> {
    /// Current session.
    session: Session,
    /// Repository for persistence.
    repository: Arc<R>,
}

impl<R: Repository + 'static> HiveState<R> {
    /// Create a new hive state.
    pub fn new(session: Session, repository: Arc<R>) -> Self {
        Self {
            session,
            repository,
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

    /// Get the repository.
    pub fn repository(&self) -> &Arc<R> {
        &self.repository
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_persistence::InMemoryRepository;

    #[tokio::test]
    async fn test_hive_state_creation() {
        let repo = Arc::new(InMemoryRepository::new());
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let state = HiveState::new(session, repo);
        assert_eq!(state.population_cap(), 8);
    }
}
