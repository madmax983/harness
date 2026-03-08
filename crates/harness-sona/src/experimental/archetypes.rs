//! Archetype Analyzer: Behavioral persona classification for agents.
//!
//! "Nova's Social Dynamics" - This module combines the `MomentumTracker` and
//! `SynergyGraph` to classify agents into behavioral personas (`Pillar`, `LoneWolf`,
//! `SocialLoafer`, `Rookie`, `Steady`) and can generate a Markdown report of hive
//! social dynamics.

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Behavioral persona classification for an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Archetype {
    /// High momentum, high synergy
    Pillar,
    /// High momentum, low synergy
    LoneWolf,
    /// Low/negative momentum, high synergy
    SocialLoafer,
    /// Low/zero momentum, low synergy
    Rookie,
    /// Medium momentum, medium synergy
    Steady,
}

impl std::fmt::Display for Archetype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Archetype::Pillar => "Pillar 🏛️",
            Archetype::LoneWolf => "Lone Wolf 🐺",
            Archetype::SocialLoafer => "Social Loafer 🛋️",
            Archetype::Rookie => "Rookie 🌱",
            Archetype::Steady => "Steady ⚖️",
        };
        write!(f, "{}", s)
    }
}

/// The Archetype Analyzer classifies agents based on their momentum and synergy scores.
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
    pub fn total_synergy(&self, agent_id: AgentId) -> f32 {
        self.synergy
            .edges()
            .iter()
            .filter(|edge| edge.agent_a == agent_id || edge.agent_b == agent_id)
            .map(|edge| edge.weight)
            .sum()
    }

    /// Classify an agent based on their momentum and total synergy.
    pub fn analyze_agent(&self, agent_id: AgentId) -> Archetype {
        let m = self.momentum.agent_momentum(agent_id);
        let s = self.total_synergy(agent_id);

        // Classification thresholds
        // Momentum: High (>= 2.0), Low (< 1.0)
        // Synergy: High (>= 2.0), Low (< 1.0)

        if m >= 2.0 && s >= 2.0 {
            Archetype::Pillar
        } else if m >= 2.0 && s < 2.0 {
            Archetype::LoneWolf
        } else if m < 1.0 && s >= 2.0 {
            Archetype::SocialLoafer
        } else if m < 1.0 && s < 1.0 {
            Archetype::Rookie
        } else {
            Archetype::Steady
        }
    }

    /// Generate a Markdown report detailing each agent's archetype, momentum, and synergy.
    pub fn generate_markdown_report(&self, agents: &[AgentId]) -> String {
        if agents.is_empty() {
            return String::from("## Hive Social Dynamics Report\n\nNo agents to analyze.\n");
        }

        let unique_agents: HashSet<AgentId> = agents.iter().copied().collect();
        let mut unique_agents_vec: Vec<AgentId> = unique_agents.into_iter().collect();
        // Sort by UUID string for deterministic output
        unique_agents_vec.sort_by_key(|a| a.as_uuid().to_string());

        let mut report = String::from("## Hive Social Dynamics Report\n\n");
        report.push_str("| Agent ID | Archetype | Momentum | Total Synergy |\n");
        report.push_str("| :--- | :--- | :--- | :--- |\n");

        for agent in unique_agents_vec {
            let archetype = self.analyze_agent(agent);
            let m = self.momentum.agent_momentum(agent);
            let s = self.total_synergy(agent);
            let short_id = agent
                .as_uuid()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>();
            report.push_str(&format!(
                "| `{}` | {} | {:.1} | {:.1} |\n",
                short_id, archetype, m, s
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

        let a_pillar = AgentId::new();
        let a_lonewolf = AgentId::new();
        let a_loafer = AgentId::new();
        let a_rookie = AgentId::new();
        let a_steady = AgentId::new();

        // Setup Momentum
        // Pillar: High (3.0)
        // LoneWolf: High (3.0)
        // Loafer: Low (0.0)
        // Rookie: Low (-1.0)
        // Steady: Medium (1.0)
        let m_events = vec![
            make_event_for_momentum(a_pillar, true),
            make_event_for_momentum(a_pillar, true),
            make_event_for_momentum(a_pillar, true),
            make_event_for_momentum(a_lonewolf, true),
            make_event_for_momentum(a_lonewolf, true),
            make_event_for_momentum(a_lonewolf, true),
            make_event_for_momentum(a_loafer, true),
            make_event_for_momentum(a_loafer, false),
            make_event_for_momentum(a_rookie, false),
            make_event_for_momentum(a_steady, true),
        ];
        momentum.process_events(&m_events);

        // Setup Synergy
        // Pillar: High
        // LoneWolf: Low (None)
        // Loafer: High
        // Rookie: Low (None)
        // Steady: Medium (1.0)
        let t1 = uuid::Uuid::new_v4().to_string();
        let t2 = uuid::Uuid::new_v4().to_string();

        let s_events = vec![
            make_event_for_synergy(a_pillar, Some(t1.clone())),
            make_event_for_synergy(a_loafer, Some(t1.clone())),
            make_event_for_synergy(a_pillar, Some(t2.clone())),
            make_event_for_synergy(a_loafer, Some(t2.clone())),
            make_event_for_synergy(a_steady, Some(t1.clone())), // Steady gets 1.0 with Pillar, 1.0 with Loafer -> Total 2.0 ... Wait, if total > 2.0 it's High.
        ];
        synergy.build_from_events(&s_events);

        // Let's manually tweak synergy to match thresholds exactly
        // Let's reset and use add_synergy
        let mut synergy2 = SynergyGraph::new();
        synergy2.add_synergy(a_pillar, a_rookie, 3.0); // Pillar gets 3.0
        synergy2.add_synergy(a_loafer, a_rookie, 3.0); // Loafer gets 3.0
        // Rookie gets 6.0, but their momentum is low, so they would be a Loafer!
        // To make a_rookie a Rookie, they need < 1.0 synergy.

        let mut precise_synergy = SynergyGraph::new();
        precise_synergy.add_synergy(a_pillar, AgentId::new(), 2.5);
        precise_synergy.add_synergy(a_loafer, AgentId::new(), 2.5);
        precise_synergy.add_synergy(a_steady, AgentId::new(), 1.5);

        let analyzer = ArchetypeAnalyzer::new(&momentum, &precise_synergy);

        assert_eq!(analyzer.analyze_agent(a_pillar), Archetype::Pillar);
        assert_eq!(analyzer.analyze_agent(a_lonewolf), Archetype::LoneWolf);
        assert_eq!(analyzer.analyze_agent(a_loafer), Archetype::SocialLoafer);
        assert_eq!(analyzer.analyze_agent(a_rookie), Archetype::Rookie);
        assert_eq!(analyzer.analyze_agent(a_steady), Archetype::Steady);
    }

    #[test]
    fn test_markdown_report_generation() {
        let momentum = MomentumTracker::new();
        let synergy = SynergyGraph::new();
        let analyzer = ArchetypeAnalyzer::new(&momentum, &synergy);

        let a1 = AgentId::new();
        let report = analyzer.generate_markdown_report(&[a1]);

        assert!(report.contains("Hive Social Dynamics Report"));
        assert!(report.contains("| Agent ID | Archetype | Momentum | Total Synergy |"));
        assert!(report.contains("Rookie 🌱")); // Because momentum and synergy are 0
    }
}
