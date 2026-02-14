//! Standalone test for HNSW-powered ReasoningBank
//!
//! Run with: rustc --edition 2024 test_reasoning_bank.rs && ./test_reasoning_bank
//! Or: cargo script test_reasoning_bank.rs

use std::sync::Arc;

#[path = "crates/harness-persistence/src/reasoning_bank.rs"]
mod reasoning_bank;

#[path = "crates/harness-persistence/src/types.rs"]
mod types;

use reasoning_bank::{PatternStore, ReasoningBank, TaskPattern, PatternQuery};
use types::{AgentRole, SessionId};

#[tokio::main]
async fn main() {
    println!("Testing HNSW-powered ReasoningBank...\n");

    // Test 1: Store and retrieve patterns
    println!("Test 1: Store and retrieve patterns");
    let (store, session_id) = setup_bank();
    let bank = ReasoningBank::new(store.clone(), session_id);

    let pattern = TaskPattern::new(
        "implement_feature",
        AgentRole::Developer,
        true,
        "Implemented JWT authentication: read requirements, designed token schema, \
         wrote middleware, added refresh token rotation, tested with integration suite",
    );

    let pattern_id = bank.store_pattern(pattern).await.unwrap();
    let retrieved = bank.get_pattern(pattern_id).await.unwrap();
    assert_eq!(retrieved.task_type(), "implement_feature");
    println!("✓ Pattern stored and retrieved successfully\n");

    // Test 2: Similarity search with HNSW
    println!("Test 2: Similarity search with HNSW");
    let (store, session_id) = setup_bank();
    let bank = ReasoningBank::new(store, session_id);

    // Store several patterns
    let patterns = vec![
        TaskPattern::new(
            "implement_feature",
            AgentRole::Developer,
            true,
            "Implemented OAuth2 login flow with Google and GitHub providers",
        ),
        TaskPattern::new(
            "implement_feature",
            AgentRole::Developer,
            true,
            "Added JWT token refresh mechanism with sliding window expiration",
        ),
        TaskPattern::new(
            "design_schema",
            AgentRole::Architect,
            true,
            "Designed user management database schema with RBAC permissions model",
        ),
    ];

    for p in patterns {
        bank.store_pattern(p).await.unwrap();
    }

    // Search for authentication-related patterns
    let query = PatternQuery::new("How to implement user authentication?")
        .with_task_type("implement_feature")
        .with_limit(3);

    let results = bank.find_similar(query).await.unwrap();
    assert!(!results.is_empty(), "Should find similar patterns");
    println!("✓ Found {} similar patterns", results.len());
    for (i, result) in results.iter().enumerate() {
        println!("  {}. Similarity: {:.3} - {}",
            i + 1,
            result.similarity,
            result.pattern.description().chars().take(60).collect::<String>()
        );
    }
    println!();

    // Test 3: Performance with larger dataset
    println!("Test 3: Performance with 1000 patterns");
    let (store, session_id) = setup_bank();
    let bank = ReasoningBank::new(store, session_id);

    let start = std::time::Instant::now();
    for i in 0..1000 {
        let pattern = TaskPattern::new(
            "implement_feature",
            AgentRole::Developer,
            true,
            &format!("Pattern {} with authentication oauth jwt tokens security", i),
        );
        bank.store_pattern(pattern).await.unwrap();
    }
    let store_time = start.elapsed();
    println!("✓ Stored 1000 patterns in {:?}", store_time);

    let start = std::time::Instant::now();
    let query = PatternQuery::new("authentication oauth security").with_limit(10);
    let results = bank.find_similar(query).await.unwrap();
    let search_time = start.elapsed();
    println!("✓ Searched 1000 patterns in {:?} (found {} results)", search_time, results.len());
    println!();

    println!("All tests passed! ✓");
}

fn setup_bank() -> (Arc<PatternStore>, SessionId) {
    let store = Arc::new(PatternStore::new());
    let session_id = SessionId::new();
    (store, session_id)
}
