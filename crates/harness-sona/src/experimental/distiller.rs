//! Distiller: Export hive mind trajectories for local LLM fine-tuning.
//!
//! Converts internal `TrajectoryEvent` streams into standard formats (JSONL)
//! structured for supervised fine-tuning (SFT) of open-source models (like Llama 3).
//! This allows the hive mind to distill its learned experiences into a smaller,
//! faster local model.

use harness_persistence::TrajectoryEvent;
use serde::Serialize;
use std::io::Write;

/// A trait for distilling trajectory events into training datasets.
pub trait TrajectoryDistiller {
    /// Distill a sequence of events into a target writer.
    fn distill<W: Write>(&self, events: &[TrajectoryEvent], writer: &mut W) -> anyhow::Result<()>;
}

/// Distills trajectories into the standard ShareGPT / OpenAI chat format (JSONL).
pub struct ChatFormatDistiller {
    system_prompt: String,
}

impl ChatFormatDistiller {
    pub fn new(system_prompt: &str) -> Self {
        Self {
            system_prompt: system_prompt.to_string(),
        }
    }
}

impl Default for ChatFormatDistiller {
    fn default() -> Self {
        Self::new(
            "You are a distilled hive mind agent. Your goal is to replicate the successful patterns of the collective.",
        )
    }
}

impl TrajectoryDistiller for ChatFormatDistiller {
    fn distill<W: Write>(&self, events: &[TrajectoryEvent], writer: &mut W) -> anyhow::Result<()> {
        #[derive(Serialize)]
        struct Message {
            role: &'static str,
            content: String,
        }

        #[derive(Serialize)]
        struct ChatCompletion {
            messages: Vec<Message>,
        }

        for event in events {
            // Only distill successful events (we want the model to imitate success)
            if !event.success() {
                continue;
            }

            let mut messages = vec![Message {
                role: "system",
                content: self.system_prompt.clone(),
            }];

            // User prompt: What triggered the action?
            let user_content = format!(
                "Trigger: {:?}\nContext: Can you execute this task successfully?",
                event.trigger_kind()
            );
            messages.push(Message {
                role: "user",
                content: user_content,
            });

            // Assistant response: The summary and steps taken
            let mut assistant_content = format!("Outcome: {}\nSteps:\n", event.summary());
            for (i, step) in event.steps().iter().enumerate() {
                assistant_content.push_str(&format!(
                    "{}. [{}] {:?}\n",
                    i + 1,
                    step.kind(),
                    step.payload()
                ));
            }
            messages.push(Message {
                role: "assistant",
                content: assistant_content,
            });

            let completion = ChatCompletion { messages };
            let json = serde_json::to_string(&completion)?;
            writeln!(writer, "{}", json)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_persistence::{AgentId, TrajectoryEventId, TriggerKind};
    use serde_json::json;

    // Helper to create synthetic events
    fn make_event(kind: TriggerKind, summary: &str, success: bool) -> TrajectoryEvent {
        let json_val = json!({
            "id": TrajectoryEventId::new(),
            "trigger_kind": kind,
            "agent_id": AgentId::new(),
            "task_id": null,
            "success": success,
            "summary": summary,
            "steps": [
                {
                    "kind": "task_outcome",
                    "agent_id": AgentId::new(),
                    "payload": { "key": "value" }
                }
            ],
            "created_at": Utc::now()
        });

        serde_json::from_value(json_val).unwrap()
    }

    #[test]
    fn test_chat_format_distiller() {
        let distiller = ChatFormatDistiller::default();
        let events = vec![
            make_event(
                TriggerKind::TaskComplete,
                "Refactored module successfully",
                true,
            ),
            make_event(TriggerKind::TaskComplete, "Failed to compile", false), // Should be skipped
        ];

        let mut output = Vec::new();
        distiller.distill(&events, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();

        // Should contain one line (successful event)
        assert_eq!(output_str.lines().count(), 1);

        assert!(output_str.contains("Refactored module successfully"));
        assert!(output_str.contains("system"));
        assert!(output_str.contains("user"));
        assert!(output_str.contains("assistant"));

        // Should NOT contain the failed event
        assert!(!output_str.contains("Failed to compile"));
    }
}
