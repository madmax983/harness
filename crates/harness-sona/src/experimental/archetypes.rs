//! Archetype Analyzer: Classifies agents into behavioral archetypes.
//!
//! "Nova's Persona Graph" - This module uses `MomentumTracker` and
//! `SynergyGraph` to determine an agent's working style (Archetype)
//! and can generate a report of hive social dynamics.

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;
use std::collections::HashSet;

/// Represents an agent's behavioral archetype based on momentum and synergy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Archetype {
    /// High momentum, high synergy
    Pillar,
    /// High momentum, low synergy
    LoneWolf,
    /// Low momentum, high synergy
    SocialLoafer,
    /// Low momentum, low synergy
    Rookie,
    /// Medium momentum, medium synergy
    Steady,
}

impl Archetype {
    /// Returns a string representation of the archetype.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pillar => "Pillar",
            Self::LoneWolf => "Lone Wolf",
            Self::SocialLoafer => "Social Loafer",
            Self::Rookie => "Rookie",
            Self::Steady => "Steady",
        }
    }
}

/// The Archetype Analyzer classifies agents into behavioral personas.
pub struct ArchetypeAnalyzer<'a> {
    momentum: &'a MomentumTracker,
    synergy: &'a SynergyGraph,
}

impl<'a> ArchetypeAnalyzer<'a> {
    /// Create a new ArchetypeAnalyzer.
    pub fn new(momentum: &'a MomentumTracker, synergy: &'a SynergyGraph) -> Self {
        Self { momentum, synergy }
    }

    /// Calculate the total synergy score for an agent.
    pub fn total_synergy(&self, agent_id: AgentId) -> f32 {
        self.synergy
            .edges()
            .iter()
            .filter(|edge| edge.agent_a == agent_id || edge.agent_b == agent_id)
            .map(|edge| edge.weight)
            .sum()
    }

    /// Analyze an agent's momentum and synergy to determine their archetype.
    pub fn analyze(&self, agent_id: AgentId) -> Archetype {
        let m = self.momentum.agent_momentum(agent_id);
        let s = self.total_synergy(agent_id);

        if m >= 2.0 && s >= 2.0 {
            Archetype::Pillar
        } else if m >= 2.0 && s < 2.0 {
            Archetype::LoneWolf
        } else if m <= -1.0 && s >= 2.0 {
            Archetype::SocialLoafer
        } else if m <= -1.0 && s < 2.0 {
            Archetype::Rookie
        } else {
            Archetype::Steady
        }
    }

    /// Generates a Markdown report of the hive's social dynamics.
    pub fn generate_markdown_report(&self) -> String {
        let mut report = String::from("# Hive Social Dynamics Report\n\n");
        report.push_str("## Agent Archetypes\n\n");

        // Collect all unique agents from both trackers
        let mut all_agents: HashSet<AgentId> = HashSet::new();
        for agent in self.momentum.agents() {
            all_agents.insert(agent);
        }
        for edge in self.synergy.edges() {
            all_agents.insert(edge.agent_a);
            all_agents.insert(edge.agent_b);
        }

        // Sort deterministically
        let mut agents_vec: Vec<AgentId> = all_agents.into_iter().collect();
        agents_vec.sort_by_key(|a| a.as_uuid().to_string());

        if agents_vec.is_empty() {
            report.push_str("*No agents found in the hive.*\n");
            return report;
        }

        for agent in agents_vec {
            let m = self.momentum.agent_momentum(agent);
            let s = self.total_synergy(agent);
            let archetype = self.analyze(agent);

            report.push_str(&format!(
                "- **Agent {}**: {}\n  - Momentum: {:.1}, Synergy: {:.1}\n",
                agent.as_uuid(),
                archetype.as_str(),
                m,
                s
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
    fn test_archetypes() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let a_pillar = AgentId::new();
        let a_lone_wolf = AgentId::new();
        let a_social_loafer = AgentId::new();
        let a_rookie = AgentId::new();
        let a_steady = AgentId::new();

        let m_events = vec![
            // Pillar: High Momentum (3 wins)
            make_event_for_momentum(a_pillar, true),
            make_event_for_momentum(a_pillar, true),
            make_event_for_momentum(a_pillar, true),
            // Lone Wolf: High Momentum (3 wins)
            make_event_for_momentum(a_lone_wolf, true),
            make_event_for_momentum(a_lone_wolf, true),
            make_event_for_momentum(a_lone_wolf, true),
            // Social Loafer: Low Momentum (2 losses)
            make_event_for_momentum(a_social_loafer, false),
            make_event_for_momentum(a_social_loafer, false),
            // Rookie: Low Momentum (2 losses)
            make_event_for_momentum(a_rookie, false),
            make_event_for_momentum(a_rookie, false),
            // Steady: Medium Momentum (1 win)
            make_event_for_momentum(a_steady, true),
        ];
        momentum.process_events(&m_events);

        let t1 = uuid::Uuid::new_v4().to_string();
        let t2 = uuid::Uuid::new_v4().to_string();

        let s_events = vec![
            // Pillar: High Synergy (collaborates with Steady)
            make_event_for_synergy(a_pillar, Some(t1.clone())),
            make_event_for_synergy(a_steady, Some(t1.clone())),
            make_event_for_synergy(a_pillar, Some(t2.clone())),
            make_event_for_synergy(a_steady, Some(t2.clone())),
            // Social Loafer: High Synergy (collaborates with Steady)
            make_event_for_synergy(a_social_loafer, Some(t1.clone())),
            make_event_for_synergy(a_social_loafer, Some(t2.clone())),
        ];
        synergy.build_from_events(&s_events);

        // Manual override for simpler testing if needed, but build_from_events should give enough

        let analyzer = ArchetypeAnalyzer::new(&momentum, &synergy);

        assert_eq!(analyzer.analyze(a_pillar), Archetype::Pillar);
        assert_eq!(analyzer.analyze(a_lone_wolf), Archetype::LoneWolf);
        assert_eq!(analyzer.analyze(a_social_loafer), Archetype::SocialLoafer);
        assert_eq!(analyzer.analyze(a_rookie), Archetype::Rookie);
        assert_eq!(analyzer.analyze(a_steady), Archetype::Steady);

        let report = analyzer.generate_markdown_report();
        assert!(report.contains("# Hive Social Dynamics Report"));
        assert!(report.contains("Pillar"));
        assert!(report.contains("Lone Wolf"));
    }
}
