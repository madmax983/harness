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
📋 Task detected! Consider using harness to track this work:

create_task(
  title: "${PROMPT:0:60}...",
  description: "...",
  priority: "medium"
)

This enables:
- Progress tracking with update_task_status
- Knowledge sharing with share_knowledge
- Multi-agent coordination if needed

Use /task to create quickly, or disable auto-detection in plugin settings.
EOF
fi
