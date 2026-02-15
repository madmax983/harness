//! EWC++ (Elastic Weight Consolidation++) catastrophic forgetting prevention.
//!
//! Prevents agents from catastrophically forgetting previous task knowledge
//! when learning new tasks by applying quadratic penalty terms weighted by
//! Fisher Information Matrix diagonals.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors from EWC operations.
#[derive(Debug, Error)]
pub enum EwcError {
    #[error("dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },

    #[error("invalid config: {0}")]
    InvalidConfig(String),

    #[error("no gradient samples provided")]
    EmptyGradients,

    #[error("gradient sample dimension mismatch at sample {index}: expected {expected}, got {got}")]
    GradientDimensionMismatch {
        index: usize,
        expected: usize,
        got: usize,
    },
}

/// Type alias for EWC results.
pub type EwcResult<T> = Result<T, EwcError>;

/// Type alias for Fisher diagonal (just a Vec<f32>).
pub type FisherDiagonal = Vec<f32>;

// ---------------------------------------------------------------------------
// EwcConfig
// ---------------------------------------------------------------------------

/// Configuration for EWC++ consolidation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EwcConfig {
    /// Penalty strength (lambda). Higher = stronger protection of old knowledge.
    pub lambda: f32,
    /// Online EWC decay factor for old Fisher info. In (0, 1].
    /// Running Fisher = gamma * F_old + F_new.
    pub gamma: f32,
    /// Maximum number of task snapshots to retain.
    pub max_tasks: usize,
    /// Whether to normalize Fisher diagonals to [0, 1] before storing.
    pub normalize_fisher: bool,
}

impl EwcConfig {
    /// Validate configuration parameters.
    pub fn validate(&self) -> EwcResult<()> {
        if self.lambda < 0.0 {
            return Err(EwcError::InvalidConfig(
                "lambda must be non-negative".into(),
            ));
        }
        if self.gamma <= 0.0 || self.gamma > 1.0 {
            return Err(EwcError::InvalidConfig("gamma must be in (0, 1]".into()));
        }
        if self.max_tasks == 0 {
            return Err(EwcError::InvalidConfig("max_tasks must be > 0".into()));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// FisherInformationMatrix
// ---------------------------------------------------------------------------

/// Fisher Information Matrix stored as a diagonal approximation.
///
/// Each entry F[i] = E[g_i^2] approximates how important weight dimension i
/// is for the current task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FisherInformationMatrix {
    diag: Vec<f32>,
}

impl FisherInformationMatrix {
    /// Compute FIM diagonal from gradient samples.
    ///
    /// F[i] = (1/N) * sum_n (g_n[i])^2
    ///
    /// where g_n is the gradient from sample n.
    pub fn from_gradients(gradients: &[Vec<f32>], dim: usize) -> Self {
        let n = gradients.len();
        if n == 0 {
            return Self {
                diag: vec![0.0; dim],
            };
        }

        let mut diag = vec![0.0f32; dim];
        for grad in gradients {
            for (d, &g) in grad.iter().enumerate().take(dim) {
                diag[d] += g * g;
            }
        }

        // Average over samples
        let n_f32 = n as f32;
        for val in &mut diag {
            *val /= n_f32;
        }

        Self { diag }
    }

    /// Return the diagonal entries.
    pub fn diagonal(&self) -> &[f32] {
        &self.diag
    }

    /// Return a normalized copy with entries scaled to [0, 1].
    pub fn normalized(&self) -> Self {
        let max_val = self.diag.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        if max_val <= 0.0 {
            return self.clone();
        }

        let diag = self.diag.iter().map(|&v| v / max_val).collect();
        Self { diag }
    }

    /// Dimensionality.
    pub fn dim(&self) -> usize {
        self.diag.len()
    }
}

// ---------------------------------------------------------------------------
// TaskSnapshot
// ---------------------------------------------------------------------------

/// A snapshot of an agent's state after completing a task.
/// Contains the optimal weights and Fisher diagonal for that task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    task_id: String,
    optimal_weights: Vec<f32>,
    fisher_diag: Vec<f32>,
}

impl TaskSnapshot {
    /// The task identifier.
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Optimal weight values from when this task was completed.
    pub fn optimal_weights(&self) -> &[f32] {
        &self.optimal_weights
    }

    /// Fisher diagonal for this task.
    pub fn fisher_diagonal(&self) -> &[f32] {
        &self.fisher_diag
    }
}

// ---------------------------------------------------------------------------
// WeightImportance
// ---------------------------------------------------------------------------

/// Importance ranking for weight dimensions within a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightImportance {
    /// (dimension_index, importance_score) sorted descending by score.
    entries: Vec<(usize, f32)>,
}

impl WeightImportance {
    /// Build from a Fisher diagonal.
    pub fn from_fisher(fisher_diag: &[f32]) -> Self {
        let mut entries: Vec<(usize, f32)> = fisher_diag
            .iter()
            .enumerate()
            .map(|(i, &v)| (i, v))
            .collect();
        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Self { entries }
    }

    /// Return the top-k most important dimensions, sorted descending.
    pub fn top_k(&self, k: usize) -> Vec<(usize, f32)> {
        self.entries.iter().take(k).cloned().collect()
    }
}

// ---------------------------------------------------------------------------
// EwcConsolidator
// ---------------------------------------------------------------------------

/// Main EWC++ consolidator. Manages task snapshots, running Fisher,
/// and penalty computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EwcConsolidator {
    config: EwcConfig,
    /// Task snapshots in insertion order (oldest first).
    snapshots: VecDeque<TaskSnapshot>,
    /// Running (accumulated) Fisher diagonal: gamma * F_old + F_new.
    running_fisher: Vec<f32>,
    /// Weight dimensionality (set on first consolidation).
    dim: Option<usize>,
}

impl EwcConsolidator {
    /// Create a new consolidator with the given config.
    pub fn new(config: EwcConfig) -> Self {
        Self {
            config,
            snapshots: VecDeque::new(),
            running_fisher: Vec::new(),
            dim: None,
        }
    }

    /// Number of retained task snapshots.
    pub fn task_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Get a task snapshot by ID.
    pub fn get_task_snapshot(&self, task_id: &str) -> Option<&TaskSnapshot> {
        self.snapshots.iter().find(|s| s.task_id == task_id)
    }

    /// Return the running Fisher diagonal (accumulated across tasks with gamma decay).
    pub fn running_fisher_diagonal(&self) -> &[f32] {
        &self.running_fisher
    }

    /// Consolidate a completed task: compute FIM, store snapshot, update running Fisher.
    pub fn consolidate_task(
        &mut self,
        task_id: &str,
        weights: &[f32],
        gradients: &[Vec<f32>],
    ) -> EwcResult<()> {
        let dim = weights.len();

        if gradients.is_empty() {
            return Err(EwcError::EmptyGradients);
        }

        // Validate gradient dimensions
        for (i, grad) in gradients.iter().enumerate() {
            if grad.len() != dim {
                return Err(EwcError::GradientDimensionMismatch {
                    index: i,
                    expected: dim,
                    got: grad.len(),
                });
            }
        }

        // Check dimension consistency with previous consolidations
        if let Some(existing_dim) = self.dim {
            if dim != existing_dim {
                return Err(EwcError::DimensionMismatch {
                    expected: existing_dim,
                    got: dim,
                });
            }
        } else {
            self.dim = Some(dim);
            self.running_fisher = vec![0.0; dim];
        }

        // Compute FIM from gradients
        let fim = FisherInformationMatrix::from_gradients(gradients, dim);

        // Optionally normalize
        let fisher_diag = if self.config.normalize_fisher {
            fim.normalized().diag
        } else {
            fim.diag.clone()
        };

        // Update running Fisher: F_running = gamma * F_running_old + F_new
        for (i, rf) in self.running_fisher.iter_mut().enumerate() {
            *rf = self.config.gamma * *rf + fisher_diag[i];
        }

        // Store snapshot
        let snapshot = TaskSnapshot {
            task_id: task_id.to_string(),
            optimal_weights: weights.to_vec(),
            fisher_diag,
        };
        self.snapshots.push_back(snapshot);

        // Enforce max_tasks by evicting oldest
        while self.snapshots.len() > self.config.max_tasks {
            self.snapshots.pop_front();
        }

        Ok(())
    }

    /// Compute the EWC penalty for the given weights against all consolidated tasks.
    ///
    /// L = (lambda / 2) * sum_tasks sum_i F_i * (theta_i - theta*_i)^2
    ///
    /// Uses the running Fisher for the accumulated penalty.
    pub fn compute_penalty(&self, weights: &[f32]) -> f32 {
        if self.snapshots.is_empty() {
            return 0.0;
        }

        // Handle dimension mismatch gracefully (return 0 rather than panic)
        if let Some(dim) = self.dim
            && weights.len() != dim
        {
            return 0.0;
        }

        let half_lambda = self.config.lambda / 2.0;
        let mut total = 0.0f32;

        for snapshot in &self.snapshots {
            for (i, &weight) in weights.iter().enumerate() {
                let delta = weight - snapshot.optimal_weights[i];
                total += snapshot.fisher_diag[i] * delta * delta;
            }
        }

        half_lambda * total
    }

    /// Compute the penalty gradient: dL/dtheta_i = lambda * sum_tasks F_i * (theta_i - theta*_i)
    pub fn compute_penalty_gradient(&self, weights: &[f32]) -> Vec<f32> {
        let dim = weights.len();
        let mut gradient = vec![0.0f32; dim];

        if self.snapshots.is_empty() {
            return gradient;
        }

        for snapshot in &self.snapshots {
            for i in 0..dim {
                let delta = weights[i] - snapshot.optimal_weights[i];
                gradient[i] += snapshot.fisher_diag[i] * delta;
            }
        }

        // Scale by lambda
        for g in &mut gradient {
            *g *= self.config.lambda;
        }

        gradient
    }

    /// Apply the penalty gradient to move weights back toward consolidated optima.
    ///
    /// corrected_i = theta_i - lr * dL/dtheta_i
    pub fn apply_penalty_gradient(
        &self,
        weights: &[f32],
        learning_rate: f32,
    ) -> EwcResult<Vec<f32>> {
        let grad = self.compute_penalty_gradient(weights);
        let corrected: Vec<f32> = weights
            .iter()
            .zip(grad.iter())
            .map(|(&w, &g)| w - learning_rate * g)
            .collect();
        Ok(corrected)
    }

    /// Compute per-task importance (penalty contribution) for the given weights.
    pub fn per_task_importance(&self, weights: &[f32]) -> Vec<(String, f32)> {
        let half_lambda = self.config.lambda / 2.0;

        self.snapshots
            .iter()
            .map(|snapshot| {
                let penalty: f32 = snapshot
                    .optimal_weights
                    .iter()
                    .zip(weights.iter())
                    .zip(snapshot.fisher_diag.iter())
                    .map(|((&opt, &cur), &f)| {
                        let delta = cur - opt;
                        f * delta * delta
                    })
                    .sum();
                (snapshot.task_id.clone(), half_lambda * penalty)
            })
            .collect()
    }

    /// Get weight importance ranking for a specific task.
    pub fn weight_importance(&self, task_id: &str) -> Option<WeightImportance> {
        self.get_task_snapshot(task_id)
            .map(|snapshot| WeightImportance::from_fisher(&snapshot.fisher_diag))
    }
}
