#!/bin/bash
# Auto-register as strategoi agent when session starts
# This ensures harness tools work immediately without manual registration
#
# IMPORTANT: Only auto-registers for the main/parent session.
# Spawned teammates should be registered by their parent with the appropriate role.

SETTINGS_FILE="${CLAUDE_PLUGIN_ROOT}/harness.local.md"

# Check if auto_register is enabled (default: true)
AUTO_REGISTER=true
if [ -f "$SETTINGS_FILE" ]; then
  AUTO_REGISTER=$(grep "auto_register:" "$SETTINGS_FILE" | awk '{print $2}' || echo "true")
fi

# Skip auto-registration if we're a spawned teammate
# Spawned agents have a team context or are launched with specific roles
if [ -n "$HARNESS_TEAM_NAME" ] || [ -n "$HARNESS_AGENT_ROLE" ]; then
  # This is a spawned teammate - parent will handle registration
  exit 0
fi

# Check if we're in a delegated/teammate mode (Claude Code specific)
if [ "$CLAUDE_MODE" = "delegate" ] || [ "$CLAUDE_MODE" = "teammate" ]; then
  # This is a spawned agent - skip auto-registration
  exit 0
fi

if [ "$AUTO_REGISTER" = "true" ]; then
  cat <<EOF
Registering as strategoi agent in harness hive...

You can now use harness tools for:
- Task tracking (create_task, list_tasks, claim_task, update_task_status)
- Knowledge sharing (share_knowledge, ask_hive)
- Agent coordination (register_agent, list_agents, send_direct_message)
- Product/Project/Plan management (create_product, create_project, create_plan)

Use /hive to see current status, or disable auto-registration in .claude/plugins/harness.local.md
EOF
fi
