//! Archetype Analyzer: Identifies agent behavioral archetypes based on momentum and synergy.
//!
//! "Nova's Social Profiler" - This module classifies agents into archetypes like
//! Pillar (High Momentum, High Synergy), Lone Wolf (High Momentum, Low Synergy),
//! Social Loafer (Low Momentum, High Synergy), Rookie (Low Momentum, Low Synergy),
//! or Steady (Average performance).
//! It also generates a Markdown report of the hive's social dynamics.

use crate::experimental::momentum::MomentumTracker;
use crate::experimental::synergy::SynergyGraph;
use harness_persistence::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};

/// Represents an agent's behavioral archetype based on momentum and synergy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Archetype {
    /// High momentum, high synergy
    Pillar,
    /// High momentum, low synergy
    LoneWolf,
    /// Low momentum, high synergy
    SocialLoafer,
    /// Low momentum, low synergy
    Rookie,
    /// Average performance
    Steady,
}

impl Display for Archetype {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Archetype::Pillar => write!(f, "Pillar 🏛️"),
            Archetype::LoneWolf => write!(f, "Lone Wolf 🐺"),
            Archetype::SocialLoafer => write!(f, "Social Loafer 🛋️"),
            Archetype::Rookie => write!(f, "Rookie 🌱"),
            Archetype::Steady => write!(f, "Steady ⚖️"),
        }
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

    /// Calculate the total synergy score for a given agent across all edges.
    fn calculate_total_synergy(&self, agent_id: AgentId) -> f32 {
        self.synergy
            .edges()
            .iter()
            .filter(|edge| edge.agent_a == agent_id || edge.agent_b == agent_id)
            .map(|edge| edge.weight)
            .sum()
    }

    /// Classify a single agent into an Archetype.
    fn classify_agent(&self, agent_id: AgentId) -> Archetype {
        let momentum_score = self.momentum.agent_momentum(agent_id);
        let synergy_score = self.calculate_total_synergy(agent_id);

        let high_momentum = momentum_score >= 2.0;
        let low_momentum = momentum_score < 0.0;

        let high_synergy = synergy_score >= 3.0; // Arbitrary threshold for high synergy
        let low_synergy = synergy_score < 1.0;

        match (high_momentum, low_momentum, high_synergy, low_synergy) {
            (true, _, true, _) => Archetype::Pillar,
            (true, _, _, _) => Archetype::LoneWolf, // Catch-all high momentum not high synergy
            (_, true, true, _) => Archetype::SocialLoafer,
            (_, true, _, _) => Archetype::Rookie, // Catch-all low momentum not high synergy
            _ => Archetype::Steady,                                        // Between 0.0 and 2.0
        }
    }

    /// Analyze a list of candidates and return their assigned archetypes.
    pub fn analyze(&self, candidates: &[AgentId]) -> HashMap<AgentId, Archetype> {
        candidates
            .iter()
            .map(|&agent_id| (agent_id, self.classify_agent(agent_id)))
            .collect()
    }

    /// Generate a formatted Markdown report of the hive's social dynamics.
    pub fn generate_markdown_report(&self, candidates: &[AgentId]) -> String {
        let profiles = self.analyze(candidates);
        let mut report = String::from("# Hive Social Dynamics Report\n\n");
        report.push_str("| Agent ID | Archetype | Momentum | Total Synergy |\n");
        report.push_str("| :--- | :--- | :--- | :--- |\n");

        for &agent_id in candidates {
            let archetype = profiles.get(&agent_id).unwrap();
            let momentum = self.momentum.agent_momentum(agent_id);
            let synergy = self.calculate_total_synergy(agent_id);

            report.push_str(&format!(
                "| `{}` | {} | {:.1} | {:.1} |\n",
                agent_id.as_uuid(),
                archetype,
                momentum,
                synergy
            ));
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experimental::momentum::MomentumTracker;
    use crate::experimental::synergy::SynergyGraph;
    use chrono::Utc;
    use harness_persistence::{AgentId, TrajectoryEvent, TrajectoryEventId, TriggerKind};

    // Helper to bypass privacy rules for testing.
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

    #[allow(dead_code)]
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
    fn test_archetype_classification() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let a_pillar = AgentId::new();
        let a_lonewolf = AgentId::new();
        let a_loafer = AgentId::new();
        let a_rookie = AgentId::new();
        let a_steady = AgentId::new();

        // 1. Pillar: High Momentum (>= 2.0), High Synergy
        momentum.process_events(&[
            make_event_for_momentum(a_pillar, true),
            make_event_for_momentum(a_pillar, true),
            make_event_for_momentum(a_pillar, true),
        ]);
        synergy.add_synergy(a_pillar, a_steady, 5.0); // Total synergy 5.0

        // 2. Lone Wolf: High Momentum (>= 2.0), Low Synergy
        momentum.process_events(&[
            make_event_for_momentum(a_lonewolf, true),
            make_event_for_momentum(a_lonewolf, true),
        ]);
        // No synergy added

        // 3. Social Loafer: Low Momentum (< 0.0), High Synergy
        momentum.process_events(&[
            make_event_for_momentum(a_loafer, false),
            make_event_for_momentum(a_loafer, false),
        ]);
        synergy.add_synergy(a_loafer, a_steady, 4.0);

        // 4. Rookie: Low Momentum (< 0.0), Low Synergy
        momentum.process_events(&[make_event_for_momentum(a_rookie, false)]);

        // 5. Steady: Momentum between 0.0 and 2.0, Any Synergy
        momentum.process_events(&[make_event_for_momentum(a_steady, true)]);

        let analyzer = ArchetypeAnalyzer::new(&momentum, &synergy);

        let candidates = vec![a_pillar, a_lonewolf, a_loafer, a_rookie, a_steady];
        let profiles = analyzer.analyze(&candidates);

        assert_eq!(profiles.get(&a_pillar).unwrap(), &Archetype::Pillar);
        assert_eq!(profiles.get(&a_lonewolf).unwrap(), &Archetype::LoneWolf);
        assert_eq!(profiles.get(&a_loafer).unwrap(), &Archetype::SocialLoafer);
        assert_eq!(profiles.get(&a_rookie).unwrap(), &Archetype::Rookie);
        assert_eq!(profiles.get(&a_steady).unwrap(), &Archetype::Steady);
    }

    #[test]
    fn test_generate_markdown_report() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let a_pillar = AgentId::new();
        let a_lonewolf = AgentId::new();

        momentum.process_events(&[
            make_event_for_momentum(a_pillar, true),
            make_event_for_momentum(a_pillar, true),
            make_event_for_momentum(a_lonewolf, true),
            make_event_for_momentum(a_lonewolf, true),
        ]);
        synergy.add_synergy(a_pillar, AgentId::new(), 3.0);

        let analyzer = ArchetypeAnalyzer::new(&momentum, &synergy);
        let candidates = vec![a_pillar, a_lonewolf];

        let report = analyzer.generate_markdown_report(&candidates);

        assert!(report.contains("# Hive Social Dynamics Report"));
        assert!(report.contains("Pillar"));
        assert!(report.contains("Lone Wolf 🐺"));
        assert!(report.contains(&a_pillar.as_uuid().to_string()));
        assert!(report.contains(&a_lonewolf.as_uuid().to_string()));
    }
}
