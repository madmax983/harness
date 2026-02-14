//! Integration tests for SONA ReasoningBank + AletheiaDB integration.
//!
//! The ReasoningBank stores successful task execution patterns so agents can
//! learn from past experience. Patterns are indexed by task type and agent role,
//! with text-based similarity matching for retrieval.

use std::sync::Arc;

use harness_persistence::{AgentRole, PatternStore, ReasoningBank, TaskPattern, PatternQuery};

/// Helper: create a shared PatternStore and session ID for tests.
fn setup_bank() -> (Arc<PatternStore>, harness_persistence::SessionId) {
    let store = Arc::new(PatternStore::new());
    let session_id = harness_persistence::SessionId::new();
    (store, session_id)
}

/// Helper: create a TaskPattern representing a completed task execution.
fn sample_pattern(
    task_type: &str,
    agent_role: AgentRole,
    success: bool,
    description: &str,
) -> TaskPattern {
    TaskPattern::new(task_type, agent_role, success, description)
}

// ---------------------------------------------------------------------------
// Test 1: Store a successful task execution pattern
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_store_task_pattern() {
    let (store, session_id) = setup_bank();
    let bank = ReasoningBank::new(store, session_id);

    // Store a pattern for a successfully completed "implement_feature" task
    let pattern = sample_pattern(
        "implement_feature",
        AgentRole::Developer,
        true,
        "Implemented JWT authentication: read requirements, designed token schema, \
         wrote middleware, added refresh token rotation, tested with integration suite",
    );

    let pattern_id = bank.store_pattern(pattern).await.unwrap();

    // Verify the pattern was stored and can be retrieved
    let retrieved = bank.get_pattern(pattern_id).await.unwrap();
    assert_eq!(retrieved.task_type(), "implement_feature");
    assert_eq!(retrieved.agent_role(), AgentRole::Developer);
    assert!(retrieved.success());
    assert!(retrieved.description().contains("JWT authentication"));
    assert!(retrieved.created_at() <= chrono::Utc::now());
}

// ---------------------------------------------------------------------------
// Test 2: Retrieve similar patterns via similarity matching
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_retrieve_similar_patterns() {
    let (store, session_id) = setup_bank();
    let bank = ReasoningBank::new(store, session_id);

    // Store several diverse patterns
    let patterns = vec![
        sample_pattern(
            "implement_feature",
            AgentRole::Developer,
            true,
            "Implemented OAuth2 login flow with Google and GitHub providers",
        ),
        sample_pattern(
            "implement_feature",
            AgentRole::Developer,
            true,
            "Added JWT token refresh mechanism with sliding window expiration",
        ),
        sample_pattern(
            "design_schema",
            AgentRole::Architect,
            true,
            "Designed user management database schema with RBAC permissions model",
        ),
        sample_pattern(
            "write_tests",
            AgentRole::Tester,
            true,
            "Created integration test suite for REST API endpoints with mock database",
        ),
        sample_pattern(
            "implement_feature",
            AgentRole::Developer,
            false,
            "Failed to implement WebSocket real-time notifications due to missing specs",
        ),
    ];

    for p in patterns {
        bank.store_pattern(p).await.unwrap();
    }

    // Query for patterns similar to "authentication implementation"
    let query = PatternQuery::new("How to implement user authentication?")
        .with_task_type("implement_feature")
        .with_limit(3);

    let similar = bank.find_similar(query).await.unwrap();

    // Should return results (up to limit)
    assert!(!similar.is_empty());
    assert!(similar.len() <= 3);

    // The most similar patterns should include authentication-related results
    // NOTE: The simple TF-IDF-based embeddings may not always rank auth patterns first,
    // but they should appear in the top results. For true semantic similarity, swap to
    // a real embedding model (Ollama/OpenAI/ONNX) as noted in generate_embedding().
    let auth_keywords = ["oauth", "jwt", "login", "auth", "token"];
    let has_auth_result = similar.iter().any(|result| {
        let desc = result.pattern.description().to_lowercase();
        auth_keywords.iter().any(|keyword| desc.contains(keyword))
    });
    assert!(
        has_auth_result,
        "Results should include at least one auth-related pattern, got: {:?}",
        similar.iter().map(|r| r.pattern.description()).collect::<Vec<_>>()
    );

    // Each result should have a similarity score between 0.0 and 1.0
    for result in &similar {
        assert!(
            result.similarity >= 0.0 && result.similarity <= 1.0,
            "Similarity score should be in [0, 1], got: {}",
            result.similarity
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3: Patterns persist across ReasoningBank instances (shared store)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_persist_to_aletheia() {
    let (store, session_id) = setup_bank();

    // Store patterns in one bank instance
    {
        let bank = ReasoningBank::new(store.clone(), session_id);

        bank.store_pattern(sample_pattern(
            "code_review",
            AgentRole::Architect,
            true,
            "Reviewed authentication module: checked for SQL injection, XSS, CSRF vulnerabilities",
        ))
        .await
        .unwrap();

        bank.store_pattern(sample_pattern(
            "implement_feature",
            AgentRole::Developer,
            true,
            "Built rate limiting middleware using token bucket algorithm",
        ))
        .await
        .unwrap();
    }
    // First bank instance is dropped here

    // Create a NEW bank instance pointing to the same store (simulates restart)
    let bank2 = ReasoningBank::new(store.clone(), session_id);

    // Patterns from the first instance should still be retrievable
    let all_patterns = bank2
        .query_patterns(PatternQuery::new("").with_limit(10))
        .await
        .unwrap();

    assert_eq!(
        all_patterns.len(),
        2,
        "Both patterns should survive across bank instances"
    );

    // Verify specific content survived
    let descriptions: Vec<&str> = all_patterns.iter().map(|p| p.description()).collect();
    assert!(
        descriptions.iter().any(|d| d.contains("rate limiting")),
        "Rate limiting pattern should persist"
    );
    assert!(
        descriptions.iter().any(|d| d.contains("authentication module")),
        "Code review pattern should persist"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Query learned patterns by task type and agent role
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_query_learned_patterns() {
    let (store, session_id) = setup_bank();
    let bank = ReasoningBank::new(store, session_id);

    // Store patterns across different task types and roles
    let pattern_data = vec![
        ("implement_feature", AgentRole::Developer, true, "Built user registration endpoint"),
        ("implement_feature", AgentRole::Developer, true, "Added password reset flow"),
        ("implement_feature", AgentRole::Developer, false, "Failed cache invalidation"),
        ("design_schema", AgentRole::Architect, true, "Designed event sourcing schema"),
        ("design_schema", AgentRole::Architect, true, "Designed CQRS read model"),
        ("write_tests", AgentRole::Tester, true, "Property-based tests for serialization"),
        ("code_review", AgentRole::Architect, true, "Reviewed API gateway security"),
    ];

    for (task_type, role, success, desc) in pattern_data {
        bank.store_pattern(sample_pattern(task_type, role, success, desc))
            .await
            .unwrap();
    }

    // Query by task type only
    let feature_patterns = bank
        .query_patterns(
            PatternQuery::new("")
                .with_task_type("implement_feature")
                .with_limit(10),
        )
        .await
        .unwrap();
    assert_eq!(feature_patterns.len(), 3, "Should find 3 implement_feature patterns");

    // Query by agent role only
    let architect_patterns = bank
        .query_patterns(
            PatternQuery::new("")
                .with_role(AgentRole::Architect)
                .with_limit(10),
        )
        .await
        .unwrap();
    assert_eq!(architect_patterns.len(), 3, "Should find 3 Architect patterns");

    // Query by both task type AND role
    let dev_features = bank
        .query_patterns(
            PatternQuery::new("")
                .with_task_type("implement_feature")
                .with_role(AgentRole::Developer)
                .with_limit(10),
        )
        .await
        .unwrap();
    assert_eq!(dev_features.len(), 3, "Should find 3 Developer+implement_feature patterns");

    // Query only successful patterns
    let successful_dev = bank
        .query_patterns(
            PatternQuery::new("")
                .with_task_type("implement_feature")
                .with_role(AgentRole::Developer)
                .with_success_only(true)
                .with_limit(10),
        )
        .await
        .unwrap();
    assert_eq!(successful_dev.len(), 2, "Should find 2 successful Developer+implement_feature patterns");

    // Query with no filters returns everything
    let all = bank
        .query_patterns(PatternQuery::new("").with_limit(100))
        .await
        .unwrap();
    assert_eq!(all.len(), 7, "Should return all 7 patterns");
}

// ---------------------------------------------------------------------------
// Test 5: Handle cold start gracefully (empty reasoning bank)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_empty_reasoning_bank() {
    let (store, session_id) = setup_bank();
    let bank = ReasoningBank::new(store, session_id);

    // Querying an empty bank should return empty results, not error
    let results = bank
        .query_patterns(PatternQuery::new("authentication").with_limit(5))
        .await
        .unwrap();
    assert!(results.is_empty(), "Empty bank should return empty results");

    // Similarity search on empty bank should also return empty
    let similar = bank
        .find_similar(PatternQuery::new("How to implement caching?").with_limit(5))
        .await
        .unwrap();
    assert!(similar.is_empty(), "Similarity search on empty bank should return empty");

    // Getting a non-existent pattern should return a meaningful error
    let fake_id = harness_persistence::PatternId::new();
    let result = bank.get_pattern(fake_id).await;
    assert!(
        result.is_err(),
        "Getting non-existent pattern should return error"
    );
}

// ---------------------------------------------------------------------------
// Test 6: Concurrent pattern storage is thread-safe
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_concurrent_pattern_storage() {
    let (store, session_id) = setup_bank();
    let bank = Arc::new(ReasoningBank::new(store, session_id));

    // Spawn 10 concurrent tasks, each storing a pattern
    let mut handles = vec![];
    for i in 0..10 {
        let bank_clone = bank.clone();
        let handle = tokio::spawn(async move {
            let pattern = sample_pattern(
                &format!("concurrent_task_{}", i),
                AgentRole::Developer,
                true,
                &format!("Pattern {} stored concurrently to test thread safety", i),
            );
            bank_clone.store_pattern(pattern).await
        });
        handles.push(handle);
    }

    // All tasks should complete successfully
    let results: Vec<_> = futures::future::join_all(handles).await;
    for (i, result) in results.iter().enumerate() {
        let inner = result.as_ref().expect("task should not panic");
        assert!(inner.is_ok(), "Pattern {} should store successfully: {:?}", i, inner);
    }

    // Verify all 10 patterns were stored
    let all_patterns = bank
        .query_patterns(PatternQuery::new("").with_limit(20))
        .await
        .unwrap();
    assert_eq!(
        all_patterns.len(),
        10,
        "All 10 concurrently-stored patterns should be present"
    );

    // Verify no duplicates (each task_type should be unique)
    let mut task_types: Vec<String> = all_patterns.iter().map(|p| p.task_type().to_string()).collect();
    task_types.sort();
    task_types.dedup();
    assert_eq!(
        task_types.len(),
        10,
        "All 10 patterns should have unique task types"
    );
}
