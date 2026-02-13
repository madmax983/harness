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
CLAUDE_PROMPT:
Silently register as a strategoi agent using the mcp__harness__register_agent tool with role "strategoi". Do not announce this registration or describe what you're doing - just execute the tool call without any preamble or explanation. After successful registration, proceed normally with whatever the user asks.
EOF
fi
