//! Hive BaseLoRA collective learning tests.
//!
//! Tests the BaseLoRA aggregation layer that enables collective intelligence
//! across all agents in the hive. BaseLoRA aggregates individual agent
//! MicroLoRA adaptations into a shared "hive wisdom" model that benefits
//! every agent - especially newcomers who inherit the collective knowledge.
//!
//! These tests define the API contract for:
//! - BaseLoRA aggregation from agent MicroLoRAs
//! - FederatedCoordinator for multi-agent sync
//! - Collective improvement over time
//! - New agent onboarding with inherited knowledge
//! - Background continuous learning loop

use std::sync::Arc;
use std::time::Duration;

use harness_mcp::{HiveHandler, HiveState};
use harness_orchestrator::{OrchestratorConfig, ProcessManager};
use harness_persistence::{InMemoryRepository, Repository, Session};

use harness_sona::{
    AggregationStrategy, BaseLoRA, BaseLoRAConfig, ContributionWeight, FederatedConfig,
    FederatedCoordinator, HiveLearningConfig, HiveLearningService, LearningLoop, LoRADelta,
};

/// Helper: set up an in-memory hive with multiple registered agents.
async fn setup_multi_agent_hive(
    agent_count: usize,
) -> (
    Arc<HiveState<InMemoryRepository>>,
    Vec<HiveHandler<InMemoryRepository>>,
) {
    let repo = Arc::new(InMemoryRepository::new());
    let session = Session::new(16);
    repo.create_session(&session).await.unwrap();

    let config = OrchestratorConfig::default();
    let process_manager = Arc::new(ProcessManager::new(config, repo.clone()));
    let state = Arc::new(HiveState::new(session, repo, process_manager));

    let mut handlers = Vec::new();
    for i in 0..agent_count {
        let handler = HiveHandler::new(state.clone());
        let role = if i == 0 { "strategoi" } else { "developer" };
        handler
            .call_tool("register_agent", serde_json::json!({"role": role}))
            .await
            .expect("register agent");
        handlers.push(handler);
    }

    (state, handlers)
}

// ============================================================
// Test 1: BaseLoRA aggregates learning from multiple agents
// ============================================================

/// BaseLoRA should aggregate MicroLoRA deltas from individual agents into
/// a shared collective model. When agents share knowledge and complete tasks,
/// their individual adaptations (MicroLoRAs) are combined via weighted
/// averaging into the BaseLoRA that represents the hive's collective wisdom.
#[tokio::test]
async fn test_base_lora_aggregates_learning() {
    let (_state, handlers) = setup_multi_agent_hive(4).await;

    // Create a BaseLoRA with default config
    let config = BaseLoRAConfig {
        rank: 8,
        alpha: 16.0,
        aggregation_strategy: AggregationStrategy::WeightedAverage,
        min_participants: 2,
        staleness_threshold_secs: 3600,
    };
    let base_lora = BaseLoRA::new(config);

    // Simulate agents completing tasks and generating MicroLoRA deltas
    // Agent 1: completed 3 tasks about authentication
    let delta_1 = LoRADelta {
        agent_id: handlers[1].agent_id().await.unwrap(),
        domain: "authentication".to_string(),
        weights: vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
        task_count: 3,
        success_rate: 0.9,
        timestamp: chrono::Utc::now(),
    };

    // Agent 2: completed 5 tasks about authentication (more experienced)
    let delta_2 = LoRADelta {
        agent_id: handlers[2].agent_id().await.unwrap(),
        domain: "authentication".to_string(),
        weights: vec![0.15, 0.25, 0.35, 0.45, 0.55, 0.65, 0.75, 0.85],
        task_count: 5,
        success_rate: 0.95,
        timestamp: chrono::Utc::now(),
    };

    // Agent 3: completed 1 task about authentication (less experienced)
    let delta_3 = LoRADelta {
        agent_id: handlers[3].agent_id().await.unwrap(),
        domain: "authentication".to_string(),
        weights: vec![0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4],
        task_count: 1,
        success_rate: 0.8,
        timestamp: chrono::Utc::now(),
    };

    // Contribute deltas to BaseLoRA
    base_lora.contribute(delta_1).await.unwrap();
    base_lora.contribute(delta_2).await.unwrap();
    base_lora.contribute(delta_3).await.unwrap();

    // Aggregate the deltas
    let aggregated = base_lora.aggregate().await.unwrap();

    // Aggregated weights should exist and have the correct rank
    assert_eq!(
        aggregated.weights.len(),
        8,
        "Aggregated weights should match rank"
    );

    // With weighted averaging, agent 2's contribution should have the
    // most influence (highest task_count * success_rate)
    // The aggregated weights should be closer to agent 2's values
    let midpoint = aggregated.weights[4];
    assert!(
        midpoint > 0.3 && midpoint < 0.7,
        "Aggregated midpoint weight should be a blend, got: {}",
        midpoint
    );

    // Verify contribution tracking
    let stats = base_lora.contribution_stats().await;
    assert_eq!(stats.total_contributors, 3, "Should track 3 contributors");
    assert_eq!(stats.total_deltas, 3, "Should have 3 deltas");
    assert_eq!(stats.domains.len(), 1, "Should have 1 domain");
    assert!(stats.domains.contains(&"authentication".to_string()));
}

/// BaseLoRA should support multiple aggregation strategies.
#[tokio::test]
async fn test_base_lora_aggregation_strategies() {
    // Test FedAvg (simple average)
    let config_avg = BaseLoRAConfig {
        rank: 4,
        alpha: 8.0,
        aggregation_strategy: AggregationStrategy::FederatedAverage,
        min_participants: 2,
        staleness_threshold_secs: 3600,
    };
    let base_avg = BaseLoRA::new(config_avg);

    let delta_a = LoRADelta::synthetic("domain_a", vec![1.0, 2.0, 3.0, 4.0], 3, 1.0);
    let delta_b = LoRADelta::synthetic("domain_a", vec![3.0, 4.0, 5.0, 6.0], 3, 1.0);

    base_avg.contribute(delta_a).await.unwrap();
    base_avg.contribute(delta_b).await.unwrap();

    let result_avg = base_avg.aggregate().await.unwrap();
    // FedAvg: simple average => [2.0, 3.0, 4.0, 5.0]
    assert!(
        (result_avg.weights[0] - 2.0).abs() < 0.01,
        "FedAvg should produce simple average, got: {}",
        result_avg.weights[0]
    );

    // Test WeightedAverage (weighted by task_count * success_rate)
    let config_weighted = BaseLoRAConfig {
        rank: 4,
        alpha: 8.0,
        aggregation_strategy: AggregationStrategy::WeightedAverage,
        min_participants: 2,
        staleness_threshold_secs: 3600,
    };
    let base_weighted = BaseLoRA::new(config_weighted);

    let delta_c = LoRADelta::synthetic("domain_a", vec![1.0, 2.0, 3.0, 4.0], 1, 0.5);
    let delta_d = LoRADelta::synthetic("domain_a", vec![3.0, 4.0, 5.0, 6.0], 10, 1.0);

    base_weighted.contribute(delta_c).await.unwrap();
    base_weighted.contribute(delta_d).await.unwrap();

    let result_weighted = base_weighted.aggregate().await.unwrap();
    // Weighted: delta_d should dominate (weight=10*1.0=10 vs weight=1*0.5=0.5)
    assert!(
        result_weighted.weights[0] > 2.5,
        "Weighted average should favor higher-performing delta, got: {}",
        result_weighted.weights[0]
    );
}

// ============================================================
// Test 2: FederatedCoordinator for multi-agent sync
// ============================================================

/// The FederatedCoordinator orchestrates the collection and distribution of
/// LoRA updates across the hive. It manages rounds of federated aggregation,
/// ensuring all agents stay in sync.
#[tokio::test]
async fn test_federated_coordinator() {
    let (state, handlers) = setup_multi_agent_hive(5).await;

    let fed_config = FederatedConfig {
        round_interval: Duration::from_millis(100),
        min_participants: 2,
        staleness_window: Duration::from_secs(300),
        convergence_threshold: 0.01,
    };

    let base_config = BaseLoRAConfig {
        rank: 8,
        alpha: 16.0,
        aggregation_strategy: AggregationStrategy::WeightedAverage,
        min_participants: 2,
        staleness_threshold_secs: 3600,
    };

    let coordinator =
        FederatedCoordinator::new(fed_config, base_config, state.repository().clone());

    // Start a federated round
    let round_id = coordinator.start_round().await.unwrap();
    assert!(!round_id.is_empty(), "Round should have an ID");

    // Agents submit their local updates for this round
    for (i, handler) in handlers.iter().enumerate().skip(1) {
        let agent_id = handler.agent_id().await.unwrap();
        let delta = LoRADelta {
            agent_id,
            domain: "testing".to_string(),
            weights: vec![i as f32 * 0.1; 8],
            task_count: i as u32 + 1,
            success_rate: 0.8 + (i as f64 * 0.05),
            timestamp: chrono::Utc::now(),
        };
        coordinator.submit_update(&round_id, delta).await.unwrap();
    }

    // Check round status
    let status = coordinator.round_status(&round_id).await.unwrap();
    assert_eq!(status.participants, 4, "4 agents submitted updates");
    assert!(!status.is_complete, "Round should not be complete yet");

    // Complete the round (triggers aggregation)
    let round_result = coordinator.complete_round(&round_id).await.unwrap();
    assert!(round_result.is_complete, "Round should be complete now");
    assert!(
        round_result.aggregated_weights.is_some(),
        "Should have aggregated weights"
    );

    // Verify round history
    let history = coordinator.round_history(10).await.unwrap();
    assert_eq!(history.len(), 1, "Should have 1 completed round");
    assert_eq!(history[0].round_id, round_id);
}

/// FederatedCoordinator should reject stale updates.
#[tokio::test]
async fn test_federated_coordinator_rejects_stale() {
    let (state, _handlers) = setup_multi_agent_hive(2).await;

    let fed_config = FederatedConfig {
        round_interval: Duration::from_millis(100),
        min_participants: 1,
        staleness_window: Duration::from_millis(1), // Extremely short window
        convergence_threshold: 0.01,
    };

    let base_config = BaseLoRAConfig {
        rank: 4,
        alpha: 8.0,
        aggregation_strategy: AggregationStrategy::FederatedAverage,
        min_participants: 1,
        staleness_threshold_secs: 1,
    };

    let coordinator =
        FederatedCoordinator::new(fed_config, base_config, state.repository().clone());

    let round_id = coordinator.start_round().await.unwrap();

    // Create a delta with an old timestamp
    let stale_delta = LoRADelta {
        agent_id: harness_persistence::AgentId::new(),
        domain: "stale".to_string(),
        weights: vec![0.5; 4],
        task_count: 1,
        success_rate: 1.0,
        timestamp: chrono::Utc::now() - chrono::Duration::hours(2),
    };

    // Should reject stale delta
    let result = coordinator.submit_update(&round_id, stale_delta).await;
    assert!(result.is_err(), "Should reject stale delta: {:?}", result);
}

// ============================================================
// Test 3: Collective improvement over time
// ============================================================

/// The hive should measurably improve over multiple federated rounds.
/// As agents complete more tasks and share knowledge, the BaseLoRA
/// should converge toward better collective performance.
#[tokio::test]
async fn test_collective_improvement() {
    let (state, handlers) = setup_multi_agent_hive(4).await;

    let fed_config = FederatedConfig {
        round_interval: Duration::from_millis(50),
        min_participants: 2,
        staleness_window: Duration::from_secs(300),
        convergence_threshold: 0.001,
    };

    let base_config = BaseLoRAConfig {
        rank: 4,
        alpha: 8.0,
        aggregation_strategy: AggregationStrategy::WeightedAverage,
        min_participants: 2,
        staleness_threshold_secs: 3600,
    };

    let coordinator =
        FederatedCoordinator::new(fed_config, base_config, state.repository().clone());

    let mut merit_scores: Vec<f64> = Vec::new();

    // Run 5 federated rounds with improving agent performance
    for round_num in 0..5u32 {
        let round_id = coordinator.start_round().await.unwrap();

        for (i, handler) in handlers.iter().enumerate().skip(1) {
            let agent_id = handler.agent_id().await.unwrap();
            // Agents improve over rounds (simulate learning)
            let improvement = round_num as f32 * 0.1;
            let base_weight = (i as f32 + 1.0) * 0.1;
            let delta = LoRADelta {
                agent_id,
                domain: "collective_test".to_string(),
                weights: vec![base_weight + improvement; 4],
                task_count: round_num + 1,
                success_rate: 0.7 + (round_num as f64 * 0.05),
                timestamp: chrono::Utc::now(),
            };
            coordinator.submit_update(&round_id, delta).await.unwrap();
        }

        let _result = coordinator.complete_round(&round_id).await.unwrap();

        // Compute collective merit score for this round
        let merit = coordinator.compute_collective_merit().await.unwrap();
        merit_scores.push(merit.score);
    }

    // Verify improvement: later rounds should have higher merit
    assert!(merit_scores.len() == 5, "Should have 5 merit scores");

    // The collective merit should generally increase (allowing some noise)
    let first_half_avg: f64 = merit_scores[..2].iter().sum::<f64>() / 2.0;
    let second_half_avg: f64 = merit_scores[3..].iter().sum::<f64>() / 2.0;
    assert!(
        second_half_avg > first_half_avg,
        "Second half merit ({}) should exceed first half ({})",
        second_half_avg,
        first_half_avg
    );

    // Verify convergence metrics
    let convergence = coordinator.convergence_metrics().await.unwrap();
    assert!(
        convergence.rounds_completed == 5,
        "Should have completed 5 rounds"
    );
    assert!(
        convergence.weight_drift >= 0.0,
        "Weight drift should be non-negative"
    );
}

/// CollectiveMerit should capture multiple dimensions of hive performance.
#[tokio::test]
async fn test_collective_merit_dimensions() {
    let (state, handlers) = setup_multi_agent_hive(3).await;

    let hive_config = HiveLearningConfig {
        base_lora_rank: 4,
        aggregation_strategy: AggregationStrategy::WeightedAverage,
        round_interval: Duration::from_millis(100),
        min_participants: 1,
    };

    let service = HiveLearningService::new(hive_config, state.repository().clone());

    // Have agents complete tasks and share knowledge
    for (i, handler) in handlers.iter().enumerate().skip(1) {
        // Create and complete a task
        let task_resp = handler
            .call_tool(
                "create_task",
                serde_json::json!({
                    "title": format!("Merit test task {}", i),
                    "description": "Testing collective merit",
                    "priority": "medium"
                }),
            )
            .await
            .unwrap();
        let task_id: String =
            serde_json::from_value(task_resp.get("task_id").unwrap().clone()).unwrap();

        handler
            .call_tool(
                "update_task_status",
                serde_json::json!({"task_id": task_id, "status": "completed", "summary": "Done"}),
            )
            .await
            .unwrap();

        // Share some knowledge
        handler
            .call_tool(
                "share_knowledge",
                serde_json::json!({
                    "content": format!("Learning {} from agent {}", i, i),
                    "kind": "discovery",
                    "task_id": task_id
                }),
            )
            .await
            .unwrap();
    }

    let merit = service.compute_collective_merit().await.unwrap();

    // Merit should have multiple dimensions
    assert!(merit.score > 0.0, "Overall merit score should be positive");
    assert!(
        merit.knowledge_coverage > 0.0,
        "Knowledge coverage should be positive"
    );
    assert!(
        merit.task_success_rate > 0.0,
        "Task success rate should be positive"
    );
    assert!(
        merit.agent_participation > 0.0,
        "Agent participation should be positive"
    );
    assert!(
        merit.knowledge_diversity >= 0.0,
        "Knowledge diversity should be non-negative"
    );
}

// ============================================================
// Test 4: New agents benefit from collective knowledge
// ============================================================

/// When a new agent joins the hive, it should immediately benefit from
/// the BaseLoRA that encodes the collective wisdom of all previous agents.
/// This tests the "standing on the shoulders of giants" capability.
#[tokio::test]
async fn test_new_agent_benefits() {
    let (state, handlers) = setup_multi_agent_hive(3).await;

    let hive_config = HiveLearningConfig {
        base_lora_rank: 8,
        aggregation_strategy: AggregationStrategy::WeightedAverage,
        round_interval: Duration::from_millis(100),
        min_participants: 2,
    };

    let service = HiveLearningService::new(hive_config, state.repository().clone());

    // Existing agents build up collective knowledge
    for (i, handler) in handlers.iter().enumerate().skip(1) {
        let agent_id = handler.agent_id().await.unwrap();

        // Simulate agent work generating LoRA deltas
        let delta = LoRADelta {
            agent_id,
            domain: "database_design".to_string(),
            weights: vec![(i as f32 + 1.0) * 0.2; 8],
            task_count: 10,
            success_rate: 0.9,
            timestamp: chrono::Utc::now(),
        };
        service.contribute_delta(delta).await.unwrap();
    }

    // Run an aggregation round
    service.run_aggregation_round().await.unwrap();

    // Verify BaseLoRA exists and has been trained
    let base_state = service.base_lora_state().await.unwrap();
    assert!(
        base_state.is_initialized,
        "BaseLoRA should be initialized after aggregation"
    );
    assert!(
        !base_state.weights.is_empty(),
        "BaseLoRA should have aggregated weights"
    );

    // Now a NEW agent joins the hive
    let new_handler = HiveHandler::new(state.clone());
    new_handler
        .call_tool("register_agent", serde_json::json!({"role": "developer"}))
        .await
        .unwrap();
    let new_agent_id = new_handler.agent_id().await.unwrap();

    // The new agent should receive the BaseLoRA initialization
    let inherited = service.initialize_new_agent(new_agent_id).await.unwrap();

    assert!(
        inherited.base_weights.is_some(),
        "New agent should receive base weights"
    );
    assert_eq!(
        inherited.base_weights.unwrap().len(),
        8,
        "Inherited weights should match BaseLoRA rank"
    );
    assert!(
        inherited
            .domains_covered
            .contains(&"database_design".to_string()),
        "New agent should know about existing domains"
    );
    assert!(
        inherited.collective_rounds > 0,
        "Should report how many collective rounds were incorporated"
    );
}

/// New agent's initial performance should be better than starting from scratch.
#[tokio::test]
async fn test_new_agent_cold_start_mitigation() {
    let (state, handlers) = setup_multi_agent_hive(4).await;

    let hive_config = HiveLearningConfig {
        base_lora_rank: 4,
        aggregation_strategy: AggregationStrategy::WeightedAverage,
        round_interval: Duration::from_millis(50),
        min_participants: 2,
    };

    let service = HiveLearningService::new(hive_config, state.repository().clone());

    // Build up substantial collective knowledge over multiple rounds
    for round in 0..3 {
        for (i, handler) in handlers.iter().enumerate().skip(1) {
            let agent_id = handler.agent_id().await.unwrap();
            let delta = LoRADelta {
                agent_id,
                domain: "api_design".to_string(),
                weights: vec![(i as f32 + round as f32) * 0.15; 4],
                task_count: (round + 1) * 3,
                success_rate: 0.75 + (round as f64 * 0.08),
                timestamp: chrono::Utc::now(),
            };
            service.contribute_delta(delta).await.unwrap();
        }
        service.run_aggregation_round().await.unwrap();
    }

    // Create a new agent WITHOUT base initialization
    let cold_agent = HiveHandler::new(state.clone());
    cold_agent
        .call_tool("register_agent", serde_json::json!({"role": "developer"}))
        .await
        .unwrap();
    let cold_id = cold_agent.agent_id().await.unwrap();

    // Create a new agent WITH base initialization
    let warm_agent = HiveHandler::new(state.clone());
    warm_agent
        .call_tool("register_agent", serde_json::json!({"role": "developer"}))
        .await
        .unwrap();
    let warm_id = warm_agent.agent_id().await.unwrap();
    service.initialize_new_agent(warm_id).await.unwrap();

    // Compare their initial capability
    let cold_capability = service.agent_capability_score(cold_id).await.unwrap();
    let warm_capability = service.agent_capability_score(warm_id).await.unwrap();

    assert!(
        warm_capability > cold_capability,
        "Warm-started agent ({}) should outperform cold-started ({})",
        warm_capability,
        cold_capability
    );
}

// ============================================================
// Test 5: Background continuous learning loop
// ============================================================

/// The background learning loop should continuously aggregate agent
/// contributions and update the BaseLoRA without blocking agent work.
#[tokio::test]
async fn test_background_learning_loop() {
    let (state, handlers) = setup_multi_agent_hive(3).await;

    let hive_config = HiveLearningConfig {
        base_lora_rank: 4,
        aggregation_strategy: AggregationStrategy::FederatedAverage,
        round_interval: Duration::from_millis(50),
        min_participants: 1,
    };

    let service = Arc::new(HiveLearningService::new(
        hive_config,
        state.repository().clone(),
    ));

    // Start the background learning loop
    let loop_handle = LearningLoop::start(service.clone());

    // Simulate agents working concurrently while the loop runs
    let mut agent_tasks = Vec::new();
    for handler in handlers.iter().skip(1) {
        let agent_id = handler.agent_id().await.unwrap();
        let svc = service.clone();

        let task = tokio::spawn(async move {
            for i in 0..5 {
                let delta = LoRADelta {
                    agent_id,
                    domain: "background_test".to_string(),
                    weights: vec![i as f32 * 0.1; 4],
                    task_count: i + 1,
                    success_rate: 0.8 + (i as f64 * 0.04),
                    timestamp: chrono::Utc::now(),
                };
                svc.contribute_delta(delta).await.unwrap();
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        agent_tasks.push(task);
    }

    // Wait for agents to finish contributing
    for task in agent_tasks {
        task.await.unwrap();
    }

    // Give the background loop time to process
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Stop the learning loop gracefully
    loop_handle.stop().await.unwrap();

    // Verify the loop processed contributions
    let stats = service.loop_stats().await.unwrap();
    assert!(
        stats.rounds_completed > 0,
        "Background loop should have completed at least 1 round, got: {}",
        stats.rounds_completed
    );
    assert!(
        stats.total_contributions_processed > 0,
        "Should have processed contributions"
    );

    // BaseLoRA should be updated
    let base_state = service.base_lora_state().await.unwrap();
    assert!(
        base_state.is_initialized,
        "BaseLoRA should be initialized from background processing"
    );
}

/// The learning loop should handle agent disconnects gracefully.
#[tokio::test]
async fn test_background_loop_handles_disconnects() {
    let (state, handlers) = setup_multi_agent_hive(3).await;

    let hive_config = HiveLearningConfig {
        base_lora_rank: 4,
        aggregation_strategy: AggregationStrategy::FederatedAverage,
        round_interval: Duration::from_millis(50),
        min_participants: 1,
    };

    let service = Arc::new(HiveLearningService::new(
        hive_config,
        state.repository().clone(),
    ));

    // Contribute some deltas
    for handler in handlers.iter().skip(1) {
        let agent_id = handler.agent_id().await.unwrap();
        let delta = LoRADelta {
            agent_id,
            domain: "disconnect_test".to_string(),
            weights: vec![0.5; 4],
            task_count: 2,
            success_rate: 0.9,
            timestamp: chrono::Utc::now(),
        };
        service.contribute_delta(delta).await.unwrap();
    }

    // Disconnect one of the agents
    let disconnecting_id = handlers[1].agent_id().await.unwrap();
    service
        .handle_agent_disconnect(disconnecting_id)
        .await
        .unwrap();

    // Run aggregation - should still work with remaining agent contributions
    let result = service.run_aggregation_round().await.unwrap();
    assert!(
        result.is_complete,
        "Aggregation should succeed even after agent disconnect"
    );

    // The disconnected agent's stale contributions should be handled
    let stats = service.contribution_stats().await;
    assert!(
        stats.active_contributors < 3,
        "Should have fewer active contributors after disconnect"
    );
}

/// The learning loop should respect the min_participants threshold.
#[tokio::test]
async fn test_learning_loop_min_participants() {
    let (state, _handlers) = setup_multi_agent_hive(1).await;

    let hive_config = HiveLearningConfig {
        base_lora_rank: 4,
        aggregation_strategy: AggregationStrategy::FederatedAverage,
        round_interval: Duration::from_millis(50),
        min_participants: 3, // Require 3 participants, but we only have 1
    };

    let service = HiveLearningService::new(hive_config, state.repository().clone());

    // Single agent contributes
    let delta = LoRADelta::synthetic("lone_wolf", vec![0.5; 4], 1, 1.0);
    service.contribute_delta(delta).await.unwrap();

    // Aggregation should fail or skip due to insufficient participants
    let result = service.run_aggregation_round().await;
    assert!(
        result.is_err() || !result.unwrap().is_complete,
        "Should not complete round without minimum participants"
    );
}

// ============================================================
// Additional edge cases and robustness tests
// ============================================================

/// BaseLoRA should handle domain-specific aggregation.
/// Different domains should maintain separate weight sets.
#[tokio::test]
async fn test_domain_specific_aggregation() {
    let config = BaseLoRAConfig {
        rank: 4,
        alpha: 8.0,
        aggregation_strategy: AggregationStrategy::WeightedAverage,
        min_participants: 1,
        staleness_threshold_secs: 3600,
    };
    let base_lora = BaseLoRA::new(config);

    // Contribute to two different domains
    let auth_delta = LoRADelta::synthetic("authentication", vec![0.9, 0.8, 0.7, 0.6], 5, 0.95);
    let db_delta = LoRADelta::synthetic("database", vec![0.1, 0.2, 0.3, 0.4], 3, 0.85);

    base_lora.contribute(auth_delta).await.unwrap();
    base_lora.contribute(db_delta).await.unwrap();

    // Aggregate per-domain
    let auth_weights = base_lora.aggregate_domain("authentication").await.unwrap();
    let db_weights = base_lora.aggregate_domain("database").await.unwrap();

    // Weights should be domain-specific, not blended
    assert!(
        auth_weights.weights[0] > 0.5,
        "Auth weights should reflect auth contributions, got: {}",
        auth_weights.weights[0]
    );
    assert!(
        db_weights.weights[0] < 0.5,
        "DB weights should reflect DB contributions, got: {}",
        db_weights.weights[0]
    );

    // List known domains
    let domains = base_lora.known_domains().await;
    assert_eq!(domains.len(), 2);
    assert!(domains.contains(&"authentication".to_string()));
    assert!(domains.contains(&"database".to_string()));
}

/// Contribution weights should reflect agent reliability and experience.
#[tokio::test]
async fn test_contribution_weight_calculation() {
    // Expert agent: many tasks, high success rate
    let expert_weight = ContributionWeight::compute(
        /* task_count */ 50, /* success_rate */ 0.95, /* recency_factor */ 1.0,
    );

    // Novice agent: few tasks, low success rate
    let novice_weight = ContributionWeight::compute(
        /* task_count */ 2, /* success_rate */ 0.60, /* recency_factor */ 1.0,
    );

    // Stale agent: was good but hasn't contributed recently
    let stale_weight = ContributionWeight::compute(
        /* task_count */ 30, /* success_rate */ 0.90,
        /* recency_factor */ 0.1, // Very stale
    );

    assert!(
        expert_weight.value > novice_weight.value,
        "Expert ({}) should outweigh novice ({})",
        expert_weight.value,
        novice_weight.value
    );

    assert!(
        expert_weight.value > stale_weight.value,
        "Active expert ({}) should outweigh stale agent ({})",
        expert_weight.value,
        stale_weight.value
    );

    assert!(
        stale_weight.value > novice_weight.value,
        "Stale experienced agent ({}) should still outweigh novice ({})",
        stale_weight.value,
        novice_weight.value
    );
}

/// BaseLoRA should persist its state across daemon restarts.
#[tokio::test]
async fn test_base_lora_persistence() {
    let repo = Arc::new(InMemoryRepository::new());
    let session = Session::new(8);
    repo.create_session(&session).await.unwrap();

    let hive_config = HiveLearningConfig {
        base_lora_rank: 4,
        aggregation_strategy: AggregationStrategy::FederatedAverage,
        round_interval: Duration::from_millis(100),
        min_participants: 1,
    };

    // First "session": train the BaseLoRA
    {
        let service = HiveLearningService::new(hive_config.clone(), repo.clone());
        let delta = LoRADelta::synthetic("persistence_test", vec![0.42, 0.84, 0.21, 0.63], 5, 0.9);
        service.contribute_delta(delta).await.unwrap();
        service.run_aggregation_round().await.unwrap();

        // Persist the BaseLoRA state
        service.persist_state().await.unwrap();
    }

    // Second "session": restore BaseLoRA from persistence
    {
        let service = HiveLearningService::new(hive_config, repo.clone());
        service.restore_state().await.unwrap();

        let state = service.base_lora_state().await.unwrap();
        assert!(
            state.is_initialized,
            "BaseLoRA should be restored from persistence"
        );
        assert_eq!(state.weights.len(), 4, "Weights should be restored");
        assert!(
            (state.weights[0] - 0.42).abs() < 0.1,
            "Restored weights should approximate original, got: {}",
            state.weights[0]
        );
    }
}
