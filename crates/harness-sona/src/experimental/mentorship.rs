//! Mentorship Matchmaker: Pairs high-performing agents with struggling agents.
//!
//! "Nova's Mentorship Matchmaker" - This module uses the `MomentumTracker` and
//! `SynergyGraph` to find the best mentor/apprentice pairings. A good pairing
//! consists of a mentor with high momentum and an apprentice with low momentum,
//! ideally prioritizing pairs that haven't worked together much (low synergy)
//! to encourage cross-pollination.

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;

/// Represents a mentor-apprentice pairing.
#[derive(Debug, Clone, PartialEq)]
pub struct MentorshipPair {
    pub mentor: AgentId,
    pub apprentice: AgentId,
}

/// The Matchmaker pairs agents based on momentum and synergy.
pub struct MentorshipMatchmaker<'a> {
    momentum: &'a MomentumTracker,
    synergy: &'a SynergyGraph,
}

impl<'a> MentorshipMatchmaker<'a> {
    /// Create a new MentorshipMatchmaker.
    pub fn new(momentum: &'a MomentumTracker, synergy: &'a SynergyGraph) -> Self {
        Self { momentum, synergy }
    }

    /// Match mentors (momentum >= mentor_threshold) with apprentices (momentum <= apprentice_threshold).
    /// Returns a list of pairings. An agent can only be in one pair.
    pub fn match_mentors(
        &self,
        candidates: &[AgentId],
        mentor_threshold: f32,
        apprentice_threshold: f32,
    ) -> Vec<MentorshipPair> {
        let mut mentors = Vec::new();
        let mut apprentices = Vec::new();

        // Categorize candidates based on their momentum
        for &agent in candidates {
            let momentum = self.momentum.agent_momentum(agent);
            if momentum >= mentor_threshold {
                mentors.push((agent, momentum));
            } else if momentum <= apprentice_threshold {
                apprentices.push((agent, momentum));
            }
        }

        // Sort mentors by highest momentum first
        mentors.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        // Sort apprentices by lowest momentum first
        apprentices.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut pairs = Vec::new();
        let mut matched_apprentices = std::collections::HashSet::new();

        for (mentor, _) in mentors {
            let mut best_apprentice = None;
            let mut best_synergy = f32::INFINITY;

            for (apprentice, _) in &apprentices {
                if matched_apprentices.contains(apprentice) {
                    continue;
                }

                // Calculate synergy between this mentor and apprentice
                let synergy = self.get_synergy(mentor, *apprentice);

                // We want to MINIMIZE synergy (encourage cross-pollination)
                if synergy < best_synergy {
                    best_synergy = synergy;
                    best_apprentice = Some(*apprentice);
                }
            }

            if let Some(apprentice) = best_apprentice {
                pairs.push(MentorshipPair { mentor, apprentice });
                matched_apprentices.insert(apprentice);
            }
        }

        pairs
    }

    /// Helper to get the synergy weight between two agents from the synergy graph.
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

    #[test]
    fn test_mentorship_matchmaker() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let mentor1 = AgentId::new(); // High momentum
        let mentor2 = AgentId::new(); // High momentum
        let apprentice1 = AgentId::new(); // Low momentum
        let apprentice2 = AgentId::new(); // Low momentum
        let average_agent = AgentId::new(); // Zero momentum

        let m_events = vec![
            make_event_for_momentum(mentor1, true),
            make_event_for_momentum(mentor1, true),
            make_event_for_momentum(mentor1, true),
            make_event_for_momentum(mentor2, true),
            make_event_for_momentum(mentor2, true),
            make_event_for_momentum(apprentice1, false),
            make_event_for_momentum(apprentice1, false),
            make_event_for_momentum(apprentice2, false),
        ];
        momentum.process_events(&m_events);

        // mentor1 and apprentice1 have worked together a lot (high synergy)
        let t1 = uuid::Uuid::new_v4().to_string();
        let t2 = uuid::Uuid::new_v4().to_string();

        let s_events = vec![
            make_event_for_synergy(mentor1, Some(t1.clone())),
            make_event_for_synergy(apprentice1, Some(t1.clone())),
            make_event_for_synergy(mentor1, Some(t2.clone())),
            make_event_for_synergy(apprentice1, Some(t2.clone())),
        ];
        synergy.build_from_events(&s_events);

        let matchmaker = MentorshipMatchmaker::new(&momentum, &synergy);
        let candidates = vec![mentor1, mentor2, apprentice1, apprentice2, average_agent];

        let pairs = matchmaker.match_mentors(&candidates, 1.0, -1.0);

        // We expect 2 pairs.
        assert_eq!(pairs.len(), 2);

        // mentor1 has high synergy with apprentice1, so it should prefer apprentice2
        // mentor2 should then be paired with apprentice1
        let pair1 = pairs.iter().find(|p| p.mentor == mentor1).unwrap();
        assert_eq!(pair1.apprentice, apprentice2);

        let pair2 = pairs.iter().find(|p| p.mentor == mentor2).unwrap();
        assert_eq!(pair2.apprentice, apprentice1);
    }
}
