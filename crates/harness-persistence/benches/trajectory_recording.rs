//! Criterion benchmarks for trajectory recording and pattern storage.
//!
//! Target: <100ns overhead for trajectory recording (hot path).
//!
//! Benchmarks:
//! 1. record_single_action - Verify <100ns target for trajectory recording
//! 2. store_pattern - Baseline measurement for pattern storage
//! 3. find_similar_10 - Pattern search with 10 patterns
//! 4. find_similar_100 - Pattern search with 100 patterns
//! 5. find_similar_1000 - Pattern search with 1000 patterns

use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use harness_persistence::{
    AgentId, AgentRole, InMemoryRepository, LearningTrigger, PatternQuery, PatternStore,
    ReasoningBank, SessionId, TaskPattern, TrajectoryRecorder, TriggerKind,
};

// ---------------------------------------------------------------------------
// Helper: Create test repository
// ---------------------------------------------------------------------------

fn create_test_repo() -> Arc<InMemoryRepository> {
    Arc::new(InMemoryRepository::new())
}

// ---------------------------------------------------------------------------
// Benchmark 1: record_single_action (TARGET: <100ns)
// ---------------------------------------------------------------------------

fn bench_record_single_action(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = create_test_repo();
    let session_id = SessionId::new();
    let recorder = TrajectoryRecorder::new(repo, session_id);
    let agent_id = AgentId::new();

    c.bench_function("record_single_action", |b| {
        b.to_async(&rt).iter(|| async {
            let trigger = LearningTrigger::new(TriggerKind::TaskComplete, agent_id)
                .with_success(true)
                .with_summary("completed task");

            let result = recorder.record(black_box(trigger)).await;
            black_box(result)
        });
    });
}

// ---------------------------------------------------------------------------
// Benchmark 2: store_pattern (baseline measurement)
// ---------------------------------------------------------------------------

fn bench_store_pattern(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let store = Arc::new(PatternStore::new());
    let session_id = SessionId::new();
    let bank = ReasoningBank::new(store, session_id);

    c.bench_function("store_pattern", |b| {
        b.to_async(&rt).iter(|| async {
            let pattern = TaskPattern::new(
                "test_task",
                AgentRole::Developer,
                true,
                "test description",
            );

            let result = bank.store_pattern(black_box(pattern)).await;
            black_box(result)
        });
    });
}

// ---------------------------------------------------------------------------
// Benchmark 3-5: find_similar with varying pattern counts
// ---------------------------------------------------------------------------

async fn populate_patterns(bank: &ReasoningBank, count: usize) {
    for i in 0..count {
        let pattern = TaskPattern::new(
            "test_task",
            AgentRole::Developer,
            true,
            format!("test description number {}", i),
        );
        bank.store_pattern(pattern).await.unwrap();
    }
}

fn bench_find_similar_10(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let store = Arc::new(PatternStore::new());
    let session_id = SessionId::new();
    let bank = ReasoningBank::new(store, session_id);

    rt.block_on(populate_patterns(&bank, 10));

    c.bench_function("find_similar_10_patterns", |b| {
        b.to_async(&rt).iter(|| async {
            let query = PatternQuery::new("test description number 5").with_limit(5);

            let result = bank.find_similar(black_box(query)).await;
            black_box(result)
        });
    });
}

fn bench_find_similar_100(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let store = Arc::new(PatternStore::new());
    let session_id = SessionId::new();
    let bank = ReasoningBank::new(store, session_id);

    rt.block_on(populate_patterns(&bank, 100));

    c.bench_function("find_similar_100_patterns", |b| {
        b.to_async(&rt).iter(|| async {
            let query = PatternQuery::new("test description number 50").with_limit(10);

            let result = bank.find_similar(black_box(query)).await;
            black_box(result)
        });
    });
}

fn bench_find_similar_1000(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let store = Arc::new(PatternStore::new());
    let session_id = SessionId::new();
    let bank = ReasoningBank::new(store, session_id);

    rt.block_on(populate_patterns(&bank, 1000));

    c.bench_function("find_similar_1000_patterns", |b| {
        b.to_async(&rt).iter(|| async {
            let query = PatternQuery::new("test description number 500").with_limit(10);

            let result = bank.find_similar(black_box(query)).await;
            black_box(result)
        });
    });
}

// ---------------------------------------------------------------------------
// Benchmark 6: Trajectory event materialization (lazy step generation)
// ---------------------------------------------------------------------------

fn bench_event_materialization(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = create_test_repo();
    let session_id = SessionId::new();
    let recorder = TrajectoryRecorder::new(repo, session_id);
    let agent_id = AgentId::new();

    // Pre-record some events
    let event_id = rt.block_on(async {
        let trigger = LearningTrigger::new(TriggerKind::TaskComplete, agent_id)
            .with_success(true)
            .with_summary("completed task for materialization test");
        recorder.record(trigger).await.unwrap()
    });

    c.bench_function("event_materialization", |b| {
        b.to_async(&rt).iter(|| async {
            let result = recorder.get_event(black_box(event_id)).await;
            black_box(result)
        });
    });
}

// ---------------------------------------------------------------------------
// Benchmark 7: Trajectory query with filters
// ---------------------------------------------------------------------------

async fn populate_trajectory_events(recorder: &TrajectoryRecorder<InMemoryRepository>, count: usize) {
    let agent_id = AgentId::new();
    for i in 0..count {
        let kind = match i % 3 {
            0 => TriggerKind::TaskComplete,
            1 => TriggerKind::KnowledgeShare,
            _ => TriggerKind::ProjectClose,
        };
        let summary = format!("event {}", i);
        let trigger = LearningTrigger::new(kind, agent_id)
            .with_success(i % 2 == 0)
            .with_summary(&summary);
        recorder.record(trigger).await.unwrap();
    }
}

fn bench_trajectory_query(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = create_test_repo();
    let session_id = SessionId::new();
    let recorder = TrajectoryRecorder::new(repo, session_id);

    rt.block_on(populate_trajectory_events(&recorder, 100));

    c.bench_function("trajectory_query_filtered", |b| {
        b.to_async(&rt).iter(|| async {
            use harness_persistence::TrajectoryQuery;
            let query = TrajectoryQuery::new()
                .with_trigger_kind(TriggerKind::TaskComplete)
                .with_success_filter(true)
                .with_limit(10);

            let result = recorder.query(black_box(query)).await;
            black_box(result)
        });
    });
}

// ---------------------------------------------------------------------------
// Benchmark 8: Hot path microbenchmark (RawEvent creation + buffer push)
// ---------------------------------------------------------------------------

fn bench_hot_path_microbenchmark(c: &mut Criterion) {
    use parking_lot::RwLock;
    use chrono::Utc;
    use harness_persistence::{RawEvent, TrajectoryEventId};

    let buffer = Arc::new(RwLock::new(Vec::<RawEvent>::with_capacity(10000)));
    let agent_id = AgentId::new();

    c.bench_function("hot_path_microbenchmark", |b| {
        b.iter(|| {
            // This is what record() actually does in the hot path:
            // 1. Generate ID (~10-20ns)
            let id = black_box(TrajectoryEventId::new());
            // 2. Capture timestamp (~5-10ns on modern CPUs with VDSO)
            let now = black_box(Utc::now());

            // 3. Create RawEvent (stack allocation, just moves)
            let raw = RawEvent {
                id,
                trigger_kind: TriggerKind::TaskComplete,
                agent_id,
                task_id: None,
                success: true,
                summary: String::from("test"),
                knowledge_kind: None,
                project_name: None,
                tasks_completed: 0,
                tasks_failed: 0,
                created_at: now,
            };

            // 4. Lock + push (~15-20ns for parking_lot RwLock uncontended)
            black_box(buffer.write().push(raw));
        });
    });
}

// ---------------------------------------------------------------------------
// Benchmark Group Configuration
// ---------------------------------------------------------------------------

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(std::time::Duration::from_millis(500))
        .measurement_time(std::time::Duration::from_secs(2))
        .sample_size(1000);
    targets =
        bench_record_single_action,
        bench_hot_path_microbenchmark,
        bench_store_pattern,
        bench_find_similar_10,
        bench_find_similar_100,
        bench_find_similar_1000,
        bench_event_materialization,
        bench_trajectory_query
}

criterion_main!(benches);
