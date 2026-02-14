//! Trajectory recording and learning trigger system for SONA integration.
//!
//! Records learning events triggered by hive activities:
//! - Task completions trigger experience recording
//! - Knowledge shares trigger learning trajectories
//! - Project closures trigger consolidation
//!
//! Events are buffered in-memory for minimal overhead, then flushed
//! to persistent storage asynchronously.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AgentId, KnowledgeKind, RepositoryError, RepositoryResult, SessionId, TaskId};

// ---------------------------------------------------------------------------
// TrajectoryEventId
// ---------------------------------------------------------------------------

/// Unique identifier for a trajectory event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TrajectoryEventId(Uuid);

impl TrajectoryEventId {
    /// Create a new random trajectory event ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the inner UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for TrajectoryEventId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TrajectoryEventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "traj-{}", &self.0.to_string()[..8])
    }
}

// ---------------------------------------------------------------------------
// TriggerKind
// ---------------------------------------------------------------------------

/// The kind of learning trigger that produced a trajectory event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    /// A task was completed (successfully or not).
    TaskComplete,
    /// Knowledge was shared to the hive.
    KnowledgeShare,
    /// A project was closed/completed.
    ProjectClose,
}

// ---------------------------------------------------------------------------
// LearningTrigger (input to record())
// ---------------------------------------------------------------------------

/// A learning trigger that describes an event to be recorded as a trajectory.
/// Uses builder pattern for optional fields.
#[derive(Debug, Clone)]
pub struct LearningTrigger {
    pub(crate) kind: TriggerKind,
    pub(crate) agent_id: AgentId,
    pub(crate) task_id: Option<TaskId>,
    pub(crate) success: bool,
    pub(crate) summary: String,
    pub(crate) knowledge_kind: Option<KnowledgeKind>,
    pub(crate) project_name: Option<String>,
    pub(crate) tasks_completed: usize,
    pub(crate) tasks_failed: usize,
}

impl LearningTrigger {
    /// Create a new learning trigger.
    pub fn new(kind: TriggerKind, agent_id: AgentId) -> Self {
        Self {
            kind,
            agent_id,
            task_id: None,
            success: true,
            summary: String::new(),
            knowledge_kind: None,
            project_name: None,
            tasks_completed: 0,
            tasks_failed: 0,
        }
    }

    /// Set the task ID for this trigger.
    pub fn with_task_id(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }

    /// Set whether the outcome was successful.
    pub fn with_success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    /// Set the summary/description.
    pub fn with_summary(mut self, summary: &str) -> Self {
        self.summary = summary.to_string();
        self
    }

    /// Set the knowledge kind (for KnowledgeShare triggers).
    pub fn with_knowledge_kind(mut self, kind: KnowledgeKind) -> Self {
        self.knowledge_kind = Some(kind);
        self
    }

    /// Set the project name (for ProjectClose triggers).
    pub fn with_project_name(mut self, name: &str) -> Self {
        self.project_name = Some(name.to_string());
        self
    }

    /// Set the task completion/failure stats (for ProjectClose triggers).
    pub fn with_stats(mut self, completed: usize, failed: usize) -> Self {
        self.tasks_completed = completed;
        self.tasks_failed = failed;
        self
    }
}

// ---------------------------------------------------------------------------
// TrajectoryStep
// ---------------------------------------------------------------------------

/// A single step in a trajectory event, carrying a typed payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryStep {
    kind: String,
    agent_id: AgentId,
    payload: serde_json::Map<String, serde_json::Value>,
}

impl TrajectoryStep {
    /// Get the step kind (e.g., "task_outcome", "knowledge_acquired", "project_consolidation").
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Get the agent ID for this step.
    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    /// Get the typed payload map.
    pub fn payload(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.payload
    }
}

// ---------------------------------------------------------------------------
// TrajectoryEvent
// ---------------------------------------------------------------------------

/// A recorded trajectory event with generated steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryEvent {
    id: TrajectoryEventId,
    trigger_kind: TriggerKind,
    agent_id: AgentId,
    task_id: Option<TaskId>,
    success: bool,
    summary: String,
    steps: Vec<TrajectoryStep>,
    created_at: DateTime<Utc>,
}

impl TrajectoryEvent {
    /// Get the event ID.
    pub fn id(&self) -> TrajectoryEventId {
        self.id
    }

    /// Get the trigger kind.
    pub fn trigger_kind(&self) -> TriggerKind {
        self.trigger_kind
    }

    /// Get the agent ID.
    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    /// Get the task ID (if applicable).
    pub fn task_id(&self) -> Option<TaskId> {
        self.task_id
    }

    /// Whether the outcome was successful.
    pub fn success(&self) -> bool {
        self.success
    }

    /// Get the summary text.
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Get the trajectory steps.
    pub fn steps(&self) -> &[TrajectoryStep] {
        &self.steps
    }

    /// Get when this event was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

// ---------------------------------------------------------------------------
// TrajectoryQuery
// ---------------------------------------------------------------------------

/// Query builder for searching trajectory events.
#[derive(Debug, Clone, Default)]
pub struct TrajectoryQuery {
    agent_id: Option<AgentId>,
    trigger_kind: Option<TriggerKind>,
    success_filter: Option<bool>,
    limit: usize,
}

impl TrajectoryQuery {
    /// Create a new empty query.
    pub fn new() -> Self {
        Self {
            agent_id: None,
            trigger_kind: None,
            success_filter: None,
            limit: 100,
        }
    }

    /// Filter by agent ID.
    pub fn with_agent(mut self, agent_id: AgentId) -> Self {
        self.agent_id = Some(agent_id);
        self
    }

    /// Filter by trigger kind.
    pub fn with_trigger_kind(mut self, kind: TriggerKind) -> Self {
        self.trigger_kind = Some(kind);
        self
    }

    /// Filter by success/failure.
    pub fn with_success_filter(mut self, success: bool) -> Self {
        self.success_filter = Some(success);
        self
    }

    /// Set result limit.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

// ---------------------------------------------------------------------------
// Step generation from triggers
// ---------------------------------------------------------------------------

fn generate_steps(trigger: &LearningTrigger, now: &DateTime<Utc>) -> Vec<TrajectoryStep> {
    let mut base_payload = serde_json::Map::new();
    base_payload.insert(
        "timestamp".to_string(),
        serde_json::Value::String(now.to_rfc3339()),
    );
    base_payload.insert(
        "agent_id".to_string(),
        serde_json::Value::String(trigger.agent_id.as_uuid().to_string()),
    );

    match trigger.kind {
        TriggerKind::TaskComplete => {
            let mut payload = base_payload;
            if let Some(task_id) = trigger.task_id {
                payload.insert(
                    "task_id".to_string(),
                    serde_json::Value::String(task_id.as_uuid().to_string()),
                );
            }
            payload.insert(
                "success".to_string(),
                serde_json::Value::Bool(trigger.success),
            );
            payload.insert(
                "summary".to_string(),
                serde_json::Value::String(trigger.summary.clone()),
            );
            vec![TrajectoryStep {
                kind: "task_outcome".to_string(),
                agent_id: trigger.agent_id,
                payload,
            }]
        }
        TriggerKind::KnowledgeShare => {
            let mut payload = base_payload;
            if let Some(kind) = trigger.knowledge_kind {
                let kind_str = match kind {
                    KnowledgeKind::Activity => "activity",
                    KnowledgeKind::Discovery => "discovery",
                    KnowledgeKind::Decision => "decision",
                    KnowledgeKind::Blocker => "blocker",
                };
                payload.insert(
                    "knowledge_kind".to_string(),
                    serde_json::Value::String(kind_str.to_string()),
                );
            }
            payload.insert(
                "summary".to_string(),
                serde_json::Value::String(trigger.summary.clone()),
            );
            vec![TrajectoryStep {
                kind: "knowledge_acquired".to_string(),
                agent_id: trigger.agent_id,
                payload,
            }]
        }
        TriggerKind::ProjectClose => {
            let mut payload = base_payload;
            if let Some(ref name) = trigger.project_name {
                payload.insert(
                    "project_name".to_string(),
                    serde_json::Value::String(name.clone()),
                );
            }
            payload.insert(
                "tasks_completed".to_string(),
                serde_json::Value::Number(trigger.tasks_completed.into()),
            );
            payload.insert(
                "tasks_failed".to_string(),
                serde_json::Value::Number(trigger.tasks_failed.into()),
            );
            payload.insert(
                "summary".to_string(),
                serde_json::Value::String(trigger.summary.clone()),
            );
            vec![TrajectoryStep {
                kind: "project_consolidation".to_string(),
                agent_id: trigger.agent_id,
                payload,
            }]
        }
    }
}

// ---------------------------------------------------------------------------
// RawEvent (internal buffer entry - minimal data for fast recording)
// ---------------------------------------------------------------------------

/// Internal buffer entry storing raw trigger data without step generation.
/// Steps are materialized lazily on get_event()/query() to keep record() fast.
///
/// Note: This is public for Repository trait implementations but should not
/// be used directly by external crates. Use TrajectoryEvent instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawEvent {
    pub id: TrajectoryEventId,
    pub session_id: SessionId,
    pub trigger_kind: TriggerKind,
    pub agent_id: AgentId,
    pub task_id: Option<TaskId>,
    pub success: bool,
    pub summary: String,
    pub knowledge_kind: Option<KnowledgeKind>,
    pub project_name: Option<String>,
    pub tasks_completed: usize,
    pub tasks_failed: usize,
    pub created_at: DateTime<Utc>,
}

impl RawEvent {
    /// Materialize this raw event into a full TrajectoryEvent with generated steps.
    fn materialize(&self) -> TrajectoryEvent {
        // Reconstruct a LearningTrigger to generate steps
        let mut trigger = LearningTrigger::new(self.trigger_kind, self.agent_id);
        trigger.task_id = self.task_id;
        trigger.success = self.success;
        trigger.summary = self.summary.clone();
        trigger.knowledge_kind = self.knowledge_kind;
        trigger.project_name = self.project_name.clone();
        trigger.tasks_completed = self.tasks_completed;
        trigger.tasks_failed = self.tasks_failed;

        let steps = generate_steps(&trigger, &self.created_at);

        TrajectoryEvent {
            id: self.id,
            trigger_kind: self.trigger_kind,
            agent_id: self.agent_id,
            task_id: self.task_id,
            success: self.success,
            summary: self.summary.clone(),
            steps,
            created_at: self.created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// TrajectoryRecorder
// ---------------------------------------------------------------------------

/// Records learning trajectories from hive events.
///
/// Uses a two-tier architecture for performance:
/// - **Hot path** (`record()`): Stores minimal `RawEvent` in a
///   `parking_lot::RwLock<Vec>` buffer. No step generation, no JSON
///   map construction. Just move the trigger data and push.
/// - **Cold path** (`get_event()`/`query()`): Materializes steps lazily
///   from `RawEvent` data when results are actually needed.
///
/// This keeps the hot path fast (lock + push) while deferring expensive
/// work (step generation with serde_json::Map) to query time.
///
/// **Persistence**: Events are persisted to AletheiaDB via `flush()`.
/// On creation, the recorder restores all persisted events for the session
/// into the in-memory buffer for fast queries.
pub struct TrajectoryRecorder<R: crate::Repository> {
    repo: Arc<R>,
    session_id: SessionId,
    /// Raw event buffer. parking_lot::RwLock is non-poisoning and
    /// has very low overhead (~15-20ns uncontended write lock).
    buffer: Arc<RwLock<Vec<RawEvent>>>,
    /// Index of the last successfully flushed event (exclusive).
    /// Events at indices [last_flushed_index..] need to be persisted.
    last_flushed_index: std::sync::atomic::AtomicUsize,
}

impl<R: crate::Repository> TrajectoryRecorder<R> {
    /// Create a new trajectory recorder.
    ///
    /// **Note**: Call `restore()` after construction to load persisted events
    /// from the database. The split creation allows async restoration without
    /// requiring an async constructor.
    pub fn new(repo: Arc<R>, session_id: SessionId) -> Self {
        Self {
            repo,
            session_id,
            buffer: Arc::new(RwLock::new(Vec::new())),
            last_flushed_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Restore all persisted trajectory events for this session from the database.
    ///
    /// Call this immediately after `new()` to populate the in-memory buffer
    /// with events that survived daemon restarts.
    pub async fn restore(&self) -> RepositoryResult<usize> {
        let events = self.repo.get_trajectory_events(self.session_id).await?;
        let count = events.len();

        let mut buffer = self.buffer.write();
        buffer.extend(events);

        // Mark all restored events as already flushed
        self.last_flushed_index.store(count, std::sync::atomic::Ordering::Release);

        Ok(count)
    }

    /// Record a learning trigger as a trajectory event.
    ///
    /// **Hot path** -- defers step generation to query time.
    /// Only generates a UUID, captures the timestamp, and pushes
    /// raw trigger data into the buffer.
    pub async fn record(&self, trigger: LearningTrigger) -> RepositoryResult<TrajectoryEventId> {
        let id = TrajectoryEventId::new();
        let now = Utc::now();

        let raw = RawEvent {
            id,
            session_id: self.session_id,
            trigger_kind: trigger.kind,
            agent_id: trigger.agent_id,
            task_id: trigger.task_id,
            success: trigger.success,
            summary: trigger.summary,
            knowledge_kind: trigger.knowledge_kind,
            project_name: trigger.project_name,
            tasks_completed: trigger.tasks_completed,
            tasks_failed: trigger.tasks_failed,
            created_at: now,
        };

        self.buffer.write().push(raw);
        Ok(id)
    }

    /// Get a trajectory event by ID.
    ///
    /// Materializes steps lazily from the raw buffer entry.
    pub async fn get_event(
        &self,
        id: TrajectoryEventId,
    ) -> RepositoryResult<TrajectoryEvent> {
        let buffer = self.buffer.read();
        buffer
            .iter()
            .find(|e| e.id == id)
            .map(|raw| raw.materialize())
            .ok_or_else(|| RepositoryError::NotFound {
                entity_type: "TrajectoryEvent".to_string(),
                id: id.to_string(),
            })
    }

    /// Query trajectory events with filters.
    ///
    /// Materializes steps lazily for matching events only.
    pub async fn query(
        &self,
        query: TrajectoryQuery,
    ) -> RepositoryResult<Vec<TrajectoryEvent>> {
        let buffer = self.buffer.read();
        let results: Vec<TrajectoryEvent> = buffer
            .iter()
            .filter(|e| {
                if let Some(agent_id) = query.agent_id
                    && e.agent_id != agent_id
                {
                    return false;
                }
                if let Some(kind) = query.trigger_kind
                    && e.trigger_kind != kind
                {
                    return false;
                }
                if let Some(success) = query.success_filter
                    && e.success != success
                {
                    return false;
                }
                true
            })
            .take(query.limit)
            .map(|raw| raw.materialize())
            .collect();

        Ok(results)
    }

    /// Flush buffered events to persistent storage.
    ///
    /// Only persists events added since the last flush, using last_flushed_index
    /// to track progress. This is O(new events) instead of O(total events).
    pub async fn flush(&self) -> RepositoryResult<()> {
        // Clone the events we need to flush (while holding lock briefly)
        let (events_to_flush, end_index) = {
            let buffer = self.buffer.read();
            let start_index = self.last_flushed_index.load(std::sync::atomic::Ordering::Acquire);
            let events: Vec<RawEvent> = buffer.iter().skip(start_index).cloned().collect();
            let end = buffer.len();
            (events, end)
        }; // Lock is dropped here

        // Persist without holding the lock
        for raw in &events_to_flush {
            self.repo.create_trajectory_event(raw).await?;
        }

        // Update the last flushed index (only if we successfully persisted all)
        self.last_flushed_index.store(end_index, std::sync::atomic::Ordering::Release);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trajectory_event_id_display() {
        let id = TrajectoryEventId::new();
        let display = id.to_string();
        assert!(display.starts_with("traj-"));
        assert_eq!(display.len(), 13); // "traj-" + 8 chars
    }

    #[test]
    fn trigger_kind_serde_roundtrip() {
        let kinds = vec![
            TriggerKind::TaskComplete,
            TriggerKind::KnowledgeShare,
            TriggerKind::ProjectClose,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let deserialized: TriggerKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, deserialized);
        }
    }

    #[test]
    fn learning_trigger_builder() {
        let agent = AgentId::new();
        let task = TaskId::new();
        let trigger = LearningTrigger::new(TriggerKind::TaskComplete, agent)
            .with_task_id(task)
            .with_success(true)
            .with_summary("test summary");

        assert_eq!(trigger.kind, TriggerKind::TaskComplete);
        assert_eq!(trigger.agent_id, agent);
        assert_eq!(trigger.task_id, Some(task));
        assert!(trigger.success);
        assert_eq!(trigger.summary, "test summary");
    }

    #[test]
    fn generate_task_outcome_steps() {
        let agent = AgentId::new();
        let task = TaskId::new();
        let trigger = LearningTrigger::new(TriggerKind::TaskComplete, agent)
            .with_task_id(task)
            .with_success(true)
            .with_summary("did a thing");
        let now = Utc::now();
        let steps = generate_steps(&trigger, &now);

        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].kind(), "task_outcome");
        assert!(steps[0].payload().contains_key("task_id"));
        assert!(steps[0].payload().contains_key("success"));
        assert!(steps[0].payload().contains_key("timestamp"));
        assert!(steps[0].payload().contains_key("agent_id"));
    }

    #[test]
    fn generate_knowledge_acquired_steps() {
        let agent = AgentId::new();
        let trigger = LearningTrigger::new(TriggerKind::KnowledgeShare, agent)
            .with_knowledge_kind(KnowledgeKind::Discovery)
            .with_summary("found a thing");
        let now = Utc::now();
        let steps = generate_steps(&trigger, &now);

        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].kind(), "knowledge_acquired");
        assert_eq!(
            steps[0].payload().get("knowledge_kind").unwrap().as_str().unwrap(),
            "discovery"
        );
    }

    #[test]
    fn generate_project_consolidation_steps() {
        let agent = AgentId::new();
        let trigger = LearningTrigger::new(TriggerKind::ProjectClose, agent)
            .with_project_name("my-project")
            .with_stats(5, 2);
        let now = Utc::now();
        let steps = generate_steps(&trigger, &now);

        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].kind(), "project_consolidation");
        assert_eq!(
            steps[0].payload().get("project_name").unwrap().as_str().unwrap(),
            "my-project"
        );
        assert_eq!(
            steps[0].payload().get("tasks_completed").unwrap().as_u64().unwrap(),
            5
        );
        assert_eq!(
            steps[0].payload().get("tasks_failed").unwrap().as_u64().unwrap(),
            2
        );
    }
}
