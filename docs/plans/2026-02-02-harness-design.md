# Harness: Multi-Claude Orchestration System

## Overview

Harness is a general-purpose agent swarm infrastructure where multiple Claude instances communicate through an MCP-powered chat server. Built on GallifreyDB for bi-temporal conversation persistence with semantic pathfinding.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         TUI (Ratatui)                          │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌───────────┐ │
│  │  #general   │ │ #implement  │ │   Agents    │ │  Control  │ │
│  │  channel    │ │  channel    │ │   panel     │ │   panel   │ │
│  └─────────────┘ └─────────────┘ └─────────────┘ └───────────┘ │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    MCP Chat Server (Rust)                       │
│  Tools: send_message, read_messages, list_channels,            │
│         spawn_agent, kill_agent, list_agents (god-mode)        │
└─────────────────────────────────────────────────────────────────┘
         │                    │                       │
         ▼                    ▼                       ▼
┌─────────────────┐  ┌─────────────────┐    ┌─────────────────────┐
│   GallifreyDB   │  │   Orchestrator  │    │   Ollama Embeddings │
│  (persistence)  │  │ (process mgmt)  │    │   (via Gallifrey)   │
└─────────────────┘  └─────────────────┘    └─────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
        ┌──────────┐   ┌──────────┐    ┌──────────┐
        │ Claude 1 │   │ Claude 2 │    │ Claude N │
        │ (claude  │   │ (claude  │    │ (claude  │
        │   -p)    │   │   -p)    │    │   -p)    │
        └──────────┘   └──────────┘    └──────────┘
```

## Core Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Communication model | Shared channels | Simple, observable, Slack-like familiarity |
| User participation | God mode | Admin controls + natural participation |
| Agent spawning | Dynamic | Agents can request spawns, max flexibility |
| Guardrails | Population cap only | YAGNI - add complexity when needed |
| Interface | TUI (Ratatui) | Real-time visibility, terminal-native |
| Persistence | GallifreyDB | Bi-temporal graph, semantic pathfinding, embeddings |
| Claude connection | `--mcp-config` flag | Clean, per-agent configuration |

## MCP Chat Server Tools

### Standard Tools (all agents)

| Tool | Parameters | Description |
|------|------------|-------------|
| `send_message` | `channel`, `content`, `reply_to?` | Post a message. Optional reply creates an edge in the graph. |
| `read_messages` | `channel`, `limit?`, `since?`, `semantic_query?` | Fetch messages. Can filter by time or semantic similarity. |
| `list_channels` | - | Get available channels and their descriptions. |
| `create_channel` | `name`, `description` | Spin up a new channel for focused discussion. |
| `whoami` | - | Returns agent's own identity, role, and spawn context. |
| `list_agents` | `channel?` | See who's in a channel or all active agents. |
| `request_spawn` | `role`, `reason`, `channel` | Request a new agent. Subject to population cap. |

### God-Mode Tools (TUI only)

| Tool | Parameters | Description |
|------|------------|-------------|
| `spawn_agent` | `role`, `system_prompt?`, `channels` | Directly spawn an agent, bypassing request flow. |
| `kill_agent` | `agent_id` | Terminate an agent's Claude process. |
| `set_population_cap` | `max` | Adjust the cap at runtime. |
| `broadcast` | `content` | Send to all channels simultaneously. |
| `clear_channel` | `channel` | Wipe a channel's messages (keeps history in Gallifrey). |

## GallifreyDB Schema

### Nodes

| Type | Properties | Purpose |
|------|------------|---------|
| `Agent` | `id`, `role`, `system_prompt`, `spawned_by`, `status` | Tracks each Claude instance |
| `Channel` | `name`, `description`, `created_at` | Communication spaces |
| `Message` | `id`, `content`, `embedding`, `timestamp` | The actual messages |
| `Session` | `id`, `started_at`, `config` | Groups activity into runs |

### Edges

| Type | From → To | Properties |
|------|-----------|------------|
| `POSTED` | Agent → Message | `timestamp` |
| `IN_CHANNEL` | Message → Channel | - |
| `REPLY_TO` | Message → Message | Builds conversation threads |
| `SPAWNED` | Agent → Agent | Who created whom |
| `SUBSCRIBED` | Agent → Channel | Agent's channel memberships |
| `PART_OF` | * → Session | Links everything to a session |

### Enabled Queries

**Temporal:**
- "Show me the #architecture channel as it was at 3:45pm"
- "What messages existed before agent-7 was spawned?"
- "Trace the lineage of this decision back to its origin"

**Semantic pathfinding:**
- "How did we get from the initial problem statement to this solution?"
- "What's the shortest conceptual path between agent-3's idea and agent-7's implementation?"
- "Find all reasoning chains that led to consensus on X"

## Orchestrator

### Responsibilities

- Spawn `claude -p` processes with `--mcp-config` pointing to the server
- Track process health (heartbeat via periodic `whoami` calls or OS-level)
- Enforce population cap, queue spawn requests when at capacity
- Clean shutdown - graceful termination signal, timeout, force kill

### Agent Lifecycle

```
┌─────────┐   spawn    ┌─────────┐  first msg  ┌────────┐
│ PENDING │ ─────────▶ │ STARTING│ ──────────▶ │ ACTIVE │
└─────────┘            └─────────┘             └────────┘
                                                   │
                          ┌────────────────────────┤
                          ▼                        ▼
                    ┌──────────┐            ┌───────────┐
                    │ FINISHED │            │  KILLED   │
                    │ (natural)│            │ (by user) │
                    └──────────┘            └───────────┘
```

### Spawn Command

```bash
claude -p "You are {role}. {system_prompt}" \
  --mcp-config '{"harness": {"command": "...", "args": [...]}}'
```

## TUI Layout

```
┌──────────────────────────────────────────────────────────────────────────┐
│ HARNESS v0.1.0                            Agents: 4/8    Session: 00:23  │
├────────────────────────────────────┬─────────────────────────────────────┤
│ CHANNELS        │ #general                                               │
│                 │─────────────────────────────────────────────────────── │
│ ▶ #general (4)  │ [14:23] architect: I suggest we split this into three  │
│   #implement    │         modules - core, api, and storage.              │
│   #review       │ [14:24] critic: What about the coupling between api    │
│   #debug        │         and storage? Seems risky.                      │
│                 │ [14:24] architect: Good point. We could introduce...   │
│─────────────────│ [14:25] ▶ YOU: What if we use an event bus instead?    │
│ AGENTS          │                                                        │
│─────────────────│                                                        │
│ ● architect     │                                                        │
│ ● critic        │                                                        │
│ ● implementer   │                                                        │
│ ○ researcher    │                                                        │
│   (spawning...) │                                                        │
├────────────────────────────────────┴─────────────────────────────────────┤
│ > Type message... (Tab: switch pane, Ctrl+S: spawn, Ctrl+K: kill, ?: help│
└──────────────────────────────────────────────────────────────────────────┘
```

### Key Bindings

- `Tab` - Cycle focus: channels → message view → input → agents
- `Enter` - Send message to selected channel
- `Ctrl+S` - Open spawn dialog (pick role, target channel)
- `Ctrl+K` - Kill selected agent (with confirmation)
- `/` - Command mode (`/clear`, `/broadcast`, `/cap 10`, `/query <semantic>`)
- `j/k` or arrows - Scroll messages/select items
- `Ctrl+F` - Semantic search across all channels

## Error Handling

### Agent Failures

- **Process crash** → Orchestrator detects via process exit, marks agent `CRASHED` in Gallifrey, posts system message to subscribed channels
- **Hang detection** → Configurable timeout, warn then kill
- **MCP connection lost** → Attempt reconnect or terminate

### Resource Exhaustion

- **Population cap reached** → `request_spawn` returns `queued` with position
- **Gallifrey connection fails** → Buffer messages in memory, retry, surface error in TUI
- **Ollama unavailable** → Graceful degradation, embeddings skipped

### Recovery

- **Session resume** → Load session from Gallifrey, spawn fresh agents (they read history to catch up)
- **Channel reconstruction** → Messages exist in Gallifrey, queryable immediately

## Crate Structure

```
harness/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── harness-mcp/        # MCP server implementation
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── tools/      # Tool handlers
│   │       ├── server.rs   # rust-mcp-sdk server setup
│   │       └── state.rs    # Shared state, channel subscriptions
│   │
│   ├── harness-orchestrator/  # Process management
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── spawn.rs    # Claude process spawning
│   │       ├── lifecycle.rs # Health checks, termination
│   │       └── config.rs   # Agent templates, population cap
│   │
│   ├── harness-persistence/   # GallifreyDB integration
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── schema.rs   # Node/edge types
│   │       ├── queries.rs  # Temporal queries, semantic pathfinding
│   │       └── embeddings.rs # Gallifrey embedding calls
│   │
│   └── harness-tui/        # Ratatui interface
│       └── src/
│           ├── lib.rs
│           ├── app.rs      # App state, event loop
│           ├── ui/         # Layout components
│           └── commands.rs # Slash command parsing
│
└── src/
    └── main.rs             # Binary entry point
```

## Dependencies

- `rust-mcp-sdk` - MCP server
- `gallifreydb` - Persistence + embeddings
- `ratatui` + `crossterm` - TUI
- `tokio` - Async runtime

## Testing Strategy

### Unit Tests
- Tool handlers in isolation (mock Gallifrey, mock orchestrator)
- Message routing logic
- Spawn command construction
- TUI state transitions

### Integration Tests
- MCP server with real Gallifrey
- Full message flow: send → persist → read → verify graph structure
- Semantic search with actual embeddings

### End-to-End Tests
- Spawn a real Claude, send a message, verify response appears
- Population cap enforcement
- God-mode commands

### Property-Based Tests
- Channel name validation
- Message ordering guarantees
- Concurrent spawn requests don't exceed cap

## Not Building (YAGNI)

- Token budgets per agent
- Web UI
- Agent memory/state persistence across restarts
- Complex permission systems beyond population cap
