//! Shared state for the Hive Mind MCP server.

use std::collections::HashMap;
use std::sync::Arc;

use harness_orchestrator::ProcessManager;
use harness_persistence::{
    AgentId, PatternStore, ReasoningBank, Repository, Session, SessionId, TaskId,
    TrajectoryRecorder,
};
use harness_sona::{SonaConfig, SonaEngine};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// A single recorded trajectory step (action + context + outcome + reward).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryRecord {
    pub action: String,
    pub context: String,
    pub outcome: String,
    pub reward: f64,
}

/// A complete trajectory (sequence of steps) with an ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTrajectory {
    pub id: String,
    pub steps: Vec<TrajectoryRecord>,
}

/// Per-agent MicroLoRA state: accumulated trajectories and computed statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentLoraData {
    pub trajectories: Vec<StoredTrajectory>,
    pub total_steps: u64,
    pub reward_sum: f64,
}

impl AgentLoraData {
    /// Number of trajectories ingested.
    pub fn trajectories_ingested(&self) -> u64 {
        self.trajectories.len() as u64
    }

    /// Mean reward across all steps.
    pub fn mean_reward(&self) -> f64 {
        if self.total_steps == 0 {
            0.0
        } else {
            self.reward_sum / self.total_steps as f64
        }
    }

    /// Compute the mean reward for a specific action across all trajectories.
    pub fn action_mean_reward(&self, action: &str) -> Option<f64> {
        let mut sum = 0.0;
        let mut count = 0u64;
        for traj in &self.trajectories {
            for step in &traj.steps {
                if step.action == action {
                    sum += step.reward;
                    count += 1;
                }
            }
        }
        if count == 0 {
            None
        } else {
            Some(sum / count as f64)
        }
    }
}

/// Counters for SONA learning loop status tracking.
#[derive(Debug, Default)]
pub struct LearningCounters {
    /// Number of instant patterns learned (from knowledge shares and task completions).
    pub instant_patterns: std::sync::atomic::AtomicU64,
    /// Number of background optimizations applied.
    pub background_optimizations: std::sync::atomic::AtomicU64,
    /// Number of coordination patterns discovered.
    pub coordination_patterns: std::sync::atomic::AtomicU64,
    /// Total trajectory events recorded.
    pub total_events: std::sync::atomic::AtomicU64,
}

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
    /// Maps MCP session ID → registered agent ID (for automatic agent context)
    session_agents: Arc<RwLock<HashMap<String, AgentId>>>,
    /// Per-agent MicroLoRA state (in-memory, persisted via tools).
    micro_lora: Arc<RwLock<HashMap<AgentId, AgentLoraData>>>,
    /// SONA: Trajectory recorder for capturing agent action sequences.
    trajectory_recorder: Arc<TrajectoryRecorder<R>>,
    /// SONA: Shared pattern store (persists across ReasoningBank instances).
    pattern_store: Arc<PatternStore>,
    /// SONA: Learning counters for status tracking.
    learning_counters: Arc<LearningCounters>,
    /// SONA: Map of task_id → trajectory event IDs for per-task trajectory lookup.
    task_trajectories: Arc<RwLock<HashMap<TaskId, Vec<String>>>>,
    /// SONA: Engine for EWC++ and adaptive learning.
    sona_engine: Option<Arc<SonaEngine>>,
    /// SONA: Configuration for runtime toggles.
    sona_config: SonaConfig,
}

impl<R: Repository + 'static> HiveState<R> {
    /// Create a new hive state with default SONA configuration.
    pub fn new(
        session: Session,
        repository: Arc<R>,
        process_manager: Arc<ProcessManager<R>>,
    ) -> Self {
        Self::with_sona_config(session, repository, process_manager, SonaConfig::default())
    }

    /// Create a new hive state with explicit SONA configuration.
    pub fn with_sona_config(
        session: Session,
        repository: Arc<R>,
        process_manager: Arc<ProcessManager<R>>,
        sona_config: SonaConfig,
    ) -> Self {
        let trajectory_recorder = Arc::new(TrajectoryRecorder::new(
            repository.clone(),
            session.id,
        ));
        let pattern_store = Arc::new(PatternStore::new());
        Self {
            session,
            repository,
            process_manager,
            embedding_service: None,
            session_agents: Arc::new(RwLock::new(HashMap::new())),
            micro_lora: Arc::new(RwLock::new(HashMap::new())),
            trajectory_recorder,
            pattern_store,
            learning_counters: Arc::new(LearningCounters::default()),
            task_trajectories: Arc::new(RwLock::new(HashMap::new())),
            sona_engine: None,
            sona_config,
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

    /// Set the SONA engine (builder pattern).
    pub fn with_sona_engine(mut self, engine: Arc<SonaEngine>) -> Self {
        self.sona_engine = Some(engine);
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

    /// Get the MicroLoRA state store.
    pub fn micro_lora(&self) -> &Arc<RwLock<HashMap<AgentId, AgentLoraData>>> {
        &self.micro_lora
    }

    /// Get the SONA trajectory recorder.
    pub fn trajectory_recorder(&self) -> &Arc<TrajectoryRecorder<R>> {
        &self.trajectory_recorder
    }

    /// Get the SONA pattern store.
    pub fn pattern_store(&self) -> &Arc<PatternStore> {
        &self.pattern_store
    }

    /// Get the SONA learning counters.
    pub fn learning_counters(&self) -> &Arc<LearningCounters> {
        &self.learning_counters
    }

    /// Get the task-to-trajectory mapping.
    pub fn task_trajectories(&self) -> &Arc<RwLock<HashMap<TaskId, Vec<String>>>> {
        &self.task_trajectories
    }

    /// Create a ReasoningBank backed by this state's pattern store.
    pub fn reasoning_bank(&self) -> ReasoningBank {
        ReasoningBank::new(self.pattern_store.clone(), self.session.id)
    }

    /// Get the SONA engine, if configured.
    pub fn sona_engine(&self) -> Option<&Arc<SonaEngine>> {
        self.sona_engine.as_ref()
    }

    /// Get the SONA configuration.
    pub fn sona_config(&self) -> &SonaConfig {
        &self.sona_config
    }

    /// Check if SONA is enabled.
    pub fn is_sona_enabled(&self) -> bool {
        self.sona_config.enabled
    }

    /// Register an agent for an MCP session ID.
    pub async fn register_session_agent(&self, mcp_session_id: String, agent_id: AgentId) {
        let mut agents = self.session_agents.write().await;
        agents.insert(mcp_session_id.clone(), agent_id);
        tracing::info!(
            mcp_session_id = %mcp_session_id,
            agent_id = %agent_id,
            "Registered agent for MCP session"
        );
    }

    /// Get the agent ID for an MCP session (if registered).
    pub async fn get_session_agent(&self, mcp_session_id: &str) -> Option<AgentId> {
        let agents = self.session_agents.read().await;
        agents.get(mcp_session_id).copied()
    }

    /// Clear session agent registration.
    pub async fn clear_session_agent(&self, mcp_session_id: &str) {
        let mut agents = self.session_agents.write().await;
        if agents.remove(mcp_session_id).is_some() {
            tracing::info!(
                mcp_session_id = %mcp_session_id,
                "Cleared MCP session agent registration"
            );
        }
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
