//! Momentum Tracker: Analyzes the ongoing momentum or streak of agents.
//!
//! "Nova's Streak Metric" - This module calculates an agent's momentum based on
//! a sequence of recent `TrajectoryEvent`s. A series of successes builds
//! high momentum ("On Fire" state), while consecutive failures reduce it.

use harness_persistence::{AgentId, TrajectoryEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tracks momentum scores across the hive.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MomentumTracker {
    scores: HashMap<AgentId, f32>,
}

impl MomentumTracker {
    /// Create a new, empty MomentumTracker.
    pub fn new() -> Self {
        Self {
            scores: HashMap::new(),
        }
    }

    /// Process a batch of trajectory events to update agent momentum.
    pub fn process_events(&mut self, events: &[TrajectoryEvent]) {
        for event in events {
            let score = self.scores.entry(event.agent_id()).or_insert(0.0);
            if event.success() {
                *score += 1.0;
            } else {
                *score -= 1.0;
            }

            // Apply a simple bounding so momentum doesn't grow infinitely
            *score = (*score).clamp(-10.0, 10.0);
        }
    }

    /// Retrieve the current momentum score for an agent.
    pub fn agent_momentum(&self, agent_id: AgentId) -> f32 {
        self.scores.get(&agent_id).copied().unwrap_or(0.0)
    }

    /// Retrieve an iterator over all tracked agents.
    pub fn agents(&self) -> impl Iterator<Item = AgentId> + '_ {
        self.scores.keys().copied()
    }

    /// Check if an agent is currently "On Fire" (momentum above a threshold).
    pub fn is_on_fire(&self, agent_id: AgentId) -> bool {
        self.agent_momentum(agent_id) >= 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_persistence::{TrajectoryEventId, TriggerKind};

    fn make_event(agent_id: AgentId, success: bool) -> TrajectoryEvent {
        let id = TrajectoryEventId::new();
        let json = serde_json::json!({
            "id": id,
            "trigger_kind": TriggerKind::TaskComplete,
            "agent_id": agent_id,
            "task_id": null,
            "success": success,
            "summary": "did work",
            "steps": [],
            "created_at": Utc::now()
        });

        serde_json::from_value(json).expect("Failed to create mock event")
    }

    #[test]
    fn test_momentum_calculation() {
        let mut tracker = MomentumTracker::new();

        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        let events = vec![
            make_event(agent1, true),
            make_event(agent1, true),
            make_event(agent1, true), // Agent 1 is on a winning streak!
            make_event(agent2, false),
            make_event(agent2, false), // Agent 2 is struggling...
        ];

        tracker.process_events(&events);

        let m1 = tracker.agent_momentum(agent1);
        let m2 = tracker.agent_momentum(agent2);

        assert!(m1 > 2.0, "Agent 1 should have high momentum");
        assert!(tracker.is_on_fire(agent1), "Agent 1 should be 'On Fire'");

        assert!(m2 < 0.0, "Agent 2 should have negative momentum");
        assert!(
            !tracker.is_on_fire(agent2),
            "Agent 2 should not be 'On Fire'"
        );
    }
}
