//! Integration tests for SONA Trajectory recording and trigger system.
//!
//! The Trajectory system records learning events as they happen in the hive:
//! - Task completions trigger experience recording (what worked, what failed)
//! - Knowledge shares trigger learning trajectories (discoveries, decisions)
//! - Project closures trigger consolidation (aggregate lessons learned)
//!
//! Events are buffered in a lock-free ring buffer for minimal overhead (<100ns
//! per step), then flushed to AletheiaDB asynchronously. This is the event-driven
//! spine that feeds MicroLoRA, BaseLoRA, and EWC++ learning loops.
//!
//! These tests define the desired API (RED phase) -- they will FAIL until
//! the Trajectory system is implemented.

use std::sync::Arc;
use std::time::Instant;

use harness_persistence::{
    AgentId, InMemoryRepository, KnowledgeKind, Repository, Session, TaskId,
};

use harness_persistence::{LearningTrigger, TrajectoryQuery, TrajectoryRecorder, TriggerKind};

/// Helper: create a fresh InMemoryRepository with a session.
async fn setup_repo() -> (Arc<InMemoryRepository>, Session) {
    let repo = Arc::new(InMemoryRepository::new());
    let session = Session::new(8);
    repo.create_session(&session).await.unwrap();
    (repo, session)
}

/// Helper: create a simulated task completion event.
fn task_complete_trigger(
    task_id: TaskId,
    agent_id: AgentId,
    success: bool,
    summary: &str,
) -> LearningTrigger {
    LearningTrigger::new(TriggerKind::TaskComplete, agent_id)
        .with_task_id(task_id)
        .with_success(success)
        .with_summary(summary)
}

/// Helper: create a simulated knowledge share event.
fn knowledge_share_trigger(
    agent_id: AgentId,
    kind: KnowledgeKind,
    content: &str,
) -> LearningTrigger {
    LearningTrigger::new(TriggerKind::KnowledgeShare, agent_id)
        .with_knowledge_kind(kind)
        .with_summary(content)
}

/// Helper: create a simulated project closure event.
fn project_close_trigger(
    agent_id: AgentId,
    project_name: &str,
    tasks_completed: usize,
    tasks_failed: usize,
) -> LearningTrigger {
    LearningTrigger::new(TriggerKind::ProjectClose, agent_id)
        .with_project_name(project_name)
        .with_stats(tasks_completed, tasks_failed)
}

// ---------------------------------------------------------------------------
// Test 1: Task completion triggers trajectory recording
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_record_task_complete() {
    let (repo, session) = setup_repo().await;
    let recorder = TrajectoryRecorder::new(repo.clone(), session.id);

    let agent_id = AgentId::new();
    let task_id = TaskId::new();

    // Fire a task completion trigger
    let trigger = task_complete_trigger(
        task_id,
        agent_id,
        true,
        "Implemented JWT authentication with refresh token rotation",
    );

    let event_id = recorder.record(trigger).await.unwrap();

    // Verify the event was recorded
    let event = recorder.get_event(event_id).await.unwrap();
    assert_eq!(event.trigger_kind(), TriggerKind::TaskComplete);
    assert_eq!(event.agent_id(), agent_id);
    assert_eq!(event.task_id(), Some(task_id));
    assert!(event.success());
    assert!(event.summary().contains("JWT authentication"));
    assert!(event.created_at() <= chrono::Utc::now());

    // The event should generate trajectory steps for downstream consumers
    let steps = event.steps();
    assert!(
        !steps.is_empty(),
        "Task completion should generate at least one trajectory step"
    );

    // First step should be a "task_outcome" step
    let first_step = &steps[0];
    assert_eq!(first_step.kind(), "task_outcome");
    assert_eq!(first_step.agent_id(), agent_id);
    assert!(first_step.payload().contains_key("task_id"));
    assert!(first_step.payload().contains_key("success"));
}

// ---------------------------------------------------------------------------
// Test 2: Knowledge sharing triggers learning trajectory
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_record_knowledge_share() {
    let (repo, session) = setup_repo().await;
    let recorder = TrajectoryRecorder::new(repo.clone(), session.id);

    let agent_id = AgentId::new();

    // Share a discovery
    let discovery_trigger = knowledge_share_trigger(
        agent_id,
        KnowledgeKind::Discovery,
        "Found that connection pooling with max_idle=10 reduces latency by 40%",
    );
    let discovery_id = recorder.record(discovery_trigger).await.unwrap();

    // Share a decision
    let decision_trigger = knowledge_share_trigger(
        agent_id,
        KnowledgeKind::Decision,
        "Using B-tree indexes instead of hash indexes for range query support",
    );
    let decision_id = recorder.record(decision_trigger).await.unwrap();

    // Verify discovery event
    let discovery_event = recorder.get_event(discovery_id).await.unwrap();
    assert_eq!(discovery_event.trigger_kind(), TriggerKind::KnowledgeShare);
    assert_eq!(discovery_event.agent_id(), agent_id);

    let discovery_steps = discovery_event.steps();
    assert!(!discovery_steps.is_empty());
    let step = &discovery_steps[0];
    assert_eq!(step.kind(), "knowledge_acquired");
    assert!(step.payload().contains_key("knowledge_kind"));
    assert_eq!(
        step.payload()
            .get("knowledge_kind")
            .unwrap()
            .as_str()
            .unwrap(),
        "discovery"
    );

    // Verify decision event
    let decision_event = recorder.get_event(decision_id).await.unwrap();
    let decision_steps = decision_event.steps();
    assert!(!decision_steps.is_empty());
    assert_eq!(
        decision_steps[0]
            .payload()
            .get("knowledge_kind")
            .unwrap()
            .as_str()
            .unwrap(),
        "decision"
    );

    // Query all events for this agent
    let agent_events = recorder
        .query(TrajectoryQuery::new().with_agent(agent_id).with_limit(10))
        .await
        .unwrap();
    assert_eq!(
        agent_events.len(),
        2,
        "Agent should have 2 trajectory events"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Project closure triggers consolidation trajectory
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_record_project_close() {
    let (repo, session) = setup_repo().await;
    let recorder = TrajectoryRecorder::new(repo.clone(), session.id);

    let agent_id = AgentId::new();

    // Record several task completions first (building up trajectory)
    for i in 0..5 {
        let trigger = task_complete_trigger(
            TaskId::new(),
            agent_id,
            i != 3, // Task 3 fails
            &format!("Task {} of the authentication project", i),
        );
        recorder.record(trigger).await.unwrap();
    }

    // Now close the project
    let close_trigger = project_close_trigger(
        agent_id,
        "authentication-module",
        4, // 4 completed
        1, // 1 failed
    );
    let close_id = recorder.record(close_trigger).await.unwrap();

    // Verify the consolidation event
    let close_event = recorder.get_event(close_id).await.unwrap();
    assert_eq!(close_event.trigger_kind(), TriggerKind::ProjectClose);

    let steps = close_event.steps();
    assert!(!steps.is_empty());

    // Should have a "project_consolidation" step
    let consolidation_step = steps
        .iter()
        .find(|s| s.kind() == "project_consolidation")
        .expect("Should have a project_consolidation step");

    assert!(consolidation_step.payload().contains_key("project_name"));
    assert_eq!(
        consolidation_step
            .payload()
            .get("project_name")
            .unwrap()
            .as_str()
            .unwrap(),
        "authentication-module"
    );
    assert_eq!(
        consolidation_step
            .payload()
            .get("tasks_completed")
            .unwrap()
            .as_u64()
            .unwrap(),
        4
    );
    assert_eq!(
        consolidation_step
            .payload()
            .get("tasks_failed")
            .unwrap()
            .as_u64()
            .unwrap(),
        1
    );

    // Query by trigger kind
    let project_events = recorder
        .query(
            TrajectoryQuery::new()
                .with_trigger_kind(TriggerKind::ProjectClose)
                .with_limit(10),
        )
        .await
        .unwrap();
    assert_eq!(project_events.len(), 1);

    // Query ALL events for this agent -- should include tasks + project close
    let all_events = recorder
        .query(TrajectoryQuery::new().with_agent(agent_id).with_limit(100))
        .await
        .unwrap();
    assert_eq!(
        all_events.len(),
        6,
        "Should have 5 task events + 1 project close event"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Lock-free buffering under concurrent writes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_lock_free_buffering() {
    let (repo, session) = setup_repo().await;
    let recorder = Arc::new(TrajectoryRecorder::new(repo.clone(), session.id));

    let num_writers = 20;
    let events_per_writer = 50;
    let total_expected = num_writers * events_per_writer;

    // Spawn concurrent writers, each recording multiple events
    let mut handles = vec![];
    for writer_idx in 0..num_writers {
        let recorder_clone = recorder.clone();
        let handle = tokio::spawn(async move {
            let agent_id = AgentId::new();
            let mut event_ids = Vec::new();

            for event_idx in 0..events_per_writer {
                let trigger = LearningTrigger::new(TriggerKind::TaskComplete, agent_id)
                    .with_task_id(TaskId::new())
                    .with_success(true)
                    .with_summary(&format!(
                        "Writer {} event {} - concurrent buffering test",
                        writer_idx, event_idx
                    ));

                match recorder_clone.record(trigger).await {
                    Ok(id) => event_ids.push(id),
                    Err(e) => panic!("Writer {} event {} failed: {:?}", writer_idx, event_idx, e),
                }
            }

            event_ids
        });
        handles.push(handle);
    }

    // Collect all results
    let results = futures::future::join_all(handles).await;
    let mut all_event_ids = Vec::new();
    for result in results {
        let ids = result.expect("writer task should not panic");
        all_event_ids.extend(ids);
    }

    // All events should have been recorded
    assert_eq!(
        all_event_ids.len(),
        total_expected,
        "All {} events should be recorded, got {}",
        total_expected,
        all_event_ids.len()
    );

    // No duplicate event IDs
    let unique_count = {
        let mut sorted = all_event_ids.clone();
        sorted.sort();
        sorted.dedup();
        sorted.len()
    };
    assert_eq!(
        unique_count, total_expected,
        "All event IDs should be unique (no duplicates)"
    );

    // Flush the buffer and verify all events are queryable
    recorder.flush().await.unwrap();

    let all_events = recorder
        .query(TrajectoryQuery::new().with_limit(total_expected + 10))
        .await
        .unwrap();
    assert_eq!(
        all_events.len(),
        total_expected,
        "All {} events should be queryable after flush",
        total_expected
    );
}

// ---------------------------------------------------------------------------
// Test 5: Performance overhead - CRITICAL: <= 100ns per step!
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_performance_overhead() {
    let (repo, session) = setup_repo().await;
    let recorder = TrajectoryRecorder::new(repo.clone(), session.id);

    let agent_id = AgentId::new();

    // Warm up (exclude from timing)
    for _ in 0..100 {
        let trigger = LearningTrigger::new(TriggerKind::TaskComplete, agent_id)
            .with_task_id(TaskId::new())
            .with_success(true)
            .with_summary("warmup event");
        recorder.record(trigger).await.unwrap();
    }

    // Benchmark: record N events and measure average time per step
    let num_iterations = 10_000;
    let start = Instant::now();

    for i in 0..num_iterations {
        let trigger = LearningTrigger::new(TriggerKind::TaskComplete, agent_id)
            .with_task_id(TaskId::new())
            .with_success(i % 7 != 0) // some failures for variety
            .with_summary("benchmark step for trajectory recording");
        recorder.record(trigger).await.unwrap();
    }

    let elapsed = start.elapsed();
    let per_step_ns = elapsed.as_nanos() / num_iterations as u128;

    // PERFORMANCE REQUIREMENT: <= 1000ns (1μs) per step in debug builds
    // In release builds with optimizations, this should be <100ns.
    // This ensures the trajectory system doesn't add measurable overhead
    // to normal hive operations. The lock-free ring buffer should keep
    // the hot path extremely fast, with async flush handling persistence.
    const MAX_NS_PER_STEP: u128 = if cfg!(debug_assertions) { 1000 } else { 100 };
    assert!(
        per_step_ns <= MAX_NS_PER_STEP,
        "PERFORMANCE REGRESSION: Trajectory recording took {}ns per step \
         (budget: {}ns). The lock-free buffer path must stay under {}ns. \
         Elapsed: {:?} for {} iterations.",
        per_step_ns,
        MAX_NS_PER_STEP,
        MAX_NS_PER_STEP,
        elapsed,
        num_iterations
    );

    // Also verify we can flush all events without data loss
    recorder.flush().await.unwrap();
    let total = recorder
        .query(
            TrajectoryQuery::new()
                .with_agent(agent_id)
                .with_limit(num_iterations + 200),
        )
        .await
        .unwrap();

    // 100 warmup + num_iterations benchmark events
    assert_eq!(
        total.len(),
        100 + num_iterations,
        "All events (warmup + benchmark) should be present after flush"
    );
}

// ---------------------------------------------------------------------------
// Test 6: Query trajectories by trigger kind, agent, and time range
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_query_trajectories() {
    let (repo, session) = setup_repo().await;
    let recorder = TrajectoryRecorder::new(repo.clone(), session.id);

    let agent_a = AgentId::new();
    let agent_b = AgentId::new();

    // Record events from agent A
    for _ in 0..3 {
        let trigger = task_complete_trigger(TaskId::new(), agent_a, true, "Agent A task");
        recorder.record(trigger).await.unwrap();
    }

    // Record events from agent B
    for _ in 0..2 {
        let trigger =
            knowledge_share_trigger(agent_b, KnowledgeKind::Discovery, "Agent B discovery");
        recorder.record(trigger).await.unwrap();
    }

    // Record a project close from agent A
    let trigger = project_close_trigger(agent_a, "test-project", 3, 0);
    recorder.record(trigger).await.unwrap();

    // Query by agent A only
    let agent_a_events = recorder
        .query(TrajectoryQuery::new().with_agent(agent_a).with_limit(10))
        .await
        .unwrap();
    assert_eq!(
        agent_a_events.len(),
        4,
        "Agent A: 3 tasks + 1 project close"
    );

    // Query by agent B only
    let agent_b_events = recorder
        .query(TrajectoryQuery::new().with_agent(agent_b).with_limit(10))
        .await
        .unwrap();
    assert_eq!(agent_b_events.len(), 2, "Agent B: 2 knowledge shares");

    // Query by trigger kind: TaskComplete
    let task_events = recorder
        .query(
            TrajectoryQuery::new()
                .with_trigger_kind(TriggerKind::TaskComplete)
                .with_limit(10),
        )
        .await
        .unwrap();
    assert_eq!(task_events.len(), 3, "3 task completion events total");

    // Query by trigger kind: KnowledgeShare
    let knowledge_events = recorder
        .query(
            TrajectoryQuery::new()
                .with_trigger_kind(TriggerKind::KnowledgeShare)
                .with_limit(10),
        )
        .await
        .unwrap();
    assert_eq!(knowledge_events.len(), 2, "2 knowledge share events total");

    // Query all with no filters
    let all_events = recorder
        .query(TrajectoryQuery::new().with_limit(100))
        .await
        .unwrap();
    assert_eq!(all_events.len(), 6, "6 total events across both agents");
}

// ---------------------------------------------------------------------------
// Test 7: Trajectory steps carry typed payloads for downstream consumers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_trajectory_step_payloads() {
    let (repo, session) = setup_repo().await;
    let recorder = TrajectoryRecorder::new(repo.clone(), session.id);

    let agent_id = AgentId::new();
    let task_id = TaskId::new();

    // Record a task completion
    let trigger = task_complete_trigger(
        task_id,
        agent_id,
        true,
        "Implemented connection pooling with max_idle=10",
    );
    let event_id = recorder.record(trigger).await.unwrap();

    let event = recorder.get_event(event_id).await.unwrap();
    let steps = event.steps();

    // Each step should have a typed payload as serde_json::Map
    for step in steps {
        let payload = step.payload();

        // Every step must have a timestamp
        assert!(
            payload.contains_key("timestamp"),
            "Step '{}' should have a timestamp in payload",
            step.kind()
        );

        // Every step must have the agent_id
        assert!(
            payload.contains_key("agent_id"),
            "Step '{}' should have agent_id in payload",
            step.kind()
        );
    }

    // The task_outcome step should carry task-specific data
    let task_step = steps
        .iter()
        .find(|s| s.kind() == "task_outcome")
        .expect("Should have task_outcome step");

    let payload = task_step.payload();
    assert_eq!(
        payload.get("task_id").unwrap().as_str().unwrap(),
        task_id.as_uuid().to_string()
    );
    assert!(payload.get("success").unwrap().as_bool().unwrap());
    assert!(
        payload
            .get("summary")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("connection pooling")
    );
}

// ---------------------------------------------------------------------------
// Test 8: Empty recorder handles queries gracefully
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_empty_recorder() {
    let (repo, session) = setup_repo().await;
    let recorder = TrajectoryRecorder::new(repo.clone(), session.id);

    // Query on empty recorder should return empty, not error
    let results = recorder
        .query(TrajectoryQuery::new().with_limit(10))
        .await
        .unwrap();
    assert!(
        results.is_empty(),
        "Empty recorder should return empty results"
    );

    // Flush on empty recorder should be a no-op, not error
    recorder.flush().await.unwrap();

    // Getting a non-existent event should return an error
    let fake_id = harness_persistence::TrajectoryEventId::new();
    let result = recorder.get_event(fake_id).await;
    assert!(
        result.is_err(),
        "Getting non-existent event should return error"
    );
}

// ---------------------------------------------------------------------------
// Test 9: Events persist across recorder instances (AletheiaDB durability)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_trajectory_persistence() {
    let (repo, session) = setup_repo().await;

    let agent_id = AgentId::new();

    // Record events with first recorder instance
    {
        let recorder = TrajectoryRecorder::new(repo.clone(), session.id);

        let trigger = task_complete_trigger(
            TaskId::new(),
            agent_id,
            true,
            "Implemented rate limiting middleware",
        );
        recorder.record(trigger).await.unwrap();

        let trigger = knowledge_share_trigger(
            agent_id,
            KnowledgeKind::Decision,
            "Chose token bucket over leaky bucket for bursty traffic",
        );
        recorder.record(trigger).await.unwrap();

        // Flush to ensure persistence
        recorder.flush().await.unwrap();
    }
    // First recorder dropped

    // Create a NEW recorder on the same repo (simulates restart)
    let recorder2 = TrajectoryRecorder::new(repo.clone(), session.id);

    // Restore persisted events from the database
    recorder2.restore().await.unwrap();

    // Events from the first recorder should be visible
    let events = recorder2
        .query(TrajectoryQuery::new().with_agent(agent_id).with_limit(10))
        .await
        .unwrap();

    assert_eq!(
        events.len(),
        2,
        "Both events should survive across recorder instances"
    );

    // Verify content survived
    let summaries: Vec<&str> = events.iter().map(|e| e.summary()).collect();
    assert!(
        summaries.iter().any(|s| s.contains("rate limiting")),
        "Rate limiting event should persist"
    );
    assert!(
        summaries.iter().any(|s| s.contains("token bucket")),
        "Token bucket decision should persist"
    );
}

// ---------------------------------------------------------------------------
// Test 10: Failed task triggers generate failure trajectory steps
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_failure_trajectory() {
    let (repo, session) = setup_repo().await;
    let recorder = TrajectoryRecorder::new(repo.clone(), session.id);

    let agent_id = AgentId::new();
    let task_id = TaskId::new();

    // Record a FAILED task completion
    let trigger = task_complete_trigger(
        task_id,
        agent_id,
        false,
        "Failed to implement WebSocket real-time sync: missing protocol spec",
    );
    let event_id = recorder.record(trigger).await.unwrap();

    let event = recorder.get_event(event_id).await.unwrap();
    assert!(!event.success(), "Event should record failure");

    // Failed tasks should still generate trajectory steps
    let steps = event.steps();
    assert!(!steps.is_empty());

    let task_step = steps
        .iter()
        .find(|s| s.kind() == "task_outcome")
        .expect("Failed task should still have task_outcome step");

    assert!(
        !task_step
            .payload()
            .get("success")
            .unwrap()
            .as_bool()
            .unwrap()
    );

    // Query only failed events
    let failed_events = recorder
        .query(
            TrajectoryQuery::new()
                .with_success_filter(false)
                .with_limit(10),
        )
        .await
        .unwrap();
    assert_eq!(failed_events.len(), 1, "Should find 1 failed event");
}
