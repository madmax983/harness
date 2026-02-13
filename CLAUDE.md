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

The harness MCP server provides 21 tools across 5 categories:
- **Task Management** (7 tools): create, list, claim, update, assign, get context
- **Knowledge Sharing** (3 tools): share, ask hive, fish (associative retrieval)
- **Agent Management** (6 tools): register, list, status, spawn, disconnect
- **Messaging** (3 tools): send DM, get messages, get thread messages
- **Planning** (6 tools): products, projects, plans (hierarchical planning)

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

---

**Version**: 0.1.0
**Last Updated**: 2026-02-13
**Maintainer**: Mark Michaelis
