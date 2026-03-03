//! Dream Team Builder: Assembles the optimal team using momentum and synergy.
//!
//! "Nova's Team Selector" - This module uses `MomentumTracker` and `SynergyGraph`
//! to construct a team of a given size that maximizes both individual performance
//! and mutual cooperation using a greedy algorithm.

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;

/// Builds an optimal team of a specific size using Momentum and Synergy.
pub struct DreamTeamBuilder<'a> {
    momentum_tracker: &'a MomentumTracker,
    synergy_graph: &'a SynergyGraph,
}

impl<'a> DreamTeamBuilder<'a> {
    /// Create a new DreamTeamBuilder with reference to trackers.
    pub fn new(momentum_tracker: &'a MomentumTracker, synergy_graph: &'a SynergyGraph) -> Self {
        Self {
            momentum_tracker,
            synergy_graph,
        }
    }

    /// Selects the best team of up to `team_size` from a list of `candidates`.
    pub fn build_team(&self, candidates: &[AgentId], team_size: usize) -> Vec<AgentId> {
        if candidates.is_empty() || team_size == 0 {
            return Vec::new();
        }

        let mut available_candidates = candidates.to_vec();
        let mut team = Vec::with_capacity(team_size);

        // Step 1: Pick the agent with the highest momentum as the seed.
        // We handle tie-breaks by picking the first candidate in the list.
        available_candidates.sort_by(|a, b| {
            let score_a = self.momentum_tracker.agent_momentum(*a);
            let score_b = self.momentum_tracker.agent_momentum(*b);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if let Some(seed) = available_candidates.first().copied() {
            team.push(seed);
            available_candidates.retain(|&c| c != seed);
        }

        // Step 2: Iteratively add the agent that maximizes (Momentum + Team Synergy)
        while team.len() < team_size && !available_candidates.is_empty() {
            let mut best_candidate = None;
            let mut best_score = f32::NEG_INFINITY;

            for &candidate in &available_candidates {
                let momentum = self.momentum_tracker.agent_momentum(candidate);

                // Calculate synergy with the *existing* team
                let mut synergy = 0.0;
                for &team_member in &team {
                    // Synergy Graph stores edges based on min/max AgentId.
                    // To easily look up edges we need to recreate the key.
                    let key = if candidate.as_uuid() < team_member.as_uuid() {
                        (candidate, team_member)
                    } else {
                        (team_member, candidate)
                    };

                    // Manually fetch edge weight
                    let edge_weight = self
                        .synergy_graph
                        .edges()
                        .into_iter()
                        .find(|e| e.agent_a == key.0 && e.agent_b == key.1)
                        .map(|e| e.weight)
                        .unwrap_or(0.0);

                    synergy += edge_weight;
                }

                // Add momentum with a stronger synergy multiplier
                let total_score = momentum + (synergy * 2.0);

                if total_score > best_score {
                    best_score = total_score;
                    best_candidate = Some(candidate);
                }
            }

            if let Some(selected) = best_candidate {
                team.push(selected);
                available_candidates.retain(|&c| c != selected);
            } else {
                break; // Should not happen unless scores are NaN
            }
        }

        team
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_persistence::{TrajectoryEvent, TrajectoryEventId, TriggerKind};

    // Helper to bypass privacy rules for testing.
    fn make_event(agent_id: AgentId, success: bool, task_id: Option<String>) -> TrajectoryEvent {
        let id = TrajectoryEventId::new();
        let task_uuid =
            task_id.map(|t| uuid::Uuid::parse_str(&t).unwrap_or_else(|_| uuid::Uuid::new_v4()));

        let json = serde_json::json!({
            "id": id,
            "trigger_kind": TriggerKind::TaskComplete,
            "agent_id": agent_id,
            "task_id": task_uuid,
            "success": success,
            "summary": "did work",
            "steps": [],
            "created_at": Utc::now()
        });

        serde_json::from_value(json).expect("Failed to create mock event")
    }

    #[test]
    fn test_dream_team_builder_success() {
        let mut tracker = MomentumTracker::new();
        let mut graph = SynergyGraph::new();

        let a1 = AgentId::new(); // High momentum, high synergy with a2
        let a2 = AgentId::new(); // Medium momentum, high synergy with a1
        let a3 = AgentId::new(); // High momentum, low synergy

        let task = uuid::Uuid::new_v4().to_string();

        let events = vec![
            make_event(a1, true, Some(task.clone())),
            make_event(a1, true, Some(task.clone())),
            make_event(a2, true, Some(task.clone())),
            make_event(a3, true, None),
            make_event(a3, true, None),
        ];

        tracker.process_events(&events);
        graph.build_from_events(&events);

        let candidates = vec![a1, a2, a3];

        let builder = DreamTeamBuilder::new(&tracker, &graph);
        let team = builder.build_team(&candidates, 2);

        // Expecting [a1, a2] because even though a3 has high momentum,
        // a1 and a2 have high synergy.
        assert_eq!(team.len(), 2);
        assert!(team.contains(&a1));
        assert!(team.contains(&a2));
    }
}
