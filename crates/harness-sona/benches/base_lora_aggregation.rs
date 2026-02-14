//! Benchmarks for BaseLoRA aggregation strategies.
//!
//! Compares sequential vs parallel performance across different delta set sizes.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use harness_sona::{AggregationStrategy, BaseLoRA, InternalBaseLoRAConfig, LoRADelta};
use std::time::Duration;

/// Generate synthetic LoRA deltas for benchmarking.
fn generate_deltas(count: usize, dim: usize) -> Vec<LoRADelta> {
    (0..count)
        .map(|i| {
            let weights: Vec<f32> = (0..dim).map(|j| (i + j) as f32 * 0.001).collect();
            LoRADelta::synthetic(
                "benchmark",
                weights,
                10 + (i as u32 % 50),      // task_count varies
                0.7 + (i as f64 % 30.0) / 100.0, // success_rate 0.7-0.99
            )
        })
        .collect()
}

/// Benchmark federated average aggregation.
fn bench_federated_average(c: &mut Criterion) {
    let mut group = c.benchmark_group("federated_average");
    group.measurement_time(Duration::from_secs(10));

    let config = InternalBaseLoRAConfig {
        rank: 128,
        alpha: 1.0,
        aggregation_strategy: AggregationStrategy::FederatedAverage,
        min_contributions: 1,
        staleness_threshold: Duration::from_secs(3600),
    };

    // Test different delta set sizes
    for delta_count in [5, 10, 20, 50, 100, 200, 500].iter() {
        let deltas = generate_deltas(*delta_count, 128);
        let delta_refs: Vec<&LoRADelta> = deltas.iter().collect();

        group.throughput(Throughput::Elements(*delta_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(delta_count),
            &delta_refs,
            |b, deltas| {
                let base_lora = BaseLoRA::new(config.clone());
                b.iter(|| {
                    let result = base_lora.benchmark_aggregate_deltas(black_box(deltas));
                    black_box(result);
                });
            },
        );
    }
    group.finish();
}

/// Benchmark weighted average aggregation.
fn bench_weighted_average(c: &mut Criterion) {
    let mut group = c.benchmark_group("weighted_average");
    group.measurement_time(Duration::from_secs(10));

    let config = InternalBaseLoRAConfig {
        rank: 128,
        alpha: 1.0,
        aggregation_strategy: AggregationStrategy::WeightedAverage,
        min_contributions: 1,
        staleness_threshold: Duration::from_secs(3600),
    };

    // Test different delta set sizes
    for delta_count in [5, 10, 20, 50, 100, 200, 500].iter() {
        let deltas = generate_deltas(*delta_count, 128);
        let delta_refs: Vec<&LoRADelta> = deltas.iter().collect();

        group.throughput(Throughput::Elements(*delta_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(delta_count),
            &delta_refs,
            |b, deltas| {
                let base_lora = BaseLoRA::new(config.clone());
                b.iter(|| {
                    let result = base_lora.benchmark_aggregate_deltas(black_box(deltas));
                    black_box(result);
                });
            },
        );
    }
    group.finish();
}

/// Benchmark different vector dimensions.
fn bench_varying_dimensions(c: &mut Criterion) {
    let mut group = c.benchmark_group("varying_dimensions");
    group.measurement_time(Duration::from_secs(10));

    let config = InternalBaseLoRAConfig {
        rank: 128,
        alpha: 1.0,
        aggregation_strategy: AggregationStrategy::FederatedAverage,
        min_contributions: 1,
        staleness_threshold: Duration::from_secs(3600),
    };

    let delta_count = 100; // Large enough to trigger parallelization

    // Test different dimensions
    for dim in [64, 128, 256, 512, 1024].iter() {
        let deltas = generate_deltas(delta_count, *dim);
        let delta_refs: Vec<&LoRADelta> = deltas.iter().collect();

        group.throughput(Throughput::Bytes((delta_count * dim * 4) as u64)); // 4 bytes per f32
        group.bench_with_input(
            BenchmarkId::from_parameter(dim),
            &delta_refs,
            |b, deltas| {
                let base_lora = BaseLoRA::new(config.clone());
                b.iter(|| {
                    let result = base_lora.benchmark_aggregate_deltas(black_box(deltas));
                    black_box(result);
                });
            },
        );
    }
    group.finish();
}


criterion_group!(
    benches,
    bench_federated_average,
    bench_weighted_average,
    bench_varying_dimensions
);
criterion_main!(benches);
