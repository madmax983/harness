//! Shared state for the Hive Mind MCP server.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use harness_orchestrator::ProcessManager;
use harness_persistence::{
    AgentId, PatternStore, ReasoningBank, Repository, Session, SessionId, TaskId,
    TrajectoryRecorder,
};
use harness_sona::{SonaConfig, SonaEngine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};

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

/// Stored spawn metadata used for supervised restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpawnSpec {
    pub role: String,
    pub cli_command: String,
    pub cli_args: Vec<String>,
    pub custom_prompt: Option<String>,
    pub directive: Option<String>,
    pub poll_interval_secs: u64,
}

/// Scheduled coding-agent maintenance workload definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodingAgentSchedule {
    pub schedule_id: String,
    pub name: String,
    pub cadence_minutes: u64,
    pub prompt_template: String,
    pub task_title_template: String,
    pub task_priority: String,
    pub auto_dispatch: bool,
    pub enabled: bool,
    pub created_by: AgentId,
    pub created_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: DateTime<Utc>,
}

/// One executable step in a workflow definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowStepDefinition {
    pub step_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    pub max_attempts: u32,
    pub timeout_secs: u64,
    pub backoff_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub red_evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub green_evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_verification: Option<String>,
}

/// Stored workflow definition for orchestration control plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub workflow_id: String,
    pub name: String,
    pub description: String,
    /// active | paused
    pub status: String,
    pub steps: Vec<WorkflowStepDefinition>,
    pub max_concurrency: usize,
    /// fail_fast | continue_on_failure
    pub failure_policy: String,
    pub retries: u32,
    pub timeout_secs: u64,
    pub backoff_secs: u64,
    pub created_by: AgentId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Persisted step-run transition record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowStepRun {
    pub step_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    /// queued | running | succeeded | failed | blocked
    pub status: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub timeout_secs: u64,
    pub backoff_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub red_evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub green_evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_verification: Option<String>,
}

/// A single scheduler workflow execution record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodingAgentWorkflowRun {
    pub run_id: String,
    pub workflow_id: String,
    #[serde(default)]
    pub schedule_id: String,
    #[serde(default)]
    pub schedule_name: String,
    pub task_id: TaskId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_agent_id: Option<AgentId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<AgentId>,
    /// queued | running | succeeded | failed | blocked
    pub status: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub timeout_secs: u64,
    pub backoff_secs: u64,
    pub last_error: Option<String>,
    pub evidence_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    pub step_runs: Vec<WorkflowStepRun>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
    /// Per-agent spawn metadata for supervision/restart flows.
    agent_spawn_specs: Arc<RwLock<HashMap<AgentId, AgentSpawnSpec>>>,
    /// Strategoi-managed coding-agent schedules.
    coding_agent_schedules: Arc<RwLock<HashMap<String, CodingAgentSchedule>>>,
    /// Workflow control-plane definitions.
    workflow_definitions: Arc<RwLock<HashMap<String, WorkflowDefinition>>>,
    /// Single-flight guard for schedule execution to prevent duplicate concurrent runs.
    coding_agent_scheduler_lock: Arc<Mutex<()>>,
    /// Scheduler + workflow execution records (append-only per run_id).
    coding_agent_workflow_runs: Arc<RwLock<HashMap<String, Vec<CodingAgentWorkflowRun>>>>,
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
        let trajectory_recorder = Arc::new(TrajectoryRecorder::new(repository.clone(), session.id));
        let pattern_store = Arc::new(PatternStore::new());
        Self {
            session,
            repository,
            process_manager,
            embedding_service: None,
            session_agents: Arc::new(RwLock::new(HashMap::new())),
            micro_lora: Arc::new(RwLock::new(HashMap::new())),
            agent_spawn_specs: Arc::new(RwLock::new(HashMap::new())),
            coding_agent_schedules: Arc::new(RwLock::new(HashMap::new())),
            workflow_definitions: Arc::new(RwLock::new(HashMap::new())),
            coding_agent_scheduler_lock: Arc::new(Mutex::new(())),
            coding_agent_workflow_runs: Arc::new(RwLock::new(HashMap::new())),
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

    /// Record spawn metadata for an agent.
    pub async fn set_agent_spawn_spec(&self, agent_id: AgentId, spec: AgentSpawnSpec) {
        let mut specs = self.agent_spawn_specs.write().await;
        specs.insert(agent_id, spec);
    }

    /// Retrieve spawn metadata for an agent.
    pub async fn get_agent_spawn_spec(&self, agent_id: AgentId) -> Option<AgentSpawnSpec> {
        let specs = self.agent_spawn_specs.read().await;
        specs.get(&agent_id).cloned()
    }

    /// Insert or update a coding-agent schedule.
    pub async fn upsert_coding_agent_schedule(&self, schedule: CodingAgentSchedule) {
        let mut schedules = self.coding_agent_schedules.write().await;
        schedules.insert(schedule.schedule_id.clone(), schedule);
    }

    /// Retrieve one coding-agent schedule by ID.
    pub async fn get_coding_agent_schedule(
        &self,
        schedule_id: &str,
    ) -> Option<CodingAgentSchedule> {
        let schedules = self.coding_agent_schedules.read().await;
        schedules.get(schedule_id).cloned()
    }

    /// List coding-agent schedules.
    pub async fn list_coding_agent_schedules(&self) -> Vec<CodingAgentSchedule> {
        let schedules = self.coding_agent_schedules.read().await;
        schedules.values().cloned().collect()
    }

    /// Insert or update a workflow definition.
    pub async fn upsert_workflow_definition(&self, workflow: WorkflowDefinition) {
        let mut workflows = self.workflow_definitions.write().await;
        workflows.insert(workflow.workflow_id.clone(), workflow);
    }

    /// Retrieve one workflow definition by ID.
    pub async fn get_workflow_definition(&self, workflow_id: &str) -> Option<WorkflowDefinition> {
        let workflows = self.workflow_definitions.read().await;
        workflows.get(workflow_id).cloned()
    }

    /// List all workflow definitions.
    pub async fn list_workflow_definitions(&self) -> Vec<WorkflowDefinition> {
        let workflows = self.workflow_definitions.read().await;
        workflows.values().cloned().collect()
    }

    /// Single-flight lock for schedule execution.
    pub fn coding_agent_scheduler_lock(&self) -> &Arc<Mutex<()>> {
        &self.coding_agent_scheduler_lock
    }

    /// Insert or update a scheduler workflow run.
    pub async fn upsert_coding_agent_workflow_run(&self, run: CodingAgentWorkflowRun) {
        let mut runs = self.coding_agent_workflow_runs.write().await;
        let history = runs.entry(run.run_id.clone()).or_default();
        if history.last().is_some_and(|existing| existing == &run) {
            return;
        }
        history.push(run);
    }

    /// List scheduler workflow runs.
    pub async fn list_coding_agent_workflow_runs(&self) -> Vec<CodingAgentWorkflowRun> {
        let runs = self.coding_agent_workflow_runs.read().await;
        let mut flattened: Vec<CodingAgentWorkflowRun> = runs
            .values()
            .flat_map(|history| history.iter().cloned())
            .collect();
        flattened.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        flattened
    }

    /// Get latest transition snapshot for a workflow run ID.
    pub async fn get_coding_agent_workflow_run(
        &self,
        run_id: &str,
    ) -> Option<CodingAgentWorkflowRun> {
        let runs = self.coding_agent_workflow_runs.read().await;
        runs.get(run_id).and_then(|history| history.last().cloned())
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
    use serde_json::Value;

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

    fn sample_workflow_run(status: &str) -> CodingAgentWorkflowRun {
        let now = Utc::now();
        CodingAgentWorkflowRun {
            run_id: "run-red-state-1".to_string(),
            workflow_id: "workflow-red-state-1".to_string(),
            schedule_id: "schedule-red-state-1".to_string(),
            schedule_name: "workflow-state-red".to_string(),
            task_id: TaskId::new(),
            worker_agent_id: None,
            created_by: None,
            status: status.to_string(),
            attempt: 1,
            max_attempts: 3,
            timeout_secs: 300,
            backoff_secs: 30,
            last_error: None,
            evidence_status: "red_missing".to_string(),
            payload: None,
            step_runs: vec![WorkflowStepRun {
                step_id: "step-red-1".to_string(),
                kind: "run_tool".to_string(),
                tool: Some("list_tasks".to_string()),
                args: None,
                status: status.to_string(),
                attempt: 1,
                max_attempts: 3,
                timeout_secs: 300,
                backoff_secs: 30,
                last_error: None,
                red_evidence: None,
                green_evidence: None,
                final_verification: None,
            }],
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn test_workflow_run_serialization_includes_evidence_gate_fields() {
        let run = sample_workflow_run("task_created");
        let serialized: Value = serde_json::to_value(run).expect("serialize workflow run");

        for expected_field in [
            "attempt",
            "max_attempts",
            "timeout_secs",
            "backoff_secs",
            "last_error",
            "evidence_status",
            "step_runs",
        ] {
            assert!(
                serialized.get(expected_field).is_some(),
                "missing expected workflow evidence field: {expected_field}"
            );
        }
    }

    #[tokio::test]
    async fn test_workflow_run_status_uses_control_plane_lifecycle_values() {
        let run = sample_workflow_run("queued");
        let allowed = ["queued", "running", "succeeded", "failed", "blocked"];

        assert!(
            allowed.contains(&run.status.as_str()),
            "workflow run status `{}` must be in control-plane lifecycle set",
            run.status
        );
    }

    #[tokio::test]
    async fn test_workflow_run_updates_preserve_transition_history() {
        let db = Arc::new(AletheiaDB::new().unwrap());
        let repo = Arc::new(AletheiaRepository::new_anon(db));
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let config = OrchestratorConfig::default();
        let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));
        let state = HiveState::new(session, repo, process_manager);

        let mut first = sample_workflow_run("task_created");
        first.run_id = "run-red-state-history".to_string();
        let mut second = first.clone();
        second.status = "failed".to_string();
        second.attempt = 2;
        second.last_error = Some("retry budget exhausted".to_string());

        state.upsert_coding_agent_workflow_run(first).await;
        state.upsert_coding_agent_workflow_run(second).await;

        let history_count = state
            .list_coding_agent_workflow_runs()
            .await
            .into_iter()
            .filter(|run| run.run_id == "run-red-state-history")
            .count();

        assert_eq!(
            history_count, 2,
            "workflow run updates should preserve transition history"
        );
    }

    #[tokio::test]
    async fn test_workflow_run_upsert_ignores_duplicate_snapshot() {
        let db = Arc::new(AletheiaDB::new().unwrap());
        let repo = Arc::new(AletheiaRepository::new_anon(db));
        let session = Session::new(8);
        repo.create_session(&session).await.unwrap();

        let config = OrchestratorConfig::default();
        let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));
        let state = HiveState::new(session, repo, process_manager);

        let mut run = sample_workflow_run("queued");
        run.run_id = "run-red-state-dedupe".to_string();

        state.upsert_coding_agent_workflow_run(run.clone()).await;
        state.upsert_coding_agent_workflow_run(run).await;

        let history_count = state
            .list_coding_agent_workflow_runs()
            .await
            .into_iter()
            .filter(|candidate| candidate.run_id == "run-red-state-dedupe")
            .count();

        assert_eq!(
            history_count, 1,
            "identical workflow run snapshots should not duplicate history entries"
        );
    }
}
