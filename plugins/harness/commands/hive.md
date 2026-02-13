---
name: hive
description: Show harness hive status - active agents, task summary, and recent knowledge
usage: /hive [--verbose]
examples:
  - /hive
  - /hive --verbose
---

Display the current harness hive mind status including:
- Active agents and their roles
- Task summary (pending, in_progress, completed counts)
- Recent knowledge shared by the hive
- Current session information

Use `--verbose` flag to see full task list and agent details.

This command calls `get_hive_status` and formats the output for easy reading.
