//! System prompt generation for spawned agents.

/// Generate a complete system prompt for a spawned agent.
///
/// Includes:
/// - Agent identity (agent_id, role)
/// - Auto-polling instructions
/// - Registration instructions
/// - Custom instructions from user
pub fn generate_agent_system_prompt(
    agent_id: &str,
    role: &str,
    custom_prompt: Option<&str>,
    poll_interval_secs: u64,
) -> String {
    let custom_section = custom_prompt
        .map(|p| format!("\n═══════════════════════════════════════════\nCustom Instructions\n═══════════════════════════════════════════\n{}\n", p))
        .unwrap_or_default();

    format!(
        r#"You are a {role} agent in the Harness hive mind.
Your agent_id is: {agent_id}

═══════════════════════════════════════════
CRITICAL: Auto-Polling Behavior (REQUIRED)
═══════════════════════════════════════════
Every {poll_interval_secs} seconds, you MUST:

1. Call mcp__harness__get_messages({{_agent_id: "{agent_id}"}})
   - Check for new direct messages from other agents
   - Respond immediately if action is needed

2. Call mcp__harness__get_hive_status({{_agent_id: "{agent_id}"}})
   - Check inbox for new task assignments
   - Monitor task_summary to see team progress
   - Review active_threads for conversations you're part of

When to poll MORE frequently:
- You're waiting for a response from another agent
- You're blocked on a dependency
- You're coordinating real-time work

When to poll LESS frequently:
- You're deep in implementation work
- No active conversations or blockers

═══════════════════════════════════════════
STARTUP PROCEDURE (Do this IMMEDIATELY!)
═══════════════════════════════════════════
You are ALREADY REGISTERED in the hive. Don't call register_agent!

IMMEDIATELY on startup:

1. Check for assigned tasks:
   mcp__harness__list_tasks({{_agent_id: "{agent_id}"}})

2. Look for tasks with assigned_to: "{agent_id}"

3. Claim and start working:
   mcp__harness__claim_task({{
     task_id: "...",
     _agent_id: "{agent_id}"
   }})

4. Update task status to "in_progress":
   mcp__harness__update_task_status({{
     task_id: "...",
     status: "in_progress",
     _agent_id: "{agent_id}"
   }})

5. Do the work and share discoveries:
   mcp__harness__share_knowledge({{
     content: "your findings",
     kind: "discovery",
     task_id: "...",
     _agent_id: "{agent_id}"
   }})
{custom_section}
═══════════════════════════════════════════
Remember: You are part of a coordinated team!
═══════════════════════════════════════════
- Share discoveries: mcp__harness__share_knowledge()
- Ask for help: mcp__harness__send_direct_message()
- Update task status: mcp__harness__update_task_status()
- Thread messages under tasks for context
"#,
        role = role,
        agent_id = agent_id,
        poll_interval_secs = poll_interval_secs,
        custom_section = custom_section,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_contains_agent_id() {
        let prompt = generate_agent_system_prompt("agent-123", "developer", None, 30);

        assert!(
            prompt.contains("agent-123"),
            "Prompt should contain agent ID"
        );
        assert!(
            prompt.contains("agent_id"),
            "Prompt should mention 'agent_id' field"
        );
    }

    #[test]
    fn test_prompt_contains_role() {
        let prompt = generate_agent_system_prompt("agent-123", "developer", None, 30);

        assert!(prompt.contains("developer"), "Prompt should contain role");
    }

    #[test]
    fn test_prompt_contains_auto_polling_instructions() {
        let prompt = generate_agent_system_prompt("agent-123", "developer", None, 45);

        // Should mention the polling interval
        assert!(prompt.contains("45"), "Should contain polling interval");

        // Should mention the tools to call
        assert!(
            prompt.contains("get_messages"),
            "Should mention get_messages tool"
        );
        assert!(
            prompt.contains("get_hive_status"),
            "Should mention get_hive_status tool"
        );

        // Should have clear instructions
        assert!(
            prompt.contains("Every") || prompt.contains("MUST"),
            "Should have clear polling requirement"
        );
    }

    #[test]
    fn test_prompt_contains_startup_instructions() {
        let prompt = generate_agent_system_prompt("agent-456", "tester", None, 30);

        assert!(
            prompt.contains("STARTUP") || prompt.contains("IMMEDIATELY"),
            "Should have clear startup instructions"
        );
        assert!(
            prompt.contains("ALREADY REGISTERED"),
            "Should clarify agent is pre-registered"
        );
        assert!(
            prompt.contains("list_tasks"),
            "Should mention checking for tasks"
        );
        assert!(
            prompt.contains("agent-456"),
            "Should include agent ID for tool calls"
        );
    }

    #[test]
    fn test_prompt_includes_custom_instructions() {
        let custom = "You are an expert Rust programmer specializing in async systems.";

        let prompt = generate_agent_system_prompt("agent-789", "developer", Some(custom), 30);

        assert!(
            prompt.contains(custom),
            "Should include custom instructions verbatim"
        );
    }

    #[test]
    fn test_prompt_without_custom_instructions() {
        let prompt = generate_agent_system_prompt("agent-999", "architect", None, 30);

        // Should still be valid prompt without custom instructions
        assert!(prompt.contains("agent-999"));
        assert!(prompt.contains("architect"));
        assert!(prompt.contains("register_agent"));
    }

    #[test]
    fn test_different_poll_intervals() {
        let prompt_30 = generate_agent_system_prompt("agent-1", "developer", None, 30);
        let prompt_60 = generate_agent_system_prompt("agent-2", "developer", None, 60);

        assert!(prompt_30.contains("30"));
        assert!(prompt_60.contains("60"));
        assert_ne!(
            prompt_30, prompt_60,
            "Different intervals should produce different prompts"
        );
    }

    #[test]
    fn test_prompt_structure_is_readable() {
        let prompt = generate_agent_system_prompt(
            "agent-readable",
            "developer",
            Some("Custom task instructions here."),
            30,
        );

        // Check for some structural elements that make it readable
        assert!(prompt.contains("\n"), "Should have line breaks");

        // Should have clear sections (we use box drawing characters ═══)
        let sections = prompt.split("═══").count();
        assert!(sections >= 3, "Should have at least 3 demarcated sections");
    }

    #[test]
    fn test_prompt_mentions_mcp_tools() {
        let prompt = generate_agent_system_prompt("agent-tools", "developer", None, 30);

        // Should mention MCP tool names with prefix
        assert!(
            prompt.contains("mcp__harness__"),
            "Should use MCP tool naming convention"
        );
        assert!(
            prompt.contains("_agent_id"),
            "Should mention _agent_id parameter"
        );
    }
}
