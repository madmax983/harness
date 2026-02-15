//! Benchmark comparing HNSW vs word-overlap similarity search performance.
//!
//! Run with: cargo bench --bench reasoning_bank_hnsw

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::sync::Arc;

use harness_persistence::{AgentRole, PatternQuery, PatternStore, ReasoningBank, TaskPattern};

/// Create sample patterns for benchmarking
fn create_sample_patterns(count: usize) -> Vec<TaskPattern> {
    let descriptions = vec![
        "Implemented JWT authentication with OAuth2 providers",
        "Added rate limiting using token bucket algorithm",
        "Built user registration endpoint with email verification",
        "Designed database schema for RBAC permissions",
        "Created integration test suite for REST API",
        "Implemented WebSocket real-time notifications",
        "Added password reset flow with secure tokens",
        "Built caching layer with Redis integration",
        "Designed event sourcing architecture for audit trail",
        "Implemented GraphQL API with DataLoader batching",
    ];

    let task_types = vec![
        "implement_feature",
        "design_schema",
        "write_tests",
        "code_review",
        "optimize_performance",
    ];

    (0..count)
        .map(|i| {
            let desc = descriptions[i % descriptions.len()];
            let task_type = task_types[i % task_types.len()];
            TaskPattern::new(task_type, AgentRole::Developer, true, desc)
        })
        .collect()
}

/// Benchmark HNSW search performance
fn bench_hnsw_search(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("pattern_search");

    // Test with different dataset sizes
    for size in [10, 100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::new("hnsw", size), size, |b, &size| {
            let (store, session_id) = setup_bank_with_patterns(size);
            let bank = ReasoningBank::new(store, session_id);
            let query = PatternQuery::new("How to implement authentication?")
                .with_task_type("implement_feature")
                .with_limit(10);

            b.to_async(&runtime).iter(|| async {
                let results = bank.find_similar(query.clone()).await.unwrap();
                black_box(results);
            });
        });
    }

    group.finish();
}

/// Setup helper: create a bank with N patterns
fn setup_bank_with_patterns(count: usize) -> (Arc<PatternStore>, harness_persistence::SessionId) {
    let store = Arc::new(PatternStore::new());
    let session_id = harness_persistence::SessionId::new();
    let bank = ReasoningBank::new(store.clone(), session_id);

    let patterns = create_sample_patterns(count);
    for pattern in patterns {
        let _ = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(bank.store_pattern(pattern));
    }

    (store, session_id)
}

/// Benchmark concurrent searches
fn bench_concurrent_search(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("concurrent_search_10_threads", |b| {
        let (store, session_id) = setup_bank_with_patterns(1000);
        let bank = Arc::new(ReasoningBank::new(store, session_id));

        b.to_async(&runtime).iter(|| async {
            let mut handles = vec![];
            for i in 0..10 {
                let bank_clone = bank.clone();
                let handle = tokio::spawn(async move {
                    let query =
                        PatternQuery::new(&format!("authentication query {}", i)).with_limit(5);
                    bank_clone.find_similar(query).await.unwrap()
                });
                handles.push(handle);
            }

            let results = futures::future::join_all(handles).await;
            black_box(results);
        });
    });
}

/// Benchmark pattern storage with indexing
fn bench_pattern_storage(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("store_pattern_with_hnsw_indexing", |b| {
        let store = Arc::new(PatternStore::new());
        let session_id = harness_persistence::SessionId::new();
        let bank = ReasoningBank::new(store, session_id);

        let pattern = TaskPattern::new(
            "implement_feature",
            AgentRole::Developer,
            true,
            "Implemented JWT authentication with OAuth2 providers and refresh tokens",
        );

        b.to_async(&runtime).iter(|| async {
            let result = bank.store_pattern(pattern.clone()).await;
            black_box(result);
        });
    });
}

criterion_group!(
    benches,
    bench_hnsw_search,
    bench_concurrent_search,
    bench_pattern_storage
);
criterion_main!(benches);
