use harness_persistence::AgentId;
use serde::{Deserialize, Serialize};

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;

/// Behavioral personas for agents based on their momentum and synergy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Archetype {
    /// High momentum, high synergy
    Pillar,
    /// High momentum, low synergy
    LoneWolf,
    /// Low momentum, high synergy
    SocialLoafer,
    /// Low momentum, low synergy
    Rookie,
    /// Average momentum and synergy
    Steady,
}

/// Analyzes agent behavior combining MomentumTracker and SynergyGraph to classify agents into Archetypes.
#[derive(Debug, Default)]
pub struct ArchetypeAnalyzer {
    momentum_threshold: f32,
    synergy_threshold: f32,
}

impl ArchetypeAnalyzer {
    pub fn new() -> Self {
        Self {
            momentum_threshold: 1.0,
            synergy_threshold: 0.5,
        }
    }

    /// Classifies an agent into an Archetype based on momentum and synergy data.
    pub fn classify(
        &self,
        agent_id: AgentId,
        momentum: &MomentumTracker,
        synergy: &SynergyGraph,
    ) -> Archetype {
        let m = momentum.agent_momentum(agent_id);

        let mut total_synergy = 0.0;
        for edge in synergy.edges() {
            if edge.agent_a == agent_id || edge.agent_b == agent_id {
                total_synergy += edge.weight;
            }
        }

        let high_momentum = m >= self.momentum_threshold;
        let high_synergy = total_synergy >= self.synergy_threshold;
        let negative_momentum = m < 0.0;
        let zero_synergy = total_synergy <= 0.0;

        if high_momentum && high_synergy {
            Archetype::Pillar
        } else if high_momentum && zero_synergy {
            Archetype::LoneWolf
        } else if negative_momentum && high_synergy {
            Archetype::SocialLoafer
        } else if negative_momentum && zero_synergy {
            Archetype::Rookie
        } else {
            Archetype::Steady
        }
    }

    /// Generates a Markdown report of the hive social dynamics.
    pub fn generate_report(
        &self,
        agents: &[AgentId],
        momentum: &MomentumTracker,
        synergy: &SynergyGraph,
    ) -> String {
        let mut report = String::from("# Hive Social Dynamics\n\n");
        report.push_str("## Agent Archetypes\n\n");

        for &agent in agents {
            let archetype = self.classify(agent, momentum, synergy);
            report.push_str(&format!(
                "* Agent `{}`: **{:?}**\n",
                agent,
                archetype
            ));
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_persistence::{TrajectoryEvent, TrajectoryEventId, TriggerKind};

    fn make_event(agent_id: AgentId, task_id: Option<&str>, success: bool) -> TrajectoryEvent {
        let id = TrajectoryEventId::new();
        let task_uuid =
            task_id.map(|t| uuid::Uuid::parse_str(t).unwrap_or_else(|_| uuid::Uuid::new_v4()));

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
    fn test_archetypes_classification() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();
        let analyzer = ArchetypeAnalyzer::new();

        let a_pillar = AgentId::new();
        let a_lonewolf = AgentId::new();
        let a_loafer = AgentId::new();
        let a_rookie = AgentId::new();
        let a_steady = AgentId::new();

        let t1 = uuid::Uuid::new_v4().to_string();
        let t2 = uuid::Uuid::new_v4().to_string();

        let events = vec![
            // Pillar: High momentum, High synergy
            make_event(a_pillar, Some(&t1), true),
            make_event(a_pillar, Some(&t1), true),
            make_event(a_pillar, Some(&t2), true),
            // Lone Wolf: High momentum, Low synergy (no shared tasks)
            make_event(a_lonewolf, None, true),
            make_event(a_lonewolf, None, true),
            make_event(a_lonewolf, None, true),
            // Social Loafer: Low momentum, High synergy
            make_event(a_loafer, Some(&t1), false),
            make_event(a_loafer, Some(&t1), false),
            make_event(a_loafer, Some(&t2), false),
            // Rookie: Low momentum, Low synergy
            make_event(a_rookie, None, false),
            make_event(a_rookie, None, false),
            make_event(a_rookie, None, false),
            // Steady: average/neutral momentum, average synergy
            make_event(a_steady, Some(&t1), true), // synergy
            make_event(a_steady, None, false),     // reduce momentum
        ];

        momentum.process_events(&events);
        synergy.build_from_events(&events);

        assert_eq!(
            analyzer.classify(a_pillar, &momentum, &synergy),
            Archetype::Pillar
        );
        assert_eq!(
            analyzer.classify(a_lonewolf, &momentum, &synergy),
            Archetype::LoneWolf
        );
        assert_eq!(
            analyzer.classify(a_loafer, &momentum, &synergy),
            Archetype::SocialLoafer
        );
        assert_eq!(
            analyzer.classify(a_rookie, &momentum, &synergy),
            Archetype::Rookie
        );
        assert_eq!(
            analyzer.classify(a_steady, &momentum, &synergy),
            Archetype::Steady
        );
    }

    #[test]
    fn test_archetypes_report_generation() {
        let momentum = MomentumTracker::new();
        let synergy = SynergyGraph::new();
        let analyzer = ArchetypeAnalyzer::new();

        let a_pillar = AgentId::new();
        let agents = vec![a_pillar];

        let report = analyzer.generate_report(&agents, &momentum, &synergy);
        assert!(report.contains("# Hive Social Dynamics"));
        assert!(report.contains(&a_pillar.to_string()));
    }
}
