//! Mentorship Matchmaker: Pairs high-momentum agents with low-momentum agents.
//!
//! "Nova's Matchmaker" - This module uses `MomentumTracker` and `SynergyGraph`
//! to pair agents for mentorship. It prioritizes pairs with the lowest existing
//! synergy to encourage cross-pollination across the hive.

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;

/// Pairs a mentor with an apprentice.
#[derive(Debug, PartialEq)]
pub struct MentorshipPair {
    pub mentor: AgentId,
    pub apprentice: AgentId,
}

pub struct MentorshipMatchmaker<'a> {
    momentum: &'a MomentumTracker,
    synergy: &'a SynergyGraph,
}

impl<'a> MentorshipMatchmaker<'a> {
    pub fn new(momentum: &'a MomentumTracker, synergy: &'a SynergyGraph) -> Self {
        Self { momentum, synergy }
    }

    /// Match agents into mentor-apprentice pairs.
    pub fn match_pairs(&self, candidates: &[AgentId]) -> Vec<MentorshipPair> {
        if candidates.is_empty() {
            return Vec::new();
        }

        let mut mentors = Vec::new();
        let mut apprentices = Vec::new();

        // Categorize candidates based on momentum
        for &agent in candidates {
            if self.momentum.agent_momentum(agent) >= 0.0 {
                mentors.push(agent);
            } else {
                apprentices.push(agent);
            }
        }

        let mut pairs = Vec::new();

        // Greedily pair a mentor with an apprentice that they have the lowest synergy with.
        for apprentice in apprentices {
            if mentors.is_empty() {
                break;
            }

            let mut best_mentor_idx = 0;
            let mut lowest_synergy = f32::INFINITY;

            for (idx, &mentor) in mentors.iter().enumerate() {
                let synergy = self.get_synergy(mentor, apprentice);
                if synergy < lowest_synergy {
                    lowest_synergy = synergy;
                    best_mentor_idx = idx;
                }
            }

            let mentor = mentors.remove(best_mentor_idx);
            pairs.push(MentorshipPair { mentor, apprentice });
        }

        pairs
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
    fn test_mentorship_matchmaker() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let mentor1 = AgentId::new();
        let mentor2 = AgentId::new();
        let apprentice1 = AgentId::new();
        let apprentice2 = AgentId::new();

        let m_events = vec![
            make_event_for_momentum(mentor1, true),
            make_event_for_momentum(mentor1, true),
            make_event_for_momentum(mentor2, true),
            make_event_for_momentum(mentor2, true),
            make_event_for_momentum(apprentice1, false),
            make_event_for_momentum(apprentice2, false),
        ];
        momentum.process_events(&m_events);

        let t1 = uuid::Uuid::new_v4().to_string();
        let s_events = vec![
            make_event_for_synergy(mentor1, Some(t1.clone())),
            make_event_for_synergy(apprentice1, Some(t1.clone())),
        ];
        synergy.build_from_events(&s_events);

        let matchmaker = MentorshipMatchmaker::new(&momentum, &synergy);
        let candidates = vec![mentor1, mentor2, apprentice1, apprentice2];

        let pairs = matchmaker.match_pairs(&candidates);

        assert_eq!(pairs.len(), 2);

        let pair1 = pairs.iter().find(|p| p.mentor == mentor1).unwrap();
        assert_eq!(pair1.apprentice, apprentice2);

        let pair2 = pairs.iter().find(|p| p.mentor == mentor2).unwrap();
        assert_eq!(pair2.apprentice, apprentice1);
    }
}
