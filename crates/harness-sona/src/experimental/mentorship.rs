//! Mentorship Matchmaker: Pairs high-momentum agents with low-momentum agents.
//!
//! "Nova's Mentorship Program" - This module uses the `MomentumTracker` to find
//! agents that are performing well and pairs them with agents that are struggling.
//! It uses the `SynergyGraph` to prioritize pairs that haven't worked together much,
//! encouraging cross-pollination of skills.

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;

/// A proposed mentorship pairing.
#[derive(Debug, Clone, PartialEq)]
pub struct MentorshipPair {
    /// The agent with high momentum who will guide.
    pub mentor: AgentId,
    /// The agent with low momentum who needs guidance.
    pub apprentice: AgentId,
    /// The current synergy score between the two (lower is better for cross-pollination).
    pub existing_synergy: f32,
}

/// The Mentorship Matchmaker pairs mentors with apprentices.
pub struct MentorshipMatchmaker<'a> {
    momentum: &'a MomentumTracker,
    synergy: &'a SynergyGraph,
}

impl<'a> MentorshipMatchmaker<'a> {
    /// Create a new MentorshipMatchmaker.
    pub fn new(momentum: &'a MomentumTracker, synergy: &'a SynergyGraph) -> Self {
        Self { momentum, synergy }
    }

    /// Find mentorship pairs from a pool of candidates.
    pub fn find_pairs(&self, candidates: &[AgentId]) -> Vec<MentorshipPair> {
        if candidates.len() < 2 {
            return Vec::new();
        }

        // 1. Separate into mentors and apprentices
        let mut mentors = Vec::new();
        let mut apprentices = Vec::new();

        for &agent in candidates {
            let mom = self.momentum.agent_momentum(agent);
            if mom > 0.0 {
                mentors.push((agent, mom));
            } else if mom <= 0.0 {
                // Anyone with <= 0 momentum is a potential apprentice
                apprentices.push((agent, mom));
            }
        }

        // Sort mentors by momentum descending (highest first)
        mentors.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        // Sort apprentices by momentum ascending (lowest first, need most help)
        apprentices.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut pairs = Vec::new();
        let mut used_mentors = std::collections::HashSet::new();
        let mut used_apprentices = std::collections::HashSet::new();

        // Need the synergy edges for lookups
        let edges = self.synergy.edges();

        // 2. Greedily pair them
        for (mentor_id, _) in mentors {
            if used_mentors.contains(&mentor_id) {
                continue;
            }

            let mut best_apprentice = None;
            let mut lowest_synergy = f32::MAX;

            for (apprentice_id, _) in &apprentices {
                if used_apprentices.contains(apprentice_id) {
                    continue;
                }

                // Find existing synergy
                let mut existing_synergy = 0.0;
                for edge in &edges {
                    if (edge.agent_a == mentor_id && edge.agent_b == *apprentice_id)
                        || (edge.agent_a == *apprentice_id && edge.agent_b == mentor_id)
                    {
                        existing_synergy = edge.weight;
                        break;
                    }
                }

                if existing_synergy < lowest_synergy {
                    lowest_synergy = existing_synergy;
                    best_apprentice = Some(*apprentice_id);
                }
            }

            if let Some(apprentice_id) = best_apprentice {
                pairs.push(MentorshipPair {
                    mentor: mentor_id,
                    apprentice: apprentice_id,
                    existing_synergy: lowest_synergy,
                });
                used_mentors.insert(mentor_id);
                used_apprentices.insert(apprentice_id);
            }
        }

        pairs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_persistence::{TrajectoryEvent, TrajectoryEventId, TriggerKind};

    // Helper to create events for synergy
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

    // Helper to create events for momentum
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
    fn test_mentorship_pairing() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let mentor1 = AgentId::new();
        let mentor2 = AgentId::new();
        let apprentice1 = AgentId::new();
        let apprentice2 = AgentId::new();
        let neutral = AgentId::new(); // Zero momentum, shouldn't be matched if there are better ones

        // Set up momentum
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

        // Set up synergy
        // mentor1 and apprentice1 have worked together a lot
        let t1 = uuid::Uuid::new_v4().to_string();
        let t2 = uuid::Uuid::new_v4().to_string();
        let s_events = vec![
            make_event_for_synergy(mentor1, Some(t1.clone())),
            make_event_for_synergy(apprentice1, Some(t1.clone())),
            make_event_for_synergy(mentor1, Some(t2.clone())),
            make_event_for_synergy(apprentice1, Some(t2.clone())),
        ];
        synergy.build_from_events(&s_events);

        // This gives synergy(mentor1, apprentice1) = 2.0
        // All other pairs have synergy 0.0

        let matchmaker = MentorshipMatchmaker::new(&momentum, &synergy);
        let candidates = vec![mentor1, mentor2, apprentice1, apprentice2, neutral];

        let pairs = matchmaker.find_pairs(&candidates);

        // We expect 2 pairs.
        assert_eq!(pairs.len(), 2);

        // Because mentor1 and apprentice1 have high synergy, mentor1 should be paired with apprentice2
        // and mentor2 should be paired with apprentice1 to encourage cross-pollination.
        // Let's check the pairs. We don't know the exact order of pairs returned, so we check both.
        let pair1 = pairs
            .iter()
            .find(|p| p.mentor == mentor1)
            .expect("Mentor 1 should have a pair");
        assert_eq!(
            pair1.apprentice, apprentice2,
            "Mentor 1 should be paired with Apprentice 2 due to lower synergy"
        );
        assert_eq!(pair1.existing_synergy, 0.0);

        let pair2 = pairs
            .iter()
            .find(|p| p.mentor == mentor2)
            .expect("Mentor 2 should have a pair");
        assert_eq!(
            pair2.apprentice, apprentice1,
            "Mentor 2 should be paired with Apprentice 1"
        );
        assert_eq!(pair2.existing_synergy, 0.0);
    }
}
