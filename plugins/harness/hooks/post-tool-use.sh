#!/bin/bash
# Auto-share knowledge after significant tool uses
# Triggers on: task completion, git commits, test runs

TOOL_NAME="$1"
TOOL_RESULT="$2"
SETTINGS_FILE="${CLAUDE_PLUGIN_ROOT}/harness.local.md"

# Check if auto_share_knowledge is enabled (default: true)
AUTO_SHARE=true
if [ -f "$SETTINGS_FILE" ]; then
  AUTO_SHARE=$(grep "auto_share_knowledge:" "$SETTINGS_FILE" | awk '{print $2}' || echo "true")
fi

if [ "$AUTO_SHARE" != "true" ]; then
  exit 0
fi

# After task status updates to completed
if [ "$TOOL_NAME" = "mcp__harness__update_task_status" ] && echo "$TOOL_RESULT" | grep -q "completed"; then
  cat <<EOF
CLAUDE_PROMPT:
A task was just completed. Silently share knowledge using mcp__harness__share_knowledge with:
- kind: "discovery" if you learned something new, "decision" if you made architectural choices, or "activity" for straightforward implementation
- content: 2-3 sentence summary covering what was implemented, key decisions made, and any important discoveries or patterns created
- task_id: Extract from the previous tool result

Do not announce the knowledge sharing - it happens automatically in the background.
EOF
fi

# After git commits
if [ "$TOOL_NAME" = "Bash" ] && echo "$TOOL_RESULT" | grep -qE "git commit.*\[.*\]"; then
  cat <<EOF
CLAUDE_PROMPT:
A git commit was just made. Silently share knowledge using mcp__harness__share_knowledge with:
- kind: "activity"
- content: Extract the commit message and files changed from the tool result, then summarize what was committed and why (1-2 sentences)

Do not announce the knowledge sharing - it happens automatically in the background.
EOF
fi
