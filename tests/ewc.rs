//! EWC++ (Elastic Weight Consolidation++) catastrophic forgetting prevention tests.
//!
//! Tests the EWC++ component of the SONA (Self-Organizing Neural Architecture) stack
//! that prevents agents from catastrophically forgetting previous task knowledge
//! when learning new tasks.
//!
//! EWC++ works by:
//! 1. Computing Fisher Information Matrix (FIM) after each task to identify important weights
//! 2. Applying quadratic penalty terms during new task learning to preserve critical weights
//! 3. Consolidating task knowledge on task switches via online EWC updates
//! 4. Supporting multi-task retention across arbitrary task sequences
//!
//! These tests are RED phase TDD -- they define the API before implementation exists.

use harness_persistence::{AgentId, Session};
use harness_sona::SonaEngine;
use harness_sona::ewc::{EwcConfig, EwcConsolidator, FisherInformationMatrix};

/// Helper: create a mock weight vector simulating an agent's learned parameters.
fn mock_weights(dim: usize, seed: f32) -> Vec<f32> {
    (0..dim).map(|i| seed + (i as f32) * 0.01).collect()
}

/// Helper: create a second distinct weight vector (simulating learning task B).
fn mock_weights_shifted(dim: usize, seed: f32, shift: f32) -> Vec<f32> {
    (0..dim).map(|i| seed + (i as f32) * 0.01 + shift).collect()
}

/// Helper: create mock gradient samples for FIM computation.
/// Each inner vec is one gradient sample (e.g., from a mini-batch).
fn mock_gradient_samples(dim: usize, n_samples: usize) -> Vec<Vec<f32>> {
    (0..n_samples)
        .map(|s| {
            (0..dim)
                .map(|i| ((s * dim + i) as f32 * 0.001).sin())
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Test 1: EWC prevents catastrophic forgetting
// ---------------------------------------------------------------------------

/// After learning task A, the agent consolidates. When learning task B,
/// the EWC penalty should keep weights close to their task-A-optimal values
/// for dimensions that were important for task A. The "forgetting score"
/// (distance from consolidated weights in important dimensions) must stay below
/// a threshold.
#[tokio::test]
async fn test_ewc_prevents_forgetting() {
    let dim = 64;
    let config = EwcConfig {
        lambda: 0.5,    // EWC penalty strength (matches production default)
        gamma: 0.95,    // Online EWC decay factor for old Fisher info
        max_tasks: 100, // Maximum number of tasks to remember
        normalize_fisher: true,
    };
    let mut consolidator = EwcConsolidator::new(config);

    // Phase 1: Agent learns task A
    let weights_after_task_a = mock_weights(dim, 1.0);
    let gradients_a = mock_gradient_samples(dim, 50);

    // Consolidate task A -- computes FIM and stores snapshot
    consolidator
        .consolidate_task("task_a", &weights_after_task_a, &gradients_a)
        .expect("consolidate task A");

    // Phase 2: Agent starts learning task B -- weights shift
    let weights_during_task_b = mock_weights_shifted(dim, 1.0, 0.5);

    // Compute the EWC penalty for the current weights
    let penalty = consolidator.compute_penalty(&weights_during_task_b);

    // The penalty should be positive (weights have drifted from task A optimum)
    assert!(
        penalty > 0.0,
        "EWC penalty should be positive when weights drift from task A optimum, got {}",
        penalty
    );

    // Phase 3: Apply EWC-regularized update -- the penalty gradient pulls weights back
    let corrected_weights = consolidator
        .apply_penalty_gradient(&weights_during_task_b, 0.01) // learning_rate
        .expect("apply penalty gradient");

    // After correction, weights should be closer to task A optimum in important dimensions
    let distance_before = weight_distance(&weights_during_task_b, &weights_after_task_a);
    let distance_after = weight_distance(&corrected_weights, &weights_after_task_a);

    assert!(
        distance_after < distance_before,
        "EWC correction should reduce distance to task A weights: before={}, after={}",
        distance_before,
        distance_after
    );
}

// ---------------------------------------------------------------------------
// Test 2: Fisher Information Matrix computation
// ---------------------------------------------------------------------------

/// The FIM diagonal should be computed from gradient samples.
/// Each diagonal entry is the expected squared gradient, approximating
/// how important each weight dimension is for the current task.
#[tokio::test]
async fn test_fisher_information_computation() {
    let dim = 32;

    // Create gradient samples with known structure:
    // dimension 0 has large gradients (important), dimension 31 has small gradients (unimportant)
    let mut gradients: Vec<Vec<f32>> = Vec::new();
    for sample_idx in 0..100 {
        let mut grad = vec![0.0f32; dim];
        for (d, item) in grad.iter_mut().enumerate().take(dim) {
            // Gradient magnitude decreases with dimension index
            let magnitude = 1.0 / (1.0 + d as f32);
            *item = magnitude * ((sample_idx as f32 * 0.1).sin());
        }
        gradients.push(grad);
    }

    let fim = FisherInformationMatrix::from_gradients(&gradients, dim);

    // Verify FIM is the correct size
    let diagonal = fim.diagonal();
    assert_eq!(
        diagonal.len(),
        dim,
        "FIM diagonal should have one entry per weight dimension"
    );

    // All entries should be non-negative (squared gradients)
    for (i, &val) in diagonal.iter().enumerate() {
        assert!(
            val >= 0.0,
            "FIM diagonal entry {} should be non-negative, got {}",
            i,
            val
        );
    }

    // Dimension 0 should have higher Fisher information than dimension 31
    // because we designed gradients to be larger for lower dimensions
    assert!(
        diagonal[0] > diagonal[dim - 1],
        "Dimension 0 (important) should have higher FIM than dimension {} (unimportant): {} vs {}",
        dim - 1,
        diagonal[0],
        diagonal[dim - 1]
    );

    // The FIM should be approximately proportional to 1/(1+d)^2 (squared gradient magnitude)
    // Verify monotonic decrease
    for d in 0..dim - 1 {
        assert!(
            diagonal[d] >= diagonal[d + 1],
            "FIM should decrease monotonically: dim {} ({}) >= dim {} ({})",
            d,
            diagonal[d],
            d + 1,
            diagonal[d + 1]
        );
    }
}

/// FIM normalization should scale entries to [0, 1] range.
#[tokio::test]
async fn test_fisher_information_normalization() {
    let dim = 16;
    let gradients = mock_gradient_samples(dim, 50);
    let fim = FisherInformationMatrix::from_gradients(&gradients, dim);

    let normalized = fim.normalized();
    let diag = normalized.diagonal();

    // After normalization, max should be 1.0 (or very close)
    let max_val = diag.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (max_val - 1.0).abs() < 1e-5,
        "Normalized FIM max should be ~1.0, got {}",
        max_val
    );

    // All entries should be in [0, 1]
    for (i, &val) in diag.iter().enumerate() {
        assert!(
            (0.0..=1.0).contains(&val),
            "Normalized FIM entry {} should be in [0,1], got {}",
            i,
            val
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3: Task consolidation on task switch
// ---------------------------------------------------------------------------

/// When an agent finishes task A and switches to task B, the consolidator
/// should store a TaskSnapshot containing the optimal weights and FIM for task A.
/// The snapshot is used to compute penalties during task B learning.
#[tokio::test]
async fn test_task_consolidation() {
    let dim = 32;
    let config = EwcConfig {
        lambda: 500.0,
        gamma: 0.99,
        max_tasks: 50,
        normalize_fisher: false,
    };
    let mut consolidator = EwcConsolidator::new(config);

    // Initially, no tasks consolidated
    assert_eq!(
        consolidator.task_count(),
        0,
        "Fresh consolidator should have 0 tasks"
    );

    // Consolidate task A
    let weights_a = mock_weights(dim, 1.0);
    let grads_a = mock_gradient_samples(dim, 30);
    consolidator
        .consolidate_task("task_a", &weights_a, &grads_a)
        .expect("consolidate A");

    assert_eq!(consolidator.task_count(), 1);

    // Verify the snapshot was stored correctly
    let snapshot_a = consolidator
        .get_task_snapshot("task_a")
        .expect("snapshot A should exist");
    assert_eq!(snapshot_a.task_id(), "task_a");
    assert_eq!(snapshot_a.optimal_weights().len(), dim);
    assert_eq!(snapshot_a.fisher_diagonal().len(), dim);

    // Consolidate task B (online EWC: merges FIM from A with FIM from B using gamma)
    let weights_b = mock_weights(dim, 2.0);
    let grads_b = mock_gradient_samples(dim, 30);
    consolidator
        .consolidate_task("task_b", &weights_b, &grads_b)
        .expect("consolidate B");

    assert_eq!(consolidator.task_count(), 2);

    // Both snapshots should exist
    assert!(consolidator.get_task_snapshot("task_a").is_some());
    assert!(consolidator.get_task_snapshot("task_b").is_some());

    // Online EWC: the running Fisher should be a gamma-weighted combination
    let running_fisher = consolidator.running_fisher_diagonal();
    assert_eq!(
        running_fisher.len(),
        dim,
        "Running Fisher should have correct dimensionality"
    );

    // Running Fisher should be non-trivial (not all zeros)
    let sum: f32 = running_fisher.iter().sum();
    assert!(
        sum > 0.0,
        "Running Fisher should have positive entries after consolidation"
    );
}

/// The consolidator should enforce max_tasks limit by dropping the oldest task.
#[tokio::test]
async fn test_task_consolidation_max_limit() {
    let dim = 16;
    let config = EwcConfig {
        lambda: 100.0,
        gamma: 0.9,
        max_tasks: 3,
        normalize_fisher: false,
    };
    let mut consolidator = EwcConsolidator::new(config);

    // Consolidate 4 tasks (exceeds max_tasks of 3)
    for i in 0..4 {
        let weights = mock_weights(dim, i as f32);
        let grads = mock_gradient_samples(dim, 10);
        consolidator
            .consolidate_task(&format!("task_{}", i), &weights, &grads)
            .expect("consolidate");
    }

    // Should only keep the most recent 3
    assert_eq!(consolidator.task_count(), 3, "Should cap at max_tasks=3");

    // Oldest task (task_0) should have been evicted
    assert!(
        consolidator.get_task_snapshot("task_0").is_none(),
        "Oldest task should be evicted"
    );
    assert!(
        consolidator.get_task_snapshot("task_1").is_some(),
        "task_1 should still exist"
    );
    assert!(
        consolidator.get_task_snapshot("task_3").is_some(),
        "Newest task should exist"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Multi-task retention
// ---------------------------------------------------------------------------

/// After consolidating N tasks sequentially, the EWC penalty should protect
/// knowledge from all retained tasks. An agent learning task N+1 should
/// incur penalties for drifting from any of the previous N task optima,
/// weighted by their respective Fisher information.
#[tokio::test]
async fn test_multi_task_retention() {
    let dim = 32;
    let n_tasks = 5;
    let config = EwcConfig {
        lambda: 500.0,
        gamma: 0.95,
        max_tasks: 10,
        normalize_fisher: true,
    };
    let mut consolidator = EwcConsolidator::new(config);

    // Consolidate N tasks with distinct weight optima
    let mut task_weights: Vec<Vec<f32>> = Vec::new();
    for i in 0..n_tasks {
        let weights = mock_weights(dim, (i + 1) as f32);
        let grads = mock_gradient_samples(dim, 30);
        consolidator
            .consolidate_task(&format!("task_{}", i), &weights, &grads)
            .expect("consolidate");
        task_weights.push(weights);
    }

    assert_eq!(consolidator.task_count(), n_tasks);

    // Test that penalty increases as weights deviate more from consolidated optima
    let base_weights = mock_weights(dim, 1.0); // close to task_0
    let far_weights = mock_weights(dim, 100.0); // far from all tasks

    let penalty_near = consolidator.compute_penalty(&base_weights);
    let penalty_far = consolidator.compute_penalty(&far_weights);

    assert!(
        penalty_far > penalty_near,
        "Penalty should be larger for weights far from all task optima: near={}, far={}",
        penalty_near,
        penalty_far
    );

    // Compute per-task importance scores
    let importance = consolidator.per_task_importance(&base_weights);
    assert_eq!(
        importance.len(),
        n_tasks,
        "Should have importance score for each retained task"
    );

    // Each task should have non-negative importance
    for (task_id, score) in &importance {
        assert!(
            *score >= 0.0,
            "Task {} importance should be non-negative, got {}",
            task_id,
            score
        );
    }
}

/// Verify that the online EWC (gamma decay) properly down-weights older tasks.
#[tokio::test]
async fn test_multi_task_temporal_decay() {
    let dim = 16;
    let gamma = 0.5; // Aggressive decay for testing
    let config = EwcConfig {
        lambda: 100.0,
        gamma,
        max_tasks: 100,
        normalize_fisher: false,
    };
    let mut consolidator = EwcConsolidator::new(config);

    // Consolidate two tasks with identical gradients
    let weights_a = mock_weights(dim, 1.0);
    let weights_b = mock_weights(dim, 2.0);
    let grads = mock_gradient_samples(dim, 30);

    consolidator
        .consolidate_task("old_task", &weights_a, &grads)
        .expect("consolidate old");
    consolidator
        .consolidate_task("new_task", &weights_b, &grads)
        .expect("consolidate new");

    // The running Fisher should have contributions from both tasks,
    // but old_task's contribution should be scaled by gamma
    let running = consolidator.running_fisher_diagonal();

    // Get individual task Fishers for comparison
    let old_snapshot = consolidator
        .get_task_snapshot("old_task")
        .expect("old snapshot");
    let new_snapshot = consolidator
        .get_task_snapshot("new_task")
        .expect("new snapshot");

    // Running Fisher ~ gamma * F_old + F_new
    // So running[i] should be approximately gamma * old_fisher[i] + new_fisher[i]
    for (d, item) in running.iter().enumerate().take(dim) {
        let expected =
            gamma * old_snapshot.fisher_diagonal()[d] + new_snapshot.fisher_diagonal()[d];
        let actual = *item;
        let tolerance = expected.abs() * 0.1 + 1e-6;
        assert!(
            (actual - expected).abs() < tolerance,
            "Running Fisher dim {} should be ~gamma*F_old + F_new: expected {}, got {}",
            d,
            expected,
            actual
        );
    }
}

// ---------------------------------------------------------------------------
// Test 5: Forgetting penalty terms
// ---------------------------------------------------------------------------

/// The EWC penalty for weight vector theta is:
///   L_ewc = (lambda / 2) * sum_i F_i * (theta_i - theta*_i)^2
/// where F_i is the Fisher diagonal and theta*_i is the optimal weight from the
/// consolidated task. Verify the formula is computed correctly.
#[tokio::test]
async fn test_forgetting_penalty_formula() {
    let config = EwcConfig {
        lambda: 2.0, // lambda=2 so lambda/2 = 1.0 for easy math
        gamma: 1.0,
        max_tasks: 10,
        normalize_fisher: false,
    };
    let mut consolidator = EwcConsolidator::new(config);

    // Manually set up a task with known Fisher and weights
    // Fisher diagonal: [1.0, 2.0, 0.5, 0.0]
    // Optimal weights: [1.0, 1.0, 1.0, 1.0]
    let optimal = vec![1.0, 1.0, 1.0, 1.0];

    // Create gradients that produce known Fisher diagonal
    // F_i = E[g_i^2], so single-sample gradients: [1.0, sqrt(2), sqrt(0.5), 0.0]
    let grads = vec![vec![1.0, 2.0_f32.sqrt(), 0.5_f32.sqrt(), 0.0]];

    consolidator
        .consolidate_task("known_task", &optimal, &grads)
        .expect("consolidate known task");

    // Current weights deviate by 1.0 in each dimension: [2.0, 2.0, 2.0, 2.0]
    let current = vec![2.0, 2.0, 2.0, 2.0];

    let penalty = consolidator.compute_penalty(&current);

    // Expected: (lambda/2) * sum_i F_i * (theta_i - theta*_i)^2
    // = 1.0 * (1.0 * 1.0 + 2.0 * 1.0 + 0.5 * 1.0 + 0.0 * 1.0)
    // = 1.0 * (1.0 + 2.0 + 0.5 + 0.0) = 3.5
    let expected = 3.5;
    assert!(
        (penalty - expected).abs() < 1e-4,
        "EWC penalty should be {}, got {}",
        expected,
        penalty
    );
}

/// The penalty gradient with respect to theta should be:
///   dL/dtheta_i = lambda * F_i * (theta_i - theta*_i)
#[tokio::test]
async fn test_forgetting_penalty_gradient() {
    let dim = 4;
    let config = EwcConfig {
        lambda: 2.0,
        gamma: 1.0,
        max_tasks: 10,
        normalize_fisher: false,
    };
    let mut consolidator = EwcConsolidator::new(config);

    let optimal = vec![1.0, 1.0, 1.0, 1.0];
    let grads = vec![vec![1.0, 2.0_f32.sqrt(), 0.5_f32.sqrt(), 0.0]];

    consolidator
        .consolidate_task("known_task", &optimal, &grads)
        .expect("consolidate");

    let current = vec![2.0, 2.0, 2.0, 2.0];

    let penalty_gradient = consolidator.compute_penalty_gradient(&current);

    // dL/dtheta_i = lambda * F_i * (theta_i - theta*_i)
    // lambda = 2.0, deviation = 1.0 for all dims
    // Expected: [2.0*1.0*1.0, 2.0*2.0*1.0, 2.0*0.5*1.0, 2.0*0.0*1.0]
    //         = [2.0, 4.0, 1.0, 0.0]
    let expected = [2.0, 4.0, 1.0, 0.0];
    for i in 0..dim {
        assert!(
            (penalty_gradient[i] - expected[i]).abs() < 1e-4,
            "Penalty gradient dim {} should be {}, got {}",
            i,
            expected[i],
            penalty_gradient[i]
        );
    }
}

/// When no tasks have been consolidated, the penalty should be zero.
#[tokio::test]
async fn test_forgetting_penalty_zero_when_empty() {
    let config = EwcConfig {
        lambda: 1000.0,
        gamma: 0.95,
        max_tasks: 10,
        normalize_fisher: true,
    };
    let consolidator = EwcConsolidator::new(config);

    let weights = vec![1.0, 2.0, 3.0, 4.0];
    let penalty = consolidator.compute_penalty(&weights);

    assert!(
        penalty.abs() < 1e-10,
        "Penalty should be zero with no consolidated tasks, got {}",
        penalty
    );
}

// ---------------------------------------------------------------------------
// Test 6: Weight importance identification
// ---------------------------------------------------------------------------

/// The EWC system should identify which weight dimensions are most important
/// for each consolidated task, enabling targeted protection.
#[tokio::test]
async fn test_weight_importance_ranking() {
    let dim = 16;
    let config = EwcConfig {
        lambda: 100.0,
        gamma: 0.95,
        max_tasks: 10,
        normalize_fisher: true,
    };
    let mut consolidator = EwcConsolidator::new(config);

    // Create gradients where dim 0 is most important, dim 15 least
    let mut gradients = Vec::new();
    for s in 0..50 {
        let mut grad = vec![0.0f32; dim];
        for (d, item) in grad.iter_mut().enumerate().take(dim) {
            *item = (10.0 / (1.0 + d as f32)) * ((s as f32 * 0.1).sin());
        }
        gradients.push(grad);
    }

    let weights = mock_weights(dim, 1.0);
    consolidator
        .consolidate_task("ranked_task", &weights, &gradients)
        .expect("consolidate");

    let importance = consolidator
        .weight_importance("ranked_task")
        .expect("get importance");

    // Top-k most important dimensions should be the lowest indices
    let top_k = importance.top_k(4);
    assert_eq!(top_k.len(), 4);

    // All top-4 should be from the first 4 dimensions (most important by design)
    for &(dim_idx, _score) in &top_k {
        assert!(
            dim_idx < 4,
            "Top-4 important dimensions should be 0-3, got dim {}",
            dim_idx
        );
    }

    // Importance scores should be sorted descending
    for i in 0..top_k.len() - 1 {
        assert!(
            top_k[i].1 >= top_k[i + 1].1,
            "Importance scores should be sorted descending"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 7: Integration with harness agent lifecycle
// ---------------------------------------------------------------------------

/// EWC consolidation should integrate with harness agent task completion events.
/// When an agent completes a task, EWC should automatically consolidate the
/// agent's current adaptation weights.
#[tokio::test]
async fn test_ewc_agent_task_lifecycle() {
    let _session = Session::new(8);
    let agent_id = AgentId::new();

    let ewc_config = EwcConfig {
        lambda: 500.0,
        gamma: 0.95,
        max_tasks: 50,
        normalize_fisher: true,
    };

    // Create a SONA engine with EWC enabled
    let engine = SonaEngine::builder()
        .with_ewc(ewc_config)
        .build()
        .expect("build SONA engine");

    // Simulate agent completing task A
    let task_a_weights = mock_weights(64, 1.0);
    let task_a_gradients = mock_gradient_samples(64, 30);

    engine
        .on_task_complete(agent_id, "task_a", &task_a_weights, &task_a_gradients)
        .await
        .expect("handle task A completion");

    // Verify EWC state for this agent
    let ewc_state = engine
        .agent_ewc_state(agent_id)
        .await
        .expect("get agent EWC state");

    assert_eq!(ewc_state.task_count(), 1);
    assert!(ewc_state.get_task_snapshot("task_a").is_some());

    // Simulate agent completing task B
    let task_b_weights = mock_weights(64, 2.0);
    let task_b_gradients = mock_gradient_samples(64, 30);

    engine
        .on_task_complete(agent_id, "task_b", &task_b_weights, &task_b_gradients)
        .await
        .expect("handle task B completion");

    let ewc_state = engine
        .agent_ewc_state(agent_id)
        .await
        .expect("get updated EWC state");

    assert_eq!(ewc_state.task_count(), 2);

    // The engine should compute penalty for current weights considering both tasks
    let current_weights = mock_weights(64, 3.0);
    let penalty = ewc_state.compute_penalty(&current_weights);
    assert!(
        penalty > 0.0,
        "Should have positive penalty when deviating from consolidated tasks"
    );
}

// ---------------------------------------------------------------------------
// Test 8: EWC serialization for persistence
// ---------------------------------------------------------------------------

/// EWC state should be serializable for storage in AletheiaDB.
/// This enables resuming EWC protection across daemon restarts.
#[tokio::test]
async fn test_ewc_state_serialization() {
    let dim = 16;
    let config = EwcConfig {
        lambda: 500.0,
        gamma: 0.95,
        max_tasks: 10,
        normalize_fisher: true,
    };
    let mut consolidator = EwcConsolidator::new(config);

    // Consolidate a couple of tasks
    let weights_a = mock_weights(dim, 1.0);
    let grads_a = mock_gradient_samples(dim, 20);
    consolidator
        .consolidate_task("task_a", &weights_a, &grads_a)
        .unwrap();

    let weights_b = mock_weights(dim, 2.0);
    let grads_b = mock_gradient_samples(dim, 20);
    consolidator
        .consolidate_task("task_b", &weights_b, &grads_b)
        .unwrap();

    // Serialize to JSON (for AletheiaDB storage)
    let serialized =
        serde_json::to_string(&consolidator).expect("EWC consolidator should be serializable");

    // Deserialize back
    let restored: EwcConsolidator =
        serde_json::from_str(&serialized).expect("EWC consolidator should be deserializable");

    // Verify restored state matches
    assert_eq!(restored.task_count(), 2);
    assert!(restored.get_task_snapshot("task_a").is_some());
    assert!(restored.get_task_snapshot("task_b").is_some());

    // Verify penalty computation gives same results
    let test_weights = mock_weights(dim, 5.0);
    let original_penalty = consolidator.compute_penalty(&test_weights);
    let restored_penalty = restored.compute_penalty(&test_weights);

    assert!(
        (original_penalty - restored_penalty).abs() < 1e-6,
        "Penalty should be identical after serialization roundtrip: {} vs {}",
        original_penalty,
        restored_penalty
    );
}

// ---------------------------------------------------------------------------
// Test 9: EWC config validation
// ---------------------------------------------------------------------------

/// EWC config should validate parameters.
#[test]
fn test_ewc_config_validation() {
    // Valid config
    let valid = EwcConfig {
        lambda: 100.0,
        gamma: 0.95,
        max_tasks: 10,
        normalize_fisher: true,
    };
    assert!(valid.validate().is_ok());

    // Lambda must be positive
    let bad_lambda = EwcConfig {
        lambda: -1.0,
        ..valid
    };
    assert!(
        bad_lambda.validate().is_err(),
        "Negative lambda should be invalid"
    );

    // Gamma must be in (0, 1]
    let bad_gamma = EwcConfig {
        gamma: 1.5,
        ..valid
    };
    assert!(bad_gamma.validate().is_err(), "Gamma > 1 should be invalid");

    let zero_gamma = EwcConfig {
        gamma: 0.0,
        ..valid
    };
    assert!(
        zero_gamma.validate().is_err(),
        "Gamma = 0 should be invalid"
    );

    // max_tasks must be > 0
    let zero_tasks = EwcConfig {
        max_tasks: 0,
        ..valid
    };
    assert!(
        zero_tasks.validate().is_err(),
        "max_tasks = 0 should be invalid"
    );
}

// ---------------------------------------------------------------------------
// Test 10: Dimension mismatch handling
// ---------------------------------------------------------------------------

/// The consolidator should return an error if weight dimensions don't match
/// across task consolidations or penalty computations.
#[tokio::test]
async fn test_dimension_mismatch_error() {
    let config = EwcConfig {
        lambda: 100.0,
        gamma: 0.95,
        max_tasks: 10,
        normalize_fisher: false,
    };
    let mut consolidator = EwcConsolidator::new(config);

    // Consolidate with dim=32
    let weights_32 = mock_weights(32, 1.0);
    let grads_32 = mock_gradient_samples(32, 10);
    consolidator
        .consolidate_task("task_32", &weights_32, &grads_32)
        .expect("consolidate dim 32");

    // Try to consolidate with dim=16 -- should fail
    let weights_16 = mock_weights(16, 1.0);
    let grads_16 = mock_gradient_samples(16, 10);
    let result = consolidator.consolidate_task("task_16", &weights_16, &grads_16);
    assert!(
        result.is_err(),
        "Consolidating mismatched dimensions should fail"
    );

    // Penalty computation with wrong dimension should also fail gracefully
    let wrong_dim_weights = mock_weights(16, 1.0);
    // compute_penalty should return 0.0 or handle gracefully for mismatched dims
    // (implementation can choose panic-free approach)
    let penalty = consolidator.compute_penalty(&wrong_dim_weights);
    // We just verify it doesn't panic -- the exact behavior is implementation-defined
    let _ = penalty;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute L2 distance between two weight vectors.
fn weight_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}
