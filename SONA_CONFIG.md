### SONA Configuration

SONA can be configured via CLI flags or environment variables.

#### CLI Flags

```bash
# Enable/disable SONA (default: enabled)
cargo run -- --enable-sona
cargo run -- --disable-sona

# Configure EWC++ (Elastic Weight Consolidation)
cargo run -- --ewc-lambda 0.4    # Penalty strength (default: 0.4)
cargo run -- --ewc-gamma 0.9     # Online decay factor (default: 0.9, range: 0-1)

# Configure BaseLoRA (collective learning)
cargo run -- --lora-rank 8       # LoRA rank (default: 8)

# Configure learning loop
cargo run -- --learning-interval-secs 300  # Background learning interval (default: 300s)

# Combined example
cargo run -- \
  --ewc-lambda 1.0 \
  --ewc-gamma 0.95 \
  --lora-rank 16 \
  --learning-interval-secs 600
```

#### Environment Variables

Environment variables have **higher priority** than CLI flags:

```bash
# Enable/disable SONA
export HARNESS_SONA_ENABLED=1  # or 0 to disable

# EWC++ configuration
export HARNESS_EWC_LAMBDA=0.4
export HARNESS_EWC_GAMMA=0.9

# BaseLoRA configuration
export HARNESS_LORA_RANK=8

# Learning loop configuration
export HARNESS_LEARNING_INTERVAL=300

# Start harness (env vars override CLI flags)
cargo run
```

#### Configuration Parameters

| Parameter | CLI Flag | Environment Variable | Default | Description |
|-----------|----------|---------------------|---------|-------------|
| **SONA Enabled** | `--enable-sona` / `--disable-sona` | `HARNESS_SONA_ENABLED` | `true` | Enable/disable all SONA features |
| **EWC Lambda** | `--ewc-lambda <FLOAT>` | `HARNESS_EWC_LAMBDA` | `0.4` | EWC penalty strength (prevents catastrophic forgetting) |
| **EWC Gamma** | `--ewc-gamma <FLOAT>` | `HARNESS_EWC_GAMMA` | `0.9` | Online EWC decay factor (0-1, higher = more weight on old tasks) |
| **LoRA Rank** | `--lora-rank <INT>` | `HARNESS_LORA_RANK` | `8` | BaseLoRA rank dimension (adaptation capacity) |
| **Learning Interval** | `--learning-interval-secs <INT>` | `HARNESS_LEARNING_INTERVAL` | `300` | Background learning loop interval (seconds) |

**Configuration Priority**: Environment Variables > CLI Flags > Defaults

#### Disabling SONA

For minimal resource usage, disable SONA entirely:

```bash
# CLI flag
cargo run -- --disable-sona

# Or environment variable
export HARNESS_SONA_ENABLED=0
cargo run
```

When disabled:
- No EWC++ consolidation
- No BaseLoRA aggregation
- No background learning loop
- Trajectory recording still active (for basic coordination)
- Pattern storage still available (for ReasoningBank)
