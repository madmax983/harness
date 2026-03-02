use chrono::{DateTime, Utc};
use harness_persistence::Knowledge;

/// Ebbinghaus Memory Decay logic for filtering and prioritizing knowledge.
///
/// Implements the formula R = e^(-t/S) where:
/// R is the retention probability (0.0 to 1.0)
/// t is the time elapsed since the knowledge was acquired (in seconds)
/// S is the strength of the memory
pub struct MemoryDecay {
    /// Base strength of a new memory before any reinforcements/recalls.
    /// Higher values mean memory decays slower.
    base_strength: f64,
}

impl MemoryDecay {
    /// Create a new `MemoryDecay` strategy with the given base strength.
    /// A typical base_strength could be the number of seconds in a day (86400)
    /// to have a significant drop-off over a few days.
    pub fn new(base_strength: f64) -> Self {
        Self { base_strength }
    }

    /// Calculate the retention score for a piece of knowledge based on its age and recall count.
    pub fn retention_score(
        &self,
        created_at: DateTime<Utc>,
        now: DateTime<Utc>,
        recalls: u32,
    ) -> f64 {
        let elapsed_secs = (now - created_at).num_seconds() as f64;

        // If time is in the future, return max retention
        if elapsed_secs <= 0.0 {
            return 1.0;
        }

        // Each recall reinforces the memory strength linearly.
        let strength = self.base_strength * (1.0 + recalls as f64);

        (-elapsed_secs / strength).exp()
    }

    /// Filters a list of Knowledge items, retaining only those with a score >= threshold.
    /// For this minimal viable feature, we assume recalls = 0. In a full system,
    /// recalls would be tracked per-knowledge-item.
    pub fn filter_stale_knowledge(
        &self,
        knowledge: Vec<Knowledge>,
        now: DateTime<Utc>,
        threshold: f64,
    ) -> Vec<Knowledge> {
        knowledge
            .into_iter()
            .filter(|k| self.retention_score(k.created_at, now, 0) >= threshold)
            .collect()
    }
}

impl Default for MemoryDecay {
    fn default() -> Self {
        // Default base strength of 1 week (604800 seconds)
        Self::new(604800.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use harness_persistence::{AgentId, KnowledgeKind, SessionId};

    #[test]
    fn test_retention_curve() {
        let decay = MemoryDecay::new(100.0); // very fast decay for testing
        let now = Utc::now();

        // At t=0, retention is 1.0
        let score_now = decay.retention_score(now, now, 0);
        assert!((score_now - 1.0).abs() < f64::EPSILON);

        // At t=100 (which is base_strength), retention is e^-1 (~0.367)
        let t100 = now - Duration::seconds(100);
        let score_t100 = decay.retention_score(t100, now, 0);
        assert!((score_t100 - std::f64::consts::E.powi(-1)).abs() < 0.01);

        // At t=200, retention is e^-2 (~0.135)
        let t200 = now - Duration::seconds(200);
        let score_t200 = decay.retention_score(t200, now, 0);
        assert!((score_t200 - std::f64::consts::E.powi(-2)).abs() < 0.01);
    }

    #[test]
    fn test_recall_reinforcement() {
        let decay = MemoryDecay::new(100.0);
        let now = Utc::now();
        let past = now - Duration::seconds(100);

        // With 0 recalls, score is e^-1
        let score_0 = decay.retention_score(past, now, 0);

        // With 1 recall, strength doubles (200), score is e^(-100/200) = e^-0.5 (~0.606)
        let score_1 = decay.retention_score(past, now, 1);

        assert!(score_1 > score_0, "Recalls should increase retention score");
        assert!((score_1 - std::f64::consts::E.powf(-0.5)).abs() < 0.01);
    }

    #[test]
    fn test_filter_stale_knowledge() {
        let decay = MemoryDecay::new(100.0);
        let now = Utc::now();
        let agent_id = AgentId::new();
        let session_id = SessionId::new();

        let mut fresh =
            Knowledge::new("Fresh item", KnowledgeKind::Discovery, agent_id, session_id);
        fresh.created_at = now - Duration::seconds(10); // score = e^-0.1 (~0.9)

        let mut stale =
            Knowledge::new("Stale item", KnowledgeKind::Discovery, agent_id, session_id);
        stale.created_at = now - Duration::seconds(200); // score = e^-2 (~0.135)

        let items = vec![fresh.clone(), stale.clone()];

        // Filter with threshold 0.5 (should keep fresh, drop stale)
        let filtered = decay.filter_stale_knowledge(items, now, 0.5);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].content, "Fresh item");
    }
}
