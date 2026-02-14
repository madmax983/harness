# EWC++ Auto-Consolidation User Guide

## Quick Start

### Enable Auto-Consolidation in MCP Daemon

Add this to your daemon startup code (e.g., `harness-mcpd/src/main.rs`):

```rust
use harness_sona::{SonaEngine, EwcConfig};
use std::sync::Arc;

// Configure EWC++
let ewc_config = EwcConfig {
    lambda: 0.5,          // Forgetting prevention strength (0.0-1.0)
    gamma: 0.9,           // How much to retain old knowledge (0.0-1.0)
    max_tasks: 10,        // Number of tasks to remember
    normalize_fisher: true,
};

// Create engine with auto-consolidation enabled
let sona_engine = Arc::new(
    SonaEngine::builder()
        .with_ewc(ewc_config)
        .with_auto_consolidate(true)  // 👈 Enable auto-consolidation
        .build()
        .expect("Failed to build SONA engine")
);

// Attach to state
let state = HiveState::new(session, repo, process_manager)
    .with_sona_engine(sona_engine);
```

### What Happens Automatically

When you complete a task via MCP:
```javascript
mcp__harness__update_task_status({
  task_id: "abc-123",
  status: "completed",
  summary: "Successfully implemented feature"
})
```

The system automatically:
1. ✅ Extracts trajectory steps (actions taken during task)
2. ✅ Computes Fisher Information Matrix (importance weights)
3. ✅ Consolidates knowledge to prevent forgetting
4. ✅ Logs consolidation success/failure

### Configuration Parameters

#### `lambda` (Penalty Strength)
- **Range**: 0.0 to 1.0+
- **Default**: 0.5
- **Effect**: Higher = stronger protection of old knowledge
- **Tuning**:
  - 0.1-0.3: Light protection (faster adaptation, some forgetting OK)
  - 0.4-0.7: Balanced (recommended for most use cases)
  - 0.8-1.0: Strong protection (slower adaptation, minimal forgetting)

#### `gamma` (Decay Factor)
- **Range**: 0.0 to 1.0
- **Default**: 0.9
- **Effect**: How much to retain previous Fisher information
- **Tuning**:
  - 0.5-0.7: Emphasize recent tasks (dynamic environments)
  - 0.8-0.9: Balance old and new (recommended)
  - 0.95-1.0: Long memory (stable environments)

#### `max_tasks` (Snapshot Limit)
- **Range**: 1 to 100+
- **Default**: 10
- **Effect**: Number of task snapshots to retain in memory
- **Tuning**:
  - 5-10: Limited memory (resource-constrained)
  - 10-20: Good balance (recommended)
  - 20-50: Long-term retention (high-value tasks)

#### `normalize_fisher` (Normalization)
- **Type**: boolean
- **Default**: true
- **Effect**: Scale Fisher values to [0, 1] range
- **Recommendation**: Keep `true` for numerical stability

## Manual Consolidation

If you disable auto-consolidation, you can consolidate manually:

```rust
// Disable auto-consolidation
let sona_engine = Arc::new(
    SonaEngine::builder()
        .with_ewc(ewc_config)
        .with_auto_consolidate(false)  // Manual mode
        .build()
        .unwrap()
);

// Later, consolidate explicitly
let weights = vec![0.1, 0.2, 0.3, ...];  // Your model weights
let gradients = vec![
    vec![1.0, 0.5, -0.3, ...],  // Gradient sample 1
    vec![0.8, 0.4, -0.2, ...],  // Gradient sample 2
];

sona_engine.on_task_complete(
    agent_id,
    "task-abc-123",
    &weights,
    &gradients
).await?;
```

## Observability

### Check Consolidation Status

```rust
// Get agent's EWC state
let consolidator = sona_engine.agent_ewc_state(agent_id);

if let Some(c) = consolidator {
    println!("Tasks consolidated: {}", c.task_count());
    println!("Fisher diagonal: {:?}", c.running_fisher_diagonal());

    // Get snapshot for specific task
    if let Some(snapshot) = c.get_task_snapshot("task-abc-123") {
        println!("Optimal weights: {:?}", snapshot.optimal_weights());
        println!("Fisher diagonal: {:?}", snapshot.fisher_diagonal());
    }
}
```

### Logs

Auto-consolidation emits structured logs:

```
INFO  EWC++ auto-consolidation completed
  task_id = "abc-123"
  agent_id = "xyz-789"
  num_gradients = 15
```

```
WARN  EWC++ auto-consolidation failed
  task_id = "abc-123"
  agent_id = "xyz-789"
  error = "dimension mismatch"
```

## Use Cases

### Case 1: Sequential Task Learning
Agent completes tasks A → B → C without forgetting A when learning C.

**Config**: `lambda: 0.6, gamma: 0.9, max_tasks: 10`

### Case 2: Multi-Agent Coordination
Each agent maintains separate EWC state, prevents cross-agent interference.

**Config**: `lambda: 0.5, gamma: 0.8, max_tasks: 15`

### Case 3: Long-Running Projects
Retain knowledge from early phases throughout project lifecycle.

**Config**: `lambda: 0.7, gamma: 0.95, max_tasks: 30`

### Case 4: Rapid Prototyping
Allow faster forgetting for experimental tasks.

**Config**: `lambda: 0.3, gamma: 0.6, max_tasks: 5`

## Troubleshooting

### "EWC not enabled" Error
**Cause**: SONA engine created without `.with_ewc(config)`
**Fix**: Add EWC config to builder:
```rust
SonaEngine::builder()
    .with_ewc(ewc_config)  // 👈 Add this
    .build()
```

### "Dimension mismatch" Error
**Cause**: Weight dimensions changed between consolidations
**Fix**: Use consistent dimensionality or reset consolidator:
```rust
// Option 1: Use fixed dimension (current: 128)
const WEIGHT_DIM: usize = 128;

// Option 2: Reset consolidator (loses history)
// Requires adding reset method to SonaEngine
```

### No Consolidation Happening
**Check**:
1. Auto-consolidation enabled? `sona_engine.auto_consolidate_enabled()`
2. Task completed successfully? (not "failed" status)
3. Trajectory exists? Check `get_task_trajectory` MCP tool
4. Logs present? Look for "EWC++ auto-consolidation" messages

### High Memory Usage
**Cause**: Too many tasks retained (`max_tasks` too high)
**Fix**: Reduce `max_tasks` or implement periodic cleanup:
```rust
// Reduce max_tasks
let ewc_config = EwcConfig {
    max_tasks: 5,  // 👈 Lower limit
    ..Default::default()
};
```

## Advanced Usage

### Weight Importance Analysis
```rust
// Get importance ranking for a task
if let Some(consolidator) = sona_engine.agent_ewc_state(agent_id) {
    if let Some(importance) = consolidator.weight_importance("task-abc-123") {
        let top_weights = importance.top_k(10);
        for (dim, score) in top_weights {
            println!("Dimension {}: importance {:.3}", dim, score);
        }
    }
}
```

### Per-Task Penalty Contribution
```rust
// See how each task contributes to forgetting penalty
let weights = vec![0.1, 0.2, ...];
let contributions = consolidator.per_task_importance(&weights);

for (task_id, penalty) in contributions {
    println!("{}: penalty {:.4}", task_id, penalty);
}
```

### Compute Penalty for New Weights
```rust
// Before updating weights, check forgetting penalty
let new_weights = vec![0.2, 0.3, ...];
let penalty = consolidator.compute_penalty(&new_weights);

if penalty > THRESHOLD {
    println!("Warning: high forgetting risk (penalty: {:.2})", penalty);
}
```

## Performance Tips

1. **Batch consolidations**: For bulk task imports, disable auto and consolidate manually
2. **Tune dimensions**: Reduce weight dimensionality for faster consolidation
3. **Limit max_tasks**: Keep under 20 for real-time performance
4. **Async background**: Auto-consolidation runs async, doesn't block MCP response

## Migration Guide

### From Manual to Auto-Consolidation

**Before**:
```rust
// Manual consolidation after each task
for task in completed_tasks {
    sona_engine.on_task_complete(agent_id, &task.id, weights, gradients).await?;
}
```

**After**:
```rust
// Enable auto-consolidation once
let sona_engine = SonaEngine::builder()
    .with_ewc(config)
    .with_auto_consolidate(true)  // 👈 Add this
    .build()?;

// Tasks now consolidate automatically on completion
// No manual calls needed
```

## References

- **Implementation Details**: `docs/ewc-auto-consolidation-integration.md`
- **SONA Architecture**: `crates/harness-sona/README.md`
- **EWC Algorithm**: Kirkpatrick et al. (2017), "Overcoming catastrophic forgetting"
- **MCP Integration**: `crates/harness-mcp/CLAUDE.md`

---

**Questions?** Check logs, verify config, or file an issue with:
- SONA config used
- Task ID
- Agent ID
- Error messages
