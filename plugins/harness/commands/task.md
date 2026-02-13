---
name: task
description: Quick task management - create, claim, update, and list tasks
usage: |
  /task create <title> [description] [priority]
  /task claim <task_id>
  /task complete <task_id> [summary]
  /task list [status]
  /task context <task_id>
examples:
  - /task create "Implement user authentication" "Add JWT-based auth" high
  - /task claim task-a1b2c3d4
  - /task complete task-a1b2c3d4 "Auth implemented with bcrypt + JWT"
  - /task list pending
  - /task context task-a1b2c3d4
---

Quick task management interface for harness hive mind.

**Subcommands:**

- `create <title> [description] [priority]` - Create a new task
  - Priority: low, medium (default), high, critical

- `claim <task_id>` - Claim a pending task for yourself

- `complete <task_id> [summary]` - Mark task as completed with optional summary

- `list [status]` - List tasks, optionally filtered by status
  - Status: pending, claimed, in_progress, completed, failed

- `context <task_id>` - Get full context for a task including knowledge and subtasks

**Examples:**

```bash
# Create high-priority task
/task create "Fix database connection pool" "Pool exhaustion under load" high

# Claim and start work
/task claim task-a1b2c3d4

# Complete with summary
/task complete task-a1b2c3d4 "Increased pool size to 20, added monitoring"

# List all pending tasks
/task list pending
```

This command provides a streamlined interface to harness task tools.
