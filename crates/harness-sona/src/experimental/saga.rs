//! Saga: Detects recurring sequences of tasks (sagas) in trajectory history.
//!
//! A "Saga" is a pattern of tasks that frequently occur together in a specific order.
//! For example: "Write Test" -> "Run Test" -> "Refactor" -> "Run Test".
//!
//! This module analyzes TrajectoryEvents to find these patterns and predict the next
//! likely task in a workflow.

use std::collections::HashMap;
use harness_persistence::{TrajectoryEvent, TriggerKind};
use serde::{Deserialize, Serialize};

/// A detected sequence of task summaries that frequently appear together.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Saga {
    /// The sequence of task summaries/types.
    pub steps: Vec<String>,
    /// How many times this sequence was observed.
    pub frequency: usize,
    /// Confidence score based on frequency (0.0 to 1.0).
    pub confidence: f32,
}

/// Detects sagas and predicts next steps.
#[derive(Debug, Clone)]
pub struct SagaDetector {
    /// Minimum frequency for a sequence to be considered a saga.
    min_frequency: usize,
    /// Length of sequence to detect (N-gram size). Default is 2 (bigrams).
    ngram_size: usize,
}

impl Default for SagaDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl SagaDetector {
    /// Create a new SagaDetector with default settings (min_freq=2, bigrams).
    pub fn new() -> Self {
        Self {
            min_frequency: 2,
            ngram_size: 2,
        }
    }

    /// Set minimum frequency threshold.
    pub fn with_min_frequency(mut self, freq: usize) -> Self {
        self.min_frequency = freq;
        self
    }

    /// Set N-gram size (sequence length).
    pub fn with_ngram_size(mut self, size: usize) -> Self {
        self.ngram_size = size;
        self
    }

    /// Detect sagas from a list of trajectory events.
    ///
    /// Analyzes the sequence of `TaskComplete` events to find repeating patterns.
    pub fn detect(&self, events: &[TrajectoryEvent]) -> Vec<Saga> {
        // Collect and sort events by time
        let mut sorted_events: Vec<&TrajectoryEvent> = events.iter().collect();
        sorted_events.sort_by_key(|e| e.created_at());

        // Extract summaries from TaskComplete events
        let summaries: Vec<String> = sorted_events
            .into_iter()
            .filter(|e| e.trigger_kind() == TriggerKind::TaskComplete)
            .map(|e| e.summary().to_string())
            .collect();

        self.detect_from_summaries(&summaries)
    }

    /// Detect sagas from a list of ordered summaries (public for testing/direct usage).
    pub fn detect_from_summaries(&self, summaries: &[String]) -> Vec<Saga> {
        if summaries.len() < self.ngram_size {
            return Vec::new();
        }

        let mut counts: HashMap<Vec<String>, usize> = HashMap::new();

        // Detect sequences of length `ngram_size`
        for window in summaries.windows(self.ngram_size) {
            let key = window.to_vec();
            *counts.entry(key).or_insert(0) += 1;
        }

        // Convert to Saga structs
        let mut results: Vec<Saga> = counts
            .into_iter()
            .filter(|(_, count)| *count >= self.min_frequency)
            .map(|(steps, count)| {
                // Confidence score: higher frequency -> higher confidence (asymptotic to 1.0)
                let confidence = 1.0 - (1.0 / (count as f32 + 1.0));

                Saga {
                    steps,
                    frequency: count,
                    confidence,
                }
            })
            .collect();

        // Sort by confidence descending
        results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        results
    }

    /// Predict the next likely task summary based on the history.
    ///
    /// Given a history of tasks (e.g., ["A"]), finds detected sagas starting with "A"
    /// (e.g., ["A", "B"]) and returns "B".
    ///
    /// Uses the last `ngram_size - 1` items from history as context.
    pub fn predict_next(&self, history: &[String], detected_sagas: &[Saga]) -> Option<String> {
        if history.is_empty() {
            return None;
        }

        // We look at the last `ngram_size - 1` items in history to match the start of a saga.
        let context_len = self.ngram_size.saturating_sub(1);
        if context_len == 0 {
             return None; // Cannot predict from nothing if size is 1 (which doesn't make sense for sequence)
        }

        if history.len() < context_len {
            return None;
        }

        let context = &history[history.len() - context_len..];

        // Find sagas where the first `context_len` steps match the context
        detected_sagas
            .iter()
            .filter(|s| s.steps.len() == self.ngram_size && s.steps.starts_with(context))
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
            .map(|s| s.steps[context_len].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_simple_repetition() {
        let detector = SagaDetector::new().with_min_frequency(2).with_ngram_size(2);

        // Sequence: A -> B, A -> B, C -> D
        let summaries = vec![
            "Task A".to_string(),
            "Task B".to_string(),
            "Task A".to_string(),
            "Task B".to_string(),
            "Task C".to_string(),
            "Task D".to_string(),
        ];

        let sagas = detector.detect_from_summaries(&summaries);

        // Should detect ["Task A", "Task B"] with frequency 2
        assert_eq!(sagas.len(), 1);
        assert_eq!(sagas[0].steps, vec!["Task A", "Task B"]);
        assert_eq!(sagas[0].frequency, 2);
    }

    #[test]
    fn test_predict_next() {
        let detector = SagaDetector::new().with_min_frequency(2).with_ngram_size(2);

        // Saga: A -> B
        let detected_sagas = vec![
            Saga {
                steps: vec!["Task A".to_string(), "Task B".to_string()],
                frequency: 5,
                confidence: 0.9,
            }
        ];

        let history = vec!["Task C".to_string(), "Task A".to_string()];

        let prediction = detector.predict_next(&history, &detected_sagas);

        assert_eq!(prediction, Some("Task B".to_string()));
    }

    #[test]
    fn test_predict_no_match() {
        let detector = SagaDetector::new().with_min_frequency(2).with_ngram_size(2);

        let detected_sagas = vec![
            Saga {
                steps: vec!["Task A".to_string(), "Task B".to_string()],
                frequency: 5,
                confidence: 0.9,
            }
        ];

        let history = vec!["Task X".to_string()];

        let prediction = detector.predict_next(&history, &detected_sagas);

        assert_eq!(prediction, None);
    }

    #[test]
    fn test_trigrams() {
        let detector = SagaDetector::new().with_min_frequency(2).with_ngram_size(3);

        // A -> B -> C repeated twice
        let summaries = vec![
            "A".to_string(), "B".to_string(), "C".to_string(),
            "X".to_string(),
            "A".to_string(), "B".to_string(), "C".to_string(),
        ];

        let sagas = detector.detect_from_summaries(&summaries);

        assert_eq!(sagas.len(), 1);
        assert_eq!(sagas[0].steps, vec!["A", "B", "C"]);
        assert_eq!(sagas[0].frequency, 2);
    }
}
