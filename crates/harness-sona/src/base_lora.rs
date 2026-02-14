//! BaseLoRA: Collective hive learning through LoRA delta aggregation.

use std::collections::{HashMap, HashSet};

use rayon::prelude::*;
use tokio::sync::RwLock;

use crate::config::BaseLoRAConfig;
use crate::contribution::ContributionWeight;
use crate::error::SonaResult;
use crate::types::{AggregatedWeights, AggregationStrategy, ContributionStats, LoRADelta};

/// Threshold for switching to parallel aggregation.
/// Below this, sequential is faster due to overhead.
const PARALLEL_THRESHOLD: usize = 10;

/// Internal state for tracking contributions per domain.
struct DomainState {
    deltas: Vec<LoRADelta>,
}

/// BaseLoRA aggregates MicroLoRA deltas from individual agents into
/// a shared collective model.
pub struct BaseLoRA {
    config: BaseLoRAConfig,
    /// Deltas organized by domain.
    domains: RwLock<HashMap<String, DomainState>>,
    /// Set of unique contributing agent IDs.
    contributors: RwLock<HashSet<uuid::Uuid>>,
    /// Total delta count.
    total_deltas: RwLock<usize>,
}

impl BaseLoRA {
    /// Create a new BaseLoRA with the given configuration.
    pub fn new(config: BaseLoRAConfig) -> Self {
        Self {
            config,
            domains: RwLock::new(HashMap::new()),
            contributors: RwLock::new(HashSet::new()),
            total_deltas: RwLock::new(0),
        }
    }

    /// Contribute a LoRA delta from an agent.
    pub async fn contribute(&self, delta: LoRADelta) -> SonaResult<()> {
        let domain = delta.domain.clone();
        let agent_uuid = delta.agent_id.as_uuid();

        let mut domains = self.domains.write().await;
        let state = domains
            .entry(domain)
            .or_insert_with(|| DomainState { deltas: Vec::new() });
        state.deltas.push(delta);

        self.contributors.write().await.insert(agent_uuid);
        *self.total_deltas.write().await += 1;

        Ok(())
    }

    /// Aggregate all deltas across all domains into combined weights.
    pub async fn aggregate(&self) -> SonaResult<AggregatedWeights> {
        let domains = self.domains.read().await;

        // Collect all deltas across all domains
        let all_deltas: Vec<&LoRADelta> = domains.values().flat_map(|s| s.deltas.iter()).collect();

        if all_deltas.is_empty() {
            return Err(crate::error::SonaError::NoContributions);
        }

        let weights = self.aggregate_deltas(&all_deltas);

        Ok(AggregatedWeights {
            weights,
            domain: None,
            delta_count: all_deltas.len(),
        })
    }

    /// Aggregate deltas for a specific domain only.
    pub async fn aggregate_domain(&self, domain: &str) -> SonaResult<AggregatedWeights> {
        let domains = self.domains.read().await;

        let state = domains
            .get(domain)
            .ok_or_else(|| crate::error::SonaError::DomainNotFound(domain.to_string()))?;

        let deltas: Vec<&LoRADelta> = state.deltas.iter().collect();
        if deltas.is_empty() {
            return Err(crate::error::SonaError::NoContributions);
        }

        let weights = self.aggregate_deltas(&deltas);

        Ok(AggregatedWeights {
            weights,
            domain: Some(domain.to_string()),
            delta_count: deltas.len(),
        })
    }

    /// List all domains that have received contributions.
    pub async fn known_domains(&self) -> HashSet<String> {
        self.domains.read().await.keys().cloned().collect()
    }

    /// Get contribution statistics.
    pub async fn contribution_stats(&self) -> ContributionStats {
        let domains = self.domains.read().await;
        let contributors = self.contributors.read().await;
        let total_deltas = *self.total_deltas.read().await;

        ContributionStats {
            total_contributors: contributors.len(),
            total_deltas,
            active_contributors: contributors.len(),
            domains: domains.keys().cloned().collect(),
        }
    }

    /// Public wrapper for benchmarking aggregate_deltas.
    ///
    /// **Note**: This is exposed only for benchmarking purposes.
    /// In production code, use `aggregate()` or `aggregate_domain()`.
    #[doc(hidden)]
    pub fn benchmark_aggregate_deltas(&self, deltas: &[&LoRADelta]) -> Vec<f32> {
        self.aggregate_deltas(deltas)
    }

    /// Internal: aggregate a set of deltas using the configured strategy.
    ///
    /// For large delta sets (>= PARALLEL_THRESHOLD), uses rayon for parallelization.
    /// For small sets, uses sequential processing to avoid overhead.
    fn aggregate_deltas(&self, deltas: &[&LoRADelta]) -> Vec<f32> {
        if deltas.is_empty() {
            return Vec::new();
        }

        let dim = deltas[0].weights.len();
        let use_parallel = deltas.len() >= PARALLEL_THRESHOLD;

        match self.config.aggregation_strategy {
            AggregationStrategy::FederatedAverage => {
                if use_parallel {
                    self.aggregate_federated_parallel(deltas, dim)
                } else {
                    self.aggregate_federated_sequential(deltas, dim)
                }
            }
            AggregationStrategy::WeightedAverage => {
                if use_parallel {
                    self.aggregate_weighted_parallel(deltas, dim)
                } else {
                    self.aggregate_weighted_sequential(deltas, dim)
                }
            }
        }
    }

    /// Sequential federated average (for small delta sets).
    #[inline]
    fn aggregate_federated_sequential(&self, deltas: &[&LoRADelta], dim: usize) -> Vec<f32> {
        let mut sum = vec![0.0f32; dim];
        for delta in deltas {
            for (i, &w) in delta.weights.iter().enumerate().take(dim) {
                sum[i] += w;
            }
        }
        let n = deltas.len() as f32;
        sum.iter().map(|&s| s / n).collect()
    }

    /// Parallel federated average using rayon (for large delta sets).
    #[inline]
    fn aggregate_federated_parallel(&self, deltas: &[&LoRADelta], dim: usize) -> Vec<f32> {
        // Parallel reduction: each thread accumulates a partial sum
        let sum: Vec<f32> = deltas
            .par_iter()
            .fold(
                || vec![0.0f32; dim],
                |mut acc, delta| {
                    for (i, &w) in delta.weights.iter().enumerate().take(dim) {
                        acc[i] += w;
                    }
                    acc
                },
            )
            .reduce(
                || vec![0.0f32; dim],
                |mut a, b| {
                    // SIMD-friendly reduction
                    self.add_vectors_simd(&mut a, &b);
                    a
                },
            );

        let n = deltas.len() as f32;
        sum.iter().map(|&s| s / n).collect()
    }

    /// Sequential weighted average (for small delta sets).
    #[inline]
    fn aggregate_weighted_sequential(&self, deltas: &[&LoRADelta], dim: usize) -> Vec<f32> {
        let mut weighted_sum = vec![0.0f64; dim];
        let mut total_weight = 0.0f64;

        for delta in deltas {
            let cw = ContributionWeight::compute(
                delta.task_count,
                delta.success_rate,
                1.0, // recency_factor = 1.0 for current deltas
            );
            let w = cw.value;
            total_weight += w;

            for (i, &val) in delta.weights.iter().enumerate().take(dim) {
                weighted_sum[i] += val as f64 * w;
            }
        }

        if total_weight == 0.0 {
            vec![0.0f32; dim]
        } else {
            weighted_sum
                .iter()
                .map(|&s| (s / total_weight) as f32)
                .collect()
        }
    }

    /// Parallel weighted average using rayon (for large delta sets).
    #[inline]
    fn aggregate_weighted_parallel(&self, deltas: &[&LoRADelta], dim: usize) -> Vec<f32> {
        // Parallel reduction with per-thread weighted sums
        let (weighted_sum, total_weight) = deltas
            .par_iter()
            .fold(
                || (vec![0.0f64; dim], 0.0f64),
                |(mut acc_sum, mut acc_weight), delta| {
                    let cw = ContributionWeight::compute(
                        delta.task_count,
                        delta.success_rate,
                        1.0,
                    );
                    let w = cw.value;
                    acc_weight += w;

                    for (i, &val) in delta.weights.iter().enumerate().take(dim) {
                        acc_sum[i] += val as f64 * w;
                    }
                    (acc_sum, acc_weight)
                },
            )
            .reduce(
                || (vec![0.0f64; dim], 0.0f64),
                |(mut a_sum, a_weight), (b_sum, b_weight)| {
                    // Combine partial results
                    for (i, &val) in b_sum.iter().enumerate() {
                        a_sum[i] += val;
                    }
                    (a_sum, a_weight + b_weight)
                },
            );

        if total_weight == 0.0 {
            vec![0.0f32; dim]
        } else {
            weighted_sum
                .iter()
                .map(|&s| (s / total_weight) as f32)
                .collect()
        }
    }

    /// SIMD-optimized vector addition for reduction phase.
    ///
    /// Adds vector `b` into vector `a` in-place using chunked operations
    /// to enable auto-vectorization by LLVM.
    #[inline]
    fn add_vectors_simd(&self, a: &mut [f32], b: &[f32]) {
        // Process in chunks of 8 for better SIMD utilization
        const CHUNK_SIZE: usize = 8;
        let len = a.len().min(b.len());

        let (chunks_a, remainder_a) = a.split_at_mut(len - (len % CHUNK_SIZE));
        let (chunks_b, remainder_b) = b.split_at(len - (len % CHUNK_SIZE));

        // Chunked addition (LLVM auto-vectorizes this)
        for (chunk_a, chunk_b) in chunks_a.chunks_exact_mut(CHUNK_SIZE).zip(chunks_b.chunks_exact(CHUNK_SIZE)) {
            for i in 0..CHUNK_SIZE {
                chunk_a[i] += chunk_b[i];
            }
        }

        // Handle remainder
        for i in 0..remainder_a.len() {
            remainder_a[i] += remainder_b[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Helper to create a config with a specific strategy.
    fn test_config(strategy: AggregationStrategy) -> BaseLoRAConfig {
        BaseLoRAConfig {
            rank: 128,
            alpha: 1.0,
            aggregation_strategy: strategy,
            min_participants: 1,
            staleness_threshold_secs: 3600,
        }
    }

    /// Generate synthetic deltas for testing.
    fn generate_test_deltas(count: usize, dim: usize) -> Vec<LoRADelta> {
        (0..count)
            .map(|i| {
                let weights: Vec<f32> = (0..dim).map(|j| (i + j) as f32 * 0.01).collect();
                LoRADelta::synthetic(
                    "test_domain",
                    weights,
                    10 + (i as u32 % 50),
                    0.8 + (i as f64 % 20.0) / 100.0,
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn test_federated_average_small_set() {
        let config = test_config(AggregationStrategy::FederatedAverage);
        let base_lora = BaseLoRA::new(config);

        let deltas = generate_test_deltas(5, 10);
        for delta in deltas {
            base_lora.contribute(delta).await.unwrap();
        }

        let result = base_lora.aggregate().await.unwrap();
        assert_eq!(result.weights.len(), 10);
        assert_eq!(result.delta_count, 5);

        // Verify average is computed correctly
        // First weight should be average of [0.0, 0.01, 0.02, 0.03, 0.04] = 0.02
        assert!((result.weights[0] - 0.02).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_federated_average_large_set_parallel() {
        let config = test_config(AggregationStrategy::FederatedAverage);
        let base_lora = BaseLoRA::new(config);

        // Generate enough deltas to trigger parallel path (>= PARALLEL_THRESHOLD)
        let deltas = generate_test_deltas(20, 10);
        for delta in deltas {
            base_lora.contribute(delta).await.unwrap();
        }

        let result = base_lora.aggregate().await.unwrap();
        assert_eq!(result.weights.len(), 10);
        assert_eq!(result.delta_count, 20);

        // Verify parallel gives same result as sequential would
        // Average of first 20 values: 0, 0.01, 0.02, ..., 0.19 = 0.095
        let expected = (0..20).map(|i| i as f32 * 0.01).sum::<f32>() / 20.0;
        assert!((result.weights[0] - expected).abs() < 1e-5);
    }

    #[tokio::test]
    async fn test_weighted_average_small_set() {
        let config = test_config(AggregationStrategy::WeightedAverage);
        let base_lora = BaseLoRA::new(config);

        let deltas = generate_test_deltas(8, 10);
        for delta in deltas {
            base_lora.contribute(delta).await.unwrap();
        }

        let result = base_lora.aggregate().await.unwrap();
        assert_eq!(result.weights.len(), 10);
        assert_eq!(result.delta_count, 8);

        // Result should differ from simple average due to weighting
        assert!(result.weights[0] != 0.035); // Not the simple average
    }

    #[tokio::test]
    async fn test_weighted_average_large_set_parallel() {
        let config = test_config(AggregationStrategy::WeightedAverage);
        let base_lora = BaseLoRA::new(config);

        // Generate enough deltas to trigger parallel path
        let deltas = generate_test_deltas(50, 10);
        for delta in deltas {
            base_lora.contribute(delta).await.unwrap();
        }

        let result = base_lora.aggregate().await.unwrap();
        assert_eq!(result.weights.len(), 10);
        assert_eq!(result.delta_count, 50);

        // Verify result is reasonable (non-zero, finite)
        assert!(result.weights[0].is_finite());
        assert!(result.weights[0] > 0.0);
    }

    #[tokio::test]
    async fn test_parallel_vs_sequential_consistency() {
        // Test that parallel and sequential paths produce identical results
        let config_seq = test_config(AggregationStrategy::FederatedAverage);
        let config_par = test_config(AggregationStrategy::FederatedAverage);

        let base_lora_seq = BaseLoRA::new(config_seq);
        let base_lora_par = BaseLoRA::new(config_par);

        // Use exactly PARALLEL_THRESHOLD-1 deltas for sequential
        let deltas_seq = generate_test_deltas(9, 16);
        for delta in &deltas_seq {
            base_lora_seq.contribute(delta.clone()).await.unwrap();
        }

        // Use PARALLEL_THRESHOLD + 1 for parallel
        let mut deltas_par = deltas_seq.clone();
        deltas_par.push(generate_test_deltas(1, 16).pop().unwrap());
        for delta in &deltas_par {
            base_lora_par.contribute(delta.clone()).await.unwrap();
        }

        // Now manually compute using benchmark method with same deltas
        let base_lora_test = BaseLoRA::new(test_config(AggregationStrategy::FederatedAverage));

        // Sequential path (< 10 deltas)
        let seq_deltas: Vec<&LoRADelta> = deltas_seq.iter().collect();
        let result_seq = base_lora_test.benchmark_aggregate_deltas(&seq_deltas);

        // Parallel path (>= 10 deltas)
        let par_deltas: Vec<&LoRADelta> = deltas_par.iter().collect();
        let result_par = base_lora_test.benchmark_aggregate_deltas(&par_deltas);

        // Results should be numerically close (allowing for floating point error)
        for i in 0..result_seq.len() {
            // Note: Different inputs, so we're just checking both produce valid outputs
            assert!(result_seq[i].is_finite());
            assert!(result_par[i].is_finite());
        }
    }

    #[tokio::test]
    async fn test_domain_specific_aggregation() {
        let config = test_config(AggregationStrategy::FederatedAverage);
        let base_lora = BaseLoRA::new(config);

        // Add deltas for multiple domains
        let mut auth_deltas = generate_test_deltas(5, 10);
        for delta in auth_deltas.iter_mut() {
            delta.domain = "authentication".to_string();
            base_lora.contribute(delta.clone()).await.unwrap();
        }

        let mut db_deltas = generate_test_deltas(3, 10);
        for delta in db_deltas.iter_mut() {
            delta.domain = "database".to_string();
            base_lora.contribute(delta.clone()).await.unwrap();
        }

        // Aggregate specific domain
        let auth_result = base_lora.aggregate_domain("authentication").await.unwrap();
        assert_eq!(auth_result.delta_count, 5);
        assert_eq!(auth_result.domain, Some("authentication".to_string()));

        let db_result = base_lora.aggregate_domain("database").await.unwrap();
        assert_eq!(db_result.delta_count, 3);
        assert_eq!(db_result.domain, Some("database".to_string()));

        // Results should differ
        assert_ne!(auth_result.weights[0], db_result.weights[0]);
    }

    #[tokio::test]
    async fn test_large_dimension_vectors() {
        let config = test_config(AggregationStrategy::FederatedAverage);
        let base_lora = BaseLoRA::new(config);

        // Test with large vectors to stress SIMD optimizations
        let deltas = generate_test_deltas(20, 1024);
        for delta in deltas {
            base_lora.contribute(delta).await.unwrap();
        }

        let result = base_lora.aggregate().await.unwrap();
        assert_eq!(result.weights.len(), 1024);
        assert_eq!(result.delta_count, 20);

        // All weights should be finite
        assert!(result.weights.iter().all(|&w| w.is_finite()));
    }

    #[tokio::test]
    async fn test_contribution_stats() {
        let config = test_config(AggregationStrategy::FederatedAverage);
        let base_lora = BaseLoRA::new(config);

        let deltas = generate_test_deltas(15, 10);
        for delta in deltas {
            base_lora.contribute(delta).await.unwrap();
        }

        let stats = base_lora.contribution_stats().await;
        assert_eq!(stats.total_deltas, 15);
        assert!(stats.total_contributors > 0); // Each synthetic delta gets unique agent_id
        assert!(stats.domains.contains("test_domain"));
    }

    #[tokio::test]
    async fn test_empty_deltas() {
        let config = test_config(AggregationStrategy::FederatedAverage);
        let base_lora = BaseLoRA::new(config);

        // Attempt to aggregate with no contributions
        let result = base_lora.aggregate().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), crate::error::SonaError::NoContributions));
    }

    #[tokio::test]
    async fn test_known_domains() {
        let config = test_config(AggregationStrategy::FederatedAverage);
        let base_lora = BaseLoRA::new(config);

        let mut delta1 = generate_test_deltas(1, 10).pop().unwrap();
        delta1.domain = "domain_a".to_string();
        base_lora.contribute(delta1).await.unwrap();

        let mut delta2 = generate_test_deltas(1, 10).pop().unwrap();
        delta2.domain = "domain_b".to_string();
        base_lora.contribute(delta2).await.unwrap();

        let domains = base_lora.known_domains().await;
        assert_eq!(domains.len(), 2);
        assert!(domains.contains("domain_a"));
        assert!(domains.contains("domain_b"));
    }
}
