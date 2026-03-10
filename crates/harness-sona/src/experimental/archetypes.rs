//! Archetype Analyzer: Classifies agents into distinct behavioral personas.
//!
//! "Nova's Archetype Analyzer" - This module combines the `MomentumTracker` and
//! `SynergyGraph` to classify agents based on their momentum and synergy scores.
//! Agents are grouped into archetypes:
//! - Pillar: High momentum and high synergy.
//! - LoneWolf: High momentum but low synergy.
//! - SocialLoafer: Low momentum but high synergy.
//! - Rookie: Low momentum and low synergy.
//! - Steady: Average momentum and synergy.

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// The various behavioral personas an agent can be classified into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Archetype {
    /// High momentum and high synergy. A core team member.
    Pillar,
    /// High momentum but low synergy. Works well alone but not with others.
    LoneWolf,
    /// Low momentum but high synergy. A team player who isn't pulling their weight.
    SocialLoafer,
    /// Low momentum and low synergy. Needs coaching or a different role.
    Rookie,
    /// Average momentum and synergy. A reliable contributor.
    Steady,
}

/// The ArchetypeAnalyzer class to classify agents.
pub struct ArchetypeAnalyzer<'a> {
    momentum: &'a MomentumTracker,
    synergy: &'a SynergyGraph,
}

impl<'a> ArchetypeAnalyzer<'a> {
    /// Create a new ArchetypeAnalyzer.
    pub fn new(momentum: &'a MomentumTracker, synergy: &'a SynergyGraph) -> Self {
        Self { momentum, synergy }
    }

    /// Calculate the total synergy score for a given agent.
    fn calculate_total_synergy(&self, agent_id: AgentId) -> f32 {
        self.synergy
            .edges()
            .iter()
            .filter(|edge| edge.agent_a == agent_id || edge.agent_b == agent_id)
            .map(|edge| edge.weight)
            .sum()
    }

    /// Classify an individual agent into an Archetype.
    pub fn classify_agent(&self, agent_id: AgentId) -> Archetype {
        let m = self.momentum.agent_momentum(agent_id);
        let s = self.calculate_total_synergy(agent_id);

        let high_momentum = m >= 2.0;
        let low_momentum = m <= -1.0;

        let high_synergy = s >= 2.0;
        let low_synergy = s < 1.0;

        if high_momentum && high_synergy {
            Archetype::Pillar
        } else if high_momentum && low_synergy {
            Archetype::LoneWolf
        } else if low_momentum && high_synergy {
            Archetype::SocialLoafer
        } else if low_momentum && low_synergy {
            Archetype::Rookie
        } else {
            Archetype::Steady
        }
    }

    /// Classify all known agents.
    pub fn classify_all(&self) -> HashMap<AgentId, Archetype> {
        let mut known_agents = HashSet::new();

        // Collect from momentum
        for agent in self.momentum.agents() {
            known_agents.insert(agent);
        }

        // Collect from synergy
        for edge in self.synergy.edges() {
            known_agents.insert(edge.agent_a);
            known_agents.insert(edge.agent_b);
        }

        let mut classifications = HashMap::new();
        for agent in known_agents {
            classifications.insert(agent, self.classify_agent(agent));
        }

        classifications
    }

    /// Generate a Markdown report summarizing the social dynamics of the hive.
    pub fn generate_report(&self) -> String {
        let classifications = self.classify_all();

        // Sort agents by UUID for deterministic output
        let mut agents: Vec<AgentId> = classifications.keys().copied().collect();
        agents.sort_by_key(|a| a.as_uuid().to_string());

        let mut report = String::from("# Hive Social Dynamics Report\n\n");

        if agents.is_empty() {
            report.push_str("No active agents found in the hive.\n");
            return report;
        }

        report.push_str("## Agent Archetypes\n\n");
        report.push_str("| Agent ID | Archetype | Momentum | Total Synergy |\n");
        report.push_str("|----------|-----------|----------|---------------|\n");

        for agent in agents {
            let arch = classifications[&agent];
            let m = self.momentum.agent_momentum(agent);
            let s = self.calculate_total_synergy(agent);

            report.push_str(&format!(
                "| {} | {:?} | {:.1} | {:.1} |\n",
                agent.as_uuid(),
                arch,
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
    fn test_archetype_classification() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let pillar = AgentId::new();
        let lone_wolf = AgentId::new();
        let social_loafer = AgentId::new();
        let rookie = AgentId::new();
        let steady = AgentId::new();

        // Setup Momentum
        let m_events = vec![
            // Pillar: High momentum
            make_event_for_momentum(pillar, true),
            make_event_for_momentum(pillar, true),
            // LoneWolf: High momentum
            make_event_for_momentum(lone_wolf, true),
            make_event_for_momentum(lone_wolf, true),
            // SocialLoafer: Low momentum
            make_event_for_momentum(social_loafer, false),
            // Rookie: Low momentum
            make_event_for_momentum(rookie, false),
            // Steady: Average momentum
            make_event_for_momentum(steady, true),
            make_event_for_momentum(steady, false),
        ];
        momentum.process_events(&m_events);

        // Setup Synergy
        let t1 = uuid::Uuid::new_v4().to_string();
        let t2 = uuid::Uuid::new_v4().to_string();
        let t3 = uuid::Uuid::new_v4().to_string();

        let s_events = vec![
            // Pillar synergy with SocialLoafer
            make_event_for_synergy(pillar, Some(t1.clone())),
            make_event_for_synergy(social_loafer, Some(t1.clone())),
            make_event_for_synergy(pillar, Some(t2.clone())),
            make_event_for_synergy(social_loafer, Some(t2.clone())),
            // Steady synergy with someone
            make_event_for_synergy(steady, Some(t3.clone())),
            make_event_for_synergy(pillar, Some(t3.clone())), // Pillar gets more synergy

                                                              // LoneWolf gets no synergy
                                                              // Rookie gets no synergy
        ];
        synergy.build_from_events(&s_events);

        let analyzer = ArchetypeAnalyzer::new(&momentum, &synergy);

        assert_eq!(analyzer.classify_agent(pillar), Archetype::Pillar);
        assert_eq!(analyzer.classify_agent(lone_wolf), Archetype::LoneWolf);
        assert_eq!(
            analyzer.classify_agent(social_loafer),
            Archetype::SocialLoafer
        );
        assert_eq!(analyzer.classify_agent(rookie), Archetype::Rookie);
        assert_eq!(analyzer.classify_agent(steady), Archetype::Steady);
    }

    #[test]
    fn test_generate_report() {
        let mut momentum = MomentumTracker::new();
        let synergy = SynergyGraph::new(); // Empty synergy

        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        momentum.process_events(&[
            make_event_for_momentum(agent1, true),
            make_event_for_momentum(agent2, false),
        ]);

        let analyzer = ArchetypeAnalyzer::new(&momentum, &synergy);
        let report = analyzer.generate_report();

        assert!(report.contains("# Hive Social Dynamics Report"));
        assert!(report.contains(&agent1.as_uuid().to_string()));
        assert!(report.contains(&agent2.as_uuid().to_string()));

        // Ensure deterministic ordering
        let mut uuids = vec![agent1.as_uuid().to_string(), agent2.as_uuid().to_string()];
        uuids.sort();

        let pos1 = report.find(&uuids[0]).unwrap();
        let pos2 = report.find(&uuids[1]).unwrap();

        assert!(
            pos1 < pos2,
            "Report output is not sorted by UUID deterministically"
        );
    }
}
