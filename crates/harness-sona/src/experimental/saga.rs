//! Saga Detector: Identify long-running workflow patterns from trajectory events.
//!
//! "Sagas" are sequences of events that represent a larger, coherent unit of work
//! or behavior (e.g., "Feature Implementation" = Design -> Code -> Test -> Deploy).
//!
//! This module allows defining expected sequences of events and detecting them
//! within a stream of noisy trajectory data. It helps in understanding agent
//! behavior at a higher level of abstraction than individual tasks.

use harness_persistence::{TrajectoryEvent, TriggerKind};
use serde::{Deserialize, Serialize};

/// Criteria for matching a single step in a Saga.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaStepCriteria {
    /// The kind of trigger to look for (e.g., TaskComplete).
    pub trigger_kind: TriggerKind,
    /// Whether the event must be successful to match.
    pub success_required: Option<bool>,
    /// Optional keyword that must appear in the summary.
    pub summary_contains: Option<String>,
}

impl SagaStepCriteria {
    pub fn new(trigger_kind: TriggerKind) -> Self {
        Self {
            trigger_kind,
            success_required: None,
            summary_contains: None,
        }
    }

    pub fn with_success(mut self, success: bool) -> Self {
        self.success_required = Some(success);
        self
    }

    pub fn with_summary(mut self, keyword: &str) -> Self {
        self.summary_contains = Some(keyword.to_string());
        self
    }

    /// Check if an event matches this criteria.
    pub fn matches(&self, event: &TrajectoryEvent) -> bool {
        if event.trigger_kind() != self.trigger_kind {
            return false;
        }
        if self.success_required.is_some_and(|req_success| event.success() != req_success) {
            return false;
        }
        if self.summary_contains.as_ref().is_some_and(|keyword| !event.summary().to_lowercase().contains(&keyword.to_lowercase())) {
            return false;
        }
        true
    }
}

/// Definition of a Saga pattern to detect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaDefinition {
    pub name: String,
    pub description: String,
    pub steps: Vec<SagaStepCriteria>,
}

impl SagaDefinition {
    pub fn new(name: &str, description: &str, steps: Vec<SagaStepCriteria>) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            steps,
        }
    }
}

/// A detected instance of a Saga.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaMatch {
    pub saga_name: String,
    /// The specific events that formed this saga, in order.
    pub events: Vec<TrajectoryEvent>,
}

/// The Saga Detector engine.
pub struct SagaDetector {
    definitions: Vec<SagaDefinition>,
}

impl SagaDetector {
    pub fn new() -> Self {
        Self {
            definitions: Vec::new(),
        }
    }

    pub fn register_saga(&mut self, definition: SagaDefinition) {
        self.definitions.push(definition);
    }

    /// Scan a stream of events for Saga patterns.
    ///
    /// Uses a greedy "eventually followed by" matching strategy.
    /// Overlapping matches are allowed (a single event can be part of multiple sagas).
    ///
    /// This implementation uses a simple state tracking approach:
    /// For each definition, we maintain a list of "active attempts".
    /// Each attempt tracks how many steps have been matched so far.
    pub fn detect(&self, events: &[TrajectoryEvent]) -> Vec<SagaMatch> {
        let mut matches = Vec::new();

        // Track active partial matches for each definition.
        // Outer Vec corresponds to self.definitions index.
        // Inner Vec contains (next_step_index, collected_events) for each active attempt.
        let mut active_states: Vec<Vec<(usize, Vec<TrajectoryEvent>)>> =
            vec![Vec::new(); self.definitions.len()];

        for event in events {
            for (def_idx, definition) in self.definitions.iter().enumerate() {
                // 1. Check if this event starts a new instance of this saga
                if let Some(first_step) = definition.steps.first()
                    && first_step.matches(event)
                {
                    if definition.steps.len() == 1 {
                        // Immediate match for single-step saga
                        matches.push(SagaMatch {
                            saga_name: definition.name.clone(),
                            events: vec![event.clone()],
                        });
                    } else {
                        // Start tracking a new attempt
                        active_states[def_idx].push((1, vec![event.clone()]));
                    }
                }

                // 2. Advance existing attempts
                // We need to be careful about mutating the vector while iterating.
                // We'll collect the new states and swap.
                let mut next_states = Vec::new();
                for (next_step_idx, mut collected_events) in active_states[def_idx].drain(..) {
                    let next_step = &definition.steps[next_step_idx];

                    if next_step.matches(event) {
                        collected_events.push(event.clone());
                        let new_idx = next_step_idx + 1;

                        if new_idx == definition.steps.len() {
                            // Full match!
                            matches.push(SagaMatch {
                                saga_name: definition.name.clone(),
                                events: collected_events,
                            });
                            // We don't continue this specific attempt after completion
                            // (unless we wanted to support cyclic sagas, which we don't yet).
                        } else {
                            // Advance to next step
                            next_states.push((new_idx, collected_events));
                        }
                    } else {
                        // Keep waiting for the next step (skip noise)
                        // "Eventually followed by" logic
                        next_states.push((next_step_idx, collected_events));
                    }
                }
                active_states[def_idx] = next_states;
            }
        }

        matches
    }
}

impl Default for SagaDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_persistence::{AgentId, TrajectoryEventId};

    // Helper to create a dummy event
    fn make_event(kind: TriggerKind, summary: &str, success: bool) -> TrajectoryEvent {
        // We can't construct TrajectoryEvent directly easily because fields are private or it requires complex setup.
        // However, we can use the `RawEvent` -> `materialize` path if we can access it,
        // OR we can rely on `TrajectoryEvent`'s public API if we had a builder.
        // Since `TrajectoryEvent` fields are private and no builder is public for *creating* arbitrary events
        // (only recording via `TrajectoryRecorder`), we might need to expose a test helper or
        // use `RawEvent`'s private `materialize` via a public wrapper if available.

        // Wait, I saw `RawEvent` has `materialize` but it is private to the crate or module?
        // Let's check `crates/harness-persistence/src/trajectory.rs`.
        // `pub struct RawEvent` is public. `materialize` is `fn materialize`, so private (module-level).
        // But `TrajectoryRecorder` is in the same module.
        // `TrajectoryEvent` struct definition is public. Fields are private.

        // Ah, I am writing code in `harness-sona`, which depends on `harness-persistence`.
        // I cannot access private fields/methods of `harness-persistence`.

        // Use `serde_json` to bypass constructor privacy if necessary for tests,
        // or check if there is a test helper in `harness-persistence`.
        // The `LearningTrigger` struct and `TrajectoryRecorder` are the intended way.
        // But `TrajectoryRecorder` requires a Repository.

        // HACK: Use `unsafe` or `std::mem::transmute`? No, that's bad.
        // Better HACK: Serialize/Deserialize. `TrajectoryEvent` derives Serialize/Deserialize.

        let id = TrajectoryEventId::new();
        let agent_id = AgentId::new();
        let now = Utc::now();

        // Construct a JSON representation matching the struct
        let json = serde_json::json!({
            "id": id,
            "trigger_kind": kind,
            "agent_id": agent_id,
            "task_id": null,
            "success": success,
            "summary": summary,
            "steps": [],
            "created_at": now
        });

        serde_json::from_value(json).expect("Failed to create test event via JSON")
    }

    #[test]
    fn test_simple_sequence() {
        let mut detector = SagaDetector::new();

        let def = SagaDefinition::new(
            "Feature Dev",
            "Design -> Code",
            vec![
                SagaStepCriteria::new(TriggerKind::TaskComplete).with_summary("Design"),
                SagaStepCriteria::new(TriggerKind::TaskComplete).with_summary("Code"),
            ],
        );
        detector.register_saga(def);

        let events = vec![
            make_event(TriggerKind::TaskComplete, "Design UI", true),
            make_event(TriggerKind::TaskComplete, "Code UI", true),
        ];

        let matches = detector.detect(&events);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].saga_name, "Feature Dev");
        assert_eq!(matches[0].events.len(), 2);
    }

    #[test]
    fn test_noisy_sequence() {
        let mut detector = SagaDetector::new();

        let def = SagaDefinition::new(
            "Consolidation",
            "Knowledge -> Project Close",
            vec![
                SagaStepCriteria::new(TriggerKind::KnowledgeShare),
                SagaStepCriteria::new(TriggerKind::ProjectClose),
            ],
        );
        detector.register_saga(def);

        let events = vec![
            make_event(TriggerKind::KnowledgeShare, "Learned X", true),
            make_event(TriggerKind::TaskComplete, "Noise 1", true),
            make_event(TriggerKind::TaskComplete, "Noise 2", false),
            make_event(TriggerKind::ProjectClose, "Close P1", true),
        ];

        let matches = detector.detect(&events);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].events[0].trigger_kind(),
            TriggerKind::KnowledgeShare
        );
        assert_eq!(
            matches[0].events[1].trigger_kind(),
            TriggerKind::ProjectClose
        );
    }

    #[test]
    fn test_interleaved_sagas() {
        let mut detector = SagaDetector::new();

        let def = SagaDefinition::new(
            "Short",
            "A -> B",
            vec![
                SagaStepCriteria::new(TriggerKind::TaskComplete).with_summary("A"),
                SagaStepCriteria::new(TriggerKind::TaskComplete).with_summary("B"),
            ],
        );
        detector.register_saga(def);

        // Sequence: A1, A2, B1, B2
        // Should find two matches: (A1, B1) and (A2, B2) ... OR (A1, B2)?
        // The current "greedy eventually" logic might be tricky.
        // Let's trace:
        // Evt A1: Starts Attempt 1 (waiting for B)
        // Evt A2: Starts Attempt 2 (waiting for B). Attempt 1 still waiting for B (ignores A).
        // Evt B1: Attempt 1 sees B -> Match! Attempt 2 sees B -> Match!
        // Wait, if B1 matches both, we get two matches ending at B1?
        // Yes, because B1 satisfies "eventually B" for both A1 and A2.

        let events = vec![
            make_event(TriggerKind::TaskComplete, "A", true), // 0
            make_event(TriggerKind::TaskComplete, "A", true), // 1
            make_event(TriggerKind::TaskComplete, "B", true), // 2
        ];

        let matches = detector.detect(&events);
        assert_eq!(matches.len(), 2);
        // Both matches end at event 2.
    }
}
