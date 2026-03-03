//! Dream Team: AI Agent Team Formation based on Momentum and Synergy.
//!
//! "Nova's Dream Team" - This module combines the `MomentumTracker` and
//! `SynergyGraph` to assemble the ultimate strike team. It selects agents
//! who are not only performing well individually (high momentum) but also
//! work well together (high synergy).

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;
use std::collections::HashSet;

/// The Dream Team Builder assembles teams of agents using their individual momentum
/// and their mutual synergy scores.
pub struct DreamTeamBuilder<'a> {
    momentum: &'a MomentumTracker,
    synergy: &'a SynergyGraph,
}

impl<'a> DreamTeamBuilder<'a> {
    /// Create a new DreamTeamBuilder.
    pub fn new(momentum: &'a MomentumTracker, synergy: &'a SynergyGraph) -> Self {
        Self { momentum, synergy }
    }

    /// Assemble a team of up to `size` agents from the given `candidates`.
    ///
    /// The algorithm uses a greedy approach:
    /// 1. Start with the candidate who has the highest momentum.
    /// 2. Iteratively add the candidate who maximizes:
    ///    `candidate_momentum + sum(synergy with already selected team members)`
    pub fn assemble_team(&self, candidates: &[AgentId], size: usize) -> Vec<AgentId> {
        if candidates.is_empty() || size == 0 {
            return Vec::new();
        }

        let mut available: HashSet<AgentId> = candidates.iter().copied().collect();
        let mut team = Vec::new();

        // Step 1: Find the anchor (agent with highest momentum)
        if let Some(&anchor) = candidates.iter().max_by(|&&a, &&b| {
            let ma = self.momentum.agent_momentum(a);
            let mb = self.momentum.agent_momentum(b);
            ma.partial_cmp(&mb).unwrap_or(std::cmp::Ordering::Equal)
        }) {
            team.push(anchor);
            available.remove(&anchor);
        }

        // Step 2: Greedily add agents that maximize (momentum + synergy with team)
        while team.len() < size && !available.is_empty() {
            let mut best_candidate = None;
            let mut best_score = f32::NEG_INFINITY;

            for &candidate in &available {
                let mut score = self.momentum.agent_momentum(candidate);

                // Add synergy with existing team members
                for &member in &team {
                    score += self.get_synergy(candidate, member);
                }

                if score > best_score {
                    best_score = score;
                    best_candidate = Some(candidate);
                }
            }

            if let Some(candidate) = best_candidate {
                team.push(candidate);
                available.remove(&candidate);
            } else {
                break;
            }
        }

        team
    }

    /// Get the synergy weight between two agents from the synergy graph.
    fn get_synergy(&self, agent_a: AgentId, agent_b: AgentId) -> f32 {
        for edge in self.synergy.edges() {
            if (edge.agent_a == agent_a && edge.agent_b == agent_b)
                || (edge.agent_a == agent_b && edge.agent_b == agent_a)
            {
                return edge.weight;
            }
        }
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_persistence::{TrajectoryEvent, TrajectoryEventId, TriggerKind};

    // Helper to bypass privacy rules for testing.
    fn make_event_for_synergy(agent_id: AgentId, task_id: Option<String>) -> TrajectoryEvent {
        let id = TrajectoryEventId::new();
        let task_uuid =
            task_id.map(|t| uuid::Uuid::parse_str(&t).unwrap_or_else(|_| uuid::Uuid::new_v4()));

        let json = serde_json::json!({
            "id": id,
            "trigger_kind": TriggerKind::TaskComplete,
            "agent_id": agent_id,
            "task_id": task_uuid,
            "success": true,
            "summary": "did work",
            "steps": [],
            "created_at": Utc::now()
        });

        serde_json::from_value(json).expect("Failed to create mock event")
    }

    fn make_event_for_momentum(agent_id: AgentId, success: bool) -> TrajectoryEvent {
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
    fn test_dream_team_assembly() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let a3 = AgentId::new();
        let a4 = AgentId::new();

        // Momentum setup
        // a1: high momentum (3 wins)
        // a2: medium momentum (1 win)
        // a3: low momentum (1 loss)
        // a4: zero momentum
        let m_events = vec![
            make_event_for_momentum(a1, true),
            make_event_for_momentum(a1, true),
            make_event_for_momentum(a1, true),
            make_event_for_momentum(a2, true),
            make_event_for_momentum(a3, false),
        ];
        momentum.process_events(&m_events);

        // Synergy setup
        // a1 and a3 work together on task1
        // a2 and a3 work together on task2
        // a3 and a4 work together on task3
        let t1 = uuid::Uuid::new_v4().to_string();
        let t2 = uuid::Uuid::new_v4().to_string();
        let t3 = uuid::Uuid::new_v4().to_string();

        let s_events = vec![
            make_event_for_synergy(a1, Some(t1.clone())),
            make_event_for_synergy(a3, Some(t1.clone())),
            make_event_for_synergy(a2, Some(t2.clone())),
            make_event_for_synergy(a3, Some(t2.clone())),
            make_event_for_synergy(a3, Some(t3.clone())),
            make_event_for_synergy(a4, Some(t3.clone())),
        ];
        synergy.build_from_events(&s_events);

        let builder = DreamTeamBuilder::new(&momentum, &synergy);
        let candidates = vec![a1, a2, a3, a4];

        // If we want a team of 1, we should just get a1 (highest momentum)
        let team1 = builder.assemble_team(&candidates, 1);
        assert_eq!(team1, vec![a1]);

        // If we want a team of 2:
        // Start with a1.
        // For remaining candidates:
        // a2 score = m(a2) + syn(a1, a2) = 1.0 + 0.0 = 1.0
        // a3 score = m(a3) + syn(a1, a3) = -1.0 + 1.0 = 0.0
        // a4 score = m(a4) + syn(a1, a4) = 0.0 + 0.0 = 0.0
        // We should pick a2 next!
        let team2 = builder.assemble_team(&candidates, 2);
        assert_eq!(team2, vec![a1, a2]);

        // What if synergy was higher? Let's add more synergy for a1 and a3.
        synergy.add_synergy(a1, a3, 3.0); // Now total synergy(a1, a3) is 4.0
        // a2 score = 1.0 + 0.0 = 1.0
        // a3 score = -1.0 + 4.0 = 3.0
        // a4 score = 0.0 + 0.0 = 0.0
        // Now it should pick a3 as the second member!
        let builder_high_syn = DreamTeamBuilder::new(&momentum, &synergy);
        let team2_high_syn = builder_high_syn.assemble_team(&candidates, 2);
        assert_eq!(team2_high_syn, vec![a1, a3]);
    }
}
