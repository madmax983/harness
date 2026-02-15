//! Agent MicroLoRA adaptation tests for SONA integration.
//!
//! MicroLoRA provides lightweight per-agent learning: each agent accumulates
//! trajectory data (action sequences + outcomes) and adapts its behavior over
//! time without affecting other agents. Weights persist across daemon restarts.
//!
//! These tests define the MicroLoRA API through TDD (RED phase).

use std::sync::Arc;

use harness_mcp::{HiveHandler, HiveState};
use harness_orchestrator::{OrchestratorConfig, ProcessManager};
use harness_persistence::{InMemoryRepository, Repository, Session};

/// Helper: create a fresh hive with an in-memory repo.
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

/// Helper: register a developer agent and return its ID.
async fn register_developer(handler: &HiveHandler<InMemoryRepository>) -> String {
    let resp = handler
        .call_tool("register_agent", serde_json::json!({"role": "developer"}))
        .await
        .expect("register developer");
    serde_json::from_value(resp.get("agent_id").unwrap().clone()).unwrap()
}

// ---------------------------------------------------------------------------
// Test 1: Agent learns from trajectory feedback
// ---------------------------------------------------------------------------
// A trajectory is a sequence of (action, outcome, reward) tuples that an agent
// produces while working on a task. MicroLoRA should ingest these trajectories
// and update per-agent adaptation weights so that future predictions improve.
//
// Expected tool: `record_agent_trajectory`
// Expected tool: `get_agent_lora_state`
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_agent_learns_from_trajectory() {
    let (_state, handler) = setup_hive().await;
    let agent_id = register_developer(&handler).await;

    // Record a positive trajectory: agent chose good action, got reward
    let record_resp = handler
        .call_tool(
            "record_agent_trajectory",
            serde_json::json!({
                "agent_id": agent_id,
                "trajectory": [
                    {
                        "action": "chose_jwt_over_sessions",
                        "context": "authentication design decision",
                        "outcome": "success",
                        "reward": 1.0
                    },
                    {
                        "action": "added_refresh_token_rotation",
                        "context": "security hardening",
                        "outcome": "success",
                        "reward": 0.8
                    }
                ]
            }),
        )
        .await
        .expect("record trajectory should succeed");

    // Should return confirmation with trajectory ID
    assert!(
        record_resp.get("trajectory_id").is_some(),
        "Should return a trajectory_id"
    );
    assert_eq!(
        record_resp.get("steps_recorded").unwrap().as_u64().unwrap(),
        2,
        "Should record 2 trajectory steps"
    );

    // Query the agent's LoRA state -- should reflect learning
    let lora_state = handler
        .call_tool(
            "get_agent_lora_state",
            serde_json::json!({"agent_id": agent_id}),
        )
        .await
        .expect("get lora state should succeed");

    assert!(
        lora_state.get("trajectories_ingested").is_some(),
        "Should report number of trajectories ingested"
    );
    assert_eq!(
        lora_state
            .get("trajectories_ingested")
            .unwrap()
            .as_u64()
            .unwrap(),
        1,
        "Should have ingested 1 trajectory"
    );
    assert!(
        lora_state.get("total_steps").is_some(),
        "Should report total steps across all trajectories"
    );
    assert_eq!(
        lora_state.get("total_steps").unwrap().as_u64().unwrap(),
        2,
        "Should have 2 total steps"
    );
    assert!(
        lora_state.get("mean_reward").is_some(),
        "Should compute mean reward across all steps"
    );

    let mean_reward = lora_state.get("mean_reward").unwrap().as_f64().unwrap();
    assert!(
        (mean_reward - 0.9).abs() < 0.01,
        "Mean reward should be ~0.9, got {}",
        mean_reward
    );
}

// ---------------------------------------------------------------------------
// Test 2: Predictions improve after learning from trajectories
// ---------------------------------------------------------------------------
// After ingesting trajectories, the agent should be able to predict better
// actions for similar contexts. The `apply_agent_optimization` tool returns
// a ranked set of suggested actions given a context, weighted by learned
// adaptation.
//
// Expected tool: `apply_agent_optimization`
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_apply_learned_optimizations() {
    let (_state, handler) = setup_hive().await;
    let agent_id = register_developer(&handler).await;

    // First: record several trajectories to build up learning signal
    // Trajectory 1: security-related decisions (positive)
    handler
        .call_tool(
            "record_agent_trajectory",
            serde_json::json!({
                "agent_id": agent_id,
                "trajectory": [
                    {
                        "action": "use_parameterized_queries",
                        "context": "database security",
                        "outcome": "success",
                        "reward": 1.0
                    },
                    {
                        "action": "add_input_validation",
                        "context": "api security",
                        "outcome": "success",
                        "reward": 0.9
                    }
                ]
            }),
        )
        .await
        .expect("record trajectory 1");

    // Trajectory 2: same domain, negative outcome for bad choice
    handler
        .call_tool(
            "record_agent_trajectory",
            serde_json::json!({
                "agent_id": agent_id,
                "trajectory": [
                    {
                        "action": "use_string_concatenation_for_sql",
                        "context": "database query building",
                        "outcome": "failure",
                        "reward": -1.0
                    }
                ]
            }),
        )
        .await
        .expect("record trajectory 2");

    // Now ask for optimization suggestions in a similar context
    let opt_resp = handler
        .call_tool(
            "apply_agent_optimization",
            serde_json::json!({
                "agent_id": agent_id,
                "context": "building a new database query",
                "candidate_actions": [
                    "use_parameterized_queries",
                    "use_string_concatenation_for_sql",
                    "use_orm_query_builder"
                ]
            }),
        )
        .await
        .expect("apply optimization should succeed");

    // Should return ranked actions with confidence scores
    let ranked = opt_resp.get("ranked_actions").unwrap().as_array().unwrap();
    assert!(
        !ranked.is_empty(),
        "Should return at least one ranked action"
    );

    // The top-ranked action should be the one with positive reward history
    let top_action = ranked[0].get("action").unwrap().as_str().unwrap();
    assert_eq!(
        top_action, "use_parameterized_queries",
        "Top action should be the one with best historical reward"
    );

    // The action with negative reward should be ranked last
    let last_action = ranked
        .last()
        .unwrap()
        .get("action")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(
        last_action, "use_string_concatenation_for_sql",
        "Worst-rewarded action should be ranked last"
    );

    // Each ranked action should have a confidence score
    for action in ranked {
        assert!(
            action.get("confidence").is_some(),
            "Each action should have a confidence score"
        );
        let confidence = action.get("confidence").unwrap().as_f64().unwrap();
        assert!(
            (0.0..=1.0).contains(&confidence),
            "Confidence should be in [0, 1], got {}",
            confidence
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3: Agent isolation -- Agent A's learning does NOT affect Agent B
// ---------------------------------------------------------------------------
// MicroLoRA weights are per-agent. Two agents can learn completely different
// things from the same domain without cross-contamination.
//
// This is critical for multi-agent coordination: a Tester agent's learning
// should not leak into a Developer agent's optimization suggestions.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_agent_isolation() {
    let (state, handler) = setup_hive().await;

    // Register two independent agents
    let agent_a_id = register_developer(&handler).await;

    let handler_b = HiveHandler::new(state.clone());
    let agent_b_resp = handler_b
        .call_tool("register_agent", serde_json::json!({"role": "tester"}))
        .await
        .expect("register tester");
    let agent_b_id: String =
        serde_json::from_value(agent_b_resp.get("agent_id").unwrap().clone()).unwrap();

    // Agent A learns: parameterized queries are good
    handler
        .call_tool(
            "record_agent_trajectory",
            serde_json::json!({
                "agent_id": agent_a_id,
                "trajectory": [
                    {
                        "action": "use_parameterized_queries",
                        "context": "database access",
                        "outcome": "success",
                        "reward": 1.0
                    }
                ]
            }),
        )
        .await
        .expect("agent A trajectory");

    // Agent B learns: string concatenation is good (different agent, different context)
    handler_b
        .call_tool(
            "record_agent_trajectory",
            serde_json::json!({
                "agent_id": agent_b_id,
                "trajectory": [
                    {
                        "action": "use_string_concatenation_for_sql",
                        "context": "test fixture generation",
                        "outcome": "success",
                        "reward": 1.0
                    }
                ]
            }),
        )
        .await
        .expect("agent B trajectory");

    // Agent A's optimization: should favor parameterized queries, NOT string concat
    let opt_a = handler
        .call_tool(
            "apply_agent_optimization",
            serde_json::json!({
                "agent_id": agent_a_id,
                "context": "database access pattern",
                "candidate_actions": [
                    "use_parameterized_queries",
                    "use_string_concatenation_for_sql"
                ]
            }),
        )
        .await
        .expect("agent A optimization");

    let ranked_a = opt_a.get("ranked_actions").unwrap().as_array().unwrap();
    let top_a = ranked_a[0].get("action").unwrap().as_str().unwrap();
    assert_eq!(
        top_a, "use_parameterized_queries",
        "Agent A should favor parameterized queries (its own learning)"
    );

    // Agent B's optimization: should favor string concat (its own learning)
    let opt_b = handler_b
        .call_tool(
            "apply_agent_optimization",
            serde_json::json!({
                "agent_id": agent_b_id,
                "context": "test data generation",
                "candidate_actions": [
                    "use_parameterized_queries",
                    "use_string_concatenation_for_sql"
                ]
            }),
        )
        .await
        .expect("agent B optimization");

    let ranked_b = opt_b.get("ranked_actions").unwrap().as_array().unwrap();
    let top_b = ranked_b[0].get("action").unwrap().as_str().unwrap();
    assert_eq!(
        top_b, "use_string_concatenation_for_sql",
        "Agent B should favor string concat (its own learning, isolated from A)"
    );

    // Verify the LoRA states are completely independent
    let state_a = handler
        .call_tool(
            "get_agent_lora_state",
            serde_json::json!({"agent_id": agent_a_id}),
        )
        .await
        .expect("get agent A lora state");

    let state_b = handler_b
        .call_tool(
            "get_agent_lora_state",
            serde_json::json!({"agent_id": agent_b_id}),
        )
        .await
        .expect("get agent B lora state");

    // Both should have exactly 1 trajectory
    assert_eq!(
        state_a
            .get("trajectories_ingested")
            .unwrap()
            .as_u64()
            .unwrap(),
        1
    );
    assert_eq!(
        state_b
            .get("trajectories_ingested")
            .unwrap()
            .as_u64()
            .unwrap(),
        1
    );

    // Agent IDs in the state should match the requesting agent
    assert_eq!(
        state_a.get("agent_id").unwrap().as_str().unwrap(),
        agent_a_id,
        "LoRA state should be tagged with agent A's ID"
    );
    assert_eq!(
        state_b.get("agent_id").unwrap().as_str().unwrap(),
        agent_b_id,
        "LoRA state should be tagged with agent B's ID"
    );
}

// ---------------------------------------------------------------------------
// Test 4: MicroLoRA weights persist across daemon restarts
// ---------------------------------------------------------------------------
// After recording trajectories and shutting down, the LoRA state should
// survive and be retrievable after restart. This tests the persistence
// layer integration.
//
// Expected tool: `persist_agent_lora` (explicit flush)
// Expected tool: `restore_agent_lora` (load from storage)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_persist_micro_lora() {
    // Phase 1: Create hive, record trajectories, persist
    let repo = Arc::new(InMemoryRepository::new());
    let session = Session::new(8);
    repo.create_session(&session).await.unwrap();

    let config = OrchestratorConfig::default();
    let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));

    let state = Arc::new(HiveState::new(
        session.clone(),
        repo.clone(),
        process_manager,
    ));
    let handler = HiveHandler::new(state.clone());

    let agent_id = register_developer(&handler).await;

    // Record trajectories
    handler
        .call_tool(
            "record_agent_trajectory",
            serde_json::json!({
                "agent_id": agent_id,
                "trajectory": [
                    {
                        "action": "use_tokio_spawn",
                        "context": "async task management",
                        "outcome": "success",
                        "reward": 0.95
                    },
                    {
                        "action": "avoid_blocking_in_async",
                        "context": "async performance",
                        "outcome": "success",
                        "reward": 0.85
                    }
                ]
            }),
        )
        .await
        .expect("record trajectory");

    // Explicitly persist the LoRA state
    let persist_resp = handler
        .call_tool(
            "persist_agent_lora",
            serde_json::json!({"agent_id": agent_id}),
        )
        .await
        .expect("persist lora should succeed");

    assert!(
        persist_resp.get("persisted").unwrap().as_bool().unwrap(),
        "Should confirm persistence"
    );

    // Phase 2: Simulate restart -- create new state from same repo
    let config2 = OrchestratorConfig::default();
    let process_manager2 = Arc::new(ProcessManager::new(config2, repo.clone()));

    let state2 = Arc::new(HiveState::new(session, repo.clone(), process_manager2));
    let handler2 = HiveHandler::new(state2.clone());

    // Restore the LoRA state for the agent
    let restore_resp = handler2
        .call_tool(
            "restore_agent_lora",
            serde_json::json!({"agent_id": agent_id}),
        )
        .await
        .expect("restore lora should succeed");

    assert!(
        restore_resp.get("restored").unwrap().as_bool().unwrap(),
        "Should confirm restoration"
    );

    // Verify the restored state matches original
    let restored_state = handler2
        .call_tool(
            "get_agent_lora_state",
            serde_json::json!({"agent_id": agent_id}),
        )
        .await
        .expect("get restored lora state");

    assert_eq!(
        restored_state
            .get("trajectories_ingested")
            .unwrap()
            .as_u64()
            .unwrap(),
        1,
        "Restored state should have 1 trajectory"
    );
    assert_eq!(
        restored_state.get("total_steps").unwrap().as_u64().unwrap(),
        2,
        "Restored state should have 2 total steps"
    );

    let restored_mean = restored_state.get("mean_reward").unwrap().as_f64().unwrap();
    assert!(
        (restored_mean - 0.9).abs() < 0.01,
        "Restored mean reward should be ~0.9, got {}",
        restored_mean
    );
}

// ---------------------------------------------------------------------------
// Test 5: Multiple agents learn simultaneously without interference
// ---------------------------------------------------------------------------
// Concurrent trajectory recording across multiple agents should be safe.
// This exercises the thread-safety of MicroLoRA's per-agent weight storage.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_concurrent_agent_learning() {
    let (state, handler) = setup_hive().await;

    // Register strategoi first (for task creation)
    handler
        .call_tool("register_agent", serde_json::json!({"role": "strategoi"}))
        .await
        .expect("register strategoi");

    // Spawn 5 agents that all learn concurrently
    let mut handles = vec![];
    for i in 0..5 {
        let state_clone = state.clone();
        let handle = tokio::spawn(async move {
            let agent_handler = HiveHandler::new(state_clone);
            let resp = agent_handler
                .call_tool("register_agent", serde_json::json!({"role": "developer"}))
                .await
                .expect("register agent");
            let agent_id: String =
                serde_json::from_value(resp.get("agent_id").unwrap().clone()).unwrap();

            // Each agent records multiple trajectories
            for j in 0..3 {
                let reward = (i as f64 * 0.1) + (j as f64 * 0.05);
                agent_handler
                    .call_tool(
                        "record_agent_trajectory",
                        serde_json::json!({
                            "agent_id": agent_id,
                            "trajectory": [
                                {
                                    "action": format!("agent_{}_action_{}", i, j),
                                    "context": format!("context_{}", j),
                                    "outcome": "success",
                                    "reward": reward.min(1.0)
                                }
                            ]
                        }),
                    )
                    .await
                    .expect("record trajectory");
            }

            // Query own LoRA state
            let lora_state = agent_handler
                .call_tool(
                    "get_agent_lora_state",
                    serde_json::json!({"agent_id": agent_id}),
                )
                .await
                .expect("get lora state");

            (
                i,
                agent_id,
                lora_state
                    .get("trajectories_ingested")
                    .unwrap()
                    .as_u64()
                    .unwrap(),
                lora_state.get("total_steps").unwrap().as_u64().unwrap(),
            )
        });
        handles.push(handle);
    }

    let results = futures::future::join_all(handles).await;

    // All 5 agents should have completed successfully
    assert_eq!(results.len(), 5);

    for result in &results {
        let (idx, agent_id, trajectories, steps) = result.as_ref().unwrap();

        // Each agent should have 3 trajectories and 3 total steps (1 step each)
        assert_eq!(
            *trajectories, 3,
            "Agent {} ({}) should have 3 trajectories, got {}",
            idx, agent_id, trajectories
        );
        assert_eq!(
            *steps, 3,
            "Agent {} ({}) should have 3 total steps, got {}",
            idx, agent_id, steps
        );
    }

    // Verify all agent IDs are unique (no cross-contamination)
    let agent_ids: Vec<&String> = results.iter().map(|r| &r.as_ref().unwrap().1).collect();
    let unique_ids: std::collections::HashSet<&String> = agent_ids.iter().copied().collect();
    assert_eq!(unique_ids.len(), 5, "All 5 agents should have unique IDs");
}
