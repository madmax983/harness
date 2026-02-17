# Harness

Multi-agent AI orchestration platform with persistent hive mind coordination.

## Overview

Harness enables multiple AI agents to collaborate on complex tasks through:
- **Task tracking** with hierarchical organization (Product → Project → Plan → Task)
- **Knowledge sharing** with semantic search and associative memory
- **Agent coordination** via direct messaging and role-based assignment
- **Persistent storage** using AletheiaDB bi-temporal graph database
- **MCP integration** for seamless LLM tool access

## Architecture

Harness consists of three main components:

### 1. Orchestrator (`harness-orchestrator`)
Manages agent lifecycle, terminal multiplexing, and swarm coordination. Supports both TUI and headless modes.

### 2. Persistence Layer (`harness-persistence`)
Provides repository abstraction over:
- **InMemoryRepository**: Fast in-memory storage for testing
- **AletheiaRepository**: Production graph database with bi-temporal storage

Entity hierarchy:
- **Session**: Hive instance with population cap
- **Product**: Top-level container (e.g., "AletheiaDB", "Harness", "Thorp")
- **Project**: Major initiatives within products
- **Plan**: Structured execution strategies
- **Task**: Individual work items with status tracking
- **Agent**: AI workers with roles (strategoi, architect, developer, tester, etc.)
- **Knowledge**: Shared discoveries, decisions, blockers, and activities
- **DirectMessage**: Agent-to-agent communication

### 3. MCP Server (`harness-mcp`)
Model Context Protocol server exposing a broad tool surface for LLM integration, including workflow orchestration:

**Task Management:**
- `create_task`, `list_tasks`, `claim_task`, `update_task_status`, `assign_task`, `get_task_context`

**Agent Coordination:**
- `register_agent`, `list_agents`, `send_direct_message`, `get_messages`, `get_thread_messages`

**Team Orchestration:**
- `spawn_agent`, `spawn_team_and_handshake`, `spawn_team_from_template`, `supervise_team`, `dispatch_ready_tasks`

**Knowledge Sharing:**
- `share_knowledge`, `ask_hive` (semantic search), `fish_knowledge` (associative memory)

**Product/Project/Plan:**
- `create_product`, `list_products`, `create_project`, `list_projects`, `create_plan`, `list_plans`

**Workflow Orchestration:**
- `create_workflow`, `list_workflows`, `trigger_workflow`
- `list_workflow_runs`, `get_workflow_run`
- `pause_workflow`, `resume_workflow`
- `retry_step`, `backfill_workflow`

**Hive Status:**
- `get_hive_status`

## Quick Start

### Running the Full Orchestrator

```bash
# TUI mode
cargo run -- --port 3000 --embedding-model nomic-embed-text

# Headless mode
cargo run -- --port 3000 --headless
```

### Running MCP Server Only

For integration with Claude Desktop, Codex, or other MCP clients:

```bash
cargo run --bin harness-mcpd -- --port 3000
```

MCP endpoint: `http://localhost:3000/sse`

### Running MCP Operator TUI

Connect a local TUI directly to the MCP server to inspect and execute tool calls:

```bash
# In one terminal: run server
cargo run --bin harness-mcpd -- --port 3000

# In another terminal: run operator TUI
cargo run --bin harness-tui -- --server-url http://127.0.0.1:3000/sse
```

In the TUI:
- Press `i` to enter tool command mode
- Run commands as `tool_name {json_args}` (example: `list_tasks {}`)
- Tool responses (including errors) are captured in the Tool Output panel

### Claude Desktop Configuration

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "harness": {
      "command": "cmd",
      "args": ["/c", "npx", "-y", "mcp-remote", "http://localhost:3000/sse"]
    }
  }
}
```

## Claude Code Plugin

For enhanced harness integration in Claude Code, install the official plugin:

```bash
# User-level (available in all projects)
cp -r plugins/harness ~/.claude/plugins/harness

# Project-level (only for this project)
cp -r plugins/harness .claude/plugins/harness
```

**Features:**
- 🤖 Auto-registers as strategoi agent on session start
- 📋 Smart task detection from user requests
- 💡 Knowledge sharing prompts after completions
- ⚡ Quick commands: `/hive`, `/task`, `/agents`
- 🎯 Multi-agent workflow skill for coordinating teams

See `plugins/README.md` for full documentation.

## Command-Line Options

```bash
Options:
  -p, --port <PORT>              MCP server port (default: 3000)
      --headless                 Run without TUI
  -e, --embedding-model <MODEL>  Ollama model for semantic search
      --ollama-url <URL>         Ollama base URL (default: http://localhost:11434)
      --agent-cli <CLI>          Agent CLI executable (default: "claude")
      --agent-runtime <RUNTIME>  Runtime adapter: claude, codex, gemini, claude_compatible
```

## Multi-Agent Workflow

1. **Create hierarchical structure:**
   ```bash
   create_product("Harness", "Multi-agent orchestration")
   create_project("MCP Server", "Implement MCP tools", product_id)
   create_plan("Phase 1", "Core tools first", project_id)
   ```

2. **Create tasks:**
   ```bash
   create_task("Design architecture", "Define entity schemas", priority="critical")
   create_task("Implement Product entity", "Add types and repository", priority="high")
   ```

3. **Spawn specialized team:**
   ```bash
   register_agent(role="architect")
   register_agent(role="developer")
   register_agent(role="tester")
   ```

4. **Coordinate work:**
   ```bash
   assign_task(task_id, agent_id)
   # Agents claim, work, and update status
   # Share knowledge as they discover insights
   ```

5. **Monitor progress:**
   ```bash
   get_hive_status()  # See agents, tasks, recent knowledge
   ask_hive("How do we handle errors?")  # Search shared knowledge
   ```

## Workflow Orchestration Quickstart

```bash
# 1) Define workflow with evidence gates
create_workflow({
  name: "harness-lib-gate",
  definition: {
    max_concurrency: 1,
    failure_policy: "fail_fast",
    retries: 1,
    steps: [
      { step_id: "red", tool: "task_completion_gate", red_evidence: "failing test evidence" },
      { step_id: "green", tool: "task_completion_gate", green_evidence: "passing targeted test evidence" },
      { step_id: "final", tool: "task_completion_gate", final_verification: "cargo test -p harness-mcp --lib && cargo clippy -p harness-mcp --lib -- -D warnings" }
    ]
  }
})

# 2) Queue and observe run state
trigger_workflow({ workflow_id })
list_workflow_runs({ workflow_id })
get_workflow_run({ workflow_run_id })

# 3) Operate workflow lifecycle
pause_workflow({ workflow_id })
resume_workflow({ workflow_id })
retry_step({ workflow_run_id, step_id: "green" })
backfill_workflow({ workflow_id, from: "2026-02-01T00:00:00Z", to: "2026-02-16T00:00:00Z", dry_run: true })
```

Status model:
- `queued`, `running`, `succeeded`, `failed`, `blocked`

Executor behavior:
- Scheduler heartbeat consumes queued workflow runs.
- Step transitions are persisted and queryable via `list_workflow_runs` / `get_workflow_run`.
- Existing schedules are a compatibility layer where each schedule run emits a single-step workflow run (`workflow_run_id`).
- `hyperv_vm` is currently a Phase-3 seam and intentionally blocks until VM execution is implemented.

## Configurable Team Templates

`spawn_team_from_template` supports built-in templates (`feature`, `bugfix`, `incident`) and custom TOML templates.

Template source precedence:
1. `template_path` on the `spawn_team_from_template` request
2. `HARNESS_TEAM_TEMPLATE_PATH` environment variable
3. Built-in templates in code

Important behavior:
- If `template_path` or `HARNESS_TEAM_TEMPLATE_PATH` is set, the requested template must exist in that TOML file.
- Built-in templates are not used as fallback when a TOML path/env is active.
- Template lookup is case-insensitive.

CLI defaults and overrides:
- Per-member defaults when omitted in TOML: `cli_command = "claude"` and `cli_args = ["-p", "{PROMPT}", "--allowedTools", "Bash,Read,Edit"]`.
- Request-level `cli_command` and `cli_args` override all template members for that spawn request.
- Request-level `directive` is appended as a global directive for each spawned member.

Example template file:
- `docs/examples/team-templates.example.toml`

Example request:
```bash
spawn_team_from_template({
  template: "review",
  template_path: "C:\\Users\\markm\\harness\\docs\\examples\\team-templates.example.toml",
  cli_command: "codex",
  cli_args: ["exec", "{PROMPT}", "--json"]
})
```

## Example: Using Harness to Build Harness

This codebase practices what it preaches! The Product/Project/Plan feature was built using harness itself:

- **Architect** agent designed the entity schemas
- **3 Developer** agents implemented Product, Project, and Plan entities in parallel
- **Tools Developer** built the MCP API layer
- All coordinated through task tracking and knowledge sharing
- **Result:** production-ready code with broad MCP coverage and full multi-agent coordination flows

Meta-level dogfooding achieved! 🐕🍲

## Development

### Running Tests

```bash
# Persistence layer (109 tests)
cargo test -p harness-persistence

# MCP server (19 tests)
cargo test -p harness-mcp

# All tests
cargo test --all
```

### Project Structure

```
harness/
├── crates/
│   ├── harness-orchestrator/   # Agent lifecycle, TUI, swarm coordination
│   ├── harness-persistence/    # Repository abstraction, entities
│   └── harness-mcp/             # MCP server, tool handlers
├── plugins/
│   └── harness/                 # Claude Code plugin
├── docs/
│   └── usage.md                 # Detailed usage documentation
└── src/
    └── main.rs                  # CLI entry point
```

## Dependencies

- **Runtime:** Tokio async runtime
- **Database:** AletheiaDB (bi-temporal graph database)
- **Embeddings:** Ollama (optional, for semantic search)
- **MCP:** Model Context Protocol for LLM integration

## Contributing

Harness is built using harness! To contribute:

1. Install the Claude Code plugin
2. Use `/task create` to track your work
3. Share knowledge with `share_knowledge` as you go
4. Coordinate with `/agents` if working on complex features
5. Submit PR with comprehensive tests

## License

[Add your license here]

## Meta

Built with ❤️ using harness itself for multi-agent coordination.

"Harness eating its own dog food since 2026" 🐕🍲
