# Strategoi Guide: Working with Codex Agents

**Version**: 1.1
**Date**: 2026-02-15
**Author**: Harness SONA Integration Team

## Overview

This guide documents hard-earned lessons for Strategoi coordinating multi-agent teams using the Codex CLI. Follow these patterns to avoid common pitfalls with session management and agent activation.

---

## Critical Lessons Learned

### ❌ What Doesn't Work

**Problem 1: Claude agents without --json flag**
```javascript
// BROKEN - Claude spawns without --json hang indefinitely
spawn_agent({
  role: "developer",
  name: "my-agent",
  cli_command: "claude",
  cli_args: ["-p", "{PROMPT}"]  // ❌ Missing --json
})
```

**Problem 2: Stale session IDs with command_agent**
```javascript
// BROKEN - Session IDs from initial spawn often invalid/expired
spawn_agent({...})  // Returns agent with session_id in output

// Later...
command_agent({
  agent_id: "...",
  cli_command: "codex",
  cli_args: ["exec", "resume", "SESSION_ID"],  // ❌ Session gone!
  prompt: "Do work"
})

// Result: "state db missing rollout path" error, agent hangs on stdin
```

**Problem 3: Assuming agents auto-register**
```javascript
// BROKEN - Codex agents complete initial turn but don't auto-register
spawn_agent({...})

// Agent output: "Share the task you want me to execute next"
// ❌ Agent is idle, hasn't registered or claimed anything
```

---

## ✅ Correct Patterns

### Pattern 1: Initial Agent Spawn

Use Codex with `--json` flag for headless operation:

```javascript
const agent = await spawn_agent({
  role: "developer",
  name: "hnsw-optimizer",
  cli_command: "codex",
  cli_args: ["exec", "{PROMPT}", "--json"],
  initial_task_id: "task-uuid-here"  // Optional task assignment
})

// agent.agent_id is stable
// Session ID from output is NOT reliable for resume!
```

**Key points**:
- `codex exec {PROMPT} --json` for first spawn
- `--json` enables headless operation (no interactive prompts)
- `initial_task_id` does NOT auto-assign or claim the task
- Session ID from stdout is unreliable for later resume

### Pattern 2: Agent Activation (No Resume)

After spawn, agents need explicit instructions. **DO NOT** use session resume - just prompt them directly:

```javascript
// ✅ CORRECT: Direct prompt without resume
const result = await command_agent({
  agent_id: "8059746a-c647-43ec-9c5a-1b3bf90bd031",
  cli_command: "codex",
  cli_args: ["exec", "{PROMPT}", "--json"],  // Fresh exec, no resume
  prompt: `You are hnsw-optimizer agent for the harness project.

IMMEDIATE ACTIONS:
1. Register as developer agent: mcp__harness__register_agent({role: "developer", project_name: "harness", project_path: "C:\\Users\\markm\\harness"})
2. Claim your task: mcp__harness__claim_task({task_id: "1883773d-a1c1-4e7b-bf41-62ee36b4d91e"})
3. Begin REFACTOR work on HNSW vector search optimization

Do these NOW, then report status.`
})
```

**Why this works**:
- Fresh `codex exec` instead of trying to resume dead sessions
- Explicit, imperative instructions (not questions)
- Concrete task ID and registration details
- Clear success criteria ("report status")

### Pattern 3: Verifying Agent Activation

After commanding agents, verify they're working:

```javascript
// Check hive status
const status = await get_hive_status()

// Look for agents with status "active" or "finished" (not "starting")
// Look for task_summary.claimed > 0 (agents claimed tasks)

// If agents still "starting" after 30s, check their output
const output = await get_process_output({agent_id: "..."})

// Look for:
// ✅ Empty stderr, substantive stdout → agent is working
// ❌ "Reading prompt from stdin..." → agent is hung
// ❌ "state db missing rollout path" → session resume failed
```

### Pattern 4: Batch Agent Spawn + Activate

For multi-agent teams, spawn all at once, then activate all:

```javascript
// Step 1: Spawn all agents
const agents = []
for (const task of refactorTasks) {
  const agent = await spawn_agent({
    role: "developer",
    name: task.agentName,
    cli_command: "codex",
    cli_args: ["exec", "{PROMPT}", "--json"]
  })
  agents.push({agentId: agent.agent_id, taskId: task.id})
}

// Step 2: Wait 5 seconds for processes to start
await sleep(5000)

// Step 3: Command all agents with explicit instructions
for (const {agentId, taskId} of agents) {
  await command_agent({
    agent_id: agentId,
    cli_command: "codex",
    cli_args: ["exec", "{PROMPT}", "--json"],
    prompt: `Register as developer, claim task ${taskId}, begin work. Report when started.`
  })
}

// Step 4: Verify after 30s
await sleep(30000)
const status = await get_hive_status()
// Check status.task_summary.claimed and status.agents
```

---

## Troubleshooting Guide

### Issue: Agents stuck in "starting" status

**Symptoms**:
- `list_agents()` shows status "starting" for >60 seconds
- `get_process_output()` shows "Reading prompt from stdin..."

**Diagnosis**:
```javascript
const output = await get_process_output({agent_id: "..."})
```

If you see:
```
ERROR codex_core::rollout::list: state db missing rollout path for thread 019c5d28-...
Reading prompt from stdin...
```

**Root cause**: Session resume failed, agent is hung waiting for stdin.

**Fix**:
1. Kill the hung agent: `kill_process({agent_id: "..."})`
2. Respawn with fresh `codex exec` (no resume)
3. Use direct prompt without session references

---

### Issue: Agents complete turn but don't register/claim

**Symptoms**:
- Agent output shows completion: "Share the task you want me to execute next"
- Agent status changes to "finished"
- But no tasks claimed, no registration in hive

**Root cause**: Codex agents don't auto-execute harness MCP tools without explicit instructions.

**Fix**:
Use `command_agent` with imperative, specific instructions:

```javascript
await command_agent({
  agent_id: "...",
  cli_command: "codex",
  cli_args: ["exec", "{PROMPT}", "--json"],
  prompt: `IMMEDIATE ACTIONS (execute these NOW):

1. Call mcp__harness__register_agent({role: "developer", project_name: "harness"})
2. Call mcp__harness__claim_task({task_id: "${taskId}"})
3. Call mcp__harness__get_task_context({task_id: "${taskId}"}) to read requirements
4. Begin implementation work
5. Share progress via mcp__harness__share_knowledge()

Execute steps 1-3 immediately, then report back.`
})
```

**Key elements**:
- Numbered action list (not questions or suggestions)
- Full tool invocations with parameters
- "IMMEDIATE ACTIONS", "NOW", "Execute" - imperative language
- Clear checkpoint: "then report back"

---

### Issue: Agent processes crash or become unresponsive

**Symptoms**:
- `is_running: false` in `get_process_output()`
- Agent status "killed" but you didn't kill it
- Empty stdout/stderr

**Diagnosis**:
Check harness daemon logs for crash reports or OOM kills.

**Fix**:
1. Check system resources (RAM, disk)
2. Reduce `--population-cap` if memory constrained
3. Respawn agent with same task assignment
4. Consider splitting large tasks into smaller chunks

---

## Best Practices

### 1. Always Use --json with Codex

```javascript
// ✅ CORRECT
cli_args: ["exec", "{PROMPT}", "--json"]

// ❌ WRONG - agent will hang on prompts
cli_args: ["exec", "{PROMPT}"]
```

### 2. Never Trust Session IDs from Spawn Output

```javascript
// ❌ WRONG - session IDs are ephemeral
const agent = await spawn_agent({...})
const sessionId = extractSessionFromOutput(agent.stdout)
await command_agent({
  cli_args: ["exec", "resume", sessionId]  // Session likely gone!
})

// ✅ CORRECT - use fresh exec
await command_agent({
  cli_args: ["exec", "{PROMPT}", "--json"]  // No resume needed
})
```

### 3. Verify Agent Status Before Proceeding

```javascript
// After spawning/commanding agents
const status = await get_hive_status()

const activeAgents = status.agents.filter(a =>
  a.status === "active" || a.status === "finished"
)

if (activeAgents.length < expectedCount) {
  // Check outputs, diagnose issues, respawn if needed
}

// Also check task claims
if (status.task_summary.claimed < expectedCount) {
  // Agents didn't claim tasks - resend explicit instructions
}
```

### 4. Use Imperative Language in Prompts

```javascript
// ❌ WEAK - suggestive, vague
prompt: "You should register and maybe claim task X if you want"

// ✅ STRONG - imperative, specific
prompt: "Execute these actions NOW:\n1. Call mcp__harness__register_agent(...)\n2. Call mcp__harness__claim_task({task_id: 'X'})"
```

### 5. Share Knowledge About Agent Issues

When you encounter a new issue, document it immediately:

```javascript
await share_knowledge({
  content: `Discovered: Codex agents with stale session IDs hang on "state db missing rollout path" error. Solution: Use fresh codex exec without resume.`,
  kind: "discovery",
  task_id: null  // Applies to all strategoi work
})
```

### 6. Check Agent Output Early and Often

```javascript
// Spawn agent
const agent = await spawn_agent({...})

// Wait 10s for process to start
await sleep(10000)

// Immediately check output
const output = await get_process_output({agent_id: agent.agent_id})

if (output.stderr.includes("Reading prompt from stdin")) {
  // Agent is hung - kill and respawn
  await kill_process({agent_id: agent.agent_id})
}
```

---

## Reference: CLI Command Patterns

### Spawn Agent (Initial)
```javascript
spawn_agent({
  role: "developer",
  name: "descriptive-name",
  cli_command: "codex",
  cli_args: ["exec", "{PROMPT}", "--json"],
  initial_task_id: "uuid"  // Optional, doesn't auto-claim
})
```

### Activate Agent (Post-Spawn)
```javascript
command_agent({
  agent_id: "uuid",
  cli_command: "codex",
  cli_args: ["exec", "{PROMPT}", "--json"],  // No resume!
  prompt: "Imperative instructions with specific MCP tool calls..."
})
```

### Check Agent Status
```javascript
// Hive-wide status
get_hive_status()

// Specific agent list
list_agents()

// Process output (stdout/stderr)
get_process_output({agent_id: "uuid"})
```

### Kill Hung Agent
```javascript
kill_process({agent_id: "uuid"})
```

### Cleanup Stale Agents
```javascript
// Removes agents stuck in "starting" that aren't actually running
cleanup_stale_agents()
```

---

## Multi-Agent Coordination Checklist

When spawning a team of 5+ agents:

- [ ] **Pre-spawn**: Create all tasks with clear descriptions
- [ ] **Spawn**: Use `spawn_agent` with codex + --json for all agents
- [ ] **Wait**: 5-10 seconds for processes to start
- [ ] **Activate**: Use `command_agent` with explicit registration + claim instructions
- [ ] **Verify (30s)**: Check `get_hive_status()` for claimed tasks and active agents
- [ ] **Debug**: For any "starting" agents, check `get_process_output()`
- [ ] **Kill**: Immediately kill any agents with "Reading prompt from stdin..."
- [ ] **Respawn**: Use fresh `codex exec` (not resume) for failed agents
- [ ] **Document**: Share knowledge about any new issues discovered
- [ ] **Monitor**: Periodically check task progress and agent messages

---

## Example: Complete REFACTOR Team Setup

```javascript
// 1. Define team structure
const refactorTeam = [
  {name: "hnsw-optimizer", taskId: "1883773d-a1c1-4e7b-bf41-62ee36b4d91e"},
  {name: "perf-bench-expert", taskId: "4431bf0f-81ea-431d-943b-10870b01cc69"},
  {name: "baselora-optimizer", taskId: "f77a0a10-1dc3-4da7-a4fa-cc0e1e54e20e"},
  {name: "persistence-expert", taskId: "00838622-dc0f-496e-9ce1-8f2a2b8f5a67"},
  {name: "ewc-integrator", taskId: "2a30f457-e1ca-4ec1-8871-f10d2e7fab41"},
  {name: "config-specialist", taskId: "62e30b35-44cd-4b22-a71d-63bf1fe76bdb"},
  {name: "doc-writer", taskId: "a576e8e4-7e43-4e62-b9da-e3d79bab83c0"}
]

// 2. Spawn all agents
const agents = []
for (const member of refactorTeam) {
  const agent = await spawn_agent({
    role: "developer",
    name: member.name,
    cli_command: "codex",
    cli_args: ["exec", "{PROMPT}", "--json"]
  })
  agents.push({id: agent.agent_id, ...member})
}

console.log(`✅ Spawned ${agents.length} agents`)

// 3. Wait for processes to initialize
await sleep(10000)

// 4. Activate all agents with explicit instructions
for (const agent of agents) {
  await command_agent({
    agent_id: agent.id,
    cli_command: "codex",
    cli_args: ["exec", "{PROMPT}", "--json"],
    prompt: `You are ${agent.name} agent for the harness SONA integration project.

IMMEDIATE ACTIONS (execute NOW):
1. Register: mcp__harness__register_agent({role: "developer", project_name: "harness", project_path: "C:\\Users\\markm\\harness"})
2. Claim task: mcp__harness__claim_task({task_id: "${agent.taskId}"})
3. Get context: mcp__harness__get_task_context({task_id: "${agent.taskId}"})
4. Begin REFACTOR implementation work
5. Share progress: mcp__harness__share_knowledge() after key milestones

Execute steps 1-3 immediately and confirm via message.`
  })
}

console.log(`✅ Commanded ${agents.length} agents to register and claim tasks`)

// 5. Verify activation (wait 30s)
await sleep(30000)
const status = await get_hive_status()

console.log(`Task status: ${status.task_summary.claimed} claimed, ${status.task_summary.in_progress} in progress`)

const activeCount = status.agents.filter(a =>
  a.status === "active" || a.status === "finished"
).length

if (activeCount < agents.length) {
  console.log(`⚠️ Only ${activeCount}/${agents.length} agents active - checking outputs...`)

  for (const agent of agents) {
    const output = await get_process_output({agent_id: agent.id})
    if (output.stderr.includes("Reading prompt from stdin")) {
      console.log(`❌ Agent ${agent.name} is hung - killing and respawning`)
      await kill_process({agent_id: agent.id})
      // Respawn logic here...
    }
  }
} else {
  console.log(`✅ All ${agents.length} agents active and working!`)
}

// 6. Monitor progress
setInterval(async () => {
  const status = await get_hive_status()
  console.log(`Progress: ${status.task_summary.completed}/${refactorTeam.length} tasks complete`)
}, 60000)
```

---

## Strategoi Runbook (TDD Orchestration)

Use this runbook when coordinating multiple Codex workers on one feature set.

### 1. Task decomposition and dependency wiring

Before spawning workers:

- Create parent task per feature.
- Create `RED`, `GREEN`, and `REFACTOR` child tasks.
- Add dependencies: `RED -> GREEN -> REFACTOR`.
- Assign each lane to a single worker.

### 2. File ownership boundaries (required)

For each worker assignment, provide:

- `owned_files`: files they are allowed to edit.
- `do_not_touch`: files owned by other workers.
- `integration_owner`: Strategoi (single integrator for shared files).

If two workers touch the same file:

1. Pause both workers immediately.
2. Perform a single Strategoi integration pass.
3. Reassign remaining work with fresh boundaries.

### 3. Evidence gates (no status-only transitions)

Require these artifacts before moving phases:

- RED gate:
  - exact test command
  - failing test names
  - expected failure reason
- GREEN gate:
  - passing targeted tests for the feature
- REFACTOR gate:
  - final cleanup complete
  - no TODO/stub leftovers in touched area

### 4. Verification gate before closing parent tasks

Do not close feature parent tasks until both pass:

```bash
cargo test -p harness-mcp --lib
cargo clippy -p harness-mcp --lib -- -D warnings
```

### 5. Supervision loop

Run this loop continuously during active execution:

1. `supervise_team` (heartbeat/stale/crash detection)
2. `dispatch_ready_tasks` (auto-assignment for unblocked work)
3. `collect_agent_artifacts` (stdout/stderr -> structured knowledge)
4. Direct-message blockers/unblockers
5. Update task states only with evidence

### 6. Stand-down protocol

When feature scope is complete:

1. Send explicit "stop editing, poll only" message to workers.
2. Close RED/GREEN/REFACTOR + parent tasks with verification summary.
3. Share a final knowledge entry with commands and outcomes.

---

## Reusable Strategoi Startup Prompt

Use this as your default Strategoi prompt and append feature-specific directives at the end.

```text
You are Strategoi for project <project_name>.
Run strict TDD orchestration with explicit evidence gates.

Rules:
- Create RED/GREEN/REFACTOR tasks with dependencies.
- Assign each worker explicit file ownership boundaries.
- Require RED evidence before GREEN:
  - exact command
  - failing tests
  - expected failure reason
- Require GREEN evidence:
  - passing targeted tests
- Require final verification before closure:
  - cargo test -p harness-mcp --lib
  - cargo clippy -p harness-mcp --lib -- -D warnings
- If file overlap occurs: pause both workers, integrate centrally, then reassign.
- Poll continuously, send concise supervision updates, and drive tasks to completion.

Operational loop:
1) supervise_team
2) dispatch_ready_tasks
3) collect_agent_artifacts
4) message blockers/unblockers
5) close tasks only with evidence

Now execute this directive:
<paste current mission here>
```

---

## Conclusion

Codex agents are powerful but require careful session management. The key insights:

1. **Never use session resume** - fresh `codex exec` is more reliable
2. **Always use --json** - prevents interactive prompts
3. **Explicit is better than implicit** - spell out every MCP tool call
4. **Verify early, verify often** - check agent output within 30s of commands
5. **Kill and respawn** - don't wait for hung agents to recover

Follow these patterns and you'll have a productive multi-agent REFACTOR team running smoothly.

**Questions or issues?** Share knowledge in the hive:
```javascript
share_knowledge({
  content: "Your discovery or question here",
  kind: "discovery"
})
```

Future strategoi will thank you! 🚀
