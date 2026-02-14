# SONA Configuration Quick Reference

## CLI Flags

```bash
--enable-sona                    # Enable SONA (default: on)
--disable-sona                   # Disable SONA
--ewc-lambda <FLOAT>             # EWC penalty (default: 0.4)
--ewc-gamma <FLOAT>              # EWC decay (default: 0.9, range: 0-1)
--lora-rank <INT>                # LoRA rank (default: 8)
--learning-interval-secs <INT>   # Learning interval (default: 300s)
--help                           # Show help
```

## Environment Variables

```bash
HARNESS_SONA_ENABLED=0|1         # Enable/disable SONA
HARNESS_EWC_LAMBDA=<FLOAT>       # EWC penalty
HARNESS_EWC_GAMMA=<FLOAT>        # EWC decay
HARNESS_LORA_RANK=<INT>          # LoRA rank
HARNESS_LEARNING_INTERVAL=<INT>  # Learning interval (seconds)
```

**Priority**: ENV > CLI > Defaults

## Quick Examples

### Default Configuration
```bash
cargo run
```
SONA: enabled | EWC λ=0.4 γ=0.9 | LoRA rank=8 | Interval=300s

### Disable SONA
```bash
cargo run -- --disable-sona
```
No EWC, no BaseLoRA, no learning loop (minimal mode)

### High Retention
```bash
cargo run -- --ewc-lambda 1.0 --ewc-gamma 0.95
```
Strong forgetting prevention, long memory

### Fast Adaptation
```bash
cargo run -- --ewc-lambda 0.2 --ewc-gamma 0.7
```
Weak forgetting prevention, short memory

### High Capacity
```bash
cargo run -- --lora-rank 16 --learning-interval-secs 600
```
More adaptation capacity, less frequent aggregation

### Environment Override
```bash
export HARNESS_EWC_LAMBDA=0.8
cargo run -- --ewc-lambda 0.4  # 0.8 used (env wins)
```

## Parameter Guide

| Parameter | Range | Low | Medium | High |
|-----------|-------|-----|--------|------|
| **EWC Lambda** | 0+ | 0.1-0.5<br>Fast adapt | 0.5-2.0<br>Balanced | 2.0+<br>Strong protect |
| **EWC Gamma** | 0-1 | 0.5-0.7<br>Recent tasks | 0.8-0.9<br>Balanced | 0.95-1.0<br>All tasks |
| **LoRA Rank** | 1+ | 4-8<br>Small footprint | 8-16<br>Balanced | 32+<br>High capacity |
| **Learning Interval** | 1+ | 60-120s<br>Fast propagation | 300-600s<br>Balanced | 600+s<br>Low CPU |

## Validation

```bash
# Valid
cargo run -- --ewc-lambda 0.5 --ewc-gamma 0.9 --lora-rank 8

# Invalid (gamma > 1.0)
cargo run -- --ewc-gamma 1.5
# ERROR: EWC gamma must be in (0, 1]

# Invalid (rank = 0)
cargo run -- --lora-rank 0
# ERROR: BaseLoRA rank must be > 0
```

## Help

```bash
cargo run -- --help
```
