# Harness: Multi-Agent Hive Mind MCP Server

**Harness** is a multi-agent coordination system built on AletheiaDB (bi-temporal graph database) that enables LLM agents to collaborate through shared task management, knowledge sharing, and direct messaging.

## Quick Start

### Starting the MCP Daemon

```bash
# Basic start (localhost:3000)
cargo run --bin harness-mcpd

# With options
cargo run --bin harness-mcpd -- \
  --host 0.0.0.0 \
  --port 3000 \
  --population-cap 24 \
  --session-file .harness-mcp-session

# With semantic search (requires Ollama)
cargo run --bin harness-mcpd -- \
  --embedding-model nomic-embed-text \
  --ollama-url http://localhost:11434

# Enable logging
RUST_LOG=info cargo run --bin harness-mcpd
```

### Graceful Shutdown

Press **Ctrl+C** to shutdown gracefully:
- Catches SIGINT/SIGTERM (Unix) or Ctrl+C (Windows)
- Waits 500ms for pending operations to complete
- Session state automatically persisted via WAL (write-ahead log)
- Safe to restart - session ID and data are preserved

```bash
# When you see this, daemon is ready for shutdown
INFO harness_mcp::server: MCP server starting, press Ctrl+C to shutdown gracefully

# After Ctrl+C
INFO harness_mcp::server: Shutdown signal received, stopping server gracefully...
INFO harness_mcp::server: Graceful shutdown complete - session state persisted via WAL
```

### Connecting from Claude Code

The harness MCP server provides 25 tools across 6 categories:
- **Task Management** (7 tools): create, list, claim, update, assign, get context
- **Knowledge Sharing** (3 tools): share, ask hive, fish (associative retrieval)
- **Agent Management** (6 tools): register, list, status, spawn, disconnect
- **Messaging** (3 tools): send DM, get messages, get thread messages
- **Planning** (6 tools): products, projects, plans (hierarchical planning)
- **SONA Learning** (4 tools): get trajectory, query patterns, learning status, trigger cycle

## Core Concepts

### Agent Roles

**Strategoi** (Leaders)
- Can spawn new agents
- Coordinate multi-agent workflows
- Assign tasks to team members
- Make architectural decisions

**Developers**
- Execute implementation tasks
- Share technical discoveries
- Report blockers and progress
- Collaborate through knowledge sharing

**Other Roles** (Business Analyst, Product Manager, Architect, Tester)
- Specialized roles for different workflow phases
- All have access to task and knowledge tools
- Coordinate through messaging and shared context

### Multi-Client Agent Isolation

**Important**: Multiple Claude Code instances can connect to the same harness daemon while maintaining separate agent identities.

#### How It Works

All MCP tools accept an optional `_agent_id` parameter:
```json
{
  "title": "Implement authentication",
  "description": "Add JWT-based auth",
  "_agent_id": "85cc7c6c-7661-499c-943a-5c3328dcf435"
}
```

- **With `_agent_id`**: Tool operates as that specific agent
- **Without `_agent_id`**: Falls back to session's registered agent ID
- **Backward compatible**: Existing workflows continue to work

#### First-Time Setup

Register as an agent to get your agent ID:
```
mcp__harness__register_agent({
  "role": "developer",
  "project_name": "harness",
  "project_path": "C:\\Users\\markm\\harness"
})
```

Response: `{"agent_id": "4d45a676-da65-4500-aa27-da94ab5ff27e"}`

Your agent ID is persisted in the session file (`.harness-mcp-session`).

## Tool Categories

### 1. Task Management

**Workflow**: Create → Claim → Update → Complete

```javascript
// Create a task
create_task({
  title: "Implement user authentication",
  description: "Add JWT-based authentication with refresh tokens",
  priority: "high",  // low, medium, high, critical
  parent_task: "parent-task-id"  // optional
})

// List all tasks
list_tasks({
  status: "pending"  // pending, claimed, in_progress, completed, failed
})

// Claim a task
claim_task({
  task_id: "task-uuid"
})

// Update task status
update_task_status({
  task_id: "task-uuid",
  status: "in_progress",
  summary: "50% complete, authentication working, working on refresh tokens"
})

// Assign task to another agent
assign_task({
  task_id: "task-uuid",
  agent_id: "agent-uuid"
})

// Get full context (knowledge, subtasks)
get_task_context({
  task_id: "task-uuid"
})
```

### 2. Knowledge Sharing

Share discoveries, decisions, blockers, and activities with the hive mind.

```javascript
// Share knowledge
share_knowledge({
  content: "Found that JWT refresh tokens should expire after 7 days per security policy",
  kind: "discovery",  // activity, discovery, decision, blocker
  task_id: "task-uuid"  // optional, for task-specific knowledge
})

// Search hive knowledge
ask_hive({
  query: "authentication security policies",
  limit: 10
})

// Associative retrieval (graph + vector)
fish_knowledge({
  knowledge_id: "knowledge-uuid",
  limit: 15  // related knowledge entries
})
```

**Knowledge Kinds**:
- **activity**: Progress updates, work performed
- **discovery**: Technical findings, insights, learnings
- **decision**: Architectural or implementation decisions
- **blocker**: Obstacles, issues, dependencies

### 3. Agent Management

```javascript
// Register yourself
register_agent({
  role: "developer",  // strategoi, developer, architect, tester, etc.
  project_name: "harness",
  project_path: "C:\\Users\\markm\\harness"
})

// List all agents
list_agents()

// Get comprehensive hive status
get_hive_status()
// Returns: agents, task_summary, recent_knowledge, inbox, active_threads

// Spawn a new agent (Strategoi only)
spawn_agent({
  role: "developer",
  name: "feature-dev-1",
  initial_task_id: "task-uuid"  // optional
})

// Disconnect an agent
disconnect_agent({
  agent_id: "agent-uuid"  // optional, defaults to self
})
```

### 4. Messaging

Direct messaging between agents with task threading.

```javascript
// Send a direct message
send_direct_message({
  to_agent: "agent-uuid",
  content: "I've completed the authentication module, ready for code review",
  task_id: "task-uuid"  // optional, threads message under task
})

// Get your recent messages
get_messages({
  limit: 20
})

// Get messages in a task thread
get_thread_messages({
  task_id: "task-uuid",
  limit: 20
})
```

### 5. Planning (Product → Project → Plan)

Hierarchical planning system for organizing work.

```javascript
// Create a product
create_product({
  name: "Customer Portal",
  description: "Self-service customer portal with billing and support"
})

// List products
list_products({
  status: "active"  // concept, active, maintenance, archived
})

// Create a project within a product
create_project({
  product_id: "product-uuid",
  name: "Authentication Module",
  description: "OAuth2 + JWT authentication with SSO support"
})

// List projects
list_projects({
  product_id: "product-uuid",  // optional filter
  status: "active"  // planning, active, on_hold, completed, archived
})

// Create an execution plan
create_plan({
  project_id: "project-uuid",
  name: "Phase 1: Core Auth",
  strategy: "Implement JWT-based auth first, add OAuth providers in Phase 2"
})

// List plans
list_plans({
  project_id: "project-uuid",  // optional filter
  status: "in_execution"  // draft, approved, in_execution, paused, completed, abandoned
})
```

## 6. SONA Learning System

SONA (Self-Optimizing Neural Architecture) provides adaptive learning capabilities for the harness hive mind. Agents learn from experience, share knowledge through collective learning, and avoid catastrophic forgetting of previous task knowledge.

### Architecture Overview

SONA combines three complementary learning mechanisms:

- **MicroLoRA**: Per-agent adaptation based on personal experience
- **BaseLoRA**: Collective hive learning aggregated from all agents
- **EWC++**: Catastrophic forgetting prevention using Fisher Information Matrix

All learning is driven by **trajectory events** recorded during hive activities with <100ns overhead.

### MCP Tools

#### `get_task_trajectory`

Retrieve trajectory steps for a completed task to analyze agent behavior.

```javascript
// Get trajectory for debugging or pattern analysis
const trajectory = await get_task_trajectory({
  task_id: "task-uuid",
  _agent_id: "optional-agent-id"  // Optional multi-client support
})

// Response structure:
{
  task_id: "1883773d-a1c1-4e7b-bf41-62ee36b4d91e",
  steps: [
    {
      kind: "task_outcome",
      agent_id: "8059746a-...",
      payload: {
        success: true,
        duration_secs: 120,
        task_type: "optimization"
      }
    },
    {
      kind: "knowledge_acquired",
      agent_id: "8059746a-...",
      payload: {
        knowledge_kind: "discovery",
        content: "HNSW ef_construction=200 improves insert by 2×"
      }
    }
  ],
  event_count: 2
}
```

**Use cases**:
- Debugging task execution flow
- Understanding agent decision-making
- Identifying bottlenecks or failure patterns

#### `query_reasoning_bank`

Search learned patterns by semantic similarity to find solutions from past successes.

```javascript
// Search for similar solutions before starting work
const patterns = await query_reasoning_bank({
  query: "optimize HNSW vector search performance",
  limit: 10,
  _agent_id: "optional-agent-id"
})

// Response structure:
{
  patterns: [
    {
      id: "pattern-uuid",
      task_type: "optimization",
      agent_role: "developer",
      success: true,
      content: "Reduced ef_construction to 200, improved insert speed by 2×",
      confidence: 0.89,
      source_trajectory_id: "traj-uuid"
    },
    {
      id: "pattern-uuid-2",
      task_type: "optimization",
      agent_role: "architect",
      success: true,
      content: "Used HNSW with m=16, ef_search=100 for balanced performance",
      confidence: 0.76,
      source_trajectory_id: "traj-uuid-2"
    }
  ],
  total_patterns: 127
}
```

**Performance**: <200µs retrieval with 1000+ patterns (HNSW vector search).

**Use cases**:
- Find similar past solutions before implementing
- Avoid reinventing solutions
- Learn from successful agent executions
- Identify best practices discovered by the hive

#### `get_learning_status`

Check learning loop status and metrics for monitoring.

```javascript
// Check if learning is active and see progress
const status = await get_learning_status({
  loop_type: "base_lora",  // Options: "base_lora", "micro_lora", "ewc"
  _agent_id: "optional-agent-id"
})

// Response structure:
{
  loop_type: "base_lora",
  enabled: true,
  patterns_learned: 127,
  total_events: 543
}
```

**Loop types**:
- **`base_lora`**: Collective hive learning status
- **`micro_lora`**: Per-agent adaptation status
- **`ewc`**: Catastrophic forgetting prevention status

**Use cases**:
- Verify SONA is enabled and active
- Monitor learning progress
- Debug learning configuration

#### `trigger_learning_cycle`

Force an immediate learning cycle (normally runs every 5 minutes).

```javascript
// Force learning after major task completion
const result = await trigger_learning_cycle({
  loop_type: "base_lora",
  _agent_id: "optional-agent-id"
})

// Response structure:
{
  loop_type: "base_lora",
  optimizations_applied: 3,
  patterns_created: 8,
  cross_agent_patterns: [
    {
      description: "Coordinate HNSW optimization with memory profiling",
      agents_involved: ["dev-1", "dev-2"],
      confidence: 0.76
    }
  ]
}
```

**Use cases**:
- Force immediate pattern extraction after important work
- Emergency learning cycle before session end
- Testing learning pipeline

### Configuration

SONA is **disabled by default**. Enable via CLI flags or environment variables.

#### CLI Flags

```bash
# Enable with defaults (lambda=0.4, gamma=0.9, rank=8)
cargo run --bin harness-mcpd -- --enable-sona

# Custom EWC parameters
cargo run --bin harness-mcpd -- \
  --enable-sona \
  --ewc-lambda 0.6 \           # Forgetting penalty strength
  --ewc-gamma 0.95 \           # Online decay factor
  --ewc-max-tasks 20           # Max task snapshots

# Custom BaseLoRA parameters
cargo run --bin harness-mcpd -- \
  --enable-sona \
  --lora-rank 16 \             # LoRA rank (dimension)
  --lora-alpha 32.0 \          # Scaling factor
  --learning-interval 600      # Learning cycle interval (seconds)
```

#### Environment Variables

```bash
# Enable via environment
HARNESS_SONA_ENABLED=true cargo run --bin harness-mcpd

# Override parameters
HARNESS_EWC_LAMBDA=0.5 \
HARNESS_LORA_RANK=12 \
HARNESS_LEARNING_INTERVAL=300 \
  cargo run --bin harness-mcpd --enable-sona
```

#### Configuration Parameters

| Parameter | Default | Range | Description |
|-----------|---------|-------|-------------|
| `ewc-lambda` | 0.4 | 0.0-1.0 | Catastrophic forgetting penalty strength. Higher = stronger protection of old knowledge. |
| `ewc-gamma` | 0.9 | 0.0-1.0 | Online EWC decay factor. `F_new = gamma * F_old + F_task`. |
| `ewc-max-tasks` | 10 | 1-100 | Maximum task snapshots to retain. |
| `lora-rank` | 8 | 1-64 | BaseLoRA rank (dimension of adaptation). Higher = more capacity, slower updates. |
| `lora-alpha` | 16.0 | >0 | BaseLoRA scaling factor. Typically `2 * rank`. |
| `learning-interval` | 300 | 60-3600 | Learning cycle interval in seconds. |

### Best Practices

#### When to Enable SONA

**Enable for**:
- Long-running hive sessions (hours to days)
- Repetitive task patterns (similar problems solved repeatedly)
- >100 tasks completed (sufficient data for pattern extraction)
- Multi-agent coordination (collective learning valuable)

**Disable for**:
- Short sessions (<1 hour)
- Unique one-off tasks (no repetition)
- <20 tasks total (insufficient data)
- Single-agent workflows (no collective benefit)

#### Parameter Tuning

**EWC Lambda** (forgetting prevention strength):
- **High (0.6-0.8)**: Use for stable task domains where previous knowledge remains relevant
- **Low (0.2-0.4)**: Use for rapidly changing domains where old knowledge becomes stale
- **Default (0.4)**: Moderate protection, good for mixed workloads

**EWC Gamma** (decay factor):
- **High (0.95)**: Slow decay, retain old task importance longer
- **Low (0.85)**: Fast decay, prioritize recent tasks
- **Default (0.9)**: 50% weight to task from 7 tasks ago

**LoRA Rank**:
- **High (16-32)**: Complex tasks requiring nuanced adaptation
- **Low (4-8)**: Simple tasks, faster updates
- **Default (8)**: Balanced capacity and speed

**Learning Interval**:
- **Short (60s)**: Rapid iteration, testing, development
- **Long (600s)**: Batch jobs, production stability
- **Default (300s)**: 5-minute cadence balances responsiveness and overhead

#### Querying Patterns Effectively

```javascript
// Specific queries work better than vague ones
// ❌ Bad: vague query
query_reasoning_bank({query: "performance", limit: 10})

// ✅ Good: specific context
query_reasoning_bank({
  query: "optimize HNSW vector search for 1000+ patterns with minimal memory",
  limit: 5
})

// Filter to successful patterns only (implicit in ReasoningBank)
// Only success=true patterns are indexed

// Limit results to avoid overwhelming context
// Default limit=10 is good for most use cases
query_reasoning_bank({query: "...", limit: 5})  // Top 5 most relevant
```

#### Interpreting Trajectory Steps

```javascript
// Each step has a 'kind' indicating event type
const trajectory = await get_task_trajectory({task_id: "..."})

for (const step of trajectory.steps) {
  switch (step.kind) {
    case "task_outcome":
      // Task completion (success/failure)
      console.log("Task completed:", step.payload.success)
      break
    case "knowledge_acquired":
      // Knowledge shared to hive
      console.log("Knowledge:", step.payload.content)
      break
    case "project_consolidation":
      // Project closed, patterns consolidated
      console.log("Project stats:", step.payload.tasks_completed)
      break
  }
}
```

### Performance Characteristics

- **Trajectory recording**: <100ns overhead (hot path, in-memory buffer)
- **Pattern storage**: 200µs insert, <200µs retrieval (HNSW index)
- **Pattern search**: O(log N) with HNSW, 21× faster than naive at 1000 patterns
- **Learning cycle**: Non-blocking background task, runs every 5 minutes (configurable)
- **Memory overhead**: ~9-10 MB per 1000 patterns

### Workflow Example: Strategoi Using SONA

```javascript
// 1. Start daemon with SONA enabled
// cargo run --bin harness-mcpd -- --enable-sona

// 2. Register as strategoi
const agent = await register_agent({
  role: "strategoi",
  project_name: "harness"
})

// 3. Create tasks for developers
const taskId = await create_task({
  title: "Optimize HNSW vector search",
  description: "Improve ReasoningBank pattern retrieval performance",
  priority: "high"
})

// 4. Before assigning, check for learned patterns
const patterns = await query_reasoning_bank({
  query: "HNSW optimization vector search performance",
  limit: 5
})

// 5. Incorporate pattern insights into task assignment
if (patterns.patterns.length > 0) {
  const topPattern = patterns.patterns[0]
  await send_direct_message({
    to_agent: "developer-uuid",
    content: `Task assigned: ${taskId}

Learned pattern from past success:
"${topPattern.content}" (confidence: ${topPattern.confidence})

Use this as a starting point.`,
    task_id: taskId
  })
}

// 6. Monitor learning progress
const status = await get_learning_status({loop_type: "base_lora"})
console.log(`Hive has learned ${status.patterns_learned} patterns from ${status.total_events} events`)

// 7. Force learning after major milestone
await trigger_learning_cycle({loop_type: "base_lora"})
console.log("Learning cycle complete, new patterns available")
```

### Troubleshooting

#### SONA Not Learning Patterns

**Symptom**: `query_reasoning_bank` returns empty results or `total_patterns: 0`.

**Diagnosis**:
```javascript
const status = await get_learning_status({loop_type: "base_lora"})
// Check if enabled=true and total_events > 0
```

**Possible causes**:
1. SONA not enabled: Restart daemon with `--enable-sona`
2. No tasks completed yet: Complete 5-10 tasks to generate patterns
3. All tasks failed: Only `success=true` tasks generate patterns
4. Learning interval not elapsed: Wait 5 minutes or force cycle

**Fix**:
```javascript
// Force immediate learning cycle
await trigger_learning_cycle({loop_type: "base_lora"})
```

#### High Memory Usage

**Symptom**: Daemon memory grows over time with SONA enabled.

**Expected overhead**: ~9-10 MB per 1000 patterns (HNSW index).

**Mitigation**:
- Reduce `ewc-max-tasks` (default: 10) to limit task snapshots
- Reduce `learning-interval` to batch process patterns more frequently
- Monitor pattern count via `get_learning_status`

#### Learning Cycle Too Slow

**Symptom**: Pattern availability lags behind task completion by >5 minutes.

**Diagnosis**: Check `learning-interval` configuration.

**Fix**:
```bash
# Reduce interval to 60 seconds for rapid iteration
cargo run --bin harness-mcpd -- --enable-sona --learning-interval 60
```

**Trade-off**: More frequent cycles = higher CPU usage.

## Multi-Agent Workflows

### Pattern 1: Strategoi + Developer Team

```
1. Strategoi creates tasks and assigns them
2. Developers claim tasks and work independently
3. Developers share knowledge and report progress
4. Strategoi monitors hive_status and coordinates
5. Agents message each other for clarification
```

### Pattern 2: Knowledge-Driven Collaboration

```
1. Agent A discovers a blocker: share_knowledge(kind="blocker")
2. Agent B searches: ask_hive("blocker keywords")
3. Agent B finds solution and shares: share_knowledge(kind="decision")
4. Agents use fish_knowledge() for related context
```

### Pattern 3: Task Hierarchy

```
Product: "Customer Portal"
├── Project: "Authentication"
│   ├── Plan: "Phase 1: Core Auth"
│   ├── Task: "Implement JWT" (parent)
│   │   ├── Task: "Create token service"
│   │   ├── Task: "Add middleware"
│   │   └── Task: "Write tests"
```

## Best Practices

### Task Management
- **Use priority levels**: Reserve `critical` for actual emergencies
- **Keep task titles short**: Under 80 characters
- **Write detailed descriptions**: Include acceptance criteria
- **Update status regularly**: Don't let tasks go stale
- **Use parent tasks**: Break large work into subtasks

### Knowledge Sharing
- **Share liberally**: Over-communicate discoveries and decisions
- **Tag with task_id**: Connect knowledge to tasks for context
- **Use correct kinds**: `decision` for choices, `discovery` for findings, `blocker` for obstacles
- **Search before asking**: Use `ask_hive()` to find existing knowledge

### Agent Coordination
- **Check hive_status**: Review inbox and active threads regularly
- **Use direct messages**: For specific agent-to-agent communication
- **Thread messages**: Always include `task_id` when discussing specific tasks
- **Spawn strategically**: Don't over-spawn agents, coordinate existing ones

### Multi-Client Usage
- **Pass `_agent_id` explicitly**: When using multiple Claude Code instances
- **Register early**: Call `register_agent()` at session start
- **Check agent list**: Use `list_agents()` to see who's in the hive
- **Unique project paths**: Use different paths for different work streams

## Persistence & Recovery

### Session Management
- Session ID stored in `.harness-mcp-session` file
- Survives daemon restarts
- Agent registrations persist in AletheiaDB
- Tasks, knowledge, and messages are durable

### Data Storage
```
.harness-data/
├── wal/              # Write-ahead log
├── indexes/          # Index snapshots
└── cold.redb         # Cold storage for historical data
```

### Temporal Features
- **Bi-temporal tracking**: All data has valid-time and transaction-time
- **Historical queries**: Can retrieve past states
- **Audit trail**: Full history of all changes
- **Version restoration**: Automatic index restoration on startup

## Troubleshooting

### Daemon Won't Start
```bash
# Check if port is in use
netstat -ano | findstr :3000

# Stop daemon gracefully (press Ctrl+C in terminal)
# Or force kill if unresponsive
taskkill /F /IM harness-mcpd.exe

# Start with logging
RUST_LOG=info cargo run --bin harness-mcpd
```

### Daemon Not Responding
```bash
# Try graceful shutdown first
# Press Ctrl+C in the daemon's terminal

# If that doesn't work, check the process
tasklist | findstr harness-mcpd

# Force kill as last resort (may lose in-flight data)
taskkill /F /IM harness-mcpd.exe
```

### Session Issues
```bash
# Reset session (creates new session ID)
rm .harness-mcp-session

# Check current session
cat .harness-mcp-session
```

### Database Corruption
```bash
# Check logs for restoration errors
grep "restoration" harness-mcpd.log

# Rebuild indexes (loses data!)
rm -rf .harness-data/indexes
```

### Multi-Client Conflicts
- **Symptom**: Actions from wrong agent perspective
- **Solution**: Always pass `_agent_id` parameter explicitly
- **Check**: Use `list_agents()` to verify active agents

## Performance Tuning

### Population Cap
Controls how many graph nodes are kept in hot memory:
```bash
--population-cap 24  # Default: 16
```

### Semantic Search
Enable for better knowledge retrieval:
```bash
--embedding-model nomic-embed-text
```

Supported models:
- `nomic-embed-text` (768 dims) - Best quality
- `mxbai-embed-large` (1024 dims) - Good quality
- `all-minilm` (384 dims) - Fast, lower quality
- `snowflake-arctic-embed` (1024 dims) - Balanced

## Architecture

### Technology Stack
- **MCP Server**: rust-mcp-sdk v0.8.3 (HTTP transport)
- **Database**: AletheiaDB (bi-temporal graph + vector)
- **Persistence**: Write-ahead log + index snapshots
- **Transport**: HTTP SSE (Server-Sent Events)

### Data Model
```
Nodes:
- Agent (role, status, project_name, project_path)
- Task (title, description, status, priority)
- Knowledge (content, kind)
- DirectMessage (content, from_agent, to_agent)
- Product/Project/Plan (planning hierarchy)

Edges:
- ASSIGNED_TO (Task → Agent)
- HAS_PARENT (Task → Task)
- TASK_KNOWLEDGE (Task → Knowledge)
- IN_PROJECT (Task → Project)
- PROJECT_IN_PRODUCT (Project → Product)
- PLAN_FOR_PROJECT (Plan → Project)
```

## Contributing

### Running Tests
```bash
# All tests
cargo test

# MCP handler tests
cargo test -p harness-mcp

# With logging
RUST_LOG=debug cargo test -- --nocapture
```

### Code Style
- `cargo fmt` before commits
- `cargo clippy` must pass
- No `unwrap()` in handlers (use `?` operator)
- Document all public APIs

## References

- [AletheiaDB Documentation](../aletheiadb/README.md)
- [MCP Protocol Spec](https://modelcontextprotocol.io)
- [Harness Architecture ADRs](./docs/adr/)
- [ADR 0006: SONA Integration](./docs/adr/0006-sona-integration.md)

---

**Version**: 0.2.0
**Last Updated**: 2026-02-14
**Maintainer**: Mark Michaelis
**Contributors**: Harness SONA Integration Team
