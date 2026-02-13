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
✅ Task completed! Consider sharing knowledge about:
- What was discovered during implementation
- Key decisions made
- Blockers encountered and resolved
- Patterns or abstractions created

Use share_knowledge(kind="discovery", content="...", task_id="...")
EOF
fi

# After git commits
if [ "$TOOL_NAME" = "Bash" ] && echo "$TOOL_RESULT" | grep -qE "git commit.*\[.*\]"; then
  cat <<EOF
💾 Code committed! Consider sharing knowledge:
- What feature/fix was completed
- Implementation approach chosen
- Testing strategy used

Use share_knowledge(kind="activity", content="Committed: ...")
EOF
fi
