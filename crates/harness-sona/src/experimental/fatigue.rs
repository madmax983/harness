//! Fatigue Tracker: Monitors agent workload and prevents burnout.
//!
//! "Nova's Exhaustion Metric" - This module calculates an agent's fatigue based on
//! the frequency and volume of `TrajectoryEvent`s. Continuous work without breaks
//! increases fatigue, potentially transitioning an agent into an `Exhausted` state.

use chrono::{DateTime, Utc};
use harness_persistence::{AgentId, TrajectoryEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents the current fatigue level of an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FatigueLevel {
    Rested,
    Active,
    Tired,
    Exhausted,
}

/// Tracks fatigue scores across the hive.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FatigueTracker {
    // Mapping of AgentId to their current fatigue score and last activity time.
    states: HashMap<AgentId, AgentFatigueState>,
    // Fatigue threshold to become Active
    active_threshold: f32,
    // Fatigue threshold to become Tired
    tired_threshold: f32,
    // Fatigue threshold to become Exhausted
    exhausted_threshold: f32,
    // How much fatigue decreases per hour of rest
    recovery_rate_per_hour: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFatigueState {
    pub score: f32,
    pub last_active: DateTime<Utc>,
}

impl FatigueTracker {
    /// Create a new FatigueTracker with default thresholds.
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            active_threshold: 10.0,
            tired_threshold: 30.0,
            exhausted_threshold: 50.0,
            recovery_rate_per_hour: 5.0,
        }
    }

    /// Retrieve the current agent state, applying any time-based recovery.
    fn get_current_state(&self, agent_id: AgentId, now: DateTime<Utc>) -> AgentFatigueState {
        if let Some(state) = self.states.get(&agent_id) {
            let elapsed_hours = (now - state.last_active).num_seconds() as f32 / 3600.0;
            if elapsed_hours > 0.0 {
                let recovery = elapsed_hours * self.recovery_rate_per_hour;
                let new_score = (state.score - recovery).max(0.0);
                AgentFatigueState {
                    score: new_score,
                    last_active: state.last_active, // Keep the old last_active until a new event occurs
                }
            } else {
                state.clone()
            }
        } else {
            AgentFatigueState {
                score: 0.0,
                last_active: now,
            }
        }
    }

    /// Process a trajectory event to update agent fatigue.
    pub fn process_event(&mut self, event: &TrajectoryEvent, now: DateTime<Utc>) {
        let mut state = self.get_current_state(event.agent_id(), now);

        // Add fatigue based on the event (hardcoded ~5.0 for minimal viable feature)
        state.score += 5.0;
        state.last_active = now; // Update last active time to current event time

        self.states.insert(event.agent_id(), state);
    }

    /// Retrieve the current fatigue state for an agent.
    pub fn agent_fatigue(&self, agent_id: AgentId, now: DateTime<Utc>) -> FatigueLevel {
        let state = self.get_current_state(agent_id, now);
        if state.score >= self.exhausted_threshold {
            FatigueLevel::Exhausted
        } else if state.score >= self.tired_threshold {
            FatigueLevel::Tired
        } else if state.score >= self.active_threshold {
            FatigueLevel::Active
        } else {
            FatigueLevel::Rested
        }
    }

    /// Check if an agent is exhausted and needs a break.
    pub fn is_exhausted(&self, agent_id: AgentId, now: DateTime<Utc>) -> bool {
        self.agent_fatigue(agent_id, now) == FatigueLevel::Exhausted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use harness_persistence::{TrajectoryEventId, TriggerKind};

    fn make_event(agent_id: AgentId, time: DateTime<Utc>) -> TrajectoryEvent {
        let id = TrajectoryEventId::new();
        let json = serde_json::json!({
            "id": id,
            "trigger_kind": TriggerKind::TaskComplete,
            "agent_id": agent_id,
            "task_id": null,
            "success": true,
            "summary": "did work",
            "steps": [],
            "created_at": time
        });

        serde_json::from_value(json).expect("Failed to create mock event")
    }

    #[test]
    fn test_fatigue_accumulation_and_recovery() {
        let mut tracker = FatigueTracker::new();
        let agent1 = AgentId::new();
        let start_time = Utc::now();

        // Agent starts Rested
        assert_eq!(
            tracker.agent_fatigue(agent1, start_time),
            FatigueLevel::Rested
        );

        // Spam events to make agent Exhausted
        let mut current_time = start_time;
        for _ in 0..15 {
            // Assuming each event adds ~5 fatigue
            current_time = current_time + Duration::minutes(1);
            let event = make_event(agent1, current_time);
            tracker.process_event(&event, current_time);
        }

        assert_eq!(
            tracker.agent_fatigue(agent1, current_time),
            FatigueLevel::Exhausted
        );
        assert!(tracker.is_exhausted(agent1, current_time));

        // Let agent rest for 12 hours
        let rest_time = current_time + Duration::hours(12);

        // After 12 hours of rest at 5 points per hour, should recover 60 points
        // If max was ~75, it should be down to ~15, which is Active or Rested depending on exact math
        let state_after_rest = tracker.agent_fatigue(agent1, rest_time);
        assert!(
            state_after_rest == FatigueLevel::Active || state_after_rest == FatigueLevel::Rested
        );
        assert!(!tracker.is_exhausted(agent1, rest_time));
    }
}
