//! Saga: Workflow pattern discovery from ReasoningBank.
//!
//! "Sagas" are frequent sequences of tasks that represent common workflows.
//! By analyzing the `ReasoningBank`, we can discover these patterns and potentially
//! suggest them as automatable workflows.

use std::collections::HashMap;
use chrono::{Duration};
use serde::{Deserialize, Serialize};
use harness_persistence::{ReasoningBank, TaskPattern, PatternQuery, SessionId};

/// A discovered saga (sequence of tasks).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveredSaga {
    /// The starting task type.
    pub from_task: String,
    /// The following task type.
    pub to_task: String,
    /// How many times this transition was observed.
    pub frequency: usize,
    /// Confidence score (frequency / total occurrences of from_task).
    pub confidence: f32,
}

/// Detector for identifying saga patterns.
pub struct SagaDetector {
    /// Maximum time gap between tasks to consider them part of a sequence.
    time_window: Duration,
    /// Minimum frequency to report a saga.
    min_frequency: usize,
}

impl SagaDetector {
    /// Create a new SagaDetector with default settings.
    pub fn new() -> Self {
        Self {
            time_window: Duration::hours(1),
            min_frequency: 2,
        }
    }

    /// Set the time window for sequence detection.
    pub fn with_time_window(mut self, window: Duration) -> Self {
        self.time_window = window;
        self
    }

    /// Set the minimum frequency threshold.
    pub fn with_min_frequency(mut self, freq: usize) -> Self {
        self.min_frequency = freq;
        self
    }

    /// Detect sagas from the ReasoningBank.
    pub async fn detect_sagas(&self, bank: &ReasoningBank) -> Result<Vec<DiscoveredSaga>, anyhow::Error> {
        // 1. Fetch all successful patterns
        let query = PatternQuery::new("")
            .with_success_only(true)
            .with_limit(usize::MAX);

        let patterns = bank.query_patterns(query).await?;

        if patterns.is_empty() {
            return Ok(Vec::new());
        }

        // 2. Group by SessionId
        let mut session_groups: HashMap<SessionId, Vec<&TaskPattern>> = HashMap::new();

        for pattern in &patterns {
            if let Some(sid) = pattern.session_id() {
                session_groups.entry(sid).or_default().push(pattern);
            }
        }

        // 3. Find transitions
        // Map of (from, to) -> count
        let mut transitions: HashMap<(String, String), usize> = HashMap::new();
        // Map of from -> total_count (for confidence)
        let mut from_counts: HashMap<String, usize> = HashMap::new();

        for group in session_groups.values_mut() {
            // Sort by creation time
            group.sort_by_key(|p| p.created_at());

            // Iterate pairwise
            for i in 0..group.len().saturating_sub(1) {
                let from = group[i];
                let to = group[i+1];

                // Check time window
                let gap = to.created_at().signed_duration_since(from.created_at());
                if gap <= self.time_window && gap >= Duration::zero() {
                    let key = (from.task_type().to_string(), to.task_type().to_string());
                    *transitions.entry(key).or_insert(0) += 1;
                    *from_counts.entry(from.task_type().to_string()).or_insert(0) += 1;
                }
            }
        }

        // 4. Build results
        let mut sagas: Vec<DiscoveredSaga> = transitions
            .into_iter()
            .filter(|&(_, count)| count >= self.min_frequency)
            .map(|((from, to), count)| {
                let total = *from_counts.get(&from).unwrap_or(&count);
                let confidence = if total > 0 { count as f32 / total as f32 } else { 0.0 };
                DiscoveredSaga {
                    from_task: from,
                    to_task: to,
                    frequency: count,
                    confidence,
                }
            })
            .collect();

        // Sort by frequency (desc) then confidence (desc)
        sagas.sort_by(|a, b| {
            b.frequency.cmp(&a.frequency)
                .then(b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal))
        });

        Ok(sagas)
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
    use harness_persistence::{PatternStore, SessionId, AgentRole};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_saga_detection() {
        let store = Arc::new(PatternStore::new());
        // Use different session IDs to simulate independent traces
        let session1 = SessionId::new();
        let session2 = SessionId::new();
        let session3 = SessionId::new();

        let bank1 = ReasoningBank::new(store.clone(), session1);
        let bank2 = ReasoningBank::new(store.clone(), session2);
        let bank3 = ReasoningBank::new(store.clone(), session3);

        // Sequence 1: A -> B
        bank1.store_pattern(TaskPattern::new("TaskA", AgentRole::Developer, true, "A1")).await.unwrap();
        // Ensure strictly increasing time (though unlikely to be equal in same thread)
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        bank1.store_pattern(TaskPattern::new("TaskB", AgentRole::Developer, true, "B1")).await.unwrap();

        // Sequence 2: A -> B
        bank2.store_pattern(TaskPattern::new("TaskA", AgentRole::Developer, true, "A2")).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        bank2.store_pattern(TaskPattern::new("TaskB", AgentRole::Developer, true, "B2")).await.unwrap();

        // Sequence 3: A -> C (different)
        bank3.store_pattern(TaskPattern::new("TaskA", AgentRole::Developer, true, "A3")).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        bank3.store_pattern(TaskPattern::new("TaskC", AgentRole::Developer, true, "C3")).await.unwrap();

        let detector = SagaDetector::new().with_min_frequency(2);

        // Use bank1 (or any bank sharing store) to detect
        let sagas = detector.detect_sagas(&bank1).await.unwrap();

        assert_eq!(sagas.len(), 1, "Should find exactly one saga (A->B)");
        assert_eq!(sagas[0].from_task, "TaskA");
        assert_eq!(sagas[0].to_task, "TaskB");
        assert_eq!(sagas[0].frequency, 2);

        // Confidence check: A appeared 3 times total, A->B happened 2 times. 2/3 = 0.666
        assert!((sagas[0].confidence - 0.666).abs() < 0.01);
    }
}
