//! Saga Detector: Recognizes long-term workflow patterns (Sagas) from trajectory events.
//!
//! A "Saga" is a high-level sequence of operations that represents a coherent
//! unit of work, such as "Feature Implementation", "Bug Fix", or "Refactoring".
//!
//! The Saga Detector analyzes a stream of `TrajectoryEvent`s to identify these
//! patterns, potentially predicting the next likely step or detecting deviations.

use harness_persistence::{TrajectoryEvent, TriggerKind};
use serde::{Deserialize, Serialize};

/// A single step within a Saga definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaStep {
    /// The expected trigger kind (e.g., TaskComplete).
    pub kind: TriggerKind,
    /// Optional keywords that must appear in the event summary (case-insensitive).
    pub keywords: Vec<String>,
    /// Optional role that must have performed the action (not implemented for MVP).
    pub role: Option<String>,
}

impl SagaStep {
    /// Create a new step requirement.
    pub fn new(kind: TriggerKind) -> Self {
        Self {
            kind,
            keywords: Vec::new(),
            role: None,
        }
    }

    /// Add a keyword requirement.
    pub fn with_keyword(mut self, keyword: &str) -> Self {
        self.keywords.push(keyword.to_lowercase());
        self
    }

    /// Check if an event matches this step.
    pub fn matches(&self, event: &TrajectoryEvent) -> bool {
        if event.trigger_kind() != self.kind {
            return false;
        }

        if !self.keywords.is_empty() {
            let summary = event.summary().to_lowercase();
            // Check if ANY keyword matches (or ALL? let's say ALL for specificity, but maybe ANY is more flexible)
            // Let's go with ALL for now to be precise.
            for keyword in &self.keywords {
                if !summary.contains(keyword) {
                    return false;
                }
            }
        }

        true
    }
}

/// A definition of a Saga (workflow pattern).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saga {
    /// Unique name of the saga (e.g., "Feature Implementation").
    pub name: String,
    /// The sequence of steps that constitute this saga.
    pub steps: Vec<SagaStep>,
    /// Description of what this saga represents.
    pub description: String,
}

impl Saga {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            steps: Vec::new(),
            description: description.to_string(),
        }
    }

    pub fn add_step(mut self, step: SagaStep) -> Self {
        self.steps.push(step);
        self
    }
}

/// Result of a saga detection attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaMatch {
    /// The name of the matched saga.
    pub saga_name: String,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,
    /// The index in the event stream where the match ends.
    pub end_index: usize,
    /// How many steps were matched.
    pub steps_matched: usize,
    /// Total steps in the saga.
    pub total_steps: usize,
}

/// The Saga Detector service.
pub struct SagaDetector {
    sagas: Vec<Saga>,
}

impl SagaDetector {
    /// Create a new detector with a default set of known sagas.
    pub fn new() -> Self {
        let mut sagas = Vec::new();

        // 1. Feature Implementation Saga
        // Pattern: Design -> Implement -> Test -> Release (Project Close)
        sagas.push(
            Saga::new(
                "Feature Implementation",
                "Standard workflow for adding a new feature.",
            )
            .add_step(
                SagaStep::new(TriggerKind::TaskComplete).with_keyword("design"),
            )
            .add_step(
                SagaStep::new(TriggerKind::TaskComplete).with_keyword("implement"),
            )
            .add_step(SagaStep::new(TriggerKind::TaskComplete).with_keyword("test"))
            .add_step(SagaStep::new(TriggerKind::ProjectClose)),
        );

        // 2. Bug Fix Saga
        // Pattern: Reproduce -> Fix -> Verify
        sagas.push(
            Saga::new("Bug Fix", "Workflow for resolving a reported bug.")
                .add_step(
                    SagaStep::new(TriggerKind::TaskComplete).with_keyword("reproduce"),
                )
                .add_step(
                    SagaStep::new(TriggerKind::TaskComplete).with_keyword("fix"),
                )
                .add_step(
                    SagaStep::new(TriggerKind::TaskComplete).with_keyword("verify"),
                ),
        );

        // 3. Knowledge Discovery Saga
        // Pattern: Research -> Share -> Document
        sagas.push(
            Saga::new(
                "Knowledge Discovery",
                "Process of researching and sharing new findings.",
            )
            .add_step(
                SagaStep::new(TriggerKind::TaskComplete).with_keyword("research"),
            )
            .add_step(SagaStep::new(TriggerKind::KnowledgeShare))
            .add_step(
                SagaStep::new(TriggerKind::TaskComplete).with_keyword("document"),
            ),
        );

        Self { sagas }
    }

    /// Add a custom saga to the detector.
    pub fn with_saga(mut self, saga: Saga) -> Self {
        self.sagas.push(saga);
        self
    }

    /// Detect active or completed sagas in a stream of events.
    ///
    /// This uses a sliding window approach to find the best match ending at the
    /// most recent event. It prefers longer matches.
    pub fn detect(&self, events: &[TrajectoryEvent]) -> Option<SagaMatch> {
        if events.is_empty() {
            return None;
        }

        let mut best_match: Option<SagaMatch> = None;

        // Try to match each saga ending at the last event
        for saga in &self.sagas {
            // Check if the sequence ends with a partial or full match of this saga
            // We iterate backwards from the last step of the saga

            // Optimization: Only check if the last event matches ANY step in the saga?
            // No, we want to find the longest suffix of the event stream that matches
            // a prefix of a saga (for prediction) or a full saga.

            // Let's simplify: strict sequence matching.
            // We look for the saga steps appearing contiguously at the end of events.

            // Check for full match ending at `events.len() - 1`
            if events.len() >= saga.steps.len() {
                let start_idx = events.len() - saga.steps.len();
                let window = &events[start_idx..];

                let mut full_match = true;
                for (i, step) in saga.steps.iter().enumerate() {
                    if !step.matches(&window[i]) {
                        full_match = false;
                        break;
                    }
                }

                if full_match {
                    let m = SagaMatch {
                        saga_name: saga.name.clone(),
                        confidence: 1.0,
                        end_index: events.len() - 1,
                        steps_matched: saga.steps.len(),
                        total_steps: saga.steps.len(),
                    };

                    // Prefer longer matches or full matches
                    if best_match.as_ref().map_or(true, |bm| m.steps_matched > bm.steps_matched) {
                        best_match = Some(m);
                    }
                }
            }

            // Check for partial match (Saga in progress)
            // The end of `events` matches the beginning of `saga`
            // Try to match the first K steps of the saga to the last K events
            for k in (1..saga.steps.len()).rev() {
                if events.len() >= k {
                     let start_idx = events.len() - k;
                     let window = &events[start_idx..];

                     let mut partial_match = true;
                     for (i, step) in saga.steps.iter().take(k).enumerate() {
                         if !step.matches(&window[i]) {
                             partial_match = false;
                             break;
                         }
                     }

                     if partial_match {
                        let m = SagaMatch {
                            saga_name: saga.name.clone(),
                            confidence: k as f32 / saga.steps.len() as f32, // Simple confidence
                            end_index: events.len() - 1,
                            steps_matched: k,
                            total_steps: saga.steps.len(),
                        };

                        // If we already have a full match, ignore partials unless we want to predict?
                        // Let's prefer full matches. If no full match, take the longest partial.
                         if best_match.as_ref().map_or(true, |bm| {
                             // Prefer full matches (confidence 1.0) over partials
                             if bm.confidence == 1.0 && m.confidence < 1.0 {
                                 return false;
                             }
                             // Otherwise prefer longer partial matches
                             m.steps_matched > bm.steps_matched
                         }) {
                             best_match = Some(m);
                         }
                         break; // Found longest partial match for this saga
                     }
                }
            }
        }

        best_match
    }

    /// Predict the next likely step based on the detected saga.
    pub fn predict_next_step(&self, match_result: &SagaMatch) -> Option<&SagaStep> {
        if match_result.steps_matched < match_result.total_steps {
            // Find the saga
            if let Some(saga) = self.sagas.iter().find(|s| s.name == match_result.saga_name) {
                return saga.steps.get(match_result.steps_matched);
            }
        }
        None
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
    use harness_persistence::{AgentId, TrajectoryEventId};
    use chrono::Utc;

    // Helper to create a dummy event
    fn create_event(kind: TriggerKind, summary: &str) -> TrajectoryEvent {
        // We can't easily create a TrajectoryEvent directly because fields are private
        // and we don't have a public constructor that sets everything easily without
        // going through RawEvent -> materialize (which is private/internal in the crate).

        // Wait, TrajectoryEvent fields are private but we are in a different crate (harness-sona),
        // so we can't access private fields.
        // But `RawEvent` is public in `harness_persistence`.
        // And `RawEvent::materialize` is private.

        // However, `TrajectoryRecorder` returns `TrajectoryEvent`.
        // Ideally we should mock it or use a public constructor if available.
        // `TrajectoryEvent` has no public constructor.

        // WORKAROUND: We can't create `TrajectoryEvent` directly from here if we can't instantiate it.
        // Let's check `harness_persistence` again.
        // `TrajectoryEvent` struct fields are private.
        // There is no `new` method.

        // But we are `Nova`. We can modify `harness-persistence` if needed to add a test helper?
        // "Never do: Delete or modify existing core logic". Adding a test helper is fine?
        // Or maybe I can use `LearningTrigger` and some public API?

        // `TrajectoryRecorder::record` returns an ID. `get_event` returns the event.
        // That requires a Repository.

        // Let's see if I can add a `#[cfg(test)]` constructor or `pub` constructor to `TrajectoryEvent`
        // in `harness-persistence`?
        // Or maybe `TrajectoryEvent` has a `new`? No.

        // Wait, `RawEvent` is public. But `materialize` is private.
        // `TrajectoryRecorder` uses `materialize`.

        // Maybe I can rely on serialization?
        // `TrajectoryEvent` derives `Deserialize`.
        // I can construct a JSON string and deserialize it!

        let json = serde_json::json!({
            "id": TrajectoryEventId::new(),
            "trigger_kind": kind,
            "agent_id": AgentId::new(),
            "task_id": null,
            "success": true,
            "summary": summary,
            "steps": [],
            "created_at": Utc::now()
        });

        serde_json::from_value(json).expect("Failed to deserialize mock event")
    }

    #[test]
    fn test_feature_implementation_saga() {
        let detector = SagaDetector::new();

        let events = vec![
            create_event(TriggerKind::TaskComplete, "Designed the user interface"),
            create_event(TriggerKind::TaskComplete, "Implemented the backend logic"),
            create_event(TriggerKind::TaskComplete, "Ran tests and verified output"),
            create_event(TriggerKind::ProjectClose, "Released version 1.0"),
        ];

        let result = detector.detect(&events).expect("Should detect saga");

        assert_eq!(result.saga_name, "Feature Implementation");
        assert_eq!(result.confidence, 1.0);
        assert_eq!(result.steps_matched, 4);
    }

    #[test]
    fn test_partial_match_prediction() {
        let detector = SagaDetector::new();

        let events = vec![
            create_event(TriggerKind::TaskComplete, "Designed the architecture"),
            create_event(TriggerKind::TaskComplete, "Implemented core module"),
        ];

        let result = detector.detect(&events).expect("Should detect partial saga");

        assert_eq!(result.saga_name, "Feature Implementation");
        assert_eq!(result.steps_matched, 2);
        assert!(result.confidence < 1.0);

        let next_step = detector.predict_next_step(&result).expect("Should predict next step");
        assert_eq!(next_step.kind, TriggerKind::TaskComplete);
        assert!(next_step.keywords.contains(&"test".to_string()));
    }

    #[test]
    fn test_bug_fix_saga() {
        let detector = SagaDetector::new();

        let events = vec![
            create_event(TriggerKind::TaskComplete, "Reproduced the crash"),
            create_event(TriggerKind::TaskComplete, "Fix null pointer exception"),
        ];

        let result = detector.detect(&events).expect("Should detect bug fix saga");

        assert_eq!(result.saga_name, "Bug Fix");

        let next_step = detector.predict_next_step(&result).expect("Should predict verify");
        assert!(next_step.keywords.contains(&"verify".to_string()));
    }

    #[test]
    fn test_no_match() {
        let detector = SagaDetector::new();

        let events = vec![
            create_event(TriggerKind::TaskComplete, "Just doing random stuff"),
            create_event(TriggerKind::KnowledgeShare, "Sharing a meme"),
        ];

        let result = detector.detect(&events);
        assert!(result.is_none());
    }
}
