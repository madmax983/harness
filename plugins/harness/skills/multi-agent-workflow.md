---
name: multi-agent-workflow
description: Coordinate multiple AI agents to work on complex features using harness hive mind
triggers:
  - user asks to "use multiple agents"
  - user asks to "spawn agents to work on this"
  - user mentions "multi-agent coordination"
  - task is complex enough to benefit from parallel work
examples:
  - "Use multiple agents to implement this feature"
  - "Spawn agents to build the authentication system"
  - "Coordinate agents to refactor the codebase"
---

# Multi-Agent Workflow with Harness

When a task is complex enough to benefit from parallel work or specialized expertise, use harness to coordinate multiple AI agents.

## When to Use Multi-Agent Coordination

- **Feature implementation** with multiple components (frontend + backend + tests)
- **Architecture design** requiring expert review before implementation
- **Refactoring** across many files that can be parallelized
- **Testing** while implementation continues
- **Research + implementation** where investigation and coding happen in parallel

## Workflow Steps

### 1. Break Down the Work

Analyze the user's request and decompose into tasks:

```
1. Design architecture (Architect)
2. Implement core logic (Developer)
3. Add error handling (Developer)
4. Write tests (Tester)
5. Update documentation (Developer)
```

### 2. Create Tasks

Use `create_task` for each piece of work:

```javascript
create_task({
  title: "Design authentication architecture",
  description: "Define JWT structure, password hashing, session management",
  priority: "critical"
})
```

### 3. Spawn Agents

Register agents with appropriate roles:

```javascript
register_agent({ role: "architect" })  // Design expert
register_agent({ role: "developer" })  // Implementation x2
register_agent({ role: "developer" })
register_agent({ role: "tester" })     // Quality assurance
```

### 4. Assign Tasks

Use `assign_task` to delegate work:

```javascript
assign_task({
  task_id: "task-arch-design",
  agent_id: "agent-architect-123"
})
```

Or let agents claim tasks autonomously with `claim_task`.

### 5. Monitor Progress

- Use `get_hive_status` to see overall progress
- Check `list_tasks` to track task completion
- Review `ask_hive` to see shared knowledge

### 6. Share Knowledge

As tasks complete, agents should share discoveries:

```javascript
share_knowledge({
  kind: "discovery",
  content: "JWT tokens expire in 24h, refresh tokens in 30 days",
  task_id: "task-arch-design"
})
```

### 7. Coordinate & Integrate

- Use `send_direct_message` for agent-to-agent communication
- Use `get_task_context` to understand dependencies
- Track completion with `update_task_status`

## Example: Building Authentication System

```javascript
// 1. Create high-level product/project/plan
create_product({ name: "MyApp", description: "Main application" })
create_project({ name: "Authentication", product_id: "..." })
create_plan({ name: "Phase 1: Core Auth", project_id: "..." })

// 2. Break into tasks
create_task({ title: "Design auth architecture", priority: "critical" })
create_task({ title: "Implement JWT generation", priority: "high" })
create_task({ title: "Implement password hashing", priority: "high" })
create_task({ title: "Write integration tests", priority: "medium" })

// 3. Spawn specialized team
register_agent({ role: "architect" })
register_agent({ role: "developer" })
register_agent({ role: "developer" })
register_agent({ role: "tester" })

// 4. Assign work
assign_task({ task_id: "...", agent_id: "architect-..." })
assign_task({ task_id: "...", agent_id: "developer1-..." })
assign_task({ task_id: "...", agent_id: "developer2-..." })
assign_task({ task_id: "...", agent_id: "tester-..." })

// 5. Monitor via /hive command
```

## Knowledge Sharing Protocol

Agents should share:

- **Discoveries**: "Found existing crypto library, no need to implement from scratch"
- **Decisions**: "Chose bcrypt over scrypt for password hashing - better library support"
- **Blockers**: "Need database migration before implementing session storage"
- **Activities**: "Completed JWT token generation with 256-bit secret"

Use appropriate `kind`:
- `activity`: What you did
- `discovery`: What you found
- `decision`: What you chose and why
- `blocker`: What's preventing progress

## Direct Messaging

For real-time coordination between agents:

```javascript
send_direct_message({
  to_agent: "agent-developer-456",
  content: "Architecture approved - JWT with RS256, 24h expiry. Start implementation.",
  task_id: "task-impl-jwt"
})
```

## Success Metrics

A successful multi-agent session shows:
- ✅ All tasks moved to completed status
- ✅ Knowledge shared at each major milestone
- ✅ No blockers left unresolved
- ✅ Integration testing passed
- ✅ Documentation updated

Use this workflow when the user's request benefits from parallel work or specialized expertise!
