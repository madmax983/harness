//! Archetype Analyzer: Classifies agents into behavioral personas.
//!
//! "Nova's Social Dynamics" - This module combines `MomentumTracker` and
//! `SynergyGraph` to classify agents into behavioral personas: `Pillar`,
//! `LoneWolf`, `SocialLoafer`, `Rookie`, and `Steady`. It can also generate
//! a Markdown report of hive social dynamics.

use harness_persistence::AgentId;
use serde::{Deserialize, Serialize};

use super::momentum::MomentumTracker;
use super::synergy::SynergyGraph;

/// Behavioral personas for agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Archetype {
    /// High momentum, high synergy. The backbone of the team.
    Pillar,
    /// High momentum, low synergy. Gets things done, but alone.
    LoneWolf,
    /// Low momentum, high synergy. Talks a lot, does little.
    SocialLoafer,
    /// Low momentum, low synergy. Needs guidance or is new.
    Rookie,
    /// Average momentum and synergy. Reliable and consistent.
    Steady,
}

/// The Archetype Analyzer engine.
pub struct ArchetypeAnalyzer<'a> {
    momentum: &'a MomentumTracker,
    synergy: &'a SynergyGraph,
}

impl<'a> ArchetypeAnalyzer<'a> {
    /// Create a new ArchetypeAnalyzer from existing momentum and synergy tracking.
    pub fn new(momentum: &'a MomentumTracker, synergy: &'a SynergyGraph) -> Self {
        Self { momentum, synergy }
    }

    /// Calculate total synergy weight for a specific agent.
    fn calculate_total_synergy(&self, agent_id: AgentId) -> f32 {
        self.synergy
            .edges()
            .iter()
            .filter(|edge| edge.agent_a == agent_id || edge.agent_b == agent_id)
            .map(|edge| edge.weight)
            .sum()
    }

    /// Classify an agent into an Archetype.
    pub fn classify(&self, agent_id: AgentId) -> Archetype {
        let momentum = self.momentum.agent_momentum(agent_id);
        let total_synergy = self.calculate_total_synergy(agent_id);

        let high_momentum = momentum >= 2.0;
        let low_momentum = momentum <= -2.0;

        let high_synergy = total_synergy >= 5.0;
        let low_synergy = total_synergy <= 1.0;

        match (high_momentum, low_momentum, high_synergy, low_synergy) {
            (true, _, true, _) => Archetype::Pillar,
            (true, _, _, true) => Archetype::LoneWolf,
            (_, true, true, _) => Archetype::SocialLoafer,
            (_, true, _, true) => Archetype::Rookie,
            _ => Archetype::Steady,
        }
    }

    /// Classify all agents currently tracked by MomentumTracker.
    pub fn classify_all(&self) -> Vec<(AgentId, Archetype)> {
        let mut classifications = Vec::new();

        // Ensure deterministic ordering by collecting unique agents into a Vec and sorting
        let mut unique_agents: std::collections::HashSet<AgentId> =
            std::collections::HashSet::new();
        for agent_id in self.momentum.agents() {
            unique_agents.insert(agent_id);
        }
        for edge in self.synergy.edges() {
            unique_agents.insert(edge.agent_a);
            unique_agents.insert(edge.agent_b);
        }

        let mut sorted_agents: Vec<_> = unique_agents.into_iter().collect();
        sorted_agents.sort_by_key(|a| a.as_uuid().to_string());

        for agent_id in sorted_agents {
            classifications.push((agent_id, self.classify(agent_id)));
        }

        classifications
    }

    /// Generate a Markdown report summarizing the hive's social dynamics.
    pub fn generate_markdown_report(&self) -> String {
        let classifications = self.classify_all();

        let mut report = String::from("# Hive Social Dynamics Report\n\n");
        report.push_str("This report summarizes the behavioral personas of all agents in the hive based on their momentum and synergy.\n\n");

        report.push_str("## Archetypes\n\n");

        // Group by archetype
        let mut pillars = Vec::new();
        let mut lone_wolves = Vec::new();
        let mut social_loafers = Vec::new();
        let mut rookies = Vec::new();
        let mut steadies = Vec::new();

        for (agent_id, archetype) in classifications {
            match archetype {
                Archetype::Pillar => pillars.push(agent_id),
                Archetype::LoneWolf => lone_wolves.push(agent_id),
                Archetype::SocialLoafer => social_loafers.push(agent_id),
                Archetype::Rookie => rookies.push(agent_id),
                Archetype::Steady => steadies.push(agent_id),
            }
        }

        // Helper to format list
        let format_list = |agents: &[AgentId]| -> String {
            if agents.is_empty() {
                return String::from("  *None*\n");
            }
            let mut s = String::new();
            for agent in agents {
                s.push_str(&format!("  - `{}`\n", agent.as_uuid()));
            }
            s
        };

        report.push_str("### 🏛️ Pillars (High Momentum, High Synergy)\n");
        report.push_str(&format_list(&pillars));
        report.push('\n');

        report.push_str("### 🐺 Lone Wolves (High Momentum, Low Synergy)\n");
        report.push_str(&format_list(&lone_wolves));
        report.push('\n');

        report.push_str("### 🗣️ Social Loafers (Low Momentum, High Synergy)\n");
        report.push_str(&format_list(&social_loafers));
        report.push('\n');

        report.push_str("### 🔰 Rookies (Low Momentum, Low Synergy)\n");
        report.push_str(&format_list(&rookies));
        report.push('\n');

        report.push_str("### ⚖️ Steady (Average Momentum/Synergy)\n");
        report.push_str(&format_list(&steadies));
        report.push('\n');

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use harness_persistence::{TrajectoryEvent, TrajectoryEventId, TriggerKind};

    fn make_event(agent_id: AgentId, success: bool, task_id: Option<String>) -> TrajectoryEvent {
        let id = TrajectoryEventId::new();
        let task_uuid =
            task_id.map(|t| uuid::Uuid::parse_str(&t).unwrap_or_else(|_| uuid::Uuid::new_v4()));

        let json = serde_json::json!({
            "id": id,
            "trigger_kind": TriggerKind::TaskComplete,
            "agent_id": agent_id,
            "task_id": task_uuid,
            "success": success,
            "summary": "test",
            "steps": [],
            "created_at": Utc::now()
        });

        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn test_archetypes() {
        let mut momentum = MomentumTracker::new();
        let mut synergy = SynergyGraph::new();

        let a_pillar = AgentId::new();
        let a_lonewolf = AgentId::new();
        let a_loafer = AgentId::new();
        let a_rookie = AgentId::new();
        let a_steady = AgentId::new();

        // 1. Pillar: High Mom (>=2.0), High Syn (>=5.0)
        // 2. Lone Wolf: High Mom (>=2.0), Low Syn (<=1.0)
        // 3. Social Loafer: Low Mom (<=-2.0), High Syn (>=5.0)
        // 4. Rookie: Low Mom (<=-2.0), Low Syn (<=1.0)
        // 5. Steady: Everything else

        // Setup Momentum
        let momentum_events = vec![
            make_event(a_pillar, true, None),
            make_event(a_pillar, true, None),
            make_event(a_pillar, true, None),
            make_event(a_lonewolf, true, None),
            make_event(a_lonewolf, true, None),
            make_event(a_lonewolf, true, None),
            make_event(a_loafer, false, None),
            make_event(a_loafer, false, None),
            make_event(a_loafer, false, None),
            make_event(a_rookie, false, None),
            make_event(a_rookie, false, None),
            make_event(a_rookie, false, None),
            make_event(a_steady, true, None),
            make_event(a_steady, false, None), // Momentum = 0
        ];
        momentum.process_events(&momentum_events);

        // Setup Synergy
        // Pillar needs >= 5.0
        synergy.add_synergy(a_pillar, a_loafer, 6.0); // Now both Pillar and Loafer have >= 5.0

        // Lone Wolf has 0.0 synergy
        // Rookie has 0.0 synergy
        // Steady gets 2.0 synergy with a random ID (to make it average)
        let a_random = AgentId::new();
        synergy.add_synergy(a_steady, a_random, 2.0);

        let analyzer = ArchetypeAnalyzer::new(&momentum, &synergy);

        assert_eq!(analyzer.classify(a_pillar), Archetype::Pillar);
        assert_eq!(analyzer.classify(a_lonewolf), Archetype::LoneWolf);
        assert_eq!(analyzer.classify(a_loafer), Archetype::SocialLoafer);
        assert_eq!(analyzer.classify(a_rookie), Archetype::Rookie);
        assert_eq!(analyzer.classify(a_steady), Archetype::Steady);

        let report = analyzer.generate_markdown_report();
        assert!(report.contains("### 🏛️ Pillars (High Momentum, High Synergy)"));
        assert!(report.contains(&a_pillar.as_uuid().to_string()));
        assert!(report.contains(&a_lonewolf.as_uuid().to_string()));
        assert!(report.contains(&a_loafer.as_uuid().to_string()));
        assert!(report.contains(&a_rookie.as_uuid().to_string()));
        assert!(report.contains(&a_steady.as_uuid().to_string()));
    }
}
