# Harness v2: Simplified Task Coordination Architecture

**Date**: 2026-02-07
**Status**: Planning
**Author**: Mark Manning + Claude Sonnet 4.5

## Executive Summary

Harness v2 pivots from a multi-agent chat system to a **task coordination test harness** for AletheiaDB. Instead of competing with Gastown/Agent Teams, Harness becomes a hands-off orchestrator with a passive dashboard, using AletheiaDB as the shared coordination layer (replacing git-backed JSONL).

**Key Changes:**
- ❌ Drop: Channels, Messages, Subscriptions, god-mode controls
- ✅ Keep: Agent spawning, TUI dashboard, AletheiaDB persistence
- ✅ Add: Tasks, Activity Logs, HTTP/SSE MCP server

## Architecture

```
┌──────────────────────────────────┐
│   Multiple Claude Processes      │
│   (spawned by ProcessManager)    │
│                                   │
│   Each with MCP config:           │
│   --mcp-config harness-mcp.json  │
└────────────┬─────────────────────┘
             │
             │ HTTP/SSE MCP (many-to-one)
             │ Tools: create_task, claim_task,
             │        log_activity, etc.
             │
             ▼
     ┌───────────────────┐
     │  Harness          │
     │  MCP Server       │ ◄── Domain-specific HTTP/SSE server
     │  (Axum + SSE)     │
     └────────┬──────────┘
              │
              │ Direct Rust API calls (in-process)
              │
              ▼
      ┌──────────────┐
      │ AletheiaDB   │ ◄── Shared coordination layer
      │ (library)    │ ◄── Stress test target
      └──────┬───────┘
             │
             │ Polling queries
             │
             ▼
      ┌──────────────┐
      │   Harness    │
      │   TUI        │ ◄── Passive observer dashboard
      │   Dashboard  │
      └──────────────┘
```

## New Domain Model

### Core Entities

```rust
/// A unit of work to be completed
struct Task {
    id: TaskId,
    description: String,
    status: TaskStatus,  // Pending, InProgress, Completed, Failed
    assigned_to: Option<AgentId>,
    created_by: Option<AgentId>,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    session_id: SessionId,
}

enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// Activity log entry (replaces Message)
struct ActivityLog {
    id: LogId,
    agent_id: AgentId,
    task_id: Option<TaskId>,
    action: String,  // "claimed_task", "completed_subtask", "error", etc.
    details: String,
    timestamp: DateTime<Utc>,
    session_id: SessionId,
}

/// Agent (simplified - no subscriptions)
struct Agent {
    id: AgentId,
    role: String,
    status: AgentStatus,
    current_task: Option<TaskId>,
    created_at: DateTime<Utc>,
    session_id: SessionId,
}

/// Session (unchanged)
struct Session {
    id: SessionId,
    started_at: DateTime<Utc>,
    population_cap: usize,
}
```

### AletheiaDB Graph Model

```
(Session)-[:CONTAINS_TASK]->(Task)
(Session)-[:CONTAINS_AGENT]->(Agent)
(Agent)-[:CLAIMS]->(Task)
(Agent)-[:LOGGED]->(ActivityLog)
(ActivityLog)-[:ABOUT_TASK]->(Task)
```

## MCP Server Design

### Transport: HTTP + Server-Sent Events

**Why not stdio?**
- Stdio is one-to-one (each agent needs separate process)
- HTTP/SSE allows many-to-one (shared coordination)

**Implementation:**
- HTTP POST `/mcp/tools/{tool_name}` for tool invocations
- SSE endpoint `/mcp/events` for streaming responses
- Axum web framework

### MCP Tools

```typescript
// Task Coordination
{
  "name": "create_task",
  "description": "Create a new task for the team to work on",
  "inputSchema": {
    "type": "object",
    "properties": {
      "description": { "type": "string" },
      "priority": { "type": "string", "enum": ["low", "medium", "high"] }
    },
    "required": ["description"]
  }
}

{
  "name": "list_tasks",
  "description": "List available tasks (pending or in-progress)",
  "inputSchema": {
    "type": "object",
    "properties": {
      "status": { "type": "string", "enum": ["pending", "in_progress", "all"] }
    }
  }
}

{
  "name": "claim_task",
  "description": "Claim a task to work on it",
  "inputSchema": {
    "type": "object",
    "properties": {
      "task_id": { "type": "string" }
    },
    "required": ["task_id"]
  }
}

{
  "name": "complete_task",
  "description": "Mark a task as completed",
  "inputSchema": {
    "type": "object",
    "properties": {
      "task_id": { "type": "string" },
      "summary": { "type": "string" }
    },
    "required": ["task_id"]
  }
}

{
  "name": "fail_task",
  "description": "Mark a task as failed with reason",
  "inputSchema": {
    "type": "object",
    "properties": {
      "task_id": { "type": "string" },
      "reason": { "type": "string" }
    },
    "required": ["task_id", "reason"]
  }
}

// Activity Logging
{
  "name": "log_activity",
  "description": "Log an activity or event",
  "inputSchema": {
    "type": "object",
    "properties": {
      "action": { "type": "string" },
      "details": { "type": "string" },
      "task_id": { "type": "string" }  // Optional
    },
    "required": ["action", "details"]
  }
}

// Context & Query
{
  "name": "query_task_history",
  "description": "Get activity history for a specific task",
  "inputSchema": {
    "type": "object",
    "properties": {
      "task_id": { "type": "string" }
    },
    "required": ["task_id"]
  }
}

{
  "name": "search_logs",
  "description": "Semantic search across activity logs",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query": { "type": "string" },
      "limit": { "type": "number", "default": 10 }
    },
    "required": ["query"]
  }
}

{
  "name": "get_agent_status",
  "description": "Get current status of all agents",
  "inputSchema": {
    "type": "object",
    "properties": {}
  }
}
```

## TUI Dashboard

### View: Main Dashboard

```
┌─ Harness Dashboard ──────────────────────────────────────────────┐
│ Session: abc-123  │  Uptime: 2h 15m  │  Agents: 3/8  │  Tasks: 5 │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│ ┌─ Active Agents ────────────────────────────────────────────┐  │
│ │ • agent-a1b2  [Architect]  ▶ Task: Design API endpoints    │  │
│ │ • agent-c3d4  [Developer]  ▶ Task: Implement auth module   │  │
│ │ • agent-e5f6  [Tester]     ⏸ Idle (last active: 2m ago)    │  │
│ └────────────────────────────────────────────────────────────┘  │
│                                                                   │
│ ┌─ Task Queue ───────────────────────────────────────────────┐  │
│ │ [✓] Design API endpoints        agent-a1b2    14:20        │  │
│ │ [→] Implement auth module       agent-c3d4    14:25        │  │
│ │ [ ] Write integration tests     unclaimed                  │  │
│ │ [ ] Update documentation        unclaimed                  │  │
│ │ [✗] Deploy to staging           failed (see logs)          │  │
│ └────────────────────────────────────────────────────────────┘  │
│                                                                   │
│ ┌─ Recent Activity (live) ───────────────────────────────────┐  │
│ │ 14:28:42  agent-c3d4  completed_task: Implement auth       │  │
│ │ 14:28:15  agent-a1b2  log: Added JWT validation logic      │  │
│ │ 14:27:03  agent-c3d4  claimed_task: Implement auth module  │  │
│ │ 14:26:12  agent-e5f6  log: All unit tests passing          │  │
│ │ 14:25:45  agent-a1b2  completed_task: Design API endpoints │  │
│ └────────────────────────────────────────────────────────────┘  │
│                                                                   │
│ [q] Quit  [r] Refresh  [l] View Logs  [t] View Tasks             │
└──────────────────────────────────────────────────────────────────┘
```

### Key TUI Features

- **Real-time updates**: Poll AletheiaDB every 500ms for changes
- **Agent health**: Show active/idle/crashed status
- **Task progress**: Visual indicators for task status
- **Activity stream**: Scrolling log of recent events
- **No controls**: Fully passive - no spawn/kill buttons

## Implementation Plan

### Phase 1: Update Domain Model (2-3 hours)

**Tasks:**
1. Create new entity types in `harness-persistence/src/entities.rs`:
   - `Task`, `TaskStatus`, `TaskId`
   - `ActivityLog`, `LogId`
   - Update `Agent` (remove subscriptions, add current_task)
   - Keep `Session` unchanged

2. Update Repository trait in `harness-persistence/src/repository.rs`:
   ```rust
   #[allow(async_fn_in_trait)]
   pub trait Repository: Send + Sync {
       // Session (unchanged)
       async fn create_session(&self, session: &Session) -> RepositoryResult<()>;
       async fn get_session(&self, id: SessionId) -> RepositoryResult<Session>;

       // Agents (simplified)
       async fn create_agent(&self, agent: &Agent) -> RepositoryResult<()>;
       async fn get_agent(&self, id: AgentId) -> RepositoryResult<Agent>;
       async fn update_agent_status(&self, id: AgentId, status: AgentStatus) -> RepositoryResult<()>;
       async fn update_agent_task(&self, id: AgentId, task_id: Option<TaskId>) -> RepositoryResult<()>;
       async fn list_active_agents(&self, session_id: SessionId) -> RepositoryResult<Vec<Agent>>;

       // Tasks (new)
       async fn create_task(&self, task: &Task) -> RepositoryResult<()>;
       async fn get_task(&self, id: TaskId) -> RepositoryResult<Task>;
       async fn update_task_status(&self, id: TaskId, status: TaskStatus) -> RepositoryResult<()>;
       async fn claim_task(&self, task_id: TaskId, agent_id: AgentId) -> RepositoryResult<()>;
       async fn list_tasks(&self, session_id: SessionId, status: Option<TaskStatus>) -> RepositoryResult<Vec<Task>>;

       // Activity Logs (new)
       async fn log_activity(&self, log: &ActivityLog) -> RepositoryResult<()>;
       async fn get_task_logs(&self, task_id: TaskId) -> RepositoryResult<Vec<ActivityLog>>;
       async fn get_recent_logs(&self, session_id: SessionId, limit: usize) -> RepositoryResult<Vec<ActivityLog>>;
       async fn search_logs(&self, query: &str, limit: usize) -> RepositoryResult<Vec<ActivityLog>>;
   }
   ```

3. Update InMemoryRepository implementation
4. Update AletheiaRepository implementation (fix compilation errors + new methods)

**Tests:**
- Agent lifecycle (Pending → Active → Idle)
- Task lifecycle (Pending → InProgress → Completed)
- Task claiming (atomic, no double-claims)
- Activity logging and retrieval
- Semantic log search

### Phase 2: HTTP/SSE MCP Server (3-4 hours)

**Tasks:**
1. Create `crates/harness-mcp/src/server.rs`:
   - Axum HTTP server
   - SSE endpoint for streaming
   - Tool routing

2. Create tool handlers in `crates/harness-mcp/src/tools/`:
   - `tasks.rs`: create_task, list_tasks, claim_task, complete_task, fail_task
   - `activity.rs`: log_activity
   - `query.rs`: query_task_history, search_logs, get_agent_status

3. Create `crates/harness-mcp/src/state.rs`:
   - Shared state (Repository + session info)
   - Agent registration (track which agent called which tool)

4. MCP protocol implementation:
   - JSON-RPC 2.0 message handling
   - Tool discovery endpoint
   - Error handling

**Tests:**
- Tool invocation and response format
- Multiple concurrent agent connections
- SSE streaming for long-running operations
- Agent identity tracking

### Phase 3: TUI Dashboard Refactor (2-3 hours)

**Tasks:**
1. Update `crates/harness-tui/src/app.rs`:
   - Remove command mode, spawn dialog, kill confirm
   - Add task view state
   - Add activity log view state

2. Create dashboard components:
   - Agent status widget
   - Task queue widget
   - Activity stream widget (scrolling)

3. Implement polling:
   - Query repository every 500ms
   - Update dashboard state
   - Handle connection errors gracefully

**Tests:**
- UI rendering with mock data
- State updates on new events
- Scrolling and navigation

### Phase 4: Integration & Testing (2-3 hours)

**Tasks:**
1. Update `src/main.rs`:
   - Initialize AletheiaDB
   - Start HTTP/SSE MCP server
   - Initialize TUI with repository reference
   - Spawn initial agents with MCP config

2. Create test scenario:
   - Define 3-5 initial tasks
   - Spawn 3 agents with different roles
   - Watch agents coordinate through MCP tools
   - Verify dashboard shows activity

3. Stress testing:
   - Spawn 8 agents (population cap)
   - Create 20+ tasks
   - Measure concurrent write performance
   - Verify ACID properties (no double-claims)

**Acceptance Criteria:**
- All agents can connect to MCP server
- Tasks are claimed atomically (no conflicts)
- Dashboard shows real-time updates
- Activity logs are searchable
- No compilation warnings
- All tests pass

## Database Stress Test Validation

### What This Tests:

**Concurrent Writes:**
- Multiple agents claiming tasks simultaneously
- Multiple agents logging activity at the same time
- Task status updates

**Graph Queries:**
- Find all tasks for a session
- Find all logs for a task
- Find agent's current task

**Property-Based Lookup:**
- find_nodes_by_property("Task", "status", "pending")
- find_nodes_by_property("Agent", "status", "active")

**Semantic Search:**
- Vector embeddings on ActivityLog.details
- search_logs(query) uses AletheiaDB vector search

**ACID Properties:**
- Task claiming must be atomic (test with race conditions)
- No lost logs during concurrent writes
- Consistent reads during writes

## Success Criteria

1. ✅ All agents coordinate through AletheiaDB MCP server
2. ✅ No double-claims on tasks (atomic operations)
3. ✅ Dashboard shows real-time activity (500ms lag max)
4. ✅ Semantic search works on activity logs
5. ✅ Can handle 8 concurrent agents writing
6. ✅ No compilation errors or warnings
7. ✅ TUI is responsive and doesn't crash
8. ✅ AletheiaDB surfaces any performance/correctness issues

## Non-Goals

- ❌ God-mode controls (spawn/kill from TUI)
- ❌ Chat/messaging system
- ❌ Channel subscriptions
- ❌ Complex task dependencies
- ❌ Production deployment concerns
- ❌ Authentication/authorization

This is a **test harness**, not a production system.

## Risks & Mitigations

**Risk**: HTTP/SSE MCP not well-tested
**Mitigation**: Start simple, follow MCP spec closely, test with curl first

**Risk**: AletheiaDB performance issues under load
**Mitigation**: This is the goal! Surface issues early

**Risk**: TUI polling creates too much load
**Mitigation**: Configurable poll interval, use change detection

**Risk**: Agents don't coordinate well through MCP
**Mitigation**: Clear tool documentation, example system prompts

## Timeline

- **Phase 1**: Domain Model - 2-3 hours
- **Phase 2**: MCP Server - 3-4 hours
- **Phase 3**: TUI Dashboard - 2-3 hours
- **Phase 4**: Integration - 2-3 hours

**Total**: 9-13 hours of focused work

## Next Steps

1. Review plan with stakeholder
2. Execute Phase 1 (domain model refactor)
3. Checkpoint: Verify all tests pass
4. Execute Phase 2 (MCP server)
5. Checkpoint: Test with curl + manual agent
6. Execute Phase 3 (TUI)
7. Execute Phase 4 (integration)
8. Final demo: Watch agents coordinate through dashboard
