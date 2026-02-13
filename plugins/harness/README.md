# Harness Plugin for Claude Code

Seamless integration with harness hive mind for multi-agent coordination, task tracking, and knowledge sharing.

## Features

### 🤖 Auto-Registration
- Automatically registers you as strategoi agent on session start
- Ensures harness tools work immediately without manual setup

### 📋 Smart Task Detection
- Parses user requests like "implement X" or "add Y"
- Suggests creating tracked tasks for accountability
- Enables progress monitoring and knowledge sharing

### 💡 Knowledge Sharing Prompts
- Reminds you to share knowledge after task completions
- Prompts knowledge capture after git commits
- Builds collective hive intelligence

### ⚡ Quick Commands
- `/hive` - Show hive status (agents, tasks, knowledge)
- `/task` - Create, claim, complete, and list tasks
- `/agents` - Spawn and coordinate multiple agents

### 🎯 Multi-Agent Workflows
- Built-in skill for coordinating multiple AI agents
- Spawn specialized teams (architect, developers, testers)
- Track parallel work through task assignments

## Installation

The plugin is auto-discovered from `.claude/plugins/harness/`.

## Configuration

Create `.claude/plugins/harness.local.md` to customize:

```yaml
---
auto_register: true
auto_parse_tasks: true
auto_share_knowledge: true
show_hive_in_status: false
---

# Harness Plugin Settings

Customize harness integration behavior here.
```

## Usage

### Basic Workflow

1. **Session starts** → Auto-registers as strategoi agent
2. **User requests work** → Plugin suggests task creation
3. **Create task** → `/task create "Implement feature X"`
4. **Work on it** → Use harness tools naturally
5. **Complete task** → `/task complete task-abc123 "Summary..."`
6. **Share knowledge** → Plugin prompts knowledge capture

### Multi-Agent Workflow

```bash
# Check current hive state
/hive

# Create tasks for complex work
/task create "Design auth system" "Architecture for JWT auth" critical
/task create "Implement JWT tokens" "Token generation + validation" high
/task create "Write auth tests" "Integration tests for auth flow" medium

# Spawn specialized team
/agents spawn architect
/agents spawn developer 2
/agents spawn tester

# Monitor progress
/hive --verbose
```

### Commands

**`/hive [--verbose]`**
- Shows hive status: agents, tasks, recent knowledge
- Use `--verbose` for detailed task list

**`/task <subcommand>`**
- `create <title> [desc] [priority]` - Create task
- `claim <task_id>` - Claim pending task
- `complete <task_id> [summary]` - Mark complete
- `list [status]` - List tasks
- `context <task_id>` - Get full task context

**`/agents <subcommand>`**
- `list` - Show all agents
- `spawn <role> [count]` - Register new agents
- `message <agent_id> <msg>` - Send direct message

## MCP Server

The plugin automatically manages the harness MCP server via `cargo run --bin harness-mcpd`.

Tools available:
- Task management (create, list, claim, update, context)
- Agent coordination (register, list, messages)
- Knowledge sharing (share, ask/search)
- Product/Project/Plan hierarchy (create, list)

## Hooks

### session-start.sh
Auto-registers as strategoi agent when session begins.

### user-prompt-submit.sh
Detects task-worthy requests and suggests tracking them.

### post-tool-use.sh
Prompts knowledge sharing after completions and commits.

## Skills

### multi-agent-workflow.md
Comprehensive guide for coordinating multiple AI agents through harness.

## Best Practices

1. **Always create tasks** for trackable work
2. **Share knowledge** at completion milestones
3. **Use multi-agent coordination** for complex features
4. **Monitor with /hive** to understand current state
5. **Leverage knowledge search** with ask_hive before starting

## Tips

- Disable auto-features in settings if they become noisy
- Use `/hive --verbose` to debug agent/task state
- Check `ask_hive` before implementing - someone may have solved it
- Share blockers immediately so others can help

## Meta!

This plugin was built **using harness itself** - multi-agent coordination, task tracking, and knowledge sharing all the way down! 🐕🍲
