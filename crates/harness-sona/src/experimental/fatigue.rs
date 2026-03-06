//! Fatigue Tracker: Monitors agent workload and burnout states.
//!
//! "Nova's Burnout Gauge" - This module calculates an agent's fatigue
//! from `TrajectoryEvent`s and applies time-based recovery using `DateTime<Utc>`.
//! Agents can transition between `Rested`, `Active`, `Tired`, and `Exhausted` states.

use chrono::{DateTime, Utc};
use harness_persistence::{AgentId, TrajectoryEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BurnoutState {
    Rested,
    Active,
    Tired,
    Exhausted,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FatigueTracker {
    fatigue_levels: HashMap<AgentId, f32>,
    last_updated: HashMap<AgentId, DateTime<Utc>>,
}

impl FatigueTracker {
    pub fn new() -> Self {
        Self {
            fatigue_levels: HashMap::new(),
            last_updated: HashMap::new(),
        }
    }

    pub fn agent_state(&self, agent_id: AgentId) -> BurnoutState {
        let fatigue = self.fatigue_levels.get(&agent_id).copied().unwrap_or(0.0);

        if fatigue < 2.0 {
            BurnoutState::Rested
        } else if fatigue < 5.0 {
            BurnoutState::Active
        } else if fatigue < 8.0 {
            BurnoutState::Tired
        } else {
            BurnoutState::Exhausted
        }
    }

    pub fn process_events(&mut self, events: &[TrajectoryEvent]) {
        for event in events {
            let agent_id = event.agent_id();
            let created_at = event.created_at();

            // First apply recovery since the last event (if any)
            self.apply_recovery(agent_id, created_at);

            // Add fatigue for this event
            let fatigue = self.fatigue_levels.entry(agent_id).or_insert(0.0);

            // Assume 1.0 fatigue per event. You could adjust this based on success/failure
            // or other event properties.
            *fatigue += 1.0;

            // Cap maximum fatigue
            *fatigue = (*fatigue).min(10.0);

            // Update last seen
            self.last_updated.insert(agent_id, created_at);
        }
    }

    pub fn apply_recovery(&mut self, agent_id: AgentId, now: DateTime<Utc>) {
        if let Some(last_updated) = self.last_updated.get_mut(&agent_id) {
            let elapsed_seconds = (now - *last_updated).num_seconds();
            if elapsed_seconds > 0 {
                let elapsed_hours = elapsed_seconds as f32 / 3600.0;
                if let Some(fatigue) = self.fatigue_levels.get_mut(&agent_id) {
                    // Recover 0.5 fatigue per hour
                    *fatigue -= elapsed_hours * 0.5;
                    *fatigue = (*fatigue).max(0.0);
                }
                *last_updated = now;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use harness_persistence::{TrajectoryEventId, TriggerKind};

    fn make_event(agent_id: AgentId, success: bool, created_at: DateTime<Utc>) -> TrajectoryEvent {
        let id = TrajectoryEventId::new();
        let json = serde_json::json!({
            "id": id,
            "trigger_kind": TriggerKind::TaskComplete,
            "agent_id": agent_id,
            "task_id": null,
            "success": success,
            "summary": "did work",
            "steps": [],
            "created_at": created_at
        });

        serde_json::from_value(json).expect("Failed to create mock event")
    }

    #[test]
    fn test_fatigue_states_and_recovery() {
        let mut tracker = FatigueTracker::new();
        let agent_id = AgentId::new();
        let now = Utc::now();

        // Initially rested
        assert_eq!(tracker.agent_state(agent_id), BurnoutState::Rested);

        // Process a bunch of events rapidly to simulate heavy workload
        let mut events = Vec::new();
        for i in 0..10 {
            events.push(make_event(agent_id, true, now + Duration::seconds(i)));
        }

        tracker.process_events(&events);

        // Should be exhausted after 10 quick tasks
        assert_eq!(tracker.agent_state(agent_id), BurnoutState::Exhausted);

        // Apply recovery after a long time
        tracker.apply_recovery(agent_id, now + Duration::hours(12));

        // After 12 hours of recovery at 0.5 per hour, fatigue decreases by 6.0.
        // It was 10.0, now it should be 4.0.
        // 4.0 is in the Active state (< 5.0).
        // Let's add more recovery to get it fully Rested.
        tracker.apply_recovery(agent_id, now + Duration::hours(24));

        // Should be rested again
        assert_eq!(tracker.agent_state(agent_id), BurnoutState::Rested);
    }
}
