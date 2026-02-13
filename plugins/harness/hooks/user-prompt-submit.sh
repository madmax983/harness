#!/bin/bash
# Parse user prompts and suggest task creation for trackable work
# Triggers on phrases like: "implement", "add", "create", "build", "fix", "refactor"

PROMPT="$1"
SETTINGS_FILE="${CLAUDE_PLUGIN_ROOT}/harness.local.md"

# Check if auto_parse_tasks is enabled (default: true)
AUTO_PARSE=true
if [ -f "$SETTINGS_FILE" ]; then
  AUTO_PARSE=$(grep "auto_parse_tasks:" "$SETTINGS_FILE" | awk '{print $2}' || echo "true")
fi

if [ "$AUTO_PARSE" != "true" ]; then
  exit 0
fi

# Detect task-worthy requests (case-insensitive)
if echo "$PROMPT" | grep -iE "^(implement|add|create|build|fix|refactor|write|update)" > /dev/null; then
  cat <<EOF
CLAUDE_PROMPT:
Before responding to the user's request, silently create a task using mcp__harness__create_task with:
- title: "${PROMPT:0:60}${PROMPT:60:+...}"
- description: Expand on what needs to be done based on the user's full request
- priority: Assess based on keywords (fix/critical = high, implement/add = medium, refactor = low)

After creating the task, immediately update its status to "in_progress" using mcp__harness__update_task_status since you'll be working on it now.

Do not announce or explain the task creation - just execute the tools and proceed with the user's request. The task tracking happens automatically in the background.
EOF
fi
