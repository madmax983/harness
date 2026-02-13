//! Shared state for the Hive Mind MCP server.

use std::sync::Arc;

use harness_orchestrator::ProcessManager;
use harness_persistence::{Repository, Session, SessionId};

/// Shared state for MCP tool handlers.
pub struct HiveState<R: Repository> {
    /// Current session.
    session: Session,
    /// Repository for persistence.
    repository: Arc<R>,
    /// Manager for agent processes.
    process_manager: Arc<ProcessManager<R>>,
    /// Optional embedding service for semantic search.
    embedding_service: Option<Arc<aletheiadb::embeddings::EmbeddingService>>,
}

impl<R: Repository + 'static> HiveState<R> {
    /// Create a new hive state.
    pub fn new(
        session: Session,
        repository: Arc<R>,
        process_manager: Arc<ProcessManager<R>>,
    ) -> Self {
        Self {
            session,
            repository,
            process_manager,
            embedding_service: None,
        }
    }

    /// Set the embedding service (builder pattern).
    pub fn with_embedding_service(
        mut self,
        svc: Arc<aletheiadb::embeddings::EmbeddingService>,
    ) -> Self {
        self.embedding_service = Some(svc);
        self
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

    /// Get the process manager.
    pub fn process_manager(&self) -> &Arc<ProcessManager<R>> {
        &self.process_manager
    }

    /// Get the embedding service, if configured.
    pub fn embedding_service(&self) -> Option<&Arc<aletheiadb::embeddings::EmbeddingService>> {
        self.embedding_service.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aletheiadb::AletheiaDB;
    use harness_orchestrator::OrchestratorConfig;
    use harness_persistence::AletheiaRepository;

    #[tokio::test]
    async fn test_hive_state_creation() {
        let db = Arc::new(AletheiaDB::new().unwrap());
        let repo = Arc::new(AletheiaRepository::new_anon(db));
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let config = OrchestratorConfig::default();
        let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));

        let state = HiveState::new(session, repo, process_manager);
        assert_eq!(state.population_cap(), 8);
        assert!(state.embedding_service().is_none());
    }
}
