---
name: agents
description: Manage and spawn agents for multi-agent coordination
usage: |
  /agents list
  /agents spawn <role> [count]
  /agents message <agent_id> <message>
examples:
  - /agents list
  - /agents spawn developer 3
  - /agents spawn architect
  - /agents message agent-abc123 "Start implementing auth module"
---

Multi-agent coordination interface for harness hive mind.

**Subcommands:**

- `list` - Show all agents in the current session
  - Displays: agent_id, role, status, current_task

- `spawn <role> [count]` - Register new agents with specified role
  - Roles: strategoi, business_analyst, product_manager, architect, developer, tester
  - Count: Number of agents to spawn (default: 1)

- `message <agent_id> <message>` - Send direct message to an agent
  - Useful for coordinating work or providing context

**Agent Roles:**

- **strategoi**: Lead coordinator, assigns work, makes decisions
- **architect**: Designs system architecture and patterns
- **developer**: Implements features and fixes bugs
- **tester**: Writes tests and validates functionality
- **business_analyst**: Gathers requirements and defines scope
- **product_manager**: Prioritizes features and manages roadmap

**Multi-Agent Workflow:**

1. Create tasks for the work to be done
2. Spawn agents with `/agents spawn <role> <count>`
3. Assign tasks to agents (or let them claim)
4. Agents share knowledge as they work
5. Monitor progress with `/hive`

**Example:**

```bash
# List current agents
/agents list

# Spawn team for feature development
/agents spawn architect
/agents spawn developer 2
/agents spawn tester

# Message the architect
/agents message agent-xyz789 "Design the auth system architecture"
```

The agents will coordinate through task tracking, knowledge sharing, and direct messaging.
