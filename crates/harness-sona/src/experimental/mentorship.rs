use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;

pub struct MentorshipMatchmaker<'a> {
    momentum: &'a MomentumTracker,
    synergy: &'a SynergyGraph,
}

impl<'a> MentorshipMatchmaker<'a> {
    pub fn new(momentum: &'a MomentumTracker, synergy: &'a SynergyGraph) -> Self {
        Self { momentum, synergy }
    }

    pub fn match_mentors(&self, candidates: &[AgentId]) -> Vec<(AgentId, AgentId)> {
        let mut mentors = Vec::new();
        let mut apprentices = Vec::new();

        for &candidate in candidates {
            if self.momentum.agent_momentum(candidate) >= 0.0 {
                mentors.push(candidate);
            } else {
                apprentices.push(candidate);
            }
        }

        let mut matches = Vec::new();

        // Greedy matching prioritizing lowest synergy
        while !mentors.is_empty() && !apprentices.is_empty() {
            let mut best_pair = None;
            let mut best_synergy = f32::INFINITY;
            let mut best_mentor_idx = 0;
            let mut best_apprentice_idx = 0;

            for (m_idx, &mentor) in mentors.iter().enumerate() {
                for (a_idx, &apprentice) in apprentices.iter().enumerate() {
                    let synergy = self.get_synergy(mentor, apprentice);
                    if synergy < best_synergy {
                        best_synergy = synergy;
                        best_pair = Some((mentor, apprentice));
                        best_mentor_idx = m_idx;
                        best_apprentice_idx = a_idx;
                    }
                }
            }

            if let Some((m, a)) = best_pair {
                matches.push((m, a));
                mentors.remove(best_mentor_idx);
                apprentices.remove(best_apprentice_idx);
            }
        }

        matches
    }

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
            make_event_for_momentum(mentor2, true),
            make_event_for_momentum(apprentice1, false),
            make_event_for_momentum(apprentice2, false),
        ];
        momentum.process_events(&m_events);

        let t1 = uuid::Uuid::new_v4().to_string();

        // High synergy between mentor1 and apprentice1
        let s_events = vec![
            make_event_for_synergy(mentor1, Some(t1.clone())),
            make_event_for_synergy(apprentice1, Some(t1.clone())),
        ];
        synergy.build_from_events(&s_events);

        let matchmaker = MentorshipMatchmaker::new(&momentum, &synergy);
        let candidates = vec![mentor1, mentor2, apprentice1, apprentice2];

        let pairs = matchmaker.match_mentors(&candidates);

        assert_eq!(pairs.len(), 2);

        assert!(pairs.contains(&(mentor1, apprentice2)) || pairs.contains(&(apprentice2, mentor1)));
        assert!(pairs.contains(&(mentor2, apprentice1)) || pairs.contains(&(apprentice1, mentor2)));
    }
}
