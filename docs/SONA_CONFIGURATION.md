# SONA Configuration Guide

This guide covers the runtime configuration options for SONA (Self-Organizing Neural Architecture) in Harness.

## Overview

SONA provides adaptive learning capabilities for the hive mind:
- **EWC++**: Elastic Weight Consolidation - prevents catastrophic forgetting
- **BaseLoRA**: Collective hive learning through aggregated agent deltas
- **MicroLoRA**: Per-agent adaptation based on personal experience
- **Trajectory Recording**: Event-driven learning trigger system

All SONA features can be enabled/disabled and tuned via CLI flags or environment variables.

## Quick Start

### Enable SONA with defaults
```bash
cargo run  # SONA enabled by default
```

### Disable SONA for lightweight mode
```bash
cargo run -- --disable-sona
```

### Custom configuration
```bash
cargo run -- \
  --ewc-lambda 1.0 \
  --ewc-gamma 0.95 \
  --lora-rank 16 \
  --learning-interval-secs 600
```

## Configuration Options

### CLI Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--enable-sona` | boolean | `true` | Enable all SONA features |
| `--disable-sona` | boolean | - | Disable all SONA features |
| `--ewc-lambda <FLOAT>` | float | `0.4` | EWC penalty strength |
| `--ewc-gamma <FLOAT>` | float | `0.9` | EWC online decay factor (0-1) |
| `--lora-rank <INT>` | int | `8` | BaseLoRA rank dimension |
| `--learning-interval-secs <INT>` | int | `300` | Learning loop interval (seconds) |
| `--help` / `-h` | boolean | - | Display help message |

### Environment Variables

Environment variables have **higher priority** than CLI flags.

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `HARNESS_SONA_ENABLED` | `0` or `1` | `1` | Enable/disable SONA |
| `HARNESS_EWC_LAMBDA` | float | `0.4` | EWC penalty strength |
| `HARNESS_EWC_GAMMA` | float | `0.9` | EWC online decay factor |
| `HARNESS_LORA_RANK` | int | `8` | BaseLoRA rank dimension |
| `HARNESS_LEARNING_INTERVAL` | int | `300` | Learning interval (seconds) |

## Parameter Details

### SONA Enabled

**Purpose**: Master toggle for all SONA features.

**When to disable**:
- Minimal resource usage required
- Debugging non-learning workflows
- Benchmarking base performance

**Impact when disabled**:
- ✗ No EWC++ consolidation
- ✗ No BaseLoRA aggregation
- ✗ No background learning loop
- ✓ Trajectory recording still active
- ✓ Pattern storage available (ReasoningBank)

### EWC Lambda

**Purpose**: Penalty strength for catastrophic forgetting prevention.

**Range**: `0.0` to positive infinity (typically `0.1` - `10.0`)

**Behavior**:
- **Low values (0.1 - 0.5)**: Allow more weight drift, faster adaptation to new tasks
- **Medium values (0.5 - 2.0)**: Balanced forgetting prevention (recommended)
- **High values (2.0+)**: Strong protection of old knowledge, slower adaptation

**Recommended**:
- `0.4` - Default, good for most workloads
- `1.0` - Stronger protection for critical knowledge retention
- `0.2` - Faster adaptation when tasks are very different

### EWC Gamma

**Purpose**: Online EWC decay factor for weighting old vs. new task importance.

**Range**: `0.0` to `1.0` (exclusive to inclusive)

**Formula**: `Running Fisher = gamma * F_old + F_new`

**Behavior**:
- **Low values (0.5 - 0.7)**: Prioritize recent tasks, faster forgetting of old tasks
- **Medium values (0.8 - 0.9)**: Balanced retention (recommended)
- **High values (0.95 - 1.0)**: Strong retention of all previous tasks

**Recommended**:
- `0.9` - Default, balanced retention
- `0.95` - Longer memory for sequential task workflows
- `0.7` - Shorter memory when task domains change frequently

### LoRA Rank

**Purpose**: Dimension of the low-rank adaptation matrices in BaseLoRA.

**Range**: Positive integers (typically `4` - `64`)

**Behavior**:
- **Low rank (4-8)**: Smaller memory footprint, faster aggregation, less capacity
- **Medium rank (8-16)**: Balanced capacity and performance (recommended)
- **High rank (32+)**: More adaptation capacity, higher memory/compute cost

**Recommended**:
- `8` - Default, good balance for most agents
- `16` - More complex task domains requiring higher capacity
- `4` - Minimal footprint for resource-constrained environments

### Learning Interval

**Purpose**: How often the background learning loop runs to aggregate deltas.

**Range**: Positive integers (seconds, typically `60` - `600`)

**Behavior**:
- **Short interval (60-120s)**: Faster knowledge propagation, higher CPU usage
- **Medium interval (300-600s)**: Balanced responsiveness (recommended)
- **Long interval (600+s)**: Lower CPU usage, slower knowledge sharing

**Recommended**:
- `300` (5 min) - Default, good balance
- `600` (10 min) - Low-activity hives or resource constraints
- `120` (2 min) - High-activity hives requiring fast knowledge propagation

## Configuration Priority

The system resolves configuration in this order:

1. **Environment Variables** (highest priority)
2. **CLI Flags**
3. **Defaults** (lowest priority)

### Example Override Behavior

```bash
# Set env var
export HARNESS_EWC_LAMBDA=1.0

# CLI flag is IGNORED because env var has higher priority
cargo run -- --ewc-lambda 0.5

# Actual lambda used: 1.0 (from environment variable)
```

### Checking Active Configuration

On startup, Harness logs the active SONA configuration:

```
INFO harness: SONA configuration loaded: enabled=true, ewc_lambda=0.4, ewc_gamma=0.9, lora_rank=8, learning_interval_secs=300
```

## Common Configurations

### Default (Balanced)
```bash
cargo run
```
- EWC lambda: 0.4
- EWC gamma: 0.9
- LoRA rank: 8
- Learning interval: 300s

### High Retention (Long Memory)
```bash
cargo run -- \
  --ewc-lambda 1.0 \
  --ewc-gamma 0.95 \
  --lora-rank 16 \
  --learning-interval-secs 600
```
- Best for: Sequential tasks with cumulative knowledge
- Tradeoff: Slower adaptation to new task types

### Fast Adaptation (Short Memory)
```bash
cargo run -- \
  --ewc-lambda 0.2 \
  --ewc-gamma 0.7 \
  --lora-rank 8 \
  --learning-interval-secs 120
```
- Best for: Rapidly changing task domains
- Tradeoff: Faster forgetting of old knowledge

### Minimal Resource (SONA Disabled)
```bash
cargo run -- --disable-sona
```
- Best for: Debugging, benchmarking, resource constraints
- Tradeoff: No adaptive learning

### Environment Variable Configuration
```bash
# Production configuration via env vars
export HARNESS_SONA_ENABLED=1
export HARNESS_EWC_LAMBDA=0.6
export HARNESS_EWC_GAMMA=0.9
export HARNESS_LORA_RANK=12
export HARNESS_LEARNING_INTERVAL=420

cargo run
```

## Validation

### Automatic Validation

Harness validates SONA configuration on startup and will exit with an error if invalid:

```
ERROR harness: Invalid SONA configuration: EWC gamma must be in (0, 1]
```

### Validation Rules

- `ewc_lambda` >= 0.0
- `ewc_gamma` > 0.0 and <= 1.0
- `lora_rank` > 0
- `learning_interval_secs` > 0
- If `enabled=false`, all other parameters are ignored (no validation)

## Troubleshooting

### SONA not learning

**Symptom**: No pattern accumulation, agents don't improve over time

**Check**:
1. Verify SONA is enabled: `grep "SONA configuration" <log-file>`
2. Check learning interval is reasonable (not too long)
3. Ensure agents are completing tasks (trajectory events trigger learning)

### High memory usage

**Symptom**: Harness consuming excessive memory

**Solution**:
- Reduce `lora_rank` (e.g., from 16 to 8 or 4)
- Increase `learning_interval_secs` (aggregate less frequently)
- Consider disabling SONA with `--disable-sona`

### Agents forgetting old tasks

**Symptom**: Performance degrades on previously completed task types

**Solution**:
- Increase `ewc_lambda` (e.g., from 0.4 to 1.0)
- Increase `ewc_gamma` (e.g., from 0.9 to 0.95)
- These changes strengthen retention of old task knowledge

### Agents not adapting to new tasks

**Symptom**: Poor performance on new task types even after multiple completions

**Solution**:
- Decrease `ewc_lambda` (e.g., from 0.4 to 0.2)
- Decrease `ewc_gamma` (e.g., from 0.9 to 0.7)
- Increase `lora_rank` (more adaptation capacity)
- These changes allow faster learning of new patterns

## Help Output

Display all configuration options:

```bash
cargo run -- --help
```

This shows:
- All CLI flags with descriptions
- Environment variable names
- Default values
- Usage examples

## See Also

- [CLAUDE.md](../CLAUDE.md) - Main harness documentation
- [EWC++ Tests](../tests/ewc.rs) - EWC++ behavior and examples
- [SONA Architecture](../crates/harness-sona/README.md) - Technical deep dive
