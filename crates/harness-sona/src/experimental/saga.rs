//! Saga Detector: Identifies multi-step behavioral patterns in agent trajectories.
//!
//! A "Saga" is a high-level sequence of events that represents a coherent workflow or strategy,
//! such as "Red-Green-Refactor" (TDD) or "Research-Plan-Execute".
//!
//! The detector monitors the stream of `TrajectoryEvent`s and maintains state for potential
//! saga matches per agent. It is resilient to "noise" (interleaved unrelated events).

use std::collections::HashMap;

use harness_persistence::{AgentId, TrajectoryEvent, TriggerKind};
use serde::{Deserialize, Serialize};

/// Criteria for matching a single step in a saga.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaStepCriteria {
    /// The kind of trigger to match (e.g., TaskComplete).
    pub trigger_kind: TriggerKind,
    /// Optional keyword that must appear in the summary (case-insensitive).
    pub summary_contains: Option<String>,
}

impl SagaStepCriteria {
    /// Create a new step criteria.
    pub fn new(trigger_kind: TriggerKind) -> Self {
        Self {
            trigger_kind,
            summary_contains: None,
        }
    }

    /// Add a keyword filter.
    pub fn with_keyword(mut self, keyword: &str) -> Self {
        self.summary_contains = Some(keyword.to_lowercase());
        self
    }

    /// Check if an event matches this criteria.
    pub fn matches(&self, event: &TrajectoryEvent) -> bool {
        if event.trigger_kind() != self.trigger_kind {
            return false;
        }

        if let Some(ref keyword) = self.summary_contains
            && !event.summary().to_lowercase().contains(keyword)
        {
            return false;
        }

        true
    }
}

/// Definition of a Saga (a named sequence of steps).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaDefinition {
    /// Unique name of the saga (e.g., "TDD Cycle").
    pub name: String,
    /// The sequence of steps that define this saga.
    pub steps: Vec<SagaStepCriteria>,
}

impl SagaDefinition {
    pub fn new(name: &str, steps: Vec<SagaStepCriteria>) -> Self {
        Self {
            name: name.to_string(),
            steps,
        }
    }
}

/// A detected instance of a completed saga.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaMatch {
    /// The name of the matched saga.
    pub saga_name: String,
    /// The agent who performed the saga.
    pub agent_id: AgentId,
    /// The events that constituted the saga.
    pub events: Vec<TrajectoryEvent>,
}

/// Internal state for tracking a specific saga definition for a specific agent.
struct SagaTracker {
    definition_name: String,
    steps: Vec<SagaStepCriteria>,
    /// Index of the next expected step.
    next_step_index: usize,
    /// Accumulated events for the current match attempt.
    matched_events: Vec<TrajectoryEvent>,
}

impl SagaTracker {
    fn new(definition: &SagaDefinition) -> Self {
        Self {
            definition_name: definition.name.clone(),
            steps: definition.steps.clone(),
            next_step_index: 0,
            matched_events: Vec::new(),
        }
    }

    /// Process an event. Returns Some(SagaMatch) if the saga is completed.
    /// Returns None if the event advanced the saga or was ignored (noise).
    /// Resets state if the event breaks the saga (implementation detail: currently we are loose and allow noise).
    fn process(&mut self, event: &TrajectoryEvent) -> Option<SagaMatch> {
        if self.next_step_index >= self.steps.len() {
            return None; // Should not happen if logic is correct
        }

        let current_step = &self.steps[self.next_step_index];

        if current_step.matches(event) {
            // Match found! Advance state.
            self.matched_events.push(event.clone());
            self.next_step_index += 1;

            // Check for completion
            if self.next_step_index == self.steps.len() {
                let match_result = SagaMatch {
                    saga_name: self.definition_name.clone(),
                    agent_id: event.agent_id(),
                    events: self.matched_events.clone(),
                };
                // Reset for next detection
                self.reset();
                return Some(match_result);
            }
        }

        // If no match, we treat it as noise and ignore it (allow interleaved events).
        // A stricter implementation might reset on incompatible events, but for
        // behavioral analysis, "eventually followed by" is usually more robust.

        None
    }

    fn reset(&mut self) {
        self.next_step_index = 0;
        self.matched_events.clear();
    }
}

/// The Saga Detector engine.
pub struct SagaDetector {
    definitions: Vec<SagaDefinition>,
    /// Map of AgentId -> List of active trackers (one per definition).
    trackers: HashMap<AgentId, Vec<SagaTracker>>,
}

impl SagaDetector {
    /// Create a new SagaDetector with the given definitions.
    pub fn new(definitions: Vec<SagaDefinition>) -> Self {
        Self {
            definitions,
            trackers: HashMap::new(),
        }
    }

    /// Register a new saga definition.
    pub fn add_definition(&mut self, definition: SagaDefinition) {
        self.definitions.push(definition);
        // Clear trackers to force rebuild on next event (simplest way to sync)
        self.trackers.clear();
    }

    /// Process a new trajectory event and return any detected sagas.
    pub fn process_event(&mut self, event: &TrajectoryEvent) -> Vec<SagaMatch> {
        let agent_id = event.agent_id();
        let trackers = self.trackers.entry(agent_id).or_insert_with(|| {
            self.definitions
                .iter()
                .map(SagaTracker::new)
                .collect()
        });

        let mut matches = Vec::new();
        for tracker in trackers {
            if let Some(saga_match) = tracker.process(event) {
                matches.push(saga_match);
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_persistence::{TaskId, TrajectoryEventId};
    use chrono::Utc;

    // Helper to create a dummy event
    fn create_event(
        trigger_kind: TriggerKind,
        summary: &str,
        agent_id: AgentId,
    ) -> TrajectoryEvent {
        // We have to use a hack to create TrajectoryEvent since fields are private and no public constructor for testing.
        // But wait, `RawEvent` is public in `harness-persistence` and has a `materialize` method which is private.
        // Actually, `TrajectoryEvent` fields are private.
        // We might need to use `RawEvent` and serialize/deserialize or use the `harness_persistence::test_helpers` if they existed.

        // Looking at `harness-persistence/src/trajectory.rs`, `TrajectoryEvent` has private fields.
        // But `RawEvent` is public. And `RawEvent::materialize` is private.
        // However, `TrajectoryRecorder::record` returns an ID, and `get_event` returns `TrajectoryEvent`.
        // We can't easily instantiate `TrajectoryEvent` directly here without a repository.

        // Wait, `TrajectoryEvent` derives `Deserialize`. We can construct it via JSON.
        let json = serde_json::json!({
            "id": TrajectoryEventId::new(),
            "trigger_kind": trigger_kind,
            "agent_id": agent_id,
            "task_id": Some(TaskId::new()),
            "success": true,
            "summary": summary,
            "steps": [],
            "created_at": Utc::now()
        });

        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn test_tdd_saga_detection() {
        let agent_id = AgentId::new();

        // Define TDD Saga: Red (fail) -> Green (pass) -> Refactor
        // For simplicity, let's just use keywords in summary since we can't easily toggle success in this test helper without more JSON work.
        let tdd_def = SagaDefinition::new(
            "TDD Cycle",
            vec![
                SagaStepCriteria::new(TriggerKind::TaskComplete).with_keyword("fail"),
                SagaStepCriteria::new(TriggerKind::TaskComplete).with_keyword("pass"),
                SagaStepCriteria::new(TriggerKind::TaskComplete).with_keyword("refactor"),
            ],
        );

        let mut detector = SagaDetector::new(vec![tdd_def]);

        // Sequence:
        // 1. Fail test (Red)
        // 2. Pass test (Green)
        // 3. Refactor (Refactor) -> MATCH!

        let e1 = create_event(TriggerKind::TaskComplete, "wrote failing test", agent_id);
        let matches1 = detector.process_event(&e1);
        assert!(matches1.is_empty());

        let e2 = create_event(TriggerKind::TaskComplete, "made test pass", agent_id);
        let matches2 = detector.process_event(&e2);
        assert!(matches2.is_empty());

        let e3 = create_event(TriggerKind::TaskComplete, "refactor code", agent_id);
        let matches3 = detector.process_event(&e3);

        assert_eq!(matches3.len(), 1);
        assert_eq!(matches3[0].saga_name, "TDD Cycle");
        assert_eq!(matches3[0].events.len(), 3);
    }

    #[test]
    fn test_interleaved_noise() {
        let agent_id = AgentId::new();
        let simple_def = SagaDefinition::new(
            "Simple",
            vec![
                SagaStepCriteria::new(TriggerKind::TaskComplete).with_keyword("start"),
                SagaStepCriteria::new(TriggerKind::TaskComplete).with_keyword("end"),
            ],
        );

        let mut detector = SagaDetector::new(vec![simple_def]);

        // Sequence: Start -> Noise -> End -> MATCH
        let e1 = create_event(TriggerKind::TaskComplete, "start task", agent_id);
        detector.process_event(&e1);

        let noise = create_event(TriggerKind::TaskComplete, "random noise", agent_id);
        let noise_matches = detector.process_event(&noise);
        assert!(noise_matches.is_empty());

        let e2 = create_event(TriggerKind::TaskComplete, "end task", agent_id);
        let matches = detector.process_event(&e2);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].events.len(), 2);
        // Ensure the noise event is NOT in the saga events
        assert_eq!(matches[0].events[0].summary(), "start task");
        assert_eq!(matches[0].events[1].summary(), "end task");
    }
}
