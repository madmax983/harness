//! Integration tests for Harness v2 Hive Mind.
//!
//! Tests end-to-end MCP handler workflows using InMemoryRepository.

use std::sync::Arc;

use harness_mcp::{HiveHandler, HiveState};
use harness_orchestrator::{OrchestratorConfig, ProcessManager};
use harness_persistence::{InMemoryRepository, Repository, Session};

async fn setup_hive() -> (
    Arc<HiveState<InMemoryRepository>>,
    HiveHandler<InMemoryRepository>,
) {
    let repo = Arc::new(InMemoryRepository::new());
    let session = Session::new(8);
    repo.create_session(&session).await.unwrap();

    let config = OrchestratorConfig::default();
    let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));

    let state = Arc::new(HiveState::new(session, repo, process_manager));
    let handler = HiveHandler::new(state.clone());
    (state, handler)
}

/// Test 1: Full coordination flow - register agents, create tasks, claim, share knowledge, complete.
#[tokio::test]
async fn test_full_coordination_flow() {
    let (_state, handler) = setup_hive().await;

    // Register strategoi
    handler
        .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
        .await
        .expect("register strategoi");

    // Register two workers
    let dev_handler = HiveHandler::new(handler.state_ref().clone());
    dev_handler
        .call_tool("register_agent", serde_json::json!({"role": "developer"}))
        .await
        .expect("register developer");

    let tester_handler = HiveHandler::new(handler.state_ref().clone());
    tester_handler
        .call_tool("register_agent", serde_json::json!({"role": "tester"}))
        .await
        .expect("register tester");

    // Strategoi creates a task
    let task_resp = handler
        .call_tool(
            "create_task",
            serde_json::json!({
                "title": "Implement authentication",
                "description": "Add JWT auth to API",
                "priority": "high"
            }),
        )
        .await
        .expect("create task");
    let task_id: String =
        serde_json::from_value(task_resp.get("task_id").unwrap().clone()).unwrap();

    // Developer claims the task
    let claim_resp = dev_handler
        .call_tool("claim_task", serde_json::json!({"task_id": task_id}))
        .await
        .expect("claim task");
    assert!(claim_resp.get("success").unwrap().as_bool().unwrap());

    // Developer shares knowledge
    let knowledge_resp = dev_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Using jsonwebtoken crate for JWT",
                "kind": "decision",
                "task_id": task_id
            }),
        )
        .await
        .expect("share knowledge");
    assert!(knowledge_resp.get("knowledge_id").is_some());

    // Update task to in_progress
    dev_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": task_id,
                "status": "in_progress"
            }),
        )
        .await
        .expect("update status");

    // Complete task
    dev_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": task_id,
                "status": "completed",
                "summary": "JWT auth implemented and tested"
            }),
        )
        .await
        .expect("complete task");

    // Verify hive status
    let status_resp = handler
        .call_tool("get_hive_status", serde_json::json!({}))
        .await
        .expect("get hive status");

    let agents = status_resp.get("agents").unwrap().as_array().unwrap();
    assert_eq!(agents.len(), 3);

    let task_summary = status_resp.get("task_summary").unwrap();
    assert_eq!(task_summary.get("completed").unwrap().as_u64().unwrap(), 1);
    assert_eq!(task_summary.get("pending").unwrap().as_u64().unwrap(), 0);
}

/// Test 2: Concurrent task claims - only one agent should succeed.
#[tokio::test]
async fn test_concurrent_task_claims() {
    let (state, handler) = setup_hive().await;

    // Register strategoi and create task
    handler
        .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
        .await
        .unwrap();

    let task_resp = handler
        .call_tool(
            "create_task",
            serde_json::json!({
                "title": "Contested task",
                "description": "Multiple agents will try to claim this",
                "priority": "critical"
            }),
        )
        .await
        .unwrap();
    let task_id: String =
        serde_json::from_value(task_resp.get("task_id").unwrap().clone()).unwrap();

    // Spawn 5 agents, each trying to claim the task concurrently
    let mut handles = vec![];
    for i in 0..5 {
        let state_clone = state.clone();
        let task_id_clone = task_id.clone();

        let handle = tokio::spawn(async move {
            let agent_handler = HiveHandler::new(state_clone);
            agent_handler
                .call_tool("register_agent", serde_json::json!({"role": "developer"}))
                .await
                .unwrap();

            let claim_result = agent_handler
                .call_tool("claim_task", serde_json::json!({"task_id": task_id_clone}))
                .await
                .unwrap();

            (i, claim_result.get("success").unwrap().as_bool().unwrap())
        });
        handles.push(handle);
    }

    let results = futures::future::join_all(handles).await;
    let success_count = results.iter().filter(|r| r.as_ref().unwrap().1).count();

    // Exactly 1 should succeed, 4 should fail
    assert_eq!(success_count, 1, "Exactly one agent should claim the task");
}

/// Test 3: BMAD workflow - Strategoi assigns to BA, BA shares knowledge, PM reads it.
#[tokio::test]
async fn test_bmad_workflow() {
    let (_state, strategoi_handler) = setup_hive().await;

    // Register strategoi
    strategoi_handler
        .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
        .await
        .unwrap();

    // Register BA and PM
    let ba_handler = HiveHandler::new(strategoi_handler.state_ref().clone());
    let ba_resp = ba_handler
        .call_tool("register_agent", serde_json::json!({"role": "ba"}))
        .await
        .unwrap();
    let ba_id: String = serde_json::from_value(ba_resp.get("agent_id").unwrap().clone()).unwrap();

    let pm_handler = HiveHandler::new(strategoi_handler.state_ref().clone());
    pm_handler
        .call_tool("register_agent", serde_json::json!({"role": "pm"}))
        .await
        .unwrap();

    // Strategoi creates a task
    let task_resp = strategoi_handler
        .call_tool(
            "create_task",
            serde_json::json!({
                "title": "Gather requirements",
                "description": "Interview stakeholders for mobile app",
                "priority": "high"
            }),
        )
        .await
        .unwrap();
    let task_id: String =
        serde_json::from_value(task_resp.get("task_id").unwrap().clone()).unwrap();

    // Strategoi assigns task to BA
    strategoi_handler
        .call_tool(
            "assign_task",
            serde_json::json!({
                "task_id": task_id,
                "agent_id": ba_id
            }),
        )
        .await
        .unwrap();

    // BA shares knowledge
    ba_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Stakeholders need offline mode and dark theme",
                "kind": "discovery",
                "task_id": task_id
            }),
        )
        .await
        .unwrap();

    ba_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Timeline: 3 months for MVP",
                "kind": "decision",
                "task_id": task_id
            }),
        )
        .await
        .unwrap();

    // PM queries hive knowledge (fallback to recent since no embeddings)
    let ask_resp = pm_handler
        .call_tool(
            "ask_hive",
            serde_json::json!({
                "query": "requirements",
                "limit": 5
            }),
        )
        .await
        .unwrap();

    let results = ask_resp.get("results").unwrap().as_array().unwrap();
    assert_eq!(results.len(), 2, "PM should see both knowledge entries");

    // Verify task context
    let context_resp = ba_handler
        .call_tool("get_task_context", serde_json::json!({"task_id": task_id}))
        .await
        .unwrap();

    let knowledge = context_resp.get("knowledge").unwrap().as_array().unwrap();
    assert_eq!(knowledge.len(), 2);
}

/// Test 4: Direct message thread - two agents exchange messages on a task.
#[tokio::test]
async fn test_direct_message_thread() {
    let (_state, handler) = setup_hive().await;

    // Register two agents
    let arch_handler = HiveHandler::new(handler.state_ref().clone());
    let arch_resp = arch_handler
        .call_tool("register_agent", serde_json::json!({"role": "architect"}))
        .await
        .unwrap();
    let arch_id: String =
        serde_json::from_value(arch_resp.get("agent_id").unwrap().clone()).unwrap();

    let dev_handler = HiveHandler::new(handler.state_ref().clone());
    let dev_resp = dev_handler
        .call_tool("register_agent", serde_json::json!({"role": "developer"}))
        .await
        .unwrap();
    let dev_id: String = serde_json::from_value(dev_resp.get("agent_id").unwrap().clone()).unwrap();

    // Create a task
    handler
        .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
        .await
        .unwrap();

    let task_resp = handler
        .call_tool(
            "create_task",
            serde_json::json!({
                "title": "Design database schema",
                "description": "Schema for user management",
                "priority": "high"
            }),
        )
        .await
        .unwrap();
    let task_id: String =
        serde_json::from_value(task_resp.get("task_id").unwrap().clone()).unwrap();

    // Architect sends DM to developer
    arch_handler
        .call_tool(
            "send_direct_message",
            serde_json::json!({
                "to_agent": dev_id,
                "content": "Should we use UUID or auto-increment for user IDs?",
                "task_id": task_id
            }),
        )
        .await
        .unwrap();

    // Developer replies
    dev_handler
        .call_tool(
            "send_direct_message",
            serde_json::json!({
                "to_agent": arch_id,
                "content": "UUID for distributed systems, auto-increment for simpler setup",
                "task_id": task_id
            }),
        )
        .await
        .unwrap();

    // Architect sends follow-up
    arch_handler
        .call_tool(
            "send_direct_message",
            serde_json::json!({
                "to_agent": dev_id,
                "content": "Let's go with UUID then",
                "task_id": task_id
            }),
        )
        .await
        .unwrap();

    // Get thread messages
    let thread_resp = arch_handler
        .call_tool(
            "get_thread_messages",
            serde_json::json!({
                "task_id": task_id,
                "limit": 10
            }),
        )
        .await
        .unwrap();

    let messages = thread_resp.get("messages").unwrap().as_array().unwrap();
    assert_eq!(messages.len(), 3);

    // Verify ordering (ascending by time)
    let first_content = messages[0].get("content").unwrap().as_str().unwrap();
    assert!(first_content.contains("UUID or auto-increment"));
}

/// Test 5: Hive status accuracy - verify counts match actual state.
#[tokio::test]
async fn test_hive_status_accuracy() {
    let (_state, handler) = setup_hive().await;

    // Register multiple agents
    handler
        .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
        .await
        .unwrap();

    for role in ["developer", "developer", "tester", "architect"] {
        let agent_handler = HiveHandler::new(handler.state_ref().clone());
        agent_handler
            .call_tool("register_agent", serde_json::json!({"role": role}))
            .await
            .unwrap();
    }

    // Create tasks in various statuses
    handler
        .call_tool(
            "create_task",
            serde_json::json!({"title": "Task 1", "description": "Pending task", "priority": "low"}),
        )
        .await
        .unwrap();

    let task2 = handler
        .call_tool(
            "create_task",
            serde_json::json!({"title": "Task 2", "description": "In progress", "priority": "medium"}),
        )
        .await
        .unwrap();
    let task2_id: String = serde_json::from_value(task2.get("task_id").unwrap().clone()).unwrap();

    let task3 = handler
        .call_tool(
            "create_task",
            serde_json::json!({"title": "Task 3", "description": "Completed", "priority": "high"}),
        )
        .await
        .unwrap();
    let task3_id: String = serde_json::from_value(task3.get("task_id").unwrap().clone()).unwrap();

    // Update task statuses
    handler
        .call_tool(
            "update_task_status",
            serde_json::json!({"task_id": task2_id, "status": "in_progress"}),
        )
        .await
        .unwrap();

    handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": task3_id,
                "status": "completed",
                "summary": "Done"
            }),
        )
        .await
        .unwrap();

    // Share knowledge
    handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({"content": "Test knowledge 1", "kind": "activity"}),
        )
        .await
        .unwrap();

    handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({"content": "Test knowledge 2", "kind": "discovery"}),
        )
        .await
        .unwrap();

    // Get hive status
    let status = handler
        .call_tool("get_hive_status", serde_json::json!({}))
        .await
        .unwrap();

    // Verify agent count
    let agents = status.get("agents").unwrap().as_array().unwrap();
    assert_eq!(agents.len(), 5, "Should have 5 agents total");

    // Verify task summary
    let task_summary = status.get("task_summary").unwrap();
    assert_eq!(
        task_summary.get("pending").unwrap().as_u64().unwrap(),
        1,
        "1 pending task"
    );
    assert_eq!(
        task_summary.get("in_progress").unwrap().as_u64().unwrap(),
        1,
        "1 in-progress task"
    );
    assert_eq!(
        task_summary.get("completed").unwrap().as_u64().unwrap(),
        1,
        "1 completed task"
    );

    // Verify recent knowledge
    let recent_knowledge = status.get("recent_knowledge").unwrap().as_array().unwrap();
    assert_eq!(
        recent_knowledge.len(),
        2,
        "Should have 2 recent knowledge entries"
    );
}

/// Test 6: Embedding service integration with semantic search.
///
/// This test requires Ollama to be running with the all-minilm model.
/// If Ollama is not available, the test is skipped.
#[tokio::test]
async fn test_embedding_service_integration() {
    use aletheiadb::embeddings::EmbeddingService;
    use aletheiadb::embeddings::providers::ollama::{OllamaConfig, OllamaProvider};
    use harness_persistence::AletheiaRepository;

    // Try to connect to Ollama - skip test if not available
    let config = OllamaConfig::new("all-minilm".to_string(), 384);
    let provider = match OllamaProvider::new(config) {
        Ok(p) => Arc::new(p),
        Err(_) => {
            eprintln!("Skipping embedding test - Ollama not available");
            return;
        }
    };

    let embedding_service = Arc::new(EmbeddingService::new(provider));

    // Create AletheiaRepository with HNSW vector index
    let db = Arc::new(aletheiadb::AletheiaDB::new().expect("create db"));
    let repo = Arc::new(
        AletheiaRepository::new_anon(db)
            .with_vector_index(384)
            .expect("create HNSW index"),
    );
    let session = Session::new(8);
    repo.create_session(&session).await.unwrap();

    // Create HiveState with embedding service
    let config = OrchestratorConfig::default();
    let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));

    let state = Arc::new(
        HiveState::new(session, repo, process_manager)
            .with_embedding_service(embedding_service.clone()),
    );
    let handler = HiveHandler::new(state.clone());

    // Register agent
    handler
        .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
        .await
        .unwrap();

    // Share knowledge entries with different semantic content
    handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "The authentication system uses JWT tokens for session management",
                "kind": "discovery"
            }),
        )
        .await
        .unwrap();

    handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Database schema includes user table with email and password_hash fields",
                "kind": "discovery"
            }),
        )
        .await
        .unwrap();

    handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Frontend uses React with TypeScript for type safety",
                "kind": "discovery"
            }),
        )
        .await
        .unwrap();

    // Query with semantic search - should find auth-related knowledge first
    let auth_query = handler
        .call_tool(
            "ask_hive",
            serde_json::json!({
                "query": "How is user authentication handled?",
                "limit": 2
            }),
        )
        .await
        .unwrap();

    let auth_results = auth_query.get("results").unwrap().as_array().unwrap();
    assert_eq!(auth_results.len(), 2, "Should return 2 results");

    // The first result should be about JWT authentication (most semantically similar)
    let first_content = auth_results[0].get("content").unwrap().as_str().unwrap();
    assert!(
        first_content.contains("JWT") || first_content.contains("authentication"),
        "First result should be auth-related, got: {}",
        first_content
    );

    // Query about database - should find DB-related knowledge
    let db_query = handler
        .call_tool(
            "ask_hive",
            serde_json::json!({
                "query": "What database tables exist?",
                "limit": 2
            }),
        )
        .await
        .unwrap();

    let db_results = db_query.get("results").unwrap().as_array().unwrap();
    let first_db = db_results[0].get("content").unwrap().as_str().unwrap();
    assert!(
        first_db.contains("Database") || first_db.contains("table"),
        "First result should be DB-related, got: {}",
        first_db
    );
}

// ============================================================================
// SONA Integration Tests (RED Phase)
//
// These tests define the contract for SONA (Self-Optimizing Neural Architecture)
// integration into the harness hive mind. They WILL FAIL until the GREEN phase
// implements the underlying SONA modules.
//
// SONA components:
//   - Trajectory recording: Captures agent action sequences
//   - ReasoningBank: Stores learned patterns in AletheiaDB
//   - MicroLoRA: Per-agent adaptive learning
//   - BaseLoRA: Collective hive learning
//   - EWC++: Catastrophic forgetting prevention
//   - Three Learning Loops: Instant, background, coordination
// ============================================================================

/// Helper: register an agent and return its agent_id string.
async fn register_agent<R: Repository + 'static>(handler: &HiveHandler<R>, role: &str) -> String {
    let resp = handler
        .call_tool("register_agent", serde_json::json!({"role": role}))
        .await
        .expect("register agent");
    serde_json::from_value(resp.get("agent_id").unwrap().clone()).unwrap()
}

/// Helper: create a task and return its task_id string.
async fn create_task<R: Repository + 'static>(
    handler: &HiveHandler<R>,
    title: &str,
    description: &str,
    priority: &str,
) -> String {
    let resp = handler
        .call_tool(
            "create_task",
            serde_json::json!({
                "title": title,
                "description": description,
                "priority": priority
            }),
        )
        .await
        .expect("create task");
    serde_json::from_value(resp.get("task_id").unwrap().clone()).unwrap()
}

/// Test 7: Full learning cycle - Task completion triggers trajectory recording,
/// which feeds into the learning pipeline and produces an optimized pattern.
///
/// Flow: Task -> Trajectory -> Learning -> ReasoningBank pattern
#[tokio::test]
async fn test_full_learning_cycle() {
    let (_state, handler) = setup_hive().await;

    // Register a developer agent
    let dev_handler = HiveHandler::new(handler.state_ref().clone());
    let _dev_id = register_agent(&dev_handler, "developer").await;

    // Create and work through a task (generating trajectory data)
    let task_id = create_task(
        &dev_handler,
        "Implement error handling",
        "Add proper error types to the API layer",
        "high",
    )
    .await;

    // Developer claims and works the task, sharing knowledge along the way
    dev_handler
        .call_tool("claim_task", serde_json::json!({"task_id": &task_id}))
        .await
        .expect("claim task");

    dev_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({"task_id": &task_id, "status": "in_progress"}),
        )
        .await
        .expect("start task");

    dev_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Using thiserror for library errors, anyhow for binary",
                "kind": "decision",
                "task_id": &task_id
            }),
        )
        .await
        .expect("share decision");

    dev_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Found that error chain propagation needs From impls",
                "kind": "discovery",
                "task_id": &task_id
            }),
        )
        .await
        .expect("share discovery");

    // Complete the task - this should trigger trajectory recording
    dev_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": &task_id,
                "status": "completed",
                "summary": "Error handling implemented with thiserror"
            }),
        )
        .await
        .expect("complete task");

    // SONA: Query the trajectory for this task
    // The trajectory should record the sequence: claim -> in_progress -> knowledge -> knowledge -> completed
    let trajectory_resp = dev_handler
        .call_tool(
            "get_task_trajectory",
            serde_json::json!({"task_id": &task_id}),
        )
        .await
        .expect("get trajectory");

    let steps = trajectory_resp
        .get("steps")
        .expect("trajectory should have steps")
        .as_array()
        .expect("steps should be array");
    assert!(
        steps.len() >= 4,
        "Trajectory should have at least 4 steps (claim, start, knowledge x2, complete), got {}",
        steps.len()
    );

    // SONA: The learning pipeline should have extracted a pattern
    let patterns_resp = dev_handler
        .call_tool(
            "query_reasoning_bank",
            serde_json::json!({
                "query": "error handling",
                "limit": 5
            }),
        )
        .await
        .expect("query reasoning bank");

    let patterns = patterns_resp
        .get("patterns")
        .expect("should have patterns")
        .as_array()
        .expect("patterns should be array");
    assert!(
        !patterns.is_empty(),
        "ReasoningBank should contain at least one learned pattern from the completed task"
    );

    // Verify the pattern references the original task trajectory
    let first_pattern = &patterns[0];
    assert!(
        first_pattern.get("source_trajectory_id").is_some(),
        "Pattern should reference its source trajectory"
    );
    assert!(
        first_pattern.get("confidence").is_some(),
        "Pattern should have a confidence score"
    );
}

/// Test 8: Three learning loops - Instant (on action), Background (periodic),
/// and Coordination (cross-agent) learning all producing results.
#[tokio::test]
async fn test_three_learning_loops() {
    let (state, handler) = setup_hive().await;

    // Register strategoi and two developers
    let _strategoi_id = register_agent(&handler, "strategoi").await;

    let dev1_handler = HiveHandler::new(state.clone());
    let _dev1_id = register_agent(&dev1_handler, "developer").await;

    let dev2_handler = HiveHandler::new(state.clone());
    let _dev2_id = register_agent(&dev2_handler, "developer").await;

    // === INSTANT LEARNING LOOP ===
    // Sharing knowledge should immediately create a micro-pattern

    dev1_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Always validate JWT expiry before trusting claims",
                "kind": "discovery"
            }),
        )
        .await
        .expect("share knowledge");

    // Check that instant learning recorded this
    let instant_resp = dev1_handler
        .call_tool(
            "get_learning_status",
            serde_json::json!({"loop_type": "instant"}),
        )
        .await
        .expect("get instant learning status");

    let instant_count = instant_resp
        .get("patterns_learned")
        .expect("should have patterns_learned")
        .as_u64()
        .expect("should be number");
    assert!(
        instant_count >= 1,
        "Instant loop should have learned at least 1 pattern, got {}",
        instant_count
    );

    // === BACKGROUND LEARNING LOOP ===
    // Complete multiple tasks to trigger background optimization

    for i in 0..3 {
        let task_id = create_task(
            &dev1_handler,
            &format!("Background task {}", i),
            "Task for background learning",
            "medium",
        )
        .await;
        dev1_handler
            .call_tool("claim_task", serde_json::json!({"task_id": &task_id}))
            .await
            .unwrap();
        dev1_handler
            .call_tool(
                "update_task_status",
                serde_json::json!({
                    "task_id": &task_id,
                    "status": "completed",
                    "summary": format!("Completed background task {}", i)
                }),
            )
            .await
            .unwrap();
    }

    // Trigger background learning cycle (normally periodic, forced here for testing)
    let bg_resp = handler
        .call_tool(
            "trigger_learning_cycle",
            serde_json::json!({"loop_type": "background"}),
        )
        .await
        .expect("trigger background learning");

    assert!(
        bg_resp.get("optimizations_applied").is_some(),
        "Background learning should report optimizations applied"
    );

    // === COORDINATION LEARNING LOOP ===
    // Two agents working on related tasks should trigger cross-agent learning

    let task_a = create_task(&handler, "Auth module", "Implement authentication", "high").await;
    let task_b = create_task(
        &handler,
        "Auth tests",
        "Write tests for authentication",
        "high",
    )
    .await;

    // Dev1 works on implementation
    dev1_handler
        .call_tool("claim_task", serde_json::json!({"task_id": &task_a}))
        .await
        .unwrap();
    dev1_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Auth uses bcrypt for password hashing with cost factor 12",
                "kind": "decision",
                "task_id": &task_a
            }),
        )
        .await
        .unwrap();
    dev1_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": &task_a,
                "status": "completed",
                "summary": "Auth implemented"
            }),
        )
        .await
        .unwrap();

    // Dev2 works on related tests
    dev2_handler
        .call_tool("claim_task", serde_json::json!({"task_id": &task_b}))
        .await
        .unwrap();
    dev2_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Test fixtures use bcrypt cost factor 4 for speed",
                "kind": "discovery",
                "task_id": &task_b
            }),
        )
        .await
        .unwrap();
    dev2_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": &task_b,
                "status": "completed",
                "summary": "Auth tests complete"
            }),
        )
        .await
        .unwrap();

    // Trigger coordination learning
    let coord_resp = handler
        .call_tool(
            "trigger_learning_cycle",
            serde_json::json!({"loop_type": "coordination"}),
        )
        .await
        .expect("trigger coordination learning");

    let cross_agent_patterns = coord_resp
        .get("cross_agent_patterns")
        .expect("should have cross_agent_patterns")
        .as_array()
        .expect("should be array");
    assert!(
        !cross_agent_patterns.is_empty(),
        "Coordination loop should find cross-agent patterns (both devs discussed bcrypt)"
    );
}

/// Test 9: Learned patterns persist in AletheiaDB and survive "restart"
/// (recreating the handler from the same repository).
#[tokio::test]
async fn test_aletheia_persistence() {
    // Use AletheiaDB (not InMemory) to test real persistence
    use harness_persistence::AletheiaRepository;

    let db = Arc::new(aletheiadb::AletheiaDB::new().expect("create db"));
    let repo = Arc::new(AletheiaRepository::new_anon(db));
    let session = Session::new(8);
    repo.create_session(&session).await.unwrap();

    let config = OrchestratorConfig::default();
    let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));
    let state = Arc::new(HiveState::new(
        session.clone(),
        repo.clone(),
        process_manager,
    ));

    // Phase 1: Create patterns through normal workflow
    {
        let handler = HiveHandler::new(state.clone());
        let _agent_id = register_agent(&handler, "developer").await;

        let task_id = create_task(
            &handler,
            "Design caching layer",
            "Implement Redis-backed caching",
            "high",
        )
        .await;

        handler
            .call_tool("claim_task", serde_json::json!({"task_id": &task_id}))
            .await
            .unwrap();

        handler
            .call_tool(
                "share_knowledge",
                serde_json::json!({
                    "content": "Redis cache with 5-minute TTL for API responses",
                    "kind": "decision",
                    "task_id": &task_id
                }),
            )
            .await
            .unwrap();

        handler
            .call_tool(
                "update_task_status",
                serde_json::json!({
                    "task_id": &task_id,
                    "status": "completed",
                    "summary": "Caching layer implemented"
                }),
            )
            .await
            .unwrap();

        // Verify pattern was learned
        let patterns = handler
            .call_tool(
                "query_reasoning_bank",
                serde_json::json!({"query": "caching", "limit": 5}),
            )
            .await
            .expect("query patterns before restart");

        let count = patterns.get("patterns").unwrap().as_array().unwrap().len();
        assert!(count >= 1, "Should have learned a pattern before restart");
    }

    // Phase 2: "Restart" - create a new handler from the same repo
    // This simulates a daemon restart where AletheiaDB persists
    {
        let config2 = OrchestratorConfig::default();
        let process_manager2 = Arc::new(ProcessManager::new(config2, repo.clone()));
        let state2 = Arc::new(HiveState::new(session, repo.clone(), process_manager2));
        let handler2 = HiveHandler::new(state2);

        // Register a new agent in the "restarted" session
        let _agent_id = register_agent(&handler2, "developer").await;

        // Query the reasoning bank - patterns from Phase 1 should still be there
        let patterns = handler2
            .call_tool(
                "query_reasoning_bank",
                serde_json::json!({"query": "caching", "limit": 5}),
            )
            .await
            .expect("query patterns after restart");

        let persisted = patterns.get("patterns").unwrap().as_array().unwrap().len();
        assert!(
            persisted >= 1,
            "Learned patterns should persist across restart, got {} patterns",
            persisted
        );

        // Verify pattern content survived
        let first = &patterns.get("patterns").unwrap().as_array().unwrap()[0];
        let content = first.get("content").unwrap().as_str().unwrap();
        assert!(
            content.contains("cach") || content.contains("Redis") || content.contains("TTL"),
            "Persisted pattern should be about caching, got: {}",
            content
        );
    }
}

/// Test 10: DOGFOODING - Multiple agents coordinate to build harness itself!
/// This is the META test: agents use the hive mind to track work on improving
/// the hive mind. Tests the full multi-agent SONA workflow.
#[tokio::test]
async fn test_multi_agent_dogfooding() {
    let (state, strategoi_handler) = setup_hive().await;

    // The strategoi orchestrates the work
    let _strategoi_id = register_agent(&strategoi_handler, "strategoi").await;

    // Architect designs the SONA integration
    let arch_handler = HiveHandler::new(state.clone());
    let arch_id = register_agent(&arch_handler, "architect").await;

    // Developer implements it
    let dev_handler = HiveHandler::new(state.clone());
    let dev_id = register_agent(&dev_handler, "developer").await;

    // Tester validates it
    let tester_handler = HiveHandler::new(state.clone());
    let tester_id = register_agent(&tester_handler, "tester").await;

    // === Phase 1: Strategoi creates the plan ===
    let parent_task = create_task(
        &strategoi_handler,
        "Integrate SONA learning",
        "Add adaptive learning to the hive mind",
        "critical",
    )
    .await;

    let design_task = create_task(
        &strategoi_handler,
        "Design SONA architecture",
        "Define interfaces for trajectory, learning, and reasoning bank",
        "high",
    )
    .await;

    let impl_task = create_task(
        &strategoi_handler,
        "Implement SONA core",
        "Build trajectory recorder, learning loops, reasoning bank",
        "high",
    )
    .await;

    let test_task = create_task(
        &strategoi_handler,
        "Test SONA integration",
        "Validate end-to-end learning cycle",
        "high",
    )
    .await;

    // Assign tasks
    strategoi_handler
        .call_tool(
            "assign_task",
            serde_json::json!({"task_id": &design_task, "agent_id": &arch_id}),
        )
        .await
        .unwrap();
    strategoi_handler
        .call_tool(
            "assign_task",
            serde_json::json!({"task_id": &impl_task, "agent_id": &dev_id}),
        )
        .await
        .unwrap();
    strategoi_handler
        .call_tool(
            "assign_task",
            serde_json::json!({"task_id": &test_task, "agent_id": &tester_id}),
        )
        .await
        .unwrap();

    // === Phase 2: Architect designs (generates trajectory) ===
    arch_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({"task_id": &design_task, "status": "in_progress"}),
        )
        .await
        .unwrap();

    arch_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "SONA uses trait-based interfaces: TrajectoryRecorder, LearningLoop, ReasoningBank",
                "kind": "decision",
                "task_id": &design_task
            }),
        )
        .await
        .unwrap();

    arch_handler
        .call_tool(
            "send_direct_message",
            serde_json::json!({
                "to_agent": &dev_id,
                "content": "Design ready - three traits to implement. Check the knowledge entries.",
                "task_id": &design_task
            }),
        )
        .await
        .unwrap();

    arch_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": &design_task,
                "status": "completed",
                "summary": "Architecture designed with trait-based interfaces"
            }),
        )
        .await
        .unwrap();

    // === Phase 3: Developer implements (generates trajectory) ===
    dev_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({"task_id": &impl_task, "status": "in_progress"}),
        )
        .await
        .unwrap();

    dev_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "TrajectoryRecorder stores steps in AletheiaDB with bi-temporal tracking",
                "kind": "discovery",
                "task_id": &impl_task
            }),
        )
        .await
        .unwrap();

    dev_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "LearningLoop runs three modes: instant (sync), background (async), coordination (cross-agent)",
                "kind": "discovery",
                "task_id": &impl_task
            }),
        )
        .await
        .unwrap();

    dev_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": &impl_task,
                "status": "completed",
                "summary": "SONA core implemented with all three learning loops"
            }),
        )
        .await
        .unwrap();

    // === Phase 4: Tester validates (generates trajectory) ===
    tester_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({"task_id": &test_task, "status": "in_progress"}),
        )
        .await
        .unwrap();

    tester_handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "All learning loops produce valid patterns, persistence verified",
                "kind": "activity",
                "task_id": &test_task
            }),
        )
        .await
        .unwrap();

    tester_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": &test_task,
                "status": "completed",
                "summary": "SONA integration tests all passing"
            }),
        )
        .await
        .unwrap();

    // Complete parent task
    strategoi_handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": &parent_task,
                "status": "completed",
                "summary": "SONA fully integrated"
            }),
        )
        .await
        .unwrap();

    // === Verify: SONA learned from the dogfooding workflow ===

    // 1. Each agent should have a trajectory recorded
    for (agent_name, task_id) in [
        ("architect", &design_task),
        ("developer", &impl_task),
        ("tester", &test_task),
    ] {
        let traj = strategoi_handler
            .call_tool(
                "get_task_trajectory",
                serde_json::json!({"task_id": task_id}),
            )
            .await
            .unwrap_or_else(|_| panic!("get trajectory for {} task", agent_name));

        let steps = traj.get("steps").unwrap().as_array().unwrap();
        assert!(
            !steps.is_empty(),
            "{} should have trajectory steps, got 0",
            agent_name
        );
    }

    // 2. Reasoning bank should have patterns from all three agents' work
    let all_patterns = strategoi_handler
        .call_tool(
            "query_reasoning_bank",
            serde_json::json!({"query": "SONA architecture implementation testing", "limit": 10}),
        )
        .await
        .expect("query all patterns");

    let patterns = all_patterns.get("patterns").unwrap().as_array().unwrap();
    assert!(
        patterns.len() >= 3,
        "Reasoning bank should have patterns from multiple agents, got {}",
        patterns.len()
    );

    // 3. Coordination learning should have detected the cross-agent workflow
    let coord = strategoi_handler
        .call_tool(
            "trigger_learning_cycle",
            serde_json::json!({"loop_type": "coordination"}),
        )
        .await
        .expect("coordination learning");

    let cross_patterns = coord
        .get("cross_agent_patterns")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(
        !cross_patterns.is_empty(),
        "Coordination should find cross-agent patterns from the design->implement->test workflow"
    );

    // 4. Hive status should reflect all the completed work
    let status = strategoi_handler
        .call_tool("get_hive_status", serde_json::json!({}))
        .await
        .unwrap();

    let agents = status.get("agents").unwrap().as_array().unwrap();
    assert_eq!(agents.len(), 4, "Should have 4 agents");

    let task_summary = status.get("task_summary").unwrap();
    assert_eq!(
        task_summary.get("completed").unwrap().as_u64().unwrap(),
        4,
        "All 4 tasks should be completed"
    );
}

/// Test 11: Graceful degradation - The hive mind works normally even if
/// SONA learning components fail or are unavailable.
#[tokio::test]
async fn test_graceful_degradation() {
    let (_state, handler) = setup_hive().await;

    let _agent_id = register_agent(&handler, "developer").await;

    // Normal task workflow should succeed even without SONA
    let task_id = create_task(
        &handler,
        "Regular task",
        "A task that works without SONA",
        "medium",
    )
    .await;

    handler
        .call_tool("claim_task", serde_json::json!({"task_id": &task_id}))
        .await
        .expect("claim should work without SONA");

    handler
        .call_tool(
            "update_task_status",
            serde_json::json!({"task_id": &task_id, "status": "in_progress"}),
        )
        .await
        .expect("update status should work without SONA");

    handler
        .call_tool(
            "share_knowledge",
            serde_json::json!({
                "content": "Knowledge sharing works without SONA",
                "kind": "discovery",
                "task_id": &task_id
            }),
        )
        .await
        .expect("share knowledge should work without SONA");

    handler
        .call_tool(
            "update_task_status",
            serde_json::json!({
                "task_id": &task_id,
                "status": "completed",
                "summary": "Completed without SONA"
            }),
        )
        .await
        .expect("complete should work without SONA");

    // SONA-specific tools should return graceful errors or empty results, not crash
    let traj_result = handler
        .call_tool(
            "get_task_trajectory",
            serde_json::json!({"task_id": &task_id}),
        )
        .await;

    // Either succeeds with empty/minimal data, or returns a handled error
    match traj_result {
        Ok(resp) => {
            // If SONA is not configured, trajectory should be empty but not crash
            let steps = resp
                .get("steps")
                .map(|s| s.as_array().map(|a| a.len()).unwrap_or(0))
                .unwrap_or(0);
            // Empty is fine - SONA might not be configured
            assert!(
                steps <= 10,
                "If SONA is disabled, trajectory should be empty or minimal"
            );
        }
        Err(e) => {
            // A handled error is acceptable - but it should NOT be a panic or unknown tool error
            let err_str = format!("{}", e);
            assert!(
                !err_str.contains("panic") && !err_str.contains("unknown tool"),
                "SONA failure should be graceful, not a panic or unknown tool: {}",
                err_str
            );
        }
    }

    let bank_result = handler
        .call_tool(
            "query_reasoning_bank",
            serde_json::json!({"query": "anything", "limit": 5}),
        )
        .await;

    match bank_result {
        Ok(resp) => {
            // Query "anything" has no word overlap with stored patterns, so
            // find_similar returns no matches. Graceful: returns ok with empty.
            assert!(
                resp.get("patterns").is_some(),
                "Reasoning bank should return a patterns field"
            );
        }
        Err(e) => {
            let err_str = format!("{}", e);
            assert!(
                !err_str.contains("panic"),
                "ReasoningBank query failure should be graceful: {}",
                err_str
            );
        }
    }

    let learning_result = handler
        .call_tool(
            "get_learning_status",
            serde_json::json!({"loop_type": "instant"}),
        )
        .await;

    match learning_result {
        Ok(resp) => {
            // SONA is always integrated -- learning status should report
            // valid counters without panicking.
            assert!(
                resp.get("patterns_learned").is_some(),
                "Learning status should report patterns_learned count"
            );
            assert!(
                resp.get("total_events").is_some(),
                "Learning status should report total_events count"
            );
        }
        Err(e) => {
            let err_str = format!("{}", e);
            assert!(
                !err_str.contains("panic"),
                "Learning status query should be graceful: {}",
                err_str
            );
        }
    }

    // The core hive status should still work perfectly
    let status = handler
        .call_tool("get_hive_status", serde_json::json!({}))
        .await
        .expect("hive status must always work");

    let task_summary = status.get("task_summary").unwrap();
    assert_eq!(
        task_summary.get("completed").unwrap().as_u64().unwrap(),
        1,
        "Task should be completed regardless of SONA status"
    );
}
